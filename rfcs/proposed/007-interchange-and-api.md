# RFC 007 — Interchange and API: a wiki that comes in with its history, and leaves with it

**Status.** **Proposed to the owner 2026-11-25.** This is 0.5.0's R-1 (`docs/releases/0.5.0-plan.md`, authorized
2026-11-25 with P-1 – P-5). **Tracks.** ROADMAP M5 (0.5.0), theme T7. Requirements `IX-1`, `IX-3`, `IX-4`, `IX-5`,
`IX-6`, `IX-8`, `IX-9`; AC-M5; threat model `T-5` (C-5a – C-5c), `INV-13`; wiki-commons 0.4 (`19f021c`) `XFER-2`,
`XFER-7`, `XFER-8`, `XFER-13`, `MKUP-13`, `LIC-4`, `LIC-5`, `SEC-7`, chapter 16. It also takes up what RFC 003 D-12,
RFC 004 and RFC 005 part 2 left to *RFC 007's* bundle import. **Touches.**
- `inklkunn-core`: the bundle import, the export's history, the interwiki map;
- `inklkunn-signer`: the replay's changes, through the existing system path;
- `inklkunn-web`: the API's additions and the proposals feed;
- `inklkunn`: the command;
- the release workflow: the self-assessment.

**Not here** (the plan, section 2):
- replaying into a wiki that has pages (P-1);
- pseudonymizing, a dry run and a scope for export;
- discussions (fidelity level 4);
- native links between wikis;
- webhooks;
- relicensing as an event;
- the operations work (0.6.0).

## Summary

wiki-commons' converters already turn a MediaWiki XML dump into a Portable Wiki Bundle **at fidelity level 3**, with
every revision's full text, its author, time and summary, and a BookStack export into one **at level 2**, which has no
history because BookStack's format carries none.
- **inklkunn reads neither today.** `wiki import-folder` brings a bundle's pages in as **one change**, and lists its
  `history/`, `users.yaml` and licences as *not imported*.
- **This RFC adds `wiki import-bundle`.** Into a wiki that has no pages yet, it brings in the pages, the attachments
  and, **when the bundle has them, every revision as a change of its own, in order**. Each change is signed by the
  wiki, as every change is, and names its original author and time as data. Licences are checked, checksums verified,
  and a report says what came in and what did not.

**inklkunn's own export becomes such a bundle too.** It claims level 3 today, but its history lines lack the two fields
the history record requires (`rev`, `at`), and carry no revision's text. After this RFC, the history lines carry both,
and renames and removals become history records. So an inklkunn wiki exported and imported again keeps its history.

The rest is small: the interwiki map, four API additions, and the conformance self-assessment.

## Decisions

- **D-1 — `wiki import-bundle <wiki> <bundle-dir>`, into a wiki with no pages** (P-1).
  - **The wiki:** it must have no change after its creation. A wiki with pages is refused, by a sentence that names
    `import-folder` for adding pages as one change.
  - **The bundle:** a directory, as `import-folder` reads one. A ZIP or a tar is unpacked by the operator first. That
    is one less parser facing untrusted input.
  - **A dry run first:** `--dry-run` is the plan (D-6's report), and writes nothing. The run is the same plan, checked
    again on the bytes read, as `import-folder` does.
  - **It is an admin's act at the command line, under the wiki's write lease,** recorded and audited as one act
    (`wiki-import-bundle`). It is not on the web or in the API.
  - **The wiki is *importing* while it runs.** It refuses every write and answers every read with *being imported*,
    so no one reads half a history.
    - **On success it opens.**
    - **On failure it stays closed,** with the reason, and the operator removes it and imports again. There is no
      resume: a wiki that no one has read can be made again (XFER-14's *resumable* is left for later).
- **D-2 — One source revision is one change, and one block.**
  - **The order:** every revision of every page, by `at`, oldest first, with ties kept in the file's order and pages
    in path order. **The same bundle always gives the same history.**
  - **Each revision writes its page's full text** (`content`). A record with a `patch` and no `content` is applied to
    the text before it, and its `sha256` is checked. A mismatch refuses the import, naming the page and the `rev`.
  - **Why one block each:** the read path decides per block (permalinks, *Why is this here?*, the writes index and its
    hidden-text rule, INV-13). Two revisions of one page in one block would make *the last writer at block X*
    ambiguous. Nothing in the read path changes.
  - **Renames** (`type: rename`, `from`/`to`) move the page. **Deletions** remove it. **A restore or a revert** writes
    the text it carries. Other types (`protect`, `review`, `merge`, `fork`, an engine's own) are **listed in the report
    and not replayed**: they are the source's acts, not page texts.
  - **After the history,** the bundle's files at their final state are compared with the replayed result. A page whose
    final file differs from its last revision gets one last change, *final state of the bundle*, so the wiki ends
    exactly at the bundle's files.
  - **A page with no history** (a level 2 bundle such as BookStack's, or a page whose records carry no text) comes in
    with the attachments **in one first change**, before the replay, and the report says so.
  - **The cost is prikk's, per block** (ledger PK-50: building a history through the signer measured 154 s for 500
    blocks and 1,438 s for 2,000). So:
    - **a cap on revisions per import** (`INKLKUNN_IMPORT_MAX_REVISIONS`). Its default is set from R-4's measurement at
      500 and 2,000 blocks, and the handoff names it;
    - **a bundle over the cap is refused at the plan**, with its count, the cap, and the way on: `--no-history` imports
      the final state as one change (level 2);
    - prikk 0.60.0's `show` fix is re-measured at its re-baseline, and the default moves only on a measurement.
- **D-3 — Who wrote it, said as data, never as a signature** (LIC-4).
  - **Every replayed change is the wiki's change** (`Kind: import`, the system principal, the wiki's keys, through
    `SystemActs` like `import-folder`).
  - **Its message carries the source's facts as typed trailers:** the `rev`, the time `at`, the author (name and kind,
    from `users.yaml` or the record), the summary, `minor` and `automated`.
  - **The history and the page say it in words:** *Imported. Originally by {name}, {date}: {summary}*. A trust label is
    never shown for the original author: inklkunn verified nothing they did.
  - **No account is made** for a source author. Their name is text, escaped as every name is.
  - **The source's suppressions are kept** (XFER-7, `suppressed`):
    - a record whose **content** is suppressed is **not replayed** (there is no text), and the report counts it;
    - a record whose **author or summary** is suppressed is replayed with that part said as *hidden at the source*.
  - **The personal data is permanent** in the repository's history (the removal procedure is 0.6.0's). The dry run's
    report lists how many people the history names, and SECURITY.md says it. IP addresses are already replaced by the
    converter (`~ip-…`), and a `users.yaml` entry with an e-mail address refuses the import, naming the entry.
- **D-4 — Licences are checked, and kept as they are** (LIC-5, LIC-3).
  - **Each page's licence** is its frontmatter `license`, else the manifest's `license`. **Each attachment's** is its
    sidecar's `license`, else the manifest's.
  - **Each is compared with the wiki's default licence** by wiki-commons' matrix (chapter 15):
    - CC0 goes into any wiki;
    - CC BY goes into BY or BY-SA;
    - BY-SA goes only into BY-SA;
    - NC, ND, GFDL and an unknown or missing licence go into none.
  - **The dry run lists every incompatible file** with its licence and the wiki's.
    - **The run refuses** while there is one, unless the admin passes `--leave-out-incompatible`. That flag leaves those
      pages (with their history) and attachments out, and the report names each.
    - **There is no override that imports them** (Q-2).
  - **A licence is never rewritten.** A page keeps its own `license` line, so a CC0 page in a BY-SA wiki still says
    CC0.
- **D-5 — The bundle is untrusted, all of it** (SEC-7; C-5a – C-5c made true).
  - **The walk is `import-folder`'s:** no link followed, identities pinned, the same path, name and size rules.
  - **Every file the bundle names is checked against its checksums file**, and so is every file that it does not name.
    A missing, extra or wrong file refuses the import, by name. **A bundle without a checksums file is refused** unless
    `--without-checksums`, and the report then says *not verified*.
  - **Each page text is judged as the folder import judges it,** at every revision, not only the last: frontmatter, the
    page cap, knowledge files by their reader.
  - **Bounds on the history:** the revision cap (D-2), the page cap per revision, a cap on the total bytes of
    `history/`, one history file per page, and record fields read by the schema's types (an unknown field is kept in
    the report, never interpreted).
  - **The manifest is read for its known keys only.** Its fidelity is read and reported, but the import trusts what it
    finds, not what is declared. Level 4 and 5 content (`discussions/`, structured data) is listed as not imported.
  - **Nothing is fetched and nothing is run:** no URL in a bundle is visited, and no converter is called.
- **D-6 — The report** (XFER-13) is the plan, printed by the dry run and kept with the act:
  - the fidelity found;
  - the pages and attachments, with the revisions per page;
  - the people named;
  - the incompatible licences;
  - the records not replayed, by type;
  - the suppressions kept;
  - the files left out, and why;
  - the time the replay is estimated to take, from D-2's measured cost.

  Every name in it is escaped, and no byte of a refused file is repeated.
- **D-7 — The export becomes a level 3 bundle in fact** (IX-1, XFER-7).
  - **Each history line is a wiki-commons history record:**
    - `rev`: the block id and the path;
    - `at`: the block's time;
    - `type`: `create`, `edit`, `rename` with `from`/`to`, `delete`, or `revert` for an undo;
    - `author` (by `users.yaml`'s id), `summary`;
    - **`content`, the page's full text at that revision**, and its `sha256`.
    - inklkunn's own facts (`block`, `seq`, `kind`, who accepted, `imported_from`) move under one `inklkunn` key, which
      an importer may ignore.
  - **A page's text is read through the one reader** (`text_at`, RFC 006 J-1), so **what moderation hides is never in
    an export**: a hidden revision is a record with `suppressed: [content]` and no `content`, as today.
  - **An export that meets *not checked yet* waits for the index to settle** (bounded, and said), and is never written
    with a gap.
  - **Removed and renamed pages get their records too,** in `history/<id>.jsonl` by their last id, so the counts in
    the manifest's report go to zero for any page inklkunn made.
  - **The interwiki map** is the wiki's (D-8).
  - **The cost is one read per revision.** It is measured at R-4, and the command says how many revisions it is
    writing.
  - **The checks:** the export of the end-to-end wiki passes wiki-commons' own `validate_bundle.py`, run at readiness
    and recorded (it needs Python and PyYAML, which CI does not install). A Rust test holds the record's shape.
- **D-8 — The interwiki map: `governance/interwiki.yaml`** (IX-9, MKUP-13).
  - **The format:** a map of prefixes to URL templates, with **`{title}` the only placeholder**, substituted
    percent-encoded.
  - **Every template is `https:`,** with a host, no user information, and `{title}` only in the path or the query
    (Q-3).
  - **Prefixes** are ASCII letters, digits and `-`, at most 32 characters, and none may be one of the wiki's own
    namespaces.
  - **It is a governance file,** so a change to it is a proposal that needs the policy-change rung (RFC 006 D-1;
    maintainer by default). A map redirects every link that uses a prefix at once, which is more than one page's edit.
    **It is validated at acceptance** like the other governance files, and a file of the wrong shape makes the wiki
    refuse writes, as D-1 of RFC 006 says.
  - **What the map changes:**
    - links resolve through it, rendered as external links (`rel="nofollow ugc"`, already so);
    - a prefix that the map does not have stays a dangling link, as today;
    - the export's manifest and `site.json` publish it;
    - an import writes the bundle's map into this file, judged as above. A template that fails is left out and
      listed.
- **D-9 — The API's additions** (IX-4, IX-5, IX-6).
  - **`Link: <…>; rel="next"`** (RFC 8288) on every paged list, beside the body's `next_cursor`, which stays.
  - **`listProposals`:** a wiki's proposals, paged, filtered by state, each as `getProposal`'s summary.
  - **`listProposalComments`:** a proposal's comments in order. A hidden comment is its tombstone, as the page shows
    it: INV-13's list gains both endpoints, and a test is written for each.
  - **A proposals feed:** `/{wiki}/-/proposals.atom`, opened and decided, with the same hiding.
  - **`site.json` names the OpenAPI document** (`describedby`), and the page head gains `service-desc`.
  - **Rate-limit headers: not in 0.5.0** (Q-4). A refusal already carries `Retry-After` (RFC 9110). IX-4's *rate-limit
    headers* waits until the IETF's header fields are published as an RFC.
- **D-10 — The self-assessment:** `wiki-commons.yaml` at the repository root, in chapter 16's shape (`subject`: the
  engine, its version).
  - **It claims profiles `@0.4`** (`core-reading`, `portable-content`, `living-history`, `accessible-authoring`,
    `many-languages`, `bundle-exporter`, `bundle-importer`), **only `yes` where a named test shows the item**. Each
    `partial` or `no` gives its reason.
  - **A test** reads it, checks the shape, and fails if an item says `yes` without a test name.
  - **`release.yml` attaches it** to each release. The architect reviews every claim at readiness.

## What "done" means (acceptance criteria)

1. **A MediaWiki dump comes in with its history:**
   - every revision is a block, in D-2's order;
   - each names its author and time as D-3 says;
   - the final pages equal the bundle's files;
   - a suppressed revision in the dump is kept as D-3 says.

   This is shown on a converter-made fixture, pinned with its command (P-2).
2. **A BookStack export comes in** as pages and attachments with their sidecars' licences, in one change.
3. **An inklkunn wiki exported and imported again** has the same pages, the same history (renames and removals
   included), and the same authors, said as D-3 says.
4. **The export passes `validate_bundle.py`** (recorded at readiness), and the record's shape test passes.
5. **A hidden revision is never in an export** (D-7). The surfaces test of INV-13 includes the export's `content` and
   both new endpoints and the feed.
6. **Refusals, each by name, with nothing written:**
   - an incompatible licence (and `--leave-out-incompatible` leaves exactly those out);
   - a checksum that lies, a missing file and an extra file;
   - a `sha256` that does not match its `content`;
   - a link, a path over prikk's caps, a page over the cap at any revision;
   - a bundle over the revision cap (and `--no-history` imports it as one change);
   - an e-mail address in `users.yaml`;
   - a wiki that has pages.
7. **A failure halfway leaves the wiki *importing*, closed,** answering *being imported* to every read and refusing
   every write. A kill test shows it.
8. **The interwiki map:**
   - a valid map changes how links render, after an accepted proposal at the policy-change rung;
   - each D-8 rule refuses at acceptance;
   - a broken file refuses writes;
   - the map is in the export and in `site.json`.
9. **The API:** `Link` headers on every paged list (a test walks one), both new endpoints and the feed described in
   OpenAPI with their problems, and the drift test still passes.
10. **The self-assessment's test passes**, and the release rehearsal attaches the file.
11. **The cost is measured:** an import and an export at 500 and 2,000 revisions, and D-2's default cap set from it.

## How this was checked against the owner's two lenses

- **Finally clean, safe and secure, robust and sophisticated.**
  - **One rule for the history:** one revision, one block. Nothing in the read path changes, and so nothing of RFC
    006's hiding has to be argued again.
  - **The bundle is untrusted everywhere,** checksums are no longer optional, and nothing is fetched or run.
  - **The import either finishes or leaves a closed wiki** with its reason. There is no half-history to read.
  - **No new trust:** the wiki signs, the source's author is a name.
- **APIs and UI that cannot confuse.**
  - **No one appears to have signed what they did not:** an imported change says *Imported. Originally by …*.
  - **A licence is never relabelled,** and an incompatible one is refused, not hidden.
  - **The report says what was not imported,** and why, before anything is written.
  - **The map's links are external,** and look so.
  - **Avoided on purpose:**
    - an archive parser;
    - a resumable import;
    - an override for licences;
    - draft rate-limit headers;
    - replaying into a wiki that has pages.

    Each would add a way to be wrong for a case no one has asked for.

## Alternatives considered

- **Keeping the source's history as records, not blocks:** import the final state, and show the bundle's history as
  *before this wiki*. It is cheaper, and it removes the cap. **Rejected:** permalinks, diffs and *Why is this here?*
  would not reach the imported revisions, and the history would be inklkunn's in name only.
- **Many revisions per block,** to cut prikk's per-block cost. **Rejected:** it breaks *the last writer at block X*,
  on which the hidden-text rule rests (D-2).
- **Running the converters from inklkunn.** Rejected by P-2.
- **The map as an ordinary file any acceptance may change.** **Rejected:** one change of a template redirects every
  link with that prefix, in every page.

## Threat model v0.5 additions (folded into `docs/src/inklkunn-03-threat-model.md` on acceptance)

- **T-5, rewritten** for D-1 – D-6:
  - **C-5a:** checksums verified, and required unless the admin says otherwise, which the report then shows;
  - **C-5b:** the bundle is a directory only;
  - **C-5c:** attribution is data, never a signature (D-3);
  - **C-5d (new):** a forged history (`at`, author, licence, `rev`) is a claim, recorded as the source's and never
    shown as inklkunn's fact;
  - **C-5e (new):** bounds on revisions, bytes and records, with the replay's cost stated before the run;
  - **C-5f (new):** an import never leaves an open half-history (D-1's *importing* state).
- **Privacy (PRV):** the people an imported history names are permanent in the repository. Shown before the run, and
  said in SECURITY.md. E-mail addresses are refused.
- **C-17 (links):** the interwiki map is a governance file, `https:` only, and its links are external (D-8).
- **INV-13:** the list of surfaces gains the export's revision `content`, `listProposalComments`, `listProposals`'
  summaries and the proposals feed.
- **RR (new):** a history longer than the cap cannot be imported with its history until prikk's per-block cost falls
  (PK-50). It is said, and `--no-history` is the way on.

## Questions for the owner

- **Q-1 — How does an imported change show its author?** *Imported. Originally by {name}, {date}*, as data, with no
  trust label for the original author (D-3). **Recommended.**
- **Q-2 — May an admin import a page whose licence is incompatible?** **No.** The only choice is to leave such files
  out, explicitly, and the report names each (D-4). **Recommended.** The admin who has the source's permission can
  record it on the page and import it as an ordinary proposal.
- **Q-3 — What may the interwiki map hold, and who changes it?** `https:` templates with `{title}` only, as a
  governance file at the policy-change rung (D-8). **Recommended.**
- **Q-4 — Rate-limit headers:** not in 0.5.0, since the standard is still a draft. `Retry-After` stays, and IX-4 says
  so (D-9). **Recommended.** This narrows the plan's section 2e.
- **Q-5 — The revision cap:** set from R-4's measurement, with `--no-history` as the way on for a longer history (D-2).
  **Recommended.**

## Sequencing and handoff

On acceptance, the architect folds the threat model additions into `docs/src/inklkunn-03-threat-model.md` (v0.5) and
writes `rfcs/handoffs/007-interchange-and-api/interchange-handoff-v1.md`. The order of the tasks is:
1. the export's history records (D-7), with the shape test, because the import's round trip is tested on them;
2. the import's plan and report (D-1, D-4 – D-6), as a dry run that writes nothing;
3. the replay (D-2, D-3), with the *importing* state, the kill test and the measurement;
4. the interwiki map (D-8);
5. the API additions and the feed (D-9);
6. the self-assessment (D-10).

Tasks 4 – 6 may run beside 2 – 3. Readiness follows the plan (R-5). The fixtures (P-2) are made once, by the command
in their README, at wiki-commons `19f021c`.
