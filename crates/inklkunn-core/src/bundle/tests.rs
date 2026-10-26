use super::*;
use inklkunn_store::{NewWiki, Slug, WikiId};

fn record() -> WikiRecord {
    let slug = Slug::new("demo").unwrap_or_else(|e| panic!("{e}"));
    WikiRecord::from_new(WikiId::new(), NewWiki::new(slug, "Demo: a wiki"), 0)
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
}

#[test]
fn the_help_page_parses_and_scans() {
    let page =
        inklkunn_markup::Page::parse(&help_page(&record())).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(page.metadata.title.as_deref(), Some("Help"));
    assert!(page.scan.links.iter().any(|l| l.target == "Help"));
    assert!(
        page.scan.headings.iter().all(|h| h.level >= 2),
        "body headings begin at level 2 (MKUP-8)"
    );
}
