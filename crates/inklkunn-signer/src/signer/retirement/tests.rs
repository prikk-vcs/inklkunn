#![allow(clippy::expect_used, clippy::unwrap_used)]
//! What a retirement's recorded findings excuse, and what they do not: every row of the judgement, with no
//! repository. The end to end rows (a real retirement, a rollback, a branch, a third key) are in `tests/accept.rs`.

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

/// `verify`'s text for the findings listed (kind and a character that makes the id), with the count prikk prints.
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

fn trust(lines: &[(&str, char)]) -> PublicationTrust {
    parse_publication_trust(&text(lines, lines.len()))
}

fn ids(cs: &[char]) -> Vec<String> {
    cs.iter().map(|c| id(*c)).collect()
}

/// A retirement of `key` that recorded blocks `a` and `b`, ref states `1` and `2`, and ref updates `8` and `9`.
fn recorded_by(key: &str) -> (String, Excused) {
    (
        key.to_owned(),
        Excused {
            blocks: ids(&['a', 'b']),
            ref_states: ids(&['1', '2']),
            ref_updates: ids(&['8', '9']),
        },
    )
}

fn judged(report: &Verification, t: &PublicationTrust, sets: &[(String, Excused)]) -> Judgement {
    let recorded: Vec<(&str, &Excused)> = sets.iter().map(|(k, e)| (k.as_str(), e)).collect();
    judge(report, t, &recorded)
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
    assert_eq!(t.issues_lines, 1);
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
fn exactly_one_count_line_is_read_and_two_are_refused_whichever_comes_last() {
    // P14: the last one used to win, so a count line planted after prikk's own could make the numbers agree.
    let one = text(&[("block", 'a')], 1);
    let t = parse_publication_trust(&one);
    assert_eq!((t.issues, t.issues_lines, t.unrecognised), (Some(1), 1, 0));
    let two = format!("{one}publication trust issues: 1\n");
    let t = parse_publication_trust(&two);
    assert_eq!((t.issues, t.issues_lines), (None, 2));
    assert!(t.unrecognised > 0);
    let planted_first = format!("publication trust issues: 1\n{one}");
    assert_eq!(parse_publication_trust(&planted_first).issues, None);
    // ... and a report with two such lines, or none, is not explained, even with everything else in order.
    let sets = [recorded_by("old")];
    for (what, bad) in [("two", two), ("planted first", planted_first)] {
        assert!(
            matches!(
                judged(&trust_only(), &parse_publication_trust(&bad), &sets),
                Judgement::Unexplained(_)
            ),
            "{what}"
        );
    }
    let none = parse_publication_trust(
        "publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: block aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa has no trusted MAINTAINER signature\n",
    );
    assert!(matches!(
        judged(&trust_only(), &none, &sets),
        Judgement::Unexplained(why) if why.contains("exactly one")
    ));
}

#[test]
fn what_the_retirement_recorded_is_all_that_is_excused() {
    let t = trust(&[
        ("block", 'a'),
        ("block", 'b'),
        ("ref-state", '1'),
        ("ref-state", '2'),
        ("ref-update", '8'),
        ("ref-update", '9'),
    ]);
    let sets = [recorded_by("old-key")];
    let Judgement::Explained { named, keys } = judged(&trust_only(), &t, &sets) else {
        panic!("everything named is recorded");
    };
    assert_eq!(named.len(), 6);
    assert_eq!(keys, ["old-key"]);
    // Less than the record is fine too: the failure need not name all of it.
    let t = trust(&[("block", 'a')]);
    assert!(matches!(
        judged(&trust_only(), &t, &sets),
        Judgement::Explained { .. }
    ));
}

#[test]
fn a_block_the_retirement_did_not_record_is_not_excused_wherever_it_sits_in_the_history() {
    // `c` is a block at any position: sealed by a third key before the rotation, or after it, or on a branch.
    // Nothing is excused by position (P2′).
    let sets = [recorded_by("old")];
    for lines in [vec![("block", 'a'), ("block", 'c')], vec![("block", 'c')]] {
        let t = trust(&lines);
        assert!(
            matches!(
                judged(&trust_only(), &t, &sets),
                Judgement::Unexplained(why) if why.contains("a block")
            ),
            "{lines:?}"
        );
    }
}

#[test]
fn a_ref_state_or_ref_update_the_retirement_did_not_record_is_not_excused_and_never_by_count() {
    let sets = [recorded_by("old")];
    // A branch published from an old block: one new ref state and one new ref update.
    let t = trust(&[("ref-state", '1'), ("ref-state", '7'), ("ref-update", '8')]);
    assert!(matches!(
        judged(&trust_only(), &t, &sets),
        Judgement::Unexplained(why) if why.contains("ref state")
    ));
    // A rollback of heads/main to an old block: a ref state that was already there (so it is recorded), and a
    // ref update that was not. One unrecorded finding is enough, however many are recorded.
    let t = trust(&[("ref-state", '1'), ("ref-update", '5')]);
    assert!(matches!(
        judged(&trust_only(), &t, &sets),
        Judgement::Unexplained(why) if why.contains("ref update")
    ));
    // The same finding by id, recorded or not, is the whole question: the same count with the recorded id passes.
    let t = trust(&[("ref-state", '1'), ("ref-update", '8')]);
    assert!(matches!(
        judged(&trust_only(), &t, &sets),
        Judgement::Explained { .. }
    ));
    // An id recorded as a block is not a ref state of that id.
    let t = trust(&[("ref-state", 'a')]);
    assert!(matches!(
        judged(&trust_only(), &t, &sets),
        Judgement::Unexplained(_)
    ));
}

#[test]
fn a_retirement_with_no_record_excuses_nothing_and_two_retirements_add_up() {
    let t = trust(&[("block", 'a')]);
    assert!(matches!(
        judged(&trust_only(), &t, &[]),
        Judgement::Unexplained(_)
    ));
    let empty = [("old".to_owned(), Excused::default())];
    assert!(matches!(
        judged(&trust_only(), &t, &empty),
        Judgement::Unexplained(_)
    ));
    // The second retirement records what the first had not: both are excused together, by their own record each.
    let first = recorded_by("first");
    let second = (
        "second".to_owned(),
        Excused {
            blocks: ids(&['c']),
            ..Excused::default()
        },
    );
    let t = trust(&[("block", 'a'), ("block", 'c')]);
    let Judgement::Explained { keys, .. } = judged(&trust_only(), &t, &[first, second]) else {
        panic!("recorded by two retirements");
    };
    assert_eq!(keys, ["first", "second"]);
}

#[test]
fn what_a_retirement_records_is_the_blocks_the_retired_key_signed_and_the_new_ref_findings() {
    let t = trust(&[
        ("block", 'a'),
        ("block", 'b'),
        ("block", 'c'),
        ("ref-state", '1'),
        ("ref-update", '8'),
    ]);
    // The verify before listed `a` and `b` as the retired key's (and `c` as another key's, and had no line for `d`):
    // what the retirement kept is the blocks the retired key signed.
    let signed_by_key = ids(&['a', 'b', 'd']);
    let (record, left_out) = record_of(&t, &signed_by_key, &[]);
    assert_eq!(record.blocks, ids(&['a', 'b']));
    assert_eq!(left_out, 1, "the block another key signed is left out");
    assert_eq!(record.ref_states, ids(&['1']));
    assert_eq!(record.ref_updates, ids(&['8']));
    // A block that no line lists is not the retired key's either.
    let (record, left_out) = record_of(&t, &[], &[]);
    assert!(record.blocks.is_empty());
    assert_eq!(left_out, 3);
    // What an earlier retirement recorded is not recorded again.
    let earlier = [(
        "first",
        &Excused {
            blocks: ids(&['c']),
            ref_states: ids(&['1']),
            ref_updates: vec![],
        },
    )];
    let (record, left_out) = record_of(&t, &signed_by_key, &earlier);
    assert_eq!((record.blocks, left_out), (ids(&['a', 'b']), 0));
    assert!(record.ref_states.is_empty());
    assert_eq!(record.ref_updates, ids(&['8']));
}

#[test]
fn any_other_failure_in_the_report_or_the_text_is_not_excused() {
    let t = trust(&[("block", 'a')]);
    let sets = [recorded_by("old")];
    let ok = |r: &Verification| judged(r, &t, &sets);
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
        judged(&trust_only(), &odd, &sets),
        Judgement::Unexplained(why) if why.contains("known shape")
    ));
    let none = parse_publication_trust("publication trust issues: 0\n");
    assert!(matches!(
        judged(&trust_only(), &none, &sets),
        Judgement::Unexplained(why) if why.contains("no publication-trust finding")
    ));
    let miscounted = parse_publication_trust(&text(&[("block", 'a')], 3));
    assert!(matches!(
        judged(&trust_only(), &miscounted, &sets),
        Judgement::Unexplained(why) if why.contains("number prikk counts")
    ));
}

#[test]
fn the_most_a_retirement_can_record_is_read_from_the_count_prikk_prints() {
    use crate::writes::checked_publication_trust_records as checked;
    assert_eq!(
        checked("a\nchecked publication trust records: 8\nb\n"),
        Some(8)
    );
    assert_eq!(checked("nothing here\n"), None);
    assert_eq!(
        checked("checked publication trust records: 1\nchecked publication trust records: 2\n"),
        None
    );
    assert_eq!(checked("checked publication trust records: many\n"), None);
}
