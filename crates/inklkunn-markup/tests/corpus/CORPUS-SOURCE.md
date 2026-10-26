# Corpus source

The markup and resolution cases under this directory are copied from wiki-commons' conformance corpus
(`corpus/markup`, `corpus/resolution`), unchanged, so that `inklkunn-markup` is tested against the same
expectations every engine reads (RFC 001 D-7). Bundle cases are not copied: the bundle validator is
RFC 007's.

source: https://github.com/kos-commons/wiki-commons
commit: 19f021c68e581b9e6423b3f2b59c99c01e82bf1a
copied: 2026-10-27
digest: 15eddc18b8dc38fc

`tools/floor-check` recomputes the digest over this tree (every file except this one) and fails when the copy
drifts from what is recorded here. To update the corpus: copy again, record the new commit, re-run the
floor check and paste the digest it prints.

Scope: every `expected.scan.json` and every resolution `expected.json` is an acceptance test here (the
scanner is dialect-agnostic once a case's `label_order` is honoured, so the gollum, logseq, obsidian and
dendron source cases pass as written). The `expected.<dialect>.md` conversion outputs are outside inklkunn's
scope: inklkunn reads and writes the portable profile only; converting between dialects is wiki-commons'
`pwb` tool's job.
