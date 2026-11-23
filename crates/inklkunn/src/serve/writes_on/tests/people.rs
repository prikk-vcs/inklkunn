#![allow(clippy::expect_used, clippy::unwrap_used)]
//! RFC 006 D-6 and D-7, end to end (handoff G-T2): **the product's own wiring**. The real server (the binary's `prepare`)
//! reads a restriction in force into the authorizer of every write, so an account an admin restricted is refused at the write
//! API with the reason and the end, an account that is not is not, and the people page's records are the service's own store.

use inklkunn_core::Principal;

use super::api::{API, api, json_field, json_quote};
use super::*;

#[test]
fn a_restriction_set_through_the_product_refuses_the_restricted_accounts_writes_with_the_reason_and_the_end()
 {
    let Some(world) = World::new("people") else {
        return;
    };
    let running = Running::start(&world);
    let _noor = join(
        &world,
        &running,
        "noor",
        "Noor Contributor",
        Role::Contributor,
    );
    let _maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let _ada = join(&world, &running, "ada", "Ada Admin", Role::Admin);
    let auth = running.services.auth.clone().expect("sign-in is served");
    let token = |login: &str, scope: Scope| -> String {
        auth.issue_token(&world.account_of(login), scope, "people", 3600, now())
            .unwrap()
            .secret
            .as_str()
            .to_owned()
    };
    let (noor, maya) = (token("noor", Scope::Propose), token("maya", Scope::Propose));
    let raw = || -> String {
        let info = api(
            &running,
            "GET",
            &format!("{API}/raw/Help"),
            None,
            None,
            None,
        );
        assert_eq!(info.status, 200, "{}", info.body);
        format!("\"{}\"", json_field(&info.body, "state_id").expect("a tip"))
    };
    // The text of `Help` as it is, with a line more.
    let body = |summary: &str| {
        let source = wire(
            running.port,
            "GET",
            "/handbook/-/raw/Help",
            &[("Host".into(), format!("localhost:{}", running.content_port))],
            &[],
        )
        .body;
        format!(
            "{{\"summary\":{},\"submit\":true,\"files\":[{{\"path\":\"pages/Help.md\",\"text\":{}}}]}}",
            json_quote(summary),
            json_quote(&format!("{source}\nA line more, {summary}.\n"))
        )
    };
    let create = format!("{API}/proposals");
    let ask = |who: &str, summary: &str| {
        api(
            &running,
            "POST",
            &create,
            Some(who),
            Some(&raw()),
            Some(&body(summary)),
        )
    };
    assert_eq!(ask(&noor, "Before").status, 201, "before the restriction");

    // An admin restricts the contributor through the service the people page uses, as an admin's session would.
    let authority = IdentityAuthority::for_tests();
    let admin = Principal::session(&authority, world.account_of("ada"));
    let wiki = world.wiki.clone();
    let made = running
        .services
        .people
        .restrict(
            &admin,
            &wiki,
            &world.account_of("noor").to_string(),
            2,
            "posting the same link everywhere",
        )
        .expect("an admin restricts a contributor");
    let refused = ask(&noor, "After");
    assert_eq!(refused.status, 403, "{}\n{}", refused.head, refused.body);
    assert_eq!(
        json_field(&refused.body, "reason").as_deref(),
        Some("restricted"),
        "{}",
        refused.body
    );
    assert_eq!(
        json_field(&refused.body, "restriction_reason").as_deref(),
        Some("posting the same link everywhere")
    );
    assert_eq!(
        json_field(&refused.body, "until"),
        made.until.map(|u| u.to_string())
    );
    // The maintainer is not restricted, and the restricted person still reads.
    assert_eq!(ask(&maya, "By someone else").status, 201);
    let read = api(
        &running,
        "GET",
        &format!("{API}/raw/Help"),
        Some(&noor),
        None,
        None,
    );
    assert_eq!(read.status, 200, "reading is never refused: {}", read.body);
}
