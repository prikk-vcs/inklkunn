#![allow(clippy::expect_used, clippy::unwrap_used)]
use crate::{NoLinks, render_html};

fn html(body: &str) -> String {
    render_html(body, &NoLinks).unsanitized_html
}

/// The conformance case `006-block-ids`: every form MKUP-9 gives, in the order the profile gives them.
const FORMS: &str = "A paragraph ending with an identifier. ^para-1

- list item one ^item-1
- list item two

> a quote ^quote-1

| a | b |
|---|---|
| 1 | 2 |
^table-1

```
code ^not-in-code
```
^code-1

## Heading ^not-a-block-id
";

#[test]
fn every_form_the_profile_gives_carries_its_id_on_its_block_and_nothing_stands_beside_it() {
    let h = html(FORMS);
    assert!(
        h.contains("<p id=\"^para-1\">A paragraph ending with an identifier.</p>"),
        "{h}"
    );
    assert!(h.contains("<li id=\"^item-1\">list item one</li>"), "{h}");
    assert!(h.contains("<li>list item two</li>"), "{h}");
    assert!(
        h.contains("<blockquote>\n<p id=\"^quote-1\">a quote</p>\n</blockquote>"),
        "{h}"
    );
    assert!(h.contains("<table id=\"^table-1\">"), "{h}");
    assert!(
        h.contains("<pre id=\"^code-1\"><code>code ^not-in-code\n</code></pre>"),
        "{h}"
    );
    // The heading's caret is text, as the scanner says.
    assert!(h.contains("Heading ^not-a-block-id</h2>"), "{h}");
    // No marker is left, no empty row came of the identifier after the table, no empty paragraph of the
    // identifier after the code.
    assert!(!h.contains("class=\"block-id\""), "{h}");
    assert!(!h.contains("<td></td>"), "{h}");
    assert!(!h.contains("<p></p>"), "{h}");
    assert_eq!(
        h.matches("<tr>").count(),
        2,
        "the header row and one row: {h}"
    );
}

#[test]
fn an_identifier_on_a_line_of_its_own_names_the_block_before_it_whatever_the_block() {
    let h = html("```rust\nlet x = 1;\n```\n\n^code\n");
    assert!(
        h.contains("<pre id=\"^code\"><code class=\"language-rust\">let x = 1;\n</code></pre>"),
        "{h}"
    );
    let h = html("| a |\n|---|\n| 1 |\n\n^table\n");
    assert!(h.contains("<table id=\"^table\">"), "{h}");
    let h = html("- one\n- two\n\n^list\n");
    assert!(h.contains("<ul id=\"^list\">"), "{h}");
    let h = html("3. three\n4. four\n\n^ordered\n");
    assert!(h.contains("<ol start=\"3\" id=\"^ordered\">"), "{h}");
    let h = html("1. one\n2. two\n\n^plain\n");
    assert!(h.contains("<ol id=\"^plain\">"), "{h}");
    let h = html("> quoted\n\n^quote\n");
    assert!(h.contains("<blockquote id=\"^quote\">"), "{h}");
    let h = html("First.\n\n^first\n\nSecond.\n");
    assert!(h.contains("<p id=\"^first\">First.</p>"), "{h}");
    assert!(h.contains("<p>Second.</p>"), "{h}");
    let h = html("    indented code\n\n^ic\n");
    assert!(
        h.contains("<pre id=\"^ic\"><code>indented code\n</code></pre>"),
        "{h}"
    );
}

#[test]
fn the_innermost_block_that_holds_the_identifier_takes_it() {
    // A loose list item: the paragraph in it.
    let h = html("- loose ^l1\n\n- two\n");
    assert!(h.contains("<p id=\"^l1\">loose</p>"), "{h}");
    assert!(!h.contains("<li id="), "{h}");
    // An identifier inside emphasis (which runs on to the next line) still names the paragraph around it.
    let h = html("*first line ^em\nsecond line*\n");
    assert!(h.contains("<p id=\"^em\"><em>first line"), "{h}");
    assert!(!h.contains("block-id"), "{h}");
    // A nested list item names its own item.
    let h = html("- outer\n  - inner ^deep\n");
    assert!(h.contains("<li id=\"^deep\">inner</li>"), "{h}");
}

#[test]
fn an_identifier_that_cannot_be_placed_keeps_its_marker_so_the_anchor_exists() {
    // Inside a heading (a setext heading is not one the scanner skips).
    let h = html("Setext ^h1\n======\n");
    assert!(
        h.contains("<span class=\"block-id\" id=\"^h1\"></span>"),
        "{h}"
    );
    assert!(h.contains("<h1 id=\"setext\">"), "{h}");
    // A second identifier in a block that has one: the first names the block, the second stays a marker.
    let h = html("One ^a\nTwo ^b\n");
    assert!(h.contains("<p id=\"^a\">One"), "{h}");
    assert!(
        h.contains("<span class=\"block-id\" id=\"^b\"></span>"),
        "{h}"
    );
    // A trailer with nothing before it is the paragraph that holds it.
    let h = html("^alone\n\nAfter.\n");
    assert!(h.contains("<p id=\"^alone\"></p>"), "{h}");
    // A trailer after a block that already has an id does not take it from it.
    let h = html("Para ^p\n\n^q\n");
    assert!(h.contains("<p id=\"^p\">Para</p>"), "{h}");
    assert!(h.contains("<p id=\"^q\"></p>"), "{h}");
    // A trailer is not carried across a rule.
    let h = html("Before.\n\n---\n\n^after-rule\n");
    assert!(h.contains("<p id=\"^after-rule\"></p>"), "{h}");
    assert!(h.contains("<p>Before.</p>"), "{h}");
}

#[test]
fn identifiers_in_callouts_and_directives_land_on_their_paragraphs() {
    let h = html("> [!NOTE]\n> Careful here. ^warn\n");
    assert!(h.contains("<p id=\"^warn\">Careful here.</p>"), "{h}");
    let h = html(":::note Heads up\nInside. ^in\n:::\n");
    assert!(h.contains("<p id=\"^in\">Inside.</p>"), "{h}");
}

#[test]
fn what_is_not_an_identifier_is_left_alone() {
    let h = html("email^address and a caret ^ alone, nor ^ with space after.\n");
    assert!(!h.contains("id="), "{h}");
    assert!(!h.contains("block-id"), "{h}");
}
