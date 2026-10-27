# Changelog

Releases are bare version tags; each section is the release's notes (assembled into the GitHub release by
`release.yml`). Kept by hand, per release.

## [Unreleased]

M0 — foundations (RFC 001, RFC 010). Nothing serves pages yet.

- **The workspace and its gates.** Eight crates in three layers (`inklkunn-prikk`, `inklkunn-signer`,
  `inklkunn-store`; `inklkunn-markup`, `inklkunn-core`, `inklkunn-auth`; `inklkunn-web`, `inklkunn`) and four
  gates as tests: layering, key reachability (only `inklkunn-core` may depend on the signer; its key store
  stays private), prikk spawn (only the driver and the signer spawn prikk; the driver names no write verb),
  markup purity. Pinned build chain: one compiler for CI and release, MSRV 1.88.0 built separately, actions
  by commit SHA, images by digest, the prikk floor 0.54.0 in one place.
- **`inklkunn-markup`.** Frontmatter in the plain YAML subset (anchors and tags refused, YAML's date and
  boolean coercions kept visible), the Portable Wiki Markdown Layer 2 scanner, MKUP-5 link resolution, HTML
  rendering with the MKUP-12 conventions, advisory lint with guideline ids, and `title_to_path` under a
  property test. wiki-commons' conformance corpus (24 scan cases, 10 resolution cases) passes.
- **`inklkunn-prikk`.** The `Storage` trait and `PrikkCli`: eight read verbs with pinned `schema_version`s
  and typed outcomes, bubblewrap with the repository read-only, a server-kept deadline, a `prlimit` ceiling
  from a per-verb cost table, and a light/heavy admission gate.
- **`inklkunn-store`.** The wiki record, validated slugs, opaque time-ordered ids, the `<root>/<shard>/<id>/`
  layout, in-memory and SQLite stores under one invariant suite.
- **`inklkunn-core` and `inklkunn-signer`.** The `authorize()` seam (default deny), the `Accept` trait, the
  `Assurance` values, the egress guard, the bundle layout, and *create wiki* end to end through a bootstrap
  signer (plain-text key directory; RFC 002 replaces it).
- **The two spikes** (RFC 001 D-10), measured on prikk 0.54.1 and reported under `rfcs/handoffs/001-foundations/`:
  the per-verb cost table is inklkunn's own (`diff` 4.6 × the store size is the expensive read; `tree` and `cat`
  1.0 ×; `verify` 2.0 ×; `branch` and `log` flat), and the content-merge spike shows that prikk's merge is
  confluent for 53 of 120 concurrent-edit pairs while a block-aware three-way merge is clean for 99 — the
  justification for inklkunn's own merge in RFC 004. `tools/corpus` generates both corpora from a seed;
  `tools/spikes/` holds the harnesses. prikk refuses repository paths over 1024 bytes, which bounds a wiki's
  hierarchy depth (about 500 one-character levels).
- **Conventions and ledgers** (RFC 001 D-11; RFC 010): README, this changelog, `SECURITY.md` stating RR-1, the
  dependency policy with a generated lock inventory that a test keeps equal to `Cargo.lock`, the upstream
  dependency ledger (PK/PL/WC rows), `docs/UPSTREAM.md`, `docs/RELEASING.md`, the mdBook.
