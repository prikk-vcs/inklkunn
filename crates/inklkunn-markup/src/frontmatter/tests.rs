#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn splits_frontmatter_and_body() {
    let (y, b, l) = split("---\ntitle: A\n---\n\nBody\n");
    assert_eq!(y, Some("title: A\n"));
    assert_eq!(b, "\nBody\n");
    assert_eq!(l, 4);
    assert_eq!(split("No frontmatter\n"), (None, "No frontmatter\n", 1));
    assert_eq!(split("---\nunclosed\n").0, None);
}

#[test]
fn coerces_like_yaml_so_the_pitfalls_are_visible() {
    let v = parse_yaml("a: 2026-06-01\nb: no\nc: \"2026-06-01\"\nd: 0123\ne: 42\nf: 1.5\ng: ~\n")
        .unwrap();
    assert_eq!(v.get("a"), Some(&Value::Date("2026-06-01".into())));
    assert_eq!(v.get("b"), Some(&Value::Bool(false)));
    assert_eq!(v.get("c"), Some(&Value::Str("2026-06-01".into())));
    assert_eq!(v.get("d"), Some(&Value::Str("0123".into())));
    assert_eq!(v.get("e"), Some(&Value::Int(42)));
    assert_eq!(v.get("f"), Some(&Value::Float(1.5)));
    assert_eq!(v.get("g"), Some(&Value::Null));
}

#[test]
fn nested_maps_sequences_and_flow_collections() {
    let v = parse_yaml(
        "tags: [\"c#\", release]\naliases:\n  - One\n  - \"Two: x\"\next:\n  obsidian:\n    cssclasses: [wide]\nlist:\n- a: 1\n  b: 2\n- c\n",
    )
    .unwrap();
    assert_eq!(
        v.get("tags"),
        Some(&Value::Seq(vec![
            Value::Str("c#".into()),
            Value::Str("release".into())
        ]))
    );
    assert_eq!(
        v.get("aliases"),
        Some(&Value::Seq(vec![
            Value::Str("One".into()),
            Value::Str("Two: x".into())
        ]))
    );
    assert_eq!(
        v.get("ext")
            .and_then(|e| e.get("obsidian"))
            .and_then(|o| o.get("cssclasses")),
        Some(&Value::Seq(vec![Value::Str("wide".into())]))
    );
    assert_eq!(
        v.get("list"),
        Some(&Value::Seq(vec![
            Value::Map(vec![
                ("a".into(), Value::Int(1)),
                ("b".into(), Value::Int(2))
            ]),
            Value::Str("c".into())
        ]))
    );
}

#[test]
fn anchors_tags_and_duplicates_are_refused() {
    assert!(matches!(
        parse_yaml("a: &x 1\n"),
        Err(YamlError::OutsideSubset { .. })
    ));
    assert!(matches!(
        parse_yaml("a: !!str 1\n"),
        Err(YamlError::OutsideSubset { .. })
    ));
    assert!(matches!(
        parse_yaml("a: 1\na: 2\n"),
        Err(YamlError::DuplicateKey { .. })
    ));
    assert!(matches!(
        parse_yaml("a: \"open\n"),
        Err(YamlError::UnterminatedQuote { .. })
    ));
}

#[test]
fn comments_quotes_and_block_scalars() {
    let v = parse_yaml(
        "a: x # comment\nb: \"has # inside\"\nc: 'it''s'\nd: |\n  line one\n  line two\n",
    )
    .unwrap();
    assert_eq!(v.get("a"), Some(&Value::Str("x".into())));
    assert_eq!(v.get("b"), Some(&Value::Str("has # inside".into())));
    assert_eq!(v.get("c"), Some(&Value::Str("it's".into())));
    assert_eq!(v.get("d"), Some(&Value::Str("line one\nline two\n".into())));
}

#[test]
fn metadata_lifts_the_vocabulary_and_keeps_the_rest() {
    let v = parse_yaml("title: T\ntags: a, b\nkind: template\ncustom: 1\n").unwrap();
    let m = Metadata::from_value(Some(&v));
    assert_eq!(m.title.as_deref(), Some("T"));
    assert_eq!(m.tags, vec!["a", "b"]);
    assert_eq!(m.kind, "template");
    assert_eq!(m.status, "stable");
    assert_eq!(m.extra, vec![("custom".to_owned(), Value::Int(1))]);
}
