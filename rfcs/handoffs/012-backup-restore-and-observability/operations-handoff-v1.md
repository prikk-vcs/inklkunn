# Backup, restore and observability — Developer Handoff v1 (RFC 012; 0.6.0)

| | |
|---|---|
| Document | `inklkunn backup` and `inklkunn restore`, the rehearsal as a test, health, `inklkunn status`, content-free logs, counters, and the deployment guide (RFC 012 D-1 … D-9). |
| Status | RFC 012 accepted by the owner 2026-11-28 (Q-1 – Q-4 as recommended, after three checks against the owner's philosophy, the last an independent review). **Assignable after M69,** beside handoff 011 (which goes first where the two compete for hands: imported images are the visible gap). |
| Basis | `rfcs/accepted/012-backup-restore-and-observability.md`: D-1 … D-9, acceptance criteria 1 – 8, *How this was checked*, and the review record. Threat model v0.6: T-19 with C-19a – C-19e, RR-18. `docs/releases/0.6.0-plan.md` (P-2: no encryption inside inklkunn; refinement 3: nothing migrates). prikk's guide `docs/src/guide/backup-restore.md`. |
| Audience | The dev team. The two lenses of handoff 003 apply unchanged, and RFC 012's *How this was checked* is the reviewer's checklist. |
| Handback | One review request per group: **O-T1 – O-T3** (backup, restore, the rehearsal); **O-T4 – O-T5** (health, status, counters, logs); **O-T6** (the guide). Each comes with the usual gates: the whole workspace on prikk 0.55.0 first, shimmed off, on 0.57.0, 0.58.0 and 0.59.0; the floor (0.54.0); mutations named per task; an independent reviewer's read for O-T1 – O-T3 (security). |

**The rules above every task:**
- **Nothing migrates, and nothing writes the secret anywhere.**
- **Every file written is owner-only,** and every temporary file or directory lives inside the data directory, is
  removed on success and on failure, and is cleared at the next start if a process was killed.
- **The secret is read only through the configuration's existing path** (`INKLKUNN_SECRET_FILE`).

## O-T1 — Backup (D-1, D-2)

- **`inklkunn backup --to <file | ->`**, with `serve`'s configuration (data, key and secret paths), runnable while a
  server runs: the leases are `flock`s, so the command line and a server exclude each other (`lease.rs`).
- **The order, exactly as D-1 lists it:**
  1. **Free space:** the repositories' and store's sizes against the data directory's free space. Too little refuses,
     naming both.
  2. **The creation mark** is held for the whole backup (`root.rs`, `creation.rs`).
  3. **Every wiki's lease, in a fixed order** (the wiki id's byte order), each within what is left of **30 seconds**.
     On a miss, every lease taken is released, and the command refuses, naming the wiki and what holds it (the lease
     record's holder).
  4. **The store** with `VACUUM INTO` a file in the staging directory (`<data>/.backup-<random>/`, `0700`), and the
     key records copied. **The wikis are then listed from the snapshot,** never from an earlier read.
  5. **Each repository** is copied file by file into staging, and its lease released at once.
     - Copy no symlink, device or fifo: refuse naming it. A repository has none.
     - Keep the file modes.
- **A server write that meets a backup's lease** waits its usual bound (10 seconds), then is refused with *this wiki is
  being backed up; try again in a moment* (D-7's words).
  - The lease record must say that its holder is a backup, so the refusal can say so. **This is a value in the record's
    holder text, not a new column.** Show the record in the request.
- **Then, with no lease held, `prikk verify` on each staged repository:**
  - a failure where the wiki's recorded state is sound refuses the backup;
  - a wiki already flagged is kept, with its findings, in the manifest and the summary.
- **The archive** (one new dependency, a pure-Rust tar crate, measured as the dependency policy says: its ledger row,
  `deny`, `audit` and the MSRV):
  - **the destination:**
    - a file, opened with `O_CREAT|O_EXCL` and mode `0600`;
    - or standard output, refused when it is a terminal;
    - a path inside the data or key directory is refused;
  - entries in a fixed order (store, keys, repositories, by path), with regular files and directories only, and
    modes kept;
  - **the manifest last** (`manifest.json`):
    - every file with its size and SHA-256;
    - the store's schema version;
    - each repository's prikk format, tip and verification state (flagged, with findings);
    - inklkunn's and prikk's versions;
    - the backup's time;
    - **an HMAC-SHA256 over the manifest's canonical bytes**, under `K = HMAC-SHA256(key = secret, message =
      "inklkunn backup v1")`, using RustCrypto's `hmac`, the crate handoff 011 A-T3 adds. If A-T3 has not landed, add
      it here, once.
- **The warning line** on standard error, word for word as D-2 says, and **the staging directory removed** at the end
  and on any failure. A start of `serve` or of `backup` removes a stale `.backup-*`.
- **Not in a backup:** the secret file, `status.json`, and the staging directories themselves.
- **Tests:**
  - the order: a write during the store's copy is refused with the backup's words;
  - the bound: a lease held by a test process for 31 seconds gives a refusal naming it, with every other wiki writable
    again;
  - free space, simulated;
  - a flagged wiki kept;
  - a staged copy corrupted between copy and verify refuses;
  - `O_EXCL` on an existing file;
  - a terminal refused;
  - a destination inside the data directory refused;
  - **a `--to -` whose reader stalls holds no lease** (a write succeeds while the pipe is blocked);
  - the manifest's HMAC.
- **Mutations:** the wikis listed before the snapshot; a lease held while streaming; verify skipped; the HMAC over a
  different encoding than the restore reads.

## O-T2 — Restore (D-3)

- **`inklkunn restore --from <file | ->`** into `serve`'s configured `INKLKUNN_DATA_DIR` and `INKLKUNN_KEY_DIR`, each
  empty or absent. Otherwise refuse, naming it.
- **The order:**
  1. **Read the archive to its end into a staging directory inside the destination** (`<data>/.restoring/`, `0700`,
     with a marker file written first).
     - **Never the tar crate's `unpack`:** each entry is checked and written by inklkunn's own code.
     - **Refused, naming the entry:** a path that is absolute or holds `..`; a link, device or fifo; a duplicate entry;
       a name beyond the bound.
     - Sizes are bounded as they are read.
  2. **The manifest's HMAC first.** On a mismatch, refuse with D-3's sentence, and remove the staging. Nothing outside
     `.restoring/` has been written.
  3. Every file named is present, with its size and SHA-256, and no file is unnamed.
  4. **The store:**
     - opened with `trusted_schema=OFF`;
     - a newer schema is refused, naming both versions;
     - an older one is left as it is (`serve` migrates).
  5. **In the staged store:** every session removed; every API token's `revoked_at` set to the restore's time where it
     is null; the write-lease records cleared. This is one transaction.
  6. **`prikk verify` on each staged repository:** a flagged one (by the manifest) stays flagged, with the store's flag
     kept; any other failure refuses.
  7. **The move into place**, entry by entry from `.restoring/` into the data directory (and the key records into the
     key directory). This is on the same filesystem, or the command says why not before step 1. Then remove the marker,
     then `.restoring/`.
- **`serve` refuses to start** over a data directory holding `.restoring/`, with *a restore did not finish: run
  `inklkunn restore` again*. The next restore clears it first.
- **The summary,** word for word as D-3 says, with the backup's time and each flagged wiki.
- **Tests:**
  - each tampering: a byte, a removed file, an added file, an escaping path, a link, **a manifest rewritten with its
    checksums recomputed**, an archive cut short;
  - **another secret refused before anything is written;**
  - a non-empty destination;
  - a newer store;
  - a kill at each step, with the directories as found or `.restoring/` alone, and `serve` refusing;
  - **the container's layout** (the data directory a mount point, the key directory inside it), run as the image's
    user in the release rehearsal's container job.
- **Mutations:** the HMAC checked after the files are written; `trusted_schema` left on; the tokens not revoked; the
  move done before verify.

## O-T3 — The rehearsal (D-4)

- **One end-to-end test on a real prikk, as D-4 lists it.**
  - A wiki with pages, views, a hide, an import's history, accounts, API tokens and keys is backed up while the server
    runs and a write waits.
  - It is restored into fresh directories under the same secret, served, and shown:
    - the same pages and history;
    - the views as views;
    - the hide hiding;
    - the accounts signing in again, and the old tokens refused;
    - **a proposal accepted and sealed under the restored keys;**
    - `prikk verify` passing.
- **The refusals of D-4** are each a case of it, or of O-T2's tests: say which in the request.
- **Through an encryptor:** where `age` is available on the runner, `backup --to - | age | age -d | restore --from -`.
  Otherwise, a `cat` pipe with a stated skip.
- **The release rehearsal** runs this test once, as it runs the kill matrix.

Hand back O-T1 – O-T3 as `012-operations-o-t1-o-t3-review-request.md`, with an independent reviewer's read.

## O-T4 — Health, status, counters (D-5, D-6, D-8)

- **`/-/health`:**
  - **the checks:** the store answers, prikk at or above the floor, the key records of every wiki;
  - made by one task **at most every ten seconds**, the answer kept between;
  - **the answers:**
    - `ok` with `200`;
    - `degraded` with `503` only for the store or prikk;
    - `degraded` with `200` for a wiki's missing key records;
    - the body is the one word.
  - The probe's doc comment in `probe.rs` says it is the liveness probe. Keep it true.
- **`inklkunn status`:**
  - each wiki as D-6 lists it, reading the store read-only and the repositories without a lease;
  - **the missing key store's two sentences:** the operator's in `status` and the log, and the contributor's on a
    refused write;
  - **the counters from `<data>/status.json` with its age:** older than a minute, it says the server may not be
    running, and shows none as current.
- **The counters:** admissions, refusals by kind, acceptances, the builder's queue, prikk calls; written every ten
  seconds, atomically (temporary file in the data directory, `fsync`, rename), `0600`.
- **Tests:** criterion 5 in full (each check, each answer, the rate under a flood of probes); criterion 6; a stale
  `status.json`.
- **Mutations:** `503` for a wiki's keys; the cache removed; stale counters shown as current.

## O-T5 — Logs (D-7)

- **One structured line per event:** `level`, `event`, the wiki's id, the request's id; a request by its route's name,
  status and time. It is logfmt, hand-written, with **values escaped** (a quote, a newline, an `=`), and no new
  dependency.
- **Never in a line:** page text, comments, names, email or IP addresses, a request's path or query, **an error's
  detail text** (log its kind; prikk's stderr never reaches a log), secrets.
  - Every `{cause}`-style interpolation in the server's log calls today (`restore.rs`, `appeals.rs` and the rest) is
    changed.
  - List them in the request.
- **The capture test:**
  - the end-to-end tests run with a log capture, plus a failing prikk call over a planted path;
  - planted markers in a page, a name, an email address, a path, a signed address (A-T3), and a prikk message;
  - it fails on any of them in any line.
- **Mutations:** one log call given back its `{cause}`; the query string logged.

Hand back O-T4 – O-T5 as `012-operations-o-t4-o-t5-review-request.md`.

## O-T6 — The deployment guide (D-9)

- **The book's operator part, with these first lines word for word as D-9 says:** for evaluation and private
  deployments; a public deployment waits for 0.7.0's removal procedure, the assistive-technology test and an external
  security review.
  - A test holds them.
- **Then:**
  - the two hosts, with **the content origin under the application's registrable name** (RFC 011's CORP);
  - the proxy and TLS (`INKLKUNN_TRUSTED_PROXIES`, and that the proxy's access log holds addresses and paths);
  - the container and a service unit;
  - the data directory's layout;
  - backup with the encryption pipe, and restore, with its summary explained;
  - upgrading (*back up first, with the new binary*);
  - **the secret: kept apart from the backups and as carefully;** without it inklkunn cannot restore a backup, and its
    repositories stay readable with prikk alone.
- **The words of O-T1 – O-T5** join `.git-exclude/ja-review/0.6.0-messages-since-0.5.0.md` (DR-7).

Hand back O-T6 as `012-operations-o-t6-review-request.md`.

## Not in this handoff

- The permanent removal procedure (0.7.0, P-1); re-keying a lost key store (IS-15, P-3); encryption inside inklkunn
  (P-2); a metrics endpoint.

## After the build

- **Done means RFC 012's acceptance criteria 1 – 8,** each named by its test in the last review request.
- Threat model §7 gains the v0.6 test lines for C-19a – C-19e, verified at 0.6.0's readiness (R-5).
- The readiness map's end-to-end proof is the rehearsal (O-T3) run on the release build, in the container, and the
  upgrade from 0.5.0 runs `backup` with the new binary first, as the guide says.
