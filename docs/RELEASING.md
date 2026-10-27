# Releasing inklkunn

Releases are **signed bare version tags** (`X.Y.Z`) pushed by the owner; `release.yml` does the rest. The
signature is an SSH signature by a key the owner holds outside CI, verified against the public keys in
`release-signers.toml` (decided 2026-10-28). Until the owner's public key is in that file the verification
step reports *pending* and the release builds unsigned (`docs/STATUS.md`, IS-6); the first key added makes
it mandatory. Until 1.0 the
minor version moves at each milestone (roadmap §milestones) and the patch version for fixes. Nothing is
released from M0; this document exists so that the first release is a rehearsed act, not a first attempt.

## Before the tag (the release-candidate commit)

1. **The gates are green on `main`** for the exact commit: `ci.yml` (build, MSRV, supply chain) and the
   gates as tests (`cargo test --workspace --locked`), with prikk on `PATH` and shimmed off.
2. **A fresh advisory database on the exact commit**: `cargo audit` and `cargo deny check` locally after
   `cargo audit fetch`; the result goes into the release notes' supply-chain line.
3. **The pins reviewed**: `pin-check` passes (actions by SHA, images by digest, runners named, the
   toolchain and the supply-chain tools pinned, the MSRV job equal to `rust-version`, the rehearsal guard
   exact); `floor-check` passes (one prikk floor and one set of asset checksums; the corpus digest).
4. **`CHANGELOG.md`**: the `[Unreleased]` section becomes `## X.Y.Z — YYYY-MM-DD` with the notes a reader
   needs; the runtime prerequisites line names the prikk floor.
5. **Versions**: `Cargo.toml`'s workspace version is `X.Y.Z`, and so is the `version = "…"` beside every
   intra-workspace `path` dependency (cargo-deny's wildcard rule wants one; `grep -rn 'version = "' crates/*/Cargo.toml`);
   `README.md` and `docs/STATUS.md` say so.
6. **Dry run**: `release.yml` in rehearsal mode (`workflow_dispatch`, `mode: rehearsal`) on the candidate
   commit; it builds everything, attests under a rehearsal label and pushes only the throwaway image
   package; the real image name is never touched (the `pin-check` test proves the guard).

## The tag (owner)

Once, on the signing machine: `git config gpg.format ssh` and `git config user.signingkey ~/.ssh/<key>.pub`;
the same public key line goes into `release-signers.toml` (`authorized_ssh_signers`) on `main` before the tag.

```sh
git tag -s X.Y.Z <release-candidate commit> -m "inklkunn X.Y.Z"
git push origin X.Y.Z
```

The architect prepares; **only the owner tags** (RFC 010 D-10; the project rules). A lightweight or
unauthorized tag fails the gates job before anything is built.

## What the workflow does

`release.yml` on a tag: the gates job (fmt, clippy, the tests **with prikk at the floor on PATH and shimmed
off**, deny, audit, pin-check, floor-check) → the binaries (Linux x86_64 and aarch64, `--locked`, the pinned
toolchain) with a build-info file naming the commit, the toolchain and the prikk floor → the container image
per architecture (`…:X.Y.Z-amd64`, `…:X.Y.Z-arm64`, from a digest-pinned base, carrying prikk at the floor
checked against the recorded checksum, bubblewrap and prlimit) → provenance attestations for the tarballs and
for each image digest → a manifest list under `ghcr.io/prikk-vcs/inklkunn:X.Y.Z` → the GitHub release with the
`CHANGELOG.md` section and the checksums → `cargo publish` of the crates in dependency order (after the
release run is green).

## Verifying a release

```sh
sha256sum -c inklkunn-x86_64-unknown-linux-gnu.tar.gz.sha256
gh attestation verify inklkunn-x86_64-unknown-linux-gnu.tar.gz --repo prikk-vcs/inklkunn
./inklkunn --version   # names the commit and the prikk floor it was built for
gh attestation verify oci://ghcr.io/prikk-vcs/inklkunn:X.Y.Z-amd64 --repo prikk-vcs/inklkunn
```

## If a job fails

A failed job before "release" leaves nothing published: fix on `main`, delete the tag locally and remotely
(`git push --delete origin X.Y.Z`), tag again at the fixed commit. A failure **after** the release step
(crates.io) is completed by hand with `cargo publish` in dependency order; a published GitHub release is
never deleted: a follow-up patch release supersedes it.

## Rollback

There is no in-place rollback of a release. Operators pin the previous image tag; a regression that matters
gets a patch release with the fix and a `CHANGELOG.md` entry naming what regressed.

## Security releases

A fix for a reported vulnerability ships as a patch release whose `CHANGELOG.md` entry names the advisory
and the affected versions; `SECURITY.md` is updated in the same commit when the policy itself changes.
