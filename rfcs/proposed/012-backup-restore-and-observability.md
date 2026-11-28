# RFC 012 — Backup, restore and observability: a server that can be kept, and says how it is

**Status.** **Proposed to the owner 2026-11-28.** Checked three times against the owner's philosophy before the rulings:
the architect's own check, a second check against the code, and an independent review at the owner's request. The text
below is the result, and the review record is at the end. This is 0.6.0's R-1, with RFC 011 (`docs/releases/0.6.0-plan.md`,
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
- **`inklkunn backup`:** one archive of all three, taken at one moment, checked before it is written, and written to a
  file or to standard output, so that it can go straight into an encryptor;
- **`inklkunn restore`:** refusing anything it cannot prove this server made, and anything that does not verify;
- **a rehearsal of both, as a test;**
- **a health answer that checks** the store, prikk and the keys;
- **`inklkunn status`:** every wiki's state;
- **structured logs** that carry no content;
- **counters;**
- **the deployment guide.**

**Nothing here migrates the store** (plan refinement 3), and **nothing writes the secret file anywhere.**

## Decisions

- **D-1 — What a backup is: an exact copy, taken under the write leases, checked before it leaves.**
  - **Each repository is copied as its directory, byte for byte, while its wiki's write lease is held.**
    - inklkunn's reads run prikk on a read-only bind, so nothing changes a repository while its lease is held, and the
      copy is exact. It holds every view, the trust record, the log and the queue.
    - **Why not `prikk archive export`?** It refuses any repository with a file over 256 MiB, and its import lands every
      ref but the exporter's under `remotes/`, so a wiki's views would not come back as views. The copy has neither
      limit (Q-1).
  - **The store** is copied with SQLite's `VACUUM INTO`: one consistent snapshot, with no new dependency and no
    migration. A store of an older schema is copied as it is.
  - **The key store** is copied as its sealed records. **The secret file is never in a backup**, and neither is the
    server's own `status.json`.
  - **One moment, and a short one** (Q-2):
    1. the free space is checked first. A backup that would not fit refuses, naming what it needs and what is free;
    2. the wiki-creation mark is held, so no wiki is made during the backup;
    3. every wiki's lease is taken, in a fixed order, **within 30 seconds**. When one is not free in time, the backup
       releases every lease it took and refuses, naming the wiki and what holds it. It never waits for ever;
    4. the store and the key store are copied, and **the wikis are listed from that snapshot**;
    5. each repository is copied, and its lease released at once.

    **Everything is copied into a staging directory inside the data directory** (owner-only, beside what it copies).
    So the leases are held only for a local copy, never for a slow disk, pipe or network, and no copy leaves the data
    directory unencrypted.
  - **A write that meets a backup's lease** waits, as any write waits for a lease (at most ten seconds), and is then
    refused with *this wiki is being backed up; try again in a moment*, never a general *busy*. **Reads never stop.**
  - **Then, with no lease held, the copies are checked:** `prikk verify` runs on each staged repository.
    - **A copy that fails where the wiki was sound refuses the whole backup:** the copy is not the wiki.
    - **A wiki that was already flagged is backed up as it is,** since that is when a backup matters most. The
      manifest and the summary name it and its findings, so a restore never presents it as sound.
  - **Only then is the archive written** (D-2). The staging directory is removed when the archive is written, on any
    failure, and at the next start if a backup was killed.
- **D-2 — One archive, to a file or to standard output, signed by this server.**
  - **The destination:**
    - `inklkunn backup --to <file>` writes one archive (tar), owner-readable only (`0600`). It refuses an existing
      file, and a path inside the data directory or the key directory, where the archive would be lost with what it
      protects.
    - `inklkunn backup --to -` writes it to standard output, so `inklkunn backup --to - | age -r … > backup.age` puts no
      unencrypted copy outside the data directory (P-2). It refuses when standard output is a terminal.
  - **A manifest, written last**, lists:
    - every file with its SHA-256;
    - the store's schema version;
    - each repository's prikk format, tip and verification state;
    - inklkunn's and prikk's versions;
    - the backup's time.
  - **The manifest is authenticated:** an HMAC-SHA256 under a key derived from the server's secret with its own label
    (`inklkunn backup v1`). So nobody who can write the archive, but does not hold the secret, can change a byte of it
    and have it restored. An archive with no manifest (a backup cut short) is not an archive.
  - **The command says, in one line on standard error:** *inklkunn does not encrypt backups: this one holds people's
    names, password hashes and API token hashes, and what moderation hides; encrypt it before it leaves this machine.*
    These words are true whether it writes a file or a pipe.
  - One new dependency for the archive format (a pure-Rust tar crate), measured as the dependency policy says.
- **D-3 — Restore: proven, checked, all or nothing, and no access comes back.**
  - **`inklkunn restore --from <file or ->`** restores into **the data directory and the key directory that `serve`
    is configured with** (`INKLKUNN_DATA_DIR`, `INKLKUNN_KEY_DIR`), so the keys land where `serve` looks. Each must
    be empty or absent.
  - **It needs the server's secret file,** as every command does, and **checks the manifest's HMAC before anything
    else.** An archive made under another secret, or changed after it was made, is refused before anything is written:
    *this backup was not made with this server's secret file, or it was changed after it was made*. So a restore
    cannot start a server whose keys would not open.
  - **It reads the archive as untrusted input:**
    - no path escapes, and no links;
    - every file in the manifest is present and matches its SHA-256;
    - no file is there that the manifest does not name.
  - **It restores into a staging directory inside the destination** (`.restoring/`), and moves the entries into place
    only at the end.
    - **Anything that fails refuses the restore** and leaves the directories as it found them.
    - A restore killed half-way leaves only `.restoring/`. `serve` refuses to start over it, naming it, and the next
      restore clears it.
  - **It runs `prikk verify` on every repository.** A wiki that the manifest names as flagged stays flagged; any other
    that fails refuses the restore.
  - **The store:**
    - it is opened with `trusted_schema=OFF`;
    - a newer store than this binary knows is refused, naming both versions;
    - an older one is restored as it is, and migrated by `serve` as an upgrade is.
  - **No access taken away after the backup comes back** (C-19e). The store goes back to the backup's moment, so in the
    restored store:
    - **every session is removed,** so everyone signs in again;
    - **every API token is revoked** (at the restore's time), so tokens must be issued again;
    - the write-lease records are cleared, so no wiki looks interrupted.
  - **It ends with a summary:**
    - the backup's time;
    - each flagged wiki;
    - *everyone signs in again; API tokens must be issued again; an account disabled or a password changed since the
      backup's time is as it was then: check them.*
- **D-4 — The rehearsal is a test.** A wiki with pages, views, a hide, an import's history, accounts, API tokens and keys
  is backed up while the server runs and a write waits. It is then restored, served, and shown:
  - the same pages and history;
  - the views as views;
  - the hide still hiding;
  - the accounts signing in again, and the old tokens refused;
  - **a proposal accepted and sealed under the restored keys;**
  - `prikk verify` passing.

  **Refused:** a byte changed, a file removed, a file added, a path that escapes, **a manifest rewritten to match**, an
  archive cut short, and a restore under another secret. **A stalled consumer of `--to -`** holds no lease.
- **D-5 — Health, and what it may say to anyone** (Q-3).
  - `/-/health` checks three things:
    - the store answers;
    - prikk runs at a version at or above the floor;
    - **the key store holds the records of every wiki.**
  - **To anyone it answers `ok` or `degraded`, nothing more:** no wiki, no path, no count.
  - **The status code is `503` only when the server cannot serve at all** (the store or prikk). A wiki missing its key
    records is `degraded` with `200`: health is the proxy's liveness probe, and one wiki must never make an orchestrator
    restart, or a proxy drop, a server whose other wikis are fine.
  - **Its checks are made at most once every ten seconds,** and the answer kept between them, so anyone polling
    `/-/health` can never make the server start a process.
- **D-6 — `inklkunn status`, the operator's view** (OPS-6).
  - It lists every wiki:
    - its prikk format;
    - its verification state;
    - whether it is flagged, and why, in RECOVERY.md's words;
    - its writes-index queue;
    - **whether its key records are present.**
  - **A wiki whose key store is missing says exactly that** (plan refinement 4), in the words of whoever reads them:
    - in `status` and the log: *the key records of wiki X are missing: restore the key directory from a backup
      (KEYS.md); re-keying is not available yet*;
    - to a contributor whose write it refuses: *this wiki cannot take changes until the operator restores its keys.*
  - **The counters** (D-8) come from the running server's status file, with its age. When the file is older than a
    minute, `status` says that the server may not be running, and never shows stale counters as current.
- **D-7 — Structured logs that carry no content.**
  - Every line the server writes becomes one structured line: `level`, `event`, the wiki's id where there is one, the
    request's id. It is logfmt-shaped, hand-written, with no new dependency.
  - **A line never holds:**
    - page text, a comment, or a name;
    - an email or IP address;
    - a request's path or query (a path holds a page's name, and a query a signed address);
    - **an error's detail text** (prikk's messages can name paths): an error is logged by its kind;
    - a secret.
  - A request is logged by its route's name (`page.view`), its status and its time. The guide says that a reverse
    proxy's access log, which holds addresses and paths, is the operator's own choice.
  - **A test** runs the end-to-end tests with a log capture, plus a failing prikk call over a planted path. It fails on
    any planted marker it finds: in a page, a name, an email address, a path, a signed address, or a prikk message.
- **D-8 — Counters.**
  - Admissions, refusals (by kind), acceptances, the background builder's queue, and prikk calls, kept in the process.
  - The server writes them every ten seconds to `<data>/status.json`, atomically and owner-only, and `status` reads it.
  - **No endpoint** (a metrics endpoint is later, with its own access rule).
- **D-9 — The deployment guide** (OPS-5), the book's operator part.
  - **Its first lines say who it is for:** evaluation and private deployments. They also say what a public deployment
    still waits for: 0.7.0's removal procedure, the assistive-technology test, and an external security review.
  - **It then covers:**
    - the two hosts and their DNS: a content origin under the application's registrable name (RFC 011's CORP);
    - the reverse proxy and TLS, with `INKLKUNN_TRUSTED_PROXIES`;
    - the container, and a service unit;
    - the data directory's layout;
    - backup (with the encryption pipe) and restore;
    - upgrading (*back up first, with the new binary*);
    - **where the secret lives, and that it is kept apart from the backups and as carefully:** without it a backup
      cannot be restored by inklkunn. Its repositories stay readable with prikk alone.

## What "done" means (acceptance criteria)

1. **The rehearsal (D-4) passes on a real prikk,** with the server running during the backup and a write waiting on a
   lease.
2. **A backup:**
   - holds no secret and no `status.json`;
   - is owner-only, and authenticated by its manifest;
   - to standard output, goes through an encryptor and back unchanged, and holds no lease while it streams;
   - refuses an existing file, a path inside the data or key directory, a terminal, too little space, and a lease not
     free within the bound (naming the wiki, with every other wiki's writes released);
   - backs up a flagged wiki as flagged, and refuses a copy that fails where the wiki was sound;
   - says its one line.
3. **A restore:**
   - refuses each tampering, including a manifest rewritten to match, an archive cut short and another secret, before
     writing anything;
   - refuses a non-empty destination, and leaves the directories as it found them;
   - puts the keys where `serve` looks;
   - leaves no session and no live API token, and no wiki looking interrupted;
   - says its summary.

   A killed restore stops `serve` until it is run again.
4. **A backup taken with the new binary of an older store** restores, and `serve` then migrates it.
5. **Health:**
   - `ok` on a healthy server;
   - `degraded` with `503` with the store gone, and with prikk missing or under the floor;
   - `degraded` with `200` with one wiki's key records missing;
   - nothing more either way;
   - no more than one check every ten seconds under any load.
6. **`status`** names a flagged wiki, a missing key store with its sentence, and the counters with their age, and says
   when the server may not be running. A contributor's refused write has the contributor's sentence.
7. **Logs:** the capture test finds no planted marker in any line.
8. **The guide's first lines** are as D-9 says, and a test holds them.

## How this was checked against the owner's two lenses

- **Finally clean, safe and secure, robust and sophisticated.**
  - **The backup is exact and checked:** prikk verifies the copy before the archive is written, and every repository
    again on restore.
  - **A backup can be restored only by the server that made it, unchanged.**
  - **A restore is all or nothing,** and **gives back no access that was taken away.**
  - **No secret is ever copied,** no unencrypted copy need leave the data directory, and writes pause only for a local
    copy.
  - **Nothing migrates.**
- **APIs and UI that cannot confuse.**
  - **The backup says what it holds, and the restore what it rolled back.**
  - **A missing key store is named with its way back to the operator,** and plainly to a contributor.
  - **Health says nothing anyone could misuse,** and never takes a whole server down for one wiki.
  - **The guide says first what it is not for.**

## Alternatives considered

- **`prikk archive export` for each repository.** **Rejected:** its 256 MiB limit and its `remotes/` landing would lose
  views and refuse large wikis. The copy under the lease is exact, and prikk verifies it.
- **A backup that stops the whole server.** **Rejected:** leases stop only writes, and only for a local copy.
- **Streaming while the leases are held.** **Rejected:** a slow pipe would hold every wiki's writes, and a failed
  verification would come after the bytes had gone.
- **Verifying each repository before and after its copy.** **Rejected:** it doubles a large wiki's time. The copy is
  verified once, against the wiki's recorded state.
- **Encryption inside inklkunn.** Rejected by P-2.

## Threat model v0.6 additions (folded on acceptance)

- **C-19a (new):** a backup never holds the secret file, is owner-only on disk, says it is not encrypted, can be written
  to standard output for an encryptor, and stages nothing outside the data directory.
- **C-19b (new):** a restore treats its archive as untrusted: the manifest's HMAC under the server's secret first, then
  checksums, no escaping path or link, all or nothing, and `prikk verify` on every repository.
- **C-19c (new):** health says only `ok` or `degraded`, and makes at most one check every ten seconds. `status` and
  `status.json` are the operator's, owner-only.
- **C-19d (new):** logs carry no content, names, addresses, paths, error details or secrets, and a test holds it.
- **C-19e (new):** a restore removes every session and revokes every API token, so no access taken away after the
  backup comes back.
- **RR-18 (new):** a backup is as safe as the operator's handling of it. inklkunn cannot encrypt it for them (P-2), and
  a lost secret file makes it unrestorable by inklkunn.

## Questions for the owner

- **Q-1 — A copy of each repository under its lease, verified by prikk,** not `prikk archive export`. **Recommended.**
- **Q-2 — Every lease taken at once (within 30 seconds), then released wiki by wiki as each is copied locally.**
  **Recommended.** The alternative is one wiki at a time, which pauses writes less but cannot be one moment for the store.
- **Q-3 — Health says only `ok` or `degraded` to anyone,** with `503` only when the server cannot serve at all, and the
  detail is the operator's. **Recommended.**
- **Q-4 — Counters in a status file, with no endpoint.** **Recommended.** An endpoint needs an access rule of its own.

## Sequencing and handoff

On acceptance, the architect folds the threat model additions (v0.6) and writes
`rfcs/handoffs/012-backup-restore-and-observability/operations-handoff-v1.md`. The order is D-1 – D-4 first (backup and
the rehearsal), then D-5 – D-8, then D-9. RFC 011's tasks may run beside these.

## Review record (2026-11-28, before the rulings)

1. **The architect's own check** (four refinements):
   - a write that meets a backup says so;
   - no `status.json` in a backup;
   - the restore says what the secret is for;
   - `status` shows its counters' age.
2. **A second check against the code, at the owner's request** (nine more), among them:
   - a flagged wiki backed up as it is;
   - the leases' bound;
   - the store's snapshot staged in the data directory (correcting a claim that no copy touched any disk);
   - no access coming back on restore;
   - health's ten-second checks;
   - logs with no path, query or address.
3. **An independent review, at the owner's second request** (adopted after the architect verified each claim in the
   code):
   - **the manifest authenticated under the secret** (checksums alone stop accidents, not someone rewriting the
     manifest);
   - **staging before streaming,** so a slow pipe holds no lease (`lease.rs`: a server write waits at most ten
     seconds);
   - **staging the restore inside the destination,** with the key directory from `serve`'s own configuration (the
     `Dockerfile`'s volume cannot be renamed over);
   - the write-lease records cleared and the creation mark held (`lease.rs`, `creation.rs`);
   - **`503` only when the whole server cannot serve** (`probe.rs`: health is the liveness probe);
   - the contributor's own sentence;
   - errors logged by kind only.

   **Cut:**
   - verifying each repository twice;
   - removing sessions from the snapshot (they are removed once, at restore).
