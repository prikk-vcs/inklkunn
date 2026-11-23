//! What the moderator typed to name a person is in the trail (RFC 006 decision 7; handoff G-T1 correction F-5): a person who
//! asks of the moderating acts whether an account exists leaves the question in the trail, **whether or not the account is
//! there**, so the probing costs an audited decision.

use inklkunn_store::{AuditEvent, AuditOutcome, Role};

use super::support::{KINDS, World};
use crate::people::PeopleError;

fn decisions(w: &World, action: &str) -> Vec<AuditEvent> {
    w.audit
        .events()
        .into_iter()
        .filter(|e| e.action == action && e.outcome == AuditOutcome::Allowed)
        .collect()
}

#[test]
fn a_restriction_asked_of_a_name_that_is_no_account_leaves_the_typed_name_in_the_moderate_decision()
{
    for kind in KINDS {
        let w = World::new(kind);
        let maintainer = w.person("Maya", Some(Role::Maintainer));
        let result =
            w.people
                .restrict(&maintainer.principal, &w.wiki, "  ghost-writer ", 3, "Why.");
        assert_eq!(result.unwrap_err(), PeopleError::NoSuchAccount);
        let [decision] = decisions(&w, "moderate").try_into().expect("one decision");
        assert!(
            decision.detail.ends_with("; typed=ghost-writer"),
            "{decision:?}"
        );
        assert!(decision.detail.starts_with("rung=maintainer; policy="));
    }
}

#[test]
fn a_name_that_is_an_account_is_in_the_decision_too_and_a_grant_by_an_admin_keeps_it_in_the_admin_decision()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let sam = w.person("Sam", None);
        let typed = sam.id.to_string();
        w.people
            .grant(
                &admin.principal,
                &w.wiki,
                &typed,
                Some(Role::Contributor),
                "",
            )
            .unwrap();
        let [decision] = decisions(&w, "admin").try_into().expect("one decision");
        assert!(
            decision.detail.ends_with(&format!("; typed={typed}")),
            "{decision:?}"
        );
    }
}

#[test]
fn the_typed_name_is_bounded_and_a_control_or_bidirectional_character_in_it_is_shown_as_a_question_mark()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let maintainer = w.person("Maya", Some(Role::Maintainer));
        let typed = format!("a\u{202e}b\u{0}c{}", "z".repeat(500));
        let _ = w
            .people
            .restrict(&maintainer.principal, &w.wiki, &typed, 3, "Why.");
        let [decision] = decisions(&w, "moderate").try_into().expect("one decision");
        let kept = decision
            .detail
            .split_once("; typed=")
            .expect("the typed name")
            .1;
        assert!(kept.starts_with("a?b?c"), "{kept:?}");
        assert!(kept.chars().count() <= 100, "{}", kept.chars().count());
    }
}
