#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::cell::RefCell;
use std::collections::BTreeMap;

use crate::scan::{Link, ScanOptions};
use crate::{
    LinkHref, MAX_TRANSCLUDED_BYTES, MAX_TRANSCLUSION_DEPTH, MAX_TRANSCLUSIONS, RenderLinks,
    Rendered, Transclusion, TransclusionRefusal, render_html, render_page,
};

/// A wiki of titled pages. Every title is a page, every path `pages/<title>.md`. It remembers which bodies
/// it was asked for.
struct Pages(BTreeMap<&'static str, &'static str>, RefCell<Vec<String>>);

impl Pages {
    fn new(pages: &[(&'static str, &'static str)]) -> Self {
        Pages(pages.iter().copied().collect(), RefCell::new(Vec::new()))
    }

    /// The titles whose bodies the renderer asked for, in order.
    fn asked(&self) -> Vec<String> {
        self.1.borrow().clone()
    }

    fn render(&self, title: &str) -> Rendered {
        let body = self.0.get(title).expect("the page exists");
        render_page(
            Some((&format!("pages/{title}.md"), title)),
            body,
            self,
            ScanOptions::default(),
        )
    }
}

impl RenderLinks for Pages {
    fn link(&self, link: &Link) -> LinkHref {
        if self.0.contains_key(link.target.as_str()) {
            LinkHref::Page {
                href: format!("/w/{}", link.target),
                title: Some(link.target.clone()),
            }
        } else {
            LinkHref::Missing { href: None }
        }
    }

    fn transclusion(&self, link: &Link) -> Option<Transclusion> {
        self.0.get(link.target.as_str())?;
        Some(Transclusion {
            path: format!("pages/{}.md", link.target),
            title: link.target.clone(),
            href: format!("/w/{}", link.target),
            label: format!("From {}", link.target),
        })
    }

    fn transclusion_body(&self, page: &Transclusion) -> Option<String> {
        self.1.borrow_mut().push(page.title.clone());
        self.0.get(page.title.as_str()).map(|b| (*b).to_owned())
    }

    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String {
        match refusal {
            TransclusionRefusal::Cycle { chain } => {
                format!(
                    "Not included, it would include itself: {}",
                    chain.join(" → ")
                )
            }
            TransclusionRefusal::TooDeep { title, limit } => {
                format!("Not included, deeper than {limit}: {title}")
            }
            TransclusionRefusal::OverBudget { title } => {
                format!("Not included, a page includes only so much: {title}")
            }
        }
    }
}

#[test]
fn a_page_alone_on_its_line_is_included_with_its_provenance() {
    let pages = Pages::new(&[
        ("Host", "Before.\n\n![[Glossary]]\n\nAfter.\n"),
        ("Glossary", "A *term*.\n"),
    ]);
    let r = pages.render("Host");
    let h = &r.unsanitized_html;
    assert!(
        h.contains(
            "<div class=\"transclusion\" data-transclusion=\"Glossary\" data-transclusion-depth=\"1\">\n\
             <p class=\"transclusion-source\"><a class=\"wikilink\" href=\"/w/Glossary\">From Glossary</a></p>\n\
             <p>A <em>term</em>.</p>\n</div>"
        ),
        "{h}"
    );
    // The text around it is still there, in order, and no paragraph holds the included block.
    let before = h.find("<p>Before.</p>").expect("before");
    let block = h.find("class=\"transclusion\"").expect("block");
    let after = h.find("<p>After.</p>").expect("after");
    assert!(before < block && block < after, "{h}");
    assert!(!h.contains("<p><div"), "{h}");
    assert_eq!(r.transcluded, ["pages/Glossary.md"]);
    assert!(r.refusals.is_empty());
}

#[test]
fn an_embed_anywhere_but_alone_on_its_line_stays_a_link() {
    let pages = Pages::new(&[
        (
            "Host",
            "In a ![[Glossary]] sentence.\n\n- ![[Glossary]]\n\n> ![[Glossary]]\n\n# ![[Glossary]]\n\n![[Glossary]]\nand a continuation.\n\n![[Glossary#Heading]]\n\n![[Glossary|label]] ![[Glossary]]\n",
        ),
        ("Glossary", "Text.\n"),
    ]);
    let r = pages.render("Host");
    let h = &r.unsanitized_html;
    assert!(!h.contains("class=\"transclusion\""), "{h}");
    assert!(r.transcluded.is_empty());
    // Each stays the link an embed of a page is: the same anchor, with the embed class.
    assert!(
        h.matches("<a class=\"wikilink wikilink-embed\" href=\"/w/Glossary\"")
            .count()
            >= 6,
        "{h}"
    );
    // No token is left in the output.
    assert!(!h.contains('\u{E000}') && !h.contains('\u{E001}'), "{h}");
}

/// Pages `P0`, `P1`, … whose bodies the caller makes on demand: `body_for(title)`.
struct Generated<F: Fn(&str) -> String> {
    body_for: F,
    asked: RefCell<usize>,
}

impl<F: Fn(&str) -> String> RenderLinks for Generated<F> {
    fn link(&self, _: &Link) -> LinkHref {
        LinkHref::Missing { href: None }
    }

    fn transclusion(&self, link: &Link) -> Option<Transclusion> {
        link.target.starts_with('P').then(|| Transclusion {
            path: format!("pages/{}.md", link.target),
            title: link.target.clone(),
            href: format!("/w/{}", link.target),
            label: String::new(),
        })
    }

    fn transclusion_body(&self, page: &Transclusion) -> Option<String> {
        *self.asked.borrow_mut() += 1;
        Some((self.body_for)(&page.title))
    }
}

fn embeds(n: usize) -> String {
    (0..n).map(|i| format!("![[P{i}]]\n\n")).collect()
}

#[test]
fn a_page_cannot_include_more_pages_than_the_budget_and_the_overflow_is_noted_once() {
    let pages = Generated {
        body_for: |_: &str| "Included.\n".to_owned(),
        asked: RefCell::new(0),
    };
    let r = render_html(&embeds(MAX_TRANSCLUSIONS + 8), &pages);
    let h = &r.unsanitized_html;
    assert_eq!(
        h.matches("class=\"transclusion\"").count(),
        MAX_TRANSCLUSIONS,
        "{h}"
    );
    assert_eq!(r.transcluded.len(), MAX_TRANSCLUSIONS);
    // The first page over the budget is a note and a refusal; the rest are the links they would have been.
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::OverBudget {
            title: format!("P{MAX_TRANSCLUSIONS}")
        }]
    );
    assert_eq!(
        h.matches("data-transclusion-refusal=\"over-budget\"")
            .count(),
        1,
        "{h}"
    );
    assert_eq!(h.matches("wikilink-missing").count(), 7, "{h}");
    assert_eq!(
        *pages.asked.borrow(),
        MAX_TRANSCLUSIONS,
        "pages over the budget are not read: a read is what the budget spares"
    );
}

#[test]
fn nor_more_source_than_the_budget_however_it_is_split() {
    let one_mib = 1 << 20;
    let pages = Generated {
        body_for: move |_: &str| "x".repeat(one_mib),
        asked: RefCell::new(0),
    };
    let r = render_html(&embeds(6), &pages);
    let h = &r.unsanitized_html;
    assert_eq!(MAX_TRANSCLUDED_BYTES, 4 * one_mib);
    assert_eq!(
        h.matches("class=\"transclusion\"").count(),
        4,
        "four fit, the fifth does not"
    );
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::OverBudget { title: "P4".into() }]
    );
    assert_eq!(
        *pages.asked.borrow(),
        5,
        "the fifth was read to learn its size, the sixth was not"
    );
    assert_eq!(
        h.matches("wikilink-missing").count(),
        1,
        "the sixth stays a link: {h}"
    );
}

#[test]
fn inclusions_that_include_are_counted_together_so_fan_out_cannot_multiply() {
    // Every page includes thirty others: unbounded, depth 2 would render 30 × 30 pages.
    let pages = Generated {
        body_for: |_: &str| embeds(30),
        asked: RefCell::new(0),
    };
    let r = render_html(&embeds(30), &pages);
    let h = &r.unsanitized_html;
    let included = h.matches("class=\"transclusion\"").count();
    assert_eq!(
        included, MAX_TRANSCLUSIONS,
        "all depths together: {included}"
    );
    assert!(*pages.asked.borrow() <= MAX_TRANSCLUSIONS);
    assert!(
        r.refusals
            .iter()
            .any(|f| matches!(f, TransclusionRefusal::OverBudget { .. }))
    );
}

#[test]
fn an_embed_whose_page_cannot_be_read_as_a_page_stays_a_link() {
    struct Unreadable;
    impl RenderLinks for Unreadable {
        fn link(&self, link: &Link) -> LinkHref {
            LinkHref::Page {
                href: format!("/w/{}", link.target),
                title: None,
            }
        }
        fn transclusion(&self, link: &Link) -> Option<Transclusion> {
            Some(Transclusion {
                path: "pages/X.md".into(),
                title: link.target.clone(),
                href: "/w/X".into(),
                label: String::new(),
            })
        }
        // `transclusion_body` is left to its default: the page cannot be read.
    }
    let h = render_html("Before.\n\n![[X]]\n\nAfter.\n", &Unreadable).unsanitized_html;
    assert!(!h.contains("class=\"transclusion\""), "{h}");
    assert!(
        h.contains(
            "<p><a class=\"wikilink wikilink-embed\" href=\"/w/X\" data-wiki-target=\"X\">X</a></p>"
        ),
        "the paragraph holds the link: {h}"
    );
    assert!(
        h.contains("<p>Before.</p>") && h.contains("<p>After.</p>"),
        "{h}"
    );
}

#[test]
fn an_embed_the_caller_does_not_call_a_page_is_not_a_transclusion() {
    let pages = Pages::new(&[("Host", "![[Nowhere]]\n\n![[photo.png]]\n")]);
    let r = pages.render("Host");
    assert!(!r.unsanitized_html.contains("transclusion"), "{r:?}");
    assert!(r.transcluded.is_empty());
}

#[test]
fn inclusion_goes_two_deep_and_the_third_level_is_refused() {
    let pages = Pages::new(&[
        ("A", "a\n\n![[B]]\n"),
        ("B", "b\n\n![[C]]\n"),
        ("C", "c\n\n![[D]]\n"),
        ("D", "d\n"),
    ]);
    let r = pages.render("A");
    assert_eq!(
        pages.asked(),
        ["B", "C"],
        "D is refused before its body is asked for"
    );
    let h = &r.unsanitized_html;
    assert!(
        h.contains("data-transclusion=\"B\" data-transclusion-depth=\"1\""),
        "{h}"
    );
    assert!(
        h.contains("data-transclusion=\"C\" data-transclusion-depth=\"2\""),
        "{h}"
    );
    assert!(
        !h.contains("data-transclusion=\"D\""),
        "D is not included: {h}"
    );
    assert!(!h.contains("<p>d</p>"), "{h}");
    assert!(
        h.contains(
            "<p class=\"transclusion-refused\" data-transclusion-refusal=\"too-deep\" role=\"note\">\
             Not included, deeper than 2: D</p>"
        ),
        "{h}"
    );
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::TooDeep {
            title: "D".into(),
            limit: MAX_TRANSCLUSION_DEPTH
        }]
    );
    assert_eq!(r.transcluded, ["pages/B.md", "pages/C.md"]);
    assert_eq!(MAX_TRANSCLUSION_DEPTH, 2);
}

#[test]
fn a_cycle_is_refused_in_place_with_the_cycle_named_and_the_rest_of_the_page_renders() {
    let pages = Pages::new(&[
        ("A", "Start of A.\n\n![[B]]\n\nEnd of A.\n"),
        ("B", "Start of B.\n\n![[A]]\n\nEnd of B.\n"),
    ]);
    let r = pages.render("A");
    assert_eq!(
        pages.asked(),
        ["B"],
        "A is refused as a cycle before its body is asked for"
    );
    let h = &r.unsanitized_html;
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::Cycle {
            chain: vec!["A".into(), "B".into(), "A".into()]
        }]
    );
    assert!(
        h.contains(
            "<p class=\"transclusion-refused\" data-transclusion-refusal=\"cycle\" role=\"note\">\
             Not included, it would include itself: A → B → A</p>"
        ),
        "{h}"
    );
    // B is included, A is not included again, and every other line of both pages is there.
    assert!(h.contains("data-transclusion=\"B\""), "{h}");
    assert!(!h.contains("data-transclusion=\"A\""), "{h}");
    for line in ["Start of A.", "End of A.", "Start of B.", "End of B."] {
        assert!(h.contains(line), "{line}: {h}");
    }
    // Seen from B, the cycle is the same one, started from B.
    let r = pages.render("B");
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::Cycle {
            chain: vec!["B".into(), "A".into(), "B".into()]
        }]
    );
}

#[test]
fn a_page_that_includes_itself_is_a_cycle_of_one() {
    let pages = Pages::new(&[("Loop", "Text.\n\n![[Loop]]\n")]);
    let r = pages.render("Loop");
    assert!(
        pages.asked().is_empty(),
        "the page being rendered is not read again"
    );
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::Cycle {
            chain: vec!["Loop".into(), "Loop".into()]
        }]
    );
    assert!(r.unsanitized_html.contains("Loop → Loop"), "{r:?}");
    assert!(r.transcluded.is_empty());
}

#[test]
fn a_cycle_is_named_before_the_depth_when_both_apply() {
    // A → B → C → A: at C the next level would be a third, and it is also a return to A.
    let pages = Pages::new(&[("A", "![[B]]\n"), ("B", "![[C]]\n"), ("C", "![[A]]\n")]);
    let r = pages.render("A");
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::Cycle {
            chain: vec!["A".into(), "B".into(), "C".into(), "A".into()]
        }]
    );
}

#[test]
fn a_render_without_an_identity_still_ends_when_pages_include_each_other() {
    let pages = Pages::new(&[("A", "![[B]]\n"), ("B", "![[A]]\n")]);
    let body = pages.0.get("A").expect("A");
    let r = render_html(body, &pages);
    // The anonymous page is not known to be A, so B includes A (depth 2), and A's embed of B is the cycle
    // the render can see: B → A → B.
    assert_eq!(
        r.refusals,
        [TransclusionRefusal::Cycle {
            chain: vec!["B".into(), "A".into(), "B".into()]
        }]
    );
}

#[test]
fn the_same_page_included_twice_is_listed_once_and_its_ids_do_not_meet() {
    let pages = Pages::new(&[
        ("Host", "# Host\n\n![[Part]]\n\n![[Part]]\n"),
        (
            "Part",
            "# Part title\n\nText ^note\n\nSee [[#Part title]].\n",
        ),
    ]);
    let r = pages.render("Host");
    let h = &r.unsanitized_html;
    assert_eq!(r.transcluded, ["pages/Part.md"], "once");
    // The host's own id is as it always was; each inclusion has a prefix of its own.
    assert!(h.contains("<h1 id=\"host\">Host</h1>"), "{h}");
    assert!(
        h.contains("<h2 id=\"t1-part-title\">Part title</h2>"),
        "{h}"
    );
    assert!(
        h.contains("<h2 id=\"t2-part-title\">Part title</h2>"),
        "{h}"
    );
    assert!(h.contains("<p id=\"t1-^note\">Text</p>"), "{h}");
    assert!(h.contains("<p id=\"t2-^note\">Text</p>"), "{h}");
    // A same-page link of an included page follows that page's ids.
    assert!(h.contains("href=\"#t1-part-title\""), "{h}");
    assert!(h.contains("href=\"#t2-part-title\""), "{h}");
    // The host's table of contents lists the host's headings only.
    assert_eq!(
        r.toc.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
        ["host"]
    );
    // No id appears twice in the whole page.
    let mut ids: Vec<&str> = h
        .match_indices(" id=\"")
        .filter_map(|(i, m)| h.get(i + m.len()..))
        .filter_map(|rest| rest.split('"').next())
        .collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total, "{h}");
}

#[test]
fn an_included_page_never_adds_a_heading_level_above_the_one_it_sits_under() {
    let pages = Pages::new(&[
        ("A", "# A\n\n![[B]]\n"),
        ("B", "# B1\n\n## B2\n\n###### B6\n\n![[C]]\n"),
        ("C", "# C1\n\n#### C4\n"),
    ]);
    let r = pages.render("A");
    let h = &r.unsanitized_html;
    assert!(h.contains("<h1 id=\"a\">A</h1>"), "{h}");
    assert!(h.contains("<h2 id=\"t1-b1\">B1</h2>"), "{h}");
    assert!(h.contains("<h3 id=\"t1-b2\">B2</h3>"), "{h}");
    assert!(h.contains("<h6 id=\"t1-b6\">B6</h6>"), "stops at 6: {h}");
    assert!(
        h.contains("<h3 id=\"t2-c1\">C1</h3>"),
        "two levels down: {h}"
    );
    assert!(h.contains("<h6 id=\"t2-c4\">C4</h6>"), "{h}");
    assert_eq!(h.matches("<h1").count(), 1, "{h}");
    assert_eq!(h.matches("</h1>").count(), 1, "{h}");
}

#[test]
fn a_page_cannot_write_a_token() {
    let pages = Pages::new(&[
        (
            "Host",
            "![[Part]]\n\n\u{E000}0\u{E001}\n\ninline \u{E000}0\u{E001} here\n",
        ),
        ("Part", "Included.\n"),
    ]);
    let r = pages.render("Host");
    let h = &r.unsanitized_html;
    assert_eq!(h.matches("class=\"transclusion\"").count(), 1, "{h}");
    assert!(!h.contains('\u{E000}') && !h.contains('\u{E001}'), "{h}");
    assert!(h.contains("\u{FFFD}0\u{FFFD}"), "{h}");
    assert_eq!(h.matches("Included.").count(), 1, "{h}");
}

#[test]
fn what_the_caller_supplies_is_escaped_and_a_blocked_href_is_not_a_link() {
    struct Hostile;
    impl RenderLinks for Hostile {
        fn link(&self, _: &Link) -> LinkHref {
            LinkHref::Missing { href: None }
        }
        fn transclusion(&self, link: &Link) -> Option<Transclusion> {
            Some(Transclusion {
                path: "pages/x.md".into(),
                title: "<script>alert(1)</script>".into(),
                href: if link.target == "Safe" {
                    "/w/x".into()
                } else {
                    "javascript:alert(1)".into()
                },
                label: "\"><img src=x onerror=alert(2)>".into(),
            })
        }
        fn transclusion_body(&self, _: &Transclusion) -> Option<String> {
            Some("Body.\n".into())
        }
        fn transclusion_notice(&self, _: &TransclusionRefusal) -> String {
            "<b>notice</b>".into()
        }
    }
    let h = render_html("![[Safe]]\n\n![[Unsafe]]\n", &Hostile).unsanitized_html;
    assert!(!h.contains("<script"), "{h}");
    assert!(!h.contains("<img"), "{h}");
    assert!(!h.contains("javascript:"), "{h}");
    assert!(h.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "{h}");
    assert!(h.contains("href=\"/w/x\""), "{h}");
    // The second inclusion of the same path is a cycle only if it is inside itself; two siblings are not.
    assert_eq!(h.matches("class=\"transclusion\"").count(), 2, "{h}");

    // The notice is escaped too.
    let pages = Pages::new(&[("Loop", "![[Loop]]\n")]);
    struct Notice<'a>(&'a Pages);
    impl RenderLinks for Notice<'_> {
        fn link(&self, l: &Link) -> LinkHref {
            self.0.link(l)
        }
        fn transclusion(&self, l: &Link) -> Option<Transclusion> {
            self.0.transclusion(l)
        }
        fn transclusion_body(&self, p: &Transclusion) -> Option<String> {
            self.0.transclusion_body(p)
        }
        fn transclusion_notice(&self, _: &TransclusionRefusal) -> String {
            "<b>notice</b> & more".into()
        }
    }
    let body = pages.0.get("Loop").expect("Loop");
    let h = render_page(
        Some(("pages/Loop.md", "Loop")),
        body,
        &Notice(&pages),
        ScanOptions::default(),
    )
    .unsanitized_html;
    assert!(h.contains("&lt;b&gt;notice&lt;/b&gt; &amp; more"), "{h}");
}

#[test]
fn an_empty_label_falls_back_to_the_title_and_the_default_notice_names_the_pages() {
    struct Bare;
    impl RenderLinks for Bare {
        fn link(&self, _: &Link) -> LinkHref {
            LinkHref::Missing { href: None }
        }
        fn transclusion(&self, _: &Link) -> Option<Transclusion> {
            Some(Transclusion {
                path: "pages/Bare.md".into(),
                title: "Bare".into(),
                href: "/w/Bare".into(),
                label: String::new(),
            })
        }
        fn transclusion_body(&self, _: &Transclusion) -> Option<String> {
            Some("![[Bare]]\n".into())
        }
    }
    let r = render_page(
        Some(("pages/Root.md", "Root")),
        "![[Bare]]\n",
        &Bare,
        ScanOptions::default(),
    );
    let h = &r.unsanitized_html;
    assert!(
        h.contains("<a class=\"wikilink\" href=\"/w/Bare\">Bare</a>"),
        "{h}"
    );
    // The default notice has no words of its own: the chain and nothing else.
    assert!(
        h.contains("role=\"note\">Bare → Bare</p>"),
        "no sentence is written by a crate that knows no language: {h}"
    );
}

#[test]
fn inclusion_works_inside_a_container_and_the_render_is_deterministic() {
    let pages = Pages::new(&[
        ("Host", ":::note Heads up\n![[Part]]\n:::\n\nAfter.\n"),
        ("Part", "Included ^inc\n"),
    ]);
    let first = pages.render("Host");
    let again = pages.render("Host");
    assert_eq!(first, again, "rendering the same inputs twice is identical");
    let h = &first.unsanitized_html;
    assert!(h.contains("class=\"transclusion\""), "{h}");
    assert!(h.contains("<p id=\"t1-^inc\">Included</p>"), "{h}");
    assert!(h.contains("<p>After.</p>"), "{h}");
}
