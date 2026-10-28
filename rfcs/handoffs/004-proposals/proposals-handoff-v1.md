# Proposals — Developer Handoff v1 (RFC 004)

| | |
|---|---|
| Document | The implementation package for RFC 004: tasks in build order, the QA checklist drawn from the RFC's acceptance criteria, and the definition of done and handback. A handoff directs implementation and never redefines its RFC; a design conflict goes back to the RFC first. |
| Status | Inherits RFC 004's: **accepted 2026-10-28**. **Assignable** once RFC 002's handoff (T1–T6) and RFC 003's T1–T4 are approved; **T1 and T2 may start** as soon as RFC 002's T5 (the message contract) has landed, since they need only the store and the markup crate. |
| Basis | RFC 004 (D-1 … D-14, both open questions ruled 2026-10-28); RFC 002 (D-1, D-2, D-4, D-5, D-6, D-7, D-9, D-10); RFC 003 (D-1, D-2, D-6, D-13); internal design `WR-*`, `MRG-*`, `CON-*`, `DM-3`, `AZ-3`, `ENF-2`, `ENF-3`; external design `WEB-03`, `WEB-04`, `WEB-05`, `WEB-09`, `API-03 … API-05`, `FL-02 … FL-07`, `FL-10`, `CT-03`, `CT-04`, `CT-08`, `OP-01`, `OP-02`, `OP-04`, `OP-06`; threat model `T-1`, `T-3`, `T-4`, `T-9`, `T-10`, `INV-1`, `INV-4`, `INV-5`, `INV-7`, `INV-10`; M0's spike B report (`../001-foundations/001-foundations-spikes-report.md`) and its data; RFC 010 D-6 (every new crate measured before its commit). |
| Audience | The dev team (the mid-capability model). Read `.git-exclude/roles/mid-capability-model-operating-instructions.md`, then `docs/STATUS.md`, then RFC 002's and RFC 003's review requests (what you inherit), then this. |
| Scope | M2: the proposal records and service, the block-aware merge, the propose form and proposal page, acceptance under the lease through `Accept`, recovery with its kill test, decline/withdraw/supersede/rollback, the inbox and watching, flags and metrics, quotas, the words, the write API, the threat model v0.2. **Not** sign-in or keys (RFC 002), **not** the read path beyond what the proposal pages need (RFC 003), **not** claims (RFC 005), **not** policy as data (RFC 006), **not** bundle import (RFC 007). |

## The two lenses every task is reviewed through

1. **Finally clean, safe and secure, robust and sophisticated.** One write path; no overlap resolved by the
   service; no half-state; every outcome a fixed word a person can act on; nothing transitional left
   reachable unnamed.
2. **No proposer, reviewer, acceptor or API client is left confused or misled.** A state, an outcome or a
   refusal that two readers could read two ways is a defect.

## Before you start

- RFC 002's T1–T6 and RFC 003's T1–T4 are on `main` and approved (or, for T1–T2 here, RFC 002's T5).
- The spike B harness (`tools/corpus/src/histories.rs`) holds the block model the merge formalizes and the
  generator that built the 120 pairs; the data are `../001-foundations/spikes/spike-b-prikk-0.54.1.json`.
  T2 moves the model into product code and turns the data into a test; the tool then uses the product code.
- Every new dependency follows RFC 010 D-6 (measure, record the delta with deny and audit before the commit).
  This RFC expects **none**: the merge is in-crate (IQ-1), the lease is an async mutex, the inbox is store
  records. If a task seems to need a crate, the architect first.
- Every route and endpoint registers with an `Action` or a public reason from its first commit (ENF-3);
  every user-facing string is a catalog entry from its first commit (RFC 003 T2); every prikk-dependent test
  prints its `skipping:` line and `EXPECTED_SKIPS` moves with it in `ci.yml` and `release.yml`.
- `Principal::System` is minted only by core's own import, recovery and system-change entry points (RFC 002
  handoff's notes); `create_view` takes a `ViewCreationAct`; `inklkunn_core::system_message` is the shape
  the composer extends with `Kind: proposal | rollback | import`.

## Notes from the architect's review of 2026-10-28, binding on the tasks below

- **T4, the write lease covers every act that writes inside the repository**, not only acceptances: RFC 003's
  `archive export` and checkpoint refresh (RFC 003 D-11; issues IS-8, IS-9) take the wiki's lease too, so an export
  can never make an acceptance meet prikk's `lock conflict` after its `commit` and leave the wiki needing recovery.
- **T5, a lock holder's liveness is decided by inklkunn's own lease record, never by `prikk unlock`'s process check
  (ledger PL-8).** prikk records only the writer's process id, and inside the sandbox's PID namespace that id means
  nothing: planeter measured `unlock` reporting a holder as "appears running" (recorded pid 2) when none was, and prikk
  confirmed that the check is no evidence there. Recovery clears a stale lock with `unlock --force` (or `--yes`)
  **only** when the durable lease record shows no live holder, says so in the plan, and records it; a test plants a
  lock with a recorded pid that the namespace makes look alive.
- **T5, `doctor`'s unreached-object warning is not damage.** Since prikk 0.54, `verify` counts unreached objects as a
  warning (exit 0) and `doctor` reports `PRIKK-DOCTOR-UNREACHED-OBJECT` warnings; recovery records them and does not
  block on them (planeter D-40, ledger PL-12).

## Task breakdown, in build order

### T1 — The records and the proposal service (RFC 004 D-1, D-11; DM-3, CT-04, CON-6)
- **Purpose.** The proposal as an object with a life, before any page shows it.
- **Change scope.** `inklkunn-store` schema version 3 (migration from 2 explicit and tested): proposals,
  revisions, comments (with their anchors), decisions, acceptances, intents, the lease holder record,
  watches, inbox events; decisions, acceptances and intents **append-only** (an update or delete is refused
  by the store, tested on both backends); ids UUID v7. `inklkunn-core`'s proposal service: create (with the
  base captured from `Storage::views`), revise, save and resume a draft, submit, comment, decide, decline,
  withdraw, supersede, the state machine with every transition named and every illegal one refused with the
  glossary's word; quotas (D-11) checked at the boundary with the limit, the current value and the reset
  time in the refusal; the metrics (D-10) computed from the records.
- **Non-change scope.** No merge, no acceptance, no route.
- **Required tests.** One invariant suite on both stores (create, revise, decide, state transitions, drafts
  invisible to others, append-only refusals, reopen persistence); a property test over the state machine
  (no sequence of operations reaches a state the RFC does not name); schema 2 → 3 migration on a copy of an
  M1 database; every quota refuses by name at its boundary; the metrics agree with a hand count on a
  fixture.

### T2 — The block-aware three-way merge (RFC 004 D-5; MRG-1 … MRG-3; IQ-1)
- **Purpose.** The function that re-authors a moved base, exact and pure.
- **Change scope.** `inklkunn-markup`: the block segmentation as a public function (MK-5: top-level blocks
  from the parser's structure, with `^id`s and trimmed comparison text). `inklkunn-core::merge`: three sides
  per page, frontmatter key by key, body blocks matched by `^id`, then content identity, then an LCS
  alignment; one-side changes taken; same-change-both-sides taken once; both-sides-different is an
  **overlap** carrying both texts; renames merged by page id (an edit lands on the renamed file; removed on
  one side and edited on the other is an overlap); a `MergeOutcome` type whose variants are the RFC's
  words. The measurement hook: for each overlap, whether a line-level three-way merge of the block would
  have been clean, counted and reported (never applied). `tools/corpus` uses the product function.
- **Non-change scope.** No within-block merging; no I/O.
- **Required tests.** The 120 spike pairs as a committed fixture yield **exactly** 99 clean and 21 overlaps,
  each overlap with both texts; properties: `merge(base, base, x) = x`, `merge(base, x, base) = x`,
  `merge(base, x, x) = x`, order of the two sides never changes the clean/overlap verdict, a rename plus an
  edit on the other side lands the edit on the new path; frontmatter: a key changed on one side taken, on
  both sides differently an overlap; never panics on arbitrary input (proptest over the construct soup of
  the markup crate's robustness test); scaling against input length as that test does.

### T3 — The propose form, the proposal page and review (RFC 004 D-2, D-3, D-12; WEB-03, WEB-04, WEB-05)
- **Purpose.** Proposing without markup knowledge or a script; review bound to the revision reviewed.
- **Change scope.** Routes under `/<slug>/-/proposals/…` and the form entry points on a page, a section and a
  block; the form (current text, summary ≤ 120 characters, sources, *Preview*, *Show changes*, *Save as
  draft*, *Submit*), bounds before parsing (proposal 2 MiB, page 1 MiB), service-owned frontmatter keys
  refused unless the role allows, a new page's title through `title_to_path` with the rule named on refusal;
  the proposal page (state, author, target, summary, what changes page by page and block by block, the
  thread, inline comments with the robust selector re-anchored per revision and shown as *orphaned* when
  none matches, decisions per reviewer bound to the revision, the actions the viewer's role allows); the
  discussion page (WEB-05); the catalog entries for every state and outcome (D-12) in English and Japanese
  (preview); the structural tests on every page state.
- **Non-change scope.** No acceptance (T4): the *Accept* action is present and says *not yet available* until
  T4 lands, never a dead control.
- **Required tests.** Route enumeration covers every route; the form round-trips multibyte text and CRLF;
  the sanitizer tests of RFC 003 T3 run on the proposal's rendered content and on diff output; a selector
  re-anchors across a revision that moves the block and orphans when the block is gone; a decision on
  revision 1 is shown as "on revision 1" after revision 2; a draft is invisible to another account and
  survives a session's expiry; every catalog and structural test passes on every page state in both
  languages; a size over the bound is refused before parsing (a counting test on the parser).

### T4 — Acceptance under the lease (RFC 004 D-4, D-6, D-14; WR-3 … WR-7, CON-1, CON-3, CON-6)
- **Purpose.** One atomic act: in the history and accepted, or unchanged and said why.
- **Change scope.** The per-wiki write lease (an async mutex with a durable holder record read at start);
  the acceptance orchestration exactly as D-4 numbers it, with `authorize(_, accept, proposal)` first, the
  tip re-read, the merge (T2), the overlap branch to *needs a revision*, the acceptance record and the intent
  before `Accept::accept`, the ids recorded after, cache invalidation (RFC 003 T3's caches), the verification
  enqueued, the notifications enqueued, the lease released; `^id`s attached to commented blocks as part of
  the change (CM-5); *busy* with depth and estimate after the bounded wait; idempotence by revision;
  `import-folder` and system changes rerouted through the same steps under the system principal (D-14).
- **Non-change scope.** No recovery yet (T5): an incomplete acceptance flags the wiki *needs recovery* and
  refuses writes, which T5 completes.
- **Required tests.** Against a real repository: an acceptance whose base is the tip seals and `prikk log`
  shows the message; one whose base moved with no overlap is re-authored and seals; one with an overlap
  stops before `commit`, records both texts and the repository is unchanged; two acceptances racing on one
  wiki serialize and the second is told *busy* with a depth; a retried accept of the same revision seals
  once (one block in `log`); an unauthorized acceptor is refused before the lease is taken; a failure
  injected before `commit` leaves the repository unchanged and the record *failed* with the word; a failure
  injected after `commit` flags the wiki and reads still serve; `import-folder` produces one block with
  `Kind: import`; RFC 002 D-7's reconciliation finds nothing after the suite.

### T5 — Recovery and the kill test (RFC 004 D-7; CON-4, FL-10, DUR-2)
- **Purpose.** No half-state survives a crash, and nothing is guessed.
- **Change scope.** The recovery procedure at start and on demand per flagged wiki: witnesses (open intents,
  the lease record, prikk `status`, `doctor`, the lock listing, foreign processes), the plan, *complete*
  (`seal` when the queued patch matches the intent's digest and files) or *discard* (`doctor
  --discard-damaged-commits`) with the acceptance marked accordingly, stale locks of the acceptance's kinds
  cleared, `verify`, the reconciliation, the flag cleared; every step an audit record; the administration
  surface shows the plan before and the record after. Two new read verbs in the driver (`status`,
  `doctor` in its read-only form) with cost rules and schema pins, and the signer's recovery operations
  (`seal` alone, `doctor --discard-damaged-commits`, `unlock`) behind the same acceptance-token discipline
  (a recovery act is recorded like an acceptance).
- **Required tests.** **The kill test**: the acceptance runs in a child process that is `SIGKILL`ed at each
  of D-4's steps in turn (a test hook names the step); after each kill, recovery leaves `prikk verify`
  green, exactly the expected blocks in `log`, the acceptance record and intent consistent with them, and
  the plan recorded; a planted queued patch that matches no intent is discarded and reported; a planted
  stale lock is cleared and reported; recovery never runs without the lease; a wiki under recovery serves
  reads and refuses writes with the word.

### T6 — Decline, withdraw, supersede, rollback; the inbox and watching; flags (RFC 004 D-8, D-9, D-10)
- **Change scope.** Decline and withdraw with a required reason through the good-faith template; supersede;
  *Propose a rollback* computing the content-level inverse of a change against its parent and opening a
  proposal with `Kind: rollback` naming the undone block; the inbox at `/-/inbox` with the events D-9 names
  and nothing the recipient could not read; watching a page or a namespace (RD-8); the channel trait with
  the in-app implementation; *minor* and *automated* flags on revisions, filtered in recent changes and
  feeds; machine-assisted content labelled and attributed to the accepting human.
- **Required tests.** A rollback proposal restores the pages' prior text exactly and names the undone change;
  a rollback of a rollback restores the change; decline and withdraw without a reason are refused; the other
  party's inbox has the event and a non-reader of a restricted wiki never receives its content; watch events
  fire on acceptance only; flags filter feeds.

### T7 — The write API (RFC 004 D-13; API-03 … API-05)
- **Change scope.** `POST` proposals, revisions, comments, decisions, acceptances, declines, withdrawals under
  the versioned path; `If-Match` with the revision's ETag (or the view's state id for a new proposal) and a
  problem-details conflict when stale; scopes `propose`, `review`, `accept` intersected with roles; rate
  limits with `Retry-After`; existence-neutral 404s; the OpenAPI document extended and generated from the
  router.
- **Required tests.** The OpenAPI document names every endpoint (drift both ways); every endpoint is in the
  route registry; a stale `If-Match` is a 409 problem detail with a stable `type`; a token with `propose`
  cannot accept; `Retry-After` is present on a refused rate; a private wiki's endpoints answer 404
  identically whether or not it exists.

### T8 — Threat model v0.2, the words, the 0.2.0 readiness report
- **Change scope.** The threat model's write-path and key sections re-verified and extended for the lease,
  the intent journal and recovery (v0.2); the glossary entries for every D-12 word with their Japanese
  preview; the retired-words test extended; the participation metrics exposed on the administration
  surface and as JSON Lines; `docs/STATUS.md`, the CHANGELOG's 0.2.0 section, the readiness report for the
  owner (what is green, what was done by a person, what is deferred, the measurement hook's numbers).

## QA checklist (from RFC 004's acceptance criteria and AC-M2)

- [ ] The 120-pair corpus: exactly 99 clean, 21 overlaps, both texts on every overlap.
- [ ] A moved base with no overlap is re-authored and accepted without the proposer; an overlap stops with
      both texts and the repository unchanged.
- [ ] Two acceptances serialize; the second hears *busy* with a depth; a retried accept seals once.
- [ ] The kill test at every step leaves history, records and intents consistent, with the plan recorded.
- [ ] Decline and withdraw carry reasons; the inbox has the events; no content leaks to a non-reader.
- [ ] A rollback restores the prior text and names the undone change.
- [ ] Every quota refuses by name; a draft survives session expiry; the metrics agree with a hand count.
- [ ] Route enumeration covers every new route and endpoint; the OpenAPI document has no drift.
- [ ] Every catalog and structural test passes on every page state in both languages; the retired-words test
      passes.
- [ ] RFC 002 D-7's reconciliation reports nothing after the whole suite.
- [ ] No new dependency; both test runs (prikk on PATH, shimmed off) recorded with commands and exit codes;
      `EXPECTED_SKIPS` updated; the release rehearsal green on the candidate.
- [ ] The threat model v0.2 is in `docs/src/` and re-verified.

## Definition of done and handback

Done when every task's tests exist and pass in both runs, the gates are green in CI, the threat model is at
v0.2, and the review request is filed as `004-proposals-review-request.md` beside this file with: the
implementation summary; the commits (hash → what landed); every gate's command and exit code, both runs; the
dependency-tree delta (expected: none) with deny and audit; the merge corpus result and the measurement
hook's numbers; the kill-test matrix (step killed → recovery outcome); the accessibility checklist with each
step marked done by a person or by a tool; deviations from this handoff and why; known limitations; open
questions for the architect. The architect reviews against RFC 004, the two lenses above, correctness,
security and evidence, and answers Approved, Conditionally Approved, Corrections Required, Design
Reconsideration Required, Requirements Clarification Required or Owner Decision Required. The 0.2.0 cut is
proposed to the owner with the readiness report and executed only on the owner's authorization (ROADMAP §5).
