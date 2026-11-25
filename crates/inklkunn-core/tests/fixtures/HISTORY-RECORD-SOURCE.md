# History record schema source

`history-record.schema.json` is wiki-commons' `schemas/history-record.schema.json`, copied unchanged, so that every line of
every `history/<page id>.jsonl` inklkunn writes is checked against the schema every engine reads (RFC 007 D-7; handoff I-T1).

source: https://github.com/kos-commons/wiki-commons
commit: 19f021c68e581b9e6423b3f2b59c99c01e82bf1a (wiki-commons 0.4.0, as `bundle-manifest.schema.json` beside it)
copied: 2026-11-26 (from the checkout under `.git-exclude/projects/wiki-commons`, which has no `.git` of its own; its
`bundle-manifest.schema.json` has the digest `BUNDLE-MANIFEST-SOURCE.md` records for 19f021c)
sha256: 0e6d56c713bc7c96043a235bba74cb1a2bdae6a9abd10065852f760146ca9736

To update it: copy the file again and record the commit and digest here. The tests read it with the small checker of their
own (`tests/support/json_schema.rs`) that fails on any keyword it does not apply.
