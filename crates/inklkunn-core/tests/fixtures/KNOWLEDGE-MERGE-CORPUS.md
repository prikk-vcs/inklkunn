# The knowledge-file merge corpus (RFC 005 part 2 D-4; handoff K-T3)

`tests/merge_knowledge_corpus.rs` builds its pairs in code; there is no fixture file to regenerate. This note says how
they are made, so that the counts it holds can be read and, if the merge ever has to change them, changed on purpose.

**Two sides, one baseline.** Each pair is a baseline claim file and two edited copies of it: the tip's ("theirs", an
edit that landed) and the proposal's ("ours"). The merge is `inklkunn_core::merge`, the function an acceptance calls
when a proposal's base has moved; the verdict is *clean* (one merged file) or *overlap* (every place two sides wrote
differently, with both texts).

**The oracle.** An edit is a presence edit (add or remove an expression or a relation: an entry with no value of its own,
which can never be written twice differently) or it writes a value-bearing place (a key of a source link, a link added
or removed, the kind, the standing). Two edits conflict when they write one place with different values, or when one
removes a source link and the other writes a key of it. The pair overlaps exactly when some edit of one side conflicts
with some edit of the other; otherwise the merged claim is the baseline with every edit applied, and nothing a side
added is lost unless the other side removed it. The test asserts the verdict, the merged claim, the symmetry of the sides
and the lost-nothing rule on **every** pair.

**Hand cases (324 pairs, 306 clean, 18 overlaps).** Every ordered pair of 18 edits to one claim file.

**Pairs derived from spike C's replay facts (216 pairs, 209 clean, 7 overlaps).** From
`rfcs/handoffs/005-knowledge-layer/spikes/replay-counts.json`, which holds counts only: for each of the 24 articles
(12 English, 12 Japanese), `edits / 9` pairs. An edit is drawn by a seeded generator (splitmix64, seed from the
article's number and the pair's) with the weights the article's counts give: `changed` events (a block kept its id and
changed its words) re-judge one source link and, half the time, set its note; `replaced` events (the id did not survive)
move an expression; `added` and `removed` events add and remove one; `reverts` leave the file as it was. The number of
expressions the baseline has is the article's surviving block ids / 20 (between 2 and 12), and the number of links its
minted ids / 400 (between 1 and 4), so a busier article collides with itself more often.

**No text.** The sentences in the files are invented in the test (one per edit, in English for the English articles
and Japanese for the others) and name an article by its number. No article text, title, editor, address or comment is
read from the replay or kept here.
