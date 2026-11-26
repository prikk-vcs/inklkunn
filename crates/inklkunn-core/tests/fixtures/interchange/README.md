# Interchange fixtures (RFC 007 P-2)

Three small bundles that the bundle import's plan reads (`import/bundle/tests.rs`, `tests/import_bundle.rs`). **Python is never
needed to run the tests**: the bundles are committed, and the commands below are for whoever regenerates them.

**Licence of the fixtures: CC0 1.0** (public domain dedication). Every word of the two sources was written for these fixtures; no
third-party content is in them.

| Directory | What it is | How it was made |
|---|---|---|
| `mediawiki/dump.xml` | a MediaWiki XML dump of four pages (one with four revisions: an anonymous edit, a revision whose text and contributor were deleted at the source, a redirect, a talk page, links with a `wikipedia:` prefix) | written by hand |
| `mediawiki/bundle/` | its conversion: a level 3 bundle with history | wiki-commons' converter, below |
| `bookstack/input/` | a BookStack export, extracted (`data.json` and `files/`): one book, one chapter, three pages (Markdown and HTML), an image and an attachment | written by hand (the image is a one-pixel PNG made for it) |
| `bookstack/bundle/` | its conversion: a level 2 bundle with no history | wiki-commons' converter, below |
| `inklkunn/input-1..3/` | three folders of pages and one attachment | written by hand |
| `inklkunn/bundle/` | **inklkunn's own export** of a wiki made from those folders (the shape our own export has) | the commands below |

## The converters

wiki-commons **at commit `19f021c`** (`19f021c68e581b9e6423b3f2b59c99c01e82bf1a`), its reference tooling, with PyYAML (the converters
need it; make a throwaway virtual environment, never commit it). From the repository root, with `$WC` the checkout at that commit:

```sh
python3 -m venv /tmp/wc-venv && /tmp/wc-venv/bin/pip install PyYAML

/tmp/wc-venv/bin/python3 -I -B $WC/tools/wikicommons.py mediawiki import \
    crates/inklkunn-core/tests/fixtures/interchange/mediawiki/dump.xml \
    crates/inklkunn-core/tests/fixtures/interchange/mediawiki/bundle \
    --name "Garden Notes" --no-report

/tmp/wc-venv/bin/python3 -I -B $WC/tools/wikicommons.py bookstack import \
    crates/inklkunn-core/tests/fixtures/interchange/bookstack/input \
    crates/inklkunn-core/tests/fixtures/interchange/bookstack/bundle \
    --name "Allotment Handbook" --no-report
```

**`--no-report` matters.** By default the converter writes `import-report.md` into the bundle *after* it wrote the checksums, so
the checksums file does not name it, and the import refuses the bundle by name (a file in the bundle that the checksums do not
name). The refusal says so, and says how to fix it. (The converters' output is otherwise unchanged by the flag.)

The converters stamp `exported_at` with the day they ran, so a regeneration gives other bytes in the manifest and nowhere else:
the page, history and `users.yaml` files, and so the checksums, are the same bytes. The tests read counts and rules, never that
date.
Pandoc was **not** installed when these were made, so the converter's built-in wikitext reader and its "HTML page kept as HTML"
rule are in the output (`fidelity.degraded` says so).

Findings about the converters' output that the import's plan depends on, or that a reader of the fixtures may trip on:

- the MediaWiki converter writes **no `license`** in the manifest (RFC 007 D-4's case for `--bundle-license`);
- the BookStack converter writes **no `license` in an attachment's sidecar**, and none in the manifest;
- neither converter's final page file equals its last history record's `content` (the file carries `id`, `source`, `ext` and the
  like; the record only the title, tags and kind), so every page of a converted bundle ends with the change *final state of the
  bundle* (D-2);
- a history file is named by the page's `id` (`mw:101`), colon included. **`cargo package` refuses a file name with a colon**, so
  `inklkunn-core`'s `Cargo.toml` excludes `mediawiki/bundle/history/` from the published crate (the tests that read it run in the
  repository, never from the package).

## inklkunn's own export

Made with the binary built from this repository (`cargo build -p inklkunn`), prikk 0.55.0 on `PATH`, in a scratch data directory:

```sh
export INKLKUNN_DATA_DIR=$SCRATCH/data INKLKUNN_SECRET_FILE=$SCRATCH/secret INKLKUNN_SEED_DIR=/dev/shm/inklkunn-seed
printf 'a secret of at least sixteen bytes' > $SCRATCH/secret && chmod 600 $SCRATCH/secret
B=target/debug/inklkunn
I=crates/inklkunn-core/tests/fixtures/interchange/inklkunn
$B --dev-unconfined wiki create orchard --name "Orchard Notes" --license CC0-1.0
$B --dev-unconfined wiki import-folder orchard $I/input-1
$B --dev-unconfined wiki import-folder orchard $I/input-2 --replace-existing
$B --dev-unconfined wiki import-folder orchard $I/input-3 --replace-existing
$B --dev-unconfined wiki export orchard --bundle $I/bundle   # the destination must not exist
```

It was exported by the tree after the corrections X-1 – X-9 to the export (2026-11-26: `sha256sums.txt` as the checksums file's name, `at` from the sealing time, `inklkunn.withheld` and `after_untraced` where they apply), and the tests that read it (counts, rules, and what the manifest names) pass on it.
