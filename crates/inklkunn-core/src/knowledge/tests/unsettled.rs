//! What an import does not bring (Z-1 of the 0.5.0 readiness review): the one step that takes out of a claim or a source file what only a
//! curation sets, and what it says of it.

use super::check::sound_claim;
use super::*;
use crate::knowledge::check::{Curated, Retirement};
use crate::knowledge::unsettled::{Unsettled, unsettle};

const PROPOSAL_ID: &str = "01930000-aaaa-7000-8000-000000000001";

fn settled_claim() -> Claim {
    let mut c = sound_claim(1);
    c.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some(PROPOSAL_ID.to_owned()),
        judged_on: Some(digest_of("wording")),
    };
    c.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(9),
    }];
    c.sources.first_mut().expect("a link").note = Some("a note".to_owned());
    c
}

fn checked_source() -> Source {
    let mut s = Source::new(source_id(1));
    s.kind = SourceKind::Secondary;
    s.identity.title = Some("A history".to_owned());
    s.reference = Some(Reference {
        form: ReferenceForm::Url,
        value: "https://example.org/a".to_owned(),
    });
    s.availability = Availability::Available;
    s.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: Some(PROPOSAL_ID.to_owned()),
    });
    s
}

fn left_out(path: &str, text: &str) -> (Option<Vec<u8>>, Unsettled) {
    let done = unsettle(path, text.as_bytes()).expect("a readable file");
    (done.rewritten, done.left_out)
}

#[test]
fn a_claim_with_a_status_is_written_unsettled_and_keeps_everything_else() {
    let claim = settled_claim();
    let (rewritten, counts) = left_out(&claim_path(&claim.id), &claim.to_yaml());
    assert_eq!(
        counts,
        Unsettled {
            statuses: 1,
            review_days: 0,
            availabilities: 0
        }
    );
    let text = String::from_utf8(rewritten.expect("rewritten")).expect("text");
    let back = Claim::parse(claim.id.clone(), &text).expect("a claim");
    assert_eq!(back.community_status, CommunityStatus::undecided());
    // Only the status changed: the anchor, the relation, the sources with their judgements and the kind and life are the bundle's.
    let mut expected = claim.clone();
    expected.community_status = CommunityStatus::undecided();
    assert_eq!(back, expected);
    assert_eq!(back.to_yaml(), text, "the canonical text");
    assert!(!text.contains("accepted"), "{text}");
    assert!(!text.contains(PROPOSAL_ID), "{text}");
}

#[test]
fn a_status_of_not_decided_that_names_a_proposal_is_a_status_too() {
    // Setting a claim back to *not decided* is a decision, and the file says which proposal made it.
    let mut claim = sound_claim(1);
    claim.community_status.proposal = Some(PROPOSAL_ID.to_owned());
    claim.community_status.judged_on = Some(digest_of("wording"));
    let (rewritten, counts) = left_out(&claim_path(&claim.id), &claim.to_yaml());
    assert_eq!(counts.statuses, 1);
    let text = String::from_utf8(rewritten.expect("rewritten")).expect("text");
    assert!(!text.contains(PROPOSAL_ID), "{text}");
}

#[test]
fn an_unsettled_file_keeps_the_bundles_exact_bytes_whatever_they_are() {
    let claim = sound_claim(1);
    // The canonical text, and a text that reads the same and is not canonical (a comment, single quotes).
    let canonical = claim.to_yaml();
    let odd = format!("# a bundle from elsewhere\n{}", canonical.replace('"', "'"));
    for text in [canonical, odd] {
        let (rewritten, counts) = left_out(&claim_path(&claim.id), &text);
        assert_eq!(rewritten, None, "{text}");
        assert!(counts.is_none());
    }
    let mut source = checked_source();
    source.availability = Availability::Unknown;
    source.last_reviewed = None;
    let (rewritten, counts) = left_out(&source_path(&source.id), &source.to_yaml());
    assert_eq!(rewritten, None);
    assert!(counts.is_none());
}

#[test]
fn a_source_that_was_checked_is_written_without_the_review_and_the_availability() {
    let source = checked_source();
    let (rewritten, counts) = left_out(&source_path(&source.id), &source.to_yaml());
    assert_eq!(
        counts,
        Unsettled {
            statuses: 0,
            review_days: 1,
            availabilities: 1
        }
    );
    let text = String::from_utf8(rewritten.expect("rewritten")).expect("text");
    let back = Source::parse(source.id.clone(), &text).expect("a source");
    let mut expected = source.clone();
    expected.availability = Availability::Unknown;
    expected.last_reviewed = None;
    assert_eq!(back, expected);
    assert!(!text.contains("last_reviewed"), "{text}");
    assert!(!text.contains("availability: available"), "{text}");
    assert!(text.contains("A history"), "{text}");
}

#[test]
fn each_of_the_two_keys_of_a_source_is_counted_on_its_own() {
    let mut only_review = checked_source();
    only_review.availability = Availability::Unknown;
    let (_, counts) = left_out(&source_path(&only_review.id), &only_review.to_yaml());
    assert_eq!((counts.review_days, counts.availabilities), (1, 0));

    let mut only_availability = checked_source();
    only_availability.last_reviewed = None;
    only_availability.availability = Availability::Unavailable;
    let (_, counts) = left_out(
        &source_path(&only_availability.id),
        &only_availability.to_yaml(),
    );
    assert_eq!((counts.review_days, counts.availabilities), (0, 1));
}

#[test]
fn what_is_written_is_no_departure_from_a_file_that_was_never_there_and_what_the_bundle_had_is() {
    for (path, text) in [
        (claim_path(&claim_id(1)), settled_claim().to_yaml()),
        (source_path(&source_id(1)), checked_source().to_yaml()),
    ] {
        let before = read_file(&path, text.as_bytes()).expect("read");
        assert!(
            Curated::of(&before)
                .departure(&Curated::baseline(&before))
                .is_some(),
            "the bundle's file sets what only a curation sets: {path}"
        );
        let done = unsettle(&path, text.as_bytes()).expect("unsettled");
        let after = read_file(&path, &done.rewritten.expect("rewritten")).expect("read");
        assert_eq!(
            Curated::of(&after).departure(&Curated::baseline(&after)),
            None,
            "{path}"
        );
    }
}

#[test]
fn a_retirement_is_not_taken_out_and_stays_a_departure() {
    let mut claim = settled_claim();
    claim.lifecycle = Lifecycle::Retired;
    claim.retired_reason = Some("Withdrawn.".to_owned());
    let done = unsettle(&claim_path(&claim.id), claim.to_yaml().as_bytes()).expect("unsettled");
    assert_eq!(done.left_out.statuses, 1);
    let after = read_file(&claim_path(&claim.id), &done.rewritten.expect("rewritten")).expect("read");
    let File::Claim(kept) = &after else {
        panic!("a claim");
    };
    assert_eq!(kept.lifecycle, Lifecycle::Retired, "not un-retired");
    assert_eq!(kept.retired_reason.as_deref(), Some("Withdrawn."));
    assert_eq!(
        Curated::of(&after).departure(&Curated::baseline(&after)),
        Some("lifecycle")
    );

    let mut source = checked_source();
    source.lifecycle = SourceLife::Retired;
    source.retired_reason = Some("Gone.".to_owned());
    let done = unsettle(&source_path(&source.id), source.to_yaml().as_bytes()).expect("unsettled");
    let after =
        read_file(&source_path(&source.id), &done.rewritten.expect("rewritten")).expect("read");
    let File::Source(kept) = &after else {
        panic!("a source");
    };
    assert_eq!(kept.lifecycle, SourceLife::Retired, "not un-retired");
    assert_eq!(
        Curated::of(&after).departure(&Curated::baseline(&after)),
        Some("lifecycle")
    );
    assert!(matches!(
        Curated::of(&after),
        Curated::Source {
            retirement: Retirement::Retired { .. },
            ..
        }
    ));
}

#[test]
fn a_file_that_does_not_read_is_the_formats_fault_and_nothing_is_written() {
    let path = claim_path(&claim_id(1));
    assert_eq!(
        unsettle(&path, b"kind: observation\n").expect_err("not wholly well formed"),
        read_file(&path, b"kind: observation\n").expect_err("the same fault")
    );
    assert!(unsettle(&path, &[0xff, 0xfe]).is_err());
    assert!(unsettle("knowledge/claims/not-an-id.yaml", b"").is_err());
    assert!(unsettle("pages/One.md", b"x").is_err());
}

#[test]
fn the_sentences_say_each_kind_in_its_own_words_one_and_many() {
    assert!(Unsettled::default().sentences().is_empty());
    let one = Unsettled {
        statuses: 1,
        review_days: 1,
        availabilities: 1,
    };
    assert_eq!(
        one.sentences(),
        [
            "1 community status was not imported: a status is set here only by a curation proposal.",
            "1 review day was not imported: a review day is set here only by a curation proposal.",
            "1 availability was not imported: an availability is set here only by a curation proposal.",
        ]
    );
    let many = Unsettled {
        statuses: 12,
        review_days: 2,
        availabilities: 0,
    };
    assert_eq!(
        many.sentences(),
        [
            "12 community statuses were not imported: a status is set here only by a curation proposal.",
            "2 review days were not imported: a review day is set here only by a curation proposal.",
        ]
    );
    assert_eq!(
        Unsettled {
            statuses: 0,
            review_days: 0,
            availabilities: 3
        }
        .sentences(),
        ["3 availabilities were not imported: an availability is set here only by a curation proposal."]
    );
}

#[test]
fn the_summary_is_the_sentence_for_one_kind_a_short_line_for_several_and_always_one_message_line() {
    assert_eq!(
        Unsettled::default().summary(4),
        "Import 4 claim or source file(s)"
    );
    let statuses = Unsettled {
        statuses: 3,
        ..Unsettled::default()
    };
    assert_eq!(
        statuses.summary(9),
        "3 community statuses were not imported: a status is set here only by a curation proposal."
    );
    let all = Unsettled {
        statuses: 999,
        review_days: 999,
        availabilities: 999,
    };
    assert_eq!(
        all.summary(1),
        "999 community statuses, 999 review days and 999 availabilities were not imported: only a curation proposal sets them."
    );
    for counts in [all, statuses, Unsettled::default()] {
        let summary =
            inklkunn_signer::message::Summary::new(&counts.summary(5)).expect("a summary");
        assert!(summary.as_str().chars().count() <= inklkunn_signer::message::SUMMARY_MAX_CHARS);
    }
    assert_eq!(
        Unsettled {
            statuses: 1,
            review_days: 1,
            availabilities: 0
        }
        .summary(2),
        "1 community status and 1 review day were not imported: only a curation proposal sets them."
    );
}

#[test]
fn counts_add_up() {
    let a = Unsettled {
        statuses: 1,
        review_days: 2,
        availabilities: 3,
    };
    assert_eq!(
        a.plus(a),
        Unsettled {
            statuses: 2,
            review_days: 4,
            availabilities: 6
        }
    );
}
