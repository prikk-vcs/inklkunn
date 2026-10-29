//! Search, the health lists and the backlinks through the real router (RFC 003 D-8, D-9; handoff 003 T6): titles
//! and aliases found by any part, width variants and diacritics folded; the offer to create a page, disabled,
//! after the pages with similar titles; every health list showing its planted case with the action that resolves
//! it; backlinks with the line that makes the link; an author's markup shown as letters wherever it lands.
//! The repository is an in-memory double, so these run in both test runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use common::{World, busy, get, page};
use inklkunn_store::Visibility;

fn files() -> Vec<(String, String)> {
    let p = |path: &str, front: &[&str], body: &str| (path.to_owned(), page(front, body));
    vec![
        p(
            "pages/Home.md",
            &["title: Home"],
            "Welcome. See [[Guide]], [[Café Society]] and [[Missing Page]].\n\nAlso [[Missing Page]] again.\n",
        ),
        p(
            "pages/Guide.md",
            &["title: Getting Started", "aliases: [Guide, \"ＦＡＱ\"]"],
            "Back to [[Home]].\nA line with [[Ghost]] in it.\n",
        ),
        p(
            "pages/Café Society.md",
            &["title: Café Society"],
            "Links to [[Home]] and to [[Getting Started]].\n",
        ),
        p(
            "pages/日本語のウィキ入門.md",
            &["title: 日本語のウィキ入門"],
            "Nobody links here and it links nowhere.\n",
        ),
        p(
            "pages/Lonely.md",
            &["title: Lonely"],
            "Links nowhere either; a dead end and an orphan.\n",
        ),
        p(
            "pages/Work.md",
            &["title: Work", "status: wip"],
            "See [[Home]].\n",
        ),
        p(
            "pages/Draft.md",
            &["title: Draft", "status: draft"],
            "See [[Home]].\n",
        ),
        p(
            "pages/Stubby.md",
            &["title: Stubby", "tags: [stub]"],
            "See [[Home]].\n",
        ),
        p(
            "pages/Old.md",
            &["title: Old", "kind: redirect", "redirect: Getting Started"],
            "",
        ),
        p(
            "pages/Older.md",
            &["title: Older", "kind: redirect", "redirect: Old"],
            "",
        ),
        p(
            "pages/Dead.md",
            &["title: Dead", "kind: redirect", "redirect: Nowhere"],
            "",
        ),
        p(
            "pages/Hostile.md",
            &[
                "title: \"<img src=x onerror=alert(1)>\"",
                "aliases: [\"<script>alert(1)</script>\"]",
            ],
            "A [[Home]] link; and <script>alert(2)</script> here.\n",
        ),
    ]
}

fn world_with(visibility: Visibility, slug: &str, name: &str) -> World {
    let owned = files();
    let borrowed: Vec<(&str, &str)> = owned
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::with_visibility(slug, name, visibility, &borrowed)
}

fn handbook() -> World {
    world_with(Visibility::Public, "handbook", "The Handbook")
}

fn q(text: &str) -> String {
    format!("/handbook/-/search?q={}", urlencode(text))
}

fn urlencode(text: &str) -> String {
    text.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

// ------------------------------------------------------------------------------------ search

#[tokio::test]
async fn search_folds_width_variants_and_diacritics_and_finds_a_japanese_title_by_a_substring() {
    let w = handbook();
    for (query, title) in [
        // Diacritics: Café is found by cafe, by CAFE, by the full-width spelling, by a part.
        ("cafe", "Café Society"),
        ("CAFE", "Café Society"),
        ("ｃａｆｅ", "Café Society"),
        ("café soc", "Café Society"),
        ("fe soc", "Café Society"),
        // A Japanese title by a substring, in the middle, at the end, by one character, and by half-width kana.
        ("ウィキ", "日本語のウィキ入門"),
        ("本語の", "日本語のウィキ入門"),
        ("入門", "日本語のウィキ入門"),
        ("日", "日本語のウィキ入門"),
        ("ｳｨｷ", "日本語のウィキ入門"),
        // Full-width letters in the stored alias, plain in the query, and the reverse.
        ("faq", "Getting Started"),
        ("ＦＡＱ", "Getting Started"),
    ] {
        let a = get(&w, &q(query)).await;
        assert_eq!(a.status, 200, "{query}");
        let body = a.body();
        assert!(
            body.contains(&format!("{title}</a>")),
            "{query} should find {title}: {body}"
        );
    }
    // A kana with a voicing mark is not the kana without it.
    let a = get(&w, &q("ガ")).await;
    assert!(!a.body().contains("日本語のウィキ入門</a>"), "{}", a.body());
}

#[tokio::test]
async fn a_result_shows_the_title_or_alias_that_matched_with_the_matching_part_marked() {
    let w = handbook();
    let body = get(&w, &q("start")).await.body();
    assert!(
        body.contains("<a href=\"/handbook/Getting%20Started\">Getting Started</a>"),
        "{body}"
    );
    assert!(
        body.contains("Title: Getting <strong>Start</strong>ed"),
        "{body}"
    );
    let body = get(&w, &q("faq")).await.body();
    assert!(body.contains("Alias: <strong>ＦＡＱ</strong>"), "{body}");
}

#[tokio::test]
async fn an_authors_markup_in_a_title_or_an_alias_is_text_in_a_search_snippet() {
    // The sanitizer test of the page body, repeated on a snippet.
    let w = handbook();
    for query in ["alert", "script", "onerror", "img src"] {
        let a = get(&w, &q(query)).await;
        assert_eq!(a.status, 200, "{query}");
        let body = a.body();
        assert!(!body.contains("<script"), "{query}: {body}");
        assert!(!body.contains("<img src=x"), "{query}: {body}");
        assert!(!body.contains("onerror=alert(1)>"), "{query}: {body}");
        // It is there, as letters.
        assert!(body.contains("&#60;"), "{query}: {body}");
    }
    // The matching part is marked; everything else about the text is the sanitizer's, which writes `<` as `&lt;`.
    let body = get(&w, &q("alert")).await.body();
    assert!(
        body.contains("Title: &lt;img src=x onerror=<strong>alert</strong>(1)&gt;"),
        "{body}"
    );
    let body = get(&w, &q("script")).await.body();
    assert!(
        body.contains("Alias: &lt;<strong>script</strong>&gt;alert(1)&lt;/script&gt;"),
        "{body}"
    );
}

#[tokio::test]
async fn when_no_page_has_the_title_similar_titles_come_first_and_creating_is_offered_but_cannot_be_done()
 {
    let w = handbook();
    let body = get(&w, &q("Getting Start")).await.body();
    let similar = body.find("Pages with similar titles").unwrap();
    let create = body.find("Create this page").unwrap();
    assert!(similar < create, "{body}");
    assert!(
        body.find("Getting Started</a>").unwrap() < create,
        "the similar title is listed before the offer: {body}"
    );
    // The offer is a button that cannot be pressed, said in the words of the wiki, with the reason.
    assert!(
        body.contains("<button type=\"button\" disabled>Create “Getting Start”</button>"),
        "{body}"
    );
    assert!(
        body.contains("Proposals arrive with the next release."),
        "{body}"
    );
    // No dead link, no form that leads to nothing.
    let after = body.split("Create this page").nth(1).unwrap();
    assert!(!after.contains("<a "), "{after}");
    assert_eq!(
        body.matches("method=\"get\"").count(),
        1,
        "the search is the one form that asks: {body}"
    );

    // Nothing resembles it: said, and still the offer.
    let body = get(&w, &q("Zzzzzz")).await.body();
    assert!(
        body.contains("No page has Zzzzzz in its title or aliases."),
        "{body}"
    );
    assert!(body.contains("Create “Zzzzzz”"), "{body}");

    // A page by that title: nothing to create.
    let body = get(&w, &q("Getting Started")).await.body();
    assert!(
        body.contains("A page with this title exists already."),
        "{body}"
    );
    assert!(!body.contains("Create this page"), "{body}");
    // An alias is a name for a page as much as its title is.
    let body = get(&w, &q("Guide")).await.body();
    assert!(!body.contains("Create this page"), "{body}");
    assert!(body.contains("Getting Started</a>"), "{body}");
}

#[tokio::test]
async fn the_search_index_is_rebuilt_when_the_tip_moves() {
    let w = handbook();
    assert!(
        get(&w, &q("Zebra"))
            .await
            .body()
            .contains("No page has Zebra")
    );
    // A page is added: a new tip. The search at the old tip is not the search now.
    w.fake.set(
        "pages/Zebra Crossing.md",
        &page(&["title: Zebra Crossing"], "x\n"),
    );
    let body = get(&w, &q("Zebra")).await.body();
    assert!(body.contains("Zebra Crossing</a>"), "{body}");
    assert!(!body.contains("No page has Zebra"), "{body}");
    // And a page removed from view is gone from the results.
    w.fake.set(
        "pages/Zebra Crossing.md",
        &page(&["title: Renamed Page"], "x\n"),
    );
    let body = get(&w, &q("Zebra")).await.body();
    assert!(!body.contains("Zebra Crossing</a>"), "{body}");
}

#[tokio::test]
async fn an_empty_search_says_what_to_type_and_a_search_that_cannot_be_answered_says_so() {
    let w = handbook();
    for uri in [
        "/handbook/-/search",
        "/handbook/-/search?q=",
        "/handbook/-/search?q=%20%20",
    ] {
        let a = get(&w, uri).await;
        assert_eq!(a.status, 200, "{uri}");
        assert!(
            a.body()
                .contains("Type a title, or part of one, to search for."),
            "{uri}"
        );
        assert!(!a.body().contains("Create"), "{uri}");
    }
    // An address that is not text is no search, not an error page of the framework's.
    let a = get(&w, "/handbook/-/search?q=%FF").await;
    assert_eq!(a.status, 200);

    let w = handbook();
    w.fake.fail_views(Some(busy()));
    let a = get(&w, &q("home")).await;
    assert_eq!(a.status, 503);
    assert!(a.header("retry-after").is_some());
}

// ------------------------------------------------------------------------------------ health

fn rows(body: &str) -> Vec<String> {
    body.split("<tbody>")
        .nth(1)
        .and_then(|b| b.split("</tbody>").next())
        .unwrap_or("")
        .split("<tr>")
        .skip(1)
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn every_health_list_shows_its_planted_case_with_what_would_resolve_it() {
    let w = handbook();
    let list = |name: &'static str| {
        let w = &w;
        async move {
            let a = get(w, &format!("/handbook/-/health/{name}")).await;
            assert_eq!(a.status, 200, "{name}: {}", a.body());
            a.body()
        }
    };

    // Wanted: ranked by the pages that link to the title; the title is one though it was written two ways.
    let body = list("wanted").await;
    let wanted = rows(&body);
    assert_eq!(wanted.len(), 2, "{body}");
    assert!(
        wanted.first().unwrap().contains("Ghost")
            && wanted.get(1).unwrap().contains("Missing Page"),
        "{body}"
    );
    assert!(body.contains("Add a page with this title, or change the links that point to it."));
    assert!(
        body.contains(
            "<a href=\"/handbook/-/links/Missing%20Page\">Pages that link to Missing Page</a>"
        ),
        "{body}"
    );
    assert!(body.contains("Proposals arrive with the next release."));
    assert!(body.contains("<caption>Wanted pages</caption>"));

    // Orphans: nothing links to it. Hostile is linked to nobody either, and is shown as letters.
    let body = list("orphans").await;
    assert!(
        body.contains("<a href=\"/handbook/Lonely\">Lonely</a>"),
        "{body}"
    );
    assert!(body.contains("日本語のウィキ入門"), "{body}");
    assert!(!body.contains("<img src=x"), "{body}");
    assert!(body.contains("Link to this page from a related page, so that readers can find it."));
    assert!(!body.contains(">Home</a></th>"), "{body}");

    // Dead ends: links to no other page.
    let body = list("dead-ends").await;
    assert!(
        body.contains("<a href=\"/handbook/Lonely\">Lonely</a>"),
        "{body}"
    );
    assert!(!body.contains(">Home</a></th>"), "{body}");
    assert!(body.contains("Add links from this page to related pages."));

    // Redirects: dead, and a redirect to a redirect.
    let body = list("redirects").await;
    let r = rows(&body);
    assert_eq!(r.len(), 2, "{body}");
    assert!(
        r.first().unwrap().contains("Dead")
            && r.first().unwrap().contains("Nowhere")
            && r.first().unwrap().contains("The target is not a page."),
        "{body}"
    );
    assert!(
        r.get(1).unwrap().contains("Older")
            && r.get(1)
                .unwrap()
                .contains("The target is itself a redirect."),
        "{body}"
    );
    assert!(body.contains("Point it at a page that exists, or remove the redirect."));
    assert!(body.contains("Point this redirect straight at the final page."));

    // Pages that want help: work in progress, a draft, a stub.
    let body = list("wants-help").await;
    let r = rows(&body);
    assert_eq!(r.len(), 3, "{body}");
    assert!(
        r.first().unwrap().contains("Draft") && r.first().unwrap().contains("Draft</td>"),
        "{body}"
    );
    assert!(
        r.get(1).unwrap().contains("Stubby") && r.get(1).unwrap().contains("Stub</td>"),
        "{body}"
    );
    assert!(
        r.get(2).unwrap().contains("Work") && r.get(2).unwrap().contains("Work in progress</td>"),
        "{body}"
    );
    assert!(body.contains("Finish or improve the page, then remove the mark."));

    // Stale: with no change touching any page, none has a last change in the window.
    let body = list("stale").await;
    assert!(
        body.contains("Not changed in the recent changes looked through"),
        "{body}"
    );
    assert!(
        body.contains("The history does not say when a page last changed"),
        "{body}"
    );
    assert!(body.contains("Read the page, and update it if it is out of date."));

    // The index names the six, each with what it shows; an unknown list is not a page.
    let body = get(&w, "/handbook/-/health").await.body();
    for name in [
        "wanted",
        "orphans",
        "dead-ends",
        "redirects",
        "wants-help",
        "stale",
    ] {
        assert!(
            body.contains(&format!("href=\"/handbook/-/health/{name}\"")),
            "{name}: {body}"
        );
    }
    assert_eq!(get(&w, "/handbook/-/health/other").await.status, 404);
    assert_eq!(get(&w, "/handbook/-/health/Wanted").await.status, 404);
}

#[tokio::test]
async fn the_stale_list_puts_the_pages_untouched_in_the_window_first() {
    let w = handbook();
    w.fake.tip_message("Touch the Home page");
    w.fake.touches(&common::block_id(1), &["pages/Home.md"]);
    let body = get(&w, "/handbook/-/health/stale").await.body();
    let r = rows(&body);
    // Home is the one page a change touched: it is last, with the change's time; the rest have none.
    assert!(r.last().unwrap().contains("Home"), "{body}");
    assert!(
        r.last().unwrap().contains("In a change first seen "),
        "{body}"
    );
    assert!(
        r.first()
            .unwrap()
            .contains("Not changed in the recent changes looked through"),
        "{body}"
    );
}

#[tokio::test]
async fn a_list_with_nothing_in_it_says_so_in_words() {
    let w = World::new(
        "tidy",
        "Tidy",
        &[
            ("pages/A.md", &page(&["title: A"], "See [[B]].\n")),
            ("pages/B.md", &page(&["title: B"], "See [[A]].\n")),
        ],
    );
    for (name, says) in [
        ("wanted", "No page is wanted: every link leads to a page."),
        ("orphans", "Every page has a page that links to it."),
        ("dead-ends", "Every page links to another page."),
        ("redirects", "Every redirect leads to a page."),
        ("wants-help", "No page is marked as needing help."),
    ] {
        let a = get(&w, &format!("/tidy/-/health/{name}")).await;
        assert_eq!(a.status, 200, "{name}");
        assert!(a.body().contains(says), "{name}: {}", a.body());
        assert!(!a.body().contains("<table"), "{name}");
    }
}

#[tokio::test]
async fn the_health_lists_and_backlinks_say_so_when_prikk_cannot_be_asked() {
    let w = handbook();
    w.fake.fail_views(Some(busy()));
    for uri in [
        "/handbook/-/health/wanted",
        "/handbook/-/links/Home",
        "/handbook/-/health/stale",
    ] {
        let a = get(&w, uri).await;
        assert_eq!(a.status, 503, "{uri}");
        assert!(a.header("retry-after").is_some(), "{uri}");
    }
}

// ------------------------------------------------------------------------------------ backlinks

#[tokio::test]
async fn backlinks_list_the_pages_that_link_here_with_the_line_that_makes_the_link() {
    let w = handbook();
    let a = get(&w, "/handbook/-/links/Home").await;
    assert_eq!(a.status, 200, "{}", a.body());
    let body = a.body();
    assert!(body.contains("<h1>Pages that link to Home</h1>"), "{body}");
    assert!(
        body.contains("<a href=\"/handbook/Home\">Go to Home</a>"),
        "{body}"
    );
    assert!(
        body.contains("<a href=\"/handbook/Getting%20Started\">Getting Started</a>"),
        "{body}"
    );
    assert!(
        body.contains("Back to [[Home]]."),
        "the line is the context: {body}"
    );
    // A redirect page that points here is a backlink of its kind; via an alias the title resolves too.
    let body = get(&w, "/handbook/-/links/Guide").await.body();
    assert!(
        body.contains("<h1>Pages that link to Getting Started</h1>"),
        "{body}"
    );
    assert!(body.contains("Redirects here"), "{body}");
    assert!(body.contains("<a href=\"/handbook/Old\">Old</a>"), "{body}");

    // A wanted title has backlinks too, and says there is no such page.
    let a = get(&w, "/handbook/-/links/Ghost").await;
    assert_eq!(a.status, 200);
    assert!(
        a.body()
            .contains("No page has this title yet. These pages link to it."),
        "{}",
        a.body()
    );
    assert!(
        a.body().contains("A line with [[Ghost]] in it."),
        "{}",
        a.body()
    );
    assert!(
        !a.body().contains("Go to Ghost"),
        "no link to a page that is not there"
    );

    // A title nobody links to and no page has is the page every missing title gets.
    let a = get(&w, "/handbook/-/links/Absent").await;
    assert_eq!(a.status, 404);
    assert!(
        a.body().contains("This page does not exist yet"),
        "{}",
        a.body()
    );
    // A page nothing links to says so.
    let body = get(&w, "/handbook/-/links/Lonely").await.body();
    assert!(body.contains("No page links here."), "{body}");
}

#[tokio::test]
async fn the_line_of_a_backlink_is_text_even_when_it_is_markup() {
    let w = handbook();
    let body = get(&w, "/handbook/-/links/Home").await.body();
    // The hostile page links to Home on a line with a script element: shown as letters, never as an element.
    assert!(
        body.contains("&#60;script&#62;alert(2)&#60;/script&#62;"),
        "{body}"
    );
    assert!(!body.contains("<script"), "{body}");
    assert!(!body.contains("<img src=x"), "{body}");
}
