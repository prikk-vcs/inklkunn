#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_store::{DisplayName, MemoryStore, NewWiki, Slug, WikiStore};

use super::*;
use crate::catalog::{EN, Lang};

fn wiki() -> WikiRecord {
    MemoryStore::new()
        .create(NewWiki::new(
            Slug::new("notes").unwrap(),
            DisplayName::new("Notes").unwrap(),
        ))
        .unwrap()
}

const CLAIM: &str = "01930000-0000-7000-8000-000000000001";

#[test]
fn a_thread_that_cannot_be_read_says_so_in_both_languages_and_offers_no_form() {
    let w = wiki();
    let ja = Lang::from_tag("ja").unwrap();
    for (lang, says, label, sentence) in [
        (
            EN,
            "The discussion of this claim could not be read now.",
            "Discussion of this claim",
            "A discussion never changes a claim",
        ),
        (
            ja,
            "この主張の議論を現在は読み取れません。",
            "この主張についての議論",
            "議論が主張を変えることはありません。",
        ),
    ] {
        let section = section(T::new(lang), &Read::none(), &w, CLAIM, None);
        assert_eq!(section.unavailable.as_deref(), Some(says));
        assert_eq!(section.heading, label);
        // The label and the sentence stand whether or not the thread can be read.
        assert!(section.never_changes.starts_with(sentence));
        assert!(section.form.is_none() && section.comments.is_empty());
        assert!(section.total.is_none() && section.none.is_none());
        assert!(section.first.is_none() && section.more.is_none());
    }
}

#[test]
fn the_addresses_of_a_place_in_a_thread_carry_only_what_was_given_and_the_reply_leads_to_the_form()
{
    assert_eq!(
        paths::claim_page("w", CLAIM, None, None, None),
        format!("/w/-/claim/{CLAIM}")
    );
    assert_eq!(
        paths::claim_page("w", CLAIM, Some("draft"), Some("c1"), None),
        format!("/w/-/claim/{CLAIM}?view=draft&after=c1")
    );
    assert_eq!(
        paths::reply("w", CLAIM, Some("draft"), Some("c1"), "c2"),
        format!("/w/-/claim/{CLAIM}?view=draft&after=c1&reply=c2#claim-comment")
    );
    assert_eq!(
        paths::discuss("w", CLAIM),
        format!("/w/-/claim/{CLAIM}/discuss")
    );
}
