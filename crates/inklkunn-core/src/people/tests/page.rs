//! The people page's model (RFC 006 D-6): the rungs in order, each with the wiki's promotion text and who holds it, who is
//! named to whom, what each person may do on the page, and who is told of a restriction.

use inklkunn_store::{Role, Visibility};

use super::support::{DAY, KINDS, World};
use crate::authorize::{HeldBy, Principal};
use crate::governance::PlatformLimits;
use crate::people::{Holders, PeopleError, PeopleView, RungRow};

fn row(view: &PeopleView, rung: Role) -> &RungRow {
    view.rungs.iter().find(|r| r.rung == rung).expect("a row")
}

fn names(row: &RungRow) -> Vec<String> {
    match &row.holders {
        Holders::Named(people) => people
            .iter()
            .map(|p| p.name.clone().unwrap_or_default())
            .collect(),
        Holders::Count(n) => panic!("only a count: {n}"),
    }
}

#[test]
fn the_page_lists_the_six_rungs_in_order_with_the_promotion_texts_and_the_holders_by_name() {
    for kind in KINDS {
        let w = World::under(
            kind,
            "promotion:\n  contributor: \"A few edits that were accepted without a correction.\"\n  reviewer: \"A season of careful reading.\"\n",
        );
        let admin = w.person("Ada", Some(Role::Admin));
        let _ = w.person("Zed", Some(Role::Contributor));
        let _ = w.person("Bea", Some(Role::Contributor));
        let _ = w.person("Rae", Some(Role::Reviewer));
        let view = w.people.view(&admin.principal, &w.wiki).expect("a page");
        let order: Vec<Role> = view.rungs.iter().map(|r| r.rung).collect();
        assert_eq!(order, Role::ALL, "lowest first");
        assert_eq!(
            row(&view, Role::Contributor).promotion.as_deref(),
            Some("A few edits that were accepted without a correction.")
        );
        assert_eq!(row(&view, Role::Observer).promotion, None);
        assert_eq!(
            names(row(&view, Role::Contributor)),
            ["Bea", "Zed"],
            "by name"
        );
        assert_eq!(names(row(&view, Role::Admin)), ["Ada"]);
        assert_eq!(view.rules_block.as_deref(), Some(super::support::BLOCK));
        let you = view.you.expect("an account");
        assert_eq!(you.rung, Some(Role::Admin));
        assert_eq!(you.held_by(), HeldBy::Grant);
    }
}

#[test]
fn a_visitor_with_no_account_is_told_the_names_of_the_reviewers_and_above_and_only_how_many_hold_the_rest()
 {
    for kind in KINDS {
        let w = World::new(kind);
        let _ = w.person("Ada", Some(Role::Admin));
        let _ = w.person("Rae", Some(Role::Reviewer));
        let _ = w.person("Cat", Some(Role::Contributor));
        let _ = w.person("Con", Some(Role::Contributor));
        let view = w
            .people
            .view(&Principal::Anonymous, &w.wiki)
            .expect("a public wiki");
        assert_eq!(names(row(&view, Role::Reviewer)), ["Rae"]);
        assert_eq!(names(row(&view, Role::Admin)), ["Ada"]);
        assert_eq!(row(&view, Role::Contributor).holders, Holders::Count(2));
        assert_eq!(row(&view, Role::Observer).holders, Holders::Count(0));
        assert!(view.you.is_none());
        assert!(view.may_grant_up_to.is_none() && !view.may_restrict);
        // An account sees every name.
        let anyone = w.person("Ann", None);
        let view = w.people.view(&anyone.principal, &w.wiki).unwrap();
        assert_eq!(names(row(&view, Role::Contributor)), ["Cat", "Con"]);
    }
}

#[test]
fn a_restricted_wiki_is_one_answer_with_an_absent_one_for_a_stranger_and_an_internal_one_is_for_accounts()
 {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Restricted,
            None,
            PlatformLimits::default(),
        );
        let member = w.person("Mem", Some(Role::Observer));
        let stranger = w.person("Sue", None);
        assert_eq!(
            w.people.view(&stranger.principal, &w.wiki).map(|_| ()),
            Err(PeopleError::NotFound)
        );
        assert_eq!(
            w.people.view(&Principal::Anonymous, &w.wiki).map(|_| ()),
            Err(PeopleError::NotFound)
        );
        let mut absent = w.wiki.clone();
        absent.id = inklkunn_store::WikiId::new();
        assert_eq!(
            w.people.view(&stranger.principal, &absent).map(|_| ()),
            Err(PeopleError::NotFound)
        );
        assert!(w.people.view(&member.principal, &w.wiki).is_ok());
        let internal = World::with(kind, Visibility::Internal, None, PlatformLimits::default());
        assert_eq!(
            internal
                .people
                .view(&Principal::Anonymous, &internal.wiki)
                .map(|_| ()),
            Err(PeopleError::NotFound)
        );
        assert!(
            internal
                .people
                .view(&internal.person("Ann", None).principal, &internal.wiki)
                .is_ok()
        );
    }
}

#[test]
fn what_entry_gives_everyone_is_said_and_it_gives_nothing_to_the_page_of_a_restricted_wiki() {
    for kind in KINDS {
        let public = World::under(kind, "entry: questioner\n");
        let a = public.person("Ann", None);
        let view = public.people.view(&a.principal, &public.wiki).unwrap();
        assert!(view.entry_applies);
        assert_eq!(view.entry.rung(), Some(Role::Questioner));
        let you = view.you.unwrap();
        assert_eq!(
            (you.rung, you.held_by()),
            (Some(Role::Questioner), HeldBy::Entry)
        );

        let closed = World::with(
            kind,
            Visibility::Restricted,
            Some("entry: questioner\n"),
            PlatformLimits::default(),
        );
        let m = closed.person("Mem", Some(Role::Observer));
        let view = closed.people.view(&m.principal, &closed.wiki).unwrap();
        assert!(
            !view.entry_applies,
            "entry has no effect in a restricted wiki"
        );
        assert_eq!(
            view.entry.rung(),
            Some(Role::Questioner),
            "the rules still say it"
        );
        let you = view.you.unwrap();
        assert_eq!(
            (you.rung, you.held_by()),
            (Some(Role::Observer), HeldBy::Grant)
        );
    }
}

#[test]
fn what_a_person_may_do_on_the_page_follows_their_rung_and_the_reach_of_a_maintainer() {
    for kind in KINDS {
        let w = World::under(kind, "rungs:\n  grant_up_to: contributor\n");
        let admin = w.person("Ada", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let contributor = w.person("Cat", Some(Role::Contributor));
        let reviewer = w.person("Rae", Some(Role::Reviewer));
        let may = |p: &super::support::Person| w.people.view(&p.principal, &w.wiki).unwrap();
        assert_eq!(may(&admin).may_grant_up_to, Some(Role::Admin));
        assert!(may(&admin).may_restrict);
        assert_eq!(may(&maintainer).may_grant_up_to, Some(Role::Contributor));
        assert!(may(&maintainer).may_restrict);
        assert_eq!(may(&contributor).may_grant_up_to, None);
        assert!(!may(&contributor).may_restrict);
        assert!(
            !may(&reviewer).may_restrict,
            "moderating is a maintainer's by default"
        );
        // A maintainer may change what is within reach and sees no link for what is above it.
        let view = may(&maintainer);
        let person = |role: Role, who: &str| {
            let Holders::Named(people) = &row(&view, role).holders else {
                panic!("named")
            };
            people
                .iter()
                .find(|p| p.name.as_deref() == Some(who))
                .cloned()
                .expect("a person")
        };
        assert!(person(Role::Contributor, "Cat").may_change);
        assert!(!person(Role::Reviewer, "Rae").may_change, "above the reach");
        assert!(!person(Role::Admin, "Ada").may_change);
        assert!(person(Role::Maintainer, "Mia").is_you);
        assert!(
            !person(Role::Maintainer, "Mia").may_change,
            "nobody changes their own"
        );
        assert!(person(Role::Reviewer, "Rae").may_restrict);
        assert!(!person(Role::Admin, "Ada").may_restrict);
        assert!(!person(Role::Maintainer, "Mia").may_restrict);
        // The default reach is none: a maintainer changes no one's rung.
        let default = World::new(kind);
        let m = default.person("Mia", Some(Role::Maintainer));
        assert_eq!(
            default
                .people
                .view(&m.principal, &default.wiki)
                .unwrap()
                .may_grant_up_to,
            None
        );
    }
}

#[test]
fn who_is_restricted_until_when_and_why_is_told_to_the_moderating_rung_and_the_person_alone() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Ada", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        let person = w.person("Pat", Some(Role::Contributor));
        let bystander = w.person("Bea", Some(Role::Reviewer));
        w.people
            .restrict(
                &maintainer.principal,
                &w.wiki,
                &person.id.to_string(),
                3,
                "posting links",
            )
            .unwrap();
        // The moderating rung sees the list, the reason and the end.
        let view = w.people.view(&admin.principal, &w.wiki).unwrap();
        let [one] = view.restricted.as_slice() else {
            panic!("one restricted: {:?}", view.restricted)
        };
        assert_eq!(one.account, person.id);
        assert_eq!(one.reason, "posting links");
        assert_eq!(one.until, super::support::T0 + 3 * DAY);
        assert!(one.may_act);
        let Holders::Named(contributors) = &row(&view, Role::Contributor).holders else {
            panic!("named")
        };
        assert_eq!(
            contributors.first().and_then(|p| p.restricted_until),
            Some(one.until)
        );
        // The person is told on their own standing, and sees no list.
        let own = w.people.view(&person.principal, &w.wiki).unwrap();
        assert!(own.restricted.is_empty());
        let notice = own.you.unwrap().restriction.expect("told their own");
        assert_eq!(
            (notice.reason.as_str(), notice.until),
            ("posting links", one.until)
        );
        // No one else is told anything: not a reviewer, not a visitor.
        for viewer in [&bystander.principal, &Principal::Anonymous] {
            let view = w.people.view(viewer, &w.wiki).unwrap();
            assert!(view.restricted.is_empty());
            if let Holders::Named(people) = &row(&view, Role::Contributor).holders {
                assert!(people.iter().all(|p| p.restricted_until.is_none()));
            }
            assert!(view.you.as_ref().is_none_or(|y| y.restriction.is_none()));
        }
        // It is gone from the list when it has ended.
        super::support::set_now(super::support::T0 + 3 * DAY);
        assert!(
            w.people
                .view(&admin.principal, &w.wiki)
                .unwrap()
                .restricted
                .is_empty()
        );
    }
}

#[test]
fn a_restricted_maintainer_is_told_it_is_the_restriction_that_holds_them_back_not_that_they_may_not()
 {
    for kind in KINDS {
        let w = World::under(kind, "rungs:\n  grant_up_to: contributor\n");
        let admin = w.person("Ada", Some(Role::Admin));
        let maintainer = w.person("Mia", Some(Role::Maintainer));
        w.people
            .restrict(
                &admin.principal,
                &w.wiki,
                &maintainer.id.to_string(),
                2,
                "a dispute",
            )
            .unwrap();
        let view = w.people.view(&maintainer.principal, &w.wiki).unwrap();
        assert_eq!(view.may_grant_up_to, None);
        assert!(!view.may_restrict);
        assert_eq!(
            view.grant_held_down_by.as_ref().map(|n| n.reason.as_str()),
            Some("a dispute")
        );
        assert_eq!(
            view.restrict_held_down_by
                .as_ref()
                .map(|n| n.reason.as_str()),
            Some("a dispute")
        );
        // A contributor who could never grant is not told of a restriction either.
        let c = w.person("Cat", Some(Role::Contributor));
        w.people
            .restrict(&admin.principal, &w.wiki, &c.id.to_string(), 2, "x")
            .unwrap();
        let view = w.people.view(&c.principal, &w.wiki).unwrap();
        assert!(view.grant_held_down_by.is_none() && view.restrict_held_down_by.is_none());
    }
}
