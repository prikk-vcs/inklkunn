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
