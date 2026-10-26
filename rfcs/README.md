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

The planned order is `../ROADMAP.md` §7. **RFCs 001, 002 and 010 were accepted by the owner on 2026-10-27**; their handoffs are assignable (002's once
001 has landed). RFC 003 is proposed. RFCs 003–009 are
reserved subjects not yet written: 003 read path and rendering (M1), 004 proposals (M2), 005 knowledge layer
(M3), 006 governance and moderation (M4), 007 interchange and API (M5), 008 the planeter module (joint),
009 real-time co-editing (deferred).

| # | RFC | Serves | State |
|---|---|---|---|
| 001 | [Foundations: the workspace and its gates, the storage driver, and the wiki repository layout](accepted/001-foundations.md) | M0 → M1 | **accepted** 2026-10-27; handoff v1 assignable |
| 002 | [Identity, keys and signing](accepted/002-identity-keys-and-signing.md) | M0 → M2 (DR-4) | **accepted** 2026-10-27; handoff v1 assignable after 001 lands |
| 003 | [Read path and rendering](proposed/003-read-path-and-rendering.md) | M1 | **proposed** 2026-10-27 |
| 010 | [Upstream tracking and supply-chain maintenance](accepted/010-upstream-and-supply-chain-maintenance.md) | all | **accepted** 2026-10-27; handoff v1 assignable with 001's |

## Current state

| State | RFCs |
|---|---|
| Proposed | [003](proposed/003-read-path-and-rendering.md) — awaiting the owner's acceptance |
| Accepted | [001](accepted/001-foundations.md), [002](accepted/002-identity-keys-and-signing.md), [010](accepted/010-upstream-and-supply-chain-maintenance.md) — accepted 2026-10-27 |
| Done | [000 — RFC lifecycle policy](done/000-rfc-lifecycle-policy.md) (adopted 2026-10-24) |
| Archived | *(none yet)* |
