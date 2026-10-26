#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn links_in_every_form() {
    let r = scan(
        "Plain [[Target]], labelled [[Target|shown]], heading [[Target#Sec]],\nblock [[T#^b-1]], self [[#Local]], ![[E]] and \\[[Esc]] and `[[Code]]`",
    );
    let raws: Vec<&str> = r.links.iter().map(|l| l.raw.as_str()).collect();
    assert_eq!(
        raws,
        [
            "[[Target]]",
            "[[Target|shown]]",
            "[[Target#Sec]]",
            "[[T#^b-1]]",
            "[[#Local]]",
            "![[E]]"
        ]
    );
    assert_eq!(
        r.links.get(2).and_then(|l| l.fragment_kind),
        Some(FragmentKind::Heading)
    );
    assert_eq!(
        r.links.get(3).and_then(|l| l.fragment_kind),
        Some(FragmentKind::Block)
    );
    assert_eq!(r.links.get(3).map(|l| l.line), Some(2));
    assert!(r.links.get(5).is_some_and(|l| l.embed));
}

#[test]
fn label_first_dialects_flip_the_pipe() {
    let r = scan_with("[[shown|Target#Sec]]", ScanOptions { label_first: true });
    let l = r.links.first().cloned();
    assert_eq!(l.as_ref().map(|l| l.target.as_str()), Some("Target"));
    assert_eq!(l.as_ref().and_then(|l| l.label.as_deref()), Some("shown"));
    assert_eq!(l.and_then(|l| l.fragment), Some("Sec".to_owned()));
}

#[test]
fn attrs_quoted_bare_and_entity_decoded() {
    let a = parse_attrs(
        r#"depth=2 style="compact" src="{{x path=&quot;D&quot;}}" params='{"a":1}' Title here"#,
    );
    assert_eq!(
        a,
        vec![
            ("depth".to_owned(), "2".to_owned()),
            ("style".to_owned(), "compact".to_owned()),
            ("src".to_owned(), "{{x path=\"D\"}}".to_owned()),
            ("params".to_owned(), "{\"a\":1}".to_owned()),
        ]
    );
}

#[test]
fn headings_block_ids_and_tags() {
    let r = scan(
        "# Title\n\nPara ^p1\n\n## Heading ^not-id\n\nTags #one and C#no and #2 and (#paren).\n",
    );
    assert_eq!(r.headings.len(), 2);
    assert_eq!(
        r.headings.get(1).map(|h| h.anchor.as_str()),
        Some("heading-not-id")
    );
    assert_eq!(
        r.block_ids,
        vec![BlockId {
            line: 3,
            id: "p1".into()
        }]
    );
    assert_eq!(r.tags.inline, vec!["one", "paren"]);
}

#[test]
fn frontmatter_errors_are_reported_not_fatal() {
    let r = scan("---\na: &x 1\n---\nBody [[L]]\n");
    assert!(
        r.frontmatter
            .as_ref()
            .and_then(|f| f.get("_error"))
            .is_some()
    );
    assert_eq!(r.links.len(), 1);
    assert_eq!(r.links.first().map(|l| l.line), Some(4));
}
