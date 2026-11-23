//! A restriction through the real service and the real authorizer (RFC 006 D-7; handoff G-T2; acceptance criterion 6; threat
//! model C-3g, C-14h): who sets one, what it caps, that it **ends by the clock** (the instant of its end and the second
//! before), that renewing extends it and lifting ends it early, and what the restricted person is told.

use inklkunn_store::{AuditOutcome, MeasureEvent, RestrictionEvent, Role, Visibility};

use super::support::{BLOCK, DAY, KINDS, Person, T0, World, set_now};
use crate::authorize::{Action, Principal, Resource};
use crate::governance::PlatformLimits;
use crate::people::{
    PeopleError, ReasonFault, RestrictRefusal, RestrictionOutcome, check_restrict,
};
use crate::proposal::{ProposalEdit, ProposalError};

fn propose(w: &World, who: &Person) -> Result<inklkunn_store::ProposalRecord, ProposalError> {
    w.proposals.create(
        &who.principal,
        &w.wiki,
        None,
        ProposalEdit {
            summary: "A small thing".to_owned(),
            files: vec![inklkunn_store::FileChange {
                path: "pages/Small.md".to_owned(),
                text: Some("text".to_owned()),
            }],
            ..ProposalEdit::default()
        },
    )
}

fn events(w: &World, who: &Person) -> Vec<RestrictionEvent> {
    w.store.restriction_events(&w.wiki.id, &who.id).unwrap()
}

#[test]
fn a_restriction_caps_a_person_at_observer_so_they_read_and_do_nothing_else_and_an_admin_is_never_capped()
 {
    for kind in KINDS {
        for held in [
            Role::Observer,
            Role::Questioner,
            Role::Contributor,
            Role::Reviewer,
            Role::Maintainer,
        ] {
            let w = World::new(kind);
            let admin = w.person("Ada", Some(Role::Admin));
            let person = w.person("Pat", Some(held));
            // Before: what the ladder says.
            let allowed = |w: &World| -> Vec<Action> {
                Action::ALL
                    .into_iter()
                    .filter(|a| w.may(&person, *a))
                    .collect()
            };
            let before = allowed(&w);
            assert!(before.contains(&Action::Read));
            let ladder = |role: Role| -> Vec<Action> {
                Action::ALL
                    .into_iter()
                    .filter(|a| role >= a.minimum_role())
                    .collect()
            };
            assert_eq!(before, ladder(held), "{kind:?} {held:?}");
            w.people
                .restrict(&admin.principal, &w.wiki, &person.id.to_string(), 7, "spam")
                .expect("restricted");
            // After: reading, and nothing else.
            assert_eq!(allowed(&w), vec![Action::Read], "{kind:?} {held:?}");
        }
    }
}

#[test]
fn an_admin_is_not_held_down_by_a_restriction_it_has_on_record_so_the_rules_can_always_be_changed()
{
    for kind in KINDS {
        let w = World::new(kind);
        let moderator = w.person("Mia", Some(Role::Maintainer));
        let admin = w.person("Ada", Some(Role::Admin));
        // A restriction recorded on an account that was then made an admin (nobody can set one on an admin).
        w.store
            .record_restriction_event(&RestrictionEvent {
                id: inklkunn_store::RestrictionEventId::new(),
                wiki: w.wiki.id.clone(),
                at: T0,
                event: MeasureEvent::Set,
                account: admin.id.clone(),
                by: inklkunn_store::Actor {
                    account: moderator.id.clone(),
                    rung: Role::Maintainer,
                },
                policy_block: None,
                reason: Some("before they were an admin".to_owned()),
                expires_at: Some(T0 + DAY),
                of_event: None,
                appeal: None,
            })
            .unwrap();
        for action in Action::ALL {
            assert!(
                w.may(&admin, action),
                "{kind:?}: an admin may {}",
                action.name()
            );
        }
        let held = w.authorizer.holding(&admin.principal, &w.wiki.id).unwrap();
        assert_eq!(held.restriction, None, "an admin is told of no restriction");
        assert_eq!(held.rung, Some(Role::Admin));
    }
}

#[test]
fn a_restriction_ends_by_the_clock_at_its_end_and_not_a_second_before_and_nothing_is_written_for_it_to_end()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        let can_propose = |w: &World| w.may(&person, Action::Propose);
        assert!(can_propose(&w));
        let made = w
            .people
            .restrict(&admin.principal, &w.wiki, &person.id.to_string(), 1, "spam")
            .unwrap();
        assert_eq!(made.outcome, RestrictionOutcome::Set);
        let end = T0 + DAY;
        assert_eq!(made.until, Some(end));
        assert!(!can_propose(&w), "it applies from the second it is set");
        set_now(end - 1);
        assert!(
            !can_propose(&w),
            "{kind:?}: the second before its end it still applies"
        );
        set_now(end);
        assert!(can_propose(&w), "{kind:?}: at its end it no longer applies");
        set_now(end + 1);
        assert!(can_propose(&w));
        assert_eq!(
            events(&w, &person).len(),
            1,
            "the clock ended it: nothing was written"
        );
        // And it applies to a person who is asked about only for what it refuses: reading was never refused.
        set_now(T0 + 1);
        assert!(w.may(&person, Action::Read));
    }
}

#[test]
fn a_renewal_replaces_the_restriction_and_ends_from_now_and_a_lifting_ends_it_early() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        let who = person.id.to_string();
        w.people
            .restrict(&admin.principal, &w.wiki, &who, 1, "first")
            .unwrap();
        set_now(T0 + DAY / 2);
        let renewed = w
            .people
            .renew(&admin.principal, &w.wiki, &who, 3, "second")
            .expect("renewed");
        assert_eq!(renewed.outcome, RestrictionOutcome::Renewed);
        assert_eq!(
            renewed.until,
            Some(T0 + DAY / 2 + 3 * DAY),
            "counted from now"
        );
        set_now(T0 + DAY + 1);
        assert!(
            !w.may(&person, Action::Propose),
            "{kind:?}: the first would have ended; the renewal holds"
        );
        let notice = w
            .authorizer
            .holding(&person.principal, &w.wiki.id)
            .unwrap()
            .restriction
            .expect("restricted");
        assert_eq!(
            notice.reason, "second",
            "the renewal's reason is the one in force"
        );
        set_now(T0 + DAY / 2 + 3 * DAY - 1);
        assert!(!w.may(&person, Action::Propose));
        set_now(T0 + DAY / 2 + 3 * DAY);
        assert!(
            w.may(&person, Action::Propose),
            "the renewal ends at its end"
        );

        // Lifting early.
        set_now(T0 + 10 * DAY);
        w.people
            .restrict(&admin.principal, &w.wiki, &who, 5, "again")
            .expect("a new restriction after the old one ended");
        assert!(!w.may(&person, Action::Propose));
        let lifted = w
            .people
            .lift(&admin.principal, &w.wiki, &who, "  it was a mistake ")
            .expect("lifted");
        assert_eq!(lifted.outcome, RestrictionOutcome::Lifted);
        assert!(w.may(&person, Action::Propose), "lifted early: it ends now");
        let chain = events(&w, &person);
        let kinds: Vec<MeasureEvent> = chain.iter().map(|e| e.event).collect();
        assert_eq!(
            kinds,
            [
                MeasureEvent::Set,
                MeasureEvent::Renewed,
                MeasureEvent::Set,
                MeasureEvent::Lifted
            ],
            "{kind:?}"
        );
        let [set, renewed, set_again, lifted] = chain.as_slice() else {
            panic!("four events: {chain:?}")
        };
        assert_eq!(
            renewed.of_event.as_ref(),
            Some(&set.id),
            "a renewal names what it follows"
        );
        assert_eq!(lifted.of_event.as_ref(), Some(&set_again.id));
        assert_eq!(lifted.reason.as_deref(), Some("it was a mistake"));
        assert_eq!(lifted.expires_at, None);
    }
}

#[test]
fn restricting_twice_renewing_nothing_and_lifting_nothing_are_each_refused_by_name() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        let who = person.id.to_string();
        let not_restricted = Err(PeopleError::Restrict(RestrictRefusal::NotRestricted));
        assert_eq!(
            w.people
                .renew(&admin.principal, &w.wiki, &who, 1, "r")
                .map(|_| ()),
            not_restricted
        );
        assert_eq!(
            w.people
                .lift(&admin.principal, &w.wiki, &who, "")
                .map(|_| ()),
            not_restricted
        );
        w.people
            .restrict(&admin.principal, &w.wiki, &who, 2, "r")
            .unwrap();
        assert_eq!(
            w.people
                .restrict(&admin.principal, &w.wiki, &who, 2, "r")
                .map(|_| ()),
            Err(PeopleError::Restrict(RestrictRefusal::AlreadyRestricted {
                until: T0 + 2 * DAY
            })),
            "renew it instead"
        );
        // Once it has ended there is nothing to renew or lift, and a new one is a new restriction.
        set_now(T0 + 2 * DAY);
        assert_eq!(
            w.people
                .renew(&admin.principal, &w.wiki, &who, 1, "r")
                .map(|_| ()),
            not_restricted
        );
        assert_eq!(
            w.people
                .restrict(&admin.principal, &w.wiki, &who, 1, "r")
                .unwrap()
                .outcome,
            RestrictionOutcome::Set
        );
    }
}

#[test]
fn a_restriction_is_at_most_the_operators_limit_and_the_refusal_names_it() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        let who = person.id.to_string();
        for days in [0, 91, 365, u32::MAX] {
            assert_eq!(
                w.people
                    .restrict(&admin.principal, &w.wiki, &who, days, "r")
                    .map(|_| ()),
                Err(PeopleError::Restrict(RestrictRefusal::DaysOutOfRange {
                    limit_days: 90
                })),
                "{kind:?} {days}"
            );
        }
        let error = w
            .people
            .restrict(&admin.principal, &w.wiki, &who, 91, "r")
            .unwrap_err();
        assert!(
            error.to_string().contains("INKLKUNN_GOVERNANCE_LIMIT_DAYS"),
            "the refusal names the operator's limit: {error}"
        );
        assert!(events(&w, &person).is_empty(), "nothing was recorded");
        w.people
            .restrict(&admin.principal, &w.wiki, &who, 90, "r")
            .expect("90 days is the limit");

        // The operator's own limit, lower.
        let tight = World::with(
            kind,
            Visibility::Public,
            None,
            PlatformLimits {
                limit_days: 30,
                ..PlatformLimits::default()
            },
        );
        let admin = tight.person("Ada", Some(Role::Admin));
        let person = tight.person("Pat", Some(Role::Contributor));
        assert_eq!(
            tight
                .people
                .restrict(
                    &admin.principal,
                    &tight.wiki,
                    &person.id.to_string(),
                    31,
                    "r"
                )
                .map(|_| ()),
            Err(PeopleError::Restrict(RestrictRefusal::DaysOutOfRange {
                limit_days: 30
            }))
        );
        assert!(
            tight
                .people
                .restrict(
                    &admin.principal,
                    &tight.wiki,
                    &person.id.to_string(),
                    30,
                    "r"
                )
                .is_ok()
        );
    }
}

#[test]
fn a_restriction_has_a_reason_that_is_one_bounded_line() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        let who = person.id.to_string();
        let long = "x".repeat(crate::people::MAX_REASON_CHARS + 1);
        for (reason, fault) in [
            ("", ReasonFault::Missing),
            ("   ", ReasonFault::Missing),
            (long.as_str(), ReasonFault::TooLong),
            ("two\nlines", ReasonFault::NotOneLine),
            // One rule for every reason (F-5): a protection's reason refuses these, and so does a restriction's.
            ("looks\u{202e}reversed", ReasonFault::NotOneLine),
            ("hidden\u{2066}isolate", ReasonFault::NotOneLine),
        ] {
            assert_eq!(
                w.people
                    .restrict(&admin.principal, &w.wiki, &who, 3, reason)
                    .map(|_| ()),
                Err(PeopleError::Reason(fault)),
                "{reason:?}"
            );
        }
        assert!(events(&w, &person).is_empty());
    }
}

#[test]
fn a_maintainer_cannot_restrict_a_maintainer_or_an_admin_an_admin_can_restrict_a_maintainer_and_nobody_restricts_an_admin_or_themselves()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let other_admin = w.person("Ben", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let other_maintainer = w.person("Max", Some(Role::Maintainer));
        let reviewer = w.person("Rae", Some(Role::Reviewer));
        let contributor = w.person("Cat", Some(Role::Contributor));
        let restrict = |by: &Person, who: &Person| {
            w.people
                .restrict(&by.principal, &w.wiki, &who.id.to_string(), 3, "reason")
                .map(|_| ())
        };
        assert_eq!(
            restrict(&maintainer, &other_maintainer),
            Err(PeopleError::Restrict(RestrictRefusal::NotBelowYou))
        );
        assert_eq!(
            restrict(&maintainer, &admin),
            Err(PeopleError::Restrict(RestrictRefusal::AdminIsNotRestricted))
        );
        assert_eq!(
            restrict(&admin, &other_admin),
            Err(PeopleError::Restrict(RestrictRefusal::AdminIsNotRestricted)),
            "nobody restricts an admin"
        );
        assert_eq!(
            restrict(&maintainer, &maintainer),
            Err(PeopleError::Restrict(RestrictRefusal::OwnAccount))
        );
        assert_eq!(
            restrict(&admin, &admin),
            Err(PeopleError::Restrict(RestrictRefusal::OwnAccount))
        );
        assert_eq!(
            restrict(&maintainer, &reviewer),
            Ok(()),
            "a maintainer restricts a reviewer"
        );
        assert_eq!(restrict(&maintainer, &contributor), Ok(()));
        assert_eq!(
            restrict(&admin, &maintainer),
            Ok(()),
            "an admin restricts a maintainer"
        );
        // A restricted maintainer is held down: they cannot restrict anyone (and are told so, not told *not allowed*).
        let fresh = w.person("Fay", Some(Role::Contributor));
        assert!(matches!(
            w.people.restrict(
                &maintainer.principal,
                &w.wiki,
                &fresh.id.to_string(),
                3,
                "r"
            ),
            Err(PeopleError::Restricted(_))
        ));
        // Nobody below the rung the rules ask for moderating restricts.
        assert_eq!(
            restrict(&reviewer, &fresh),
            Err(PeopleError::NotAllowed),
            "a restricted reviewer is no moderator either"
        );
        // And the pure rule agrees with every row above.
        assert!(
            check_restrict(
                Role::Maintainer,
                Role::Maintainer,
                Some(Role::Maintainer),
                false
            )
            .is_err()
        );
    }
}

#[test]
fn where_the_rules_let_a_reviewer_moderate_a_reviewer_restricts_below_themselves_and_the_rung_entry_gives_counts()
 {
    for kind in KINDS {
        let w = World::under(kind, "entry: contributor\nrungs:\n  moderate: reviewer\n");
        let reviewer = w.person("Rae", Some(Role::Reviewer));
        let other_reviewer = w.person("Rob", Some(Role::Reviewer));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        // No grant at all: holds contributor through the wiki's `entry`, below a reviewer, so a reviewer may restrict them.
        let newcomer = w.person("Nia", None);
        let who = |p: &Person| p.id.to_string();
        assert!(
            w.people
                .restrict(&reviewer.principal, &w.wiki, &who(&newcomer), 2, "r")
                .is_ok()
        );
        assert!(
            !w.may(&newcomer, Action::Propose),
            "the entry rung is capped too"
        );
        assert_eq!(
            w.people
                .restrict(&reviewer.principal, &w.wiki, &who(&other_reviewer), 2, "r")
                .map(|_| ()),
            Err(PeopleError::Restrict(RestrictRefusal::NotBelowYou))
        );
        assert_eq!(
            w.people
                .restrict(&reviewer.principal, &w.wiki, &who(&maintainer), 2, "r")
                .map(|_| ()),
            Err(PeopleError::Restrict(RestrictRefusal::NotBelowYou))
        );
    }
}

#[test]
fn whoever_may_restrict_an_account_may_lift_it_and_no_one_else_and_not_the_restricted_person() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let other_maintainer = w.person("Max", Some(Role::Maintainer));
        let contributor = w.person("Cat", Some(Role::Contributor));
        w.people
            .restrict(
                &admin.principal,
                &w.wiki,
                &other_maintainer.id.to_string(),
                5,
                "r",
            )
            .unwrap();
        // Another maintainer cannot lift what an admin set on a maintainer: they could not have set it.
        assert_eq!(
            w.people
                .lift(
                    &maintainer.principal,
                    &w.wiki,
                    &other_maintainer.id.to_string(),
                    ""
                )
                .map(|_| ()),
            Err(PeopleError::Restrict(RestrictRefusal::NotBelowYou))
        );
        // The restricted person cannot lift their own.
        assert!(
            w.people
                .lift(
                    &other_maintainer.principal,
                    &w.wiki,
                    &other_maintainer.id.to_string(),
                    ""
                )
                .is_err()
        );
        assert!(
            w.people
                .lift(
                    &contributor.principal,
                    &w.wiki,
                    &other_maintainer.id.to_string(),
                    ""
                )
                .is_err()
        );
        assert!(
            w.people
                .lift(
                    &admin.principal,
                    &w.wiki,
                    &other_maintainer.id.to_string(),
                    ""
                )
                .is_ok()
        );
    }
}

#[test]
fn the_restricted_person_is_told_the_reason_and_the_end_by_every_service_that_refuses_them_and_no_one_else_is()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let person = w.person("Pat", Some(Role::Contributor));
        let observer = w.person("Obi", Some(Role::Observer));
        let outsider = w.person("Out", None);
        assert!(propose(&w, &person).is_ok(), "before the restriction");
        w.people
            .restrict(
                &admin.principal,
                &w.wiki,
                &person.id.to_string(),
                4,
                "posting the same link everywhere",
            )
            .unwrap();
        let Err(ProposalError::Restricted(notice)) = propose(&w, &person) else {
            panic!("the restricted person is told they are restricted")
        };
        assert_eq!(notice.reason, "posting the same link everywhere");
        assert_eq!(notice.until, T0 + 4 * DAY);
        // The same notice from the authorizer, for any resource: a page of the wiki, the wiki.
        for action in [
            Action::Comment,
            Action::Propose,
            Action::Review,
            Action::CreateView,
        ] {
            let got = w.authorizer.restriction_refusal(
                &person.principal,
                action,
                &Resource::Page {
                    wiki: w.wiki.id.clone(),
                    path: "pages/Home.md".to_owned(),
                },
            );
            // A contributor may comment and propose: the restriction is why they may not. Review and creating a view
            // are refused for the role first.
            let expected = matches!(action, Action::Comment | Action::Propose);
            assert_eq!(got.is_some(), expected, "{}", action.name());
        }
        // A person whose refusal has another cause is never told of a restriction: an observer is refused for the role,
        // and no one else is told this one's.
        assert_eq!(
            propose(&w, &observer).map(|_| ()),
            Err(ProposalError::NotAllowed)
        );
        assert_eq!(
            propose(&w, &outsider).map(|_| ()),
            Err(ProposalError::NotAllowed)
        );
        assert_eq!(
            w.authorizer.restriction_refusal(
                &observer.principal,
                Action::Propose,
                &Resource::Wiki {
                    wiki: w.wiki.id.clone()
                }
            ),
            None
        );
        assert_eq!(
            w.authorizer.restriction_refusal(
                &Principal::Anonymous,
                Action::Propose,
                &Resource::Wiki {
                    wiki: w.wiki.id.clone()
                }
            ),
            None
        );
        // And the notice ends with the restriction.
        set_now(T0 + 4 * DAY);
        assert!(propose(&w, &person).is_ok(), "{kind:?}");
    }
}

#[test]
fn the_restriction_is_asked_about_for_every_write_by_an_account_and_never_for_a_read() {
    // Through the authorizer's own source: a read is never refused by a restriction, so it is not asked.
    use crate::authorize::{AuthzSource, SourceError};
    use inklkunn_store::{AccountId, WikiId};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Counting {
        asked: AtomicUsize,
    }
    impl AuthzSource for Counting {
        fn visibility(&self, _: &WikiId) -> Result<Option<Visibility>, SourceError> {
            Ok(Some(Visibility::Public))
        }
        fn role(&self, _: &AccountId, _: &WikiId) -> Result<Option<Role>, SourceError> {
            Ok(Some(Role::Contributor))
        }
        fn restriction(
            &self,
            _: &AccountId,
            _: &WikiId,
            _: i64,
        ) -> Result<Option<RestrictionEvent>, SourceError> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            Err(SourceError("down".to_owned()))
        }
    }
    let source = Arc::new(Counting {
        asked: AtomicUsize::new(0),
    });
    let a = crate::authorize::Authorizer::new(
        source.clone(),
        Arc::new(inklkunn_store::MemoryAudit::new()),
    );
    let person = Principal::session(
        &crate::authorize::IdentityAuthority::for_tests(),
        AccountId::new(),
    );
    let wiki = Resource::Wiki {
        wiki: WikiId::new(),
    };
    assert!(a.authorize(&person, Action::Read, &wiki).is_allowed());
    assert_eq!(
        source.asked.load(Ordering::SeqCst),
        0,
        "a read asks nothing about a restriction"
    );
    assert!(
        !a.authorize(&person, Action::Propose, &wiki).is_allowed(),
        "a restriction that cannot be read now is a refusal: fail closed"
    );
    assert_eq!(source.asked.load(Ordering::SeqCst), 1);
    assert!(
        !a.authorize(&Principal::Anonymous, Action::Propose, &wiki)
            .is_allowed()
    );
    assert_eq!(
        source.asked.load(Ordering::SeqCst),
        1,
        "an anonymous caller has no account to restrict"
    );
}

#[test]
fn a_restriction_on_a_member_of_a_restricted_wiki_still_lets_them_read_and_entry_gives_nothing_there()
 {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Restricted,
            Some("entry: contributor\n"),
            PlatformLimits::default(),
        );
        let admin = w.person("Ada", Some(Role::Admin));
        let member = w.person("Mem", Some(Role::Contributor));
        let stranger = w.person("Sue", None);
        assert!(
            !w.may(&stranger, Action::Read),
            "entry gives a stranger nothing in a restricted wiki"
        );
        assert!(!w.may(&stranger, Action::Propose));
        w.people
            .restrict(&admin.principal, &w.wiki, &member.id.to_string(), 2, "r")
            .unwrap();
        assert!(
            w.may(&member, Action::Read),
            "they remain a member: reading is never reduced"
        );
        assert!(!w.may(&member, Action::Propose));
        // The same account, in a wiki it holds no role in, is not told it is restricted somewhere else.
        assert_eq!(
            w.authorizer.restriction_refusal(
                &stranger.principal,
                Action::Propose,
                &Resource::Wiki {
                    wiki: w.wiki.id.clone()
                }
            ),
            None
        );
    }
}

#[test]
fn each_restriction_act_is_audited_with_the_rung_and_the_block_and_the_record_names_them_too() {
    for kind in KINDS {
        let w = World::under(kind, "entry: none\n");
        let admin = w.person("Ada", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let person = w.person("Pat", Some(Role::Contributor));
        let who = person.id.to_string();
        w.people
            .restrict(&maintainer.principal, &w.wiki, &who, 2, "spam")
            .unwrap();
        set_now(T0 + 10);
        w.people
            .renew(&admin.principal, &w.wiki, &who, 5, "still spam")
            .unwrap();
        set_now(T0 + 20);
        w.people.lift(&admin.principal, &w.wiki, &who, "").unwrap();
        let short = &BLOCK[..12];
        let trail = w.audit.events();
        let acts: Vec<(&str, &str, &str)> = trail
            .iter()
            .filter(|e| {
                ["restrict", "renew-restriction", "lift-restriction"].contains(&e.action.as_str())
            })
            .map(|e| (e.action.as_str(), e.detail.as_str(), e.resource.as_str()))
            .collect();
        let resource = format!("member:{}:{}", w.wiki.id, person.id);
        assert_eq!(
            acts,
            [
                (
                    "restrict",
                    format!("rung=maintainer; policy={short}; days=2").as_str(),
                    resource.as_str()
                ),
                (
                    "renew-restriction",
                    format!("rung=admin; policy={short}; days=5").as_str(),
                    resource.as_str()
                ),
                (
                    "lift-restriction",
                    format!("rung=admin; policy={short}; ").as_str(),
                    resource.as_str()
                ),
            ],
            "{kind:?}"
        );
        // Each was authorized as `moderate`, recorded with the rung and the block.
        let moderate: Vec<_> = trail.iter().filter(|e| e.action == "moderate").collect();
        let details: Vec<&str> = moderate.iter().map(|e| e.detail.as_str()).collect();
        assert_eq!(
            details,
            [
                format!("rung=maintainer; policy={short}; typed={}", person.id),
                format!("rung=admin; policy={short}; typed={}", person.id),
                format!("rung=admin; policy={short}; typed={}", person.id),
            ]
        );
        assert!(moderate.iter().all(|e| e.outcome == AuditOutcome::Allowed));
        // The record: actor, rung, block, reason, end, and what each follows.
        let chain = events(&w, &person);
        let [first, second, third] = chain.as_slice() else {
            panic!("three events: {chain:?}")
        };
        assert_eq!(first.by.account, maintainer.id);
        assert_eq!(first.by.rung, Role::Maintainer);
        assert_eq!(first.policy_block.as_deref(), Some(BLOCK));
        assert_eq!(first.reason.as_deref(), Some("spam"));
        assert_eq!(first.expires_at, Some(T0 + 2 * DAY));
        assert_eq!(second.by.rung, Role::Admin);
        assert_eq!(second.of_event.as_ref(), Some(&first.id));
        assert_eq!(third.event, MeasureEvent::Lifted);
    }
}

#[test]
fn a_restriction_that_was_not_authorized_leaves_nothing_a_wiki_that_cannot_be_seen_is_not_found_and_an_anonymous_caller_is_not_allowed()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let contributor = w.person("Cat", Some(Role::Contributor));
        let other = w.person("Oli", Some(Role::Contributor));
        assert_eq!(
            w.people
                .restrict(
                    &contributor.principal,
                    &w.wiki,
                    &other.id.to_string(),
                    2,
                    "r"
                )
                .map(|_| ()),
            Err(PeopleError::NotAllowed)
        );
        assert_eq!(
            w.people
                .restrict(
                    &Principal::Anonymous,
                    &w.wiki,
                    &other.id.to_string(),
                    2,
                    "r"
                )
                .map(|_| ()),
            Err(PeopleError::NotAllowed)
        );
        assert!(events(&w, &other).is_empty());
        let restricted = World::with(
            kind,
            Visibility::Restricted,
            None,
            PlatformLimits::default(),
        );
        let stranger = restricted.person("Sue", None);
        assert_eq!(
            restricted
                .people
                .restrict(
                    &stranger.principal,
                    &restricted.wiki,
                    &stranger.id.to_string(),
                    2,
                    "r"
                )
                .map(|_| ()),
            Err(PeopleError::NotFound)
        );
    }
}
