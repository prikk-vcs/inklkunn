# RFC 004 — Proposals: propose, review, accept and seal; re-author a moved base gently; never lose a half-state

**Status.** **Accepted** by the owner on 2026-10-28 ("RFC 004 is accepted."; both open questions ruled as the
architect recommended); proposed the same day. Handoff: `../handoffs/004-proposals/proposals-handoff-v1.md`.
**Tracks.** ROADMAP M2 (0.2.0), theme T4, risk R2 (measured by M0's spike B) and R3. Requirements
`PR-1 … PR-15`, `PR-17`, `CM-5`, `CM-9`, `CM-10`, `ID-3 … ID-5`, `SEC-3`, `SEC-7`, `PERF-2`, `PERF-4`,
`DUR-2`, `INT-2`, `INT-4`, `INT-7`; external design `WEB-03`, `WEB-04`, `WEB-05`, `WEB-09`, `API-03`,
`API-04`, `API-05`, `FL-02 … FL-07`, `FL-10`, `CT-03`, `CT-04`, `CT-08`, `OP-01`, `OP-02`, `OP-04`,
`OP-06`; internal design `WR-1 … WR-8`, `MRG-1 … MRG-4`, `CON-1 … CON-6`, `DM-3`, `AZ-3`, `ENF-2`,
`ENF-3`, `IQ-1`; threat model `T-1`, `T-3`, `T-4`, `T-9`, `T-10`, `INV-1`, `INV-4`, `INV-5`, `INV-7`,
`INV-10`; wiki-commons `HIST-3`, `HIST-4`, `HIST-8`, `HIST-9`, `AUTH-1 … AUTH-13`, `COLL-3 … COLL-7`.
**Touches.** `inklkunn-core` (the proposal service, the merge, the acceptance orchestration, the intent
journal and recovery, quotas, the metrics), `inklkunn-store` (the proposal, revision, comment, decision,
intent, acceptance and notification records; the lease record), `inklkunn-web` (the propose form, the
proposal page and review, the discussion, the inbox, the write API), `inklkunn-markup` (the block
segmentation the merge shares with MK-5). **Not here:** who a principal is and how the service signs (RFC
002, which this RFC calls), the read path (RFC 003), claims and evidence anchored to blocks (RFC 005),
policy as data and the participation ladder (RFC 006: this RFC uses the bootstrap's fixed role set),
bundle import (RFC 007: this RFC gives `import-folder` its acceptance path).

## Summary

M2 is the thesis's core loop: **every change to a wiki is a proposal**, read before it is written, decided by
an authorized person, and sealed by the service on their behalf. This RFC fixes the proposal as an object
with a URL and a life (D-1), the form that needs no markup knowledge and no script (D-2), review bound to
the revision that was reviewed (D-3), **acceptance as one atomic act** that holds the wiki's lease, re-authors
a proposal whose base moved, seals through RFC 002's `Accept`, and either lands in the history or changes
nothing and says why (D-4, D-6), **the content-level merge that M0's spike justified** (D-5: block-aware,
three-way, never resolving an overlap by itself), **recovery that leaves no half-state** (D-7), undo as a
proposal (D-8), and the quotas, flags, notifications, metrics, words and API around the loop (D-9 … D-13).

What the spike settled: prikk's own merge is confluent for edits to different pages and for a rename beside
an edit (53 of 120 wiki-shaped pairs) and refuses every pair that touches one page; a block-aware three-way
merge is clean for 99 of 120, and the 21 it refuses are two edits to the same paragraph, which are true
conflicts for a person. On any wiki whose reviews take a day, most bases move before acceptance. So the
merge is inklkunn's, it works on blocks, and it is on the path of every acceptance.

## Decisions

- **D-1 — The proposal.** A proposal (DM-3, CT-04) is a record `{id (UUID v7), wiki, view, state, author,
  summary, sources, created}` with one or more **revisions** `{seq, base (the view's RefState id and block id
  when the revision was made), content per page: full page text or removal, declared renames (from, to),
  flags minor | automated, created}`. States: **open** (sub-states *draft*: unsent, visible to its author
  only, PR-10; *held for review*; *needs a revision*: an overlap or a decline-with-changes awaits the
  proposer), **accepted**, **declined**, **withdrawn**, **superseded** (a later proposal names it). A
  proposal is addressed at `/<slug>/-/proposals/<id>` (RFC 003 D-1) and never deleted; its revisions,
  comments, decisions and acceptance records are append-only (decisions and acceptances are never updated).
  A revision supersedes the decisions on earlier revisions for the purpose of acceptance; they stay recorded
  and shown as "on revision N" (WR-2).
- **D-2 — Proposing** (WEB-03, FL-02, PR-3). From a page, a section (a heading's range) or a block, or as a
  new page: a plain-text form with the current text, a summary ("what and why", required, ≤ 120
  characters, the CT-03 first line), optional sources, *Preview* and *Show changes* as server round trips,
  *Save as draft* and *Submit*; no script required. The base is captured when the form opens. Lint findings
  show as suggestions, never blockers (PR-9). Bounds before parsing: proposal 2 MiB, page 1 MiB (PERF-4,
  C-4c); a new page's title goes through `title_to_path` and a refusal names the rule, never the path;
  frontmatter keys the service owns (`kind`, `visibility`, `system`) are refused unless the role allows
  (C-4b); content is data (C-4d). A proposal may touch several pages and declare renames (PR-15). Who may
  propose is `authorize(principal, propose, page | namespace)` (RFC 002 D-2; the bootstrap role set).
  Anonymous proposing follows RFC 002 D-10 and is off by default (OQ-1).
- **D-3 — Review** (WEB-04, FL-03, PR-5). A threaded discussion on the proposal; **inline comments anchored
  to blocks** by a robust selector: the block's `^id` when it has one, else a quote selector (exact text
  with prefix and suffix) plus the block's ordinal, re-anchored on each revision and shown as *orphaned*
  when no anchor matches (COLL-5); **decisions** *approve* or *request changes*, one per reviewer per
  revision, bound to the revision (planeter RFC 005 D-5); required approvals from the policy bootstrap
  (`default`: none required; an acceptor may accept their own proposal only when the policy allows, which
  the bootstrap does for maintainers of a wiki with fewer than three maintainers, so a one-person wiki is
  not deadlocked; RFC 006 makes this data). Review kinds are labelled (KN-7). A reviewer is anyone
  `authorize(_, review, proposal)` allows.
- **D-4 — Acceptance** (WR-3, FL-04, PR-6). One action by a principal `authorize(_, accept, proposal)`
  allows, keyed by the proposal revision (CON-6: a retry finds the record and never seals twice):
  1. take the wiki's **write lease** (CON-1: an in-process async mutex per wiki with a durable holder record;
     a waiter is told *busy* with the queue depth and an estimate after a bounded wait, OP-01, PERF-2);
  2. re-read the view's tip through `Storage::views`; if it equals the revision's base, the change is the
     revision's content; else **re-author** each page by D-5 against the tip;
  3. on an **overlap**: release the lease, record both texts on the proposal, move it to *needs a revision*,
     tell the proposer and the acceptor; nothing in prikk changed (HIST-9);
  4. else write the **acceptance record** (`pending seal`, with the merged content's digest) and the
     **intent** (CON-3: acceptance id, wiki, view, base, message digest), compose the CT-03 message (RFC 002
     D-5; `Kind: proposal`, or `rollback` for D-8), and call `Accept::accept` (RFC 002 D-4) with the
     `AcceptanceToken`;
  5. on success record the block and patch ids, mark the proposal *accepted* and the acceptance *sealed*,
     complete the intent, invalidate the view's caches (DM-6, DM-7), enqueue the verification (OP-06) and
     the notifications, release the lease;
  6. attach a `^id` to any block that a comment anchored to and that had none, as part of the same change
     (CM-5), so the comment's anchor survives into the history.
  The acceptor's role and the policy version are recorded on the acceptance and shown (PR-12).
- **D-5 — The content-level merge** (MRG-1 … MRG-3; IQ-1 decided: in `inklkunn-core`, no new crate). Three
  sides per page: *base* (`cat` at the revision's base block), *theirs* (`cat` at the tip), *ours* (the
  revision's text). **Units are blocks**: the frontmatter as one unit merged key by key, then the body's
  top-level blocks as `inklkunn-markup`'s structure delimits them (MK-5: a block is a paragraph, heading,
  list, quotation, table, code block or container, compared with surrounding whitespace trimmed). Blocks
  are matched across the three sides by `^id` when present, else by content identity and then by a
  longest-common-subsequence alignment of the remaining blocks. A block changed, added or removed on one
  side only is taken; a frontmatter key changed on one side only is taken; **a block or key changed on both
  sides is an overlap unless both sides made the same change**. An overlap is never resolved by inklkunn
  (MRG-2, INT-7): both texts are stored and shown side by side, and a person revises. Renames merge by page
  identity: a page renamed on one side and edited on the other lands the edit on the renamed file; removed
  on one side and edited on the other is an overlap (MRG-3). The merge is a pure function with a committed
  corpus: M0's 120 spike pairs become its test (99 clean, 21 overlaps, exactly). **Within-block merging is
  not done in M2**: the 17 % it could touch are the same paragraph edited twice, where a line-level merge
  would risk a wrong sentence for a small gain; a measurement hook (how many overlaps a line-level merge
  would have declared clean) runs on real wikis before M3 decides.
- **D-6 — Failure is honest** (WR-4, WR-5, DUR-2). A failure **before `commit`** (authorization, lease
  timeout, merge overlap, a prikk refusal of the prepared files) changes nothing in prikk; the acceptance
  record says *failed* with the reason in the glossary's words and the proposal stays open. A failure
  **after `commit`** (the signer's *incomplete*) or a crash leaves an open intent: the wiki is flagged
  *needs recovery*, refuses writes and still serves reads (OP-03) until D-7 completes. Every outcome is one
  of a fixed set of words a person can act on; none is prikk's text.
- **D-7 — Recovery** (CON-4, FL-10; planeter RFC 004 part 4 adapted). At start and after any incomplete
  acceptance, per flagged wiki, under the lease: read the witnesses (open intents, the lease record, prikk
  `status`, `doctor`, the lock listing); when prikk holds exactly the queued patch the intent describes
  (message digest and files match), **complete** it with `seal` and reconcile the acceptance record; when the
  witness does not match, **discard** it (`doctor --discard-damaged-commits`) and mark the acceptance
  *failed*; clear stale locks of the kinds an acceptance leaves; `verify`; run RFC 002 D-7's reconciliation;
  clear the flag. Every step is recorded in the audit log; the administration surface shows the plan before
  and the record after; nothing is rewritten and nothing is guessed (INV-7). **A kill test** is part of
  done: the server is killed at each step of D-4 and recovery leaves the history and the records consistent.
- **D-8 — Decline, withdraw, supersede, undo** (PR-7, PR-8, FL-05, FL-06). Decline (an acceptor) and
  withdraw (the proposer) require a reason, worded by a template that assumes good faith and names the next
  step; the other party is told. A proposal may name one it supersedes. **Undo is a proposal**: *Propose a
  rollback* on a change computes, at content level, the inverse of that change against its parent (the
  pages as they were), opens a proposal with `Kind: rollback` naming the undone block, and goes through D-2
  … D-4 like any other; history is never rewritten and prikk's rollback vocabulary stays internal.
- **D-9 — Notifications** (PR-11). An **inbox** per account (`/-/inbox`) of events that involve the person:
  their proposal reviewed, accepted, declined or needing a revision; a reply or a mention; a watched page
  changed (watching arrives with this RFC, RD-8). Events never carry content the recipient could not read.
  Channels are a trait with one implementation in M2 (in-app); e-mail and digests are a later RFC and the
  person's choice. Everything else is a feed (RFC 003 D-7).
- **D-10 — Flags and the participation metrics** (PR-13; register item 18). Revisions carry *minor* and
  *automated*; recent changes and feeds filter on them; machine-assisted content is labelled and attributed to
  the human who accepted it (C-13a). The service computes, per wiki and period, from the records alone:
  proposals by state, median time to first review and to decision, acceptance and decline rates, distinct
  proposers and first-time proposers, reviewers per proposal. Exposed on the administration surface and as
  JSON Lines (CT-08); the inclusiveness dashboard that interprets them is RFC 006's (GV-9).
- **D-11 — Quotas and rate limits** (PR-14, C-9b). Per principal and per wiki, defaults configurable by the
  operator: open proposals (20 per principal), revisions per proposal (50), accepted bytes per day per
  principal (8 MiB), proposals per hour per principal (30) and per wiki (300); new accounts at half. A
  refusal names the limit, the current value and when it resets (`Retry-After` on the API). The acceptance
  queue is bounded (per wiki, 64 waiting) and visible.
- **D-12 — The words** (RFC 002 D-6; RFC 003 D-6). Proposal states are *draft*, *held for review*, *needs a
  revision*, *accepted*, *declined*, *withdrawn*, *superseded*; the outcomes of an acceptance are *signed
  into the history*, *stopped: changes overlap* (with both texts), *not allowed*, *the wiki is busy*, *the
  wiki needs recovery*, *refused: <named limit>*. The glossary fixes each; the retired-words test applies.
- **D-13 — The write API** (API-03, API-04, API-05; the other half of RFC 003 D-13). `POST` proposals,
  revisions, comments, decisions, acceptances, declines, withdrawals under a versioned path; each write names
  the base (`If-Match` with the proposal revision's ETag, or the view's state id for a new proposal) and
  gets a problem-details conflict when stale; scopes `propose`, `review`, `accept` intersected with roles
  (AZ-3); existence-neutral 404s; the route-enumeration harness covers every endpoint (ENF-3). The OpenAPI
  document grows from RFC 003's.
- **D-14 — Import and system changes reuse the path.** RFC 003 D-12's `import-folder` and every system
  change go through D-4's steps 1, 4 and 5 with the system principal (RFC 002 D-1) and `Kind: import |
  system`; no second write path exists (INV-4).

## What "done" means (acceptance criteria)

AC-M2 of the requirements, plus: the merge corpus (the 120 spike pairs) yields exactly 99 clean merges and 21
overlaps, and every overlap shows both texts; a proposal whose base moved is re-authored and accepted without
the proposer's involvement when there is no overlap; two acceptances racing on one wiki serialize and the
second is told *busy* with a depth; a retried accept of the same revision seals once; the kill test at every
step of D-4 leaves the history, the acceptance records and the intents consistent after recovery, with the
plan recorded; a declined or withdrawn proposal carries its reason and the other party's inbox has the
event; a rollback proposal restores the pages' prior text and names the undone change in its message;
every quota refuses by name; a draft survives session expiry; the metrics agree with a hand count on a
fixture; the route-enumeration test covers every new route and endpoint; RFC 002 D-7's reconciliation finds
nothing to report after the whole suite; the threat model v0.2 covers the write path and the keys and is
re-verified.

## Alternatives considered

- **Delegate the merge to prikk** (commit the proposal against the new tip and let prikk's confluence
  decide). Rejected by the spike: prikk is per file for text; every same-page pair is refused.
- **A diff3 crate.** Deferred (IQ-1): the block model is small, is already the spike's harness, and keeps
  the dependency tree where the policy wants it; a crate is reconsidered only if within-block merging is
  adopted.
- **Line- or word-level merging inside a changed block.** Deferred with a measurement hook (D-5): the gain
  is bounded to the true-conflict cases and the risk is a wrong sentence sealed under a person's name.
- **Optimistic acceptance without a lease.** Rejected: prikk is single-writer per repository with fail-fast
  locks; a lease makes *busy* an honest answer instead of a surprising refusal.
- **Auto-resolving overlaps by recency or by role.** Refused by principle (HIST-9, INT-7): a person decides.
- **Direct edit for maintainers.** Rejected (PR-1): one path, always a proposal record, even when the same
  person proposes and accepts.

## Open questions (ruled by the owner, 2026-10-28)

- A comment's quote selector is re-anchored across revisions of the proposal (D-3); whether it is also
  re-anchored across accepted changes of the page is decided in the handoff after RFC 005's needs are
  clearer. **Ruled: as recommended.**
- The default of required approvals is none, with self-acceptance allowed for maintainers of a wiki with
  fewer than three maintainers (D-3); RFC 006 makes it policy data. **Ruled: as recommended.**

## Sequencing and handoff

On RFC 002 (T1–T6: principals, `authorize()` with roles, the key store, `Accept`, the message contract, the
reconciliation and audit) and RFC 003 T1–T4 (the web skeleton, the catalog and tests, the read service, the
page view). The merge (D-5) and the proposal records (D-1) can start as soon as RFC 002's T5 message
contract lands, since they need only the store and the markup crate. On acceptance the architect writes the
handoff; the dev team builds M2 after M1's review. The threat model moves to v0.2 with this RFC's handoff.
