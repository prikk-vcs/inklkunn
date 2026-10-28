# Security policy

## What inklkunn holds, and the risk it accepts

inklkunn signs changes **on behalf of its wikis**: each wiki has an author key and a maintainer key, and the
service holds both (roadmap decision DR-4; threat model residual risk **RR-1**). A compromise of the service
is therefore a compromise of every hosted wiki's signing keys, in the sense that the attacker could seal
changes that verify. The design bounds, detects and attributes that risk rather than denying it:

- keys live only in `inklkunn-signer`'s key store, reachable through one call that takes a recorded,
  accepted proposal (RFC 001 D-2, RFC 002 D-3, D-4). A CI gate keeps every other crate away from the store
  and the writable sandbox mounts, and a second gate fails if any public item of the signer crate mentions a
  seed, a secret or a key directory, so nothing it exports can return key material or the path of any;
- the seeds are sealed at rest (Argon2id from the operator's secret, XChaCha20-Poly1305 per record, with the
  wiki, key ids and times bound in, so no record can be edited or moved to another wiki), and are decrypted
  only into a fresh directory on a tmpfs for the one call that signs, overwritten and removed afterwards.
  **A server killed in the middle of a signing call (`SIGKILL`) cannot clean up after itself**: its
  decrypted seeds stay on the tmpfs until the next start, which removes any such directory older than five
  minutes. Run the seed directory on a tmpfs that is not swapped to disk, and treat an unclean stop as a
  reason to look at the host (the next start reports how many directories it removed);
- every sealed change carries the proposer and the acceptor in its signed message, and a reconciliation
  compares the history with the acceptance records so an unrecorded seal is reported (RFC 002 D-5, D-7);
- keys rotate per wiki, and the surface tells readers only what was checked (*Verified*, *Verification
  failed*, *Could not be checked now*), never that a person signed cryptographically (RFC 002 D-6, D-8).

The **operator's secret** opens every wiki's keys: it is a file named by the operator (never stored beside the
records), 16 to 1024 bytes, and anyone who holds both it and the key records holds every wiki's keys. Keep it
out of backups of the key records, readable by the server's user only, and long and random. There is **no
second factor** on local accounts in v1 (threat model RR-4); an OpenID Connect provider that enforces one is the
interim answer.

The **bootstrap signer** over a plain-text key directory exists only behind the `bootstrap` cargo feature, for
tests; the gate fails if a product crate enables it.

## Reporting a vulnerability

Please report privately through GitHub's security advisories for this repository
(**Security → Report a vulnerability**), or to the maintainer named in `Cargo.toml`. Do not open a public
issue. You will get an acknowledgement within seven days and a decision on severity and timeline after
triage; fixes ship as a release with a `CHANGELOG.md` entry naming the advisory.

## Supported versions

Until 1.0, only the latest release is supported. Security fixes are not backported.

## Dependencies and upstream

Dependency advisories are triaged on a fixed ladder (upgrade; remove the path; a scoped, reasoned ignore;
never silence) in CI on every push, at every release, and at every RFC disposition checkpoint on a freshly
fetched advisory database (`docs/dependency-policy.md`). inklkunn's storage is
prikk; a prikk security release moves inklkunn's floor by the re-baseline procedure in `docs/UPSTREAM.md`.
