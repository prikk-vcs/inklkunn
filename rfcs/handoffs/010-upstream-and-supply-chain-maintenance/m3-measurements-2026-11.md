# M3 measurements: prikk 0.56.0 and 0.57.0 (November 2026)

| | |
|---|---|
| Document | What maintenance handoff M3 measured, so that the architect's ledger rows (PK-21, PK-26, PK-36, PK-39, PK-43) come from a table and not from prose. |
| Tree | `770f0be` plus M3's commits (named in the report that handed this back). **Section 8 repeats everything stated about 0.57.0 on the published release (M10, tree `d267267` plus M8 – M11); where it differs from sections 1 – 7, section 8 is the measurement and the earlier text is kept as what was said of the `bfd5b3c9` build.** |
| Method | One repository built the way the signer builds one (`init`, `trust maintainer add`, `commit`, `seal --allow-no-audit`, a second commit and seal, a view with `branch create`, a third commit left queued), and every verb the driver and the signer run, recorded on each version and compared after the ids, paths and times were normalised. The whole suite on 0.56.0 and on 0.57.0. |

## 1. The builds, and what each is

| Version | What was run | Provenance |
|---|---|---|
| 0.55.0 | `~/.pgtmp/rel-0.55.0/prikk` | the release asset (the version the with-prikk gate runs used until now) |
| 0.56.0 | `~/.pgtmp/rel-0.56.0/prikk` | the release asset; also the host's `prikk` on `PATH` |
| 0.57.0 (published, M10) | the host's `prikk` on `PATH` (`/home/nabbisen/.cargo/bin/prikk`, installed by the owner): `prikk --version` prints `prikk 0.57.0`; `sha256sum` of that binary: `6106dc4085040e51fa8bcc57eda8fac3f2ffa5015b58aad5789688d86644ed97` (**the binary's hash as installed, not the release asset's checksum**: the asset and its `.sha256` were not fetched here, so nothing below claims the asset's checksum or that the binary is the asset byte for byte; the version string and the tag `149a9e3d` are what the owner and the architect gave) | the published release, tag `149a9e3d` (2026-11-14); **section 8 is measured on it** |
| 0.57.0 (before publication) | a release build of the source at prikk `main` `bfd5b3c9` (`CHANGELOG.md` says 0.57.0, 2026-11-12), built offline with `--locked` into a scratch directory and **never installed** | **not a published asset**: prikk's checkout has no `0.57.0` tag, `crates.io` answers 404 for `prikk` 0.57.0, and the GitHub release asset was not fetched. The tree is four commits past the commit named "Release 0.57.0" (`ee13618e`); the later ones are the release preparation's P3b (the test harness) and P3c (plain `doctor` writes nothing again). **Every statement below about 0.57.0 is about that build**; the re-baseline must be repeated on the published asset before the floor is touched. |

## 2. prikk 0.56.0: every verb inklkunn runs

"Parsing" is what inklkunn reads from the output; "Affected" is whether anything it reads changed.

| Verb (who runs it) | 0.56.0 change | Affected | Evidence |
|---|---|---|---|
| `log --ref <view> --format json` (driver `history`; signer `view_blocks`, `tip`, `view_history`) | unchanged for a view | no | `probe` outputs identical after normalising; `heads/nonexistent` is still `precondition not met: ref ... does not exist`, exit 1 |
| `log --ref <block id>` / `<junk>` (no caller) | exit 1 with a reason, where 0.55.0 exited 0 with `"blocks": []` (**PK-36 fixed**) | no: the driver refuses a block point itself (`StorageError::InvalidArgument`) before it runs prikk, and the signer passes views only | `malformed_arguments_are_refused_before_prikk_runs`; new `a_log_that_prikk_0_56_refuses_is_an_error_and_never_an_empty_history` |
| `branch list [--all] --format json` (driver `views`/`all_views`; signer `all_views`) | **none**: `branch-list-v1` is byte-identical to 0.55.0's, with no signer field | no. **PK-26's "the signer in `branch --format json`" is not in 0.56.0, and not in 0.57.0 either** (measured, and absent from both changelogs) | normalised diff of `branch-list*.out`, 0.55.0 against 0.56.0 and against the 0.57.0 build: empty |
| `show <id> --format json` (driver `change`; signer `show`) | each patch gains `state` and `resolved_through_claim_id`; an unresolved node gains `reason` | no: the reader ignores unknown fields; the fields it reads (`patch_id`, `queued`, `operations`) are unchanged. `state` could replace `queued` later | new `the_fields_prikk_0_56_and_0_57_added_to_their_reports_are_read_past` |
| `extract --plan-only` / `extract` (driver `plan`, `materialize`) | `extract-report-v1` gains `chain` and `chain_signature` | no (same reason) | same test |
| `worktree-status --format json` (signer `worktree_clean`) | gains `behind_ref`; `changes` and `clean` unchanged | no. A worktree behind its ref still lists `missing` changes, so `clean` is still false | probe; prikk's changelog |
| `verify --format json` (driver `verify`; signer `verify`) | `unreached_objects[]` also lists patches and blobs; the verdict and exit code do not change | no: the driver reads the verdict and stages | probe with an unreached blob |
| `verify` text (signer `verify_run`, `sealed_block_signers`, `retirement`) | `unreached objects: N` counts patches and blobs, and a `warning: blob\|patch <id> ...` line each | no: the `sealed-block <id>: <key>` lines, `publication trust issues:` and `checked publication trust records:` are unchanged, and no new line has the `<condition> [PRIKK-...]:` shape the other-condition check looks for | probe |
| `doctor` (signer `doctor`, `parse_doctor_output`) | the unreached-object warning also names a blob or a patch, same code `PRIKK-DOCTOR-UNREACHED-OBJECT`, same recommendation (`the bytes stay; no command removes them yet`); one new `info [PRIKK-DOCTOR-HELD-LOCK]` per held lock | no: recovery already reads that warning as a note (ledger PL-12), and an `info` line blocks nothing | new `prikk_0_56_and_0_57_report_unreached_content_in_shapes_recovery_does_not_stop_at`; real `doctor` of a repository with an unreached blob |
| `unlock` listing (signer `locks`) | none in 0.56.0 (**the break is 0.57.0's, section 3**) | no | |
| `unlock --lock <path> --yes` (signer `unlock`) | none in 0.56.0 | no | |
| `commit`, `seal`, `branch switch`, `branch create --from`, `mv`, `trust ...`, `key ...`, `init`, `cat`, `tree`, `diff`, `status`, `archive export`, `ref complete --plan-only`, `doctor --discard-damaged-commits` | `commit` refuses to author over a path the worktree was never brought up to date for; `branch create --from`, `merge --into`, `sync seal` and `archive import --adopt` write the worktree and refuse a dirty one or a ref the queue holds work for; `archive export`'s `tool_version` | no: every flow the suite runs on a real prikk (the signer's lifecycle, view creation, recovery, rotation) passes on 0.56.0 | the whole suite on 0.56.0 |

The whole suite on the published 0.56.0, tree `770f0be`: **2,618 passed, 1 failed, 1 ignored**; the failure is M2's curl test, the same as on 0.55.0.

0.56.0's security fixes are in `bundle import`, `archive import` and `extract --claim`'s signature check. inklkunn runs `archive export` only and `extract` with `--ref`, so the floor does not have to move for them (the handoff's reading, confirmed).

## 3. prikk 0.57.0 (the build of `bfd5b3c9`, superseded by section 8): the whole suite

The suite on that build: **2,606 passed, 13 failed, 1 ignored**. Each failure:

| Failing test(s) | Why | What it is |
|---|---|---|
| `inklkunn-fetch real_vector::the_real_vector_runs_under_the_ceiling_...` | the host's curl starts under 1 MiB | M2's test, unrelated to prikk |
| `key_store::tests::the_production_open_uses_the_default_costs...`, `signer::tests::the_in_process_turn_is_given_up_only_after_the_file_lock_is_released` | timing under load (the suite ran beside builds); both pass alone on 0.57.0 | not prikk |
| `a_real_sigkill_during_branch_create_leaves_a_state_recovery_finishes_or_ends_whatever_it_is` (core `view_recovery`) | **a real product break.** 0.57.0 prints one more detail line in the `unlock` listing for a lock it wrote itself, `  recorded start: <time>`, between `recorded pid` and `liveness`. The strict reader refused the listing (`a lock's detail lines are not kind, recorded pid, liveness`), so **recovery could not read its witnesses** and the wiki stayed flagged | fixed in this change: the line is read past (one place, a non-empty value), a test per shape |
| nine more: `admin::tests::{recover_stops_on_what_no_verb_answers..., recover_shows_its_plan_and_changes_nothing..., a_start_recovers_every_wiki_...}`, core `recovery` `{a_lock_of_a_kind_an_acceptance_does_not_leave..., a_planted_stale_lock_is_cleared_..., a_seal_killed_between_publishing_and_draining_...}`, signer `recover` `{a_lock_the_plan_did_not_name_..., an_empty_lock_file_is_unreadable_..., a_stale_lock_is_cleared_on_the_record_...}` | **fixtures that were not what any prikk wrote.** They planted `pid=..`, `kind=..`, `note=planted` (or another note). 0.57.0 tells an old body from a cut one by the fixed last line `note=PR-007 lock has no stale-lock stealing yet`; any other note is an **incomplete body**, listed as `recorded pid: (not read: the lock body is incomplete)` | the fixtures now write 0.56.0's body byte for byte (0.55.0 and 0.56.0 read pid and kind only, so nothing changes for them). Two assertions that named prikk's words for such a lock (`appears running` / `does not appear`) now also accept 0.57.0's `unknown: written by an older prikk` (an old body records no start time); the empty-lock test accepts `(unparseable)` (0.55.0, 0.56.0) or `(not read: ...)` (0.57.0), and `docs/RECOVERY.md` says both |

After the fix the whole set of affected tests passes on 0.55.0, 0.56.0 and the 0.57.0 build alike.

Other 0.57.0 changes measured, none of which broke anything:

* `log --format json`: each block gains `parent_block_ids` and `sealed_by` (key ids as recorded, not as trusted).
* `verify --format json`: gains `block_seals` (every reached block with `sealed_by`, `trusted_by` and per-signature outcomes), `object_items`, `failed_objects`, `unverifiable_author_signatures`, `publication_trust_issues`, `non_ascii_paths_in_old_format`, `sealed_path_collisions`. **On a repository whose verification fails because a maintainer key was removed (the retired-key case, PK-34), `block_seals` is still printed (`sealed_by: ["m1"]`, `trusted_by: null`) while the text prints no `sealed-block` line at all.** That is what PK-39 asked for.
* `doctor` / `verify`: an unreached blob or patch is one `info [PRIKK-DOCTOR-UNREACHED-CONTENT]` line and `unreached content: N object(s) ...` (no longer a warning each); a block or ref state that nothing reaches keeps its warning.
* New verb `prikk uncommit <patch-id> [--plan-only]` (PK-43). Text only.
* **New repositories are format 8** (`.prikk/FORMAT` is `8`; 0.55.0 and 0.56.0 wrote `7`). **0.55.0 and 0.56.0 refuse a format-8 repository on every command** (`error: unsupported format version: 8`), measured. An existing repository is never upgraded as a side effect. So a wiki created under 0.57.0 cannot be opened by an older prikk, and an operator who rolls prikk back loses those wikis until they return to 0.57.0; the archive of a format-8 repository cannot be imported by 0.55.0 or 0.56.0.
* `unlock --lock <path> --yes` exits 1, leaving the lock, when the holder appears running (`--force` clears it as before). inklkunn's `unlock` step keeps `--yes`: a holder that prikk 0.57.0 can prove is running is a holder recovery must not clear, and the plan already stops on a prikk of somebody else's (`foreign-process`). The refusal is not exercised by a test, because it needs a lock body only a 0.57.0 prikk writes for a live holder in the same pid and time namespace; the planted old-style bodies read `unknown` on 0.57.0.
* `uncommit --plan-only` takes prikk's active lock, so it needs the repository writable (a read-only mount refuses it: `Read-only file system`), although it writes nothing.

## 4. What was adopted, and what the floor does

| | Adopted | How it behaves on 0.54.0 to 0.56.0 | On the 0.57.0 build | Needs a floor of 0.57.0? |
|---|---|---|---|---|
| C11's late-seal check reads `sealed_by` (PK-39) | yes: `verify --format json`'s `block_seals`, read strictly, where the report has the field | the report has no field, and `verify`'s text is read exactly as before (stand-in test for both shapes; the real-prikk tests expect each prikk's behaviour) | the check also runs **after a retirement** (the text printed no `sealed-block` line for a repository that does not verify) and finds a late seal by a removed key; `keys status` stops saying "this check cannot see <key>" | no |
| Recovery takes an orphan queued patch back on a person's word (PK-43) | yes: `inklkunn wiki recover <wiki> --discard-queued-patch <patch id>` | `uncommit` is `unknown command` (exit 2), which the signer reads as "no such verb": the plan says so, the word is ignored, the stop is unchanged | the plan shows prikk's preview and the command; with the id the patch leaves the queue (kept in prikk's recovery log, undoable), the worktree goes back to the tip, the flag clears | no |

**The floor does not move.** It moves only with a feature that needs it, and both features are absent below 0.57.0 rather than broken; neither needs a floor of 0.57.0. A floor of 0.57.0 would also need, and cannot have yet: the published release asset and its SHA-256 (CI downloads the asset and checks it against a checksum recorded in `ci.yml`; the tool `floor-check` makes `MIN_PRIKK_VERSION`, `PRIKK_VERSION` in `ci.yml` and `release.yml`, `ARG PRIKK_VERSION` in the `Dockerfile` and the recorded checksums one value), the signer's cost table re-measured (`BOUND_TO` is the floor and a test fails when they differ), `FLOOR` in `driver.rs`, and the documents that name 0.54.0. **When the floor does move to 0.57.0 it should move for a reason a person can state**, such as "the late-seal check must run after a retirement on every supported prikk" or "recovery must be able to take a patch back", and **it carries format 8**: an operator on the floor would create repositories no earlier prikk opens. Until then CI's floor stays 0.54.0 and the with-prikk gate runs of this change name their prikk (below).

## 5. Gates of this change (tree after the last M3 commit)

| Run | prikk | passed | failed |
|---|---|---|---|
| `cargo test --workspace --locked --no-fail-fast` | 0.55.0 | 2,644 | 1 (the curl test, M2) |
| the same | 0.56.0 | 2,644 | 1 (the same) |
| the same | the 0.57.0 build of section 1 | 2,644 | 1 (the same) |
| the same, prikk shimmed off `PATH` (as CI) | none | 2,644 | 1 (the same), **294 self-skips** |

The skip count was 289 (`EXPECTED_SKIPS`); this change adds **5** prikk-dependent tests: two in `inklkunn-core/tests/recovery.rs` (the take-back, and showing the plan with the word), two in `inklkunn-signer/tests/recover.rs` (the signer's guards and the undo; the empty queue and torn log), one in `inklkunn/src/admin/tests.rs` (the operator's word in the audit trail) -- so `EXPECTED_SKIPS` becomes 294. The new stand-in tests (`prikk_shapes.rs`, the `Writes` stand-ins, the plan's pure rules, the text readers) need no prikk and add none.

## 6. For the ledger rows (the architect's)

* **PK-36**: fixed in 0.56.0 as promised (`log --ref <block id>` and `log --ref <junk>` exit 1 with a reason; an existing ref that does not exist already exited 1). inklkunn needs nothing: the driver refuses a block id itself. Reproduced on 0.55.0, closed on 0.56.0.
* **PK-26**: 0.56.0 delivered the received-ref rollback refusal, the Unicode-path design round, `extract --claim` over a chain. **The signer in `branch --format json` did not ship in 0.56.0 or 0.57.0** (measured; `branch-list-v1` is identical on 0.55.0, 0.56.0 and the 0.57.0 build, and neither changelog names it). inklkunn reads no signer from `branch list`.
* **PK-39**: delivered by the 0.57.0 build (`log --format json` `sealed_by`, `verify --format json` `block_seals`, both "as recorded, not as trusted"); adopted by `block_seals` (section 4); the check now runs after a retirement.
* **PK-43**: delivered by the 0.57.0 build as `prikk uncommit <patch-id> [--plan-only]`: **the newest queued commit only**, healthy queue only, text only (no `--format json`), the undo `prikk doctor --recovery-restore <run>`; adopted as an operator-confirmed step (section 4). It needs the repository writable even for `--plan-only`.
* **PK-21**: 0.57.0 admits non-ASCII paths in format 8 (NFC; **a path not in NFC is refused at `commit`, not rewritten**, and U+200C is refused: section 8.2). inklkunn still percent-encodes (`title_to_path`); the migration to native paths is a separate decision and is untouched.
* **New (0.57.0)**: new repositories are format 8 and 0.55.0/0.56.0 refuse them; `unlock --yes` no longer clears a holder that appears running; the `unlock` listing gained `recorded start`; `unreached content` replaces the per-patch and per-blob warnings.

## 7. Decision requests (answered at the review of 2026-11-14; section 8 is the repeat on the published release)

1. **Is 0.57.0 published, and where is the asset?** prikk's checkout has no `0.57.0` tag and `crates.io` has no such crate (404); the commit named "Release 0.57.0" is followed by four preparation commits, two of which change behaviour (P3b the test harness, P3c plain `doctor` writes nothing again). Every 0.57.0 statement here is about a build of `bfd5b3c9`. Once the asset exists, repeat section 3 and the gate run on it; the floor is not touched before that.
2. **Format 8.** A wiki created under 0.57.0 cannot be opened by 0.55.0 or 0.56.0, and an operator who rolls prikk back loses those wikis until they return to 0.57.0. `prikk init` has no option to choose the format. Should the operator notes say that a wiki created under 0.57.0 needs 0.57.0 from then on (this change does not touch the notes), and should inklkunn record the repository format it created (`prikk_format_version` exists in the store) and refuse to open a wiki whose format its prikk cannot read with a message that says so?
3. **The floor.** Not moved (section 4). If the owner wants the floor at 0.57.0 for the late-seal check or the take-back, say which and when; the list of what changes is in section 4.
4. **`EXPECTED_SKIPS` 289 to 294** in `ci.yml` and `release.yml` (the coordinator's; section 5).

## 8. prikk 0.57.0, the published release: everything of sections 3 – 6 repeated (maintenance M10, 2026-11-15)

**What was run.** The host's `prikk` (`/home/nabbisen/.cargo/bin/prikk`; `prikk --version` prints `prikk 0.57.0`; the binary's `sha256sum` is `6106dc4085040e51fa8bcc57eda8fac3f2ffa5015b58aad5789688d86644ed97` -- the hash of the binary as installed, **not** the asset's `.sha256`, which was not fetched; the tag `149a9e3d` is the owner's and the architect's word, not something checked here). Repositories were built the way the signer builds one (`init`, `trust maintainer add`, `commit`, `seal --allow-no-audit` twice, a queued third commit) under that prikk, with one made by 0.55.0 (`~/.pgtmp/rel-0.55.0/prikk`) and the older builds beside it for the comparisons. Nothing was fetched from the network.

### 8.1 The whole suite

| Run | prikk | Tree | passed | failed | ignored |
|---|---|---|---|---|---|
| `cargo test --workspace --locked --no-fail-fast` | the host's 0.57.0 | `d267267` (before M8 – M11) | **2,686** | **0** | 1 |
| the same | the host's 0.57.0 | after M8 – M11 | **2,697** | **0** | 1 |
| the same | 0.55.0 first on `PATH` | after M8 – M11 | 2,697 | 0 | 1 |
| the same, prikk off `PATH` (as CI) | none | after M8 – M11 | 2,697 | 0 | 1 (**299 self-skips**, `EXPECTED_SKIPS` 298 + 1) |

**No test fails on the published 0.57.0.** Of section 3's thirteen failures on the `bfd5b3c9` build, the curl test is fixed by M2, the two timing tests passed alone then and pass now, the listing's `recorded start` line is read (M3) and the nine fixtures write what a prikk writes; nothing new broke between that build and the release.

### 8.2 Every statement of section 3 and 4, on the published release

| Statement made on `bfd5b3c9` | On the published 0.57.0 | Evidence |
|---|---|---|
| `log --format json` gains `parent_block_ids` and `sealed_by` | **holds** | the shape of `log` differs from 0.55.0's by exactly those two fields |
| `verify --format json` gains `block_seals`, `object_items`, `failed_objects`, `unverifiable_author_signatures`, `publication_trust_issues`, `non_ascii_paths_in_old_format`, `sealed_path_collisions` | **holds** | the same comparison for `verify` |
| `show` gains `state` and `resolved_through_claim_id`; `extract --plan-only` gains `chain` and `chain_signature`; `worktree-status` gains `behind_ref` | **holds** | shapes compared with 0.55.0's on a repository with a queued patch |
| `branch-list-v1` is identical to 0.55.0's and has no signer field (**PK-26 not delivered**) | **holds** | the shape of `branch list` and `branch list --all` differs from 0.55.0's in nothing |
| a retired maintainer key: `block_seals` is still printed (`sealed_by: ["m1"]`, `trusted_by: null`) while the text prints no `sealed-block` line | **holds** | `trust maintainer add` a second key, `remove` the first: the verdict fails on `publication-trust` alone, all 17 stages `evaluated`, `block_seals` lists both blocks with `trusted_by: null`, the text says `sealed blocks: 0` and no `sealed-block` line; a healthy repository prints `sealed-block <id>: m1` for each block |
| an unreached blob or patch is one `info [PRIKK-DOCTOR-UNREACHED-CONTENT]` line and `unreached content: N object(s) ...` | **holds** | after `uncommit`, `doctor` prints `unreached objects: 1`, the long `unreached content:` line, `info [PRIKK-DOCTOR-UNREACHED-CONTENT]` and `info [PRIKK-DOCTOR-VERIFY-OK]`, `errors=0, warnings=0`; `verify`'s text prints `unreached objects: 1` and the same line |
| `uncommit <patch-id> [--plan-only]`, text only | **holds** | `--format json` is `error: unknown uncommit argument: --format` (exit 2); the plan prints `commit <id> (sequence N on <ref>)`, `message:`, one line per operation, `plan only -- nothing written` |
| `uncommit` takes the newest queued commit only | **holds** | with two queued, the older id: `error: precondition not met: <id> is not the newest queued commit: 1 patch is queued after it (<newer>). ...` (exit 1) for the plan and for the real call |
| the undo is `prikk doctor --recovery-restore <run>` | **holds** | the real take-back prints `commit taken back; undo with `prikk doctor --recovery-restore 2cdb54646add2117` (it refuses once anything has changed since)`; running it queued the commit again |
| `uncommit --plan-only` needs the repository writable | **holds** | on a read-only bind: `error: i/o error: active lock: active/default/active.lock: Read-only file system (os error 30)`, exit 1 |
| new repositories are format 8 (`.prikk/FORMAT` is `8`); 0.55.0 and 0.56.0 wrote `7` | **holds** | `cat .prikk/FORMAT` on a repository made by each; a repository made by 0.55.0 (format 7) is opened by 0.57.0 and its `FORMAT` is still `7` after `verify` and `status` (never upgraded as a side effect; `prikk format upgrade` is the explicit way) |
| 0.55.0 and 0.56.0 refuse a format-8 repository on every command, `error: unsupported format version: 8` | **holds, and the words are exact** | stderr, exit 1, nothing on stdout, from both builds, for `verify`, `verify --format json`, `status --format json`, `log --format json`, `branch list --format json`, `doctor`, `unlock`, `worktree-status --format json`, `trust maintainer list`, `archive export`, `commit`, `seal`, `cat` -- the commands that do not open the repository (`key status`, `uncommit`, which they do not have) answer as before. 0.57.0 says `unsupported format version: 9` for a repository marked 9, and also `8` for a marker of `8` without its newline, so the sentence names what prikk read, not who wrote it |
| `unlock --lock <path> --yes` exits 1 and leaves the lock when the holder appears running | **holds (and is now exercised, which section 3 could not)** | a real `commit` of a 3 GiB sparse file holds `active/default/active.lock` and `containers/objects.lock`; `unlock` lists `recorded pid`, `recorded start: <n>` and `liveness: appears running -- do not clear: the process that took the lock is still running (same pid and start time)`; `unlock --lock <path> --yes` -> `error: precondition not met: the process that took <path> appears running (pid ..., start time ...) -- refusing to clear its lock. --yes does not clear a lock whose holder appears running. --force clears it anyway ...`, exit 1, the lock file still there |
| the `unlock` listing prints `recorded start: <time>` between `recorded pid` and `liveness` | **holds** | the listing above; `a_real_sigkill_during_branch_create_leaves_a_state_recovery_finishes_or_ends_whatever_it_is` reads it |
| a lock body with another `note=` than `PR-007 lock has no stale-lock stealing yet` is an incomplete body | **not measured again on its own**: the nine fixtures that write the 0.56.0 body byte for byte pass on the published release, which is all this repeat shows | the suite (8.1) |
| PK-21: 0.57.0 admits non-ASCII paths in format 8 (NFC, authored at commit) | **holds, with a correction to the words**: a path in NFC is committed; **a path not in NFC is refused, not rewritten** (`cafe\u{301}.txt: this name is not in Unicode normalization form C (NFC), the only form prikk records, and prikk never rewrites a name for you; its NFC spelling is caf\u{e9}.txt`), and U+200C is refused (`this version of prikk does not accept U+200C (invisible format character) in a repository path`) | `commit` on each |
| PK-39 and PK-43 are delivered | **holds** | the rows above |

### 8.3 PK-39: is `block_seals` whole when `verify` fails? (M8)

**No.** Measured on the published release over the repository damaged in 42 single-file ways three times over (each file of `.prikk/` garbled, deleted and truncated), eight directory-level ways (deleted, or replaced by a file) and seven multi-file ones, on a repository of two sealed blocks. The list `block_seals` is **not** filled independently of the other checks:

| What was damaged | The verdict | Stages | `block_seals` |
|---|---|---|---|
| nothing | holds | 17 evaluated | 2 of 2 |
| a retired key; `trust/keys.container`; `trust/policy-*` (garbled, deleted, truncated); `.prikk/trust/` deleted | fails on `publication-trust` alone | 17 evaluated | **2 of 2, whole** |
| `active/default/` replaced by a file (the WAL cannot be replayed) | fails on `stage-failure` | `wal-replay` failed, seven stages `not_evaluated` behind it | 2 of 2 (whole here, but nothing in the report shows it) |
| `containers/block/a.container` or `patch/a.container` garbled or deleted | fails on `item-failure`, `lifecycle-cache-divergence` | **all 17 evaluated** | **0 of 2** |
| `containers/block/a.container`, `patch/a.container` or `blob/a.container` truncated; `blob/a.container` garbled or deleted | fails on `item-failure` (and `lifecycle-cache-divergence`) | **all 17 evaluated** | **1 of 2** |
| `refs/containers/log-a.container` (the ref log) garbled or deleted | fails on `item-failure` | all 17 evaluated | 2 of 2 (an `item-failure` does not always shorten the list) |
| `refs/containers/pointer-index-a.container` | fails on `item-failure` | **all 17 evaluated** | **0 of 2** (truncated: `blocking-ref-publication`, 1 of 2) |
| `containers/ref-state/a.container` garbled, deleted or truncated; `containers/index.container` deleted | fails on `stage-failure`, `item-failure` | `local-tag-trust` failed | **0 of 2** |
| `refs/` garbled | fails on `stage-failure`, `item-failure` | `objects`, `refs`, `received-refs`, `local-tag-trust` failed; `ref-update-schema-trust`, `publication-reclassification` `not_evaluated` behind `refs` | **0 of 2** |
| `refs/` deleted (no ref reaches any block) | **holds** | 17 evaluated | **0 of 2**, the two blocks listed under `unreached_objects` |

So a failing `verify` can give a **short or empty list with every stage `evaluated`**, which `is_partial` (a stage that did not run) cannot see, and which, read as the list of who sealed what, says *no late seals* of a repository whose blocks could not be read. The decision (M8): the list is used only where the report is whole -- every stage `evaluated`, and the verdict holds or fails on `publication-trust` alone -- and anything else is *unavailable*, with the text, which lists less, no fallback. The one case in the table where the list was whole beside an unevaluated stage (`wal-replay`) is refused too; nothing in the report tells that case from the others. The `refs/` deleted case (a verdict that holds and an empty list) is a repository with no views, where the late-seal check has no history to compare and the unreached blocks are not part of one.

### 8.4 PK-43 and the cost of `uncommit` (M9)

Re-measured on the published release, the same method (the smallest `RLIMIT_DATA`, `prlimit --data`, bisected to 256 KiB over a fresh copy of the repository per probe with new modification times, `--plan-only` and the real take-back alike -- they needed the same in every shape):

| Shape | Store | Smallest working limit | Against `5.0 x S` (ceiling 1.5 x) |
|---|---|---|---|
| 3,000 pages (350 bytes), a few bytes queued | 3.2 MiB | 5.5 MiB | holds |
| 20,000 pages (1,900 bytes), a few bytes queued | 51.1 MiB | 35.0 MiB | holds |
| a sealed 64 MiB binary attachment (also as text, and as zeros), a few unrelated bytes queued | 64.0 MiB | **under 256 KiB** (it works at the smallest probe) | holds |
| the queued patch adds a 64 MiB file | 64.0 MiB | 128.2 MiB (2.0 x S) | holds |
| an edit of one page, in the small and the large store | 3.2 and 51.1 MiB | 5.2 and 35.0 MiB | holds |
| **the queued patch is an edit of a sealed 64 MiB text file** (64 K lines of 1 KiB, and 8 M lines of 8 bytes: the same) | 64.1 MiB | **577.0 MiB (9.0 x S)** | **does not hold** (ceiling 480 MiB) |

The pre-release build had been measured at 320 MiB (5.0 x S) for "a 64 MiB attachment in an earlier block and a few bytes queued"; the published release needs under 256 KiB for that and 9.0 x S for an edit of the large file, so the shape of the earlier figure was not reproduced (a different shape, or a different build). The signer's rule is therefore **9.0 x S, floor 16 MiB** (ceiling 1.5 x), which holds all six rows. `BOUND_TO` is the floor (0.54.0) and does not move.

### 8.5 Gates of M8 – M11

The counts of 8.1. New prikk-dependent tests: **one** (M8's real-prikk check that a block container prikk cannot read makes the late-seal check unavailable), so `EXPECTED_SKIPS` becomes 299; M9 and M11 add none.

### 8.6 What is still not measured

* The release asset's own checksum: the asset and its `.sha256` were not fetched (there is no instruction to fetch them in this task); the host binary's hash above is the installed file's.
* A live 0.57 holder of a lock in a *different* pid namespace, and `unlock --yes` against a holder that is `unknown`: the planted old-style bodies read `unknown`, as in section 3.
