//! **The text of a page at a block** (RFC 006 D-8; INV-13; handoff G-T4, correction H-1): B writes personal data to `Foo`, C edits
//! only `Bar`, and D takes the data out and is the tip. Hiding B is allowed, and then the text of `Foo` **at C** is still B's: every
//! surface that reads a page's text at a block (the permalink, both sides of a diff, a proposal's base, a revise, a rollback, *Why is
//! this here?*) says *hidden* instead, for the visitor who is signed out, a member and the moderator; an unhide gives it all back.
//! The records are the in-memory store's and the repository is a double. Every text that must never appear carries `ZEBRA`.

use axum::http::StatusCode;
use inklkunn_core::read::{BlockText, NoRollback, RollbackOutcome};
use inklkunn_store::{AccountId, GovernanceStore, ProposalId, ProposalStore, Role, TombstoneAct};

use super::{HIDING, nothing_hidden_in};
use crate::common::{
    Answer, World, add_person, block_id, diff_of, get_as, op, person, post_form, service_text,
    shown, state_id, with_accounts,
};

const FOO: &str = "pages/Foo.md";
const BAR: &str = "pages/Bar.md";
const BAZ: &str = "pages/Baz.md";

fn foo(body: &str) -> String {
    format!("---\ntitle: Foo\n---\n\n{body}\n")
}

fn bar(body: &str) -> String {
    format!("---\ntitle: Bar\n---\n\n{body}\n")
}

/// What B wrote: the text that is hidden.
const DATA: &str = "Call ZEBRAPERSONAL on 555-0100.";
/// The sentence the hiding of a text says in its place (the tombstone's own words are `HIDING`).
const EARLIER: &str = "The text of the earlier change is hidden, so no difference is shown.";

/// The wiki: 1 creates `Foo` and `Bar`; 2 (B) writes the data to `Foo`; 3 (C) edits `Bar` only; 4 (D) takes the data out of `Foo`
/// and is the tip.
struct Story {
    w: World,
    maintainer: AccountId,
    author: AccountId,
    reader: AccountId,
}

/// `Foo`'s history by the shape: the same story, or the data moved before D.
fn story() -> Story {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[
            (FOO, &foo("Foo as first written.")),
            (BAR, &bar("Bar as first written.")),
        ],
    ));
    let (maintainer, author, reader) = (person(4), person(1), person(6));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &author, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&w, &reader, "Olu Reader", Some(Role::Observer));
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![op("create-file", &[FOO]), op("create-file", &[BAR])]),
    );
    // B: the data.
    w.fake.set(FOO, &foo(DATA));
    w.fake.say(2, &service_text("Add the contact"));
    w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    // C: another page.
    w.fake.set(BAR, &bar("Bar, edited."));
    w.fake.say(3, &service_text("Tidy the bar"));
    w.fake.show(3, shown(vec![op("edit-text", &[BAR])]));
    // D: the data is taken out. What `diff` says of `Foo` between any two blocks is the removal of the data.
    w.fake.set(FOO, &foo("Foo without the contact."));
    w.fake.say(4, &service_text("Take the contact out"));
    w.fake.show(4, shown(vec![op("edit-text", &[FOO])]));
    w.fake.diff_says(Ok(diff_of(
        FOO,
        "modified",
        &["@@ -3,1 +3,1 @@\n-Call ZEBRAPERSONAL on 555-0100.\n+Foo without the contact.\n"],
        3,
        4,
    )));
    Story {
        w,
        maintainer,
        author,
        reader,
    }
}

impl Story {
    fn viewers(&self) -> Vec<Option<AccountId>> {
        vec![
            None,
            Some(self.reader.clone()),
            Some(self.author.clone()),
            Some(self.maintainer.clone()),
        ]
    }

    async fn get(&self, uri: &str, who: Option<&AccountId>) -> Answer {
        get_as(&self.w, uri, who).await
    }

    /// Hide the content of change `n`, as the moderator.
    async fn hide(&self, n: u64) -> Answer {
        post_form(
            &self.w,
            "/handbook/-/moderate",
            Some(&self.maintainer),
            &[
                ("kind", "page-revision"),
                ("block", block_id(n).as_str()),
                ("part_content", "1"),
                ("category", "personal-data"),
                ("note", ""),
            ],
        )
        .await
    }

    async fn unhide_all(&self) {
        let log = self
            .w
            .store
            .tombstone_log(&self.w.wiki.id, None, 50)
            .expect("the log");
        let mut done = 0;
        for hide in log.iter().filter(|e| e.act == TombstoneAct::Hide) {
            let a = post_form(
                &self.w,
                "/handbook/-/moderate/unhide",
                Some(&self.maintainer),
                &[("hide", hide.id.as_str())],
            )
            .await;
            assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
            done += 1;
        }
        assert!(done > 0, "there was nothing to restore");
    }

    /// Ask `uri` as every viewer: it answers, says nothing hidden, and says `tombstone` in place of the text.
    async fn says_hidden(&self, uri: &str, tombstone: &str) {
        for who in self.viewers() {
            let a = self.get(uri, who.as_ref()).await;
            assert_eq!(a.status, StatusCode::OK, "{uri} as {who:?}: {}", a.body());
            nothing_hidden_in(&a, uri);
            assert!(
                a.body().contains(tombstone),
                "{uri} as {who:?} does not say `{tombstone}`: {}",
                a.body()
            );
        }
    }
}

/// A write of the API, as `who` (a session), naming `tag` in `If-Match`.
async fn api_post(w: &World, who: &AccountId, uri: &str, tag: &str, body: String) -> Answer {
    use tower::ServiceExt as _;
    let req = axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header(axum::http::header::HOST, crate::common::APP)
        .header("content-type", "application/json")
        .header("x-test-account", who.as_str())
        .header(inklkunn_web::CSRF_HEADER, crate::common::TEST_CSRF)
        .header("if-match", tag)
        .body(axum::body::Body::from(body))
        .unwrap();
    let res = crate::common::site(w).oneshot(req).await.unwrap();
    let (parts, body) = res.into_parts();
    Answer {
        status: parts.status,
        headers: parts.headers,
        bytes: axum::body::to_bytes(body, 1 << 24).await.unwrap().to_vec(),
    }
}

/// The permalink of `Foo` at C.
fn permalink_c() -> String {
    format!("/handbook/-/at/{}/Foo", block_id(3))
}

/// The diff of `Foo` from C to D.
fn diff_c_d() -> String {
    format!("/handbook/-/diff/{}/{}/Foo", block_id(3), block_id(4))
}

/// The surfaces that read `Foo` at C, and the words each says in the text's place.
fn at_c() -> Vec<(String, &'static str)> {
    let (b, c, d) = (block_id(2), block_id(3), block_id(4));
    vec![
        (format!("/handbook/-/at/{c}/Foo"), HIDING),
        (format!("/handbook/-/diff/{c}/{d}/Foo"), HIDING),
        (format!("/handbook/-/diff/{b}/{d}/Foo"), HIDING),
        (
            format!("/handbook/-/api/v1/revisions/{c}?title=Foo"),
            "\"hidden\"",
        ),
        (
            format!("/handbook/-/api/v1/diff?from={c}&to={d}&path=Foo"),
            "\"hidden\"",
        ),
        (
            format!("/handbook/-/api/v1/diff?from={b}&to={d}&path=Foo"),
            "\"hidden\"",
        ),
    ]
}

#[tokio::test]
async fn the_text_a_change_left_a_page_as_is_hidden_at_every_block_that_did_not_write_it_again() {
    let s = story();
    // Before: the permalink of C shows B's text, and so does the diff from C.
    let before = s.get(&permalink_c(), None).await;
    assert!(before.body().contains("ZEBRAPERSONAL"), "{}", before.body());
    let a = s.hide(2).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    for (uri, tombstone) in at_c() {
        s.says_hidden(&uri, tombstone).await;
    }
    // The writer's own permalink, and the page at D (which holds none of it), are as they were.
    let at_d = s
        .get(&format!("/handbook/-/at/{}/Foo", block_id(4)), None)
        .await;
    assert!(
        at_d.body().contains("Foo without the contact."),
        "{}",
        at_d.body()
    );
    // **A page the hidden change did not write is not hidden at its block**: `Bar` at B.
    let bar_at_b = s
        .get(&format!("/handbook/-/at/{}/Bar", block_id(2)), None)
        .await;
    assert_eq!(bar_at_b.status, StatusCode::OK);
    assert!(
        bar_at_b.body().contains("Bar as first written."),
        "{}",
        bar_at_b.body()
    );
    // An unhide gives it all back.
    s.unhide_all().await;
    let back = s.get(&permalink_c(), None).await;
    assert!(back.body().contains("ZEBRAPERSONAL"), "{}", back.body());
    assert!(!back.body().contains(HIDING));
    let diff = s.get(&diff_c_d(), None).await;
    assert!(diff.body().contains("ZEBRAPERSONAL"), "{}", diff.body());
}

#[tokio::test]
async fn a_diff_from_a_block_before_the_hidden_change_says_which_side_is_hidden() {
    let s = story();
    s.hide(2).await;
    // From A (1) to C (3): the later side is B's text.
    let uri = format!("/handbook/-/diff/{}/{}/Foo", block_id(1), block_id(3));
    s.says_hidden(&uri, HIDING).await;
    for who in s.viewers() {
        let a = s.get(&uri, who.as_ref()).await;
        assert!(a.body().contains("later change is hidden"), "{}", a.body());
    }
    // C to D: the earlier side.
    let uri = format!("/handbook/-/diff/{}/{}/Foo", block_id(3), block_id(4));
    for who in s.viewers() {
        let a = s.get(&uri, who.as_ref()).await;
        assert!(a.body().contains(EARLIER), "{}", a.body());
    }
}

/// A proposal of the author's whose base is C: it replaces `Foo`, which C held as B wrote it. Its address.
async fn proposal_on_c(s: &Story) -> String {
    let fields = [
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", state_id(3)),
        ("base_block", block_id(3)),
        ("p0_path", FOO.to_owned()),
        ("p0_title", "Foo".to_owned()),
        ("p0_text", foo("Foo, proposed.")),
        ("summary", "Replace the foo".to_owned()),
        ("sources", String::new()),
    ];
    let borrowed: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let made = post_form(
        &s.w,
        "/handbook/-/proposals/new",
        Some(&s.author),
        &borrowed,
    )
    .await;
    assert_eq!(made.status, StatusCode::SEE_OTHER, "{}", made.body());
    made.header("location").expect("an address").to_owned()
}

#[tokio::test]
async fn a_proposal_written_against_the_hidden_text_shows_no_difference_from_it() {
    let s = story();
    let at = proposal_on_c(&s).await;
    // Before the hiding, the page of the proposal is read against the text it was written on.
    let before = s.get(&at, Some(&s.author)).await;
    assert_eq!(before.status, StatusCode::OK, "{}", before.body());
    assert!(!before.body().contains(EARLIER), "{}", before.body());
    s.hide(2).await;
    for who in s.viewers() {
        let a = s.get(&at, who.as_ref()).await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.body());
        nothing_hidden_in(&a, &at);
        assert!(a.body().contains(EARLIER), "as {who:?}: {}", a.body());
        assert!(a.body().contains(HIDING), "as {who:?}: {}", a.body());
        // What the proposal itself says is still there.
        assert!(a.body().contains("Foo, proposed."), "as {who:?}");
    }
    s.unhide_all().await;
    let back = s.get(&at, Some(&s.author)).await;
    assert!(!back.body().contains(EARLIER), "{}", back.body());
}

#[tokio::test]
async fn the_apis_revise_refuses_to_copy_the_hidden_text_into_a_new_revision() {
    let s = story();
    let at = proposal_on_c(&s).await;
    let id = at
        .split("/proposals/")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .expect("an id")
        .to_owned();
    let revisions = |s: &Story| {
        s.w.store
            .revisions(&ProposalId::parse(&id).expect("an id"), Some(&s.author))
            .expect("the revisions")
            .len()
    };
    let revise = |text: &'static str| {
        let body = serde_json::json!({
            "summary": "Replace the foo, again",
            "files": [{"path": FOO, "text": foo(text)}],
        })
        .to_string();
        (format!("\"{id}-r1\""), body)
    };
    s.hide(2).await;
    let before = revisions(&s);
    // The base of the proposal is C, which holds B's text: the revise would copy it, so it is refused, and nothing of it is said.
    let (tag, body) = revise("Foo, revised.");
    let a = api_post(
        &s.w,
        &s.author,
        &format!("/handbook/-/api/v1/proposals/{id}/revisions"),
        &tag,
        body.clone(),
    )
    .await;
    assert!(
        a.status.is_client_error(),
        "refused: {} {}",
        a.status,
        a.body()
    );
    nothing_hidden_in(&a, "the API's revise");
    assert_eq!(before, revisions(&s), "no revision was made");
    // Once the text is no longer hidden, the same request is made.
    s.unhide_all().await;
    let a = api_post(
        &s.w,
        &s.author,
        &format!("/handbook/-/api/v1/proposals/{id}/revisions"),
        &tag,
        body,
    )
    .await;
    assert!(a.status.is_success(), "{} {}", a.status, a.body());
    assert_eq!(before + 1, revisions(&s));
}

#[tokio::test]
async fn a_rollback_of_the_change_that_took_the_data_out_is_refused_and_a_hidden_change_is_still_undone()
 {
    let s = story();
    let read = &s.w.services.read;
    let plan = |n: u64| {
        read.rollback_plan(&s.w.wiki, None, &block_id(n))
            .expect("a read")
    };
    // Before: D can be undone (it writes the data back); so can B (it takes the data away).
    assert!(matches!(plan(4), RollbackOutcome::Plan(_)));
    assert!(matches!(plan(2), RollbackOutcome::Plan(_)));
    s.hide(2).await;
    // D would write B's text again: refused whole, naming the page and nothing of the text.
    assert_eq!(
        plan(4),
        RollbackOutcome::No(NoRollback::TextHidden {
            titles: vec!["Foo".to_owned()]
        })
    );
    // B itself is undone by writing what was before it: the text that is hidden is taken away and never written.
    match plan(2) {
        RollbackOutcome::Plan(plan) => {
            let all = format!("{plan:?}");
            assert!(!all.contains("ZEBRA"), "{all}");
            assert_eq!(plan.files.len(), 1);
        }
        other => panic!("the hidden change can still be undone: {other:?}"),
    }
    s.unhide_all().await;
    assert!(matches!(plan(4), RollbackOutcome::Plan(_)));
}

#[tokio::test]
async fn why_is_this_here_does_not_follow_a_page_across_a_hidden_text() {
    let s = story();
    for uri in ["/handbook/-/why/Foo", "/handbook/-/api/v1/provenance/Foo"] {
        let a = s.get(uri, None).await;
        assert_eq!(a.status, StatusCode::OK, "{uri}: {}", a.body());
    }
    let followed = s
        .get("/handbook/-/api/v1/provenance/Foo", None)
        .await
        .body();
    s.hide(2).await;
    for who in s.viewers() {
        for uri in ["/handbook/-/why/Foo", "/handbook/-/api/v1/provenance/Foo"] {
            let a = s.get(uri, who.as_ref()).await;
            assert_eq!(a.status, StatusCode::OK, "{uri}: {}", a.body());
            nothing_hidden_in(&a, uri);
        }
    }
    let after = s
        .get("/handbook/-/api/v1/provenance/Foo", None)
        .await
        .body();
    assert_ne!(
        followed, after,
        "the reading does not go across the text that is hidden"
    );
    assert!(after.contains("unfollowed"), "{after}");
    s.unhide_all().await;
    assert_eq!(
        followed,
        s.get("/handbook/-/api/v1/provenance/Foo", None)
            .await
            .body(),
        "an unhide restores it exactly"
    );
}

#[tokio::test]
async fn a_change_that_wrote_the_same_bytes_again_is_its_own_writer() {
    let s = story();
    s.hide(2).await;
    // E writes the very bytes B wrote, by hand: the text at E is E's, and the hiding of B does not reach it.
    s.w.fake.set(FOO, &foo(DATA));
    s.w.fake.say(5, &service_text("Put the contact back"));
    s.w.fake.show(5, shown(vec![op("edit-text", &[FOO])]));
    let at_e = s
        .get(&format!("/handbook/-/at/{}/Foo", block_id(5)), None)
        .await;
    assert_eq!(at_e.status, StatusCode::OK, "{}", at_e.body());
    assert!(at_e.body().contains("ZEBRAPERSONAL"), "{}", at_e.body());
    // Between B and E nothing wrote it, so C is still B's.
    s.says_hidden(&permalink_c(), HIDING).await;
}

#[tokio::test]
async fn a_page_moved_with_its_bytes_is_still_the_hidden_changes_text() {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[(FOO, &foo("Foo as first written."))],
    ));
    let maintainer = person(4);
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    w.fake.say(1, "Create the handbook");
    w.fake.show(1, shown(vec![op("create-file", &[FOO])]));
    w.fake.set(FOO, &foo(DATA));
    w.fake.say(2, &service_text("Add the contact"));
    w.fake.show(2, shown(vec![op("edit-text", &[FOO])]));
    // C moves the page, with its bytes; D edits it at its new place and is the tip.
    w.fake.rename(FOO, BAZ);
    w.fake.say(3, &service_text("Move the page"));
    w.fake.show(3, shown(vec![op("rename-path", &[FOO, BAZ])]));
    w.fake.set(BAZ, &foo("Foo without the contact."));
    w.fake.say(4, &service_text("Take the contact out"));
    w.fake.show(4, shown(vec![op("edit-text", &[BAZ])]));
    let a = post_form(
        &w,
        "/handbook/-/moderate",
        Some(&maintainer),
        &[
            ("kind", "page-revision"),
            ("block", block_id(2).as_str()),
            ("part_content", "1"),
            ("category", "personal-data"),
            ("note", ""),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let uri = format!("/handbook/-/at/{}/Baz", block_id(3));
    let a = get_as(&w, &uri, None).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    nothing_hidden_in(&a, &uri);
    assert!(a.body().contains(HIDING), "{}", a.body());
    // Another page at that block is not hidden.
    let at = get_as(&w, &format!("/handbook/-/at/{}/Baz", block_id(4)), None).await;
    assert!(
        at.body().contains("Foo without the contact."),
        "{}",
        at.body()
    );
}

/// The text of a file at a block, as the reader says it (the core, not a route): the hiding names the change that wrote it.
#[tokio::test]
async fn the_reader_names_the_hiding_of_the_change_that_wrote_the_text_and_gives_none_of_it() {
    let s = story();
    let read = &s.w.services.read;
    let text = |n: u64| {
        read.text_at(&s.w.wiki, None, &block_id(n), FOO)
            .expect("a read")
    };
    assert!(matches!(text(3), BlockText::Text(t) if t.contains("ZEBRAPERSONAL")));
    s.hide(2).await;
    for n in [2, 3] {
        match text(n) {
            BlockText::Hidden(mark) => {
                let events =
                    s.w.store
                        .tombstone_events_of_id(
                            &s.w.wiki.id,
                            inklkunn_store::HiddenItemKind::PageRevision,
                            &block_id(2),
                        )
                        .expect("events");
                assert!(events.iter().any(|e| e.id == mark.event), "block {n}");
            }
            other => panic!("block {n}: {other:?}"),
        }
    }
    assert!(matches!(text(1), BlockText::Text(t) if !t.contains("ZEBRA")));
    assert!(matches!(text(4), BlockText::Text(t) if !t.contains("ZEBRA")));
    // The page `Bar`, which B did not write, is read at B.
    assert!(matches!(
        read.text_at(&s.w.wiki, None, &block_id(2), BAR)
            .expect("a read"),
        BlockText::Text(_)
    ));
}
