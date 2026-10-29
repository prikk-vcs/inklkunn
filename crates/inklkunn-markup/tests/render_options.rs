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
            demote_headings: 0,
            id_prefix: "",
        }
    );
}

fn render_prefixed(
    body: &str,
    links: &dyn inklkunn_markup::RenderLinks,
) -> inklkunn_markup::Rendered {
    render_page_with(
        None,
        body,
        links,
        ScanOptions::default(),
        RenderOptions {
            id_prefix: "h-",
            ..RenderOptions::default()
        },
    )
}

#[test]
fn a_prefix_goes_before_every_heading_id_the_contents_list_and_every_link_to_one() {
    let body = "# Main\n\n## Two words\n\nA paragraph. ^para\n\nSee [[#Main]], [[#^para]] and [[Other#Two words]].\n";
    let r = render_prefixed(body, &NoLinks);
    let h = &r.unsanitized_html;
    assert!(h.contains("<h1 id=\"h-main\">Main</h1>"), "{h}");
    assert!(h.contains("<h2 id=\"h-two-words\">Two words</h2>"), "{h}");
    assert!(h.contains("id=\"h-^para\""), "{h}");
    assert!(h.contains("href=\"#h-main\""), "{h}");
    assert!(h.contains("href=\"#h-%5Epara\""), "{h}");
    let ids: Vec<&str> = r.toc.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["h-main", "h-two-words"]);
    // No id of the page is one a template could have.
    assert!(!h.contains("id=\"main\""), "{h}");
}

#[test]
fn a_link_to_a_heading_of_another_page_carries_the_prefix_of_a_page_of_its_own() {
    struct To;
    impl inklkunn_markup::RenderLinks for To {
        fn link(&self, _: &inklkunn_markup::scan::Link) -> inklkunn_markup::LinkHref {
            inklkunn_markup::LinkHref::Page {
                href: "/w/Other".to_owned(),
                title: Some("Other".to_owned()),
            }
        }
    }
    let r = render_prefixed("[[Other#Two words]] and [[Other#^para]]\n", &To);
    let h = &r.unsanitized_html;
    assert!(h.contains("href=\"/w/Other#h-two-words\""), "{h}");
    assert!(h.contains("href=\"/w/Other#h-%5Epara\""), "{h}");
    let plain = render_page_with(
        None,
        "[[Other#Two words]]\n",
        &To,
        ScanOptions::default(),
        RenderOptions::default(),
    );
    assert!(
        plain
            .unsanitized_html
            .contains("href=\"/w/Other#two-words\""),
        "no prefix unless asked for"
    );
}
