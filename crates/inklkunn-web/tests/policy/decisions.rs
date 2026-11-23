//! Authority is visible (RFC 006 acceptance criterion 12): every page that names a decision shows the rung and the policy block,
//! with the block's own link; and a proposal that changes the rules says the change as before and after sentences, what
//! accepting it takes, and the hatch to an admin.

use inklkunn_store::{DecisionId, DecisionKind, DecisionRecord, ProposalStore};

use super::page::RICH;
use super::*;
use common::{NOW, accept_by_hand_under, block_id, state_id};

async fn page_proposal(e: &Env) -> ProposalId {
    let a = post_form(
        &e.w,
        "/handbook/-/proposals/new",
        Some(&e.contributor),
        &[
            ("op", "submit"),
            ("pages", "1"),
            ("renames", "0"),
            ("part", "whole"),
            ("base_state", &state_id(1)),
            ("base_block", &block_id(1)),
            ("p0_path", "pages/Home.md"),
            ("p0_title", "Home"),
            ("p0_text", &home_with("The bridge opened in 1951.")),
            ("summary", "Correct the year"),
            ("sources", ""),
        ],
    )
    .await;
    proposal_of(&a)
}

fn link_of(block: &str) -> String {
    format!(
        "<a href=\"/handbook/-/change/{block}\">{}</a>",
        &block[..12]
    )
}

#[tokio::test]
async fn an_accepted_proposal_says_who_accepted_it_as_which_role_under_which_rules_with_the_blocks_link()
 {
    let e = env_under(RICH);
    let id = page_proposal(&e).await;
    accept_by_hand_under(
        &e.w,
        &id,
        &e.maintainer,
        &e.contributor,
        Some(common::POLICY_BLOCK),
        false,
    );
    let main = main_of(&as_(&e, &e.observer, &format!("/handbook/-/proposals/{id}")).await);
    assert!(
        main.contains(&format!(
            "Accepted by Mika Ito (maintainer), under the rules as of {}.",
            link_of(common::POLICY_BLOCK)
        )),
        "{main}"
    );
    // The same in Japanese, with the same link.
    let ja = main_of(
        &as_ja(
            &e,
            Some(&e.observer),
            &format!("/handbook/-/proposals/{id}"),
        )
        .await,
    );
    assert!(
        ja.contains(&format!(
            "Mika Ito（メンテナー）が受け入れました。適用されたルール: 変更 {} の時点。",
            link_of(common::POLICY_BLOCK)
        )),
        "{ja}"
    );
}

#[tokio::test]
async fn an_acceptance_under_no_rules_of_the_wikis_own_says_the_default_rules_and_the_hatch_says_it_was_used()
 {
    let e = env();
    let id = page_proposal(&e).await;
    accept_by_hand_under(&e.w, &id, &e.maintainer, &e.contributor, None, false);
    let main = main_of(&as_(&e, &e.observer, &format!("/handbook/-/proposals/{id}")).await);
    assert!(
        main.contains("Accepted by Mika Ito (maintainer), under the default rules."),
        "{main}"
    );
    let other = env();
    let id = page_proposal(&other).await;
    accept_by_hand_under(
        &other.w,
        &id,
        &other.admin,
        &other.contributor,
        Some(common::POLICY_BLOCK),
        true,
    );
    let main = main_of(
        &as_(
            &other,
            &other.observer,
            &format!("/handbook/-/proposals/{id}"),
        )
        .await,
    );
    assert!(
        main.contains("Accepted by Ada Admin (admin) as an admin of the wiki, with no approvals asked, under the rules as of"),
        "{main}"
    );
}

#[tokio::test]
async fn a_reviewers_decision_shows_the_role_they_held_and_the_rules_and_an_old_one_says_it_was_not_recorded()
 {
    let e = env_under(RICH);
    let id = page_proposal(&e).await;
    let a = post_form(
        &e.w,
        &format!("/handbook/-/proposals/{id}/decide"),
        Some(&e.reviewer),
        &[
            ("op", "approve"),
            ("revision", "1"),
            ("body", "Looks right."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let main = main_of(&as_(&e, &e.observer, &format!("/handbook/-/proposals/{id}")).await);
    assert!(
        main.contains(&format!(
            "Decided as reviewer, under the rules as of {}.",
            link_of(common::POLICY_BLOCK)
        )),
        "{main}"
    );
    // A decision a record of before 0.4.0 holds has no role and no block: nothing is guessed, and the page says so.
    e.w.store
        .add_decision(&DecisionRecord {
            id: DecisionId::new(),
            proposal: id.clone(),
            revision: 1,
            reviewer: e.maintainer.clone(),
            kind: DecisionKind::Approve,
            body: String::new(),
            reviewer_rung: None,
            policy_block: None,
            created_at: NOW,
        })
        .unwrap();
    let main = main_of(&as_(&e, &e.observer, &format!("/handbook/-/proposals/{id}")).await);
    assert!(
        main.contains("The role and the rules this was decided under were not recorded."),
        "{main}"
    );
}

#[tokio::test]
async fn a_proposal_that_changes_the_rules_says_the_change_as_before_and_after_sentences_and_what_accepting_it_takes()
 {
    let e = env_under("acceptance:\n  approvals: 5\n");
    let fields = with(
        &opened(&e, &e.maintainer, "send").await,
        "who_accept",
        "reviewer",
    );
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    let a = as_(&e, &e.contributor, &format!("/handbook/-/proposals/{id}")).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    for want in [
        "What this proposal changes in the rules",
        "Who may accept a proposal: maintainer or above → reviewer or above",
        "Accepting this change takes: maintainer or above, with 5 approvals from people other than the proposer",
        // The approvals warning stays beside it.
        "not enough people can give them yet",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    // Only an admin is told the hatch is theirs.
    assert!(!main.contains("You are an admin of this wiki"), "{main}");
    let admin = main_of(&as_(&e, &e.admin, &format!("/handbook/-/proposals/{id}")).await);
    assert!(
        admin.contains(
            "You are an admin of this wiki, so you may accept this change without any approval."
        ),
        "{admin}"
    );
    assert_plain_words("a proposal that changes the rules", &a.body());
    let ja = as_ja(
        &e,
        Some(&e.contributor),
        &format!("/handbook/-/proposals/{id}"),
    )
    .await;
    assert!(
        main_of(&ja).contains("この提案がルールで変えること"),
        "{}",
        ja.body()
    );
    assert!(
        main_of(&ja).contains("提案を受け入れられる人: メンテナー 以上 → レビュアー 以上"),
        "{}",
        ja.body()
    );
    assert_plain_words("the Japanese proposal", &ja.body());
}

#[tokio::test]
async fn the_removal_of_a_file_of_the_rules_is_said_as_back_to_the_defaults() {
    let e = env_under(RICH);
    e.w.fake.set_files(&[("governance/policy.yaml", RICH)]);
    let fields = opened(&e, &e.maintainer, "defaults").await;
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    let main = main_of(&as_(&e, &e.contributor, &format!("/handbook/-/proposals/{id}")).await);
    for want in [
        "This removes a file of the rules: the default rules apply again for what it set.",
        "Who may accept a proposal: reviewer or above → maintainer or above",
        "Protection removed: The pages under Help/ (not the page Help)",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
}

#[tokio::test]
async fn a_row_of_the_moderation_log_shows_the_role_and_the_rules_the_act_was_done_under() {
    for (e, expected) in [
        (env(), "Done under the default rules.".to_owned()),
        (
            env_under(RICH),
            format!(
                "Done under the rules as of {}.",
                link_of(common::POLICY_BLOCK)
            ),
        ),
    ] {
        let id = page_proposal(&e).await;
        let a = post_form(
            &e.w,
            &format!("/handbook/-/proposals/{id}/comment"),
            Some(&e.contributor),
            &[("body", "A comment to hide.")],
        )
        .await;
        assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
        let comment =
            e.w.store
                .comments(&id, None)
                .unwrap()
                .first()
                .expect("a comment")
                .id
                .to_string();
        let a = post_form(
            &e.w,
            "/handbook/-/moderate",
            Some(&e.maintainer),
            &[
                ("kind", "proposal-comment"),
                ("proposal", id.as_str()),
                ("comment", &comment),
                ("part_content", "1"),
                ("category", "spam"),
                ("note", ""),
            ],
        )
        .await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.body());
        let main = main_of(&common::get(&e.w, "/handbook/-/moderation").await);
        assert!(main.contains(&expected), "{expected}: {main}");
        assert!(
            main.contains("maintainer"),
            "the role is in the row: {main}"
        );
    }
}

#[tokio::test]
async fn a_restriction_names_the_role_that_set_it_and_the_rules_to_the_moderator_and_to_the_restricted_person()
 {
    let e = env();
    let a = post_form(
        &e.w,
        "/handbook/-/people/restrict",
        Some(&e.maintainer),
        &[
            ("who", e.contributor.as_str()),
            ("days", "3"),
            ("reason", "Too many edits."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let moderator = main_of(&as_(&e, &e.maintainer, "/handbook/-/people").await);
    assert!(
        moderator.contains("Set by a person holding maintainer, under the default rules."),
        "{moderator}"
    );
    // The restricted person is told the same, in the paragraphs of their own page.
    let own = main_of(&as_(&e, &e.contributor, "/handbook/-/people").await);
    assert!(
        own.contains("Set by a person holding maintainer, under the default rules."),
        "{own}"
    );
}
