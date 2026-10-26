# inklkunn — Project Charter: background, goals, and governance

| | |
|---|---|
| Document | The project's background and goals in one place: why inklkunn exists, what "done" means, what it will not do, who decides what. The requirements specification (`inklkunn-01`) is the contract; this is the frame around it. |
| Version | v0.1 |
| Date | 2026-10-25 (begun at the end of the startup phase on 2026-10-24, the day the roadmap was authorized) |
| Basis | The requirements analysis under `.git-exclude/specs/`; the project rules under `.git-exclude/rules/`; the roadmap (`ROADMAP.md`) and the owner's rulings DR-1 … DR-8; prikk RFC 145 (the ecosystem builds over prikk's CLI); planeter's design set and open question OQ-4; wiki-commons 0.4. |

## 1. Background

**prikk** is a verifiability-focused version-control system built around block-oriented patch theory: an
offline, single-repository CLI with Ed25519 author and maintainer signatures, sealed immutable blocks,
offline `verify`, and artifact-based exchange. Around it an ecosystem follows one pattern: each companion
carries a weight prikk refuses, so prikk stays small. **planeter** carries the *hosting* weight (the forge).
**wiki-commons** is the owner's open guideline suite for wiki engines: a pattern language and portable
conventions so that knowledge outlives the software it was written in.

The owner's question that opened this project: *what becomes possible when a wiki's storage is a
patch-theoretic VCS, and could such a wiki foster an inclusive, open community culture?* The answer worked
out in the requirements analysis is that the interesting move is not "store wiki data in a VCS" but **model
the knowledge community itself as a patch system**: knowledge is a set of revisable claims; every change is
a proposal; disagreement can coexist as branches; forks are legitimate; governance is explicit and itself
versioned. The same analysis recorded why people give up on wikis: an edit reverted without a word, opaque
local rules, the pressure to be perfect, unfamiliar markup, formalism, information chaos, the burden of
maintenance landing on whoever wrote the page, and no recognition. inklkunn is designed against that list.

planeter's open question **OQ-4** asks whether wikis belong in the forge's v1. inklkunn is the answer the
owner chose: a wiki engine that joins the forge as a module. **inklkunn** is Norwegian, "inclusive" +
"knowledge".

## 2. Goal

**Be a wiki engine where participation starts with a proposal, whose history is prikk's, usable on its own
and embedded in planeter.** A reader should be able to read, ask, suggest, add a source and propose a change
without knowing Markdown, Git, or the community's power structure; a reviewer should see exactly what a
proposal changes and why; a maintainer should accept it with one action; and anyone holding the repository
should be able to verify its history offline with prikk alone.

The commitments that make it *this* project (the owner's rulings of 2026-10-24):

1. **Wiki-first, claims layered in** (DR-1). Pages are the stored artifact; claims, evidence, views and
   provenance arrive as explicit objects above them, named from the start, load-bearing from the third
   milestone.
2. **An independent service that planeter embeds** (DR-2). inklkunn owns its repositories, keys and threat
   model; planeter integrates it at the identity, authorization and presentation level and may host the
   same repositories read-only. planeter stays keyless.
3. **Built above prikk, over its CLI** (DR-3). A sandboxed, version-pinned subprocess behind a storage
   trait; never prikk's crates; never weight pushed into prikk.
4. **The wiki service signs as itself** (DR-4). It holds one author and one maintainer key per wiki, records
   the human behind each change in the signed message and in its own records, and labels every signature
   honestly. Per-account and client-side keys are later opt-ins.
5. **A wiki repository is a Portable Wiki Bundle** (DR-5). Pages are Markdown files in the Portable Wiki
   Markdown profile with frontmatter; export is a copy and import is a commit; prikk's file-level patches
   fit page-level edits.
6. **Inclusive by construction.** A message catalog in English and Japanese (DR-7), plain words, WCAG 2.2 AA
   checks that fail the build, and no JavaScript required to read or to propose.
7. **Clean, safe, secure and robust before rich**, and **less is more** (the owner's standing design
   philosophy). Real-time co-editing is deferred (DR-6); the default content license for public wikis is
   CC BY-SA 4.0 (DR-8).

## 3. What "done" means

**1.0 is the usable wiki, complete**: read, propose and accept, the knowledge layer (claims, evidence, views,
provenance), explicit governance and moderation, interchange and an API, the planeter module, hardened and
backup-tested, with versioned conformance claims against wiki-commons. **Promotion to 1.0 is an explicit
owner decision**; green gates and feature completeness are necessary, not sufficient. Until then everything is
0.x and honest about what is not there.

Each milestone is usable on its own: M1 read a wiki → M2 propose and accept → M3 claims and views → M4
governance → M5 interchange and API, with the planeter module as a joint RFC alongside (`ROADMAP.md` §3).

## 4. What inklkunn will not do (the non-goals, as firm as the goals)

Not a code forge or a prikk host (planeter is); not a change to prikk (a needed capability is a named
dependency and a letter, never a prikk patch inklkunn writes); not a truth oracle (provenance is not truth;
no universal truth or reputation score); not a real-time editor or a block database in v1; not federation,
portable identity or AI authority in v1 (AI assistance, when it comes, is advisory and labelled); not a
second source of truth for history and never a rewriter of it; not a lock-in (export, archive and
independent verification are requirements); not a general content-management system, and never an
executor of content-provided code. Requirements NG-1 … NG-9.

## 5. Constraints inklkunn accepts

- **prikk's model binds the write path.** Every commit is author-signed, every seal maintainer-signed; prikk
  detects conflicts and never resolves them; same-file text transforms are deferred in prikk, so
  content-level merge is inklkunn's; prikk has no network, no object deletion and no ref deletion.
- **prikk moves fast and its JSON is pre-1.0.** inklkunn pins a floor (0.54 for M1) and re-baselines at each
  prikk release; schema drift is a refusal, not a guess.
- **planeter has no module mechanism today.** The module is a joint RFC on both sides; until it exists
  inklkunn ships standalone.
- **Linux first for the server** (the sandbox for the prikk subprocess is bubblewrap, as in planeter);
  prikk itself runs on Linux, macOS and Windows. The standalone binary makes no claim it cannot run.
- **The ecosystem's engineering rules**: Rust 2024, a declared and tested minimum compiler, `forbid(unsafe_code)`,
  a measured and allow-listed dependency tree, `--locked` everywhere, tests apart from implementation files,
  English for all documentation and code, bare `X.Y.Z` release tags.
- **Security is release-gating.** A release touching identity, signing, the prikk boundary, rendering or
  inbound content updates the threat model; the others re-verify it.

## 6. Who decides what

- **The owner** (nabbisen) decides direction, priorities, scope, milestones, the owner-only questions
  (requirements §16), every tag, publish and release, letters to the prikk and planeter teams, and the 1.0
  promotion.
- **The architect** (a high-capability AI agent) writes requirements, design, RFCs, handoffs, the schedule and
  the records; reviews every implementation; drafts upstream letters; recommends releases.
- **The dev team** (a mid-capability AI agent) implements from handoffs and returns evidence.
- **The prikk and planeter teams** are upstream and sibling: their rulings and schedules are recorded in the
  dependency ledger; asks go by letter through the owner.

The workflow, the RFC lifecycle and the handoff conventions are `.git-exclude/roles/`,
`rfcs/done/000-rfc-lifecycle-policy.md` and, once it exists, `rfcs/handoffs/README.md`.

## 7. History to date

| Date | Event |
|---|---|
| 2026-10-24 | Startup review of the requirements analysis, the rules, the roles and the three sibling projects; RFC 000 adopted; the roadmap proposed and **authorized the same day** with DR-1 … DR-8 ruled as recommended; this charter begun |
| 2026-10-25 | This charter and the requirements specification v0.1 written for Phase 1 approval |
| 2026-10-27 | Requirements v0.1 **approved by the owner** and baselined; Phase 2 (the design set) begins |

## 8. Where to read next

`docs/STATUS.md` (where things stand) → `ROADMAP.md` (milestones, release cycles, dependencies, the RFC
plan) → `docs/src/inklkunn-01-requirements-spec.md` (the contract) → `rfcs/` (the decisions, as they are
written).
