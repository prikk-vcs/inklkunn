# Changelog

Releases are bare version tags; each section is the release's notes (assembled into the GitHub release by
`release.yml`). Kept by hand, per release.

## [Unreleased]

M0 — foundations (RFC 001, RFC 010). Nothing serves pages yet.

- **The workspace and its gates.** Eight crates in three layers (`inklkunn-prikk`, `inklkunn-signer`,
  `inklkunn-store`; `inklkunn-markup`, `inklkunn-core`, `inklkunn-auth`; `inklkunn-web`, `inklkunn`) and four
  gates as tests: layering, key reachability (only `inklkunn-core` may depend on the signer; its key store
  stays private), prikk spawn (only the driver and the signer spawn prikk; the driver names no write verb),
  markup purity. Pinned build chain: one compiler for CI and release, MSRV 1.88.0 built separately, actions
  by commit SHA, images by digest, the prikk floor 0.54.0 in one place.
- **`inklkunn-markup`.** Frontmatter in the plain YAML subset (anchors and tags refused, YAML's date and
  boolean coercions kept visible), the Portable Wiki Markdown Layer 2 scanner, MKUP-5 link resolution, HTML
  rendering with the MKUP-12 conventions, advisory lint with guideline ids, and `title_to_path` under a
  property test. wiki-commons' conformance corpus (24 scan cases, 10 resolution cases) passes.
- **`inklkunn-prikk`.** The `Storage` trait and `PrikkCli`: eight read verbs with pinned `schema_version`s
  and typed outcomes, bubblewrap with the repository read-only, a server-kept deadline, a `prlimit` ceiling
  from a per-verb cost table, and a light/heavy admission gate.
- **`inklkunn-store`.** The wiki record, validated slugs, opaque time-ordered ids, the `<root>/<shard>/<id>/`
  layout, in-memory and SQLite stores under one invariant suite.
- **`inklkunn-core` and `inklkunn-signer`.** The `authorize()` seam (default deny), the `Accept` trait, the
  `Assurance` values, the egress guard, the bundle layout, and *create wiki* end to end through a bootstrap
  signer (plain-text key directory; RFC 002 replaces it).
- **The two spikes** (RFC 001 D-10), measured on prikk 0.54.1 and reported under `rfcs/handoffs/001-foundations/`:
  the per-verb cost table is inklkunn's own (`diff` 4.6 × the store size is the expensive read; `tree` and `cat`
  1.0 ×; `verify` 2.0 ×; `branch` and `log` flat), and the content-merge spike shows that prikk's merge is
  confluent for 53 of 120 concurrent-edit pairs while a block-aware three-way merge is clean for 99 — the
  justification for inklkunn's own merge in RFC 004. `tools/corpus` generates both corpora from a seed;
  `tools/spikes/` holds the harnesses. prikk refuses repository paths over 1024 bytes, which bounds a wiki's
  hierarchy depth (about 500 one-character levels).
- **The M0 review's corrections** (2026-10-27; review requests v2). The driver reads a ceiling hit through
  bubblewrap (exit 128 + n, or bwrap's "Out of memory") as a ceiling, charges the admission gate the real 1.5 ×
  ceiling, checks every argument's shape before prikk runs, binds exactly the caller's scratch directory for
  `materialize` and resolves every mount against symlinks, bounds a call's output, and refuses bubblewrap's own
  failures as prerequisites rather than prikk's refusals. The store validates ids, slugs and display names on
  every path including deserialization, refuses a newer schema before touching the file, and runs each change
  in a transaction. *Create wiki* stores its record last and removes a failed attempt's directories. The signer
  exposes only named operations; the bootstrap signer is behind the `bootstrap` feature. The egress guard
  refuses control characters, port 0, Teredo, ORCHID and more, and resolves through a seam. The markup crate
  bounds frontmatter nesting, takes code and heading ranges from CommonMark's parser, replaces constructs at
  their scanned spans, writes the scanner's anchors as ids, never emits a `javascript:` or `data:` href, and
  is linear on every pathological input in a bounded test. `title_to_path` percent-encodes non-ASCII titles
  (prikk refuses non-ASCII paths) exactly reversibly. The gates read manifests in every spelling, scan tests
  and nested files, require the lints and `forbid(unsafe_code)` in every crate, and keep test features out of
  product crates; `pin-check` understands job-level guards and pins `cargo install`; `floor-check` records
  the prikk assets' checksums; CI counts the shimmed self-skips; the release gates run both test passes, join
  the images under one tag and attest them.
- **Conventions and ledgers** (RFC 001 D-11; RFC 010): README, this changelog, `SECURITY.md` stating RR-1, the
  dependency policy with a generated lock inventory that a test keeps equal to `Cargo.lock`, the upstream
  dependency ledger (PK/PL/WC rows), `docs/UPSTREAM.md`, `docs/RELEASING.md`, the mdBook.

RFC 002 and RFC 003 — the first handoffs after M0, in progress. Still nothing serves pages.

- **The patch message** (RFC 002 D-5; handoff 002 T5). One grammar, in `inklkunn-signer::message` and
  re-exported as `inklkunn-core::message`, writes and reads what every signed change carries: a one-line
  summary of at most 120 characters, then `Proposal`, `Proposed-By`, `Accepted-By`, `Base`, `Kind` and
  `Imported-From` once each in that order, for the four kinds (proposal, rollback, import, system). The parser
  is strict and total: only canonical text parses, and an error never repeats the text it refused. A real
  prikk returns a sealed change's message byte for byte, which a test checks for every kind. The text M0
  wrote for a created wiki still parses.
  *Output change:* `DisplayName` now refuses U+2028 and U+2029 as well as controls, as its error text always
  said: a reader that splits lines on them would otherwise see a forged trailer in a signed message.
- **`authorize()` and the roles** (RFC 002 D-2; handoff 002 T2). The decision is a pure rule (`decide`) over a
  snapshot that an `Authorizer` loads itself, so a handler can no longer say that a wiki is public: a
  resource names its wiki and nothing about it. The ladder is observer, questioner, contributor, reviewer,
  maintainer, admin; a token's scope (`read ⊂ propose ⊂ review ⊂ accept ⊂ admin`) is intersected with the
  account's role. A principal with no grant may read what the wiki's visibility lets it read, and nothing else;
  a wiki that is not there and a restricted one it has no role in give the same answer, the same audit event
  and the same number of lookups. The decision fails closed, records consequential decisions (accept,
  moderate, admin, export, import) allowed or not, and refuses an allowed one it could not record. The
  system principal cannot be built outside `inklkunn-core`. `inklkunn-store` gains the role grants and the
  audit sink trait, and its SQLite file now migrates forward from M0's schema (schema version 2).
  Two gates join the others: routes are registered in the web crate's `routes.rs` only (so the route table is
  complete), and no web source names a system principal or builds an account one.
  *Output change:* `inklkunn_core::authorize` (the free function that denied everything) is replaced by
  `Authorizer::authorize`; `Resource::Wiki` no longer carries a visibility; `Principal::Account` carries the
  credential's scope.
- **The key store** (RFC 002 D-3; handoff 002 T3). Each wiki's author and maintainer seeds are sealed in one
  record, `<wiki id>.key.json`, under a master key that Argon2id derives once at start from the operator's
  secret (a file; 16 to 1024 bytes without its trailing line break) and a store header that detects a wrong
  secret when the store is opened. Records use XChaCha20-Poly1305 with a fresh 192-bit nonce each, and bind
  the wiki, key ids, generation and times as associated data, so none can be edited or moved to another wiki.
  A seed is decrypted only into a new 0700 directory on a verified tmpfs with 0600 files, for one write call,
  overwritten and removed afterwards (and on a panic); a removal that fails is reported, never swallowed.
  Randomness comes from `/dev/urandom` with no fallback. Nothing in the module is public, and a new gate fails
  if any public item of the signer crate mentions a seed, a secret or a key directory. The module is built
  and tested here and is wired to `Accept` by the next task. Dependencies: `argon2`, `chacha20poly1305`,
  `zeroize` (+22 lock entries, measured; see the dependency policy).
- **`Accept` and `create_view`** (RFC 002 D-4; handoff 002 T4). `Accept` is now defined in `inklkunn-signer`, the
  crate that implements and enforces it, and re-exported by `inklkunn-core`; `Signer` implements it. Before a byte
  is written the signer reads the acceptance record the token names (through an `AcceptanceLedger`) and
  refuses unless it is pending seal for this wiki and view and the message says exactly what the record says
  (proposal, proposer, acceptor, base, kind). Then: nothing unsealed and a clean worktree, the view's worktree,
  the renames, writes and removals (paths validated whole beforehand: ASCII, prikk's rules, the bundle's
  roots), `commit`, `seal`, read the block back and check it is this change, `verify`. A failure before
  `commit` puts the worktree back and says so; a failure after is `Incomplete` for recovery. A view is created
  only for a recorded act. The first change of a new wiki goes through `RepositoryBootstrap`, which now
  refuses anything but a `system` message to a repository with no history. A start-up sweep removes the
  seed directories a killed server left behind.
  *Output change:* `inklkunn_core::accept` re-exports the signer's types; `PreparedChange.message` is the text
  of a `Message`; `SignerConfig` is built with `SignerConfig::new`.
- **The audit trail and the reconciliation** (RFC 002 D-7, D-9; handoff 002 T6). The audit log is a SQLite
  table that the database itself refuses to update or delete from (two triggers abort with *the audit log is
  append-only*, so a stray `UPDATE` or `DELETE` fails even from a hand-opened connection); the store's schema
  is now version 3 and migrates forward from M0's schema. Every sink cleans each field again on the way in
  (control characters and U+2028/U+2029 become `?`, at most 200 characters), so an event built by hand cannot
  forge a second line. `AuditLog` reads the trail in pages after a position, and `export_jsonl` writes it, oldest
  first, one object per line with fixed keys. The reconciliation (`inklkunn_core::reconcile`) reads every
  view's whole history, a page of 500 blocks at a time, and compares it with the acceptance records and the
  system-act records: a block whose message cannot be read, a seal no record explains, a system change no act
  explains, a record whose block is not in the history, and a record that disagrees with the message it names
  (kind, proposal, proposer, acceptor, base, source) or puts the change in a view that does not hold it. It
  only reads: it repairs nothing, and `record_findings` writes ids and
  a fixed word to the audit trail (outcome *reported*), never a message, a name or a path. Tested against a
  fake storage for every finding and against a real prikk history, where it also leaves the history and the
  current view as they were.
