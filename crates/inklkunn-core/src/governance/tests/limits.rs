//! The platform constitution (D-3) and the approvals warning (D-4), as pure functions.

use super::*;

#[test]
fn a_policy_above_a_limit_is_read_as_if_it_were_at_it_and_says_so() {
    let p = policy("entry: contributor\nrungs:\n  accept: admin\n");
    for (max, now) in [
        (Entry::Questioner, Entry::Questioner),
        (Entry::Observer, Entry::Observer),
        (Entry::None, Entry::None),
    ] {
        let limits = PlatformLimits {
            entry_max: max,
            limit_days: 90,
        };
        let (read, lowered) = p.as_read_under(&limits);
        assert_eq!(read.entry(), now);
        assert_eq!(
            lowered,
            [Lowered::Entry {
                was: Entry::Contributor,
                now
            }]
        );
        let first = lowered.first().expect("one lowered value");
        assert_eq!(first.limit(), Limit::EntryMax);
        assert_eq!(first.field(), "entry");
        // Only the limited value moved.
        assert_eq!(read.rungs(), p.rungs());
        assert_eq!(read.acceptance(), p.acceptance());
        // Idempotent: what is read under a limit is within it.
        let (again, more) = read.as_read_under(&limits);
        assert_eq!(again, read);
        assert!(more.is_empty());
    }
}

#[test]
fn a_policy_within_the_limits_is_read_as_it_is() {
    for entry in [
        Entry::None,
        Entry::Observer,
        Entry::Questioner,
        Entry::Contributor,
    ] {
        let p = policy(&format!("entry: {}\n", entry.word()));
        let (read, lowered) = p.as_read_under(&PlatformLimits::default());
        assert_eq!(read, p);
        assert!(lowered.is_empty());
    }
    let (read, lowered) = Policy::default().as_read_under(&PlatformLimits {
        entry_max: Entry::None,
        limit_days: 1,
    });
    assert_eq!(read, Policy::default());
    assert!(lowered.is_empty());
}

#[test]
fn what_acceptance_refuses_is_exactly_what_reading_lowers() {
    for max in [
        Entry::None,
        Entry::Observer,
        Entry::Questioner,
        Entry::Contributor,
    ] {
        let limits = PlatformLimits {
            entry_max: max,
            limit_days: 90,
        };
        for entry in [
            Entry::None,
            Entry::Observer,
            Entry::Questioner,
            Entry::Contributor,
        ] {
            let text = format!("entry: {}\n", entry.word());
            let refused = validate_policy(&text, &limits).is_err();
            let (_, lowered) = policy(&text).as_read_under(&limits);
            assert_eq!(refused, !lowered.is_empty(), "{entry:?} under {max:?}");
        }
    }
}

#[test]
fn the_entry_words_are_ordered_like_the_ladder() {
    assert!(Entry::None < Entry::Observer);
    assert!(Entry::Observer < Entry::Questioner);
    assert!(Entry::Questioner < Entry::Contributor);
    for entry in Entry::ALL {
        assert_eq!(Entry::parse(entry.word()), Some(*entry));
    }
    assert_eq!(Entry::parse("reviewer"), None);
    // The words are the ladder's own where they overlap.
    for entry in Entry::ALL {
        if let Some(rung) = entry.rung() {
            assert_eq!(rung.as_str(), entry.word());
        }
    }
}

#[test]
fn the_approvals_warning_is_computed_from_the_count_alone() {
    let none = Policy::default();
    let three = policy("acceptance:\n  approvals: 3\n");
    for holders in 0..6 {
        assert!(
            !approvals_unreachable(&none, holders),
            "no approvals needed"
        );
        assert_eq!(
            approvals_unreachable(&three, holders),
            holders < 3,
            "{holders}"
        );
    }
    assert_eq!(
        approvals_warning(&three, 1),
        Some(ApprovalsWarning {
            needed: 3,
            available: 1
        })
    );
    assert_eq!(approvals_warning(&three, 3), None);
    assert_eq!(approvals_warning(&none, 0), None);
    assert!(approvals_unreachable(
        &policy("acceptance:\n  approvals: 5\n"),
        4
    ));
    assert!(!approvals_unreachable(
        &policy("acceptance:\n  approvals: 5\n"),
        5
    ));
}
