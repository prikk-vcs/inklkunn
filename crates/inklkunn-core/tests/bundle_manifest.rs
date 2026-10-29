//! The bundle manifest follows wiki-commons (R6, handoff 003 corrections): every manifest the code writes is
//! valid against `schemas/bundle-manifest.schema.json` (the copy under `tests/fixtures/`), `checksums` is the
//! name of a `SHA256SUMS`-style file, and that file's lines are `"<sha256>  <path>"`. The export of a real wiki
//! is checked the same way in `export_roundtrip.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/bundle_manifest.rs"]
mod bundle_manifest;
#[path = "support/json_schema.rs"]
mod json_schema;

use inklkunn_core::bundle::{
    CHECKSUMS, ExportFacts, ManifestFacts, manifest, manifest_with, sha256sums,
};
use inklkunn_store::{DisplayName, NewWiki, Slug, WikiId, WikiRecord};

fn record() -> WikiRecord {
    WikiRecord::from_new(
        WikiId::new(),
        NewWiki::new(
            Slug::new("manifest").unwrap(),
            DisplayName::new("A \"quoted\" wiki: yes").unwrap(),
        ),
        1,
    )
    .unwrap()
}

fn facts() -> ManifestFacts {
    ManifestFacts {
        exported: Some(ExportFacts {
            pages: 3,
            attachments: 2,
            exported_at: 1_700_000_000,
            tip: "ab".repeat(32),
            interwiki: vec![("wp".to_owned(), "https://en.wikipedia.org/wiki/".to_owned())],
            unattributed_changes: 4,
            histories_not_written: 1,
        }),
    }
}

#[test]
fn the_schema_copy_is_the_one_the_checker_understands() {
    // A checker that skipped a keyword would pass anything; it names every keyword it does not apply.
    let schema: serde_json::Value = serde_json::from_str(bundle_manifest::SCHEMA).unwrap();
    let empty = json_schema::violations(&schema, &serde_json::json!({}));
    assert_eq!(
        empty,
        vec![
            ": `format` is required".to_owned(),
            ": `version` is required".to_owned()
        ],
        "an empty object fails exactly on the schema's two required keys"
    );
}

#[test]
fn the_manifest_a_new_wiki_gets_is_valid_against_wiki_commons_schema() {
    let text = manifest(&record(), "1.2.3");
    let problems = bundle_manifest::violations(&text);
    assert!(problems.is_empty(), "{problems:#?}\n{text}");
}

#[test]
fn the_manifest_an_export_writes_is_valid_and_names_the_checksums_file() {
    let text = manifest_with(&record(), "1.2.3", &facts());
    let problems = bundle_manifest::violations(&text);
    assert!(problems.is_empty(), "{problems:#?}\n{text}");
    assert!(
        text.lines().any(|l| l == format!("checksums: {CHECKSUMS}")),
        "{text}"
    );
    assert_eq!(CHECKSUMS, "SHA256SUMS");
    assert!(
        text.contains("  histories_not_written: 1"),
        "a history that was not written is counted in the manifest\n{text}"
    );
    assert!(text.contains("  history_changes_without_a_page: 4"));
}

#[test]
fn the_old_shape_of_checksums_is_what_the_schema_check_catches() {
    // The manifest before R6 wrote `checksums` as an object; the schema says it is a path.
    let good = manifest_with(&record(), "1.2.3", &facts());
    let old = good.replace(
        "checksums: SHA256SUMS",
        "checksums:\n  algorithm: sha256\n  files:\n    \"pages/A.md\": \"00ff\"",
    );
    let problems = bundle_manifest::violations(&old);
    assert!(
        problems.iter().any(|p| p.starts_with("/checksums:")),
        "{problems:#?}"
    );
}

#[test]
fn the_checksums_file_is_sha256_two_spaces_and_a_path_in_path_order() {
    let entries = vec![
        ("pages/B.md".to_owned(), "bb".repeat(32)),
        ("attachments/a b.png".to_owned(), "aa".repeat(32)),
        ("users.yaml".to_owned(), "cc".repeat(32)),
    ];
    let text = sha256sums(&entries).unwrap();
    assert_eq!(
        text,
        format!(
            "{a}  attachments/a b.png\n{b}  pages/B.md\n{c}  users.yaml\n",
            a = "aa".repeat(32),
            b = "bb".repeat(32),
            c = "cc".repeat(32)
        )
    );
    for line in text.lines() {
        let (sum, path) = line.split_once("  ").unwrap();
        assert_eq!(sum.len(), 64);
        assert!(sum.bytes().all(|b| b.is_ascii_hexdigit()));
        assert!(!path.is_empty());
    }
    assert_eq!(sha256sums(&[]).unwrap(), "");
    // A path with a line break cannot be a line of the file.
    for bad in ["pages/a\nb.md", "pages/a\rb.md"] {
        assert!(sha256sums(&[(bad.to_owned(), "00".repeat(32))]).is_none());
    }
}
