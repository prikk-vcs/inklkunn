//! A grant or a revocation through the real service (RFC 006 D-6; handoff G-T2; acceptance criterion 6): who may, within what
//! reach, the record it writes, the audit it leaves, and what a refusal changes (nothing).

use inklkunn_store::{Role, Visibility};

use super::support::{BLOCK, KINDS, Person, T0, World, set_now};
use crate::authorize::Principal;
use crate::governance::{GrantUpTo, PlatformLimits};
use crate::people::{GrantRefusal, PeopleError, ReasonFault};

fn up_to(word: &str) -> String {
    format!("rungs:\n  grant_up_to: {word}\n")
}

#[test]
fn an_admin_grants_and_revokes_and_each_is_a_recorded_decision_naming_who_acted_their_rung_the_block_and_the_reason()
 {
    for kind in KINDS {
        let w = World::under(kind, &up_to("none"));
        let admin = w.person("Ada", Some(Role::Admin));
        let someone = w.person("Sam", None);
        set_now(T0 + 5);
        let made = w
            .people
            .grant(
                &admin.principal,
                &w.wiki,
                &someone.id.to_string(),
                Some(Role::Contributor),
                "  has sent three good edits ",
            )
            .expect("an admin grants");
        assert_eq!((made.before, made.after), (None, Some(Role::Contributor)));
        assert_eq!(
            w.grant_of(&someone),
            Some(Role::Contributor),
            "applied in the same step"
        );
        let decisions = w
            .store
            .grant_decisions(&w.wiki.id, Some(&someone.id), 10)
            .unwrap();
        let [d] = decisions.as_slice() else {
            panic!("one decision: {decisions:?}")
        };
        assert_eq!(d.id, made.id);
        let by = d.by.as_ref().expect("an account acted");
        assert_eq!((&by.account, by.rung), (&admin.id, Role::Admin));
        assert_eq!(
            d.policy_block.as_deref(),
            Some(BLOCK),
            "the rules it was decided under"
        );
        assert_eq!(
            d.reason.as_deref(),
            Some("has sent three good edits"),
            "the reason, trimmed"
        );
        assert_eq!(
            (d.before, d.after, d.at),
            (None, Some(Role::Contributor), T0 + 5)
        );

        // A revocation is a decision whose rung after is none.
        let revoked = w
            .people
            .grant(&admin.principal, &w.wiki, &someone.id.to_string(), None, "")
            .expect("an admin revokes");
        assert_eq!(
            (revoked.before, revoked.after),
            (Some(Role::Contributor), None)
        );
        assert_eq!(w.grant_of(&someone), None);
        let all = w
            .store
            .grant_decisions(&w.wiki.id, Some(&someone.id), 10)
            .unwrap();
        assert_eq!(all.len(), 2, "{kind:?}");
        assert_eq!(
            all.first().and_then(|d| d.reason.clone()),
            None,
            "no reason: none kept"
        );
    }
}

#[test]
fn a_person_is_found_by_account_id_or_by_sign_in_name_and_an_unknown_one_is_one_answer() {
    use inklkunn_store::{Account, AccountId, DisplayName, LocalCredential, LoginName};
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let id = AccountId::new();
        w.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new("Lena").unwrap(),
                local: Some(LocalCredential {
                    login: LoginName::parse("lena.k").unwrap(),
                    password_hash: "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNoaGFzaGhhc2g".to_owned(),
                }),
                created_at: 0,
                disabled: false,
            })
            .unwrap();
        let by_login = w
            .people
            .grant(
                &admin.principal,
                &w.wiki,
                " Lena.K ",
                Some(Role::Observer),
                "",
            )
            .expect("by sign-in name, folded");
        assert_eq!(by_login.account, id);
        assert_eq!(
            w.people.grant(
                &admin.principal,
                &w.wiki,
                "no-such-person",
                Some(Role::Observer),
                ""
            ),
            Err(PeopleError::NoSuchAccount)
        );
        assert_eq!(
            w.people.grant(
                &admin.principal,
                &w.wiki,
                &AccountId::new().to_string(),
                Some(Role::Observer),
                ""
            ),
            Err(PeopleError::NoSuchAccount)
        );
        assert_eq!(
            w.people
                .grant(&admin.principal, &w.wiki, "!!", Some(Role::Observer), ""),
            Err(PeopleError::NoSuchAccount),
            "something that cannot be a name is the same answer"
        );
    }
}

/// The reach table through the real service: every `grant_up_to` and every pair of rungs (the rung the account holds, the
/// rung it would hold), asked of a maintainer, as the rule says.
#[test]
fn a_maintainer_changes_a_rung_only_when_the_rung_held_and_the_rung_given_are_both_within_grant_up_to()
 {
    let rungs: Vec<Option<Role>> = std::iter::once(None)
        .chain(Role::ALL.into_iter().map(Some))
        .collect();
    for kind in KINDS {
        for word in GrantUpTo::ALL {
            let w = World::under(kind, &up_to(word.word()));
            let maintainer = w.person("Mia", Some(Role::Maintainer));
            // A second admin, so that no change is refused as the last admin's.
            let _ = w.person("Ada", Some(Role::Admin));
            let _ = w.person("Bo", Some(Role::Admin));
            for current in &rungs {
                for new in &rungs {
                    let target = w.person("Target", *current);
                    let got = w.people.grant(
                        &maintainer.principal,
                        &w.wiki,
                        &target.id.to_string(),
                        *new,
                        "",
                    );
                    let within = |r: &Option<Role>| r.is_none_or(|r| word.maintainer_may_grant(r));
                    let expected_ok = word.highest().is_some()
                        && current != new
                        && within(current)
                        && within(new);
                    assert_eq!(
                        got.is_ok(),
                        expected_ok,
                        "{kind:?} reach {word:?}: {current:?} -> {new:?}: {got:?}"
                    );
                    let held_after = if expected_ok { *new } else { *current };
                    assert_eq!(w.grant_of(&target), held_after, "a refusal changes nothing");
                }
            }
        }
    }
}

#[test]
fn a_maintainer_in_a_wiki_whose_rules_let_maintainers_grant_none_is_not_allowed_and_nothing_is_recorded()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let someone = w.person("Sam", None);
        assert_eq!(
            w.people.grant(
                &maintainer.principal,
                &w.wiki,
                &someone.id.to_string(),
                Some(Role::Observer),
                ""
            ),
            Err(PeopleError::NotAllowed)
        );
        assert_eq!(w.grant_of(&someone), None);
        assert!(
            w.store
                .grant_decisions(&w.wiki.id, None, 10)
                .unwrap()
                .is_empty()
        );
        assert!(
            w.audit.events().iter().all(|e| e.action != "moderate"),
            "a person who could never grant is told so before an act is decided"
        );
    }
}

#[test]
fn nobody_below_a_maintainer_an_outsider_an_anonymous_visitor_or_a_narrow_token_grants() {
    for kind in KINDS {
        let w = World::under(kind, &up_to("reviewer"));
        let someone = w.person("Sam", None);
        let who = someone.id.to_string();
        for role in [
            Role::Observer,
            Role::Questioner,
            Role::Contributor,
            Role::Reviewer,
        ] {
            let p = w.person("P", Some(role));
            assert_eq!(
                w.people
                    .grant(&p.principal, &w.wiki, &who, Some(Role::Observer), ""),
                Err(PeopleError::NotAllowed),
                "{role:?}"
            );
        }
        let outsider = w.person("Out", None);
        assert_eq!(
            w.people
                .grant(&outsider.principal, &w.wiki, &who, Some(Role::Observer), ""),
            Err(PeopleError::NotAllowed)
        );
        assert_eq!(
            w.people.grant(
                &Principal::Anonymous,
                &w.wiki,
                &who,
                Some(Role::Observer),
                ""
            ),
            Err(PeopleError::NotAllowed)
        );
        // The credential's scope still intersects: an admin's token that cannot administer cannot grant.
        let admin = w.person("Ada", Some(Role::Admin));
        for scope in [
            inklkunn_store::Scope::Read,
            inklkunn_store::Scope::Propose,
            inklkunn_store::Scope::Review,
            inklkunn_store::Scope::Accept,
        ] {
            let token = Principal::token(&w.authority, admin.id.clone(), scope);
            assert_eq!(
                w.people
                    .grant(&token, &w.wiki, &who, Some(Role::Observer), ""),
                Err(PeopleError::NotAllowed),
                "a token of scope {scope:?}"
            );
        }
        assert_eq!(w.grant_of(&someone), None);
    }
}

#[test]
fn nobody_changes_their_own_rung() {
    for kind in KINDS {
        let w = World::under(kind, &up_to("reviewer"));
        let admin = w.person("Ada", Some(Role::Admin));
        let _second = w.person("Bo", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        for (who, new) in [
            (&admin, Some(Role::Maintainer)),
            (&maintainer, Some(Role::Observer)),
            (&maintainer, None),
        ] {
            assert_eq!(
                w.people
                    .grant(&who.principal, &w.wiki, &who.id.to_string(), new, ""),
                Err(PeopleError::Grant(GrantRefusal::OwnRung))
            );
        }
        assert_eq!(w.grant_of(&admin), Some(Role::Admin));
        assert_eq!(w.grant_of(&maintainer), Some(Role::Maintainer));
    }
}

#[test]
fn the_only_admin_cannot_demote_or_revoke_themself_so_a_wiki_is_never_left_with_none_and_one_admin_may_demote_another()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        // The stranding that can happen is a sole admin changing their own rung: refused, and nothing is recorded.
        for new in [None, Some(Role::Maintainer)] {
            assert_eq!(
                w.people
                    .grant(&admin.principal, &w.wiki, &admin.id.to_string(), new, ""),
                Err(PeopleError::Grant(GrantRefusal::OwnRung))
            );
        }
        assert_eq!(w.grant_of(&admin), Some(Role::Admin));
        assert!(
            w.store
                .grant_decisions(&w.wiki.id, None, 10)
                .unwrap()
                .is_empty()
        );
        // A second admin: either may demote the other, and the one who demotes stays an admin.
        let other = w.person("Bo", Some(Role::Admin));
        w.people
            .grant(
                &admin.principal,
                &w.wiki,
                &other.id.to_string(),
                Some(Role::Maintainer),
                "",
            )
            .expect("two admins: one goes");
        assert_eq!(w.grant_of(&other), Some(Role::Maintainer));
        assert_eq!(w.grant_of(&admin), Some(Role::Admin));
        // A demoted admin is not an admin to demote the remaining one.
        assert_eq!(
            w.people.grant(
                &other.principal,
                &w.wiki,
                &admin.id.to_string(),
                Some(Role::Observer),
                ""
            ),
            Err(PeopleError::NotAllowed)
        );
    }
}

#[test]
fn a_disabled_admin_is_demoted_or_revoked_like_any_other_and_the_admin_who_acts_stays() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let gone = w.person("Bo", Some(Role::Admin));
        w.disable(&gone);
        w.people
            .grant(&admin.principal, &w.wiki, &gone.id.to_string(), None, "")
            .expect("the admin who can sign in is the one who is left");
        assert_eq!(w.grant_of(&gone), None);
        assert_eq!(w.grant_of(&admin), Some(Role::Admin));
    }
}

#[test]
fn a_reason_that_is_too_long_or_not_one_line_refuses_the_grant_and_changes_nothing() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let someone = w.person("Sam", None);
        let who = someone.id.to_string();
        let long = "x".repeat(crate::people::MAX_REASON_CHARS + 1);
        assert_eq!(
            w.people
                .grant(&admin.principal, &w.wiki, &who, Some(Role::Observer), &long),
            Err(PeopleError::Reason(ReasonFault::TooLong))
        );
        assert_eq!(
            w.people.grant(
                &admin.principal,
                &w.wiki,
                &who,
                Some(Role::Observer),
                "a\nb"
            ),
            Err(PeopleError::Reason(ReasonFault::NotOneLine))
        );
        assert_eq!(w.grant_of(&someone), None);
        assert!(
            w.store
                .grant_decisions(&w.wiki.id, None, 10)
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn each_grant_is_an_audited_act_naming_the_rung_the_block_and_what_changed_and_it_is_recorded_before_the_record()
 {
    for kind in KINDS {
        let w = World::under(kind, &up_to("reviewer"));
        let admin = w.person("Ada", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let a = w.person("Sam", None);
        let b = w.person("Kit", Some(Role::Observer));
        w.people
            .grant(
                &admin.principal,
                &w.wiki,
                &a.id.to_string(),
                Some(Role::Reviewer),
                "",
            )
            .unwrap();
        w.people
            .grant(&maintainer.principal, &w.wiki, &b.id.to_string(), None, "")
            .unwrap();
        let events = w.audit.events();
        let short = &BLOCK[..12];
        // An admin's grant is the `admin` decision, a maintainer's the `moderate` one; each names rung and block; then the act.
        let find = |action: &str, principal: &Person| {
            events
                .iter()
                .find(|e| e.action == action && e.principal == principal.principal.audit_name())
                .unwrap_or_else(|| panic!("no `{action}` of {}: {events:#?}", principal.id))
                .clone()
        };
        let decision = find("admin", &admin);
        assert_eq!(decision.detail, format!("rung=admin; policy={short}"));
        assert_eq!(decision.outcome, inklkunn_store::AuditOutcome::Allowed);
        let act = find("grant", &admin);
        assert_eq!(act.resource, format!("member:{}:{}", w.wiki.id, a.id));
        assert_eq!(
            act.detail,
            format!("rung=admin; policy={short}; from=none; to=reviewer")
        );
        let decision = find("moderate", &maintainer);
        assert_eq!(decision.detail, format!("rung=maintainer; policy={short}"));
        let act = find("revoke", &maintainer);
        assert_eq!(
            act.detail,
            format!("rung=maintainer; policy={short}; from=observer; to=none")
        );
        // The audit never holds the reason.
        assert!(
            events.iter().all(|e| !e.detail.contains("reason")),
            "{kind:?}"
        );
    }
}

#[test]
fn a_wiki_that_cannot_be_seen_is_not_found_for_the_one_who_may_not_see_it_and_a_wiki_whose_rules_cannot_be_read_refuses()
 {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Restricted,
            None,
            PlatformLimits::default(),
        );
        let stranger = w.person("Sue", None);
        let admin = w.person("Ada", Some(Role::Admin));
        let someone = w.person("Sam", None);
        let who = someone.id.to_string();
        // One answer for a wiki that is not there and a restricted one the person is no member of.
        assert_eq!(
            w.people
                .grant(&stranger.principal, &w.wiki, &who, Some(Role::Observer), ""),
            Err(PeopleError::NotFound)
        );
        assert_eq!(
            w.people.grant(
                &Principal::Anonymous,
                &w.wiki,
                &who,
                Some(Role::Observer),
                ""
            ),
            Err(PeopleError::NotFound)
        );
        let mut absent = w.wiki.clone();
        absent.id = inklkunn_store::WikiId::new();
        assert_eq!(
            w.people
                .grant(&stranger.principal, &absent, &who, Some(Role::Observer), ""),
            Err(PeopleError::NotFound),
            "the same answer"
        );
        // A member who is an admin may.
        assert!(
            w.people
                .grant(&admin.principal, &w.wiki, &who, Some(Role::Observer), "")
                .is_ok()
        );
    }
}
