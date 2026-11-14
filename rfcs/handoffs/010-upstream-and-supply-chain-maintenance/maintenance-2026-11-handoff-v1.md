# Upstream and supply-chain maintenance, November 2026 — Developer Handoff v1 (RFC 010)

| | |
|---|---|
| Document | Three things on `main` that changed outside inklkunn, found at the review of 2026-11-13: an advisory against the bundled SQLite (M1), a test that assumed one curl build's memory footprint (M2), and prikk 0.56.0 and 0.57.0, both released (M3). |
| Status | Written by the architect 2026-11-13. **Assignable now**, beside handoff 005's K15 – K23. M1 first: CI's supply-chain job is red on `main` until it lands. |
| Decision | M1 by the owner's standing rule (`docs/dependency-policy.md`: **no standing ignore**): fix by upgrading. Reachability is low and is **not** the reason to wait (below). planeter is making the same change for the same advisory (its `advisory-rusqlite-handoff-v1`); the two projects' minimum Rust versions move in step. |
| Basis | RUSTSEC-2026-0459 (`libsqlite3-sys` < 0.39.0, bundled SQLite < 3.54.0: a crafted database and `-wal` pair with different page sizes can return uninitialized page-cache memory; local; CVSS 4.0 medium); `rusqlite` 0.41 / 0.42 release notes; prikk's `CHANGELOG.md` for 0.56.0 (2026-11-08) and 0.57.0 (2026-11-12); `docs/dependency-policy.md` rules 1 and 3; ledger PK-21, PK-26, PK-36, PK-39, PK-43. |
| Handback | One review request, `010-maintenance-2026-11-review-request.md`, with every gate, the prikk version of each with-prikk run, and the advisory database's HEAD. |

## M1 — RUSTSEC-2026-0459: upgrade `rusqlite`, no ignore

1. **Measure first, in a scratch worktree** (dependency policy rule 1), for `rusqlite` 0.41 and the newest release (0.42 today):
   the lock's crate count before and after, deny, audit (a fresh advisory database), new licenses, **the bundled SQLite version
   (≥ 3.54.0) with how you know it**, the declared Rust version of `rusqlite` and `libsqlite3-sys`, and the source lines each needs.
2. **Adopt the newest that clears the advisory with the fewest source changes** (say why if not the newest). If it declares a
   higher Rust version than the workspace's 1.88.0, raise `rust-version` to **exactly** what it declares, nothing more, and
   update everything that names it (the `msrv` CI job, `pin-check`'s rule, `docs/`, the CHANGELOG). The compiler pin (1.99.1) does
   not change.
3. Code changes in `inklkunn-store` (and wherever `rusqlite` is used) only for API changes; **no test edited to pass** except for a
   signature that no longer exists (list each).
4. **Reachability, as evidence:** list every `Connection::open*` and the file it opens and who can write it. (Today one: the store's
   own database in the operator's data directory.) Report anything that opens a path an outsider can influence.
5. A row in `docs/dependency-policy.md` (the advisory, the versions, the bundled SQLite, the lock counts, no ignore) and one line
   under `[Unreleased]` in `CHANGELOG.md`.

**0.2.0's exposure (for the record):** a crafted pair needs write access to the data directory as the service's user, which
already gives the attacker the store itself. No patch release; 0.3.0 ships the fixed SQLite.

## M2 — the curl ceiling test

`inklkunn-fetch`'s `the_real_vector_runs_under_the_ceiling_and_the_real_curl_is_stopped_by_a_small_one` assumes curl cannot start
under 1 MiB; curl 8.23.0 can. Keep what the test is for (handoff 002's F3: the ceiling reaches the real curl) without assuming any
curl build's footprint: the real vector runs curl through the one shared `prlimit` wrapper with exactly the configured ceiling
(assert the command the wrapper runs and the limit the child reports, as the first half does), and a deterministic stand-in that
allocates past a small ceiling is stopped. No test may depend on the host's curl version.

## M3 — prikk 0.56.0 and 0.57.0: the re-baseline

**Measured by the architect on 2026-11-13** (tree `1ac27d8`): the whole suite with prikk **0.56.0** on `PATH` passes except M2's test
(2,618 passed, 1 failed; the same failure as on 0.55.0). 0.56.0's security fixes (`bundle import`, `archive import` and
`extract --claim` admitted forged signatures) are in commands inklkunn never runs (it runs `archive export` only), so the floor does
not have to move for them; the operator notes say an operator who imports by hand should run ≥ 0.56.0.

1. **0.56.0 — confirm and record.** Read the 0.56.0 output changes against every verb the driver and the signer run (`log --ref` now
   exits 1 for a block id — PK-36's fix; `branch --format json` and its signer field — PK-26; `verify` and `doctor` may report
   `unreached objects` as a warning) and say for each whether inklkunn's parsing is affected; a test where one is. The ledger rows
   are the architect's, from your table.
2. **0.57.0 — measure, then adopt.** Install it privately (never over the host's `prikk`), run the suite with it, and report
   per failing test why. Then: C11's late-seal check reads `sealed_by` (PK-39) where prikk provides it, and recovery offers
   discarding a sound queued patch as an operator-confirmed step (PK-43) — or say why either cannot be taken yet. **The floor moves
   only with a feature that needs it,** and then to exactly the release that has it; say which.
3. The with-prikk gate runs name the prikk version they used; until the floor moves, CI's floor stays 0.54.0.

## Non-change scope

No `deny.toml` or `.cargo/audit.toml` ignore. No other dependency update unless the new `rusqlite` forces it (list it). No schema,
file layout or WAL change. No tag, no release, no version change.

## Corrections from the review of 2026-11-14 (binding; **before 0.3.0**)

M1 and M2 are **Approved**; M3 is **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-review-result.md`). The
architect applied the lock bump of `ammonia` 4.2.5 and `rustix` 1.1.6. Hand back M4 – M11 as one request,
`010-maintenance-2026-11-corrections-review-request.md`.

- **M4 — the audit trail says what happened.** The begin event records *asked to take back queued patch X*, not *confirmed*; whenever
  no `discard-queued` step results, the plan says *`--discard-queued-patch X` was not used: <why>* and the end event records it.
- **M5 — `recover --plan` writes nothing.** prikk's `uncommit --plan-only` preview is asked only in a run, under the lease; a run
  without the operator's word stops with the preview and the command, as now.
- **M6 — a stale lock beside an orphan patch is not a dead end:** recovery clears the stale lock (`ClearLocks`) and then stops at
  `foreign-queued-patch` with the preview; a test.
- **M7.** The discard command is offered only when it would be honoured, with the wiki's id in place of `<wiki>`; the preview's lines
  replace K23's direction characters as well as control characters.
- **M8 — *no late seals* only when the list is whole.** PK-39's list is used only when no stage of the report went unevaluated, or
  when the published 0.57.0 is shown to fill `block_seals` independently of the other stages (say which, with the evidence).
- **M9.** The `Uncommit` cost rule is re-measured on the published 0.57.0.
- **M10 — 0.57.0 measured on the published release** (tag `149a9e3d`; the host's `prikk` is now 0.57.0, installed by the owner): the
  suite, PK-39 and PK-43's behaviours and every statement of `m3-measurements-2026-11.md` that was made on the `bfd5b3c9` build. A binary
  the work needs is the owner's installation, or `gh release download` with its `.sha256` checked; **never an address built to get
  past a guard**.
- **M11 — format 8.** The operator notes say a wiki created under prikk 0.57.0 is format 8 and an older prikk refuses it; the driver
  turns prikk's `unsupported format version: N` into a typed error that names the version and does not flag the wiki;
  `prikk_format_version` is filled at creation for information.

## Corrections from the review of 2026-11-14, second round (binding; **before 0.3.0**)

M4 – M11 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-review-result.md`).
**prikk's reply of 2026-11-14** (ledger v1.7, PK-44 – PK-47) is the basis of M13 – M16. Hand back M12 – M19 as one
request, `010-maintenance-2026-11-corrections-2-review-request.md`.

- **M12 — M6 only where it helps.**
  - The stale lock beside an orphan patch is cleared only when prikk has `uncommit`. On 0.55.0 and 0.56.0 the stop clears nothing.
  - No other stop clears a lock.
  - A plan promises *the command that does it* only when a run would offer it: the queue targets the worktree's branch.
  - One real-prikk test on 0.57.0: a held stale lock makes `uncommit --plan-only` refuse (PK-47), and the run that clears it then reads the preview.
- **M13 — `block_seals` is whole by prikk's rule as well as ours.**
  - `block_seals_are_whole` also requires that the `objects` stage is `evaluated` and that `failed_objects` is empty (PK-39). The existing conditions stay until prikk's reference states the rule.
  - `docs/KEYS.md` says what prikk says: a damaged Block is missing from the list and named in `failed_objects`; a later failure does not shorten it.
  - A stand-in shape where every stage is evaluated but `failed_objects` is not empty.
- **M14 — the cleanup-pending step says what it checks.** The comment, the step's note in the plan, and `docs/RECOVERY.md` say this: the queue holds exactly the tip block's patches whenever `verify` reports the code (PK-44), and the step requires a tip block of exactly the one published patch. So the queue's **content** is checked, not only its size. Keep the size check from the plain text as well.
- **M15 — an empty lock: the operator's step that keeps working.** Recovery stays fail-closed (PK-45: an empty lock names no process, so no lease record proves it ours). `docs/RECOVERY.md`'s step:
  - becomes `prikk unlock --lock <that path> --force`, after the checks it already lists (size 0, no prikk or bwrap working in the repository);
  - says that `--force` also overrides a holder that appears running, so it is for this file only, and only after those checks;
  - drops the sentence *`--force` is not needed*.

  The runbook test runs exactly that form. On 0.55.0 it does what the help says. inklkunn's own stale-lock step keeps `--yes`.
- **M16 — a lone metadata-debris warning is not in the way.** `DoctorReport::others` never counts `PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS` (PK-46: harmless, and the next commit clears it). The plan notes it in one line: *prikk's stale ref metadata; the next accepted change clears it*.
  - The `doctor-trouble` section of `docs/RECOVERY.md` for it is replaced by that sentence.
  - Kill-matrix rows that ended at `doctor-trouble` for it now recover. One test through the real state on 0.57.0 shows the next acceptance's commit clearing it.
  - Never `seal` with nothing queued as a way out.
- **M17 — a format error is classified, never assumed** (R-a, R-h). It is an operator mismatch, not flagged, only when the running prikk's version is known not to read format N. The rule is one table in `inklkunn-prikk`: format 8 is read from 0.57.0 on. Any other case is a **damaged or unknown format marker**:
  - the wiki is flagged, so the verdict does not stay *Verified*;
  - recovery stops with a named stop (`format-marker`), and its runbook section says to compare `.prikk/FORMAT` with a backup;
  - nothing claims *nothing is damaged*.

  The sentence names the version prikk gave, never a fixed *8*. The signer and the driver use one classifier with the same conditions (exit 1, no `refusal-v1` report). Leading zeros are refused. RECOVERY.md covers a wiki that was created in format 8. Tests: the marker without its newline on 0.57.0, `9` on 0.57.0, `8` on 0.55.0.
- **M18 — the word's fate says what the run did** (R-b, R-c). The end event's sentence is derived from the steps that **finished**, never from the plan's intent:
  - *taken back* whenever `discard-queued` finished, whatever failed after it;
  - *the lock was not cleared: <why>* when `clear-locks` failed;
  - *recovery ended in an error before it took the patch back* only when it did end before that.

  A recovery error therefore carries the steps done, or the trail is written per step. Tests for both orders.
- **M19 — the preview's line separators.** The preview's replacement set gains U+2028 and U+2029, as `message.rs` and `audit.rs` treat them. Make it one shared definition if they are the same set; otherwise name why they differ. The test that keeps U+2029 changes with it.

Process: the dates of records are the day's real date (the request said 2026-11-15 on 2026-11-14).
