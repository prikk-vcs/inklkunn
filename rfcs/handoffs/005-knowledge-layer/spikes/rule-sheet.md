# Spike C — the labelling rule sheet (version 1)

Written before any label was made and **not changed while labelling** (a change would make the two label sets
incomparable). It is published with the labels and the report (RFC 005 part 1, "Honesty").

You are shown an **item**: an OLD block of an article and the NEW block that took its place in the next edit
of the same article (English or Japanese). That is all you are shown. You do not know which article, which
edit, who made it, or what any tool thinks. Judge only the two texts, with the rules below. Where a rule does
not settle it, use `cannot-tell`; do not guess to be agreeable.

## Step 1 — is there a factual statement?

A **factual statement** is a sentence, or a clause of one, that asserts something which could be true or false
and could be checked against a source: a date, a number or measurement, a place, an attribution (who did,
built, said or discovered something), a classification, a cause, a sequence of events, a name.

Mark `factual: true` if the OLD or the NEW text contains at least one factual statement. Mark `factual: false`
if neither does: markup only (a heading, a list of links, a table of contents), a navigation or maintenance
note, a quotation of opinion with nothing checkable, a caption with no claim, a citation or reference line,
punctuation or formatting changes only, a block of prose that only restates the title. When in doubt about
whether a statement is factual, it is factual.

If `factual: false`, give no label. Otherwise give exactly one label from step 2.

## Step 2 — what happened to the claim?

Take the **main factual statement** of the OLD block (the first one, if several). Compare it with the NEW
block's statement about the same subject. Choose one label.

| Label | Choose it when | Do not choose it when |
|---|---|---|
| `same-claim` | The NEW text asserts **exactly what the OLD text asserted**; only the form changed: rewording, a different sentence structure, tense or voice, punctuation, a typo fixed, a link or a reference added or removed, spelling, a unit written differently with the same value, a translation of the same assertion. A reader who knew the OLD statement would say "that is the same thing". | The assertion became narrower, wider, more exact, or different in any checkable respect. |
| `refines` | The NEW text asserts **the same thing more precisely, or with its precision changed, without contradicting the old one**: *1950* → *the early 1950s*; *about 300 m* → *298 m*; *a bridge in Scotland* → *a bridge over the Firth of Forth in Scotland*; a qualifier added that narrows it. The OLD statement is still **true if the NEW one is** (or the NEW is a more exact version of it). | A different value that cannot both hold with the old one (that is `supersedes` or `contradicts`). |
| `supersedes` | The NEW text **replaces** the OLD assertion with a different one on the same point, as a **correction or an update** and the OLD no longer stands: *opened in 1951* → *opened in 1950*; *height 8,848 m* → *8,849 m* (a new measurement replacing the old one); *the largest* → *the second largest*. The editor evidently means the old to be gone. | The edit leaves both views visible, or the NEW text says people disagree (that is `contradicts`). |
| `contradicts` | The NEW text asserts something that **cannot hold together with the OLD one**, and the edit looks like **one side of a disagreement** rather than a settled correction: it reverses or negates the old claim, flips an attribution, or sets two stated views against each other (*"some say 1950, others 1951"*). Use it when you would expect the old version to come back. | A plain correction that reads as settled (that is `supersedes`). |
| `new-claim` | The NEW block is **about a different proposition**: a different fact, event, person, place or aspect. It merely sits where the OLD block was; the OLD block's assertion is neither kept, refined, replaced nor contested. | The NEW text is about the same thing as the OLD (use one of the four above). |
| `cannot-tell` | You cannot decide from the two texts: they are cut off, depend on context you were not given, the statement is garbled, or reasonable readers would split. Use it **rarely and honestly**; it is a real answer, not a way to skip. | You are merely unsure between `refines` and `supersedes`: pick the better fit; use `cannot-tell` only when no label fits. |

## Tie-breaks

- If the NEW text keeps the old assertion **and adds an unrelated second fact**, label by the **main
  (first) statement**: it is `same-claim` if that statement is unchanged.
- If a number changes by a **rounding** (*about 300* → *300*), it is `same-claim` when the sentence still says
  "about"/"approximately" and `refines` when precision was added.
- A **unit conversion with the same value** (*1 km* → *1,000 m*) is `same-claim`.
- A **vandalism or obvious nonsense** NEW text that asserts something false is labelled by what it asserts:
  usually `contradicts` or `supersedes`; do not rate truth, rate relation.
- A NEW text that **deletes the statement and writes nothing factual in its place** is `cannot-tell`
  (nothing to compare).
- Do not look anything up. Do not use knowledge of the real world to decide whether the NEW text is *true*;
  decide only how the NEW relates to the OLD.

## Worked examples (two per label, in both languages)

**same-claim**
1. OLD *The bridge opened in 1950.* → NEW *The bridge was opened in 1950.*
2. OLD *橋は1950年に開通した。* → NEW *この橋は1950年に開通しました。*

**refines**
1. OLD *The bridge opened in 1950.* → NEW *The bridge opened in the early 1950s.*
   (the precision changed; the old statement is no longer asserted exactly, and the new one does not contradict it)
2. OLD *全長は約300メートル。* → NEW *全長は298メートル。*

**supersedes**
1. OLD *The bridge opened in 1951.* → NEW *The bridge opened in 1950.*
2. OLD *標高は3,776メートルである。* → NEW *標高は3,775メートルである。*  (a new figure replacing the old one)

**contradicts**
1. OLD *The tower was designed by A.* → NEW *The tower was designed by B, not by A as is sometimes claimed.*
2. OLD *この橋は最長の吊り橋である。* → NEW *この橋は最長の吊り橋ではない。*

**new-claim**
1. OLD *The bridge opened in 1950.* → NEW *The bridge carries two lanes of traffic.*
2. OLD *橋は1950年に開通した。* → NEW *橋の塗装は数年ごとに塗り替えられる。*

**cannot-tell**
1. OLD *It was completed after the war.* → NEW *It was completed in 1950 after the war ended in the city.*  (without knowing which war and which city the statement is about, the narrowing cannot be judged)
2. OLD *（途中で切れた文）* → NEW *（別の途中で切れた文）*  (both cut off)

## The requirements register's bridge example, as a test of the rules

*"The bridge opened in 1950"* → *"The bridge was opened in 1950"*: `same-claim`.
→ *"… in the early 1950s"*: `refines`.
→ *"The bridge began operating in 1950"*: `refines` if "began operating" is read as the opening, `new-claim` if
it is read as a different event (e.g. a ferry service), `cannot-tell` if you cannot say. Choose by what the
two sentences *assert*; do not try to settle what is true.

## What to return

One JSON object per item, one per line, nothing else:

`{"item": 17, "factual": true, "label": "refines"}`
`{"item": 18, "factual": false}`

`label` is one of `same-claim`, `refines`, `supersedes`, `contradicts`, `new-claim`, `cannot-tell`.
