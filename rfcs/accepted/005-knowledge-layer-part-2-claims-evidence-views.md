# RFC 005 part 2 — Claims, sources, views: what a reader can ask of a sentence, and how a community records disagreement without a truth mark

**Status.** **Accepted by the owner 2026-10-31** (*"Accepted."*; the open question ruled as recommended: reviewers and
above create views by default). Handoff: `../handoffs/005-knowledge-layer/knowledge-layer-handoff-v1.md`. Proposed by the architect on 2026-10-31, on part 1's decisions D-1 … D-4 (accepted by the owner the same
day). Reviewed by the architect against the owner's two lenses before proposing (see *How this was checked*). **Tracks.** ROADMAP M3 (0.3.0), theme T5, risk R4. Requirements `KN-1 … KN-9`, `CM-5`, `CM-7`, `CM-11`, `NG-3`,
`PU-6`, `PR-1`; the requirements register's items **3** (evidence), **4** (state classes), **5** (discussion kinds),
**6** (fork kinds), **10** (no "canonical"); external design `WEB-11`; RFC 005 part 1 (D-1 … D-4); RFC 004 (every change
is a proposal; the block-aware merge; the inbox); RFC 003 (the read path, the derived indexes, the health lists, the
API); wiki-commons XFER-1 (`extensions_used`). **Touches.** `inklkunn-markup` (nothing new: the `^id` rule exists),
`inklkunn-core` (the knowledge files' reader and validator, the derived knowledge index, provenance, the merge of
knowledge files, the curation proposals), `inklkunn-store` (claim discussion records), `inklkunn-web` (the surfaces),
the read and write API. **Not here:** governance and moderation, governance forks, free-standing page talk (RFC 006);
bundle import from other engines (RFC 007).

## Summary

A reader of an inklkunn page can ask three things of any sentence: **where did this come from** (*Why is this here?*),
**what supports it** (its sources), and **do people disagree** (its other views). This RFC answers them with as little
new machinery as possible: claims and sources are **small files beside the pages** (part 1's D-2), changed only by
proposals (RFC 004), read through a **derived index** like the title index, and shown in plain words that never say or
suggest "true". A newcomer who never touches a claim sees an ordinary wiki with one quiet link per paragraph.

## Decisions

- **D-1 — A claim file.** `knowledge/claims/<id>.yaml` (plain YAML subset, the frontmatter rules), id a UUID v7:
  `kind` (KN-3: `observation | interpretation | hypothesis | opinion | decision | policy | question`, or `unknown`);
  `expressions` — the anchors `{page_id, block}` (`block` the `^id` in the page source, part 1 D-1 a; a claim may have
  expressions in several pages and languages); `relations` — `{type: refines | supersedes | contradicts | split-from |
  merged-into, claim}`; `lifecycle` (`proposed | adopted | retired | unknown`); `community_status` (`accepted | contested
  | unresolved | unknown`, with the proposal that decided it); and `sources` — the claim's relations to source files:
  `{source, relation: supports | challenges, bearing: direct | partial | indirect, note, judged_on}`. **`bearing` says how
  directly the source speaks to the claim, never how true either is** (KN-5: no equal weighting is implied, and no
  number is). `judged_on` is the digest of the expression's text when the relation was judged: when the text changes,
  the relation (and the community status, which carries its own `judged_on`) is shown as **judged on an earlier wording**
  until a reviewer confirms it (part 1 D-1, point 3). **Scope** (part 1 D-4) is the view the file is read in: a view is a
  branch, so each view's files are that view's state; the field is the view's name and is never stored twice.
- **D-2 — A source file.** `knowledge/sources/<id>.yaml`: `kind` (`primary | secondary | observational | computational |
  anecdotal | other`); `identity` (a title, an author or publisher as written, a date as written); `reference` (a URL, a
  DOI, an ISBN, a page in this wiki, or a citation in words); `snapshot` (an attachment of an archived copy **only where
  its licence allows**, else an archive address and a digest); `last_reviewed` (a date and the proposal that set it, KN-8);
  `superseded_by` (a source, when a newer edition or a correction replaces it); `availability` (`available | unavailable
  | unknown`, set by a person who checked, never by a crawler). A source is shared by any number of claims, and each
  claim states its own relation to it (register item 3: the same evidence cited differently). A source can itself be
  challenged: a claim about the source (*this survey's sample was 40 people*) is an ordinary claim whose expression is on
  the source's page in this wiki, if someone writes one.
- **D-3 — Changes are proposals; curation is small.** Every change to a claim or source is a proposal (PR-1, part 1 D-3):
  attaching a source to a paragraph creates the claim (if none) and the source in **the same proposal** as any page edit
  it belongs with; changing a status, a relation, a bearing or `last_reviewed` is a **curation proposal** (its own
  small form, no page text). `community_status` changes **only** through an accepted curation proposal (KN-2): never by
  a count, a vote tally or a timer. Reviewers see, on the proposal page, the claims a change touches with each *judged on
  an earlier wording* consequence and one re-confirmation step for all of them (part 1 D-1 c); their decisions are
  recorded and measured (part 1 D-1 d).
- **D-4 — The knowledge files merge, never resolve.** RFC 004's merge gains the knowledge files: YAML key by key (as
  frontmatter), lists by stable identity (`expressions` by anchor, `relations` by `(type, claim)`, `sources` by source
  id); two sides changing the same value are an **overlap** with both texts, never resolved. A claim file whose anchors
  name blocks that no longer exist is refused at acceptance with the anchors named (the proposer removes the expression
  or keeps the block).
- **D-5 — Validation at acceptance.** A knowledge file is validated by inklkunn before the signer is asked: the schema;
  ids well formed and unique; every `source` and `claim` it names present in the same view; no owned page key affected.
  An invalid file is refused by name; a repository imported or written by other means that already holds invalid files
  serves its pages and reports each invalid file on the health list, never a half-read claim.
- **D-6 — The derived knowledge index.** Built from the **same `extract` per tip** that builds the title and link
  indexes (RFC 003 D-2): claims, sources, relations, anchors and, per expression, whether the anchor's text still matches
  each `judged_on`. Never a second source of truth; rebuilt with the derived store cleared (CM-7, C-11). Bounded like the
  title index (`INKLKUNN_INDEX_MAX_MIB`, the file count).
- **D-7 — What a reader sees** (less is more, PU-6). On an ordinary page, nothing new **except one quiet link per
  paragraph that holds a claim** — *Sources and views* — and **one** *Why is this here?* link among the page's tools,
  leading to a page that shows where each paragraph came from (D-8); a paragraph without a claim carries no extra
  control, so a page with no claims reads exactly as before. The claim page says, in words: what kind of statement it is; *Sources:* each with
  its relation and bearing in words (*supports, directly*; *challenges, in part*), and *judged on an earlier wording* where
  that is so; *Status in this view:* (*accepted*, *contested*, *unresolved*, *not decided*) with the proposal that decided
  it; *Other views:* each view where the claim reads differently, with a link to compare. **Never** the words *true*,
  *false*, *verified*, *proven*, *canonical*, or a colour, icon or number that ranks (the retired-words test grows; the
  word *Verified* stays reserved for prikk's verification of history, so a reader cannot confuse the two).
- **D-8 — *Why is this here?*** (KN-6). One page per wiki page, listing its paragraphs, each with an anchor so a single
  paragraph can be linked to; for each paragraph: the accepted change that last changed it and, back through
  the page's history, the change that first wrote it; for each, the proposal, the people as its message records them,
  the review (comments and decisions), the acceptor, and the sources attached in that change. Derived from prikk's
  history and the service's records only; for a paragraph that holds a claim the `^id` gives the chain exactly; for any
  other paragraph the page's history is walked block by block with the product's own segmentation, **bounded** (the
  history page's bounds; beyond them the page says so), and the paragraph's chain is labelled *traced by matching the
  text* where no anchor carries it.
- **D-9 — Views are where disagreement lives** (KN-5, KN-9, CM-11). An **alternative view** is a branch with `forked_from`
  and a `kind`: `experimental` (a working state, may be merged back or dropped) or `interpretation` (a competing
  understanding, kept beside the default view). A governance fork is RFC 006's. *Compare views* shows, claim by claim, what
  differs (wording, status, sources) and page by page the differences (RFC 003's diff). Readers see the default view as
  **the default view** (the glossary's word, already shipped); no view is called canonical, adopted or official. Creating a view is the `create-view` action (RFC 002 D-2),
  allowed by default to reviewers and above; anyone may propose a change *in* a view they may read.
- **D-10 — Freshness** (KN-8). A source's `last_reviewed` is set by a curation proposal; a source older than the wiki's
  setting (`knowledge.review_after`, default 3 years) and every source marked `unavailable` appear on a new health list,
  *Sources to review*, with the claims that rely on them. No link checker or crawler runs: a person checks, and says so.
- **D-11 — Discussing a claim** (KN-7, register item 5). A claim has a **thread** (an *epistemic* discussion), with the
  proposal-review thread's records and rules (service-authoritative, append-only, exported with the bundle's
  discussions, CM-7) and its own label; a thread never changes a claim: a decision does, through D-3. Governance and
  moderation discussions are RFC 006's.
- **D-12 — Interchange and API.** `knowledge/` is exported as is, declared in the bundle manifest's `extensions_used`
  (`inklkunn-knowledge@1`), and degrades to readable YAML beside readable pages (wiki-commons XFER-1); the import of
  another inklkunn's bundle validates it (D-5). The read API gains `claims/{id}`, `sources/{id}`, a paragraph's
  provenance and a view comparison; the write API gains the curation proposal; all in the OpenAPI document, existence
  neutral, authorized as the pages are.

## What "done" means (acceptance criteria)

- D-1, D-2: every field round-trips; `judged_on` marks *judged on an earlier wording* exactly when the anchor's text
  changed, and a reviewer's confirmation clears it in one step for all the claims of a change.
- D-3: no path changes a claim or source without an accepted proposal; `community_status` cannot change by any other
  means (a test tries each).
- D-4: the knowledge merge on a corpus of knowledge-file pairs (from spike C's histories and hand cases): clean where no
  value is changed twice, an overlap with both texts otherwise; never a lost relation.
- D-5: malformed files refused at acceptance by name; a repository with planted invalid files serves its pages and lists
  them.
- D-7: the retired-words test covers the forbidden words in both languages; every new page state passes the structural
  and catalog tests; a page without claims renders byte-identical to 0.2.0's except the one *Why is this here?* link.
- D-8: the chain for a claim paragraph and for an ordinary paragraph on real histories, within the bounds, labelled.
- D-9: compare views on two views that differ in wording, status and sources; *canonical* appears nowhere a reader sees.
- D-10, D-11, D-12: the health list; threads exported; the bundle validated by wiki-commons' schema with the extension
  declared; API drift tested both ways.

## How this was checked against the owner's two lenses

- **Safe by default:** no crawler or outbound fetch; snapshots only where a licence allows; creating a view needs a
  reviewer; nothing changes a claim without an accepted proposal; invalid files refused, never half-read.
- **Less is more for a newcomer:** a page with no claims looks as it does today plus one link; claims appear where someone
  attached a source; the newcomer is never asked about identity (part 1 D-1); curation has its own small form.
- **Nothing that can mislead:** no word, colour or number that ranks or says *true*; *Verified* stays prikk's word for the
  history alone; *bearing* describes how a source speaks to a claim, not how good either is; *judged on an earlier
  wording* says exactly what happened; the default view keeps its shipped name; *Why is this here?* labels a chain that
  was traced by matching text rather than by an anchor.
- **Clean and robust:** one store of truth (the files), one derived index from the extract that already runs, the
  existing proposal and merge paths, the existing health-list and API patterns.

## Alternatives considered

- **A claims database beside the repository** (faster queries). Rejected: claims are VCS-authoritative (CM-7); a second
  store would be a second truth to reconcile. The derived index gives the speed.
- **A computed evidence score or a "supported" badge.** Rejected (NG-3, KN-2, KN-5): any single word or number reads as
  truth. The page lists the relations in words instead.
- **Annotating every paragraph as a claim.** Rejected (part 1 D-1 point 1): most text is not a claim anyone contests.
- **Automatic link checking of sources.** Rejected for M3: outbound fetching from the server is a privacy and egress
  question (RFC 002 C8's confinement exists, but crawling third parties on a schedule is a policy matter), and "a person
  checked" is the honest freshness signal.

## Open question (ruled by the owner, 2026-10-31: as recommended)

1. **Who may create an alternative view** (D-9)? *Recommendation:* reviewers and above by default (a view is a lasting
   community object, and it is named on every claim page that differs in it); anyone may propose a change in a view
   they can read; a wiki may change this by policy when RFC 006 lands.

## Sequencing and handoff

On acceptance the architect writes `rfcs/handoffs/005-knowledge-layer/knowledge-layer-handoff-v1.md`. It is assigned
when M2 (0.2.0) is cut, since it builds on the proposal path and sign-in; the files, the validator, the merge and the
index (D-1 … D-6) may start earlier if the dev team has room, as they touch no surface.

## Clarifications (the architect, 2026-11-08, from the review of K-T5 – K-T8)

- **D-3, the re-confirmation step.** The reviewers' one step on the proposal page (part 1 D-1 c) proposes the re-judgement of
  every claim the change touches as **one curation proposal, once the change is accepted**; until that proposal is accepted,
  each such claim reads *judged on an earlier wording*. A digest so never names wording that is not in the history, and a
  status keeps changing only through an accepted curation proposal (KN-2). The re-judgement is shown on its proposal's page,
  line by line, like every other change of trust (part 1 H-1 point 3).
- **D-3, what curation covers.** A community status; a source's `last_reviewed` and `availability`; and any change to an
  existing source link (its `relation`, `bearing` or `judged_on`, or its removal). Attaching a new source link stays with the
  page edit it belongs to.
- **Part 1 D-1 d, what is measured.** Per accepted change and per claim it touches: *kept* (the default of H-1 point 2),
  *dropped* (a confirmed removal of the expression), and later *re-confirmed by* (the curation proposal that re-judged it). An
  **override** is the default rejected: a confirmed drop, or *request changes* on a proposal that touches a claim. The
  participation metrics report the overrides beside the defaults kept.
- **Part 1 D-1 d, how it is reported (2026-11-14).** An override stays the concept; it is reported as **two counts — drops (claims)
  and change requests (decisions) — never summed**, since they count different things. Changes accepted before the review record
  existed (store schema 16) have no records, and the metrics say so.
- **D-3, a new link (2026-11-14).** A link attached in any proposal is judged on a wording the claim's paragraph has in the result of
  that proposal: a digest never names wording that is not in the history.
- **D-3, every judgement (2026-11-14).** The rule of *a new link* holds for **every** `judged_on` a proposal sets or changes:
  a link's or a community status's, in a page edit or a curation proposal, through the form, the API or a raw edit. Each one
  names a wording that the claim's paragraphs have in the result of that proposal.
- **D-3 and D-7, naming a subject (2026-11-14).** A line that names a claim or a source carries its **whole** identifier,
  linked to the subject's page. Identifiers are written by whoever writes the file, so a shortened form can be copied, and a
  page that names one subject could then read like a page that names its twin.
- **D-10, whose setting (2026-11-14).** Wikis have no settings of their own until RFC 006's policy as data. Until then
  `knowledge.review_after` is **the operator's, one value for every wiki on the instance**
  (`INKLKUNN_KNOWLEDGE_REVIEW_AFTER_DAYS`, default 1096 days). Every surface that names it says so.
- **D-11, a thread in the bundle (2026-11-14).** wiki-commons' `discussions/` holds one file per **page**, with bodies in Markdown
  (XFER-10). A claim's thread is neither, so it is the knowledge extension's data:
  - it is exported where K-T11's extension (`inklkunn-knowledge@1`) declares;
  - its bodies are carried as fenced text blocks (amended 2026-11-14 from *escaped*), so that every Markdown reader shows exactly the plain text inklkunn showed;
  - the manifest's `discussions` stays a statement about page discussions.

  Proposal comments are interchange of RFC 004's records, so they go to RFC 007 (M5), not to 0.3.0.
- **D-12, what an import may carry (2026-11-14).** An import is the service writing: the page rules are the import plan's, and
  the knowledge rules are an acceptance's. It may bring claims, sources and links, **but no curated state**: no decided
  status, no review day, no availability, and no source that is checked. Such a bundle is refused whole, by file and rule,
  never stripped: stripping would be a decision, and carrying the state would assert decisions this wiki's people never made.
  Importing curated state with its provenance, and reading back threads and view records, are RFC 007's (M5).
- **D-3, correcting a source (2026-11-14).** A correction of an existing source's identity (title, author, date), reference
  or `superseded_by` is made by a curation proposal **whether or not the source is checked**, since no page edit writes an
  existing source file. On a checked source only curation may make it (K30). A `superseded_by` chain never returns to a
  source it has passed.
  *Amended 2026-11-14:*
  - **A change of a source's reference** (where it is found) is a change of what was checked. It clears the source's review day and availability, unless the same proposal records a new check.
  - **A correction of its title, author or date** keeps them. The proposal's page says that the review of that day stays and names the other claims that cite the source, so the reviewer who accepts it vouches that it is still the same work.
- **What is not offered in 0.3.0 (2026-11-14).** Removing a claim file or a source file, and replacing a file that cannot be
  read. Every surface that meets one says so plainly, and names no reviewer or form that cannot act on it. Retiring claims
  and sources is designed with RFC 006 (moderation, M4).
