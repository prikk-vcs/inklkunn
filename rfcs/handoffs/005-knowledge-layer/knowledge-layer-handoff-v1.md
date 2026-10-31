# The knowledge layer — Developer Handoff v1 (RFC 005 part 2; M3)

| | |
|---|---|
| Document | Claims, sources and views (RFC 005 part 2, D-1 … D-12), on part 1's decisions D-1 … D-4. |
| Status | RFC 005 part 2 accepted by the owner 2026-10-31. **K-T1 … K-T4 are assignable now** (files, validator, merge, index: no surface, no sign-in needed); **K-T5 onward after 0.2.0 is cut** (they need signed-in proposers, the proposal path served and the reviewers' forms). The spike's follow-up K1 goes first. |
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
