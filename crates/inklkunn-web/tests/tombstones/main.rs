//! Tombstones through the real router (RFC 006 D-8, D-12; handoff G-T4; acceptance criterion 8; threat model C-14f, C-15d, C-15f,
//! INV-13): what is hidden is absent from every surface of the table, in HTML and in the API; each surface says why in its
//! place; an unhide restores exactly; hiding the text of a tip is refused; hiding an open proposal's latest revision declines it;
//! *show hidden content* is audited and the `moderate` rung's alone; the note is for that rung only; no name is in a tombstone.
//! The records are the in-memory store's and the repository is a double, so these run in both test runs. Every name and
//! sentence is invented, and every text that must never appear carries the word `ZEBRA`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

mod acts;
mod gate;
mod surfaces;

use axum::http::StatusCode;
use common::{
    Answer, World, accept_by_hand, add_person, block_id, get_as, op, page, person, post_form,
    proposal_message, shown, with_accounts,
};
use inklkunn_store::{AccountId, ProposalId, Role};

pub(crate) const HOME: &str = "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond paragraph.\n";
/// The text the accepted proposal wrote: the hidden text of the fixture.
pub(crate) const HIDDEN_TEXT: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nZEBRATEXT paragraph.\n";
/// The text the page has at its tip: its text cannot be hidden.
pub(crate) const TIP_TEXT: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nPlain tip paragraph.\n";
pub(crate) const SUMMARY: &str = "ZEBRASUMMARY accepted edit";
pub(crate) const AUTHOR: &str = "Zed ZEBRA";
pub(crate) const COMMENT: &str = "ZEBRACOMMENT said on the proposal";
pub(crate) const HIDING: &str = "Hidden by moderation";

pub(crate) struct Env {
    pub(crate) w: World,
    pub(crate) maintainer: AccountId,
    pub(crate) author: AccountId,
    /// Another contributor: the author of the open proposal.
    pub(crate) other: AccountId,
    pub(crate) reader: AccountId,
    /// The accepted proposal (its revision 1 is change 2 of the history).
    pub(crate) accepted: ProposalId,
    /// An open proposal, held for review, with a comment.
    pub(crate) open: ProposalId,
    pub(crate) comment: String,
}

pub(crate) fn fields<'a>(text: &'a str, summary: &'a str) -> Vec<(&'static str, String)> {
    vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", common::state_id(1)),
        ("base_block", block_id(1)),
        ("p0_path", "pages/Home.md".to_owned()),
        ("p0_title", "Home".to_owned()),
        ("p0_text", text.to_owned()),
        ("summary", summary.to_owned()),
        ("sources", String::new()),
    ]
}

pub(crate) async fn propose(w: &World, who: &AccountId, text: &str, summary: &str) -> ProposalId {
    let f = fields(text, summary);
    let borrowed: Vec<(&str, &str)> = f.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let a = post_form(w, "/handbook/-/proposals/new", Some(who), &borrowed).await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let to = a.header("location").unwrap().to_owned();
    let tail = to.split("/proposals/").nth(1).expect("an address");
    ProposalId::parse(tail.split(['/', '?', '#']).next().unwrap()).expect("an id")
}

/// The wiki: the accepted proposal is change 2 (it wrote the hidden text), a service change is the tip (change 3), and an
/// open proposal holds a comment by the author.
pub(crate) async fn env() -> Env {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", HOME)],
    ));
    let (maintainer, author, reader, other) = (person(4), person(1), person(6), person(2));
    add_person(&w, &other, "Ben Okoye", Some(Role::Contributor));
    add_person(&w, &author, AUTHOR, Some(Role::Contributor));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &reader, "Olu Reader", Some(Role::Observer));
    let accepted = propose(&w, &author, HIDDEN_TEXT, SUMMARY).await;
    let open = propose(
        &w,
        &other,
        "---\ntitle: Home\n---\n\nFirst paragraph.\n\nOpen paragraph.\n",
        "An open proposal",
    )
    .await;
    let a = post_form(
        &w,
        &format!("/handbook/-/proposals/{open}/comment"),
        Some(&author),
        &[("body", COMMENT)],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let comment = open_comments(&w, &open);
    // The first proposal was taken into the history as change 2; the service then edited the page (change 3).
    accept_by_hand(&w, &accepted, &maintainer, &author);
    w.fake.say(1, "Create the handbook");
    w.fake
        .show(1, shown(vec![op("create-file", &["pages/Home.md"])]));
    w.fake.set_files(&[("pages/Home.md", HIDDEN_TEXT)]);
    w.fake.say(2, &message(&accepted, &author, &maintainer));
    w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
    w.fake.set_files(&[("pages/Home.md", TIP_TEXT)]);
    w.fake.say(3, &common::service_text("Tidy the home page"));
    w.fake
        .show(3, shown(vec![op("edit-text", &["pages/Home.md"])]));
    w.fake.diff_says(Ok(common::diff_of(
        "pages/Home.md",
        "modified",
        &["@@ -3,3 +3,3 @@\n First paragraph.\n-ZEBRATEXT paragraph.\n+Plain tip paragraph.\n"],
        2,
        3,
    )));
    Env {
        w,
        maintainer,
        author,
        other,
        reader,
        accepted,
        open,
        comment,
    }
}

fn message(id: &ProposalId, proposer: &AccountId, acceptor: &AccountId) -> String {
    use inklkunn_core::message::{Actor, BlockId, Message, Summary};
    use inklkunn_store::DisplayName;
    let actor = |id: &AccountId, name: &str| Actor::Account {
        id: id.clone(),
        name: DisplayName::new(name).expect("name"),
    };
    Message::proposal(
        Summary::new(SUMMARY).expect("summary"),
        id.clone(),
        actor(proposer, AUTHOR),
        actor(acceptor, "Mika Ito"),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a message")
    .to_text()
}

/// The id of the one comment of the proposal.
fn open_comments(w: &World, id: &ProposalId) -> String {
    use inklkunn_store::ProposalStore;
    w.store
        .comments(id, None)
        .unwrap()
        .first()
        .expect("a comment")
        .id
        .to_string()
}

pub(crate) fn change(n: u64) -> String {
    block_id(n)
}

pub(crate) async fn read(e: &Env, uri: &str, who: Option<&AccountId>) -> Answer {
    get_as(&e.w, uri, who).await
}

/// Hide parts of an item as the maintainer. `item` is the item's fields; the answer is the act's.
pub(crate) async fn hide(e: &Env, item: &[(&str, &str)], parts: &[&str], note: &str) -> Answer {
    let mut fields: Vec<(&str, &str)> = item.to_vec();
    for p in parts {
        fields.push((p, "1"));
    }
    fields.push(("category", "personal-data"));
    fields.push(("note", note));
    post_form(&e.w, "/handbook/-/moderate", Some(&e.maintainer), &fields).await
}

pub(crate) fn page_item(n: u64) -> Vec<(&'static str, String)> {
    vec![("kind", "page-revision".to_owned()), ("block", block_id(n))]
}

pub(crate) fn proposal_item(id: &ProposalId, seq: u32) -> Vec<(&'static str, String)> {
    vec![
        ("kind", "proposal-revision".to_owned()),
        ("proposal", id.to_string()),
        ("seq", seq.to_string()),
    ]
}

pub(crate) fn comment_item(proposal: &ProposalId, comment: &str) -> Vec<(&'static str, String)> {
    vec![
        ("kind", "proposal-comment".to_owned()),
        ("proposal", proposal.to_string()),
        ("comment", comment.to_owned()),
    ]
}

pub(crate) fn borrowed<'a>(item: &'a [(&'static str, String)]) -> Vec<(&'a str, &'a str)> {
    item.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

/// The page, with nothing of the hidden text in any byte: the body and every header.
pub(crate) fn nothing_hidden_in(a: &Answer, which: &str) {
    let all = format!("{:?}\n{}", a.headers, a.body());
    for word in ["ZEBRA", "Zebra", "zebra"] {
        assert!(
            !all.contains(word),
            "{which}: `{word}` is in the answer: {all}"
        );
    }
}

#[allow(dead_code)]
pub(crate) fn _keeps(_: &Option<&str>) {
    let _ = proposal_message;
    let _ = page;
}
