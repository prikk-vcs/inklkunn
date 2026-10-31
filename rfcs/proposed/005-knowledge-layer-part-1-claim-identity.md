# RFC 005 part 1 — The knowledge layer's ground: when a claim stays the same claim, how claims attach to pages, and what one change is

**Status.** **Proposed** by the architect on 2026-10-31 (ROADMAP §7: "the claim-identity and patch-granularity
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

- **H-1 — A claim's identity is a recorded human decision, never a computed one.** A claim is a small file
  `knowledge/claims/<id>.yaml` with an id minted when it is first proposed. Its **expressions** are block anchors
  (`[[Page#^id]]`, in a view). **Editing an expression's wording keeps the claim by default.** When the meaning
  changes, the proposer says how — *refines*, *supersedes*, *contradicts*, *splits*, *merges with* — and the
  reviewer sees it as part of the change. The service may **suggest** (the block's `^id` survived the edit; the
  wording changed little), but it never decides, and it never shows a suggestion as a fact.
  *Rejected:* identity as a hash of normalized text (every rewording would make a new claim: exactly the failure
  the register warns of); identity by automatic semantic equivalence (opaque, language-dependent, wrong in the cases
  that matter, and a truth judgement by machine, NG-3).
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
factual statements that people corrected over time (dates, figures, attributions) — fetched at run time from a
public wiki under CC BY-SA 4.0, **with only the measurements, the labels by revision id and an attribution file
committed** (no article text in the repository). Plus M0's seeded generator for volume. Target: about 2,000
consecutive edits across 20 – 30 articles.

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
| M-2 | How many identity questions a proposer would face per 100 edits under H-1 | ≤ 5 | identity is asked only at adoption, not at every edit |
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

1. **The corpus:** may the investigation use real revision histories of a public wiki under CC BY-SA 4.0, English and
   Japanese, fetched at run time, with only measurements, labels by revision id and an attribution file committed?
   *Recommendation:* yes; synthetic edits alone would measure the generator, not people.
2. **The principle of H-1:** is "a claim's identity is a recorded human decision; the service may suggest, never
   decide" the principle you want the layer built on? *Recommendation:* yes; it is the one that cannot mislead a reader
   about what the system knows.

## Sequencing and handoff

On acceptance, the architect writes `rfcs/handoffs/005-knowledge-layer/claim-identity-spike-handoff-v1.md` (the tool,
the corpus fetcher, the rule sheet, the measures). The spike runs beside M2's remaining work and touches no product
crate. Part 2 follows its report.
