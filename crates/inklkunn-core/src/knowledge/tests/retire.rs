//! Retiring a claim and a source (RFC 006 D-10; handoff G-T6): the optional keys of version 1 files, what the acceptance judges, the
//! merge, the summary and the index. A file of version 1 without the new keys reads, and is written, exactly as before.

use super::check::{Mem, files_of, page, sound_claim};
use super::*;
use crate::knowledge::check::{Curated, RETIRED_REASON, Retirement, Touched, validate};
use crate::knowledge::compose::{Curation, Retire, SourceEdit, curate};
use crate::knowledge::freshness::sources_to_review;
use crate::knowledge::index as kix;
use crate::merge::{PageText, merge};
use crate::read::ReadConfig;
use crate::read::testing::World;

fn pages() -> Mem {
    Mem(vec![(
        "pages/One.md".to_owned(),
        page("p1", "Said. ^b1\n\nOther. ^b2"),
    )])
}

fn retired_claim(reason: Option<&str>) -> Claim {
    let mut c = sound_claim(1);
    c.lifecycle = Lifecycle::Retired;
    c.retired_reason = reason.map(str::to_owned);
    c
}

fn retired_source(reason: Option<&str>) -> Source {
    let mut s = Source::new(source_id(1));
    s.identity.title = Some("A history".to_owned());
    s.lifecycle = SourceLife::Retired;
    s.retired_reason = reason.map(str::to_owned);
    s
}

// ------------------------------------------------------------------------------------ the format

#[test]
fn a_file_of_version_1_without_the_new_keys_reads_and_is_written_as_it_was() {
    let claim_text = sound_claim(1).to_yaml();
    assert!(!claim_text.contains("retired_reason"), "{claim_text}");
    let claim = Claim::parse(claim_id(1), &claim_text).unwrap();
    assert_eq!(claim.retired_reason, None);
    assert_eq!(claim.to_yaml(), claim_text);

    let mut source = Source::new(source_id(1));
    source.identity.title = Some("A history".to_owned());
    let source_text = source.to_yaml();
    // A source that is active writes nothing of its life: the text is the 0.3.0 text, byte for byte.
    assert!(!source_text.contains("lifecycle"), "{source_text}");
    assert!(!source_text.contains("retired_reason"), "{source_text}");
    let read = Source::parse(source_id(1), &source_text).unwrap();
    assert_eq!(read.lifecycle, SourceLife::Active);
    assert_eq!(read.retired_reason, None);
    assert_eq!(read.to_yaml(), source_text);
}

#[test]
fn a_retired_claim_and_a_retired_source_are_written_with_their_reason_and_read_back() {
    let claim = retired_claim(Some("The survey was withdrawn."));
    let text = claim.to_yaml();
    assert!(
        text.contains("lifecycle: retired\nretired_reason: \"The survey was withdrawn.\"\n"),
        "{text}"
    );
    assert_eq!(Claim::parse(claim_id(1), &text).unwrap(), claim);

    let source = retired_source(Some("A better edition is cited."));
    let text = source.to_yaml();
    assert!(text.contains("lifecycle: retired\n"), "{text}");
    assert!(
        text.contains("retired_reason: \"A better edition is cited.\"\n"),
        "{text}"
    );
    assert_eq!(Source::parse(source_id(1), &text).unwrap(), source);
}

#[test]
fn a_claim_that_0_3_0_said_was_retired_still_reads_with_no_reason() {
    let text = retired_claim(None).to_yaml();
    let read = Claim::parse(claim_id(1), &text).unwrap();
    assert_eq!(read.lifecycle, Lifecycle::Retired);
    assert_eq!(read.retired_reason, None);
    assert_eq!(read.to_yaml(), text);
}

#[test]
fn a_reason_belongs_to_a_retirement_and_is_one_line() {
    let fault = |text: &str| Claim::parse(claim_id(1), text).unwrap_err();
    let mut active = sound_claim(1).to_yaml();
    active.push_str("retired_reason: \"Why?\"\n");
    // A reason on a claim that is not retired.
    let got = fault(&active);
    assert_eq!((got.rule, got.field), (Rule::Value, "retired_reason"));
    // A reason that is more than a line, or empty.
    for bad in ["a\\nb", "", "   "] {
        let text = retired_claim(None).to_yaml().replace(
            "lifecycle: retired\n",
            &format!("lifecycle: retired\nretired_reason: \"{bad}\"\n"),
        );
        let got = Claim::parse(claim_id(1), &text).unwrap_err();
        assert_eq!(got.field, "retired_reason", "{bad:?}");
    }
    // The same of a source.
    let mut active = Source::new(source_id(1)).to_yaml();
    active.push_str("retired_reason: \"Why?\"\n");
    let got = Source::parse(source_id(1), &active).unwrap_err();
    assert_eq!((got.rule, got.field), (Rule::Value, "retired_reason"));
    let got = Source::parse(
        source_id(1),
        &Source::new(source_id(1))
            .to_yaml()
            .replace("kind:", "lifecycle: nope\nkind:"),
    )
    .unwrap_err();
    assert_eq!((got.rule, got.field), (Rule::Value, "lifecycle"));
}

// ------------------------------------------------------------------------------------ the acceptance

/// What the validator says of one claim file written beside the tip's, in a curation proposal or not.
fn judged(tip: &Claim, now: &Claim, curation: bool) -> Vec<(Rule, &'static str)> {
    let path = claim_path(&now.id);
    let files = files_of(&[
        (path.clone(), now.to_yaml()),
        (
            source_path(&source_id(1)),
            Source::new(source_id(1)).to_yaml(),
        ),
    ]);
    let touched = Touched {
        written: [path.clone()].into(),
        curated_before: [(path, Curated::of(&File::Claim(Box::new(tip.clone()))))].into(),
        curation,
        ..Touched::default()
    };
    validate(&files, &touched, &pages(), &pages())
        .iter()
        .map(|r| (r.rule, r.field))
        .collect()
}

#[test]
fn retiring_is_curations_and_says_why() {
    let tip = sound_claim(1);
    let with = retired_claim(Some("It was withdrawn."));
    // By curation, with a reason: sound.
    assert_eq!(judged(&tip, &with, true), []);
    // Beside anything else: refused as what only curation sets.
    assert_eq!(judged(&tip, &with, false), [(Rule::Status, "lifecycle")]);
    // Without a reason, even by curation.
    assert_eq!(
        judged(&tip, &retired_claim(None), true),
        [(Rule::Missing, RETIRED_REASON)]
    );
    // Taking it back is curation's too.
    assert_eq!(judged(&with, &tip, true), []);
    assert_eq!(judged(&with, &tip, false), [(Rule::Status, "lifecycle")]);
    // A claim a 0.3.0 file said was retired, left as it is by any proposal, is not asked for a reason it never had.
    assert_eq!(
        judged(&retired_claim(None), &retired_claim(None), false),
        []
    );
    // The other three words of a claim's life stay anyone's, as they were.
    let mut taken_up = sound_claim(1);
    taken_up.lifecycle = Lifecycle::Adopted;
    assert_eq!(judged(&tip, &taken_up, false), []);
}

#[test]
fn a_source_is_retired_by_curation_with_a_reason_and_in_no_other_way() {
    let tip = Source::new(source_id(1));
    let with = retired_source(Some("A better edition is cited."));
    let path = source_path(&tip.id);
    let judged = |was: &Source, now: &Source, curation: bool| {
        let files = files_of(&[(path.clone(), now.to_yaml())]);
        let touched = Touched {
            written: [path.clone()].into(),
            curated_before: [(
                path.clone(),
                Curated::of(&File::Source(Box::new(was.clone()))),
            )]
            .into(),
            curation,
            ..Touched::default()
        };
        validate(&files, &touched, &pages(), &pages())
            .iter()
            .map(|r| (r.rule, r.field))
            .collect::<Vec<_>>()
    };
    assert_eq!(judged(&tip, &with, true), []);
    // A source nobody checked still has its retirement curated: it is not a page edit's to say.
    assert_eq!(judged(&tip, &with, false), [(Rule::Status, "lifecycle")]);
    assert_eq!(
        judged(&tip, &retired_source(None), true),
        [(Rule::Missing, RETIRED_REASON)]
    );
    assert_eq!(judged(&with, &tip, true), []);
    assert_eq!(judged(&with, &tip, false), [(Rule::Status, "lifecycle")]);
}

#[test]
fn retirement_is_a_difference_the_removal_rule_knows() {
    let mut bare = Claim::new(claim_id(8));
    bare.lifecycle = Lifecycle::Retired;
    bare.retired_reason = Some("Why.".to_owned());
    let retired = Curated::of(&File::Claim(Box::new(bare)));
    assert_eq!(retired.removal_field(), Some("lifecycle"));
    assert_eq!(
        Curated::of(&File::Claim(Box::new(Claim::new(claim_id(9))))).removal_field(),
        None
    );
    assert!(matches!(
        Retirement::of_source(&retired_source(None)),
        Retirement::Retired { reason: None }
    ));
}

// ------------------------------------------------------------------------------------ the core's curation

#[test]
fn a_curation_retires_with_a_reason_one_line_long_and_takes_it_back() {
    let claim = sound_claim(1);
    let source = Source::new(source_id(1));
    let edit = |retire: Retire, source_retire: Retire| Curation {
        status: claim.community_status.status,
        links: Vec::new(),
        removed: Vec::new(),
        sources: vec![SourceEdit {
            source: source.id.clone(),
            reviewed: String::new(),
            availability: source.availability,
            work: None,
            checked: None,
            retire: source_retire,
        }],
        confirm: false,
        retire,
        proposal: None,
    };
    let made = |retire: Retire, source_retire: Retire| {
        curate(
            &claim,
            std::slice::from_ref(&source),
            None,
            &edit(retire, source_retire),
        )
    };
    // Nothing said: nothing written.
    assert_eq!(made(Retire::Leave, Retire::Leave).unwrap(), []);
    // A reason is needed, and is one line.
    for bad in ["", "  \t "] {
        assert_eq!(
            made(
                Retire::Retire {
                    reason: bad.to_owned()
                },
                Retire::Leave
            ),
            Err(crate::knowledge::compose::Mistake::ReasonMissing)
        );
        assert_eq!(
            made(
                Retire::Leave,
                Retire::Retire {
                    reason: bad.to_owned()
                }
            ),
            Err(crate::knowledge::compose::Mistake::ReasonMissing)
        );
    }
    assert!(matches!(
        made(
            Retire::Retire {
                reason: "a\nb".to_owned()
            },
            Retire::Leave
        ),
        Err(crate::knowledge::compose::Mistake::ControlCharacter(
            "retired_reason"
        ))
    ));
    // Both at once: two files, each as it reads.
    let files = made(
        Retire::Retire {
            reason: "  The survey was withdrawn.  ".to_owned(),
        },
        Retire::Retire {
            reason: "Replaced.".to_owned(),
        },
    )
    .unwrap();
    assert_eq!(files.len(), 2);
    let (claim_text, source_text) = (
        files
            .iter()
            .find(|(p, _)| p == &claim_path(&claim.id))
            .unwrap()
            .1
            .clone(),
        files
            .iter()
            .find(|(p, _)| p == &source_path(&source.id))
            .unwrap()
            .1
            .clone(),
    );
    let retired = Claim::parse(claim.id.clone(), &claim_text).unwrap();
    assert_eq!(retired.lifecycle, Lifecycle::Retired);
    assert_eq!(
        retired.retired_reason.as_deref(),
        Some("The survey was withdrawn.")
    );
    assert_eq!(
        Source::parse(source.id.clone(), &source_text)
            .unwrap()
            .lifecycle,
        SourceLife::Retired
    );
    // Taking it back puts the claim forward again and the source in use, and drops the reasons.
    let back = Curation {
        retire: Retire::Reinstate,
        sources: vec![SourceEdit {
            source: source.id.clone(),
            reviewed: String::new(),
            availability: source.availability,
            work: None,
            checked: None,
            retire: Retire::Reinstate,
        }],
        ..edit(Retire::Leave, Retire::Leave)
    };
    let was_source = Source::parse(source.id.clone(), &source_text).unwrap();
    let files = curate(&retired, std::slice::from_ref(&was_source), None, &back).unwrap();
    let claim_text = files
        .iter()
        .find(|(p, _)| p == &claim_path(&claim.id))
        .unwrap()
        .1
        .clone();
    let source_text = files
        .iter()
        .find(|(p, _)| p == &source_path(&source.id))
        .unwrap()
        .1
        .clone();
    let again = Claim::parse(claim.id.clone(), &claim_text).unwrap();
    assert_eq!(
        (again.lifecycle, again.retired_reason),
        (Lifecycle::Proposed, None)
    );
    assert_eq!(source_text, Source::new(source.id.clone()).to_yaml());
    // Reinstating what is not retired changes nothing.
    assert_eq!(
        curate(&claim, std::slice::from_ref(&source), None, &back).unwrap(),
        []
    );
}

// ------------------------------------------------------------------------------------ the merge, the summary, the index

fn file(c: &Claim) -> Vec<PageText> {
    vec![PageText::new(claim_path(&c.id), c.to_yaml())]
}

#[test]
fn two_sides_that_retire_for_different_reasons_overlap_and_a_reason_never_outlives_its_retirement()
{
    let base = sound_claim(1);
    let theirs = retired_claim(Some("Withdrawn."));
    let ours = retired_claim(Some("Replaced."));
    let out = merge(&file(&base), &file(&theirs), &file(&ours));
    assert!(!out.is_clean(), "{out:?}");
    // The same reason on both sides is one retirement.
    assert!(merge(&file(&base), &file(&theirs), &file(&theirs)).is_clean());
    // One side retires and the other leaves the claim: the retirement is taken.
    assert!(merge(&file(&base), &file(&theirs), &file(&base)).is_clean());
    // One side takes the retirement back and the other rewords it: they cannot be taken together.
    let out = merge(&file(&theirs), &file(&base), &file(&ours));
    assert!(!out.is_clean(), "{out:?}");
    // A source, likewise.
    let source = |s: &Source| vec![PageText::new(source_path(&s.id), s.to_yaml())];
    let (base, theirs, ours) = (
        Source::new(source_id(1)),
        retired_source(Some("Withdrawn.")),
        retired_source(Some("Replaced.")),
    );
    assert!(!merge(&source(&base), &source(&theirs), &source(&ours)).is_clean());
    assert!(merge(&source(&base), &source(&theirs), &source(&base)).is_clean());
}

const ONE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther.\n";

#[test]
fn the_index_says_which_claims_and_sources_are_retired_and_why_and_the_list_to_review_leaves_a_retired_source()
 {
    let mut claim = retired_claim(Some("Withdrawn."));
    for link in &mut claim.sources {
        link.judged_on = digest_of_block("Said. ^b1");
    }
    let other = {
        let mut c = sound_claim(2);
        for link in &mut c.sources {
            link.judged_on = digest_of_block("Said. ^b1");
        }
        c
    };
    let mut active = Source::new(source_id(2));
    active.identity.title = Some("Another".to_owned());
    let mut retired = retired_source(Some("Replaced."));
    retired.id = source_id(1);
    let mut extra = vec![("pages/One.md".to_owned(), ONE.to_owned())];
    for c in [&claim, &other] {
        extra.push((claim_path(&c.id), c.to_yaml()));
    }
    for s in [&retired, &active] {
        extra.push((source_path(&s.id), s.to_yaml()));
    }
    let refs: Vec<(&str, &str)> = extra
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    let w = World::with(
        &refs,
        ReadConfig {
            tip_ttl: std::time::Duration::from_secs(3600),
            ..ReadConfig::default()
        },
    );
    let index = w.service.knowledge_index(&w.wiki, None).expect("an index");
    let row = index.claim(claim.id.as_str()).expect("the retired claim");
    assert_eq!(
        (row.lifecycle.as_str(), row.retired_reason.as_deref()),
        (kix::RETIRED, Some("Withdrawn."))
    );
    let row = index.claim(other.id.as_str()).expect("the other claim");
    assert_ne!(row.lifecycle, kix::RETIRED);
    assert_eq!(row.retired_reason, None);
    let row = index
        .source(retired.id.as_str())
        .expect("the retired source");
    assert_eq!(
        (row.lifecycle.as_str(), row.retired_reason.as_deref()),
        (kix::RETIRED, Some("Replaced."))
    );
    assert_eq!(
        index.source(active.id.as_str()).unwrap().lifecycle,
        "active"
    );
    // A retired claim is not offered beside its paragraph; the other, said at the same place, is. Its anchor is still stated.
    let offered: Vec<&str> = index
        .claims_on_page("p1")
        .map(|a| a.claim.as_str())
        .collect();
    assert_eq!(offered, [other.id.as_str()]);
    assert_eq!(index.anchors_of(claim.id.as_str()).count(), 1);
    // Nobody checked either source; the one in use is to be reviewed, the retired one is not, and it keeps its link.
    let listed: Vec<String> = sources_to_review(&index, 1_800_000_000, 1096)
        .into_iter()
        .map(|s| s.source.id)
        .collect();
    assert_eq!(listed, [active.id.to_string()]);
    assert_eq!(index.links_of(claim.id.as_str()).count(), 1);
}

#[test]
fn a_retirement_has_its_lines_on_the_proposal_page_and_nothing_goes_unsaid() {
    use crate::knowledge::summary::{ClaimChange, Item, SourceChange, summarize, undescribed};
    let tip = sound_claim(1);
    let now = retired_claim(Some("Withdrawn."));
    let path = claim_path(&tip.id);
    let tip_text = tip.to_yaml();
    let before = |p: &str| (p == path).then(|| tip_text.clone());
    let files = vec![(path.clone(), Some(now.to_yaml()))];
    let items = summarize(&files, &before);
    let [Item::ClaimChanged { changes, .. }] = items.as_slice() else {
        panic!("{items:?}");
    };
    assert!(changes.contains(&ClaimChange::Lifecycle(Lifecycle::Retired)));
    assert!(changes.contains(&ClaimChange::RetiredReason(Some("Withdrawn.".to_owned()))));
    assert!(undescribed(&files, &before).is_empty());
    // A source: its life and its reason.
    let source = Source::new(source_id(1));
    let (spath, stext) = (source_path(&source.id), source.to_yaml());
    let sbefore = |p: &str| (p == spath).then(|| stext.clone());
    let files = vec![(
        spath.clone(),
        Some(retired_source(Some("Replaced.")).to_yaml()),
    )];
    let items = summarize(&files, &sbefore);
    let [Item::SourceChanged { changes, .. }] = items.as_slice() else {
        panic!("{items:?}");
    };
    assert!(changes.contains(&SourceChange::Lifecycle(SourceLife::Retired)));
    assert!(changes.contains(&SourceChange::RetiredReason(Some("Replaced.".to_owned()))));
    assert!(undescribed(&files, &sbefore).is_empty());
}
