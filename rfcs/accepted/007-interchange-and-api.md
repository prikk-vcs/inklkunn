# RFC 007 — Interchange and API: a wiki that comes in with its history, and leaves with it

**Status.** **Accepted by the owner 2026-11-26** (*"Your recommendations are accepted."*: Q-1 – Q-5 ruled as
recommended). The owner asked the architect to verify that each recommendation matches the owner's philosophy. That
check made eight refinements before acceptance (each marked *refined* below):
1. the import creates its wiki, which appears only when it is complete (D-1). There was no way to remove a wiki that
   a killed import left;
2. the new wiki takes the bundle's licence, so no page is ever shown under another (D-4);
3. the two dates and the source are said apart, and a source's name is never taken for a local account (D-3);
4. checksums are verified when present and said absent when not, with no bypass flag (D-5);
5. a page's own order cannot be rearranged by a forged time (D-2);
6. the report lists the link prefixes the map lacks (D-6);
7. `rev` is the block id, and the CHANGELOG says the history's shape changed (D-7);
8. the new lists show what the same person's pages show (D-9).

This is 0.5.0's R-1 (`docs/releases/0.5.0-plan.md`, authorized
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
- **This RFC adds `wiki import-bundle`.** It creates a new wiki from a bundle: the pages, the attachments
  and, **when the bundle has them, every revision as a change of its own, in order**. Each change is signed by the
  wiki, as every change is, and names its original author and time as data. Licences are checked, checksums verified,
  and a report says what came in and what did not.

**inklkunn's own export becomes such a bundle too.** It claims level 3 today, but its history lines lack the two fields
the history record requires (`rev`, `at`), and carry no revision's text. After this RFC, the history lines carry both,
and renames and removals become history records. So an inklkunn wiki exported and imported again keeps its history.

The rest is small: the interwiki map, four API additions, and the conformance self-assessment.

## Decisions

- **D-1 — `wiki import-bundle <new-wiki> <bundle-dir>` creates a wiki from a bundle** (P-1; *refined*).
  - **The wiki is new:** the command takes `wiki create`'s options (the slug, the name, the visibility) and makes the
    wiki itself. **Its default licence is the bundle's** (D-4). An existing wiki is never the target: `import-folder`
    stays the way to add pages to one, as one change, and the refusal of an existing slug says so.
  - **The bundle:** a directory, as `import-folder` reads one. A ZIP or a tar is unpacked by the operator first. That
    is one less parser facing untrusted input.
  - **A dry run first:** `--dry-run` is the plan (D-6's report), and writes nothing. The run is the same plan, checked
    again on the bytes read, as `import-folder` does.
  - **It is an admin's act at the command line,** recorded and audited as one act (`wiki-import-bundle`). It is not on
    the web or in the API.
  - **The wiki appears only when the import is complete.** The import is *create wiki* extended to the whole history:
    the creation mark (IS-13) is held for the whole run, and the record is stored last. Until then nothing serves,
    lists or links the wiki, so no one ever reads half a history, and there is no *importing* state for a reader to
    meet.
    - **A failure the process survives** removes everything the attempt made, as a failed creation does, and the
      audit keeps the reason.
    - **A kill** leaves a creation mark with sealed blocks, which the start-up sweep reports and does not remove (R9:
      only a person discards a history). **`wiki discard-unfinished <id>`** is that person's act, and it is the only
      new removal:
      - it removes exactly what the sweep would remove for a creation that never finished;
      - it is refused for any wiki whose record was ever stored, and for a creation whose process is alive;
      - it asks for the id to be typed again, and it is audited.
    - **There is no resume.** A wiki that no one has read can be made again (XFER-14's *resumable* is left for later).
- **D-2 — One source revision is one change, and one block.**
  - **The order** (*refined*):
    - **Within a page, its file's order is its history.** A record whose `parent` names anything but the record
      before it, or whose `at` is earlier than the one before it, refuses the import, naming the page and the `rev`.
      So a forged time cannot rearrange a page's own revisions.
    - **Across pages,** the revisions are merged by `at`, compared as instants (never as strings), with ties in path
      order.
    - **The same bundle always gives the same history.**
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
  - **Two dates, never confused** (*refined*):
    - an imported row's date is the source's, always labelled *originally*;
    - the day of the import is said once, in the wiki's notice.
  - **The notice** (LIC-4's *imported from*) is on the wiki's history and on every imported page: *Imported from
    {source} on {date}*. The source's name and address come from the manifest's `source`, shown **as text, never as a
    link** (an address from a bundle is untrusted, C-17a).
  - **No account is made** for a source author (*refined*):
    - their name is text, escaped as every name is;
    - it is **never linked to, or matched with, an account of this server**, even one with the same name;
    - it is always marked as the source's.
  - **Imported changes are not participation here:** the participation counts (recorded since 0.2.0) leave them out.
  - **The source's suppressions are kept** (XFER-7, `suppressed`):
    - a record whose **content** is suppressed is **not replayed** (there is no text), and the report counts it;
    - a record whose **author or summary** is suppressed is replayed with that part said as *hidden at the source*.
  - **The personal data is permanent** in the repository's history (the removal procedure is 0.6.0's). The dry run's
    report lists how many people the history names, and SECURITY.md says it. IP addresses are already replaced by the
    converter (`~ip-…`), and a `users.yaml` entry with an e-mail address refuses the import, naming the entry.
- **D-4 — Licences are checked, and never relabelled** (LIC-5, LIC-3; *refined*).
  - **The new wiki's default licence is the bundle's:** the manifest's `license`. So a page that carries no licence of
    its own is shown under exactly the licence it came with.
    - **A bundle that declares none** (wiki-commons' MediaWiki converter writes none) is refused at the plan, with a
      sentence. The admin who knows the source's licence declares it with `--bundle-license <SPDX id>`. The act
      records it, and the notice says *licence declared by the importing admin*.
    - **A licence that a wiki cannot have** as its default (D-4's matrix finds it compatible with nothing: NC, ND,
      GFDL, unknown) refuses the import, naming it.
  - **Each page's own licence** (its frontmatter `license`) and **each attachment's** (its sidecar's `license`) that
    differs from the bundle's is compared with the wiki's default by wiki-commons' matrix (chapter 15):
    - CC0 goes into any wiki;
    - CC BY goes into BY or BY-SA;
    - BY-SA goes only into BY-SA;
    - NC, ND, GFDL and an unknown or missing licence go into none.
  - **The dry run lists every incompatible file** with its licence and the wiki's.
    - **The run refuses** while there is one, unless the admin passes `--leave-out-incompatible`. That flag leaves those
      pages (with their history) and attachments out, and the report names each.
    - **There is no override that imports them** (Q-2).
  - **A licence is never rewritten.** A page keeps its own `license` line, so a CC0 page in a BY-SA wiki still says
    CC0, and a page without one has the bundle's, which is the wiki's.
- **D-5 — The bundle is untrusted, all of it** (SEC-7; C-5a – C-5c made true).
  - **The walk is `import-folder`'s:** no link followed, identities pinned, the same path, name and size rules.
  - **Checksums, when the bundle has them, are verified strictly** (*refined*): every file against the manifest's
    `checksums` file, and every file the bundle holds that the file does not name. A missing, extra or wrong file
    refuses the import, by name.
    - **A bundle without them** is imported (SEC-7: *when present*), and the report and the wiki's notice say
      *checksums: none in the bundle*. There is no flag to skip checksums that are present.
    - **What checksums show is integrity, not authorship:** anyone can write them. They prove that the bundle the
      admin checked is the bundle that was read, and nothing about who made it (the threat model says so).
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
  - **the link prefixes the pages use that the interwiki map does not have**, with how many links each (*refined*).
    Those links stay dangling until the map gets the prefix (D-8), and the report says so, so that `[[wikipedia:…]]`
    does not look like a page to create;
  - whether the checksums were verified, and the licence the wiki takes, with where it came from;
  - the time the replay is estimated to take, from D-2's measured cost.

  Every name in it is escaped, and no byte of a refused file is repeated.
- **D-7 — The export becomes a level 3 bundle in fact** (IX-1, XFER-7).
  - **Each history line is a wiki-commons history record:**
    - `rev`: the block id (*refined*: a block writes a page once, so it is unique in the page's file, and it is the
      id a permalink already names);
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
  - **The history lines change shape** (*refined*). The CHANGELOG's *Output changes* says so, field by field, for anyone
    who read 0.4.0's lines (OPS-4).
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
  - **They show what the same person's pages show** (*refined*): never a draft, a restricted wiki as an absent one, and
    the same `authorize()` read as the proposal page. The tests compare each list with the page for an anonymous
    visitor, a contributor and a moderator.
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
   - a bundle that declares no licence, without `--bundle-license`;
   - a page whose `parent` or `at` breaks its file's order;
   - an existing slug.
7. **A wiki appears only when its import is complete** (D-1):
   - a failure the process survives leaves nothing;
   - a kill at each step leaves what the sweep reports and keeps;
   - `wiki discard-unfinished` removes exactly that, and is refused for a wiki that was ever stored or a creation
     that is alive.

   Kill tests show it.
8. **The words:** an imported row says *Imported. Originally by …* with the source's date as *originally*; the
   notice names the source as text; a source name equal to a local account's is not linked to it; and the
   participation counts leave imported changes out.
9. **The interwiki map:**
   - a valid map changes how links render, after an accepted proposal at the policy-change rung;
   - each D-8 rule refuses at acceptance;
   - a broken file refuses writes;
   - the map is in the export and in `site.json`.
10. **The API:** `Link` headers on every paged list (a test walks one), both new endpoints and the feed described in
   OpenAPI with their problems, and the drift test still passes.
11. **The self-assessment's test passes**, and the release rehearsal attaches the file.
12. **The cost is measured:** an import and an export at 500 and 2,000 revisions, and D-2's default cap set from it.

## How this was checked against the owner's two lenses

- **Finally clean, safe and secure, robust and sophisticated.**
  - **One rule for the history:** one revision, one block. Nothing in the read path changes, and so nothing of RFC
    006's hiding has to be argued again.
  - **The bundle is untrusted everywhere.** Checksums that are present are never skipped, their absence is said, and
    nothing is fetched or run.
  - **The import is *create wiki*, all or nothing:** the wiki appears only when complete, and the one new removal
    acts only on what a killed import left, by a person's explicit act (R9).
  - **No new trust:** the wiki signs, the source's author is a name.
- **APIs and UI that cannot confuse.**
  - **No one appears to have signed what they did not:** an imported change says *Imported. Originally by …*.
  - **No date or name can be taken for another:** the source's date is always *originally*, the import's day is in
    the notice, and a source's name is never a local account.
  - **A licence is never relabelled:** the new wiki takes the bundle's, an undeclared one is declared by a person and
    said, and an incompatible one is refused, not hidden.
  - **A prefix the map lacks is reported,** so imported interwiki links do not look like pages to create.
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
  - **C-5a:** checksums verified strictly when present, and their absence said;
  - **C-5b:** the bundle is a directory only;
  - **C-5c:** attribution is data, never a signature (D-3);
  - **C-5d (new):** a forged history (`at`, author, licence, `rev`) is a claim, recorded as the source's and never
    shown as inklkunn's fact;
  - **C-5e (new):** bounds on revisions, bytes and records, with the replay's cost stated before the run;
  - **C-5f (new):** an import never leaves a served half-history (D-1: the wiki appears only when complete; a killed
    import's leftovers are removed only by `wiki discard-unfinished`, a person's audited act);
  - **C-5g (new):** checksums show integrity, not authorship;
- **Privacy (PRV):** the people an imported history names are permanent in the repository. Shown before the run, and
  said in SECURITY.md. E-mail addresses are refused.
- **C-17k (links):** the interwiki map is a governance file, `https:` only, and its links are external (D-8).
- **INV-13:** the list of surfaces gains the export's revision `content`, `listProposalComments`, `listProposals`'
  summaries and the proposals feed.
- **RR-17 (new):** a history longer than the cap cannot be imported with its history until prikk's per-block cost falls
  (PK-50). It is said, and `--no-history` is the way on.

## Questions for the owner (ruled 2026-11-26: all as recommended)

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
3. the replay (D-2, D-3), as *create wiki* extended, with `wiki discard-unfinished`, the kill tests and the measurement;
4. the interwiki map (D-8);
5. the API additions and the feed (D-9);
6. the self-assessment (D-10).

Tasks 4 – 6 may run beside 2 – 3. Readiness follows the plan (R-5). The fixtures (P-2) are made once, by the command
in their README, at wiki-commons `19f021c`.
