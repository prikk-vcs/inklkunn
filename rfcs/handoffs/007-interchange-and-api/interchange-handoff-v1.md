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
