# Security policy

## Deployment notes (read before you put it on a network)

1. **Declare your reverse proxy: `INKLKUNN_TRUSTED_PROXIES`.** The server has no TLS of its own and binds
   `127.0.0.1:8080` by default, so the usual deployment is a TLS proxy on the same host, which reaches the server as a
   *loopback* peer. Unless the proxy's address is listed in `INKLKUNN_TRUSTED_PROXIES` (comma-separated addresses,
   such as `127.0.0.1`), `X-Forwarded-For` is ignored, as it must be, and **every client counts as one address**: thirty bad passwords by anyone lock every password sign-in for fifteen minutes, and
   repeatably (the same budget holds the provider sign-in starts and the new accounts). `inklkunn serve` says so at
   every start, for **any** bind address, while no proxy is declared, and once more at run time the first time a loopback
   peer sends `X-Forwarded-For` while none is; and once more if a proxy you did declare sends a header that cannot be
   read (ports, `unknown`, none), which puts every client on the proxy's own address again. The header is read from
   the right and stops at the first address that is not a declared proxy, so what a client writes on its left is never
   believed. **List only the proxies' own addresses, never a network that clients are in**: a client inside a listed
   network would be counted as a proxy and could choose its own address.
2. **No core files.** The serving process holds the key store's derived master key for its whole life and a wiki's seed
   for the length of a signing call. It makes itself non-dumpable when it starts (`prctl(PR_SET_DUMPABLE, 0)`, which
   also stops another process of the same user from attaching with `ptrace` or reading `/proc/<pid>/mem`), and **your
   unit says `LimitCORE=0` as well** (systemd; `ulimit -c 0` or `--ulimit core=0` elsewhere), so a change to the first
   is not the only wall. Keep swap off, or encrypted, for the same reason.
3. **The secret file.** Owned by the service's user with mode `0400` or `0600`, outside the data directory and out of
   any backup that holds the key records (see below). The server **warns at start**, and does not refuse, when group or
   others can read or change it, or when its owner is neither the service's user nor root.
4. **The seed directory** is a tmpfs owned by the service's user (see below), and **an unclean stop is a reason to look
   at the host**.

## What inklkunn holds, and the risk it accepts

inklkunn signs changes **on behalf of its wikis**: each wiki has an author key and a maintainer key, and the
service holds both (roadmap decision DR-4; threat model residual risk **RR-1**). A compromise of the service
is therefore a compromise of every hosted wiki's signing keys, in the sense that the attacker could seal
changes that verify. The design bounds, detects and attributes that risk rather than denying it:

- keys live only in `inklkunn-signer`'s key store, reachable through one call that takes a recorded,
  accepted proposal (RFC 001 D-2, RFC 002 D-3, D-4). A CI gate keeps every other crate away from the store
  and the writable sandbox mounts, and a second gate fails if any public item of the signer crate mentions a
  seed, a secret or a key directory, so nothing it exports can return key material or the path of any;
  the right to make or change keys, and the right to say who a request is, are values that exist once per process
  (`KeyAuthority`, `IdentityAuthority`): the binary claims each for the code that is meant to use it, and a
  surface never holds either, so the compiler, and not only the gate, refuses a handler that builds an account
  principal or opens a key operation;
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
- **Two things write a repository without a key.** `archive export` (which writes prikk's own lock files inside
  `.prikk/` and refuses a read-only repository) and the checkpoint refresh (a `verify` that re-records prikk's
  checkpoint) run in the signer's **keyless writable mode** (`Maintenance`): the repository writable, no key store,
  no secret, no key directory mounted and no seed variable, under the same per-wiki write lock, so an export never
  runs beside an acceptance. An archive and a bundle hold every page, private ones included: they are written
  readable by the operator alone, and the operator decides where they go. A **wiki being created** is marked
  (`<id>.creating`) from before its first directory until its record is stored, and every start removes whole the
  creation of a process that was killed in between (the repository, the scratch directory and the sealed key
  record), so a kill leaves either a complete wiki or nothing;
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
locked, and the address throttle bounds how many names one client can lock. The account page asks for the password again
(to change it, to make a token) and counts those guesses in the **same bucket as the sign-in page**, per name, so
someone who holds a stolen session cannot guess there with a second budget, and can lock the name's owner out of
the sign-in page for the fifteen minutes. The owner, from a session of their own, **ends the thief's session at the
account page, which needs no password**. **Declare your reverse proxy**
(`INKLKUNN_TRUSTED_PROXIES`, the first of the deployment notes above): behind one that is not declared, every client
shares the proxy's address and thirty bad attempts by anyone lock everyone. A provider sign-in's start is counted
against the address and **given back when that sign-in succeeds**; the start answers only a person's own navigation
when the browser says what a request is (`Sec-Fetch-Mode`, `Sec-Fetch-Dest`), so another site's images, scripts and
frames cannot spend it (a link or script that sends the person's own tab there can spend one, and a start is given back only
when its sign-in succeeds); and the logins in flight are bounded per address as well as overall (an address is an IPv4 address or an
IPv6 /64, so one /48 can still push out other people's pending logins, whose harm is a sign-in to start again).
Redeeming an invitation is throttled like joining, and **given back when it succeeds** (people invited together behind
one address all get in; a refused try stays counted). A sign-in, a join or an invitation made while signed in ends
the session the request presented, **except through the provider**: the `Strict` session cookie is not sent on the
provider's callback, so the old session is not seen there and stays until it ends; end it from the account page.
Sessions and API tokens are stored as hashes; a copy of the database opens neither. An account holds at most a
hundred live sessions and a hundred live tokens; a sign-in beyond the hundredth is never refused, it ends the
account's oldest session. The session cookie is `SameSite=Strict`; the one-shot login-state cookie of an OpenID
Connect sign-in is `SameSite=Lax` (the provider's redirect must carry it back), and the callback is answered with
a small page that moves the browser on, never with a redirect, so the Strict cookie is sent on the next request. A password change
(from the account page: the current password is asked) ends **every** session, the one making the change included,
and revokes every token, and the page that answers sets a new session cookie and carries its new CSRF token, so a
leaked copy of the old cookie dies with the change; the new password, the ended sessions and the revoked tokens change
in one store step, so a failure leaves all three as they were. An administrator's reset and a disabling end every
session and revoke every token too. **Making an API token asks for the
password again** (an account with none, which signs in through a provider, needs a sign-in of the last ten minutes),
because a token outlives the session that made it. OpenID Connect accounts are
reached by the provider's issuer and subject and by nothing else (**never by e-mail address**: the provider's other
claims are not read): the operator links one (`inklkunn account link`, audited), an invitation may be redeemed with
the provider instead of a password, and a first sign-in makes an account only where `INKLKUNN_JOIN` is `provider` or
`anyone` (off by default; **a public provider makes `provider` open registration**). The provider is switched on by
`INKLKUNN_OIDC_ISSUER` and `INKLKUNN_OIDC_CLIENT_ID`; the client secret is read from the file
`INKLKUNN_OIDC_CLIENT_SECRET_FILE` names (readable by the server's user only). The server reaches the provider only
through its egress guard and a confined, memory-limited `curl`.

The **operator's secret** opens every wiki's keys: it is a file named by the operator (never stored beside the
records), 16 to 1024 bytes, and anyone who holds both it and the key records holds every wiki's keys. Keep it
out of backups of the key records, readable by the server's user only, and long and random. There is **no
second factor** on local accounts in v1 (threat model RR-4); an OpenID Connect provider that enforces one is the
interim answer.

**The serving process opens the key store** (it did not in 0.1.0; threat model RR-1, RR-11). `inklkunn serve`
reads the operator's secret from the file `INKLKUNN_SECRET_FILE` names **at start** (not once: the sweep of leftover seed
directories, the start-up recovery and the acceptor each open the signer, so up to three times, and each copy is
zeroized when its signer is dropped), and opens the key store in `INKLKUNN_KEY_DIR`; a wiki's seed is decrypted onto the tmpfs (`INKLKUNN_SEED_DIR`) for the one signing
call that needs it and removed with the call. The secret is **never asked for on a terminal**, never read from an
environment variable, and never written to a log line or a page (a test searches both for its value). What the
signer checks, and what it does not:

- **The secret file**: an absolute path; a symbolic link is followed (secret mounts are often links) and the target
  must be a regular file; 16 to 1024 bytes once the line breaks at its end are taken off (so `echo` and an editor
  give the same secret). **It does not refuse on the file's owner or mode** (a Kubernetes secret mount is 0644 by
  default), **it warns at every start** when group or others can read or change it or its owner is neither the service's
  user nor root: make it owned by the service's user with mode 0400 or 0600, outside the data directory and out of any
  backup that holds the key records. Anyone who can read it, and the key records, holds every wiki's keys.
- **The key directory**: made with mode 0700 when it is absent; when it is there, it must be a real directory (not a
  link) that group and others cannot enter (any permission bit for them refuses the start of the signer). The sealed
  records inside are mode 0600. A key directory with no key store in it is refused, never made into one, by the
  server: only `inklkunn wiki create` and `wiki keys create` make a store.
- **The seed directory**: absolute, on a tmpfs or ramfs, owned by the user the server runs as, and not writable by
  group or others; the server makes it mode 0700 itself and refuses a directory it cannot do that to.
- **bubblewrap** must work on the host: every prikk call the signer makes is confined (`--dev-unconfined` is for
  development and prints a banner).

**A server started without the means to sign still serves.** If the secret file is not there, or the key store does
not open (a wrong secret, a key directory open to others, no keys made yet, a seed directory that is not on a
tmpfs), the server prints **one line** at start that names the setting and the cause, serves every page, **still
stores proposals and comments**, and refuses **every change to the history** (an acceptance) with one plain sentence
(*this server is not set up to sign changes into the history …*). It does not
ask for the secret and does not try again on its own: fix the cause and restart. Accepting a proposal from the web
and the command line (`wiki import-folder`, `wiki recover`, `wiki export`, key rotation) take the same per-wiki
write lock, so a command beside a running server is told the wiki is busy, naming the process that holds it.

**A wiki made under prikk 0.57.0 is format 8, and an older prikk refuses it.** The floor stays prikk 0.54.0, but prikk
0.57.0 makes new repositories in format 8 (`.prikk/FORMAT` holds `8`; 0.55.0 and 0.56.0 wrote 7), never upgrades an
existing one by itself, and prikk 0.55.0 and 0.56.0 refuse a format-8 repository on every command
(`error: unsupported format version: 8`). So an operator who creates wikis under 0.57.0 and later puts an older prikk
back loses those wikis until prikk 0.57.0 (or a later one) returns: pages are served only from what is cached, nothing is accepted, and the
log of each call says `prikk … cannot open this repository: it is format 8 …`. **It is not damage and not a case for
recovery**: the wiki is not flagged, its verification record keeps its last verdict, readers are shown *could not be read
now* (never the version), and the repository is exactly as it was. Put the prikk that made the wiki back, or restore the
wiki from a backup made before. The format prikk wrote is kept in the wiki's store record when the wiki is created
(`prikk_format_version`, for information: nothing reads it to decide anything, prikk is always the one asked).

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
