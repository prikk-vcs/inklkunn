# Claim identity — Spike C Handoff v1 (RFC 005 part 1)

| | |
|---|---|
| Document | The prototype that tests RFC 005 part 1's hypotheses H-1 … H-4 on real edit histories, and the report that the architect turns into part 1's decisions D-1 … D-4. Measurement only: **no product crate changes**. |
| Status | RFC 005 part 1 accepted by the owner 2026-10-31. **Assignable now**, beside handoff 004's corrections and the sign-in handoff (it touches neither). |
| Basis | `rfcs/accepted/005-knowledge-layer-part-1-claim-identity.md` (H-1 points 1 – 7, H-2 … H-4, the measures M-1 … M-6 and M-2b, the corpus and its handling); requirements `KN-1 … KN-9`, `CM-5`, `NG-3`; RFC 004's block segmentation (`inklkunn_markup::blocks`) and block matching in the merge (`inklkunn_core::merge`: `^id`, then unique content, then a bounded LCS); M0's spikes as the precedent (`rfcs/handoffs/001-foundations/`). |
| Audience | The dev team. Work in the open: the rule sheet, both label sets and every number are published with the report. |
| Handback | One review request, `005-claim-identity-spike-review-request.md`, with the report, the gates, both test runs and the tool's own tests. |

## T1 — The corpus

- **Articles.** 20 – 30 encyclopedic articles, about half English and half Japanese, each with a long history (several
  hundred revisions) and many factual statements that people corrected: places, structures, historical events, natural
  science. **No biography of a living person** (privacy, and disputes the spike does not need). The list, with a
  one-line reason per article, is committed.
- **Fetching** (`inklkunn-corpus claims fetch`): the wiki's public API (`action=query&prop=revisions`, `rvprop=ids|content`,
  `rvslots=main`), through `curl` run as a subprocess (no HTTP or TLS crate enters the lock; the house rule), with a stated
  user agent naming the project and a contact, at most one request per second. **No user, comment or IP field is
  requested or kept.** Text goes to a cache **outside the repository** (`$XDG_CACHE_HOME/inklkunn-corpus/`); the
  repository keeps `revisions.txt` (article, revision id, parent id), `ATTRIBUTION.md` (each article's title, URL and
  licence, CC BY-SA 4.0, with "revision history of" wording), and nothing of the text.
- **Pinned.** A run reads exactly the committed revision ids; a revision deleted upstream is skipped and counted in the
  report, never replaced.
- **Converted** to Portable Wiki Markdown with **wiki-commons' MediaWiki converter** (`tools/wikicommons.py`, at the
  commit the project pins, run as a Python subprocess; it is a measurement aid, not a dependency of any crate). Record
  what the converter dropped or degraded per revision (its import report); a revision it cannot convert is skipped and
  counted. Converter defects found on the way are written up for wiki-commons, not worked around silently.

## T2 — Replaying a history through the product's functions

- For each article, walk the revisions in order. At the first revision every top-level block is given an id, as the
  service would for a block a claim attaches to (CM-5). For each later revision, carry ids forward with **the product's
  own** block segmentation and matching (call `inklkunn_markup::blocks` and the merge's block matcher; never a copy),
  and record per edited block: the id survived or not; the wording change (characters and words changed, as the diff's
  word marks count them); which other blocks changed in the same revision; whether the revision was a revert of an
  earlier one (identical text to an earlier revision).
- The seeded generator of M0 adds volume for M-4 and M-5 only, never for M-1, M-2, M-2b, M-3 or M-6 (it would measure
  itself).
- Output: one JSON Lines file per article in the cache, and the counts committed.

## T3 — The rule sheet and the labels

- **The rule sheet** (`rfcs/handoffs/005-knowledge-layer/spikes/rule-sheet.md`, written first and not changed during
  labelling): what a *factual statement* is for this purpose; the six labels with two worked examples each in both
  languages — *same claim* (rewording, punctuation, a typo, a link added), *refines* (narrower or more exact: *1950* →
  *the early 1950s* counts as refining the precision), *supersedes* (a correction that replaces the old content),
  *contradicts* (both cannot hold, and both stay, as an edit war shows), *new claim* (a different proposition), *cannot
  tell*. The bridge example of the requirements register is one of them.
- **The sample:** 400 edited blocks that contain a factual statement (a labeller first marks *factual* or *not*; the
  skipped are counted), drawn uniformly across articles and the two languages, by a seeded draw recorded in the report.
- **Two labellers, apart.** Each labels the whole sample with the rule sheet only, without seeing the other's labels or
  the tool's facts beyond the two texts. Labellers that are models are said to be models in the report. The owner reads
  Japanese and *may* spot-check a handful of the Japanese labels; nothing waits on it.
- Both label sets are committed by revision id and block number (no text).

## T4 — The measures and the report

Compute, per language and overall, exactly the measures RFC 005 part 1 names, from the labels and T2's facts:

- **M-1** — of the blocks whose id survived with a wording change, the share both labellers marked *same claim*.
- **M-2** — re-confirmations that H-1 point 3 would ask of reviewers per 100 edits, counting only blocks that would hold
  a claim (a labelled factual statement) and any wording change there.
- **M-2b** — of the blocks labelled a change of meaning (*refines*, *supersedes*, *contradicts*, *new claim*), the share
  whose id survived (so evidence or status would have stayed attached without point 3); and the confirmation that point
  3 marks each of them *judged on an earlier wording*.
- **M-3** — the labellers' disagreement, overall and per label, with Cohen's kappa; and the cases they split on, by kind.
- **M-4** — changes per block over a history, and the share that touch only wording.
- **M-5** — notifications a watcher of one claim would receive per 100 edits under H-3.
- **M-6** — every labelled case placed in H-4's fields, or the missing state named.

The report (`rfcs/handoffs/005-knowledge-layer/spikes/claim-identity-report.md`) states each measure against the RFC's
acceptance line, the corpus actually used (fetched, skipped, deleted upstream, unconvertible), the converter's losses,
the generator's share, and **what the numbers cannot say** (direct edits, not reviewed proposals). It recommends; the
architect decides D-1 … D-4.

## Non-change scope

No change to any product crate, schema, page or API; no new crate in the lock (`curl` and the wiki-commons converter are
subprocesses of the measurement tool); no article text, no editor identity, in the repository.

## Acceptance criteria

The tool's own tests (the fetcher's refusal to request user fields, the pinning, the cache outside the repository, the
replay on a small hand-made history, each measure on a hand-labelled fixture); every gate green with prikk on `PATH` and
shimmed off; the committed files contain no text of an article (a test greps the committed spike folder for any line of
the cached text) and no user name or address.
