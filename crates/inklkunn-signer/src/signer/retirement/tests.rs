#![allow(clippy::expect_used, clippy::unwrap_used)]
//! What a retired key's failed `verify` is excused for, and what it is not: every row of the judgement, with no
//! repository. The end to end rows (a real retirement, a real planted second failure) are in `tests/accept.rs`.

use std::collections::{HashMap, HashSet};

use super::*;

fn id(c: char) -> String {
    c.to_string().repeat(64)
}

fn report(value: serde_json::Value) -> Verification {
    serde_json::from_value(value).unwrap()
}

/// What `verify --format json` prints after a retirement (measured on prikk 0.55.0): every stage evaluated, the
/// one failed condition publication trust.
fn trust_only() -> Verification {
    report(serde_json::json!({
        "schema_version": "verify-report-v1",
        "verdict": {"ok": false, "failed_conditions": [
            {"id": "publication-trust", "message": "repository has publication-trust issues"}
        ]},
        "stages": [
            {"stage": "objects", "status": "evaluated"},
            {"stage": "refs", "status": "evaluated"}
        ]
    }))
}

/// `verify`'s text for blocks `a` and `b`, their ref states and ref updates.
fn text(lines: &[(&str, char)], issues: usize) -> String {
    let mut out = format!("publication trust issues: {issues}\n");
    for (kind, c) in lines {
        out.push_str(&format!(
            "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: {kind} {} has no trusted MAINTAINER signature\n",
            id(*c)
        ));
    }
    out.push_str("error: repository has publication-trust issues\n");
    out
}

fn known(blocks: &[char]) -> HashSet<String> {
    blocks.iter().map(|c| id(*c)).collect()
}

/// Blocks `a`, `b` (known) and `c` (newer), with ref states `1`, `2` and `3` after them.
fn states() -> HashMap<String, String> {
    HashMap::from([(id('1'), id('a')), (id('2'), id('b')), (id('3'), id('c'))])
}

#[test]
fn what_prikk_prints_about_publication_trust_is_read_strictly() {
    let t = parse_publication_trust(&text(
        &[
            ("block", 'a'),
            ("ref-state", '1'),
            ("ref-update", '9'),
            ("block", 'b'),
        ],
        4,
    ));
    assert_eq!(t.blocks, [id('a'), id('b')]);
    assert_eq!(t.ref_states, [id('1')]);
    assert_eq!(t.ref_updates, [id('9')]);
    assert_eq!(t.issues, Some(4));
    assert_eq!(t.unrecognised, 0);
    // A line of another shape is counted, never guessed at: another kind, a short id, upper case, other words.
    let odd = [
        format!(
            "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: tag {} has no trusted MAINTAINER signature",
            id('a')
        ),
        "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: block abc has no trusted MAINTAINER signature"
            .to_owned(),
        format!(
            "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: block {} has no trusted MAINTAINER signature",
            "A".repeat(64)
        ),
        format!(
            "publication-trust [PRIKK-TRUST-OTHER]: block {} has no trusted MAINTAINER signature",
            id('a')
        ),
        format!(
            "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: block {} is revoked",
            id('a')
        ),
    ];
    let t = parse_publication_trust(&odd.join("\n"));
    assert_eq!(t.unrecognised, odd.len());
    assert!(t.blocks.is_empty() && t.ref_states.is_empty() && t.ref_updates.is_empty());
    assert_eq!(t.issues, None);
}

#[test]
fn a_retired_keys_blocks_and_their_states_are_all_that_is_excused() {
    let t = parse_publication_trust(&text(
        &[
            ("block", 'a'),
            ("block", 'b'),
            ("ref-state", '1'),
            ("ref-state", '2'),
            ("ref-update", '8'),
            ("ref-update", '9'),
        ],
        6,
    ));
    assert_eq!(
        judge(&trust_only(), &t, &known(&['a', 'b']), &states(), 1),
        Judgement::Explained { issues: 6 }
    );
}

#[test]
fn a_block_that_no_retired_key_could_have_sealed_is_not_excused() {
    // `c` is newer than any retired key's recorded point.
    let t = parse_publication_trust(&text(&[("block", 'a'), ("block", 'c')], 2));
    assert!(matches!(
        judge(&trust_only(), &t, &known(&['a', 'b']), &states(), 1),
        Judgement::Unexplained(why) if why.contains("not sealed before a retired key was replaced")
    ));
    // ... and a retirement that recorded no point excuses nothing.
    let t = parse_publication_trust(&text(&[("block", 'a')], 1));
    assert!(matches!(
        judge(&trust_only(), &t, &known(&[]), &states(), 1),
        Judgement::Unexplained(_)
    ));
}

#[test]
fn a_ref_state_after_a_newer_block_is_not_excused_and_unmatched_ones_are_bounded_by_the_views() {
    let t = parse_publication_trust(&text(&[("block", 'a'), ("ref-state", '3')], 2));
    assert!(matches!(
        judge(&trust_only(), &t, &known(&['a']), &states(), 1),
        Judgement::Unexplained(why) if why.contains("newer than a retired key's point")
    ));
    // A state that follows no block (a branch's publication): one per view, not more.
    let t = parse_publication_trust(&text(
        &[("block", 'a'), ("ref-state", '7'), ("ref-state", '8')],
        3,
    ));
    assert_eq!(
        judge(&trust_only(), &t, &known(&['a']), &states(), 2),
        Judgement::Explained { issues: 3 }
    );
    assert!(matches!(
        judge(&trust_only(), &t, &known(&['a']), &states(), 1),
        Judgement::Unexplained(why) if why.contains("than the wiki has views")
    ));
    // More ref updates than ref states is not the shape of a publication.
    let t = parse_publication_trust(&text(
        &[("block", 'a'), ("ref-update", '8'), ("ref-update", '9')],
        3,
    ));
    assert!(matches!(
        judge(&trust_only(), &t, &known(&['a']), &states(), 1),
        Judgement::Unexplained(why) if why.contains("more ref updates")
    ));
}

#[test]
fn any_other_failure_in_the_report_or_the_text_is_not_excused() {
    let t = parse_publication_trust(&text(&[("block", 'a')], 1));
    let ok = |r: &Verification| judge(r, &t, &known(&['a']), &states(), 1);
    assert!(matches!(ok(&trust_only()), Judgement::Explained { .. }));
    // A second failed condition.
    let two = report(serde_json::json!({
        "schema_version": "verify-report-v1",
        "verdict": {"ok": false, "failed_conditions": [
            {"id": "publication-trust", "message": "m"},
            {"id": "object-integrity", "message": "m"}
        ]},
        "stages": [{"stage": "objects", "status": "evaluated"}]
    }));
    assert!(
        matches!(ok(&two), Judgement::Unexplained(why) if why.contains("besides publication trust"))
    );
    // A failed condition that is not publication trust at all.
    let other = report(serde_json::json!({
        "schema_version": "verify-report-v1",
        "verdict": {"ok": false, "failed_conditions": [{"id": "object-integrity", "message": "m"}]},
        "stages": [{"stage": "objects", "status": "evaluated"}]
    }));
    assert!(matches!(ok(&other), Judgement::Unexplained(_)));
    // A report that did not run to the end, one with no stages, one that passes.
    let partial = report(serde_json::json!({
        "schema_version": "verify-report-v1",
        "verdict": {"ok": false, "failed_conditions": [{"id": "publication-trust", "message": "m"}]},
        "stages": [{"stage": "objects", "status": "halted"}]
    }));
    assert!(matches!(ok(&partial), Judgement::Unexplained(why) if why.contains("did not run")));
    let empty = report(serde_json::json!({
        "schema_version": "verify-report-v1",
        "verdict": {"ok": false, "failed_conditions": [{"id": "publication-trust", "message": "m"}]},
        "stages": []
    }));
    assert!(matches!(ok(&empty), Judgement::Unexplained(_)));
    let passing = report(serde_json::json!({
        "schema_version": "verify-report-v1",
        "verdict": {"ok": true, "failed_conditions": []},
        "stages": [{"stage": "objects", "status": "evaluated"}]
    }));
    assert!(
        matches!(ok(&passing), Judgement::Unexplained(why) if why.contains("nothing to explain"))
    );
    // The text names a finding that is not of the known shape, or names none, or a different number than it counts.
    let odd = parse_publication_trust(&format!(
        "publication trust issues: 2\npublication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: block {} has no trusted MAINTAINER signature\npublication-trust [PRIKK-TRUST-REVOKED-WHATEVER]: block {} is odd\n",
        id('a'),
        id('b')
    ));
    assert!(matches!(
        judge(&trust_only(), &odd, &known(&['a', 'b']), &states(), 1),
        Judgement::Unexplained(why) if why.contains("known shape")
    ));
    let none = parse_publication_trust("publication trust issues: 0\n");
    assert!(matches!(
        judge(&trust_only(), &none, &known(&['a']), &states(), 1),
        Judgement::Unexplained(why) if why.contains("no publication-trust finding")
    ));
    let miscounted = parse_publication_trust(&text(&[("block", 'a')], 3));
    assert!(matches!(
        judge(&trust_only(), &miscounted, &known(&['a']), &states(), 1),
        Judgement::Unexplained(why) if why.contains("number prikk counts")
    ));
}
