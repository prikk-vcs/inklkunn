# inklkunn

[![CI](https://github.com/prikk-vcs/inklkunn/actions/workflows/ci.yml/badge.svg)](https://github.com/prikk-vcs/inklkunn/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](Cargo.toml)

**A wiki whose every page has a signed, verifiable history — built for inclusion and shared knowledge.**

*inklkunn* (Norwegian *inkludering* + *kunnskap*: inclusion, knowledge) is a wiki engine whose storage is
the [prikk](https://github.com/prikk-vcs/prikk) patch-theoretic version control system. Changes are
proposed, reviewed and accepted; what is accepted is sealed into a history anyone can verify; what the
reader is told about that history is never more than what was checked.

## Overview

inklkunn is a Rust service with a web interface. It reads and writes
[Portable Wiki Markdown](https://github.com/kos-commons/wiki-commons) (wiki-commons' profile: free links,
embeds, block identifiers, tags, callouts, directives), keeps each wiki as a Portable Wiki Bundle in a prikk
repository, and signs on the wiki's behalf with per-wiki keys it holds. It is designed to run alone and,
later, as a module of the [planeter](https://github.com/prikk-vcs/planeter) forge.

**Status: M0 (foundations) is under construction; nothing serves pages yet.** See [`docs/STATUS.md`](docs/STATUS.md)
for the read-first brief and [`ROADMAP.md`](ROADMAP.md) for where it goes.

## Why

- **Honesty about trust.** A wiki should be able to say *this page's history is signed and verifies*, and
  should say *could not be checked now* when it cannot. inklkunn's vocabulary has exactly those words.
- **Inclusion by design.** Proposals, review and acceptance are the normal path for everyone, with
  attribution to proposer and acceptor on every change; the default view shows little, and every
  pattern comes from a published guideline ([wiki-commons](https://github.com/kos-commons/wiki-commons)).
- **Portable knowledge.** Pages are plain Markdown files in a documented bundle. Export is the repository.

## Quick start

There is nothing to run yet. To build and test what exists:

```sh
# prerequisites: Rust 1.88+, prikk ≥ 0.54.0 on PATH, bubblewrap (bwrap), prlimit (util-linux)
cargo build --workspace --locked
cargo test --workspace --locked          # prikk-dependent tests self-skip with a visible line without prikk
```

## Design notes

- **One storage boundary.** `inklkunn-prikk` drives the prikk CLI's `--format json` read verbs through
  bubblewrap, pinned to a floor version and to each report's `schema_version`; the repository is bound
  read-only. Only `inklkunn-signer` runs a write verb or touches a key; a CI gate enforces both.
- **Pure markup.** `inklkunn-markup` does no I/O; wiki-commons' conformance corpus is its acceptance test.
- **Default deny.** One `authorize()` is consulted on every path; in M0 it denies everything.
- **Measured dependencies.** Every crate enters with its measured cost, license and advisory state
  ([`docs/dependency-policy.md`](docs/dependency-policy.md)); upstream behaviours we rely on are a ledger.

## More detail

- [Status brief](docs/STATUS.md) · [Roadmap](ROADMAP.md) · [RFCs](rfcs/README.md)
- [Project charter](docs/src/inklkunn-00-project-charter.md) · [Requirements](docs/src/inklkunn-01-requirements-spec.md) ·
  [External design](docs/src/inklkunn-02-external-design.md) · [Threat model](docs/src/inklkunn-03-threat-model.md) ·
  [Internal design](docs/src/inklkunn-04-internal-design.md)
- [Upstream and re-baseline](docs/UPSTREAM.md) · [Releasing](docs/RELEASING.md) · [Security policy](SECURITY.md)

## License

Apache-2.0. Author: nabbisen.
