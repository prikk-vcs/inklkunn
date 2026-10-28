# RFC 001 — Foundations: the workspace and its gates, the storage driver, and the wiki repository layout

**Status.** **Done** — implemented on `main` (M0) and **approved by the owner on 2026-10-28** (review request v2 under `../handoffs/`, after the architect's independent review; *"Your recommendations are approved."*). Accepted by the owner 2026-10-27 (*"Accepted."*); proposed the same day by the architect. The
first inklkunn RFC and the substrate every later one sits on. **The implementer may start** per the handoff,
which is assignable now: [`../handoffs/001-foundations/foundations-handoff-v1.md`](../handoffs/001-foundations/foundations-handoff-v1.md).
**Follow-on (2026-10-28, assignable):** [`../handoffs/001-foundations/verify-verdict-honesty-handoff-v1.md`](../handoffs/001-foundations/verify-verdict-honesty-handoff-v1.md)
— the driver must read a `verify` report whose stage ran out of memory as a ceiling hit, never as a failed
verification (issue IS-7; measured on prikk 0.54.1); it lands before RFC 003's T4.
**Tracks.** ROADMAP M0 → M1, theme T1. Requirements `PU-3`, `PU-7`, `CM-1 … CM-5`, `CM-7`, `INT-1 … INT-3`,
`INT-10`, `SEC-5`, `SEC-7`, `SEC-8`, `OPS-1`, `OPS-5`; external design `BD-03 … BD-05`, `PK-01`, `PK-02`,
`PK-04`, `CT-01`, `CT-02`; internal design `CR-*`, `LAY-*`, `STO-*`, `MK-*`, `DM-1`, `DM-2`, `ENF-1`,
`ENF-4`, `ENF-7`; threat model `TB-3`, `TB-4`, `C-4a`, `C-4b`, `C-9a`, `INV-2` (its build gate), `INV-3`.
**Touches.** The Cargo workspace skeleton (`inklkunn-prikk`, `inklkunn-store`, `inklkunn-markup`,
`inklkunn-core`, `inklkunn-auth`, `inklkunn-signer`, `inklkunn-web`, the `inklkunn` binary),
`tools/boundary-check`, `tools/floor-check`, CI, the repository conventions (README, CHANGELOG, SECURITY,
docs). **Not here:** the key store and the signer's operations (RFC 002), the read surfaces (RFC 003), the
proposal loop (RFC 004), the upstream and supply-chain procedures (RFC 010, written alongside).

## Summary

inklkunn's value flows through prikk and through the files in a wiki's repository. This RFC fixes the three
things that must be right once: **(1) the workspace and its gates**, so that the security invariants are
properties of the build graph from the first commit; **(2) the storage driver**, the one place that runs
prikk's read verbs, sandboxed, version-pinned and typed; and **(3) the wiki repository layout**, a Portable
Wiki Bundle with a deterministic title-to-path function, so that a wiki is readable by any prikk and any
bundle importer with no inklkunn running. It also establishes the pure markup crate and the two spikes that
calibrate the roadmap's risks.

## The constraints that scope this design

- prikk is consumed over its CLI, never its crates (DR-3; prikk RFC 145); its JSON is pre-1.0, so a schema
  drift must be a refusal (INT-1).
- prikk parses attacker-influenced bytes (paths, content), so the process that runs it is confined (C-4a,
  TB-4).
- The web tier must never be able to reach key material (INV-2); the gate for that is a workspace property
  and belongs here even though the keys arrive with RFC 002.
- A wiki repository is a Portable Wiki Bundle (DR-5) and carries nothing of inklkunn's inside `.prikk/`
  (INT-10).
- Rust 2024; tests apart from implementation files; `forbid(unsafe_code)`; English everywhere (project
  rules).

## Decisions

- **D-1 — The workspace and its layers.** Eight members in three layers, dependencies downward only:
  lower `inklkunn-prikk`, `inklkunn-signer`, `inklkunn-store`; core `inklkunn-markup`, `inklkunn-core`,
  `inklkunn-auth`; upper `inklkunn-web` and the `inklkunn` binary; plus `tools/boundary-check` and
  `tools/floor-check` (not product crates, `publish = false`). Workspace-wide: `edition = "2024"`, one
  declared `rust-version` proven by a CI job on exactly that compiler, `unsafe_code = "forbid"`,
  `missing_docs = "warn"`, clippy `unwrap_used`, `expect_used` and `indexing_slicing` denied, one version
  for all crates, tests in `src/<mod>/tests.rs` or `tests/`, never inline.
- **D-2 — The gates are tests in the workspace** (`tools/boundary-check`, std-only), run by CI and by the
  release gates: (a) **layering**: no lower or core crate declares a dependency on an upper crate; (b)
  **key reachability**: only `inklkunn-signer` may name the key-store module, and only `inklkunn-core` may
  depend on `inklkunn-signer` (INV-2); (c) **prikk spawn**: no source outside `inklkunn-prikk` and
  `inklkunn-signer` spawns a `prikk` process, and `inklkunn-prikk` names no write verb; (d) **markup
  purity**: `inklkunn-markup` has no I/O, clock or randomness dependency; (e) each gate has a planted
  failure it must catch. `tools/floor-check` asserts the prikk floor is one value across the driver
  constant, CI and the container recipe.
- **D-3 — The `Storage` trait and the prikk driver.** `inklkunn-prikk` defines `Storage` in inklkunn's words
  (views and tips; tree at a point; file at a point; history; one change's content; diff; materialize a
  point; verify) and implements it over prikk's read verbs `branch`, `tree`, `cat`, `log`, `show`, `diff`,
  `extract`, `verify`, all `--format json`. Each report's `schema_version` is checked against a pinned set;
  an unknown one is a typed refusal. The floor is `0.54.0`, checked at start with `prikk --version`. A
  non-zero prikk exit maps to a typed outcome (refusal with prikk's text, unavailable, damaged), never a
  panic. The trait is the seam for another backend or a future library (UD-2, not assumed).
- **D-4 — Sandbox and bounds.** Every prikk invocation runs under bubblewrap as the server's direct child
  (`--unshare-all`, `--clearenv`, `--die-with-parent`, a read-only system view, the repository and the
  call's scratch directory mounted), with a wall-time bound kept by the server and a `prlimit` data ceiling
  from a per-verb cost table measured on the floor. An admission gate holds a memory budget split into a
  light pool and a heavy pool (planeter RFC 011's shape); refusals are `busy` or `too large`. The table
  carries the prikk version it was measured on, and a test fails when that is not the floor.
- **D-5 — The wiki repository layout is a Portable Wiki Bundle.** `wiki-bundle.yaml`, `pages/`,
  `attachments/` with `<name>.meta.yaml` sidecars, `knowledge/` and `governance/` (reserved; schemas by
  RFCs 005 and 006), `.prikkignore` for the service's scratch paths. Nothing of inklkunn's inside
  `.prikk/`. A page file is Portable Wiki Markdown Layer 2 with YAML frontmatter per CM-2 (`title`
  required, `id` UUID v7); UTF-8, LF, NFC, a trailing newline.
- **D-6 — Title ↔ path is a pure function in `inklkunn-markup`** (CT-01, C-4a): NFC; `/` is hierarchy; the
  characters `\ : * ? " < > |`, control characters and a leading `.` are replaced by a documented
  substitute; reserved names (`.`, `..`, `.prikk`, `wiki-bundle.yaml`, the reserved directories) are
  refused; a case-insensitive collision gets a numeric suffix on the file name only; the result is always a
  relative path under `pages/` with the extension `.md`; the inverse recovers the title where frontmatter
  is absent, and frontmatter wins where present. Fuzz-tested: no input escapes `pages/`.
- **D-7 — `inklkunn-markup` is pure and corpus-tested.** It parses CommonMark + GFM + footnotes with
  `pulldown-cmark`, layers the profile's constructs, parses the plain YAML subset for frontmatter
  (scalars, sequences, mappings; anchors and tags refused), resolves links by the MKUP-5 order against a
  caller-supplied title index, renders to an HTML tree with the MKUP-12 conventions, and lints. wiki-commons'
  `corpus/` markup and resolution cases are copied in as fixtures with their expected outputs; the copy
  records the wiki-commons commit.
- **D-8 — The wiki record and the on-disk root.** `inklkunn-store` holds the wiki record (DM-1): opaque id,
  slug, display name, default view, default license, languages, visibility, policy bootstrap, prikk format
  version, verification state. Repositories live at `<root>/<shard>/<wiki id>/`, never under the wiki's
  name; renames move no bytes.
- **D-9 — The seams established now.** In `inklkunn-core`: the `authorize()` seam (RFC 002 fills it; every
  surface calls it from the first route); the `Accept` trait (RFC 002's signer implements it; nothing else
  may); the honesty view-model types (the Assurance values of ID-9); the egress guard (one SSRF-filtering
  component for every outbound fetch). In `inklkunn-signer`: the crate exists with the trait and an empty
  key-store module so the D-2(b) gate has something to guard.
- **D-10 — The two spikes** (measure only, scratch directories, no product code): (a) the per-verb cost and
  latency table of D-4 on a corpus of wiki-shaped repositories (sizes 1 / 16 / 64 / 200 MiB; 1,000 and
  10,000 pages; depth 1,000); (b) the content-merge spike (internal design MRG-4, roadmap R2): on a corpus of
  wiki-shaped edit histories, how often a proposal's base moves before acceptance, how often a three-way
  merge is clean, how often prikk's own span anchors would have survived. Both report into the dependency
  and cost ledgers and into RFC 004.
- **D-11 — Repository conventions.** README in the project rules' structure (hero, overview, why, quick
  start, design notes, more detail); CHANGELOG with "Upgrading" and "Output changes" sections per release;
  SECURITY.md stating RR-1 plainly; `docs/` as an mdBook; CI running fmt, clippy with warnings denied, tests
  (with prikk on PATH and once with it shimmed off, so CI's own no-prikk run is seen), `cargo deny`,
  `cargo audit`, the gates of D-2, the MSRV job, and the rustdoc check with warnings denied. Dependency
  additions follow the policy of RFC 010.

## What "done" means (acceptance criteria)

- The workspace builds with the eight members and the two tools; all gates of D-2 pass and each catches
  its planted failure; the MSRV job is green on the declared compiler.
- `inklkunn-prikk`, against a real repository at the floor: refuses an older prikk; runs each read verb and
  returns typed values; refuses a doctored `schema_version`; runs confined (a test shows the sandbox denies
  a path outside the mounts); the admission gate refuses `too large` and `busy` as designed.
- `inklkunn-markup`: every copied corpus case passes; the title-to-path fuzz finds no escape; a page with
  frontmatter round-trips through parse and serialize unchanged in meaning.
- `inklkunn-store` creates, opens, renames and resolves a wiki record; a wiki is created on disk under D-8
  with `prikk init`, the manifest and an empty `pages/`, and `prikk verify` of it passes.
- The spikes of D-10 have reported, with their corpora committed under `tools/corpus/`.
- README, CHANGELOG, SECURITY.md, the CI workflows and `docs/` exist per D-11.

## Alternatives considered

- **Link `prikk-store` for in-process reads.** Rejected: prikk's crates carry no stability promise and
  prikk ruled the CLI the substrate (RFC 145); linking forfeits the process-boundary containment.
- **One repository holding many wikis.** Rejected: a wiki's keys, verification and export are per
  repository; sharing would couple their trust and their locks.
- **A bespoke page format.** Rejected (DR-5): the bundle layout buys interchange for free and fits prikk's
  file-level patches.
- **Gates by review instead of tests.** Rejected: a layer holds only when it is a checked rule (prikk's
  RFC 149, planeter's RFC 001).

## Open questions and dependencies

- The exact per-verb cost multiples (D-4) come from spike (a); the first table is a conservative guess
  labelled as such.
- Whether `inklkunn-markup` needs a directive registry beyond wiki-commons EXT-10's vocabulary is RFC 003's.
- UD-2 (a prikk library) is not assumed; the trait keeps the door open.

## Sequencing and handoff

First, and blocking everything else. On acceptance the architect's handoff (already drafted, v1) becomes
assignable; the dev team builds M0's foundation against it; RFC 002's handoff follows on this base.
All v0.x; 1.0 is the owner's gate.
