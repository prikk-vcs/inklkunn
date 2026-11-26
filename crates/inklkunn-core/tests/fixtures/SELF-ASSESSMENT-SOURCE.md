# Self-assessment schema source

`self-assessment.schema.json` is wiki-commons' `schemas/self-assessment.schema.json`, copied unchanged, so that the
`wiki-commons.yaml` at the repository root is checked against the schema every reader of such a file uses (RFC 007 D-10;
handoff I-T6).

source: https://github.com/kos-commons/wiki-commons
commit: 19f021c68e581b9e6423b3f2b59c99c01e82bf1a (wiki-commons 0.4.0, as `bundle-manifest.schema.json` beside it)
copied: 2026-11-26 (from the checkout under `.git-exclude/projects/wiki-commons`, which has no `.git` of its own)
sha256: 9a01487be3370223a4d4d990fed557c7870dda55034aee8f80ba2b68042efbe9

To update it: copy the file again and record the commit and digest here. The test reads it with the small checker of its
own (`tests/support/json_schema.rs`) that fails on any keyword it does not apply; `anyOf` and the `date` and `uri` formats
were added to it for this schema.
