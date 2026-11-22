#![allow(clippy::expect_used, clippy::unwrap_used)]
//! RFC 006 D-5, end to end (handoff G-T1 part C): **the product's own wiring**. The real server (the binary's `prepare`)
//! over a real prikk reads the rules of a wiki from the repository's default view, through the read service and the derived
//! store, and decides by them: a policy accepted through the write API changes who may accept from the next decision on, a
//! policy that asks for more approvals than people can give shows its warning on the proposal's page, an admin's token
//! accepts a change back by the hatch, and what the API will not write under `governance/` is refused by name.

use super::api::{API, api, json_field, json_quote};
use super::*;

/// A proposal body that writes one file at `path`.
fn file_body(summary: &str, path: &str, text: &str) -> String {
    format!(
        "{{\"summary\":{},\"submit\":true,\"files\":[{{\"path\":{},\"text\":{}}}]}}",
        json_quote(summary),
        json_quote(path),
        json_quote(text)
    )
}

#[test]
fn a_policy_accepted_through_the_api_changes_who_may_accept_and_the_warning_and_the_hatch_are_the_pages_and_the_tokens()
 {
    let Some(world) = World::new("gov") else {
        return;
    };
    let running = Running::start(&world);
    let mut noor = join(
        &world,
        &running,
        "noor",
        "Noor Contributor",
        Role::Contributor,
    );
    let mut maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let _quinn = join(&world, &running, "quinn", "Quinn Reviewer", Role::Reviewer);
    let _ada = join(&world, &running, "ada", "Ada Admin", Role::Admin);
    let auth = running.services.auth.clone().expect("sign-in is served");
    let token = |login: &str, scope: Scope| -> String {
        auth.issue_token(&world.account_of(login), scope, "gov", 3600, now())
            .unwrap()
            .secret
            .as_str()
            .to_owned()
    };
    let (noor_propose, quinn_accept, maya_accept, ada_accept) = (
        token("noor", Scope::Propose),
        token("quinn", Scope::Accept),
        token("maya", Scope::Accept),
        token("ada", Scope::Admin),
    );
    let create = format!("{API}/proposals");
    let acceptances = |id: &str| format!("{API}/proposals/{id}/acceptances");
    let raw = |what: &str| -> String {
        let info = api(
            &running,
            "GET",
            &format!("{API}/raw/{what}"),
            None,
            None,
            None,
        );
        assert_eq!(info.status, 200, "{}", info.body);
        format!("\"{}\"", json_field(&info.body, "state_id").expect("a tip"))
    };
    let open = |who: &str, body: &str| -> (String, String) {
        let made = api(
            &running,
            "POST",
            &create,
            Some(who),
            Some(&raw("Help")),
            Some(body),
        );
        assert_eq!(made.status, 201, "{}\n{}", made.head, made.body);
        (
            json_field(&made.body, "id").expect("proposal id"),
            json_field(&made.body, "etag").expect("etag"),
        )
    };
    let accept = |who: &str, id: &str, tag: &str| {
        api(
            &running,
            "POST",
            &acceptances(id),
            Some(who),
            Some(tag),
            Some("{}"),
        )
    };
    let page_edit = |summary: &str, edit: &dyn Fn(&str) -> String| {
        let source = wire(
            running.port,
            "GET",
            "/handbook/-/raw/Help",
            &[("Host".into(), format!("localhost:{}", running.content_port))],
            &[],
        )
        .body;
        let body = format!(
            "{{\"summary\":{},\"submit\":true,\"files\":[{{\"path\":\"pages/Help.md\",\"text\":{}}}]}}",
            json_quote(summary),
            json_quote(&edit(&source))
        );
        open(&noor_propose, &body)
    };

    // What the API will write under `governance/`: the two files, and nothing else, by name.
    let other = api(
        &running,
        "POST",
        &create,
        Some(&noor_propose),
        Some(&raw("Help")),
        Some(&file_body(
            "Another file",
            "governance/other.yaml",
            "entry: none\n",
        )),
    );
    assert_eq!(other.status, 422, "{}\n{}", other.head, other.body);
    assert_eq!(
        json_field(&other.body, "reason").as_deref(),
        Some("governance-path"),
        "{}",
        other.body
    );

    // Under the defaults a reviewer cannot accept.
    let (page_one, page_one_tag) = page_edit("Say which brackets", &clarify_reading);
    let refused = accept(&quinn_accept, &page_one, &page_one_tag);
    assert_eq!(refused.status, 403, "{}\n{}", refused.head, refused.body);

    // A policy that lets reviewers accept is a proposal like another; a maintainer accepts it, under the defaults.
    let (rules, rules_tag) = open(
        &noor_propose,
        &file_body(
            "Let reviewers accept",
            "governance/policy.yaml",
            "rungs:\n  accept: reviewer\n",
        ),
    );
    let before = world.blocks();
    let signed = accept(&maya_accept, &rules, &rules_tag);
    assert_eq!(signed.status, 200, "{}\n{}", signed.head, signed.body);
    assert_eq!(world.blocks().len(), before.len() + 1);

    // From the next decision on the reviewer may accept the page she was refused.
    let accepted = accept(&quinn_accept, &page_one, &page_one_tag);
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert_eq!(world.blocks().len(), before.len() + 2);
    assert!(
        page_text(&mut maya.browser, HELP).contains(READING_AFTER),
        "the page the reviewer accepted is the page now"
    );

    // A policy that asks for five approvals, when three people besides the proposer could give them: accepted, with its
    // warning on every open proposal's page.
    let (ask, ask_tag) = open(
        &noor_propose,
        &file_body(
            "Five approvals",
            "governance/policy.yaml",
            "rungs:\n  accept: reviewer\nacceptance:\n  approvals: 5\n",
        ),
    );
    let (page_two, page_two_tag) = page_edit("Say that it says so", &say_so);
    let before_warning = noor.browser.get(&proposal_page(&page_two));
    assert!(
        !before_warning
            .body
            .contains("approvals from people other than the proposer"),
        "no warning while the rules ask for none: {}",
        before_warning.body
    );
    let signed = accept(&maya_accept, &ask, &ask_tag);
    assert_eq!(signed.status, 200, "{}\n{}", signed.head, signed.body);
    let shown = noor.browser.get(&proposal_page(&page_two));
    assert_eq!(shown.status, 200, "{}", shown.body);
    assert!(
        shown.body.contains(
            "The rules of this wiki ask for 5 approvals from people other than the proposer, and not enough people can give them yet. No proposal can be accepted until more people hold the role that reviews."
        ),
        "{}",
        shown.body
    );
    let japanese = noor
        .browser
        .visit(
            "GET",
            &proposal_page(&page_two),
            None,
            &[("Accept-Language", "ja")],
        )
        .body;
    assert!(
        japanese.contains("提案者以外の人からの承認が 5 件必要ですが"),
        "{japanese}"
    );
    // No proposal can be accepted now: the policy is asked, the approvals are not there.
    let held = accept(&maya_accept, &page_two, &page_two_tag);
    assert_eq!(held.status, 403, "{}\n{}", held.head, held.body);
    assert_eq!(
        json_field(&held.body, "reason").as_deref(),
        Some("approvals-missing"),
        "{}",
        held.body
    );

    // An admin's token (the admin scope) accepts a change back, by the hatch: no approvals.
    let (back, back_tag) = open(
        &noor_propose,
        &file_body(
            "Back to the defaults",
            "governance/policy.yaml",
            "acceptance:\n  approvals: 0\n",
        ),
    );
    let by_hatch = accept(&ada_accept, &back, &back_tag);
    assert_eq!(by_hatch.status, 200, "{}\n{}", by_hatch.head, by_hatch.body);
    // The wiki can be used again by a maintainer; the warning is gone from the page.
    let freed = accept(&maya_accept, &page_two, &page_two_tag);
    assert_eq!(freed.status, 200, "{}\n{}", freed.head, freed.body);
    running.stop();
}
