# inklkunn RFCs

Design records for inklkunn, one per subsystem or decision. This directory follows the ecosystem's
**RFC lifecycle policy** ([`done/000-rfc-lifecycle-policy.md`](done/000-rfc-lifecycle-policy.md),
adopted 2026-10-24), **5-folder variant**: the architect (design) and the dev team (implementation) are
distinct roles, so "the design is settled" (`accepted/`) is a real event separate from "the work
shipped" (`done/`).

The **roadmap, milestones, release cycles and the RFC priority and order** live in
[`../ROADMAP.md`](../ROADMAP.md). This file is the **index**: it lists every RFC by state. The folder is
the source of truth for state; a file's Status field is kept consistent with its folder.

## Folders

```
rfcs/
  README.md      ← this index
  proposed/      ← open for review (implementer should not yet start)
  accepted/      ← design settled; implementer may start; not yet shipped
  done/          ← shipped
  archive/       ← withdrawn or superseded
  handoffs/      ← companion execution docs, under handoffs/NNN-slug/; their state follows the RFC
```

## Pipeline (from the roadmap)

The planned order is `../ROADMAP.md` §7. **RFCs 001 and 010 are done** (implemented in M0, review requests v2
approved by the owner on 2026-10-28); 001 has one follow-on handoff (IS-7, 2026-10-28) and 010's first re-baseline
waits for prikk 0.55.0. **RFCs 002 and 003 are accepted** (2026-10-27): 002's handoff is assignable
now; 003's T1–T3 are startable now and T4 onward after 002's T2. **RFC 004 is accepted** (2026-10-28); its
handoff is assignable after 002's corrections C1 – C7 and 003's T1 – T4 are approved (its T1–T2 may start now: 002's T5 has landed). RFCs 005–009 are reserved
subjects not yet written: 005 knowledge layer
(M3), 006 governance and moderation (M4), 007 interchange and API (M5), 008 the planeter module (joint),
009 real-time co-editing (deferred).

| # | RFC | Serves | State |
|---|---|---|---|
| 001 | [Foundations: the workspace and its gates, the storage driver, and the wiki repository layout](done/001-foundations.md) | M0 → M1 | **done** 2026-10-28 (approved after review; handoff review request v2); **follow-on handoff assignable** (2026-10-28): verification verdicts under a memory ceiling (IS-7), before 003's T4 |
| 002 | [Identity, keys and signing](accepted/002-identity-keys-and-signing.md) | M0 → M2 (DR-4) | **accepted** 2026-10-27; handoff built and **Conditionally Approved 2026-10-29**; corrections handoff assignable (C1 – C4 before 003's T7; C11, added with the owner's DR-10 ruling of 2026-10-29, before 0.1.0); D-8 ruled (DR-10) |
| 003 | [Read path and rendering](accepted/003-read-path-and-rendering.md) | M1 | **accepted** 2026-10-27, concretized 2026-10-28 and 2026-10-29; T1 – T3 built and **Conditionally Approved 2026-10-29**; T4 next, after 001's follow-on and under the 2026-10-29 notes (author HTML escaped, reads pinned); T7 after 002's corrections C1 – C4 |
| 004 | [Proposals: propose, review, accept and seal; re-author a moved base gently; never lose a half-state](accepted/004-proposals.md) | M2 | **accepted** 2026-10-28; handoff v1 assignable after 002's corrections C1 – C7 and 003's T1 – T4 are approved |
| 010 | [Upstream tracking and supply-chain maintenance](done/010-upstream-and-supply-chain-maintenance.md) | all | **done** 2026-10-28 (approved after review); its procedures run at every release and re-baseline |

## Current state

| State | RFCs |
|---|---|
| Proposed | *(none)* |
| Accepted | [002](accepted/002-identity-keys-and-signing.md), [003](accepted/003-read-path-and-rendering.md) — accepted 2026-10-27; [004](accepted/004-proposals.md) — accepted 2026-10-28 |
| Done | [000 — RFC lifecycle policy](done/000-rfc-lifecycle-policy.md) (adopted 2026-10-24); [001](done/001-foundations.md), [010](done/010-upstream-and-supply-chain-maintenance.md) (M0, approved 2026-10-28) |
| Archived | *(none yet)* |
