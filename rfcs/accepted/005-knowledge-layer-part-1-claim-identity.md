# RFC 005 part 1 — The knowledge layer's ground: when a claim stays the same claim, how claims attach to pages, and what one change is

**Status.** **Accepted by the owner 2026-10-31** (*"Yes. Accepted."*, after the corpus section and H-1 were revised at
the owner's request the same day). Handoff: `../handoffs/005-knowledge-layer/claim-identity-spike-handoff-v1.md`.
Proposed by the architect on 2026-10-31 (ROADMAP §7: "the claim-identity and patch-granularity
investigation", due once M1 is under way; M1 shipped as 0.1.0 the same day). An **investigation** RFC: it fixes
the questions, the working hypotheses, the prototype that tests them and the measures that decide; part 2
(claims, evidence, views, provenance, disagreement as branches) is designed only after its results.
**Tracks.** ROADMAP M3 (0.3.0), theme T5, risk R4; the owner's ruling DR-1 (wiki-first). Requirements
`KN-1 … KN-9`, `CM-5`, `NG-3`; the requirements register's P0 items **1** (what a claim is), **2** (the epistemic
model), **11** (graph-native or document-native), **12** (patch granularity); external design `WEB-11`,
`GATED-1`; internal design `NR-5`; RFC 001's reserved `knowledge/`; RFC 004's block segmentation, block ids on
commented blocks (CM-5) and *one accepted proposal = one sealed change*.
**Touches.** Nothing in the product. The prototype lives under `tools/` and `rfcs/handoffs/005-…/spikes/`, as M0's
spikes did.

## Summary

M3 puts claims above pages: *this sentence states a claim; here is its evidence; here is where people disagree.*
Four questions decide whether that layer is clear to people or a source of arbitrary choices, and the register
asks that they be prototyped before anything is frozen:

1. **When does an edit keep a claim and when does it make a new one?** ("The bridge opened in 1950" → "The bridge
   was opened in 1950" is the same claim; "… in the early 1950s" refines it; "began operating in 1950" is related,
   not equal.)
2. **Are claims the store and pages a projection, or the reverse, or both?** (DR-1 chose pages first; this part
   checks that claims can sit on pages without strain.)
3. **What is one change?** (One semantic act, one session, one coherent contribution, or many small patches.)
4. **Which states does a claim have,** without a single status that reads as truth?

This part proposes an answer to each as a **hypothesis**, a prototype that replays real edit histories through
it, and the measures that accept or reject each one.

## Working hypotheses (to be validated, not decided)

- **H-1 — A claim's identity is a recorded human decision, never a computed one; and trust never moves silently.**
  (Revised 2026-10-31 at the owner's request, against the owner's two lenses: a safe, clean end state, and nothing a
  person can misread.)
  1. **Only explicit claims have an identity.** Most text is not a claim anyone will contest. A claim exists where
     someone made it one (by attaching evidence, or marking it contested), as a small file `knowledge/claims/<id>.yaml`
     with an id minted when it is first proposed; its **expressions** are block anchors (`[[Page#^id]]`, in a view, in
     any language). Ordinary text carries nothing.
  2. **An edit to a claim's wording keeps the claim** (its id and its history): rewording is the common case, and a new
     claim per edit would scatter its history (the register's failure).
  3. **Trust is never carried over silently.** Evidence relations and community status are bound to the **wording they
     were judged on**. When that wording changes, they are shown as *judged on an earlier wording* until a reviewer
     confirms them again. A change of meaning that nobody noticed therefore cannot borrow the support or the standing
     the old wording had: the danger of the default in point 2 is closed by this point, not by asking everyone.
  4. **A change of meaning is a decision inside the accepted change.** *Refines*, *supersedes*, *contradicts*,
     *splits*, *merges with* are recorded in the claim files of the proposal that makes them, so the existing review
     and acceptance decide it (no new authority), the history shows who decided, and a later proposal can correct it
     (succession is a relation, never a deletion).
  5. **The service states observable facts, never meaning.** It may show that a block's anchor was kept and how much
     of its wording changed; it never suggests *same* or *different* (a suggestion about meaning biases the person
     and reads as a fact; and automatic equivalence is wrong exactly in the cases that matter, NG-3).
  6. **No newcomer is asked about identity.** A person editing a page edits text. The identity question appears
     **only to reviewers**, on the proposal page, **only for blocks that hold claims**, with the default (point 2)
     stated and the trust consequence (point 3) visible.
  7. **Across views and languages**, a claim keeps one id; two views that decided differently about one claim are an
     overlap when they meet, never resolved by the service (as RFC 004's merge).
  *Rejected:* identity as a hash of normalized text (every rewording a new claim); identity by automatic semantic
  equivalence (opaque, language-dependent, a truth judgement by machine); asking every editor (confusing, and most
  text is not a claim).
- **H-2 — Hybrid, pages first** (register item 11, option C; DR-1). Pages stay the stored artifact. Claims and
  evidence are files in `knowledge/`, pointing to block anchors and to each other; relations live in the claim
  files. A **view** is a prikk view, so "disagreement as branches" uses what exists. Any graph query (what refines
  what; which claims a source supports) is a **derived index** rebuilt from the files at a tip, like the title index:
  never a second source of truth.
- **H-3 — One change is one coherent contribution** (register item 12). As built in RFC 004, one accepted proposal is
  one sealed change. A claim annotation travels **in the same proposal** as the page edit it belongs to; pure
  curation (a claim's status, a relation, a piece of evidence) is a small proposal of its own. No change is split
  below what a person decided as one act, and none bundles acts a person decided separately.
- **H-4 — Epistemic state is several plain fields, each with `unknown`** (register item 2; KN-2): *lifecycle*
  (proposed, adopted, retired), *community status* (accepted, contested, unresolved), *evidence state* (supported,
  mixed, challenged), *scope* (the default view, a named view). No aggregate, no score, no colour that reads as
  "true". Community status changes only through a recorded decision.

## The investigation (spike C)

**Corpus.** Real revision histories of a small set of encyclopedic articles, in **English and Japanese**, chosen for
factual statements that people corrected over time (dates, figures, attributions), from a public wiki under CC BY-SA
4.0; plus M0's seeded generator for volume. Target: about 2,000 consecutive edits across 20 – 30 articles.

*What real histories are good for:* (a) **the ratio of rewording to change of meaning** — H-1's default and the
reviewers' burden stand or fall on it, and a generator would return only the ratio written into it; (b) **real
structure in two scripts** — lists, tables, references, Japanese without spaces between words — through the product's
own block segmentation, which is what a claim's anchor rests on; (c) **the hard cases** — edit wars, reverts, partial
corrections (*1950* → *the early 1950s*) — which the relations and the *contested* / *unresolved* states must express;
(d) **volume** — changes per claim, and so notifications. *What they are not good for:* they **estimate, they do not
predict**: those edits were made directly, not as reviewed proposals, so inklkunn's numbers will differ and the
report says so.

*Handling.* Fetched at run time through the wiki's public API, politely (a stated user agent, a rate limit), pinned by
revision id (a revision deleted upstream is skipped and counted, never silently replaced). **No editor identity is
read or kept** (user names and IP addresses are personal data; only revision ids and text are used). **No article text
is committed** (share-alike would bind any stored adaptation); the repository keeps the measurements, the labels by
revision id, the list of revision ids and an attribution file.

**Prototype.** A tool (`tools/corpus`, a new `claims` subcommand) replays each history as a sequence of proposals
through the product's own block segmentation and merge (`inklkunn_markup::blocks`, `inklkunn_core::merge`), carries
block ids forward as RFC 004 does, and records for each edited block: did its `^id` survive, how much did the wording
change, which other blocks changed in the same edit. **Two labellers**, working apart under one written rule sheet,
mark a sample of 400 edited factual sentences as *same claim*, *refines*, *supersedes*, *contradicts*, *new claim*,
or *cannot tell*.

**Measures, and what they decide.**

| # | Measure | Accepts | If not |
|---|---|---|---|
| M-1 | How often H-1's default (a surviving `^id` with a wording edit is the same claim) agrees with the labellers | ≥ 90 % | the default becomes "ask", and part 2 designs the question |
| M-2 | How many identity questions **reviewers** would face per 100 edits under H-1 (only blocks that hold claims) | ≤ 5 | the question is asked only when evidence or status is re-confirmed |
| M-2b | How often a labelled change of meaning would have kept evidence or status attached (the trust that point 3 stops from moving silently) | reported; every such case shown as *judged on an earlier wording* | point 3 is wrong and part 2 redesigns the binding |
| M-3 | How often the two labellers disagree | reported; above 15 % the rule sheet is the problem, and part 2's words are designed for the cases they split on |
| M-4 | How many changes a typical claim's history has, and how many touch only its wording (H-3: provenance readable) | reported | a "wording only" filter in provenance (part 2) |
| M-5 | Notifications per watched claim per 100 edits under H-3 | reported against RFC 004's inbox bound | grouping of curation notices (part 2) |
| M-6 | Whether every labelled case maps onto H-4's fields without a new state | every case | the missing state is named and added |

**Honesty.** Labellers who are models are said to be models; the rule sheet, the two label sets and their
agreement are published with the results. The tool measures the product's functions, never a copy.

## What part 1 decides once the spike reports

The architect writes the results report and, with it, the **identity rule** (H-1 as confirmed or amended), the
**storage shape** of `knowledge/` (H-2), the **change rule** (H-3) and the **state fields** (H-4), as decisions D-1 …
D-4 of this RFC, for the owner's acceptance. Part 2 is designed on them.

## Alternatives considered

- **Claims-first now** (register item 11, option A): pages generated from claims. Faithful to "knowledge is a set of
  revisable claims", but it would replace a shipped, readable wiki with a model no one has exercised; DR-1 chose
  otherwise, and nothing here forecloses it later (a claim is a file either way).
- **Decide identity by argument, without a prototype.** The register calls these P0 investigations precisely because
  argument alone settles them wrongly.
- **Annotate every sentence.** Most text is not a claim anyone will contest; claims are made where someone attaches
  evidence or disagreement.

## Open questions for the owner

1. **The corpus** (the owner, 2026-10-31: "almost approved", asking what it is good for; answered in *Corpus* above):
   real revision histories of a public wiki under CC BY-SA 4.0, English and Japanese, fetched at run time, no editor
   identity read, no text committed. *Recommendation:* yes.
2. **The principle of H-1** (the owner, 2026-10-31: "sounds good", asking for a careful review; revised above with
   points 1 – 7: only explicit claims have identity; wording edits keep it; **trust never moves silently**; meaning
   changes are decided inside the accepted change; the service states facts, never meaning; no newcomer is asked).
   *Recommendation:* yes.

## Sequencing and handoff

On acceptance, the architect writes `rfcs/handoffs/005-knowledge-layer/claim-identity-spike-handoff-v1.md` (the tool,
the corpus fetcher, the rule sheet, the measures). The spike runs beside M2's remaining work and touches no product
crate. Part 2 follows its report.
