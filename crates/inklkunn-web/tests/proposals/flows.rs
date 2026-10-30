//! Opening a proposal: who is let in, what the form carries, what *Preview*, *Show changes*, *Save as draft* and
//! *Submit* do, and what is stored.

use axum::http::StatusCode;
use inklkunn_store::{ProposalFilter, ProposalState};

use super::{HOME, cast, home_fields, location, propose, read, send};
use crate::common::{get, post_form};

#[tokio::test]
async fn an_anonymous_visitor_is_told_in_words_that_proposing_needs_an_account() {
    let c = cast();
    for uri in [
        "/handbook/-/proposals/new?page=Home",
        "/handbook/-/proposals/new?new=1",
    ] {
        let a = get(&c.w, uri).await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{uri}");
        let body = a.body();
        assert!(body.contains("Proposing needs an account"), "{body}");
        assert!(body.contains("You are not signed in"), "{body}");
        assert!(
            !body.contains("<textarea") && !body.contains("name=\"summary\""),
            "{body}"
        );
    }
    // A post is told the same, and nothing is made.
    let a = post_form(&c.w, "/handbook/-/proposals/new", None, &[("op", "submit")]).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(a.body().contains("Proposing needs an account"));
    let all =
        c.w.services
            .proposals
            .list(
                &inklkunn_core::Principal::Anonymous,
                &c.w.wiki,
                &ProposalFilter::default(),
            )
            .unwrap();
    assert!(all.is_empty());
}

#[tokio::test]
async fn an_observer_is_not_allowed_to_propose_and_is_told_so_before_any_form() {
    let c = cast();
    let a = read(&c, "/handbook/-/proposals/new?page=Home", Some(&c.observer)).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    let body = a.body();
    assert!(body.contains("<h1>Not allowed</h1>"), "{body}");
    assert!(body.contains("Your role in this wiki does not include this."));
    assert!(!body.contains("<textarea"));
}

#[tokio::test]
async fn the_form_for_a_page_carries_its_text_its_base_and_no_script() {
    let c = cast();
    let a = read(
        &c,
        "/handbook/-/proposals/new?page=Home",
        Some(&c.contributor),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(body.contains("<h1>Propose a change to Home</h1>"), "{body}");
    assert!(body.contains("You are changing the whole page."));
    // The text, as the text area holds it (the first line break after the tag is the parser's own).
    assert!(
        body.contains("<textarea id=\"p0_text\" name=\"p0_text\" rows=\""),
        "{body}"
    );
    assert!(
        body.contains("cols=\"80\">\n---\ntitle: Home\n---\n\nFirst paragraph."),
        "{body}"
    );
    for name in [
        "base_state",
        "base_block",
        "p0_path",
        "pages",
        "summary",
        "sources",
    ] {
        assert!(body.contains(&format!("name=\"{name}\"")), "{name}: {body}");
    }
    assert!(body.contains(&format!("value=\"{}\"", crate::common::block_id(1))));
    assert!(body.contains("maxlength=\"120\""));
    for button in ["Preview", "Show changes", "Save as draft", "Submit"] {
        assert!(body.contains(&format!(">{button}</button>")), "{button}");
    }
    assert!(!body.contains("<script"));
    // A section and a part are asked for by position; one that is not there leads back to the choice.
    let a = read(
        &c,
        "/handbook/-/proposals/new?page=Guide&part=section&ordinal=1",
        Some(&c.contributor),
    )
    .await;
    let body = a.body();
    assert!(
        body.contains("You are changing one section: Details"),
        "{body}"
    );
    assert!(body.contains("## Details\n\nMore.</textarea>"), "{body}");
    assert!(!body.contains("Install it."));
    let a = read(
        &c,
        "/handbook/-/proposals/new?page=Home&part=block&ordinal=9",
        Some(&c.contributor),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER);
    assert!(a.header("location").unwrap().contains("parts=1"));
    // A page that is not there is the page that invites it.
    let a = read(
        &c,
        "/handbook/-/proposals/new?page=Nowhere",
        Some(&c.contributor),
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_chooser_lists_the_sections_and_parts_of_a_page() {
    let c = cast();
    let a = read(
        &c,
        "/handbook/-/proposals/new?page=Guide&parts=1",
        Some(&c.contributor),
    )
    .await;
    let body = a.body();
    assert!(body.contains("Choose what to change in Guide"), "{body}");
    assert!(
        body.contains("Section: Setup")
            && body.contains("Section: Details")
            && body.contains("Section: Use")
    );
    assert!(body.contains("Part 1: # Setup"));
    assert!(body.contains("part=section&#38;ordinal=1"));
}

#[tokio::test]
async fn submitting_makes_a_proposal_held_for_review_with_the_text_that_was_sent() {
    let c = cast();
    let text = HOME.replace("Second paragraph.", "Second paragraph, better.");
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&text, "Improve the second paragraph", "submit"),
    )
    .await;
    let to = location(&a);
    assert!(to.starts_with("/handbook/-/proposals/"), "{to}");
    let all =
        c.w.services
            .proposals
            .list(
                &inklkunn_core::Principal::Anonymous,
                &c.w.wiki,
                &ProposalFilter::default(),
            )
            .unwrap();
    let p = all.first().unwrap();
    assert_eq!(p.state, ProposalState::HeldForReview);
    assert_eq!(p.summary, "Improve the second paragraph");
    // The page names it all.
    let page = read(&c, &to, None).await;
    assert_eq!(page.status, StatusCode::OK);
    let body = page.body();
    assert!(
        body.contains("<h1>Improve the second paragraph</h1>"),
        "{body}"
    );
    assert!(body.contains("<strong>Held for review</strong>"));
    assert!(body.contains("Proposed by Aiko Tanaka"));
    assert!(body.contains("It changes 1 page."));
    assert!(body.contains("Page: Home"));
    assert!(body.contains("Part 2 is changed"));
    assert!(
        body.contains("1 part is the same on both sides.")
            || body.contains("2 parts are the same on both sides.")
    );
    assert!(body.contains("<del>"), "{body}");
    assert!(body.contains("This is revision 1 of 1."));
}

#[tokio::test]
async fn the_list_shows_what_was_sent_and_a_draft_only_to_its_author() {
    let c = cast();
    let sent = propose(&c, &c.contributor, "Sent for review").await;
    let text = HOME.replace("Third paragraph.", "Third, drafted.");
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&text, "", "draft"),
    )
    .await;
    let draft = location(&a);
    assert!(draft.ends_with("/edit?saved=1"), "{draft}");
    let list = read(&c, "/handbook/-/proposals", Some(&c.contributor))
        .await
        .body();
    assert!(
        list.contains("Your drafts") && list.contains("Open proposals"),
        "{list}"
    );
    assert!(list.contains(&sent) && list.contains("(no summary yet)"));
    // To anyone else the draft is not in the list, and its address is the page that is not there.
    let other = read(&c, "/handbook/-/proposals", Some(&c.other))
        .await
        .body();
    assert!(
        other.contains(&sent)
            && !other.contains("(no summary yet)")
            && !other.contains("Your drafts")
    );
    let id = draft.trim_end_matches("/edit?saved=1");
    let seen = read(&c, id, Some(&c.other)).await;
    let missing = read(
        &c,
        "/handbook/-/proposals/019a0000-0000-7000-8000-00000000dddd",
        Some(&c.other),
    )
    .await;
    assert_eq!(seen.status, StatusCode::NOT_FOUND);
    assert_eq!(seen.status, missing.status);
    assert_eq!(
        seen.body().replace(id, "X"),
        missing.body().replace(
            "/handbook/-/proposals/019a0000-0000-7000-8000-00000000dddd",
            "X"
        )
    );
    let edit = read(&c, &format!("{id}/edit"), Some(&c.other)).await;
    assert_eq!(edit.status, StatusCode::NOT_FOUND);
    // The author reads it, and a draft is not held for review.
    let own = read(&c, id, Some(&c.contributor)).await.body();
    assert!(
        own.contains("<strong>Draft</strong>") && own.contains("Only the person who started it")
    );
}

#[tokio::test]
async fn a_draft_survives_a_session_and_is_resumed_with_its_text_and_summary() {
    let c = cast();
    let text = HOME.replace("Third paragraph.", "Third, drafted.");
    let mut fields = home_fields(&text, "Work in progress", "draft");
    fields.retain(|(k, _)| *k != "sources");
    fields.push(("sources", "https://example.org/a".to_owned()));
    let a = send(&c, "/handbook/-/proposals/new", &c.contributor, &fields).await;
    let edit = location(&a);
    // A new session: services built again over the same stores and the same repository.
    let again = crate::common::fresh(&c.w);
    let a = crate::common::get_as(&again, &edit, Some(&c.contributor)).await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(
        body.contains("Your draft is saved. Only you can see it."),
        "{body}"
    );
    assert!(body.contains("Third, drafted."), "{body}");
    assert!(body.contains("value=\"Work in progress\""));
    assert!(body.contains("https://example.org/a"));
    assert!(body.contains(">Save as draft</button>"));
    // Another person's session does not see it.
    let a = crate::common::get_as(&again, &edit, Some(&c.other)).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
}
