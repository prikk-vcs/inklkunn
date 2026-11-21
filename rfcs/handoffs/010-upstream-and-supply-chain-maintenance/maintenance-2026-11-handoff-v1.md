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

## Corrections from the review of 2026-11-14, fourth round (binding; **before 0.3.0**)

M20 – M27 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-3-review-result.md`); M27
step 1 was pushed alone and CI is green on it. RFC 010 D-3 now names the test-only prikk. Hand back M28 – M33 as one request,
`010-maintenance-2026-11-corrections-4-review-request.md`.

- **M28 — CI takes a patch back on 0.57.0** (W-a).
  - Every test whose subject needs a 0.57.0 verb (the take-back, its worktree restore, the confirmation, the kill matrix's take-back row) runs that subject on the first prikk 0.57.0 or newer on `PATH` (`new_057`, `prikk_where`), and prints the guarded skip line when there is none.
  - The floor's *no verb* answer stays a test of its own on the first prikk on `PATH`.
  - List every test that branches on the prikk version, and say for each which prikk runs which branch.
  - The comments in `ci.yml` and `release.yml` say what is true.
- **M29 — every install of the pin names its components** (W-b).
  - `supply-chain.yml` and `release.yml` (`:144`, `:314`) install 1.99.1 with `--component rustfmt --component clippy`, as the toolchain file asks.
  - pin-check requires the file's components on every install of the pin (a planted-failure test).
  - Say whether any job now downloads anything it did not before.
- **M30 — tests that fail rather than pass or hang** (W-c, W-d).
  - K40's counters count units of work: links and anchors read. The quadratic rewrite above must fail.
  - The follow-up test bounds its wait (a `recv_timeout` long enough for a loaded runner) so a regression fails.
  - Every CI job gets a `timeout-minutes` (twice its slowest recent run, stated).
- **M31 — the sentences hold in every order** (W-e, W-f).
  - The sentence of a step that failed survives a later audit failure: the error carries it, and the steps done.
  - `AfterLocks` promises a clear only when prikk's liveness says the holder is not running. Otherwise the plan says the run will stop at the lock and why, as the run does.
  - A test of each.
- **M32 — the documents** (W-g).
  - `docs/RELEASING.md` and `docs/dependency-policy.md` describe the test-only prikk, the skip guard and `rust-toolchain.toml`.
  - `SECURITY.md` and `docs/RECOVERY.md` say that the check after a seal is throttled (at most once a minute), and use one wording for a retirement's end.
  - `inklkunn-fetch/src/scrub.rs` uses `is_line_or_control`.
- **M33 — `Witness.format_marker` is an enum** (decision 2): a marker error with a number, and a marker error without one. There is no `Option<Option<_>>`.

## Corrections from the review of 2026-11-14, fifth round (binding; **before 0.3.0**)

M28 – M33 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-4-review-result.md`). Hand
back M34 – M37 as one request, `010-maintenance-2026-11-corrections-5-review-request.md`.

- **M34 — no test of a 0.57.0 subject passes by not reaching it** (Z-a, decision 2).
  - The four `reports_block_seals` tests are split as M28 split the take-back: a floor test, and an `on_0_57_0_…` test on the first prikk 0.57.0 or newer, with the guarded lines.
  - `a_block_prikk_cannot_read_…` never returns without a guarded line.
  - A workspace check (a test in `floor-check`, or a grep in CI) fails on any early return after a prikk-capability probe that prints no guarded line.
- **M35 — `pin-check` reads the workflows as YAML** (Z-b).
  - Jobs and their `timeout-minutes` come from a YAML parser (`serde_yaml` is already in the lock for the workflow tests), not from line shapes.
  - An install is any `rustup toolchain install` or `rustup install` of the pin anywhere in a `run:` script, single-line or block, split at `;`, `&&`, `||` and newlines.
  - Every one of the reviewer's forms is a planted-failure test.
  - Each non-vacuity guard counts what it guards: jobs, and installs found by the parser.
- **M36 — `scrub` and the lock step** (Z-c, Z-d, decision 3).
  - `scrub` filters line separators and controls **before** it redacts, with the reviewer's case as a test.
  - A parity test (fetch's dev-dependency on the store) holds `scrub`'s copy of `is_line_or_control` equal to the store's over every code point.
  - A `ClearLocks` step whose lock prikk says appears running says so in the plan: the run will try it and stop there, and why. A test on 0.57.0's listing.
- **M37 — smaller points and CI** (Z-e; the knowledge result's decision 2).
  - The kill matrix's 0.57.0 row takes the patch back from the kill state and shows the worktree put back.
  - `binaries` gets `timeout-minutes: 10`.
  - **CI lifts the runner's restriction on unprivileged user namespaces** for the test steps (`sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`, Ubuntu 24.04), so the PID-namespace test of K55 runs. The with-prikk run's guard fails on `skipping: PID namespaces unavailable` as on the 0.57.0 lines.
  - Say whether the bubblewrap self-skips then stop printing in CI, which would make the sandbox tests real there too.

## Corrections from the review of 2026-11-14, sixth round (binding; **before 0.3.0**)

M34, M36 and M37 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-5-review-result.md`).
**M35's premise was the architect's error** (no YAML crate is in the lock), and it is replaced by M38. Hand back M38 – M40 as one
request, `010-maintenance-2026-11-corrections-6-review-request.md`.

- **M38 — `pin-check` refuses what it cannot read** (replaces M35; no new crate). The workflows are held to one canonical form, and `pin-check` fails on anything outside it:
  - `jobs:` is a line of its own, and each job key is `^  [a-z0-9-]+:$`, with no comment or trailing space;
  - each job has `timeout-minutes: <N>` among its own keys, before its `steps:`;
  - every line that mentions `rustup` is one of the canonical install forms (`rustup toolchain install <pin> --profile minimal --component rustfmt --component clippy --no-self-update`, or the `msrv` job's form), inside a block `run: |`, never a one-line `run:` and never after `;`, `&&` or `||`.

  The reviewer's six shapes are planted failures, each **refused**. The non-vacuity guards count jobs and installs as the canonical reader finds them.
- **M39 — a test reaches a prikk capability only through one helper** (Z′-a; replaces the heuristic scanner).
  - One module per test crate (or one shared test-support module) holds every capability probe: `prikk_where`, `real_prikk_at_least_057`, `reports_block_seals`, `has_uncommit` and the version probes. It returns the binary or a world, and **prints the guarded line itself** when it returns none.
  - The workspace check fails on any call of a probe name outside those modules, and on any helper there that can return none without printing.
  - Probe names are found by their definition in those modules, never by a hand-kept list.
  - The reviewer's nine shapes become planted failures of the new rule where they still compile.
- **M40 — small points and the bubblewrap guard** (Z′-b, Z′-c, decision 2).
  - **`scrub`:** it filters the store's `is_line_or_control` set plus the format characters (Cf, as `yaml::is_bidi_or_format` names them) before it measures a piece. It chooses the line to show after flattening. A test for each of the three cases.
  - **`user_namespace_violations`** checks each job that runs `cargo test`, honouring `if:`.
  - **The bubblewrap lines: not yet in the guard.** The architect read CI run 43606787565 (`2e0125c`), the first run with the sysctl step:
    - the step ran (`kernel.apparmor_restrict_unprivileged_userns = 0`), and **the PID-namespace tests ran** in both test steps;
    - but bubblewrap still **cannot confine**: 27 skip lines (17 *running unconfined*, 6 plain, 4 *the confined policy*);
    - the line does not say why.

    So `probe_bwrap`'s skip line carries bubblewrap's own reason, the first line of its stderr (scrubbed, at most 200 characters). The next CI run then shows the cause. Name it in the request, and whether a further runner step (for example Ubuntu 24.04's `bwrap-userns-restrict` AppArmor profile) is needed and safe. The guard joins once a run shows the lines gone.
  - **The serve refusal test** runs everywhere through an injected *cannot confine* answer, so it never depends on the runner.

## Corrections from the review of 2026-11-17 (binding; **before 0.3.0**)

M38 – M40 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-6-review-result.md`), which
also states these gates' threat model: **accidents, not deliberate evasion**. Hand back M41 – M42 as one request,
`010-maintenance-2026-11-corrections-7-review-request.md`.

- **M41 — the guarded-skip rule, closed for honest edits** (E-a, E-b).
  - The with-prikk run in `ci.yml` and `release.yml` also fails on `skipping: prikk not on PATH`, since prikk is always on `PATH` there.
  - `real_prikk` moves into each `prikk_probes.rs`, as the entry `prikk_first`, and its callers use it.
  - Entries return only `Option<PathBuf>` or `Option<` a world type defined in that test file `>`. No `bool` or other answer is allowed.
  - An entry's tail must be exactly `Some(<binding>)`.
- **M42 — small points** (E-c, E-d; decision 1).
  - `pin-check` refuses any `cargo +X` or `rustc +X` outside the `msrv` job, and any `+X` there other than the MSRV.
  - `user_namespace_violations`:
    - compares the whole `if:` value, folded lines included;
    - refuses a lift with `continue-on-error`;
    - sees `cargo +<pin> test` as a test.
  - The serve proof asserts, where bwrap fails, that `sandbox()` refuses and carries bubblewrap's reason.
  - Fetch's parity test uses a constant list of the 12 direction characters, so the dev-dependency on the signer goes.
  - The doc comments of `is_direction_or_format` and `is_format_character` say why the two sets differ (ZWNJ and ZWJ in knowledge text).
- **M43 — first: the sandboxes take their layout from the host** (IS-19; read by the architect from CI run 43985682946, `bwrap: execvp /usr/bin/true: No such file or directory`).
  - **One function** builds the root layout for every bubblewrap use: the probe, prikk's sandbox and curl's vector. For each of `/bin`, `/sbin`, `/lib`, `/lib64` and `/lib32`, the host's own entry is read: a symlink is recreated with **its own target**, a real directory is bound read-only, and an absent one is left out. Nothing assumes Arch's or Debian's layout.
  - **Tests:**
    - the layout for a planted Arch-style and a planted Debian-style root;
    - CI's confined tests running on Ubuntu, with no bubblewrap skip line. The bubblewrap lines then join the guard: `skipping: bubblewrap cannot confine` and `skipping the confined policy`.
  - **The release workflow runs the built image once before it publishes:** `docker run` with the user-namespace setting the docs give, and a command that confines prikk, for example `wiki create` on a scratch volume, or a probe subcommand. It fails if the image cannot confine.
  - **`docs/` and the image's README** say what a host needs for confinement, measured in that run.
  - **For 0.2.1** (the owner's decision): the same change applies on `release/0.2`. Say whether it cherry-picks clean.

## Corrections from the review of 2026-11-17, second round (binding; **before 0.3.0**)

M41 – M43 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-7-review-result.md`).
M43's layout holds on Debian bookworm, Ubuntu 24.04, Fedora, Alpine and absolute-link roots. Hand back M44 – M46 as one
request, `010-maintenance-2026-11-corrections-8-review-request.md`, **except M44's first point, which comes first, on its
own commit, and is pushed alone by the architect** (main's CI is red, run 44065172543).

- **M44 — first: main green on Ubuntu; then a missing bubblewrap is a guarded skip, never a silent one** (G-0, G-a, G-b).
  - **First (G-0):**
    - The stand-ins in `real_vector.rs:216` and `standin.rs:371` read `/proc/self/limits` with shell builtins only, not
      `awk`. On Ubuntu `awk` is a link through `/etc/alternatives`, and no sandbox mounts `/etc`.
    - The with-prikk test step in `ci.yml` and `release.yml` runs `cargo test` with `--no-fail-fast`, so one red never
      hides the rest of the workspace. floor-check holds it.
    - Say whether a production bind (prikk's binary, curl's) can be a link through `/etc`. If it can, bind the resolved
      file, with a test.
  - The probe's spawn error says `bubblewrap cannot confine a process here (`bwrap` could not run: …)`, so every skip line
    for it matches the guard's `bubblewrap cannot confine` fragment. A planted test shows a missing binary's line is guarded.
  - The real-probe test in `sandbox/tests.rs` skips with that guarded line when bwrap cannot be spawned. It fails only on
    bubblewrap's own `execvp … No such file or directory` (the loader not found).
- **M45 — prikk's sandbox and the probe are tied to `root_layout`** (G-c). A parity test like the vector's for
  `BwrapConfig::command`, and one for the probe through the stand-in bwrap harness. Reverting either call site to a fixed
  list fails a test on any host.
- **M46 — what an operator reads** (G-d, G-e; decisions 1, 2).
  - **SECURITY.md** says what the three container settings take away (docker's seccomp filter, its AppArmor profile, the
    `/proc` masks), and that bubblewrap's sandbox around prikk and curl is then the confinement inside the container. It says
    the docker names are the ones the release workflow runs with; the architect's rehearsal measures them before a candidate.
  - **The release notes'** runtime-prerequisites line points to *What a host needs for confinement*. The image carries an
    `org.opencontainers.image.documentation` label with that URL.
  - **Small points:**
    - `vector.rs:7` names all five entries;
    - the two module docs (`guarded_skips.rs:30-31`, the signer's `prikk_probes.rs:5-6`) name the guard as it is;
    - the with-prikk step's name says what the guard holds, not *none may skip*;
    - `config.rs:158`'s advice for a refusal that is bubblewrap's `execvp … No such file or directory` names the host's
      loader and libraries (see SECURITY.md), not user namespaces.

## Carried into 0.3.0's readiness (2026-11-17; binding, reviewed with the readiness package)

M44 – M46 are **Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-8-review-result.md`), and this
package's correction rounds end, **except M48, which comes first** (main's CI is red, run 44077152844). M48 goes on its own commit
with a short request, `010-maintenance-2026-11-corrections-9-review-request.md`; the architect checks it and pushes it
alone. The rest goes into 0.3.0's readiness, with no separate request.

- **M48 — first: a ceiling hit in bubblewrap's own loader is a ceiling hit** (J-0).
  - `died_of_ceiling` reads glibc's `failed to map segment from shared object` (exit 127, under a ceiling) as the loader
    failing for memory, as it already does *cannot map*. On Debian and Ubuntu, bwrap's `libpcre2-8` is then what fails to
    load.
  - A unit test uses CI's exact line: `bwrap: error while loading shared libraries: libpcre2-8.so.0: failed to map segment
    from shared object`. With no ceiling set, the same line stays a prerequisite.
  - Say whether another loader sentence under a ceiling can still fall through to the `bwrap:` rule. If one can, make the
    rule *a ceiling was set and the loader (exit 127) failed* rather than a list of phrases. Name the cost of doing so.

- **M47 — small points and the container paragraph** (J-a – J-f; decisions 4, 5).
  - **The confined link test's comment** says the failure it really gives (`execvp …`), or the test plants the link
    inside a tree bound before it, so that it shows the production shape.
  - **SECURITY.md:**
    - The section becomes a heading, `### What a host needs for confinement`. The release-notes link, the image label and
      `config.rs`'s advice carry its anchor.
    - The container paragraph says that `seccomp=unconfined` opens the whole system-call surface to the server.
    - It names what can be added back, as measured by the architect's rehearsal: `--cap-drop ALL`,
      `--security-opt no-new-privileges`, `--read-only`. If they work, the release step runs with them too, so what is
      documented is what is tested.
    - The *operator who cannot accept that* sentence is rewritten, and the note to maintainers is dropped.
    - Podman's settings get `apparmor=unconfined`, or say they were measured on a host without AppArmor.
  - **The start refusal** says *bubblewrap cannot confine* once.
  - **The Dockerfile's** `ARG DOCUMENTATION_URL` and `LABEL` move after the package layer.
  - **The shimmed run** is `--no-fail-fast` too, and floor-check holds it.
  - **The vector's `bwrap_command` doc** says its paths are absolute and resolved.

## Readiness: M49 (2026-11-17; binding, before 0.3.0)

M47 is **Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-10-review-result.md`). What only a
measurement settles remains:

- **M49 — measure the container's add-backs, then run what is documented.**
  - **Measure first under podman,** and say what each addition did: `--cap-drop ALL`, `--security-opt no-new-privileges`
    and `--read-only`, with `--tmpfs /tmp` for the scratch secret.
  - **Add them to the release step's `docker run`.** The architect's rehearsal then measures docker's.
  - **Whichever works** is named in SECURITY.md's container paragraph as what an operator adds, and stays in the step.
    **Whichever fails** is dropped from both, with bubblewrap's reason recorded.
  - **SECURITY.md:** the paragraph *A server started without the means to sign still serves* gets its own heading,
    `### When the server cannot sign`, in place.

## M49 reviewed (2026-11-17)

M49 is **Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-corrections-11-review-result.md`). The release
rehearsal, run 44103325108, ran both images confined under docker on Debian 12 (amd64 and arm64), with every option
SECURITY.md names. One sentence remains, at the next touch of SECURITY.md (before 0.3.0):

- *What an operator adds back* should say that docker's run **was measured**: in that rehearsal, on Debian 12, amd64
  and arm64. The Debian 13 caveat then applies to the podman measurement only.

## prikk 0.58.0 (2026-11-18; binding, **before 0.3.0**)

prikk 0.58.0 was released on 2026-11-18. It ships our asks 3, 4 and 5 (`.git-exclude/upstream/prikk/receive/2026-11-18-prikk-0-58-0-released-your-asks-3-4-5-shipped.md`).
The architect ran the whole workspace on the 0.3.0 candidate (`3122e38`) against the published asset (sha256
`d2e2c389…`, the binary) and against a local build, with the same result on both: **3,082 passed, 23 failed**, all in
recovery (`inklkunn-core/tests/recovery.rs`, `inklkunn-signer/tests/recover.rs`, `inklkunn`'s `admin::tests`). Logs
are at the architect's; the causes are the three changes prikk made at our request. **One is a defect of 0.3.0
against the current prikk** (M50). The rest are tests that pin 0.57.0's behaviour (M51, M52). By the plan's rule, a
defect found during readiness is fixed in 0.3.0. **The cut waits for this package.** The version stays 0.3.0, and
the CHANGELOG heading stays as it is.

- **M50 — recovery cannot clear a stale lock on 0.58.0 (the defect).**
  - 0.58.0's `unlock --lock <path> --yes` refuses a lock whose holder's liveness is *unknown*: an empty lock, and a
    lock written by an older prikk, which recorded no start time. The second is every lock left before an operator
    upgrades prikk.
  - Recovery then stops at `clear-locks`. The wiki stays flagged, and the server's start says so: *recovery stopped at
    clear-locks: the lock was not cleared: prikk refused: precondition not met: …*. It fails closed, but the way out
    is a manual `prikk unlock --force`.
  - **Clear with `--force`, on the lease record's word,** which is already the only thing that decides (PL-8). prikk
    agrees: *"Your fail-closed handling is right, with `--force` on your own lease's word."*
  - **Every other guard stays:**
    - the lock is still held;
    - it is of a kind an acceptance leaves;
    - it is inside the wiki's own `.prikk`;
    - a foreign process in the repository stops every step.
  - **Measure `--force` on 0.54.0 (the floor), 0.55.0, 0.57.0 and 0.58.0** before relying on it. 0.55.0's help reads
    `[--yes|--force]`, so the two may be exclusive there. One rule across the range is preferred; if the range needs
    two, the rule is chosen by version and the choice is recorded.
  - Tests:
    - an empty lock and a lock in the older format are both cleared on 0.58.0;
    - the existing lock tests pass on every prikk in the range.
- **M51 — the cleanup-pending state on 0.58.0** (ask 4).
  - `status --format json` now exits 0 in that state, listing the queue, with `queue.already_sealed` =
    `{block_id, route}`. `doctor`'s way-out heading is new.
  - **Recovery's decision must be the same on 0.57.0 and 0.58.0:** doctor's cleanup-pending error, with the queue equal
    to the tip block's patches (PK-44).
  - Find why `a_pending_cleanup_is_not_finished_unless_doctor_names_exactly_that_and_the_block_is_the_acts` sees
    *another error beside it* on 0.58.0, and whether any product code depends on 0.57.0's wording or on its exit 1.
  - **Where 0.58.0 states `already_sealed`, it must agree** with the block that recovery found: the same `block_id`.
    If it disagrees, that is a stop, not a step. Read nothing else from it.
  - Assert the fixtures' preconditions per version: ≤ 0.57.0 refuses, ≥ 0.58.0 exits 0 with `already_sealed`.
- **M52 — the debris notice is `info` on 0.58.0** (ask 5).
  - `PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS` keeps its id. `is_metadata_debris` matches `Warning` only.
  - **Recognize it at either severity,** so that the plan's note and every decision read the same on both versions.
  - The tests assert per version.
- **M53 — `PRIKK-VERIFY-REF-PUBLICATION-IN-FLIGHT`** (`info`, exit 0: a publisher is still running).
  - Show that nothing in inklkunn treats it as a failure or acts on it. A parse-level test is enough.
  - prikk's residual (a `verify` during a running `sync seal` can fail `local-tag-trust`) does not apply if inklkunn
    never runs `sync seal`; say whether it does.
- **M54 — CI and the release.**
  - `PRIKK_TEST_VERSION` becomes **0.58.0** in `ci.yml` and `release.yml`, with sha256 (x86_64 tarball)
    `0462b9a590d0b945bb6266ce09983055768cb564415837bdb8ed462b5aa10dfe`. Check it yourself against prikk's published
    `.sha256`.
  - The floor stays 0.54.0. Update `EXPECTED_SKIPS` if it moves.
  - 0.57.0 stays in the architect's gates.
- **M55 — the record.**
  - The ledger: PK-44, PK-45 and PK-46 as shipped in 0.58.0; a new entry for the in-flight id and prikk's residual.
  - `docs/RECOVERY.md`, wherever it describes `unlock`'s behaviour by version.
  - One CHANGELOG entry under `## [0.3.0]` (entries, not the lead). The lead's runtime line stays *prikk 0.54.0 or
    later*, and it then holds for 0.58.0.

Hand back as `010-maintenance-2026-11-prikk-0-58-0-review-request.md`, with the whole workspace's numbers on 0.55.0
first, shimmed off, on 0.57.0 and on 0.58.0. **No letter to prikk now:** the release asks nothing of us. An
acknowledgement goes with the next letter.

## prikk 0.58.0 reviewed (2026-11-19)

M50 – M55 are **Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-prikk-0-58-0-review-result.md`), and
IS-20 is fixed. Every decision request is confirmed as the dev team chose:
- `--force` for every holder, on the lease's word;
- the witness errors, not a new named stop;
- the invented fixture text, labelled;
- the `info` ids left unhandled.

The handoff's bullet asking for an empty lock to be cleared was wrong, and the PK-45 rule stands. **Nothing is asked
for 0.3.0.** The architect runs CI and the rehearsal on the pushed tip, and brings the cut to the owner. Two small
points are recorded for 0.4.0 in the review result:
- `report_outcome` and a `refusal-v1` on a non-zero exit;
- a doctor line with an unknown severity word.

## 0.4.0's R-0: four small items (2026-11-19; binding)

0.4.0's plan is authorized (`docs/releases/0.4.0-plan.md`, the owner, 2026-11-19). Its P-3 runs four items recorded
for 0.4.0, now, before RFC 006 is accepted. **None of them adds a feature.** Each is reviewed as a correction is.

- **M56 — *Why is this here?* lists the sources attached in each change** (RFC 005 D-8 in full; recorded at the 0.3.0
  readiness review).
  - For each of a paragraph's two changes (the one that last changed it, and the one that first wrote it), list the
    source files that change added under `knowledge/`.
  - Each source is shown as its reference, as **text, never a link to the outside** (C-17a; threat model v0.3), with
    a link to its own source page in inklkunn.
  - **A change that attached none says nothing,** not *no source was given*.
  - *The proposal's Sources field* stays as it is: that section shows the proposal's own text, and is hidden when it
    is empty.
  - In both languages, the same in the API's provenance member, and in OpenAPI.
  - Tests:
    - a change that attaches two sources lists both;
    - a later change that touches the paragraph and attaches none lists none for itself;
    - a reference that holds markup or a bidirectional control is shown as text, or refused as at acceptance;
    - a restricted wiki answers as an absent wiki does (RC-5's test extended).
- **M57 — the signer reads a `refusal-v1` on a non-zero exit as prikk's refusal.**
  - `report_outcome` (`crates/inklkunn-signer/src/writes.rs`, used by `verify`, `worktree-status` and `status`) takes
    any stdout that begins with `{` as the report.
  - Check `schema_version` first: `refusal-v1` takes the refusal path, as the read driver's does (`driver.rs`), and its
    `message` is preferred over stderr.
  - Tests: a stand-in prints a `refusal-v1` document with exit 1 for each of the three verbs, and the answer is
    `Refused` naming its message, never `Damaged`.
- **M58 — a doctor line with an unknown severity is refused.**
  - In `parse_doctor_output` (`crates/inklkunn-signer/src/recover/text.rs`), a line of the shape `<word> [PRIKK-…]: …`
    whose word is not `error`, `warning` or `info` is now silently skipped.
  - Make it *unreadable `doctor` output*, naming the word.
  - Tests:
    - `critical [PRIKK-X]: …` is refused;
    - the three known words still parse;
    - real doctor text from 0.55.0, 0.57.0 and 0.58.0 still parses;
    - a line that is not of that shape is still not a finding.
- **M59 — the joined image tag is attested.** `gh attestation verify oci://ghcr.io/prikk-vcs/inklkunn:0.3.0` answers 404
  today, because only `0.3.0-amd64` and `0.3.0-arm64` are attested.
  - In `release.yml`'s `manifest` job, attest the manifest list's digest after the join, with `push-to-registry`. The
    job needs `id-token: write` and `attestations: write`.
  - **Run it in rehearsal too,** against the throwaway package's list (`inklkunn-rehearsal:rehearsal`), as the tarball's
    rehearsal attestation is labelled. Then the first tag run is not the step's first run. Rehearsal attestations may
    only name the rehearsal package.
  - `pin-check` keeps holding the shape: no step in rehearsal names the real image.
  - `docs/RELEASING.md`'s *Verifying a release* names the plain tag beside `-amd64`.
  - The architect's rehearsal on the result is the test.

Hand back as `010-maintenance-2026-11-r0-review-request.md`, with the whole workspace's numbers on 0.55.0 first,
shimmed off, on 0.57.0 and on 0.58.0. One CHANGELOG entry goes under `## [Unreleased]`.

## M60 — the Japanese catalog signed off (2026-11-19; binding, with R-0)

The owner signed off the Japanese catalog (DR-7), all 1,315 messages as of 0.3.0, and asked the architect for a final
check. That check is `.git-exclude/reviewed/ja-catalog-final-check-result.md`. **M60 applies it, and records the
sign-off, in one commit:**
- **The corrections:**
  - §1 (errors);
  - §2 (one rendering per meaning), with the new rows in `docs/glossary.md` §1 so that 0.4.0 takes them from there;
  - §3 (typography rules);
  - §4 (the adopted texts).

  For a rule, find every place by grep, and list the keys changed in the request. **`docs/glossary.md` §2's
  Japanese column changes with `ja.rs`**, because the test holds them equal.
- **The sign-off record:**
  - `JA_DEF.review` becomes `Review::Reviewed { by: "the owner, 2026-11-19" }`;
  - `ja.rs`'s module text, the glossary's status line and §4, the accessibility statement and the about page say so,
    with the one sentence of §6 about later messages;
  - from then on `Accept-Language: ja` selects Japanese. Test it, and test that a preview language still never is.
- **Tests that hold the rules:**
  - no ` さん` after a person placeholder;
  - no full-width colon;
  - a placeholder next to Japanese text is spaced, except inside 「」;
  - the §2 terms: for example, no 異議を唱え, no 争われ, no リビジョン, no 運用している人;
  - each added to the catalog tests, with a mutation.

Hand back with R-0, in `010-maintenance-2026-11-r0-review-request.md`.

## R-0 and M60 reviewed: three corrections (2026-11-21; binding)

M56 – M60 are **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-r0-review-result.md`), and
pushed. Every decision request is answered there. The corrections are one small round, and G-T1 may start beside it:
- **C-3, first, pushed alone when approved.** The kill matrix's *inside* rows tolerate 0.58.0's form of the state that
  0.57.0's `status` refused: prikk's `status` and `doctor` disagree, recovery fails closed, nothing is written, the
  wiki stays flagged, and the repository is kept for study.
  - Then soak the *inside prikk seal* rows on 0.58.0 until one lands there, and put that repository's `status`,
    `doctor` and `verify` output in the request.
  - Add a RECOVERY.md paragraph for the state.
- **C-1.** The accessibility sentence names *the owner of the inklkunn project* (inklkunn プロジェクトのオーナー), with a
  glossary row, in both languages as the result gives them.
- **C-2.** 機械 → 自動化ツール in `changes.machine_assisted*`, and 機械 joins the retired renderings.

Hand back as `010-maintenance-2026-11-r0-corrections-review-request.md`.

## R-0's corrections approved; M61 (2026-11-21; binding, before 0.4.0's candidate)

C-1 – C-3 are **Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-r0-corrections-review-result.md`). C-3 is
pushed alone first, then the rest. The soak found a deterministic state that recovery stops on today, on both prikk
versions, and that no operator can finish by hand: a `seal` killed after it published, before the ref log was written.
`doctor` names one `POINTER-LEADS-LOG` whose way out, `ref complete`, signs with the wiki's key. **M61, beside G-T1:**
- recovery plans K19's `complete-publication` for **the default view** when the leading block is the interrupted act's
  own block, under K19's witnesses (and on 0.58.0, `already_sealed` names that block);
- then the cleanup, as today;
- everything else stays a stop, and M51's witness error keeps only the true contradictions;
- the kill matrix's rows in that state recover fully, with a deterministic row added;
- RECOVERY.md and PK-49 follow.

The details and the tests are in the review result. Hand back as `010-maintenance-2026-11-m61-review-request.md`. A
longer kill-matrix soak with every failing log kept is part of 0.4.0's readiness (R-5), after M61.

## M61 reviewed: two corrections (2026-11-21; binding, beside G-T1)

M61 is **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-m61-review-result.md`) and pushed.
The step's name `complete-ref-publication` is accepted. *K19's `complete-publication`* above was the architect's slip:
K19's step is `complete-view-creation`, and `complete-publication` is P15's.
- **C-1.** The no-error-at-all witness error reads *… while prikk doctor names no error at all*, in `recovery/cleanup.rs`,
  C-3's helper and RECOVERY.md.
- **C-2.** The act's own view, not only *the default view*: the line above was too narrow, because a proposal may be
  accepted into another view.
  - Drop the default-view condition. The lead must still be the act's view.
  - Remove the clause from the stop's sentence and from RECOVERY.md.
  - Tests: a full recovery for an acceptance into a non-default view on a real prikk; the stop for a lead on another
    view; the mutation *the act's view not compared*.

Hand back as `010-maintenance-2026-11-m61-corrections-review-request.md`.

## M61's corrections approved; M62 (2026-11-21; beside G-T1, before readiness)

C-1 and C-2 are **Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-m61-corrections-review-result.md`) and
pushed. **M62:** widen the core integration world's acceptor to another view, then run `wiki recover` end to end on a
real prikk for a seal killed before its ref log on `heads/other`, and for P15's state there. Assert:
- `verify` green;
- one new block on that view, `heads/main` unchanged;
- the worktree as it was before the acceptance;
- the act ended and the flag cleared;
- a write afterwards on both views.

Fix any defect it finds in M62, and close the ledger's v1.11 parenthesis. The details are in the review result. Hand
back as `010-maintenance-2026-11-m62-review-request.md`.

## M62 reviewed: one correction (2026-11-22; binding, beside G-T1)

M62 is **Conditionally Approved** (`.git-exclude/reviewed/010-maintenance-2026-11-m62-review-result.md`) and pushed. The
defect it found is IS-21 (Low; fixed).
- **The worktree.** *The worktree as it was before the acceptance* above was wrong: an acceptance switches the worktree
  to its own view. The invariant is *what an uninterrupted acceptance leaves*, as the tests now assert.
- **C-1.** `restore_worktree` refuses, changing nothing, when the worktree's `reference` is not the view the step names,
  and the refusal names both. Tests: a real-prikk signer test with a mismatched view, and the mutation of removing the
  check. RECOVERY.md gets one sentence.

Hand back as `010-maintenance-2026-11-m62-corrections-review-request.md`.

