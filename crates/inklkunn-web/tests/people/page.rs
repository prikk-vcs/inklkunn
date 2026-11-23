//! The people page: the roles in order, the promotion texts, who holds each, what `entry` gives everyone, and who is told what,
//! in both languages and for each visibility.

use super::*;

fn headings(main: &str) -> Vec<String> {
    main.split("<h3 id=\"role-")
        .skip(1)
        .filter_map(|part| part.split('"').next().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn the_page_lists_the_roles_in_order_with_the_promotion_texts_and_who_holds_each() {
    let e = env_under(POLICY);
    let a = as_(&e, &e.contributor, PEOPLE).await;
    assert_eq!(a.status, StatusCode::OK);
    let main = main_of(&a);
    assert!(main.contains("<h1>People and roles</h1>"), "{main}");
    assert_eq!(
        headings(&main),
        [
            "observer-name",
            "questioner-name",
            "contributor-name",
            "reviewer-name",
            "maintainer-name",
            "admin-name"
        ],
        "lowest first"
    );
    // The wiki's own promotion text sits under the role, as guidance.
    assert!(
        main.contains("What this wiki looks for before it grants this role: A few edits that were accepted without a correction."),
        "{main}"
    );
    assert!(main.contains("A season of careful reading."), "{main}");
    // Who holds which: by the names the wiki shows people everywhere.
    for name in [
        "Ada Admin",
        "Mika Ito",
        "Rin Sato",
        "Aiko Tanaka",
        "Olu Reader",
    ] {
        assert!(main.contains(name), "{name}: {main}");
    }
    assert!(
        !main.contains("Nia Newcomer"),
        "who holds no role is on no list: {main}"
    );
    // Nothing promotes anyone by itself, and the page is no raw YAML.
    assert!(
        main.contains("Nothing here promotes anyone by itself"),
        "{main}"
    );
    for raw in ["entry:", "grant_up_to", "rungs:"] {
        assert!(!main.contains(raw), "{raw}: {main}");
    }
    // Where the rules say nothing of promotion, there is no text of the kind.
    let plain = cast(base(Visibility::Public));
    let main = main_of(&as_(&plain, &plain.contributor, PEOPLE).await);
    assert!(!main.contains("What this wiki looks for"), "{main}");
}

#[tokio::test]
async fn the_page_is_in_japanese_with_the_japanese_names_of_the_roles() {
    let e = env_under(POLICY);
    let a = as_ja(&e, &e.contributor, PEOPLE).await;
    assert_eq!(a.status, StatusCode::OK);
    let main = main_of(&a);
    for want in [
        "人と役割",
        "閲覧者",
        "質問者",
        "コントリビューター",
        "レビュアー",
        "メンテナー",
        "管理者",
        "このウィキがこの役割を付与する前に見ること: A season of careful reading.",
        "あなたの役割",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(!main.contains("People and roles"), "{main}");
}

#[tokio::test]
async fn a_visitor_with_no_account_is_told_the_names_of_the_reviewers_and_above_and_only_how_many_hold_the_rest()
 {
    let e = env();
    let a = get(&e.w, PEOPLE).await;
    assert_eq!(a.status, StatusCode::OK, "a public wiki is for everyone");
    let main = main_of(&a);
    for name in ["Ada Admin", "Mika Ito", "Max Mori", "Rin Sato"] {
        assert!(main.contains(name), "{name}: {main}");
    }
    for name in ["Aiko Tanaka", "Olu Reader"] {
        assert!(
            !main.contains(name),
            "{name} is not named to a visitor: {main}"
        );
    }
    assert!(main.contains("1 person holds this role."), "{main}");
    assert!(main.contains("You are not signed in"), "{main}");
    // No form is offered to a visitor.
    assert!(!main.contains("/-/people/"), "{main}");
}

#[tokio::test]
async fn a_restricted_wiki_answers_a_stranger_and_a_visitor_byte_for_byte_as_an_absent_wiki_does() {
    let w = base(Visibility::Restricted);
    let stranger = person(9);
    add_person(&w, &stranger, "Sue Stranger", None);
    let member = person(8);
    add_person(&w, &member, "Mem Member", Some(Role::Observer));
    for visitor in [None, Some(&stranger)] {
        for uri in [
            PEOPLE,
            "/handbook/-/people/grant",
            "/handbook/-/people/restrict",
        ] {
            let there = get_as(&w, uri, visitor).await;
            let absent = get_as(&w, &uri.replace("handbook", "no-such-wiki"), visitor).await;
            assert_eq!(there.status, absent.status, "{uri}");
            assert_eq!(
                there.body().replace("handbook", "no-such-wiki"),
                absent.body(),
                "{uri}: the same page, byte for byte"
            );
        }
    }
    // A member reads it.
    assert_eq!(
        get_as(&w, PEOPLE, Some(&member)).await.status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn what_entry_gives_everyone_is_said_in_a_sentence_for_each_visibility_and_has_no_effect_in_a_restricted_wiki()
 {
    for (visibility, entry, english, japanese) in [
        (
            Visibility::Public,
            "none",
            "A person who signs in holds no role in this wiki until someone grants them one",
            "サインインした人は、だれかが役割を付与するまで",
        ),
        (
            Visibility::Public,
            "contributor",
            "Every person who signs in holds the role contributor in this wiki without being granted it.",
            "サインインしたすべての人は、付与されなくても、このウィキで コントリビューター の役割を持ちます。",
        ),
        (
            Visibility::Internal,
            "questioner",
            "Every person who signs in holds the role questioner in this wiki without being granted it.",
            "サインインしたすべての人は、付与されなくても、このウィキで 質問者 の役割を持ちます。",
        ),
        (
            Visibility::Restricted,
            "contributor",
            "but that has no effect here: only people who were granted a role in this wiki can read it or take part.",
            "このウィキでは効力がありません。",
        ),
        (
            Visibility::Restricted,
            "none",
            "Only people who were granted a role in this wiki can read it or take part.",
            "このウィキの役割を付与された人だけが、読んだり参加したりできます。",
        ),
    ] {
        let e = cast(under(&base(visibility), &format!("entry: {entry}\n")));
        let a = as_(&e, &e.observer, PEOPLE).await;
        assert_eq!(a.status, StatusCode::OK, "{visibility:?} {entry}");
        assert!(
            main_of(&a).contains(english),
            "{visibility:?} {entry}: {}",
            main_of(&a)
        );
        let ja = as_ja(&e, &e.observer, PEOPLE).await;
        assert!(
            main_of(&ja).contains(japanese),
            "{visibility:?} {entry}: {}",
            main_of(&ja)
        );
    }
}

#[tokio::test]
async fn the_page_tells_a_person_which_role_they_hold_and_why_without_raw_rules() {
    let e = env_under("entry: questioner\n");
    // A grant that is above entry: the grant.
    let main = main_of(&as_(&e, &e.reviewer, PEOPLE).await);
    assert!(
        main.contains("You hold the role reviewer in this wiki, because it was granted to you."),
        "{main}"
    );
    // No grant: the wiki's entry.
    let main = main_of(&as_(&e, &e.newcomer, PEOPLE).await);
    assert!(
        main.contains("You hold the role questioner in this wiki, because the wiki gives it to everyone who signs in."),
        "{main}"
    );
    // Under the default rules a person without a grant holds none.
    let plain = cast(base(Visibility::Public));
    let main = main_of(&as_(&plain, &plain.newcomer, PEOPLE).await);
    assert!(
        main.contains("You hold no role in this wiki: you can read it, and nothing more."),
        "{main}"
    );
    // And in Japanese.
    let ja = main_of(&as_ja(&plain, &plain.reviewer, PEOPLE).await);
    assert!(
        ja.contains(
            "あなたはこのウィキで レビュアー の役割を持っています。この役割は付与されたものです。"
        ),
        "{ja}"
    );
}

#[tokio::test]
async fn the_about_page_links_to_the_people_page() {
    let e = env();
    let main = main_of(&get(&e.w, "/handbook/-/about").await);
    assert!(
        main.contains("<a href=\"/handbook/-/people\">People and roles of this wiki</a>"),
        "{main}"
    );
}

#[tokio::test]
async fn what_a_person_may_do_is_offered_as_links_and_nothing_else_is() {
    let e = env_under(POLICY);
    // A maintainer whose rules let maintainers grant up to reviewer: sees the form, and a link for each person in reach.
    let main = main_of(&as_(&e, &e.maintainer, PEOPLE).await);
    assert!(
        main.contains("/handbook/-/people/grant\">Set a person&#39;s role</a>")
            || main.contains("/handbook/-/people/grant\">Set a person's role</a>"),
        "{main}"
    );
    assert!(
        main.contains("/handbook/-/people/restrict\">Restrict a person</a>"),
        "{main}"
    );
    let aiko = format!("/handbook/-/people/grant?account={}", e.contributor);
    assert!(
        main.contains(&aiko),
        "a contributor is within the reach: {main}"
    );
    let rin = format!("/handbook/-/people/grant?account={}", e.reviewer);
    assert!(main.contains(&rin), "up to reviewer: {main}");
    let ada = format!("/handbook/-/people/grant?account={}", e.admin);
    assert!(
        !main.contains(&ada),
        "an admin stands above the reach: {main}"
    );
    let mika = format!("/handbook/-/people/grant?account={}", e.maintainer);
    assert!(!main.contains(&mika), "nobody sets their own role: {main}");
    // To restrict: below themselves only.
    assert!(
        main.contains(&format!(
            "/handbook/-/people/restrict?account={}",
            e.reviewer
        )),
        "{main}"
    );
    assert!(
        !main.contains(&format!(
            "/handbook/-/people/restrict?account={}",
            e.other_maintainer
        )),
        "{main}"
    );
    assert!(
        !main.contains(&format!("/handbook/-/people/restrict?account={}", e.admin)),
        "{main}"
    );
    // A contributor sees neither form.
    let main = main_of(&as_(&e, &e.contributor, PEOPLE).await);
    assert!(!main.contains("/handbook/-/people/"), "{main}");
    // Under the default rules a maintainer changes no one's role, and may still restrict.
    let plain = cast(base(Visibility::Public));
    let main = main_of(&as_(&plain, &plain.maintainer, PEOPLE).await);
    assert!(!main.contains("/handbook/-/people/grant"), "{main}");
    assert!(main.contains("/handbook/-/people/restrict\">"), "{main}");
}

#[tokio::test]
async fn the_restricted_are_listed_with_reason_and_end_to_those_who_may_restrict_and_to_no_one_else()
 {
    let e = env();
    let a = post_form(
        &e.w,
        "/handbook/-/people/restrict",
        Some(&e.maintainer),
        &restrict_form(
            &e.contributor.to_string(),
            "3",
            "pasting the same link into every page",
        ),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/people?done=restricted")
    );
    let until = "2023-11-17 22:13 UTC";
    let seen_by_moderator = main_of(&as_(&e, &e.admin, PEOPLE).await);
    assert!(
        seen_by_moderator.contains("Restricted people"),
        "{seen_by_moderator}"
    );
    assert!(
        seen_by_moderator.contains(&format!("Aiko Tanaka, until {until}.")),
        "{seen_by_moderator}"
    );
    assert!(
        seen_by_moderator.contains("The reason given: pasting the same link into every page"),
        "{seen_by_moderator}"
    );
    assert!(
        seen_by_moderator.contains(&format!(
            "/handbook/-/people/lift?account={}",
            e.contributor
        )),
        "{seen_by_moderator}"
    );
    // Not told to a reviewer, a stranger or a visitor.
    for visitor in [Some(&e.reviewer), Some(&e.newcomer), None] {
        let main = main_of(&get_as(&e.w, PEOPLE, visitor).await);
        assert!(
            !main.contains("pasting the same link"),
            "{visitor:?}: {main}"
        );
        assert!(!main.contains("Restricted people"), "{visitor:?}: {main}");
        assert!(!main.contains(until), "{visitor:?}: {main}");
    }
    // The person is told on their own page, and sees no list.
    let own = main_of(&as_(&e, &e.contributor, PEOPLE).await);
    assert!(own.contains("You are restricted in this wiki"), "{own}");
    assert!(
        own.contains("The reason given: pasting the same link into every page"),
        "{own}"
    );
    assert!(
        own.contains(&format!("The restriction ends on {until}.")),
        "{own}"
    );
    assert!(!own.contains("Restricted people"), "{own}");
    let _ = NOW;
}
