# Review request — handoff 010 (upstream and supply-chain maintenance), v1

| | |
|---|---|
| Handoff | `upstream-and-supply-chain-handoff-v1.md` (RFC 010, accepted 2026-10-27) |
| Implementer | the architect session itself, at the owner's request ("proceed on the first implementation carefully", 2026-10-27) — **the author reviews its own work**; see "Independence" below |
| Submitted | 2026-10-27 |
| Asks | Approved / Conditionally Approved / Corrections Required / Design Reconsideration Required / Owner Decision Required |

## Implementation summary

RFC 010's four tasks are in `main`. **T1** the floor check (`tools/floor-check`: one prikk `X.Y.Z` across the
driver constant, `ci.yml` and the `Dockerfile`; the wiki-commons corpus digest). **T2** the pin check
(`tools/pin-check`: actions by commit SHA, images by digest, one pinned toolchain, the MSRV job equal to
`rust-version`, the real image name guarded by the tag-push condition) and the three workflows (`ci.yml`,
`supply-chain.yml`, `release.yml` with its rehearsal mode). **T3** the ledgers: `docs/dependency-policy.md`
(rules; accepted and rejected tables, every row measured; a generated lock inventory that a test keeps equal
to `Cargo.lock`) and `docs/src/inklkunn-upstream-dependency-ledger.md` (PK-1 … PK-20, PL-1 … PL-6, WC-1 … WC-4,
the UD map). **T4** the procedures: `docs/UPSTREAM.md`, `docs/RELEASING.md`, the private
`.git-exclude/upstream/{prikk,planeter}/{send/draft,send,receive}/` folders.

## Commits

| Commit | What landed |
|---|---|
| `086ff86` | T1, T2: workspace, gates, floor-check, pin-check, the three workflows, `deny.toml`, `.cargo/audit.toml`, `Dockerfile` |
| `6cb3c92` | the shimmed-PATH CI step keeps `rustdoc`; `release.yml` step names quoted (parse failure fixed) |
| `3d82a1e` | T3, T4: dependency policy with the lock inventory and its test; the upstream ledger; `UPSTREAM.md`; `RELEASING.md`; cargo-deny wildcard fix (versions beside path dependencies) |
| `301f975` | fmt; the built book untracked |
| (this commit) | the spikes' rows in the ledger (PK-11, PK-16, PK-20); this request |

## Gates: commands and exit codes

Run on 2026-10-27 at the final commit, toolchain 1.99.1 (local) and 1.88.0 (MSRV), prikk 0.54.1 on `PATH`:

| Command | Exit |
|---|---|
| `cargo fmt --all --check` | 0 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `cargo +1.88.0 clippy --workspace --all-targets --locked -- -D warnings` | 0 |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | 0 |
| `cargo test --workspace --locked` (includes floor-check, pin-check incl. the ledger test, the gates) | 0 (97 passed) |
| `cargo deny check` | 0 (advisories, bans, licenses, sources ok; 4 `license-not-encountered` warnings for allow-list entries no crate uses yet) |
| `cargo audit` (advisory database fetched 2026-10-27) | 0 (80 crates, 0 vulnerabilities) |
| `mdbook build docs` | 0 |

CI on GitHub: the `build`, `msrv` and `supply-chain` jobs are green from commit `ebe9cd6` + `3d82a1e` on
(the earlier red runs are recorded under "Deviations").

## Release rehearsal (RFC 010 D-10)

`gh workflow run release.yml -f mode=rehearsal` dispatched 2026-10-27 00:01 UTC
([run 40423716043](https://github.com/prikk-vcs/inklkunn/actions/runs/40423716043)): **success**. Jobs: `gates`
✓; `binaries` for `x86_64-unknown-linux-gnu` (ubuntu-24.04) and `aarch64-unknown-linux-gnu` (ubuntu-24.04-arm) ✓;
`image` for amd64 and arm64 ✓ (pushed to the throwaway rehearsal package only); `release` and `crates` skipped,
as the rehearsal mode requires. The real image name was never touched (the `pin-check` guard).

## Dependency-tree deltas (RFC 010 D-6; every manifest change measured before its commit)

| Change | Lock | deny | audit |
|---|---|---|---|
| T1 workspace (serde, serde_json for the tools) | 0 → 11 | ok | 0 |
| `inklkunn-markup`: pulldown-cmark 0.13, serde, serde_json, unicode-normalization; proptest (dev) | 11 → 69 (+58) | ok | 0 |
| `inklkunn-prikk`: serde, serde_json | 69 → 69 | ok | 0 |
| `inklkunn-store`: rusqlite 0.40 (bundled) | 69 → 79 (+10) | ok | 0 |
| `inklkunn-signer`, `inklkunn-core`: none | 79 → 79 | ok | 0 |
| `tools/corpus` (workspace member only) | 79 → 80 (+1 member) | ok | 0 |

New licenses: none outside the allow-list (`Unlicense OR MIT`, `Zlib OR Apache-2.0 OR MIT` and
`(MIT OR Apache-2.0) AND Unicode-3.0` resolve through MIT/Apache-2.0). The rejected table carries planeter's
settled rejections and one of inklkunn's own (a YAML crate).

## Deviations from the handoff, and why

- **The first CI runs were red** (`086ff86` through `c9fb329`): the shimmed-PATH step removed `rustdoc` with
  everything else (doctests need it); `release.yml` had two unquoted `name:` values containing `: ` (a YAML
  parse failure that fired on *every* push, not only on tags); the ceiling test read a GitHub runner's
  `SIGSEGV` under `RLIMIT_DATA` as "damaged" rather than as the ceiling; cargo-deny's wildcard rule rejected
  path dependencies without a version. Each was fixed in its own commit, named above, and each is a lesson
  recorded (PK-9 for the signal shape).
- **The lock inventory is a generated appendix**, not a hand-written list: the handoff asked for a test that
  a lock crate has an accepted row; with 69 transitive crates a hand list would drift, so
  `tools/spikes/lock-inventory.py` derives it from `cargo tree` and the test compares sets both ways. Twelve
  crates are in the lock but never built (optional dependencies of dependencies); the inventory says so
  rather than attributing them to a feature.
- **No letters were sent.** The two first asks (PK-8, PK-11) are drafted as rows in the ledger and named in
  `UPSTREAM.md`; the drafts under `.git-exclude/upstream/prikk/send/draft/` are the owner's to convey and
  are not written yet.

## Known limitations

- The release workflow has run only in rehearsal mode (above); the real tag path (attestation of the real
  image, `cargo publish`) is exercised at the first release.
- The `supply-chain.yml` schedule has not yet fired (weekly).
- The upstream ledger's PK-8 (a failing `verify` verdict's exit code) is PARTIAL until a deliberately broken
  fixture is observed.

## Independence

This implementation was done by the architect, so the usual author–reviewer separation is missing. What
compensates: every gate is a test that fails on a planted violation (the gate tests include the planted
cases); the ledger test is set-based and cannot pass with a stale inventory; the measurements are reproducible
from committed tools and a seed; and this request names its own red runs. The owner, or a fresh reviewing
session reading only `main` and this request, is asked to review for RFC 010 conformance and for anything a
second pair of eyes would catch.

## Open questions for the reviewer

1. Should the lock inventory's never-built entries be pruned by `cargo update` discipline (they are scanned by
   deny and audit, which is conservative), or is listing them as "never built" enough?
2. Is the rehearsal's throwaway image package acceptable as a permanent fixture of the org's packages, or
   should the rehearsal delete it at the end (an extra API call with its own failure mode)?
