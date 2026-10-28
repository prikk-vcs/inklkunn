use super::*;

#[test]
fn lists_are_sorted_and_free_of_duplicates() {
    for (name, list) in [
        ("ELEMENTS", ELEMENTS),
        ("CLASSES", CLASSES),
        ("GLOBAL_ATTRIBUTES", GLOBAL_ATTRIBUTES),
    ] {
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, list, "{name} is sorted without duplicates");
    }
    for (element, attrs) in ELEMENT_ATTRIBUTES {
        assert!(ELEMENTS.contains(element), "{element} is an element");
        let mut sorted = attrs.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(&sorted, attrs, "{element}'s attributes are sorted");
    }
    for (element, _) in UNALLOWED_ATTRIBUTES {
        assert!(ELEMENTS.contains(element), "{element} is an element");
    }
}

#[test]
fn no_vocabulary_element_can_run_script_or_load_a_frame() {
    for banned in [
        "script", "style", "iframe", "object", "embed", "form", "link", "meta", "base", "svg",
        "math", "video", "audio", "source", "button", "select", "textarea", "frame", "noscript",
        "template",
    ] {
        assert!(!ELEMENTS.contains(&banned), "{banned}");
    }
    for global in GLOBAL_ATTRIBUTES {
        assert!(!global.starts_with("on"), "{global} is an event handler");
        assert_ne!(*global, "style", "an inline style is blocked by the CSP");
    }
    for (_, attrs) in ELEMENT_ATTRIBUTES {
        for a in *attrs {
            assert!(!a.starts_with("on"), "{a} is an event handler");
            assert!(
                !matches!(*a, "style" | "srcset" | "formaction" | "action" | "target"),
                "{a}"
            );
        }
    }
}

#[test]
fn class_names_are_whole_or_a_known_prefix_and_a_plain_name() {
    for ok in [
        "wikilink",
        "wikilink-missing",
        "callout",
        "callout-tip",
        "callout-warning",
        "directive-toc",
        "language-rust",
        "language-c++",
        "language-objective-c",
    ] {
        assert!(class_is_known(ok), "{ok}");
    }
    for bad in [
        "",
        "site-header",
        "verified",
        "badge",
        "callout-",
        "language-",
        "Wikilink",
        "callout-a b",
        "callout-<script>",
        "language-\"",
        &format!("language-{}", "x".repeat(49)),
    ] {
        assert!(!class_is_known(bad), "{bad:?}");
    }
    assert!(class_is_known(&format!("language-{}", "x".repeat(48))));
}

#[test]
fn attributes_are_global_prefixed_or_the_elements_own() {
    assert!(attribute_is_known("a", "href"));
    assert!(attribute_is_known("img", "src"));
    assert!(attribute_is_known("p", "id"));
    assert!(attribute_is_known("span", "data-wiki-target"));
    assert!(attribute_is_known("a", "aria-label"));
    assert!(!attribute_is_known("p", "href"));
    assert!(!attribute_is_known("a", "onclick"));
    assert!(!attribute_is_known("a", "style"));
    assert!(!attribute_is_known("img", "srcset"));
    assert!(!attribute_is_known("div", "data"));
}
