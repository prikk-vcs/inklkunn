//! The SQLite store (`rusqlite`, bundled): WAL, a versioned schema, one connection behind a mutex.
//!
//! **Opening never guesses about the schema.** The version is read first. A database newer than this build
//! knows, or one whose version cannot be read, is refused before anything is written. An older one is
//! brought forward by [`MIGRATIONS`], in order, inside one immediate transaction that re-reads the version
//! first (a second process may have migrated already); a fresh database gets every migration and the
//! version row in one transaction. Every migration but versions 6 and 17 only **adds**; version 6 rebuilds the role
//! grants with their foreign keys to accounts, copies every row, checks the result with SQLite's own
//! `foreign_key_check`, and **refuses** (naming the version, rolling everything back) if a grant refers to
//! an account that does not exist; version 17 removes **derived data only** (the knowledge index of every tip, rebuilt
//! from the tips' files when next asked for) and nothing a person or the service recorded. A build one step behind
//! refuses a newer database cleanly in either case. Every change to a record runs in an immediate transaction, so
//! a second process (a command-line run beside the service) cannot slip between a check and a write; a
//! slug or id already taken is reported as exactly that.

mod acceptance;
mod acts;
mod claim_thread;
mod derived;
mod governance;
mod identity;
mod inbox;
mod invitation;
mod proposal;
mod review;
mod view_acts;

use std::path::Path;
use std::sync::{Mutex, PoisonError};

use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

use crate::access::{Role, RoleGrant, RoleStore};
use crate::audit::{AuditEvent, AuditLog, AuditOutcome, AuditRecord, AuditSink};
use crate::record::{
    AccountId, Attempt, Checkpoint, DisplayName, NotRun, PrikkVersion, Slug, Verification,
    VerificationState, Visibility, WikiId, WikiRecord,
};
use crate::{Result, StoreError, WikiStore};

/// The schema version this build writes and reads: the last of [`MIGRATIONS`].
pub const SCHEMA_VERSION: i64 = 19;

/// Version 1: M0's schema, the wiki records.
const V1: &str = "
CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS wikis (
    id                   TEXT PRIMARY KEY,
    slug                 TEXT NOT NULL UNIQUE,
    display_name         TEXT NOT NULL,
    default_view         TEXT NOT NULL,
    default_license      TEXT NOT NULL,
    languages            TEXT NOT NULL,
    visibility           TEXT NOT NULL,
    policy_bootstrap     TEXT NOT NULL,
    prikk_format_version TEXT,
    verification_state   TEXT NOT NULL,
    verification_at      INTEGER,
    author_key_id        TEXT,
    maintainer_key_id    TEXT,
    created_at           INTEGER NOT NULL,
    seq                  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS wikis_seq ON wikis(seq);
";

/// Version 2: who holds which role in which wiki (RFC 002 D-2). One role per account per wiki; `seq` orders
/// grants by when each was last made.
const V2: &str = "
CREATE TABLE IF NOT EXISTS role_grants (
    wiki_id    TEXT NOT NULL,
    account_id TEXT NOT NULL,
    role       TEXT NOT NULL,
    granted_by TEXT,
    granted_at INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, account_id)
);
CREATE INDEX IF NOT EXISTS role_grants_wiki ON role_grants(wiki_id, seq);
";

/// Version 3: the audit trail (RFC 002 D-9). **Append-only in the database itself**: triggers abort any
/// `UPDATE`, any `DELETE` and any `INSERT` over a position already taken (`INSERT OR REPLACE`), so even code
/// that holds a connection cannot edit or erase an event through SQL's data statements. This is a guard
/// against code paths, not against someone with the file: they can drop the triggers, and there is no hash
/// chain to show that they did (shipping the trail off the box, RFC 002 D-9's sink trait, is what answers that).
const V3: &str = "
CREATE TABLE IF NOT EXISTS audit_log (
    seq       INTEGER PRIMARY KEY AUTOINCREMENT,
    at        INTEGER NOT NULL,
    principal TEXT NOT NULL,
    action    TEXT NOT NULL,
    resource  TEXT NOT NULL,
    outcome   TEXT NOT NULL,
    detail    TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
CREATE TRIGGER IF NOT EXISTS audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
-- `INSERT OR REPLACE` with a position that exists deletes the old row without firing a delete trigger
-- (recursive triggers are off by default), so a row may not be inserted under a position already taken.
CREATE TRIGGER IF NOT EXISTS audit_log_no_replace BEFORE INSERT ON audit_log
WHEN NEW.seq IS NOT NULL AND EXISTS (SELECT 1 FROM audit_log WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'the audit log is append-only'); END;
";

/// Version 4: accounts, the external identities linked to them, browser sessions and API tokens (RFC 002
/// D-1). A password is an Argon2id PHC string and a session id or a token is the SHA-256 of what the client
/// holds, so a copy of this file opens no session and reveals no token. An account has a password and a
/// sign-in name together or neither (`CHECK`); a name is unique; an external identity belongs to at most one
/// account; a token's scope is one of the five (`CHECK`). Foreign keys are enforced.
const V4: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    id            TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL,
    login         TEXT UNIQUE,
    password_hash TEXT,
    created_at    INTEGER NOT NULL,
    disabled      INTEGER NOT NULL DEFAULT 0 CHECK (disabled IN (0, 1)),
    CHECK ((login IS NULL) = (password_hash IS NULL))
);
CREATE TABLE IF NOT EXISTS account_identities (
    issuer     TEXT NOT NULL,
    subject    TEXT NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    linked_at  INTEGER NOT NULL,
    PRIMARY KEY (issuer, subject)
);
CREATE INDEX IF NOT EXISTS account_identities_account ON account_identities(account_id);
CREATE TABLE IF NOT EXISTS sessions (
    id_hash    TEXT PRIMARY KEY,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    csrf       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS sessions_account ON sessions(account_id);
CREATE INDEX IF NOT EXISTS sessions_expiry ON sessions(expires_at);
CREATE TABLE IF NOT EXISTS api_tokens (
    id         TEXT PRIMARY KEY,
    hash       TEXT NOT NULL UNIQUE,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    scope      TEXT NOT NULL CHECK (scope IN ('read', 'propose', 'review', 'accept', 'admin')),
    label      TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked_at INTEGER
);
CREATE INDEX IF NOT EXISTS api_tokens_account ON api_tokens(account_id);
";

/// Version 5: the derived cache and the checkpoint columns (RFC 003 D-2; handoff 003 T3). **Everything
/// here but the two columns can be dropped and rebuilt from the repositories** (INV-5): rendered pages, and
/// the title index of a view's tip, keyed by prikk's ids. A tip's index is written with its entries in one
/// transaction and counted in `title_index_tips`, which is how a reader knows it is whole. No column holds a
/// verification result, a state or a time of a verification: the cache has nowhere to put one.
///
/// The two columns added to `wikis` are the service's knowledge of prikk's checkpoint record (ledger PK-24):
/// the prikk version it was last refreshed under, and the time of the first unanswered request for a refresh.
const V5: &str = "
CREATE TABLE IF NOT EXISTS derived_pages (
    wiki_id      TEXT NOT NULL,
    tip          TEXT NOT NULL,
    path         TEXT NOT NULL,
    lang         TEXT NOT NULL,
    presentation TEXT NOT NULL,
    html         TEXT NOT NULL,
    meta         TEXT NOT NULL,
    links        TEXT NOT NULL,
    toc          TEXT NOT NULL,
    computed_at  INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip, path, lang, presentation)
);
CREATE INDEX IF NOT EXISTS derived_pages_tip ON derived_pages(wiki_id, tip);
CREATE TABLE IF NOT EXISTS title_index_tips (
    wiki_id  TEXT NOT NULL,
    tip      TEXT NOT NULL,
    entries  INTEGER NOT NULL,
    built_at INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip)
);
CREATE TABLE IF NOT EXISTS title_index_entries (
    wiki_id TEXT NOT NULL,
    tip     TEXT NOT NULL,
    path    TEXT NOT NULL,
    title   TEXT NOT NULL,
    aliases TEXT NOT NULL,
    redirect TEXT,
    page_id TEXT,
    kind    TEXT NOT NULL,
    problem TEXT,
    PRIMARY KEY (wiki_id, tip, path)
);
ALTER TABLE wikis ADD COLUMN checkpoint_prikk_version TEXT;
ALTER TABLE wikis ADD COLUMN checkpoint_refresh_requested_at INTEGER;
";

/// Version 6: the role grants gain their foreign keys (handoff 002 C10; version 4 left them out because the
/// accounts table did not exist yet when version 2 made them): `account_id` and `granted_by` refer to
/// `accounts(id)`. `granted_by` stays nullable: `NULL` is the operator, from the command line (there is no
/// system principal stored; the first administrator of a wiki can only be named that way), and every other
/// value is an account. SQLite cannot add a constraint to a table, so the table is rebuilt: the new one is
/// made, every row copied, the old one dropped, the new one renamed, the index remade. Violations are
/// **deferred** for this transaction only (`defer_foreign_keys` resets at its end), so a database that holds a
/// grant to an account that is not there copies whole and is then refused by name by `apply_migration`,
/// which runs `PRAGMA foreign_key_check(role_grants)` before the version is raised and lets the transaction
/// roll back. The fresh-database path runs the same code.
const V6: &str = "
PRAGMA defer_foreign_keys = ON;
CREATE TABLE role_grants_v6 (
    wiki_id    TEXT NOT NULL,
    account_id TEXT NOT NULL REFERENCES accounts(id),
    role       TEXT NOT NULL,
    granted_by TEXT REFERENCES accounts(id),
    granted_at INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, account_id)
);
INSERT INTO role_grants_v6 (wiki_id, account_id, role, granted_by, granted_at, seq)
    SELECT wiki_id, account_id, role, granted_by, granted_at, seq FROM role_grants;
DROP TABLE role_grants;
ALTER TABLE role_grants_v6 RENAME TO role_grants;
CREATE INDEX role_grants_wiki ON role_grants(wiki_id, seq);
";

/// Version 7: the search index and the observed times (RFC 003 D-8, D-7; handoff 003 T6). **Only adds.**
/// `search_docs` holds every title and alias of a tip with the folded text the search compares, `search_fts` is
/// an FTS5 index over that folded text with the **trigram** tokenizer (a query matches any part of a text, which
/// is how a Japanese title is found by a substring), and `search_tips` says an index is whole (the same device
/// as `title_index_tips`). All three are derived and can be dropped and rebuilt (INV-5). `block_seen` is not:
/// prikk's history carries no time, so the time a block was **first seen by this service** is the only time
/// the pages can show, and it cannot be recovered once lost; the cache's `clear` leaves it alone.
const V7: &str = "
CREATE TABLE IF NOT EXISTS search_tips (
    wiki_id  TEXT NOT NULL,
    tip      TEXT NOT NULL,
    docs     INTEGER NOT NULL,
    built_at INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip)
);
CREATE TABLE IF NOT EXISTS search_docs (
    id      INTEGER PRIMARY KEY,
    wiki_id TEXT NOT NULL,
    tip     TEXT NOT NULL,
    path    TEXT NOT NULL,
    kind    TEXT NOT NULL,
    text    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS search_docs_tip ON search_docs(wiki_id, tip);
CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(folded, tokenize = 'trigram');
CREATE TABLE IF NOT EXISTS block_seen (
    wiki_id  TEXT NOT NULL,
    block_id TEXT NOT NULL,
    seen_at  INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, block_id)
);
";

/// Version 8: a verification that could not run is kept apart from the last verdict (handoff 003, review R1).
/// `verification_state` / `verification_at` now hold **verdicts only** (`unknown`, `verified`, `failed`);
/// `attempt_at` / `attempt_why` hold the latest attempt that could not run, if it is the latest thing that
/// happened. A row that held `could-not-check` has lost the verdict it replaced, and says so honestly: it is
/// made `unknown`, with the attempt (reason `unavailable`: the old record never said why) kept.
const V8: &str = "
ALTER TABLE wikis ADD COLUMN attempt_at INTEGER;
ALTER TABLE wikis ADD COLUMN attempt_why TEXT;
UPDATE wikis SET attempt_at = verification_at, attempt_why = 'unavailable',
                 verification_state = 'unknown', verification_at = NULL
    WHERE verification_state = 'could-not-check';
";

/// Version 9: proposals and review (RFC 004 D-1, D-3, D-4, D-9). **Only adds.** The proposal row is the one
/// thing that changes (its state, its text while it is shaped, the times it was submitted and closed), always
/// in one transaction with the `proposal_transitions` row that explains the change. Revisions, transitions,
/// comments, decisions, acceptances, acceptance results, intents and intent completions are **written once, in
/// the database itself**: triggers abort any `UPDATE`, any `DELETE` and any `INSERT` over a key already taken
/// (the audit log's device, version 3), counting every unique key of the table, because a replacing `INSERT`
/// removes the row it collides with without firing a delete trigger. As there, this is a guard against code
/// paths, not against someone with the file. A draft's working copy, the lease holder, watches and inbox events
/// are mutable on purpose. Accounts and wikis are referenced by foreign keys.
const V9: &str = "
CREATE TABLE IF NOT EXISTS proposals (
    id            TEXT PRIMARY KEY,
    seq           INTEGER NOT NULL,
    wiki_id       TEXT NOT NULL REFERENCES wikis(id),
    view          TEXT NOT NULL,
    state         TEXT NOT NULL CHECK (state IN ('draft', 'held-for-review', 'needs-revision', 'accepted',
                                                 'declined', 'withdrawn', 'superseded')),
    author        TEXT NOT NULL REFERENCES accounts(id),
    summary       TEXT NOT NULL,
    sources       TEXT NOT NULL,
    superseded_by TEXT REFERENCES proposals(id),
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    submitted_at  INTEGER,
    closed_at     INTEGER
);
CREATE INDEX IF NOT EXISTS proposals_wiki ON proposals(wiki_id, seq);
CREATE INDEX IF NOT EXISTS proposals_author ON proposals(wiki_id, author);
CREATE TABLE IF NOT EXISTS proposal_drafts (
    proposal_id TEXT PRIMARY KEY REFERENCES proposals(id),
    content     TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS proposal_revisions (
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    seq         INTEGER NOT NULL CHECK (seq >= 1),
    content     TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (proposal_id, seq)
);
CREATE TRIGGER IF NOT EXISTS proposal_revisions_no_update BEFORE UPDATE ON proposal_revisions
BEGIN SELECT RAISE(ABORT, 'proposal revisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_revisions_no_delete BEFORE DELETE ON proposal_revisions
BEGIN SELECT RAISE(ABORT, 'proposal revisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_revisions_no_replace BEFORE INSERT ON proposal_revisions
WHEN EXISTS (SELECT 1 FROM proposal_revisions WHERE proposal_id = NEW.proposal_id AND seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'proposal revisions are append-only'); END;
CREATE TABLE IF NOT EXISTS proposal_transitions (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    op          TEXT NOT NULL,
    from_state  TEXT,
    to_state    TEXT NOT NULL,
    actor       TEXT NOT NULL REFERENCES accounts(id),
    reason      TEXT,
    at          INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS proposal_transitions_proposal ON proposal_transitions(proposal_id, seq);
CREATE TRIGGER IF NOT EXISTS proposal_transitions_no_update BEFORE UPDATE ON proposal_transitions
BEGIN SELECT RAISE(ABORT, 'the proposal log is append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_transitions_no_delete BEFORE DELETE ON proposal_transitions
BEGIN SELECT RAISE(ABORT, 'the proposal log is append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_transitions_no_replace BEFORE INSERT ON proposal_transitions
WHEN NEW.seq IS NOT NULL AND EXISTS (SELECT 1 FROM proposal_transitions WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'the proposal log is append-only'); END;
CREATE TABLE IF NOT EXISTS proposal_comments (
    id          TEXT PRIMARY KEY,
    seq         INTEGER NOT NULL,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    revision    INTEGER NOT NULL,
    author      TEXT NOT NULL REFERENCES accounts(id),
    parent      TEXT REFERENCES proposal_comments(id),
    body        TEXT NOT NULL,
    anchor      TEXT,
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (proposal_id, revision) REFERENCES proposal_revisions(proposal_id, seq)
);
CREATE INDEX IF NOT EXISTS proposal_comments_proposal ON proposal_comments(proposal_id, seq);
CREATE TRIGGER IF NOT EXISTS proposal_comments_no_update BEFORE UPDATE ON proposal_comments
BEGIN SELECT RAISE(ABORT, 'proposal comments are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_comments_no_delete BEFORE DELETE ON proposal_comments
BEGIN SELECT RAISE(ABORT, 'proposal comments are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_comments_no_replace BEFORE INSERT ON proposal_comments
WHEN EXISTS (SELECT 1 FROM proposal_comments WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'proposal comments are append-only'); END;
CREATE TABLE IF NOT EXISTS proposal_decisions (
    id          TEXT PRIMARY KEY,
    seq         INTEGER NOT NULL,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    revision    INTEGER NOT NULL,
    reviewer    TEXT NOT NULL REFERENCES accounts(id),
    kind        TEXT NOT NULL CHECK (kind IN ('approve', 'request-changes')),
    body        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    UNIQUE (proposal_id, revision, reviewer),
    FOREIGN KEY (proposal_id, revision) REFERENCES proposal_revisions(proposal_id, seq)
);
CREATE INDEX IF NOT EXISTS proposal_decisions_proposal ON proposal_decisions(proposal_id, seq);
CREATE TRIGGER IF NOT EXISTS proposal_decisions_no_update BEFORE UPDATE ON proposal_decisions
BEGIN SELECT RAISE(ABORT, 'decisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_decisions_no_delete BEFORE DELETE ON proposal_decisions
BEGIN SELECT RAISE(ABORT, 'decisions are append-only'); END;
-- Two unique keys: the id, and one decision per reviewer per revision. A replacing insert on either would
-- remove the row it collides with without firing the delete trigger.
CREATE TRIGGER IF NOT EXISTS proposal_decisions_no_replace BEFORE INSERT ON proposal_decisions
WHEN EXISTS (SELECT 1 FROM proposal_decisions WHERE id = NEW.id
             OR (proposal_id = NEW.proposal_id AND revision = NEW.revision AND reviewer = NEW.reviewer))
BEGIN SELECT RAISE(ABORT, 'decisions are append-only'); END;
CREATE TABLE IF NOT EXISTS acceptances (
    id             TEXT PRIMARY KEY,
    seq            INTEGER NOT NULL,
    proposal_id    TEXT NOT NULL REFERENCES proposals(id),
    revision       INTEGER NOT NULL,
    wiki_id        TEXT NOT NULL REFERENCES wikis(id),
    view           TEXT NOT NULL,
    acceptor       TEXT NOT NULL REFERENCES accounts(id),
    acceptor_role  TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    proposer       TEXT NOT NULL REFERENCES accounts(id),
    digest         TEXT NOT NULL,
    bytes          INTEGER NOT NULL,
    created_at     INTEGER NOT NULL,
    FOREIGN KEY (proposal_id, revision) REFERENCES proposal_revisions(proposal_id, seq)
);
CREATE INDEX IF NOT EXISTS acceptances_revision ON acceptances(proposal_id, revision);
CREATE INDEX IF NOT EXISTS acceptances_proposer ON acceptances(wiki_id, proposer, created_at);
CREATE TRIGGER IF NOT EXISTS acceptances_no_update BEFORE UPDATE ON acceptances
BEGIN SELECT RAISE(ABORT, 'acceptances are append-only'); END;
CREATE TRIGGER IF NOT EXISTS acceptances_no_delete BEFORE DELETE ON acceptances
BEGIN SELECT RAISE(ABORT, 'acceptances are append-only'); END;
CREATE TRIGGER IF NOT EXISTS acceptances_no_replace BEFORE INSERT ON acceptances
WHEN EXISTS (SELECT 1 FROM acceptances WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'acceptances are append-only'); END;
CREATE TABLE IF NOT EXISTS acceptance_results (
    acceptance_id TEXT PRIMARY KEY REFERENCES acceptances(id),
    outcome       TEXT NOT NULL CHECK (outcome IN ('sealed', 'failed')),
    block_id      TEXT,
    patch_id      TEXT,
    word          TEXT,
    at            INTEGER NOT NULL,
    CHECK ((outcome = 'sealed' AND block_id IS NOT NULL AND patch_id IS NOT NULL AND word IS NULL)
        OR (outcome = 'failed' AND word IS NOT NULL AND block_id IS NULL AND patch_id IS NULL))
);
CREATE TRIGGER IF NOT EXISTS acceptance_results_no_update BEFORE UPDATE ON acceptance_results
BEGIN SELECT RAISE(ABORT, 'acceptance results are append-only'); END;
CREATE TRIGGER IF NOT EXISTS acceptance_results_no_delete BEFORE DELETE ON acceptance_results
BEGIN SELECT RAISE(ABORT, 'acceptance results are append-only'); END;
CREATE TRIGGER IF NOT EXISTS acceptance_results_no_replace BEFORE INSERT ON acceptance_results
WHEN EXISTS (SELECT 1 FROM acceptance_results WHERE acceptance_id = NEW.acceptance_id)
BEGIN SELECT RAISE(ABORT, 'acceptance results are append-only'); END;
CREATE TABLE IF NOT EXISTS intents (
    id             TEXT PRIMARY KEY,
    seq            INTEGER NOT NULL,
    acceptance_id  TEXT NOT NULL UNIQUE REFERENCES acceptances(id),
    wiki_id        TEXT NOT NULL REFERENCES wikis(id),
    view           TEXT NOT NULL,
    base_block     TEXT NOT NULL,
    message_digest TEXT NOT NULL,
    created_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS intents_wiki ON intents(wiki_id, seq);
CREATE TRIGGER IF NOT EXISTS intents_no_update BEFORE UPDATE ON intents
BEGIN SELECT RAISE(ABORT, 'intents are append-only'); END;
CREATE TRIGGER IF NOT EXISTS intents_no_delete BEFORE DELETE ON intents
BEGIN SELECT RAISE(ABORT, 'intents are append-only'); END;
CREATE TRIGGER IF NOT EXISTS intents_no_replace BEFORE INSERT ON intents
WHEN EXISTS (SELECT 1 FROM intents WHERE id = NEW.id OR acceptance_id = NEW.acceptance_id)
BEGIN SELECT RAISE(ABORT, 'intents are append-only'); END;
CREATE TABLE IF NOT EXISTS intent_completions (
    intent_id TEXT PRIMARY KEY REFERENCES intents(id),
    how       TEXT NOT NULL CHECK (how IN ('completed', 'abandoned', 'discarded')),
    at        INTEGER NOT NULL
);
CREATE TRIGGER IF NOT EXISTS intent_completions_no_update BEFORE UPDATE ON intent_completions
BEGIN SELECT RAISE(ABORT, 'intent completions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS intent_completions_no_delete BEFORE DELETE ON intent_completions
BEGIN SELECT RAISE(ABORT, 'intent completions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS intent_completions_no_replace BEFORE INSERT ON intent_completions
WHEN EXISTS (SELECT 1 FROM intent_completions WHERE intent_id = NEW.intent_id)
BEGIN SELECT RAISE(ABORT, 'intent completions are append-only'); END;
CREATE TABLE IF NOT EXISTS lease_holders (
    wiki_id       TEXT PRIMARY KEY REFERENCES wikis(id),
    holder        TEXT NOT NULL,
    purpose       TEXT NOT NULL,
    acceptance_id TEXT REFERENCES acceptances(id),
    since         INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS watches (
    account_id TEXT NOT NULL REFERENCES accounts(id),
    wiki_id    TEXT NOT NULL REFERENCES wikis(id),
    kind       TEXT NOT NULL CHECK (kind IN ('page', 'namespace')),
    target     TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    PRIMARY KEY (account_id, wiki_id, kind, target)
);
CREATE INDEX IF NOT EXISTS watches_wiki ON watches(wiki_id, seq);
CREATE TABLE IF NOT EXISTS inbox_events (
    id          TEXT PRIMARY KEY,
    seq         INTEGER NOT NULL,
    account_id  TEXT NOT NULL REFERENCES accounts(id),
    wiki_id     TEXT NOT NULL REFERENCES wikis(id),
    proposal_id TEXT REFERENCES proposals(id),
    kind        TEXT NOT NULL,
    actor       TEXT REFERENCES accounts(id),
    created_at  INTEGER NOT NULL,
    read_at     INTEGER
);
CREATE INDEX IF NOT EXISTS inbox_events_account ON inbox_events(account_id, seq);
";

/// Version 10: which proposals are rollbacks (RFC 004 D-8; handoff 004 T6). A rollback proposal is
/// created as one and stays one: its row says which block it undoes (64 lower-case hex), is written in the
/// same transaction as the proposal, and is **written once** like the records of a proposal's history (the
/// triggers abort any `UPDATE`, `DELETE` and `INSERT` over a key already taken). A proposal with no row
/// is an ordinary one. Nothing else changes: version 9's tables are kept as they are.
const V10: &str = "
CREATE TABLE IF NOT EXISTS proposal_rollbacks (
    proposal_id TEXT PRIMARY KEY REFERENCES proposals(id),
    undoes      TEXT NOT NULL CHECK (length(undoes) = 64 AND undoes NOT GLOB '*[^0-9a-f]*')
);
CREATE TRIGGER IF NOT EXISTS proposal_rollbacks_no_update BEFORE UPDATE ON proposal_rollbacks
BEGIN SELECT RAISE(ABORT, 'proposal rollbacks are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_rollbacks_no_delete BEFORE DELETE ON proposal_rollbacks
BEGIN SELECT RAISE(ABORT, 'proposal rollbacks are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_rollbacks_no_replace BEFORE INSERT ON proposal_rollbacks
WHEN EXISTS (SELECT 1 FROM proposal_rollbacks WHERE proposal_id = NEW.proposal_id)
BEGIN SELECT RAISE(ABORT, 'proposal rollbacks are append-only'); END;
";

/// Version 11 (RFC 004 D-4, D-6, D-14; handoff 004 T4): the message an intent means to seal (the facts a seal
/// is bound to and the reconciliation compares are read from it), the recovery flag, the overlaps kept on a
/// proposal and the system acts (an import, a system change), which have no proposal. The overlaps, the system
/// acts and their ends are written once, as the acceptance records are.
const V11: &str = "
ALTER TABLE intents ADD COLUMN message TEXT NOT NULL DEFAULT '';
CREATE TABLE IF NOT EXISTS wiki_flags (
    wiki_id TEXT PRIMARY KEY REFERENCES wikis(id),
    reason  TEXT NOT NULL CHECK (reason IN ('queued', 'sealed-unconfirmed', 'worktree-dirty', 'interrupted')),
    act     TEXT,
    since   INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS proposal_overlaps (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id TEXT NOT NULL REFERENCES proposals(id),
    revision    INTEGER NOT NULL,
    tip_block   TEXT NOT NULL,
    texts       TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    FOREIGN KEY (proposal_id, revision) REFERENCES proposal_revisions(proposal_id, seq)
);
CREATE INDEX IF NOT EXISTS proposal_overlaps_proposal ON proposal_overlaps(proposal_id, seq);
CREATE TRIGGER IF NOT EXISTS proposal_overlaps_no_update BEFORE UPDATE ON proposal_overlaps
BEGIN SELECT RAISE(ABORT, 'proposal overlaps are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_overlaps_no_delete BEFORE DELETE ON proposal_overlaps
BEGIN SELECT RAISE(ABORT, 'proposal overlaps are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_overlaps_no_replace BEFORE INSERT ON proposal_overlaps
WHEN NEW.seq IS NOT NULL AND EXISTS (SELECT 1 FROM proposal_overlaps WHERE seq = NEW.seq)
BEGIN SELECT RAISE(ABORT, 'proposal overlaps are append-only'); END;
CREATE TABLE IF NOT EXISTS system_acts (
    id             TEXT PRIMARY KEY,
    seq            INTEGER NOT NULL,
    wiki_id        TEXT NOT NULL REFERENCES wikis(id),
    view           TEXT NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('import', 'system')),
    base_block     TEXT NOT NULL,
    message        TEXT NOT NULL,
    message_digest TEXT NOT NULL,
    digest         TEXT NOT NULL,
    bytes          INTEGER NOT NULL,
    created_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS system_acts_wiki ON system_acts(wiki_id, seq);
CREATE TRIGGER IF NOT EXISTS system_acts_no_update BEFORE UPDATE ON system_acts
BEGIN SELECT RAISE(ABORT, 'system acts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS system_acts_no_delete BEFORE DELETE ON system_acts
BEGIN SELECT RAISE(ABORT, 'system acts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS system_acts_no_replace BEFORE INSERT ON system_acts
WHEN EXISTS (SELECT 1 FROM system_acts WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'system acts are append-only'); END;
CREATE TABLE IF NOT EXISTS system_act_ends (
    act_id   TEXT PRIMARY KEY REFERENCES system_acts(id),
    outcome  TEXT NOT NULL CHECK (outcome IN ('sealed', 'failed')),
    block_id TEXT,
    patch_id TEXT,
    word     TEXT,
    at       INTEGER NOT NULL,
    CHECK ((outcome = 'sealed' AND block_id IS NOT NULL AND patch_id IS NOT NULL AND word IS NULL)
        OR (outcome = 'failed' AND word IS NOT NULL AND block_id IS NULL AND patch_id IS NULL))
);
CREATE TRIGGER IF NOT EXISTS system_act_ends_no_update BEFORE UPDATE ON system_act_ends
BEGIN SELECT RAISE(ABORT, 'system act ends are append-only'); END;
CREATE TRIGGER IF NOT EXISTS system_act_ends_no_delete BEFORE DELETE ON system_act_ends
BEGIN SELECT RAISE(ABORT, 'system act ends are append-only'); END;
CREATE TRIGGER IF NOT EXISTS system_act_ends_no_replace BEFORE INSERT ON system_act_ends
WHEN EXISTS (SELECT 1 FROM system_act_ends WHERE act_id = NEW.act_id)
BEGIN SELECT RAISE(ABORT, 'system act ends are append-only'); END;
";

/// Version 12 (RFC 004 D-7; handoff 004 T5): the recovery acts. Recovery is recorded like an acceptance: the
/// plan it made is written before the signer is asked to do any of it, and how it ended is a second record,
/// written once. Both tables carry the write-once triggers of the acceptance records. Nothing else changes:
/// version 11's tables are kept as they are.
const V12: &str = "
CREATE TABLE IF NOT EXISTS recovery_acts (
    id         TEXT PRIMARY KEY,
    seq        INTEGER NOT NULL,
    wiki_id    TEXT NOT NULL REFERENCES wikis(id),
    plan       TEXT NOT NULL CHECK (length(CAST(plan AS BLOB)) <= 262144),
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS recovery_acts_wiki ON recovery_acts(wiki_id, seq);
CREATE TRIGGER IF NOT EXISTS recovery_acts_no_update BEFORE UPDATE ON recovery_acts
BEGIN SELECT RAISE(ABORT, 'recovery acts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS recovery_acts_no_delete BEFORE DELETE ON recovery_acts
BEGIN SELECT RAISE(ABORT, 'recovery acts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS recovery_acts_no_replace BEFORE INSERT ON recovery_acts
WHEN EXISTS (SELECT 1 FROM recovery_acts WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'recovery acts are append-only'); END;
CREATE TABLE IF NOT EXISTS recovery_act_ends (
    act_id  TEXT PRIMARY KEY REFERENCES recovery_acts(id),
    ending  TEXT NOT NULL CHECK (ending IN ('recovered', 'stopped')),
    at      INTEGER NOT NULL
);
CREATE TRIGGER IF NOT EXISTS recovery_act_ends_no_update BEFORE UPDATE ON recovery_act_ends
BEGIN SELECT RAISE(ABORT, 'recovery act ends are append-only'); END;
CREATE TRIGGER IF NOT EXISTS recovery_act_ends_no_delete BEFORE DELETE ON recovery_act_ends
BEGIN SELECT RAISE(ABORT, 'recovery act ends are append-only'); END;
CREATE TRIGGER IF NOT EXISTS recovery_act_ends_no_replace BEFORE INSERT ON recovery_act_ends
WHEN EXISTS (SELECT 1 FROM recovery_act_ends WHERE act_id = NEW.act_id)
BEGIN SELECT RAISE(ABORT, 'recovery act ends are append-only'); END;
";

/// Version 13 (RFC 005 part 2 D-6; handoff K-T4): the derived knowledge index of a tip. Seven tables, all derived
/// from the claim and source files of the tip's one extract and droppable and rebuildable like the title index
/// (INV-5): `knowledge_tips` says an index is whole (the number of rows the other tables hold for it), and the
/// others hold the claims, the sources, the claim-to-claim relations, the claim-to-source links with whether each was
/// judged on the wording the claim has now, the expressions with what their blocks are now, and the files that
/// cannot be read. No column is numeric but the counts that say an index is whole, and none holds a score, a rank
/// or a verdict of truth. Nothing else changes: version 12's tables are kept as they are.
const V13: &str = "
CREATE TABLE IF NOT EXISTS knowledge_tips (
    wiki_id  TEXT NOT NULL,
    tip      TEXT NOT NULL,
    row_count INTEGER NOT NULL,
    built_at INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip)
);
CREATE TABLE IF NOT EXISTS knowledge_claims (
    wiki_id            TEXT NOT NULL,
    tip                TEXT NOT NULL,
    id                 TEXT NOT NULL,
    kind               TEXT NOT NULL,
    lifecycle          TEXT NOT NULL,
    standing           TEXT NOT NULL,
    standing_proposal  TEXT,
    standing_judgement TEXT NOT NULL,
    PRIMARY KEY (wiki_id, tip, id)
);
CREATE TABLE IF NOT EXISTS knowledge_sources (
    wiki_id       TEXT NOT NULL,
    tip           TEXT NOT NULL,
    id            TEXT NOT NULL,
    kind          TEXT NOT NULL,
    availability  TEXT NOT NULL,
    last_reviewed TEXT,
    superseded_by TEXT,
    PRIMARY KEY (wiki_id, tip, id)
);
CREATE TABLE IF NOT EXISTS knowledge_relations (
    wiki_id TEXT NOT NULL,
    tip     TEXT NOT NULL,
    claim   TEXT NOT NULL,
    kind    TEXT NOT NULL,
    other   TEXT NOT NULL,
    PRIMARY KEY (wiki_id, tip, claim, kind, other)
);
CREATE TABLE IF NOT EXISTS knowledge_links (
    wiki_id   TEXT NOT NULL,
    tip       TEXT NOT NULL,
    claim     TEXT NOT NULL,
    source    TEXT NOT NULL,
    relation  TEXT NOT NULL,
    bearing   TEXT NOT NULL,
    judgement TEXT NOT NULL,
    PRIMARY KEY (wiki_id, tip, claim, source)
);
CREATE INDEX IF NOT EXISTS knowledge_links_source ON knowledge_links(wiki_id, tip, source);
CREATE TABLE IF NOT EXISTS knowledge_anchors (
    wiki_id   TEXT NOT NULL,
    tip       TEXT NOT NULL,
    claim     TEXT NOT NULL,
    page_id   TEXT NOT NULL,
    block     TEXT NOT NULL,
    page_path TEXT,
    state     TEXT NOT NULL,
    digest    TEXT,
    PRIMARY KEY (wiki_id, tip, claim, page_id, block)
);
CREATE TABLE IF NOT EXISTS knowledge_invalid (
    wiki_id TEXT NOT NULL,
    tip     TEXT NOT NULL,
    path    TEXT NOT NULL,
    rule    TEXT NOT NULL,
    PRIMARY KEY (wiki_id, tip, path)
);
";

/// Version 14 (the sign-in handoff's U2; the owner's ruling DR-11): invitations, the one-time links an account is
/// made from. The link's secret is kept as its SHA-256, so a copy of this file opens no invitation. An
/// invitation names a wiki and a role together or neither (`CHECK`). Both tables are **written once, in the
/// database itself**: triggers abort any `UPDATE`, any `DELETE` and any `INSERT` over a key already taken
/// (version 3's device, counting every unique key). The use of an invitation is a second record keyed by the
/// invitation, so it can exist once, and its account is unique too: one account is made from at most one
/// invitation. Accounts and wikis are referenced by foreign keys.
const V14: &str = "
CREATE TABLE IF NOT EXISTS invitations (
    id         TEXT PRIMARY KEY,
    hash       TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    wiki_id    TEXT REFERENCES wikis(id),
    role       TEXT CHECK (role IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    CHECK ((wiki_id IS NULL) = (role IS NULL)),
    CHECK (expires_at > created_at)
);
CREATE TRIGGER IF NOT EXISTS invitations_no_update BEFORE UPDATE ON invitations
BEGIN SELECT RAISE(ABORT, 'invitations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS invitations_no_delete BEFORE DELETE ON invitations
BEGIN SELECT RAISE(ABORT, 'invitations are append-only'); END;
CREATE TRIGGER IF NOT EXISTS invitations_no_replace BEFORE INSERT ON invitations
WHEN EXISTS (SELECT 1 FROM invitations WHERE id = NEW.id OR hash = NEW.hash)
BEGIN SELECT RAISE(ABORT, 'invitations are append-only'); END;
CREATE TABLE IF NOT EXISTS invitation_uses (
    invitation_id TEXT PRIMARY KEY REFERENCES invitations(id),
    account_id    TEXT NOT NULL UNIQUE REFERENCES accounts(id),
    at            INTEGER NOT NULL
);
CREATE TRIGGER IF NOT EXISTS invitation_uses_no_update BEFORE UPDATE ON invitation_uses
BEGIN SELECT RAISE(ABORT, 'invitation uses are append-only'); END;
CREATE TRIGGER IF NOT EXISTS invitation_uses_no_delete BEFORE DELETE ON invitation_uses
BEGIN SELECT RAISE(ABORT, 'invitation uses are append-only'); END;
CREATE TRIGGER IF NOT EXISTS invitation_uses_no_replace BEFORE INSERT ON invitation_uses
WHEN EXISTS (SELECT 1 FROM invitation_uses WHERE invitation_id = NEW.invitation_id OR account_id = NEW.account_id)
BEGIN SELECT RAISE(ABORT, 'invitation uses are append-only'); END;
";

/// Version 15 (handoff 004, Q5): the retired keys whose retirement recorded every failure of a wiki's last failed
/// verdict, stored **with the verdict** so that the verification page says so on every load. A JSON array of key ids,
/// `NULL` for none; the database refuses one beside a verdict that is not a failure.
const V15: &str = "
ALTER TABLE wikis ADD COLUMN verification_explained_by TEXT
    CHECK (verification_explained_by IS NULL OR verification_state = 'failed');
";

/// Version 16 (handoff 005 K3; RFC 005 part 2 D-9): the record of a view's creation, the recorded act the signer's
/// `create-view` is bound to. `view_acts` holds the start, written before the signer is asked, and `view_act_ends` how
/// it ended, written once; both carry the write-once triggers of the system acts. **A name is held by one act that
/// stands** (open or created) in a wiki: the store checks it inside the `IMMEDIATE` transaction and a `BEFORE INSERT`
/// trigger refuses it from any connection; an act that failed took nothing in and frees the name. The name's form is
/// checked by the database too (lower case, 1 - 64 characters of `a-z 0-9 . - _`, starting with a letter or a digit, no
/// `..`, no trailing `.`: the form `inklkunn-core`'s `views::new_name` allows). The creator is an account that is there
/// (as every actor column is). An end names an act that is there, a created view has no word and a failed one always
/// has one that is not empty.
///
/// The same version holds **the review record of RFC 005 part 1 D-1 (d)** (handoff 005 K3 b), four tables written once:
/// `change_claims` (per acceptance and per claim the change touched, *kept* or *dropped*: written after the
/// acceptance record and before the signer is asked, so a trigger refuses it once the acceptance has ended),
/// `claim_reconfirmations` (the curation acceptance that judged a kept claim again, naming the sealed change it
/// re-confirms), `proposal_reconfirms` (a curation proposal made by the reconfirm step names the change it re-confirms
/// from its creation or never; **at most one such proposal is open per change**, a trigger refusing a second from any
/// connection, K22) and `decision_claims` (the claims a *request changes* decision's revision touched).
const V16: &str = "
CREATE TABLE IF NOT EXISTS view_acts (
    id          TEXT PRIMARY KEY,
    seq         INTEGER NOT NULL,
    wiki_id     TEXT NOT NULL REFERENCES wikis(id),
    name        TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 64
                    AND name = lower(name)
                    AND name GLOB '[a-z0-9]*'
                    AND name NOT GLOB '*[^a-z0-9._-]*'
                    AND name NOT GLOB '*..*'
                    AND name NOT GLOB '*.'),
    forked_from TEXT NOT NULL CHECK (length(forked_from) >= 1),
    from_block  TEXT NOT NULL CHECK (length(from_block) >= 1),
    kind        TEXT NOT NULL CHECK (kind IN ('experimental', 'interpretation')),
    created_by  TEXT NOT NULL REFERENCES accounts(id),
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS view_acts_wiki ON view_acts(wiki_id, seq);
CREATE INDEX IF NOT EXISTS view_acts_name ON view_acts(wiki_id, name);
CREATE TRIGGER IF NOT EXISTS view_acts_no_update BEFORE UPDATE ON view_acts
BEGIN SELECT RAISE(ABORT, 'view acts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS view_acts_no_delete BEFORE DELETE ON view_acts
BEGIN SELECT RAISE(ABORT, 'view acts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS view_acts_no_replace BEFORE INSERT ON view_acts
WHEN EXISTS (SELECT 1 FROM view_acts WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'view acts are append-only'); END;
CREATE TRIGGER IF NOT EXISTS view_acts_name_held BEFORE INSERT ON view_acts
WHEN EXISTS (SELECT 1 FROM view_acts a LEFT JOIN view_act_ends e ON e.act_id = a.id
             WHERE a.wiki_id = NEW.wiki_id AND a.name = NEW.name
               AND (e.act_id IS NULL OR e.outcome = 'created'))
BEGIN SELECT RAISE(ABORT, 'view acts are append-only: the name is held by an act that stands'); END;
CREATE TABLE IF NOT EXISTS view_act_ends (
    act_id  TEXT PRIMARY KEY REFERENCES view_acts(id),
    outcome TEXT NOT NULL CHECK (outcome IN ('created', 'failed')),
    word    TEXT CHECK (word IS NULL OR length(word) >= 1),
    at      INTEGER NOT NULL,
    CHECK ((outcome = 'created' AND word IS NULL) OR (outcome = 'failed' AND word IS NOT NULL))
);
CREATE TRIGGER IF NOT EXISTS view_act_ends_no_update BEFORE UPDATE ON view_act_ends
BEGIN SELECT RAISE(ABORT, 'view act ends are append-only'); END;
CREATE TRIGGER IF NOT EXISTS view_act_ends_no_delete BEFORE DELETE ON view_act_ends
BEGIN SELECT RAISE(ABORT, 'view act ends are append-only'); END;
CREATE TRIGGER IF NOT EXISTS view_act_ends_no_replace BEFORE INSERT ON view_act_ends
WHEN EXISTS (SELECT 1 FROM view_act_ends WHERE act_id = NEW.act_id)
BEGIN SELECT RAISE(ABORT, 'view act ends are append-only'); END;
CREATE TABLE IF NOT EXISTS change_claims (
    acceptance_id TEXT NOT NULL REFERENCES acceptances(id),
    claim         TEXT NOT NULL CHECK (length(claim) = 36 AND claim NOT GLOB '*[^0-9a-f-]*'),
    how           TEXT NOT NULL CHECK (how IN ('kept', 'dropped')),
    PRIMARY KEY (acceptance_id, claim)
);
CREATE TRIGGER IF NOT EXISTS change_claims_no_update BEFORE UPDATE ON change_claims
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_claims_no_delete BEFORE DELETE ON change_claims
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_claims_no_replace BEFORE INSERT ON change_claims
WHEN EXISTS (SELECT 1 FROM change_claims WHERE acceptance_id = NEW.acceptance_id AND claim = NEW.claim)
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS change_claims_while_pending BEFORE INSERT ON change_claims
WHEN EXISTS (SELECT 1 FROM acceptance_results WHERE acceptance_id = NEW.acceptance_id)
BEGIN SELECT RAISE(ABORT, 'review records are append-only: the acceptance has ended'); END;
CREATE TABLE IF NOT EXISTS claim_reconfirmations (
    change_id TEXT NOT NULL,
    claim     TEXT NOT NULL,
    by_id     TEXT NOT NULL REFERENCES acceptances(id),
    PRIMARY KEY (change_id, claim, by_id),
    FOREIGN KEY (change_id, claim) REFERENCES change_claims(acceptance_id, claim),
    CHECK (change_id <> by_id)
);
CREATE TRIGGER IF NOT EXISTS claim_reconfirmations_no_update BEFORE UPDATE ON claim_reconfirmations
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS claim_reconfirmations_no_delete BEFORE DELETE ON claim_reconfirmations
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS claim_reconfirmations_no_replace BEFORE INSERT ON claim_reconfirmations
WHEN EXISTS (SELECT 1 FROM claim_reconfirmations
             WHERE change_id = NEW.change_id AND claim = NEW.claim AND by_id = NEW.by_id)
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS claim_reconfirmations_of_a_kept_claim BEFORE INSERT ON claim_reconfirmations
WHEN NOT EXISTS (SELECT 1 FROM change_claims
                 WHERE acceptance_id = NEW.change_id AND claim = NEW.claim AND how = 'kept')
BEGIN SELECT RAISE(ABORT, 'review records are append-only: only a kept claim is re-confirmed'); END;
CREATE TRIGGER IF NOT EXISTS claim_reconfirmations_of_a_sealed_change BEFORE INSERT ON claim_reconfirmations
WHEN NOT EXISTS (SELECT 1 FROM acceptance_results WHERE acceptance_id = NEW.change_id AND outcome = 'sealed')
BEGIN SELECT RAISE(ABORT, 'review records are append-only: only a sealed change is re-confirmed'); END;
CREATE TRIGGER IF NOT EXISTS claim_reconfirmations_while_pending BEFORE INSERT ON claim_reconfirmations
WHEN EXISTS (SELECT 1 FROM acceptance_results WHERE acceptance_id = NEW.by_id)
BEGIN SELECT RAISE(ABORT, 'review records are append-only: the acceptance has ended'); END;
CREATE TABLE IF NOT EXISTS proposal_reconfirms (
    proposal_id TEXT PRIMARY KEY REFERENCES proposals(id),
    change_id   TEXT NOT NULL REFERENCES acceptances(id)
);
CREATE INDEX IF NOT EXISTS proposal_reconfirms_change ON proposal_reconfirms(change_id);
CREATE TRIGGER IF NOT EXISTS proposal_reconfirms_no_update BEFORE UPDATE ON proposal_reconfirms
BEGIN SELECT RAISE(ABORT, 'proposal reconfirms are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_reconfirms_no_delete BEFORE DELETE ON proposal_reconfirms
BEGIN SELECT RAISE(ABORT, 'proposal reconfirms are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_reconfirms_no_replace BEFORE INSERT ON proposal_reconfirms
WHEN EXISTS (SELECT 1 FROM proposal_reconfirms WHERE proposal_id = NEW.proposal_id)
BEGIN SELECT RAISE(ABORT, 'proposal reconfirms are append-only'); END;
CREATE TRIGGER IF NOT EXISTS proposal_reconfirms_one_open BEFORE INSERT ON proposal_reconfirms
WHEN EXISTS (SELECT 1 FROM proposal_reconfirms r JOIN proposals p ON p.id = r.proposal_id
             WHERE r.change_id = NEW.change_id AND p.state IN ('draft', 'held-for-review', 'needs-revision'))
BEGIN SELECT RAISE(ABORT, 'proposal reconfirms are append-only: a proposal that re-confirms this change is open'); END;
CREATE TABLE IF NOT EXISTS decision_claims (
    decision_id TEXT NOT NULL REFERENCES proposal_decisions(id),
    claim       TEXT NOT NULL CHECK (length(claim) = 36 AND claim NOT GLOB '*[^0-9a-f-]*'),
    PRIMARY KEY (decision_id, claim)
);
CREATE TRIGGER IF NOT EXISTS decision_claims_no_update BEFORE UPDATE ON decision_claims
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS decision_claims_no_delete BEFORE DELETE ON decision_claims
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS decision_claims_no_replace BEFORE INSERT ON decision_claims
WHEN EXISTS (SELECT 1 FROM decision_claims WHERE decision_id = NEW.decision_id AND claim = NEW.claim)
BEGIN SELECT RAISE(ABORT, 'review records are append-only'); END;
CREATE TRIGGER IF NOT EXISTS decision_claims_of_a_request BEFORE INSERT ON decision_claims
WHEN NOT EXISTS (SELECT 1 FROM proposal_decisions WHERE id = NEW.decision_id AND kind = 'request-changes')
BEGIN SELECT RAISE(ABORT, 'review records are append-only: only a request for changes records its claims'); END;
";

/// Version 17 (handoff 005 corrections, K9): a source has a name in the derived knowledge index: its title, author or
/// publisher, date and reference, as written and cut at a bound when the index is built. **Derived data only**: the
/// index of every tip is dropped here (all its tables, the tips row with them), so that none is read without the
/// columns it now has; each is rebuilt from the tip's files when it is next asked for (INV-5). Version 16 is the
/// view-creation and review records of another correction (K3).
const V17: &str = "
ALTER TABLE knowledge_sources ADD COLUMN title     TEXT;
ALTER TABLE knowledge_sources ADD COLUMN author    TEXT;
ALTER TABLE knowledge_sources ADD COLUMN date      TEXT;
ALTER TABLE knowledge_sources ADD COLUMN reference TEXT;
DELETE FROM knowledge_tips;
DELETE FROM knowledge_claims;
DELETE FROM knowledge_sources;
DELETE FROM knowledge_relations;
DELETE FROM knowledge_links;
DELETE FROM knowledge_anchors;
DELETE FROM knowledge_invalid;
";

/// Version 18 (handoff 005, K-T10; RFC 005 part 2 D-11): **a claim's thread**, the epistemic discussion of a claim, with
/// the shape and the rules of the proposal comments (version 9): service-authoritative and append-only, a reply in the
/// thread of the comment it answers, and keyed by the wiki and the claim's id. Additive: one table, one index of its own (beside the one the UNIQUE key makes) and
/// four triggers; nothing that exists is touched. `seq` is the comment's place in its thread (from 1), assigned inside
/// the write's immediate transaction, and unique per thread, so that two writers cannot take one place.
const V18: &str = "
CREATE TABLE IF NOT EXISTS claim_comments (
    id         TEXT PRIMARY KEY,
    wiki_id    TEXT NOT NULL REFERENCES wikis(id),
    claim_id   TEXT NOT NULL CHECK (length(claim_id) = 36 AND claim_id NOT GLOB '*[^0-9a-f-]*'),
    seq        INTEGER NOT NULL CHECK (seq >= 1),
    author     TEXT NOT NULL REFERENCES accounts(id),
    parent     TEXT REFERENCES claim_comments(id),
    body       TEXT NOT NULL CHECK (length(body) > 0),
    created_at INTEGER NOT NULL,
    UNIQUE (wiki_id, claim_id, seq)
);
CREATE INDEX IF NOT EXISTS claim_comments_author ON claim_comments(wiki_id, author, created_at);
CREATE TRIGGER IF NOT EXISTS claim_comments_no_update BEFORE UPDATE ON claim_comments
BEGIN SELECT RAISE(ABORT, 'claim comments are append-only'); END;
CREATE TRIGGER IF NOT EXISTS claim_comments_no_delete BEFORE DELETE ON claim_comments
BEGIN SELECT RAISE(ABORT, 'claim comments are append-only'); END;
-- Two unique keys: the id, and the place in a thread. A replacing insert on either would remove the row it collides
-- with without firing the delete trigger.
CREATE TRIGGER IF NOT EXISTS claim_comments_no_replace BEFORE INSERT ON claim_comments
WHEN EXISTS (SELECT 1 FROM claim_comments WHERE id = NEW.id
             OR (wiki_id = NEW.wiki_id AND claim_id = NEW.claim_id AND seq = NEW.seq))
BEGIN SELECT RAISE(ABORT, 'claim comments are append-only'); END;
CREATE TRIGGER IF NOT EXISTS claim_comments_reply_in_thread BEFORE INSERT ON claim_comments
WHEN NEW.parent IS NOT NULL AND NOT EXISTS (SELECT 1 FROM claim_comments
    WHERE id = NEW.parent AND wiki_id = NEW.wiki_id AND claim_id = NEW.claim_id)
BEGIN SELECT RAISE(ABORT, 'a reply is in the thread of the comment it answers'); END;
";

/// Version 19 (RFC 006 D-6 - D-10, D-14; handoff 006 G-T1): **every table the governance and moderation tasks (G-T2 - G-T5)
/// need, in one migration**, so that none of them adds another. Additive: six tables, their indexes and their triggers,
/// nothing that exists is touched, **and nothing is written to any repository on upgrade** (D-14).
///
/// Five tables are **authoritative** (never derived, backed up with the store; CM-7, D-6): `grant_decisions`,
/// `restrictions`, `protections`, `tombstones` and `appeals`. They are written once, in the database itself (version 3's
/// device: a trigger aborts any `UPDATE`, any `DELETE` and any `INSERT` over a key already taken, counting every unique
/// key). The one exception is an appeal, which is **decided once**: an open appeal is updated to *upheld* or *overturned*
/// and nothing else about it ever changes. `role_grants` stays the current state, and a grant decision is its record.
///
/// - **Shared words.** A rung is one of the six roles. `policy_block` is the block that set the rules in force when the act
///   was done (`NULL`: the wiki had no governance files, so the defaults), in lower-case hex. Every free text is bounded in
///   **bytes** (a reason or a note 4 KiB, as a proposal's reasons; an appeal 16 KiB, as a comment) and every account is a
///   foreign key. **A measure** (a restriction, a temporary protection) is a chain of events: `set`, then `renewed` any
///   number of times, then at most one `lifted` (early, by a person) or `reversed` (by the overturn of an appeal). Each
///   event after the first names the one it follows (`of_event`), and an event is followed by at most one (a partial unique
///   index), so a chain is a line, and *active* is the end of it that has not been followed and has not run out. Expiry is
///   a time (`expires_at`), never a row written later: nothing has to run for a measure to end.
/// - **`grant_decisions`:** who acted (`NULL` with no rung: the operator, from the command line), their rung, the rules,
///   the person, the rung before and after (`NULL` is no grant, so a revocation is a decision whose `rung_after` is `NULL`;
///   a decision that changes nothing is refused), an optional reason, and the appeal whose overturn it is.
/// - **`restrictions`:** one account's rung in one wiki capped at `observer`, with a reason and an end. The person and the
///   one who acts are never the same.
/// - **`protections`:** a *temporary* protection of a page or a namespace (`kind` and `target`), requiring review or a
///   rung. A standing protection is the policy's, in the repository, and not here.
/// - **`tombstones`:** a hide or an unhide of an item (`item_kind` of four, `item_id`, and `item_ref` for what an id does
///   not name alone: a proposal's revision number, a page's path) and of its parts (content, summary, author, as three
///   flags), with a reason category of six (a closed set), an optional note for the `moderate` rung, the account the item
///   is about, and, for a hide that declined an open proposal, the decline's own record (`proposal_transitions.seq`). An
///   unhide follows a hide, names the same item and parts, and restores exactly. **An act** (a moderator's one hide of a
///   change, a record for each page it wrote and one for its summary and author) is its events' shared `act_id`: they have
///   one time, one person, one wiki and one kind (a trigger), so an overturn reverses exactly an act and the audit trail
///   names it. **The hidden text is not here**: the store holds the fact of a hiding, never the content.
/// - **`appeals`:** the action appealed (`action_kind` of five and its id: a record's id, or for a decline the
///   transition's number), who took it (so that the database itself refuses a decision by that person, and by the
///   appellant), the appellant's text, and how it stands: open, upheld or overturned, with the decision's time, person and
///   reason, and for an overturn the id of the record that reversed the action (of the appealed kind). **One open appeal
///   per action per person** is a partial unique index (and a trigger, for a replacing insert, which a unique index alone
///   would let through); the quotas of RFC 004 D-11 count appeals by time and by appellant, as they count proposals. A
///   reversal that names an appeal must name an **open** appeal of its own kind in its own wiki.
///
/// The sixth table is **derived** and dropped and rebuilt with the rest of the derived store (INV-5, CM-7):
/// `governance_derived`, one row per wiki and tip: the block that last set the rules, the two files as they were read
/// (normalized, bounded) and the fault that makes a wiki refuse writes (file, line, rule). **Show hidden content** has no
/// table: it is an event of the audit trail (version 3), which already is append-only, one line, never content, and
/// names the principal, the action and the tombstone it opened.
///
/// Three more tables are **derived** the same way (handoff G-T4, corrections J-1, J-2): `write_state`, `write_blocks` and
/// `path_writes`, the index of **who wrote each path** in each view's log, which the one reader of a page's text at a block asks
/// to know whose text it is. They are of blocks, not of tips, and hold no text: paths, block ids and what each block did to them.
/// Two small tables go with them (corrections L-1, L-9): `write_generation`, how many times a view's index was dropped, and
/// `write_absent`, the blocks found not to be in a view's log (with the view's tip), so that the question is not asked for every request.
const V19: &str = "
-- The two records of a decision that name **the rung it was made at and the rules it was made under** (RFC 006 D-5;
-- acceptance criterion 12). Added here, in the one migration of this release, so that no later version has to: a column of a
-- row written before this migration is NULL (a decision of 0.3.0 says no rung and no block, and is shown as one that
-- recorded none), and a row's own columns never change (the triggers above refuse an UPDATE).
ALTER TABLE proposal_decisions ADD COLUMN reviewer_rung TEXT
    CHECK (reviewer_rung IS NULL OR reviewer_rung IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin'));
ALTER TABLE proposal_decisions ADD COLUMN policy_block TEXT
    CHECK (policy_block IS NULL OR (length(policy_block) BETWEEN 1 AND 128 AND policy_block NOT GLOB '*[^0-9a-f]*'));
ALTER TABLE acceptances ADD COLUMN policy_block TEXT
    CHECK (policy_block IS NULL OR (length(policy_block) BETWEEN 1 AND 128 AND policy_block NOT GLOB '*[^0-9a-f]*'));
ALTER TABLE acceptances ADD COLUMN hatch INTEGER NOT NULL DEFAULT 0 CHECK (hatch IN (0, 1));
CREATE TABLE IF NOT EXISTS appeals (
    id              TEXT PRIMARY KEY,
    seq             INTEGER NOT NULL,
    wiki_id         TEXT NOT NULL REFERENCES wikis(id),
    at              INTEGER NOT NULL,
    action_kind     TEXT NOT NULL CHECK (action_kind IN ('decline', 'tombstone', 'restriction', 'protection', 'grant')),
    action_id       TEXT NOT NULL CHECK (length(action_id) BETWEEN 1 AND 36 AND action_id NOT GLOB '*[^0-9a-f-]*'),
    action_actor    TEXT REFERENCES accounts(id),
    appellant       TEXT NOT NULL REFERENCES accounts(id),
    text            TEXT NOT NULL CHECK (length(CAST(text AS BLOB)) BETWEEN 1 AND 16384),
    status          TEXT NOT NULL CHECK (status IN ('open', 'upheld', 'overturned')),
    decided_at      INTEGER,
    decided_by      TEXT REFERENCES accounts(id),
    decision_reason TEXT CHECK (decision_reason IS NULL OR length(CAST(decision_reason AS BLOB)) BETWEEN 1 AND 4096),
    overturn_id     TEXT CHECK (overturn_id IS NULL OR (length(overturn_id) BETWEEN 1 AND 36 AND overturn_id NOT GLOB '*[^0-9a-f-]*')),
    -- The rung the decider held and the block of the rules it was held under, written with the decision (acceptance criterion 12).
    -- NULL rung is *not recorded*; NULL block with a rung is the default rules.
    decided_rung    TEXT CHECK (decided_rung IS NULL OR decided_rung IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    decided_block   TEXT CHECK (decided_block IS NULL OR (length(decided_block) BETWEEN 1 AND 128 AND decided_block NOT GLOB '*[^0-9a-f]*')),
    CHECK (decided_block IS NULL OR decided_rung IS NOT NULL),
    CHECK ((status = 'open' AND decided_at IS NULL AND decided_by IS NULL AND decision_reason IS NULL AND overturn_id IS NULL
            AND decided_rung IS NULL AND decided_block IS NULL)
        OR (status = 'upheld' AND decided_at IS NOT NULL AND decided_by IS NOT NULL AND decision_reason IS NOT NULL AND overturn_id IS NULL)
        OR (status = 'overturned' AND decided_at IS NOT NULL AND decided_by IS NOT NULL AND decision_reason IS NOT NULL AND overturn_id IS NOT NULL)),
    CHECK (decided_at IS NULL OR decided_at >= at),
    CHECK (decided_by IS NULL OR decided_by <> appellant),
    CHECK (decided_by IS NULL OR action_actor IS NULL OR decided_by <> action_actor)
);
CREATE UNIQUE INDEX IF NOT EXISTS appeals_one_open ON appeals(wiki_id, action_kind, action_id, appellant) WHERE status = 'open';
CREATE INDEX IF NOT EXISTS appeals_wiki ON appeals(wiki_id, status, seq);
CREATE INDEX IF NOT EXISTS appeals_appellant ON appeals(wiki_id, appellant, at);
CREATE INDEX IF NOT EXISTS appeals_action ON appeals(wiki_id, action_kind, action_id);
CREATE TRIGGER IF NOT EXISTS appeals_no_delete BEFORE DELETE ON appeals
BEGIN SELECT RAISE(ABORT, 'appeals are append-only'); END;
-- Decided once: an open appeal becomes upheld or overturned, and no other column of any appeal ever changes.
CREATE TRIGGER IF NOT EXISTS appeals_decided_once BEFORE UPDATE ON appeals
WHEN NOT (OLD.status = 'open' AND NEW.status IN ('upheld', 'overturned')
          AND NEW.id = OLD.id AND NEW.seq = OLD.seq AND NEW.wiki_id = OLD.wiki_id AND NEW.at = OLD.at
          AND NEW.action_kind = OLD.action_kind AND NEW.action_id = OLD.action_id
          AND NEW.action_actor IS OLD.action_actor AND NEW.appellant = OLD.appellant AND NEW.text = OLD.text)
BEGIN SELECT RAISE(ABORT, 'appeals are append-only: an open appeal is decided once and nothing else about it changes'); END;
-- Two unique keys: the id, and one open appeal per action per person. A replacing insert on either would remove the row
-- it collides with without firing the delete trigger.
CREATE TRIGGER IF NOT EXISTS appeals_no_replace BEFORE INSERT ON appeals
WHEN EXISTS (SELECT 1 FROM appeals WHERE id = NEW.id)
  OR (NEW.status = 'open' AND EXISTS (SELECT 1 FROM appeals WHERE status = 'open' AND wiki_id = NEW.wiki_id
        AND action_kind = NEW.action_kind AND action_id = NEW.action_id AND appellant = NEW.appellant))
BEGIN SELECT RAISE(ABORT, 'appeals are append-only: one open appeal per action per person'); END;
CREATE TRIGGER IF NOT EXISTS appeals_filed_open BEFORE INSERT ON appeals
WHEN NEW.status <> 'open'
BEGIN SELECT RAISE(ABORT, 'appeals are append-only: an appeal is filed open'); END;
-- An overturn names the record that reversed the action: of the appealed kind, in the wiki, naming this appeal.
CREATE TRIGGER IF NOT EXISTS appeals_overturn_recorded BEFORE UPDATE ON appeals
WHEN NEW.status = 'overturned' AND NOT (
       (NEW.action_kind = 'grant' AND EXISTS (SELECT 1 FROM grant_decisions WHERE id = NEW.overturn_id AND wiki_id = NEW.wiki_id AND appeal_id = NEW.id))
    OR (NEW.action_kind = 'restriction' AND EXISTS (SELECT 1 FROM restrictions WHERE id = NEW.overturn_id AND wiki_id = NEW.wiki_id AND appeal_id = NEW.id))
    OR (NEW.action_kind = 'protection' AND EXISTS (SELECT 1 FROM protections WHERE id = NEW.overturn_id AND wiki_id = NEW.wiki_id AND appeal_id = NEW.id))
    OR (NEW.action_kind = 'tombstone' AND EXISTS (SELECT 1 FROM tombstones WHERE id = NEW.overturn_id AND wiki_id = NEW.wiki_id AND appeal_id = NEW.id))
    OR (NEW.action_kind = 'decline' AND EXISTS (SELECT 1 FROM proposal_transitions t JOIN proposals p ON p.id = t.proposal_id
                                                WHERE CAST(t.seq AS TEXT) = NEW.overturn_id AND p.wiki_id = NEW.wiki_id)))
BEGIN SELECT RAISE(ABORT, 'appeals are append-only: an overturn names the record that reversed the action'); END;

CREATE TABLE IF NOT EXISTS grant_decisions (
    id           TEXT PRIMARY KEY,
    seq          INTEGER NOT NULL,
    wiki_id      TEXT NOT NULL REFERENCES wikis(id),
    at           INTEGER NOT NULL,
    actor        TEXT REFERENCES accounts(id),
    actor_rung   TEXT CHECK (actor_rung IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    policy_block TEXT CHECK (policy_block IS NULL OR (length(policy_block) BETWEEN 1 AND 128 AND policy_block NOT GLOB '*[^0-9a-f]*')),
    target       TEXT NOT NULL REFERENCES accounts(id),
    rung_before  TEXT CHECK (rung_before IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    rung_after   TEXT CHECK (rung_after IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    reason       TEXT CHECK (reason IS NULL OR length(CAST(reason AS BLOB)) BETWEEN 1 AND 4096),
    appeal_id    TEXT REFERENCES appeals(id),
    CHECK ((actor IS NULL) = (actor_rung IS NULL)),
    CHECK (rung_before IS NOT rung_after)
);
CREATE INDEX IF NOT EXISTS grant_decisions_wiki ON grant_decisions(wiki_id, seq);
CREATE INDEX IF NOT EXISTS grant_decisions_target ON grant_decisions(wiki_id, target, seq);
CREATE TRIGGER IF NOT EXISTS grant_decisions_no_update BEFORE UPDATE ON grant_decisions
BEGIN SELECT RAISE(ABORT, 'grant decisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS grant_decisions_no_delete BEFORE DELETE ON grant_decisions
BEGIN SELECT RAISE(ABORT, 'grant decisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS grant_decisions_no_replace BEFORE INSERT ON grant_decisions
WHEN EXISTS (SELECT 1 FROM grant_decisions WHERE id = NEW.id)
BEGIN SELECT RAISE(ABORT, 'grant decisions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS grant_decisions_by_appeal BEFORE INSERT ON grant_decisions
WHEN NEW.appeal_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM appeals WHERE id = NEW.appeal_id AND wiki_id = NEW.wiki_id
                                                AND status = 'open' AND action_kind = 'grant')
BEGIN SELECT RAISE(ABORT, 'grant decisions are append-only: an appeal named here is an open appeal of a grant in this wiki'); END;

CREATE TABLE IF NOT EXISTS restrictions (
    id           TEXT PRIMARY KEY,
    seq          INTEGER NOT NULL,
    wiki_id      TEXT NOT NULL REFERENCES wikis(id),
    at           INTEGER NOT NULL,
    event        TEXT NOT NULL CHECK (event IN ('set', 'renewed', 'lifted', 'reversed')),
    account      TEXT NOT NULL REFERENCES accounts(id),
    actor        TEXT NOT NULL REFERENCES accounts(id),
    actor_rung   TEXT NOT NULL CHECK (actor_rung IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    policy_block TEXT CHECK (policy_block IS NULL OR (length(policy_block) BETWEEN 1 AND 128 AND policy_block NOT GLOB '*[^0-9a-f]*')),
    reason       TEXT CHECK (reason IS NULL OR length(CAST(reason AS BLOB)) BETWEEN 1 AND 4096),
    expires_at   INTEGER,
    of_event     TEXT REFERENCES restrictions(id),
    appeal_id    TEXT REFERENCES appeals(id),
    CHECK (actor <> account),
    CHECK ((event = 'set') = (of_event IS NULL)),
    CHECK ((event = 'reversed') = (appeal_id IS NOT NULL)),
    CHECK ((event IN ('set', 'renewed') AND reason IS NOT NULL AND expires_at IS NOT NULL AND expires_at > at)
        OR (event IN ('lifted', 'reversed') AND expires_at IS NULL))
);
CREATE INDEX IF NOT EXISTS restrictions_wiki ON restrictions(wiki_id, seq);
CREATE INDEX IF NOT EXISTS restrictions_account ON restrictions(wiki_id, account, seq);
CREATE UNIQUE INDEX IF NOT EXISTS restrictions_one_follow ON restrictions(of_event) WHERE of_event IS NOT NULL;
CREATE TRIGGER IF NOT EXISTS restrictions_no_update BEFORE UPDATE ON restrictions
BEGIN SELECT RAISE(ABORT, 'restrictions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS restrictions_no_delete BEFORE DELETE ON restrictions
BEGIN SELECT RAISE(ABORT, 'restrictions are append-only'); END;
-- Two unique keys: the id, and the one event that follows another.
CREATE TRIGGER IF NOT EXISTS restrictions_no_replace BEFORE INSERT ON restrictions
WHEN EXISTS (SELECT 1 FROM restrictions WHERE id = NEW.id)
  OR (NEW.of_event IS NOT NULL AND EXISTS (SELECT 1 FROM restrictions WHERE of_event = NEW.of_event))
BEGIN SELECT RAISE(ABORT, 'restrictions are append-only'); END;
CREATE TRIGGER IF NOT EXISTS restrictions_follow_a_set BEFORE INSERT ON restrictions
WHEN NEW.of_event IS NOT NULL AND NOT EXISTS (SELECT 1 FROM restrictions p WHERE p.id = NEW.of_event
       AND p.wiki_id = NEW.wiki_id AND p.account = NEW.account AND p.event IN ('set', 'renewed') AND p.at <= NEW.at)
BEGIN SELECT RAISE(ABORT, 'restrictions are append-only: an event follows a set or renewed event of the same account in the same wiki'); END;
CREATE TRIGGER IF NOT EXISTS restrictions_by_appeal BEFORE INSERT ON restrictions
WHEN NEW.appeal_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM appeals WHERE id = NEW.appeal_id AND wiki_id = NEW.wiki_id
                                                AND status = 'open' AND action_kind = 'restriction')
BEGIN SELECT RAISE(ABORT, 'restrictions are append-only: an appeal named here is an open appeal of a restriction in this wiki'); END;

CREATE TABLE IF NOT EXISTS protections (
    id           TEXT PRIMARY KEY,
    seq          INTEGER NOT NULL,
    wiki_id      TEXT NOT NULL REFERENCES wikis(id),
    at           INTEGER NOT NULL,
    event        TEXT NOT NULL CHECK (event IN ('set', 'renewed', 'lifted', 'reversed')),
    kind         TEXT NOT NULL CHECK (kind IN ('page', 'namespace')),
    target       TEXT NOT NULL CHECK (length(CAST(target AS BLOB)) BETWEEN 1 AND 1024),
    requirement  TEXT CHECK (requirement IN ('review', 'rung')),
    rung         TEXT CHECK (rung IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    actor        TEXT NOT NULL REFERENCES accounts(id),
    actor_rung   TEXT NOT NULL CHECK (actor_rung IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    policy_block TEXT CHECK (policy_block IS NULL OR (length(policy_block) BETWEEN 1 AND 128 AND policy_block NOT GLOB '*[^0-9a-f]*')),
    reason       TEXT CHECK (reason IS NULL OR length(CAST(reason AS BLOB)) BETWEEN 1 AND 4096),
    expires_at   INTEGER,
    of_event     TEXT REFERENCES protections(id),
    appeal_id    TEXT REFERENCES appeals(id),
    CHECK ((event = 'set') = (of_event IS NULL)),
    CHECK ((event = 'reversed') = (appeal_id IS NOT NULL)),
    CHECK ((event IN ('set', 'renewed') AND requirement IS NOT NULL AND ((requirement = 'rung') = (rung IS NOT NULL))
            AND reason IS NOT NULL AND expires_at IS NOT NULL AND expires_at > at)
        OR (event IN ('lifted', 'reversed') AND requirement IS NULL AND rung IS NULL AND expires_at IS NULL))
);
CREATE INDEX IF NOT EXISTS protections_wiki ON protections(wiki_id, seq);
CREATE INDEX IF NOT EXISTS protections_target ON protections(wiki_id, kind, target);
CREATE UNIQUE INDEX IF NOT EXISTS protections_one_follow ON protections(of_event) WHERE of_event IS NOT NULL;
CREATE TRIGGER IF NOT EXISTS protections_no_update BEFORE UPDATE ON protections
BEGIN SELECT RAISE(ABORT, 'protections are append-only'); END;
CREATE TRIGGER IF NOT EXISTS protections_no_delete BEFORE DELETE ON protections
BEGIN SELECT RAISE(ABORT, 'protections are append-only'); END;
CREATE TRIGGER IF NOT EXISTS protections_no_replace BEFORE INSERT ON protections
WHEN EXISTS (SELECT 1 FROM protections WHERE id = NEW.id)
  OR (NEW.of_event IS NOT NULL AND EXISTS (SELECT 1 FROM protections WHERE of_event = NEW.of_event))
BEGIN SELECT RAISE(ABORT, 'protections are append-only'); END;
CREATE TRIGGER IF NOT EXISTS protections_follow_a_set BEFORE INSERT ON protections
WHEN NEW.of_event IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protections p WHERE p.id = NEW.of_event
       AND p.wiki_id = NEW.wiki_id AND p.kind = NEW.kind AND p.target = NEW.target AND p.event IN ('set', 'renewed') AND p.at <= NEW.at)
BEGIN SELECT RAISE(ABORT, 'protections are append-only: an event follows a set or renewed event of the same page or namespace in the same wiki'); END;
CREATE TRIGGER IF NOT EXISTS protections_by_appeal BEFORE INSERT ON protections
WHEN NEW.appeal_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM appeals WHERE id = NEW.appeal_id AND wiki_id = NEW.wiki_id
                                                AND status = 'open' AND action_kind = 'protection')
BEGIN SELECT RAISE(ABORT, 'protections are append-only: an appeal named here is an open appeal of a protection in this wiki'); END;

CREATE TABLE IF NOT EXISTS tombstones (
    id                  TEXT PRIMARY KEY,
    seq                 INTEGER NOT NULL,
    wiki_id             TEXT NOT NULL REFERENCES wikis(id),
    at                  INTEGER NOT NULL,
    act_id              TEXT NOT NULL CHECK (length(act_id) BETWEEN 1 AND 128 AND act_id NOT GLOB '*[^0-9a-f-]*'),
    event               TEXT NOT NULL CHECK (event IN ('hide', 'unhide')),
    item_kind           TEXT NOT NULL CHECK (item_kind IN ('proposal-comment', 'claim-comment', 'proposal-revision', 'page-revision')),
    item_id             TEXT NOT NULL CHECK (length(item_id) BETWEEN 1 AND 128 AND item_id NOT GLOB '*[^0-9a-f-]*'),
    item_ref            TEXT CHECK (item_ref IS NULL OR length(CAST(item_ref AS BLOB)) BETWEEN 1 AND 1024),
    hides_content       INTEGER NOT NULL CHECK (hides_content IN (0, 1)),
    hides_summary       INTEGER NOT NULL CHECK (hides_summary IN (0, 1)),
    hides_author        INTEGER NOT NULL CHECK (hides_author IN (0, 1)),
    reason_category     TEXT CHECK (reason_category IN ('personal-data', 'harassment', 'unlawful', 'copyright', 'spam', 'other')),
    note                TEXT CHECK (note IS NULL OR length(CAST(note AS BLOB)) BETWEEN 1 AND 4096),
    affected            TEXT REFERENCES accounts(id),
    actor               TEXT NOT NULL REFERENCES accounts(id),
    actor_rung          TEXT NOT NULL CHECK (actor_rung IN ('observer', 'questioner', 'contributor', 'reviewer', 'maintainer', 'admin')),
    policy_block        TEXT CHECK (policy_block IS NULL OR (length(policy_block) BETWEEN 1 AND 128 AND policy_block NOT GLOB '*[^0-9a-f]*')),
    of_event            TEXT REFERENCES tombstones(id),
    appeal_id           TEXT REFERENCES appeals(id),
    declined_transition INTEGER REFERENCES proposal_transitions(seq),
    CHECK (hides_content + hides_summary + hides_author >= 1),
    CHECK (item_kind NOT IN ('proposal-comment', 'claim-comment') OR (hides_summary = 0 AND item_ref IS NULL)),
    CHECK (item_kind <> 'proposal-revision' OR (item_ref IS NOT NULL AND item_ref NOT GLOB '*[^0-9]*')),
    CHECK (declined_transition IS NULL OR item_kind = 'proposal-revision'),
    CHECK ((event = 'hide' AND of_event IS NULL AND appeal_id IS NULL AND reason_category IS NOT NULL)
        OR (event = 'unhide' AND of_event IS NOT NULL AND reason_category IS NULL AND declined_transition IS NULL))
);
CREATE INDEX IF NOT EXISTS tombstones_wiki ON tombstones(wiki_id, seq);
CREATE INDEX IF NOT EXISTS tombstones_item ON tombstones(wiki_id, item_kind, item_id);
CREATE INDEX IF NOT EXISTS tombstones_act ON tombstones(wiki_id, act_id);
-- The hides that cover a change's text, newest first: what a reader of a page's text at a block asks first (one row, or none).
CREATE INDEX IF NOT EXISTS tombstones_content_hides ON tombstones(wiki_id, seq)
    WHERE event = 'hide' AND hides_content = 1 AND item_kind IN ('page-revision', 'proposal-revision');
CREATE UNIQUE INDEX IF NOT EXISTS tombstones_one_unhide ON tombstones(of_event) WHERE of_event IS NOT NULL;
CREATE TRIGGER IF NOT EXISTS tombstones_no_update BEFORE UPDATE ON tombstones
BEGIN SELECT RAISE(ABORT, 'tombstones are append-only'); END;
CREATE TRIGGER IF NOT EXISTS tombstones_no_delete BEFORE DELETE ON tombstones
BEGIN SELECT RAISE(ABORT, 'tombstones are append-only'); END;
CREATE TRIGGER IF NOT EXISTS tombstones_no_replace BEFORE INSERT ON tombstones
WHEN EXISTS (SELECT 1 FROM tombstones WHERE id = NEW.id)
  OR (NEW.of_event IS NOT NULL AND EXISTS (SELECT 1 FROM tombstones WHERE of_event = NEW.of_event))
BEGIN SELECT RAISE(ABORT, 'tombstones are append-only'); END;
-- An unhide restores exactly: it follows a hide of the same item and the same parts, once.
CREATE TRIGGER IF NOT EXISTS tombstones_unhide_a_hide BEFORE INSERT ON tombstones
WHEN NEW.of_event IS NOT NULL AND NOT EXISTS (SELECT 1 FROM tombstones h WHERE h.id = NEW.of_event
       AND h.event = 'hide' AND h.wiki_id = NEW.wiki_id AND h.item_kind = NEW.item_kind AND h.item_id = NEW.item_id
       AND h.item_ref IS NEW.item_ref AND h.hides_content = NEW.hides_content AND h.hides_summary = NEW.hides_summary
       AND h.hides_author = NEW.hides_author AND h.at <= NEW.at)
BEGIN SELECT RAISE(ABORT, 'tombstones are append-only: an unhide follows a hide of the same item and parts'); END;
-- The events of one act (`act_id`: one hide of a change, a record for each page it wrote and one for its summary and author)
-- are of one wiki, one time, one person and one kind (hide or unhide): an overturn reverses exactly an act's events.
CREATE TRIGGER IF NOT EXISTS tombstones_one_act BEFORE INSERT ON tombstones
WHEN EXISTS (SELECT 1 FROM tombstones a WHERE a.act_id = NEW.act_id
       AND (a.wiki_id <> NEW.wiki_id OR a.at <> NEW.at OR a.actor <> NEW.actor OR a.event <> NEW.event))
BEGIN SELECT RAISE(ABORT, 'tombstones are append-only: the events of one act are of one wiki, one time, one person and one kind'); END;
CREATE TRIGGER IF NOT EXISTS tombstones_by_appeal BEFORE INSERT ON tombstones
WHEN NEW.appeal_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM appeals WHERE id = NEW.appeal_id AND wiki_id = NEW.wiki_id
                                                AND status = 'open' AND action_kind = 'tombstone')
BEGIN SELECT RAISE(ABORT, 'tombstones are append-only: an appeal named here is an open appeal of a tombstone in this wiki'); END;

CREATE TABLE IF NOT EXISTS governance_derived (
    wiki_id      TEXT NOT NULL,
    tip          TEXT NOT NULL,
    policy_block TEXT CHECK (policy_block IS NULL OR (length(policy_block) BETWEEN 1 AND 128 AND policy_block NOT GLOB '*[^0-9a-f]*')),
    constitution TEXT CHECK (constitution IS NULL OR length(CAST(constitution AS BLOB)) <= 1048576),
    policy       TEXT CHECK (policy IS NULL OR length(CAST(policy AS BLOB)) <= 1048576),
    fault_file   TEXT CHECK (fault_file IS NULL OR length(CAST(fault_file AS BLOB)) BETWEEN 1 AND 256),
    fault_line   INTEGER CHECK (fault_line IS NULL OR fault_line >= 1),
    fault_rule   TEXT CHECK (fault_rule IS NULL OR length(CAST(fault_rule AS BLOB)) BETWEEN 1 AND 128),
    -- What stands at or below the place of a governance file that is not an ordinary file (E-1): the paths found there, as a
    -- JSON list of at most 50, and how many more there were. NULL when there is none to list.
    fault_found  TEXT CHECK (fault_found IS NULL OR length(CAST(fault_found AS BLOB)) BETWEEN 2 AND 400000),
    fault_more   INTEGER CHECK (fault_more IS NULL OR fault_more >= 1),
    built_at     INTEGER NOT NULL,
    PRIMARY KEY (wiki_id, tip),
    CHECK ((fault_file IS NULL) = (fault_rule IS NULL)),
    CHECK (fault_line IS NULL OR fault_file IS NOT NULL),
    CHECK (fault_found IS NULL OR fault_file IS NOT NULL),
    CHECK (fault_more IS NULL OR fault_found IS NOT NULL)
);

-- A retired claim and a retired source say so in the derived knowledge index, with the reason (RFC 006 D-10; handoff G-T6). **Derived
-- data only**, as version 17 did: the index of every tip is dropped here, so that none is read without the columns it now has, and
-- each is rebuilt from the tip's files when it is next asked for (INV-5). A source of 0.3.0 has no `lifecycle` in its file: it is
-- `active`.
ALTER TABLE knowledge_claims ADD COLUMN retired_reason TEXT;
ALTER TABLE knowledge_sources ADD COLUMN lifecycle TEXT NOT NULL DEFAULT 'active' CHECK (lifecycle IN ('active', 'retired'));
ALTER TABLE knowledge_sources ADD COLUMN retired_reason TEXT;
DELETE FROM knowledge_tips;
DELETE FROM knowledge_claims;
DELETE FROM knowledge_sources;
DELETE FROM knowledge_relations;
DELETE FROM knowledge_links;
DELETE FROM knowledge_anchors;
DELETE FROM knowledge_invalid;

-- **Who wrote each path** (RFC 006 D-8; handoff G-T4, corrections J-1, J-2): the text of a path at a block is the text the change that
-- last wrote it gave it, so the reader that must not serve hidden text asks *the last writer of this path at or before this block*,
-- one indexed lookup. **Derived data only**, dropped by the derived store's `clear` and built again from the repository's own record
-- of each block (`show`), oldest block first, as readers need it (INV-5); of blocks and not of tips, since a block's writes never
-- change. Per view, because a view's log is what orders its blocks (a view forked from another shares its first blocks and
-- numbers them its own way). `write_state` says how far a view's index goes and from where; `write_blocks` names its blocks;
-- `path_writes` has one row per path a block wrote, removed or moved (and a row of the path '' for a block that may have written any
-- path, or whose effect cannot be told).
CREATE TABLE IF NOT EXISTS write_state (
    wiki_id      TEXT NOT NULL,
    view         TEXT NOT NULL,
    head_number  INTEGER NOT NULL CHECK (head_number >= 0),
    head_block   TEXT NOT NULL,
    floor_number INTEGER NOT NULL CHECK (floor_number >= 0 AND floor_number <= head_number),
    genesis      INTEGER NOT NULL CHECK (genesis IN (0, 1)),
    PRIMARY KEY (wiki_id, view)
);
CREATE TABLE IF NOT EXISTS write_blocks (
    wiki_id  TEXT NOT NULL,
    view     TEXT NOT NULL,
    number   INTEGER NOT NULL CHECK (number >= 0),
    block_id TEXT NOT NULL,
    proposal TEXT,
    PRIMARY KEY (wiki_id, view, number)
);
CREATE UNIQUE INDEX IF NOT EXISTS write_blocks_by_id ON write_blocks(wiki_id, view, block_id);
CREATE TABLE IF NOT EXISTS path_writes (
    wiki_id   TEXT NOT NULL,
    view      TEXT NOT NULL,
    path      TEXT NOT NULL,
    number    INTEGER NOT NULL CHECK (number >= 0),
    kind      TEXT NOT NULL CHECK (kind IN ('wrote', 'deleted', 'carried', 'maybe', 'opaque')),
    from_path TEXT,
    PRIMARY KEY (wiki_id, view, path, number),
    CHECK ((kind = 'carried') = (from_path IS NOT NULL)),
    CHECK (kind <> 'opaque' OR path = ''),
    CHECK (path <> '' OR kind IN ('maybe', 'opaque'))
);
-- Where a file went: the rows that carry a path from another (a pure rename), found by the path they came from, so that the changes
-- after a block that moved a page can be followed without reading the history (correction L-6).
CREATE INDEX IF NOT EXISTS path_writes_by_origin ON path_writes(wiki_id, view, from_path, number) WHERE from_path IS NOT NULL;

-- The generation of a view's index (handoff G-T4, correction L-9): a number that dropping the index (a reset, or the derived store's `clear`)
-- moves forward and nothing else does. A reader that found the index built reads its generation first and again after the rows, and gives
-- no answer when the two differ: the rows it read may be those of an index that was being dropped. A view with no row is at 0. Kept when
-- the index is dropped: it is what tells a reader that it was.
CREATE TABLE IF NOT EXISTS write_generation (
    wiki_id    TEXT NOT NULL,
    view       TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK (generation >= 0),
    PRIMARY KEY (wiki_id, view)
);
-- The blocks found **not in a view's log** (correction L-1), with the view's tip at the time: a reader that asked once whether a view
-- holds a block, found that it does not, and need not ask again until the view has moved. At most 1,000 a view (the oldest dropped by the
-- store), dropped with the view's index.
CREATE TABLE IF NOT EXISTS write_absent (
    wiki_id  TEXT NOT NULL,
    view     TEXT NOT NULL,
    block_id TEXT NOT NULL,
    view_tip TEXT NOT NULL,
    PRIMARY KEY (wiki_id, view, block_id)
);
";

/// Every migration, in order, by the version it brings the database to. A new version is appended here and
/// [`SCHEMA_VERSION`] moves with it (a test holds the two together).
pub const MIGRATIONS: &[(i64, &str)] = &[
    (1, V1),
    (2, V2),
    (3, V3),
    (4, V4),
    (5, V5),
    (6, V6),
    (7, V7),
    (8, V8),
    (9, V9),
    (10, V10),
    (11, V11),
    (12, V12),
    (13, V13),
    (14, V14),
    (15, V15),
    (16, V16),
    (17, V17),
    (18, V18),
    (19, V19),
];

/// The SQLite-backed store.
#[derive(Debug)]
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

/// The schema version a database records: `None` for a fresh one (no `schema_meta` table), the version
/// otherwise. A table without a readable version row is refused, never treated as fresh.
fn read_version(conn: &Connection) -> Result<Option<i64>> {
    let has_meta: bool = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'",
            [],
            |_| Ok(true),
        )
        .optional()
        .map_err(backend)?
        .unwrap_or(false);
    if !has_meta {
        return Ok(None);
    }
    let found: Option<String> = conn
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(backend)?;
    match found.as_deref().map(str::parse::<i64>) {
        Some(Ok(v)) => Ok(Some(v)),
        Some(Err(_)) => Err(StoreError::SchemaVersion {
            found: found.unwrap_or_default(),
            known: SCHEMA_VERSION,
        }),
        None => Err(StoreError::SchemaVersion {
            found: "(none)".to_owned(),
            known: SCHEMA_VERSION,
        }),
    }
}

/// Run one migration inside the transaction `tx`, then its check, if it has one. Version 6's check is the
/// foreign keys of the rebuilt role grants: a row that refers to an account that is not there refuses the
/// whole migration, by name; the caller's transaction is dropped, which rolls it back.
fn apply_migration(tx: &rusqlite::Transaction<'_>, version: i64, sql: &str) -> Result<()> {
    tx.execute_batch(sql).map_err(backend)?;
    if version == 6 {
        check_role_grants(tx, version)?;
    }
    Ok(())
}

/// `PRAGMA foreign_key_check(role_grants)`: refuse if any grant names an account that does not exist.
fn check_role_grants(conn: &Connection, version: i64) -> Result<()> {
    let mut stmt = conn
        .prepare("PRAGMA foreign_key_check(role_grants)")
        .map_err(backend)?;
    let rowids: Vec<i64> = stmt
        .query_map([], |r| r.get::<_, i64>(1))
        .map_err(backend)?
        .collect::<std::result::Result<_, _>>()
        .map_err(backend)?;
    let Some(first) = rowids.first() else {
        return Ok(());
    };
    // Say which one, by the ids the person who repairs the database will look for.
    let (wiki, account): (String, String) = conn
        .query_row(
            "SELECT wiki_id, account_id FROM role_grants WHERE rowid = ?1",
            params![first],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(backend)?;
    Err(StoreError::MigrationRefused {
        version,
        why: format!(
            "{} role grant(s) refer to an account that does not exist (the first: wiki {wiki}, account {account}); \
             remove those grants or add the accounts, then start again",
            rowids.len()
        ),
    })
}

/// Bring an older database forward. The version is read **again inside the transaction**: a second process
/// may have migrated it between our read and our lock, and migrating twice must not happen.
fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(backend)?;
    let current = read_version(&tx)?.ok_or_else(|| StoreError::SchemaVersion {
        found: "(none)".to_owned(),
        known: SCHEMA_VERSION,
    })?;
    if !(1..=SCHEMA_VERSION).contains(&current) {
        return Err(StoreError::SchemaVersion {
            found: current.to_string(),
            known: SCHEMA_VERSION,
        });
    }
    for (version, sql) in MIGRATIONS {
        if *version > current {
            apply_migration(&tx, *version, sql)?;
        }
    }
    tx.execute(
        "UPDATE schema_meta SET value = ?1 WHERE key = 'schema_version'",
        params![SCHEMA_VERSION.to_string()],
    )
    .map_err(backend)?;
    tx.commit().map_err(backend)
}

fn backend(e: rusqlite::Error) -> StoreError {
    StoreError::Backend(e.to_string())
}

/// Did SQLite refuse the statement because of a `REFERENCES` clause?
fn is_foreign_key_refusal(e: &rusqlite::Error) -> bool {
    matches!(
        e,
        rusqlite::Error::SqliteFailure(code, _)
            if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY
    )
}

/// A uniqueness failure, read from SQLite's own message: which column refused.
fn constraint(e: &rusqlite::Error, record: &WikiRecord) -> Option<StoreError> {
    let rusqlite::Error::SqliteFailure(code, Some(msg)) = e else {
        return None;
    };
    if code.code != rusqlite::ErrorCode::ConstraintViolation {
        return None;
    }
    if msg.contains("wikis.slug") {
        Some(StoreError::SlugTaken(record.slug.to_string()))
    } else if msg.contains("wikis.id") {
        Some(StoreError::IdTaken(record.id.to_string()))
    } else {
        None
    }
}

/// Which column a lookup is by.
#[derive(Debug, Clone, Copy)]
enum Key {
    Id,
    Slug,
}

impl Key {
    const fn column(self) -> &'static str {
        match self {
            Key::Id => "id",
            Key::Slug => "slug",
        }
    }
}

impl SqliteStore {
    /// Open or create the database at `path` (its parent must exist), in WAL mode, with the schema applied.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(backend)?;
        Self::init(conn, true)
    }

    /// An in-memory database (tests).
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(backend)?;
        Self::init(conn, false)
    }

    fn init(mut conn: Connection, file_backed: bool) -> Result<Self> {
        conn.execute_batch(
            "PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;",
        )
        .map_err(backend)?;
        if file_backed {
            let mode: String = conn
                .query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))
                .map_err(backend)?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(StoreError::Backend(format!(
                    "the database could not be put in WAL mode (journal_mode is `{mode}`)"
                )));
            }
        }
        // 1. Read the version before touching anything.
        match read_version(&conn)? {
            // 2a. A fresh database: every migration and the version row in one transaction.
            None => {
                let tx = conn
                    .transaction_with_behavior(TransactionBehavior::Immediate)
                    .map_err(backend)?;
                for (version, sql) in MIGRATIONS {
                    apply_migration(&tx, *version, sql)?;
                }
                tx.execute(
                    "INSERT OR IGNORE INTO schema_meta (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )
                .map_err(backend)?;
                tx.commit().map_err(backend)?;
            }
            Some(v) if v == SCHEMA_VERSION => {}
            // 2b. An older database: forward, in order, in one transaction.
            Some(v) if (1..SCHEMA_VERSION).contains(&v) => migrate(&mut conn)?,
            // 2c. Newer than this build, or not a version at all: refused, nothing written.
            Some(v) => {
                return Err(StoreError::SchemaVersion {
                    found: v.to_string(),
                    known: SCHEMA_VERSION,
                });
            }
        }
        Ok(SqliteStore {
            conn: Mutex::new(conn),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().unwrap_or_else(PoisonError::into_inner);
        f(&mut conn)
    }

    fn fetch(conn: &Connection, key: Key, value: &str) -> Result<WikiRecord> {
        let sql = format!("SELECT {COLUMNS} FROM wikis WHERE {} = ?1", key.column());
        conn.query_row(&sql, params![value], row_to_record)
            .optional()
            .map_err(backend)?
            .ok_or(StoreError::NotFound)
    }
}

/// The stored form of a verdict's explanation: a JSON array of key ids, `None` for none.
fn explanation_text(explained_by: &[String]) -> Option<String> {
    (!explained_by.is_empty())
        .then(|| serde_json::to_string(explained_by).unwrap_or_else(|_| "[]".to_owned()))
}

const COLUMNS: &str = "id, slug, display_name, default_view, default_license, languages, visibility, \
                       policy_bootstrap, prikk_format_version, verification_state, verification_at, \
                       author_key_id, maintainer_key_id, created_at, checkpoint_prikk_version, \
                       checkpoint_refresh_requested_at, attempt_at, attempt_why, verification_explained_by";

fn row_to_record(row: &Row<'_>) -> rusqlite::Result<WikiRecord> {
    let id: String = row.get(0)?;
    let slug: String = row.get(1)?;
    let display_name: String = row.get(2)?;
    let languages: String = row.get(5)?;
    let visibility: String = row.get(6)?;
    let state: String = row.get(9)?;
    let checkpoint_version: Option<String> = row.get(14)?;
    let attempt_at: Option<i64> = row.get(16)?;
    let attempt_why: Option<String> = row.get(17)?;
    let explained_by: Option<String> = row.get(18)?;
    let invalid = |what: &str| {
        rusqlite::Error::InvalidColumnType(0, what.to_owned(), rusqlite::types::Type::Text)
    };
    Ok(WikiRecord {
        id: WikiId::parse(&id).ok_or_else(|| invalid("id"))?,
        slug: Slug::from_stored(&slug).map_err(|_| invalid("slug"))?,
        display_name: DisplayName::new(&display_name).map_err(|_| invalid("display_name"))?,
        default_view: row.get(3)?,
        default_license: row.get(4)?,
        languages: serde_json::from_str(&languages).map_err(|_| invalid("languages"))?,
        visibility: Visibility::parse(&visibility).ok_or_else(|| invalid("visibility"))?,
        policy_bootstrap: row.get(7)?,
        prikk_format_version: row.get(8)?,
        verification: Verification {
            state: VerificationState::parse(&state).ok_or_else(|| invalid("verification_state"))?,
            at: row.get(10)?,
        },
        last_attempt: match (attempt_at, attempt_why) {
            (None, None) => None,
            (Some(at), Some(why)) => Some(Attempt {
                at,
                why: NotRun::parse(&why).ok_or_else(|| invalid("attempt_why"))?,
            }),
            _ => return Err(invalid("attempt")),
        },
        verification_explained_by: match explained_by {
            None => Vec::new(),
            Some(text) => {
                serde_json::from_str(&text).map_err(|_| invalid("verification_explained_by"))?
            }
        },
        author_key_id: row.get(11)?,
        maintainer_key_id: row.get(12)?,
        checkpoint: Checkpoint {
            refreshed_with: checkpoint_version
                .map(|v| PrikkVersion::parse(&v).ok_or_else(|| invalid("checkpoint_prikk_version")))
                .transpose()?,
            refresh_requested_at: row.get(15)?,
        },
        created_at: row.get(13)?,
    })
}

/// The `INSERT` of a wiki's row inside `tx`: the refusals of [`WikiStore::insert`] (a taken slug or id) as typed
/// errors. Nothing is committed here.
pub(crate) fn insert_wiki_row(tx: &rusqlite::Transaction<'_>, record: &WikiRecord) -> Result<()> {
    let languages = serde_json::to_string(&record.languages).unwrap_or_else(|_| "[]".to_owned());
    let r = tx.execute(
                "INSERT INTO wikis (id, slug, display_name, default_view, default_license, languages, visibility, \
                 policy_bootstrap, prikk_format_version, verification_state, verification_at, author_key_id, \
                 maintainer_key_id, created_at, checkpoint_prikk_version, checkpoint_refresh_requested_at, \
                 attempt_at, attempt_why, verification_explained_by, seq) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, \
                 (SELECT COALESCE(MAX(seq), 0) + 1 FROM wikis))",
                params![
                    record.id.as_str(),
                    record.slug.as_str(),
                    record.display_name.as_str(),
                    record.default_view,
                    record.default_license,
                    languages,
                    record.visibility.as_str(),
                    record.policy_bootstrap,
                    record.prikk_format_version,
                    record.verification.state.as_str(),
                    record.verification.at,
                    record.author_key_id,
                    record.maintainer_key_id,
                    record.created_at,
                    record.checkpoint.refreshed_with.as_ref().map(PrikkVersion::as_str),
                    record.checkpoint.refresh_requested_at,
                    record.last_attempt.map(|a| a.at),
                    record.last_attempt.map(|a| a.why.as_str()),
                    explanation_text(&record.verification_explained_by),
                ],
            );
    if let Err(e) = r {
        return Err(constraint(&e, record).unwrap_or_else(|| backend(e)));
    }
    Ok(())
}

impl WikiStore for SqliteStore {
    fn insert(&self, record: WikiRecord) -> Result<WikiRecord> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            insert_wiki_row(&tx, &record)?;
            tx.commit().map_err(backend)?;
            Ok(record)
        })
    }

    fn get(&self, id: &WikiId) -> Result<WikiRecord> {
        self.with(|conn| Self::fetch(conn, Key::Id, id.as_str()))
    }

    fn by_slug(&self, slug: &Slug) -> Result<WikiRecord> {
        self.with(|conn| Self::fetch(conn, Key::Slug, slug.as_str()))
    }

    fn list(&self) -> Result<Vec<WikiRecord>> {
        self.with(|conn| {
            let sql = format!("SELECT {COLUMNS} FROM wikis ORDER BY seq");
            let mut stmt = conn.prepare(&sql).map_err(backend)?;
            let rows = stmt.query_map([], row_to_record).map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(backend)
        })
    }

    fn rename(
        &self,
        id: &WikiId,
        slug: Option<Slug>,
        display_name: Option<DisplayName>,
    ) -> Result<WikiRecord> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let current = Self::fetch(&tx, Key::Id, id.as_str())?;
            let slug = slug.unwrap_or(current.slug);
            let display_name = display_name.unwrap_or(current.display_name);
            let r = tx.execute(
                "UPDATE wikis SET slug = ?1, display_name = ?2 WHERE id = ?3",
                params![slug.as_str(), display_name.as_str(), id.as_str()],
            );
            if let Err(e) = r {
                let probe = WikiRecord {
                    slug: slug.clone(),
                    ..Self::fetch(&tx, Key::Id, id.as_str())?
                };
                return Err(constraint(&e, &probe).unwrap_or_else(|| backend(e)));
            }
            let record = Self::fetch(&tx, Key::Id, id.as_str())?;
            tx.commit().map_err(backend)?;
            Ok(record)
        })
    }

    fn set_verification(&self, id: &WikiId, verification: Verification) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET verification_state = ?1, verification_at = ?2, \
                     attempt_at = NULL, attempt_why = NULL, verification_explained_by = NULL WHERE id = ?3",
                    params![verification.state.as_str(), verification.at, id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn set_verification_explained(
        &self,
        id: &WikiId,
        verification: Verification,
        explained_by: &[String],
    ) -> Result<WikiRecord> {
        crate::record::check_explanation(verification.state, explained_by)?;
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET verification_state = ?1, verification_at = ?2, \
                     attempt_at = NULL, attempt_why = NULL, verification_explained_by = ?3 WHERE id = ?4",
                    params![
                        verification.state.as_str(),
                        verification.at,
                        explanation_text(explained_by),
                        id.as_str()
                    ],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn record_attempt(&self, id: &WikiId, attempt: Attempt) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET attempt_at = ?1, attempt_why = ?2 WHERE id = ?3",
                    params![attempt.at, attempt.why.as_str(), id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn set_keys(
        &self,
        id: &WikiId,
        author_key_id: Option<String>,
        maintainer_key_id: Option<String>,
        prikk_format_version: Option<String>,
    ) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET author_key_id = ?1, maintainer_key_id = ?2, prikk_format_version = ?3 WHERE id = ?4",
                    params![author_key_id, maintainer_key_id, prikk_format_version, id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn request_checkpoint_refresh(&self, id: &WikiId, at: i64) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET checkpoint_refresh_requested_at = COALESCE(checkpoint_refresh_requested_at, ?1) \
                     WHERE id = ?2",
                    params![at, id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }

    fn record_checkpoint_refresh(&self, id: &WikiId, prikk: &PrikkVersion) -> Result<WikiRecord> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "UPDATE wikis SET checkpoint_prikk_version = ?1, checkpoint_refresh_requested_at = NULL \
                     WHERE id = ?2",
                    params![prikk.as_str(), id.as_str()],
                )
                .map_err(backend)?;
            if n == 0 {
                return Err(StoreError::NotFound);
            }
            Self::fetch(conn, Key::Id, id.as_str())
        })
    }
}

fn unreadable(what: &str) -> StoreError {
    StoreError::Backend(format!("a stored {what} could not be read"))
}

fn row_to_grant(row: &Row<'_>) -> rusqlite::Result<RoleGrant> {
    let wiki: String = row.get(0)?;
    let account: String = row.get(1)?;
    let role: String = row.get(2)?;
    let granted_by: Option<String> = row.get(3)?;
    let invalid = |what: &str| {
        rusqlite::Error::InvalidColumnType(0, what.to_owned(), rusqlite::types::Type::Text)
    };
    Ok(RoleGrant {
        wiki: WikiId::parse(&wiki).ok_or_else(|| invalid("wiki_id"))?,
        account: AccountId::parse(&account).ok_or_else(|| invalid("account_id"))?,
        role: Role::parse(&role).ok_or_else(|| invalid("role"))?,
        granted_by: granted_by
            .map(|g| AccountId::parse(&g).ok_or_else(|| invalid("granted_by")))
            .transpose()?,
        granted_at: row.get(4)?,
    })
}

impl RoleStore for SqliteStore {
    fn grant(&self, grant: RoleGrant) -> Result<()> {
        self.with(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(backend)?;
            let wiki_exists = tx
                .query_row(
                    "SELECT 1 FROM wikis WHERE id = ?1",
                    params![grant.wiki.as_str()],
                    |_| Ok(()),
                )
                .optional()
                .map_err(backend)?
                .is_some();
            if !wiki_exists {
                return Err(StoreError::NotFound);
            }
            tx.execute(
                "INSERT INTO role_grants (wiki_id, account_id, role, granted_by, granted_at, seq) \
                 VALUES (?1, ?2, ?3, ?4, ?5, (SELECT COALESCE(MAX(seq), 0) + 1 FROM role_grants)) \
                 ON CONFLICT (wiki_id, account_id) DO UPDATE SET role = excluded.role, \
                 granted_by = excluded.granted_by, granted_at = excluded.granted_at, seq = excluded.seq",
                params![
                    grant.wiki.as_str(),
                    grant.account.as_str(),
                    grant.role.as_str(),
                    grant.granted_by.as_ref().map(AccountId::as_str),
                    grant.granted_at,
                ],
            )
            .map_err(|e| {
                // The foreign keys to `accounts` are the check: an account (or a grantor) that is not there.
                if is_foreign_key_refusal(&e) {
                    StoreError::AccountNotFound
                } else {
                    backend(e)
                }
            })?;
            tx.commit().map_err(backend)
        })
    }

    fn revoke(&self, account: &AccountId, wiki: &WikiId) -> Result<bool> {
        self.with(|conn| {
            let n = conn
                .execute(
                    "DELETE FROM role_grants WHERE wiki_id = ?1 AND account_id = ?2",
                    params![wiki.as_str(), account.as_str()],
                )
                .map_err(backend)?;
            Ok(n > 0)
        })
    }

    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>> {
        self.with(|conn| {
            let stored: Option<String> = conn
                .query_row(
                    "SELECT role FROM role_grants WHERE wiki_id = ?1 AND account_id = ?2",
                    params![wiki.as_str(), account.as_str()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(backend)?;
            stored
                .map(|s| Role::parse(&s).ok_or_else(|| unreadable("role")))
                .transpose()
        })
    }

    fn grants_in(&self, wiki: &WikiId) -> Result<Vec<RoleGrant>> {
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT wiki_id, account_id, role, granted_by, granted_at FROM role_grants \
                     WHERE wiki_id = ?1 ORDER BY seq",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![wiki.as_str()], row_to_grant)
                .map_err(backend)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| unreadable("role grant"))
        })
    }
}

impl AuditSink for SqliteStore {
    fn record(&self, event: &AuditEvent) -> Result<()> {
        let e = event.cleaned();
        self.with(|conn| {
            conn.execute(
                "INSERT INTO audit_log (at, principal, action, resource, outcome, detail) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    e.at,
                    e.principal,
                    e.action,
                    e.resource,
                    e.outcome.as_str(),
                    e.detail
                ],
            )
            .map(|_| ())
            .map_err(backend)
        })
    }
}

impl AuditLog for SqliteStore {
    fn read(&self, after_seq: i64, limit: usize) -> Result<Vec<AuditRecord>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        self.with(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT seq, at, principal, action, resource, outcome, detail FROM audit_log \
                     WHERE seq > ?1 ORDER BY seq LIMIT ?2",
                )
                .map_err(backend)?;
            let rows = stmt
                .query_map(params![after_seq, limit], |r| {
                    let outcome: String = r.get(5)?;
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, String>(4)?,
                        outcome,
                        r.get::<_, String>(6)?,
                    ))
                })
                .map_err(backend)?;
            let mut out = Vec::new();
            for row in rows {
                let (seq, at, principal, action, resource, outcome, detail) =
                    row.map_err(|_| unreadable("audit event"))?;
                let outcome =
                    AuditOutcome::parse(&outcome).ok_or_else(|| unreadable("audit event"))?;
                out.push(AuditRecord {
                    seq,
                    event: AuditEvent {
                        at,
                        principal,
                        action,
                        resource,
                        outcome,
                        detail,
                    },
                });
            }
            Ok(out)
        })
    }
}
