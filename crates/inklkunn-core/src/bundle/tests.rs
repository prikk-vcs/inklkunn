use super::*;
use inklkunn_store::{DisplayName, NewWiki, Slug, WikiId};

fn record_named(name: &str) -> WikiRecord {
    let slug = Slug::new("demo").unwrap_or_else(|e| panic!("{e}"));
    let name = DisplayName::new(name).unwrap_or_else(|e| panic!("{e}"));
    WikiRecord::from_new(WikiId::new(), NewWiki::new(slug, name), 0)
        .unwrap_or_else(|e| panic!("{e}"))
}

fn record() -> WikiRecord {
    record_named("Demo: a wiki")
}

#[test]
fn the_manifest_is_plain_yaml_the_markup_crate_can_read() {
    let m = manifest(&record(), "0.0.0");
    let v = inklkunn_markup::frontmatter::parse_yaml(&m).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        v.get("format").and_then(|f| f.as_str()),
        Some("portable-wiki-bundle")
    );
    assert_eq!(
        v.get("source")
            .and_then(|s| s.get("name"))
            .and_then(|n| n.as_str()),
        Some("Demo: a wiki")
    );
    assert_eq!(
        v.get("fidelity").and_then(|f| f.get("level")),
        Some(&inklkunn_markup::frontmatter::Value::Int(3))
    );
    // Values a person chose are quoted whenever YAML could read them otherwise.
    let mut r = record();
    r.default_license = "LicenseRef-x OR (MIT AND no)".to_owned();
    let m = manifest(&r, "0.0.0");
    let v = inklkunn_markup::frontmatter::parse_yaml(&m).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        v.get("license").and_then(|l| l.as_str()),
        Some("LicenseRef-x OR (MIT AND no)")
    );
    assert_eq!(yaml_quote("yes"), "\"yes\"");
    assert_eq!(yaml_quote("1.0"), "\"1.0\"");
    assert_eq!(yaml_quote("CC-BY-SA-4.0"), "CC-BY-SA-4.0");
}

#[test]
fn the_help_page_parses_scans_and_keeps_a_name_as_text() {
    let page = inklkunn_markup::Page::parse(&help_page(
        &record(),
        "019a0000-0000-7000-8000-000000000001",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(page.metadata.title.as_deref(), Some("Help"));
    assert!(page.scan.links.iter().any(|l| l.target == "Help"));
    assert!(
        page.scan.headings.iter().all(|h| h.level >= 2),
        "body headings begin at level 2 (MKUP-8)"
    );
    // A name with markup characters is text in the page, not structure.
    let tricky = record_named("Links [[Evil]] and #tags *here*");
    let page =
        inklkunn_markup::Page::parse(&help_page(&tricky, "019a0000-0000-7000-8000-000000000002"))
            .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        !page.scan.links.iter().any(|l| l.target == "Evil"),
        "{:?}",
        page.scan.links
    );
    assert!(page.scan.tags.inline.is_empty(), "{:?}", page.scan.tags);
    assert_eq!(page.scan.headings.len(), 5);
}

#[test]
fn the_ignore_file_reserves_the_service_prefix() {
    assert_eq!(prikkignore(), ".inklkunn/\n");
}
