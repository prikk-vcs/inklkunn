#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

fn shape(body: &str) -> Vec<(BlockKind, Option<&str>, &str)> {
    blocks(body)
        .into_iter()
        .map(|b| (b.kind, b.id, b.text))
        .collect()
}

#[test]
fn every_kind_of_top_level_block_is_one_block() {
    let body = "# Title\n\nA paragraph\nover two lines. ^p1\n\n- one\n- two\n\n> quoted\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nlet x = 1;\n\nlet y = 2;\n```\n\n---\n\n<!-- a comment -->\n\n[^n]: A note.\n";
    let kinds: Vec<BlockKind> = blocks(body).iter().map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        [
            BlockKind::Heading,
            BlockKind::Paragraph,
            BlockKind::List,
            BlockKind::Quotation,
            BlockKind::Table,
            BlockKind::CodeBlock,
            BlockKind::Rule,
            BlockKind::Html,
            BlockKind::Footnote,
        ]
    );
    // The blank line inside the code block does not split it.
    let code = blocks(body)
        .into_iter()
        .find(|b| b.kind == BlockKind::CodeBlock)
        .unwrap();
    assert_eq!(code.text, "```rust\nlet x = 1;\n\nlet y = 2;\n```");
}

#[test]
fn a_block_is_compared_with_its_surrounding_whitespace_trimmed() {
    let a = blocks("  First  \n\n\n\nSecond\n\n");
    let b = blocks("First\nSecond");
    assert_eq!(
        a.iter().map(|x| x.text).collect::<Vec<_>>(),
        ["First", "Second"]
    );
    // Two lines with no blank line between them are one paragraph.
    assert_eq!(b.len(), 1);
    let c = blocks("First\n\nSecond");
    assert_eq!(
        c.iter().map(|x| x.text).collect::<Vec<_>>(),
        ["First", "Second"]
    );
}

#[test]
fn a_block_identifier_names_the_block_it_ends() {
    assert_eq!(
        shape("Para ^p1\n\nNo id\n\n## Heading ^h\n"),
        [
            (BlockKind::Paragraph, Some("p1"), "Para ^p1"),
            (BlockKind::Paragraph, None, "No id"),
            // An identifier on a heading line is not one (MKUP-9).
            (BlockKind::Heading, None, "## Heading ^h"),
        ]
    );
}

#[test]
fn an_identifier_on_a_line_of_its_own_names_the_block_before_it() {
    let body = "- a\n- b\n\n^list\n\n> quote\n\n^quote\n\n```\ncode\n```\n^code\n\n| a |\n|---|\n| 1 |\n^table\n\nplain\n\n^plain\n";
    assert_eq!(
        shape(body),
        [
            (BlockKind::List, Some("list"), "- a\n- b\n\n^list"),
            (BlockKind::Quotation, Some("quote"), "> quote\n\n^quote"),
            (BlockKind::CodeBlock, Some("code"), "```\ncode\n```\n^code"),
            (
                BlockKind::Table,
                Some("table"),
                "| a |\n|---|\n| 1 |\n^table"
            ),
            (BlockKind::Paragraph, Some("plain"), "plain\n\n^plain"),
        ]
    );
}

#[test]
fn an_identifier_inside_a_list_item_names_the_item_not_the_list() {
    let blocks = blocks("- item ^i\n- other\n");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks.first().map(|b| b.kind), Some(BlockKind::List));
    assert_eq!(blocks.first().and_then(|b| b.id), None);
}

#[test]
fn a_second_identifier_after_a_block_that_has_one_is_a_block_of_its_own() {
    assert_eq!(
        shape("One ^a\n\n^b\n"),
        [
            (BlockKind::Paragraph, Some("a"), "One ^a"),
            (BlockKind::Paragraph, Some("b"), "^b"),
        ]
    );
}

#[test]
fn a_container_is_one_block_with_everything_in_it() {
    let body = "Before.\n\n::: note Heads up\nInside one.\n\n- list\n\n::: inner\nNested.\n:::\n\nAfter nested.\n:::\n\nAfter.\n";
    assert_eq!(
        shape(body),
        [
            (BlockKind::Paragraph, None, "Before."),
            (
                BlockKind::Container,
                None,
                "::: note Heads up\nInside one.\n\n- list\n\n::: inner\nNested.\n:::\n\nAfter nested.\n:::"
            ),
            (BlockKind::Paragraph, None, "After."),
        ]
    );
}

#[test]
fn a_container_ends_the_paragraph_before_it_and_an_unclosed_one_runs_to_the_end() {
    assert_eq!(
        shape("Line\n::: tip\nInside\n\nStill inside\n"),
        [
            (BlockKind::Paragraph, None, "Line"),
            (
                BlockKind::Container,
                None,
                "::: tip\nInside\n\nStill inside"
            ),
        ]
    );
}

#[test]
fn container_markers_in_code_or_html_or_with_nothing_open_are_text() {
    let fenced = blocks("```\n::: note\ntext\n```\n\nAfter\n");
    assert_eq!(fenced.len(), 2);
    assert!(fenced.iter().all(|b| b.kind != BlockKind::Container));
    let orphan = blocks(":::\n\nText\n");
    assert!(orphan.iter().all(|b| b.kind != BlockKind::Container));
    // Not a marker: `:::` followed by something that is not a name.
    let not_a_name = blocks("::: 1\n\nText\n");
    assert!(not_a_name.iter().all(|b| b.kind != BlockKind::Container));
}

#[test]
fn what_the_parser_consumes_without_an_event_is_still_a_block() {
    let body = "Text\n\n[ref]: https://example.org/\n\nMore\n";
    assert_eq!(
        shape(body),
        [
            (BlockKind::Paragraph, None, "Text"),
            (BlockKind::Other, None, "[ref]: https://example.org/"),
            (BlockKind::Paragraph, None, "More"),
        ]
    );
}

#[test]
fn only_whitespace_lies_between_blocks() {
    for body in [
        "",
        "\n\n",
        "# H\n\n[a]: /b\n\n::: x\n[c]: /d\n:::\n\ntext ^i\n\n^j\n- l\n\n\n",
        "---\n\nplain\r\n\r\nwith crlf\r\n",
        "> [!NOTE]\n> callout\n\n| a |\n|-|\n^t\n",
    ] {
        let found = blocks(body);
        let mut end = 0;
        for b in &found {
            assert!(b.range.start >= end, "overlap in {body:?}");
            assert!(
                body.get(end..b.range.start)
                    .is_some_and(|gap| gap.trim().is_empty()),
                "text between blocks in {body:?}"
            );
            assert_eq!(body.get(b.range.clone()), Some(b.text));
            assert_eq!(b.text, b.text.trim());
            assert!(!b.text.is_empty());
            end = b.range.end;
        }
        assert!(body.get(end..).is_some_and(|t| t.trim().is_empty()));
    }
}

#[test]
fn crlf_text_is_segmented_like_lf_text() {
    let lf = blocks("One ^a\n\nTwo\n\n- x\n- y\n");
    let crlf = blocks("One ^a\r\n\r\nTwo\r\n\r\n- x\r\n- y\r\n");
    assert_eq!(
        lf.iter().map(|b| (b.kind, b.id)).collect::<Vec<_>>(),
        crlf.iter().map(|b| (b.kind, b.id)).collect::<Vec<_>>()
    );
}

#[test]
fn an_identifier_right_after_the_last_item_names_that_item() {
    // No blank line: the line is part of the item (the parser's rule, which the renderer follows).
    let found = blocks("- a\n- b\n^b\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found.first().and_then(|b| b.id), None);
}
