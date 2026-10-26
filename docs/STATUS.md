# inklkunn — project status

| | |
|---|---|
| Document | The dated brief a new session reads **first**: where the project is, what is pending on whom, and the issue register. Updated at every milestone, RFC disposition and owner ruling. |
| As of | 2026-10-27 — **RFCs 001, 002 and 010 accepted**; **M0 under construction by the architect at the owner's request** (T1 workspace and gates, T2 `inklkunn-markup` committed; T3–T7 open); RFC 003 proposed |
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
  No binary runs yet; `inklkunn-prikk`, `inklkunn-store`, the core seams and the spikes are T3–T6.

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
`rfcs/handoffs/010-upstream-and-supply-chain-maintenance/upstream-and-supply-chain-handoff-v1.md`: T3
`inklkunn-prikk` (the `Storage` trait and the sandboxed read driver), T4 `inklkunn-store`, T5 the core seams
and the bundle layout, T6 the two spikes, T7 the conventions (README, CHANGELOG, SECURITY, ledgers, book),
then writes the review request with the gate runs, the dependency deltas and the author-review gap, and the
owner (or a fresh reviewing session) reviews. RFC 002's handoff follows 001's approval. RFC 004 (proposals) is
drafted next.

## Issue register

| ID | Issue | Origin | Disposition |
|---|---|---|---|
| IS-1 | `README.md` is a one-line placeholder with a typo ("powerd") and does not follow the project's README structure | startup review 2026-10-24 | rewritten in M0 with the workspace, per the project rules' README structure |
| IS-2 | The M0 spike on content-level merge (internal design MRG-4, roadmap R2) has no corpus yet | internal design 2026-10-27 | the RFC 001 handoff names the corpus to build (wiki-shaped edit histories) |

## Process lessons (keep)

- The git-excluded material under `.git-exclude/` is the specification input and is not in history; a new
  session reads it, and this file, before acting.
