//! What a proposal's pages need of the read path (handoff 004 T3): a file as it was at a block, and source text
//! that is not in the repository drawn as the page would be.

use super::super::testing::{Env, block_id};
use super::*;

fn world() -> World {
    World::pages_with(
        &[
            ("Home", &[], "Home text. See [[Guide]] and [[Nowhere]].\n"),
            ("Guide", &[], "Guide text.\n"),
        ],
        ttl_zero(),
    )
}

#[test]
fn a_file_is_read_as_it_was_at_the_block_it_is_asked_at() {
    let w = world();
    let first = block_id(1);
    w.fake()
        .set("pages/Guide.md", "---\ntitle: Guide\n---\n\nChanged.\n");
    let then = w.service.source_at(&w.wiki, &first, "pages/Guide.md");
    assert_eq!(
        then.unwrap().as_deref(),
        Some("---\ntitle: Guide\n---\n\nGuide text.\n")
    );
    let now = w
        .service
        .source_at(&w.wiki, &w.fake().block_id(), "pages/Guide.md");
    assert!(now.unwrap().unwrap().contains("Changed."));
}

#[test]
fn a_file_that_was_not_there_is_none_and_one_that_is_not_text_is_none() {
    let w = world();
    let first = block_id(1);
    assert_eq!(
        w.service
            .source_at(&w.wiki, &first, "pages/Nowhere.md")
            .unwrap(),
        None
    );
    w.fake().set_bytes("pages/Binary.md", &[0xff, 0xfe, 0x00]);
    assert_eq!(
        w.service
            .source_at(&w.wiki, &w.fake().block_id(), "pages/Binary.md")
            .unwrap(),
        None
    );
}

#[test]
fn text_is_drawn_as_the_page_would_be_with_its_links_resolved_against_the_tip() {
    let w = world();
    let p = w
        .service
        .preview(
            &w.wiki,
            None,
            "pages/New.md",
            "New",
            "# Heading\n\nSee [[Guide]] and [[Nowhere]].\n",
            &Env::EN,
        )
        .unwrap();
    let html = p.html.as_str();
    assert!(
        html.contains("href=\"/guide\"") || html.contains("Guide"),
        "{html}"
    );
    assert!(html.contains("Nowhere"));
    assert!(p.toc.iter().any(|h| h.text == "Heading"));
}

#[test]
fn a_preview_goes_through_the_sanitizer_and_nothing_of_it_is_kept() {
    let w = world();
    let p = w
        .service
        .preview(
            &w.wiki,
            None,
            "pages/New.md",
            "New",
            "<script>alert(1)</script>\n\n[x](javascript:alert(2))\n\n<img src=x onerror=alert(3)>\n",
            &Env::EN,
        )
        .unwrap();
    let html = p.html.as_str();
    for raw in ["<script", "javascript:", "<img"] {
        assert!(!html.contains(raw), "{raw}: {html}");
    }
    // The page cache and the index are not touched by what one person typed.
    assert!(matches!(
        w.service.page(&w.wiki, None, "New", &Env::EN).unwrap(),
        PageOutcome::Missing { .. }
    ));
}

#[test]
fn text_whose_information_cannot_be_read_is_unreadable_and_a_bad_title_is_refused_first() {
    let w = world();
    let bad = w.service.preview(
        &w.wiki,
        None,
        "pages/New.md",
        "New",
        "---\ntitle: &anchor New\n---\n\nBody.\n",
        &Env::EN,
    );
    assert!(matches!(bad, Err(ReadError::PageUnreadable(_))));
    let (result, calls) = w.counted(|| {
        w.service
            .preview(&w.wiki, None, "pages/x.md", "..", "text", &Env::EN)
    });
    assert!(matches!(result, Err(ReadError::InvalidTitle(_))));
    assert_eq!(
        calls,
        Calls::default(),
        "no prikk call for a title that cannot be one"
    );
}
