//! The reads of the proposals through the real router (RFC 007 D-9; handoff 007 I-T5): `listProposals`, `listProposalComments`
//! and the feed of what was opened and decided. **Each shows what the same person's pages show**: for an anonymous visitor, a
//! contributor, an observer and a moderator the list holds the proposals the proposals' page holds (but for the drafts, which
//! the list never holds, not even the reader's own), a restricted wiki is absent, and what moderation hides is its tombstone (the
//! hiding itself is held for every surface in `tests/tombstones/surfaces.rs`).

use axum::body::Body;
use axum::http::Method;
use inklkunn_store::{AccountId, ProposalId, Role, Visibility};
use serde_json::{Value, json};

use super::super::common::{self, World, add_person, page, person, with_accounts};
use super::super::{Answer, At, assert_next_link};
use super::{
    API, Cast, cast, id_of, open, post, problem_type, session, tag_header, tip, token, urn,
};

/// A request with these headers and no body.
pub(super) async fn ask(
    w: &World,
    method: Method,
    uri: &str,
    headers: &[(&'static str, String)],
) -> Answer {
    super::send(w, method, uri, headers, Body::empty()).await
}

/// An API read as `who` (a session), or with no credential.
pub(super) async fn read(w: &World, uri: &str, who: Option<&AccountId>) -> Answer {
    match who {
        Some(who) => ask(w, Method::GET, uri, &[session(who)]).await,
        None => ask(w, Method::GET, uri, &[]).await,
    }
}

/// The ids of the proposals a page of the wiki links to (`/handbook/-/proposals/<id>`), once each, in the order they stand.
pub(super) fn ids_in(html: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for (at, _) in html.match_indices("/handbook/-/proposals/") {
        let id: String = html
            .get(at + "/handbook/-/proposals/".len()..)
            .unwrap_or_default()
            .chars()
            .take(36)
            .collect();
        if ProposalId::parse(&id).is_some() && !found.contains(&id) {
            found.push(id);
        }
    }
    found
}

pub(super) fn listed(body: &Value) -> Vec<String> {
    body.at("/items")
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.at("/id").as_str().unwrap().to_owned())
        .collect()
}

pub(super) fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// Who reads: the roles of the cast, and nobody.
pub(super) fn readers(c: &Cast) -> Vec<(&'static str, Option<AccountId>)> {
    vec![
        ("an anonymous visitor", None),
        ("a contributor", Some(c.contributor.clone())),
        ("another contributor", Some(c.other.clone())),
        ("an observer", Some(c.observer.clone())),
        ("a moderator", Some(c.maintainer.clone())),
    ]
}

// ------------------------------------------------------------------------------------ the list of proposals

#[tokio::test]
async fn the_list_holds_what_the_proposals_page_holds_for_each_reader_and_never_a_draft() {
    let c = cast();
    let a = id_of(&open(&c, &c.contributor, true).await).to_string();
    let b = id_of(&open(&c, &c.other, true).await).to_string();
    let draft_of_contributor = id_of(&open(&c, &c.contributor, false).await).to_string();
    let draft_of_other = id_of(&open(&c, &c.other, false).await).to_string();
    for (who, account) in readers(&c) {
        let api = read(&c.w, &format!("{API}/proposals"), account.as_ref()).await;
        assert_eq!(api.status, 200, "{who}: {}", api.text());
        let body = api.json();
        let from_api = listed(&body);
        // The page, for the same person.
        let html = common::get_as(&c.w, "/handbook/-/proposals", account.as_ref())
            .await
            .body();
        let on_page = ids_in(&html);
        // The page also shows its reader's own drafts, under their own heading: that is all the two differ by.
        let own_drafts: Vec<String> = [&draft_of_contributor, &draft_of_other]
            .into_iter()
            .filter(|d| {
                let owner = if **d == draft_of_contributor {
                    &c.contributor
                } else {
                    &c.other
                };
                account.as_ref() == Some(owner)
            })
            .cloned()
            .collect();
        let without_drafts: Vec<String> = on_page
            .iter()
            .filter(|id| !own_drafts.contains(id))
            .cloned()
            .collect();
        assert_eq!(
            sorted(from_api.clone()),
            sorted(without_drafts),
            "{who}: the list is the page's, without drafts"
        );
        assert_eq!(
            sorted(from_api.clone()),
            sorted(vec![a.clone(), b.clone()]),
            "{who}"
        );
        // No draft, not the reader's own and not another's, in any member.
        for draft in [&draft_of_contributor, &draft_of_other] {
            assert!(
                !api.text().contains(draft.as_str()),
                "{who}: a draft is listed"
            );
        }
        for item in body.at("/items").as_array().unwrap() {
            assert_ne!(item.at("/state"), "draft", "{who}");
            assert_eq!(item.at("/revision"), 1, "{who}");
        }
        // A reader's own is told as theirs, and nobody else's.
        for item in body.at("/items").as_array().unwrap() {
            let mine = account.as_ref().is_some_and(|me| {
                (item.at("/id") == a.as_str() && *me == c.contributor)
                    || (item.at("/id") == b.as_str() && *me == c.other)
            });
            assert_eq!(item.at("/yours"), mine, "{who}");
        }
    }
}

#[tokio::test]
async fn the_list_is_walked_by_cursor_and_by_link_newest_first_and_each_page_says_where_the_next_is()
 {
    let c = cast();
    let mut opened = Vec::new();
    for _ in 0..3 {
        opened.push(id_of(&open(&c, &c.contributor, true).await).to_string());
    }
    let whole = read(&c.w, &format!("{API}/proposals"), None).await;
    assert_eq!(whole.status, 200);
    assert_eq!(assert_next_link(&whole, &format!("{API}/proposals")), None);
    let all = listed(&whole.json());
    assert_eq!(sorted(all.clone()), sorted(opened.clone()));
    // Every page of a walk of one at a time: the Link is the next, the last has none.
    let first = format!("{API}/proposals?limit=1");
    let mut uri = first.clone();
    let mut walked: Vec<String> = Vec::new();
    let mut pages = 0;
    loop {
        let a = read(&c.w, &uri, None).await;
        assert_eq!(a.status, 200, "{}", a.text());
        let next = assert_next_link(&a, &uri);
        let body = a.json();
        assert!(body.at("/items").as_array().unwrap().len() <= 1);
        walked.extend(listed(&body));
        pages += 1;
        match (next, body.at("/next_cursor").as_str()) {
            (Some(target), Some(cursor)) => {
                assert_eq!(target, format!("{first}&cursor={cursor}"));
                assert!(cursor.starts_with("p1"), "{cursor}");
                uri = target;
            }
            (None, None) => break,
            other => panic!("the body and the Link disagree: {other:?}"),
        }
        assert!(pages < 10);
    }
    assert_eq!(pages, 3);
    assert_eq!(
        walked, all,
        "every proposal once, in the order of the whole list"
    );
    // The state that narrows the list is kept in the link.
    let narrowed = read(
        &c.w,
        &format!("{API}/proposals?state=held-for-review&limit=2"),
        None,
    )
    .await;
    assert_eq!(
        assert_next_link(
            &narrowed,
            &format!("{API}/proposals?state=held-for-review&limit=2")
        )
        .unwrap(),
        format!(
            "{API}/proposals?state=held-for-review&limit=2&cursor={}",
            narrowed.json().at("/next_cursor").as_str().unwrap()
        )
    );
}

#[tokio::test]
async fn a_state_narrows_the_list_and_a_draft_a_cursor_of_another_list_or_a_made_up_one_is_refused()
{
    let c = cast();
    let held = id_of(&open(&c, &c.contributor, true).await).to_string();
    let declined = id_of(&open(&c, &c.other, true).await);
    // The moderator declines the second.
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
    let only = |state: &str| {
        let uri = format!("{API}/proposals?state={state}");
        let c = &c;
        async move { listed(&read(&c.w, &uri, None).await.json()) }
    };
    assert_eq!(only("held-for-review").await, std::slice::from_ref(&held));
    assert_eq!(only("declined").await, [declined.to_string()]);
    assert!(only("accepted").await.is_empty());
    // A draft is no state a list has: refused by name, for every reader, not answered with the reader's own.
    for bad in ["draft", "open", "Declined", ""] {
        let a = read(
            &c.w,
            &format!("{API}/proposals?state={bad}"),
            Some(&c.contributor),
        )
        .await;
        assert_eq!(a.status, 400, "{bad}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-parameter"), "{bad}");
    }
    // The cursors: this list's own, and nothing else.
    let first = read(&c.w, &format!("{API}/proposals?limit=1"), None)
        .await
        .json();
    let cursor = first.at("/next_cursor").as_str().unwrap().to_owned();
    let pages_cursor = read(&c.w, &format!("{API}/pages?limit=1"), None)
        .await
        .json()
        .at("/next_cursor")
        .as_str()
        .map(str::to_owned);
    let mut bad = vec![
        "zz".to_owned(),
        "p1".to_owned(),
        "p1.".to_owned(),
        "p1x.019a0000-0000-7000-8000-00000000cccc".to_owned(),
        "p105.019a0000-0000-7000-8000-00000000cccc".to_owned(),
        "p1-5.019a0000-0000-7000-8000-00000000cccc".to_owned(),
        "p15.NOT-AN-ID".to_owned(),
        "n13".to_owned(),
        "m1019a0000-0000-7000-8000-00000000cccc".to_owned(),
        format!("{cursor}x"),
    ];
    bad.extend(pages_cursor);
    for text in bad {
        let a = read(&c.w, &format!("{API}/proposals?cursor={text}"), None).await;
        assert_eq!(a.status, 400, "{text}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-cursor"), "{text}");
    }
    // A bound that is not the list's: more than a page, or none.
    for bad in [
        "limit=101",
        "limit=0",
        "limt=5",
        "state=held-for-review&state=declined",
    ] {
        let a = read(&c.w, &format!("{API}/proposals?{bad}"), None).await;
        assert_eq!(a.status, 400, "{bad}");
        assert_eq!(problem_type(&a), urn("invalid-parameter"), "{bad}");
    }
}

#[tokio::test]
async fn the_list_answers_with_an_etag_and_a_matching_one_is_not_modified() {
    let c = cast();
    open(&c, &c.contributor, true).await;
    for uri in [
        format!("{API}/proposals"),
        format!("{API}/proposals?limit=1"),
    ] {
        let a = read(&c.w, &uri, None).await;
        let etag = a.header("etag").expect("an etag").to_owned();
        let again = ask(&c.w, Method::GET, &uri, &[("if-none-match", etag.clone())]).await;
        assert_eq!(again.status, 304, "{uri}");
        let head = ask(&c.w, Method::HEAD, &uri, &[]).await;
        assert_eq!(head.status, 200, "{uri}");
        assert_eq!(head.header("etag"), Some(etag.as_str()));
    }
}

#[tokio::test]
async fn a_token_reads_the_lists_with_the_scope_of_reading_and_one_that_does_not_authenticate_reads_as_nobody()
 {
    let c = cast();
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    for uri in [
        format!("{API}/proposals"),
        format!("{API}/proposals/{id}/comments"),
    ] {
        // A token that may only read lists them: the list is a read, whatever else the address takes.
        let a = ask(&c.w, Method::GET, &uri, &[token("read", &c.observer)]).await;
        assert_eq!(a.status, 200, "{uri}: {}", a.text());
        // And a token that could write does not need to be asked to: it is not refused by what it may not do.
        let b = ask(&c.w, Method::GET, &uri, &[token("propose", &c.contributor)]).await;
        assert_eq!(b.status, 200, "{uri}");
        // A credential that does not authenticate reads as nobody does, as it does at every other read of the API.
        let none = ask(
            &c.w,
            Method::GET,
            &uri,
            &[("authorization", "Bearer nope".to_owned())],
        )
        .await;
        let nobody = ask(&c.w, Method::GET, &uri, &[]).await;
        assert_eq!(none.status, 200, "{uri}");
        assert_eq!(none.bytes, nobody.bytes, "{uri}");
    }
}

// ------------------------------------------------------------------------------------ the comments of a proposal

/// Comment on `id` as `who`, on the revision named by `p`.
async fn comment(c: &Cast, who: &AccountId, p: &Value, text: &str) -> Value {
    let a = post(
        &c.w,
        &format!("{API}/proposals/{}/comments", id_of(p)),
        &[session(who), tag_header(p)],
        json!({"body": text}).to_string(),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    a.json()
}

#[tokio::test]
async fn a_proposals_comments_are_listed_in_order_by_page_and_link_to_every_reader_of_the_wiki() {
    let c = cast();
    let p = open(&c, &c.contributor, true).await;
    let id = id_of(&p);
    let mut said = Vec::new();
    for (n, who) in [&c.other, &c.reviewer, &c.contributor, &c.maintainer]
        .into_iter()
        .enumerate()
    {
        said.push(comment(&c, who, &p, &format!("Comment number {n}")).await);
    }
    let uri = format!("{API}/proposals/{id}/comments");
    for (who, account) in readers(&c) {
        let a = read(&c.w, &uri, account.as_ref()).await;
        assert_eq!(a.status, 200, "{who}: {}", a.text());
        assert_eq!(assert_next_link(&a, &uri), None, "{who}");
        let body = a.json();
        let bodies: Vec<&str> = body
            .at("/items")
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.at("/body").as_str().unwrap())
            .collect();
        assert_eq!(
            bodies,
            [
                "Comment number 0",
                "Comment number 1",
                "Comment number 2",
                "Comment number 3"
            ],
            "{who}: in the order they were made"
        );
        // The page that shows the same thread shows the same comments, to the same reader.
        let html = common::get_as(
            &c.w,
            &format!("/handbook/-/proposals/{id}"),
            account.as_ref(),
        )
        .await
        .body();
        for text in &bodies {
            assert!(html.contains(text), "{who}: the page lacks `{text}`");
        }
        // No comment names its author, and the shape is the write API's.
        for (item, written) in body.at("/items").as_array().unwrap().iter().zip(&said) {
            assert_eq!(item.at("/id"), written.at("/id"), "{who}");
            assert_eq!(item.at("/revision"), written.at("/revision"), "{who}");
            assert!(item.get("author").is_none(), "{who}");
            assert!(item.get("hidden").is_none(), "{who}: nothing is hidden");
        }
        for name in ["Aiko Tanaka", "Ben Okoye", "Rin Sato", "Mika Ito"] {
            assert!(!a.text().contains(name), "{who}: names {name}");
        }
    }
    // A walk of one comment at a time follows the Link, in order, once each.
    let first = format!("{uri}?limit=1");
    let mut target = Some(first.clone());
    let mut walked = Vec::new();
    let mut pages = 0;
    while let Some(next) = target {
        let a = read(&c.w, &next, None).await;
        assert_eq!(a.status, 200, "{}", a.text());
        target = assert_next_link(&a, &next);
        if let Some(target) = &target {
            assert!(
                target.contains(&format!(
                    "cursor=m1{}",
                    a.json().at("/items/0/id").as_str().unwrap()
                )),
                "{target}"
            );
        }
        walked.extend(listed(&a.json()));
        pages += 1;
        assert!(pages < 10);
    }
    assert_eq!(pages, 4);
    assert_eq!(
        walked,
        said.iter()
            .map(|s| s.at("/id").as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    // Comments made while it is walked do not move the place of a cursor.
    let page_one = read(&c.w, &format!("{uri}?limit=2"), None).await.json();
    let cursor = page_one.at("/next_cursor").as_str().unwrap().to_owned();
    comment(&c, &c.other, &p, "Comment number 4").await;
    let rest = read(&c.w, &format!("{uri}?limit=10&cursor={cursor}"), None)
        .await
        .json();
    assert_eq!(rest.at("/items").as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn the_comments_of_a_draft_are_its_authors_alone_and_a_proposal_that_is_not_there_is_not_found()
 {
    let c = cast();
    let draft = open(&c, &c.contributor, false).await;
    let id = id_of(&draft);
    let uri = format!("{API}/proposals/{id}/comments");
    // The author reads the draft's thread (empty); everyone else is told what they are told of a proposal that does not exist.
    let own = read(&c.w, &uri, Some(&c.contributor)).await;
    assert_eq!(own.status, 200, "{}", own.text());
    assert_eq!(own.json().at("/items").as_array().unwrap().len(), 0);
    let absent_uri = format!("{API}/proposals/{}/comments", ProposalId::new());
    for (who, account) in readers(&c) {
        if account.as_ref() == Some(&c.contributor) {
            continue;
        }
        let a = read(&c.w, &uri, account.as_ref()).await;
        let absent = read(&c.w, &absent_uri, account.as_ref()).await;
        assert_eq!(a.status, 404, "{who}: {}", a.text());
        assert_eq!(problem_type(&a), urn("not-found"), "{who}");
        // The same bytes: the draft's id is told from no other.
        assert_eq!(a.bytes, absent.bytes, "{who}");
    }
    // An id that is not an id, and a cursor that is not a comment's of this proposal.
    let a = read(&c.w, &format!("{API}/proposals/not-an-id/comments"), None).await;
    assert_eq!(a.status, 404);
    for bad in ["zz", "m1", "m1019a0000-0000-7000-8000-00000000cccc", "n13"] {
        let a = read(&c.w, &format!("{uri}?cursor={bad}"), Some(&c.contributor)).await;
        assert_eq!(a.status, 400, "{bad}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-cursor"), "{bad}");
    }
}

// ------------------------------------------------------------------------------------ a restricted wiki is absent

fn restricted() -> (World, AccountId, AccountId) {
    let w = with_accounts(World::with_visibility(
        "secret",
        "Secret Wiki",
        Visibility::Restricted,
        &[(
            "pages/Home.md",
            &page(&["title: Home"], "the hidden text\n"),
        )],
    ));
    let outsider = person(7);
    add_person(&w, &outsider, "Out Sider", None);
    let insider = person(8);
    add_person(&w, &insider, "In Sider", Some(Role::Admin));
    (w, outsider, insider)
}

#[tokio::test]
async fn a_restricted_wiki_is_absent_to_both_lists_and_the_feed_for_whoever_may_not_read_it() {
    let (w, outsider, insider) = restricted();
    // The wiki's own admin opens a proposal and comments on it.
    let opened = post(
        &w,
        "/secret/-/api/v1/proposals",
        &[session(&insider), tip()],
        json!({
            "summary": "A secret change",
            "submit": true,
            "files": [{"path": "pages/Home.md", "text": "the hidden text, changed\n"}],
        })
        .to_string(),
    )
    .await;
    assert_eq!(opened.status, 201, "{}", opened.text());
    let id = id_of(&opened.json());
    let credentials: Vec<(&str, Vec<(&'static str, String)>)> = vec![
        ("an outsider's token", vec![token("read", &outsider)]),
        ("an outsider's session", vec![session(&outsider)]),
        ("no credential", vec![]),
    ];
    let addresses = [
        "/{}/-/api/v1/proposals".to_owned(),
        "/{}/-/api/v1/proposals?state=declined&limit=5".to_owned(),
        format!("/{{}}/-/api/v1/proposals/{id}/comments"),
        "/{}/-/proposals.atom".to_owned(),
    ];
    for (who, creds) in &credentials {
        for address in &addresses {
            let ask = |slug: &str| {
                let uri = address.replace("{}", slug);
                let creds = creds.clone();
                let w = &w;
                async move { ask(w, Method::GET, &uri, &creds).await }
            };
            let real = ask("secret").await;
            let absent = ask("no-such-wiki").await;
            let norm = |a: &Answer, slug: &str| {
                let mut lines = a.header_lines();
                lines.retain(|l| !l.starts_with("content-length"));
                (a.status, lines, a.text().replace(slug, "X"))
            };
            assert_eq!(
                norm(&real, "secret"),
                norm(&absent, "no-such-wiki"),
                "{who} {address}: a restricted wiki is told from an absent one"
            );
            assert_eq!(real.status, 404, "{who} {address}: {}", real.text());
            for secret in ["hidden text", "Secret Wiki", "A secret change"] {
                assert!(
                    !real.text().contains(secret),
                    "{who} {address}: leaked `{secret}`"
                );
            }
        }
    }
    // Whoever may read it reads all three.
    for address in &addresses {
        let a = ask(
            &w,
            Method::GET,
            &address.replace("{}", "secret"),
            &[session(&insider)],
        )
        .await;
        assert_eq!(a.status, 200, "{address}: {}", a.text());
    }
    let list = read(&w, "/secret/-/api/v1/proposals", Some(&insider))
        .await
        .json();
    assert_eq!(listed(&list), [id.to_string()]);
    assert_eq!(list.at("/items/0/summary"), "A secret change");
}

// ------------------------------------------------------------------------------------ the answers are what the document says

#[tokio::test]
async fn every_member_of_the_two_lists_is_declared_in_the_document_and_every_required_one_is_there()
{
    let c = cast();
    let p = open(&c, &c.contributor, true).await;
    comment(&c, &c.other, &p, "A comment").await;
    let document: Value = serde_json::from_slice(
        &ask(&c.w, Method::GET, "/-/api/v1/openapi.json", &[])
            .await
            .bytes,
    )
    .unwrap();
    for (uri, list, item) in [
        (format!("{API}/proposals"), "ProposalList", "Proposal"),
        (
            format!("{API}/proposals/{}/comments", id_of(&p)),
            "ProposalComments",
            "Comment",
        ),
    ] {
        let a = read(&c.w, &uri, None).await;
        assert_eq!(a.status, 200, "{}", a.text());
        let body = a.json();
        for (value, schema) in [(&body, list), (body.at("/items/0"), item)] {
            let declared = document
                .at(&format!("/components/schemas/{schema}/properties"))
                .as_object()
                .unwrap();
            let required: Vec<&str> = document
                .at(&format!("/components/schemas/{schema}/required"))
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect();
            let object = value.as_object().unwrap();
            for name in object.keys() {
                assert!(
                    declared.contains_key(name),
                    "{schema}.{name} is not declared"
                );
            }
            for name in required {
                assert!(
                    object.contains_key(name),
                    "{schema}.{name} is not in the answer"
                );
            }
        }
        // The page size is bounded in the schema as in the server.
        assert_eq!(
            document.at(&format!(
                "/components/schemas/{list}/properties/items/maxItems"
            )),
            inklkunn_core::read::MAX_PAGE_SIZE
        );
    }
    // Both operations are described, each once, beside the write of the same address, with their own guard.
    for (path, post_id, get_id) in [
        (
            "/{slug}/-/api/v1/proposals",
            "createProposal",
            "listProposals",
        ),
        (
            "/{slug}/-/api/v1/proposals/{id}/comments",
            "commentOnProposal",
            "listProposalComments",
        ),
    ] {
        let item = document.at("/paths").get(path).unwrap();
        assert_eq!(item.at("/post/operationId"), post_id);
        assert_eq!(item.at("/get/operationId"), get_id);
        assert_eq!(item.at("/get/x-inklkunn-action"), "read");
        assert!(
            item.at("/get/responses/200/headers/Link").is_object(),
            "{path}"
        );
        assert!(
            item.at("/get/responses/400").is_object(),
            "{path}: the problems of the list"
        );
        assert!(item.at("/get/responses/304").is_object(), "{path}");
    }
    let state = document
        .at("/paths")
        .get("/{slug}/-/api/v1/proposals")
        .unwrap()
        .at("/get/parameters")
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p.at("/name") == "state")
        .unwrap()
        .clone();
    let words: Vec<&str> = state
        .at("/schema/enum")
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .collect();
    // The words of the list's states are the proposals' own, every one but a draft.
    let own: Vec<&str> = inklkunn_store::ProposalState::ALL
        .iter()
        .filter(|s| **s != inklkunn_store::ProposalState::Draft)
        .map(|s| s.as_str())
        .collect();
    assert_eq!(words, own);
}

// ------------------------------------------------------------------------------------ no rate-limit headers

/// `Retry-After` is the whole of what an answer says of waiting: **there are no rate-limit header fields** until the IETF's are an
/// RFC (RFC 007 Q-4; requirements IX-4), on a refusal, on a read, on a write or in the document that describes them.
#[tokio::test]
async fn no_answer_carries_a_rate_limit_header_and_the_document_names_none() {
    let c = super::with_quota(
        cast(),
        inklkunn_core::proposal::QuotaConfig {
            proposals_per_hour_per_principal: 1,
            ..inklkunn_core::proposal::QuotaConfig::default()
        },
    );
    let written = open(&c, &c.contributor, true).await;
    let refused = post(
        &c.w,
        &format!("{API}/proposals"),
        &[token("propose", &c.contributor), tip()],
        super::opening("One too many", &super::better(), false),
    )
    .await;
    assert_eq!(refused.status, 429, "{}", refused.text());
    assert_eq!(refused.header("retry-after"), Some("3600"));
    let id = id_of(&written);
    for (what, answer) in [
        ("a refusal", refused),
        (
            "a list",
            read(&c.w, &format!("{API}/proposals"), None).await,
        ),
        (
            "the comments",
            read(&c.w, &format!("{API}/proposals/{id}/comments"), None).await,
        ),
        (
            "a proposal",
            read(&c.w, &format!("{API}/proposals/{id}"), None).await,
        ),
        (
            "the feed",
            read(&c.w, "/handbook/-/proposals.atom", None).await,
        ),
        (
            "the description",
            read(&c.w, "/-/api/v1/openapi.json", None).await,
        ),
    ] {
        let names: Vec<String> = answer
            .headers
            .keys()
            .map(|k| k.as_str().to_lowercase())
            .collect();
        assert!(
            !names
                .iter()
                .any(|n| n.contains("ratelimit") || n.contains("rate-limit")),
            "{what}: {names:?}"
        );
    }
    let text = read(&c.w, "/-/api/v1/openapi.json", None)
        .await
        .text()
        .to_lowercase();
    assert!(
        !text.contains("ratelimit") && !text.contains("x-rate"),
        "the document names a rate-limit header"
    );
}
