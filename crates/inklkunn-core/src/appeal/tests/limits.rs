//! The quotas of appeals (RFC 006 D-9, RFC 004 D-11): **appeals cannot flood the rung that decides them.** A refusal names the limit,
//! the numbers and when it ends, in the one typed refusal every quota gives.

use inklkunn_store::{AppealedKind, Role, Visibility};

use super::scenes::scene;
use super::support::{DAY, KINDS, Kind, T0, World, now, set_now};
use crate::appeal::{AppealError, AppealLimits, Outcome, QUEUE_BOUND};
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
                ..AppealLimits::default()
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
                ..AppealLimits::default()
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
                ..AppealLimits::default()
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

#[test]
fn a_person_holds_so_many_open_appeals_and_the_next_is_refused_by_name_until_one_of_theirs_is_decided()
 {
    for kind in KINDS {
        let w = World::with(
            kind,
            Visibility::Public,
            None,
            AppealLimits {
                per_person_per_day: 10,
                open_per_person: 2,
                open_per_wiki: 50,
            },
            now,
        );
        let admin = w.person("Dee", Some(Role::Admin));
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
        let first = w
            .appeals
            .file(&author.principal, &w.wiki, one, WORDS)
            .unwrap();
        w.appeals
            .file(&author.principal, &w.wiki, two, WORDS)
            .unwrap();
        let refused = refusal(w.appeals.file(&author.principal, &w.wiki, three, WORDS));
        assert_eq!(
            refused,
            QuotaRefusal {
                limit: QuotaLimit::OpenAppealsPerPerson,
                allowed: 2,
                current: 2,
                asked: 1,
                reset: Reset::WhenOneOfYoursIsDecided,
            },
            "{kind:?}"
        );
        assert_eq!(refused.limit.name(), "open-appeals-per-person");
        // Another person is not held back by theirs, and the wiki's room is still there.
        let other = scene(&w, AppealedKind::Restriction);
        w.appeals
            .file(&other.appellant.principal, &w.wiki, &other.contest, WORDS)
            .expect("another person's appeal");
        // A decision on one of theirs gives the room back.
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
            .file(&author.principal, &w.wiki, three, WORDS)
            .expect("room again");
    }
}

#[test]
fn an_old_open_appeal_is_listed_however_many_decided_ones_follow_it() {
    for kind in KINDS {
        let w = World::new(kind);
        let admin = w.person("Dee", Some(Role::Admin));
        let old = scene(&w, AppealedKind::Restriction);
        let waiting = w
            .appeals
            .file(&old.appellant.principal, &w.wiki, &old.contest, WORDS)
            .unwrap();
        // More decided appeals than a page of the store holds, all newer than it.
        for _ in 0..205 {
            let s = scene(&w, AppealedKind::Restriction);
            let filed = w
                .appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap();
            w.appeals
                .decide(
                    &admin.principal,
                    &w.wiki,
                    &filed.id,
                    Outcome::Upheld,
                    "Done.",
                )
                .unwrap();
        }
        let view = w.appeals.appeals(&admin.principal, &w.wiki).unwrap();
        let open: Vec<_> = view.open.iter().map(|q| q.row.id.clone()).collect();
        assert_eq!(
            open,
            vec![waiting.id],
            "{kind:?}: the old open one is listed"
        );
        assert_eq!(view.more_open, 0);
        assert_eq!(
            view.decided.len(),
            200,
            "{kind:?}: the newest decided ones, a page of them"
        );
    }
}

#[test]
fn a_queue_cut_at_its_bound_says_how_many_more_are_open_and_lists_the_oldest_first() {
    let w = World::with(
        Kind::Memory,
        Visibility::Public,
        None,
        AppealLimits {
            per_person_per_day: 10,
            open_per_person: 5,
            open_per_wiki: 120,
        },
        now,
    );
    let admin = w.person("Dee", Some(Role::Admin));
    let mut ids = Vec::new();
    for _ in 0..(QUEUE_BOUND + 7) {
        let s = scene(&w, AppealedKind::Restriction);
        ids.push(
            w.appeals
                .file(&s.appellant.principal, &w.wiki, &s.contest, WORDS)
                .unwrap()
                .id,
        );
    }
    let view = w.appeals.appeals(&admin.principal, &w.wiki).unwrap();
    assert_eq!(view.open.len(), QUEUE_BOUND);
    assert_eq!(view.more_open, 7);
    let listed: Vec<_> = view.open.iter().map(|q| q.row.id.clone()).collect();
    assert_eq!(
        listed,
        ids.iter().take(QUEUE_BOUND).cloned().collect::<Vec<_>>()
    );
}
