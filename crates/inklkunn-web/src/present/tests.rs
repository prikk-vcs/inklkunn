#![allow(clippy::expect_used, clippy::unwrap_used)]
use inklkunn_core::read::Presentation;

use super::*;
use crate::catalog::{EN, Lang};

fn ja() -> Lang {
    Lang::from_tag("ja").expect("the Japanese preview is compiled in")
}

#[test]
fn links_follow_the_url_scheme_and_a_view_has_its_own_cache_key() {
    let main = WikiLinks::new(T::new(EN), "handbook", None);
    let draft = WikiLinks::new(T::new(EN), "handbook", Some("draft"));
    assert_eq!(main.page_href("Home"), "/handbook/Home");
    assert_eq!(
        main.page_href("日本語/ページ"),
        "/handbook/%E6%97%A5%E6%9C%AC%E8%AA%9E/%E3%83%9A%E3%83%BC%E3%82%B8"
    );
    assert_eq!(draft.page_href("Home"), "/handbook/~draft/Home");
    assert_eq!(main.page_href("~odd"), "/handbook/%7Eodd");
    assert_ne!(main.key(), draft.key());
    assert_ne!(
        WikiLinks::new(T::new(EN), "a", None).key(),
        WikiLinks::new(T::new(EN), "b", None).key()
    );
    assert_eq!(main.lang(), "en");
    assert_eq!(WikiLinks::new(T::new(ja()), "handbook", None).lang(), "ja");
    assert!(
        main.create_href("Nowhere").is_none(),
        "no page can be created yet"
    );
}

#[test]
fn the_words_of_an_included_page_come_from_the_catalog_in_the_readers_language() {
    let en = WikiLinks::new(T::new(EN), "w", None);
    let ja = WikiLinks::new(T::new(ja()), "w", None);
    assert_eq!(en.transclusion_label("Glossary"), "From Glossary");
    assert_eq!(ja.transclusion_label("Glossary"), "Glossary から");
    let cycle = TransclusionRefusal::Cycle {
        chain: vec!["A".into(), "B".into(), "A".into()],
    };
    assert!(en.transclusion_notice(&cycle).contains("A → B → A"));
    assert!(ja.transclusion_notice(&cycle).contains("A → B → A"));
    assert_ne!(
        en.transclusion_notice(&cycle),
        ja.transclusion_notice(&cycle)
    );
    let deep = TransclusionRefusal::TooDeep {
        title: "X".into(),
        limit: 2,
    };
    assert_eq!(
        en.transclusion_notice(&deep),
        "Not included, deeper than 2: X"
    );
    let budget = TransclusionRefusal::OverBudget { title: "Y".into() };
    assert!(en.transclusion_notice(&budget).ends_with(": Y"));
}
