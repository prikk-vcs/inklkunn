//! **Showing hidden content, and the unhide, for each kind of item** (RFC 006 D-8; acceptance criterion 8; threat model C-15f; readiness
//! gaps 8-ii and 8-iii): a page revision's content is shown in `acts.rs`; this holds the other kinds that carry text a person wrote, each
//! in the same four steps. A reader sees a tombstone and none of the text, on every surface that carried it, whoever the viewer; the
//! `moderate` rung's explicit act (a `POST`, audited with the rung and the rules, and never with the content) shows the text, no one
//! else's does, and showing it unhides nothing; an unhide gives every surface back byte for byte.
//!
//! **A new kind of item that carries text is added to [`KINDS`].**

use axum::http::StatusCode;
use inklkunn_store::{AccountId, GovernanceStore, TombstoneAct, TombstoneEvent};

use super::{
    Env, borrowed, comment_item, env, hide, nothing_hidden_in, page_item, proposal_item, read,
};
use crate::common::post_form;

/// One kind of item that carries text.
struct Kind {
    /// What it is.
    name: &'static str,
    /// The item's fields, as the form of the act names it.
    item: fn(&Env) -> Vec<(&'static str, String)>,
    /// The parts hidden: the text, and what else its tombstone covers.
    parts: &'static [&'static str],
    /// The addresses (HTML and API) that carried the text.
    surfaces: fn(&Env) -> Vec<String>,
    /// What the act that shows says: a word of the text that was hidden.
    revealed: &'static str,
}

/// **The table.** A kind of item the moderate rung can hide that carries text is a row; the rows are the kinds of
/// `HiddenItemKind` that have a text (a claim's thread comment is held beside them, in `surfaces.rs`, over a wiki of claims).
const KINDS: &[Kind] = &[
    Kind {
        name: "a page revision",
        item: |_| page_item(2),
        parts: &["part_content", "part_summary", "part_author"],
        surfaces: |_| {
            vec![
                "/handbook/-/history/Home".to_owned(),
                format!("/handbook/-/change/{}", super::change(2)),
                format!("/handbook/-/at/{}/Home", super::change(2)),
                "/handbook/-/api/v1/history/Home".to_owned(),
            ]
        },
        revealed: "ZEBRATEXT",
    },
    Kind {
        name: "a comment on a proposal",
        item: |e| comment_item(&e.open, &e.comment),
        parts: &["part_content", "part_author"],
        surfaces: |e| {
            vec![
                format!("/handbook/-/proposals/{}", e.open),
                format!("/handbook/-/api/v1/proposals/{}", e.open),
                "/handbook/-/discussion/Home".to_owned(),
            ]
        },
        revealed: "ZEBRACOMMENT",
    },
    Kind {
        name: "a revision of a proposal",
        item: |e| proposal_item(&e.accepted, 1),
        parts: &["part_content", "part_summary", "part_author"],
        surfaces: |e| {
            vec![
                format!("/handbook/-/proposals/{}", e.accepted),
                format!("/handbook/-/api/v1/proposals/{}", e.accepted),
                "/handbook/-/proposals".to_owned(),
            ]
        },
        revealed: "ZEBRATEXT",
    },
];

fn log(e: &Env) -> Vec<TombstoneEvent> {
    e.w.store
        .tombstone_log(&e.w.wiki.id, None, 50)
        .expect("the log")
}

/// Everyone who may look: signed out, an observer, the author, the moderator.
fn viewers(e: &Env) -> Vec<Option<AccountId>> {
    vec![
        None,
        Some(e.reader.clone()),
        Some(e.author.clone()),
        Some(e.maintainer.clone()),
    ]
}

/// Every surface of the kind as every viewer reads it: the bytes and the headers' text.
async fn read_all(e: &Env, kind: &Kind) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for uri in (kind.surfaces)(e) {
        for who in viewers(e) {
            let a = read(e, &uri, who.as_ref()).await;
            assert_eq!(a.status, StatusCode::OK, "{} {uri} as {who:?}", kind.name);
            out.push((format!("{uri} as {who:?}"), a.bytes));
        }
    }
    out
}

#[tokio::test]
async fn the_moderate_rung_shows_the_hidden_text_of_each_kind_of_item_to_nobody_else_and_an_unhide_restores_every_surface()
 {
    for kind in KINDS {
        let e = env().await;
        // Before: the text is where a reader finds it, on one surface at least.
        let before = read_all(&e, kind).await;
        assert!(
            before
                .iter()
                .any(|(_, bytes)| String::from_utf8_lossy(bytes).contains(kind.revealed)),
            "{}: the text is on no surface before it is hidden, so the test shows nothing",
            kind.name
        );

        // Hidden: a tombstone, and none of the text, for every viewer on every surface.
        let item = (kind.item)(&e);
        let a = hide(&e, &borrowed(&item), kind.parts, "").await;
        assert_eq!(a.status, StatusCode::OK, "{}: {}", kind.name, a.body());
        let hidden = read_all(&e, kind).await;
        assert_ne!(
            before, hidden,
            "{}: the hiding changed no surface",
            kind.name
        );
        for (which, bytes) in &hidden {
            assert!(
                !String::from_utf8_lossy(bytes).contains(kind.revealed),
                "{}: {which} still says the text",
                kind.name
            );
        }

        // The hide that covers the content, and the act that shows it.
        let content = log(&e)
            .into_iter()
            .find(|ev| ev.act == TombstoneAct::Hide && ev.parts.content)
            .unwrap_or_else(|| panic!("{}: no hide of the content is in the log", kind.name));
        let mut fields: Vec<(&str, &str)> = borrowed(&item);
        fields.push(("hide", content.id.as_str()));
        // Nobody else: signed out, an observer, the author of the text.
        for who in [None, Some(&e.reader), Some(&e.author), Some(&e.other)] {
            let a = post_form(&e.w, "/handbook/-/moderate/show", who, &fields).await;
            assert!(
                a.status.is_client_error(),
                "{}: {who:?} may not see hidden content: {}",
                kind.name,
                a.status
            );
            nothing_hidden_in(&a, "show as someone without the rung");
        }
        let asked_before = e.w.audit.events().len();
        let a = post_form(
            &e.w,
            "/handbook/-/moderate/show",
            Some(&e.maintainer),
            &fields,
        )
        .await;
        assert_eq!(a.status, StatusCode::OK, "{}: {}", kind.name, a.body());
        assert!(
            a.body().contains(kind.revealed),
            "{}: the act that shows does not show: {}",
            kind.name,
            a.body()
        );
        assert_eq!(a.header("cache-control"), Some("no-store"), "{}", kind.name);
        // Audited, with the rung and the rules and never the content.
        let line =
            e.w.audit
                .events()
                .into_iter()
                .skip(asked_before)
                .find(|ev| ev.action == "tombstone-show")
                .unwrap_or_else(|| panic!("{}: the act is not in the audit trail", kind.name));
        assert!(
            line.detail.contains("rung=maintainer; policy="),
            "{}: {line:?}",
            kind.name
        );
        for ev in e.w.audit.events() {
            assert!(
                !format!("{ev:?}").contains("ZEBRA"),
                "{}: the trail holds content: {ev:?}",
                kind.name
            );
        }
        // Showing hides nothing less: the surfaces are as they were hidden.
        assert_eq!(
            hidden,
            read_all(&e, kind).await,
            "{}: showing hidden content changed what a reader is shown",
            kind.name
        );

        // An unhide gives every surface back, byte for byte, for every hide of the act.
        for hide in log(&e)
            .into_iter()
            .filter(|ev| ev.act == TombstoneAct::Hide)
        {
            let a = post_form(
                &e.w,
                "/handbook/-/moderate/unhide",
                Some(&e.maintainer),
                &[("hide", hide.id.as_str())],
            )
            .await;
            assert_eq!(
                a.status,
                StatusCode::SEE_OTHER,
                "{}: {}",
                kind.name,
                a.body()
            );
        }
        assert_eq!(
            before,
            read_all(&e, kind).await,
            "{}: an unhide did not restore every surface exactly",
            kind.name
        );
    }
}

#[tokio::test]
async fn the_comment_of_a_claims_thread_is_hidden_shown_by_the_moderate_rung_alone_and_restored_by_an_unhide()
 {
    use crate::common::{
        Answer, add_person, claim, claim_id, get_as, home_with, person, put, with_accounts,
    };
    use inklkunn_core::knowledge::Standing;
    use inklkunn_store::{ClaimThreadStore, Role};

    const SAID: &str = "The bridge opened in 1950.";
    let w = crate::common::World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home_with(SAID))],
    );
    put(
        &w,
        &home_with(SAID),
        &[claim(1, Standing::Contested, None, SAID)],
        &[],
    );
    let w = with_accounts(w);
    let (questioner, maintainer, reader) = (person(1), person(4), person(6));
    add_person(&w, &questioner, "Zed Zebra", Some(Role::Questioner));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &reader, "Olu Reader", Some(Role::Observer));
    let said: Answer = post_form(
        &w,
        &format!("/handbook/-/claim/{}/discuss", claim_id(1)),
        Some(&questioner),
        &[("body", "ZEBRACLAIM said in the thread")],
    )
    .await;
    assert_eq!(said.status, StatusCode::SEE_OTHER, "{}", said.body());
    let comment = w
        .store
        .claim_comments(&w.wiki.id, &claim_id(1).to_string(), None, 10)
        .unwrap()
        .rows
        .first()
        .expect("a comment")
        .comment
        .id
        .to_string();
    let uri = format!("/handbook/-/claim/{}", claim_id(1));
    let viewers = [None, Some(&reader), Some(&questioner), Some(&maintainer)];
    let read_all = || async {
        let mut out = Vec::new();
        for who in viewers {
            out.push(get_as(&w, &uri, who).await.bytes);
        }
        out
    };
    let before = read_all().await;
    assert!(
        String::from_utf8_lossy(&before[0]).contains("ZEBRACLAIM"),
        "the text is not on the page before it is hidden"
    );
    let item = [
        ("kind", "claim-comment"),
        ("claim", &claim_id(1).to_string()),
        ("comment", comment.as_str()),
    ];
    let mut hide_fields: Vec<(&str, &str)> = item.to_vec();
    hide_fields.extend([
        ("part_content", "1"),
        ("part_author", "1"),
        ("category", "spam"),
        ("note", ""),
    ]);
    let a = post_form(&w, "/handbook/-/moderate", Some(&maintainer), &hide_fields).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let hidden = read_all().await;
    for bytes in &hidden {
        assert!(!String::from_utf8_lossy(bytes).contains("ZEBRACLAIM"));
    }
    let hide = w
        .store
        .tombstone_log(&w.wiki.id, None, 10)
        .unwrap()
        .into_iter()
        .find(|ev| ev.act == TombstoneAct::Hide && ev.parts.content)
        .expect("a hide of the content");
    let mut show: Vec<(&str, &str)> = item.to_vec();
    show.push(("hide", hide.id.as_str()));
    for who in [None, Some(&reader), Some(&questioner)] {
        let a = post_form(&w, "/handbook/-/moderate/show", who, &show).await;
        assert!(a.status.is_client_error(), "{who:?}: {}", a.status);
        assert!(!a.body().contains("ZEBRACLAIM"), "{who:?}: {}", a.body());
    }
    let a = post_form(&w, "/handbook/-/moderate/show", Some(&maintainer), &show).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert!(a.body().contains("ZEBRACLAIM"), "{}", a.body());
    assert_eq!(a.header("cache-control"), Some("no-store"));
    assert!(
        w.audit
            .events()
            .iter()
            .any(|ev| ev.action == "tombstone-show" && ev.detail.contains("rung=maintainer")),
        "the act is not in the audit trail"
    );
    assert_eq!(hidden, read_all().await, "showing changed what is shown");
    for hide in w
        .store
        .tombstone_log(&w.wiki.id, None, 10)
        .unwrap()
        .into_iter()
        .filter(|ev| ev.act == TombstoneAct::Hide)
    {
        let a = post_form(
            &w,
            "/handbook/-/moderate/unhide",
            Some(&maintainer),
            &[("hide", hide.id.as_str())],
        )
        .await;
        assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    }
    assert_eq!(
        before,
        read_all().await,
        "an unhide did not restore the page"
    );
}
