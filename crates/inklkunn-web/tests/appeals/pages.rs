//! The pages of appeals: where *contest this* is offered and to whom, the form, a person's own appeals, the page of the people who
//! decide, in both languages, and what no one else reads.

use super::*;

fn plain(body: String) -> String {
    body.replace("&#38;", "&")
}

#[tokio::test]
async fn a_restricted_person_is_offered_contest_this_on_the_refusal_and_on_the_people_page_and_nobody_else_is()
 {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    let link = format!("/handbook/-/appeals/new?kind=restriction&id={}", r.id);

    // The page that refuses them says why, and offers the way to contest it.
    let refused = as_(&e, &e.contributor, "/handbook/-/proposals/new").await;
    assert_eq!(refused.status, StatusCode::FORBIDDEN, "{}", refused.body());
    let body = plain(refused.body());
    assert!(body.contains("You are restricted in this wiki"), "{body}");
    assert!(body.contains("Contest this"), "{body}");
    assert!(body.contains(&link), "{body}");

    // Their own page in the wiki's people: the same link, and the way to their appeals.
    let people = plain(as_(&e, &e.contributor, "/handbook/-/people").await.body());
    assert!(people.contains(&link), "{people}");
    assert!(people.contains("/handbook/-/appeals/mine"), "{people}");

    // Nobody else is offered it, and the restriction is not named to them.
    for who in [&e.bystander, &e.maintainer, &e.admin] {
        let people = plain(as_(&e, who, "/handbook/-/people").await.body());
        assert!(!people.contains(&link), "{who}: {people}");
    }
    let anonymous = plain(common::get(&e.w, "/handbook/-/people").await.body());
    assert!(!anonymous.contains(&link), "{anonymous}");
}

#[tokio::test]
async fn the_form_files_the_appeal_of_a_restricted_person_in_either_language_and_says_what_is_contested()
 {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    let uri = format!("/handbook/-/appeals/new?kind=restriction&id={}", r.id);

    // The form: no script, a token, what is contested in the person's own sentence, and the one text.
    let a = as_(&e, &e.contributor, &uri).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    assert!(main.contains("<h1>Contest this decision</h1>"), "{main}");
    assert!(main.contains("You are restricted until"), "{main}");
    assert!(main.contains("The reason given: Edit-warring."), "{main}");
    assert!(main.contains("<textarea"), "{main}");
    assert!(main.contains("name=\"csrf\""), "{main}");
    assert!(!main.contains("<script"), "{main}");
    let ja = main_of(&as_ja(&e, &e.contributor, &uri).await);
    assert!(ja.contains("<h1>この決定に申し立てをする</h1>"), "{ja}");
    assert!(ja.contains("まで制限を受けています"), "{ja}");

    // A text with a problem shows the form again with what was typed.
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/new",
        Some(&e.contributor),
        &[
            ("kind", "restriction"),
            ("id", &r.id.to_string()),
            ("text", "  "),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(
        a.body().contains("Write what you want to say."),
        "{}",
        a.body()
    );
    assert!(open_appeals(&e).is_empty());

    // Filing: they are held down, and may file all the same; it ends at their own page with a note.
    let a = file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/appeals/mine?done=filed")
    );
    let appeal = open_appeals(&e).pop().expect("filed");
    assert_eq!(appeal.text, GRIEVANCE);
    let mine = main_of(&as_(&e, &e.contributor, "/handbook/-/appeals/mine?done=filed").await);
    assert!(mine.contains("Your appeal was filed."), "{mine}");
    assert!(mine.contains("ZEBRAWORDS"), "{mine}");
    assert!(mine.contains("Waiting for a decision."), "{mine}");
    assert!(
        mine.contains("You have appealed this."),
        "once filed, no second link: {mine}"
    );
    let mine_ja = main_of(&as_ja(&e, &e.contributor, "/handbook/-/appeals/mine").await);
    assert!(mine_ja.contains("<h1>自分の申し立て</h1>"), "{mine_ja}");
    assert!(mine_ja.contains("決定を待っています。"), "{mine_ja}");
    // They still cannot write: filing is not a way round the restriction.
    let propose = as_(&e, &e.contributor, "/handbook/-/proposals/new").await;
    assert_eq!(propose.status, StatusCode::FORBIDDEN);
    // And a second appeal of the same restriction is refused, naming the first.
    let again = file(&e, &e.contributor, "restriction", &r.id.to_string()).await;
    assert_eq!(again.status, StatusCode::CONFLICT, "{}", again.body());
    assert!(
        again.body().contains("already appealed this"),
        "{}",
        again.body()
    );
    assert_eq!(open_appeals(&e).len(), 1);
}

#[tokio::test]
async fn a_form_for_what_is_not_theirs_or_not_there_says_the_same_to_everyone() {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    let theirs = format!("/handbook/-/appeals/new?kind=restriction&id={}", r.id);
    let nothing =
        "/handbook/-/appeals/new?kind=restriction&id=019a0000-0000-7000-8000-00000000ffff";
    let garbage = "/handbook/-/appeals/new?kind=nonsense&id=x";
    let mut answers = Vec::new();
    for (who, uri) in [
        (&e.bystander, theirs.as_str()),
        (&e.maintainer, theirs.as_str()),
        (&e.contributor, nothing),
        (&e.contributor, garbage),
    ] {
        let a = as_(&e, who, uri).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{who}: {}", a.body());
        answers.push(main_of(&a));
    }
    assert!(
        answers.windows(2).all(|pair| pair.first() == pair.get(1)),
        "a decision that is not theirs, one that is not there and a malformed one read alike: {answers:?}"
    );
    let first = answers.first().expect("an answer");
    assert!(first.contains("Nothing to contest here"), "{first}");
    // A visitor who is not signed in has no account to appeal with.
    let a = common::get(&e.w, &theirs).await;
    assert!(a.status.is_client_error(), "{}", a.status);
}

#[tokio::test]
async fn the_page_of_the_deciders_lists_the_appeals_with_their_words_and_is_theirs_alone() {
    let e = env();
    restrict(&e, &e.contributor, "3").await;
    let r = restriction_of(&e, &e.contributor);
    assert_eq!(
        file(&e, &e.contributor, "restriction", &r.id.to_string())
            .await
            .status,
        StatusCode::SEE_OTHER
    );
    let queue = as_(&e, &e.admin, "/handbook/-/appeals").await;
    assert_eq!(queue.status, StatusCode::OK, "{}", queue.body());
    let main = main_of(&queue);
    assert!(main.contains("Waiting for a decision"), "{main}");
    assert!(
        main.contains("Aiko Tanaka: appeal against a restriction"),
        "{main}"
    );
    assert!(main.contains("ZEBRAWORDS"), "{main}");
    assert!(main.contains("The action was taken by Mika Ito."), "{main}");
    assert!(main.contains("name=\"decision\""), "{main}");
    assert!(!main.contains("<script"), "{main}");
    let ja = main_of(&as_ja(&e, &e.admin, "/handbook/-/appeals").await);
    assert!(ja.contains("<h1>申し立て</h1>"), "{ja}");
    assert!(ja.contains("Aiko Tanaka の申し立て: 制限"), "{ja}");
    // No one who does not decide reads it: not a contributor, not a maintainer under the default rules, not a visitor.
    for who in [&e.contributor, &e.maintainer, &e.bystander] {
        let a = as_(&e, who, "/handbook/-/appeals").await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{who}");
        assert!(!a.body().contains("ZEBRAWORDS"), "{}", a.body());
    }
    let a = common::get(&e.w, "/handbook/-/appeals").await;
    assert!(a.status.is_client_error(), "{}", a.status);
    assert!(!a.body().contains("ZEBRAWORDS"));
}

#[tokio::test]
async fn the_proposer_of_a_declined_proposal_is_offered_contest_this_on_its_page_and_nobody_else_is()
 {
    let e = env();
    let id = propose(&e, &e.contributor, "A correction").await;
    let uri = format!("/handbook/-/proposals/{id}");
    assert!(
        !plain(as_(&e, &e.contributor, &uri).await.body()).contains("Contest this"),
        "a proposal that is held for review has nothing to contest"
    );
    decline(&e, &e.maintainer, &id).await;
    let position =
        e.w.store
            .last_transition_position(&id)
            .unwrap()
            .expect("the decline's place in the log");
    let link = format!("/handbook/-/appeals/new?kind=decline&id={position}");
    for (who, sees) in [
        (Some(&e.contributor), true),
        (Some(&e.bystander), false),
        (Some(&e.maintainer), false),
        (Some(&e.admin), false),
        (None, false),
    ] {
        let body = plain(get_as(&e.w, &uri, who).await.body());
        assert_eq!(body.contains(&link), sees, "{who:?}: {body}");
    }
    // The form says what is contested, and the filing is recorded against the decline's place in the proposal log.
    let form = main_of(&as_(&e, &e.contributor, &link).await);
    assert!(form.contains("Your proposal was declined."), "{form}");
    let a = file(&e, &e.contributor, "decline", &position.to_string()).await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let appeal = open_appeals(&e).pop().expect("filed");
    assert_eq!(appeal.action.id, position.to_string());
    assert_eq!(appeal.action_actor, Some(e.maintainer.clone()));
    // Once filed, the page offers no second link and says so.
    let body = plain(as_(&e, &e.contributor, &uri).await.body());
    assert!(!body.contains(&link), "{body}");
    assert!(body.contains("You have appealed this."), "{body}");
}

#[tokio::test]
async fn a_stranger_to_a_closed_wiki_is_answered_as_if_the_wiki_were_not_there() {
    let w = with_accounts(World::with_visibility(
        "closed",
        "Closed",
        Visibility::Restricted,
        &[("pages/Home.md", &home_with("Private."))],
    ));
    let (member, stranger) = (person(1), person(2));
    add_person(&w, &member, "Ada Admin", Some(Role::Admin));
    add_person(&w, &stranger, "Sam Stranger", None);
    for uri in [
        "/closed/-/appeals",
        "/closed/-/appeals/mine",
        "/closed/-/appeals/new?kind=restriction&id=019a0000-0000-7000-8000-00000000ffff",
    ] {
        let gone = get_as(&w, uri, Some(&stranger)).await;
        let absent = get_as(&w, &uri.replace("/closed/", "/nowhere/"), Some(&stranger)).await;
        assert_eq!(gone.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(gone.status, absent.status, "{uri}");
        assert_eq!(
            gone.body().replace("closed", "nowhere"),
            absent.body(),
            "{uri}: byte for byte an absent wiki's"
        );
    }
    let a = post_form(
        &w,
        "/closed/-/appeals/new",
        Some(&stranger),
        &[("kind", "restriction"), ("id", "x"), ("text", "hello")],
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    let a = post_form(
        &w,
        "/closed/-/appeals/decide",
        Some(&stranger),
        &[("appeal", "x"), ("decision", "upheld"), ("reason", "x")],
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
}
