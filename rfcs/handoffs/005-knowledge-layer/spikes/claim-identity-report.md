# Spike C — claim identity: results report (RFC 005 part 1)

| | |
|---|---|
| Written by | the dev team, 2026-10-31, for the architect, who decides D-1 … D-4 |
| Inputs | `articles.txt`, `revisions.txt`, `rule-sheet.md` (v1, unchanged), `pool.jsonl`, `labels-A.jsonl`, `labels-B.jsonl`, `measures.json`, `stratified.json`, `replay-counts.json`, `fetch-report.json`, `convert-report.json`, `ATTRIBUTION.md` — all in this folder; the tool is `inklkunn-corpus claims` (`tools/corpus/CLAIMS.md`) |
| Status | measurement only; **it recommends, it does not decide**. No product behaviour changed (one additive public function, see the deviations) |

## 1. In one page

| # | RFC's acceptance line | Result | Verdict |
|---|---|---|---|
| M-1 | ≥ 90 % agreement of H-1's default (a surviving `^id` with a wording edit is the same claim) | **92.4 %** overall (146 of 158), **95.9 %** Japanese, **86.9 %** English. **But 84 % (47 of 56; 95 % interval 72 – 91 %) for small edits** (an edit that changes at most two other blocks), and 97 % for the sweeping edits that carry 87 % of all events | **Met overall; not for small human edits, and not for English.** Read with §4 |
| M-2 | ≤ 5 identity questions per 100 edits | **187 per 100 revisions** if every factual block were a claim (91 per 100 without the sweeping revisions) | **Not met as an upper bound**; met only if ≤ 2.7 % (5.5 % without sweeping revisions) of factual blocks hold claims. So the fallback applies: *ask only when evidence or status is re-confirmed* |
| M-2b | reported; each case shown as *judged on an earlier wording* | Of 32 changes of meaning both labellers agreed on, **8 (25 %)** had a surviving id: one in four would have kept its evidence or status attached silently without H-1 point 3 | **Point 3 is needed and is right** |
| M-3 | reported; above 15 % disagreement the rule sheet is the problem | **1.0 %** disagreement (4 of 400), kappa **0.956**; the only split is *refines* / *same-claim* | The rule sheet is not the problem (see the caution in §5) |
| M-4 | reported | median **1**, mean 2.83, p90 6, max 53 changes per id chain that changed at all; **92.4 %** of the changes of a surviving id are wording-only | A "wording only" filter in provenance is cheap and true |
| M-5 | reported against RFC 004's inbox bound | a watcher of one id chain would get **3.2 per 100 revisions** (median 1.1, p90 6.7, max 60) under H-3 | Far inside the inbox bounds; group per accepted change, not per claim |
| M-6 | every labelled case maps onto H-4's fields without a new state | **Yes**: 17 cases to place by hand, all placed (§6) | **Met** |

## 2. What was done

**Corpus.** 24 articles (12 English, 12 Japanese; list and reasons in `articles.txt`; no biography of a living person), the
newest 90 revisions of each at planning time, pinned by revision id in `revisions.txt`: **2,160 pinned; 2,158 fetched; 2 deleted
upstream (skipped and counted, never replaced); 0 unconvertible.** The text was read through the public API with `curl` as a
subprocess, one request a second, a user agent naming the project and a contact; the request names no user, comment or tag
field (a unit test holds this); the cache is outside the repository (the tool refuses a cache inside one); nothing of any
article is committed, and a test (`committed.rs`) greps every cached line of 40 characters or more against the committed folder.
Wikipedia is CC BY-SA 4.0; `ATTRIBUTION.md` carries the credit.

**Conversion.** Through wiki-commons' MediaWiki importer (`tools/wikicommons.py`, commit `19f021c`), the **built-in minimal
converter** (pandoc was not used), from a minimal export the tool builds itself (no contributor or comment element;
**synthetic timestamps**, because the importer wants one and the spike needs no time). The converter's report, over all 24:
templates and parser functions kept as source in snapshot envelopes, not expanded (41 – 1,328 per article where a count is given); tables
kept as wikitext source; media files and editors' IP addresses dropped. In practice a converted page is wiki markup with
templates, infoboxes and references still in their source form. This is **less faithful than what a production import would
give** and it matters for §4 (references are blocks).

**Replay.** Each article's converted revisions were walked oldest-first. Blocks are `inklkunn_markup::blocks`; the alignment of
an old revision's blocks with the next one's is **the merge's own** (`inklkunn_core::align_blocks`, the function `merge`
uses; see the deviations); ids are minted at the first revision and carried through that alignment, exactly as an accepted
proposal would. Wording change is counted from the product's word marks (`Hunk::refine`). Over 2,158 revisions the replay found
**26,635 edit events**: 5,821 `changed` (an aligned pair whose text differs; the id survives), 17,689 `replaced` (a run of
unmatched old blocks against a run of unmatched new blocks, paired in order; the id does not survive), 1,576 `added`, 1,549
`removed`; 166 reverts.

**Labelling.** The rule sheet (`rule-sheet.md`, written first, **not changed**) defines *factual* and the six labels with two
worked examples each in both languages. A pool of 1,200 events (`changed` and `replaced` only, those with an old and a new text;
600 per language; equal shares per article; seeded; blocks over 700 characters left out, 3,146 events) was drawn in a seeded
shuffled order. Each labeller marked *factual or not* and, if factual, one label. **Two model labellers, working apart under the
rule sheet only, each by separate agent runs of 160 – 200 items: labeller A is a Claude Sonnet model, labeller B a Claude Opus
model.** The first 880 pool items were labelled (22 packets); the sample is **the first 400 items both marked factual** (scanned
830; 430 skipped as not factual by at least one). Neither labeller was shown anything but the two texts of an item (snapshots
displayed as the template source they stand for). The owner, who reads Japanese, may spot-check Japanese labels; nothing
waits on it.

**The generator.** M0's seeded generator was **not used**: M-4 and M-5 came out of the real histories and the generator would
only have returned the ratio written into it.

## 3. The measures in full

(Computed by `inklkunn-corpus claims measure`; the split by edit size by `stratify.py`; both outputs are committed.)

| Measure | Overall | English | Japanese |
|---|---|---|---|
| Sample (both labellers factual) | 400 | 182 | 218 |
| **M-1** both `same-claim`, of the sample's `changed` items (id survived) | 146 / 158 = **92.4 %** | 86.9 % (53 / 61) | 95.9 % |
| M-1, labeller A alone / B alone | 93.0 % / 93.7 % | 88.5 % / 86.9 % | 95.9 % / 97.9 % |
| **M-2** reconfirmations per 100 revisions, all factual blocks counted as claims | 187.3 | 176.4 | 198.2 |
| M-2 per 100 edited blocks | 15.0 | 12.7 | 17.9 |
| M-2 without sweeping revisions, per 100 ordinary revisions | 91.4 | | |
| **M-2b** changes of meaning (agreed) whose id survived | 8 / 32 = **25 %** | 35 % | 13 % |
| **M-3** disagreement; kappa | 1.0 % (4 / 400); **0.956** | 0.5 %; 0.98 | 1.4 %; 0.94 |
| **M-4** changes per chain: mean / median / p90 / max | 2.83 / 1 / 6 / 53 | 2.99 / 2 / 6 / 53 | 2.64 / 1 / 7 / 22 |
| M-4 wording-only share of a surviving id's changes | 92.4 % | 86.9 % | 95.9 % |
| **M-5** notifications per 100 revisions for a watcher of one chain: mean / p90 / max | 3.2 / 6.7 / 60 | 3.4 / 6.7 / 60 | 3.0 / 7.9 / 25 |
| **M-6** cases to place by hand | 17 | 6 | 11 |

Agreed labels over the 400 (both labellers): `same-claim` 348, `new-claim` 24, `cannot-tell` 16, `refines` 5, `supersedes` 2,
`contradicts` 1. Labeller disagreements: *refines* against *same-claim*, four items; nothing else.

The split by the size of the edit (`stratified.json`), for the sample's `changed` items:

| Edit size (other blocks changed in the same revision) | n | both `same-claim` | 95 % interval |
|---|---|---|---|
| small (0 – 2) | 56 | 47 (84 %) | 72 – 91 % |
| medium (3 – 20) | 23 | 22 (96 %) | 79 – 99 % |
| sweeping (over 20) | 79 | 77 (97 %) | 91 – 99 % |

## 4. What the numbers say — and the caution that comes with each

1. **The replay is dominated by sweeping edits.** **87 % of all events happen in 11 % of the revisions** (revisions that change
   over 20 blocks at once: reference renumbering and reflow, template migrations, reverts of them). These are cheap to label
   (almost all `same-claim`), they lift M-1 to 92 %, and they pile up M-2. **A reviewer-facing design must treat a sweeping
   edit as one decision, not one question per block.** Of the 242 `replaced` sample items, 218 are from sweeping revisions.
2. **M-1 passes overall and fails where it matters most.** For small, human-sized edits the default "a surviving id with a
   wording change is the same claim" agreed with both labellers in **84 %** of 56 cases (interval 72 – 91 %); the failures are
   the product's one-for-one pairing: *a stretch of one block against one block is one block that changed*, however different
   they are (the merge's rule, tuned for merging; it is a reasonable rule for merging and a poor identity rule). The sample is
   small here; a bigger one of small edits would tighten it. **The default is acceptable only because point 3 exists**: every
   such case is shown as *judged on an earlier wording*.
3. **M-2b is the argument for point 3.** One change of meaning in four kept its id. Without "trust never moves silently", a
   quarter of the real meaning changes would have inherited evidence or status.
4. **M-2 cannot be met by asking about every factual block.** Even excluding sweeping edits, 91 per 100 revisions would be
   questions if every factual statement were a claim. Point 1 of H-1 (only explicit claims have identity) is what makes the
   number small; the RFC's own fallback — *ask only when evidence or status is re-confirmed* — is therefore the design, not
   the exception: a reviewer is asked only about a block that carries evidence or status, and the number of such blocks is a
   property of how wikis are used, not something this spike can measure.
5. **Id survival is block-level and brittle across references.** A footnote or reference definition is a block; renumbering
   `[^16]` to `[^15]` is a "change" and, when it moves, a lost id. If claims anchor on prose blocks only, as H-1 says, this
   matters less; it matters for evidence blocks (part 2).

## 5. What the numbers cannot say

- **They estimate; they do not predict.** The histories are *direct* edits by people and bots; inklkunn's changes will be
  *reviewed proposals*, so the mix (and the share of sweeping edits) will differ.
- **Model labellers agreed almost completely** (kappa 0.956). That is partly because 85 % of the sample is trivially
  `same-claim` (renumbering, formatting), and partly because two models read one rule sheet alike. **It does not show that two
  people would agree**; M-3's 15 % line was designed for humans. The meaningful cases are few: 32 changes of meaning in 400,
  16 `cannot-tell`. A human labelling of the 56 small-edit items and of the 48 non-`same-claim` items would be the most
  valuable next step; the owner's spot-check of the Japanese ones is a start.
- **The built-in converter** leaves templates and tables as source; with a faithful converter the blocks (and the
  alignment) would differ, in references especially.
- **The window is the newest 90 revisions** of each article, not a random sample of a history; edit wars appear (1 `contradicts`
  case, a vandalism edit) but are rare in 90 revisions of well-watched articles.
- **Counts for `refines`, `supersedes`, `contradicts` are tiny** (5, 2, 1): nothing about their frequencies should be
  concluded. The six states of H-4 are exercised by the *kinds* of case, not by their number.
- Pairing `replaced` blocks in order inside a run is the tool's own choice (the product has no such pairing); it turns the
  question "did meaning change?" into a question about two blocks that are often unrelated (hence 16 `cannot-tell`).

## 6. M-6: the labelled cases against H-4's fields

H-4: *lifecycle* (proposed, adopted, retired), *community status* (accepted, contested, unresolved), *evidence state*
(supported, mixed, challenged), *scope* (default view, a named view), each with `unknown`.

| Agreed label (count) | Where it lands | A new state? |
|---|---|---|
| `same-claim` (348) | no change of any field; the wording history grows; evidence and status shown *judged on an earlier wording* until confirmed (point 3) | no |
| `refines` (5) | the old wording's lifecycle `retired` with a succession relation to the refined claim; community status carried only by a reviewer's confirmation | no |
| `supersedes` (2) | lifecycle `retired` with succession; the new claim `proposed` | no |
| `contradicts` (1; a vandalism edit: *South America* → *South Korea*) | community status `contested` for as long as both stand; here the next edit reverted it, so `retired` | no |
| `new-claim` (24) | a new claim, lifecycle `proposed`; the old block's claim, if there was one, is untouched (or `retired` if the block is gone) | no |
| `cannot-tell` (16) | **none is a claim.** All 16 are pairs of unrelated or non-statement blocks (a footnote against another, a heading against a paragraph, an infobox row against a comment, one table fragment); the 17th hand case is the `contradicts` above. They point at an alignment limit (§4.5), not a missing state | no |

**M-6 is met: every labelled case lands in H-4's fields with `unknown` as the only extra value.** One observation for part 2:
the case that ends a claim without replacing it (a block deleted) is `retired` with no successor, which H-4's lifecycle
already allows.

## 7. Recommendations (for the architect's D-1 … D-4)

- **D-1, the identity rule.** Keep H-1 as written. Its default passes M-1 overall, and **fails for small edits at 84 %**, which
  is exactly why point 3 (trust never moves silently; *judged on an earlier wording*) and point 6 (the question only to
  reviewers, only for blocks that hold claims) carry the design. Two amendments the data support: (a) the "surviving id" signal
  the service shows should be *an observable fact about the block's text* (as point 5 says), not the merge's alignment alone —
  the one-for-one pairing rule is a merging convenience; (b) a sweeping edit (many blocks at once) is **one confirmation
  step** for the reviewer, never a question per block.
- **D-2, the storage shape.** Nothing in the data argues against H-2 (pages first, claims as files). The alignment lost the id
  on references in the converted pages; anchor claims to prose blocks and keep evidence in the claim's own file, not in the
  page's references.
- **D-3, the change rule.** H-3 stands. A watcher of a claim gets about 3 notifications per 100 revisions (p90 7); the
  worst chain 60 per 100 (a hot block). Notify per accepted change, not per claim, and offer a *wording-only* filter (92 %
  of a surviving id's changes).
- **D-4, the state fields.** H-4 stands; M-6 found no case it cannot place.
- **Before part 2 is frozen:** a human labelling of the small-edit items and the non-`same-claim` items (the cheapest way to
  tell whether 84 % is the real figure for people), and a rerun on a faithful conversion (pandoc, or references stripped).

## 8. Deviations from the handoff

1. **One additive public function in a product crate**: `inklkunn_core::align_blocks(base, other)` exposes the merge's own
   block alignment, because the handoff requires the product's matcher and forbids a copy while the matcher was private. No
   behaviour changed; it has a test. The architect should confirm or rule otherwise.
2. **A length cap in the draw**: blocks over 700 characters (as a labeller reads them) are left out (3,146 of the events). They
   are mostly whole infoboxes, link lists and tables kept as source — not statements a person can label.
3. **The pool was drawn at 1,200 and labelled in order** until 400 items both marked factual were reached (880 labelled), because
   only 47 % of items are factual by both; the unlabelled 320 are in the committed pool and unused.
4. **`replaced` pairing is the tool's**, described in §5; **the packets show a template's source in place of its converter
   snapshot**, so a labeller reads `{{convert|9|cm|in}}` rather than a comment.
5. **Model labellers**, chunked in runs of 160 – 200 items; two models, two families of the same vendor (§2).
6. The subcommand is `claims get`, the handoff says `claims fetch`.

## 9. Reproduce

```
inklkunn-corpus claims plan    --articles articles.txt --window 90 --out revisions.txt
inklkunn-corpus claims get     --revisions revisions.txt --report fetch-report.json
inklkunn-corpus claims convert --revisions revisions.txt --wikicommons <wiki-commons checkout> --report convert-report.json
inklkunn-corpus claims replay  --revisions revisions.txt --out-dir <events> --counts replay-counts.json
inklkunn-corpus claims pool    --events-dir <events> --seed 20261031 --size 1200 --max-chars 700 --out pool.jsonl --packets-dir <packets>
(labelling by the rule sheet; labels-A.jsonl, labels-B.jsonl)
inklkunn-corpus claims measure --pool pool.jsonl --events-dir <events> --labels labels-A.jsonl --labels labels-B.jsonl --sample 400 --report-json measures.json
python3 stratify.py pool.jsonl labels-A.jsonl labels-B.jsonl <events> stratified.json
```

The text itself lives only in `$XDG_CACHE_HOME/inklkunn-corpus/` on the machine that ran the spike.
