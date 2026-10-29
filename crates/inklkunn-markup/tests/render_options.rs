//! The options of a render beyond how the source is read.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_markup::{NoLinks, RenderOptions, ScanOptions, render_page_with};

fn render(body: &str, demote: u8) -> inklkunn_markup::Rendered {
    render_page_with(
        None,
        body,
        &NoLinks,
        ScanOptions::default(),
        RenderOptions {
            demote_headings: demote,
            ..RenderOptions::default()
        },
    )
}

#[test]
fn headings_move_down_by_the_asked_number_and_never_below_h6() {
    let body = "# One\n\n## Two\n\n##### Five\n\n###### Six\n";
    let plain = render(body, 0).unsanitized_html;
    assert!(
        plain.contains("<h1 id=") && plain.contains("<h6 id="),
        "{plain}"
    );
    let down = render(body, 1);
    let h = &down.unsanitized_html;
    assert!(h.contains("<h2 id=\"one\">One</h2>"), "{h}");
    assert!(h.contains("<h3 id=\"two\">Two</h3>"), "{h}");
    assert!(h.contains("<h6 id=\"five\">Five</h6>"), "{h}");
    assert!(h.contains("<h6 id=\"six\">Six</h6>"), "never below h6: {h}");
    assert!(!h.contains("<h1"), "{h}");
    // The contents list is about the same headings with the same ids, whatever their level on the page.
    let ids: Vec<&str> = down.toc.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["one", "two", "five", "six"]);
}

#[test]
fn the_default_leaves_headings_where_they_are() {
    assert_eq!(
        RenderOptions::default(),
        RenderOptions {
            author_html: inklkunn_markup::AuthorHtml::Escape,
            demote_headings: 0
        }
    );
}
