# inklkunn — project status

| | |
|---|---|
| Document | The dated brief a new session reads **first**: where the project is, what is pending on whom, and the issue register. Updated at every milestone, RFC disposition and owner ruling. |
| As of | 2026-10-25 — startup complete (2026-10-24); the roadmap is authorized; **Phase 1 (the charter and the requirements specification v0.1) is written and awaits the owner's approval** |
| Read next | [`src/inklkunn-00-project-charter.md`](src/inklkunn-00-project-charter.md) → [`../ROADMAP.md`](../ROADMAP.md) → [`src/inklkunn-01-requirements-spec.md`](src/inklkunn-01-requirements-spec.md) → [`../rfcs/README.md`](../rfcs/README.md) |

## Where we are

- **2026-10-24 — startup review complete** (the two requirements-analysis transcripts, the project rules,
  the roles, and the three sibling projects prikk 0.54.1, planeter 0.4.0 and wiki-commons 0.4).
- **RFC 000 adopted** (`rfcs/done/000-rfc-lifecycle-policy.md`, the 5-folder variant); the `rfcs/` scaffold
  and index exist; no other RFC exists yet.
- **Roadmap authorized** (`ROADMAP.md`, owner: *"Accepted. Proceed on the next."*): milestones M0–M5 to an
  owner-gated 1.0, the planeter module as a joint RFC, and the eight decisions DR-1 … DR-8 ruled as
  recommended.
- **Phase 1 delivered for approval (2026-10-25):** the project charter and the requirements specification v0.1
  (`docs/src/`). The approval gate is the owner's: major requirements and scope (workflow §5, Phase 1).
- **Nothing is built.** No crate, no binary, no CI. Rust 2024 is the target; the workspace arrives with M0.

## Pending on the owner

- **Approve the requirements specification v0.1 and its scope**, or name what to change. The open
  questions it raises (its §16, OQ-1 … OQ-6) can be ruled with the approval or at their triggers; none
  blocks Phase 2.
- Name the Japanese reviewers for when the first catalog exists (DR-7; the planeter precedent is the owner
  and a colleague).
- Say whether the four kickoff documents the requirements transcript links (concept, requirements
  analysis, project plan, architecture specification v0.1) exist anywhere; Phase 1 assumed they do not.

## Pending upstream

| Upstream | What touches inklkunn | inklkunn action |
|---|---|---|
| prikk 0.54.1 | the floor for M1 (`extract`, `diff`, `tree`, `cat`, the `show <block-id>` fix); RFC 170 (library embedding) unscheduled; RFC 154 unscheduled | pin 0.54; re-baseline at each release (RFC 010); nothing asked yet |
| planeter 0.4.0 | no module mechanism exists; OQ-4 (wikis in its v1) unruled; RFC 012's catalog pattern and glossary are reused | the joint RFC 008 is drafted after M2's design; a letter through the owner when it is |
| wiki-commons 0.4 | identifiers may change before its 1.0 | conformance claims are versioned (`@0.4`) |

## Next

Phase 2 on approval: the design set (external design, threat model v0.1, internal design), then RFCs 001
(foundations), 002 (identity, keys and signing) and 010 (upstream tracking and supply chain) with their
handoffs; then the dev team is organized and M0 begins.

## Issue register

Small, known items that have no RFC of their own. Nothing here is assigned until a handoff names it.

| ID | Issue | Origin | Disposition |
|---|---|---|---|
| IS-1 | `README.md` is a one-line placeholder with a typo ("powerd") and does not follow the project's README structure | startup review 2026-10-24 | rewritten in M0 with the workspace, per the project rules' README structure |

## Process lessons (keep)

- The git-excluded material under `.git-exclude/` is the specification input and is not in history; a new
  session must read it, and this file, before acting.
