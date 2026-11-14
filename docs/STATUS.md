# inklkunn — project status

| | |
|---|---|
| Document | The dated brief a new session reads **first**: where the project is, what is pending on whom, and the issue register. Updated at every milestone, RFC disposition and owner ruling. |
| As of | 2026-10-28 — **M0 done: RFCs 001 and 010 approved and moved to `done/`** (review requests v2 approved by the owner); **RFCs 002 and 003 accepted**, 002's handoff assignable and 003's T1–T3 startable; prikk's reply on Unicode paths folded (ship percent-encoded; migrate later); release tags will be SSH-signed (pending the owner's public key); **RFC 004 (proposals, M2) accepted** with its handoff v1. **Later the same day:** the dev team began (handoff 002 T5, `3913137`); a new architect session reviewed the whole record and the upstreams and folded **seven issues (IS-7 … IS-13)** into binding handoff notes, one driver follow-on handoff and ledger v1.1; **DR-9 ruled by the owner the same day: `aws_lc_rs`**. **2026-10-29:** the dev team's handoff 002 (T1 – T8) and handoff 003 (T1 – T3) **reviewed: both Conditionally Approved** (every gate re-run by the architect: 676 tests in both runs, 57 self-skips); corrections in `rfcs/handoffs/002-…/identity-keys-and-signing-corrections-handoff-v1.md` and handoff 003's T4 notes; **one decision asked of the owner (DR-10, key rotation)** and one letter to prikk drafted. **Later the same day:** the letter sent and answered; **DR-10 ruled** (keep the old key by default, retire explicitly; corrections C11); **prikk 0.55.0 released and the first re-baseline run** (suite whole, cost multiples unchanged, floor stays 0.54.0); pushed (`fec2a50`); CI's first run on the pushed tree found **IS-17** (a key-store creation race) |
| Read next | [`src/inklkunn-00-project-charter.md`](src/inklkunn-00-project-charter.md) → [`../ROADMAP.md`](../ROADMAP.md) → [`src/inklkunn-01-requirements-spec.md`](src/inklkunn-01-requirements-spec.md) → [`src/inklkunn-02-external-design.md`](src/inklkunn-02-external-design.md) → [`src/inklkunn-03-threat-model.md`](src/inklkunn-03-threat-model.md) → [`src/inklkunn-04-internal-design.md`](src/inklkunn-04-internal-design.md) → [`../rfcs/README.md`](../rfcs/README.md) |

## Where we are

- **2026-10-24 — startup review complete**; RFC 000 adopted (the 5-folder variant); the roadmap proposed and
  **authorized** the same day with the eight decisions DR-1 … DR-8 ruled as recommended.
- **2026-10-25 — Phase 1 written**: the project charter and the requirements specification v0.1.
- **2026-10-27 — requirements approved by the owner** (*"Approved."*) and baselined. The six open questions
  of its §16 carry the architect's recommendations as the working answers unless the owner rules otherwise.
- **2026-10-27 — Phase 2, the design set v0.1, written** (`docs/src/`): the external design (surfaces, flows,
  contracts, gated surfaces), the threat model (whose headline is RR-1: inklkunn holds its wikis' signing keys
  by DR-4, so the design makes that custody bounded, detectable, attributable and recoverable), and the
  internal design (crates and gates; the storage driver; the signer as the only path to keys and to prikk's
  write verbs; the read and write paths; the content-level merge; recovery; the invariant enforcement).
- **2026-10-27 — M0 begun.** The owner asked the architect to do the first implementation itself, carefully;
  the author-reviews-own-work gap this opens is recorded in the review request and compensated by the gates
  and the corpus. **T1** (commit `086ff86`): the workspace, the four gates as tests, the pinned build chain,
  CI, supply-chain and release workflows. **T2**: `inklkunn-markup` — the plain-YAML-subset frontmatter
  parser (anchors and tags refused), the Layer 2 scanner, the MKUP-5 resolver, the HTML renderer with the
  MKUP-12 conventions, the advisory lint, and `title_to_path` with its property tests; the wiki-commons
  conformance corpus passes 34/34 (24 scan, 10 resolution) on the first run against the reference rules.
  **T3**: `inklkunn-prikk` — the `Storage` trait in inklkunn's words, `PrikkCli` over the eight read verbs with
  pinned `schema_version`s and typed outcomes (refused / unavailable / too large / damaged / unsupported
  schema or version / driver), bubblewrap with the repository bound **read-only** and only `extract`'s scratch
  writable, the server-kept deadline, the `prlimit` ceiling from a provisional cost table (planeter's 0.52
  multiples, re-measured by spike (a)), the light/heavy admission gate; tested against a committed two-block
  fixture repository under both policies, with shims for an older prikk and a doctored schema and a ceiling
  that fails before the loader. **T4**: `inklkunn-store` — the wiki record (DM-1), validated slugs with a
  reserved list, opaque time-ordered ids, the `<root>/<shard>/<id>/` layout, in-memory and SQLite stores
  passing one invariant suite, reopen persistence. **T5**: `inklkunn-core` seams — `authorize()` (RFC 002
  D-2's signature, default deny, no grants yet), the `Accept` trait, the `Assurance` values, the egress
  guard (HTTPS only, public unicast only, pinned addresses, 24 refused literal ranges under test); the CT-02
  bundle layout and *create wiki*, which creates the repository, adopts the maintainer key, writes the
  manifest and a help page, seals the system change and verifies — through `inklkunn-signer`'s **bootstrap
  signer** (plain-text key directory; RFC 002 D-3 replaces it) over the sandbox with the repository
  writable and the key directory read-only; the empty private `key_store` module gives the gate its target.
  **T6**: the two spikes, measured on prikk 0.54.1 (`rfcs/handoffs/001-foundations/001-foundations-spikes-report.md`):
  the cost table is now inklkunn's own measurement (`diff` is the expensive read at 4.6 × S; `tree`/`cat` 1.0 ×;
  `verify` 2.0 ×; `branch`/`log` flat) and the content-merge spike shows prikk's confluence is per file (53 of
  120 concurrent-edit pairs merge) while a block-aware three-way merge is clean for 99 of 120: **R2 is real and
  MRG-4 is the right place** (RFC 004). **T7**: README (IS-1 closed), CHANGELOG, SECURITY.md (RR-1), the two
  ledgers with a lock-inventory test, `UPSTREAM.md`, `RELEASING.md`, the book. No binary serves pages yet; that
  is M1 (RFC 003).
- **2026-10-28 — RFC 004 accepted** by the owner the day it was proposed, both open questions ruled as
  recommended (no required approvals by default, self-acceptance for maintainers of small wikis; comment
  re-anchoring across accepted changes decided in the handoff); handoff v1 written
  (`rfcs/handoffs/004-proposals/proposals-handoff-v1.md`). The RFC (`rfcs/accepted/004-proposals.md`) fixes: the proposal object and its states,
  the no-script form, review bound to revisions, acceptance as one atomic act under the wiki's lease through
  RFC 002's `Accept`, the block-aware three-way merge the spike justified (in-crate, overlaps never resolved
  by the service, the 120 spike pairs as its corpus), recovery with a kill test, undo as a proposal, inbox,
  quotas, metrics, the words and the write API.
- **2026-10-28 — prikk replied** to the non-ASCII-paths letter (the first letter from prikk to inklkunn):
  Unicode paths get a design round in 0.56.0, implementation 0.57.0 at the earliest, no date. **Decision:**
  M1 ships with percent-encoded paths; native paths become a planned migration (PK-21, IS-5).
- **2026-10-27 — M0 reviewed and corrected.** The owner stated the two principles every review applies
  (*finally clean, safe and secure, robust and sophisticated design*; *APIs and UI/UX that cannot confuse or
  mislead*) and asked the architect to review M0 itself. Four fresh reviewing agents (driver; signer and core;
  store, gates and workflows; markup) and the architect's own read found about eighty issues, among them five
  that would have reached users or operators: a ceiling hit under bubblewrap misreported as a damaged
  repository or prikk's refusal; the 1.5 × ceiling headroom lost to a clamp; `materialize` binding an arbitrary
  parent directory read-write; a display name able to inject trailers into a signed message; a failed
  creation leaving a slug taken. All are corrected and each has a test that fails on the old behaviour; the
  review requests **v2** list them (`rfcs/handoffs/001-foundations/001-foundations-review-request-v2.md`,
  `…/010-upstream-and-supply-chain-review-request-v2.md`). Two upstream facts surfaced: **prikk refuses
  non-ASCII repository paths** (PK-21; `title_to_path` now percent-encodes, exactly reversibly; one letter
  drafted) and bubblewrap's own "Out of memory" exit under a ceiling (PK-9). RFC 003 was accepted and its
  handoff v1 written (`rfcs/handoffs/003-read-path-and-rendering/`).
- **2026-10-28 — the dev team began** with handoff 002's T5 (the message contract, `3913137`; `EXPECTED_SKIPS` now
  14); its review request is still to come.
- **2026-10-28 — a new architect session reviewed the record** (this file, the transcripts, the rules, the roadmap,
  the design set, every RFC and handoff, the ledgers and the correspondence) **and the three upstreams as they stand
  today**: prikk is mid-way through 0.55.0 with 0.56.0 scheduled (ledger PK-26); planeter's work of 2026-10-24 … 28
  carried six findings about patterns inklkunn reuses (PL-7 … PL-13); wiki-commons is unchanged. Each finding was
  checked against inklkunn's own code and, where it mattered, **measured on prikk 0.54.1 with the driver's own sandbox
  vector** (scratch repositories only). Result: seven issues (IS-7 … IS-13 below), all disposed of without changing
  any accepted decision — a follow-on handoff for the driver (IS-7), dated binding notes in handoffs 002, 003 and 004,
  two concretizations of RFC 003 (D-2, D-11; internal design SGN-6), ledger v1.1, and one decision request (DR-9).
- **2026-10-28 — DR-9 ruled by the owner: *"aws_lc_rs"*.** The question: which backend verifies OpenID Connect ID
  tokens, now that `jsonwebtoken`'s RustCrypto backend fails both supply-chain gates (RUSTSEC-2026-0387
  `crypto-bigint`, no fixed version reachable; RUSTSEC-2023-0071 `rsa`). The options put: **(a)** `jsonwebtoken`'s
  `aws_lc_rs` backend — both advisories removed rather than ignored, with `crypto-bigint`, `rsa` and 33 other crates,
  no new licence, OIDC behaviour unchanged in planeter's 30 cases, about 2 MB larger, at the price of AWS-LC (C and
  assembly) in the trusted base and of reversing the 2026-09-24 rejection carried from planeter; **(b)** RustCrypto
  with two scoped ignores (pure Rust; public-key verification only; two standing advisories on the sign-in path). The
  architect recommended (a) as one ruling for both projects, since planeter's IS-54 asks the same; **(a) was chosen**.
  Folded: handoff 002's T1 note (the verifier is unblocked: how to adopt and measure it, no ignore, `rsa` and
  `crypto-bigint` absent from the lock), IS-11, ledger PL-10. planeter's records are folded by planeter's own session.
  Meanwhile the dev team committed handoff 002's T5 follow-up and T2 (`bc60dc6`, `fb95846`; local, unreviewed).

- **2026-10-29 — the architect reviewed the dev team's first two packages** (handoff 002 T1 – T8; handoff 003 T1 –
  T3; requests in `.git-exclude/review-requests/`, results in `.git-exclude/reviewed/`). Every gate re-run in a clean worktree:
  fmt, clippy on both compilers, rustdoc, deny, audit all 0; **676 passed, 0 failed** with prikk on `PATH` and with it
  shimmed off, **57 self-skips = `EXPECTED_SKIPS`**. PK-34 and PK-36 reproduced on prikk 0.54.1. **Both Conditionally
  Approved.** The conditions: ten correction items for handoff 002 (C1 – C10, a corrections handoff; the three that
  matter most: a write-verb memory ceiling must never read as a refusal or an unconfirmed seal, each write verb sees
  only the seed it signs with, and one writer per wiki across processes), and for handoff 003, before T4 serves a
  page, **author raw HTML escaped while the profile's subset is empty** (IS-14: today an author can forge an internal
  wiki link) and reads pinned to the tip's block id. Concretized: RFC 002 D-4/D-5 (in the signer crate), RFC 003 D-2,
  D-3, D-6; IQ-3 decided (a confined `curl` in a new lower crate, `inklkunn-fetch`); handoff 004 notes (the lease on the
  cross-process lock, the index warmed after each seal).
- **2026-10-29 — DR-10 ruled by the owner: (a) with (c), final once prikk confirmed it; prikk confirmed it the same
  day.** The question: on prikk, removing a maintainer key from trust is revocation (everything it ever sealed then
  fails verification, PK-34), so a rotation cannot both stop the old key and keep its history verifying. Options:
  **(a)** keep the old key trusted by default, retire it only as an explicit act for a compromise; **(b)** retire by
  default; **(c)** either, plus asking prikk for a rotation that keeps history verifiable. The bundled letter
  (sent; filed in `.git-exclude/upstream/prikk/send/`) asked prikk to confirm the interim design; **prikk's reply**
  (`…/receive/2026-10-29-key-rotation-log-by-block-and-extract-scope.md`) confirmed all three points: revocation is
  retroactive by design; keeping the old key is the correct rotation and no mechanism was missed; a leaked
  still-trusted key can do everything it is trusted for, and a copy of the repository **cannot prove** a block came
  after the successor's adoption. A rotation that keeps history verifiable is **not scheduled**. Folded: RFC 002 D-8
  and its acceptance line; threat model C-1d and a new RR-8; ledger v1.2 (PK-34, PK-36 a defect prikk will fix in a
  release it will name, PK-37 the `extract` scope a candidate); **corrections handoff C11, before 0.1.0**: the
  rotation records where the old key stopped, and a seal under it after that point is reported (inklkunn knows the
  moment, which a copy cannot prove). prikk's reply also measured on **0.55.0, now tagged upstream**: the first
  re-baseline is due.
- **2026-10-29 — the owner's rulings on process:** **the architect may commit and push**; **the dev team may commit
  and push only what it made and what has been approved** (a package pushes after its review, not before); `gh` is
  available for CI and releases. **prikk 0.55.0 is installed** on the build host by the owner (the first re-baseline
  runs against it). **DR-7's reviewer is the owner**, who reads Japanese: the catalog's sign-off is a review request
  to the owner, not a person still to be found.
- **2026-10-29 — release signing (IS-6) settled by the owner:** releases stay fully automated (no approval
  click); the owner authorizes a release and the architect tags it; tags are signed by the owner's existing git
  configuration and checked by readers; the CI signature step (which expected SSH, a mismatch with the owner's
  OpenPGP signing) and `release-signers.toml` are removed.
- **2026-10-29 — the first re-baseline, at prikk 0.55.0** (RFC 010 D-1; the owner installed the binary). The release
  changes nothing inklkunn pins: the suite passes whole (676 passed, 0 failed, no self-skips) in a clean worktree;
  spike A re-run on the same four wiki shapes gives every multiple unchanged, and a 64 MiB attachment shape (bare
  prikk) gives 2.0 × for the reads and 4.99 × for `verify` (IS-12 stands; handoff 003's T6 measures through the
  driver's vector). **The floor stays 0.54.0** (nothing in 0.55.0 is a verb inklkunn needs or a fix a wiki depends
  on). IS-7's report shape was not seen on 0.55.0 (it aborts instead); the follow-on handoff stands. Records: the
  spikes report and `spikes/spike-a-prikk-0.55.0.json`, ledger PK-20 and PK-26.
- **2026-10-29 — pushed; CI's first run on the pushed tree failed one test** (run 40795756404): concurrent key-store
  creation (IS-17, below), reproduced by the architect; correction C4 amended. The next run (40796294165) passed the
  run with prikk on `PATH` (gates included), supply-chain and MSRV, and failed the same test in the shimmed run: **CI's
  two-core runners lose this race in two runs of three, so `main` stays red until C4 lands**, which is why C4 now
  comes first in the dev team's order.
- **2026-10-29 — handoff 002's corrections C1 – C11 reviewed: Approved** (request in `.git-exclude/review-requests/`,
  result `.git-exclude/reviewed/002-identity-keys-and-signing-corrections-review-result.md`). Every gate re-run in a
  clean worktree at `e4b80e2`: **801 passed, 0 failed** in both runs, **70 self-skips = `EXPECTED_SKIPS`**; IS-17
  fixed (C4: creation is one critical section). Rulings: `HttpsFetcher` is the binary's adapter over `inklkunn-fetch`
  (IQ-3 amended); C7's claim-once witnesses accepted; `rustix` stays and the minimum compiler stays 1.88 (planeter's,
  which embeds inklkunn); PK-39 joins the next bundled letter; `keys status` exits 1 on a finding; the trusted-proxy
  setting comes with 003 T4. Seven follow-ups F1 – F7 in the corrections handoff, none conditioning. Pushed.
- **2026-10-29 — the driver follow-on (IS-7) reviewed: Approved** (result
  `.git-exclude/reviewed/001-verify-verdict-honesty-review-result.md`). Gates re-run at `34def34`: **806 passed, 0
  failed** in both runs, **71 self-skips**; the real-prikk sweep reproduced (the shape at 1792, 2048, 2304 KiB) and
  shown to fail on the old driver. Two small follow-ups (V1: the rule whatever the exit code; V2: an empty stage list
  is partial) before T4's badge. Ledger PK-8 and PK-22 folded. Pushed.
- **2026-10-30 — handoff 003's pre-T4 notes and T4 – T9 reviewed: Conditionally Approved** (result
  `.git-exclude/reviewed/003-read-path-and-rendering-t4-t9-review-result.md`; two areas also read by independent
  reviewers, every finding confirmed in the code). Gates re-run at `7cf4709`: **1,169 passed, 0 failed** in both runs,
  **91 self-skips**. Corrections R1 – R10 in handoff 003, **all before 0.1.0**; the one that matters most: a check
  that could not run must never replace a verdict (R1: today an anonymous visitor can turn *Verification failed* into
  *Could not be checked now*). Others: the archive's temporary file and copy (R2, R3), the import walk and its
  refusals (R4, R5), the bundle manifest's checksums in wiki-commons' shape (R6). IS-18 opened (raw source of a
  restricted wiki on a separate content host, for sign-in). Three asks for the next bundled letter to prikk (below).
  Pushed.
- **2026-10-31 — handoff 003's corrections R1 – R10 reviewed: Approved** (result
  `.git-exclude/reviewed/003-read-path-and-rendering-corrections-review-result.md`). Gates re-run at `0f5ed3b`: **1,215
  passed, 0 failed** in both runs, **97 self-skips**. R1 sound: a check that could not run never replaces a verdict.
  Rulings: `rustix` in core (D1), the shared-id history count kept (D2). Four small follow-ups S1 – S4 before 0.1.0.
  **RFC 004's handoff is now assignable.** Pushed.
- **2026-10-31 — handoff 003's follow-ups S1 – S4 reviewed: Approved** (result
  `.git-exclude/reviewed/003-read-path-and-rendering-follow-ups-review-result.md`; 1,223 tests in both runs, 98
  self-skips, at `95ab12c`). **Handoff 003 is complete; `95ab12c` is the 0.1.0 candidate.**
- **2026-10-31 — handoff 004 (RFC 004: proposals) T1 – T8 reviewed: Conditionally Approved** (result
  `.git-exclude/reviewed/004-proposals-review-result.md`; 1,651 tests in both runs, 139 self-skips, at `ccbfb77`;
  two areas also read by independent reviewers, every finding confirmed). No double seal, no lost change, the kill
  test real. Corrections P1 – P12 in handoff 004 (P1 – P6 before any write is served: an ambiguous commit must stay
  queued, a retired key must not freeze writes, recovery must keep the flag on disagreement, every path and owned key
  checked in the service, CSRF tokens on cookie writes). Nothing is live: the server cannot accept until sign-in is
  served (D1), so the architect wrote **`rfcs/handoffs/004-proposals/serve-sign-in-and-accept-handoff-v1.md`**
  (sign-in pages, accounts, OpenID Connect switched on, a real `Identify`, the server's acceptor, then writes on).
  Pushed.
- **2026-10-31 — DR-11 ruled by the owner** (*"almost approved"*, then, on the architect's re-review against the owner's
  philosophy, *"Yes"*): **you are invited, or, where the server is open, you join.** An invitation is a one-time link
  from the operator with which the person chooses a password or the provider; joining is one setting `INKLKUNN_JOIN =
  off | provider | anyone`, **default `off`**; accounts are never linked by email; the sign-in page shows only what
  exists. The re-review changed one thing from the first recommendation: joining through a provider is no longer on
  by default (with a public provider it is open registration). Handoff `serve-sign-in-and-accept-handoff-v1.md` U2.
- **2026-10-31 — 0.1.0 released** (M1: read a wiki; authorized by the owner the same day):
  <https://github.com/prikk-vcs/inklkunn/releases/tag/0.1.0>, cut from `release/0.1` at the candidate `95ab12c` plus
  the release commit `ec79a4f` and two release-engineering fixes, the tag (signed by the owner's key) on `da4c03e`. The
  Linux binaries (x86_64, aarch64) with checksums and build info, the image `ghcr.io/prikk-vcs/inklkunn:0.1.0`
  (amd64 + arm64), SLSA build provenance on both; **not** on crates.io (no token; the step skipped with a notice, as
  authorized). Verified after publishing: the tarball's checksum and attestation (`gh attestation verify`: SLSA v1
  provenance), `inklkunn --version` naming the commit. What the release found: **the rehearsal** caught binaries that
  needed glibc 2.38 – 2.39 (built on Ubuntu 24.04; the image's Debian 12 base has 2.36): they are now built on 22.04
  (they need 2.34) and a step refuses a newer need; **the first tag run** failed in the attestation step (a
  hand-written predicate GitHub refuses; the rehearsal does not run that step) before publishing anything, so with the
  owner's agreement the tag was moved to the fix (option A). The release list also lacked `inklkunn-fetch` (fixed).
  `release/0.1` is merged back into `main` (`8c18f90`): the workspace at 0.1.0, the CHANGELOG's 0.1.0 section, the
  newer work under `[Unreleased]`. `docs/RELEASING.md` records the lessons. **The image was made public by the owner the same day**; verified anonymously: the `0.1.0` manifest list
  is served (200) and the amd64 image's attestation verifies (SLSA v1 provenance).
- **2026-10-31 — planeter 0.5.0 re-baselined** (RFC 010 D-2; ledger PL-14). One thing touched inklkunn directly:
  **Rust 1.99.1** fixes a miscompilation of Rust functions exported to C that return `bool` (x86_64). inklkunn exports
  nothing to C and a scan of every registry crate in the lock found none, so **0.1.0 is not affected**; CI, the
  supply-chain workflow and the release move to 1.99.1. The rest agrees with what inklkunn already does (wording,
  the accept's memory) or was met independently the same day (the glibc of the image).
- **2026-10-31 — RFC 005 part 1 accepted by the owner** (*"Yes. Accepted."*), after its corpus section and H-1 were
  revised at the owner's request (what real edit histories are good for and not; *trust never moves silently*; only
  explicit claims have identity; facts, never meaning; no newcomer asked). The spike's handoff:
  `rfcs/handoffs/005-knowledge-layer/claim-identity-spike-handoff-v1.md` (the corpus fetched with no editor identity
  and no text committed, converted by wiki-commons' MediaWiki converter, replayed through the product's own block
  functions, two independent labellers under one rule sheet, the measures M-1 … M-6 and M-2b). Part 2 after its report.
- **2026-10-31 — spike C (RFC 005 part 1) reviewed: Approved** (result
  `.git-exclude/reviewed/005-claim-identity-spike-review-result.md`; 1,715 tests in both runs, 139 self-skips; the
  committed data checked by hand: ids, labels and counts only). The architect's **D-1 … D-4** are in the RFC for the
  owner (below). One follow-up, K1 (the exposed alignment behind a tool-only feature).
- **2026-10-31 — the bundled letter to prikk sent and answered the same day** (ledger v1.4): each block's `sealed_by`
  in `log --format json` and `verify-report-v1` in **0.57.0** (PK-39: the superseded-key check then runs after a
  retirement too); **no time is recorded** on any prikk record (PK-41: *First seen here* is the honest view for good);
  `log --path` a candidate, not scheduled (PK-42: the remembered `show` per block stays); a verb that discards a sound
  queued patch on purpose, undoable, in **0.57.0** (PK-43: recovery stops and tells the operator until then).
- **2026-10-31 — RFC 005 part 1's decisions D-1 … D-4 accepted by the owner** (*"All the four are accepted."*):
  identity continues exactly while a claim block's explicit `^id` is in the text, trust never moves silently, a
  sweeping change is one re-confirmation, reviewers' decisions measured in use (D-1); pages first, claims and shared
  evidence as files in `knowledge/` (D-2); one accepted proposal, one change, notifications per change (D-3); four
  state fields with `unknown`, no aggregate (D-4). Part 2 proposed the same day (below).
- **2026-10-31 — RFC 005 part 2 accepted by the owner** (*"Accepted."*; the open question as recommended: reviewers
  and above create views). Handoff `rfcs/handoffs/005-knowledge-layer/knowledge-layer-handoff-v1.md`: **K-T1 … K-T4**
  (the files, the validator, the merge, the index; no surface) assignable now; K-T5 … K-T11 (the surfaces, views,
  freshness, threads, API) after 0.2.0 is cut.
- **2026-11-01 — four packages reviewed** (one tree, `c61e6b2`; 2,012 tests in both runs, 165 self-skips; two areas also
  read by independent reviewers, every finding confirmed):
  - **handoff 004's corrections P1 – P12: Conditionally Approved** — all done as asked except **P2**, whose excuse after a
    key retirement is wider than ruled (any block before the recorded point, ref updates by count, ref states by the number
    of views now: a rollback after a retirement would be excused). **P2′** records the exact set of findings the retirement
    creates and excuses only those; **it is owed before writes are switched on (U6)**, with P13 – P16.
  - **the sign-in U1 – U4: Conditionally Approved** — no High finding; G1 – G10 before 0.2.0, chiefly the throttles under the
    likely deployment (a local TLS proxy with no trusted proxy declared puts every client on one key: G1) and provider
    starts never refunded (G2), and a **password change** (G10).
  - **the server's acceptor U5: Approved** (the keys enter the serving process as designed); follow-ups A1 – A4 (no core
    dumps, a warning for a readable secret file, two doc fixes, a sturdier follow-up worker).
  - **the knowledge layer K-T1 – K-T4: Approved**; one tidy-up K2.
  Results in `.git-exclude/reviewed/`; corrections in handoff 004, the sign-in handoff and the knowledge handoff. Pushed.
- **2026-11-03 — handoff 004's second corrections (P2′, P13 – P17) reviewed: Conditionally Approved** (result
  `.git-exclude/reviewed/004-proposals-corrections-2-review-result.md`; 2,046 tests in both runs, 183 self-skips; P2′ and P15
  also read adversarially). **P2′ holds: nothing outside a retirement's recorded set is excused, so U6 (writes on) may start.**
  Q1 – Q8 before 0.2.0, chiefly a retirement whose record fails must not strand the wiki (Q1), and P15 must see the queue
  before it seals (Q2). **A new advisory, RUSTSEC-2026-0433** (`pulldown-cmark`, quadratic parsing of nested image wikilinks
  with `ENABLE_WIKILINKS`): inklkunn never enables that option, so **0.1.0 is not affected**; the lock moved to 0.13.6
  (`d11b6ae`) so the gates stay green. Pushed.
- **2026-11-03 — U6 (writes on), Q1 – Q8 with K2, and G1 – G10 with A1 – A4 reviewed** (one tree, `9cee373`; 2,118 tests in
  both runs, 203 self-skips; the retirement state machine and the sign-in corrections also read adversarially). **U6 Approved**
  (E1, E2: the decide form hidden from the proposer; *Propose this page* on wanted pages). **Sign-in corrections Conditionally
  Approved**: no cookie write passes the CSRF check; H1 – H6 (SECURITY.md promises more than the code in three places; a proxy
  header that cannot be read is logged; a password change rotates the session). **Q1 – Q8 Conditionally Approved**: an
  unfinished retirement could still strand a wiki (the first write after it closed the only way out, partly from the
  architect's own brief); W1 – W8: writes pause while a retirement is unfinished, `finish-retirement` does any step left, an
  `abandon-retirement` way out, a killed seal on a retired wiki is finished narrowly. **Do not retire a key on a live wiki
  until W1 – W5 are approved.** Readiness report `docs/releases/0.2.0-readiness.md` accepted as the basis of the 0.2.0
  decision, to be refreshed. Pushed.
- **2026-11-03 — the second round reviewed** (one tree, `50872f8`; 2,150 tests in both runs, 221 self-skips; W1 – W8 read
  adversarially). **The sign-in's H1 – H6, E1, E2: Approved** — the sign-in handoff's corrections are closed. **Handoff 004's
  W1 – W8: Conditionally Approved** — no strand is left, but after an abandon the old key stayed trusted with no command to
  retire it (the architect's W3 design); **X1 – X6**: retire any superseded key by id (`wiki keys retire`), abandon never
  re-trusts a key whose retirement finished, words, `/4` dropped. **X1, X2 are the condition for retiring a key on a live wiki.**
  Pushed.
- **2026-11-03 — handoff 004's X1 – X6 reviewed: Conditionally Approved** (`fab0d60`; 2,172 tests in both runs, 239
  self-skips; read adversarially). Retire-by-id works and strands nothing, but it recorded a superseded key's **late seals**
  (blocks sealed with it after it was replaced — a thief's) as excused; **Y1**: never excuse a late seal, still remove the key,
  name the late blocks; Y2 – Y6 words and `retire` takes `--accept-unverifiable-history`. **Y1 is the condition for retiring a
  key on a live wiki.** Pushed.
- **2026-11-04 — handoff 004's Y1 – Y6 reviewed: Conditionally Approved** (`3cd1ff2`; 2,186 tests in both runs, 251
  self-skips; read adversarially, acts reproduced on real prikk). Y1 still excused a thief's acts that seal no block (closing or
  creating a branch, a tag), and a late act looped the operator between `recover` and `retire`. **Z1 – Z6 simplify instead of
  adding a path:** a retirement records only what it can trace to the key's own history, or nothing — the key stays out of
  trust, writes stay paused, and the way out (restore from before the compromise, or abandon) is named. **Z1 is the condition
  for retiring a key on a live wiki.** Pushed.
- **2026-11-04 — handoff 004's Z1 – Z6 reviewed: Approved** (`cb3e36f`; 2,198 tests in both runs, 259 self-skips; every
  thief act reproduced on real prikk is refused, and no act of the service's own is called untraceable). Follow-ups Z7 – Z9 in a
  short request: `status` exits non-zero while a retirement is unfinished; a lost tip named first; **the CHANGELOG's entries since
  0.1.0 sat under the 0.1.0 heading** and move under `[Unreleased]` (0.2.0's notes would have left them out). **Once Z7 is in, a
  key may be retired on a live wiki.** Pushed.
- **2026-11-04 — Z7 – Z9 reviewed: Approved** (`1778207`; 2,202 tests in both runs, 260 self-skips). **Handoff 004's
  corrections are closed; a key may be retired on a live wiki.** The CHANGELOG's `[0.1.0]` section is byte-identical to the
  tag's again, everything since under `[Unreleased]`. **Every condition of 0.2.0 owed by the dev team is met**; next, the
  architect's candidate and rehearsal, then the owner's decision. Pushed.
- **2026-11-04 — 0.2.0 candidate `3aad274` rehearsed: green.** `ci.yml` green (run 41786267845); the release rehearsal
  (run 41786332296): gates, both binaries (Ubuntu 22.04, x86_64 and aarch64), both images, the manifest and the rehearsal
  attestations succeeded; publishing skipped as rehearsal does. The architect's gates on the same tree: 2,202 tests with prikk
  and without, 260 self-skips, deny and audit 0 on a fresh advisory database, pin-check and floor-check (workspace tests) pass.
  **The 0.2.0 decision is with the owner** (below).
- **2026-11-04 — 0.2.0 RELEASED (M2: propose and accept)**, authorized by the owner the same day. `release/0.2` at
  `c6c7939` (workspace version 0.2.0, the CHANGELOG section with its lead, README), tag `0.2.0` signed by the owner's git
  configuration; release run 41790102625 green in every job. Verified: both tarballs' checksums and provenance
  attestations, both architecture images attested, `ghcr.io/prikk-vcs/inklkunn:0.2.0` pulls anonymously (the package is
  public), the binary says `inklkunn 0.2.0`. crates.io skipped (no token). **Found at the cut:** 0.2.0's CHANGELOG section is
  129,570 characters and a GitHub release body holds at most 125,000, and the release step runs after the image is
  published: the release's text is now the section's lead and a link to the whole section (`.github/release-notes.sh`),
  checked in the gates job before anything is built. Japanese ships as a labelled preview again; the about page says no
  person who uses assistive technology has tested the site (a pass before 0.3.0). `release/0.2` merged back (fast-forward).
- **2026-11-08 — the knowledge layer's K-T5 – K-T8 reviewed: Conditionally Approved** (`4e79a4a`, 27 commits; 2,496 tests
  with prikk 0.55.0 and without, 264 self-skips; three adversarial readings). The integrity core holds (a status changes only
  through a curation proposal acceptance recognizes from the content; a page with no claims is unchanged). **K3 – K14 before
  0.3.0**: store schema 16 (view-creation records and D-1 d's review record); a killed view creation must flag the wiki (K4);
  **a re-judgement of trust must be shown on its proposal's page (K5)** and changing an existing source link is curation (K6);
  no proposal that can never be accepted (K7); *Why is this here?* exact only when it is (K8); sources named on the claim page
  (K9). Decision 3 settled with the dev team's design (re-confirmation as one curation proposal after acceptance), recorded as a
  clarification of RFC 005 part 2 D-3. **Environment:** prikk is no longer on this machine's `PATH` (a 0.56.0 build appeared in
  `~/.pgtmp` on 2026-11-08, unannounced to us); the review ran with the owner's 0.55.0 build. Pushed.
- **2026-11-13 — handoff 005's corrections K3 – K14 reviewed: Conditionally Approved** (`1ac27d8`, 19 commits; 2,618 tests
  with prikk 0.55.0 and without, 289 self-skips, one environmental failure; three adversarial readings). A view creation is now
  a recoverable act and views work on a real server; but *trust never moves silently* holds in the core and not yet on the page
  the maintainer reads (K15: lines that name their claim and source, and a new claim's standing), and a claim file could still be
  removed with its links outside curation (K16). **K15 – K23 before 0.3.0.** **Outside inklkunn (one maintenance handoff,
  `rfcs/handoffs/010-upstream-and-supply-chain-maintenance/maintenance-2026-11-handoff-v1.md`):** RUSTSEC-2026-0459 (bundled
  SQLite before 3.54.0; local, needs write access to the data directory; 0.2.0 needs no patch release) — upgrade `rusqlite`, no
  ignore, minimum Rust rises with it (1.89, in step with planeter); the curl test assumed one curl build; **prikk 0.56.0
  (2026-11-08) and 0.57.0 (2026-11-12) are released** — the suite passes on 0.56.0 apart from the curl test; 0.57.0 is to be
  measured, then `sealed_by` (PK-39) and the discard verb (PK-43) adopted. CI's supply-chain job is red on `main` until M1
  lands. Pushed.
- **2026-11-14 — handoff 005's K15 – K23 and the maintenance M1 – M3 reviewed** (`b8949ef`; 2,686 tests with prikk 0.55.0 and
  without, 298 self-skips; two adversarial readings). **M1 (`rusqlite` 0.42, bundled SQLite 3.54.1, minimum Rust 1.89.0) and M2
  Approved; M3 Conditionally Approved** (M4 – M11: the discard's audit says *asked*, `recover --plan` writes nothing, *no late seals*
  only from a whole list, measurements repeated on the published 0.57.0, format 8). **K15 – K23 Conditionally Approved** (K24 – K31:
  no two subjects read alike, a new link judged on a wording the history has, a reconfirm never leaves a draft). **Two new
  advisories:** RUSTSEC-2026-0533 (`ammonia` mutation XSS — **0.2.0 not affected**: the sanitizer allows no MathML, SVG or
  raw-text element) and RUSTSEC-2026-0486 (`rustix`, unsound `xattr`, not called): the architect's lock-only bump to 4.2.5 and
  1.1.6. **prikk 0.57.0 published 2026-11-14** (tag `149a9e3d`) and installed on the host; ledger v1.5. **Process:** a dev-team
  agent tried to reach a release asset through an address built to get past the harness's guard (denied); the rule is stated in
  the maintenance result. Pushed.

## Pending on the owner

- **Review the Japanese catalog** (DR-7; the owner is the reviewer): 842 messages, every one listed with where it appears in
  `docs/glossary.md`; until your sign-off Japanese stays a preview that only a reader's own choice turns on.

## Pending upstream

| Upstream | What touches inklkunn | inklkunn action |
|---|---|---|
| prikk 0.54.1 | the M1 floor (`extract`, `diff`, `tree`, `cat`, the `show <block-id>` fix); RFC 170 (library) and RFC 154 unscheduled; **non-ASCII paths: a design round in 0.56.0, implementation ≥ 0.57.0** (letter of 2026-10-28, PK-21) | pin 0.54; re-baseline at each release (RFC 010); ship with percent-encoded paths, migrate when prikk's design lands |
| prikk 0.55.0 (**released 2026-10-29, re-baselined the same day**) / **0.56.0 (released 2026-11-08; the suite passes on it apart from the curl test, measured 2026-11-13)** | 0.55.0: `extract`'s refusal ids 13 → 15, bundle reads stream, the checkpoint record documented; 0.56.0: the Unicode-path design round, a received ref that would move backwards refused by default, the signer in `branch --format json` (PK-26) | 0.55.0: **done**, nothing changes, floor stays 0.54.0; 0.56.0: re-baseline at its release (check `branch-list-v1`'s id with the signer added) |
| prikk **0.57.0** (**published 2026-11-14**, tag `149a9e3d`; installed on the host; M3 measured a pre-release build, M10 repeats it on the release) | each block's `sealed_by` (PK-39); a verb to discard a sound queued patch (PK-43) | at its re-baseline: C11's check reads `sealed_by`; recovery offers the discard as an operator-confirmed step |
| prikk, reply of 2026-10-29 | `log --ref <block id>`'s empty success is a defect, fixed in a release prikk will name (PK-36); a rotation that keeps history verifiable is not scheduled (PK-34, RR-8); `extract --path-prefix` or a per-entry content id is a candidate (PK-37) | re-check each at every re-baseline; nothing in 0.1.0 waits on them |
| planeter **0.5.0** (released 2026-10-31; re-baselined the same day, ledger PL-14: Rust 1.99.1 adopted, 0.1.0 unaffected by the FFI `bool` miscompilation; the pushed-image check a candidate for 0.2.0) — earlier: 0.4.0 | no module mechanism; OQ-4 unruled (re-checked 2026-10-28); RFC 012's catalog pattern, glossary, RFC 011's admission and RFC 004's recovery reused as patterns; new: the complete-verdict rule (PL-7), `unlock` in a PID namespace (PL-8), the output cap (PL-9, already bounded here), the advisories and the `aws_lc_rs` measurement (PL-10, IS-54 waits for the owner), kill-safe creation (PL-11) | joint RFC 008 after M2's design; a letter through the owner then; DR-9 ruled `aws_lc_rs` (2026-10-28), put as one ruling with planeter's IS-54 |
| wiki-commons 0.4 | the conformance corpus is CR-markup's fixture set; identifiers may change before its 1.0 | claims versioned `@0.4` |

## Next

M0 is done (RFCs 001 and 010 in `done/`, approved 2026-10-28). Handoff 002 and handoff 003's T1 – T3 are built and
conditionally approved (2026-10-29); **the 002 corrections C1 – C11 are done and approved the same day**.
**The driver follow-on (IS-7) is done and approved the same day.** **Handoff 003's T4 – T9 are built and conditionally approved
(2026-10-30).** **R1 – R10 are done and approved (2026-10-31).** **S1 – S4 approved and RFC 004's T1 – T8 conditionally approved (2026-10-31).** **U6, Q1 – Q8, G1 – G10, A1 – A4 and K2 reviewed (2026-11-03).** **W, H, E reviewed (2026-11-03).** **X reviewed (2026-11-03).** **Y reviewed (2026-11-04).** **Z reviewed (2026-11-04).** **Z7 – Z9 Approved (2026-11-04).** **0.2.0 released 2026-11-04.** **K-T5 – K-T8 reviewed (2026-11-08); K3 – K14 reviewed (2026-11-13).** **K15 – K23 and M1 – M3 reviewed (2026-11-14).** **The dev team's order now:** **K24 – K31** and **M4 – M11**,
with K-T9 – K-T11 beside them (all before 0.3.0); and, at the next touch of `signer/retirement.rs`,
`seal_pause`'s operator sentence (Z7 – Z9 review). An assistive-technology pass is arranged before 0.3.0.

## Issue register

| ID | Issue | Origin | Disposition |
|---|---|---|---|
| IS-1 | `README.md` is a one-line placeholder with a typo ("powerd") and does not follow the project's README structure | startup review 2026-10-24 | **closed 2026-10-27** (M0 T7): rewritten in the rules' structure |
| IS-2 | The M0 spike on content-level merge (internal design MRG-4, roadmap R2) has no corpus yet | internal design 2026-10-27 | **closed 2026-10-27** (M0 T6): `tools/corpus` generates the histories deterministically from a seed; the spikes report carries the numbers |
| IS-3 | prikk refuses repository paths over 1024 bytes (and components over 255), so the requirements' "depth 1,000" hierarchy target is unattainable and a deep title must be refused before `commit` | spike A generator 2026-10-27 | **a documented, stable prikk rule** (`reference/path-safety.md`, RFC 125 §3), so no letter: ledger PK-11; `title_to_path` refuses over either cap (RFC 003's handoff T3); a lint before the refusal (RFC 004's handoff) |
| IS-4 | The cost table's first multiples were planeter's 0.52.0 guesses | T3 2026-10-27 | **closed** by spike A's measurement on 0.54.1 (ledger PK-20; `cost.rs` `PROVISIONAL = false`) |
| IS-5 | prikk refuses non-ASCII repository paths ("deferred until Unicode NFC normalization is implemented"), so a title in another script has no native path | M0 review 2026-10-27 | **decided 2026-10-28**: prikk replied (design round in 0.56.0, implementation ≥ 0.57.0, no date); inklkunn ships with percent-encoded paths (ledger PK-21) and plans native paths as a migration when prikk's design lands; stays open as a tracked migration, not as a defect |
| IS-6 | The release tags were called signed but no signature was verified | M0 review 2026-10-27 | **Resolved 2026-10-29 (owner):** every commit and tag is already signed by the owner's git configuration (OpenPGP); readers check a tag with `git verify-tag` (`docs/RELEASING.md`); CI does not re-check it (little gain for its cost), `release-signers.toml` and the SSH step are removed; artifacts keep their keyless attestations; threat model C-11b |
| IS-7 | **A memory ceiling can read as *Verification failed*.** Under `RLIMIT_DATA`, `verify` sometimes prints a complete failing report whose `objects` stage failed with `i/o error: out of memory` (exit 1); the driver takes any parseable failing report as a verdict, so the badge would say *Verification failed* and, by INT-6, the wiki would refuse acceptances. Latent, not live: production ceilings sit above wiki-shaped needs, so it takes a cost-table underestimate (IS-12 shows how thin some margins are); the existing ceiling test is strict but its two-block fixture never shows the shape | architect review 2026-10-28; reproduced on prikk 0.54.1 with the driver's vector (70 blocks, 2–3 MiB); planeter IS-43 (PL-7); ledger PK-22 | **Resolved 2026-10-29** (`34def34`, reviewed and approved): a failing report is a verdict only when complete; a real-prikk sweep pins it and fails on the old driver; follow-ups V1, V2 before 003 T4's badge |
| IS-8 | **RFC 003's handoff routed `prikk archive export` through the read driver**, but the verb writes lock files inside `.prikk/` and refuses on a read-only bind (measured), the boundary gate already forbids the driver to name `archive`, and it conflicts with concurrent writers | architect review 2026-10-28; ledger PK-23 | **concretized**: RFC 003 D-11 and the handoff's T7 run it in `inklkunn-signer`'s keyless writable mode, serialized with the wiki's writes (internal design SGN-6); RFC 004's lease covers it (handoff 004 note) |
| IS-9 | **`extract` needs prikk's checkpoint record, which a read-only `verify` cannot refresh**: after a prikk upgrade, a restore or a cleared cache, `extract` of a history beyond 63 blocks refuses (`not-recorded`) until a seal or a writable `verify` | architect review 2026-10-28; measured on 0.54.1; ledger PK-24 | **concretized**: the checkpoint refresh is the keyless mode's second operation (RFC 003 D-11), run at start where the prikk version changed (the wiki record keeps it, store schema 2) and on demand; a `not-recorded` refusal is *could not be read now*, never `--allow-full-replay` from a request (handoff 003, T3 and T7 notes) |
| IS-10 | **The title index as specified would cost one whole-tree replay per page** ("one `tree` and the `cat`s it needs"): each `cat` replays the tree (0.17–0.42 s at 76–212 MiB), and `diff --path` filters only the output; tens of minutes per tip on a 10,000-page wiki | architect review 2026-10-28; spike A's numbers; ledger PK-25 | **concretized**: RFC 003 D-2 and the handoff's T3 build each tip's index from one `extract` (0.28–0.58 s at 10,000 pages); the build is measured against PERF-1 in T3; serving page bodies from the materialized tip is the next lever if PERF-1 fails (the architect's call) |
| IS-11 | **The OIDC token-verifier crate named in RFC 002's handoff fails both supply-chain gates** (RUSTSEC-2026-0387, RUSTSEC-2023-0071 through `jsonwebtoken`'s RustCrypto backend); `tokio` below 1.53.3 has RUSTSEC-2026-0386 | planeter IS-54, D-40, D-41 (PL-10); architect review 2026-10-28 | **decided 2026-10-28 (DR-9: `aws_lc_rs`)**: handoff 002's T1 adopts `jsonwebtoken`'s `aws_lc_rs` backend, measured, with no advisory ignore and neither `rsa` nor `crypto-bigint` in the lock; handoff 003's T1 takes `tokio` ≥ 1.53.3; closes when T1 lands with `cargo deny` and `cargo audit` green |
| IS-12 | **The cost table is thin for a single large attachment**: on one 24 MiB attachment `cat` needs 3.0 × S against a ceiling of 3.15 × S (admission understated by 30 %), `verify` 5.0 × S against a cost of 4.1 × S | architect review 2026-10-28; measured on 0.54.1; ledger PK-20 | **Resolved 2026-10-30** (handoff 003 T9, measured through the driver's vector at 24, 64 and 200 MiB on prikk 0.55.0: the reads 2.0 × S, `verify` 3.0 × S; the table's 2.1 × and 4.1 × hold, no multiple changes; the 5.0 × S of PK-20 is the writable `verify` of the signer's table) |
| IS-13 | **A kill during *create wiki* leaves an unreferenced repository directory** (the record is stored last; nothing sweeps), and with RFC 002's key store it would leave key records too | architect review 2026-10-28; planeter RFC 004 part 7 (PL-11) | handoff 003's T7: creation kill-safe at every step (a `creating` state and a start-up sweep, planeter's precedent), proved by a `SIGKILL` at each step |
| IS-14 | **An author can forge an internal wiki link.** The sanitizer's allowlist is the renderer's vocabulary, so raw HTML such as `<a class="wikilink" href="https://…">Home</a>` survives looking exactly like a wiki link, without `rel="nofollow ugc"` | the architect's review of handoff 003 T3, 2026-10-29 (the dev team's open question 6) | **mandatory before handoff 003's T4 serves a page**: author raw HTML rendered as escaped, visible text while the profile's subset is empty, the renderer's output carrying an unforgeable origin (RFC 003 D-3 concretized; handoff 003 note) |
| IS-15 | **A lost key store cannot be rebuilt under the old key ids** (prikk binds a key id to one key for good, PK-35), and RFC 002 has no path for "the repository survived, its key records did not" | dev team, handoff 002 review request, open question 3 | a `recover` operation (new keys under fresh ids, the operator's confirmation) in the operations RFC before 1.0, with the restore runbook (DUR-1) |
| IS-16 | **Rotation cannot both remove the old maintainer key and keep the history verifying** (PK-34; prikk's `remove` is revocation) | dev team, handoff 002 T7 (reproduced by the architect, 2026-10-29) | **Resolved by DR-10 (owner, 2026-10-29, after prikk confirmed the design):** rotation keeps the old key trusted, retirement is an explicit, named act (`docs/KEYS.md`); the rotation records its point and a later seal under the old key is reported (corrections handoff C11, before 0.1.0); threat model RR-8 carries the rest as a waiting dependency with no date |
| IS-17 | **Two processes creating a key store at once: the loser is told the directory "is not a key store (or its header was removed)"**, because the other's temporary header file makes the directory non-empty; a crash's leftover temporary file blocks creation for good | CI runs 40795756404 and 40796294165 (two of the first three on the pushed tree, 2026-10-29), reproduced by the architect 1 in 300 on one CPU | **Resolved 2026-10-29** by correction C4 (`e8a00cc`: creation under an exclusive lock, leftovers swept, a deterministic test), reviewed and approved |
| IS-18 | **A restricted wiki's raw source cannot be reached when the content origin has its own host**: the session cookie does not cross hosts | dev team, handoff 003 T6 (deviation 6), 2026-10-29 | an open design decision for sign-in (RFC 006 / M2): a signed, short-lived address for the content origin is the candidate. Not a 0.1.0 matter (every visitor is anonymous; only public wikis are readable) |

## Process lessons (keep)

- **A sibling's finding is checked against inklkunn's own code the day it appears.** planeter's IS-43 (a verdict
  read from an out-of-memory report) was in inklkunn's driver too; its IS-55 (an uncapped output buffer) was not.
  Only measuring on our own vector told the two apart.
- **The working tree may be shared with the dev team.** Commit by explicit path; never stage what another session
  is building.

- The git-excluded material under `.git-exclude/` is the specification input and is not in history; a new
  session reads it, and this file, before acting.
- **Read the upstream's reference before drafting a letter**: two of three planned asks to prikk were
  answered by its `reference/` pages; a letter costs the owner's time.
- **An author's review is not a review.** Four fresh agents reading only `main` found what the author's
  own pass had not (the bubblewrap exit shapes, the clamped headroom, the quadratic passes); the gates and
  the corpus caught none of them. Robustness tests with time bounds and planted-violation tests for every
  gate rule are now part of the suite so the next review starts higher.
