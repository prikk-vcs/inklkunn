# M3 measurements: prikk 0.56.0 and 0.57.0 (November 2026)

| | |
|---|---|
| Document | What maintenance handoff M3 measured, so that the architect's ledger rows (PK-21, PK-26, PK-36, PK-39, PK-43) come from a table and not from prose. |
| Tree | `770f0be` plus M3's commits (named in the report that handed this back). |
| Method | One repository built the way the signer builds one (`init`, `trust maintainer add`, `commit`, `seal --allow-no-audit`, a second commit and seal, a view with `branch create`, a third commit left queued), and every verb the driver and the signer run, recorded on each version and compared after the ids, paths and times were normalised. The whole suite on 0.56.0 and on 0.57.0. |

## 1. The builds, and what each is

| Version | What was run | Provenance |
|---|---|---|
| 0.55.0 | `~/.pgtmp/rel-0.55.0/prikk` | the release asset (the version the with-prikk gate runs used until now) |
| 0.56.0 | `~/.pgtmp/rel-0.56.0/prikk` | the release asset; also the host's `prikk` on `PATH` |
| 0.57.0 | a release build of the source at prikk `main` `bfd5b3c9` (`CHANGELOG.md` says 0.57.0, 2026-11-12), built offline with `--locked` into a scratch directory and **never installed** | **not a published asset**: prikk's checkout has no `0.57.0` tag, `crates.io` answers 404 for `prikk` 0.57.0, and the GitHub release asset was not fetched. The tree is four commits past the commit named "Release 0.57.0" (`ee13618e`); the later ones are the release preparation's P3b (the test harness) and P3c (plain `doctor` writes nothing again). **Every statement below about 0.57.0 is about that build**; the re-baseline must be repeated on the published asset before the floor is touched. |

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

## 3. prikk 0.57.0 (the build of section 1): the whole suite

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
* **PK-21**: 0.57.0 admits non-ASCII paths in format 8 (NFC, authored at `commit`). inklkunn still percent-encodes (`title_to_path`); the migration to native paths is a separate decision and is untouched.
* **New (0.57.0)**: new repositories are format 8 and 0.55.0/0.56.0 refuse them; `unlock --yes` no longer clears a holder that appears running; the `unlock` listing gained `recorded start`; `unreached content` replaces the per-patch and per-blob warnings.

## 7. Decision requests

1. **Is 0.57.0 published, and where is the asset?** prikk's checkout has no `0.57.0` tag and `crates.io` has no such crate (404); the commit named "Release 0.57.0" is followed by four preparation commits, two of which change behaviour (P3b the test harness, P3c plain `doctor` writes nothing again). Every 0.57.0 statement here is about a build of `bfd5b3c9`. Once the asset exists, repeat section 3 and the gate run on it; the floor is not touched before that.
2. **Format 8.** A wiki created under 0.57.0 cannot be opened by 0.55.0 or 0.56.0, and an operator who rolls prikk back loses those wikis until they return to 0.57.0. `prikk init` has no option to choose the format. Should the operator notes say that a wiki created under 0.57.0 needs 0.57.0 from then on (this change does not touch the notes), and should inklkunn record the repository format it created (`prikk_format_version` exists in the store) and refuse to open a wiki whose format its prikk cannot read with a message that says so?
3. **The floor.** Not moved (section 4). If the owner wants the floor at 0.57.0 for the late-seal check or the take-back, say which and when; the list of what changes is in section 4.
4. **`EXPECTED_SKIPS` 289 to 294** in `ci.yml` and `release.yml` (the coordinator's; section 5).
