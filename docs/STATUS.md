# inklkunn — project status

| | |
|---|---|
| Document | The dated brief a new session reads **first**: where the project is, what is pending on whom, and the issue register. Updated at every milestone, RFC disposition and owner ruling. |
| As of | 2026-10-28 — **M0 done: RFCs 001 and 010 approved and moved to `done/`** (review requests v2 approved by the owner); **RFCs 002 and 003 accepted**, 002's handoff assignable and 003's T1–T3 startable; prikk's reply on Unicode paths folded (ship percent-encoded; migrate later); release tags will be SSH-signed (pending the owner's public key); **RFC 004 (proposals, M2) accepted** with its handoff v1. **Later the same day:** the dev team began (handoff 002 T5, `3913137`); a new architect session reviewed the whole record and the upstreams and folded **seven issues (IS-7 … IS-13)** into binding handoff notes, one driver follow-on handoff and ledger v1.1; **DR-9 ruled by the owner the same day: `aws_lc_rs`**. **2026-10-29:** the dev team's handoff 002 (T1 – T8) and handoff 003 (T1 – T3) **reviewed: both Conditionally Approved** (every gate re-run by the architect: 676 tests in both runs, 57 self-skips); corrections in `rfcs/handoffs/002-…/identity-keys-and-signing-corrections-handoff-v1.md` and handoff 003's T4 notes; **one decision asked of the owner (DR-10, key rotation)** and one letter to prikk drafted. **Later the same day:** the letter sent and answered; **DR-10 ruled** (keep the old key by default, retire explicitly; corrections C11); **prikk 0.55.0 is tagged upstream: the first re-baseline is due** |
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
  T3; requests and the review report in `.git-exclude/review-requests/`). Every gate re-run in a clean worktree:
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

## Pending on the owner

- **Add your SSH public key line to `release-signers.toml`** (`authorized_ssh_signers`) before the first
  release; the decision to sign tags is taken (2026-10-28), only the key is missing (IS-6).
- **Hand the dev team its next work:** the review report (`.git-exclude/review-requests/002-003-architect-review-2026-10-29.md`);
  `rfcs/handoffs/002-identity-keys-and-signing/identity-keys-and-signing-corrections-handoff-v1.md` (C1 – C4 before
  003's T7); `rfcs/handoffs/001-foundations/verify-verdict-honesty-handoff-v1.md` (before 003's T4); then handoff 003
  from T4 under its dated notes.
- **Review the Japanese catalog** when the dev team files it (DR-7; the owner is the reviewer, 2026-10-29): one
  review request listing every message in English and Japanese with where it appears, before 0.1.0. Until the
  owner signs off, Japanese stays a preview that only a reader's own choice turns on.

## Pending upstream

| Upstream | What touches inklkunn | inklkunn action |
|---|---|---|
| prikk 0.54.1 | the M1 floor (`extract`, `diff`, `tree`, `cat`, the `show <block-id>` fix); RFC 170 (library) and RFC 154 unscheduled; **non-ASCII paths: a design round in 0.56.0, implementation ≥ 0.57.0** (letter of 2026-10-28, PK-21) | pin 0.54; re-baseline at each release (RFC 010); ship with percent-encoded paths, migrate when prikk's design lands |
| prikk 0.55.0 (**tagged upstream**, seen 2026-10-29) / 0.56.0 (scheduled) | 0.55.0: `extract`'s refusal ids 13 → 15, bundle reads stream, the checkpoint record documented; 0.56.0: the Unicode-path design round, a received ref that would move backwards refused by default, the signer in `branch --format json` (PK-26) | **the first re-baseline is due now** (RFC 010's deferred criterion; the architect's next task); no inklkunn change is expected from either plan |
| prikk, reply of 2026-10-29 | `log --ref <block id>`'s empty success is a defect, fixed in a release prikk will name (PK-36); a rotation that keeps history verifiable is not scheduled (PK-34, RR-8); `extract --path-prefix` or a per-entry content id is a candidate (PK-37) | re-check each at every re-baseline; nothing in 0.1.0 waits on them |
| planeter 0.4.0 (+ unreleased work to 2026-10-28) | no module mechanism; OQ-4 unruled (re-checked 2026-10-28); RFC 012's catalog pattern, glossary, RFC 011's admission and RFC 004's recovery reused as patterns; new: the complete-verdict rule (PL-7), `unlock` in a PID namespace (PL-8), the output cap (PL-9, already bounded here), the advisories and the `aws_lc_rs` measurement (PL-10, IS-54 waits for the owner), kill-safe creation (PL-11) | joint RFC 008 after M2's design; a letter through the owner then; DR-9 ruled `aws_lc_rs` (2026-10-28), put as one ruling with planeter's IS-54 |
| wiki-commons 0.4 | the conformance corpus is CR-markup's fixture set; identifiers may change before its 1.0 | claims versioned `@0.4` |

## Next

M0 is done (RFCs 001 and 010 in `done/`, approved 2026-10-28). Handoff 002 and handoff 003's T1 – T3 are built and
conditionally approved (2026-10-29). **The dev team's order now:** the driver follow-on (IS-7) and the 002 corrections
C1 – C4 (both small, independent), then handoff 003's T4 – T6 under the 2026-10-29 notes (author HTML escaped and reads
pinned first), then T7 (which needs C1 – C4) and T8 – T9 toward the 0.1.0 readiness report; C5 – C7 before RFC 004's
T4, C8 – C10 before OpenID Connect is switched on, C11 (rotation records its point) before 0.1.0. The architect reviews
each request as it arrives and runs the first re-baseline at prikk 0.55.0, which is now tagged. RFC 004 (proposals) is accepted and its handoff v1 is written; it is assigned when the 002 corrections C1 – C7 and
handoff 003's T1 – T4 are approved. The single-large-attachment measurement is now handoff 003's T6 (IS-12; the architect's
early data are in ledger PK-20). The architect drafts RFC 005 part 1 (the claim-identity investigation) when M1 is
under way. The four kickoff documents the requirements transcript
linked do not exist (owner, 2026-10-28): the transcripts under `.git-exclude/specs/` are the complete startup
input, all in English. The reworked `release.yml` was rehearsed on `d90f901`
([run 40490072717](https://github.com/prikk-vcs/inklkunn/actions/runs/40490072717)): gates with both test
passes, binaries for both architectures, images for both, the manifest list; release and crates skipped as
the mode requires. CI is green from `d0a8307` on ([run 40489623629](https://github.com/prikk-vcs/inklkunn/actions/runs/40489623629)).

## Issue register

| ID | Issue | Origin | Disposition |
|---|---|---|---|
| IS-1 | `README.md` is a one-line placeholder with a typo ("powerd") and does not follow the project's README structure | startup review 2026-10-24 | **closed 2026-10-27** (M0 T7): rewritten in the rules' structure |
| IS-2 | The M0 spike on content-level merge (internal design MRG-4, roadmap R2) has no corpus yet | internal design 2026-10-27 | **closed 2026-10-27** (M0 T6): `tools/corpus` generates the histories deterministically from a seed; the spikes report carries the numbers |
| IS-3 | prikk refuses repository paths over 1024 bytes (and components over 255), so the requirements' "depth 1,000" hierarchy target is unattainable and a deep title must be refused before `commit` | spike A generator 2026-10-27 | **a documented, stable prikk rule** (`reference/path-safety.md`, RFC 125 §3), so no letter: ledger PK-11; `title_to_path` refuses over either cap (RFC 003's handoff T3); a lint before the refusal (RFC 004's handoff) |
| IS-4 | The cost table's first multiples were planeter's 0.52.0 guesses | T3 2026-10-27 | **closed** by spike A's measurement on 0.54.1 (ledger PK-20; `cost.rs` `PROVISIONAL = false`) |
| IS-5 | prikk refuses non-ASCII repository paths ("deferred until Unicode NFC normalization is implemented"), so a title in another script has no native path | M0 review 2026-10-27 | **decided 2026-10-28**: prikk replied (design round in 0.56.0, implementation ≥ 0.57.0, no date); inklkunn ships with percent-encoded paths (ledger PK-21) and plans native paths as a migration when prikk's design lands; stays open as a tracked migration, not as a defect |
| IS-6 | The release tags were called signed but no signature was verified | M0 review 2026-10-27 | **decided 2026-10-28**: tags are SSH-signed by the owner's key and `release.yml` verifies them against `release-signers.toml`; the step reports *pending* until the owner adds the public key, which closes this item |
| IS-7 | **A memory ceiling can read as *Verification failed*.** Under `RLIMIT_DATA`, `verify` sometimes prints a complete failing report whose `objects` stage failed with `i/o error: out of memory` (exit 1); the driver takes any parseable failing report as a verdict, so the badge would say *Verification failed* and, by INT-6, the wiki would refuse acceptances. Latent, not live: production ceilings sit above wiki-shaped needs, so it takes a cost-table underestimate (IS-12 shows how thin some margins are); the existing ceiling test is strict but its two-block fixture never shows the shape | architect review 2026-10-28; reproduced on prikk 0.54.1 with the driver's vector (70 blocks, 2–3 MiB); planeter IS-43 (PL-7); ledger PK-22 | **follow-on handoff assignable**: `rfcs/handoffs/001-foundations/verify-verdict-honesty-handoff-v1.md` (the complete-verdict rule, a ceiling sweep on a long history that fails on today's driver); lands before RFC 003's T4 |
| IS-8 | **RFC 003's handoff routed `prikk archive export` through the read driver**, but the verb writes lock files inside `.prikk/` and refuses on a read-only bind (measured), the boundary gate already forbids the driver to name `archive`, and it conflicts with concurrent writers | architect review 2026-10-28; ledger PK-23 | **concretized**: RFC 003 D-11 and the handoff's T7 run it in `inklkunn-signer`'s keyless writable mode, serialized with the wiki's writes (internal design SGN-6); RFC 004's lease covers it (handoff 004 note) |
| IS-9 | **`extract` needs prikk's checkpoint record, which a read-only `verify` cannot refresh**: after a prikk upgrade, a restore or a cleared cache, `extract` of a history beyond 63 blocks refuses (`not-recorded`) until a seal or a writable `verify` | architect review 2026-10-28; measured on 0.54.1; ledger PK-24 | **concretized**: the checkpoint refresh is the keyless mode's second operation (RFC 003 D-11), run at start where the prikk version changed (the wiki record keeps it, store schema 2) and on demand; a `not-recorded` refusal is *could not be read now*, never `--allow-full-replay` from a request (handoff 003, T3 and T7 notes) |
| IS-10 | **The title index as specified would cost one whole-tree replay per page** ("one `tree` and the `cat`s it needs"): each `cat` replays the tree (0.17–0.42 s at 76–212 MiB), and `diff --path` filters only the output; tens of minutes per tip on a 10,000-page wiki | architect review 2026-10-28; spike A's numbers; ledger PK-25 | **concretized**: RFC 003 D-2 and the handoff's T3 build each tip's index from one `extract` (0.28–0.58 s at 10,000 pages); the build is measured against PERF-1 in T3; serving page bodies from the materialized tip is the next lever if PERF-1 fails (the architect's call) |
| IS-11 | **The OIDC token-verifier crate named in RFC 002's handoff fails both supply-chain gates** (RUSTSEC-2026-0387, RUSTSEC-2023-0071 through `jsonwebtoken`'s RustCrypto backend); `tokio` below 1.53.3 has RUSTSEC-2026-0386 | planeter IS-54, D-40, D-41 (PL-10); architect review 2026-10-28 | **decided 2026-10-28 (DR-9: `aws_lc_rs`)**: handoff 002's T1 adopts `jsonwebtoken`'s `aws_lc_rs` backend, measured, with no advisory ignore and neither `rsa` nor `crypto-bigint` in the lock; handoff 003's T1 takes `tokio` ≥ 1.53.3; closes when T1 lands with `cargo deny` and `cargo audit` green |
| IS-12 | **The cost table is thin for a single large attachment**: on one 24 MiB attachment `cat` needs 3.0 × S against a ceiling of 3.15 × S (admission understated by 30 %), `verify` 5.0 × S against a cost of 4.1 × S | architect review 2026-10-28; measured on 0.54.1; ledger PK-20 | handoff 003's T6 re-measures at 64 and 200 MiB and raises the multiples **before `-/raw/` serves attachments** (T9's measurement brought forward); a `cat` rule that follows the file's size is to be proposed from the data |
| IS-13 | **A kill during *create wiki* leaves an unreferenced repository directory** (the record is stored last; nothing sweeps), and with RFC 002's key store it would leave key records too | architect review 2026-10-28; planeter RFC 004 part 7 (PL-11) | handoff 003's T7: creation kill-safe at every step (a `creating` state and a start-up sweep, planeter's precedent), proved by a `SIGKILL` at each step |
| IS-14 | **An author can forge an internal wiki link.** The sanitizer's allowlist is the renderer's vocabulary, so raw HTML such as `<a class="wikilink" href="https://…">Home</a>` survives looking exactly like a wiki link, without `rel="nofollow ugc"` | the architect's review of handoff 003 T3, 2026-10-29 (the dev team's open question 6) | **mandatory before handoff 003's T4 serves a page**: author raw HTML rendered as escaped, visible text while the profile's subset is empty, the renderer's output carrying an unforgeable origin (RFC 003 D-3 concretized; handoff 003 note) |
| IS-15 | **A lost key store cannot be rebuilt under the old key ids** (prikk binds a key id to one key for good, PK-35), and RFC 002 has no path for "the repository survived, its key records did not" | dev team, handoff 002 review request, open question 3 | a `recover` operation (new keys under fresh ids, the operator's confirmation) in the operations RFC before 1.0, with the restore runbook (DUR-1) |
| IS-16 | **Rotation cannot both remove the old maintainer key and keep the history verifying** (PK-34; prikk's `remove` is revocation) | dev team, handoff 002 T7 (reproduced by the architect, 2026-10-29) | **Resolved by DR-10 (owner, 2026-10-29, after prikk confirmed the design):** rotation keeps the old key trusted, retirement is an explicit, named act (`docs/KEYS.md`); the rotation records its point and a later seal under the old key is reported (corrections handoff C11, before 0.1.0); threat model RR-8 carries the rest as a waiting dependency with no date |
| IS-17 | **Two processes creating a key store at once: the loser is told the directory "is not a key store (or its header was removed)"**, because the other's temporary header file makes the directory non-empty; a crash's leftover temporary file blocks creation for good | CI run 40795756404 (the first on the pushed tree, 2026-10-29), reproduced by the architect 1 in 300 on one CPU | correction C4 (amended 2026-10-29): creation under an exclusive lock, leftovers swept, a deterministic test; **mandatory before handoff 003's T7**. Fails closed (no key is exposed); the message is what is wrong, and an operator could believe their keys were tampered with |

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
