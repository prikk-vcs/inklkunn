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

## Pipeline (planned, from the roadmap)

The planned RFCs and their order are proposed in [`../ROADMAP.md` §7](../ROADMAP.md). None exists as a
file yet: an RFC is written only after the owner authorizes the roadmap and the decisions it depends on.

## Current state

| State | RFCs |
|---|---|
| Proposed | *(none yet)* |
| Accepted | *(none yet)* |
| Done | [000 — RFC lifecycle policy](done/000-rfc-lifecycle-policy.md) (adopted 2026-10-24) |
| Archived | *(none yet)* |
