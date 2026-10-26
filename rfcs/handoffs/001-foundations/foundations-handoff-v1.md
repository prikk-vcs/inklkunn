# Foundations — Developer Handoff v1 (RFC 001)

| | |
|---|---|
| Document | The implementation package for RFC 001: tasks in build order, the QA checklist drawn from the RFC's acceptance criteria, and the definition of done and handback. A handoff directs implementation and never redefines its RFC; a design conflict goes back to the RFC first. |
| Status | Inherits RFC 001's: **accepted 2026-10-27 — assignable now**. |
| Basis | RFC 001 (D-1 … D-11); internal design v0.1 (`CR-*`, `LAY-*`, `STO-*`, `MK-*`, `DM-1`, `DM-2`, `ENF-1`, `ENF-4`, `ENF-7`); external design `CT-01`, `CT-02`; threat model `C-4a`, `C-4b`, `C-9a`; RFC 010 (the gates and workflows that land with M0); the project rules (Rust 2024, tests apart from implementation, README structure, mdBook). |
| Audience | The dev team (the mid-capability model). Read `.git-exclude/roles/mid-capability-model-operating-instructions.md` first, then `docs/STATUS.md`, then this. |
| Scope | M0's foundation: the workspace and gates, `inklkunn-markup`, `inklkunn-prikk`, `inklkunn-store`'s wiki record, `inklkunn-core`'s seams, the two spikes, the repository conventions. **Not** the key store or signing (RFC 002), **not** any web route (RFC 003), **not** proposals (RFC 004). |

## Before you start

- prikk **0.54.x** installed on the development host (`prikk --version`), plus `bwrap` and `prlimit`.
- Read the prikk reference pages for `commands`, `data-model`, `concurrency-locking` and the `tree-and-cat`
  guide; the driver's typed JSON mirrors their `schema_version`s.
- wiki-commons is at `.git-exclude/projects/wiki-commons`; copy its `corpus/markup` and `corpus/resolution`
  cases into `crates/inklkunn-markup/tests/corpus/` with a `CORPUS-SOURCE.md` naming the commit.
- Every task below names its tests. A task is not done until they exist, pass, and have run **with prikk on
  PATH and once with it shimmed off** (a directory holding only `cargo`, `rustc`, `rustup` symlinks to the
  real toolchain binaries, first on PATH).

## Task breakdown, in build order

### T1 — The workspace and its gates (RFC 001 D-1, D-2; RFC 010 D-3, D-9)
- **Purpose.** The build graph carries the invariants from the first commit.
- **Change scope.** Root `Cargo.toml` (workspace, `resolver = "3"`, `edition = "2024"`, `rust-version`,
  workspace lints as RFC 001 D-1, one version `0.0.0`); the eight member crates as skeletons with `lib.rs`
  or `main.rs` and `#![forbid(unsafe_code)]`; `tools/boundary-check` (std-only tests: layering,
  key-reachability, prikk-spawn, markup-purity, each with a planted-failure test); `tools/floor-check`;
  `tools/pin-check` (RFC 010 D-9); `.github/workflows/ci.yml` running fmt, clippy `-D warnings`, tests, the
  MSRV job, deny, audit, the gates, rustdoc `-D warnings`; `deny.toml` and `.cargo/audit.toml` with the
  license allow-list of RFC 010 D-6 and no ignores.
- **Non-change scope.** No product logic in any crate yet.
- **Required tests.** Each gate catches its planted failure; the workspace builds on the declared minimum
  compiler.

### T2 — `inklkunn-markup` (RFC 001 D-6, D-7)
- **Purpose.** The pure profile crate, corpus-tested, with the title↔path function.
- **Change scope.** Frontmatter parser for the plain YAML subset with schema validation and unknown-key
  preservation; the profile's Layer 2 constructs over `pulldown-cmark` (free links in every MKUP-4 form,
  embeds, block identifiers, inline tags, callouts, hidden comments, interwiki prefixes, generic directives,
  snapshot envelopes recognized); link resolution by the MKUP-5 order against a `TitleIndex` the caller
  supplies; rendering to an HTML tree with MKUP-12 conventions (no sanitization here; that is web's); lint
  with guideline ids; `title_to_path` and `path_to_title` per RFC 001 D-6.
- **Non-change scope.** No I/O, no clock, no randomness (the purity gate enforces it); no sanitizer.
- **Required tests.** Every copied corpus case; property tests for `title_to_path` (never escapes `pages/`,
  deterministic, inverse recovers the title when no frontmatter); a frontmatter round-trip; a planted
  anchor and a planted tag in YAML are refused.

### T3 — `inklkunn-prikk` (RFC 001 D-3, D-4)
- **Purpose.** The `Storage` trait and the sandboxed, pinned, typed prikk read driver.
- **Change scope.** The `Storage` trait; `PrikkCli` implementing it over `branch`, `tree`, `cat`, `log`,
  `show`, `diff`, `extract`, `verify` with `--format json`; typed structs per report with the pinned
  `schema_version` set; the floor constant `MIN_PRIKK_VERSION = "0.54.0"` and the start-up check; the
  bubblewrap vector (`--unshare-all --clearenv --die-with-parent`, read-only system view, the repository and
  the scratch directory mounted) as the server's direct child with the server-kept wall-time bound; the
  `prlimit` data-ceiling prefix from a per-verb cost table carrying its measured prikk version; the
  admission gate (light and heavy pools, bounded wait, `busy` and `too large` outcomes); typed outcomes for
  non-zero exits (refusal with prikk's text, unavailable, damaged).
- **Non-change scope.** No write verb anywhere in this crate (the spawn gate enforces it).
- **Required tests.** Against a real repository at the floor (self-skipping with a visible line when prikk
  is absent): each verb returns typed values; an older prikk (shimmed) is refused; a doctored
  `schema_version` is refused; the sandbox denies a path outside the mounts; the ceiling turns a too-small
  budget into `unavailable`, never a verdict; the cost table's version equals the floor (a test fails
  otherwise).

### T4 — `inklkunn-store` (RFC 001 D-8; internal design DM-1)
- **Purpose.** The wiki record behind a trait, SQLite first.
- **Change scope.** `WikiStore` trait and in-memory plus SQLite (`rusqlite`, bundled, WAL, idempotent schema)
  implementations; the wiki record of DM-1; the on-disk root layout `<root>/<shard>/<wiki id>/`.
- **Required tests.** Create, open, rename, resolve by slug; the same invariant tests on both
  implementations; reopen persistence.

### T5 — `inklkunn-core` seams and the bundle layout (RFC 001 D-5, D-9)
- **Purpose.** The seams later RFCs fill, and a wiki created on disk as a bundle.
- **Change scope.** The `authorize()` seam (signature fixed per RFC 002 D-2, body default-deny, no roles
  yet); the `Accept` trait (RFC 002 D-4's signature; no implementation; `inklkunn-signer` holds an empty
  key-store module so the gate has a target); the `Assurance` enum (RFC 002 D-6 values); the egress guard
  (public unicast only, pinned addresses; planeter's `StdEgressGuard` shape re-derived, not copied); `create
  wiki`: `prikk init` under the root layout, `wiki-bundle.yaml`, empty `pages/`, `attachments/`,
  `knowledge/`, `governance/`, `.prikkignore`, and a first help page, committed and sealed **by a
  test-only signer** in this task (the real signer is RFC 002's); the manifest's fidelity declaration.
- **Non-change scope.** No roles, no proposals, no routes.
- **Required tests.** A created wiki passes `prikk verify`; the layout matches `CT-02`; the egress guard
  refuses loopback, private, link-local and IPv4-mapped targets; the `authorize()` seam denies everything.

### T6 — The two spikes (RFC 001 D-10) — measure only, scratch directories, no product code
- **Spike A, cost table.** Build repositories of 1 / 16 / 64 / 200 MiB, 1,000 and 10,000 pages, depth 1,000
  (a generator under `tools/corpus/`, committed); record peak memory and latency per verb with the release
  build; write the table into T3 with the prikk version.
- **Spike B, content merge (internal design MRG-4, roadmap R2).** Generate wiki-shaped edit histories
  (concurrent proposals on the same and different sections, renames, frontmatter edits); measure how often a
  base moves before a simulated acceptance, how often a block-aware three-way merge is clean, how often
  prikk's own `EditText` span anchors would still apply at the new tip (`prikk commit` of the proposal's
  files against the moved tip, in a scratch clone). Report numbers and the corpus (IS-2 closes).
- **Deliverable.** A measurement report under `rfcs/handoffs/001-foundations/` with the numbers and the
  commands; rows for the upstream ledger.

### T7 — Repository conventions (RFC 001 D-11; RFC 010 D-6, D-7)
- README in the rules' structure (replacing the placeholder; IS-1 closes); CHANGELOG with `[Unreleased]`;
  SECURITY.md stating RR-1 plainly and the reporting route; `docs/dependency-policy.md` with the first
  ledger rows (every crate T1–T5 added, measured per RFC 010 D-6); `docs/src/inklkunn-upstream-dependency-ledger.md`
  with UD-1 … UD-10 as rows; `docs/UPSTREAM.md`; `docs/src/SUMMARY.md` extended; the book builds.

## QA checklist (from RFC 001's acceptance criteria)

- [ ] Workspace builds; all gates pass and each catches its planted failure; MSRV job green.
- [ ] Driver: typed values for every verb; older prikk refused; doctored schema refused; sandbox denial
      shown; admission refusals as designed; cost table version equals the floor.
- [ ] Markup: all corpus cases; title↔path fuzz clean; frontmatter round-trip; YAML anchors and tags refused.
- [ ] Store: record lifecycle on both backends; a created wiki verifies with prikk.
- [ ] Spikes reported with corpora committed.
- [ ] README, CHANGELOG, SECURITY.md, CI, dependency and upstream ledgers, book build.
- [ ] Both test runs (prikk on PATH, prikk shimmed off) recorded with commands and exit codes.

## Definition of done and handback

Done when every task's tests exist and pass in both runs, the gates are green in CI, and the review request
is filed. The review request (`001-foundations-review-request.md` beside this file) carries: the
implementation summary; the commits (hash → what landed); every gate's command and exit code, both runs;
the dependency-tree delta with deny and audit results for every manifest change; the spike reports;
deviations from this handoff and why; known limitations; open questions for the architect. The architect
reviews for requirements and design conformance, correctness, security and evidence, and answers Approved,
Conditionally Approved, Corrections Required, Design Reconsideration Required, or Owner Decision Required.
