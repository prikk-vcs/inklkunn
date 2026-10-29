//! The word-level refinement never changes the line diff (handoff 003 T5): whatever is marked, writing the
//! model back gives prikk's hunk byte for byte.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

fn lines(raw: &str) -> Hunk {
    Hunk::parse(raw).expect("a unified hunk")
}

/// Line `i` of a hunk.
fn at(h: &Hunk, i: usize) -> &DiffLine {
    h.lines.get(i).expect("a line")
}

fn marked_of(line: &DiffLine) -> Vec<&str> {
    line.segments
        .iter()
        .filter(|s| s.marked)
        .map(|s| s.text.as_str())
        .collect()
}

#[test]
fn a_hunk_is_parsed_into_lines_and_written_back_exactly() {
    let raw = "@@ -6,4 +6,4 @@ Section\n \n # Home\n \n-Welcome. See [[A]].\n+Welcome to the whole wiki. See [[A]].\n\\ No newline at end of file\n";
    let h = lines(raw);
    assert_eq!((h.old_start, h.new_start), (Some(6), Some(6)));
    assert_eq!(
        h.lines.iter().map(|l| l.kind).collect::<Vec<_>>(),
        vec![
            LineKind::Context,
            LineKind::Context,
            LineKind::Context,
            LineKind::Removed,
            LineKind::Added,
            LineKind::Marker
        ]
    );
    assert_eq!((h.removed(), h.added()), (1, 1));
    assert_eq!(h.to_unified(), raw);
    // A hunk with no closing line feed is kept without one.
    let bare = "@@ -1 +1 @@\n-a\n+b";
    assert_eq!(lines(bare).to_unified(), bare);
}

#[test]
fn the_words_that_differ_are_marked_and_nothing_else() {
    let mut h =
        lines("@@ -1 +1 @@\n-Welcome to the wiki, friend.\n+Welcome to the whole wiki, friend.\n");
    h.refine();
    let (old, new) = (at(&h, 0), at(&h, 1));
    assert!(
        marked_of(old).is_empty(),
        "nothing was removed from the words"
    );
    assert_eq!(
        marked_of(new).iter().map(|m| m.trim()).collect::<Vec<_>>(),
        vec!["whole"]
    );
    assert_eq!(old.text(), "Welcome to the wiki, friend.");
    assert_eq!(new.text(), "Welcome to the whole wiki, friend.");

    let mut h = lines("@@ -1 +1 @@\n-The cat sat on the mat.\n+The dog sat on the mat.\n");
    h.refine();
    assert_eq!(marked_of(at(&h, 0)), vec!["cat"]);
    assert_eq!(marked_of(at(&h, 1)), vec!["dog"]);
}

#[test]
fn japanese_text_is_marked_character_by_character() {
    let mut h = lines("@@ -1 +1 @@\n-日本語のページです。\n+日本語の新しいページです。\n");
    h.refine();
    assert_eq!(marked_of(at(&h, 1)), vec!["新しい"]);
    assert!(marked_of(at(&h, 0)).is_empty());
}

#[test]
fn lines_with_little_in_common_are_left_whole() {
    let mut h = lines("@@ -1 +1 @@\n-alpha beta gamma delta\n+one two three four\n");
    h.refine();
    assert!(
        h.lines
            .iter()
            .all(|l| l.segments.len() == 1 && l.segments.iter().all(|s| !s.marked))
    );
    // A line longer than the bound is not compared.
    let long_a = "word ".repeat(MAX_WORDS);
    let long_b = format!("{long_a}more");
    let mut h = lines(&format!("@@ -1 +1 @@\n-{long_a}\n+{long_b}\n"));
    h.refine();
    assert!(h.lines.iter().all(|l| l.segments.iter().all(|s| !s.marked)));
}

#[test]
fn removed_and_added_lines_are_paired_in_order_and_surplus_lines_stay_whole() {
    let raw = "@@ -1,3 +1,2 @@\n-one two three\n-four five six\n-seven eight nine\n+one 2 three\n+four 5 six\n";
    let mut h = lines(raw);
    h.refine();
    assert_eq!(marked_of(at(&h, 0)), vec!["two"]);
    assert_eq!(marked_of(at(&h, 1)), vec!["five"]);
    assert!(
        marked_of(at(&h, 2)).is_empty(),
        "no added line to pair with"
    );
    assert_eq!(marked_of(at(&h, 3)), vec!["2"]);
    assert_eq!(h.to_unified(), raw);
}

#[test]
fn what_is_not_a_unified_hunk_is_kept_as_text_and_never_interpreted() {
    for raw in [
        "",
        "not a hunk\n+x\n",
        "@@ -1 +1 @@\nno prefix\n",
        "@@ -1 +1 @@\n\n+an empty line is not a context line\n",
        "diff --git a b\n",
    ] {
        match HunkView::of(raw) {
            HunkView::Raw(text) => assert_eq!(text, raw),
            HunkView::Lines(h) => panic!("{raw:?} was read as lines: {h:?}"),
        }
        assert_eq!(HunkView::of(raw).to_unified(), raw);
    }
}

/// A small deterministic generator, so the property is checked on many inputs and fails the same way twice.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
        of.get(self.next() as usize % of.len()).copied().unwrap()
    }
}

#[test]
fn marks_never_change_the_line_diff_whatever_the_text() {
    let vocabulary = [
        "the",
        "wiki",
        "Wiki",
        "page",
        " ",
        "  ",
        "\t",
        ",",
        ".",
        "[[Link]]",
        "**bold**",
        "日本",
        "語",
        "ページ",
        "<b>",
        "&amp;",
        "'",
        "\"",
        "é",
        "ß",
        "123",
        "a_b",
        "-",
        "+",
        "@@",
        "\\",
        "\r",
    ];
    let mut rng = Lcg(7);
    let line = |rng: &mut Lcg| -> String {
        (0..rng.next() % 14)
            .map(|_| rng.pick(&vocabulary))
            .collect::<String>()
    };
    for round in 0..400 {
        let mut raw = String::from("@@ -1,9 +1,9 @@ x\n");
        for _ in 0..(1 + rng.next() % 6) {
            let (kind, text) = match rng.next() % 4 {
                0 => (' ', line(&mut rng)),
                1 => ('-', line(&mut rng)),
                _ => ('+', line(&mut rng)),
            };
            raw.push(kind);
            raw.push_str(&text.replace('\n', " "));
            raw.push('\n');
        }
        let before = Hunk::parse(&raw).expect("a hunk");
        let mut refined = before.clone();
        refined.refine();
        assert_eq!(refined.to_unified(), raw, "round {round}: {raw:?}");
        for (b, r) in before.lines.iter().zip(&refined.lines) {
            assert_eq!(b.kind, r.kind, "round {round}");
            assert_eq!(b.text(), r.text(), "round {round}: a mark changed a line");
            if r.kind != LineKind::Removed && r.kind != LineKind::Added {
                assert!(r.segments.iter().all(|s| !s.marked), "round {round}");
            }
        }
        assert_eq!(
            (before.added(), before.removed()),
            (refined.added(), refined.removed())
        );
    }
}
