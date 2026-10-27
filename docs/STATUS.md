# inklkunn — project status

| | |
|---|---|
| Document | The dated brief a new session reads **first**: where the project is, what is pending on whom, and the issue register. Updated at every milestone, RFC disposition and owner ruling. |
| As of | 2026-10-28 — **M0 done: RFCs 001 and 010 approved and moved to `done/`** (review requests v2 approved by the owner); **RFCs 002 and 003 accepted**, 002's handoff assignable and 003's T1–T3 startable; prikk's reply on Unicode paths folded (ship percent-encoded; migrate later); release tags will be SSH-signed (pending the owner's public key) |
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

## Pending on the owner

- **Add your SSH public key line to `release-signers.toml`** (`authorized_ssh_signers`) before the first
  release; the decision to sign tags is taken (2026-10-28), only the key is missing (IS-6).
- **Start the dev team**: M0 is approved; RFC 002's handoff is assignable and RFC 003's T1–T3 are startable.
  Its prepare task is `.git-exclude/tasks/dev-team/inklkunn-001-prepare.md` (a fresh architect session has
  `.git-exclude/tasks/architect/inklkunn-001-prepare.md`).
- Name the Japanese reviewers before the first catalog ships (DR-7; Japanese is compiled in as a preview
  until then).

## Pending upstream

| Upstream | What touches inklkunn | inklkunn action |
|---|---|---|
| prikk 0.54.1 | the M1 floor (`extract`, `diff`, `tree`, `cat`, the `show <block-id>` fix); RFC 170 (library) and RFC 154 unscheduled; **non-ASCII paths: a design round in 0.56.0, implementation ≥ 0.57.0** (letter of 2026-10-28, PK-21) | pin 0.54; re-baseline at each release (RFC 010); ship with percent-encoded paths, migrate when prikk's design lands |
| planeter 0.4.0 | no module mechanism; OQ-4 unruled; RFC 012's catalog pattern, glossary, RFC 011's admission and RFC 004's recovery reused as patterns | joint RFC 008 after M2's design; a letter through the owner then |
| wiki-commons 0.4 | the conformance corpus is CR-markup's fixture set; identifiers may change before its 1.0 | claims versioned `@0.4` |

## Next

M0 is done (RFCs 001 and 010 in `done/`, approved 2026-10-28). The dev team takes RFC 002's handoff (the real
signer and key store, the roles; its "Notes from M0's review" are binding) and RFC 003's handoff T1–T3 (the web
skeleton and headers, the catalog and the structural tests, the read service and caches); T4 onward follows
002's T2. The architect drafts RFC 004 (proposals, with the block-aware merge the spike justified) and prepares
the M1 re-baseline (the single-large-attachment shape). The four kickoff documents the requirements transcript
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

## Process lessons (keep)

- The git-excluded material under `.git-exclude/` is the specification input and is not in history; a new
  session reads it, and this file, before acting.
- **Read the upstream's reference before drafting a letter**: two of three planned asks to prikk were
  answered by its `reference/` pages; a letter costs the owner's time.
- **An author's review is not a review.** Four fresh agents reading only `main` found what the author's
  own pass had not (the bubblewrap exit shapes, the clamped headroom, the quadratic passes); the gates and
  the corpus caught none of them. Robustness tests with time bounds and planted-violation tests for every
  gate rule are now part of the suite so the next review starts higher.
