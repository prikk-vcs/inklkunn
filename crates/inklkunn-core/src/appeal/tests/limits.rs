//! The quotas of appeals (RFC 006 D-9, RFC 004 D-11): **appeals cannot flood the rung that decides them.** A refusal names the limit,
//! the numbers and when it ends, in the one typed refusal every quota gives.

use inklkunn_store::{AppealedKind, Role, Visibility};

use super::scenes::scene;
use super::support::{DAY, KINDS, T0, World, now, set_now};
use crate::appeal::{AppealError, AppealLimits, Outcome};
use crate::proposal::{QuotaLimit, QuotaRefusal, Reset};

const WORDS: &str = "That was not me.";

fn refusal(result: Result<crate::appeal::AppealFiled, AppealError>) -> QuotaRefusal {
    match result {
        Err(AppealError::Quota(q)) => q,
        other => panic!("a quota refusal: {other:?}"),
    }
}

#[test]
fn a_person_files_so_many_appeals_a_day_and_the_next_is_refused_by_name_with_when_it_will_be_possible()
 {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Public,
            None,
            AppealLimits {
                per_person_per_day: 2,
                open_per_wiki: 50,
            },
            now,
        );
        let mover = w.person("Max", Some(Role::Maintainer));
        let author = w.person("Pat", Some(Role::Contributor));
        let declined: Vec<_> = (0..3)
            .map(|n| {
                let proposal = w.proposal(&author, &format!("Page{n}"));
                w.decline(&proposal, &mover)
            })
            .collect();
        let [one, two, three] = declined.as_slice() else {
            panic!("three");
        };
        w.appeals
            .file(&author.principal, &w.wiki, one, WORDS)
            .unwrap();
        set_now(T0 + 60);
        w.appeals
            .file(&author.principal, &w.wiki, two, WORDS)
            .unwrap();
        set_now(T0 + 120);
        let refused = refusal(w.appeals.file(&author.principal, &w.wiki, three, WORDS));
        assert_eq!(
            refused,
            QuotaRefusal {
                limit: QuotaLimit::AppealsPerDayPerPrincipal,
                allowed: 2,
                current: 2,
                asked: 1,
                reset: Reset::At(T0 + DAY),
            },
            "the first one leaves the day at T0 + a day, and another fits then"
        );
        assert_eq!(refused.limit.name(), "appeals-per-day-per-principal");
        assert_eq!(
            w.store.open_appeals(&w.wiki.id, 10).unwrap().len(),
            2,
            "nothing was recorded for the refused one"
        );
        // Another person is not bound by this person's day.
        let scene_two = scene(&w, AppealedKind::Restriction);
        w.appeals
            .file(
                &scene_two.appellant.principal,
                &w.wiki,
                &scene_two.contest,
                WORDS,
            )
            .unwrap();
        // When the first leaves the day, one more fits.
        set_now(T0 + DAY);
        w.appeals
            .file(&author.principal, &w.wiki, three, WORDS)
            .expect("a day later");
        set_now(T0);
    }
}

#[test]
fn a_wiki_holds_so_many_open_appeals_and_the_next_is_refused_by_name_until_one_is_decided() {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Public,
            None,
            AppealLimits {
                per_person_per_day: 10,
                open_per_wiki: 2,
            },
            now,
        );
        let admin = w.person("Dee", Some(Role::Admin));
        let scenes: Vec<_> = (0..3)
            .map(|_| scene(&w, AppealedKind::Restriction))
            .collect();
        let [one, two, three] = scenes.as_slice() else {
            panic!("three");
        };
        let first = w
            .appeals
            .file(&one.appellant.principal, &w.wiki, &one.contest, WORDS)
            .unwrap();
        w.appeals
            .file(&two.appellant.principal, &w.wiki, &two.contest, WORDS)
            .unwrap();
        let refused =
            refusal(
                w.appeals
                    .file(&three.appellant.principal, &w.wiki, &three.contest, WORDS),
            );
        assert_eq!(
            refused,
            QuotaRefusal {
                limit: QuotaLimit::OpenAppealsPerWiki,
                allowed: 2,
                current: 2,
                asked: 1,
                reset: Reset::WhenAnAppealIsDecided,
            },
            "the count shown is the limit itself: the refusal is not a way to watch other people's appeals"
        );
        assert_eq!(refused.limit.name(), "open-appeals-per-wiki");
        // Once one is decided, the room is back.
        w.appeals
            .decide(
                &admin.principal,
                &w.wiki,
                &first.id,
                Outcome::Upheld,
                "Done.",
            )
            .unwrap();
        w.appeals
            .file(&three.appellant.principal, &w.wiki, &three.contest, WORDS)
            .expect("room again");
    }
}

#[test]
fn the_default_limits_are_the_documented_ones() {
    let d = AppealLimits::default();
    assert_eq!(
        (d.per_person_per_day, d.open_per_wiki),
        (
            crate::appeal::PER_DAY_DEFAULT,
            crate::appeal::OPEN_PER_WIKI_DEFAULT
        )
    );
    assert_eq!((d.per_person_per_day, d.open_per_wiki), (3, 50));
    assert_eq!(crate::appeal::OPEN_PER_WIKI_MAX, 200);
}

#[test]
fn a_second_appeal_against_the_same_action_is_told_it_is_a_second_even_when_the_person_is_at_their_limit()
 {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Public,
            None,
            AppealLimits {
                per_person_per_day: 1,
                open_per_wiki: 50,
            },
            now,
        );
        let s = scene(&w, AppealedKind::Restriction);
        let first = w
            .appeals
            .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
            .unwrap();
        // The day's one is used, and the same action again is a duplicate before it is a limit: the person is told what is true.
        assert_eq!(
            w.appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap_err(),
            AppealError::AlreadyOpen { appeal: first.id }
        );
    }
}
