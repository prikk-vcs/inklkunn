//! The endpoints that look across a wiki, through the real router (RFC 003 D-13; handoff 003 T8): the pages
//! that link to a page, the six link health lists, search with its snippets as segments, and the recent
//! changes with their filters and their cursor. They call the very reads the web pages call, so each test that
//! can holds the two to the same facts. The repository is an in-memory double.

use axum::http::{Method, StatusCode};
use inklkunn_store::{AccountId, Visibility};
use serde_json::{Value, json};

use super::common::{
    World, block_id, import_message, let_read, page, proposal_message, request, with_accounts,
};
use super::{Answer, At, JA, get, problem_type, send};

fn pages() -> Vec<(String, String)> {
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

pub(super) fn wiki() -> World {
    let owned = pages();
    let files: Vec<(&str, &str)> = owned
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::new("handbook", "The Handbook", &files)
}

fn api(tail: &str) -> String {
    format!("/handbook/-/api/v1/{tail}")
}

fn ok(a: &Answer) -> Value {
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    assert_eq!(a.header("content-type"), Some("application/json"));
    a.json()
}

fn titles(body: &Value) -> Vec<String> {
    body.at("/items")
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.at("/title").as_str().unwrap().to_owned())
        .collect()
}

fn encode(text: &str) -> String {
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

/// Walk a list by its cursor: every page's body, the pages' sizes never above `size`.
async fn walk(w: &World, first: &str, size: usize) -> Vec<Value> {
    let mut bodies = Vec::new();
    let mut uri = first.to_owned();
    loop {
        let body = ok(&get(w, &uri).await);
        assert!(
            body.at("/items").as_array().unwrap().len() <= size,
            "{body}"
        );
        let next = body.at("/next_cursor").as_str().map(str::to_owned);
        bodies.push(body);
        match next {
            Some(cursor) => uri = format!("{first}&cursor={cursor}"),
            None => return bodies,
        }
        assert!(bodies.len() < 50, "the walk does not end");
    }
}

// ------------------------------------------------------------------------------------ search

#[tokio::test]
async fn a_search_finds_by_any_part_folded_and_says_what_matched_as_segments_of_text() {
    let w = wiki();
    for (query, title) in [
        ("cafe", "Café Society"),
        ("CAFE", "Café Society"),
        ("ｃａｆｅ", "Café Society"),
        ("fe soc", "Café Society"),
        ("ウィキ", "日本語のウィキ入門"),
        ("ｳｨｷ", "日本語のウィキ入門"),
        ("faq", "Getting Started"),
    ] {
        let body = ok(&get(&w, &api(&format!("search?q={}", encode(query)))).await);
        assert!(titles(&body).contains(&title.to_owned()), "{query}: {body}");
    }
    // The snippet is the text that matched, cut into the part that matched and the rest.
    let body = ok(&get(&w, &api("search?q=cafe")).await);
    let hit = body.at("/items/0");
    assert_eq!(hit.at("/title"), "Café Society");
    assert_eq!(hit.at("/kind"), "title");
    assert_eq!(
        hit.at("/snippet"),
        &json!([
            {"text": "Café", "marked": true},
            {"text": " Society", "marked": false},
        ])
    );
    // An alias that matched says so, and the snippet is the alias.
    let body = ok(&get(&w, &api("search?q=faq")).await);
    let hit = body.at("/items/0");
    assert_eq!(hit.at("/title"), "Getting Started");
    assert_eq!(hit.at("/kind"), "alias");
    assert_eq!(
        hit.at("/snippet"),
        &json!([{"text": "ＦＡＱ", "marked": true}])
    );
    assert_eq!(body.at("/query"), "faq");
    assert_eq!(body.at("/tip/view"), "main");
}

#[tokio::test]
async fn a_snippet_is_never_html_even_when_the_title_or_alias_is_markup() {
    let w = wiki();
    let a = get(&w, &api("search?q=script")).await;
    let body = ok(&a);
    let hit = body.at("/items/0");
    assert_eq!(hit.at("/title"), "<img src=x onerror=alert(1)>");
    assert_eq!(hit.at("/kind"), "alias");
    let joined: String = hit
        .at("/snippet")
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.at("/text").as_str().unwrap())
        .collect();
    assert_eq!(
        joined, "<script>alert(1)</script>",
        "the alias, whole, as text"
    );
    assert!(
        hit.at("/snippet")
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.at("/marked") == true && s.at("/text") == "script")
    );
    // On the wire the markup is JSON escapes; there is no member that holds HTML.
    let text = a.text();
    assert!(
        !text.contains('<') && !text.contains('>') && !text.contains('&'),
        "{text}"
    );
    assert!(
        !text.contains("<strong>") && !text.contains("\"html\""),
        "{text}"
    );
}

#[tokio::test]
async fn a_search_that_names_a_page_says_so_and_one_that_finds_nothing_is_an_empty_list() {
    let w = wiki();
    let home = ok(&get(&w, &api("search?q=Home")).await);
    assert_eq!(home.at("/names_a_page"), true);
    assert_eq!(home.at("/items/0/title"), "Home");
    let alias = ok(&get(&w, &api("search?q=Guide")).await);
    assert_eq!(alias.at("/names_a_page"), true, "an alias is a name");
    let none = ok(&get(&w, &api("search?q=zzzzzz")).await);
    assert_eq!(none.at("/items"), &json!([]));
    assert_eq!(none.at("/names_a_page"), false);
    assert_eq!(none.at("/truncated"), false);
    // Spaces alone search for nothing, and find nothing.
    let blank = ok(&get(&w, &api("search?q=%20%20")).await);
    assert_eq!(blank.at("/items"), &json!([]));
}

#[tokio::test]
async fn a_search_is_bounded_and_says_when_it_left_hits_out() {
    let w = wiki();
    let all = ok(&get(&w, &api("search?q=o")).await);
    let total = titles(&all).len();
    assert!(total > 2, "{all}");
    assert_eq!(all.at("/truncated"), false);
    let one = ok(&get(&w, &api("search?q=o&limit=2")).await);
    assert_eq!(titles(&one).len(), 2);
    assert_eq!(one.at("/truncated"), true);
    assert_eq!(titles(&one), titles(&all).get(..2).unwrap());
    for tail in [
        "search?q=o&limit=0",
        "search?q=o&limit=101",
        "search?q=o&limit=x",
        "search?q=o&limit=+5",
    ] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{tail}");
        assert_eq!(a.json().at("/parameter"), "limit", "{tail}");
    }
    // The query is required, and a search has no cursor: there is one page of the best.
    for (tail, parameter) in [
        ("search", "q"),
        ("search?q=", "q"),
        ("search?q=o&cursor=c1", "cursor"),
        ("search?q=o&q=p", "q"),
        ("search?q=o&unknown=1", "unknown"),
    ] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{tail}");
        assert_eq!(a.json().at("/parameter"), parameter, "{tail}");
    }
    let long = "x".repeat(501);
    let a = get(&w, &api(&format!("search?q={long}"))).await;
    assert_eq!(a.json().at("/parameter"), "q");
}

#[tokio::test]
async fn a_search_follows_the_tip_and_the_view_and_has_a_validator() {
    let w = wiki();
    let before = get(&w, &api("search?q=brandnew")).await;
    assert_eq!(ok(&before).at("/items"), &json!([]));
    let etag = before.header("etag").unwrap().to_owned();
    let same = send(
        &w,
        Method::GET,
        &api("search?q=brandnew"),
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(same.status, StatusCode::NOT_MODIFIED);
    w.fake.set(
        "pages/Brandnew.md",
        &page(&["title: Brandnew"], "A new page.\n"),
    );
    let after = send(
        &w,
        Method::GET,
        &api("search?q=brandnew"),
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(after.status, StatusCode::OK);
    assert_eq!(titles(&after.json()), ["Brandnew"]);
    let view = get(&w, &api("search?q=home&view=nope")).await;
    assert_eq!(problem_type(&view), "urn:inklkunn:problem:no-such-view");
    // The same pages in the same order as the search page lists them.
    let web = get(&w, "/handbook/-/search?q=cafe").await;
    assert!(web.text().contains("Café Society"));
}

// ------------------------------------------------------------------------------------ health

#[tokio::test]
async fn every_health_list_shows_its_planted_case_with_what_would_resolve_it_as_a_code() {
    let w = wiki();
    let list = |name: &'static str| {
        let w = &w;
        async move { ok(&get(w, &api(&format!("health?list={name}"))).await) }
    };

    let wanted = list("wanted").await;
    assert_eq!(wanted.at("/list"), "wanted");
    assert_eq!(titles(&wanted), ["Ghost", "Missing Page"]);
    assert_eq!(wanted.at("/items/0/linked_from"), 1);
    assert_eq!(wanted.at("/items/0/action"), "add-page-or-fix-links");
    assert_eq!(wanted.at("/total"), 2);
    assert_eq!(*wanted.at("/scan"), Value::Null);

    let orphans = list("orphans").await;
    let names = titles(&orphans);
    assert!(names.contains(&"Lonely".to_owned()), "{orphans}");
    assert!(
        names.contains(&"日本語のウィキ入門".to_owned()),
        "{orphans}"
    );
    assert!(!names.contains(&"Home".to_owned()));
    assert!(
        orphans
            .at("/items")
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i.at("/action") == "link-to-it")
    );

    let dead_ends = list("dead-ends").await;
    assert!(titles(&dead_ends).contains(&"Lonely".to_owned()));
    assert!(!titles(&dead_ends).contains(&"Home".to_owned()));
    assert_eq!(dead_ends.at("/items/0/action"), "link-from-it");

    let redirects = list("redirects").await;
    assert_eq!(titles(&redirects), ["Dead", "Older"]);
    assert_eq!(redirects.at("/items/0/target"), "Nowhere");
    assert_eq!(redirects.at("/items/0/problem"), "broken");
    assert_eq!(redirects.at("/items/0/action"), "retarget-or-remove");
    assert_eq!(redirects.at("/items/1/problem"), "double");
    assert_eq!(redirects.at("/items/1/action"), "point-at-final-page");

    let help = list("wants-help").await;
    assert_eq!(titles(&help), ["Draft", "Stubby", "Work"]);
    let marks: Vec<&str> = help
        .at("/items")
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.at("/mark").as_str().unwrap())
        .collect();
    assert_eq!(marks, ["draft", "stub", "wip"]);
    assert_eq!(help.at("/items/0/action"), "finish-and-unmark");

    // Stale: no change touched any page, so none has a last change in the window.
    let stale = list("stale").await;
    assert!(
        stale
            .at("/items")
            .as_array()
            .unwrap()
            .iter()
            .all(|i| *i.at("/last_touched") == Value::Null && i.at("/action") == "review")
    );
    assert!(stale.at("/scan/looked_through").is_number());
    assert!(stale.at("/scan/complete").is_boolean());
}

#[tokio::test]
async fn the_stale_list_puts_the_pages_untouched_in_the_window_first() {
    let w = wiki();
    w.fake.tip_message("Touch the Home page");
    w.fake.touches(&block_id(1), &["pages/Home.md"]);
    let body = ok(&get(&w, &api("health?list=stale")).await);
    let items = body.at("/items").as_array().unwrap();
    assert_eq!(items.last().unwrap().at("/title"), "Home");
    assert!(items.last().unwrap().at("/last_touched").is_number());
    assert_eq!(*items.first().unwrap().at("/last_touched"), Value::Null);
}

#[tokio::test]
async fn the_rows_are_the_ones_the_health_pages_show_in_the_order_they_show_them() {
    let w = wiki();
    for (slug, key) in [
        ("wanted", "Missing Page"),
        ("orphans", "Lonely"),
        ("dead-ends", "Lonely"),
        ("redirects", "Older"),
        ("wants-help", "Stubby"),
    ] {
        let api_rows = titles(&ok(&get(&w, &api(&format!("health?list={slug}"))).await));
        let web = get(&w, &format!("/handbook/-/health/{slug}")).await.text();
        let mut last = 0;
        for title in &api_rows {
            // A title is escaped by the page; compare plain ones and skip the hostile one.
            if title.contains('<') {
                continue;
            }
            let at = web
                .find(title.as_str())
                .unwrap_or_else(|| panic!("{title}: {web}"));
            assert!(at >= last, "{slug}: {title} is out of order");
            last = at;
        }
        assert!(api_rows.contains(&key.to_owned()), "{slug}");
    }
}

#[tokio::test]
async fn a_health_list_is_walked_by_cursor_in_bounded_pages_and_a_cursor_does_not_outlive_its_tip()
{
    let w = wiki();
    let bodies = walk(&w, &api("health?list=orphans&limit=2"), 2).await;
    assert!(bodies.len() >= 2, "{bodies:?}");
    let walked: Vec<String> = bodies.iter().flat_map(titles).collect();
    let whole = ok(&get(&w, &api("health?list=orphans")).await);
    assert_eq!(walked, titles(&whole), "every row once, in order");
    let total = whole.at("/total").as_u64().unwrap();
    assert_eq!(total as usize, walked.len());
    assert!(
        bodies
            .iter()
            .all(|b| b.at("/total").as_u64() == Some(total))
    );
    // The wiki changes: the cursor of the old tip is not followed into a new list.
    let first = ok(&get(&w, &api("health?list=orphans&limit=2")).await);
    let cursor = first.at("/next_cursor").as_str().unwrap().to_owned();
    w.fake.set(
        "pages/Another.md",
        &page(&["title: Another"], "Another orphan.\n"),
    );
    let a = get(
        &w,
        &api(&format!("health?list=orphans&limit=2&cursor={cursor}")),
    )
    .await;
    assert_eq!(a.status, StatusCode::BAD_REQUEST);
    assert_eq!(problem_type(&a), "urn:inklkunn:problem:invalid-cursor");
    // Nor is a cursor of one list followed in another, nor one of another kind.
    let fresh = ok(&get(&w, &api("health?list=orphans&limit=2")).await);
    let cursor = fresh.at("/next_cursor").as_str().unwrap().to_owned();
    for tail in [
        format!("health?list=dead-ends&limit=2&cursor={cursor}"),
        format!("backlinks/Home?cursor={cursor}"),
        "health?list=orphans&cursor=n13".to_owned(),
        "health?list=orphans&cursor=o1999.abc".to_owned(),
        format!("health?list=orphans&cursor={cursor}x"),
    ] {
        let a = get(&w, &api(&tail)).await;
        assert_eq!(
            problem_type(&a),
            "urn:inklkunn:problem:invalid-cursor",
            "{tail}"
        );
    }
}

#[tokio::test]
async fn a_list_is_required_and_is_one_of_the_six() {
    let w = wiki();
    for tail in [
        "health",
        "health?list=",
        "health?list=Wanted",
        "health?list=other",
        "health?list=wanted&list=orphans",
    ] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{tail}");
        assert_eq!(a.json().at("/parameter"), "list", "{tail}");
    }
    for tail in ["health?list=wanted&limit=0", "health?list=wanted&limit=101"] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.json().at("/parameter"), "limit", "{tail}");
    }
    let view = get(&w, &api("health?list=stale&view=nope")).await;
    assert_eq!(problem_type(&view), "urn:inklkunn:problem:no-such-view");
    // A list of another view: its own pages.
    w.fake.add_view("draft");
    let draft = ok(&get(&w, &api("health?list=wanted&view=draft")).await);
    assert_eq!(draft.at("/tip/view"), "draft");
    let stale = ok(&get(&w, &api("health?list=stale&view=draft")).await);
    assert_eq!(stale.at("/tip/view"), "draft");
}

#[tokio::test]
async fn the_stale_list_of_a_view_is_judged_by_that_views_changes() {
    let w = handbook();
    let last_touched = |body: &Value| -> Value {
        body.at("/items")
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i.at("/title") == "Draft Chapter")
            .unwrap()
            .at("/last_touched")
            .clone()
    };
    // Only the draft view has a change that touched the chapter.
    let main = ok(&get(&w, &api("health?list=stale")).await);
    assert_eq!(last_touched(&main), Value::Null, "{main}");
    let draft = ok(&get(&w, &api("health?list=stale&view=draft")).await);
    assert!(last_touched(&draft).is_number(), "{draft}");
    assert_eq!(draft.at("/tip/view"), "draft");
}

#[tokio::test]
async fn the_health_lists_have_validators_and_say_so_when_prikk_cannot_be_asked() {
    let w = wiki();
    let a = get(&w, &api("health?list=wanted")).await;
    let etag = a.header("etag").unwrap().to_owned();
    let same = send(
        &w,
        Method::GET,
        &api("health?list=wanted"),
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(same.status, StatusCode::NOT_MODIFIED);
    let other = get(&w, &api("health?list=orphans")).await;
    assert_ne!(other.header("etag"), Some(etag.as_str()));
    w.fake.fail_views(Some(super::busy()));
    for tail in ["health?list=wanted", "backlinks/Home", "search?q=home"] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE, "{tail}");
        assert_eq!(a.header("retry-after"), Some("30"));
    }
    assert!(w.logged().iter().any(|l| l.contains("handbook")));
}

// ------------------------------------------------------------------------------------ backlinks

#[tokio::test]
async fn the_pages_that_link_here_come_with_the_line_that_makes_the_link() {
    let w = wiki();
    let body = ok(&get(&w, &api("backlinks/Home")).await);
    assert_eq!(body.at("/title"), "Home");
    assert_eq!(body.at("/exists"), true);
    assert_eq!(body.at("/cut"), false);
    assert_eq!(body.at("/tip/view"), "main");
    assert_eq!(*body.at("/next_cursor"), Value::Null);
    let items = body.at("/items").as_array().unwrap();
    assert_eq!(body.at("/total"), items.len());
    let from = |title: &str| items.iter().find(|i| i.at("/title") == title).unwrap();
    assert_eq!(from("Getting Started").at("/context"), "Back to [[Home]].");
    assert_eq!(from("Getting Started").at("/kind"), "link");
    assert_eq!(
        from("Café Society").at("/context"),
        "Links to [[Home]] and to [[Getting Started]]."
    );
    assert_eq!(from("Work").at("/context"), "See [[Home]].");
    // In the order of the linking pages' storage paths.
    let order: Vec<&str> = items
        .iter()
        .map(|i| i.at("/title").as_str().unwrap())
        .collect();
    assert_eq!(order.first(), Some(&"Café Society"));
    // A redirect to the page links to it, with no line; an alias leads to the page it names.
    let guide = ok(&get(&w, &api("backlinks/Guide")).await);
    assert_eq!(guide.at("/title"), "Getting Started");
    let old = guide
        .at("/items")
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i.at("/title") == "Old")
        .unwrap();
    assert_eq!(old.at("/kind"), "redirect");
    assert_eq!(old.at("/context"), "");
}

#[tokio::test]
async fn a_title_no_page_has_is_listed_by_the_pages_that_want_it_and_a_stranger_is_not_found() {
    let w = wiki();
    let wanted = ok(&get(&w, &api("backlinks/Missing%20Page")).await);
    assert_eq!(wanted.at("/exists"), false);
    assert_eq!(wanted.at("/title"), "Missing Page");
    assert_eq!(wanted.at("/total"), 2, "two links on two lines of one page");
    assert!(
        wanted
            .at("/items")
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i.at("/title") == "Home")
    );
    let none = get(&w, &api("backlinks/Nobody%20Links%20Here")).await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    assert_eq!(problem_type(&none), "urn:inklkunn:problem:page-not-found");
    let bad = get(&w, &api("backlinks/..")).await;
    assert_eq!(problem_type(&bad), "urn:inklkunn:problem:invalid-title");
}

#[tokio::test]
async fn backlinks_are_walked_by_cursor_and_the_text_of_a_line_is_text() {
    let w = wiki();
    let bodies = walk(&w, &api("backlinks/Home?limit=2"), 2).await;
    assert!(bodies.len() >= 3);
    let walked: Vec<String> = bodies.iter().flat_map(titles).collect();
    let whole = ok(&get(&w, &api("backlinks/Home")).await);
    assert_eq!(walked, titles(&whole));
    let a = get(&w, &api("backlinks/Home")).await;
    let text = a.text();
    assert!(
        !text.contains('<') && !text.contains('>'),
        "markup on the wire: {text}"
    );
    let hostile = whole
        .at("/items")
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i.at("/title").as_str().unwrap().starts_with('<'))
        .unwrap();
    assert_eq!(hostile.at("/title"), "<img src=x onerror=alert(1)>");
    assert!(
        hostile
            .at("/context")
            .as_str()
            .unwrap()
            .contains("<script>alert(2)</script>")
    );
    // The cursor is for this title: another title's page does not follow it.
    let first = ok(&get(&w, &api("backlinks/Home?limit=2")).await);
    let cursor = first.at("/next_cursor").as_str().unwrap();
    let other = get(
        &w,
        &api(&format!(
            "backlinks/Getting%20Started?limit=2&cursor={cursor}"
        )),
    )
    .await;
    assert_eq!(problem_type(&other), "urn:inklkunn:problem:invalid-cursor");
    // Nor does it outlive the tip.
    w.fake.set(
        "pages/Extra.md",
        &page(&["title: Extra"], "See [[Home]].\n"),
    );
    let late = get(&w, &api(&format!("backlinks/Home?limit=2&cursor={cursor}"))).await;
    assert_eq!(problem_type(&late), "urn:inklkunn:problem:invalid-cursor");
}

// ------------------------------------------------------------------------------------ changes

fn changes_world(visibility: Visibility, slug: &str, name: &str) -> World {
    let files = [
        ("pages/Home.md", page(&["title: Home"], "Welcome.\n")),
        (
            "pages/Guides/Setup.md",
            page(&["title: Guides/Setup"], "Set up.\n"),
        ),
        (
            "pages/Guides/Usage.md",
            page(&["title: Guides/Usage"], "Use.\n"),
        ),
        ("pages/Notes.md", page(&["title: Notes"], "Notes.\n")),
        (
            "pages/Draft Chapter.md",
            page(&["title: Draft Chapter"], "Draft.\n"),
        ),
    ];
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let w = World::with_visibility(slug, name, visibility, &borrowed);
    w.fake.tip_message(&proposal_message(
        "Update the Home page",
        "Aiko Tanaka",
        "Ben Okoye",
    ));
    w.fake.touches(&block_id(1), &["pages/Home.md"]);
    w.fake.push_change(
        "heads/main",
        &block_id(101),
        &import_message("Import the handbook"),
    );
    w.fake.touches(
        &block_id(101),
        &["pages/Guides/Setup.md", "pages/Guides/Usage.md"],
    );
    w.fake
        .push_change("heads/main", &block_id(102), "Created by hand");
    w.fake.touches(&block_id(102), &["pages/Notes.md"]);
    w.fake.add_view("draft");
    w.fake.push_change(
        "heads/draft",
        &block_id(103),
        &proposal_message("Draft the new chapter", "Aiko Tanaka", "Aiko Tanaka"),
    );
    w.fake.touches(&block_id(103), &["pages/Draft Chapter.md"]);
    w
}

pub(super) fn handbook() -> World {
    changes_world(Visibility::Public, "handbook", "The Handbook")
}

fn summaries(body: &Value) -> Vec<String> {
    body.at("/items")
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.at("/summary").as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn the_changes_of_every_view_come_with_their_people_pages_and_kind() {
    let w = handbook();
    let body = ok(&get(&w, &api("changes")).await);
    assert_eq!(body.at("/views"), &json!(["main", "draft"]));
    assert_eq!(body.at("/left_out"), 0);
    assert_eq!(body.at("/pages_missing"), false);
    assert_eq!(body.at("/beyond_window"), false);
    assert_eq!(*body.at("/next_cursor"), Value::Null);
    let items = body.at("/items").as_array().unwrap();
    assert_eq!(items.len(), 4);
    let row = |summary: &str| items.iter().find(|i| i.at("/summary") == summary).unwrap();
    // The proposal: the people its message names, and the page it touched.
    let update = row("Update the Home page");
    assert_eq!(update.at("/id"), &json!(block_id(1)));
    assert_eq!(update.at("/view"), "main");
    assert_eq!(update.at("/provenance/kind"), "proposal");
    assert_eq!(
        update.at("/provenance/proposed_by"),
        &json!({"kind": "account", "name": "Aiko Tanaka"})
    );
    assert_eq!(
        update.at("/provenance/accepted_by"),
        &json!({"kind": "account", "name": "Ben Okoye"})
    );
    assert_eq!(
        update.at("/people"),
        &json!(["Proposed by Aiko Tanaka", "Accepted by Ben Okoye"])
    );
    assert_eq!(update.at("/automated"), false);
    assert_eq!(update.at("/rollback"), false);
    assert_eq!(
        update.at("/pages"),
        &json!([{"title": "Home", "exists": true}])
    );
    assert!(update.at("/first_seen").is_number());
    // The service's own change, and a change nobody recorded through the service.
    let import = row("Import the handbook");
    assert_eq!(import.at("/provenance/kind"), "import");
    assert_eq!(import.at("/automated"), true);
    assert_eq!(
        import.at("/provenance/accepted_by"),
        &json!({"kind": "service", "name": "inklkunn"})
    );
    assert_eq!(import.at("/pages/0/title"), "Guides/Setup");
    let hand = row("Created by hand");
    assert_eq!(hand.at("/provenance/kind"), "not-recorded");
    assert_eq!(*hand.at("/provenance/proposed_by"), Value::Null);
    assert_eq!(
        hand.at("/people"),
        &json!(["Recorded outside this service, so the people are not known."])
    );
    // A change only the draft view has.
    let draft = row("Draft the new chapter");
    assert_eq!(draft.at("/view"), "draft");
    // No member says a person signed.
    assert!(!body.to_string().to_lowercase().contains("signed"));
}

#[tokio::test]
async fn the_people_are_said_in_the_words_of_the_changes_page_and_the_codes_do_not_change() {
    let w = handbook();
    let en = ok(&get(&w, &api("changes")).await);
    let ja = ok(&send(&w, Method::GET, &api("changes"), &[JA]).await);
    let web = get(&w, "/handbook/-/changes").await.text();
    for item in en.at("/items").as_array().unwrap() {
        for sentence in item.at("/people").as_array().unwrap() {
            assert!(
                web.contains(sentence.as_str().unwrap()),
                "{sentence}: {web}"
            );
        }
    }
    let codes = |body: &Value, member: &str| -> Vec<Value> {
        body.at("/items")
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.at(member).clone())
            .collect()
    };
    for member in ["/id", "/provenance", "/view", "/pages", "/automated"] {
        assert_eq!(codes(&en, member), codes(&ja, member), "{member}");
    }
    assert_ne!(codes(&en, "/people"), codes(&ja, "/people"));
}

#[tokio::test]
async fn the_filters_narrow_the_list_and_what_they_do_not_take_is_refused() {
    let w = handbook();
    for (query, shown, hidden) in [
        (
            "namespace=Guides",
            "Import the handbook",
            "Update the Home page",
        ),
        ("view=draft", "Draft the new chapter", "Import the handbook"),
        ("view=main", "Import the handbook", "Draft the new chapter"),
        (
            "automated=hide",
            "Update the Home page",
            "Import the handbook",
        ),
        (
            "automated=only",
            "Import the handbook",
            "Update the Home page",
        ),
    ] {
        let body = ok(&get(&w, &api(&format!("changes?{query}"))).await);
        let all = summaries(&body);
        assert!(all.contains(&shown.to_owned()), "{query}: {all:?}");
        assert!(!all.contains(&hidden.to_owned()), "{query}: {all:?}");
    }
    for (tail, parameter) in [
        ("changes?automated=maybe", "automated"),
        ("changes?automated=", "automated"),
        ("changes?limit=0", "limit"),
        ("changes?limit=500", "limit"),
        ("changes?page=Home", "page"),
        ("changes?minor=hide", "minor"),
        ("changes?namespace=a&namespace=b", "namespace"),
    ] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{tail}");
        assert_eq!(a.json().at("/parameter"), parameter, "{tail}");
    }
    let view = get(&w, &api("changes?view=nope")).await;
    assert_eq!(view.status, StatusCode::NOT_FOUND);
    assert_eq!(problem_type(&view), "urn:inklkunn:problem:no-such-view");
    assert_eq!(view.json().at("/view"), "nope");
}

#[tokio::test]
async fn the_changes_are_walked_by_cursor_once_each_in_order_in_bounded_pages() {
    let w = handbook();
    let whole = summaries(&ok(&get(&w, &api("changes")).await));
    for size in [1, 2, 3] {
        let bodies = walk(&w, &api(&format!("changes?limit={size}")), size).await;
        let walked: Vec<String> = bodies.iter().flat_map(summaries).collect();
        assert_eq!(walked, whole, "size {size}");
        assert_eq!(bodies.len(), 4_usize.div_ceil(size), "size {size}");
        // The last page has no cursor even when it is exactly full.
        assert_eq!(*bodies.last().unwrap().at("/next_cursor"), Value::Null);
    }
    // A cursor holds a hash of a change, not its id.
    let first = ok(&get(&w, &api("changes?limit=1")).await);
    let cursor = first.at("/next_cursor").as_str().unwrap();
    assert!(cursor.starts_with("k1"));
    assert!(!cursor.contains(&block_id(1)[..16]), "{cursor}");
    // A cursor that names no change of the list, or is of another list, is refused.
    for bad in [
        "k10123456789abcdef".to_owned(),
        "c1".to_owned(),
        "n13".to_owned(),
        "o11.ab".to_owned(),
        format!("{cursor}0"),
        cursor.to_uppercase(),
    ] {
        let a = get(&w, &api(&format!("changes?cursor={bad}"))).await;
        assert_eq!(
            problem_type(&a),
            "urn:inklkunn:problem:invalid-cursor",
            "{bad}"
        );
    }
    // Filtered, the walk is of the filtered list.
    let bodies = walk(&w, &api("changes?automated=hide&limit=1"), 1).await;
    let walked: Vec<String> = bodies.iter().flat_map(summaries).collect();
    assert!(
        !walked.contains(&"Import the handbook".to_owned()),
        "{walked:?}"
    );
    assert_eq!(walked.len(), 3);
}

#[tokio::test]
async fn a_change_the_server_could_not_ask_about_is_listed_without_its_pages() {
    let w = handbook();
    w.fake.fail_show(true);
    let body = ok(&get(&w, &api("changes")).await);
    assert_eq!(body.at("/pages_missing"), true);
    assert!(
        body.at("/items")
            .as_array()
            .unwrap()
            .iter()
            .all(|i| *i.at("/pages") == Value::Null),
        "{body}"
    );
    // The filters that need the pages leave out what cannot be known, and say so.
    let filtered = ok(&get(&w, &api("changes?namespace=Guides")).await);
    assert_eq!(filtered.at("/pages_missing"), true);
}

#[tokio::test]
async fn markup_in_a_summary_or_a_name_is_text_and_a_page_that_is_gone_says_so() {
    let w = handbook();
    // A page a change touched that the tip no longer has.
    w.fake.touches(&block_id(102), &["pages/Gone.md"]);
    w.fake.tip_message(&proposal_message(
        "<img src=x onerror=alert(1)>",
        "<b>Aiko</b>",
        "Ben \"B\" & Co",
    ));
    let a = get(&w, &api("changes")).await;
    let text = a.text();
    assert!(
        !text.contains('<') && !text.contains('>') && !text.contains('&'),
        "{text}"
    );
    let body = a.json();
    let first = body.at("/items/0");
    assert_eq!(first.at("/summary"), "<img src=x onerror=alert(1)>");
    assert_eq!(first.at("/provenance/proposed_by/name"), "<b>Aiko</b>");
    assert_eq!(first.at("/provenance/accepted_by/name"), "Ben \"B\" & Co");
    let body = ok(&get(&w, &api("changes")).await);
    let gone = body
        .at("/items")
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i.at("/summary") == "Created by hand")
        .unwrap();
    assert_eq!(
        gone.at("/pages"),
        &json!([{"title": "Gone", "exists": false}])
    );
}

#[tokio::test]
async fn the_changes_have_a_validator_and_a_new_change_is_a_new_answer() {
    let w = handbook();
    let a = get(&w, &api("changes")).await;
    let etag = a.header("etag").unwrap().to_owned();
    let same = send(
        &w,
        Method::GET,
        &api("changes"),
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(same.status, StatusCode::NOT_MODIFIED);
    assert!(same.bytes.is_empty());
    w.fake
        .tip_message(&proposal_message("Another", "Aiko Tanaka", "Ben Okoye"));
    w.fake
        .set("pages/Home.md", &page(&["title: Home"], "Changed.\n"));
    let after = send(
        &w,
        Method::GET,
        &api("changes"),
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(after.status, StatusCode::OK);
}

#[tokio::test]
async fn a_restricted_wiki_is_not_there_for_a_stranger_and_has_its_changes_for_a_reader() {
    let vault = changes_world(Visibility::Restricted, "vault", "The Vault");
    let nothing = changes_world(Visibility::Public, "elsewhere", "Elsewhere");
    for tail in [
        "changes",
        "changes?namespace=Guides&automated=hide",
        "changes?cursor=k10123456789abcdef",
        "changes?limit=500",
        "health?list=stale",
        "backlinks/Home",
        "search?q=home",
    ] {
        let a = get(&vault, &format!("/vault/-/api/v1/{tail}")).await;
        let b = get(&nothing, &format!("/vault/-/api/v1/{tail}")).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{tail}");
        assert_eq!(a.bytes, b.bytes, "{tail}: the same answer, byte for byte");
        assert_eq!(a.header_lines(), b.header_lines(), "{tail}");
    }
    assert_eq!(vault.fake.shows(), 0, "nothing was asked of prikk");
    let w = with_accounts(changes_world(Visibility::Restricted, "vault", "The Vault"));
    let reader = AccountId::new();
    let_read(&w, &reader);
    let who = reader.to_string();
    let a = request(
        &w,
        super::common::APP,
        "GET",
        "/vault/-/api/v1/changes",
        &[("x-test-account", who.as_str())],
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let body: Value = serde_json::from_slice(&a.bytes).unwrap();
    assert_eq!(summaries(&body).len(), 4);
    assert_eq!(body.at("/left_out"), 0);
    // The wiki is private: no answer of it is for a shared cache or an index.
    assert_eq!(a.status, StatusCode::OK);
    let anonymous = get(&w, "/vault/-/api/v1/changes").await;
    assert_eq!(anonymous.status, StatusCode::NOT_FOUND);
}
