//! **A text that cannot be checked yet** (RFC 006 D-8; INV-13; handoff G-T4, correction L-11): when the index of who wrote each
//! path does not know who wrote the text of a block, the text is not given, and **no tombstone is shown**, because no hide is the
//! reason. Every surface that reads a text at a block says *this text cannot be checked yet; try again in a little while* (`503`
//! with `Retry-After`; in the API `unavailable` with the `reason` `not-checked-yet`) or leaves out what needs the text, and its row of
//! the table of surfaces says which (`inklkunn_core::tombstone::surfaces::Unchecked`). The test of each row is here, by the name the
//! row gives. The index is planted by hand without a row for the page, which is one of the cases that leave a text unattributed (a path
//! the index has no row for); the records are the in-memory store's and the repository is a double.

use axum::http::StatusCode;
use inklkunn_core::read::ReadError;
use inklkunn_store::{BlockWrites, DerivedStore, PathWrite, WriteKind, WritesState};

use super::removed::{Shape, gone};
use super::texts::{BAR, proposal_on_c, story};
use super::{HIDING, nothing_hidden_in};
use crate::common::{APP, Answer, World, block_id, get_as, request};

const CANNOT: &str = "This text cannot be checked yet";
const CANNOT_JA: &str = "この本文はまだ確認できません";

/// Plant an index of the default view that reaches change `last` and says that **only `decoy` was ever written**: a path it has no
/// row for is a text nobody is recorded as having written, which is not given.
pub(crate) fn plant(w: &World, last: u64, decoy: &str) {
    let (wiki, view) = (&w.wiki.id, &w.wiki.default_view);
    w.store.reset_writes(wiki, view).expect("a reset");
    let blocks: Vec<BlockWrites> = (1..=last)
        .map(|n| BlockWrites {
            number: n,
            block: block_id(n),
            proposal: None,
            writes: vec![PathWrite {
                path: decoy.to_owned(),
                kind: WriteKind::Wrote,
            }],
        })
        .collect();
    w.store
        .append_writes(
            wiki,
            view,
            &blocks,
            &WritesState {
                head_number: last,
                head_block: block_id(last),
                floor_number: 1,
                genesis: true,
            },
        )
        .expect("an index");
}

/// A page that says the text cannot be checked yet: `503`, `Retry-After`, the sentence, nothing of the text, no tombstone.
fn says_unchecked(a: &Answer, which: &str) {
    assert_eq!(
        a.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{which}: {}",
        a.body()
    );
    assert!(
        a.header("retry-after").is_some(),
        "{which}: no `Retry-After`: {:?}",
        a.headers
    );
    assert!(a.body().contains(CANNOT), "{which}: {}", a.body());
    nothing_hidden_in(a, which);
    assert!(
        !a.body().contains(HIDING),
        "{which}: no tombstone is shown where no hide is the reason: {}",
        a.body()
    );
}

/// The API's answer: the problem `unavailable`, `503`, `Retry-After`, the `reason` `not-checked-yet`, nothing of the text.
fn problem_unchecked(a: &Answer, which: &str) {
    assert_eq!(
        a.status,
        StatusCode::SERVICE_UNAVAILABLE,
        "{which}: {}",
        a.body()
    );
    assert!(
        a.header("retry-after").is_some(),
        "{which}: no `Retry-After`"
    );
    let body: serde_json::Value = serde_json::from_slice(&a.bytes).expect("a problem");
    assert_eq!(
        body.pointer("/type").and_then(|v| v.as_str()),
        Some("urn:inklkunn:problem:unavailable"),
        "{which}: {body}"
    );
    assert_eq!(
        body.pointer("/reason").and_then(|v| v.as_str()),
        Some("not-checked-yet"),
        "{which}: {body}"
    );
    assert_eq!(body.pointer("/status").and_then(|v| v.as_u64()), Some(503));
    nothing_hidden_in(a, which);
    assert!(!a.body().contains("hidden"), "{which}: {body}");
}

/// **The surface `page-as-of`**: the permalink of a page at a block whose writer the index does not know.
#[tokio::test]
pub(crate) async fn unchecked_page_as_of() {
    let s = story();
    s.hide(2).await;
    let (c, uri) = (block_id(3), format!("/handbook/-/at/{}/Foo", block_id(3)));
    // Before the index is planted the text is B's and hidden, with the tombstone: a hide is the reason.
    let before = s.get(&uri, None).await;
    assert_eq!(before.status, StatusCode::OK, "{}", before.body());
    assert!(before.body().contains(HIDING), "{}", before.body());
    plant(&s.w, 4, BAR);
    for who in s.viewers() {
        says_unchecked(&s.get(&uri, who.as_ref()).await, &uri);
    }
    // In Japanese too.
    let ja = request(&s.w, APP, "GET", &uri, &[("accept-language", "ja")]).await;
    assert_eq!(ja.status, StatusCode::SERVICE_UNAVAILABLE, "{}", ja.body());
    assert!(ja.body().contains(CANNOT_JA), "{}", ja.body());
    // A page whose writer the index knows is not touched by it: `Bar` is read, and it is not hidden.
    let bar = s.get(&format!("/handbook/-/at/{c}/Bar"), None).await;
    assert_eq!(bar.status, StatusCode::OK, "{}", bar.body());
    assert!(bar.body().contains("Bar, edited."), "{}", bar.body());
    // The text of the permalink at the tip is the tip's, whose content cannot be hidden.
    assert_eq!(
        s.get("/handbook/Foo", None).await.status,
        StatusCode::OK,
        "the tip is read as a tip"
    );
}

/// **The surface `diff`**: both sides of a difference are read by the one reader, and either side that cannot be checked makes the
/// answer *not checked yet*, in the page and in the API.
#[tokio::test]
pub(crate) async fn unchecked_diff() {
    let s = story();
    s.hide(2).await;
    plant(&s.w, 4, BAR);
    let (b, c, d) = (block_id(2), block_id(3), block_id(4));
    for uri in [
        format!("/handbook/-/diff/{c}/{d}/Foo"),
        format!("/handbook/-/diff/{b}/{d}/Foo"),
    ] {
        for who in s.viewers() {
            says_unchecked(&s.get(&uri, who.as_ref()).await, &uri);
        }
    }
    for uri in [
        format!("/handbook/-/api/v1/diff?from={c}&to={d}&path=Foo"),
        format!("/handbook/-/api/v1/diff?from={b}&to={d}&path=Foo"),
    ] {
        problem_unchecked(&s.get(&uri, None).await, &uri);
    }
}

/// **The surface `change-view`**: the API gives the page as of a revision as a member of the change, which is read by the one reader.
/// (The change's own page lists the paths a change touched, which are read from the log.)
#[tokio::test]
pub(crate) async fn unchecked_change_view() {
    let s = story();
    s.hide(2).await;
    plant(&s.w, 4, BAR);
    let c = block_id(3);
    let uri = format!("/handbook/-/api/v1/revisions/{c}?title=Foo");
    for who in s.viewers() {
        problem_unchecked(&s.get(&uri, who.as_ref()).await, &uri);
    }
    // The change itself, without a page as of it, is read from the log and is not touched.
    let change = s.get(&format!("/handbook/-/change/{c}"), None).await;
    assert_eq!(change.status, StatusCode::OK, "{}", change.body());
}

/// **The surface `proposal-page`**: a proposal written against a base reads the text it was written on, and a text that cannot be
/// checked is not read, so the page is *not checked yet* and says nothing of a hide.
#[tokio::test]
pub(crate) async fn unchecked_proposal_page() {
    let s = story();
    let at = proposal_on_c(&s).await;
    s.hide(2).await;
    plant(&s.w, 4, BAR);
    for who in s.viewers() {
        says_unchecked(&s.get(&at, who.as_ref()).await, &at);
    }
}

/// **The surface `restore-offer`**: the text a removed page would come back with is read by the one reader, and not checked is no
/// offer, and no sentence that a hide is the reason.
#[tokio::test]
pub(crate) async fn unchecked_restore_offer() {
    let g = gone(Shape::Clean);
    g.hide(2, &["part_content"]).await;
    plant(&g.w, 4, "pages/Home.md");
    for who in [Some(&g.maintainer), Some(&g.author)] {
        says_unchecked(&g.get("/handbook/-/restore/Guide", who).await, "restore");
    }
}

/// **The surface `why-is-this-here`**: the source files a change attached are read by the one reader, and one that cannot be checked
/// is not read and is counted with the files that could not be listed (the page says how many were not): no text, and no tombstone.
#[tokio::test]
pub(crate) async fn unchecked_why_is_this_here() {
    let (w, maintainer) = super::changes::attaching();
    let a = crate::common::post_form(
        &w,
        "/handbook/-/moderate",
        Some(&maintainer),
        &[
            ("kind", "page-revision"),
            ("block", block_id(1).as_str()),
            ("part_content", "1"),
            ("category", "personal-data"),
            ("note", ""),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    plant(&w, 3, "pages/Home.md");
    let attached = w
        .services
        .read
        .attached_sources(&w.wiki, &block_id(2), 10)
        .expect("a list");
    assert!(attached.sources.is_empty(), "{attached:?}");
    assert_eq!(attached.unreadable, 1, "{attached:?}");
    assert!(attached.hidden.is_none(), "no tombstone: {attached:?}");
    for uri in ["/handbook/-/why/Home", "/handbook/-/api/v1/provenance/Home"] {
        let a = get_as(&w, uri, None).await;
        assert_eq!(a.status, StatusCode::OK, "{uri}: {}", a.body());
        nothing_hidden_in(&a, uri);
        assert!(!a.body().contains("harbour survey"), "{uri}");
        // No hide is said of the sources (the hidden change 1 is a hidden change, and its own tombstone is its own).
        assert!(
            !a.body().contains("hidden by moderation"),
            "{uri}: {}",
            a.body()
        );
    }
    let page = get_as(&w, "/handbook/-/why/Home", None).await.body();
    assert!(
        page.contains("1 source file added in this change cannot be read."),
        "the page says how many were not listed: {page}"
    );
}

/// **The surface `rules-history`**: the changes of the rules are read file by file at each change, and a text that cannot be checked
/// leaves the list out (the page of the rules is served without it), and says no hide.
#[tokio::test]
pub(crate) async fn unchecked_rules_history() {
    let w = crate::common::with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Home.md", "---\ntitle: Home\n---\n\nHome.\n"),
            ("governance/policy.yaml", "entry: none\n"),
        ],
    ));
    let maintainer = crate::common::person(4);
    crate::common::add_person(
        &w,
        &maintainer,
        "Mika Ito",
        Some(inklkunn_store::Role::Maintainer),
    );
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        crate::common::shown(vec![
            crate::common::op("create-file", &["pages/Home.md"]),
            crate::common::op("create-file", &["governance/policy.yaml"]),
        ]),
    );
    w.fake
        .set("pages/Home.md", "---\ntitle: Home\n---\n\nHome again.\n");
    w.fake
        .say(2, &crate::common::service_text("Tidy the home page"));
    w.fake.show(
        2,
        crate::common::shown(vec![crate::common::op("edit-text", &["pages/Home.md"])]),
    );
    w.fake.set(
        "pages/Home.md",
        "---\ntitle: Home\n---\n\nHome, a third time.\n",
    );
    w.fake.say(3, &crate::common::service_text("Tidy it again"));
    w.fake.show(
        3,
        crate::common::shown(vec![crate::common::op("edit-text", &["pages/Home.md"])]),
    );
    // Something is hidden (the content of change 2), so every text is asked whose it is.
    let a = crate::common::post_form(
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
    plant(&w, 3, "pages/Home.md");
    let got = w.services.read.rule_history(&w.wiki);
    assert!(matches!(got, Err(ReadError::NotCheckedYet)), "{got:?}");
    // The page of the rules is served without the list.
    let page = get_as(&w, "/handbook/-/policy", Some(&maintainer)).await;
    assert_eq!(page.status, StatusCode::OK, "{}", page.body());
    assert!(!page.body().contains(HIDING), "{}", page.body());
}
