# The knowledge layer — Developer Handoff v1 (RFC 005 part 2; M3)

| | |
|---|---|
| Document | Claims, sources and views (RFC 005 part 2, D-1 … D-12), on part 1's decisions D-1 … D-4. |
| Status | RFC 005 part 2 accepted by the owner 2026-10-31. **K-T1 … K-T4 are assignable now** (files, validator, merge, index: no surface, no sign-in needed); **K-T5 onward are assignable now: 0.2.0 was cut on 2026-11-04** (signed-in proposers, the proposal path served and the reviewers' forms are released). The spike's follow-up K1 goes first. |
| Basis | `rfcs/accepted/005-knowledge-layer-part-1-claim-identity.md` (D-1 … D-4), `rfcs/accepted/005-knowledge-layer-part-2-claims-evidence-views.md` (D-1 … D-12, *How this was checked*); requirements `KN-1 … KN-9`, `CM-5`, `CM-7`, `CM-11`, `NG-3`, `PU-6`, `PR-1`; RFC 004 (proposals, the merge, the inbox, metrics D-10); RFC 003 (the extract per tip, the derived indexes, health lists, the API and its OpenAPI document); wiki-commons XFER-1. |
| Audience | The dev team. The two lenses of handoff 003 apply unchanged, and part 2's *How this was checked* is the reviewer's checklist. |
| Handback | One review request per group: K-T1 … K-T4; K-T5 … K-T8; K-T9 … K-T11. Each with the usual gates, both test runs, the mutation table, and (for surfaces) every new page state in both languages. |

## K-T1 — The two files and their reader

- `knowledge/claims/<id>.yaml` and `knowledge/sources/<id>.yaml` exactly as part 2 D-1 and D-2 define them, read with the
  frontmatter's plain YAML subset rules (no anchors, no tags, duplicate keys refused). Enumerations closed, each with
  `unknown`; `bearing` is `direct | partial | indirect` and nothing numeric exists anywhere in either file.
- `judged_on` is SHA-256 of the expression's block text as `inklkunn_markup::blocks` trims it (one definition, one
  function, tested on both languages); the community status carries its own.
- Types in `inklkunn-core::knowledge` (no surface); round-trip tests for every field.

## K-T2 — Validation at acceptance (part 2 D-5)

- Before the signer is asked: schema, ids, every named source and claim present in the same view, every anchor naming
  a block that exists in the named page at the proposal's result, no owned page key affected. Refusals name the file
  and the rule, never echo content (the import's escaping).
- A repository that already holds invalid files (written by other means, imported) serves its pages; each invalid file
  is listed on the health list *Knowledge files that cannot be read*, and no claim is half-read from it.

## K-T3 — The knowledge files in the merge (part 2 D-4)

- RFC 004's merge gains a third file family beside pages and frontmatter: YAML key by key; `expressions` matched by
  anchor, `relations` by `(type, claim)`, `sources` by source id; one value changed on both sides is an **overlap with
  both texts**, never resolved; nothing is lost.
- A corpus of knowledge-file pairs (hand cases plus pairs derived from spike C's histories, as M0's spike B did for
  pages), with the clean / overlap counts committed and held by a test.

## K-T4 — The derived knowledge index (part 2 D-6)

- Built from the **same** `extract` per tip as the title and link indexes; claims, sources, relations, anchors, and per
  relation whether the anchor's current text equals its `judged_on`. Bounded like the title index; rebuilt with the
  derived store cleared (a test).

## K-T5 — Making claims and attaching sources (part 2 D-3)

- In the propose form, a paragraph's part menu gains *Attach a source* (creates the claim, if the paragraph has none, and
  the source, in the same proposal as any page edit) and *Challenge this* (a challenging source, or the claim's thread).
  The paragraph's `^id` is held by the form and never shown in the text box (part 1 D-1 b); removing such a paragraph is
  an explicit act shown to the reviewer.
- The **curation proposal**: its own small form (status, relation, bearing, `last_reviewed`, `availability`), no page text.
- On the proposal page, the claims a change touches, each *judged on an earlier wording* consequence, and **one**
  re-confirmation step for all of them (part 1 D-1 c). The reviewers' identity decisions and re-confirmations are
  recorded and counted in the participation metrics (part 1 D-1 d): how often the default was overridden.

## K-T6 — What a reader sees (part 2 D-7)

- One quiet *Sources and views* link on a paragraph that holds a claim; one *Why is this here?* link among the page's tools.
- The claim page in words: kind, sources with relation and bearing, *judged on an earlier wording* where so, status in
  this view with the deciding proposal, other views with *compare*. The retired-words test gains *true*, *false*,
  *verified* (outside the verification badge and page), *proven*, *canonical*, *adopted* (for views), *official*, in both
  languages; no ranking colour, icon or number (a stylesheet test).
- **A page with no claims renders byte-identical to before except the one link** (a test).

## K-T7 — *Why is this here?* (part 2 D-8)

- One page per wiki page; each paragraph anchored; for each: the change that last changed it and the one that first wrote
  it, with proposal, people as the message records them, review, acceptor and the sources attached then. Exact through the
  `^id` for a claim paragraph; for any other paragraph traced through the history with the product's own segmentation,
  **bounded** by the history page's limits, and labelled *traced by matching the text*.

## K-T8 — Views (part 2 D-9)

- Create an alternative view (`create-view`, reviewers and above by default: the owner's ruling) with `forked_from` and
  `kind: experimental | interpretation`; *Compare views*: claims that differ (wording, status, sources) and pages that
  differ (RFC 003's diff). Readers see *the default view* (the glossary's word); no view is called canonical, adopted or
  official.

## K-T9 — Freshness (part 2 D-10)

- `knowledge.review_after` (default 3 years) in the wiki's settings; the health list *Sources to review* (older than the
  setting, or `unavailable`), with the claims relying on each. **No outbound fetch** for sources.

## K-T10 — A claim's thread (part 2 D-11)

- The proposal thread's records and rules, labelled *discussion of this claim*; exported with the bundle's discussions; a
  thread never changes a claim.

## K-T11 — Interchange and API (part 2 D-12)

- `knowledge/` exported as is, `extensions_used: [inklkunn-knowledge@1]` in the manifest, the manifest validated against
  wiki-commons' schema as today; an import of another inklkunn bundle validates `knowledge/` (K-T2).
- Read API: `claims/{id}`, `sources/{id}`, `pages/{title}/provenance`, `views/compare`; write API: the curation proposal.
- **Hold note (prikk's letter of 2026-11-14 §4):** an import that adopts history (`bundle import --adopt`, `archive import
  --adopt`) on prikk 0.57.0 or earlier runs `prikk verify` once after it, or every later commit reads about twice the whole history
  (42 MB per commit at 20 MiB measured by prikk); fixed in prikk 0.58.0.
  In the OpenAPI document (drift tested both ways); existence-neutral; authorized as the pages are.

## Non-change scope

No new crate; no score, rank or truth field anywhere; no outbound fetch; no change to the history, the signer or the
verification badge's meaning.

## Review of K-T1 – K-T4 (2026-11-01)

**Approved** (`.git-exclude/reviewed/005-knowledge-layer-k-t1-k-t4-review-result.md`). Follow-up **K2** with the next package: one list
of owned page keys (`knowledge/check.rs` uses the proposal service's `OWNED_KEYS`). K-T5 onward after 0.2.0, as before.


## Corrections from the review of 2026-11-08 (binding; **before 0.3.0**)

K-T5 – K-T8 are **Conditionally Approved** (`.git-exclude/reviewed/005-knowledge-layer-k-t5-k-t8-review-result.md`). Hand back
K3 – K14 as one request, `005-knowledge-layer-corrections-review-request.md`; K-T9 – K-T11 may proceed beside them. **Creating views
on a real server waits for K3 and K4 together.**

- **K3 — store schema 16, one migration.** (a) `view_acts` and `view_act_ends` as proposed, tightened: a name is unique among
  **standing** acts (open or created) — checked inside the `IMMEDIATE` transaction as the other acts are, or by a `BEFORE INSERT`
  trigger; `CHECK` the name's form (`name = lower(name)`, 1 – 64 characters, a `GLOB` of the allowed characters); `act_id
  REFERENCES view_acts(id)` and `wiki_id` referencing the wiki; `CHECK ((outcome = 'created' AND word IS NULL) OR (outcome =
  'failed' AND word IS NOT NULL))`; an index on `(wiki_id, name)`; the write-once triggers. (b) **The review record of part 1
  D-1 d**, append-only: per accepted change and per claim it touches, *kept*, *dropped* (a confirmed removal), and later
  *re-confirmed by* (the curation proposal, naming the change it re-confirms); `inklkunn wiki metrics` counts the **drops**
  (a confirmed removal) and the *request changes* on a proposal that touches a claim, apart (K21), beside the defaults kept. The migration from 15 is
  additive and tested on both stores; the pins move to 16.
- **K4 — a view creation is a recoverable act.** `Purpose::View` leaves `leaves_nothing_to_recover`, so a holder that died
  creating a view flags the wiki. Recovery treats an open view act as a known act: prikk shows the view → the act ends
  *created*; prikk shows an incomplete publication of exactly that view's ref → the step prikk itself recommends, run by the
  signer bound to the recovery act as P15's `complete-publication` is, or a stop with its own word and a `docs/RECOVERY.md`
  section; anything else → a stop. `settle_earlier_acts` settles only what recovery's witnesses agree on (or leaves it to
  recovery). Tests: a real kill during `branch create`.
- **K5 — trust never moves unseen.** The proposal page shows every re-judgement — a source link's or a status's `judged_on`
  rebound to new wording — as its own line with an excerpt of the new wording; a proposal whose knowledge change would show
  nothing on its page cannot be submitted.
- **K6 — the gate is D-3.** Changing an existing source link's `relation`, `bearing` or `judged_on`, or removing it, is a curation
  change only; adding a link stays with the page edit it belongs to.
- **K7 — no proposal that can never be accepted.** The acceptance gate's knowledge rules (a curated value outside a curation; the
  removal of a decided claim; K6) run at submit and at every revision, refusing in the same words; the proposal page's warning
  is the same rule.
- **K8 — *Why is this here?* is exact only when it is.** A step that finds no earlier counterpart clears *followed through its
  anchor* (only a page's creation is exact); a test with *Attach a source* adding `^b1` while the next paragraph is reworded.
- **K9 — a source has a name on the claim page.** The derived index keeps each source's title, author or publisher and date as
  written, and reference (each cut at about 300 characters when the index is built; escaped when shown); the derived store's
  schema moves and is rebuilt.
- **K10 — markers.** A held `^id` returns to the k-th occurrence of equal words, never the first; the one-paragraph form keeps
  the whitespace before the marker as it was.
- **K11 — the reconfirm step** works from the **accepted** revision, names the change it re-confirms (recorded by K3 b), and is
  not offered again while its curation proposal is open.
- **K12 — views.** The name check sees every view, closed ones included, ignoring case; the name is validated before it enters
  any resource or audit name; *Compare views* is cached by its two tips (both immutable) or counts lines without keeping hunks,
  and the claim page's *Other views* uses the same; view names that are retired words (*official*, *canonical*, *adopted*,
  *true*, *false*, *verified*, *proven*) are refused.
- **K13 — *Why is this here?*, smaller.** A per-request work budget on the alignment that ends in the existing *bounded*
  wording; a repeated `^id` uses the paragraph's number for the element id; a change whose message names a proposal the records
  do not tie to it reads *the message of this change names the proposal …*; a sanitizer test with `</p>` and `<p id="h-^b1">` in
  an image's alt and a link's title.
- **K14 — small.** Source text fields refuse bidirectional and format characters (U+202A – U+202E, U+2066 – U+2069, U+200E,
  U+200F); un-deciding a status records the deciding proposal.
- **Recorded gap (not owed for 0.3.0):** attaching an existing source instead of a new one (D-2, KN-4).

## Corrections from the review of 2026-11-13 (binding; **before 0.3.0**)

K3 – K14 are **Conditionally Approved** (`.git-exclude/reviewed/005-knowledge-layer-corrections-review-result.md`). Hand back
K15 – K23 as one request, `005-knowledge-layer-corrections-2-review-request.md`; K-T9 – K-T11 may proceed beside them. The
environmental reds are in `rfcs/handoffs/010-upstream-and-supply-chain-maintenance/maintenance-2026-11-handoff-v1.md`, a separate request.

- **K15 — the page says everything the core describes, and which.** Every rendered knowledge line names its subject: a claim by an
  excerpt of a paragraph it is said in (escaped, in `<bdi>`), a source by its K9 name; a new claim's standing and a new source's
  availability and review date are on its line; a changed source's identity and reference show their new values. No renderer arm
  drops a field with `..`. `KnowledgeNotShown`'s property tests move to the rendered layer: every single-field change from every
  starting point gives a different rendered page.
- **K16 — removal is curation when it would take anything curated with it.** `removal_refused` is `Curated::of(file) !=
  Curated::baseline(file)` (a status, a link, an un-decided record); removing or overwriting a knowledge file the tip has but cannot
  read is a curation change with its own line (*a file that cannot be read is replaced*); the refusal sentence names removals.
- **K17.** *Re-confirmed* is recorded for a claim only when every `judged_on` it carries names the wording its paragraph has in the
  accepted change.
- **K18 — schema 16 before it ships.** `view_acts.created_by REFERENCES accounts(id)` (the tests make real accounts);
  `view_act_ends.word` `CHECK (length(word) >= 1)` where it is not null. Schema 16 is unreleased, so the migration is edited, not
  followed by another.
- **K19 — views and the signer.** `ViewCreationFacts` carries the recorded `from_block`, and `complete_view_creation` requires prikk's
  plan target to equal it; the doc says what the code checks (or the code re-reads the tip, as the doc says); a rotation (Keep or
  Retire) refuses while `doctor` names a view lead, in Z5's words; the plan names `retirement-unfinished` for a view lead as it does
  for a queued patch.
- **K20.** *Why is this here?*: a paragraph whose `^id` is repeated at the page's creation is not *followed through its anchor*.
- **K21 — metrics.** `dropped` and `changes_requested` are reported separately (not summed across units), and the metrics say that
  changes accepted before schema 16 have no review records.
- **K22 — reconfirm.** Its proposal links the change it re-confirms; at most one open reconfirm proposal per change (the store
  enforces it); the offer and the record are computed against the same side (the tip); a renamed page is read at the base by its
  base path.
- **K23 — small.** U+061C joins K14's set; the work meter charges a comparison by the length compared (or the docs state the
  factor); a source's name is shown with a visible boundary (`<cite>`); `Storage::all_views` is a required method;
  `complete_view_creation` keeps the *seeds not removed* note; *Other views* reads the view at the tip the page's index was read at;
  the stale comments in `sqlite.rs:7` and `invariants.rs:507-509`.

## Corrections from the review of 2026-11-14 (binding; **before 0.3.0**)

K15 – K23 are **Conditionally Approved** (`.git-exclude/reviewed/005-knowledge-layer-corrections-2-review-result.md`). Hand back
K24 – K31 as one request, `005-knowledge-layer-corrections-3-review-request.md`; K-T9 – K-T11 may proceed beside them.

- **K24 — no two subjects read alike.** A claim's and a source's line carries its identifier as secondary text whenever its excerpt
  or name is not unique among the subjects the page names (or always — the dev team's choice, said in the request); a test with two
  claims said in one paragraph, one with two paragraphs that share their first 80 characters, and one with two sources of one name.
- **K25 — a new link is judged on a wording the history has.** At submit, at every revision and at acceptance, a new link's
  `judged_on` must name a wording one of the claim's paragraphs has in the result (what `compose` writes); otherwise the proposal is
  refused in words. The cite line then says *on the wording it has now*.
- **K26 — a reconfirm never leaves a draft.** The step creates its curation proposal **already held for review**, in one store
  transaction, after the submit checks have passed (a refusal leaves nothing behind); a test pins the trigger's list of open states
  to `ProposalState::is_open`.
- **K27.** A removal's line names what leaves with it: a claim's status and each link; a source's review day and availability.
- **K28.** K17 judges *current* against the paragraphs the claim has **after** acceptance (the written file's expressions); a decided
  status with no `judged_on` is a judgement that fails.
- **K29.** The core's `Rule::Status` sentence and the web's `health.rule_status` name removals and unreadable files; a source's or a
  link's refusal names its own field, not `community_status`.
- **K30.** On a source whose review day or availability is set, changing its identity or reference is a curation change.
- **K31 — small.** `Resolver::index()` reads the base as its doc says (`knowledge_index_at_base`); the rendered property test starts
  from a link judged on the paragraph's real wording too; `<cite>` and `<bdi>` get a visible boundary in `site.css` that a title
  cannot imitate (the stylesheet test holds it neutral: no colour that ranks).

## Corrections from the review of 2026-11-14, fourth round (binding; **before 0.3.0**)

K24 – K31, K-T9 and K-T10 are **Conditionally Approved** (`.git-exclude/reviewed/005-knowledge-layer-corrections-3-review-result.md`).
Four RFC clarifications were added on 2026-11-14: every judgement, naming a subject, whose setting, a thread in the bundle. Hand back
K32 – K42 as one request, `005-knowledge-layer-corrections-4-review-request.md`. K-T11 proceeds beside them.

- **K32 — the whole identifier.**
  - Every claim and source a line names carries its whole identifier in `<code>`, linked to the subject's page (the claim page; a source's place on the claim page), and wrapping where the line is narrow.
  - The abbreviation (`names.rs`) and the two-pass render go.
  - Tests:
    - a twin source with the same name and an id that shares the first and last 8 characters reads differently on a page that names only one of them;
    - the same for a claim;
    - both languages.
- **K33 — every judgement names a wording the result has.**
  - The K25 rule is widened from new links to every `judged_on` a proposal **sets or changes**: a link's and a community status's, in a page edit or a curation proposal.
  - One function, at submit, at every revision and at acceptance.
  - Its refusal names the field.
  - The compose path's digests stay as they are.
  - Tests through the API and through a raw file.
- **K34 — the proposal page judges against the service's pages.** The web `Resolver` reads the proposal's pages through the core's `ProposedPages`: removals, renames, and a rewritten `id:`. The page then never says *judged on the wording …* where submit would refuse. Test: the removed page and the changed `id:`, each through the real page and the submit.
- **K35 — the reconfirm reads the acceptance's tip.** `reconfirmed` takes the tip the acceptance prepared on, as `claim_outcomes` does, and reads the title index at that block. Nothing it uses comes through the TTL cache. A test where the cache holds an older tip.
- **K36 — the sentences say the rule, the same in both languages:**
  - `know.not_curation` says *replaces or removes* (ja too).
  - K30's protected fields are named the same way in en and ja: title, author, date and reference. `kind` is not among them.
  - `superseded_by` set or changed on a **checked** source is a curation change, as identity and reference are.
  - `know.cite` frames the wording as *the wording it has in this proposal's result*.
  - The freshness list says *Claims that cite it* (with the relation already shown).
  - The API and its OpenAPI text call the setting the operator's, for every wiki.
  - The glossary follows each.
- **K37 — a paragraph line names its page.** `know.change_said` and `know.change_unsaid` name the page as well as the paragraph's opening words (and the block's id when two paragraphs of one page begin alike). Test: the move between two paragraphs that begin alike.
- **K38 — the create paths:**
  - **The reconfirm** answers *already proposed* (with the link) before any quota. The read happens under the admission lock, so the order cannot be raced.
  - **The curation form's** create and submit become one step with the same shape as K26: the proposal is created held for review in one transaction, or refused with nothing left.
  - A test for each.
- **K39 — the curation form is reachable** (decision 3).
  - These offer *Propose a change to its status or sources*, linking `…/proposals/curate?claim=<id>&view=<view>`, to those who may propose (nothing for others):
    - the claim page;
    - every refusal that says a change needs a curation proposal;
    - each claim in a *Sources to review* row.
  - A route-enumeration test that the form is linked.
- **K40 — the freshness list's cost and a future day.**
  - The index builds the source→links and claim→anchors maps once.
  - The list is computed once per (tip, day, setting) and kept with the index.
  - The web list pages like the API (50 rows, a cursor).
  - A `last_reviewed` after today is listed with its own reason: *recorded as reviewed on a day that has not come* (en, ja, glossary, OpenAPI enum).
  - A test with 2,000 sources, bounded in work.
- **K41 — a claim's thread in the bundle** (decision 5; RFC clarification D-11).
  - Claim threads are written where K-T11 declares the `inklkunn-knowledge@1` extension, never under `discussions/`.
  - Bodies are escaped so that portable Markdown shows exactly the plain text inklkunn showed (a round-trip test with link, image and HTML syntax).
  - The manifest's `discussions` is not set by claim threads.
  - inklkunn's own import names the extension's directory as what it is.
  - Until K-T11 lands, the export carries no claim threads, and says so in the manifest's dropped list.
- **K42 — K-T10's small points:**
  - the last page comes from one count or `seq` query;
  - `ClaimDiscussion.claim` is a `ClaimId`, so the path is built only from a checked id;
  - *In reply to comment N* links to that comment (on its page);
  - V18's doc comment says one index.

Process: never kill processes by pattern (`pkill`, `killall`); a record carries the day it was written.

## Corrections from the review of 2026-11-14, fifth round (binding; **before 0.3.0**)

K32 – K40, K42 and K-T11 with K41 are **Conditionally Approved**:
- `.git-exclude/reviewed/005-knowledge-layer-corrections-4-review-result.md`
- `.git-exclude/reviewed/005-knowledge-layer-k-t11-review-result.md`

RFC 005 part 2: D-11's clarification is amended (the bodies are a fenced text block), and D-12, *what an import may carry*, is added. Hand back
K43 – K54 as one request, `005-knowledge-layer-corrections-5-review-request.md`. **K51 first.**

- **K43 — the whole identifier where readers look** (U-a).
  - The claim page's source lines and *Sources to review* carry each source's whole id in `<code>`, as K32's lines do. *Sources to review* names each claim by its id as well as its page.
  - The test that asserts the claim page shows no id is turned around.
  - A twin source gives two lines that differ, on both surfaces, en and ja.
- **K44 — curation can make every change only curation may** (U-b).
  - The curation proposal (form and `POST …/proposals/curation`, one `Curation` in `compose`) gains:
    - **removing a link**;
    - **on a checked source:** its identity (title, author, date), its reference and its `superseded_by`.
  - Shown and confirmed line by line like the rest, and judged by the same rules at acceptance.
  - K39's link is offered only for refusals the form can then make. For any other refusal the sentence says what can be done instead (for example: a removal of a claim or of a source file is decided by a reviewer, not a proposer).
  - Tests: each newly reachable change, from the refusal's link to the accepted file.
- **K45 — every judgement, every way** (U-c).
  - A decided community status (not `undecided`) must carry a `judged_on`.
  - A link whose relation or bearing changes, and a status whose value changes, is a new judgement: its `judged_on` must name a wording the result has, even if the digest is unchanged.
  - One function, at the three places.
  - The `cleared` test is turned around.
- **K46 — the form reads renames** (U-d, decision 4). The page-edit and revise form passes the edit's renames to `ProposedPages`. Test: revising a proposal that renames and edits a page, where the page and submit agree.
- **K47 — the API's curation is one step** (U-e). `POST …/proposals/curation` uses `create_and_submit`. A refused send leaves nothing (test).
- **K48 — a link leads to what exists** (U-f).
  - A claim is linked only when the view's **tip** has it.
  - A source fragment is used only when that claim's page at the tip lists the source; otherwise the claim is linked without the fragment.
- **K49 — a future day, in any timezone** (U-g).
  - *A day that has not come* means a day after the current date in the earliest timezone (UTC+14), so no reviewer's local today is called future.
  - The curation form and the API refuse such a day, by field, before a proposal exists.
- **K50 — the list's cursor survives other edits** (U-h).
  - The cursor continues after the last source id shown (the list's order), not after an offset stamped with the tip. An edit elsewhere does not restart it.
  - A cursor past the end shows *No more sources*, never *56 to 55*.
- **K51 — one extract cannot remove another's, and a vanished extract is an error** (V-a).
  - Each process extracts under its own directory (`scratch/<pid>-<start time>/`, or an equivalent unique name). The sweep removes only directories whose process is gone (the record of the pid and start time, as the lease records do).
  - `with_tree` checks that its extract root still exists after the work. A missing root is *could not be read now*, never an empty tree.
  - Test: two `ReadService`s on one layout, one extracting while the other sweeps; neither is judged on an empty tree.
- **K52 — an import is judged as the service, not as a proposer** (V-b). The import's knowledge judgement applies the knowledge rules. The page-key ownership rule is the import plan's own (as without knowledge files). Test: the reviewer's fixture with a `kind: redirect` page and claims.
- **K53 — a thread's body is a fenced text block** (V-c; RFC clarification D-11 amended).
  - Each body is carried as a fenced code block with the info string `text`, fenced with one more backtick than the longest run of backticks in the body. Line endings are normalised to LF first; no other change is made.
  - Every CommonMark and GFM reader shows exactly the text, and no construct inside renders.
  - `markdown_text`'s construct-by-construct escaping goes.
  - Tests, through a CommonMark and GFM parser other than inklkunn's renderer (a dev-dependency):
    - the reviewer's cases (`www.`, mail addresses, `\r`, NBSP, `$…$`, `%%`);
    - a body full of backticks;
    - that a round trip of the fence returns the text.
- **K54 — small points:**
  - **The fourth-round result (U-items).**
    - **U-i:** the one-claim link names the claim; `curate_link_of`'s id is in `<code>`; the numbering of claims in *Sources to review* continues across pages; *and N more* appears beyond five.
    - **U-j:** `begins_alike` compares both lines against the base and the result together.
    - **U-l:** the reconfirm reads the title index at the prepared state (`title_index_at_base`, cached) instead of extracting the tree.
  - **The K-T11 result (V-items).**
    - **V-d:** the import judges with the operator's `ReadConfig`.
    - **V-e:** each drop names its page and block, and so does its refusal. The doc comment says the tip's index. `api.claim_lost` says *claims are said there*. `drop-not-removed` is mutation-checked.
    - **V-f:** provenance reads the index at `page_why`'s tip.
    - **V-g:** the five drift points, with `compare_views` in path order and a test.

## Corrections from the review of 2026-11-14, sixth round (binding; **before 0.3.0**)

K51 is **Changes Requested**; K43 – K50 and K52 – K54 are **Conditionally Approved**
(`.git-exclude/reviewed/005-knowledge-layer-corrections-5-review-result.md`). Two RFC clarifications were added: *correcting a
source*, and *what is not offered in 0.3.0*. Hand back K55 – K64 as one request,
`005-knowledge-layer-corrections-6-review-request.md`. **K55 first.**

- **K55 — an extract is live while its lock is held** (K51 redone; X-a, X-b, X-e).
  - **The lock.** Each extract has a lock file beside it (`<name>.lock`, created before `materialize`). Its owner holds it with an exclusive `flock` for the extract's whole life, and releases it only after the directory is removed.
  - **The sweep** removes an extract only after it has taken that lock without waiting. It then removes the directory, and then the lock file.
  - **Liveness from `/proc` goes.** `flock` on one filesystem holds across PID namespaces, containers and uids, and the signer already relies on it (`inklkunn-signer/src/flock.rs`; share it or move it to the lowest crate that needs it).
  - **No partial tree.** A held lock means no sweep can remove a live extract, so a half-swept tree is never read. The root check stays.
  - **`wiki_state`'s extract** goes through the same module, under the operator's `index_max_bytes` and `index_max_files`.
  - **Docs:** say that the data directory must be on a filesystem where `flock` works between the processes that share it (a local filesystem or a bind mount), as the key store already needs.
  - **Tests:**
    - a sweep from another PID namespace (`unshare -Urpf --mount-proc` where it works, with a counted self-skip line where it does not) leaves a live extract and removes a dead one;
    - the two-`ReadService` test;
    - an import's extract beside a server's.
- **K56 — one owned-key rule** (X-c). The knowledge check reads `proposal::files::changed_owned_keys`, which carries D8's redirect exemption, instead of its own comparison. Test: a proposer's redirect page plus a claim, through submit and acceptance.
- **K57 — the import is judged by the operator's limits throughout** (X-d, X-f).
  - `plan_folder` takes the `ReadConfig`, and its page checks and `wiki_state` use it.
  - `wiki import-folder` reads the same configuration file `serve` reads when it is given one (`--config`), so the two cannot differ.
  - Tests at both edges of a lowered and a raised page cap.
- **K58 — the OpenAPI request shapes are what the handler takes** (Y-a).
  - The curation schemas mark exactly the optional members as optional.
  - A request-shape test sends every documented example body, and the shapes the handler accepts, through both the schema and the handler, and fails where they disagree. The same is done for the other write bodies.
- **K59 — a curation changes only the fields it names** (Y-b, Y-d, decision 6).
  - Each identity field (title, author, date, reference, `superseded_by`) is compared and replaced on its own. A field that is not changed is never re-cleaned or rewritten.
  - In the API, every source member is optional, and left out means unchanged. `reviewed: ""` and `availability: unknown` clear, and say so in OpenAPI.
  - Tests:
    - a source with a trailing space and a newline in its title, curated for its status only, keeps its title byte for byte;
    - an API call with only `author`;
    - an API call with only `title` keeps the review day.
- **K60 — curation corrects any source** (Y-c; RFC clarification *correcting a source*).
  - The `NotChecked` refusal goes: curation corrects an unchecked source's identity, reference and `superseded_by` as it does a checked one's.
  - The sentences no longer send anyone to a page edit for it (en, ja, glossary, OpenAPI).
  - The page-edit rule of K30 is unchanged.
- **K61 — no `superseded_by` cycle** (Y-e). Acceptance's reference rule refuses a chain that returns to a source it passed. The form and the API refuse it by field before a proposal exists. Test: A→B with B→A in one curation, and across two.
- **K62 — say what is not offered** (Y-f; RFC clarification *what is not offered in 0.3.0*).
  - `know.not_by_form` and every surface that meets a removal of a claim or source file, or the replacement of an unreadable file, say it is **not offered in this version**. They name no reviewer or form.
  - The glossary row and the operator notes say the same.
- **K63 — the numbering costs nothing per page** (Y-g). Cumulative claim counts are kept with the list's memo (per tip, day and setting). A page reads its first number in O(1), and the numbers are those of the list it belongs to.
- **K64 — small points:**
  - **X-g:** comment bodies are stored trimmed, so a body never ends in `\n`; assert it, and handle a NUL by refusing it at the comment form, or by naming in the bundle that it is carried as U+FFFD. Add `\t` and `\0` to the exhaustive alphabet.
  - **Decision 4:** `docs/dependency-policy.md` gets `pulldown-cmark`'s dev-dependency row.

## Corrections from the review of 2026-11-14, seventh round (binding; **before 0.3.0**)

K55 is **Approved for the read path**; K56 – K64 and K57 are **Conditionally Approved**
(`.git-exclude/reviewed/005-knowledge-layer-corrections-6-review-result.md`). The clarification *correcting a source* is amended (a
changed reference clears the review). Hand back K65 – K70 as one request, `005-knowledge-layer-corrections-7-review-request.md`.
**K65 first.**

- **K65 — no liveness from `/proc` in the scratch directory, anywhere** (A-a, A-b, A-c, A-d, and the architect's note).
  - **The export** extracts through `Extract`, and its `.partial-` directory is held the same way: a lock file beside it, swept only when its lock can be taken.
  - **`build`** compares the files it bundles with the `Materialized` report and refuses a difference.
  - **`process_has_ended` and the `/proc` sweeps go.**
  - **Docs:**
    - network filesystems (NFS, SMB, any `local_lock` setup) are **unsupported** for the data directory, and the docs no longer promise that a failure is detected there;
    - recovery's foreign-process check sees only its own PID namespace, so before a recovery the operator stops every process that writes the repository, in any container.
  - **The CHANGELOG's upgrade note:** stop every 0.2.x process before starting 0.3.0, and delete any `import-<pid>-<nanos>` directory that 0.2.0's crashed imports left in the scratch directory (with the path to look in).
  - **Tests:**
    - an export beside a sweep from another PID namespace;
    - a `build` given a tree with a file missing;
    - the reviewer's 2-second stress of `is_at`, which fails when the check is forced true.
- **K66 — a changed reference clears the check** (B-b; the amended clarification).
  - A curation that changes a source's reference clears its `last_reviewed` and `availability`, unless the same proposal records a new review day or availability.
  - A correction of title, author or date keeps them. The proposal page then says *the review of <day> stays: accept only if this is still the same work*, and names the other claims that cite the source.
  - A white-space change of a name is shown as such: the two spellings with their white space made visible.
  - Tests through the core, the form and the API, en and ja.
- **K67 — the removal clause, said once** (B-a, decision 3). `know.not_curation`, `health.rule_status`, `Rule::Status` and the write API's `knowledge-curated` explanation lose the removal and unreadable-file clauses. A refusal that is about a removal or an unreadable file says `know.not_by_form` instead (`Refusal::removes` and `UNREADABLE` choose). Test: no page or API body carries both sentences, en and ja.
- **K68 — the request schemas are standard and the test sees closedness** (B-c).
  - `superseded_by` is an `anyOf`, or a `pattern` on the id branch, valid under OpenAPI 3.1.
  - Ids carry the lower-case v7 `pattern`.
  - The test's walk honours `additionalProperties`: open unless `false`. A row shows that removing `closed()` fails.
  - Rows for the trimmed `superseded_by` and for a byte-length bound (or the schema says bytes).
- **K69 — the cycle refusal and its cost** (B-d, B-e, decision 4).
  - The form names the source whose chain returns, at its field.
  - The surfaces read each source once per request.
  - `validate` remembers the sources already walked, so a chain costs O(N).
  - The form's hint says that a change of line breaks alone is made through the API.
- **K70 — `--config` once** (A-e). A second `--config`, or a flag in its place, is a usage error.

## Corrections from the review of 2026-11-14, eighth round (binding; **before 0.3.0**)

K65 is **Approved for the scratch directory**; K66 – K70 are **Conditionally Approved**
(`.git-exclude/reviewed/005-knowledge-layer-corrections-7-review-result.md`). The clarification *correcting a source* is amended
again. Hand back K71 – K75 as one request, `005-knowledge-layer-corrections-8-review-request.md`. **K71 first.**

- **K71 — the reference rule holds at acceptance** (D-a, decision 4).
  - At acceptance, a written source whose reference differs from the **tip's** may carry a review day or an availability only when this proposal states that check (K72). Otherwise it is refused by field (`last_reviewed`), with a sentence saying why.
  - The merge makes a reference changed on one side and a check changed on the other an overlap, so the re-authoring asks a person.
  - Tests: the reviewer's two orders, each refused or overlapping, never merged into *checked*; the compose path unchanged.
- **K72 — a check of a new reference is stated** (D-b, decision 3).
  - When a curation changes a reference, the form shows a separate, empty *the day you checked the new reference* field, with its availability. The API takes the same as an explicit member (`checked: {day, availability}`, or the name you choose, said in OpenAPI).
  - The old day is never carried by inference.
  - The day must be no earlier than the source's previous review day and not after today in UTC+14. A day outside that is refused by field.
  - A white-space-only change of a reference keeps the check.
  - The form hint, the API description and the `curate` doc comment say exactly this (en, ja, glossary).
  - The API test that kept `2023-11-10` is turned around.
- **K73 — the cited-by line and the stale sentence** (D-c, D-d).
  - The line says *a claim that cites it* (ja likewise), and lists from the tip's index.
  - The held-draft `submit_draft` path and `words.rs` choose their sentence through `curate::sentences`, so a removal or unreadable-file refusal says `know.not_by_form`. A test of a held draft whose file became unreadable.
- **K74 — the export's partial sweep stays inside what the export made** (C-b, C-c).
  - Beside a destination, a partial is swept only when its lock file already exists and its tail is the three-part `<pid>-<start>-<n>`. A directory with no lock file, or with the two-part tail, is never touched there. The two-part tail is swept only inside the service's scratch directory.
  - The upgrade note tells operators to remove a 0.2.x `.<dest>.partial-…` by hand.
  - `build` compares the per-path entries (path and size) from `storage.tree`, not the totals.
  - A destination name too long for its partial is refused as *the name is too long by N bytes*.
  - Tests: the reviewer's `~/exports/.notes.partial-2026-11/` stays whole; a swap of two files of equal size is refused.
- **K75 — small points** (D-e, D-f).
  - **The test's pattern reader** refuses any escape or class it does not implement. Its `\s` is ECMA-262's set, and the handler's `trim` agrees with it or the schema says the handler's set. `oneOf` and `anyOf` check their sibling keywords.
  - **White space shown:**
    - a literal marker character in a value is itself escaped (as `[U+2423]`), so markers cannot be imitated;
    - the legend names `[U+XXXX]`;
    - a missing `was` says *none*;
    - a white-space-only reference change is shown as one.
  - **The cycle refusal** names a source by its whole id in `<code>` beside its title.
