# Review request — handoff 010 (upstream and supply-chain maintenance), v2

| | |
|---|---|
| Supersedes | `010-upstream-and-supply-chain-review-request.md` (v1, 2026-10-27), kept as the record |
| Handoff | `upstream-and-supply-chain-handoff-v1.md` (RFC 010, accepted 2026-10-27) |
| Implementer | the architect session, at the owner's request; reviewed as described in the 001 request v2 |
| Submitted | 2026-10-27 |
| Asks | Approved / Conditionally Approved / Corrections Required / Design Reconsideration Required / Owner Decision Required |

## What the review found and what changed

### The gates (`tools/boundary-check`, `tools/pin-check`, `tools/floor-check`)

- **Bypasses in spelling.** The layering and key-reachability rules matched a dependency only as a line
  starting with the crate name; `package = "…"` renames, quoted keys, `[dependencies.name]` tables,
  `[target.'cfg'.dependencies]` and multi-line inline tables passed. Manifests are now read by a small TOML
  reader that knows every dependency table spelling cargo accepts, and a planted test covers each.
- **An unknown crate was invisible.** Only the three hard-coded lists were read; a new `crates/inklkunn-api`
  would have been in no layer and never checked. Every member under `crates/` must now be in exactly one
  layer (a test), and an unlayered crate fails the gate.
- **The spawn rule saw only `Command::new(` beside the word `prikk`** and skipped the driver's own idiom
  (`Command::new(&binary)`). It now forbids `std::process::Command` and `tokio::process` in any spelling
  (including inside a `use` group) outside the driver and the signer, scans tests and nested directories
  (only the workspace's own `target/` is skipped), and a planted *file* exercises discovery.
- **Comments and strings fooled the matchers** (`// serde(` hid a literal; a doc comment mentioning
  `key_store` failed the gate). Rules about code now strip comments and string literals first and respect
  word boundaries; the attribute carve-out is attribute-aware instead of substring-based.
- **Markup purity** missed `use std::{fs, env}` groups and `[target.…]` tables; `use` groups are expanded,
  `std::time`, `std::io::Read/Write`, `std::thread` joined the list, and dependency tables are read in every
  spelling.
- **New rules:** every member crate carries `[lints] workspace = true` and opens with
  `#![forbid(unsafe_code)]` (the policy's claim, now enforced); no product crate enables a test-only
  feature (`test-knobs`, `bootstrap`); the writable sandbox mounts and the bootstrap signer are named by no
  product source outside the signer and the sandbox crate.
- **pin-check** split steps only at `- name:`/`- uses:`/`- run:`, so a `- id:` step inherited its
  neighbour's guard; a matrix `- os:` was read as a step; a guard was any substring, so an `echo` or an
  `|| inputs.mode == 'force'` satisfied it; job-level `if:` was dropped. Steps are now the `- ` items under a
  job's `steps:` key, the job's `if:` applies to them, and the guard must be exactly
  `github.event_name == 'push'` (a widened or quoted-away expression is no guard). Also: `*.yaml`
  workflows are read, a toolchain must be `X.Y.Z`, `-latest` is caught inside a matrix, `cargo install`
  must pin `name@X.Y.Z` and the pins must agree across workflows, `FROM` is case-insensitive and
  `COPY --from=` is checked while a declared stage name is allowed.
- **floor-check** returned silently when the corpus directory was missing (a deleted corpus passed); it now
  fails. It reads every `PRIKK_VERSION:` line, not the first, and **the release-asset checksums are
  recorded** (`PRIKK_SHA256_X86_64`, `PRIKK_SHA256_AARCH64`) and must agree across `ci.yml`, `release.yml`
  and the `Dockerfile`: the asset is checked against what inklkunn measured, not against a file fetched
  beside it.
- **The ledger test** could be satisfied by regenerating the appendix; a second test now fails when a
  direct dependency named in the appendix has no row in the accepted table.

### The workflows and the image

- The release `gates` job never installed prikk, so every prikk-dependent test self-skipped there; it now
  installs prikk at the floor with bubblewrap and prlimit and runs **both** test passes, like CI.
- **The shimmed pass only echoed when no skip line was found.** Both workflows now fail unless exactly
  `EXPECTED_SKIPS` (13) self-skip lines appear; a new prikk-dependent test updates the number.
- `cargo install cargo-deny cargo-audit` installed whatever was latest; pinned to `0.20.2` and `0.22.2` in
  all three workflows (pin-check requires it and requires agreement).
- Only `:<tag>-amd64` and `:<tag>-arm64` images were pushed while the notes advertised `:<tag>`; a
  `manifest` job now joins them under the release tag (and the rehearsal's throwaway tag).
- The image was not attested although the docs said it was; each pushed image's digest is attested now.
- `${{ github.actor }}` was interpolated into a `run:` line; it goes through `env:`.
- Concurrency groups on CI (cancel in progress) and release (never cancel).
- The build-info file names the toolchain; `inklkunn --version` names the commit and the floor.
- The `Dockerfile` fetches prikk in a separate stage against the recorded checksum, so the runtime image
  carries no download tool; a `.dockerignore` limits the context to the staged binaries.
- `deny.toml` sets `[graph] all-features = true`, so crates that are locked but never built are in what
  deny scans, as the policy claims.

### The documents

`UPSTREAM.md` (four floors plus the checksums; the counted shimmed run; no weekly schedule exists, RFC 010
chose checkpoints), `SECURITY.md` (the gates run at pushes, releases and checkpoints; what the key gate
guards in M0), `RELEASING.md` (both test passes in the gates job, per-architecture images and the manifest
list, image attestation, the verify commands; **tag signing withdrawn until the owner names a key**),
`dependency-policy.md` (rules 5–7: the lints and `forbid(unsafe_code)` enforced, `all-features`, no test
feature in a product crate; the direct-dependency test).

## Answers to v1's open questions

1. **Never-built lock entries:** listed as such and scanned (`all-features` makes deny see them; audit scans
   the lock). No `cargo update` discipline beyond `--locked` is needed.
2. **The rehearsal's throwaway package:** kept. Deleting it would add an API call with its own failure mode
   to a rehearsal whose point is to fail only on real defects; the package is visibly named `-rehearsal`.

## Gates: commands and exit codes

As in the 001 request v2: fmt 0; clippy 0 on 1.99.1 and 1.88.0; rustdoc `-D warnings` 0; tests 0 (138
passed) with prikk and 0 with 13 self-skips shimmed; deny 0; audit 0 (80 crates, 0 vulnerabilities);
mdbook 0. The pin, floor and ledger tests pass on the real workflows and each catches its planted failures.

## Release rehearsal

v1's rehearsal (run 40423716043) ran the old workflow. The reworked `release.yml` (prikk in the gates job,
the counted shimmed pass, the manifest job, image attestation) is dispatched in rehearsal mode after this
request lands on `main`; its run id is appended to `docs/STATUS.md`.

## Dependency-tree deltas

None (no new crate).

## Known limitations

- The real tag path (image attestation of the real package, `cargo publish`) is exercised at the first
  release.
- Tag signature verification is pending the owner's key (open question 1 of the 001 request v2).
- The apt packages in the image are not pinned to a snapshot; the base image is digest-pinned and the only
  packages are bubblewrap and ca-certificates.

## Open questions for the reviewer

None beyond the owner's two in the 001 request v2.
