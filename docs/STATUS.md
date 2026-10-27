# inklkunn — project status

| | |
|---|---|
| Document | The dated brief a new session reads **first**: where the project is, what is pending on whom, and the issue register. Updated at every milestone, RFC disposition and owner ruling. |
| As of | 2026-10-27 — **RFCs 001, 002 and 010 accepted**; **M0 under construction by the architect at the owner's request** (T1 gates, T2 markup, T3 prikk driver, T4 store, T5 core seams and *create wiki* committed; T6 spikes and T7 conventions open); RFC 003 proposed |
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
  No binary runs yet; the spikes and the conventions are T6 and T7.

## Pending on the owner

- **Accept RFC 003** (read path and rendering, `rfcs/proposed/`), or name what to change; it serves M1 and
  can be accepted while M0 is built.
- Start the dev team when M0 is reviewed: its prepare task is `.git-exclude/tasks/dev-team/inklkunn-001-prepare.md`
  (a fresh architect session has `.git-exclude/tasks/architect/inklkunn-001-prepare.md`).
- Name the Japanese reviewers before the first catalog ships (DR-7).
- Say whether the four kickoff documents the requirements transcript links exist anywhere; the work so far
  assumes they do not.

## Pending upstream

| Upstream | What touches inklkunn | inklkunn action |
|---|---|---|
| prikk 0.54.1 | the M1 floor (`extract`, `diff`, `tree`, `cat`, the `show <block-id>` fix); RFC 170 (library) and RFC 154 unscheduled | pin 0.54; re-baseline at each release (RFC 010) |
| planeter 0.4.0 | no module mechanism; OQ-4 unruled; RFC 012's catalog pattern, glossary, RFC 011's admission and RFC 004's recovery reused as patterns | joint RFC 008 after M2's design; a letter through the owner then |
| wiki-commons 0.4 | the conformance corpus is CR-markup's fixture set; identifiers may change before its 1.0 | claims versioned `@0.4` |

## Next

The architect continues M0 from `rfcs/handoffs/001-foundations/foundations-handoff-v1.md` and
`rfcs/handoffs/010-upstream-and-supply-chain-maintenance/upstream-and-supply-chain-handoff-v1.md`: T4 `inklkunn-store`, T5 the core seams
and the bundle layout, T6 the two spikes, T7 the conventions (README, CHANGELOG, SECURITY, ledgers, book),
then writes the review request with the gate runs, the dependency deltas and the author-review gap, and the
owner (or a fresh reviewing session) reviews. RFC 002's handoff follows 001's approval. RFC 004 (proposals) is
drafted next.

## Issue register

| ID | Issue | Origin | Disposition |
|---|---|---|---|
| IS-1 | `README.md` is a one-line placeholder with a typo ("powerd") and does not follow the project's README structure | startup review 2026-10-24 | **closed 2026-10-27** (M0 T7): rewritten in the rules' structure |
| IS-2 | The M0 spike on content-level merge (internal design MRG-4, roadmap R2) has no corpus yet | internal design 2026-10-27 | **closed 2026-10-27** (M0 T6): `tools/corpus` generates the histories deterministically from a seed; the spikes report carries the numbers |
| IS-3 | prikk refuses repository paths over 1024 bytes, so the requirements' "depth 1,000" hierarchy target is unattainable and a deep title must be refused before `commit` | spike A generator 2026-10-27 | ledger PK-11; a lint and a refusal in RFC 004's handoff; an ask to prikk by letter (UD-10) |
| IS-4 | The cost table's first multiples were planeter's 0.52.0 guesses | T3 2026-10-27 | **closed** by spike A's measurement on 0.54.1 (ledger PK-20; `cost.rs` `PROVISIONAL = false`) |

## Process lessons (keep)

- The git-excluded material under `.git-exclude/` is the specification input and is not in history; a new
  session reads it, and this file, before acting.
