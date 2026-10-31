//! `judged_on`: SHA-256 of the block text as `inklkunn_markup::blocks` trims it, one function, in both languages.

use super::*;

fn only_block(page: &str) -> String {
    let (_, body, _) = inklkunn_markup::frontmatter::split(page);
    let found = inklkunn_markup::blocks(body);
    assert_eq!(found.len(), 1, "one block");
    block_digest(found.first().expect("a block")).to_string()
}

#[test]
fn english_block_text_has_the_known_digest() {
    // sha256("The bridge opened in 1950. ^b1"), computed outside the product.
    assert_eq!(
        only_block("The bridge opened in 1950. ^b1\n"),
        "9feb492ac6a129a47bb2554309b111960bf4fc3c03cc5518f3795ea1a92633cd"
    );
}

#[test]
fn japanese_block_text_has_the_known_digest() {
    // sha256("橋は一九五〇年に開通した。 ^b1"), computed outside the product.
    assert_eq!(
        only_block("橋は一九五〇年に開通した。 ^b1\n"),
        "f88fa2b03c28fea8b0594d11b5f71096b1b93fde7f0093dec351190c0a3caeaa"
    );
}

#[test]
fn the_trimming_is_the_blocks_not_ours() {
    let plain = only_block("The bridge opened in 1950. ^b1");
    // Blank lines, a frontmatter, a closing newline and a CRLF page are the same words.
    assert_eq!(
        only_block("\n\n The bridge opened in 1950. ^b1  \n\n"),
        plain
    );
    assert_eq!(
        only_block("---\ntitle: Bridge\nid: p1\n---\n\nThe bridge opened in 1950. ^b1\n"),
        plain
    );
    assert_eq!(only_block("The bridge opened in 1950. ^b1\r\n"), plain);
}

#[test]
fn a_changed_word_is_a_changed_digest_in_both_languages() {
    assert_ne!(
        only_block("The bridge opened in 1950. ^b1"),
        only_block("The bridge opened in 1951. ^b1")
    );
    assert_ne!(
        only_block("橋は一九五〇年に開通した。 ^b1"),
        only_block("橋は一九五一年に開通した。 ^b1")
    );
}

#[test]
fn a_digest_is_sixty_four_lower_case_hexadecimal_characters() {
    let d = digest_of("anything");
    assert_eq!(d.as_str().len(), 64);
    assert_eq!(Digest::parse(d.as_str()), Some(d.clone()));
    assert_eq!(Digest::parse(&d.as_str().to_uppercase()), None);
    assert_eq!(
        Digest::parse(d.as_str().get(..63).expect("63 characters")),
        None
    );
    assert_eq!(Digest::parse(""), None);
}

#[test]
fn the_anchor_state_uses_the_same_function() {
    let page = "---\nid: p1\n---\n\nIntro.\n\nThe bridge opened in 1950. ^b1\n\nAnd this. ^b2\n";
    let expect = only_block("The bridge opened in 1950. ^b1");
    assert_eq!(
        anchor_state(page, "b1"),
        AnchorState::Present(Digest::parse(&expect).expect("a digest"))
    );
    assert_eq!(anchor_state(page, "nope"), AnchorState::BlockMissing);
    let twice = "One. ^b1\n\nTwo. ^b1\n";
    assert_eq!(anchor_state(twice, "b1"), AnchorState::BlockRepeated);
}
