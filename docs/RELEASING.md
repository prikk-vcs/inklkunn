# Releasing inklkunn

Releases are **bare version tags** (`X.Y.Z`); `release.yml` does the rest, with no manual step. The owner
authorizes a release and the architect tags it (the project rules). Every commit and tag is signed by the owner's
git configuration (OpenPGP), so a reader can check a tag by hand (below); CI does not re-check it, and the built
artifacts carry GitHub's keyless provenance attestations (owner, 2026-10-29). Until 1.0 the
minor version moves at each milestone (roadmap §milestones) and the patch version for fixes. Nothing is
released from M0; this document exists so that the first release is a rehearsed act, not a first attempt.

## Before the tag (the release-candidate commit)

1. **The gates are green on `main`** for the exact commit: `ci.yml` (build, MSRV, supply chain) and the
   gates as tests (`cargo test --workspace --locked`), run twice: with prikk on `PATH` (the floor first, the
   test-only prikk second; see below) and with prikk shimmed off. The first run fails if a test that needs the
   test-only prikk printed its skip line (`skipping: prikk 0.57.0 or newer not found`, or `skipping: no prikk
   older than 0.57.0 (and at the floor) found`); the second fails unless exactly `EXPECTED_SKIPS` tests printed
   `skipping: prikk not on PATH`. A prikk-dependent test added or removed moves that number in `ci.yml` and
   `release.yml` together.
2. **A fresh advisory database on the exact commit**: `cargo audit` and `cargo deny check` locally after
   `cargo audit fetch`; the result goes into the release notes' supply-chain line.
3. **The pins reviewed**: `pin-check` passes (actions by SHA, images by digest, runners named, the
   toolchain and the supply-chain tools pinned, every install of the pinned compiler naming the components
   `rust-toolchain.toml` names, `rust-toolchain.toml` equal to the compiler the workflows install, the MSRV job
   equal to `rust-version` and naming it on every command, every job with a `timeout-minutes`, the rehearsal
   guard exact); `floor-check` passes (one prikk floor and one set of asset checksums; the test-only prikk
   recorded once, with its own checksum, installed first so that it comes after the floor on `PATH`, and in no
   floor or image; the corpus digest).
4. **`CHANGELOG.md`**: the release's entries move under `## [X.Y.Z] — YYYY-MM-DD`, after a lead a reader needs and the line
   `The entries below are in the order the work landed.`; an empty `## [Unreleased]` stays above it. The GitHub release's
   body is that lead and a link to the whole section at the tag (`.github/release-notes.sh`; a release body holds at most
   125,000 characters, and 0.2.0's section alone is longer); the gates job checks it exists and fits before anything is
   built or published (the brackets in the heading matter).
5. **Versions**: `Cargo.toml`'s workspace version is `X.Y.Z`, and so is the `version = "…"` beside every
   intra-workspace `path` dependency (cargo-deny's wildcard rule wants one; `grep -rn 'version = "' crates/*/Cargo.toml`);
   `README.md` and `docs/STATUS.md` say so.
6. **Dry run**: `release.yml` in rehearsal mode (`workflow_dispatch`, `mode: rehearsal`) on the candidate
   commit; it builds everything, attests under a rehearsal label and pushes only the throwaway image
   package; the real image name is never touched (the `pin-check` test proves the guard).

## The tag (the architect, once the owner has authorized the release)

When `main` has moved past the candidate, the release is cut from a branch `release/X.Y` at the candidate; the version
and CHANGELOG commit lands there, the rehearsal runs there, and the branch is merged back into `main` afterwards (the
release's entries under its heading, everything newer under `[Unreleased]`).

```sh
git tag -s X.Y.Z <release commit> -m "inklkunn X.Y.Z"   # signed by the owner's git configuration
git push origin X.Y.Z
```

A tag whose version differs from the workspace version fails the gates job before anything is built. **The rehearsal
does not run the attestation steps** (they need a tag push), so the first tag run is their first run: watch it. If a
tag run fails before anything is published (no GitHub release, no image under the real name, no crate), the tag may
be moved to the fixed commit, with the owner's agreement, and the move recorded in `docs/STATUS.md`. Once anything is
published, a fix is a new patch release, never a moved tag.

**crates.io is optional per release:** with no `CARGO_REGISTRY_TOKEN` secret the crates step skips with a notice. When
it is set, nine crates are published (the eight libraries and the `inklkunn` binary, in dependency order); the tools
are never published. A published crate cannot be deleted, only yanked.

**The binaries are built on Ubuntu 22.04** (glibc 2.35) so they run on the image's Debian 12 base and on any Linux with
glibc 2.35 or newer; a step refuses a binary that needs a newer glibc. The container package on `ghcr.io` must be
**public** for readers to pull it (a new organization package is private; the owner sets it once).

## The two prikks and the one compiler

**The floor and the test-only prikk.** The floor (`PRIKK_VERSION`, today 0.54.0) is the oldest prikk inklkunn
runs against: it is the driver's `MIN_PRIKK_VERSION`, the prikk in the image, and the first prikk on `PATH` in the
test runs, so every test that takes "the prikk" takes it. CI and the release gates also install **one test-only
prikk** (`PRIKK_TEST_VERSION`, today 0.57.0; RFC 010 D-3, amended by maintenance M25): a newer release, with its own
checksum recorded in the workflow (`PRIKK_TEST_SHA256_X86_64`, the checksum of the release asset inklkunn measured,
not one fetched beside it), whose only use is to run the tests of what the floor cannot (the take-back, the
stale-lock preview, the format-marker classifier). It is never a floor, never in the driver's constant or the image,
and it is the second prikk on `PATH`. Each such test finds it itself, runs its subject on it, and prints
`skipping: prikk 0.57.0 or newer not found` (or `skipping: no prikk older than 0.57.0 (and at the floor) found`
for the test of the floor's *no verb* answer) when it is not there; the with-prikk run fails if either line
appears, so in CI no such test passes by skipping. Moving the test-only prikk is a one-value change in `ci.yml`
and `release.yml` (version and checksum), checked by `floor-check`; moving the floor is a decision (RFC 010 D-3).

**The compiler.** `rust-toolchain.toml` pins the compiler (`channel = "X.Y.Z"`, today 1.99.1, with `rustfmt` and
`clippy`), so a plain `cargo` in a checkout is the compiler CI and the release build with. `pin-check` holds the
file equal to the compiler `ci.yml`, `supply-chain.yml` and `release.yml` install and to `RUST_TOOLCHAIN`, and
holds every install of it to `--component rustfmt --component clippy` (without them the first plain `cargo` of a job
would download them, a step the job did not plan for). Moving the compiler is one change to the file and to every
install, in one commit. The declared minimum (`rust-version` in `Cargo.toml`, the `msrv` job) is the one exception
and is named explicitly on every command (`cargo +1.89.0 check ...`), because a toolchain file beats
`rustup default`.

## What the workflow does

`release.yml` on a tag: the gates job (fmt, clippy, the tests **with prikk at the floor on PATH (and the test-only
prikk beside it) and shimmed off**, deny, audit, pin-check, floor-check) → the binaries (Linux x86_64 and aarch64, `--locked`, the pinned
toolchain) with a build-info file naming the commit, the toolchain and the prikk floor → the container image
per architecture (`…:X.Y.Z-amd64`, `…:X.Y.Z-arm64`, from a digest-pinned base, carrying prikk at the floor
checked against the recorded checksum, bubblewrap and prlimit) → provenance attestations for the tarballs and
for each image digest → a manifest list under `ghcr.io/prikk-vcs/inklkunn:X.Y.Z` → the GitHub release with the
`CHANGELOG.md` section and the checksums → `cargo publish` of the crates in dependency order (after the
release run is green).

## Verifying a release

```sh
curl -fsSL https://github.com/nabbisen.gpg | gpg --import   # the owner's public key, once
git verify-tag X.Y.Z                                          # in a clone: the tag is the owner's
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
