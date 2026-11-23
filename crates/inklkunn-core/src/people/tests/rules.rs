//! The two rules as pure tables: who may change whose rung (RFC 006 D-6) and who may restrict whom (D-7; C-14h). Each is held
//! by the named cases the handoff asks for, then by a sweep of every combination against an oracle written in the words of
//! the rule, not in the code's.

use inklkunn_store::Role;

use crate::governance::GrantUpTo;
use crate::people::{
    GrantCheck, GrantRefusal, MAX_REASON_CHARS, ReasonFault, RestrictRefusal, check_grant,
    check_restrict, clean_reason, grants_at_all, highest_grantable,
};

fn grant(
    actor: Role,
    up_to: GrantUpTo,
    current: Option<Role>,
    new: Option<Role>,
) -> Result<(), GrantRefusal> {
    check_grant(&GrantCheck {
        actor,
        own: false,
        current,
        new,
        grant_up_to: up_to,
        other_admins_who_can_sign_in: 1,
    })
}

#[test]
fn an_admin_changes_any_rung_to_any_rung_and_a_maintainer_only_within_reach_on_both_ends() {
    use Role::*;
    // An admin: any account, any rung, whatever `grant_up_to` says.
    for up_to in GrantUpTo::ALL {
        assert_eq!(grant(Admin, *up_to, None, Some(Admin)), Ok(()));
        assert_eq!(
            grant(Admin, *up_to, Some(Maintainer), Some(Observer)),
            Ok(())
        );
        assert_eq!(grant(Admin, *up_to, Some(Reviewer), None), Ok(()));
    }
    // A maintainer where the rules let a maintainer grant up to reviewer.
    let r = GrantUpTo::Reviewer;
    assert_eq!(
        grant(Maintainer, r, None, Some(Reviewer)),
        Ok(()),
        "up to the reach"
    );
    assert_eq!(
        grant(Maintainer, r, Some(Reviewer), Some(Observer)),
        Ok(()),
        "a demotion within reach"
    );
    assert_eq!(
        grant(Maintainer, r, Some(Contributor), None),
        Ok(()),
        "a revocation within reach"
    );
    let above = Err(GrantRefusal::AboveReach { up_to: r });
    assert_eq!(
        grant(Maintainer, r, None, Some(Maintainer)),
        above,
        "a rung above the reach"
    );
    assert_eq!(grant(Maintainer, r, None, Some(Admin)), above);
    assert_eq!(
        grant(Maintainer, r, Some(Maintainer), Some(Reviewer)),
        above,
        "a demotion of someone who stands above the reach"
    );
    assert_eq!(
        grant(Maintainer, r, Some(Admin), None),
        above,
        "a revocation of someone above the reach"
    );
    // A narrower reach.
    let c = GrantUpTo::Contributor;
    assert_eq!(grant(Maintainer, c, None, Some(Contributor)), Ok(()));
    assert_eq!(
        grant(Maintainer, c, None, Some(Reviewer)),
        Err(GrantRefusal::AboveReach { up_to: c })
    );
    assert_eq!(
        grant(Maintainer, c, Some(Reviewer), Some(Observer)),
        Err(GrantRefusal::AboveReach { up_to: c }),
        "a reviewer stands above a reach of contributor"
    );
    // Where the rules let a maintainer grant none, only admins grant.
    for new in [None, Some(Observer), Some(Contributor)] {
        assert_eq!(
            grant(Maintainer, GrantUpTo::None, None, new.or(Some(Observer))),
            Err(GrantRefusal::NotAllowed)
        );
    }
}

#[test]
fn nobody_below_a_maintainer_grants_and_nobody_changes_their_own_rung() {
    use Role::*;
    for actor in [Observer, Questioner, Contributor, Reviewer] {
        for up_to in GrantUpTo::ALL {
            assert_eq!(
                grant(actor, *up_to, None, Some(Observer)),
                Err(GrantRefusal::NotAllowed),
                "{actor:?} {up_to:?}"
            );
            assert!(!grants_at_all(actor, *up_to));
            assert_eq!(highest_grantable(actor, *up_to), None);
        }
    }
    for actor in Role::ALL {
        let own = check_grant(&GrantCheck {
            actor,
            own: true,
            current: Some(Contributor),
            new: Some(Observer),
            grant_up_to: GrantUpTo::Reviewer,
            other_admins_who_can_sign_in: 1,
        });
        assert_eq!(
            own,
            Err(GrantRefusal::OwnRung),
            "{actor:?}: even an admin's own rung stays"
        );
    }
    // The ceiling is what the rules say and nothing else.
    assert_eq!(highest_grantable(Admin, GrantUpTo::None), Some(Admin));
    assert_eq!(highest_grantable(Maintainer, GrantUpTo::None), None);
    assert_eq!(
        highest_grantable(Maintainer, GrantUpTo::Questioner),
        Some(Questioner)
    );
}

#[test]
fn the_last_admin_who_can_sign_in_is_not_demoted_or_revoked_and_a_change_that_changes_nothing_is_refused()
 {
    use Role::*;
    let one = |current, new| {
        check_grant(&GrantCheck {
            actor: Admin,
            own: false,
            current,
            new,
            grant_up_to: GrantUpTo::None,
            other_admins_who_can_sign_in: 0,
        })
    };
    assert_eq!(
        one(Some(Admin), Some(Maintainer)),
        Err(GrantRefusal::LastAdmin)
    );
    assert_eq!(one(Some(Admin), None), Err(GrantRefusal::LastAdmin));
    assert_eq!(
        one(Some(Admin), Some(Admin)),
        Err(GrantRefusal::NothingChanges)
    );
    assert_eq!(
        one(None, Some(Admin)),
        Ok(()),
        "making a second admin is the way out"
    );
    assert_eq!(
        one(Some(Maintainer), Some(Observer)),
        Ok(()),
        "only an admin's own rung is the last one"
    );
    assert_eq!(
        check_grant(&GrantCheck {
            actor: Admin,
            own: false,
            current: Some(Admin),
            new: None,
            grant_up_to: GrantUpTo::None,
            other_admins_who_can_sign_in: 1,
        }),
        Ok(()),
        "two admins: one may go"
    );
    assert_eq!(
        grant(Admin, GrantUpTo::None, None, None),
        Err(GrantRefusal::NothingChanges)
    );
    assert_eq!(
        grant(Admin, GrantUpTo::None, Some(Reviewer), Some(Reviewer)),
        Err(GrantRefusal::NothingChanges)
    );
}

/// The rule in the words of the handoff, written another way round: the sweep's oracle.
fn oracle(c: &GrantCheck) -> Result<(), GrantRefusal> {
    if c.own {
        return Err(GrantRefusal::OwnRung);
    }
    let reach = match c.actor {
        Role::Admin => Some(Role::Admin),
        Role::Maintainer => c.grant_up_to.highest(),
        _ => None,
    };
    let Some(reach) = reach else {
        return Err(GrantRefusal::NotAllowed);
    };
    if c.current == c.new {
        return Err(GrantRefusal::NothingChanges);
    }
    let under = |r: Option<Role>| r.is_none_or(|r| r <= reach);
    if !(under(c.current) && under(c.new)) {
        return Err(GrantRefusal::AboveReach {
            up_to: c.grant_up_to,
        });
    }
    if c.current == Some(Role::Admin)
        && c.new != Some(Role::Admin)
        && c.other_admins_who_can_sign_in == 0
    {
        return Err(GrantRefusal::LastAdmin);
    }
    Ok(())
}

#[test]
fn every_combination_of_actor_reach_current_new_and_admins_is_decided_as_the_rule_says() {
    let rungs: Vec<Option<Role>> = std::iter::once(None)
        .chain(Role::ALL.into_iter().map(Some))
        .collect();
    let mut cases = 0;
    for actor in Role::ALL {
        for up_to in GrantUpTo::ALL {
            for current in &rungs {
                for new in &rungs {
                    for own in [false, true] {
                        for admins in [0, 1] {
                            let c = GrantCheck {
                                actor,
                                own,
                                current: *current,
                                new: *new,
                                grant_up_to: *up_to,
                                other_admins_who_can_sign_in: admins,
                            };
                            assert_eq!(check_grant(&c), oracle(&c), "{c:?}");
                            cases += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases, 6 * 5 * 7 * 7 * 2 * 2);
}

fn restrict(
    actor: Role,
    moderate: Role,
    target: Option<Role>,
    own: bool,
) -> Result<(), RestrictRefusal> {
    check_restrict(actor, moderate, target, own)
}

#[test]
fn a_maintainer_cannot_restrict_a_maintainer_or_an_admin_an_admin_can_restrict_a_maintainer_and_nobody_restricts_an_admin()
 {
    use Role::*;
    let m = Maintainer;
    assert_eq!(
        restrict(m, m, Some(Maintainer), false),
        Err(RestrictRefusal::NotBelowYou)
    );
    assert_eq!(
        restrict(m, m, Some(Admin), false),
        Err(RestrictRefusal::AdminIsNotRestricted)
    );
    assert_eq!(
        restrict(Admin, m, Some(Maintainer), false),
        Ok(()),
        "an admin restricts a maintainer"
    );
    assert_eq!(
        restrict(Admin, m, Some(Admin), false),
        Err(RestrictRefusal::AdminIsNotRestricted),
        "nobody restricts an admin, an admin included"
    );
    for rung in [Observer, Questioner, Contributor, Reviewer] {
        assert_eq!(
            restrict(m, m, Some(rung), false),
            Ok(()),
            "a maintainer restricts a {rung:?}"
        );
    }
    assert_eq!(
        restrict(m, m, None, false),
        Ok(()),
        "an account that holds no rung can be restricted"
    );
    // Nobody restricts themselves.
    for actor in Role::ALL {
        assert_eq!(
            restrict(actor, Reviewer, Some(Observer), true),
            Err(RestrictRefusal::OwnAccount),
            "{actor:?}"
        );
    }
    // Below the rung the rules ask for moderating there is no act.
    assert_eq!(
        restrict(Reviewer, m, Some(Observer), false),
        Err(RestrictRefusal::NotAllowed)
    );
    // Where the rules let a reviewer moderate, a reviewer restricts below a reviewer and no further.
    assert_eq!(
        restrict(Reviewer, Reviewer, Some(Contributor), false),
        Ok(())
    );
    assert_eq!(
        restrict(Reviewer, Reviewer, Some(Reviewer), false),
        Err(RestrictRefusal::NotBelowYou)
    );
    assert_eq!(
        restrict(Reviewer, Reviewer, Some(Maintainer), false),
        Err(RestrictRefusal::NotBelowYou)
    );
}

#[test]
fn a_reason_is_one_trimmed_bounded_line_counted_in_characters() {
    assert_eq!(clean_reason("   "), Ok(None));
    assert_eq!(clean_reason(""), Ok(None));
    assert_eq!(clean_reason("  spam  "), Ok(Some("spam".to_owned())));
    let longest = "あ".repeat(MAX_REASON_CHARS);
    assert_eq!(
        clean_reason(&longest),
        Ok(Some(longest.clone())),
        "characters, not bytes"
    );
    assert_eq!(
        clean_reason(&format!("{longest}あ")),
        Err(ReasonFault::TooLong),
        "one more is too many"
    );
    for bad in ["a\nb", "a\rb", "a\u{2028}b", "a\u{0}b", "a\tb"] {
        assert_eq!(clean_reason(bad), Err(ReasonFault::NotOneLine), "{bad:?}");
    }
}
