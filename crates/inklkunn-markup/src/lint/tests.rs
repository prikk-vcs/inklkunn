#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use crate::resolve::PageEntry;

fn codes(src: &str, ctx: Context<'_>) -> Vec<&'static str> {
    let page = Page::parse(src).unwrap_or_else(|e| panic!("{e}"));
    lint(&page, src, ctx).into_iter().map(|f| f.code).collect()
}

#[test]
fn text_and_metadata_rules() {
    let c = codes(
        "---\ntitle: T\ntags: a, b\ncreated: 2026-06-01\nstatus: odd\n---\n\n# Other\n\n\tTabbed",
        Context::default(),
    );
    for expected in [
        "MKUP-19-newline",
        "MKUP-19-tabs",
        "META-3-coerced",
        "META-5-list",
        "META-6-status",
        "MKUP-8-h1",
    ] {
        assert!(c.contains(&expected), "{expected} missing in {c:?}");
    }
    assert!(codes("No frontmatter\n", Context::default()).contains(&"META-1-missing"));
}

#[test]
fn body_rules() {
    let c = codes(
        "---\ntitle: T\n---\n\nA ^x\n\nB ^x\n\n> [!CUSTOM]\n> hi\n\n## One\n\n#### Four\n\n#tag here [[Target|]] [[#^nope]]\n\n<!-- wiki:snapshot kind=\"macro\" name=\"open\" -->\n",
        Context::default(),
    );
    for expected in [
        "MKUP-9-duplicate",
        "MKUP-15-callout",
        "A11Y-3-heading-skip",
        "MKUP-14-tags",
        "MKUP-4-empty-label",
        "MKUP-9-missing-block",
        "EXT-4-unclosed",
    ] {
        assert!(c.contains(&expected), "{expected} missing in {c:?}");
    }
}

#[test]
fn link_rules_need_an_index() {
    let mut ix = Index::new("");
    ix.add(PageEntry::new("A/X.md", "X"));
    ix.add(PageEntry::new("B/X.md", "X"));
    ix.add(PageEntry::new("Real.md", "Real"));
    let ctx = Context {
        index: Some(&ix),
        from_path: Some("Home.md"),
    };
    let c = codes(
        "---\ntitle: T\n---\n\n[[X]] [[Nope]] [[wp:Thing]] [[Nope|Real]]\n",
        ctx,
    );
    assert!(c.contains(&"MKUP-5-ambiguous"), "{c:?}");
    assert!(c.contains(&"NAV-3-dangling"), "{c:?}");
    assert!(c.contains(&"MKUP-13-prefix"), "{c:?}");
    assert!(c.contains(&"MKUP-6-order"), "{c:?}");
}
