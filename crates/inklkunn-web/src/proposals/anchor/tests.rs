#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

const REV1: &str =
    "---\ntitle: Home\n---\n\nAlpha paragraph.\n\nBeta paragraph.\n\nGamma paragraph.\n";

fn anchor_on(text: &str, ordinal: usize) -> Anchor {
    make("pages/Home.md", text, ordinal).unwrap()
}

#[test]
fn an_anchor_holds_the_parts_text_with_its_surroundings_and_its_position() {
    let a = anchor_on(REV1, 1);
    let q = a.quote.unwrap();
    assert_eq!(q.exact, "Beta paragraph.");
    assert!(q.prefix.ends_with("Alpha paragraph."));
    assert!(q.suffix.starts_with("Gamma paragraph."));
    assert_eq!(a.ordinal, 1);
    assert_eq!(a.block_id, None);
    assert!(make("pages/Home.md", REV1, 9).is_none());
}

#[test]
fn a_quote_finds_its_part_again_after_a_revision_moves_it() {
    let a = anchor_on(REV1, 1);
    // Revision 2 puts a new part in front: Beta is the third part now.
    let rev2 = "---\ntitle: Home\n---\n\nNew first.\n\nAlpha paragraph.\n\nBeta paragraph.\n\nGamma paragraph.\n";
    assert_eq!(land(&a, rev2), Landing::At(2));
    // The same text in two places: the surroundings decide.
    let twice = "Beta paragraph.\n\nAlpha paragraph.\n\nBeta paragraph.\n\nGamma paragraph.\n";
    assert_eq!(land(&a, twice), Landing::At(2));
}

#[test]
fn a_quote_that_matches_nothing_is_orphaned() {
    let a = anchor_on(REV1, 1);
    let gone = "---\ntitle: Home\n---\n\nAlpha paragraph.\n\nGamma paragraph.\n";
    assert_eq!(land(&a, gone), Landing::Orphaned);
    assert_eq!(land(&a, ""), Landing::Orphaned);
}

#[test]
fn an_identifier_is_followed_even_when_the_text_changes() {
    let with_id = "Alpha.\n\nBeta paragraph. ^beta\n\nGamma.\n";
    let a = make("pages/Home.md", with_id, 1).unwrap();
    assert_eq!(a.block_id.as_deref(), Some("beta"));
    let edited = "Gamma.\n\nBeta, rewritten entirely. ^beta\n";
    assert_eq!(land(&a, edited), Landing::At(1));
    // Without the identifier and with the text gone, nothing matches.
    let no_id = "Gamma.\n\nSomething else.\n";
    assert_eq!(land(&a, no_id), Landing::Orphaned);
}

#[test]
fn a_long_part_is_quoted_by_its_beginning_at_a_character_boundary() {
    let long = "日本語".repeat(1000);
    let text = format!("{long}\n\nafter\n");
    let a = make("pages/Home.md", &text, 0).unwrap();
    let q = a.quote.clone().unwrap();
    assert!(q.exact.len() <= 1024 && text.contains(&q.exact));
    assert_eq!(land(&a, &text), Landing::At(0));
}
