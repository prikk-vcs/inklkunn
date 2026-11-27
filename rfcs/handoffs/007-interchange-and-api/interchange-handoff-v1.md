# Interchange and API — Developer Handoff v1 (RFC 007; M5, 0.5.0)

| | |
|---|---|
| Document | A wiki created from a bundle with its history; the export as a level 3 bundle in fact; the interwiki map; the API's additions; the conformance self-assessment (RFC 007 D-1 … D-10). |
| Status | RFC 007 accepted by the owner 2026-11-26 (Q-1 – Q-5 as recommended, after the architect's philosophy check made eight refinements). **I-T1 is assignable now,** beside M63, which is handed back first. **I-T2 onward follow I-T1's approval,** and I-T4 – I-T6 may then run beside I-T2 – I-T3. The 0.5.0 plan's dates apply: built and reviewed by 2026-12-08 |
| Basis | `rfcs/accepted/007-interchange-and-api.md` (D-1 … D-10, the acceptance criteria 1 – 12, *How this was checked*); threat model v0.5 (T-5 with C-5a – C-5h, C-15g, C-15h, C-17k, INV-13, RR-17); requirements `IX-1`, `IX-3` – `IX-6`, `IX-8`, `IX-9`; wiki-commons 0.4 at `19f021c` (chapter 10 XFER-2, XFER-7, XFER-8, XFER-13; chapter 15 LIC-3 – LIC-5; chapter 14 SEC-7; chapter 16; `schemas/history-record.schema.json`, `schemas/bundle-manifest.schema.json`, `schemas/self-assessment.schema.json`) |
| Audience | The dev team. The two lenses of handoff 003 apply unchanged, and RFC 007's *How this was checked* is the reviewer's checklist. |
| Handback | One review request per group: I-T1; I-T2 – I-T3; I-T4 – I-T6. Each comes with the usual gates (the whole workspace on prikk 0.55.0 first, shimmed off, on 0.57.0 and on 0.58.0), a mutation table for each refusal and invariant, and every new message in both languages, listed for the owner (key, English, Japanese) |

**The rule above every task:** **nothing in the read path changes.** One source revision is one block (D-2), so
permalinks, *Why is this here?*, the writes index and RFC 006's hidden-text rule apply to imported history as they
are. A change to `read/` beyond a new caller is a question for the architect before it is code.

## I-T1 — The export's history records (D-7)

- **Each line of `history/<id>.jsonl` is a wiki-commons history record:**
  - `rev`: the block id;
  - `parent`: the previous record's `rev` in the same file, or `null` for the first;
  - `at`: the block's time, RFC 3339;
  - `type`: `create`, `edit`, `rename` with `from`/`to`, `delete`, or `revert` for an undo;
  - `author`: by `users.yaml`'s id; `summary`;
  - **`content`**: the page's full text at that revision, and its `sha256`;
  - **inklkunn's own facts under one `inklkunn` key:** `block`, `seq`, `kind`, who proposed and who accepted,
    `imported_from`, `message_readable`.
- **The text is read through `text_at`** (RFC 006 J-1), and never otherwise:
  - a hidden revision is `suppressed: [content]` with no `content` and no `sha256`, as a hidden summary or author is
    omitted today;
  - *not checked yet* makes the export wait for the index (bounded, said on the command's output) and then fail with a
    sentence, never write a gap.
- **Removed and renamed pages** get their files, by their last id. The manifest's count of changes without a page goes
  to zero for every page inklkunn made, and it stays as a count for what is not a page.
- **The manifest** keeps level 3, and `contents.history: true` is now true in substance.
- **The CHANGELOG's *Output changes*** lists the fields that moved or are new, for anyone who read 0.4.0's lines.
- **Tests:**
  - a shape test of every record against the schema's required fields and types (Rust, no Python);
  - a page edited three times, renamed and removed has five records in order, each with the right text;
  - a hidden revision has no `content`, and the next revision's `content` is whole;
  - a bundle is the same bytes twice from the same tip.
  - **Mutations:** the text read around `text_at`; a suppressed `content` kept; `parent` skipped.
- **Measure:** the export at 500 and 2,000 revisions (`tools/corpus`), with the time and the reads, in the request.
- **Run once and record:** wiki-commons' `validate_bundle.py` on a small export, and paste its output. Its two known
  quirks (string comparison of `at`; an error for `content` on a record with only `author` suppressed) are
  wiki-commons', and are named if they show.

Hand back as `007-interchange-i-t1-review-request.md`.

## I-T2 — The import's plan and report (D-1, D-4 – D-6), as a dry run

- **`wiki import-bundle <new-wiki> <bundle-dir> [--dry-run] [--bundle-license <SPDX>] [--leave-out-incompatible]
  [--no-history]`**, with `wiki create`'s options. An existing slug is refused, by a sentence that names
  `import-folder`.
- **The walk is `import-folder`'s** (`openat`, `O_NOFOLLOW`, identities pinned, the same path, name and size rules).
  Share the code; do not copy it.
- **The plan reads and judges everything before anything is written:**
  - the manifest's known keys only, with `fidelity` reported and not trusted;
  - checksums, strictly when present (missing, extra or wrong file: refused by name), and *none in the bundle* said;
  - `users.yaml` (an e-mail address refuses, naming the entry);
  - every history file: records by the schema's types; within a page, file order is history (a `parent` or `at` that
    breaks it refuses, naming the page and the `rev`; times compared as instants); `content` or `patch`, with
    `sha256` checked;
  - every revision's text judged as the folder import judges a page (frontmatter, the page cap, knowledge files by
    their reader);
  - **licences (D-4):**
    - the wiki's default is the manifest's `license`, or `--bundle-license` when there is none (refused without it);
    - a licence no wiki may have as its default refuses;
    - each page's and attachment's own licence is checked by chapter 15's matrix;
    - incompatible files refuse the run unless `--leave-out-incompatible`;
  - the revision cap and the bytes of `history/` (C-5e).
- **The report (D-6)** is printed by `--dry-run` and kept with the act. It lists:
  - the fidelity found;
  - the pages and attachments, with the revisions of each;
  - the people named;
  - the incompatible licences;
  - the records not replayed, by type;
  - the suppressions kept;
  - the files left out, and why;
  - the link prefixes that the map lacks, with counts;
  - the checksums;
  - the licence and where it came from;
  - the estimated time.

  Every name in it is escaped, and no byte of a refused file is repeated.
- **Fixtures (P-2):**
  - write a small MediaWiki XML dump and a small BookStack export **yourselves**, with no third-party content (CC0,
    said in their README). wiki-commons' `tests/test_formats.py` shows the shapes;
  - convert them with wiki-commons at `19f021c` (`python3 tools/wikicommons.py mediawiki import …`,
    `… bookstack import …`);
  - commit the source, the bundle and the exact command in `crates/inklkunn-core/tests/fixtures/interchange/README.md`.

  Python is never needed to run the tests.
- **Tests:** acceptance criterion 6's refusals, each on the dry run and on the run with nothing written, and the
  report's content for each fixture.
  - **Mutations:** each refusal removed; the checksums skipped when present; the file-order rule removed; the licence
    taken from the wiki instead of the bundle.

## I-T3 — The replay (D-1 – D-3), as *create wiki* extended

- **The import is a creation.** The creation mark (IS-13) is held from before the first directory to after the record
  is stored, and the record is stored last, in the same transaction as the act. Until then nothing serves, lists or
  links the wiki.
  - If something in the store needs the record earlier (the lease, the acts), say so in the request with the
    smallest way, and keep the property: **no reader can reach the wiki before it is complete.**
- **The first change** is the bundle's files that have no history (a level 2 bundle, or pages without text records),
  with the attachments. Then **one change per revision, one block each**, in D-2's order, through `SystemActs`
  (`Kind: import`, the system principal, the wiki's keys).
  - **Each message carries typed trailers:** the source `rev`, `at`, the author's name and kind, the summary, `minor`
    and `automated`.
  - Renames move, deletions remove, a restore or a revert writes its text.
  - Other types are not replayed, and are counted.
  - A record with suppressed content is not replayed. A suppressed author or summary is replayed, said as *hidden at
    the source*.
  - Last, if any page's final file differs from its last revision, one change *final state of the bundle*, listing the
    pages.
- **The words (acceptance criterion 8; new messages in both languages):**
  - the history row: *Imported. Originally by {name}, {date}: {summary}*, the date labelled *originally*;
  - the notice on the wiki's history and on each imported page: *Imported from {source} on {date}*, with the source's
    name and address as text, the checksums' state, and *licence declared by the importing admin* when it was;
  - a source author's name is never linked to or matched with an account, and is marked as the source's;
  - the participation counts leave `Kind: import` changes out (a test).
- **`wiki discard-unfinished <id>`:**
  - it removes exactly what `sweep_unfinished_creations` removes for a creation that never finished, and nothing else;
  - it is refused for a wiki whose record was ever stored, and for a creation whose process is alive;
  - the id must be typed again;
  - it is audited (`wiki-discard-unfinished`).

  The sweep's *left behind* line names this command.
- **Tests:**
  - acceptance criteria 1 – 3, and 7 with a kill at each step (the harness of IS-13's kill tests);
  - a reader's request during the import answers as an absent wiki does;
  - `discard-unfinished` on each refused case.
  - **Mutations:** the record stored before the history; two revisions in one block; a source name linked to an
    account; the trailers' author shown as a signer.
- **Measure** (acceptance criterion 12): imports of 500 and 2,000 revisions, on prikk 0.58.0, with the time per block
  at the start and the end. **Propose `INKLKUNN_IMPORT_MAX_REVISIONS`'s default** from it in the request. The architect
  decides it in the review, and `--no-history` is the way on.

Hand back I-T2 and I-T3 together as `007-interchange-i-t2-i-t3-review-request.md`.

## I-T4 — The interwiki map (D-8)

- **`governance/interwiki.yaml`:** prefix → template. It is read with the governance files' YAML subset and judged at
  acceptance like them:
  - `https:` with a host, no user information, `{title}` the only placeholder, in the path or the query;
  - prefixes of ASCII letters, digits and `-`, at most 32, and none equal to one of the wiki's namespaces;
  - unknown keys refused.

  A broken file refuses writes, as RFC 006 D-1 does.
- **A change to it** is a governance proposal at the policy-change rung, and `governance-mixed` applies.
- **What the map changes:**
  - the title index gives the map to the resolver, so links resolve as external (`rel="nofollow ugc"`);
  - a missing prefix stays dangling, as today;
  - the export's manifest and `site.json` publish it (replace the hard-coded empty maps);
  - the import writes the bundle's map, judged the same, with a failing template left out and listed.
- **Tests:** acceptance criterion 9, a mutation for each rule, and the rendering of a mapped and of an unmapped prefix.

## I-T5 — The API's additions (D-9)

- **`Link: <…>; rel="next"`** on every paged list, beside `next_cursor`.
- **New operations:**
  - `listProposals`: paged, filtered by state, each as `getProposal`'s summary;
  - `listProposalComments`: a hidden comment is its tombstone;
  - `/{wiki}/-/proposals.atom`: opened and decided, with the same hiding;
  - `site.json` names the OpenAPI document (`describedby`), and the head gains `service-desc`.
- **They show what the same person's pages show:** never a draft, a restricted wiki as absent, and the proposal page's
  own `authorize()` read. Compare each with the page for an anonymous visitor, a contributor and a moderator.
- **INV-13's surfaces test** gains both endpoints and the feed.
- **OpenAPI:** every addition is described with its problems, and the drift test still passes. **No rate-limit
  headers** (Q-4): IX-4's line in the requirements says why.

## I-T6 — The self-assessment (D-10)

- **`wiki-commons.yaml`** at the repository root, in chapter 16's shape (`subject`: inklkunn, its version,
  `kind: engine`).
  - Its profiles are `@0.4`: `core-reading`, `portable-content`, `living-history`, `accessible-authoring`,
    `many-languages`, `bundle-exporter`, `bundle-importer`.
  - **`yes` only where a named test shows the item.** Every `partial` or `no` gives its reason. Quote `"yes"` and
    `"no"`.
- **A test** reads it, checks the shape, and fails on a `yes` without a test name in its `note`.
- **`release.yml` attaches it** to the release, and the rehearsal shows it attached.
- The architect reviews every claim at readiness (R-6).

Hand back I-T4 – I-T6 together as `007-interchange-i-t4-i-t6-review-request.md`.

## After the build

Readiness (R-5) follows the plan:
- the end-to-end proof of the plan's criterion 4, on a real prikk;
- the upgrade from 0.4.0 against its release binary (pin its asset by checksum, as 0.3.0's);
- the CHANGELOG lead, with its *Output changes*;
- the report against criteria 1 – 12 and threat model v0.5's §7 lines;
- the Japanese list since `162f0cc`, in `.git-exclude/ja-review/` format.

Hand back as `0.5.0-readiness-review-request.md`.

## I-T1 reviewed: corrections X-1 – X-10 (2026-11-26; binding), and I-T2 assigned

**I-T1: Conditionally Approved** (`.git-exclude/reviewed/007-interchange-i-t1-review-result.md`), and pushed. An
independent review found no way for hidden content to leave in an export. **Corrections:**
- **X-1 (Medium):** tell *not yet* from *never*. Wait only while the index is short of the tip. Once it is at the tip,
  withhold a text that is unattributable for good: `inklkunn.withheld` with its reason, never `suppressed`, said and
  counted. One opaque block must not refuse every export.
- **X-2 (Medium):** `at` is the act's sealing time, then the source's (I-T3), then first-seen, with `inklkunn.at_from`.
  It never decreases within a file.
- **X-3:** a block whose effect is not listed is said in the file after it (`after_untraced`), and nothing is
  invented.
- **X-4:** a removal and a recreation with one id in one block is a rename; a move out of `pages/` and back is carried.
- **X-5:** `~` names escape upper-case letters.
- **X-6:** `first_seen` writes in chunks.
- **X-7:** a moderation act recorded during the export fails it, to be run again.
- **X-8:** the cap counts serialised bytes, and files are written one at a time.
- **X-9:** `sha256sums.txt`, named by the manifest; the import reads what the manifest names.
- **X-10:** two tests that fail under load (a recovery test on 0.58.0; the 0.2.0 upgrade wait), fixed without
  loosening them.

Hand back as `007-interchange-i-t1-corrections-review-request.md`. **I-T2 is assigned now** (the section above), beside
the corrections, built on X-1, X-2 and X-9's shape. It is handed back with I-T3 as
`007-interchange-i-t2-i-t3-review-request.md`.

## I-T1's corrections reviewed; I-T3 assigned (2026-11-26)

**X-1 – X-10: Approved** (`.git-exclude/reviewed/007-interchange-i-t1-corrections-review-result.md`), and pushed up to
`1b8e2c0`. I-T2's commits stay local until they are reviewed with I-T3.
- **The time of an imported change: the source's `at` wins over the seal** (decision 2). It is one line in `best_time`,
  with its test, in I-T3.
- **For I-T2 (in advance):**
  - **(a)** the *extra file* rule is about what is read. Every file the import reads must be in the checksums. A file
    it does not read (a converter's `import-report.md`) is listed as not imported and does not refuse.
  - **(b)** sidecar licences are shown by a fixture made by hand.
  - **(c)** the excluded MediaWiki history files in the package are accepted.
- **I-T3 is assigned now** (the I-T3 section above), and handed back with I-T2 as
  `007-interchange-i-t2-i-t3-review-request.md`.
- **First, alone: B-1** (the maintenance handoff's last section): `release.yml` does not load on `main`.

## I-T2 – I-T3 reviewed: Changes Requested, V-1 – V-12 (2026-11-26; binding, one round)

**Changes Requested for one High** (`.git-exclude/reviewed/007-interchange-i-t2-i-t3-review-result.md`). The rest is
sound: an independent review found no forged message, no path escape, nothing served before completion, and no removal
beyond scope. **Nothing is pushed** until this round is approved. B-1 goes out with it.
- **V-1 (High):** a removed page's history is never checked for its licence, so an incompatible revision is replayed
  and served under the wiki's licence. Check every history's licences, keyed by the history.
- **V-2 (Medium):** every file the replay reads is checked against the plan's digest (attachments, sidecars, knowledge
  files too), so *verified* is true.
- **V-3 (Medium):** the revision cap and a bound on patched bytes apply while the records are read, before any patch.
- **V-4 – V-8:** case collisions over revision paths; `at`'s upper bound; the bundle's digest identifies the bundle;
  two histories on one path at overlapping times; `discard-unfinished`'s description after its checks.
- **V-9 (the words):** *the wiki it was imported from* / インポート元, not *source* / 出典 (a claim's source). Glossary
  rows for バンドル, チェックサム, インポート元, and the twelve messages listed for the owner.
- **V-10:** a history that begins mid-way is accepted and said; a licence that is not an SPDX id names the near one.
- **V-11:** an id that cannot name a history file is named by its path, so a converted MediaWiki wiki's round trip
  keeps its history.
- **V-12:** `wiki create`'s usage text.
- **Decided:** `INKLKUNN_IMPORT_MAX_REVISIONS` defaults to **1,000**. The history is the page's notice for 0.5.0 (the
  page view's notice is recorded for later).

Hand back as `007-interchange-i-t2-i-t3-corrections-review-request.md`. **I-T4 – I-T6 may start now, beside it.**

## The V round and I-T4 – I-T6 reviewed: W-1 – W-10 (2026-11-26; binding, one round)

**The V round: Approved** (`.git-exclude/reviewed/007-interchange-i-t2-i-t3-corrections-review-result.md`), so I-T2
and I-T3 are approved. **I-T4 – I-T6: Conditionally Approved** (`.git-exclude/reviewed/007-interchange-i-t4-i-t6-review-result.md`).
An independent review found V-1, V-2 and the import-only guard real and complete. No unsafe link, hidden content or
restricted wiki can leak through the map, the lists, the feed or the `Link` header. **Pushed after the rehearsal of
this exact tree.**
- **W-1:** case collisions judged by lifetime, so a case-only rename imports.
- **W-2:** the CLI report says *the wiki it was imported from*.
- **W-3:** id-named history files that fold to one name are named by their paths.
- **W-4 (Medium):** one wiki, one map, read from the default view's tip for every view and state.
- **W-5:** the rule is symmetric: a page title whose namespace equals a mapped prefix is refused.
- **W-6 (Medium):** the policy form edits the map, in rows, never as raw YAML.
- **W-7:** `.` and `..` path segments of a title are encoded, and the doc is corrected.
- **W-8:** the guard compares the map's path exactly.
- **W-9:** the lists' and the feed's windows are in the store, and the feed selects by the event's time.
- **W-10:** the self-assessment is re-read after I-T4 and I-T5, and every `yes` note says no more than its test.

Hand back as `007-interchange-w-review-request.md`. Then **readiness (R-5)**, as *After the build* says, with the
Japanese list since `162f0cc`. **R-7, the candidate, also bumps `wiki-commons.yaml`'s `subject.version`.**

## The W round reviewed: the build is complete; readiness (R-5) assigned (2026-11-26)

**W-1 – W-10: Approved** (`.git-exclude/reviewed/007-interchange-w-review-result.md`). **RFC 007's build, I-T1 – I-T6, is
complete.** W-7's deviation is confirmed: a title with a `.` or `..` segment is no interwiki link, because the URL
standard reads `%2E%2E` as `..`.

**R-5, readiness, is assigned now:**
- the end-to-end proof of the plan's criterion 4, on a real prikk;
- the upgrade from 0.4.0 against its release binary, pinned by checksum (`277e7962…` for the x86_64 tarball);
- the CHANGELOG lead with its *Output changes*;
- the report against RFC 007's criteria 1 – 12 and threat model v0.5's §7 lines;
- the Japanese list since `162f0cc` in `.git-exclude/ja-review/` format, with the five term questions (バンドル,
  チェックサム, インポート元, Wiki 間リンクの対応表, 接頭辞);
- **Y-1:** the broken-map repair end to end, fixed if it is refused;
- **Y-2:** `web/history.rs`'s doc comments;
- **Y-3:** drop `interwiki_of`'s `view`.

Hand back as `0.5.0-readiness-review-request.md`. Then R-6 (the architect's review), R-7 (the candidate: the version,
the CHANGELOG heading **and `wiki-commons.yaml`'s `subject.version`**), R-8, R-9.

## Readiness reviewed (R-6): the Z round, then the candidate (2026-11-27)

**R-5: Approved, with R-6's one correction round** (`.git-exclude/reviewed/0.5.0-readiness-review-result.md`). The
threat model's §7.6 is written from the map. **Z-1 – Z-5 (binding):**
- **Z-1 (Medium):** a wiki with claims round-trips. Knowledge files go into the change at the end, after the history.
  What only a curation sets is not imported, said with the count. INV-11 is unchanged.
- **Z-2:** a real `SIGKILL` of a running `wiki import-bundle`, then the sweep, `discard-unfinished`, and a second
  import.
- **Z-3:** no web or API route imports, discards or creates a wiki (route enumeration).
- **Z-4:** the release notes say that attachments are not yet served, and where an imported page says it is imported.
- **Z-5:** a doc comment's pronoun.

Hand back as `0.5.0-readiness-corrections-review-request.md`. **Then R-7, the candidate:**
- the version 0.5.0;
- `## [0.5.0] — 2026-12-15` under an empty `## [Unreleased]`;
- `wiki-commons.yaml`'s `subject.version`;
- any comparison mask, named;

and nothing else, as `0.5.0-candidate-review-request.md`.

## The Z round reviewed: Z-6, then the candidate (2026-11-27)

**Z-1 – Z-5: Approved** (`.git-exclude/reviewed/0.5.0-readiness-corrections-review-result.md`). A wiki with claims
round-trips, a real kill is proved, and no route imports.
- **Z-6 (binding, before the candidate):** a retired claim or source is left out, counted and said, and no longer
  stops the import. A file that names one left out is judged as acceptance judges it, and refused by name if that
  refuses.
- **Then R-7, the candidate:**
  - the version 0.5.0;
  - `## [0.5.0] — 2026-12-15` under an empty `## [Unreleased]`;
  - `wiki-commons.yaml`'s `subject.version`;
  - any mask, named;

  and nothing else.

Hand Z-6 and the candidate back together, in two commits, as `0.5.0-candidate-review-request.md`.
- **Z-7 (added the same day, from the rehearsal):** the arm64 image's push failed with ghcr's `unknown blob` again (run
  45922584015, as at 0.4.0's release), and a rerun passed. Both push steps retry `docker push` up to three times with a
  short pause. It is handed back with Z-6 and the candidate, so the request has three commits.
