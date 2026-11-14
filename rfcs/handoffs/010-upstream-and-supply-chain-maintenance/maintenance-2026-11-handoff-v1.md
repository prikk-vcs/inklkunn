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

## Corrections from the review of 2026-11-14, third round (binding; **before 0.3.0**)

M12 – M19 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-2-review-result.md`). Hand
back M20 – M26 as one request, `010-maintenance-2026-11-corrections-3-review-request.md`.

- **M20 — every damaged marker is a marker error** (T-a, T-e, T-f).
  - **What is classified.** The classifier also recognises prikk's own sentence for a marker it cannot read: `integrity error: the repository format marker .prikk/FORMAT holds "<text>", which is not a format number this version reads (it reads formats …)`. It is matched exactly as M17 matches the other, and measured on 0.55.0 and 0.57.0. Its result is `FormatMarker`.
  - **Tests:** the markers `8\r\n`, ` 8\n`, `x\n` and an empty file, each on 0.55.0 and 0.57.0, each ending flagged and at the `format-marker` stop.
  - **The flag's reason says what it is.** Add a reason for a damaged format marker, or say in the plan's line why `interrupted` is used; never print *interrupted* for it.
  - **`docs/RECOVERY.md` and `SECURITY.md`** say that the next check flags the wiki, and when checks run.
- **M21 — the plan promises only what the run will do** (T-b).
  - `--plan` asks `has_uncommit`, which is safe in a plan (M12).
  - `InARun` promises the take-back only when prikk has the verb and no stale lock stands in the way.
  - Otherwise the plan says what the run will do: clear the lock and stop with the preview, or nothing, and why.
  - Tests on 0.55.0 and on 0.57.0 with a stale lock, comparing the plan's sentence with what the run did.
- **M22 — the sentences are true** (T-c, T-g, T-h).
  - `AfterSteps` never says *nothing was done* when steps were done.
  - *The lock was not cleared* only when the clear itself failed; a failed re-listing after a clear says so.
  - *Still held* names the lock that is.
  - `is_line_or_control` is used by every text shown as one line, including the foreign-block summary and rollback's `fit`, or the doc comment claims less.
  - Tests assert the whole sentence.
- **M23 — debris is never in the way, in the signer too** (decision 1, T-d). The cleanup-pending step and the view-creation step read *only that finding* through the same rule as `others()`. RECOVERY.md's sentence is then true. Test: debris beside each of the two, on the real state where it can be made.
- **M24 — `failed_objects` read leniently** (T-i). A value that is not a list makes the list not whole; the rest of the report is still read. A driver test.
- **M25 — CI runs the 0.57.0 tests** (T-j, decision 4).
  - `ci.yml`'s build job and `release.yml` install the prikk **0.57.0** release asset as well as the floor. The asset's checksum is pinned in the workflow beside the floor's: `4e621147c3064c1f456803c67f120df9ea35d93e40554e920c9ea55e69d3f9a8` for `prikk-x86_64-unknown-linux-gnu.tar.gz`, measured by the architect against its `.sha256`.
  - It goes in its own directory **after** the floor on `PATH`, so the floor stays the prikk every other test uses.
  - The with-prikk run fails if any `skipping: prikk 0.57.0 or newer not found` or `skipping: no prikk older than 0.57.0` line appears.
  - The shimmed run removes both directories, and `EXPECTED_SKIPS` counts what it counts today.
  - `floor-check` and RFC 010's pin rule name the second pin as a **test-only** prikk, not a floor.
- **M26 — no test fails because the machine is busy** (T-k, and the knowledge result's U-k).
  - **The lock test:** the waiter takes the lock with the product's grace, or the test runs in its own process. What it asserts stays.
  - **The Argon2 test:** it asserts the parameters, and keeps a timing bound only as generous as the slowest supported runner needs (say how much that is).
  - **K40's bounds:** they become counts of work (lookups, or `Arc::ptr_eq` for the memo) instead of wall-clock times.
  - **Every other wall-clock assertion in the workspace:** list each, and say for each why it is safe under load or change it.
- **M27 — first: CI's clippy is red on `main`** (`ce3a3bf`, run 43578514323; T-l).
  - **The fault.** The pinned compiler's clippy (1.99.1) refuses `crates/inklkunn-web/src/api/endpoints/knowledge_api.rs:236`, `.map(&said_in)`, under `needless_borrows_for_generic_args`. The fix is `.map(said_in)`. The architect measured on 1.99.1 that this is the only finding (fmt is clean too).
  - **The cause.** The host's default toolchain is now **1.100.0**, whose clippy does not report it, and every local gate (the dev team's and the architect's) ran `cargo clippy` on the default.
  - **So, in this order:**
    1. The fix, alone, as the first commit, pushed by the architect.
    2. A `rust-toolchain.toml` naming exactly the pinned compiler and the components CI installs (`rustfmt`, `clippy`), so every plain `cargo` in the checkout uses the pin. `pin-check` holds it equal to `ci.yml`, `supply-chain.yml` and `release.yml`'s `RUST_TOOLCHAIN`.
    3. Checks of the rest:
       - The `msrv` job and `cargo +1.89.0 check` keep their explicit toolchain.
       - The Dockerfile's builder is named in the same check if it names a toolchain.
       - Say whether anything else (the measurement tool, the scripts) assumed the default.
