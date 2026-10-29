# Bundle manifest schema source

`bundle-manifest.schema.json` is wiki-commons' `schemas/bundle-manifest.schema.json`, copied unchanged, so that
every manifest inklkunn writes is checked against the schema every engine reads (R6, handoff 003 corrections).

source: https://github.com/kos-commons/wiki-commons
commit: 19f021c68e581b9e6423b3f2b59c99c01e82bf1a (wiki-commons 0.4.0; the commit the markup corpus was copied from)
copied: 2026-10-30
sha256: 066854daeb21b0dbb7258b7acf72ac6e093c851008f1b15f0ec81fdc64e26950

To update it: copy the file again and record the commit and digest here. The tests read it with a small checker of
their own (`tests/support/json_schema.rs`) that fails on any keyword it does not apply.
