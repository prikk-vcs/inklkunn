//! The feed of a wiki's proposals through the real router (RFC 007 D-9; handoff 007 I-T5): `-/proposals.atom` says when a
//! proposal was opened and when it was decided, to everyone who may read the wiki, **the same to each of them**: never a draft,
//! not even the reader's own, nothing of a hidden text (held for every surface in `tests/tombstones/surfaces.rs`), and every
//! word of it escaped.

use axum::http::Method;
use inklkunn_core::proposal::QuotaConfig;
use serde_json::json;

use super::super::At;
use super::lists::{ask, ids_in, listed, read, readers, sorted};
use super::{API, cast, id_of, open, opening, post, session, tip, token, with_quota};

#[path = "../../changes_feeds/xml.rs"]
mod xml;

const FEED: &str = "/handbook/-/proposals.atom";

#[tokio::test]
async fn the_feed_says_when_each_proposal_was_opened_and_decided_the_same_to_every_reader_and_never_a_draft()
 {
    let c = cast();
    let held = id_of(&open(&c, &c.contributor, true).await);
    let declined = id_of(&open(&c, &c.other, true).await);
    let draft = id_of(&open(&c, &c.contributor, false).await);
    let other_draft = id_of(&open(&c, &c.other, false).await);
    let a = post(
        &c.w,
        &format!("{API}/proposals/{declined}/declines"),
        &[
            token("accept", &c.maintainer),
            ("if-match", format!("\"{declined}-r1\"")),
        ],
        json!({"reason": "Not now."}).to_string(),
    )
    .await;
    assert_eq!(a.status, 200, "{}", a.text());

    let mut bodies = Vec::new();
    for (who, account) in readers(&c) {
        let a = read(&c.w, FEED, account.as_ref()).await;
        assert_eq!(a.status, 200, "{who}: {}", a.text());
        assert_eq!(
            a.header("content-type"),
            Some("application/atom+xml; charset=utf-8"),
            "{who}"
        );
        assert!(
            a.header("x-robots-tag").is_none(),
            "{who}: a public wiki is for indexing"
        );
        let text = a.text();
        assert_eq!(
            xml::atom_violations(&text),
            Vec::<String>::new(),
            "{who}: {text}"
        );
        // No draft, not the reader's own and not another's.
        for id in [&draft, &other_draft] {
            assert!(
                !text.contains(&id.to_string()),
                "{who}: a draft is in the feed"
            );
        }
        bodies.push((who, text));
    }
    // The same feed for everyone who may read the wiki: what a reader is changes nothing in it.
    let first = bodies.first().map(|(_, b)| b.clone()).unwrap();
    for (who, body) in &bodies {
        assert_eq!(*body, first, "{who}: the feed depends on who reads it");
    }

    let feed = xml::parse(&first).unwrap();
    assert_eq!(
        feed.first("title").unwrap().text,
        "Proposals · The Handbook"
    );
    assert_eq!(
        feed.first("id").unwrap().text,
        format!("urn:inklkunn:{}:proposals", c.w.wiki.id)
    );
    let entries: Vec<&xml::Element> = feed.all("entry").collect();
    // Two opened, one decided: three events, each once.
    assert_eq!(entries.len(), 3, "{first}");
    let by_id = |suffix: &str, id: &inklkunn_store::ProposalId| {
        let want = format!("urn:inklkunn:{}:proposal:{id}:{suffix}", c.w.wiki.id);
        *entries
            .iter()
            .find(|e| e.first("id").is_some_and(|i| i.text == want))
            .unwrap_or_else(|| panic!("no entry {want} in {first}"))
    };
    let opened = by_id("opened", &held);
    assert_eq!(
        opened.first("title").unwrap().text,
        "Proposed: Improve the second paragraph"
    );
    assert_eq!(
        opened.first("summary").unwrap().text,
        "Proposed by Aiko Tanaka"
    );
    assert_eq!(
        opened.first("link").unwrap().attr("href"),
        Some(format!("http://localhost:8080/handbook/-/proposals/{held}").as_str())
    );
    let categories = |e: &xml::Element| -> Vec<String> {
        e.all("category")
            .map(|c| c.attr("term").unwrap().to_owned())
            .collect()
    };
    assert_eq!(categories(opened), ["opened"]);
    by_id("opened", &declined);
    let closed = by_id("closed", &declined);
    assert_eq!(
        closed.first("title").unwrap().text,
        "Declined: Improve the second paragraph"
    );
    assert_eq!(categories(closed), ["decided", "declined"]);
    // The proposal that is still held was not decided.
    assert!(!entries.iter().any(|e| {
        e.first("id").is_some_and(|i| {
            i.text == format!("urn:inklkunn:{}:proposal:{held}:closed", c.w.wiki.id)
        })
    }));

    // The feed lists what the list of the API lists, and what the page lists.
    let api = read(&c.w, &format!("{API}/proposals"), None).await.json();
    let mut in_feed: Vec<String> = entries
        .iter()
        .filter_map(|e| e.first("link")?.attr("href"))
        .filter_map(|href| ids_in(href).into_iter().next())
        .collect();
    in_feed.sort();
    in_feed.dedup();
    assert_eq!(sorted(in_feed), sorted(listed(&api)));
    let page = crate::common::get_as(&c.w, "/handbook/-/proposals", None)
        .await
        .body();
    assert_eq!(sorted(ids_in(&page)), sorted(listed(&api)));
}

#[tokio::test]
async fn every_word_of_the_feed_is_escaped_and_the_feed_lists_fifty_events_and_no_more() {
    let c = cast();
    // A summary that is markup.
    let sent = post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        opening(
            "Fix <script>alert(1)</script> & \"quote\"",
            &super::better(),
            true,
        ),
    )
    .await;
    assert_eq!(sent.status, 201, "{}", sent.text());
    let a = read(&c.w, FEED, None).await;
    let text = a.text();
    assert!(!text.contains("<script>"), "{text}");
    assert_eq!(xml::atom_violations(&text), Vec::<String>::new(), "{text}");
    let feed = xml::parse(&text).unwrap();
    assert_eq!(
        feed.first("entry").unwrap().first("title").unwrap().text,
        "Proposed: Fix <script>alert(1)</script> & \"quote\""
    );
    // The feed lists the newest fifty events and no more; the API's list, the same proposals, goes on by its link.
    let c = with_quota(
        cast(),
        QuotaConfig {
            open_proposals: 1000,
            proposals_per_hour_per_principal: 1000,
            proposals_per_hour_per_wiki: 1000,
            ..QuotaConfig::default()
        },
    );
    for _ in 0..55 {
        open(&c, &c.contributor, true).await;
    }
    let long = xml::parse(&read(&c.w, FEED, None).await.text()).unwrap();
    assert_eq!(long.all("entry").count(), 50);
    let list = read(&c.w, &format!("{API}/proposals"), None).await;
    assert_eq!(list.json().at("/items").as_array().unwrap().len(), 50);
    assert!(list.json().at("/next_cursor").is_string());
    // A method it does not take, and a feed of a wiki that is not there.
    let post_feed = ask(&c.w, Method::POST, FEED, &[]).await;
    assert_eq!(post_feed.status, 405);
    let absent = read(&c.w, "/no-such-wiki/-/proposals.atom", None).await;
    assert_eq!(absent.status, 404);
}
