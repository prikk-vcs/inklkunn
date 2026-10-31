#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

#[test]
fn the_archive_export_is_flat_because_it_streams_and_the_refresh_follows_verify() {
    // Measured on prikk 0.55.0: 0.4 – 3.5 MiB whatever the store (3 MiB to 200 MiB, 10,000 pages or one
    // attachment). The rule is a flat 16 MiB cost, ceiling 24 MiB, and does not follow the store.
    for store in [0, 3 * MIB, 90 * MIB, 200 * MIB, 4096 * MIB] {
        assert_eq!(WriteVerb::ArchiveExport.cost(store, 0), 16 * MIB, "{store}");
        assert_eq!(
            WriteVerb::ArchiveExport.ceiling(store, 0),
            ceiling_for(16 * MIB)
        );
    }
    // The checkpoint refresh is a `verify` with the repository writable: the same verb, the same rule.
    assert_eq!(
        WriteVerb::of(&["verify", "--format", "json"]),
        Some(WriteVerb::Verify)
    );
    // 2.0 × S on wiki stores and 5.0 × S for one attachment (200 MiB measured at 1000.4 MiB) fit under 5.0 × S.
    assert!(WriteVerb::Verify.cost(200 * MIB, 0) >= 1000 * MIB);
}

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
    let vectors: [(&[&str], WriteVerb); 21] = [
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
        (&["branch", "list", "--all"], WriteVerb::BranchList),
        (
            &["archive", "export", "/x/a.tmp", "--format", "json"],
            WriteVerb::ArchiveExport,
        ),
        (&["archive", "export", "/x/a.tmp"], WriteVerb::ArchiveExport),
        // Recovery's verbs (handoff 004 T5).
        (&["doctor"], WriteVerb::Doctor),
        (&["doctor", "--discard-damaged-commits"], WriteVerb::Doctor),
        (&["unlock"], WriteVerb::Unlock),
        (&["unlock", "--lock", "/x/y", "--yes"], WriteVerb::Unlock),
        (
            &["cat", "--path", "p", "--ref", "heads/main"],
            WriteVerb::Cat,
        ),
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
        // The other archive verbs are not run: only `export` has a rule.
        &["archive", "import", "x"],
        &["archive", "verify", "x"],
        &["archive"],
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
