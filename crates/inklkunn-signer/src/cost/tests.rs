#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn the_table_is_bound_to_the_floor() {
    let mut parts = inklkunn_prikk::MIN_PRIKK_VERSION
        .split('.')
        .map(|p| p.parse::<u64>().unwrap_or(u64::MAX));
    let parsed = (
        parts.next().unwrap_or(u64::MAX),
        parts.next().unwrap_or(u64::MAX),
        parts.next().unwrap_or(u64::MAX),
    );
    assert_eq!(
        BOUND_TO, parsed,
        "re-measure the signer's cost table when the floor moves (RFC 010 re-baseline)"
    );
}

#[test]
fn every_verb_the_signer_runs_has_a_rule_and_is_found_by_its_arguments() {
    // The argument vectors the runner is given, one per verb: each is found, none is found twice.
    let vectors: [(&[&str], WriteVerb); 13] = [
        (&["init"], WriteVerb::Init),
        (
            &["trust", "maintainer", "add", "--key-id", "k"],
            WriteVerb::Trust,
        ),
        (
            &["trust", "maintainer", "remove", "--key-id", "k"],
            WriteVerb::Trust,
        ),
        (&["key", "generate", "--out", "x"], WriteVerb::KeyGenerate),
        (&["key", "public", "--seed-file", "x"], WriteVerb::KeyPublic),
        (&["status", "--format", "json"], WriteVerb::Status),
        (
            &["worktree-status", "--format", "json"],
            WriteVerb::WorktreeStatus,
        ),
        (&["log", "--limit", "1"], WriteVerb::Log),
        (&["verify", "--format", "json"], WriteVerb::Verify),
        (&["branch", "switch", "heads/x"], WriteVerb::Switch),
        (&["mv", "a", "b"], WriteVerb::Mv),
        (&["commit", "-m", "m"], WriteVerb::Commit),
        (&["seal", "--allow-no-audit"], WriteVerb::Seal),
    ];
    for (args, verb) in vectors {
        assert_eq!(WriteVerb::of(args), Some(verb), "{args:?}");
    }
    assert_eq!(
        WriteVerb::of(&["branch", "create", "heads/x", "--from", "heads/main"]),
        Some(WriteVerb::BranchCreate)
    );
    let seen: std::collections::HashSet<WriteVerb> = WriteVerb::ALL.into_iter().collect();
    assert_eq!(seen.len(), WriteVerb::ALL.len());
    // A verb nobody costed is not run: not `export`, not `gc`, not a bare `branch`, not nothing.
    for unknown in [
        &["export"][..],
        &["gc"],
        &["branch"],
        &["branch", "delete", "x"],
        &[],
    ] {
        assert_eq!(WriteVerb::of(unknown), None, "{unknown:?}");
    }
}

#[test]
fn a_ceiling_follows_the_store_and_never_goes_below_the_floor() {
    for verb in WriteVerb::ALL {
        let r = verb.rule();
        assert_eq!(verb.cost(0, 0), r.floor, "{verb:?}");
        assert!(verb.ceiling(0, 0) >= r.floor, "{verb:?}");
        // Monotone in the store size.
        assert!(
            verb.ceiling(10 * MIB, 0) <= verb.ceiling(500 * MIB, 0)
                && verb.ceiling(0, 10 * MIB) <= verb.ceiling(0, 500 * MIB),
            "{verb:?}"
        );
        // Headroom over the cost, as the read table has it.
        assert_eq!(
            verb.ceiling(300 * MIB, 0),
            ceiling_for(verb.cost(300 * MIB, 0))
        );
        assert!(
            verb.ceiling(300 * MIB, 0) > verb.cost(300 * MIB, 0),
            "{verb:?}"
        );
    }
    // The two checks that scale with the store hardest are costed at the measured 5.0 × S and 6.0 × S for one
    // large attachment, and the commit of a large file at 7 × its bytes (the measurement of 2026-10-29).
    let big = 300 * MIB;
    assert!(WriteVerb::Verify.cost(big, 0) >= big * 5);
    assert!(WriteVerb::WorktreeStatus.cost(big, 0) >= big * 5);
    assert!(WriteVerb::Switch.cost(big, 0) >= big * 6);
    assert!(WriteVerb::Commit.cost(0, big) >= big * 7);
    assert!(WriteVerb::Commit.cost(big, 0) >= big * 5);
    assert!(WriteVerb::Seal.cost(0, big) >= big * 2);
    assert!(WriteVerb::Seal.cost(big, 0) >= big * 2);
}
