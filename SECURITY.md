# Security policy

## What inklkunn holds, and the risk it accepts

inklkunn signs changes **on behalf of its wikis**: each wiki has an author key and a maintainer key, and the
service holds both (roadmap decision DR-4; threat model residual risk **RR-1**). A compromise of the service
is therefore a compromise of every hosted wiki's signing keys, in the sense that the attacker could seal
changes that verify. The design bounds, detects and attributes that risk rather than denying it:

- keys live only in `inklkunn-signer`'s key store, reachable through one call that takes a recorded,
  accepted proposal; a CI gate keeps every other crate away from them (RFC 001 D-2, RFC 002 D-3, D-4);
- every sealed change carries the proposer and the acceptor in its signed message, and a reconciliation
  compares the history with the acceptance records so an unrecorded seal is reported (RFC 002 D-5, D-7);
- keys rotate per wiki, and the surface tells readers only what was checked (*Verified*, *Verification
  failed*, *Could not be checked now*), never that a person signed cryptographically (RFC 002 D-6, D-8).

In M0 the signer is a **bootstrap signer over a plain-text key directory** meant for tests and a developer's
first wiki; it must not be exposed to a request path. RFC 002's handoff replaces it before any hosting.

## Reporting a vulnerability

Please report privately through GitHub's security advisories for this repository
(**Security → Report a vulnerability**), or to the maintainer named in `Cargo.toml`. Do not open a public
issue. You will get an acknowledgement within seven days and a decision on severity and timeline after
triage; fixes ship as a release with a `CHANGELOG.md` entry naming the advisory.

## Supported versions

Until 1.0, only the latest release is supported. Security fixes are not backported.

## Dependencies and upstream

Dependency advisories are triaged on a fixed ladder (upgrade; remove the path; a scoped, reasoned ignore;
never silence) in CI on every push and on a schedule (`docs/dependency-policy.md`). inklkunn's storage is
prikk; a prikk security release moves inklkunn's floor by the re-baseline procedure in `docs/UPSTREAM.md`.
