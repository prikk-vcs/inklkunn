# `inklkunn-corpus claims` — the claim-identity measurement (spike C, RFC 005 part 1)

Measurement only: no product crate depends on it. It tests RFC 005 part 1's hypotheses on real edit histories of public
wiki articles, English and Japanese, through the product's own block functions. Governing documents:
`rfcs/accepted/005-knowledge-layer-part-1-claim-identity.md` and
`rfcs/handoffs/005-knowledge-layer/claim-identity-spike-handoff-v1.md`.

## The pipeline

```text
articles.tsv ──plan──▶ revisions.txt ──get──▶ cache/raw ──convert──▶ cache/converted ──replay──▶ events/*.jsonl + counts.json
                                                                                                  │
                                         pool ◀────────────────────────────────────────────────────┘
                                          │  pool.jsonl (committed)         packets/ (cache)
                                          ▼
                              two labellers ──▶ labels-1.jsonl, labels-2.jsonl ──measure──▶ report.json + a table on stdout
```

| Verb | Reads | Writes |
|---|---|---|
| `plan --articles FILE --window N --out FILE [--cache DIR]` | the article list (`lang<TAB>title<TAB>reason`, `#` comments) and the wiki's API (ids only) | `revisions.txt`: `lang<TAB>title<TAB>revid<TAB>parentid`, oldest first |
| `get --revisions FILE --report FILE [--cache DIR]` | exactly the ids in `revisions.txt` (batches of 50) | `raw/<lang>/<key>/<revid>.txt` in the cache; a report of requested / fetched / deleted upstream per article |
| `convert --revisions FILE --wikicommons DIR --report FILE [--cache DIR]` | `raw/`; runs `python3 DIR/tools/wikicommons.py mediawiki import` per article | `converted/<lang>/<key>/<revid>.md` in the cache; a report with the converter's losses, the revisions it skipped and the checkout's commit |
| `replay --revisions FILE --out-dir DIR --counts FILE [--cache DIR]` | `converted/` | `DIR/<lang>-<key>.jsonl` (a record per revision, an event per edited block: metrics, no text); `counts.json` |
| `pool --events-dir DIR --seed S --size N --out FILE --packets-dir DIR [--packet-size 40] [--shuffle-order yes\|no] [--cache DIR]` | the events and `converted/` | `pool.jsonl` (item, language, article, revision, block numbers, kind); `packet-001.md` … |
| `measure --pool FILE --events-dir DIR --labels FILE --labels FILE [--sample 400] --report-json FILE` | the pool, the events, both label files | the report; a table on standard output |

`--cache` defaults to `$XDG_CACHE_HOME/inklkunn-corpus/claims` (or `~/.cache/inklkunn-corpus/claims`) and is **refused when it
is inside a repository**. All paths are absolute (the tool's house rule).

## What is committed and what lives only in the cache

| Committed (measurements, no text) | Only in the cache (article text, never committed) |
|---|---|
| the article list with its reasons | `raw/` — each revision's wikitext |
| `revisions.txt` (revision ids and parent ids) | `converted/` — Portable Wiki Markdown |
| the `get`, `convert` and `replay` reports and `counts.json` | `convert/` — the converter's input and output |
| `pool.jsonl` (item numbers and block addresses) | the replay's events (`events/`: metrics derived from text, no text) — kept beside the cache; committing them is allowed but unnecessary |
| the two label files (item, factual, label) and the rule sheet | `packets/` — the old and new text of each item, for the labellers |
| the measure report | |

The attribution file (each article's title, URL and licence, CC BY-SA 4.0) is written by hand with the list.

## Handling of the wiki's data

- Only revision ids and text are requested: `rvprop` is exactly `ids` (plan) and `ids|content` (get); no user, user id, comment,
  parsed comment, tag, IP address or timestamp is asked for, so none can be read or kept. A test checks the request builder.
- `curl` is run as a subprocess (`-sS --max-time 60 --fail`) with the user agent
  `inklkunn-corpus/0.1 (spike C, RFC 005; contact: nabbisen@scqr.net)`, at most one request per second.
- A revision the API reports missing or hidden is marked (`raw/…/<revid>.deleted`), counted as `deleted_upstream` and skipped; it is never
  replaced by another.
- The converter is given synthetic timestamps (`2001-01-01T00:00:00Z` plus the revision's position in seconds) and no contributor or comment
  element; the `convert` report says so.

## How the replay works

For each article the converted revisions are walked oldest first. The first revision's blocks (`inklkunn_markup::blocks` of the text
after the frontmatter) get ids `<article index>-<n>`, minted by the tool (the stand-in for CM-5). For each later revision
`inklkunn_core::align_blocks(previous, next)` — the alignment the merge itself makes — says which new block each old block is; ids are
carried through it. One event per edited block:

- `changed`: an aligned pair whose text differs; the id survived.
- `replaced`: between two aligned blocks, a run of unmatched old blocks and a run of unmatched new blocks, paired in order; the id did not survive.
- `added` / `removed`: an unmatched new / old block that has no partner.

Note what the product's alignment does: **one old block and one new block between the same two matches are paired as a changed block, however
different they are** (that is what lets a block be edited at all). So an unrelated one-for-one swap is a `changed` event whose id survived;
M-1 and M-2b measure exactly how often that is wrong.

Wording change (`chars_changed`, `words_changed`): the old lines as `-` and the new lines as `+` in a unified hunk, read and refined by the
product's own `Hunk::parse` and `Hunk::refine`; the characters (not counting whitespace) and words (runs of one script, split at spaces,
punctuation and script changes) in the marked segments of both sides are counted. A pair of differing lines that received no mark is
counted whole and the event says `unrefined: true`.

`is_revert`: the revision's whole body is byte-identical to an earlier revision's of the same article that is not its parent.

## The measures

`measure` takes the first `--sample` pool items that **both** labellers marked factual, and reports how many were skipped. It computes, per
language and overall: M-1, M-2 (with the formula in the report), M-2b (and the check that old and new text differ), M-3 (disagreement, per
label, the confusion matrix, Cohen's kappa, the label pairs split on), M-4, M-5 and M-6 (the agreed labels and the items, by number,
that either labeller called `cannot-tell` or `contradicts`, for placing in H-4's fields by hand). Nothing it prints holds article text.

## Tests

`cargo test -p inklkunn-corpus` — no network (a scripted fetcher replaces `curl`; the rate limiter takes an injectable clock), invented
sentences only. The conversion test runs a small fake `wikicommons.py` and prints `skipping: python3 not on PATH` when there is no Python.
