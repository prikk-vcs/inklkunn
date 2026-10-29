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
  **A call sees only the seed it signs with**: `commit` the author seed, `seal` and `branch create` the
  maintainer seed, and `init`, `trust`, `mv`, `branch switch`, `verify` and `log` none, so a fault in the parser
  that reads a proposal's paths and content cannot reach the maintainer seed. **Every call runs under a memory
  ceiling sized from the repository and the change** (measured per verb on prikk 0.55.0, in
  `crates/inklkunn-signer/src/cost.rs`); a call that runs out of memory is reported as that, *not refused and not
  a failed verification*, together with where the repository is (nothing committed; one patch queued; sealed,
  not confirmed).
  **A server killed in the middle of a signing call (`SIGKILL`) cannot clean up after itself**: its
  decrypted seeds stay on the tmpfs until the next start, which removes them at once: a seed directory is held
  under a lock for the life of its call and the kernel drops the lock with a killed process, so a start removes
  every directory it can lock and none a live call holds, however long the call runs. Run the seed directory on a tmpfs that is not swapped to disk, owned by the
  service's user and writable by nobody else (the signer refuses anything else), and treat an unclean stop as a
  reason to look at the host (the next start reports how many directories it removed);
- every sealed change carries the proposer and the acceptor in its signed message, and a reconciliation
  (a function today; no command or schedule runs it yet) compares the history with the acceptance records so
  an unrecorded seal is reported (RFC 002 D-5, D-7). One wiki has one writer at a time: every act that
  writes its repository or its key record takes an exclusive `flock` on a lock file in the data directory (never
  inside `.prikk/`), so the command line beside a running server and a second server process are refused at once,
  naming the process that holds it, and a killed holder leaves nothing to clear (RFC 004's lease is built on it);
- keys rotate per wiki (`inklkunn wiki keys rotate`; the runbook is `docs/KEYS.md`, including the cost of
  retiring a key under prikk 0.54.1), and the surface tells readers only what was checked (*Verified*, *Verification
  failed*, *Could not be checked now*), never that a person signed cryptographically (RFC 002 D-6, D-8).

**Signing in** is built to give nothing away: past the throttles every password attempt costs one Argon2id
verification whether the name exists or not (a stored hash nobody can verify included), and one answer covers
every way to fail. Attempts are counted **before** they are judged, so a burst of simultaneous guesses is held to
the limit as a sequence is. Failures are counted per sign-in name and per client address (the address from a
declared trusted proxy only) **in the memory of one process** (threat model RR-7): a restart forgets them, and a
deployment with several replicas needs the proxy to limit too. Locking a name after five failures lets someone
who knows it lock its owner out for fifteen minutes; that is the price of refusing a correct password while
locked, and the address throttle bounds how many names one client can lock. **Declare your reverse proxy**: behind
one that is not declared, every client shares the proxy's address and thirty bad attempts by anyone lock everyone.
Sessions and API tokens are stored as hashes; a copy of the database opens neither. A password change, an
administrator's reset and a disabling end every session and revoke every token. OpenID Connect accounts are
linked by issuer and subject by an administrator; nothing is created at first sign-in in this release.

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
