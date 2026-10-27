# Review request — handoff 001 (foundations), v1

| | |
|---|---|
| Handoff | `foundations-handoff-v1.md` (RFC 001, accepted 2026-10-27), tasks T1 … T7 |
| Implementer | the architect session itself, at the owner's request ("You are right. However, I would like to ask you to proceed on the first implementation carefully.", 2026-10-27) — **the author reviews its own work**; see "Independence" |
| Submitted | 2026-10-27 |
| Asks | Approved / Conditionally Approved / Corrections Required / Design Reconsideration Required / Owner Decision Required |

## Implementation summary

M0 is built as RFC 001 decided it: eight crates in three layers with four gates as tests (**T1**); the pure,
corpus-tested markup crate (**T2**); the sandboxed, pinned, typed prikk read driver with its cost table and
admission gate (**T3**); the wiki record store (**T4**); the core seams and *create wiki* end to end through a
bootstrap signer (**T5**); the two spikes, measured and reported (**T6**); the conventions and ledgers
(**T7**). Nothing serves pages yet; that is M1. The two spikes changed two things in the design's
assumptions: the cost table is now measured (and `diff` is the expensive read), and prikk's merge is per
file for text, so the block-aware merge of MRG-4 is confirmed as inklkunn's own work (RFC 004).

## Commits (hash → what landed)

| Commit | What landed |
|---|---|
| `086ff86` | **T1** workspace (8 crates, 3 tool crates), lints, the four gates as tests with planted failures, floor-check, pin-check, CI / supply-chain / release workflows, `deny.toml`, `.cargo/audit.toml`, `Dockerfile` |
| `6cb3c92` | **T2** `inklkunn-markup`: frontmatter (plain YAML subset), scanner, resolver, renderer, lint, `title_to_path`; the wiki-commons corpus (34 cases) with its digest; CI fixes (shimmed-PATH `rustdoc`, `release.yml` YAML) |
| `d3696ea` | **T3** `inklkunn-prikk`: `Storage`, `PrikkCli`, sandbox, cost table, admission, the committed fixture repository; the spawn gate learns to skip serde attributes |
| `c9fb329` | **T4** `inklkunn-store`: record, slugs, ids, root layout, in-memory and SQLite stores under one invariant suite |
| `215a83a` | **T5** `inklkunn-core` seams (`authorize`, `Accept`, `Assurance`, egress guard), the CT-02 bundle, *create wiki*; `inklkunn-signer`'s bootstrap signer and empty private `key_store`; the sandbox's write mounts |
| `ebe9cd6` | the ceiling mapping reads a death by signal as the ceiling (GitHub runners) |
| `3d82a1e`, `301f975` | **T7** README (IS-1), CHANGELOG, SECURITY.md, dependency policy with lock inventory and test, upstream ledger, UPSTREAM.md, RELEASING.md, book; **T6** corpus generator and harnesses; cargo-deny wildcard fix |
| (this commit) | **T6** spikes report and data; the measured cost table; the shim tests made immune to the fork–exec race; this request |

## Gates: commands and exit codes

Final commit, 2026-10-27, local toolchain 1.99.1 with 1.88.0 for the MSRV run, prikk 0.54.1, bubblewrap and
prlimit present.

| Command | Exit |
|---|---|
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | 0 |
| **Run 1** `cargo test --workspace --locked` (prikk on `PATH`) | 0 — **97 passed, 0 failed**, five consecutive runs; the driver and create-wiki tests print `policies: confined (bubblewrap) and unconfined` |
| **Run 2** the same with prikk shimmed off `PATH` (`cargo`, `rustc`, `rustdoc`, `rustup` reachable; `~/.cargo/bin` removed) | 0 — every prikk-dependent test prints `skipping: prikk not on PATH` (9 lines); 86 passed |
| `cargo deny check` | 0 |
| `cargo audit` (database fetched 2026-10-27) | 0 — 80 crates, 0 vulnerabilities |
| `mdbook build docs` | 0 |
| GitHub CI (`build`, `msrv`, `supply-chain`) | green from `301f975` on |

Each gate catches its planted failure (`tools/boundary-check/tests/gates.rs`, `tools/pin-check/tests/pins.rs`,
`tools/floor-check/tests/floor.rs`): layering, key reachability (a planted `pub mod key_store`), prikk spawn
(a planted write verb), markup purity (a planted `std::fs`), unpinned action, moving runner, undigested image,
floor drift, corpus drift.

## What each task delivers against its required tests

- **T1.** Workspace builds; four gates as tests; MSRV job separate and green; one compiler for CI and
  release; actions by SHA, images by digest.
- **T2.** All 24 scan cases and all 10 resolution cases of the corpus pass (`tests/corpus.rs` fails if a case
  is missing); `title_to_path` under proptest (never escapes `pages/`, deterministic, inverse recovers clean
  titles); a planted YAML anchor and a planted tag are refused; frontmatter coercions kept visible (case 013).
- **T3.** Every verb returns typed values from the committed fixture under both policies; an older prikk
  (shim `prikk 0.53.9`) and an unparsable one are refused at open; a doctored `schema_version` is refused before
  the shape is read; the sandbox denies writing into the repository (`materialize` into the repo fails, the
  directory does not exist afterwards) and allows only the scratch sibling; a 64 KiB ceiling is `Unavailable
  { CeilingExceeded }`, never a verdict; the admission gate answers `TooLarge` and `Busy`; `BOUND_TO` equals
  the floor and `PROVISIONAL` is `false` at compile time.
- **T4.** Create, open by id and by slug, rename (id and path unchanged), slug uniqueness, verification and
  keys, list order — the same suite on both backends; SQLite persists across reopen; the root layout never
  contains the slug.
- **T5.** `authorize()` denies every (principal, action, resource) in M0; the egress guard refuses 24 literal
  non-public targets, `http`, userinfo and malformed URLs, and pins public literals without DNS; a created wiki
  passes `prikk verify` through the driver, has the CT-02 layout with nothing of ours in `.prikk/`, records key
  ids and no seed, and reads back its help page and `Kind: system` message; a taken slug is refused before
  anything is written.
- **T6.** `001-foundations-spikes-report.md` with the data; the corpus generator committed under
  `tools/corpus/` (IS-2 closes).
- **T7.** README in the rules' structure, CHANGELOG `[Unreleased]`, SECURITY.md stating RR-1, the dependency
  policy with every T1–T5 row measured, the upstream ledger with UD-1 … UD-10 mapped, UPSTREAM.md,
  RELEASING.md, SUMMARY.md extended, the book builds.

## Dependency-tree deltas

See the 010 review request's table (the same commits): 0 → 11 → 69 (+58, markup) → 69 (prikk) → 79 (+10,
rusqlite) → 80 (+1 workspace member). deny and audit clean at every step; no license outside the allow-list.

## Spike reports

`001-foundations-spikes-report.md`. Headlines: `diff` needs 4.6 × S of address space (the table now says
5.0 ×); `tree`/`cat` 1.0 × S, `verify` 2.0 × S, `branch`/`log` flat on wiki-shaped text; prikk refuses
repository paths over 1024 bytes (depth ≈ 500); prikk's merge is confluent for 53 of 120 concurrent-edit
pairs, a block-aware merge is clean for 99 of 120, and the same-paragraph 21 are true conflicts; a base moves
before acceptance in most cases once reviews take a day.

## Deviations from the handoff, and why

1. **The architect implemented** (owner's request). Independence is addressed below.
2. **The signer exists beyond "an empty key-store module"**: `inklkunn-signer` holds the write sequence
   (`init`, `trust maintainer add`, `commit`, `seal`, `key generate`, `key public`) and a bootstrap signer over a
   plain-text key directory, because *create wiki* (T5) needs a sealed first change and the spawn gate allows
   write verbs only there. It is documented as test/bootstrap only; RFC 002 D-3/D-4 supersede it. **This is the
   deviation most worth a second look**: the plain-text key directory must never be reachable from a request
   path, and nothing in M0 exposes a route.
3. **The driver's sandbox gained a `Mounts` type with a writable-repository mode** for the signer's calls
   (repo rw, key directory ro, `PRIKK_*_SEED_FILE` variables). The read path still binds read-only; the gate
   still finds no write verb in the driver crate.
4. **The fixture repository is committed** (`crates/inklkunn-prikk/tests/fixtures/wiki-repo`, built with a
   throwaway key pair whose seeds were discarded) instead of built by the tests, so the driver crate's tests
   need no write verb at all.
5. **"Depth 1,000" was measured as depth 500**: prikk's 1024-byte path limit (PK-11, IS-3).
6. **The cost table keeps planeter's single-large-file multiples where they exceed the measurement** (`tree`,
   `cat`, `verify`, `show`), because attachments are that shape and M0 had none; the shape joins the M1
   re-baseline.
7. **The first CI runs were red** for four reasons, each fixed in its own commit (shimmed `rustdoc`;
   `release.yml` YAML; the ceiling's signal shape on GitHub runners; cargo-deny's wildcard rule).
8. **Two shim tests raced once** in a full-workspace run (`ETXTBSY` when a sibling test forked while the shim
   was being written); the tests now retry that one error.

## Known limitations

- The `authorize()` seam denies everything, including public read: the visibility rule and the roles are
  RFC 002's handoff, as the handoff said. No route exists yet to call it from.
- `render` places block identifiers as empty `<span id="^id">` elements rather than `id` attributes on the
  enclosing block (MKUP-12 wants the latter); RFC 003's handoff can move them when the HTML tree is shaped
  for the web layer. Transclusion (`![[Page]]`) renders as a link; RFC 003 decides depth and cycles.
- The `Index` resolver lower-cases rather than case-folds (no `std` case folding); differs for a few
  scripts (ß).
- `PK-8` (a failing `verify` verdict's exit code) is PARTIAL; the driver reads the verdict from stdout
  regardless.
- The admission gate is a process-local `Mutex`+`Condvar`; one server process is the M1–M2 deployment shape.

## Independence

The author reviewed its own work, which RFC 000's workflow does not intend. What compensates, and what the
reviewer should weigh: every gate is a test with a planted failure; the corpus is an external oracle (wiki-
commons' expectations, not ours); the fixture, the shims and the ceiling test exercise the failure paths,
not only the happy ones; the spikes are reproducible from committed tools and seeds; this request names its
own red runs and the deviation that matters most (2). A fresh session reading only `main` and this request,
or the owner, is asked to review for requirements and design conformance, correctness, security and evidence.

## Open questions for the architect / owner

1. Deviation 2: keep the bootstrap signer in `inklkunn-signer` behind its documentation, or gate it behind a
   cargo feature that production builds never enable? (Recommendation: a feature `bootstrap`, in RFC 002's
   handoff, once the real store exists; today nothing else can create a wiki.)
2. Should `authorize()` allow public read already in M0 (it is the one decision M1 cannot ship without), or
   stay default-deny until RFC 002's handoff fixes the visibility rule with the route-enumeration test?
   (Recommendation: stay; M1's read path lands with RFC 002's roles.)
3. The spikes report's `diff` finding: should RFC 003 forbid whole-tree diffs on the web (always `--path`)?
