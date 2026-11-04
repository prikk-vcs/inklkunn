//! *Why is this here?* (RFC 005 part 2 D-8; handoff 005 K-T7), through the real router over an in-memory
//! repository: the page's paragraphs, each with an anchor; the chain of a paragraph that holds a claim and of an
//! ordinary one, labelled by how it was followed; the changes they name, with the people as the message records
//! them, the proposal, its review, the acceptance records and the sources given; the bounds of the history
//! page; existence neutrality; both languages. No prikk is needed, so these run in both test runs. Every
//! sentence is invented.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;
mod proposals;

use axum::http::StatusCode;
use common::{
    APP, Answer, HOME_ID, World, block_id, claim, claim_id, get, home_with, op, page, put, request,
    shown,
};
use inklkunn_core::knowledge::Standing;

const JA: &[(&str, &str)] = &[("cookie", "inklkunn_lang=ja")];

fn main_of(a: &Answer) -> String {
    let body = a.body();
    let start = body.find("<main").expect("a main landmark");
    let end = body.find("</main>").expect("the end of main");
    body.get(start..end).expect("main").to_owned()
}

/// A wiki whose `Home` has one version per body (the first creates it, each later one edits it) and a `Guide`
/// nobody edits. The messages are plain; the blocks say what they did.
fn story(bodies: &[&str]) -> World {
    let home = |body: &str| page(&["title: Home", &format!("id: {HOME_ID}")], body);
    let (first, rest) = bodies.split_first().expect("a version");
    let w = World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Home.md", &home(first)),
            (
                "pages/Guide.md",
                &page(&["title: Guide"], "The guide, with no claim.\n"),
            ),
        ],
    );
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    for (i, body) in rest.iter().enumerate() {
        let version = i as u64 + 2;
        w.fake.set_files(&[("pages/Home.md", &home(body))]);
        w.fake.say(version, &format!("Edit {version}"));
        w.fake
            .show(version, shown(vec![op("edit-text", &["pages/Home.md"])]));
    }
    w
}

const V1: &str = "The bridge opened in 1950. ^b1\n\nIt crosses the river.\n";
const V2: &str = "The bridge was opened in 1950. ^b1\n\nIt crosses the river.\n";
const V3: &str = "The bridge was opened in 1950. ^b1\n\nIt crosses the wide river.\n";

// ------------------------------------------------------------------------------------ the page

#[tokio::test]
async fn the_page_lists_each_paragraph_with_an_anchor_and_says_when_it_was_written_and_last_changed()
 {
    let w = story(&[V1, V2, V3]);
    let a = get(&w, "/handbook/-/why/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    let body = main_of(&a);
    assert!(body.contains("<h1>Why is this here?</h1>"), "{body}");
    assert!(
        body.contains(
            "Where each paragraph of Home came from, as far as the page&#39;s history says."
        ),
        "{body}"
    );
    // Each paragraph has an anchor: the paragraph with an `^id` is named by it, any other by its place.
    assert!(body.contains("<li id=\"w-^b1\">"), "{body}");
    assert!(body.contains("<li id=\"w-2\">"), "{body}");
    assert!(
        body.contains("<a href=\"#w-^b1\">Paragraph 1</a>"),
        "{body}"
    );
    assert!(body.contains("<a href=\"#w-2\">Paragraph 2</a>"), "{body}");
    assert!(
        body.contains("<p class=\"why-excerpt\">The bridge was opened in 1950.</p>"),
        "{body}"
    );
    // The paragraph with the anchor: exact. Written in change 1, last changed in change 2.
    let first = body.get(body.find("<li id=\"w-^b1\">").unwrap()..).unwrap();
    let first = first
        .get(..first.find("</li>\n<li id=\"w-2\"").unwrap())
        .unwrap();
    assert!(
        first.contains("Last changed in <a href=\"/handbook/-/why/Home#c-2\">Change 2</a>."),
        "{first}"
    );
    assert!(
        first.contains("First written in <a href=\"/handbook/-/why/Home#c-1\">Change 1</a>."),
        "{first}"
    );
    assert!(
        first.contains("Followed through its anchor: in every change read, the one paragraph with this anchor is this paragraph."),
        "{first}"
    );
    assert!(!first.contains("Traced by matching the text"), "{first}");
    // The ordinary paragraph: found by its words, and it says so; what it says is what was found.
    let second = body.get(body.find("<li id=\"w-2\">").unwrap()..).unwrap();
    assert!(
        second
            .contains("Latest change found in <a href=\"/handbook/-/why/Home#c-3\">Change 3</a>."),
        "{second}"
    );
    assert!(
        second.contains(
            "Earliest wording found in <a href=\"/handbook/-/why/Home#c-1\">Change 1</a>. No earlier wording matches it."
        ),
        "{second}"
    );
    assert!(second.contains("Traced by matching the text."), "{second}");
    assert!(!second.contains("Followed through its anchor"), "{second}");
    assert!(!second.contains("First written in"), "{second}");
    // The page links back, to the page's history and to the page.
    assert!(
        body.contains("<a href=\"/handbook/Home\">Back to Home</a>"),
        "{body}"
    );
    assert!(
        body.contains("<a href=\"/handbook/-/history/Home\">The whole history of Home</a>"),
        "{body}"
    );
    // A page read to its start says nothing about bounds.
    assert!(!body.contains("Only the latest"), "{body}");
}

#[tokio::test]
async fn a_paragraph_that_holds_a_claim_links_to_the_claim_and_is_followed_through_its_anchor() {
    let w = story(&[V1]);
    // The claim is said in the first paragraph (`^b1`), added in one change that touches no page.
    let said = "The bridge opened in 1950.";
    put(
        &w,
        &page(
            &["title: Home", &format!("id: {HOME_ID}")],
            &format!("{said} ^b1\n\nIt crosses the river.\n"),
        ),
        &[claim(1, Standing::Contested, None, said)],
        &[],
    );
    w.fake.say(2, "Add a claim");
    w.fake.show(
        2,
        shown(vec![op("create-file", &["knowledge/claims/c.yaml"])]),
    );
    let a = get(&w, "/handbook/-/why/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    let body = main_of(&a);
    let href = format!("/handbook/-/claim/{}", claim_id(1));
    assert!(
        body.contains(&format!(
            "<a class=\"claim-link\" href=\"{href}\" aria-label=\"Sources and views: paragraph beginning “The bridge opened in 1950.”\">Sources and views</a>"
        )),
        "{body}"
    );
    // The link is on the claim's paragraph only.
    assert_eq!(body.matches("claim-link").count(), 1, "{body}");
    assert!(
        body.contains(
            "Written in <a href=\"/handbook/-/why/Home#c-1\">Change 1</a>. Not changed since."
        ),
        "{body}"
    );
}

#[tokio::test]
async fn a_page_without_paragraphs_and_a_page_with_one_changed_at_once_say_what_they_are() {
    let w = story(&["Alpha one.\n\nBeta two.\n", "Gamma three.\n\nDelta four.\n"]);
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    // Two paragraphs reworded together have no earlier wording to match, and the page says what was found.
    assert!(
        body.contains(
            "Earliest wording found in <a href=\"/handbook/-/why/Home#c-2\">Change 2</a>."
        ),
        "{body}"
    );
    assert!(!body.contains("First written in"), "{body}");
    let empty = story(&[""]);
    let body = main_of(&get(&empty, "/handbook/-/why/Home").await);
    assert!(body.contains("This page has no paragraphs."), "{body}");
}

#[tokio::test]
async fn a_view_keeps_every_link_in_the_view() {
    let w = story(&[V1, V2]);
    w.fake.add_view("draft");
    let a = get(&w, "/handbook/-/why/Home?view=draft").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let body = main_of(&a);
    assert!(body.contains("You are reading the view draft."), "{body}");
    assert!(
        body.contains("<a href=\"/handbook/~draft/Home\">Back to Home</a>"),
        "{body}"
    );
    assert!(
        body.contains("href=\"/handbook/-/history/Home?view=draft\""),
        "{body}"
    );
    assert!(
        body.contains(&format!(
            "href=\"/handbook/-/change/{}?view=draft\"",
            block_id(2)
        )),
        "{body}"
    );
    // A view the address cannot spell, and one that is not there, are the page for a view that is not there.
    assert_eq!(
        get(&w, "/handbook/-/why/Home?view=..%2Fx").await.status,
        StatusCode::NOT_FOUND
    );
    let none = get(&w, "/handbook/-/why/Home?view=nope").await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    assert!(none.body().contains("nope"), "{}", none.body());
}

#[tokio::test]
async fn a_title_no_page_has_is_the_page_every_missing_title_gets() {
    let w = story(&[V1]);
    let why = get(&w, "/handbook/-/why/Nothing%20here").await;
    let page = get(&w, "/handbook/Nothing%20here").await;
    assert_eq!(why.status, StatusCode::NOT_FOUND);
    assert_eq!(why.status, page.status);
    assert!(
        main_of(&why).contains("No page in The Handbook has the title Nothing here."),
        "{}",
        main_of(&why)
    );
    // A title that cannot name a page is the page for that.
    assert_eq!(
        get(&w, "/handbook/-/why/a%2F%2Fb").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn a_wiki_that_may_not_be_read_answers_as_a_wiki_that_is_not_there() {
    let w = World::with_visibility(
        "secret",
        "Secret",
        inklkunn_store::Visibility::Restricted,
        &[("pages/Home.md", &home_with("Hidden."))],
    );
    for path in [
        "/{}/-/why/Home",
        "/{}/-/why/Nothing",
        "/{}/-/why/Home?view=main",
    ] {
        let real = get(&w, &path.replace("{}", "secret")).await;
        let absent = get(&w, &path.replace("{}", "no-such-wiki")).await;
        assert_eq!(real.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(real.status, absent.status, "{path}");
        assert_eq!(
            real.body().replace("secret", "X"),
            absent.body().replace("no-such-wiki", "X"),
            "{path}"
        );
        assert!(!real.body().contains("Hidden"), "{path}");
    }
}

#[tokio::test]
async fn the_page_is_in_both_languages() {
    let w = story(&[V1, V2, V3]);
    let ja = request(&w, APP, "GET", "/handbook/-/why/Home", JA).await;
    assert_eq!(ja.status, StatusCode::OK);
    let body = main_of(&ja);
    assert!(body.contains("<h1>なぜここにあるのか</h1>"), "{body}");
    assert!(body.contains("段落 1"), "{body}");
    assert!(body.contains("アンカーでたどりました。"), "{body}");
    assert!(body.contains("文面の一致でたどりました。"), "{body}");
    assert!(body.contains("読んだ変更"), "{body}");
    // No English sentence of the page is left among them.
    assert!(!body.contains("Paragraph 1"), "{body}");
    assert!(!body.contains("Followed through"), "{body}");
}

#[tokio::test]
async fn what_a_paragraph_or_a_message_says_is_escaped_and_never_markup() {
    let w = story(&[
        "Beware <script>alert(1)</script> & more. ^b1\n\nPlain.\n",
        "Beware <script>alert(2)</script> & more. ^b1\n\nPlain.\n",
    ]);
    w.fake.say(2, "<img src=x onerror=alert(3)> a summary");
    let a = get(&w, "/handbook/-/why/Home").await;
    let body = a.body();
    assert!(!body.contains("<script>alert"), "{body}");
    assert!(!body.contains("<img src=x"), "{body}");
    assert!(
        body.contains("Beware &#60;script&#62;alert(2)&#60;/script&#62; &#38; more."),
        "{body}"
    );
    assert!(!body.contains("<script"), "{body}");
}

// ------------------------------------------------------------------------------------ the bounds

#[tokio::test]
async fn a_long_history_is_read_within_the_history_pages_bounds_and_the_page_says_where_it_stopped()
{
    let bodies: Vec<String> = (1..=70)
        .map(|n| format!("First paragraph, edit {n}.\n\nSecond stays as it was.\n"))
        .collect();
    let refs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let w = story(&refs);
    let a = get(&w, "/handbook/-/why/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    let body = main_of(&a);
    assert!(
        body.contains("Only the latest 20 changes to this page were read here. There are older changes, and what came before them is not shown."),
        "{body}"
    );
    // The paragraph nothing in those changes touched is not claimed to be older or younger than they are.
    assert!(
        body.contains("Not changed in the changes read here."),
        "{body}"
    );
    assert!(
        body.contains("First written before the oldest change read here."),
        "{body}"
    );
    // At most the changes the history page lists are shown, and prikk was asked for no more than its bound.
    assert!(body.matches("<li id=\"c-").count() <= 20, "{body}");
    assert!(w.fake.shows() <= 24, "{} shows", w.fake.shows());
}

#[tokio::test]
async fn a_history_the_bounds_end_before_any_change_to_the_page_says_so() {
    let w = story(&["Alpha.\n"]);
    for version in 2..=62_u64 {
        w.fake.set_files(&[(
            "pages/Guide.md",
            &format!("---\ntitle: Guide\n---\n\nv{version}.\n"),
        )]);
        w.fake.say(version, "Edit the guide");
        w.fake
            .show(version, shown(vec![op("edit-text", &["pages/Guide.md"])]));
    }
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    assert!(
        body.contains("No change to this page was found among the latest changes of this wiki that one request can read."),
        "{body}"
    );
    assert!(body.contains("No change to this page was read."), "{body}");
}

#[tokio::test]
async fn a_history_that_cannot_be_followed_says_so_and_claims_nothing_of_the_start() {
    let w = story(&["Alpha.\n", "Alpha, reworded.\n"]);
    // The first block says it made only the guide: no change that wrote the page is listed.
    w.fake
        .show(1, shown(vec![op("create-file", &["pages/Guide.md"])]));
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    assert!(
        body.contains("The history of this page could not be followed to its start"),
        "{body}"
    );
    assert!(!body.contains("First written in"), "{body}");
}

#[tokio::test]
async fn a_page_of_more_paragraphs_than_are_followed_counts_the_rest() {
    let body: String = (0..450).map(|n| format!("Paragraph {n}.\n\n")).collect();
    let w = story(&[&body]);
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    assert!(
        body.contains(
            "50 more paragraphs are not followed: only the first 400 paragraphs of a page are."
        ),
        "{body}"
    );
    assert_eq!(body.matches("<p class=\"why-excerpt\">").count(), 400);
}

#[tokio::test]
async fn nothing_on_the_page_ranks_or_says_that_a_paragraph_is_true() {
    let w = story(&[V1, V2, V3]);
    for lang in [&[][..], JA] {
        let a = request(&w, APP, "GET", "/handbook/-/why/Home", lang).await;
        let main = main_of(&a);
        for banned in [
            "true",
            "false",
            "verified",
            "proven",
            "canonical",
            "official",
            "adopted",
            "score",
            "rank",
            "reliable",
            "trusted",
        ] {
            let lower = main.to_lowercase();
            assert!(
                !lower
                    .split(|c: char| !c.is_alphanumeric())
                    .any(|w| w == banned),
                "{banned}: {main}"
            );
        }
        assert!(!main.contains("style="), "{main}");
        assert!(!main.contains("<img"), "{main}");
        assert!(!main.contains("<script"), "{main}");
        assert!(
            !main.contains('%') && !main.contains('★') && !main.contains('✓'),
            "{main}"
        );
    }
}

#[tokio::test]
async fn the_name_of_a_claim_link_is_the_beginning_of_its_paragraph_cut_where_it_is_long() {
    let w = story(&[V1]);
    let said =
        "The bridge opened in 1950 after a very long and careful piece of work by many hands.";
    put(
        &w,
        &page(
            &["title: Home", &format!("id: {HOME_ID}")],
            &format!("{said} ^b1\n\nIt crosses the river. ^b2\n"),
        ),
        &[claim(1, Standing::Contested, None, said)],
        &[],
    );
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    assert!(
        body.contains("aria-label=\"Sources and views: paragraph beginning “The bridge opened in 1950 after a very l…”\""),
        "{body}"
    );
    // The other paragraph holds no claim and so has no link.
    assert_eq!(body.matches("claim-link").count(), 1, "{body}");
}

#[tokio::test]
async fn a_title_that_two_pages_share_offers_each_of_them() {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/a/Twin.md", &page(&["title: Twin"], "One.\n")),
            ("pages/b/Twin.md", &page(&["title: Twin"], "Two.\n")),
        ],
    );
    let a = get(&w, "/handbook/-/why/Twin").await;
    assert_eq!(a.status, StatusCode::OK);
    let body = main_of(&a);
    assert_eq!(
        body.matches("<li><a href=\"/handbook/Twin\">Twin</a></li>")
            .count(),
        2,
        "{body}"
    );
}

#[tokio::test]
async fn how_a_paragraph_was_followed_is_not_said_when_no_change_was_read() {
    let w = story(&["Alpha.\n"]);
    for version in 2..=62_u64 {
        w.fake.set_files(&[(
            "pages/Guide.md",
            &format!("---\ntitle: Guide\n---\n\nv{version}.\n"),
        )]);
        w.fake.say(version, "Edit the guide");
        w.fake
            .show(version, shown(vec![op("edit-text", &["pages/Guide.md"])]));
    }
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    assert!(!body.contains("Traced by matching the text"), "{body}");
    assert!(!body.contains("Followed through its anchor"), "{body}");
    assert!(
        body.contains("Not changed in the changes read here."),
        "{body}"
    );
}

#[tokio::test]
async fn a_change_is_marked_as_the_wiki_marks_it() {
    let w = story(&[V1, V2]);
    let body = main_of(&get(&w, "/handbook/-/why/Home").await);
    assert!(
        body.contains("<li>The first change of this wiki</li>"),
        "{body}"
    );
}
