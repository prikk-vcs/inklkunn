//! The endpoints about a page's past through the real router (RFC 003 D-13, D-4; handoff 003 T8): the history
//! by cursor, one change with the page as it stood, and a page's difference. They call the very reads the
//! history pages call, so these tests hold the two to the same facts: the people a change names, the one `diff`
//! with the page's path, the lines being the version control's own.

use axum::http::{Method, StatusCode};
use inklkunn_prikk::Point;
use serde_json::{Value, json};

use super::common::{
    HOME_HUNK, World, block_id, diff_of, op, page, proposal_text, script_history, shown,
};
use super::{Answer, At, JA, get, problem_type, send};

pub(super) fn handbook() -> World {
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

fn api(tail: &str) -> String {
    format!("/handbook/-/api/v1/{tail}")
}

fn numbers(body: &Value) -> Vec<u64> {
    body.at("/items")
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.at("/number").as_u64().unwrap())
        .collect()
}

fn ok(a: &Answer) -> Value {
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    assert_eq!(a.header("content-type"), Some("application/json"));
    a.json()
}

// ------------------------------------------------------------------------------------ the history

#[tokio::test]
async fn a_pages_history_lists_the_changes_that_touched_it_newest_first_with_what_each_did() {
    let w = handbook();
    let body = ok(&get(&w, &api("history/Home")).await);
    assert_eq!(body.at("/title"), "Home");
    assert_eq!(body.at("/tip/view"), "main");
    assert_eq!(numbers(&body), [4, 2, 1], "change 3 touched only the guide");
    assert_eq!(*body.at("/next_cursor"), Value::Null);
    assert_eq!(body.at("/beyond_window"), false);
    let items = body.at("/items").as_array().unwrap();
    let ids: Vec<&str> = items
        .iter()
        .map(|i| i.at("/id").as_str().unwrap())
        .collect();
    assert_eq!(ids, [block_id(4), block_id(2), block_id(1)]);
    // What each did to the page, and the change before it (the `from` of a diff).
    assert_eq!(
        items.first().unwrap().at("/events"),
        &json!([{"kind": "edited"}])
    );
    assert_eq!(items.first().unwrap().at("/parent"), &json!(block_id(3)));
    assert_eq!(
        items.get(2).unwrap().at("/events"),
        &json!([{"kind": "created"}])
    );
    assert_eq!(*items.get(2).unwrap().at("/parent"), Value::Null);
    assert_eq!(items.get(2).unwrap().at("/first"), true);
    assert_eq!(items.get(2).unwrap().at("/flags"), &json!(["first"]));
    assert_eq!(items.get(1).unwrap().at("/page_title"), "Home");
}

#[tokio::test]
async fn the_people_of_a_change_are_what_its_message_says_and_never_a_signature() {
    let w = handbook();
    let body = ok(&get(&w, &api("history/Home")).await);
    let items = body.at("/items").as_array().unwrap();
    // The accepted proposal: the two people the message names.
    let proposal = items.get(1).unwrap();
    assert_eq!(proposal.at("/summary"), "Improve the welcome");
    assert_eq!(proposal.at("/provenance/kind"), "proposal");
    assert_eq!(
        proposal.at("/provenance/proposed_by"),
        &json!({"kind": "account", "name": "Mika Tanaka"})
    );
    assert_eq!(
        proposal.at("/provenance/accepted_by"),
        &json!({"kind": "account", "name": "Ren Sato"})
    );
    assert_eq!(
        proposal.at("/people"),
        &json!(["Proposed by Mika Tanaka", "Accepted by Ren Sato"])
    );
    // A message that is not in the service's form names nobody, and nobody is assumed.
    let foreign = items.first().unwrap();
    assert_eq!(foreign.at("/provenance/kind"), "not-recorded");
    assert_eq!(*foreign.at("/provenance/proposed_by"), Value::Null);
    assert_eq!(*foreign.at("/provenance/accepted_by"), Value::Null);
    assert_eq!(
        foreign.at("/people"),
        &json!(["The record of this change does not say who made it."])
    );
    // The control character of that summary is gone; the rest is text.
    assert_eq!(foreign.at("/summary"), "fixup : a plain message");
    // The change the service made itself, in the guide's history.
    let guide = ok(&get(&w, &api("history/Guide")).await);
    let import = guide
        .at("/items")
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i.at("/number") == 3)
        .unwrap();
    assert_eq!(import.at("/provenance/kind"), "system");
    assert_eq!(import.at("/flags"), &json!(["service"]));
    assert_eq!(
        import.at("/provenance/proposed_by"),
        &json!({"kind": "service", "name": Value::Null})
    );
    // No member of any shape says a person signed: the service signs the history.
    let all = [body.to_string(), guide.to_string()]
        .join(" ")
        .to_lowercase();
    assert!(!all.contains("signed"), "{all}");
    assert!(
        !all.contains("\"signer\"") && !all.contains("signed_by"),
        "{all}"
    );
}

#[tokio::test]
async fn the_sentences_about_the_people_are_the_ones_the_pages_say() {
    let w = handbook();
    let history = ok(&get(&w, &api("history/Home")).await);
    let page = get(&w, "/handbook/-/history/Home").await;
    let page = page.text();
    for item in history.at("/items").as_array().unwrap() {
        for sentence in item.at("/people").as_array().unwrap() {
            let sentence = sentence.as_str().unwrap();
            assert!(page.contains(sentence), "{sentence}: {page}");
        }
    }
    let change = get(&w, &api(&format!("revisions/{}", block_id(2)))).await;
    let web = get(&w, &format!("/handbook/-/change/{}", block_id(2))).await;
    for sentence in ok(&change).at("/people").as_array().unwrap() {
        assert!(web.text().contains(sentence.as_str().unwrap()));
    }
}

#[tokio::test]
async fn the_codes_are_the_same_in_every_language_and_the_sentences_are_not() {
    let w = handbook();
    let en = ok(&get(&w, &api("history/Home")).await);
    let ja_answer = send(&w, Method::GET, &api("history/Home"), &[JA]).await;
    let ja = ok(&ja_answer);
    assert_eq!(ja_answer.header("content-language"), Some("ja"));
    let (en_items, ja_items) = (
        en.at("/items").as_array().unwrap(),
        ja.at("/items").as_array().unwrap(),
    );
    assert_eq!(en_items.len(), ja_items.len());
    for (a, b) in en_items.iter().zip(ja_items) {
        for member in ["id", "number", "provenance", "flags", "events", "summary"] {
            assert_eq!(
                a.at(&format!("/{member}")),
                b.at(&format!("/{member}")),
                "{member}"
            );
        }
        assert_ne!(a.at("/people"), b.at("/people"));
    }
}

#[tokio::test]
async fn a_summary_or_a_name_that_is_markup_is_text_in_every_answer() {
    let w = handbook();
    w.fake.say(4, "<img src=x onerror=alert(1)>");
    w.fake.say(
        2,
        &proposal_text(
            "<script>alert(2)</script>",
            "<b>Mika</b>",
            "Ren \"the\" Sato",
        ),
    );
    let a = get(&w, &api("history/Home")).await;
    let body = ok(&a);
    assert_eq!(
        body.at("/items/0/summary"),
        "<img src=x onerror=alert(1)>",
        "the text, whole"
    );
    assert_eq!(
        body.at("/items/1/provenance/proposed_by/name"),
        "<b>Mika</b>"
    );
    assert_eq!(body.at("/items/1/summary"), "<script>alert(2)</script>");
    // On the wire the markup is not markup: `<`, `>` and `&` are JSON escapes.
    let text = a.text();
    assert!(
        !text.contains('<') && !text.contains('>') && !text.contains('&'),
        "{text}"
    );
    assert!(
        text.contains("\\u003cimg src=x onerror=alert(1)\\u003e"),
        "{text}"
    );
    // The same in the change's own answer.
    let change = get(&w, &api(&format!("revisions/{}", block_id(2)))).await;
    let text = change.text();
    assert!(!text.contains('<') && !text.contains('&'), "{text}");
    assert_eq!(
        ok(&change).at("/provenance/proposed_by/name"),
        "<b>Mika</b>"
    );
}

#[tokio::test]
async fn a_long_history_is_walked_by_cursor_in_steps_of_at_most_twenty_rows() {
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
    // Home's three changes are far back: the first step finds none and says where to go on from.
    let first = ok(&get(&w, &api("history/Home")).await);
    assert!(first.at("/items").as_array().unwrap().len() <= 20);
    let mut seen: Vec<u64> = numbers(&first);
    let mut cursor = first.at("/next_cursor").as_str().map(str::to_owned);
    assert!(cursor.is_some(), "{first}");
    let mut steps = 1;
    while let Some(next) = cursor {
        let body = ok(&get(&w, &api(&format!("history/Home?cursor={next}"))).await);
        assert!(body.at("/items").as_array().unwrap().len() <= 20);
        seen.extend(numbers(&body));
        cursor = body.at("/next_cursor").as_str().map(str::to_owned);
        steps += 1;
        assert!(steps < 20, "the walk does not end");
    }
    assert_eq!(seen, [4, 2, 1], "every change once, in order");
    // Every request asks about its own share of changes only.
    assert!(
        w.fake.shows_asked() <= steps * 24 + 4,
        "{}",
        w.fake.shows_asked()
    );
    // A page with twenty rows or more is cut into steps of twenty.
    let guide = ok(&get(&w, &api("history/Guide")).await);
    assert_eq!(guide.at("/items").as_array().unwrap().len(), 20);
    assert!(guide.at("/next_cursor").is_string());
}

#[tokio::test]
async fn a_cursor_of_another_list_or_of_no_list_is_refused() {
    let w = handbook();
    let pages = ok(&get(&w, &api("pages?limit=1")).await);
    let page_cursor = pages.at("/next_cursor").as_str().unwrap().to_owned();
    for bad in [
        page_cursor.as_str(),
        "zz",
        "n1",
        "n10",
        "n1x",
        "n105",
        "n1-5",
        "o13.abc",
        "k10123456789abcdef",
    ] {
        let a = get(&w, &api(&format!("history/Home?cursor={bad}"))).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{bad}");
        assert_eq!(
            problem_type(&a),
            "urn:inklkunn:problem:invalid-cursor",
            "{bad}"
        );
    }
    // And the other way: a history's cursor is not a position in the list of pages.
    let history_cursor = "n13";
    let a = get(&w, &api(&format!("pages?cursor={history_cursor}"))).await;
    assert_eq!(problem_type(&a), "urn:inklkunn:problem:invalid-cursor");
    // The history takes no `limit`: a step is always the same size.
    let a = get(&w, &api("history/Home?limit=5")).await;
    assert_eq!(problem_type(&a), "urn:inklkunn:problem:invalid-parameter");
}

#[tokio::test]
async fn a_title_that_is_not_a_page_has_an_empty_history_and_a_bad_title_is_a_problem() {
    let w = handbook();
    let none = ok(&get(&w, &api("history/Never%20Written")).await);
    assert_eq!(none.at("/items"), &json!([]));
    assert_eq!(*none.at("/next_cursor"), Value::Null);
    let alias = ok(&get(&w, &api("history/How-to")).await);
    assert_eq!(alias.at("/title"), "Guide", "an alias leads to its page");
    let bad = get(&w, &api("history/..")).await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    assert_eq!(problem_type(&bad), "urn:inklkunn:problem:invalid-title");
    let view = get(&w, &api("history/Home?view=nope")).await;
    assert_eq!(view.status, StatusCode::NOT_FOUND);
    assert_eq!(problem_type(&view), "urn:inklkunn:problem:no-such-view");
    assert_eq!(view.json().at("/view"), "nope");
}

#[tokio::test]
async fn several_pages_with_the_title_are_listed_as_candidates() {
    let w = World::new(
        "twins",
        "Twins",
        &[
            ("pages/A/Same.md", &page(&["title: A/Same"], "x\n")),
            ("pages/B/Same.md", &page(&["title: B/Same"], "x\n")),
        ],
    );
    let point = block_id(1);
    for tail in [
        "history/Same".to_owned(),
        format!("diff?path=Same&from={point}&to={point}"),
    ] {
        let a = get(&w, &format!("/twins/-/api/v1/{tail}")).await;
        assert_eq!(
            a.status,
            StatusCode::MULTIPLE_CHOICES,
            "{tail}: {}",
            a.text()
        );
        assert_eq!(problem_type(&a), "urn:inklkunn:problem:ambiguous-title");
        assert_eq!(*a.json().at("/candidates"), json!(["A/Same", "B/Same"]));
    }
}

#[tokio::test]
async fn the_history_has_a_validator_and_a_new_change_is_a_new_answer() {
    let w = handbook();
    let a = get(&w, &api("history/Home")).await;
    let etag = a.header("etag").unwrap().to_owned();
    let same = send(
        &w,
        Method::GET,
        &api("history/Home"),
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(same.status, StatusCode::NOT_MODIFIED);
    assert!(same.bytes.is_empty());
    assert_eq!(same.header("etag"), Some(etag.as_str()));
    // The page is changed: the history is another document, and the old validator no longer matches.
    w.fake.set_files(&[(
        "pages/Home.md",
        &page(&["title: Home", "description: Start here"], "Fifth.\n"),
    )]);
    w.fake.say(5, "Another edit");
    w.fake
        .show(5, shown(vec![op("edit-text", &["pages/Home.md"])]));
    let changed = send(
        &w,
        Method::GET,
        &api("history/Home"),
        &[("if-none-match", &etag)],
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_ne!(changed.header("etag"), Some(etag.as_str()));
    assert_eq!(numbers(&changed.json()), [5, 4, 2, 1]);
}

#[tokio::test]
async fn a_busy_prikk_is_unavailable_with_a_retry_and_a_cause_for_the_operator() {
    let w = handbook();
    w.fake.fail_views(Some(super::busy()));
    for tail in [
        "history/Home".to_owned(),
        format!("revisions/{}", block_id(2)),
        "diff?path=Home".to_owned(),
    ] {
        let a = get(&w, &api(&tail)).await;
        assert_eq!(a.status, StatusCode::SERVICE_UNAVAILABLE, "{tail}");
        assert_eq!(a.header("retry-after"), Some("30"));
        assert_eq!(problem_type(&a), "urn:inklkunn:problem:unavailable");
    }
    assert!(
        w.logged().iter().any(|l| l.contains("handbook")),
        "{:?}",
        w.logged()
    );
}

// ------------------------------------------------------------------------------------ one change

#[tokio::test]
async fn one_change_says_what_it_did_to_pages_and_to_files_it_does_not_name() {
    let w = handbook();
    let body = ok(&get(&w, &api(&format!("revisions/{}", block_id(3)))).await);
    assert_eq!(body.at("/id"), &json!(block_id(3)));
    assert_eq!(body.at("/number"), 3);
    assert_eq!(body.at("/parent"), &json!(block_id(2)));
    assert_eq!(body.at("/view"), "main");
    assert_eq!(body.at("/summary"), "Import the guide");
    assert_eq!(body.at("/provenance/kind"), "system");
    assert_eq!(
        body.at("/did"),
        &json!([
            {"action": "edited", "subject": {"kind": "page", "title": "Guide"}},
            {"action": "added", "subject": {"kind": "file", "title": Value::Null}},
        ])
    );
    assert_eq!(body.at("/omitted"), 0);
    assert_eq!(body.at("/too_large"), false);
    assert_eq!(*body.at("/page"), Value::Null);
    // A move names both sides.
    w.fake.show(
        2,
        shown(vec![op(
            "rename-path",
            &["pages/Home.md", "pages/Start.md"],
        )]),
    );
    let moved = ok(&get(&w, &api(&format!("revisions/{}", block_id(2)))).await);
    assert_eq!(moved.at("/did/0/action"), "moved", "{moved}");
    assert_eq!(moved.at("/did/0/from/title"), "Home");
    assert_eq!(moved.at("/did/0/to/title"), "Start");
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
    let body = ok(&get(&w, &api(&format!("revisions/{}", block_id(3)))).await);
    assert_eq!(body.at("/too_large"), true);
    assert_eq!(body.at("/did"), &json!([]));
}

#[tokio::test]
async fn a_change_with_a_title_carries_the_page_as_it_stood_after_it() {
    let w = handbook();
    let at = ok(&get(&w, &api(&format!("revisions/{}?title=Home", block_id(2)))).await);
    let page = at.at("/page");
    assert_eq!(page.at("/title"), "Home");
    assert!(page.at("/html").as_str().unwrap().contains("Second."));
    assert!(!page.at("/html").as_str().unwrap().contains("Third."));
    // Pinned to that change, not to the tip.
    assert_eq!(page.at("/tip/block_id"), &json!(block_id(2)));
    assert_eq!(page.at("/stale"), false);
    // The current page says what the tip says.
    let now = ok(&get(&w, &api("pages/Home")).await);
    assert!(now.at("/html").as_str().unwrap().contains("Third."));
    // By an alias, the page it names.
    let alias = ok(&get(&w, &api(&format!("revisions/{}?title=How-to", block_id(1)))).await);
    assert_eq!(alias.at("/page/title"), "Guide");
    // A title with no page at that change is not found, and the change is.
    let before = get(
        &w,
        &api(&format!("revisions/{}?title=Nowhere", block_id(2))),
    )
    .await;
    assert_eq!(before.status, StatusCode::NOT_FOUND);
    assert_eq!(problem_type(&before), "urn:inklkunn:problem:page-not-found");
    let bad = get(&w, &api(&format!("revisions/{}?title=..", block_id(2)))).await;
    assert_eq!(problem_type(&bad), "urn:inklkunn:problem:invalid-title");
}

#[tokio::test]
async fn a_change_that_is_not_in_the_history_is_one_answer_whatever_the_reason() {
    let w = handbook();
    for id in [
        "e".repeat(64),
        "abc".to_owned(),
        block_id(2).replace('b', "c"),
    ] {
        let a = get(&w, &api(&format!("revisions/{id}"))).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{id}");
        assert_eq!(problem_type(&a), "urn:inklkunn:problem:change-not-found");
    }
    // Not hexadecimal at all, upper case, or too long: the parameter is refused by name.
    for id in [
        "NOT-HEX".to_owned(),
        block_id(2).to_uppercase(),
        "a".repeat(65),
    ] {
        let a = get(&w, &api(&format!("revisions/{id}"))).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{id}");
        assert_eq!(a.json().at("/parameter"), "id");
    }
    let view = get(&w, &api(&format!("revisions/{}?view=nope", block_id(2)))).await;
    assert_eq!(problem_type(&view), "urn:inklkunn:problem:no-such-view");
}

// ------------------------------------------------------------------------------------ a difference

#[tokio::test]
async fn a_diff_is_one_diff_with_the_pages_path_and_the_title_is_the_address() {
    let w = handbook();
    let uri = api(&format!(
        "diff?path=Home&from={}&to={}",
        block_id(1),
        block_id(2)
    ));
    let a = get(&w, &uri).await;
    let body = ok(&a);
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
    assert_eq!(body.at("/title"), "Home");
    assert_eq!(body.at("/from"), &json!(block_id(1)));
    assert_eq!(body.at("/to"), &json!(block_id(2)));
    assert_eq!(body.at("/from_number"), 1);
    assert_eq!(body.at("/to_number"), 2);
    assert_eq!(body.at("/status"), "modified");
    assert_eq!(body.at("/added"), 3);
    assert_eq!(body.at("/removed"), 2);
    assert_eq!(body.at("/truncated"), false);
    assert_eq!(body.at("/no_lines"), false);
    // By an alias the path is the page's own; by a title nobody has it is still one page's path, never a tree.
    w.fake
        .diff_says(Ok(diff_of("pages/Guide.md", "modified", &[], 1, 3)));
    let _ = get(
        &w,
        &api(&format!(
            "diff?path=How-to&from={}&to={}",
            block_id(1),
            block_id(3)
        )),
    )
    .await;
    let _ = get(
        &w,
        &api(&format!(
            "diff?path=Nothing%20Here&from={}&to={}",
            block_id(1),
            block_id(3)
        )),
    )
    .await;
    let asked = w.fake.diffs_asked();
    assert_eq!(asked.len(), 3);
    assert_eq!(asked.get(1).unwrap().2, vec!["pages/Guide.md".to_owned()]);
    assert_eq!(
        asked.get(2).unwrap().2,
        vec!["pages/Nothing Here.md".to_owned()]
    );
    for (_, _, paths) in &asked {
        assert_eq!(paths.len(), 1, "a path, never the whole tree: {paths:?}");
    }
}

#[tokio::test]
async fn the_lines_are_the_version_controls_the_marks_are_ours_and_markup_is_text() {
    let w = handbook();
    let a = get(
        &w,
        &api(&format!(
            "diff?path=Home&from={}&to={}",
            block_id(1),
            block_id(2)
        )),
    )
    .await;
    let text = a.text();
    // Not one markup character on the wire.
    assert!(
        !text.contains('<') && !text.contains('>') && !text.contains('&'),
        "{text}"
    );
    let body = a.json();
    let hunk = body.at("/hunks/0");
    assert_eq!(hunk.at("/kind"), "lines");
    assert_eq!(hunk.at("/old_start"), 6);
    assert_eq!(hunk.at("/new_start"), 6);
    let lines = hunk.at("/lines").as_array().unwrap();
    // The lines joined back are prikk's lines, whatever the marks.
    let unified: Vec<String> = lines
        .iter()
        .map(|l| {
            let sign = match l.at("/kind").as_str().unwrap() {
                "context" => ' ',
                "removed" => '-',
                "added" => '+',
                "no-newline-marker" => '\\',
                other => panic!("{other}"),
            };
            let joined: String = l
                .at("/segments")
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s.at("/text").as_str().unwrap())
                .collect();
            format!("{sign}{joined}")
        })
        .collect();
    let given: Vec<&str> = HOME_HUNK.lines().skip(1).collect();
    assert_eq!(unified, given);
    // Markup in a line is text, in segments, marked where the words differ.
    let added: String = lines
        .iter()
        .filter(|l| l.at("/kind") == "added")
        .flat_map(|l| l.at("/segments").as_array().unwrap())
        .map(|s| s.at("/text").as_str().unwrap())
        .collect();
    assert!(added.contains("<script>alert(1)</script>"), "{added}");
    assert!(
        lines
            .iter()
            .flat_map(|l| l.at("/segments").as_array().unwrap())
            .any(|s| s.at("/marked") == true),
        "the refinement marked some words"
    );
}

#[tokio::test]
async fn a_diff_without_its_points_compares_the_tip_with_the_change_before_it() {
    let w = handbook();
    w.fake
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &[HOME_HUNK], 3, 4)));
    let body = ok(&get(&w, &api("diff?path=Home")).await);
    assert_eq!(body.at("/from"), &json!(block_id(3)));
    assert_eq!(body.at("/to"), &json!(block_id(4)));
    // With `to` only: the change before it.
    let body = ok(&get(&w, &api(&format!("diff?path=Home&to={}", block_id(2)))).await);
    assert_eq!(body.at("/from"), &json!(block_id(1)));
    assert_eq!(body.at("/to"), &json!(block_id(2)));
    // With `from` only: up to the tip.
    let body = ok(&get(&w, &api(&format!("diff?path=Home&from={}", block_id(2)))).await);
    assert_eq!(body.at("/from"), &json!(block_id(2)));
    assert_eq!(body.at("/to"), &json!(block_id(4)));
    // The first change has nothing before it to compare with.
    let first = get(&w, &api(&format!("diff?path=Home&to={}", block_id(1)))).await;
    assert_eq!(first.status, StatusCode::NOT_FOUND);
    assert_eq!(
        problem_type(&first),
        "urn:inklkunn:problem:change-not-found"
    );
}

#[tokio::test]
async fn a_point_that_is_not_a_change_of_the_view_never_reaches_the_diff() {
    let w = handbook();
    let before = w.fake.diffs_asked().len();
    for tail in [
        format!("diff?path=Home&from=abc&to={}", block_id(2)),
        format!("diff?path=Home&from={}&to={}", "e".repeat(64), block_id(2)),
        format!("diff?path=Home&from={}&to={}", block_id(1), "e".repeat(64)),
    ] {
        let a = get(&w, &api(&tail)).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{tail}");
        assert_eq!(problem_type(&a), "urn:inklkunn:problem:change-not-found");
    }
    assert_eq!(w.fake.diffs_asked().len(), before);
    // Not even hexadecimal: refused by name before anything is read.
    for (tail, parameter) in [
        ("diff?path=Home&from=--all", "from"),
        ("diff?path=Home&to=heads%2Fmain", "to"),
        ("diff?path=Home&from=ABC", "from"),
    ] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{tail}");
        assert_eq!(a.json().at("/parameter"), parameter);
    }
    assert_eq!(w.fake.diffs_asked().len(), before);
}

#[tokio::test]
async fn a_page_that_is_new_removed_unchanged_or_not_text_is_told_so() {
    let w = handbook();
    let range = format!("&from={}&to={}", block_id(1), block_id(2));
    for (status, hunks, code) in [
        ("added", vec![HOME_HUNK], "added"),
        ("removed", vec![], "removed"),
        ("modified", vec![], "modified"),
        ("renamed", vec![], "other"),
    ] {
        w.fake
            .diff_says(Ok(diff_of("pages/Home.md", status, &hunks, 1, 2)));
        let body = ok(&get(&w, &api(&format!("diff?path=Home{range}"))).await);
        assert_eq!(body.at("/status"), code, "{status}");
        assert_eq!(
            body.at("/no_lines"),
            hunks.is_empty() && code != "unchanged",
            "{status}"
        );
    }
    // A page prikk reports nothing about is the same at both points.
    w.fake.diff_says(Ok(inklkunn_prikk::Diff {
        entries: Vec::new(),
        ..diff_of("pages/Home.md", "modified", &[], 1, 2)
    }));
    let body = ok(&get(&w, &api(&format!("diff?path=Home{range}"))).await);
    assert_eq!(body.at("/status"), "unchanged");
    assert_eq!(body.at("/hunks"), &json!([]));
}

#[tokio::test]
async fn a_diff_is_bounded_and_says_when_it_was_cut() {
    let w = handbook();
    let lines: String = (0..5000).map(|i| format!("+line {i}\n")).collect();
    let hunk = format!("@@ -0,0 +1,5000 @@\n{lines}");
    w.fake
        .diff_says(Ok(diff_of("pages/Home.md", "modified", &[&hunk], 1, 2)));
    let body = ok(&get(
        &w,
        &api(&format!(
            "diff?path=Home&from={}&to={}",
            block_id(1),
            block_id(2)
        )),
    )
    .await);
    assert_eq!(body.at("/truncated"), true);
    assert_eq!(body.at("/added"), 5000, "the count is of all the lines");
    assert_eq!(
        body.at("/hunks"),
        &json!([]),
        "a hunk over the bound is dropped whole"
    );
}

#[tokio::test]
async fn a_diff_has_a_validator_and_answers_304_to_it() {
    let w = handbook();
    let uri = api(&format!(
        "diff?path=Home&from={}&to={}",
        block_id(1),
        block_id(2)
    ));
    let etag = get(&w, &uri).await.header("etag").unwrap().to_owned();
    let again = send(&w, Method::GET, &uri, &[("if-none-match", &etag)]).await;
    assert_eq!(again.status, StatusCode::NOT_MODIFIED);
    assert!(again.bytes.is_empty());
}

#[tokio::test]
async fn the_diff_needs_a_path_and_a_view_that_exists() {
    let w = handbook();
    for tail in ["diff", "diff?path=", "diff?from=abc&to=def"] {
        let a = get(&w, &api(tail)).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{tail}");
        assert_eq!(problem_type(&a), "urn:inklkunn:problem:diff-path-required");
    }
    let view = get(&w, &api("diff?path=Home&view=nope")).await;
    assert_eq!(problem_type(&view), "urn:inklkunn:problem:no-such-view");
    let bad = get(&w, &api("diff?path=..")).await;
    assert_eq!(problem_type(&bad), "urn:inklkunn:problem:invalid-title");
}
