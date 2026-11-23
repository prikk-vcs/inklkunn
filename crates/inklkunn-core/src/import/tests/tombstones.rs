//! A bundle that carries tombstones (RFC 006 D-13) is refused by name: the import writes the tip's files into a wiki that
//! has no record of what was hidden, so it would look whole. No test needs prikk.

use super::*;
use crate::bundle::{ExportFacts, ManifestFacts, manifest_with};

/// A manifest the export writes, for a wiki with `hidden` entries that had a part hidden.
fn manifest(hidden: usize) -> String {
    let record = inklkunn_store::WikiRecord::from_new(
        inklkunn_store::WikiId::new(),
        inklkunn_store::NewWiki::new(
            inklkunn_store::Slug::new("exp").unwrap(),
            inklkunn_store::DisplayName::new("Exp").unwrap(),
        ),
        1,
    )
    .unwrap();
    manifest_with(
        &record,
        "0.0.0",
        &ManifestFacts {
            exported: Some(ExportFacts {
                pages: 1,
                tip: "ab".repeat(32),
                records_read: true,
                suppressed_entries: hidden,
                ..ExportFacts::default()
            }),
        },
    )
}

fn folder(tag: &str, manifest: &str) -> Dir {
    let d = dir(tag);
    put(&d.0, "wiki-bundle.yaml", manifest.as_bytes());
    put(&d.0, "pages/Good.md", GOOD.as_bytes());
    put(&d.0, "history/g-1.jsonl", b"{}\n");
    d
}

#[test]
fn a_bundle_that_declares_tombstones_is_refused_whole_and_says_why() {
    let d = folder("tombstones", &manifest(2));
    let plan = plan_folder(&d.0, &nothing(), false, &ReadConfig::default()).unwrap();
    assert!(!plan.is_importable(), "{}", plan.report());
    let why = plan
        .refused
        .iter()
        .find(|r| r.name == "wiki-bundle.yaml")
        .map(|r| r.reason.as_str())
        .unwrap_or_default();
    assert!(
        why.contains("tombstones are not imported yet")
            && why.contains("nothing of this bundle is imported"),
        "{why}"
    );
    assert!(plan.report().contains("tombstones are not imported yet"));
}

#[test]
fn the_same_bundle_with_nothing_hidden_is_imported_as_before() {
    let d = folder("nothing-hidden", &manifest(0));
    let plan = plan_folder(&d.0, &nothing(), false, &ReadConfig::default()).unwrap();
    assert!(plan.is_importable(), "{}", plan.report());
    assert_eq!(plan.counts(), (1, 0));
}

#[test]
fn a_hand_written_declaration_of_any_shape_is_a_declaration() {
    for own in ["tombstones: {}", "tombstones: 0", "tombstones: nothing"] {
        let d = folder(
            "any",
            &format!("format: portable-wiki-bundle\nversion: '0.4'\ninklkunn:\n  {own}\n"),
        );
        let plan = plan_folder(&d.0, &nothing(), false, &ReadConfig::default()).unwrap();
        assert!(!plan.is_importable(), "{own}: {}", plan.report());
    }
}

#[test]
fn a_member_of_that_name_anywhere_else_is_not_a_declaration() {
    for text in [
        "format: portable-wiki-bundle\nversion: '0.4'\ntombstones: {}\n",
        "format: portable-wiki-bundle\nversion: '0.4'\nsource:\n  tombstones: {}\n",
        "format: portable-wiki-bundle\nversion: '0.4'\ninklkunn:\n  default_view: main\n",
    ] {
        let d = folder("elsewhere", text);
        let plan = plan_folder(&d.0, &nothing(), false, &ReadConfig::default()).unwrap();
        assert!(plan.is_importable(), "{text}: {}", plan.report());
    }
}
