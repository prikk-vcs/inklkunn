//! The changes a chain names, with what the proposal records and the acceptance records say of them: the people as
//! the message records them, the proposal, its review, the sources given with it, and what is left out when the
//! visitor may not read the proposal or the records do not tie it to the change.

use axum::http::StatusCode;
use inklkunn_core::message::{Actor, BlockId, Message, Summary};
use inklkunn_store::{AccountId, DisplayName, ProposalId, Role};

use super::common::{
    Answer, World, accept_by_hand, add_person, block_id, get, get_as, op, page, person, post_form,
    shown, with_accounts,
};
use super::main_of;

const HOME: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n";
const BETTER: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond paragraph, better.\n\nThird paragraph.\n";

struct Cast {
    w: World,
    contributor: AccountId,
    reviewer: AccountId,
    maintainer: AccountId,
}

fn cast() -> Cast {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Home.md", HOME),
            ("pages/Guide.md", &page(&["title: Guide"], "The guide.\n")),
        ],
    ));
    let c = Cast {
        contributor: person(1),
        reviewer: person(3),
        maintainer: person(4),
        w,
    };
    add_person(&c.w, &c.contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&c.w, &c.reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(&c.w, &c.maintainer, "Mika Ito", Some(Role::Maintainer));
    c
}

/// The fields of the form that proposes `text` for `Home` against version 1.
fn fields<'a>(
    text: &'a str,
    summary: &'a str,
    sources: &'a str,
    op: &'a str,
) -> Vec<(&'a str, String)> {
    vec![
        ("op", op.to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", super::common::state_id(1)),
        ("base_block", block_id(1)),
        ("p0_path", "pages/Home.md".to_owned()),
        ("p0_title", "Home".to_owned()),
        ("p0_text", text.to_owned()),
        ("summary", summary.to_owned()),
        ("sources", sources.to_owned()),
    ]
}

/// Open a proposal of `BETTER` as the contributor; its id.
async fn propose(c: &Cast, summary: &str, sources: &str, op: &str) -> ProposalId {
    let fields = fields(BETTER, summary, sources, op);
    let borrowed: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let a = post_form(
        &c.w,
        "/handbook/-/proposals/new",
        Some(&c.contributor),
        &borrowed,
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let location = a.header("location").expect("a location").to_owned();
    let tail = location
        .split("/proposals/")
        .nth(1)
        .expect("a proposal's address");
    ProposalId::parse(tail.split(['/', '?', '#']).next().expect("an id")).expect("a proposal id")
}

fn message(proposal: &ProposalId, c: &Cast) -> String {
    let actor = |id: &AccountId, name: &str| Actor::Account {
        id: id.clone(),
        name: DisplayName::new(name).expect("name"),
    };
    Message::proposal(
        Summary::new("Improve the second paragraph").expect("summary"),
        proposal.clone(),
        actor(&c.contributor, "Aiko Tanaka"),
        actor(&c.maintainer, "Mika Ito"),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a proposal message")
    .to_text()
}

/// The repository's history: version 1 creates the pages, version 2 is the proposal taken in (its message names
/// `proposal`).
fn history(c: &Cast, proposal: &ProposalId) {
    c.w.fake.say(1, "Create the handbook");
    c.w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    c.w.fake.set_files(&[("pages/Home.md", BETTER)]);
    c.w.fake.say(2, &message(proposal, c));
    c.w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
}

async fn why(c: &Cast, account: Option<&AccountId>) -> Answer {
    get_as(&c.w, "/handbook/-/why/Home", account).await
}

/// A change's own block on the page: from its heading to the next change's.
fn change_block(body: &str, number: u64) -> String {
    let start = body
        .find(&format!("<li id=\"c-{number}\">"))
        .expect("the change is listed");
    let rest = body.get(start..).expect("the rest");
    let end = ["</li>\n</ol>", "</li>\n<li id=\"c-"]
        .iter()
        .filter_map(|marker| rest.find(marker))
        .min()
        .expect("its end");
    rest.get(..end).expect("the change").to_owned()
}

#[tokio::test]
async fn a_change_from_a_proposal_names_its_people_its_proposal_its_review_and_the_sources_given() {
    let c = cast();
    let id = propose(
        &c,
        "Improve the second paragraph",
        "https://example.org/report\nA book, page 12",
        "submit",
    )
    .await;
    let uri = format!("/handbook/-/proposals/{id}");
    post_form(
        &c.w,
        &format!("{uri}/comment"),
        Some(&c.reviewer),
        &[("body", "Looks fine to me")],
    )
    .await;
    post_form(
        &c.w,
        &format!("{uri}/decide"),
        Some(&c.reviewer),
        &[
            ("op", "approve"),
            ("revision", "1"),
            ("body", "Ready to take in"),
        ],
    )
    .await;
    accept_by_hand(&c.w, &id, &c.maintainer, &c.contributor);
    history(&c, &id);

    let a = why(&c, None).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let body = main_of(&a);
    // The paragraph the proposal changed names the change; the others were written with the page.
    assert!(
        body.contains("Latest change found in <a href=\"/handbook/-/why/Home#c-2\">Change 2</a>."),
        "{body}"
    );
    let two = change_block(&body, 2);
    assert!(two.contains("<h3>Change 2</h3>"), "{two}");
    assert!(
        two.contains("<p class=\"summary\">Improve the second paragraph</p>"),
        "{two}"
    );
    // The people are what the message records, in the history page's own sentences.
    assert!(two.contains("<li>Proposed by Aiko Tanaka</li>"), "{two}");
    assert!(two.contains("<li>Accepted by Mika Ito</li>"), "{two}");
    // The proposal, by its summary, linked.
    assert!(
        two.contains(&format!(
            "Made from the proposal <a href=\"/handbook/-/proposals/{id}\">Improve the second paragraph</a>."
        )),
        "{two}"
    );
    // The review: the decision and the comment, with who and which revision.
    assert!(two.contains("<h4>Review</h4>"), "{two}");
    assert!(two.contains("Rin Sato approved, on revision 1."), "{two}");
    assert!(two.contains("They wrote: “Ready to take in”"), "{two}");
    assert!(
        two.contains("Rin Sato commented, on revision 1: “Looks fine to me”"),
        "{two}"
    );
    // The sources given with the proposal, as the proposer wrote them.
    assert!(
        two.contains("<h4>Sources given with the proposal</h4>"),
        "{two}"
    );
    assert!(two.contains("<li>https://example.org/report</li>"), "{two}");
    assert!(two.contains("<li>A book, page 12</li>"), "{two}");
    // And the way to the change itself.
    assert!(
        two.contains(&format!(
            "<a href=\"/handbook/-/change/{}\">See change 2 in the history</a>",
            block_id(2)
        )),
        "{two}"
    );
    // The creation of the page is a change made by the service: its people, no proposal.
    let one = change_block(&body, 1);
    assert!(!one.contains("Made from the proposal"), "{one}");
    assert!(!one.contains("<h4>"), "{one}");
}

#[tokio::test]
async fn a_proposal_with_no_review_and_no_sources_says_so() {
    let c = cast();
    let id = propose(&c, "Improve the second paragraph", "", "submit").await;
    accept_by_hand(&c.w, &id, &c.maintainer, &c.contributor);
    history(&c, &id);
    let body = main_of(&why(&c, None).await);
    let two = change_block(&body, 2);
    assert!(
        two.contains("Nobody commented or decided on this proposal."),
        "{two}"
    );
    assert!(
        two.contains("No source was given with the proposal."),
        "{two}"
    );
}

#[tokio::test]
async fn a_long_review_shows_a_bounded_part_and_counts_the_rest() {
    let c = cast();
    let id = propose(&c, "Improve the second paragraph", "", "submit").await;
    let uri = format!("/handbook/-/proposals/{id}");
    for n in 0..25 {
        post_form(
            &c.w,
            &format!("{uri}/comment"),
            Some(&c.reviewer),
            &[(
                "body",
                &format!("Comment number {n} {}", "word ".repeat(100)),
            )],
        )
        .await;
    }
    accept_by_hand(&c.w, &id, &c.maintainer, &c.contributor);
    history(&c, &id);
    let body = main_of(&why(&c, None).await);
    let two = change_block(&body, 2);
    assert_eq!(two.matches("commented, on revision").count(), 20, "{two}");
    assert!(
        two.contains("5 more items of the review are on the proposal&#39;s page."),
        "{two}"
    );
    // A comment is cut where it is long, and ends in an ellipsis.
    assert!(two.contains("…”"), "{two}");
}

#[tokio::test]
async fn a_proposal_the_records_do_not_tie_to_the_change_shows_no_review() {
    let c = cast();
    // The proposal exists and was commented, but nobody sealed it as version 2.
    let id = propose(&c, "Improve the second paragraph", "", "submit").await;
    post_form(
        &c.w,
        &format!("/handbook/-/proposals/{id}/comment"),
        Some(&c.reviewer),
        &[("body", "A secret comment")],
    )
    .await;
    history(&c, &id);
    let body = main_of(&why(&c, None).await);
    let two = change_block(&body, 2);
    assert!(
        two.contains("This wiki&#39;s records do not show that this proposal was accepted as this change, so its review is not shown here."),
        "{two}"
    );
    assert!(!two.contains("A secret comment"), "{two}");
    assert!(!two.contains("<h4>Review</h4>"), "{two}");
    assert!(!two.contains("Sources given"), "{two}");
}

#[tokio::test]
async fn a_proposal_the_message_names_that_the_wiki_does_not_hold_is_one_plain_sentence() {
    let c = cast();
    let stranger = ProposalId::parse("019a0000-0000-7000-8000-00000000bbbb").expect("id");
    history(&c, &stranger);
    let body = main_of(&why(&c, None).await);
    let two = change_block(&body, 2);
    assert!(
        two.contains("The message of this change names a proposal that is not among the records of this wiki that you may read."),
        "{two}"
    );
    assert!(!two.contains("Made from the proposal"), "{two}");
    // The people are still what the message records.
    assert!(two.contains("Proposed by Aiko Tanaka"), "{two}");
}

#[tokio::test]
async fn a_draft_is_its_authors_alone_and_to_anyone_else_is_a_proposal_that_cannot_be_read() {
    let c = cast();
    let id = propose(&c, "Improve the second paragraph", "", "draft").await;
    history(&c, &id);
    for reader in [None, Some(&c.reviewer)] {
        let body = main_of(&why(&c, reader).await);
        let two = change_block(&body, 2);
        assert!(
            two.contains(
                "names a proposal that is not among the records of this wiki that you may read."
            ),
            "{two}"
        );
        assert!(!two.contains("Improve the second paragraph</a>"), "{two}");
    }
}

#[tokio::test]
async fn a_change_made_by_the_service_or_sealed_by_someone_else_names_no_proposal() {
    let c = cast();
    c.w.fake
        .say(1, "A plain message, not in the service's form");
    c.w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    c.w.fake.set_files(&[("pages/Home.md", BETTER)]);
    c.w.fake.say(2, "fixup\u{7}: a plain message");
    c.w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
    let body = main_of(&get(&c.w, "/handbook/-/why/Home").await);
    let two = change_block(&body, 2);
    assert!(
        two.contains("The record of this change does not say who made it."),
        "{two}"
    );
    assert!(!two.contains("Made from the proposal"), "{two}");
}

#[tokio::test]
async fn a_request_for_changes_is_said_as_that_and_with_its_revision() {
    let c = cast();
    let id = propose(&c, "Improve the second paragraph", "", "submit").await;
    post_form(
        &c.w,
        &format!("/handbook/-/proposals/{id}/decide"),
        Some(&c.reviewer),
        &[("op", "request-changes"), ("revision", "1"), ("body", "")],
    )
    .await;
    accept_by_hand(&c.w, &id, &c.maintainer, &c.contributor);
    history(&c, &id);
    let body = main_of(&why(&c, None).await);
    let two = change_block(&body, 2);
    assert!(
        two.contains("Rin Sato asked for changes, on revision 1."),
        "{two}"
    );
    assert!(!two.contains("They wrote"), "{two}");
}

#[tokio::test]
async fn a_proposal_sealed_as_another_change_is_not_tied_to_this_one() {
    let c = cast();
    let id = propose(&c, "Improve the second paragraph", "", "submit").await;
    post_form(
        &c.w,
        &format!("/handbook/-/proposals/{id}/comment"),
        Some(&c.reviewer),
        &[("body", "A comment of another change")],
    )
    .await;
    // The records seal it as version 2; the history names it in version 3.
    accept_by_hand(&c.w, &id, &c.maintainer, &c.contributor);
    history(&c, &id);
    c.w.fake.set_files(&[(
        "pages/Home.md",
        &BETTER.replace("Third paragraph.", "Third paragraph, edited."),
    )]);
    c.w.fake.say(3, &message(&id, &c));
    c.w.fake
        .show(3, shown(vec![op("edit-text", &["pages/Home.md"])]));
    let body = main_of(&why(&c, None).await);
    let three = change_block(&body, 3);
    assert!(
        three.contains("do not show that this proposal was accepted as this change"),
        "{three}"
    );
    assert!(!three.contains("A comment of another change"), "{three}");
    // Version 2, the one it was sealed as, shows the review.
    let two = change_block(&body, 2);
    assert!(two.contains("A comment of another change"), "{two}");
}

#[tokio::test]
async fn a_proposal_revised_since_is_tied_to_the_change_by_any_of_its_revisions() {
    let c = cast();
    let id = propose(&c, "Improve the second paragraph", "", "submit").await;
    // A second revision, then the records of the first one's acceptance.
    let fields = fields(BETTER, "Improve the second paragraph", "", "submit");
    let borrowed: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let a = post_form(
        &c.w,
        &format!("/handbook/-/proposals/{id}/edit"),
        Some(&c.contributor),
        &borrowed,
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    accept_by_hand(&c.w, &id, &c.maintainer, &c.contributor);
    history(&c, &id);
    let body = main_of(&why(&c, None).await);
    let two = change_block(&body, 2);
    assert!(two.contains("<h4>Review</h4>"), "{two}");
}
