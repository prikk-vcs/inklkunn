# inklkunn — project status

| | |
|---|---|
| Document | The dated brief a new session reads **first**: where the project is, what is pending on whom, and the issue register. Updated at every milestone, RFC disposition and owner ruling. |
| As of | 2026-10-27 — requirements v0.1 approved and baselined; the design set v0.1 written; **RFCs 001, 002 and 010 proposed with their handoffs — awaiting the owner's acceptance** |
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
- **Nothing is built.** No crate, no binary, no CI. The workspace arrives with M0.

## Pending on the owner

- **Accept RFCs 001, 002 and 010** (`rfcs/proposed/`), or name what to change. Acceptance moves them to
  `accepted/` and makes their handoffs assignable; the dev team is then organized and M0 begins. One small
  question rides with RFC 010: whether the first release publishes a container image as well as binaries
  (recommended: both).
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

On acceptance: the dev team is organized (`.git-exclude/tasks/dev-team/`), RFC 001's and RFC 010's handoffs are
assigned together, RFC 002's follows when 001 has landed; M0 begins. The architect drafts RFC 003 (read path
and rendering) meanwhile.

## Issue register

| ID | Issue | Origin | Disposition |
|---|---|---|---|
| IS-1 | `README.md` is a one-line placeholder with a typo ("powerd") and does not follow the project's README structure | startup review 2026-10-24 | rewritten in M0 with the workspace, per the project rules' README structure |
| IS-2 | The M0 spike on content-level merge (internal design MRG-4, roadmap R2) has no corpus yet | internal design 2026-10-27 | the RFC 001 handoff names the corpus to build (wiki-shaped edit histories) |

## Process lessons (keep)

- The git-excluded material under `.git-exclude/` is the specification input and is not in history; a new
  session reads it, and this file, before acting.
