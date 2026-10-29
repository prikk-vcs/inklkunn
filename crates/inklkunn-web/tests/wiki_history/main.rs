//! A page's past through the real router (RFC 003 D-4; handoff 003 T5): the history, one change, a page's
//! difference and a permalink. The repository is an in-memory double with a scripted history, so these run in
//! both test runs; the real prikk is in `real_prikk.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{
    HOME_HUNK, World, block_id, busy, diff_of, op, page, proposal_text, script_history, shown,
};
use inklkunn_prikk::Point;
use inklkunn_store::Visibility;
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site_with;
use tower::ServiceExt as _;

fn router(w: &World) -> Router {
    site_with(
        &WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
        Some(w.services.clone()),
    )
    .unwrap()
}

struct Answer {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: String,
}

async fn send(w: &World, uri: &str, extra: &[(&str, &str)]) -> Answer {
    let mut req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::HOST, "localhost:8080");
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let res = router(w)
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 22).await.unwrap();
    Answer {
        status: parts.status,
        headers: parts.headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn get(w: &World, uri: &str) -> Answer {
    send(w, uri, &[]).await
}

fn handbook() -> World {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[
            (
                "pages/Home.md",
                &page(
                    &["title: Home", "description: Start here"],
                    "# Welcome\n\nSee [[Guide]] and [[Nowhere]].\n",
                ),
            ),
            (
                "pages/Guide.md",
                &page(&["title: Guide", "aliases: [How-to]"], "The guide.\n"),
            ),
        ],
    );
    script_history(&w);
    w
}

/// `text` as the templates escape it (numeric character references).
fn esc(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' | '<' | '>' | '"' | '\'' => format!("&#{};", c as u32),
            c => c.to_string(),
        })
        .collect()
}

/// What is inside `<main>`: the language buttons in the footer echo the address asked for.
fn main_of(body: &str) -> &str {
    let from = body.find("<main").expect("a main");
    let to = body.find("</main>").expect("a main end");
    body.get(from..to).expect("main")
}

fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

fn at(haystack: &str, needle: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` is not in the page: {haystack}"))
}

/// The text of a fragment of HTML: tags removed, character references decoded.
fn text_of(fragment: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    let mut chars = fragment.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '<' => tag = true,
            '>' if tag => tag = false,
            '&' if !tag => {
                let mut reference = String::new();
                for next in chars.by_ref() {
                    if next == ';' {
                        break;
                    }
                    reference.push(next);
                }
                match reference.as_str() {
                    "amp" => out.push('&'),
                    "lt" => out.push('<'),
                    "gt" => out.push('>'),
                    "quot" => out.push('"'),
                    r => match r.strip_prefix('#').and_then(|n| n.parse::<u32>().ok()) {
                        Some(n) => out.push(char::from_u32(n).unwrap()),
                        None => panic!("unknown character reference &{r};"),
                    },
                }
            }
            c if !tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// The lines of a difference as prikk wrote them (`+`, `-`, a space or `\`, then the text), read back from the
/// page: the signs and the hidden labels are taken off, the marks are only tags, and the text is decoded.
fn lines_on(page: &str) -> Vec<String> {
    let mut out = Vec::new();
    for chunk in page.split("<div class=\"diff-line ").skip(1) {
        let (class, rest) = chunk.split_once("\">").expect("a class");
        let inner = rest.split("</div>").next().expect("a line");
        let line = match class {
            "diff-removed" => format!(
                "-{}",
                text_of(
                    &inner
                        .replace(
                            "<span class=\"diff-sign\" aria-hidden=\"true\">-</span>",
                            ""
                        )
                        .replace("<span class=\"visually-hidden\">Removed: </span>", "")
                )
            ),
            "diff-added" => format!(
                "+{}",
                text_of(
                    &inner
                        .replace(
                            "<span class=\"diff-sign\" aria-hidden=\"true\">+</span>",
                            ""
                        )
                        .replace("<span class=\"visually-hidden\">Added: </span>", "")
                )
            ),
            "diff-note" => "\\ No newline at end of file".to_owned(),
            "diff-context" => format!(" {}", text_of(inner)),
            other => panic!("a line of kind {other}"),
        };
        out.push(line);
    }
    out
}

// ------------------------------------------------------------------------------------ the history

#[tokio::test]
async fn a_pages_history_lists_the_changes_that_touched_it_newest_first() {
    let w = handbook();
    let a = get(&w, "/handbook/-/history/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("<h1>History of Home</h1>"), "{}", a.body);
    assert!(
        a.body
            .contains("<title>History of Home · The Handbook · inklkunn</title>")
    );
    let (c4, c2, c1) = (
        at(&a.body, "Change 4"),
        at(&a.body, "Change 2"),
        at(&a.body, "Change 1"),
    );
    assert!(c4 < c2 && c2 < c1, "newest first");
    assert!(
        !a.body.contains("Change 3"),
        "change 3 touched only the guide"
    );
    // The row of the proposal: its summary, who proposed and who accepted, what it did to the page.
    assert!(a.body.contains(": Improve the welcome"));
    assert!(a.body.contains("Proposed by Mika Tanaka"));
    assert!(a.body.contains("Accepted by Ren Sato"));
    assert!(a.body.contains("The page was edited"));
    assert!(a.body.contains("The page was created"));
    // A message someone else wrote names nobody, and its control character is gone.
    assert!(
        a.body
            .contains("The record of this change does not say who made it.")
    );
    assert!(a.body.contains(": fixup : a plain message"), "{}", a.body);
    assert!(!a.body.contains('\u{7}'));
    // The links: the change, what it did to the page (from the block before), the page after it.
    assert!(
        a.body
            .contains(&format!("href=\"/handbook/-/change/{}\"", block_id(4)))
    );
    assert!(a.body.contains(&format!(
        "href=\"/handbook/-/diff/{}/{}/Home\"",
        block_id(3),
        block_id(4)
    )));
    assert!(
        a.body
            .contains(&format!("href=\"/handbook/-/at/{}/Home\"", block_id(2)))
    );
    assert!(
        a.body.contains("href=\"/handbook/Home\""),
        "back to the page"
    );
    // A history is a list of changes, not of signatures or checks: it claims neither.
    assert!(!a.body.contains("Signed"), "{}", a.body);
    assert!(!a.body.contains("Verified"));
    assert_eq!(count(&a.body, "<h1"), 1);
    assert!(a.headers.contains_key(header::CONTENT_SECURITY_POLICY));
}

#[tokio::test]
async fn a_blocks_operations_are_asked_for_once_and_a_page_with_no_change_says_so() {
    let w = handbook();
    let _ = get(&w, "/handbook/-/history/Home").await;
    let first = w.fake.shows_asked();
    assert_eq!(first, 4, "one `show` for each block");
    let again = get(&w, "/handbook/-/history/Home").await;
    assert_eq!(again.status, StatusCode::OK);
    let other = get(&w, "/handbook/-/history/Guide").await;
    assert_eq!(other.status, StatusCode::OK);
    assert_eq!(w.fake.shows_asked(), first, "a block never changes");
    let none = get(&w, "/handbook/-/history/Never%20Written").await;
    assert_eq!(none.status, StatusCode::OK);
    assert!(
        none.body.contains("No change to this page was found."),
        "{}",
        none.body
    );
}

#[tokio::test]
async fn a_long_history_is_read_in_steps_the_reader_can_follow() {
    let w = handbook();
    for v in 5..=44_u64 {
        w.fake.set_files(&[(
            "pages/Guide.md",
            &page(&["title: Guide"], &format!("v{v}.\n")),
        )]);
        w.fake.say(v, "Edit the guide");
        w.fake
            .show(v, shown(vec![op("edit-text", &["pages/Guide.md"])]));
    }
    let a = get(&w, "/handbook/-/history/Home").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(
        a.body
            .contains("No change to this page was found among those read so far."),
        "{}",
        a.body
    );
    let marker = "href=\"/handbook/-/history/Home?after=";
    let start = at(&a.body, marker);
    let rest = a.body.get(start + 6..).unwrap();
    let link = rest.split('"').next().unwrap().to_owned();
    // Following the link goes on from where the step stopped, to the changes the first step did not reach.
    let next = get(&w, &link).await;
    assert_eq!(next.status, StatusCode::OK, "{}", next.body);
    assert!(
        next.body.contains("Change 4") || next.body.contains("Change 1"),
        "{}",
        next.body
    );
    // The reads were bounded: a request never asks about more than its share of blocks.
    assert!(
        w.fake.shows_asked() <= 2 * 24 + 4,
        "{}",
        w.fake.shows_asked()
    );
}

#[tokio::test]
async fn a_summary_that_is_markup_is_text() {
    let w = handbook();
    w.fake.say(4, "<img src=x onerror=alert(1)>");
    let a = get(&w, "/handbook/-/history/Home").await;
    assert!(
        a.body.contains(&esc("<img src=x onerror=alert(1)>")),
        "{}",
        a.body
    );
    assert!(!a.body.contains("<img"));
}

// ------------------------------------------------------------------------------------ a change

#[tokio::test]
async fn a_change_page_says_signed_into_the_history_and_who_the_service_signed_for() {
    let w = handbook();
    let a = get(&w, &format!("/handbook/-/change/{}", block_id(2))).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("<h1>Change 2</h1>"));
    assert!(
        a.body
            .contains("<p class=\"signed\">Signed into the history</p>")
    );
    assert!(
        a.body
            .contains("Signed by the wiki service on behalf of Mika Tanaka, accepted by Ren Sato."),
        "{}",
        a.body
    );
    assert!(a.body.contains("Improve the welcome"));
    assert!(
        a.body
            .contains("Edited the page <a href=\"/handbook/Home\">Home</a>")
    );
    assert!(a.body.contains(&format!(
        "href=\"/handbook/-/diff/{}/{}/Home\"",
        block_id(1),
        block_id(2)
    )));
    // A person is never said to have signed.
    for forbidden in [
        "Mika Tanaka signed",
        "Ren Sato signed",
        "signed by Mika",
        "signed by Ren",
        "Signed by Mika",
        "Signed by Ren",
    ] {
        assert!(!a.body.contains(forbidden), "{forbidden}");
    }
    // It is not a verification either.
    assert!(!a.body.contains("Verified"));
}

#[tokio::test]
async fn a_change_the_service_made_itself_and_one_nobody_is_named_for_say_only_what_is_known() {
    let w = handbook();
    let service = get(&w, &format!("/handbook/-/change/{}", block_id(3))).await;
    assert!(service.body.contains("Signed into the history"));
    assert!(service.body.contains("Signed by the wiki service."));
    assert!(!service.body.contains("on behalf of"));
    assert!(service.body.contains("Made by the wiki service"));
    assert!(service.body.contains("Import the guide"));
    // A file that is not a page is not named: its storage path is not an address of this site.
    assert!(
        service
            .body
            .contains("Added an attachment or another file that is not a page")
    );
    assert!(!service.body.contains("diagram.svg"));
    assert!(!service.body.contains("attachments/"));

    let foreign = get(&w, &format!("/handbook/-/change/{}", block_id(4))).await;
    assert!(foreign.body.contains("Signed into the history"));
    assert!(
        foreign
            .body
            .contains("The record of this change does not say who made it.")
    );
    assert!(!foreign.body.contains("on behalf of"));
    assert!(!foreign.body.contains("wiki service"));
    assert!(!foreign.body.contains("Proposed by"));
}

#[tokio::test]
async fn a_name_that_is_markup_is_text_on_the_change_page() {
    let w = handbook();
    w.fake.say(
        2,
        &proposal_text("Improve", "<b>Mika</b>", "Ren \"the\" Sato"),
    );
    let a = get(&w, &format!("/handbook/-/change/{}", block_id(2))).await;
    assert!(a.body.contains(&esc("<b>Mika</b>")), "{}", a.body);
    assert!(!a.body.contains("<b>Mika"));
    assert!(a.body.contains(&esc("Ren \"the\" Sato")), "{}", a.body);
}

#[tokio::test]
async fn a_change_that_is_not_in_the_history_is_one_answer_whatever_the_reason() {
    let w = handbook();
    let unknown = get(&w, &format!("/handbook/-/change/{}", "e".repeat(64))).await;
    let malformed = get(&w, "/handbook/-/change/not-a-block").await;
    let upper = get(
        &w,
        &format!("/handbook/-/change/{}", block_id(2).to_uppercase()),
    )
    .await;
    for a in [&unknown, &malformed, &upper] {
        assert_eq!(a.status, StatusCode::NOT_FOUND);
        assert!(a.body.contains("There is no such change"), "{}", a.body);
        assert_eq!(count(&a.body, "<h1"), 1);
    }
    assert_eq!(main_of(&unknown.body), main_of(&malformed.body));
    assert_eq!(main_of(&unknown.body), main_of(&upper.body));
}

#[tokio::test]
async fn a_change_too_large_to_describe_says_so_instead_of_listing_nothing() {
    let w = handbook();
    w.fake.show(
        3,
        Err(inklkunn_prikk::StorageError::Unavailable {
            verb: "show",
            reason: inklkunn_prikk::Unavailable::OutputExceeded,
        }),
    );
    let a = get(&w, &format!("/handbook/-/change/{}", block_id(3))).await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(
        a.body
            .contains("This change is too large to list what it did.")
    );
    // And in a page's history it is listed, not skipped.
    let h = get(&w, "/handbook/-/history/Guide").await;
    assert!(
        h.body
            .contains("too large to list what it did, so it may have touched this page"),
        "{}",
        h.body
    );
}

// ------------------------------------------------------------------------------------ a difference

#[tokio::test]
async fn a_diff_request_issues_exactly_one_diff_and_always_with_the_pages_path() {
    let w = handbook();
    let uri = format!("/handbook/-/diff/{}/{}/Home", block_id(1), block_id(2));
    let a = get(&w, &uri).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    let asked = w.fake.diffs_asked();
    assert_eq!(asked.len(), 1, "exactly one diff");
    assert_eq!(
        asked.first().unwrap(),
        &(
            Point::Block(block_id(1)),
            Point::Block(block_id(2)),
            vec!["pages/Home.md".to_owned()]
        )
    );
    // By an alias, the path is the page's own; by a title nobody has, it is still one page's path.
    w.fake
        .diff_says(Ok(diff_of("pages/Guide.md", "modified", &[], 1, 3)));
    let _ = get(
        &w,
        &format!("/handbook/-/diff/{}/{}/How-to", block_id(1), block_id(3)),
    )
    .await;
    let _ = get(
        &w,
        &format!(
            "/handbook/-/diff/{}/{}/Nothing%20Here",
            block_id(1),
            block_id(3)
        ),
    )
    .await;
    let asked = w.fake.diffs_asked();
    assert_eq!(asked.len(), 3);
    for (_, _, paths) in &asked {
        assert_eq!(paths.len(), 1, "a path, never the whole tree: {paths:?}");
    }
    assert_eq!(asked.get(1).unwrap().2, vec!["pages/Guide.md".to_owned()]);
    assert_eq!(
        asked.get(2).unwrap().2,
        vec!["pages/Nothing Here.md".to_owned()]
    );
}

#[tokio::test]
async fn a_malformed_block_never_reaches_prikk() {
    let w = handbook();
    for uri in [
        "/handbook/-/diff/--all/00/Home".to_owned(),
        format!(
            "/handbook/-/diff/{}/{}/Home",
            block_id(1).to_uppercase(),
            block_id(2)
        ),
        format!("/handbook/-/diff/{}/heads%2Fmain/Home", block_id(1)),
    ] {
        let a = get(&w, &uri).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{uri}");
        assert!(a.body.contains("There is no such change"), "{uri}");
    }
    assert!(w.fake.diffs_asked().is_empty());
}

#[tokio::test]
async fn the_lines_are_prikks_the_marks_are_inkklkunns_and_ins_and_del_carry_text_alternatives() {
    let w = handbook();
    let a = get(
        &w,
        &format!("/handbook/-/diff/{}/{}/Home", block_id(1), block_id(2)),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("<h1>Changes to Home</h1>"));
    // The text summary.
    assert!(a.body.contains("3 lines added"), "{}", a.body);
    assert!(a.body.contains("2 lines removed"));
    assert!(a.body.contains("From <a href=\"/handbook/-/change/"));
    // Every ins and every del begins with its hidden text alternative.
    let ins = count(&a.body, "<ins>");
    let del = count(&a.body, "<del>");
    assert_eq!((ins, del), (3, 2));
    assert_eq!(
        count(
            &a.body,
            "<ins><span class=\"visually-hidden\">Added: </span>"
        ),
        ins
    );
    assert_eq!(
        count(
            &a.body,
            "<del><span class=\"visually-hidden\">Removed: </span>"
        ),
        del
    );
    // The word-level marks are there, and the page says whose they are.
    assert!(a.body.contains("<mark>"), "{}", a.body);
    assert!(a.body.contains("Marked words are found by inklkunn"));
    assert!(a.body.contains("The lines themselves come from prikk."));
    // The round trip: the page's lines, read back, are exactly the hunk prikk gave (marks add nothing).
    let hunk_lines: Vec<String> = HOME_HUNK.lines().skip(1).map(str::to_owned).collect();
    assert_eq!(lines_on(&a.body), hunk_lines);
    assert!(a.body.contains("Changes near line 6"));
    assert!(a.body.contains("The text ends here without a line break."));
}

#[tokio::test]
async fn markup_in_a_diff_is_text() {
    let w = handbook();
    let a = get(
        &w,
        &format!("/handbook/-/diff/{}/{}/Home", block_id(1), block_id(2)),
    )
    .await;
    assert!(!a.body.contains("<script"), "{}", a.body);
    assert!(
        lines_on(&a.body)
            .iter()
            .any(|l| l.contains("<script>alert(1)</script>"))
    );
    assert!(
        a.body.contains("&#60;"),
        "angle brackets are escaped, never passed on"
    );
    assert!(lines_on(&a.body).iter().any(|l| l == "+A new line & more."));
    // A hunk that is not a unified hunk is shown as the text it is, escaped.
    w.fake.diff_says(Ok(diff_of(
        "pages/Home.md",
        "modified",
        &["<img src=x onerror=alert(1)>\nnot a hunk"],
        1,
        2,
    )));
    let raw = get(
        &w,
        &format!("/handbook/-/diff/{}/{}/Home", block_id(1), block_id(2)),
    )
    .await;
    assert!(raw.body.contains("could not be read as a list of changes"));
    assert!(raw.body.contains(&esc("<img src=x onerror=alert(1)>")));
    assert!(!raw.body.contains("<img"));
}

#[tokio::test]
async fn a_page_that_is_new_removed_unchanged_or_not_text_is_told_so() {
    let w = handbook();
    let uri = format!("/handbook/-/diff/{}/{}/Home", block_id(1), block_id(2));
    for (status, hunks, said) in [
        (
            "added",
            vec!["@@ -0,0 +1 @@\n+first\n"],
            "did not exist at the first point and exists at the second",
        ),
        (
            "removed",
            vec!["@@ -1 +0,0 @@\n-gone\n"],
            "existed at the first point and does not exist at the second",
        ),
        (
            "modified",
            vec![],
            "The lines of this page cannot be shown.",
        ),
    ] {
        w.fake
            .diff_says(Ok(diff_of("pages/Home.md", status, &hunks, 1, 2)));
        let a = get(&w, &uri).await;
        assert_eq!(a.status, StatusCode::OK);
        assert!(a.body.contains(said), "{status}: {}", a.body);
    }
    let mut none = diff_of("pages/Home.md", "modified", &[], 1, 2);
    none.entries.clear();
    w.fake.diff_says(Ok(none));
    let a = get(&w, &uri).await;
    assert!(a.body.contains("The page is the same at both points."));
    assert!(a.body.contains("No line differs."));
}

// ------------------------------------------------------------------------------------ a permalink

#[tokio::test]
async fn a_permalink_shows_the_page_as_it_was_with_a_banner_that_links_to_the_current_page() {
    let w = handbook();
    let a = get(&w, &format!("/handbook/-/at/{}/Home", block_id(2))).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body);
    assert!(a.body.contains("Second."), "the page after change 2");
    assert!(!a.body.contains("Third."), "not the page as it is now");
    assert!(
        a.body
            .contains("<title>Home after change 2 · The Handbook · inklkunn</title>")
    );
    let start = at(&a.body, "<div class=\"at-banner\"");
    let banner = a.body.get(start..).unwrap().split("</div>").next().unwrap();
    assert!(
        banner.contains("This is the page as it was after <a href=\"/handbook/-/change/"),
        "{banner}"
    );
    assert!(banner.contains("Change 2</a>"));
    assert!(
        banner.contains("<a href=\"/handbook/Home\">the current version of this page</a>"),
        "the banner links to the current page: {banner}"
    );
    assert!(
        banner.contains(
            "<p>The links in this page lead to the pages as they are now, not as they were then.</p>"
        ),
        "the banner says where the page's links lead: {banner}"
    );
    // The page as it is now offers a change; the page as it was does not.
    assert!(!a.body.contains("Propose a change"));
    let now = get(&w, "/handbook/Home").await;
    assert!(now.body.contains("Propose a change"));
    assert!(!now.body.contains("at-banner"));
    assert_eq!(count(&a.body, "<h1"), 1);
    // The page now links its history, and a permalink does too.
    assert!(now.body.contains("href=\"/handbook/-/history/Home\""));
    assert!(a.body.contains("href=\"/handbook/-/history/Home\""));
}

#[tokio::test]
async fn a_permalink_before_the_page_existed_or_to_no_change_is_a_page_that_says_so() {
    let w = handbook();
    w.fake
        .set_files(&[("pages/Fresh.md", &page(&["title: Fresh"], "New.\n"))]);
    w.fake.say(5, "Add Fresh");
    w.fake
        .show(5, shown(vec![op("create-file", &["pages/Fresh.md"])]));
    let early = get(&w, &format!("/handbook/-/at/{}/Fresh", block_id(1))).await;
    assert_eq!(early.status, StatusCode::NOT_FOUND);
    assert!(early.body.contains("The page did not exist then"));
    assert!(
        early.body.contains("this wiki had no page titled Fresh"),
        "{}",
        early.body
    );
    let later = get(&w, &format!("/handbook/-/at/{}/Fresh", block_id(5))).await;
    assert_eq!(later.status, StatusCode::OK);
    let none = get(&w, &format!("/handbook/-/at/{}/Home", "f".repeat(64))).await;
    assert_eq!(none.status, StatusCode::NOT_FOUND);
    assert!(none.body.contains("There is no such change"));
}

// ------------------------------------------------------------------------------------ views, languages, failures

#[tokio::test]
async fn a_view_is_carried_in_every_link_and_an_unknown_view_is_a_page() {
    let w = handbook();
    let a = get(&w, "/handbook/-/history/Home?view=main").await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(a.body.contains("You are reading the view main."));
    assert!(a.body.contains(&format!(
        "href=\"/handbook/-/change/{}?view=main\"",
        block_id(4)
    )));
    assert!(a.body.contains("href=\"/handbook/~main/Home\""));
    for bad in ["nope", "..%2Fx", "-x"] {
        let a = get(&w, &format!("/handbook/-/history/Home?view={bad}")).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{bad}");
        assert_eq!(count(&a.body, "<h1"), 1);
    }
}

#[tokio::test]
async fn a_busy_prikk_is_a_page_that_asks_to_try_again() {
    let w = handbook();
    w.fake.fail_views(Some(busy()));
    for uri in [
        "/handbook/-/history/Home".to_owned(),
        format!("/handbook/-/change/{}", block_id(2)),
        format!("/handbook/-/diff/{}/{}/Home", block_id(1), block_id(2)),
        format!("/handbook/-/at/{}/Home", block_id(2)),
    ] {
        let a = get(&w, &uri).await;
        assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE, "{uri}");
        assert_eq!(
            a.headers
                .get(header::RETRY_AFTER)
                .map(|v| v.to_str().unwrap()),
            Some("30")
        );
    }
    assert!(!w.logged().is_empty(), "the operator is told why");
}

#[tokio::test]
async fn the_pages_are_in_the_readers_language() {
    let w = handbook();
    let ja = [("cookie", "inklkunn_lang=ja")];
    let h = send(&w, "/handbook/-/history/Home", &ja).await;
    assert!(h.body.contains("<html lang=\"ja\""));
    assert!(h.body.contains("<h1>Home の履歴</h1>"), "{}", h.body);
    assert!(h.body.contains("提案者: Mika Tanaka"));
    let c = send(&w, &format!("/handbook/-/change/{}", block_id(2)), &ja).await;
    assert!(c.body.contains("署名して履歴に記録済み"));
    assert!(
        c.body
            .contains("ウィキサービスが Mika Tanaka に代わって署名し、Ren Sato が受け入れました。")
    );
    let d = send(
        &w,
        &format!("/handbook/-/diff/{}/{}/Home", block_id(1), block_id(2)),
        &ja,
    )
    .await;
    assert!(
        d.body
            .contains("<span class=\"visually-hidden\">追加: </span>")
    );
    assert!(d.body.contains("3 行追加"));
}

// ------------------------------------------------------------------------------------ privacy

#[tokio::test]
async fn a_private_wiki_and_a_wiki_that_is_not_there_are_the_same_answer_and_nothing_is_asked_of_prikk()
 {
    let w = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &page(&["title: Home"], "hidden\n"))],
    );
    script_history(&w);
    for path in [
        "/{}/-/history/Home".to_owned(),
        "/{}/-/history/Home?view=main".to_owned(),
        format!("/{{}}/-/change/{}", block_id(2)),
        format!("/{{}}/-/diff/{}/{}/Home", block_id(1), block_id(2)),
        format!("/{{}}/-/at/{}/Home", block_id(2)),
        "/{}/-/at/nothing/Home".to_owned(),
    ] {
        let real = get(&w, &path.replace("{}", "secret")).await;
        let absent = get(&w, &path.replace("{}", "no-such-wiki")).await;
        assert_eq!(real.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(real.status, absent.status, "{path}");
        assert_eq!(
            real.body.replace("secret", "X"),
            absent.body.replace("no-such-wiki", "X"),
            "{path}: the answers differ"
        );
        assert!(!real.body.contains("Secret"), "{path}");
        assert!(!real.body.contains("Change"), "{path}");
    }
    assert_eq!(w.fake.shows_asked(), 0, "no block was described");
    assert!(w.fake.diffs_asked().is_empty(), "no diff was asked");
}
