#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn folding_makes_width_variants_diacritics_and_case_one_letter() {
    // Full-width Latin and digits, half-width katakana, compatibility forms.
    assert_eq!(fold("Ｗｉｋｉ　１２３"), "wiki 123");
    assert_eq!(fold("ｳｨｷ"), "ウィキ");
    assert_eq!(fold("ﾊﾟｰﾄ"), "パート");
    // Diacritics, composed or not.
    assert_eq!(fold("Café"), "cafe");
    assert_eq!(fold("Cafe\u{301}"), "cafe");
    assert_eq!(fold("Zürich Ångström"), "zurich angstrom");
    // Case, and runs of space.
    assert_eq!(fold("  Getting \t Started  "), "getting started");
    assert_eq!(fold("STRASSE"), "strasse");
}

#[test]
fn a_kana_voicing_mark_is_not_a_diacritic() {
    assert_eq!(fold("ガ"), "ガ");
    assert_ne!(fold("ガ"), fold("カ"));
    assert_ne!(fold("パ"), fold("ハ"));
    // The same letter spelled with the combining mark.
    assert_eq!(fold("カ\u{3099}"), "ガ");
    // Hangul survives the round trip through decomposition.
    assert_eq!(fold("한국어"), "한국어");
}

#[test]
fn a_japanese_title_is_found_by_any_part_of_it() {
    let folded = fold("日本語のウィキ入門");
    for part in ["ウィキ", "本語の", "入門", "日"] {
        assert!(folded.contains(&fold(part)), "{part}");
    }
}

fn joined(segments: &[Segment]) -> String {
    segments.iter().map(|s| s.text.as_str()).collect()
}

#[test]
fn a_highlight_marks_the_match_and_the_segments_add_up_to_the_text() {
    let s = highlight("Getting Started", "start");
    assert_eq!(joined(&s), "Getting Started");
    assert_eq!(
        s.iter().find(|x| x.matched).map(|x| x.text.as_str()),
        Some("Start")
    );

    let s = highlight("Café Crème", &fold("creme"));
    assert_eq!(joined(&s), "Café Crème");
    assert_eq!(
        s.iter().find(|x| x.matched).map(|x| x.text.as_str()),
        Some("Crème")
    );

    let s = highlight("日本語のウィキ入門", "ウィキ");
    assert_eq!(joined(&s), "日本語のウィキ入門");
    assert_eq!(
        s.iter().find(|x| x.matched).map(|x| x.text.as_str()),
        Some("ウィキ")
    );

    // Full-width in the text, plain in the query.
    let s = highlight("Ｗｉｋｉ pages", "wiki");
    assert_eq!(joined(&s), "Ｗｉｋｉ pages");
    assert!(s.iter().any(|x| x.matched && x.text == "Ｗｉｋｉ"));
}

#[test]
fn a_highlight_that_cannot_be_placed_is_the_whole_text_unmarked() {
    // A half-width kana and its voicing mark fold together; letter by letter they cannot be located.
    let s = highlight("ﾊﾟｰﾄ", &fold("パート"));
    assert_eq!(joined(&s), "ﾊﾟｰﾄ");
    // No match at all.
    let s = highlight("Home", "zzz");
    assert_eq!(s.len(), 1);
    assert!(!s.first().unwrap().matched);
    assert_eq!(highlight("Home", "").first().unwrap().text, "Home");
}

#[test]
fn a_highlight_keeps_what_an_author_wrote_as_it_is() {
    // The segments are the author's text, unescaped: escaping is the surface's.
    let s = highlight("<script>alert(1)</script>", "alert");
    assert_eq!(joined(&s), "<script>alert(1)</script>");
}
