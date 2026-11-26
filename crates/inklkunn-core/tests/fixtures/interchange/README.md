# Interchange fixtures (RFC 007 P-2)

Four small bundles that the bundle import's plan and replay read (`import/bundle/tests.rs`, `tests/import_bundle.rs`). **Python is never
needed to run the tests**: the bundles are committed, and the commands below are for whoever regenerates them.

**Licence of the fixtures: CC0 1.0** (public domain dedication). Every word of the two sources was written for these fixtures; no
third-party content is in them.

| Directory | What it is | How it was made |
|---|---|---|
| `mediawiki/dump.xml` | a MediaWiki XML dump of four pages (one with four revisions: an anonymous edit, a revision whose text and contributor were deleted at the source, a redirect, a talk page, links with a `wikipedia:` prefix) | written by hand |
| `mediawiki/bundle/` | its conversion: a level 3 bundle with history | wiki-commons' converter, below |
| `bookstack/input/` | a BookStack export, extracted (`data.json` and `files/`): one book, one chapter, three pages (Markdown and HTML), an image and an attachment | written by hand (the image is a one-pixel PNG made for it) |
| `bookstack/bundle/` | its conversion: a level 2 bundle with no history | wiki-commons' converter, below |
| `sidecars/bundle/` | a level 2 bundle of one page and two attachments, each with a sidecar that declares a licence (CC0-1.0 and CC-BY-4.0) in a wiki of CC-BY-SA-4.0 | **written by hand** (see below) |
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

**`--no-report` no longer matters to the import.** By default the converter writes `import-report.md` into the bundle *after* it
wrote the checksums, so the checksums file does not name it. The import's rule is about what it **reads**: every file it reads (a
page, an attachment and its sidecar, a history file, `users.yaml`, the file of checksums the manifest names) must be named by the
checksums, and a file it does not read, such as that report, is listed as *not imported* and refuses nothing (the I-T1 review's
ruling on I-T2). So the converters' default output imports. The fixtures are made with the flag anyway, so that what is committed is
what the checksums cover. A test puts a report beside each converted bundle and shows both importable.

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

## The bundle written by hand (`sidecars/bundle/`)

**Neither converter writes a licence in an attachment's sidecar** (the BookStack converter writes none, and MediaWiki has no
sidecars), so acceptance criterion 2's *sidecars' licences* cannot be shown by a converted bundle. This one is written by hand, for
that and for nothing else: a manifest (`license: CC-BY-SA-4.0`, `checksums: sha256sums.txt`, fidelity level 2), a page, two
attachments (`plan.svg`, `notes.txt`) and their sidecars (`license: CC0-1.0` and `license: CC-BY-4.0`, which CC-BY-SA takes). The
checksums are made with `sha256sum` over every file but the manifest:

```sh
cd crates/inklkunn-core/tests/fixtures/interchange/sidecars/bundle
sha256sum attachments/notes.txt attachments/notes.txt.meta.yaml attachments/plan.svg attachments/plan.svg.meta.yaml "pages/Orchard Plan.md" > sha256sums.txt
```

The tests that read it make their own changed copies (a sidecar whose licence the wiki cannot take) and re-seal them.

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
