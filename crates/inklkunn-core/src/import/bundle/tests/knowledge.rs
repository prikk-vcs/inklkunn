//! The claim and source files of a bundle (Z-1 of the 0.5.0 readiness review): they arrive **unsettled**, after the pages they anchor to,
//! each after the files it names, in as many changes as the caps of one acceptance need; the plan counts what is left out and says so.
//! No test here needs prikk (the replay's are in `tests/import_replay_knowledge.rs`).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::knowledge::{
    Availability, Claim, ClaimId, CommunityStatus, Expression, Link, Relation, RelationType,
    Reviewed, Source, SourceId, SourceLink, Standing, Unsettled, claim_path, source_path,
};

const PROPOSAL: &str = "01930000-aaaa-7000-8000-000000000001";

pub(super) fn claim_id(n: u32) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-{n:012x}")).expect("a claim id")
}

pub(super) fn source_id(n: u32) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-{n:012x}")).expect("a source id")
}

/// A claim anchored to `home#b1`, resting on source `source`, with the status `Accepted` that a curation proposal decided.
fn settled_claim(n: u32, source: u32) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.expressions = vec![Expression::new("home", "b1").expect("an expression")];
    c.sources = vec![SourceLink {
        source: source_id(source),
        relation: Link::Supports,
        bearing: crate::knowledge::Bearing::Direct,
        note: None,
        judged_on: crate::knowledge::block_digest(
            inklkunn_markup::blocks("Version 3. ^b1")
                .first()
                .expect("a block"),
        ),
    }];
    c.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some(PROPOSAL.to_owned()),
        judged_on: None,
    };
    c
}

fn checked_source(n: u32) -> Source {
    let mut s = Source::new(source_id(n));
    s.identity.title = Some("A history".to_owned());
    s.availability = Availability::Available;
    s.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: Some(PROPOSAL.to_owned()),
    });
    s
}

const DECLARED: [&str; 6] = [
    "license: CC-BY-SA-4.0",
    "checksums: sha256sums.txt",
    "extensions_used:",
    "  - name: inklkunn-knowledge",
    "    version: \"1\"",
    "",
];

/// A sealed bundle of the page `Home` (three revisions, a block `^b1` in each) and the given knowledge files `(path, text)`.
pub(super) fn with_claims(tag: &str, files: &[(String, String)]) -> Dir {
    let d = dir(tag);
    manifest(&d.0, &DECLARED[..5]);
    let page = |n: u32| format!("---\nid: home\ntitle: Home\n---\n\nVersion {n}. ^b1\n");
    put(&d.0, "pages/Home.md", page(3).as_bytes());
    let lines = [
        record("r1", None, "2025-01-01T00:00:00Z", Some(&page(1))),
        record("r2", Some("r1"), "2025-01-02T00:00:00Z", Some(&page(2))),
        record("r3", Some("r2"), "2025-01-03T00:00:00Z", Some(&page(3))),
    ];
    put(
        &d.0,
        "history/home.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    for (path, text) in files {
        put(&d.0, path, text.as_bytes());
    }
    seal(&d.0);
    d
}

fn files_of(claims: &[Claim], sources: &[Source]) -> Vec<(String, String)> {
    claims
        .iter()
        .map(|c| (claim_path(&c.id), c.to_yaml()))
        .chain(sources.iter().map(|s| (source_path(&s.id), s.to_yaml())))
        .collect()
}

#[test]
fn the_plan_counts_what_only_a_curation_sets_and_says_so_in_the_text_and_the_json() {
    let mut unsettled = settled_claim(2, 1);
    unsettled.community_status = CommunityStatus::undecided();
    let d = with_claims(
        "kn-counts",
        &files_of(&[settled_claim(1, 1), unsettled], &[checked_source(1)]),
    );
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.knowledge_files, 3);
    assert_eq!(
        p.report.knowledge_not_imported,
        Unsettled {
            statuses: 1,
            review_days: 1,
            availabilities: 1,
            ..Unsettled::default()
        }
    );
    let text = p.render();
    for line in [
        "1 community status was not imported: a status is set here only by a curation proposal.",
        "1 review day was not imported: a review day is set here only by a curation proposal.",
        "1 availability was not imported: an availability is set here only by a curation proposal.",
        "A claim or source file that had any of them is written without them; its bytes differ from the bundle's by exactly those keys.",
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
    let json = p.report.to_json();
    assert_eq!(json.pointer("/contents/knowledge_files").unwrap(), 3);
    assert_eq!(
        json.pointer("/contents/knowledge_not_imported").unwrap(),
        &serde_json::json!({"community_statuses": 1, "review_days": 1, "availabilities": 1, "retired_claims": 0, "retired_sources": 0})
    );
}

#[test]
fn a_bundle_of_unsettled_files_says_nothing_of_what_was_left_out() {
    let mut claim = settled_claim(1, 1);
    claim.community_status = CommunityStatus::undecided();
    let d = with_claims("kn-none", &files_of(&[claim], &[Source::new(source_id(1))]));
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.knowledge_files, 2);
    assert!(p.report.knowledge_not_imported.is_none());
    let text = p.render();
    assert!(!text.contains("not imported"), "{text}");
    assert!(!text.contains("written without them"), "{text}");
    assert_eq!(
        p.report
            .to_json()
            .pointer("/contents/knowledge_not_imported")
            .unwrap(),
        &serde_json::json!({"community_statuses": 0, "review_days": 0, "availabilities": 0, "retired_claims": 0, "retired_sources": 0})
    );
}

#[test]
fn the_counts_are_many_and_plural_as_the_words_are() {
    let claims: Vec<Claim> = (1..=3).map(|n| settled_claim(n, 1)).collect();
    let sources: Vec<Source> = vec![checked_source(1)];
    let d = with_claims("kn-plural", &files_of(&claims, &sources));
    let text = plan(&d.0).render();
    assert!(
        text.contains("3 community statuses were not imported: a status is set here only by a curation proposal."),
        "{text}"
    );
    assert!(text.contains("1 review day was not imported"), "{text}");
}

fn retired_claim(n: u32, source: u32) -> Claim {
    let mut c = settled_claim(n, source);
    c.lifecycle = crate::knowledge::Lifecycle::Retired;
    c.retired_reason = Some("Withdrawn.".to_owned());
    c
}

fn retired_source(n: u32) -> Source {
    let mut s = checked_source(n);
    s.lifecycle = crate::knowledge::SourceLife::Retired;
    s.retired_reason = Some("Gone.".to_owned());
    s
}

#[test]
fn a_retired_claim_and_a_retired_source_are_left_out_counted_and_said_in_the_text_and_the_json() {
    // Claim 1 and source 1 are active; claim 2 (resting on source 1) and source 2 (which no claim cites) are retired.
    let d = with_claims(
        "kn-retired",
        &files_of(
            &[settled_claim(1, 1), retired_claim(2, 1)],
            &[checked_source(1), retired_source(2)],
        ),
    );
    let p = plan(&d.0);
    assert_importable(&p);
    // Only the two active files are written; the two retired ones are in no change and in none of the plan's files.
    assert_eq!(p.report.knowledge_files, 2);
    assert_eq!(p.plan.knowledge_count(), 2);
    let order: Vec<String> = p.knowledge_changes.iter().flatten().cloned().collect();
    assert_eq!(
        order,
        [source_path(&source_id(1)), claim_path(&claim_id(1))]
    );
    let retired: Vec<&str> = p
        .retired_knowledge
        .iter()
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(
        retired.iter().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from([
            claim_path(&claim_id(2)).as_str(),
            source_path(&source_id(2)).as_str()
        ])
    );
    // A retired file is counted as retired and as nothing else (its status, its review day, its availability are not counted: the file is
    // not written); the active ones are counted as before.
    assert_eq!(
        p.report.knowledge_not_imported,
        Unsettled {
            statuses: 1,
            review_days: 1,
            availabilities: 1,
            retired_claims: 1,
            retired_sources: 1,
        }
    );
    let text = p.render();
    assert!(
        text.contains(
            "1 retired claim and 1 retired source were not imported: retiring is a curation here."
        ),
        "{text}"
    );
    // They were read like every file of the bundle: the checksums name them and were verified, and they are not listed as a part of the
    // bundle that is not read.
    assert!(
        matches!(p.report.checksums, Checksums::Verified { .. }),
        "{text}"
    );
    assert!(
        p.report.left_out.iter().all(|l| l.what != "knowledge/"),
        "{text}"
    );
    assert_eq!(
        p.report
            .to_json()
            .pointer("/contents/knowledge_not_imported")
            .unwrap(),
        &serde_json::json!({
            "community_statuses": 1,
            "review_days": 1,
            "availabilities": 1,
            "retired_claims": 1,
            "retired_sources": 1
        })
    );
    // Both the digest of what was read and the plan's own are kept for the replay to check the retired files by.
    for f in &p.retired_knowledge {
        assert!(p.file_digests.contains_key(&f.path), "{}", f.path);
    }
}

#[test]
fn the_retired_sentence_is_singular_and_plural_as_the_words_are_and_absent_when_nothing_is_retired()
{
    let says = |tag: &str, claims: &[Claim], sources: &[Source]| {
        let mut all: Vec<Claim> = vec![settled_claim(1, 1)];
        all.extend_from_slice(claims);
        let mut srcs: Vec<Source> = vec![checked_source(1)];
        srcs.extend_from_slice(sources);
        let d = with_claims(tag, &files_of(&all, &srcs));
        let p = plan(&d.0);
        assert_importable(&p);
        p.render()
    };
    let text = says("kn-ret-1", &[retired_claim(2, 1)], &[]);
    assert!(
        text.contains("1 retired claim was not imported: retiring is a curation here."),
        "{text}"
    );
    let text = says("kn-ret-2", &[], &[retired_source(2), retired_source(3)]);
    assert!(
        text.contains("2 retired sources were not imported: retiring is a curation here."),
        "{text}"
    );
    let text = says(
        "kn-ret-3",
        &[retired_claim(2, 1), retired_claim(3, 1)],
        &[retired_source(2)],
    );
    assert!(
        text.contains(
            "2 retired claims and 1 retired source were not imported: retiring is a curation here."
        ),
        "{text}"
    );
    let text = says("kn-ret-0", &[], &[]);
    assert!(!text.contains("retired"), "{text}");
    assert!(!text.contains("retiring"), "{text}");
}

#[test]
fn a_bundle_whose_every_knowledge_file_is_retired_has_no_knowledge_change_and_says_so() {
    let d = with_claims(
        "kn-all-retired",
        &files_of(&[retired_claim(1, 1)], &[retired_source(1)]),
    );
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.knowledge_files, 0);
    assert!(p.knowledge_changes.is_empty());
    assert_eq!(p.retired_knowledge.len(), 2);
    let text = p.render();
    assert!(
        text.contains(
            "1 retired claim and 1 retired source were not imported: retiring is a curation here."
        ),
        "{text}"
    );
    // Nothing was written without keys: the sentence for the written files' bytes is not said.
    assert!(!text.contains("written without them"), "{text}");
}

#[test]
fn the_files_are_ordered_and_cut_without_the_files_that_are_left_out() {
    // 10,000 active sources fill one change exactly; one more source, retired, would have made it two if it had been counted in the cut.
    let mut files: Vec<(String, String)> = (1..=MAX_CHANGE_FILES as u32)
        .map(|n| {
            let s = Source::new(source_id(n));
            (source_path(&s.id), s.to_yaml())
        })
        .collect();
    let retired = retired_source(MAX_CHANGE_FILES as u32 + 1);
    files.push((source_path(&retired.id), retired.to_yaml()));
    let d = with_claims("kn-cut", &files);
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.knowledge_changes.len(), 1, "{}", p.render());
    assert_eq!(
        p.knowledge_changes.first().map(Vec::len),
        Some(MAX_CHANGE_FILES)
    );
    assert!(
        p.knowledge_changes
            .iter()
            .flatten()
            .all(|f| *f != source_path(&retired.id))
    );
    assert_eq!(p.report.knowledge_not_imported.retired_sources, 1);
}

#[test]
fn a_file_that_names_a_retired_file_is_refused_by_name_at_the_plan_and_one_that_is_retired_itself_is_not()
 {
    // Claim 1 rests on retired source 5; claim 2 relates to retired claim 6; source 7 is replaced by retired source 8. Each is refused, by
    // name, for the retired file it names.
    let mut rests = settled_claim(1, 5);
    rests.community_status = CommunityStatus::undecided();
    let mut relates = settled_claim(2, 1);
    relates.community_status = CommunityStatus::undecided();
    relates.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(6),
    }];
    relates.normalize();
    let mut replaced = checked_source(7);
    replaced.superseded_by = Some(source_id(8));
    // Retired files may name each other and anything: they are not written.
    let mut retired_names = retired_claim(6, 5);
    retired_names.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(1),
    }];
    retired_names.normalize();
    let d = with_claims(
        "kn-names-retired",
        &files_of(
            &[rests, relates, retired_names],
            &[
                checked_source(1),
                retired_source(5),
                replaced,
                retired_source(8),
            ],
        ),
    );
    let p = plan(&d.0);
    assert!(!p.is_importable(), "{}", p.render());
    assert_refused(
        &p,
        &claim_path(&claim_id(1)),
        &format!(
            "names `{}`, which is retired and was not imported (retiring is a curation here)",
            source_path(&source_id(5))
        ),
    );
    assert_refused(
        &p,
        &claim_path(&claim_id(2)),
        &format!(
            "names `{}`, which is retired and was not imported",
            claim_path(&claim_id(6))
        ),
    );
    assert_refused(
        &p,
        &source_path(&source_id(7)),
        &format!(
            "names `{}`, which is retired and was not imported",
            source_path(&source_id(8))
        ),
    );
    // Exactly those three: the retired claim 6 names claim 1 and source 5, and is not refused for it.
    assert_eq!(p.plan.refused.len(), 3, "{}", p.render());
}

#[test]
fn a_retired_file_is_read_and_checked_like_every_file_of_the_bundle() {
    let files = files_of(
        &[settled_claim(1, 1), retired_claim(2, 1)],
        &[checked_source(1)],
    );
    // Changed after the checksums were made.
    let d = with_claims("kn-ret-lie", &files);
    let other = retired_claim(2, 1);
    let mut other = other;
    other.retired_reason = Some("Another reason.".to_owned());
    put(&d.0, &claim_path(&claim_id(2)), other.to_yaml().as_bytes());
    assert_refused(
        &plan(&d.0),
        &claim_path(&claim_id(2)),
        "SHA-256 that is not the one the checksums file gives it",
    );
    // Not named by the checksums at all.
    let d = with_claims("kn-ret-unnamed", &files);
    let sums = std::fs::read_to_string(d.0.join("sha256sums.txt")).unwrap();
    let kept: Vec<&str> = sums
        .lines()
        .filter(|l| !l.ends_with(&claim_path(&claim_id(2))))
        .collect();
    std::fs::write(d.0.join("sha256sums.txt"), kept.join("\n") + "\n").unwrap();
    assert_refused(
        &plan(&d.0),
        &claim_path(&claim_id(2)),
        "the checksums file does not name it",
    );
    // A retired file that does not read is refused by the rule, as any file that does not.
    let d = with_claims(
        "kn-ret-unreadable",
        &[(
            claim_path(&claim_id(3)),
            "kind: observation\nlifecycle: retired\n".to_owned(),
        )],
    );
    assert!(!plan(&d.0).is_importable());
}

#[test]
fn the_knowledge_files_are_a_change_of_their_own_after_the_pages_each_file_after_the_files_it_names()
 {
    // Claim 1 relates to claim 2, which rests on source 3; claim 4 rests on source 3 too. The order is the files named first, and the same
    // every time.
    let mut one = settled_claim(1, 3);
    one.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(2),
    }];
    one.normalize();
    let two = settled_claim(2, 3);
    let four = settled_claim(4, 3);
    let files = files_of(&[one, two, four], &[Source::new(source_id(3))]);
    let d = with_claims("kn-order", &files);
    let p = plan(&d.0);
    assert_importable(&p);
    // One change of the source, then one of the claims (a change holds one family, so that what it says of what was left out is one
    // family's sentence).
    assert_eq!(p.knowledge_changes.len(), 2);
    assert_eq!(
        p.knowledge_changes.first().unwrap(),
        &[source_path(&source_id(3))]
    );
    let order: Vec<String> = p.knowledge_changes.iter().flatten().cloned().collect();
    assert_eq!(order.len(), 4);
    let at = |path: String| order.iter().position(|p| *p == path).unwrap();
    assert!(at(source_path(&source_id(3))) < at(claim_path(&claim_id(2))));
    assert!(at(claim_path(&claim_id(2))) < at(claim_path(&claim_id(1))));
    assert!(at(source_path(&source_id(3))) < at(claim_path(&claim_id(4))));
    assert_eq!(plan(&d.0).knowledge_changes, p.knowledge_changes);
    // None of them is in the first change's pages or in the change at the end.
    assert!(p.first_pages.is_empty());
    assert!(
        p.knowledge_changes
            .iter()
            .flatten()
            .all(|f| f.starts_with("knowledge/"))
    );
}

#[test]
fn a_bundle_without_knowledge_files_has_no_knowledge_change() {
    let d = with_history("kn-absent", &three());
    let p = plan(&d.0);
    assert_importable(&p);
    assert!(p.knowledge_changes.is_empty());
}

#[test]
fn the_files_are_cut_into_the_changes_one_acceptance_carries_by_count_and_by_bytes() {
    let names = |n: usize| (0..n).map(|i| format!("f{i:05}")).collect::<Vec<_>>();
    let small = |_: &str| 100;
    assert_eq!(knowledge_groups(&names(0), &small).len(), 0);
    assert_eq!(knowledge_groups(&names(1), &small).len(), 1);
    let at = knowledge_groups(&names(MAX_CHANGE_FILES), &small);
    assert_eq!(at.len(), 1);
    let over = knowledge_groups(&names(MAX_CHANGE_FILES + 1), &small);
    assert_eq!(over.len(), 2);
    assert_eq!(over.first().unwrap().len(), MAX_CHANGE_FILES);
    assert_eq!(over.get(1).unwrap(), &["f10000".to_owned()]);
    // By bytes: two files that together are one byte over the cap are two changes; a file alone over it still goes in one.
    let half = |_: &str| MAX_CHANGE_BYTES / 2 + 1;
    assert_eq!(knowledge_groups(&names(2), &half).len(), 2);
    let huge = |_: &str| MAX_CHANGE_BYTES + 1;
    assert_eq!(knowledge_groups(&names(1), &huge).len(), 1);
    // Every file is in exactly one change, in the order given.
    let all: Vec<String> = over.into_iter().flatten().collect();
    assert_eq!(all, names(MAX_CHANGE_FILES + 1));
}

#[test]
fn the_order_follows_names_through_a_long_chain_and_survives_a_cycle() {
    // A chain of 5,000 sources each replaced by the next: no recursion to overflow, and the last comes first.
    let mut refs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for i in 0..5_000_u32 {
        refs.insert(format!("s{i:05}"), vec![format!("s{:05}", i + 1)]);
    }
    refs.insert("s05000".to_owned(), Vec::new());
    let order = dependency_order(&refs);
    assert_eq!(order.len(), 5_001);
    assert_eq!(order.first().map(String::as_str), Some("s05000"));
    assert_eq!(order.last().map(String::as_str), Some("s00000"));
    // Two claims that name each other: both are in the order, once.
    let cycle: BTreeMap<String, Vec<String>> = [
        ("a".to_owned(), vec!["b".to_owned()]),
        ("b".to_owned(), vec!["a".to_owned()]),
    ]
    .into();
    assert_eq!(dependency_order(&cycle), ["b".to_owned(), "a".to_owned()]);
    // A name that is no file of the bundle is left to acceptance.
    let loose: BTreeMap<String, Vec<String>> =
        [("a".to_owned(), vec!["nowhere".to_owned()])].into();
    assert_eq!(dependency_order(&loose), ["a".to_owned()]);
}

#[test]
fn a_file_that_names_a_file_the_caps_put_in_a_later_change_is_refused_by_name_at_the_plan() {
    // Claim 1 relates to claims 2 and 3; claim 2 relates back to claim 1; claim 3 rests on source 0, which is replaced by source 1, which is
    // replaced by source 2, and so on for 10,000 sources. The files named first come first: claim 2, then the chain, then claim 3, then
    // claim 1. The boundary of the first change (10,000 files) falls inside the chain, so claim 2 (in the first change) names claim 1 (in
    // the second): no order can fix a cycle, and the plan says so by name instead of failing a replay.
    let relate = |c: &mut Claim, to: &[u32]| {
        c.relations = to
            .iter()
            .map(|n| Relation {
                kind: RelationType::Contradicts,
                claim: claim_id(*n),
            })
            .collect();
        c.normalize();
    };
    let mut one = settled_claim(1, 100);
    let mut two = settled_claim(2, 100);
    let mut three = settled_claim(3, 100);
    one.community_status = CommunityStatus::undecided();
    two.community_status = CommunityStatus::undecided();
    three.community_status = CommunityStatus::undecided();
    relate(&mut one, &[2, 3]);
    relate(&mut two, &[1]);
    let chain = 10_000_u32;
    three.sources = vec![SourceLink {
        source: source_id(1_000_000),
        ..three.sources.first().unwrap().clone()
    }];
    two.sources = Vec::new();
    one.sources = Vec::new();
    let mut files = files_of(&[one, two, three], &[]);
    for n in 0..chain {
        let mut s = Source::new(source_id(1_000_000 + n));
        if n + 1 < chain {
            s.superseded_by = Some(source_id(1_000_000 + n + 1));
        }
        files.push((source_path(&s.id), s.to_yaml()));
    }
    let d = with_claims("kn-cross", &files);
    let p = plan(&d.0);
    // The first cut (10,000 files) holds claim 2 and the first 9,999 sources of the chain: the sources are a change, claim 2 the next; the
    // second cut holds source 0 (a change) and claims 3 and 1 (the next).
    assert_eq!(p.knowledge_changes.len(), 4, "{}", p.render());
    assert_eq!(
        p.knowledge_changes.first().map(Vec::len),
        Some(MAX_CHANGE_FILES - 1)
    );
    assert_refused(
        &p,
        &claim_path(&claim_id(2)),
        &format!(
            "names `{}`, which the caps of one acceptance ({MAX_CHANGE_FILES} files, 512.0 MiB) would write in a later change",
            claim_path(&claim_id(1))
        ),
    );
}
