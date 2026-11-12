//! What a reader sees of the knowledge layer (RFC 005 part 2 D-7; handoff 005 K-T6), through the real router over
//! an in-memory repository: the one quiet link on a paragraph that holds a claim, the claim's page in words, the
//! *Why is this here?* link and the state it leads to, existence neutrality, and that a page with no claims is
//! the page it was. No prikk is needed, so these run in both test runs. Every sentence is invented.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod golden;

#[path = "../common/mod.rs"]
mod common;

use axum::http::{StatusCode, header};
use common::{
    APP, Answer, HOME_ID, World, claim, claim_id, get, home_with, link, page, put, request, source,
    source_id,
};
use inklkunn_core::knowledge::{
    Bearing, Identity, Link, Reference, ReferenceForm, SourceKind, Standing,
};
use inklkunn_store::Visibility;

const JA: &[(&str, &str)] = &[("cookie", "inklkunn_lang=ja")];

/// A wiki whose `Home` has the claimed paragraph `first` (`^b1`) and another that says nothing.
fn handbook() -> World {
    World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Home.md", &home_with("The bridge opened in 1950.")),
            (
                "pages/Guide.md",
                &page(&["title: Guide"], "The guide, with no claim.\n"),
            ),
        ],
    )
}

/// The first claim of the tests: a contested observation, judged on the paragraph as it is, with a source that
/// supports it directly and one that challenges it in part.
fn with_claim(w: &World) {
    let said = "The bridge opened in 1950.";
    let mut c = claim(1, Standing::Contested, None, said);
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, said),
        link(2, Link::Challenges, Bearing::Partial, said),
    ];
    put(
        w,
        &home_with(said),
        &[c],
        &[
            source(1, SourceKind::Primary),
            source(2, SourceKind::Secondary),
        ],
    );
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// The `<main>` of a page: what the page says, without the header and footer.
fn main_of(a: &Answer) -> String {
    let body = a.body();
    let start = body.find("<main").expect("a main landmark");
    let end = body.find("</main>").expect("the end of main");
    body.get(start..end).expect("main").to_owned()
}

fn claim_uri(n: u8) -> String {
    format!("/handbook/-/claim/{}", claim_id(n))
}

// ------------------------------------------------------------------------------------ a page with no claims

/// The wiki the golden pages were captured from: `Home` has an `id`, a block id and a list item with one, and no
/// claim anywhere; `Guide` is a plain page.
fn without_claims() -> World {
    World::new(
        "handbook",
        "The Handbook",
        &[
            (
                "pages/Home.md",
                &page(
                    &["title: Home", &format!("id: {HOME_ID}")],
                    "The bridge opened in 1950. ^b1\n\nPlain words, no claim.\n\n* A list item. ^l1\n* Another.\n",
                ),
            ),
            (
                "pages/Guide.md",
                &page(&["title: Guide"], "The guide, with no claim.\n"),
            ),
        ],
    )
}

/// From `<article>` to the end of the page's tools.
fn region(a: &Answer) -> String {
    let body = a.body();
    let start = body
        .find("<article class=\"page-body\"")
        .expect("an article");
    let rest = body.get(start..).expect("the rest");
    let end = rest.find("</div>").expect("the end of the tools") + "</div>".len();
    rest.get(..end).expect("the region").to_owned()
}

/// The one line a page's tools gained.
fn why_line(address: &str) -> String {
    format!("<p><a href=\"{address}\">Why is this here?</a></p>\n")
}

#[tokio::test]
async fn a_page_with_no_claims_is_the_page_it_was_except_for_the_one_link() {
    let w = without_claims();
    for (uri, golden, why) in [
        (
            "/handbook/Home",
            golden::GOLDEN_HOME,
            "/handbook/-/why/Home",
        ),
        (
            "/handbook/~main/Home",
            golden::GOLDEN_HOME_IN_A_VIEW,
            "/handbook/-/why/Home?view=main",
        ),
        (
            "/handbook/Guide",
            golden::GOLDEN_GUIDE,
            "/handbook/-/why/Guide",
        ),
    ] {
        let now = region(&get(&w, uri).await);
        assert_eq!(count(&now, &why_line(why)), 1, "{uri}: {now}");
        assert_eq!(now.replace(&why_line(why), ""), golden, "{uri}");
    }
}

#[tokio::test]
async fn claims_elsewhere_change_nothing_on_a_page_that_holds_none() {
    // The same pages in a wiki that has claims (said in a paragraph `Home` does not have): the bytes are the same.
    let w = without_claims();
    let before = region(&get(&w, "/handbook/Home").await);
    let mut c = claim(1, Standing::Contested, None, "Elsewhere.");
    c.expressions = vec![inklkunn_core::knowledge::Expression::new(HOME_ID, "gone").unwrap()];
    put(
        &w,
        &page(
            &["title: Home", &format!("id: {HOME_ID}")],
            "The bridge opened in 1950. ^b1\n\nPlain words, no claim.\n\n* A list item. ^l1\n* Another.\n",
        ),
        &[c],
        &[],
    );
    assert_eq!(region(&get(&w, "/handbook/Home").await), before);
}

#[tokio::test]
async fn the_link_is_the_only_thing_a_claim_adds_to_a_page() {
    let w = without_claims();
    let before = region(&get(&w, "/handbook/Home").await);
    let said = "The bridge opened in 1950.";
    let mut c = claim(1, Standing::Contested, None, said);
    c.sources = vec![link(1, Link::Supports, Bearing::Direct, said)];
    put(
        &w,
        &page(
            &["title: Home", &format!("id: {HOME_ID}")],
            "The bridge opened in 1950. ^b1\n\nPlain words, no claim.\n\n* A list item. ^l1\n* Another.\n",
        ),
        &[c],
        &[source(1, SourceKind::Primary)],
    );
    let after = region(&get(&w, "/handbook/Home").await);
    let tool = format!(
        " <a class=\"claim-link\" href=\"{}\" aria-label=\"Sources and views: paragraph beginning “The bridge opened in 1950.”\">Sources and views</a>",
        claim_uri(1)
    );
    assert_eq!(count(&after, &tool), 1, "{after}");
    assert_eq!(after.replace(&tool, ""), before);
}

// ------------------------------------------------------------------------------------ the paragraph

#[tokio::test]
async fn a_paragraph_that_holds_a_claim_has_one_quiet_link_and_the_others_have_none() {
    let w = handbook();
    with_claim(&w);
    let a = get(&w, "/handbook/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert_eq!(count(&body, "class=\"claim-link\""), 1, "{body}");
    // The link is inside the claimed paragraph, at its end, named by the paragraph.
    let want = format!(
        "<p id=\"h-^b1\">The bridge opened in 1950. <a class=\"claim-link\" href=\"{}\" aria-label=\"Sources and views: paragraph beginning “The bridge opened in 1950.”\">Sources and views</a></p>",
        claim_uri(1)
    );
    assert!(body.contains(&want), "{body}");
    // The paragraph that holds no claim carries nothing, and neither does a page that has no claims.
    assert!(body.contains("<p>Plain words, no claim.</p>"), "{body}");
    let guide = get(&w, "/handbook/Guide").await.body();
    assert!(!guide.contains("claim-link"), "{guide}");
}

#[tokio::test]
async fn the_link_stays_in_the_view_and_speaks_the_readers_language() {
    let w = handbook();
    with_claim(&w);
    let in_view = get(&w, "/handbook/~main/Home").await.body();
    assert!(
        in_view.contains(&format!("href=\"{}?view=main\"", claim_uri(1))),
        "{in_view}"
    );
    let ja = request(&w, APP, "GET", "/handbook/Home", JA).await.body();
    assert!(
        ja.contains(
            "aria-label=\"出典とビュー: 「The bridge opened in 1950.」で始まる段落\">出典とビュー</a>"
        ),
        "{ja}"
    );
}

#[tokio::test]
async fn a_claim_whose_paragraph_the_page_no_longer_has_is_offered_on_no_page() {
    let w = handbook();
    with_claim(&w);
    // The paragraph loses its block id: the claim is still in the wiki, and the page offers nothing for it.
    w.fake.set(
        "pages/Home.md",
        &page(
            &["title: Home", &format!("id: {HOME_ID}")],
            "The bridge opened in 1950.\n",
        ),
    );
    let body = get(&w, "/handbook/Home").await.body();
    assert!(!body.contains("claim-link"), "{body}");
    // And the claim page says so, in words.
    let claim = get(&w, &claim_uri(1)).await;
    assert!(
        claim
            .body()
            .contains("No page has the paragraph this claim is said in, as the pages are now."),
        "{}",
        claim.body()
    );
}

#[tokio::test]
async fn a_stale_page_is_shown_without_the_links() {
    let w = handbook();
    with_claim(&w);
    let fresh = get(&w, "/handbook/Home").await.body();
    assert!(fresh.contains("claim-link"));
    w.fake.fail_views(Some(common::busy()));
    let stale = get(&w, "/handbook/Home").await;
    assert_eq!(stale.status, StatusCode::OK);
    let body = stale.body();
    assert!(body.contains("stale-note"), "{body}");
    assert!(!body.contains("claim-link"), "{body}");
}

// ------------------------------------------------------------------------------------ the page's tools

#[tokio::test]
async fn one_why_is_this_here_link_is_among_the_pages_tools() {
    let w = handbook();
    let body = get(&w, "/handbook/Home").await.body();
    assert_eq!(count(&body, "Why is this here?"), 1, "{body}");
    assert!(
        body.contains("<p><a href=\"/handbook/-/why/Home\">Why is this here?</a></p>"),
        "{body}"
    );
    // In a view it stays in the view; a title is encoded as every address of a title is.
    let in_view = get(&w, "/handbook/~main/Home").await.body();
    assert!(in_view.contains("href=\"/handbook/-/why/Home?view=main\""));
    let guide = get(&w, "/handbook/Guide").await.body();
    assert!(guide.contains("Why is this here?"));
}

#[tokio::test]
async fn the_why_link_is_not_offered_on_a_permalink() {
    let w = handbook();
    common::script_history(&w);
    let at = get(&w, &format!("/handbook/-/at/{}/Home", common::block_id(2))).await;
    assert_eq!(at.status, StatusCode::OK);
    assert!(!at.body().contains("Why is this here?"), "{}", at.body());
    // The page itself, at the tip, has it.
    assert!(
        get(&w, "/handbook/Home")
            .await
            .body()
            .contains("Why is this here?")
    );
}

#[tokio::test]
async fn the_link_leads_to_the_page_that_shows_where_each_paragraph_came_from() {
    let w = handbook();
    let a = get(&w, "/handbook/-/why/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    let body = main_of(&a);
    assert!(body.contains("<h1>Why is this here?</h1>"), "{body}");
    assert!(
        body.contains("<a href=\"/handbook/Home\">Back to Home</a>"),
        "{body}"
    );
    // Each paragraph of the page the link names has an anchor.
    assert!(
        body.contains("<li id=\"w-^b1\">") && body.contains("<li id=\"w-2\">"),
        "{body}"
    );
    let ja = request(&w, APP, "GET", "/handbook/-/why/Home", JA)
        .await
        .body();
    assert!(ja.contains("なぜここにあるのか"), "{ja}");
}

#[tokio::test]
async fn a_title_no_page_has_gets_the_page_every_missing_title_gets() {
    let w = handbook();
    let none = get(&w, "/handbook/-/why/Nothing%20here").await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    assert!(
        main_of(&none).contains("This page does not exist yet"),
        "{}",
        main_of(&none)
    );
    // A wiki that is not there is the generic page, not this one.
    let ghost = get(&w, "/no-such-wiki/-/why/Home").await;
    assert_eq!(ghost.status, StatusCode::NOT_FOUND);
    assert!(!ghost.body().contains("This page does not exist yet"));
}

// ------------------------------------------------------------------------------------ the claim's page

#[tokio::test]
async fn a_claim_page_says_its_kind_its_sources_and_its_status_in_words() {
    let w = handbook();
    with_claim(&w);
    let a = get(&w, &claim_uri(1)).await;
    assert_eq!(a.status, StatusCode::OK);
    let main = main_of(&a);
    assert!(main.contains("<h1>Sources and views</h1>"), "{main}");
    assert!(
        main.contains("What kind of statement it is: an observation."),
        "{main}"
    );
    // Where it is said, linked to the paragraph.
    assert!(
        main.contains("<a href=\"/handbook/Home#h-^b1\">Home</a>"),
        "{main}"
    );
    // Each source with its relation and how directly it speaks, and nothing else about it. These two say nothing
    // of themselves, so the identifier is the one way left to tell them apart.
    assert!(
        main.contains(&format!(
            "A source without a title (primary): supports, directly. Its identifier is {}.",
            source_id(1)
        )),
        "{main}"
    );
    assert!(
        main.contains(&format!(
            "A source without a title (secondary): challenges, in part. Its identifier is {}.",
            source_id(2)
        )),
        "{main}"
    );
    assert!(
        main.contains("This claim is contested in this view."),
        "{main}"
    );
    assert!(!main.contains("earlier wording"), "{main}");
    // The page's landmarks and headings are the page's own.
    assert_eq!(count(&a.body(), "<h1"), 1);
    for h in ["Where it is said", "Sources", "Status in this view"] {
        assert!(main.contains(&format!("<h2>{h}</h2>")), "{h}: {main}");
    }
}

/// A claim with three sources: one that names itself fully, one by its title alone, one by its reference alone.
fn with_named_sources(w: &World, title: &str, author: &str) {
    let said = "The bridge opened in 1950.";
    let mut c = claim(1, Standing::Contested, None, said);
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, said),
        link(2, Link::Challenges, Bearing::Partial, said),
        link(3, Link::Supports, Bearing::Indirect, said),
    ];
    let mut full = source(1, SourceKind::Primary);
    full.identity = Identity {
        title: Some(title.to_owned()),
        author: Some(author.to_owned()),
        date: Some("Spring 1950".to_owned()),
    };
    full.reference = Some(Reference {
        form: ReferenceForm::Url,
        value: "https://example.org/survey".to_owned(),
    });
    let mut titled = source(2, SourceKind::Secondary);
    titled.identity.title = Some("A Short History".to_owned());
    let mut referenced = source(3, SourceKind::Observational);
    referenced.reference = Some(Reference {
        form: ReferenceForm::Citation,
        value: "Harbour Notes 12, p. 4".to_owned(),
    });
    put(w, &home_with(said), &[c], &[full, titled, referenced]);
}

#[tokio::test]
async fn a_source_is_named_by_what_it_says_of_itself_and_the_identifier_is_not_its_name() {
    let w = handbook();
    with_named_sources(&w, "The Bridge Survey", "The Harbour Authority");
    let main = main_of(&get(&w, &claim_uri(1)).await);
    assert!(
        main.contains("“The Bridge Survey”, by The Harbour Authority, dated Spring 1950 (primary): supports, directly. Where it is found: https://example.org/survey."),
        "{main}"
    );
    assert!(
        main.contains("“A Short History” (secondary): challenges, in part."),
        "{main}"
    );
    // A source with a reference and no title is still a source without a title, and says where it is found.
    assert!(
        main.contains("A source without a title (observational): supports, indirectly. Where it is found: Harbour Notes 12, p. 4."),
        "{main}"
    );
    // Not one of them is known by its identifier: it is told only of a source that gives nothing else.
    for n in 1..=3 {
        assert!(!main.contains(&source_id(n).to_string()), "{main}");
    }
    assert!(!main.contains("Source 0"), "{main}");
    // A reference is words, never a link the wiki sends a reader to.
    assert!(!main.contains("href=\"https://example.org"), "{main}");
}

#[tokio::test]
async fn what_a_source_says_of_itself_is_escaped_and_on_one_line() {
    let w = handbook();
    with_named_sources(
        &w,
        "A <script>alert(1)</script> title\n\nin   two   lines",
        "Tom & \"Jerry\" <b>Ltd</b>",
    );
    let main = main_of(&get(&w, &claim_uri(1)).await);
    assert!(!main.contains("<script>alert"), "{main}");
    assert!(!main.contains("<b>Ltd"), "{main}");
    assert!(
        main.contains("“A &#60;script&#62;alert(1)&#60;/script&#62; title in two lines”"),
        "{main}"
    );
    assert!(
        main.contains("by Tom &#38; &#34;Jerry&#34; &#60;b&#62;Ltd&#60;/b&#62;"),
        "{main}"
    );
}

#[tokio::test]
async fn a_source_is_named_in_japanese_with_the_japanese_words_around_its_own() {
    let w = handbook();
    with_named_sources(&w, "橋の調査", "港湾局");
    let main = main_of(&request(&w, APP, "GET", &claim_uri(1), JA).await);
    assert!(
        main.contains("「橋の調査」、港湾局 による、日付 Spring 1950（一次）: 直接に裏付けている。 所在: https://example.org/survey。"),
        "{main}"
    );
    assert!(main.contains("題名のない出典"), "{main}");
}

#[tokio::test]
async fn a_long_name_is_cut_once_with_an_ellipsis_where_it_was_cut() {
    let w = handbook();
    with_named_sources(&w, &"橋".repeat(500), "The Harbour Authority");
    let main = main_of(&get(&w, &claim_uri(1)).await);
    let long = format!("“{}…”", "橋".repeat(299));
    assert!(main.contains(&long), "{main}");
    assert!(!main.contains(&"橋".repeat(300)), "{main}");
}

#[tokio::test]
async fn a_claim_page_in_japanese_is_a_page_of_its_own_words() {
    let w = handbook();
    with_claim(&w);
    let a = request(&w, APP, "GET", &claim_uri(1), JA).await;
    let main = main_of(&a);
    assert!(main.contains("この主張の種類: 観察。"), "{main}");
    assert!(main.contains("直接に裏付けている"), "{main}");
    assert!(main.contains("一部に異議を唱えている"), "{main}");
    assert!(
        main.contains("このビューでは、この主張は争われています。"),
        "{main}"
    );
}

#[tokio::test]
async fn a_relation_judged_on_a_wording_the_paragraph_no_longer_has_says_so_until_it_is_confirmed()
{
    let w = handbook();
    with_claim(&w);
    // The paragraph's words change; the id stays. Everything was judged on the earlier wording.
    let reworded = "The bridge was opened in 1950.";
    let old = "The bridge opened in 1950.";
    w.fake.set("pages/Home.md", &home_with(reworded));
    let main = main_of(&get(&w, &claim_uri(1)).await);
    assert_eq!(count(&main, "Judged on an earlier wording."), 3, "{main}");
    // A reviewer's confirmation (the claim rewritten on the new wording) clears it for the relation and the status.
    let mut c = claim(1, Standing::Contested, None, reworded);
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, reworded),
        link(2, Link::Challenges, Bearing::Partial, old),
    ];
    put(
        &w,
        &home_with(reworded),
        &[c],
        &[
            source(1, SourceKind::Primary),
            source(2, SourceKind::Secondary),
        ],
    );
    let main = main_of(&get(&w, &claim_uri(1)).await);
    // The one relation not yet confirmed still says so; the others do not.
    assert_eq!(count(&main, "Judged on an earlier wording."), 1, "{main}");
    assert!(
        main.contains(&format!(
            "challenges, in part. Its identifier is {}. <span class=\"claim-note\">Judged on an earlier wording.</span>",
            source_id(2)
        )),
        "{main}"
    );
}

#[tokio::test]
async fn each_status_is_said_in_its_own_words_and_a_status_nobody_decided_names_no_proposal() {
    let w = handbook();
    let said = "The bridge opened in 1950.";
    let cases = [
        (Standing::Accepted, "This claim is accepted in this view."),
        (Standing::Contested, "This claim is contested in this view."),
        (
            Standing::Unresolved,
            "This claim is unresolved in this view.",
        ),
        (
            Standing::Unknown,
            "This claim has not been decided in this view.",
        ),
    ];
    for (standing, sentence) in cases {
        let proposal = "01930000-aaaa-7000-8000-000000000001";
        put(
            &w,
            &home_with(said),
            &[claim(1, standing, Some(proposal), said)],
            &[],
        );
        let main = main_of(&get(&w, &claim_uri(1)).await);
        assert!(main.contains(sentence), "{standing:?}: {main}");
        // Of the four, only a decided status says who decided it (here: a proposal this wiki does not hold).
        assert_eq!(
            main.contains("Decided by"),
            standing != Standing::Unknown,
            "{standing:?}: {main}"
        );
    }
}

#[tokio::test]
async fn the_deciding_proposal_is_linked_when_the_wiki_holds_it_and_named_when_it_does_not() {
    let w = common::with_accounts(handbook());
    let me = common::person(1);
    common::let_read(&w, &me);
    let held = common::held_proposal(&w, &me, "Settle the date");
    let said = "The bridge opened in 1950.";
    put(
        &w,
        &home_with(said),
        &[claim(1, Standing::Accepted, Some(held.id.as_str()), said)],
        &[],
    );
    let main = main_of(&get(&w, &claim_uri(1)).await);
    assert!(
        main.contains(&format!(
            "Decided by <a href=\"/handbook/-/proposals/{}\">the proposal that decided it</a>.",
            held.id
        )),
        "{main}"
    );
    // A proposal that is not among this wiki's records (an imported wiki's) is said so, with no link.
    put(
        &w,
        &home_with(said),
        &[claim(
            1,
            Standing::Accepted,
            Some("01930000-aaaa-7000-8000-000000000009"),
            said,
        )],
        &[],
    );
    let main = main_of(&get(&w, &claim_uri(1)).await);
    assert!(
        main.contains("Decided by a proposal that this wiki&#39;s records do not hold."),
        "{main}"
    );
    assert!(!main.contains("/-/proposals/"), "{main}");
}

#[tokio::test]
async fn a_claim_with_no_source_and_a_source_whose_file_cannot_be_read_are_said_so() {
    let w = handbook();
    let said = "The bridge opened in 1950.";
    put(
        &w,
        &home_with(said),
        &[claim(1, Standing::Unknown, None, said)],
        &[],
    );
    let main = main_of(&get(&w, &claim_uri(1)).await);
    assert!(
        main.contains("No source is attached to this claim."),
        "{main}"
    );
    // A relation to a source whose file is not there (a wiki written by other means).
    let mut c = claim(1, Standing::Unknown, None, said);
    c.sources = vec![link(7, Link::Supports, Bearing::Indirect, said)];
    put(&w, &home_with(said), &[c], &[]);
    let main = main_of(&get(&w, &claim_uri(1)).await);
    assert!(
        main.contains(&format!(
            "Source {} (its file cannot be read): supports, indirectly.",
            source_id(7)
        )),
        "{main}"
    );
}

#[tokio::test]
async fn the_claim_page_in_a_view_says_which_view_and_a_view_that_is_not_there_is_not_a_claim_page()
{
    let w = handbook();
    with_claim(&w);
    let a = get(&w, &format!("{}?view=main", claim_uri(1))).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(main_of(&a).contains("main"), "{}", main_of(&a));
    assert!(main_of(&a).contains("class=\"view-note\""));
    // A name no view can have, and a name no view has.
    for bad in ["..%2Fx", "draft"] {
        let a = get(&w, &format!("{}?view={bad}", claim_uri(1))).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn an_address_that_names_no_claim_is_one_plain_page() {
    let w = handbook();
    with_claim(&w);
    let none = get(&w, &claim_uri(9)).await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    assert!(
        main_of(&none).contains("No such claim"),
        "{}",
        main_of(&none)
    );
    // Not an id, an id in the wrong case, a source's id: the same page as a claim that is not there.
    for uri in [
        "/handbook/-/claim/nothing",
        "/handbook/-/claim/01930000-0000-7000-8000-000000000001-x",
        &format!("/handbook/-/claim/{}", source_id(1)),
        "/handbook/-/claim/01930000-0000-7000-8000-00000000000A",
    ] {
        let a = get(&w, uri).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{uri}");
        // The only difference allowed is the reader's own address, echoed in the language buttons.
        assert_eq!(
            a.body().replace(uri, "U"),
            none.body().replace(&claim_uri(9), "U"),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn a_wiki_that_may_not_be_read_answers_as_a_wiki_that_is_not_there() {
    let w = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &home_with("Hidden."))],
    );
    put(
        &w,
        &home_with("Hidden."),
        &[claim(1, Standing::Contested, None, "Hidden.")],
        &[],
    );
    for path in [
        "/{}/-/claim/01930000-0000-7000-8000-000000000001",
        "/{}/-/claim/nothing",
        "/{}/-/why/Home",
        "/{}/-/why/Home?view=main",
    ] {
        let real = get(&w, &path.replace("{}", "secret")).await;
        let absent = get(&w, &path.replace("{}", "no-such-wiki")).await;
        assert_eq!(real.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(real.status, absent.status, "{path}");
        assert_eq!(
            real.body().replace("secret", "X"),
            absent.body().replace("no-such-wiki", "X"),
            "{path}: the answers differ"
        );
        assert_eq!(
            real.headers.get(header::CONTENT_LANGUAGE),
            absent.headers.get(header::CONTENT_LANGUAGE)
        );
        assert!(!real.body().contains("Hidden"), "{path}");
        assert!(!real.body().contains("Secret"), "{path}");
    }
}

// ------------------------------------------------------------------------------------ nothing ranks

/// Words a page about a claim must not say, in English (the catalog's retired words, plus the ranking words a
/// score would use).
const RANKING_WORDS: &[&str] = &[
    "true",
    "false",
    "verified",
    "proven",
    "canonical",
    "adopted",
    "official",
    "score",
    "rank",
    "reliable",
    "credible",
    "trusted",
    "best",
    "strong",
    "weak",
];

fn words_of(text: &str) -> Vec<String> {
    // The text of the page: tags dropped, then the words in it, lower-cased.
    let mut plain = String::new();
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => {
                in_tag = true;
                plain.push(' ');
            }
            '>' => in_tag = false,
            c if !in_tag => plain.push(c),
            _ => {}
        }
    }
    plain
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[tokio::test]
async fn no_page_of_the_knowledge_layer_says_that_a_claim_is_true_or_ranks_one() {
    let w = handbook();
    with_claim(&w);
    // Every state of the claim page and the page it links from, in both languages.
    let said = "The bridge opened in 1950.";
    for standing in [
        Standing::Accepted,
        Standing::Contested,
        Standing::Unresolved,
        Standing::Unknown,
    ] {
        let mut c = claim(
            1,
            standing,
            Some("01930000-aaaa-7000-8000-000000000001"),
            said,
        );
        c.sources = vec![
            link(1, Link::Supports, Bearing::Direct, said),
            link(2, Link::Challenges, Bearing::Indirect, said),
        ];
        put(
            &w,
            &home_with(said),
            &[c],
            &[
                source(1, SourceKind::Primary),
                source(2, SourceKind::Anecdotal),
            ],
        );
        for lang in [&[][..], JA] {
            for uri in [
                claim_uri(1),
                "/handbook/Home".to_owned(),
                "/handbook/-/why/Home".to_owned(),
            ] {
                let a = request(&w, APP, "GET", &uri, lang).await;
                let main = main_of(&a);
                // The English words are matched whole in the English pages; the Japanese pages hold none of them.
                let words = words_of(&main);
                for banned in RANKING_WORDS {
                    assert!(
                        !words.iter().any(|w| w == banned),
                        "{uri} ({lang:?}) says {banned:?}: {main}"
                    );
                }
                // No colour, icon or number is drawn: the page holds no style, no image and no glyph that ranks.
                assert!(!main.contains("style="), "{uri}: {main}");
                assert!(!main.contains("<img"), "{uri}: {main}");
                assert!(
                    !main.contains('★') && !main.contains('✓') && !main.contains('%'),
                    "{uri}"
                );
            }
        }
    }
}
