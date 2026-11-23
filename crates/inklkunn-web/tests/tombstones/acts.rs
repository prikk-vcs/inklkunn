//! The acts of the `moderate` rung: hide, restore, show hidden content, and what each leaves in the records, the audit trail and
//! the log (RFC 006 D-8; acceptance criterion 8; threat model C-14f, C-15f).

use axum::http::StatusCode;
use inklkunn_store::{
    AccountId, GovernanceStore, ProposalState, ProposalStore, TombstoneAct, TombstoneEvent,
};

use super::{
    Env, HIDING, borrowed, change, comment_item, env, hide, nothing_hidden_in, page_item,
    proposal_item, read,
};
use crate::common::{add_person, post_form};
use inklkunn_store::Role;

const ALL: &[&str] = &["part_content", "part_summary", "part_author"];

fn log(e: &Env) -> Vec<TombstoneEvent> {
    e.w.store
        .tombstone_log(&e.w.wiki.id, None, 50)
        .expect("the log")
}

/// The pages a hiding of change 2 touches, as every viewer reads them.
pub(crate) async fn pages_of_change_two(
    e: &Env,
    who: Option<&AccountId>,
) -> Vec<(String, Vec<u8>)> {
    let b = change(2);
    let mut out = Vec::new();
    for uri in [
        "/handbook/-/history/Home".to_owned(),
        format!("/handbook/-/change/{b}"),
        format!("/handbook/-/at/{b}/Home"),
        format!("/handbook/-/diff/{b}/{}/Home", change(3)),
        "/handbook/-/changes".to_owned(),
        "/handbook/-/api/v1/history/Home".to_owned(),
        format!("/handbook/-/api/v1/revisions/{b}?title=Home"),
        format!("/handbook/-/proposals/{}", e.accepted),
    ] {
        let a = read(e, &uri, who).await;
        assert_eq!(a.status, StatusCode::OK, "{uri}");
        out.push((uri, a.bytes));
    }
    out
}

#[tokio::test]
async fn an_unhide_restores_every_surface_exactly() {
    let e = env().await;
    let before = pages_of_change_two(&e, None).await;
    let item = page_item(2);
    let a = hide(&e, &borrowed(&item), ALL, "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert_ne!(
        before,
        pages_of_change_two(&e, None).await,
        "the hiding changed the pages"
    );
    // Restore every hide in force, as the log's own buttons do.
    let hides: Vec<_> = log(&e)
        .into_iter()
        .filter(|ev| ev.act == TombstoneAct::Hide)
        .collect();
    assert!(!hides.is_empty());
    for h in &hides {
        let a = post_form(
            &e.w,
            "/handbook/-/moderate/unhide",
            Some(&e.maintainer),
            &[("hide", h.id.as_str())],
        )
        .await;
        assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    }
    assert_eq!(
        before,
        pages_of_change_two(&e, None).await,
        "byte for byte, as before"
    );
    // An unhide follows its hide once.
    let again = post_form(
        &e.w,
        "/handbook/-/moderate/unhide",
        Some(&e.maintainer),
        &[("hide", hides.first().unwrap().id.as_str())],
    )
    .await;
    assert_eq!(again.status, StatusCode::NOT_FOUND, "{}", again.body());
}

#[tokio::test]
async fn the_text_of_a_tip_cannot_be_hidden_but_its_summary_and_author_can() {
    let e = env().await;
    let item = page_item(3);
    let refused = hide(&e, &borrowed(&item), &["part_content"], "").await;
    assert_eq!(refused.status, StatusCode::CONFLICT, "{}", refused.body());
    let body = refused.body();
    // It says why, and offers the way back: the change, where it is undone by a proposal.
    assert!(body.contains("current text"), "{body}");
    assert!(
        body.contains(&format!("/handbook/-/change/{}", change(3))),
        "{body}"
    );
    assert!(log(&e).is_empty(), "nothing was recorded");
    // Its summary and its author can be hidden, and the history names neither where it names the last change.
    let a = hide(&e, &borrowed(&item), &["part_summary", "part_author"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let history = read(&e, "/handbook/-/history/Home", None).await.body();
    assert!(!history.contains("Tidy the home page"), "{history}");
    assert!(history.contains(HIDING), "{history}");
    // The text of the tip is still the page.
    let page = read(&e, "/handbook/Home", None).await.body();
    assert!(page.contains("Plain tip paragraph."), "{page}");
}

#[tokio::test]
async fn hiding_an_open_proposals_latest_revision_declines_it_in_the_same_act() {
    let e = env().await;
    let item = proposal_item(&e.open, 1);
    let a = hide(&e, &borrowed(&item), &["part_content"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert!(
        a.body().contains("the proposal was declined"),
        "{}",
        a.body()
    );
    let proposal = e.w.store.proposal(&e.open, None).unwrap().unwrap();
    assert_eq!(proposal.state, ProposalState::Declined);
    let transitions = e.w.store.transitions(&e.open, None).unwrap();
    let decline = transitions.last().unwrap();
    assert_eq!(decline.actor, e.maintainer);
    assert!(
        decline
            .reason
            .as_deref()
            .is_some_and(|r| r.contains("personal data")),
        "{decline:?}"
    );
    // The tombstone event names the decline's own record.
    let made = log(&e)
        .into_iter()
        .find(|ev| ev.act == TombstoneAct::Hide)
        .unwrap();
    assert!(made.declined_transition.is_some());
    // The summary or the author of a revision alone does not decline a proposal that can still be read.
    let e2 = env().await;
    let item = proposal_item(&e2.open, 1);
    let a = hide(&e2, &borrowed(&item), &["part_summary"], "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let proposal = e2.w.store.proposal(&e2.open, None).unwrap().unwrap();
    assert_eq!(proposal.state, ProposalState::HeldForReview);
}

#[tokio::test]
async fn show_hidden_content_is_audited_and_only_the_moderate_rung_gets_it() {
    let e = env().await;
    let item = page_item(2);
    let a = hide(&e, &borrowed(&item), ALL, "").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let hides: Vec<TombstoneEvent> = log(&e)
        .into_iter()
        .filter(|ev| ev.act == TombstoneAct::Hide)
        .collect();
    let content = hides.iter().find(|h| h.parts.content).unwrap();
    // A page revision's content is recorded against the page's repository path (the export names the page by it).
    assert_eq!(content.item.reference.as_deref(), Some("pages/Home.md"));
    let mut fields: Vec<(&str, &str)> = borrowed(&item);
    fields.push(("hide", content.id.as_str()));
    for who in [None, Some(&e.reader), Some(&e.author)] {
        let a = match who {
            Some(w) => post_form(&e.w, "/handbook/-/moderate/show", Some(w), &fields).await,
            None => post_form(&e.w, "/handbook/-/moderate/show", None, &fields).await,
        };
        assert!(
            a.status.is_client_error(),
            "{who:?} may not see hidden content: {}",
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
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert!(a.body().contains("ZEBRATEXT"), "{}", a.body());
    assert_eq!(a.header("cache-control"), Some("no-store"));
    let shown: Vec<_> = e.w.audit.events().into_iter().skip(asked_before).collect();
    let line = shown
        .iter()
        .find(|ev| ev.action == "tombstone-show")
        .unwrap_or_else(|| panic!("the act is in the audit trail: {:?}", e.w.audit.events()));
    assert!(
        line.resource
            .starts_with(&format!("tombstone:{}:", e.w.wiki.id)),
        "{line:?}"
    );
    // One line, with the rung and the rules, and never content.
    assert!(line.detail.contains("rung=maintainer; policy="), "{line:?}");
    for ev in e.w.audit.events() {
        let all = format!("{ev:?}");
        assert!(!all.contains("ZEBRA"), "the trail holds content: {all}");
    }
}

#[tokio::test]
async fn the_note_is_for_the_moderate_rung_and_no_name_is_in_a_tombstone() {
    let e = env().await;
    // Another maintainer hides, so that her name is told apart from the acceptor's.
    let max = crate::common::person(9);
    add_person(&e.w, &max, "Max Mori", Some(Role::Maintainer));
    let item = page_item(2);
    let mut fields = borrowed(&item);
    for p in ALL {
        fields.push((p, "1"));
    }
    fields.push(("category", "harassment"));
    fields.push(("note", "ZEBRANOTE for the moderators"));
    let a = post_form(&e.w, "/handbook/-/moderate", Some(&max), &fields).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    // The note is in the log for the moderate rung, and nowhere for anyone else.
    let as_moderator = read(&e, "/handbook/-/moderation", Some(&e.maintainer))
        .await
        .body();
    assert!(as_moderator.contains("ZEBRANOTE"), "{as_moderator}");
    for who in [None, Some(&e.reader), Some(&e.author)] {
        for uri in [
            "/handbook/-/moderation",
            "/handbook/-/history/Home",
            "/handbook/-/change/b",
        ] {
            let uri = uri.replace("/b", &format!("/{}", change(2)));
            let a = read(&e, &uri, who).await;
            assert!(!a.body().contains("ZEBRANOTE"), "{uri}: the note is shown");
        }
    }
    // The tombstone says the category, the role and the date, never the name of who hid.
    for who in [None, Some(&e.maintainer)] {
        for uri in [
            "/handbook/-/moderation".to_owned(),
            "/handbook/-/history/Home".to_owned(),
            format!("/handbook/-/change/{}", change(2)),
            format!("/handbook/-/api/v1/revisions/{}", change(2)),
        ] {
            let a = read(&e, &uri, who).await;
            assert!(!a.body().contains("Max Mori"), "{uri}: names who hid");
        }
    }
    let log_page = read(&e, "/handbook/-/moderation", None).await.body();
    assert!(log_page.contains("harassment"), "{log_page}");
    assert!(log_page.contains("maintainer"), "{log_page}");
    nothing_hidden_in(&read(&e, "/handbook/-/moderation", None).await, "the log");
}

#[tokio::test]
async fn only_the_moderate_rung_hides_and_a_refusal_changes_nothing() {
    let e = env().await;
    let item = page_item(2);
    let mut fields = borrowed(&item);
    fields.push(("part_content", "1"));
    fields.push(("category", "spam"));
    fields.push(("note", ""));
    for who in [&e.reader, &e.author, &e.other] {
        let a = post_form(&e.w, "/handbook/-/moderate", Some(who), &fields).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{who}: {}", a.body());
    }
    let a = post_form(&e.w, "/handbook/-/moderate", None, &fields).await;
    assert!(a.status.is_client_error(), "{}", a.status);
    assert!(log(&e).is_empty());
    // A comment, a proposal's revision, and a change: each is refused to everyone else, and nothing is hidden.
    let comment = comment_item(&e.open, &e.comment);
    let mut c = borrowed(&comment);
    c.push(("part_content", "1"));
    c.push(("category", "spam"));
    let a = post_form(&e.w, "/handbook/-/moderate", Some(&e.reader), &c).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(log(&e).is_empty());
}

#[tokio::test]
async fn the_log_lists_the_item_the_category_the_role_and_the_date_never_the_content() {
    let e = env().await;
    let item = proposal_item(&e.accepted, 1);
    let a = hide(&e, &borrowed(&item), ALL, "ZEBRANOTE").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    for who in [None, Some(&e.reader), Some(&e.maintainer)] {
        let a = read(&e, "/handbook/-/moderation", who).await;
        assert_eq!(a.status, StatusCode::OK);
        let body = a.body();
        assert!(body.contains("Hidden: A change in the history"), "{body}");
        assert!(body.contains("personal data"), "{body}");
        assert!(body.contains("Role: maintainer"), "{body}");
        for word in ["ZEBRATEXT", "ZEBRASUMMARY", "Zed"] {
            assert!(!body.contains(word), "{word} is in the log");
        }
    }
    // Only the moderate rung is offered the way to restore.
    let moderator = read(&e, "/handbook/-/moderation", Some(&e.maintainer))
        .await
        .body();
    assert!(
        moderator.contains("/handbook/-/moderate/unhide"),
        "{moderator}"
    );
    let reader = read(&e, "/handbook/-/moderation", Some(&e.reader))
        .await
        .body();
    assert!(!reader.contains("/handbook/-/moderate/unhide"), "{reader}");
}

#[tokio::test]
async fn the_form_is_offered_to_the_moderate_rung_alone_where_a_change_is_shown() {
    let e = env().await;
    let uri = format!("/handbook/-/change/{}", change(2));
    let form = format!(
        "/handbook/-/moderate?kind=page-revision&block={}",
        change(2)
    );
    let moderator = read(&e, &uri, Some(&e.maintainer)).await.body();
    assert!(
        moderator.contains(&form.replace('&', "&#38;")),
        "{moderator}"
    );
    for who in [None, Some(&e.reader), Some(&e.author)] {
        let body = read(&e, &uri, who).await.body();
        assert!(!body.contains("/handbook/-/moderate"), "{body}");
    }
    // The form itself says what still holds the content.
    let page = read(&e, &form, Some(&e.maintainer)).await.body();
    assert!(
        page.contains("prikk keeps every object"),
        "the form does not say what still holds the content: {page}"
    );
    let refused = read(&e, &form, Some(&e.reader)).await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN);
}
