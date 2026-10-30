//! Review: comments anchored to parts and where they land on a later revision, decisions bound to the
//! revision reviewed, decline / withdraw / ask for a revision with a required reason, Accept, the quotas, and a
//! page's discussion.

use axum::http::StatusCode;

use super::{HOME, cast, home_fields, location, propose, read, send};
use crate::common::post_form;

fn revise_text(top: &str, second: &str) -> String {
    let mut text = String::from("---\ntitle: Home\n---\n\n");
    if !top.is_empty() {
        text.push_str(top);
        text.push_str("\n\n");
    }
    text.push_str("First paragraph.\n\n");
    if !second.is_empty() {
        text.push_str(second);
        text.push_str("\n\n");
    }
    text.push_str("Third paragraph.\n");
    text
}

async fn revise(c: &super::Cast, proposal: &str, text: &str) {
    let a = send(
        c,
        &format!("{proposal}/edit"),
        &c.contributor,
        &home_fields(text, "Improve the second paragraph", "submit"),
    )
    .await;
    assert_eq!(location(&a), proposal);
}

#[tokio::test]
async fn a_comment_on_a_part_lands_on_the_part_follows_it_across_a_revision_and_is_orphaned_when_it_is_gone()
 {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve the second paragraph").await;
    // The link on the part opens the comment form for it.
    let page = read(&c, &p, Some(&c.reviewer)).await.body();
    assert!(page.contains("Comment on part 2"), "{page}");
    let form = read(&c, &format!("{p}?on=0-1#comment"), Some(&c.reviewer))
        .await
        .body();
    assert!(
        form.contains("Your comment is about part 2 of Home."),
        "{form}"
    );
    for name in ["anchor_page", "anchor_ordinal", "anchor_revision"] {
        assert!(form.contains(&format!("name=\"{name}\"")), "{name}");
    }
    let a = post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.reviewer),
        &[
            ("body", "Is this the better word?"),
            ("anchor_page", "pages/Home.md"),
            ("anchor_ordinal", "1"),
            ("anchor_revision", "1"),
        ],
    )
    .await;
    assert!(location(&a).starts_with(&format!("{p}#k-")));
    // Beside the part, with its author and the revision it was made on.
    let page = read(&c, &p, None).await.body();
    let part = page.find("Part 2 is changed").unwrap();
    let comment = page.find("Is this the better word?").unwrap();
    assert!(part < comment, "{page}");
    assert!(page.contains("Rin Sato</strong> · 2023-11-14") && page.contains("on revision 1"));
    assert!(!page.contains("Orphaned"));

    // Revision 2 puts a part in front: the same text is the third part now, and the comment is still beside it.
    revise(
        &c,
        &p,
        &revise_text("A new first part.", "Second paragraph, better."),
    )
    .await;
    let page = read(&c, &p, None).await.body();
    assert!(page.contains("This is revision 2 of 2."));
    let part = page
        .find("Part 3 is changed")
        .expect("the part moved to position 3");
    let comment = page.find("Is this the better word?").unwrap();
    assert!(part < comment, "{page}");
    assert!(
        page.contains("on revision 1"),
        "the comment keeps the revision it was made on"
    );
    assert!(!page.contains("Orphaned"));

    // Revision 3 no longer has the part: nothing matches, and the comment is kept and said to be orphaned.
    revise(&c, &p, &revise_text("A new first part.", "")).await;
    let page = read(&c, &p, None).await.body();
    assert!(
        page.contains(
            "Orphaned: the part of Home this comment was about is no longer in the proposal."
        ),
        "{page}"
    );
    assert!(
        page.contains("<p>Second paragraph, better.</p>"),
        "the text it was about is shown: {page}"
    );
    assert!(page.contains("Is this the better word?"));
}

#[tokio::test]
async fn a_comment_is_refused_for_a_stale_revision_an_empty_body_and_a_stranger() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    revise(&c, &p, &revise_text("", "Second paragraph, again.")).await;
    // The form was drawn from revision 1; the proposal is on revision 2.
    let a = post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.reviewer),
        &[
            ("body", "Late"),
            ("anchor_page", "pages/Home.md"),
            ("anchor_ordinal", "1"),
            ("anchor_revision", "1"),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::CONFLICT);
    assert!(
        a.body()
            .contains("The proposal changed while you were working on it.")
    );
    let a = post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.reviewer),
        &[("body", "  ")],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(a.body().contains("A comment needs some words."));
    // An observer may read and may not comment; nobody signed out may.
    let a = post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.observer),
        &[("body", "Hello")],
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(a.body().contains("<h1>Not allowed</h1>"));
    let a = post_form(&c.w, &format!("{p}/comment"), None, &[("body", "Hello")]).await;
    assert!(a.body().contains("Proposing needs an account"));
    // The form for a comment is on the page of whoever may use it, and only theirs.
    assert!(
        read(&c, &p, Some(&c.reviewer))
            .await
            .body()
            .contains("Add a comment")
    );
    assert!(
        !read(&c, &p, Some(&c.observer))
            .await
            .body()
            .contains("Add a comment")
    );
    assert!(!read(&c, &p, None).await.body().contains("Add a comment"));
}

#[tokio::test]
async fn replies_are_nested_under_the_comment_they_answer() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let a = post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.reviewer),
        &[("body", "A question")],
    )
    .await;
    let first = location(&a).split_once("#k-").unwrap().1.to_owned();
    let form = read(
        &c,
        &format!("{p}?reply={first}#comment"),
        Some(&c.contributor),
    )
    .await
    .body();
    assert!(
        form.contains("Your comment is a reply to Rin Sato."),
        "{form}"
    );
    post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.contributor),
        &[("body", "An answer"), ("parent", first.as_str())],
    )
    .await;
    let page = read(&c, &p, Some(&c.contributor)).await.body();
    let q = page.find("A question").unwrap();
    let r = page.find("An answer").unwrap();
    assert!(
        q < r && page.contains("class=\"comment depth-1\""),
        "{page}"
    );
    assert!(page.contains("Reply to Rin Sato"));
}

#[tokio::test]
async fn a_decision_is_bound_to_its_revision_and_still_says_so_after_the_next() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let a = post_form(
        &c.w,
        &format!("{p}/decide"),
        Some(&c.reviewer),
        &[
            ("op", "approve"),
            ("revision", "1"),
            ("body", "Looks right"),
        ],
    )
    .await;
    assert_eq!(location(&a), format!("{p}#decisions"));
    let page = read(&c, &p, None).await.body();
    assert!(page.contains("Rin Sato approved, on revision 1."), "{page}");
    assert!(page.contains("Looks right"));
    assert!(!page.contains("no longer counts"));
    // Deciding twice on one revision is refused in words.
    let a = post_form(
        &c.w,
        &format!("{p}/decide"),
        Some(&c.reviewer),
        &[("op", "request-changes"), ("revision", "1")],
    )
    .await;
    assert_eq!(a.status, StatusCode::CONFLICT);
    assert!(
        a.body()
            .contains("You have already decided on this revision.")
    );

    revise(&c, &p, &revise_text("", "Second paragraph, again.")).await;
    let page = read(&c, &p, None).await.body();
    assert!(
        page.contains("Rin Sato approved, on revision 1."),
        "the decision stays recorded: {page}"
    );
    assert!(page.contains("A newer revision was written after this. The decision no longer counts toward accepting it."));
    // A decision on the old revision is refused; on the latest it is taken.
    let a = post_form(
        &c.w,
        &format!("{p}/decide"),
        Some(&c.reviewer),
        &[("op", "approve"), ("revision", "1")],
    )
    .await;
    assert_eq!(a.status, StatusCode::CONFLICT);
    assert!(
        a.body()
            .contains("This proposal has a newer revision, number 2.")
    );
    let a = post_form(
        &c.w,
        &format!("{p}/decide"),
        Some(&c.reviewer),
        &[
            ("op", "request-changes"),
            ("revision", "2"),
            ("body", "Not yet"),
        ],
    )
    .await;
    assert!(a.status.is_redirection());
    let page = read(&c, &p, None).await.body();
    assert!(
        page.contains("Rin Sato asked for changes, on revision 2."),
        "{page}"
    );
    // Only a reviewer sees the buttons.
    assert!(
        read(&c, &p, Some(&c.reviewer))
            .await
            .body()
            .contains("Approve this revision")
    );
    assert!(
        !read(&c, &p, Some(&c.contributor))
            .await
            .body()
            .contains("Approve this revision")
    );
    let a = post_form(
        &c.w,
        &format!("{p}/decide"),
        Some(&c.contributor),
        &[("op", "approve"), ("revision", "2")],
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn declining_needs_a_reason_and_the_reason_is_shown() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    // The form is the maintainer's; no one else is given it.
    assert_eq!(
        read(&c, &format!("{p}/decline"), Some(&c.contributor))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        read(&c, &format!("{p}/decline"), Some(&c.reviewer))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let form = read(&c, &format!("{p}/decline"), Some(&c.maintainer)).await;
    assert_eq!(form.status, StatusCode::OK);
    assert!(form.body().contains("Thank the proposer for the effort"));
    // Without a reason it is refused on the form, with what was typed kept.
    let a = post_form(
        &c.w,
        &format!("{p}/decline"),
        Some(&c.maintainer),
        &[("reason", "   ")],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(a.body().contains("A reason is needed."));
    let page = read(&c, &p, None).await.body();
    assert!(page.contains("<strong>Held for review</strong>"));
    let a = post_form(
        &c.w,
        &format!("{p}/decline"),
        Some(&c.maintainer),
        &[(
            "reason",
            "Thank you. The wording is already used elsewhere.",
        )],
    )
    .await;
    assert_eq!(location(&a), p);
    let page = read(&c, &p, None).await.body();
    assert!(page.contains("<strong>Declined</strong>"));
    assert!(page.contains("Mika Ito declined this proposal. Reason: Thank you. The wording is already used elsewhere."), "{page}");
    // A declined proposal cannot be declined again, or commented on.
    let a = post_form(
        &c.w,
        &format!("{p}/decline"),
        Some(&c.maintainer),
        &[("reason", "Again")],
    )
    .await;
    assert_eq!(a.status, StatusCode::CONFLICT);
    assert!(
        a.body()
            .contains("This proposal is Declined, so this cannot be done now.")
    );
}

#[tokio::test]
async fn withdrawing_is_the_proposers_and_needs_a_reason() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    assert_eq!(
        read(&c, &format!("{p}/withdraw"), Some(&c.other))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        read(&c, &format!("{p}/withdraw"), Some(&c.maintainer))
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    assert!(
        read(&c, &p, Some(&c.contributor))
            .await
            .body()
            .contains("Withdraw this proposal")
    );
    assert!(
        !read(&c, &p, Some(&c.other))
            .await
            .body()
            .contains("Withdraw this proposal")
    );
    let a = post_form(
        &c.w,
        &format!("{p}/withdraw"),
        Some(&c.contributor),
        &[("reason", "")],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let a = post_form(
        &c.w,
        &format!("{p}/withdraw"),
        Some(&c.contributor),
        &[("reason", "I found a better way.")],
    )
    .await;
    assert_eq!(location(&a), p);
    let page = read(&c, &p, None).await.body();
    assert!(
        page.contains("<strong>Withdrawn</strong>")
            && page.contains("Aiko Tanaka withdrew this proposal. Reason: I found a better way."),
        "{page}"
    );
}

#[tokio::test]
async fn asking_for_a_revision_sends_it_back_and_a_revision_brings_it_back_for_review() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let a = post_form(
        &c.w,
        &format!("{p}/ask"),
        Some(&c.maintainer),
        &[("reason", "Please keep the old heading.")],
    )
    .await;
    assert_eq!(location(&a), p);
    let page = read(&c, &p, Some(&c.contributor)).await.body();
    assert!(page.contains("<strong>Needs a revision</strong>"));
    assert!(page.contains("This proposal needs a revision from Aiko Tanaka"));
    assert!(page.contains("Mika Ito asked for a revision. Reason: Please keep the old heading."));
    assert!(page.contains("Revise this proposal"));
    revise(
        &c,
        &p,
        &revise_text("", "Second paragraph, with the old heading."),
    )
    .await;
    let page = read(&c, &p, Some(&c.contributor)).await.body();
    assert!(page.contains("<strong>Held for review</strong>"), "{page}");
}

#[tokio::test]
async fn accept_is_there_and_says_it_is_not_available_yet() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let page = read(&c, &p, Some(&c.maintainer)).await.body();
    assert!(page.contains(">Accept</button>"), "{page}");
    assert!(page.contains("Accepting is not available yet on this server."));
    // Not a person who may accept: no button.
    assert!(
        !read(&c, &p, Some(&c.contributor))
            .await
            .body()
            .contains(">Accept</button>")
    );
    let a = post_form(&c.w, &format!("{p}/accept"), Some(&c.maintainer), &[]).await;
    assert_eq!(a.status, StatusCode::NOT_IMPLEMENTED);
    let body = a.body();
    assert!(
        body.contains("<h1>Accepting is not available yet</h1>"),
        "{body}"
    );
    assert!(body.contains("The proposal stays held for review and nothing was changed."));
    assert!(
        read(&c, &p, None)
            .await
            .body()
            .contains("<strong>Held for review</strong>")
    );
    let a = post_form(&c.w, &format!("{p}/accept"), Some(&c.contributor), &[]).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(a.body().contains("<h1>Not allowed</h1>"));
    let a = post_form(&c.w, &format!("{p}/accept"), None, &[]).await;
    assert!(a.body().contains("Proposing needs an account"));
    let a = post_form(
        &c.w,
        "/handbook/-/proposals/019a0000-0000-7000-8000-00000000dddd/accept",
        Some(&c.maintainer),
        &[],
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_quota_refuses_by_name_with_the_count_and_when_it_ends() {
    let c = cast();
    let text = HOME.replace("Third paragraph.", "Third, drafted.");
    for _ in 0..20 {
        let a = send(
            &c,
            "/handbook/-/proposals/new",
            &c.contributor,
            &home_fields(&text, "", "draft"),
        )
        .await;
        assert!(a.status.is_redirection(), "{}", a.body());
    }
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&text, "", "draft"),
    )
    .await;
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS);
    let body = a.body();
    assert!(body.contains("<h1>Refused</h1>"), "{body}");
    assert!(body.contains("Refused: too many open proposals"));
    assert!(body.contains("The limit is 20. The count now is 20, and this asked for 1."));
    assert!(body.contains(
        "It resets when one of your open proposals is accepted, declined, withdrawn or superseded."
    ));
    // Another person is not held to it.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.other,
        &home_fields(&text, "", "draft"),
    )
    .await;
    assert!(a.status.is_redirection());
}

#[tokio::test]
async fn a_pages_discussion_lists_the_proposals_that_change_it_with_what_was_said() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve the second paragraph").await;
    post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.reviewer),
        &[("body", "Looks fine to me")],
    )
    .await;
    let a = read(&c, "/handbook/-/discussion/Home", None).await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(body.contains("<h1>Discussion of Home</h1>"), "{body}");
    assert!(body.contains("Improve the second paragraph") && body.contains("Looks fine to me"));
    assert!(body.contains("Held for review"));
    assert!(body.contains("href=\"/handbook/Home\""));
    // A page nothing touches has an honest empty discussion.
    let body = read(&c, "/handbook/-/discussion/Guide", None).await.body();
    assert!(
        body.contains("No proposal changes this page yet."),
        "{body}"
    );
    // The link is on the page itself.
    assert!(
        read(&c, "/handbook/Home", None)
            .await
            .body()
            .contains("/handbook/-/discussion/Home")
    );
}

#[tokio::test]
async fn an_accepted_proposal_says_it_is_signed_into_the_history_and_links_to_its_change() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Taken in").await;
    let id = inklkunn_store::ProposalId::parse(p.rsplit('/').next().unwrap()).unwrap();
    crate::common::accept_by_hand(&c.w, &id, &c.maintainer, &c.contributor);
    let page = read(&c, &p, None).await.body();
    assert!(page.contains("<strong>Accepted</strong>"), "{page}");
    assert!(page.contains("Signed into the history"));
    assert!(
        page.contains(&format!(
            "href=\"/handbook/-/change/{}\"",
            crate::common::block_id(2)
        )),
        "{page}"
    );
    // Nothing more can be done to it, and the forms are gone.
    assert!(
        !read(&c, &p, Some(&c.maintainer))
            .await
            .body()
            .contains(">Accept</button>")
    );
    let a = post_form(
        &c.w,
        &format!("{p}/comment"),
        Some(&c.reviewer),
        &[("body", "Late")],
    )
    .await;
    assert_eq!(a.status, StatusCode::CONFLICT);
}
