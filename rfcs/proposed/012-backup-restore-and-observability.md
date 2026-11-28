# RFC 012 — Backup, restore and observability: a server that can be kept, and says how it is

**Status.** **Proposed to the owner 2026-11-28; refined the same day by the architect's own check against the owner's philosophy, before the rulings** (four refinements, marked *refined*); **checked again at the owner's request the same day, before the rulings** (nine more, marked *refined, second check*). This is 0.6.0's R-1, with RFC 011 (`docs/releases/0.6.0-plan.md`,
authorized 2026-11-28 with P-1 – P-5 and six refinements). **Tracks.** ROADMAP T11; requirements DUR-1, DUR-4, OPS-3,
OPS-5, OPS-6; threat model RR-8, IS-15 (deferred, P-3); prikk's own backup and restore guide
(`docs/src/guide/backup-restore.md`). **Touches.**
- `inklkunn` (the binary): `backup`, `restore`, `status`, the logs, the deployment guide;
- `inklkunn-core`: the leases held together, the counters;
- `inklkunn-store`: a consistent copy of the store;
- `inklkunn-web`: `/-/health`.

**Not here:**
- the permanent removal procedure (0.7.0, P-1);
- re-keying a lost key store (IS-15, P-3);
- encrypting inside inklkunn (P-2);
- a metrics endpoint;
- RFC 011's attachments.

## Summary

Today a server's authoritative state is three things kept apart:
- the wikis' repositories;
- the service store;
- the key store.

No command saves them, and no command brings them back. Health answers a fixed `ok`, the logs are plain lines, and
nothing counts what the server does.

This RFC adds:
- **`inklkunn backup`:** one archive of all three, taken at one moment, written to a file or to standard output, so that
  it can go straight into an encryptor;
- **`inklkunn restore`:** into an empty data directory, refusing anything that does not verify;
- **a rehearsal of both, as a test;**
- **a health answer that checks** the store, prikk and the keys;
- **`inklkunn status`:** every wiki's state;
- **structured logs** that carry no content;
- **counters;**
- **the deployment guide.**

**Nothing here migrates the store** (plan refinement 3), and **nothing writes the secret file anywhere.**

## Decisions

- **D-1 — What a backup is: an exact copy, taken under the write leases.**
  - **Each repository is copied as its directory, byte for byte, while its wiki's write lease is held.**
    - inklkunn's reads run prikk on a read-only bind, so nothing changes a repository while its lease is held, and the
      copy is exact. It holds every view, the trust record, the log and the queue.
    - **Why not `prikk archive export`?** It refuses any repository with a file over 256 MiB, and its import lands every
      ref but the exporter's under `remotes/`, so a wiki's views would not come back as views. The copy has neither
      limit. **`prikk verify` is run on the copy before the backup is declared good** (Q-1).
  - **A damaged or flagged wiki is backed up, not a reason to refuse the backup** (*refined, second check*): that is when a backup matters
    most. The rule is that **the copy verifies exactly as the original does**, the same findings. The manifest and the
    summary name each flagged wiki and its findings, so a restore never presents one as sound.
  - **The store** is copied with SQLite's `VACUUM INTO`: one consistent snapshot, with no new dependency and no
    migration. A store of an older schema is copied as it is.
  - **The key store** is copied as its sealed records. **The secret file is never in a backup.** The command refuses a
    secret file inside the destination, and says where the secret must be kept instead.
  - **One moment** (Q-2):
    1. every wiki's lease is taken, in a fixed order;
    2. the store and the key store are copied;
    3. then each repository is copied, and its lease released.

    **Taking the leases is bounded** (*refined, second check*): when one is not free within 30 seconds, the backup releases every lease
    it took and refuses, naming the wiki and what holds it. It never waits for ever, and never holds the other wikis'
    writes while it waits for one.

    So no write happens anywhere between the store's snapshot and any repository's copy, and each wiki's writes wait
    only until that wiki is copied. **Reads never stop.**
  - **A write that meets a backup's lease says so** (*refined*): *this wiki is being backed up; try again in a moment*,
    never a general *busy*.
  - **The server's own `status.json` is not in a backup** (*refined*).
  - **Sign-in sessions are not in a backup** (*refined, second check*): they are removed from the store's snapshot before it is archived.
  - **The store's snapshot is staged inside the data directory** (*refined, second check*), owner-only, beside the store it copies, and
    removed when the archive is written, on any failure, and at the next start if a backup was killed. So no copy
    leaves the data directory unencrypted, and the snapshot is never on a disk the store was not already on.
- **D-2 — One archive, to a file or to standard output.**
  - `inklkunn backup --to <file>` writes one archive (tar), owner-readable only (`0600`), and refuses an existing file.
    `inklkunn backup --to -` writes it to standard output, so `inklkunn backup --to - | age -r … > backup.age` puts no
    unencrypted copy outside the data directory (P-2; *refined, second check*: the old claim, *on any disk*, missed the store's staged
    snapshot). With `-`, it refuses when standard output is a terminal.
  - **A manifest inside it** lists every file with its SHA-256, the store's schema version, each repository's prikk
    format and tip, and inklkunn's and prikk's versions.
  - **The command says, in one line on standard error** (*refined, second check*: words true whether it writes a file or a pipe):
    *inklkunn does not encrypt backups: this one holds people's names, password hashes and API token hashes, and what
    moderation hides; encrypt it before it leaves this machine.*
  - One new dependency for the archive format (a pure-Rust tar crate), measured as the dependency policy says.
- **D-3 — Restore, into an empty data directory only.**
  - `inklkunn restore --from <file or -> --data <empty dir>`.
    - It reads the archive as **untrusted input**: no path escapes, no links, every file in the manifest present and
      matching its SHA-256, and no file the manifest does not name.
    - It writes the set.
    - **It runs `prikk verify` on every repository.**
  - **Anything that does not verify refuses the restore and leaves the directory as it found it** (it restores into a
    temporary directory beside the destination, and renames it into place only at the end).
  - **The secret is the operator's to supply,** as at start. A restore never asks for it, and the first `serve` opens the
    keys with it as always. **The restore says so in one line** (*refined*): *the wikis' keys open only with the secret
    file this server had; without it they can be read and never written*.
  - **A newer store than this binary knows is refused,** naming both versions. An older one is opened, and migrated by
    `serve` as an upgrade is.
  - **A restore never brings back an access that was taken away** (*refined, second check*). The store goes back to the backup's moment,
    so an API token revoked or an account disabled since would live again. So **the restore revokes every API token**
    (marked *revoked by restore*), there are no sessions to restore, and it says, in its summary: the backup's time;
    *everyone signs in again; API tokens must be issued again; an account disabled or a password changed since that
    time is as it was then: check them.*
- **D-4 — The rehearsal is a test.** A wiki with pages, views, a hide, an import's history, accounts and keys is backed
  up while the server runs. It is then restored into another directory, served, and shown:
  - the same pages and history;
  - the views as views;
  - the hide still hiding;
  - the accounts signing in;
  - **a proposal accepted and sealed under the restored keys;**
  - `prikk verify` passing.

  **A tampered archive is refused:** a byte changed, a file removed, a file added, a path that escapes.
- **D-5 — Health, and what it may say to anyone** (Q-3).
  - `/-/health` checks three things:
    - the store answers;
    - prikk runs at a version at or above the floor;
    - **the key store holds the records of every wiki.**
  - **To anyone it answers `ok` or `degraded`, nothing more:** no wiki, no path, no count. The status code is `200` or
    `503`.
  - **Its checks are made at most once every ten seconds** (*refined, second check*) and the answer kept between them, so anyone polling
    `/-/health` can never make the server start a process.
- **D-6 — `inklkunn status`, the operator's view** (OPS-6).
  - It lists every wiki:
    - its prikk format;
    - its verification state;
    - whether it is flagged, and why, in RECOVERY.md's words;
    - its writes-index queue;
    - **whether its key records are present.**
  - **A wiki whose key store is missing says exactly that** (plan refinement 4): *the key records of wiki X are missing:
    restore the key directory from a backup (KEYS.md); re-keying is not available yet*. The same sentence is the wiki's
    refusal of a write.
  - The counters (D-8) come from the running server's status file, **with its age** (*refined*). When the file is older
    than a minute, `status` says that the server may not be running, and never shows stale counters as current.
- **D-7 — Structured logs that carry no content.**
  - Every line the server writes becomes one structured line: `level`, `event`, the wiki's id where there is one, the
    request's id. It is logfmt-shaped, hand-written, with no new dependency.
  - **No page text, no comment, no name, no email or IP address, no request path or query, no secret** (*refined, second check*: *no
    address* was ambiguous, and a path holds a page's name and a query a signed address). A request is logged by its
    route's name (`page.view`), its status and its time. The guide says that a reverse proxy's access log, which holds
    addresses and paths, is the operator's own choice.
  - A test runs the end-to-end tests with a log capture, and fails on any planted marker text it finds, in a page, a
    name, an email address, a path, and a signed address.
- **D-8 — Counters.**
  - Admissions, refusals (by kind), acceptances, the background builder's queue, and prikk calls, kept in the process.
  - The server writes them every ten seconds to `<data>/status.json`, atomically and owner-only, and `status` reads it.
  - **No endpoint** (a metrics endpoint is later, with its own access rule).
- **D-9 — The deployment guide** (OPS-5), the book's operator part.
  - **Its first lines say who it is for:** evaluation and private deployments. They also say what a public deployment
    still waits for: 0.7.0's removal procedure, the assistive-technology test, and an external security review.
  - **It then covers:**
    - the two hosts and their DNS;
    - the reverse proxy and TLS, with `INKLKUNN_TRUSTED_PROXIES`;
    - the container, and a service unit;
    - the data directory's layout;
    - backup (with the encryption pipe) and restore;
    - upgrading (*back up first, with the new binary*);
    - where the secret lives.

## What "done" means (acceptance criteria)

1. **The rehearsal (D-4) passes on a real prikk,** with the server running during the backup and a write waiting on a
   lease.
2. **A backup:**
   - holds no secret;
   - is owner-only;
   - to standard output, goes through an encryptor and back unchanged;
   - refuses an existing file and a secret inside the destination;
   - says its one line.
3. **A restore refuses each tampering,** and an existing or non-empty destination, and leaves the directory as it was.
   **A restored server has no session and no live API token,** and the restore's summary says so.
   **A flagged wiki is backed up and restored as flagged,** with the same findings. **A lease held past the bound**
   makes the backup refuse, naming the wiki, with every other wiki's writes released.
4. **A backup taken with the new binary of an older store** restores, and `serve` then migrates it.
5. **Health:** `ok` on a healthy server; `degraded` with the store gone, with prikk missing or under the floor, and with
   a wiki's key records missing; and nothing more either way.
6. **`status`** names a flagged wiki, a missing key store with its sentence, and the counters, and says when no server
   runs.
7. **Logs:** the capture test finds no planted content in any line.
8. **The guide's first lines** are as D-9 says, and a test holds them.

## How this was checked against the owner's two lenses

- **Finally clean, safe and secure, robust and sophisticated.**
  - **The backup is exact and checked:** prikk verifies it twice, once taken and once restored.
  - **A restore is all or nothing,** into an empty place.
  - **No secret is ever copied,** and no plaintext copy need touch a disk.
  - **Nothing migrates.**
- **APIs and UI that cannot confuse.**
  - **The backup says what it holds.**
  - **A missing key store is named with its way back,** not as an error.
  - **Health says nothing anyone could misuse.**
  - **The guide says first what it is not for.**

## Alternatives considered

- **`prikk archive export` for each repository.** **Rejected:** its 256 MiB limit and its `remotes/` landing would lose
  views and refuse large wikis. The copy under the lease is exact, and prikk verifies it.
- **A backup that stops the whole server.** **Rejected:** leases stop only writes, and only until each wiki is copied.
- **Encryption inside inklkunn.** Rejected by P-2.

## Threat model v0.6 additions (folded on acceptance)

- **C-19a (new):** a backup never holds the secret file or a session, is owner-only on disk, says it is not encrypted,
  and can be written to standard output for an encryptor; its store snapshot never leaves the data directory.
- **C-19e (new):** a restore revokes every API token and restores no session, so no access taken away after the
  backup comes back.
- **C-19b (new):** a restore treats its archive as untrusted: checksums, no escaping path or link, all or nothing, and
  `prikk verify` on every repository.
- **C-19c (new):** health says only `ok` or `degraded`. `status` and `status.json` are the operator's, owner-only.
- **C-19d (new):** logs carry no content, names or secrets, and a test holds it.
- **RR-18 (new):** a backup is as safe as the operator's handling of it. inklkunn cannot encrypt it for them (P-2).

## Questions for the owner

- **Q-1 — A copy of each repository under its lease, verified by prikk,** not `prikk archive export`. **Recommended.**
- **Q-2 — Every lease taken at once, then released wiki by wiki as each is copied.** **Recommended.** The alternative is
  one wiki at a time, which pauses writes less but cannot be one moment for the store.
- **Q-3 — Health says only `ok` or `degraded` to anyone,** and the detail is the operator's. **Recommended.**
- **Q-4 — Counters in a status file, with no endpoint.** **Recommended.** An endpoint needs an access rule of its own.

## Sequencing and handoff

On acceptance, the architect folds the threat model additions (v0.6) and writes
`rfcs/handoffs/012-backup-restore-and-observability/operations-handoff-v1.md`. The order is D-1 – D-4 first (backup and
the rehearsal), then D-5 – D-8, then D-9. RFC 011's tasks may run beside these.
