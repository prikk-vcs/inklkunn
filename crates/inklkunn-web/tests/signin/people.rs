//! The account page and the people of the wikis (RFC 006 D-6, D-7; handoff G-T2): the page tells a person which role they hold
//! in each wiki they can read and what gives it to them, and a restriction with its reason and its end, in words and never as
//! rules; it names no wiki the person cannot read and no one else's standing.

use inklkunn_store::{
    Actor, DisplayName, GovernanceStore, MeasureEvent, NewWiki, RestrictionEvent,
    RestrictionEventId, Role, RoleGrant, RoleStore, Slug, Visibility, WikiStore,
};

use crate::common::NOW;
use crate::harness::*;

#[tokio::test]
async fn the_account_page_tells_the_role_in_each_readable_wiki_and_why_and_names_no_wiki_it_may_not_read()
 {
    let r = real(config());
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let s = r.sign_in("alice").await;
    // A second wiki that is restricted, which alice holds no role in, and a third where she is an observer.
    let secret =
        r.w.store
            .create(NewWiki {
                visibility: Visibility::Restricted,
                ..NewWiki::new(
                    Slug::new("secret").unwrap(),
                    DisplayName::new("The Secret Place").unwrap(),
                )
            })
            .unwrap();
    let _ = secret;
    let internal =
        r.w.store
            .create(NewWiki {
                visibility: Visibility::Internal,
                ..NewWiki::new(
                    Slug::new("lodge").unwrap(),
                    DisplayName::new("The Lodge").unwrap(),
                )
            })
            .unwrap();
    r.w.store
        .grant(RoleGrant {
            account: alice.clone(),
            wiki: internal.id.clone(),
            role: Role::Observer,
            granted_by: None,
            granted_at: NOW,
        })
        .unwrap();
    let a = r.get("/-/account", &[("cookie", &s.cookie)]).await;
    assert_eq!(a.status, 200);
    let body = a.body();
    assert!(body.contains("Your role in each wiki"), "{body}");
    assert!(
        body.contains("<a href=\"/garden\">The Garden</a>"),
        "{body}"
    );
    assert!(
        body.contains("You hold the role contributor in this wiki, because it was granted to you."),
        "{body}"
    );
    assert!(body.contains("<a href=\"/lodge\">The Lodge</a>"), "{body}");
    assert!(
        body.contains("You hold the role observer in this wiki, because it was granted to you."),
        "{body}"
    );
    assert!(
        !body.contains("The Secret Place") && !body.contains("/secret"),
        "a wiki she may not read is not named: {body}"
    );
    assert!(!body.contains("You are restricted"), "{body}");
    // Without a grant, in the Japanese a reader chose.
    let bob = r.person("bob", "Bob", None);
    let _ = bob;
    let sb = r.sign_in("bob").await;
    let a = r
        .get(
            "/-/account",
            &[("cookie", &format!("{}; inklkunn_lang=ja", sb.cookie))],
        )
        .await;
    assert!(
        a.body()
            .contains("あなたはこのウィキで役割を持っていません。読むことはできますが、それ以上のことはできません。"),
        "{}",
        a.body()
    );
    assert!(
        a.body().contains("各ウィキでのあなたの役割"),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_restriction_is_told_on_the_account_page_to_the_restricted_person_alone() {
    let r = real(config());
    let admin = r.person("ada", "Ada", Some(Role::Admin));
    let alice = r.person("alice", "Alice", Some(Role::Contributor));
    let bob = r.person("bob", "Bob", Some(Role::Contributor));
    let _ = bob;
    r.w.store
        .record_restriction_event(&RestrictionEvent {
            id: RestrictionEventId::new(),
            wiki: r.w.wiki.id.clone(),
            at: NOW - 60,
            event: MeasureEvent::Set,
            account: alice.clone(),
            by: Actor {
                account: admin.clone(),
                rung: Role::Admin,
            },
            policy_block: None,
            reason: Some("pasting the same link into every page".to_owned()),
            expires_at: Some(NOW + 2 * 86_400),
            of_event: None,
            appeal: None,
        })
        .unwrap();
    let sa = r.sign_in("alice").await;
    let a = r.get("/-/account", &[("cookie", &sa.cookie)]).await;
    let body = a.body();
    for want in [
        "You hold the role contributor in this wiki, because it was granted to you.",
        "You are restricted in this wiki: you can read it, and you cannot comment, propose or review until the restriction ends.",
        "The reason given: pasting the same link into every page",
        "The restriction ends on 2023-11-16 22:13 UTC.",
    ] {
        assert!(body.contains(want), "{want}: {body}");
    }
    // Someone else's page says nothing of it.
    let sb = r.sign_in("bob").await;
    let body = r.get("/-/account", &[("cookie", &sb.cookie)]).await.body();
    assert!(!body.contains("pasting the same link"), "{body}");
    assert!(!body.contains("You are restricted"), "{body}");
    // Nor does an admin's, who may restrict: the account page is the person's own standing.
    let sd = r.sign_in("ada").await;
    let body = r.get("/-/account", &[("cookie", &sd.cookie)]).await.body();
    assert!(!body.contains("pasting the same link"), "{body}");
}
