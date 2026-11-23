//! The forms: set a role, restrict, renew, lift. Each is a form with no script, needs an account and the session's token, is
//! authorized by the core, and ends with a redirect to the people page or says in words what it refused.

use super::*;

const GRANT: &str = "/handbook/-/people/grant";
const RESTRICT: &str = "/handbook/-/people/restrict";
const RENEW: &str = "/handbook/-/people/renew";
const LIFT: &str = "/handbook/-/people/lift";

#[tokio::test]
async fn the_grant_form_offers_the_roles_up_to_the_reach_with_the_promotion_text_beside_each() {
    let e = env_under(POLICY);
    let a = as_(&e, &e.maintainer, GRANT).await;
    assert_eq!(a.status, StatusCode::OK);
    let main = main_of(&a);
    assert!(main.contains("<h1>Set a role</h1>"), "{main}");
    assert!(
        main.contains("You can give roles up to reviewer."),
        "{main}"
    );
    for role in ["observer", "questioner", "contributor", "reviewer"] {
        assert!(
            main.contains(&format!("value=\"{role}\"")),
            "{role}: {main}"
        );
    }
    for above in ["maintainer", "admin"] {
        assert!(
            !main.contains(&format!("value=\"{above}\"")),
            "{above} is above the reach: {main}"
        );
    }
    assert!(
        main.contains("value=\"none\""),
        "taking the role away: {main}"
    );
    assert!(
        main.contains(
            "What this wiki looks for before it grants this role: A season of careful reading."
        ),
        "the promotion text, beside the role: {main}"
    );
    assert!(main.contains("name=\"csrf\""), "{main}");
    assert!(!main.contains("<script"), "{main}");
    // An admin is offered every role.
    let main = main_of(&as_(&e, &e.admin, GRANT).await);
    assert!(main.contains("You can give roles up to admin."), "{main}");
    assert!(main.contains("value=\"admin\""), "{main}");
    // Opened on a person the page names: by name, never by the id the link carries.
    let main = main_of(
        &as_(
            &e,
            &e.maintainer,
            &format!("{GRANT}?account={}", e.contributor),
        )
        .await,
    );
    assert!(main.contains("Aiko Tanaka"), "{main}");
    assert!(
        main.contains(&format!(
            "type=\"hidden\" name=\"who\" value=\"{}\"",
            e.contributor
        )),
        "{main}"
    );
    assert!(
        !main.contains("The person</label>"),
        "no field to type a name into: {main}"
    );
    // An id the page does not name is no one: the form asks for a name.
    let stranger = AccountId::new();
    let main = main_of(&as_(&e, &e.maintainer, &format!("{GRANT}?account={stranger}")).await);
    assert!(!main.contains(&stranger.to_string()), "{main}");
    assert!(main.contains("The person</label>"), "{main}");
}

#[tokio::test]
async fn a_maintainer_sets_a_role_within_reach_and_it_is_recorded_with_who_acted_and_their_role() {
    let e = env_under(POLICY);
    let who = e.newcomer.to_string();
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form(&who, "contributor", "three good edits"),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/people?done=granted")
    );
    assert_eq!(grant_of(&e, &e.newcomer), Some(Role::Contributor));
    let decisions =
        e.w.store
            .grant_decisions(&e.w.wiki.id, Some(&e.newcomer), 10)
            .unwrap();
    let [d] = decisions.as_slice() else {
        panic!("{decisions:?}")
    };
    assert_eq!(
        d.by.as_ref().map(|a| (&a.account, a.rung)),
        Some((&e.maintainer, Role::Maintainer))
    );
    assert_eq!(d.policy_block.as_deref(), Some(common::POLICY_BLOCK));
    assert_eq!(d.reason.as_deref(), Some("three good edits"));
    // The page then says so.
    let main = main_of(&as_(&e, &e.maintainer, "/handbook/-/people?done=granted").await);
    assert!(main.contains("The role was set."), "{main}");
    // The person holds it now and says why.
    let own = main_of(&as_(&e, &e.newcomer, PEOPLE).await);
    assert!(own.contains("You hold the role contributor"), "{own}");
    // Taking a role away is the same form.
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form(&who, "none", ""),
    )
    .await;
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/people?done=revoked"),
        "{}",
        a.body()
    );
    assert_eq!(grant_of(&e, &e.newcomer), None);
    let main = main_of(&as_(&e, &e.maintainer, "/handbook/-/people?done=revoked").await);
    assert!(main.contains("The role was taken away."), "{main}");
    // The person can also be named by a sign-in name or an id: a name that is no one's is a problem of the form.
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form("nobody-here", "observer", ""),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        main_of(&a).contains("There is no account with that sign-in name or id."),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_role_above_the_reach_one_s_own_role_and_the_last_choices_are_problems_of_the_form_and_change_nothing()
 {
    let e = env_under(POLICY);
    let before = grant_of(&e, &e.contributor);
    let who = e.contributor.to_string();
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form(&who, "maintainer", ""),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let main = main_of(&a);
    assert!(
        main.contains("let a maintainer give and take away roles only up to reviewer, and this goes above that."),
        "{main}"
    );
    assert!(main.contains("Please fix this first"), "{main}");
    // What was typed comes back.
    assert!(main.contains(&format!("value=\"{who}\"")), "{main}");
    // Demoting someone who stands above the reach.
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form(&e.other_maintainer.to_string(), "observer", ""),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(grant_of(&e, &e.other_maintainer), Some(Role::Maintainer));
    // One's own role.
    let me = e.maintainer.to_string();
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form(&me, "observer", ""),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        main_of(&a).contains("Nobody sets their own role."),
        "{}",
        a.body()
    );
    // The same role again; no choice; a reason that is too long.
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form(&who, "contributor", ""),
    )
    .await;
    assert!(
        main_of(&a).contains("This person already holds that role."),
        "{}",
        a.body()
    );
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &[("who", who.as_str()), ("reason", "")],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        main_of(&a).contains("Choose a role, or choose to take the role away."),
        "{}",
        a.body()
    );
    let long = "x".repeat(600);
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.maintainer),
        &set_form(&who, "reviewer", &long),
    )
    .await;
    assert!(
        main_of(&a).contains("The reason is longer than 500 characters."),
        "{}",
        a.body()
    );
    assert_eq!(grant_of(&e, &e.contributor), before, "nothing was changed");
    assert!(
        e.w.store
            .grant_decisions(&e.w.wiki.id, None, 10)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn an_admin_sets_any_role_and_a_maintainer_under_the_default_rules_is_not_allowed() {
    let e = env_under(POLICY);
    let a = post_form(
        &e.w,
        GRANT,
        Some(&e.admin),
        &set_form(&e.reviewer.to_string(), "maintainer", ""),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(grant_of(&e, &e.reviewer), Some(Role::Maintainer));
    // The default rules: a maintainer gives no role.
    let plain = cast(base(Visibility::Public));
    for a in [
        as_(&plain, &plain.maintainer, GRANT).await,
        post_form(
            &plain.w,
            GRANT,
            Some(&plain.maintainer),
            &set_form(&plain.newcomer.to_string(), "observer", ""),
        )
        .await,
    ] {
        assert_eq!(a.status, StatusCode::FORBIDDEN);
        let main = main_of(&a);
        assert!(main.contains("Not allowed"), "{main}");
        assert!(
            main.contains("Your role in this wiki does not include this. Nothing was changed."),
            "{main}"
        );
    }
    assert_eq!(grant_of(&plain, &plain.newcomer), None);
    // Anyone below a maintainer, and a visitor with no account.
    for who in [&plain.contributor, &plain.reviewer, &plain.observer] {
        assert_eq!(as_(&plain, who, GRANT).await.status, StatusCode::FORBIDDEN);
    }
    let a = get(&plain.w, GRANT).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(
        main_of(&a).contains("sign in") || main_of(&a).contains("account"),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_form_post_is_refused_without_the_sessions_token_and_from_another_site() {
    let e = env_under(POLICY);
    let who = e.newcomer.to_string();
    // The test's `post_form` adds the token for a signed-in person; none is a request that is not the session's.
    let a = common::send_body(
        &e.w,
        "POST",
        GRANT,
        Some(&e.maintainer),
        &[(common::NO_TOKEN, "1"), ("sec-fetch-site", "same-origin")],
        format!("who={who}&role=observer"),
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert_eq!(grant_of(&e, &e.newcomer), None);
    let a = common::send_body(
        &e.w,
        "POST",
        GRANT,
        Some(&e.maintainer),
        &[("sec-fetch-site", "cross-site")],
        format!("who={who}&role=observer&csrf={}", common::TEST_CSRF),
    )
    .await;
    assert_eq!(a.status, StatusCode::FORBIDDEN, "{}", a.body());
    assert_eq!(grant_of(&e, &e.newcomer), None);
}

#[tokio::test]
async fn a_moderator_restricts_renews_and_lifts_and_each_step_is_recorded_and_told() {
    let e = env();
    let who = e.contributor.to_string();
    let a = post_form(
        &e.w,
        RESTRICT,
        Some(&e.maintainer),
        &restrict_form(&who, "2", "spam"),
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/people?done=restricted")
    );
    let chain = restriction_of(&e, &e.contributor);
    let [set] = chain.as_slice() else {
        panic!("{chain:?}")
    };
    assert_eq!(
        (set.by.account.clone(), set.by.rung),
        (e.maintainer.clone(), Role::Maintainer)
    );
    assert_eq!(set.expires_at, Some(NOW + 2 * DAY));
    assert_eq!(set.reason.as_deref(), Some("spam"));
    // Restricting twice says to renew.
    let a = post_form(
        &e.w,
        RESTRICT,
        Some(&e.maintainer),
        &restrict_form(&who, "2", "spam"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        main_of(&a).contains(
            "is restricted until 2023-11-16 22:13 UTC already. Renew the restriction to change it."
        ),
        "{}",
        a.body()
    );
    // The renew form, opened on the restricted person from the list.
    let main = main_of(&as_(&e, &e.maintainer, &format!("{RENEW}?account={who}")).await);
    assert!(main.contains("<h1>Renew a restriction</h1>"), "{main}");
    assert!(main.contains("Aiko Tanaka"), "{main}");
    let a = post_form(
        &e.w,
        RENEW,
        Some(&e.admin),
        &restrict_form(&who, "5", "still spam"),
    )
    .await;
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/people?done=renewed"),
        "{}",
        a.body()
    );
    let own = main_of(&as_(&e, &e.contributor, PEOPLE).await);
    assert!(own.contains("The reason given: still spam"), "{own}");
    assert!(
        own.contains("The restriction ends on 2023-11-19 22:13 UTC."),
        "{own}"
    );
    // The lift form, and the lifting.
    let main = main_of(&as_(&e, &e.admin, &format!("{LIFT}?account={who}")).await);
    assert!(main.contains("<h1>Lift a restriction</h1>"), "{main}");
    assert!(!main.contains("name=\"days\""), "{main}");
    let a = post_form(
        &e.w,
        LIFT,
        Some(&e.admin),
        &[("who", who.as_str()), ("reason", "settled")],
    )
    .await;
    assert_eq!(
        a.header("location"),
        Some("/handbook/-/people?done=lifted"),
        "{}",
        a.body()
    );
    let own = main_of(&as_(&e, &e.contributor, PEOPLE).await);
    assert!(!own.contains("You are restricted"), "{own}");
    let kinds: Vec<MeasureEvent> = restriction_of(&e, &e.contributor)
        .iter()
        .map(|r| r.event)
        .collect();
    assert_eq!(
        kinds,
        [
            MeasureEvent::Set,
            MeasureEvent::Renewed,
            MeasureEvent::Lifted
        ]
    );
    // Nothing left to renew or lift.
    let a = post_form(
        &e.w,
        LIFT,
        Some(&e.admin),
        &[("who", who.as_str()), ("reason", "")],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        main_of(&a).contains("This person is not restricted."),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_maintainer_cannot_restrict_a_maintainer_or_an_admin_and_an_admin_restricts_a_maintainer_but_not_an_admin()
 {
    let e = env();
    let restrict = |by: &AccountId, who: &AccountId| {
        let by = by.clone();
        let who = who.to_string();
        let w = &e.w;
        async move { post_form(w, RESTRICT, Some(&by), &restrict_form(&who, "3", "reason")).await }
    };
    let a = restrict(&e.maintainer, &e.other_maintainer).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        main_of(&a).contains("You can restrict only a person whose role is below your own."),
        "{}",
        a.body()
    );
    let a = restrict(&e.maintainer, &e.admin).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        main_of(&a).contains("Nobody restricts an admin of a wiki."),
        "{}",
        a.body()
    );
    let a = restrict(&e.maintainer, &e.maintainer).await;
    assert!(
        main_of(&a).contains("Nobody acts on their own restriction."),
        "{}",
        a.body()
    );
    let a = restrict(&e.admin, &e.admin).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(restriction_of(&e, &e.other_maintainer).is_empty());
    assert!(restriction_of(&e, &e.admin).is_empty());
    let a = restrict(&e.admin, &e.other_maintainer).await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    assert_eq!(restriction_of(&e, &e.other_maintainer).len(), 1);
    // Below a maintainer there is no form at all.
    assert_eq!(
        as_(&e, &e.reviewer, RESTRICT).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        as_(&e, &e.contributor, RENEW).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        as_(&e, &e.observer, LIFT).await.status,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn a_restriction_longer_than_the_operators_limit_is_refused_naming_the_limit_and_so_is_one_with_no_reason()
 {
    let e = env();
    let who = e.contributor.to_string();
    for days in ["91", "0", "365", "many", "-3"] {
        let a = post_form(
            &e.w,
            RESTRICT,
            Some(&e.maintainer),
            &restrict_form(&who, days, "r"),
        )
        .await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{days}");
        assert!(
            main_of(&a).contains("A restriction lasts from 1 day to 90 days."),
            "{days}: {}",
            a.body()
        );
    }
    let main = main_of(&as_(&e, &e.maintainer, RESTRICT).await);
    assert!(
        main.contains("From 1 to 90 days: the most the person who runs this server allows."),
        "{main}"
    );
    let a = post_form(
        &e.w,
        RESTRICT,
        Some(&e.maintainer),
        &restrict_form(&who, "3", "  "),
    )
    .await;
    assert!(
        main_of(&a).contains("Write the reason: the person is told it."),
        "{}",
        a.body()
    );
    assert!(restriction_of(&e, &e.contributor).is_empty());
    let ok = post_form(
        &e.w,
        RESTRICT,
        Some(&e.maintainer),
        &restrict_form(&who, "90", "r"),
    )
    .await;
    assert_eq!(ok.status, StatusCode::SEE_OTHER);
}

#[tokio::test]
async fn the_forms_are_in_japanese_for_a_reader_who_chose_it() {
    let e = env_under(POLICY);
    let ja = |uri: &'static str| as_ja(&e, &e.maintainer, uri);
    let main = main_of(&ja(GRANT).await);
    for want in [
        "役割を設定する",
        "あなたが付与できる役割は、レビュアー までです。",
        "役割なし（役割を取り消す）",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    let main = main_of(&ja(RESTRICT).await);
    for want in [
        "人を制限する",
        "何日間",
        "1 日から 90 日まで。",
        "本人に伝える理由",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(main_of(&ja(RENEW).await).contains("制限を更新する"));
    assert!(main_of(&ja(LIFT).await).contains("制限を解除する"));
}
