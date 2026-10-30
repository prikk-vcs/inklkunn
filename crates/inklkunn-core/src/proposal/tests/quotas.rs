//! Every quota refuses by name at its boundary, with the limit, the current value, what was asked and when it
//! resets (RFC 004 D-11), on both stores. The boundary is tested on both sides: the last act that fits, and
//! the first that does not.

use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, ProposalFilter, Role,
};

use super::support::{Env, T0, edit, on_both_with, set_now};
use crate::proposal::{ProposalError, QuotaConfig, QuotaLimit, QuotaRefusal, Reset};

fn refusal(r: Result<impl std::fmt::Debug, ProposalError>) -> QuotaRefusal {
    match r {
        Err(ProposalError::Quota(q)) => q,
        other => panic!("expected a quota refusal, got {other:?}"),
    }
}

fn create(
    e: &Env,
    who: &super::support::Actor,
    n: usize,
) -> Result<inklkunn_store::ProposalRecord, ProposalError> {
    e.service.create(
        &who.principal,
        &e.wiki,
        None,
        edit(&format!("Change {n}"), &format!("pages/p{n}.md"), "x"),
    )
}

const HOUR: i64 = 3600;
const DAY: i64 = 86_400;

#[test]
fn the_defaults_are_the_rfcs() {
    let d = QuotaConfig::default();
    assert_eq!(d.open_proposals, 20);
    assert_eq!(d.revisions_per_proposal, 50);
    assert_eq!(d.accepted_bytes_per_day, 8 * 1024 * 1024);
    assert_eq!(d.proposals_per_hour_per_principal, 30);
    assert_eq!(d.proposals_per_hour_per_wiki, 300);
    assert_eq!(d.acceptance_queue, 64);
}

#[test]
fn the_limits_have_fixed_distinct_names() {
    let names: Vec<_> = QuotaLimit::ALL.iter().map(|l| l.name()).collect();
    assert_eq!(
        names,
        vec![
            "open-proposals",
            "revisions-per-proposal",
            "accepted-bytes-per-day",
            "proposals-per-hour-per-principal",
            "proposals-per-hour-per-wiki",
            "acceptance-queue",
        ]
    );
}

#[test]
fn open_proposals_refuse_the_one_past_the_limit_and_free_up_when_one_ends() {
    let config = QuotaConfig {
        open_proposals: 3,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        let made: Vec<_> = (0..3).map(|n| create(e, &e.author, n).unwrap()).collect();
        let q = refusal(create(e, &e.author, 3));
        assert_eq!(
            q,
            QuotaRefusal {
                limit: QuotaLimit::OpenProposals,
                allowed: 3,
                current: 3,
                asked: 1,
                reset: Reset::WhenAProposalCloses,
            }
        );
        // The refusal created nothing, and is one person's: someone else still has room.
        assert_eq!(
            e.service
                .list(&e.author.principal, &e.wiki, &ProposalFilter::default())
                .unwrap()
                .len(),
            3
        );
        assert!(create(e, &e.person(Some(Role::Contributor)), 9).is_ok());
        // A drafts counts, a submitted one counts, an ended one does not.
        e.service
            .submit(&e.author.principal, &e.wiki, &made.first().unwrap().id)
            .unwrap();
        assert_eq!(refusal(create(e, &e.author, 4)).current, 3);
        e.service
            .withdraw(
                &e.author.principal,
                &e.wiki,
                &made.first().unwrap().id,
                "Not needed.",
            )
            .unwrap();
        assert!(create(e, &e.author, 5).is_ok());
        assert_eq!(refusal(create(e, &e.author, 6)).current, 3);
    });
}

#[test]
fn the_hourly_rate_per_principal_is_a_rolling_window_and_says_when_it_resets() {
    let config = QuotaConfig {
        proposals_per_hour_per_principal: 2,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        set_now(T0);
        create(e, &e.author, 0).unwrap();
        set_now(T0 + 600);
        create(e, &e.author, 1).unwrap();
        // The third, in the same hour, is refused; the window ends when the *first* ages out.
        set_now(T0 + 1200);
        let q = refusal(create(e, &e.author, 2));
        assert_eq!(
            q,
            QuotaRefusal {
                limit: QuotaLimit::ProposalsPerHourPerPrincipal,
                allowed: 2,
                current: 2,
                asked: 1,
                reset: Reset::At(T0 + HOUR),
            }
        );
        // One second before the first leaves the window: still refused, with the same reset.
        set_now(T0 + HOUR - 1);
        assert_eq!(refusal(create(e, &e.author, 2)).reset, Reset::At(T0 + HOUR));
        // At the reset time, it fits.
        set_now(T0 + HOUR);
        create(e, &e.author, 2).unwrap();
        // And now the window holds the second and the third: the next resets when the second leaves.
        set_now(T0 + HOUR + 1);
        assert_eq!(
            refusal(create(e, &e.author, 3)).reset,
            Reset::At(T0 + 600 + HOUR)
        );
    });
}

#[test]
fn the_hourly_rate_per_wiki_counts_everyone_and_the_principals_limit_is_asked_first() {
    let config = QuotaConfig {
        proposals_per_hour_per_wiki: 3,
        proposals_per_hour_per_principal: 2,
        open_proposals: 100,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        set_now(T0);
        let (a, b, c) = (
            e.person(Some(Role::Contributor)),
            e.person(Some(Role::Contributor)),
            e.person(Some(Role::Contributor)),
        );
        create(e, &a, 0).unwrap();
        set_now(T0 + 10);
        create(e, &b, 1).unwrap();
        set_now(T0 + 20);
        create(e, &c, 2).unwrap();
        // A fourth, by a fresh person, is refused by the wiki's limit, naming the wiki's limit.
        set_now(T0 + 30);
        let d = e.person(Some(Role::Contributor));
        assert_eq!(
            refusal(create(e, &d, 3)),
            QuotaRefusal {
                limit: QuotaLimit::ProposalsPerHourPerWiki,
                allowed: 3,
                current: 3,
                asked: 1,
                reset: Reset::At(T0 + HOUR),
            }
        );
        // When the person is over their own limit too, theirs is the one named: it is the one they can act on.
        set_now(T0 + HOUR + 15);
        create(e, &a, 4).unwrap();
        create(e, &a, 5).unwrap();
        let q = refusal(create(e, &a, 6));
        assert_eq!(q.limit, QuotaLimit::ProposalsPerHourPerPrincipal);
    });
}

#[test]
fn revisions_per_proposal_refuse_the_one_past_the_limit_and_do_not_reset() {
    let config = QuotaConfig {
        revisions_per_proposal: 3,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        let p = e.held(&e.author, "One", "pages/a.md");
        for n in 2..=3 {
            e.service
                .revise(
                    &e.author.principal,
                    &e.wiki,
                    &p.id,
                    edit("One", "pages/a.md", &format!("v{n}")),
                )
                .unwrap();
        }
        let q = refusal(e.service.revise(
            &e.author.principal,
            &e.wiki,
            &p.id,
            edit("One", "pages/a.md", "v4"),
        ));
        assert_eq!(
            q,
            QuotaRefusal {
                limit: QuotaLimit::RevisionsPerProposal,
                allowed: 3,
                current: 3,
                asked: 1,
                reset: Reset::Never,
            }
        );
        assert_eq!(
            e.service
                .revisions(&e.author.principal, &e.wiki, &p.id)
                .unwrap()
                .len(),
            3
        );
        // Another proposal has its own.
        let other = e.held(&e.author, "Two", "pages/b.md");
        assert!(
            e.service
                .revise(
                    &e.author.principal,
                    &e.wiki,
                    &other.id,
                    edit("Two", "pages/b.md", "v2")
                )
                .is_ok()
        );
    });
}

fn accepted(e: &Env, bytes: u64, at: i64, failed: bool) {
    let proposal = e.held(&e.author, "Accepted", &format!("pages/q{at}.md"));
    let a = AcceptanceRecord {
        id: AcceptanceId::new(),
        proposal: proposal.id,
        revision: 1,
        wiki: e.wiki.id.clone(),
        view: "heads/main".into(),
        acceptor: e.maintainer.id.clone(),
        acceptor_role: Role::Maintainer,
        policy_version: "default-1".into(),
        proposer: e.author.id.clone(),
        digest: "d".repeat(64),
        bytes,
        created_at: at,
    };
    e.store.begin_acceptance(&a).unwrap();
    if failed {
        e.store
            .finish_acceptance(&AcceptanceResult {
                acceptance: a.id,
                outcome: AcceptanceOutcome::Failed {
                    word: "wiki-busy".into(),
                },
                at,
            })
            .unwrap();
    }
}

#[test]
fn accepted_bytes_refuse_what_does_not_fit_in_the_day_and_say_when_it_will() {
    let config = QuotaConfig {
        accepted_bytes_per_day: 1000,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        let (w, p, q) = (&e.wiki.id, &e.author.id, e.service.quotas());
        set_now(T0);
        accepted(e, 600, T0 - 3000, false);
        accepted(e, 300, T0 - 2000, false);
        accepted(e, 5000, T0 - 1000, true); // failed: took nothing in
        // 900 are in. 100 more fit exactly; 101 do not.
        assert!(q.check_accepted_bytes(w, p, 100).is_ok());
        let r = refusal(q.check_accepted_bytes(w, p, 101));
        assert_eq!(
            r,
            QuotaRefusal {
                limit: QuotaLimit::AcceptedBytesPerDay,
                allowed: 1000,
                current: 900,
                asked: 101,
                // Once the 600 leave (a day after they were accepted), 300 + 101 fit.
                reset: Reset::At(T0 - 3000 + DAY),
            }
        );
        // More than a day's worth in one go can never fit: waiting does not help.
        assert_eq!(
            refusal(q.check_accepted_bytes(w, p, 1001)).reset,
            Reset::Never
        );
        // A request that needs both to age out resets when the second does.
        assert_eq!(
            refusal(q.check_accepted_bytes(w, p, 800)).reset,
            Reset::At(T0 - 2000 + DAY)
        );
        // The window is rolling: a day later the first is out.
        set_now(T0 - 3000 + DAY);
        assert!(q.check_accepted_bytes(w, p, 700).is_ok());
        assert!(q.check_accepted_bytes(w, p, 701).is_err());
        // It is one person's quota in one wiki.
        assert!(q.check_accepted_bytes(w, &e.reviewer.id, 1000).is_ok());
    });
}

#[test]
fn the_acceptance_queue_is_bounded_and_visible_in_the_refusal() {
    on_both_with(&QuotaConfig::default(), |e| {
        let q = e.service.quotas();
        assert!(q.check_queue(63).is_ok());
        assert_eq!(
            refusal(q.check_queue(64)),
            QuotaRefusal {
                limit: QuotaLimit::AcceptanceQueue,
                allowed: 64,
                current: 64,
                asked: 1,
                reset: Reset::WhenTheQueueMoves,
            }
        );
    });
}

#[test]
fn a_new_account_has_half_of_every_per_principal_limit_for_its_first_week() {
    let config = QuotaConfig {
        open_proposals: 4,
        revisions_per_proposal: 4,
        accepted_bytes_per_day: 1000,
        proposals_per_hour_per_principal: 4,
        proposals_per_hour_per_wiki: 100,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        set_now(T0);
        let fresh = e.person_created(Some(Role::Contributor), T0 - 3600);
        let old = e.person_created(Some(Role::Contributor), T0 - 8 * DAY);
        // Open proposals: 2 for the new account, 4 for the old one.
        create(e, &fresh, 0).unwrap();
        create(e, &fresh, 1).unwrap();
        let q = refusal(create(e, &fresh, 2));
        assert_eq!(
            (q.limit, q.allowed, q.current),
            (QuotaLimit::OpenProposals, 2, 2)
        );
        for n in 10..14 {
            create(e, &old, n).unwrap();
        }
        assert_eq!(refusal(create(e, &old, 14)).allowed, 4);
        // Revisions: 2 for the new account (the first is revision 1, so one more).
        let fresh2 = e.person_created(Some(Role::Contributor), T0 - 3600);
        let p = e.held(&fresh2, "Mine", "pages/m.md");
        e.service
            .revise(
                &fresh2.principal,
                &e.wiki,
                &p.id,
                edit("Mine", "pages/m.md", "v2"),
            )
            .unwrap();
        let q = refusal(e.service.revise(
            &fresh2.principal,
            &e.wiki,
            &p.id,
            edit("Mine", "pages/m.md", "v3"),
        ));
        assert_eq!(
            (q.limit, q.allowed, q.current),
            (QuotaLimit::RevisionsPerProposal, 2, 2)
        );
        // Accepted bytes: 500 against 1000.
        let quotas = e.service.quotas();
        assert!(
            quotas
                .check_accepted_bytes(&e.wiki.id, &fresh.id, 500)
                .is_ok()
        );
        assert_eq!(
            refusal(quotas.check_accepted_bytes(&e.wiki.id, &fresh.id, 501)).allowed,
            500
        );
        assert!(
            quotas
                .check_accepted_bytes(&e.wiki.id, &old.id, 1000)
                .is_ok()
        );
        // After the first week the account counts as old.
        set_now(T0 + 7 * DAY);
        let q = e.service.quotas();
        assert!(q.check_accepted_bytes(&e.wiki.id, &fresh.id, 1000).is_ok());
    });
}

#[test]
fn half_of_one_is_one_not_nothing() {
    let config = QuotaConfig {
        open_proposals: 1,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        let fresh = e.person_created(Some(Role::Contributor), T0 - 10);
        create(e, &fresh, 0).unwrap();
        assert_eq!(refusal(create(e, &fresh, 1)).allowed, 1);
    });
}

#[test]
fn a_refusal_reads_plainly_in_the_operators_log() {
    let q = ProposalError::Quota(QuotaRefusal {
        limit: QuotaLimit::OpenProposals,
        allowed: 20,
        current: 20,
        asked: 1,
        reset: Reset::WhenAProposalCloses,
    });
    assert_eq!(
        q.to_string(),
        "refused: open-proposals (allowed 20, now 20, asked 1)"
    );
}
