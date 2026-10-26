# Upstream tracking and supply-chain maintenance — Developer Handoff v1 (RFC 010)

| | |
|---|---|
| Document | The implementation package for RFC 010: the gates, the workflows and the ledgers that land with M0. |
| Status | Inherits RFC 010's: **proposed — not startable** until accepted; T1 and T2 are built together with RFC 001's T1 (the same commits). |
| Basis | RFC 010 (D-1 … D-10); RFC 001 D-2, D-11; planeter's `tools/pin-check`, `tools/floor-check`, `docs/dependency-policy.md`, `docs/UPSTREAM.md` and `.github/workflows/` as the house precedent (re-derived for inklkunn, not copied blindly). |
| Audience | The dev team. |
| Scope | `tools/floor-check`, `tools/pin-check`, `.github/workflows/{ci,supply-chain,release}.yml`, `deny.toml`, `.cargo/audit.toml`, `docs/dependency-policy.md`, `docs/src/inklkunn-upstream-dependency-ledger.md`, `docs/UPSTREAM.md`, `docs/RELEASING.md`. **Not** any product crate. |

## Task breakdown, in build order

### T1 — `floor-check` (RFC 010 D-3)
- **Change scope.** A std-only test crate asserting the prikk floor is one `X.Y.Z` across
  `crates/inklkunn-prikk/src/...` (`MIN_PRIKK_VERSION`), the CI workflow's `PRIKK_VERSION`, and the container
  recipe's `ARG PRIKK_VERSION`; and that `crates/inklkunn-markup/tests/corpus/CORPUS-SOURCE.md` names a
  commit that matches a recorded hash of the fixture tree.
- **Required tests.** Fails on a planted mismatch in each of the four places.

### T2 — `pin-check` and the workflows (RFC 010 D-4, D-9, D-10)
- **Change scope.** `pin-check`: every `uses:` is a 40-hex SHA from a GitHub-owned namespace; every image
  `FROM` and `docker` reference carries a digest; one `toolchain:` version across all workflows (with the
  single `msrv` job exception equal to `rust-version`); no `*-latest` runner; the release workflow's
  rehearsal mode can never name the real package or run a publish step (the planeter rule set). `ci.yml`:
  fmt, clippy `-D warnings`, tests with prikk installed at the floor (download by checksum) and the
  shimmed-off run, the `msrv` job, deny, audit, the RFC 001 gates, floor-check, pin-check, rustdoc.
  `supply-chain.yml`: manual dispatch, deny and audit on a fresh database, `contents: read`. `release.yml`:
  tag-triggered gates → Linux x86_64 and aarch64 binaries → container image built natively per
  architecture → attestations with GitHub's own action → GitHub release from the CHANGELOG section →
  crates in dependency order; `rehearsal` dispatch to a throwaway package only.
- **Required tests.** Each pin-check rule has a planted failure; the rehearsal has run green once and
  published nothing real (record the run id).

### T3 — The ledgers and the policy (RFC 010 D-6, D-7)
- **Change scope.** `docs/dependency-policy.md` with the rules and the two tables, seeded with every crate
  RFC 001's tasks add, each row measured ("N → M crates (+K); deny; audit; new licenses") before its
  commit; `docs/src/inklkunn-upstream-dependency-ledger.md` with the status legend and rows PK-1 … (UD-1 …
  UD-10 and the facts the design set relies on: `extract`'s checkpoint rule, `sync build` keyed, no object
  deletion, one active queue, span-anchor behaviour from spike B), PL-1 … (planeter: no module mechanism,
  OQ-4 unruled, the reused patterns and their versions), WC-1 (corpus commit, identifier stability).
- **Required tests.** A test fails if a crate in `Cargo.lock` has no accepted row (the ledger is
  derivable from the lock: compare the set).

### T4 — The procedures (RFC 010 D-1, D-2, D-8)
- **Change scope.** `docs/UPSTREAM.md` with the re-baseline procedures for prikk, planeter and wiki-commons
  and the correspondence protocol; `docs/RELEASING.md` with the pre-tag steps (fresh advisory database on
  the exact commit, dry run, pins reviewed), the tag and publish steps, and the rollback note;
  `.git-exclude/upstream/{prikk,planeter}/{send/draft,send,receive}/` created (git-excluded; an empty
  `.keep` in each).
- **Required tests.** None executable; the architect reviews the text against RFC 010.

## QA checklist (from RFC 010's acceptance criteria)

- [ ] floor-check and pin-check run in CI and in the release gates; each fails its planted failure.
- [ ] CI runs the full gate set with a pinned toolchain and named runners; the checkpoint workflow exists.
- [ ] The release rehearsal ran green to the throwaway package (run id recorded); nothing real published.
- [ ] The dependency policy, the upstream ledger and `docs/UPSTREAM.md` exist; the lock-versus-ledger test
      passes.
- [ ] `.git-exclude/upstream/` folders exist.

## Definition of done and handback

As RFC 001's handoff; the review request (`010-upstream-and-supply-chain-review-request.md`) additionally
names the rehearsal run id and the first measured dependency rows.
