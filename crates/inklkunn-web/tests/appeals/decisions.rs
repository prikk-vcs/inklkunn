//! Deciding an appeal from the page of the people who decide: what an overturn does, what a refusal says, and the limits.

use super::*;

const MAINTAINERS_DECIDE: &str = "rungs:\n  decide_appeal: maintainer\n";

#[tokio::test]
async fn an_overturn_ends_a_restriction_the_person_is_told_and_may_write_again_and_a_decision_is_made_once()
 {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    assert_eq!(
        as_(&e, &e.contributor, "/handbook/-/proposals/new")
            .await
            .status,
        StatusCode::FORBIDDEN
    );

    // A reason is required, and one line.
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/decide",
        Some(&e.admin),
        &[
            ("appeal", appeal.id.as_str()),
            ("decision", "overturned"),
            ("reason", ""),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert_eq!(open_appeals(&e).len(), 1, "nothing was decided");
    // The decision has to be one of the two.
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/decide",
        Some(&e.admin),
        &[("appeal", appeal.id.as_str()), ("reason", REASON)],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());

    // The rung that decides overturns it.
    let a = decide(&e, &e.admin, &appeal, "overturned").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/appeals?done=overturned")
    );
    let done = main_of(&as_(&e, &e.admin, "/handbook/-/appeals?done=overturned").await);
    assert!(
        done.contains("The decision was recorded: the action was reversed."),
        "{done}"
    );
    assert!(done.contains("Decided on"), "{done}");
    assert!(done.contains(REASON), "{done}");
    // The person may write again, and their own page says how it ended.
    assert_ne!(
        as_(&e, &e.contributor, "/handbook/-/proposals/new")
            .await
            .status,
        StatusCode::FORBIDDEN,
        "the person may propose again"
    );
    let mine = main_of(&as_(&e, &e.contributor, "/handbook/-/appeals/mine").await);
    assert!(
        mine.contains("the action was reversed. Reason: The record supports the appeal."),
        "{mine}"
    );
    assert!(
        mine.contains("Nothing that affects you can be contested now."),
        "{mine}"
    );
    let record = e.w.store.appeal(&appeal.id).unwrap().unwrap();
    assert!(matches!(record.state, AppealState::Overturned(..)));

    // Once, and for good: a second decision is refused and changes nothing.
    for decision in ["upheld", "overturned"] {
        let a = decide(&e, &e.admin, &appeal, decision).await;
        assert_eq!(a.status, StatusCode::CONFLICT, "{}", a.body());
        assert!(a.body().contains("decided in the meantime"), "{}", a.body());
    }
    // And the restriction record names the appeal that reversed it.
    let events =
        e.w.store
            .restriction_events(&e.w.wiki.id, &e.contributor)
            .unwrap();
    assert!(
        events
            .iter()
            .any(|ev| ev.appeal.as_ref() == Some(&appeal.id))
    );
}

#[tokio::test]
async fn an_upheld_appeal_leaves_the_action_and_tells_the_person_the_reason() {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    let a = decide(&e, &e.admin, &appeal, "upheld").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/appeals?done=upheld")
    );
    assert_eq!(
        as_(&e, &e.contributor, "/handbook/-/proposals/new")
            .await
            .status,
        StatusCode::FORBIDDEN,
        "the restriction stands"
    );
    let mine = main_of(&as_(&e, &e.contributor, "/handbook/-/appeals/mine").await);
    assert!(
        mine.contains("the action stands. Reason: The record supports the appeal."),
        "{mine}"
    );
    // The restriction is still theirs to contest, and a new appeal is possible once this one is decided.
    assert!(mine.contains("Contest this"), "{mine}");
    let again = file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    assert_eq!(again.status, StatusCode::SEE_OTHER, "{}", again.body());
}

#[tokio::test]
async fn an_overturned_decline_reopens_the_proposal_and_its_page_says_it_is_held_for_review_again()
{
    let e = env();
    let id = propose(&e, &e.contributor, "A correction").await;
    decline(&e, &e.maintainer, &id).await;
    let position = e.w.store.last_transition_position(&id).unwrap().unwrap();
    file(&e, &e.contributor, "decline", &position.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    let a = decide(&e, &e.admin, &appeal, "overturned").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/appeals?done=reopened")
    );
    assert_eq!(
        e.w.store.proposal(&id, None).unwrap().unwrap().state,
        ProposalState::HeldForReview
    );
    let page = as_(&e, &e.contributor, &format!("/handbook/-/proposals/{id}"))
        .await
        .body();
    assert!(page.contains("<strong>Held for review</strong>"), "{page}");
    assert!(!page.contains("Contest this"), "{page}");
    // The reviewer who declined may now decline it again, as any reviewer may: it is an ordinary open proposal.
    decline(&e, &e.maintainer, &id).await;
}

#[tokio::test]
async fn the_one_who_acted_and_the_appellant_are_refused_by_name_and_the_appeal_stays_open() {
    let e = env_under(MAINTAINERS_DECIDE);
    let id = propose(&e, &e.contributor, "A correction").await;
    decline(&e, &e.maintainer, &id).await;
    let position = e.w.store.last_transition_position(&id).unwrap().unwrap();
    file(&e, &e.contributor, "decline", &position.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");

    // The one who declined holds the rung that decides, and does not decide this one: the page says so beside the appeal, and the
    // act is refused by name.
    let queue = main_of(&as_(&e, &e.maintainer, "/handbook/-/appeals").await);
    assert!(
        queue.contains("You took this action, so someone else decides the appeal against it."),
        "{queue}"
    );
    assert!(
        !queue.contains("name=\"decision\""),
        "no form for them: {queue}"
    );
    let a = decide(&e, &e.maintainer, &appeal, "overturned").await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert!(
        a.body()
            .contains("You took this action, so someone else decides the appeal against it."),
        "{}",
        a.body()
    );

    // The appellant, given the rung that decides, does not decide their own.
    e.w.store
        .grant(inklkunn_store::RoleGrant {
            account: e.contributor.clone(),
            wiki: e.w.wiki.id.clone(),
            role: Role::Maintainer,
            granted_by: None,
            granted_at: 0,
        })
        .unwrap();
    let a = decide(&e, &e.contributor, &appeal, "overturned").await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert!(
        a.body()
            .contains("You made this appeal, so someone else decides it."),
        "{}",
        a.body()
    );
    assert_eq!(open_appeals(&e).len(), 1, "nothing was decided");
    assert_eq!(
        e.w.store.proposal(&id, None).unwrap().unwrap().state,
        ProposalState::Declined
    );
    // Someone who is neither decides it.
    let a = decide(&e, &e.admin, &appeal, "upheld").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
}

#[tokio::test]
async fn an_overturn_that_later_changes_made_moot_is_refused_in_words_and_can_still_be_upheld() {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    // A moderator lifts the restriction meanwhile.
    let a = post_form(
        &e.w,
        "/handbook/-/people/lift",
        Some(&e.maintainer),
        &[("who", &e.contributor.to_string()), ("reason", "")],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let a = decide(&e, &e.admin, &appeal, "overturned").await;
    assert_eq!(a.status, StatusCode::CONFLICT, "{}", a.body());
    assert!(
        a.body()
            .contains("This restriction has ended already, so there is nothing to reverse."),
        "{}",
        a.body()
    );
    assert_eq!(open_appeals(&e).len(), 1);
    let a = decide(&e, &e.admin, &appeal, "upheld").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
}

#[tokio::test]
async fn a_person_files_so_many_appeals_a_day_and_the_next_is_refused_naming_the_limit() {
    let e = env();
    let mut positions = Vec::new();
    for n in 0..4 {
        let id = propose(&e, &e.contributor, &format!("A correction {n}")).await;
        decline(&e, &e.maintainer, &id).await;
        positions.push(e.w.store.last_transition_position(&id).unwrap().unwrap());
    }
    for position in positions.iter().take(3) {
        let a = file(&e, &e.contributor, "decline", &position.to_string()).await;
        assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    }
    let a = file(
        &e,
        &e.contributor,
        "decline",
        &positions.last().unwrap().to_string(),
    )
    .await;
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS, "{}", a.body());
    let body = a.body();
    assert!(body.contains("too many appeals in the last day"), "{body}");
    assert!(body.contains("The limit is 3."), "{body}");
    assert_eq!(
        open_appeals(&e).len(),
        3,
        "the refused one was not recorded"
    );
}

#[tokio::test]
async fn an_appeal_is_decided_by_a_form_from_this_site_with_the_sessions_token() {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    // A post that is not from this site is refused before anything is read.
    let a = request(
        &e.w,
        common::APP,
        "POST",
        "/handbook/-/appeals/decide",
        &[
            ("x-test-account", e.admin.as_str()),
            ("sec-fetch-site", "cross-site"),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert_eq!(
        e.w.store.appeal(&appeal.id).unwrap().unwrap().state,
        AppealState::Open
    );
}

/// An open appeal of `who` against a restriction that is not there, recorded straight in the store (the filing path's limits are
/// not what is being tested here).
fn open_appeal_of(e: &Env, who: &AccountId, at: i64) {
    e.w.store
        .file_appeal(&AppealRecord {
            id: inklkunn_store::AppealId::new(),
            wiki: e.w.wiki.id.clone(),
            at,
            action: inklkunn_store::AppealedAction {
                kind: inklkunn_store::AppealedKind::Restriction,
                id: AccountId::new().to_string(),
            },
            action_actor: None,
            appellant: who.clone(),
            text: "ZEBRA".to_owned(),
            state: AppealState::Open,
        })
        .unwrap();
}

#[tokio::test]
async fn a_person_holds_so_many_open_appeals_and_the_next_is_refused_naming_that_limit() {
    let e = env();
    for _ in 0..5 {
        open_appeal_of(&e, &e.contributor, common::NOW - 3 * 86_400);
    }
    let id = propose(&e, &e.contributor, "A correction").await;
    decline(&e, &e.maintainer, &id).await;
    let position = e.w.store.last_transition_position(&id).unwrap().unwrap();
    let a = file(&e, &e.contributor, "decline", &position.to_string()).await;
    assert_eq!(a.status, StatusCode::TOO_MANY_REQUESTS, "{}", a.body());
    let body = a.body();
    assert!(
        body.contains("too many of your appeals are waiting for a decision in this wiki"),
        "{body}"
    );
    assert!(body.contains("The limit is 5."), "{body}");
    assert!(
        body.contains("It resets when one of your open appeals is decided."),
        "{body}"
    );
    assert_eq!(
        open_appeals(&e).len(),
        5,
        "the refused one was not recorded"
    );
}

#[tokio::test]
async fn the_page_of_the_people_who_decide_says_how_many_more_are_open_when_it_lists_only_the_oldest()
 {
    let e = env();
    // The oldest one is the contributor's; a hundred and five more follow.
    open_appeal_of(&e, &e.contributor, common::NOW - 10_000);
    for n in 0..105_u8 {
        let who = common::person(20 + n);
        add_person(&e.w, &who, "Some Person", Some(Role::Contributor));
        open_appeal_of(&e, &who, common::NOW - 5_000 + i64::from(n));
    }
    let main = main_of(&as_(&e, &e.admin, "/handbook/-/appeals").await);
    assert_eq!(main.matches("class=\"change-head\"").count(), 100, "{main}");
    assert!(
        main.contains("and 6 more open. The ones that have waited longest are listed first"),
        "{main}"
    );
    let ja = main_of(&as_ja(&e, &e.admin, "/handbook/-/appeals").await);
    assert!(
        ja.contains("ほかに決定待ちの申し立てが 6 件あります"),
        "{ja}"
    );
    // With nothing cut, the sentence is not there.
    let fewer = env();
    open_appeal_of(&fewer, &fewer.contributor, common::NOW - 100);
    let main = main_of(&as_(&fewer, &fewer.admin, "/handbook/-/appeals").await);
    assert!(!main.contains("more open"), "{main}");
}

/// The pages that show an appeal's decision: the page of the people who decide, and the appellant's own.
const DECISION_PAGES: [&str; 2] = ["/handbook/-/appeals", "/handbook/-/appeals/mine"];

#[tokio::test]
async fn a_decision_says_the_rung_of_the_decider_and_the_rules_it_was_made_under_on_both_pages() {
    // Under the rules of a block (a policy file): the decider is the admin, and the block is named by its first characters.
    let e = env_under(MAINTAINERS_DECIDE);
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    let a = decide(&e, &e.admin, &appeal, "upheld").await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let block = &common::POLICY_BLOCK[..12];
    for (who, uri) in [
        (&e.admin, DECISION_PAGES[0]),
        (&e.contributor, DECISION_PAGES[1]),
    ] {
        let main = main_of(&as_(&e, who, uri).await);
        assert!(
            main.contains(&format!("Decided as admin, under the rules as of {block}.")),
            "{uri}: {main}"
        );
        assert!(!main.contains("were not recorded"), "{uri}: {main}");
        // Never the person: the rung is a role.
        let ja = main_of(&as_ja(&e, who, uri).await);
        assert!(
            ja.contains(&format!(
                "管理者 として決定しました。適用されたルール: 変更 {block} の時点。"
            )),
            "{uri}: {ja}"
        );
    }

    // Under the default rules: no block, and it says so.
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    decide(&e, &e.admin, &appeal, "overturned").await;
    for (who, uri) in [
        (&e.admin, DECISION_PAGES[0]),
        (&e.contributor, DECISION_PAGES[1]),
    ] {
        let main = main_of(&as_(&e, who, uri).await);
        assert!(
            main.contains("Decided as admin, under the default rules."),
            "{uri}: {main}"
        );
    }
}

#[tokio::test]
async fn a_decision_recorded_without_its_rung_and_rules_says_they_were_not_recorded() {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    let appeal = open_appeals(&e).pop().expect("filed");
    // A decision written as one was before the columns existed: no rung and no block, and nothing is guessed.
    e.w.store
        .decide_appeal(
            &appeal.id,
            &inklkunn_store::AppealDecision {
                at: common::NOW,
                by: e.admin.clone(),
                reason: REASON.to_owned(),
                rung: None,
                policy_block: None,
                outcome: inklkunn_store::AppealOutcome::Upheld,
            },
        )
        .unwrap();
    for (who, uri) in [
        (&e.admin, DECISION_PAGES[0]),
        (&e.contributor, DECISION_PAGES[1]),
    ] {
        let main = main_of(&as_(&e, who, uri).await);
        assert!(
            main.contains("The role and the rules this was decided under were not recorded."),
            "{uri}: {main}"
        );
        assert!(!main.contains("Decided as"), "{uri}: {main}");
        assert!(!main.contains("default rules"), "{uri}: {main}");
    }
}
