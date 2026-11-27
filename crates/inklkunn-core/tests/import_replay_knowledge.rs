//! The claim and source files of a bundle come in with the wiki **after its history, unsettled** (Z-1 of the 0.5.0 readiness review), with the
//! real signer and a real prikk: an inklkunn wiki's own export can be imported again while it has claims on pages that have a history. The
//! files are written in the change(s) after the last revision and the change at the end of the pages, each after the files it names; **what
//! only a curation sets** (a community status, a review day, an availability) is not written, and the plan, the import and the report agree
//! on how many; **a retired claim or source is left out whole** (Z-6), counted and said, and a file that names one is refused at the plan, by
//! name; a claim that names a page the bundle lacks and a file that changed since the plan stay refused, by name. Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "support/replay.rs"]
mod replay;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use inklkunn_core::import::bundle::{BundleOptions, ImportBundleError, ImportStep, plan_bundle};
use inklkunn_core::knowledge::{
    Availability, Bearing, Claim, ClaimId, CommunityStatus, Expression, File, Lifecycle, Link,
    Relation, RelationType, Reviewed, Source, SourceId, SourceLife, SourceLink, Standing,
    Unsettled, block_digest, claim_path, read_file, source_path,
};
use inklkunn_core::read::ReadConfig;
use inklkunn_prikk::Point;
use inklkunn_store::{AcceptanceOutcome, ActStore, Slug, WikiStore};
use replay::{Dir, dir_in, leftovers, messages, plan_of, prikk_on_path, run, world};
use sha2::{Digest as _, Sha256};

const PROPOSAL: &str = "01930000-aaaa-7000-8000-000000000001";

fn claim_id(n: u32) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-{n:012x}")).expect("a claim id")
}

fn source_id(n: u32) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-{n:012x}")).expect("a source id")
}

fn page(n: u32) -> String {
    format!("---\nid: home\ntitle: Home\n---\n\nVersion {n}. ^b1\n")
}

/// A claim anchored to `home#b1`, resting on `source`, as an inklkunn export writes one: with the status a curation proposal decided.
fn settled_claim(n: u32, source: u32) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.expressions = vec![Expression::new("home", "b1").expect("an expression")];
    c.sources = vec![SourceLink {
        source: source_id(source),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: Some("a note".to_owned()),
        judged_on: block_digest(
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

fn unsettled(mut c: Claim) -> Claim {
    c.community_status = CommunityStatus::undecided();
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

fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn files_of(root: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(at).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                walk(root, &e.path(), out);
            } else {
                out.push(
                    e.path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// `sha256sums.txt` over every file but the manifest and itself.
fn seal(root: &Path) {
    let mut lines = Vec::new();
    for rel in files_of(root) {
        if rel == "wiki-bundle.yaml" || rel == "sha256sums.txt" {
            continue;
        }
        let digest = Sha256::digest(std::fs::read(root.join(&rel)).unwrap());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        lines.push(format!("{hex}  {rel}"));
    }
    std::fs::write(root.join("sha256sums.txt"), lines.join("\n") + "\n").unwrap();
}

fn record(rev: &str, parent: Option<&str>, at: &str, content: &str) -> String {
    serde_json::json!({
        "rev": rev,
        "parent": parent,
        "type": if parent.is_none() { "create" } else { "edit" },
        "at": at,
        "author": {"id": "ann", "name": "Ann"},
        "summary": format!("change {rev}"),
        "content": content,
        "sha256": Sha256::digest(content.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>(),
    })
    .to_string()
}

fn manifest(root: &Path, extra: &[&str]) {
    let mut text = String::from(
        "format: portable-wiki-bundle\nversion: '0.4'\nlicense: CC-BY-SA-4.0\nchecksums: sha256sums.txt\nextensions_used:\n  - name: inklkunn-knowledge\n    version: \"1\"\n",
    );
    for line in extra {
        text.push_str(line);
        text.push('\n');
    }
    std::fs::write(root.join("wiki-bundle.yaml"), text).unwrap();
}

/// A sealed bundle of the page `Home`, whose three revisions are its history and whose last has the block `^b1`, with these knowledge files.
/// `with_notes` adds a page that has no history (so the import has a first change).
fn bundle(tag: &str, with_notes: bool, files: &[(String, String)]) -> Dir {
    let d = dir_in(&std::env::temp_dir(), tag);
    manifest(&d.0, &[]);
    put(&d.0, "pages/Home.md", &page(3));
    let lines = [
        record("r1", None, "2025-01-01T00:00:00Z", &page(1)),
        record("r2", Some("r1"), "2025-01-02T00:00:00Z", &page(2)),
        record("r3", Some("r2"), "2025-01-03T00:00:00Z", &page(3)),
    ];
    put(&d.0, "history/home.jsonl", &(lines.join("\n") + "\n"));
    if with_notes {
        put(
            &d.0,
            "pages/Notes.md",
            "---\nid: notes\ntitle: Notes\n---\n\nNotes.\n",
        );
    }
    for (path, text) in files {
        put(&d.0, path, text);
    }
    seal(&d.0);
    d
}

fn claim_file(c: &Claim) -> (String, String) {
    (claim_path(&c.id), c.to_yaml())
}

fn source_file(s: &Source) -> (String, String) {
    (source_path(&s.id), s.to_yaml())
}

/// The blocks the wiki's acts sealed, oldest first.
fn sealed_blocks(w: &replay::World, id: &inklkunn_store::WikiId) -> Vec<String> {
    w.store
        .system_acts(id)
        .expect("acts")
        .into_iter()
        .filter_map(|e| match e.end?.outcome {
            AcceptanceOutcome::Sealed { block_id, .. } => Some(block_id),
            AcceptanceOutcome::Failed { .. } => None,
        })
        .collect()
}

/// The file at `path` as the repository had it at `block`, when it had it.
fn at_block(w: &replay::World, repo: &Path, block: &str, path: &str) -> Option<Vec<u8>> {
    w.storage
        .read(repo, &Point::Block(block.to_owned()), path, 1 << 20)
        .ok()
}

#[test]
fn a_wikis_claims_come_in_after_its_history_unsettled_with_their_sources_and_anchors_and_the_numbers_agree()
 {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-roundtrip");
    let settled = settled_claim(1, 1);
    let calm = unsettled(settled_claim(2, 1));
    let source = checked_source(1);
    let files = [
        claim_file(&settled),
        claim_file(&calm),
        source_file(&source),
    ];
    let d = bundle("kn-roundtrip-bundle", true, &files);
    let plan = plan_of(&d.0, None);
    assert_eq!(
        plan.report.knowledge_not_imported,
        Unsettled {
            statuses: 1,
            review_days: 1,
            availabilities: 1,
            ..Unsettled::default()
        }
    );
    // The dry run says it before anything is written.
    let said = plan.render();
    assert!(
        said.contains(
            "1 community status was not imported: a status is set here only by a curation proposal."
        ),
        "{said}"
    );
    let seen = std::cell::RefCell::new(Vec::new());
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|s| {
        seen.borrow_mut().push(s);
    })
    .expect("a wiki with claims on a page with a history is imported");
    // **The count a report gives is what the replay writes.**
    assert_eq!(
        done.knowledge_not_imported,
        plan.report.knowledge_not_imported
    );
    assert_eq!(done.knowledge_files, 3);
    // A change holds one family: the source first, then the claims.
    assert_eq!(done.knowledge_changes, 2);

    // The order of the changes: the first (the page with no history), the revisions, then the knowledge, last.
    let steps = seen.into_inner();
    let at = |step: ImportStep| steps.iter().position(|s| *s == step).unwrap();
    assert!(at(ImportStep::FirstChange) < at(ImportStep::Revision(1)));
    assert!(at(ImportStep::Revision(3)) < at(ImportStep::Knowledge));
    assert!(at(ImportStep::Knowledge) < at(ImportStep::Verified));
    let blocks = sealed_blocks(&w, &done.record.id);
    assert_eq!(
        blocks.len(),
        1 + 1 + 3 + 2,
        "creation, first, three revisions, the source, the claims"
    );
    assert_eq!(blocks.last(), Some(&done.tip));
    let first = blocks.get(1).unwrap();
    let last = blocks.last().unwrap();
    let sources_change = blocks.get(blocks.len() - 2).unwrap();
    // **The first change carries no knowledge file**, and the last carries all of them.
    for path in files.iter().map(|(p, _)| p) {
        assert_eq!(
            at_block(&w, &done.repo, first, path),
            None,
            "{path} in the first change"
        );
        assert!(
            at_block(&w, &done.repo, last, path).is_some(),
            "{path} at the end"
        );
    }
    // The source is written before the claims that rest on it, and the claims are the last block.
    assert!(at_block(&w, &done.repo, sources_change, &source_path(&source.id)).is_some());
    assert_eq!(
        at_block(&w, &done.repo, sources_change, &claim_path(&settled.id)),
        None
    );
    // The messages: the knowledge changes are the last blocks, imports, and each summary is what that change left out, in words.
    let messages = messages(&w, &done.record.id);
    let said: Vec<&str> = messages.iter().map(|m| m.summary().as_str()).collect();
    assert_eq!(
        said.last().copied(),
        Some(
            "1 community status was not imported: a status is set here only by a curation proposal."
        )
    );
    assert_eq!(
        said.get(said.len() - 2).copied(),
        Some(
            "1 review day and 1 availability were not imported: only a curation proposal sets them."
        )
    );
    assert!(messages.last().unwrap().source().is_none());

    // What the wiki has: the settled claim, unsettled, with its anchor, its source and the rest of the file as it was.
    let tip = |path: &str| std::fs::read(done.repo.join(path)).expect("a file");
    let File::Claim(got) =
        read_file(&claim_path(&settled.id), &tip(&claim_path(&settled.id))).unwrap()
    else {
        panic!("a claim");
    };
    assert_eq!(got.community_status, CommunityStatus::undecided());
    assert_eq!(*got, unsettled(settled.clone()));
    assert_eq!(got.expressions, settled.expressions);
    assert_eq!(got.sources, settled.sources);
    // The source has neither key; its identity is the bundle's.
    let File::Source(got) =
        read_file(&source_path(&source.id), &tip(&source_path(&source.id))).unwrap()
    else {
        panic!("a source");
    };
    assert_eq!(got.last_reviewed, None);
    assert_eq!(got.availability, Availability::Unknown);
    assert_eq!(got.identity.title.as_deref(), Some("A history"));
    // **A file that had none of them keeps the bundle's exact bytes.**
    assert_eq!(
        tip(&claim_path(&calm.id)),
        std::fs::read(d.0.join(claim_path(&calm.id))).unwrap()
    );
    // The rewritten ones are not the bundle's bytes, by exactly those keys.
    let bundle_bytes = std::fs::read_to_string(d.0.join(claim_path(&settled.id))).unwrap();
    let wiki_bytes = String::from_utf8(tip(&claim_path(&settled.id))).unwrap();
    assert!(bundle_bytes.contains("status: accepted") && !wiki_bytes.contains("accepted"));
    assert!(wiki_bytes.contains("status: unknown"));
    // The pages are the bundle's, and the wiki verifies.
    assert_eq!(
        std::fs::read_to_string(done.repo.join("pages/Home.md")).unwrap(),
        page(3)
    );
    assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
    // The report kept with the act has the sentences.
    let kept = std::fs::read_to_string(w.layout.import_report(&done.record.id)).unwrap();
    for line in [
        "1 community status was not imported: a status is set here only by a curation proposal.",
        "1 review day was not imported: a review day is set here only by a curation proposal.",
        "1 availability was not imported: an availability is set here only by a curation proposal.",
        "its bytes differ from the bundle's by exactly those keys.",
    ] {
        assert!(kept.contains(line), "{line}\n{kept}");
    }
}

#[test]
fn one_kind_left_out_gives_the_sentence_itself_as_the_summary_of_the_change() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-summary");
    let files = [
        claim_file(&settled_claim(1, 1)),
        claim_file(&settled_claim(2, 1)),
        source_file(&Source::new(source_id(1))),
    ];
    let d = bundle("kn-summary-bundle", false, &files);
    let plan = plan_of(&d.0, None);
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    assert!(!done.first_change);
    assert_eq!(
        messages(&w, &done.record.id)
            .last()
            .unwrap()
            .summary()
            .as_str(),
        "2 community statuses were not imported: a status is set here only by a curation proposal."
    );
}

#[test]
fn a_bundle_of_knowledge_files_and_an_interwiki_map_alone_still_comes_in() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-only");
    let d = dir_in(&std::env::temp_dir(), "kn-only-bundle");
    manifest(
        &d.0,
        &[
            "interwiki:",
            "  docs: https://docs.example.org/read?page={title}",
        ],
    );
    // No page: a claim that anchors to none, and its source.
    let mut claim = unsettled(settled_claim(1, 1));
    claim.expressions = Vec::new();
    claim.sources = Vec::new();
    for (path, text) in [claim_file(&claim), source_file(&Source::new(source_id(1)))] {
        put(&d.0, &path, &text);
    }
    seal(&d.0);
    let plan = plan_of(&d.0, None);
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    // The first change is the map alone, and the knowledge files come after it.
    assert!(done.first_change);
    assert_eq!((done.knowledge_files, done.knowledge_changes), (2, 2));
    let said: Vec<String> = messages(&w, &done.record.id)
        .iter()
        .map(|m| m.summary().as_str().to_owned())
        .collect();
    assert_eq!(
        said.get(1).map(String::as_str),
        Some("Import the interwiki map")
    );
    assert_eq!(
        said.get(2).map(String::as_str),
        Some("Import 1 claim or source file(s)")
    );
    assert_eq!(
        said.get(3).map(String::as_str),
        Some("Import 1 claim or source file(s)")
    );
    assert_eq!(said.len(), 4);
    assert!(done.repo.join(claim_path(&claim.id)).is_file());
    assert!(done.repo.join("governance/interwiki.yaml").is_file());
    assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
}

/// What an import of a bundle with these knowledge files says when it fails, and that it left nothing.
fn refused_at_the_replay(tag: &str, files: &[(String, String)]) -> String {
    let w = world(tag);
    let d = bundle(&format!("{tag}-bundle"), false, files);
    let plan = plan_of(&d.0, None);
    let Err(error) = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}) else {
        panic!("the import was not refused");
    };
    assert!(matches!(error, ImportBundleError::Failed { .. }), "{error}");
    // **Nothing was made**: no record, and nothing on disk but what a clean world has.
    assert!(w.store.list().expect("list").is_empty());
    assert!(w.store.by_slug(&Slug::new("orchard").unwrap()).is_err());
    assert_eq!(leftovers(&w), Vec::<String>::new(), "{error}");
    error.to_string()
}

#[test]
fn a_claim_that_names_a_page_the_bundle_lacks_is_still_refused_by_name_and_nothing_is_made() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let mut lost = unsettled(settled_claim(1, 1));
    lost.expressions = vec![Expression::new("no-such-page", "b1").expect("an expression")];
    let said = refused_at_the_replay(
        "kn-missing",
        &[claim_file(&lost), source_file(&Source::new(source_id(1)))],
    );
    eprintln!("MISSING PAGE: {said}");
    assert!(said.contains(&claim_path(&lost.id)), "{said}");
    assert!(
        said.contains("the knowledge files of the import are refused"),
        "{said}"
    );
    assert!(said.contains("expressions"), "{said}");
}

fn retired_claim(n: u32, source: u32) -> Claim {
    let mut c = settled_claim(n, source);
    c.lifecycle = Lifecycle::Retired;
    c.retired_reason = Some("Withdrawn.".to_owned());
    c
}

fn retired_source(n: u32) -> Source {
    let mut s = checked_source(n);
    s.lifecycle = SourceLife::Retired;
    s.retired_reason = Some("Gone.".to_owned());
    s
}

/// The summaries of the knowledge changes of an import, in the order they were made (the changes after the history).
fn knowledge_summaries(
    w: &replay::World,
    id: &inklkunn_store::WikiId,
    changes: usize,
) -> Vec<String> {
    let said: Vec<String> = messages(w, id)
        .iter()
        .map(|m| m.summary().as_str().to_owned())
        .collect();
    said.get(said.len() - changes..)
        .unwrap_or_default()
        .to_vec()
}

#[test]
fn a_wiki_with_a_retired_claim_and_a_retired_source_round_trips_without_them_and_says_so() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-retire");
    // Claim 1 and source 1 are active (claim 1 has a status, source 1 a review and an availability); claim 2 (which rests on source 1) and
    // source 2 (which no claim cites) are retired, and they too have the keys that only a curation sets.
    let files = [
        claim_file(&settled_claim(1, 1)),
        claim_file(&retired_claim(2, 1)),
        source_file(&checked_source(1)),
        source_file(&retired_source(2)),
    ];
    let d = bundle("kn-retire-bundle", true, &files);
    let plan = plan_of(&d.0, None);
    let counted = Unsettled {
        statuses: 1,
        review_days: 1,
        availabilities: 1,
        retired_claims: 1,
        retired_sources: 1,
    };
    assert_eq!(plan.report.knowledge_not_imported, counted);
    // The dry run says it before anything is written.
    let said = plan.render();
    assert!(
        said.contains(
            "1 retired claim and 1 retired source were not imported: retiring is a curation here."
        ),
        "{said}"
    );
    let seen = std::cell::RefCell::new(Vec::new());
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|s| {
        seen.borrow_mut().push(s);
    })
    .expect("a wiki that retired a claim and a source is imported");
    // **The count the plan gave is the count the replay made**, and only the two active files were written.
    assert_eq!(done.knowledge_not_imported, counted);
    assert_eq!(done.knowledge_files, 2);
    assert_eq!(done.knowledge_changes, 2);
    assert!(seen.into_inner().contains(&ImportStep::Knowledge));
    // The retired two are in no block of the wiki, at the tip or before it; the active two arrive.
    for block in sealed_blocks(&w, &done.record.id) {
        for path in [claim_path(&claim_id(2)), source_path(&source_id(2))] {
            assert_eq!(
                at_block(&w, &done.repo, &block, &path),
                None,
                "{path} at {block}"
            );
        }
    }
    assert!(!done.repo.join(claim_path(&claim_id(2))).exists());
    assert!(!done.repo.join(source_path(&source_id(2))).exists());
    assert!(done.repo.join(claim_path(&claim_id(1))).is_file());
    assert!(done.repo.join(source_path(&source_id(1))).is_file());
    // Each change says its family's count beside what it left out of the files it wrote.
    assert_eq!(
        knowledge_summaries(&w, &done.record.id, 2),
        [
            "1 review day, 1 availability and 1 retired source were not imported: only a curation proposal sets them.",
            "1 community status and 1 retired claim were not imported: only a curation proposal sets them."
        ]
    );
    // The report kept with the act has the sentence, and the wiki verifies.
    let kept = std::fs::read_to_string(w.layout.import_report(&done.record.id)).unwrap();
    assert!(
        kept.contains(
            "1 retired claim and 1 retired source were not imported: retiring is a curation here."
        ),
        "{kept}"
    );
    assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
}

#[test]
fn a_family_with_no_file_left_to_write_has_its_retired_count_said_by_the_change_of_the_other() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // Every claim is retired: the one change is the sources', and it says the retired claim.
    let w = world("kn-fam-claims");
    let d = bundle(
        "kn-fam-claims-bundle",
        false,
        &[
            claim_file(&retired_claim(1, 1)),
            source_file(&checked_source(1)),
        ],
    );
    let plan = plan_of(&d.0, None);
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    assert_eq!((done.knowledge_files, done.knowledge_changes), (1, 1));
    assert_eq!(
        done.knowledge_not_imported,
        plan.report.knowledge_not_imported
    );
    assert_eq!(
        knowledge_summaries(&w, &done.record.id, 1),
        [
            "1 review day, 1 availability and 1 retired claim were not imported: only a curation proposal sets them."
        ]
    );
    assert!(!done.repo.join(claim_path(&claim_id(1))).exists());

    // Every source is retired (and no claim cites one): the one change is the claims', and it says the retired source.
    let w = world("kn-fam-sources");
    let mut claim = settled_claim(1, 1);
    claim.sources = Vec::new();
    let d = bundle(
        "kn-fam-sources-bundle",
        false,
        &[claim_file(&claim), source_file(&retired_source(1))],
    );
    let plan = plan_of(&d.0, None);
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    assert_eq!((done.knowledge_files, done.knowledge_changes), (1, 1));
    assert_eq!(
        done.knowledge_not_imported,
        plan.report.knowledge_not_imported
    );
    assert_eq!(
        knowledge_summaries(&w, &done.record.id, 1),
        [
            "1 community status and 1 retired source were not imported: only a curation proposal sets them."
        ]
    );
}

#[test]
fn a_bundle_whose_every_knowledge_file_is_retired_makes_no_knowledge_change_and_the_report_says_so()
{
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-all-retired");
    let d = bundle(
        "kn-all-retired-bundle",
        false,
        &[
            claim_file(&retired_claim(1, 1)),
            source_file(&retired_source(1)),
        ],
    );
    let plan = plan_of(&d.0, None);
    let seen = std::cell::RefCell::new(Vec::new());
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|s| {
        seen.borrow_mut().push(s);
    })
    .expect("imported");
    // No change of knowledge files: none to write. The wiki is complete, and the count is the plan's.
    assert_eq!((done.knowledge_files, done.knowledge_changes), (0, 0));
    assert!(!seen.into_inner().contains(&ImportStep::Knowledge));
    assert_eq!(
        done.knowledge_not_imported,
        Unsettled {
            retired_claims: 1,
            retired_sources: 1,
            ..Unsettled::default()
        }
    );
    assert_eq!(
        sealed_blocks(&w, &done.record.id).len(),
        1 + 3,
        "creation and the three revisions, and nothing after"
    );
    assert!(!done.repo.join(claim_path(&claim_id(1))).exists());
    assert!(!done.repo.join(source_path(&source_id(1))).exists());
    assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
    // No change carries the sentence: the report does.
    let kept = std::fs::read_to_string(w.layout.import_report(&done.record.id)).unwrap();
    assert!(
        kept.contains(
            "1 retired claim and 1 retired source were not imported: retiring is a curation here."
        ),
        "{kept}"
    );
    assert!(!kept.contains("written without them"), "{kept}");
}

#[test]
fn a_claim_that_cites_a_retired_source_is_refused_by_name_at_the_plan_and_nothing_is_made() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-names-retired");
    let citing = unsettled(settled_claim(1, 1));
    let d = bundle(
        "kn-names-retired-bundle",
        false,
        &[claim_file(&citing), source_file(&retired_source(1))],
    );
    let plan = plan_bundle(&d.0, &ReadConfig::default(), &BundleOptions::default()).expect("plan");
    assert!(!plan.is_importable(), "{}", plan.render());
    let refused: Vec<(&str, &str)> = plan
        .plan
        .refused
        .iter()
        .map(|r| (r.name.as_str(), r.reason.as_str()))
        .collect();
    assert_eq!(
        refused,
        [(
            claim_path(&citing.id).as_str(),
            format!(
                "names `{}`, which is retired and was not imported (retiring is a curation here)",
                source_path(&source_id(1))
            )
            .as_str()
        )]
    );
    // The run refuses before anything: no record, nothing on disk.
    let Err(error) = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}) else {
        panic!("a refused plan was imported");
    };
    assert!(matches!(error, ImportBundleError::Refused(1)), "{error}");
    assert!(w.store.list().expect("list").is_empty());
    assert!(w.store.by_slug(&Slug::new("orchard").unwrap()).is_err());
    assert_eq!(leftovers(&w), Vec::<String>::new());
}

#[test]
fn a_retired_file_that_changed_since_the_plan_stops_the_import_whole() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-ret-changed");
    let retired = retired_claim(2, 1);
    let d = bundle(
        "kn-ret-changed-bundle",
        false,
        &[
            claim_file(&settled_claim(1, 1)),
            claim_file(&retired),
            source_file(&checked_source(1)),
        ],
    );
    let plan = plan_of(&d.0, None);
    // The same size, in place: only the digest can tell. (It is a retired file, which is not written; it is read, and it is the file it
    // was.)
    let path = d.0.join(claim_path(&retired.id));
    let text = std::fs::read_to_string(&path).unwrap();
    let changed = text.replace("a note", "a nope");
    assert_eq!(changed.len(), text.len());
    assert_ne!(changed, text);
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.write_all(changed.as_bytes()).unwrap();
    }
    let Err(error) = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}) else {
        panic!("a changed file was imported");
    };
    let said = error.to_string();
    assert!(said.contains("is not the file that was planned"), "{said}");
    assert!(said.contains(&claim_path(&retired.id)), "{said}");
    assert!(w.store.list().expect("list").is_empty());
    assert_eq!(leftovers(&w), Vec::<String>::new());
}

#[test]
fn a_claim_with_sources_and_relations_arrives_with_its_links_intact() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // The baseline of a file the wiki does not have holds no link: a link's judgement is no departure. Two sources, one challenging, and a
    // relation to another claim: all arrive as they were.
    let w = world("kn-links");
    let mut one = settled_claim(1, 1);
    one.sources.push(SourceLink {
        source: source_id(2),
        relation: Link::Challenges,
        bearing: Bearing::Partial,
        note: None,
        judged_on: block_digest(
            inklkunn_markup::blocks("Version 3. ^b1")
                .first()
                .expect("a block"),
        ),
    });
    one.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(2),
    }];
    one.normalize();
    let two = unsettled(settled_claim(2, 1));
    let d = bundle(
        "kn-links-bundle",
        false,
        &[
            claim_file(&one),
            claim_file(&two),
            source_file(&Source::new(source_id(1))),
            source_file(&Source::new(source_id(2))),
        ],
    );
    let plan = plan_of(&d.0, None);
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    let File::Claim(got) = read_file(
        &claim_path(&one.id),
        &std::fs::read(done.repo.join(claim_path(&one.id))).unwrap(),
    )
    .unwrap() else {
        panic!("a claim");
    };
    assert_eq!(*got, unsettled(one.clone()));
    assert_eq!(got.sources.len(), 2);
    assert_eq!(got.relations, one.relations);
}

#[test]
fn knowledge_files_cut_into_several_changes_all_arrive_each_change_after_the_files_it_names() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    // The real cut (more files than one acceptance carries: 10,000) is the plan's, and `import/bundle/tests/knowledge.rs` holds it to
    // its cap and to the order of the names, with a bundle of 10,000 files; replaying that many through the real signer takes nine
    // minutes (it was run once by hand: it passed, in 543 s), so this test **cuts the plan's changes small** (the field is public) and
    // runs the replay through the same loop: three changes, a claim in the last resting on a source of the second.
    let w = world("kn-caps");
    let files = [
        source_file(&Source::new(source_id(1))),
        source_file(&Source::new(source_id(2))),
        source_file(&Source::new(source_id(3))),
        source_file(&Source::new(source_id(4))),
        claim_file(&unsettled(settled_claim(1, 4))),
        claim_file(&unsettled(settled_claim(2, 1))),
    ];
    let d = bundle("kn-caps-bundle", false, &files);
    let mut plan = plan_of(&d.0, None);
    let path = |n: u32, source: bool| {
        if source {
            source_path(&source_id(n))
        } else {
            claim_path(&claim_id(n))
        }
    };
    assert_eq!(
        plan.knowledge_changes.len(),
        2,
        "the plan's own cut: sources, claims"
    );
    plan.knowledge_changes = vec![
        vec![path(1, true), path(2, true)],
        vec![path(3, true), path(4, true)],
        vec![path(1, false), path(2, false)],
    ];
    let done = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}).expect("imported");
    assert_eq!(done.knowledge_changes, 3);
    assert_eq!(done.knowledge_files, 6);
    // Creation, the three revisions, the three knowledge changes: and every file is in the tree, each in the change the plan named.
    let blocks = sealed_blocks(&w, &done.record.id);
    assert_eq!(blocks.len(), 1 + 3 + 3);
    let after_three = blocks.len() - 3;
    for (change, expected) in plan.knowledge_changes.iter().enumerate() {
        let block = blocks.get(after_three + change).unwrap();
        let before = blocks.get(after_three + change - 1).unwrap();
        for file in expected {
            assert!(
                at_block(&w, &done.repo, block, file).is_some(),
                "{file} in change {change}"
            );
            assert_eq!(
                at_block(&w, &done.repo, before, file),
                None,
                "{file} before change {change}"
            );
        }
    }
    for (file, _) in &files {
        assert!(done.repo.join(file).is_file(), "{file}");
    }
    assert!(w.storage.verify(&done.repo).expect("verify").verdict.ok);
}

#[test]
fn a_knowledge_file_that_changed_since_the_plan_stops_the_import_whole() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let w = world("kn-changed");
    let claim = unsettled(settled_claim(1, 1));
    let d = bundle(
        "kn-changed-bundle",
        false,
        &[claim_file(&claim), source_file(&Source::new(source_id(1)))],
    );
    let plan = plan_of(&d.0, None);
    // The same size, in place (the same inode): only the digest can tell.
    let path = d.0.join(claim_path(&claim.id));
    let text = std::fs::read_to_string(&path).unwrap();
    let changed = text.replace("a note", "a nope");
    assert_eq!(changed.len(), text.len());
    assert_ne!(changed, text);
    {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.write_all(changed.as_bytes()).unwrap();
    }
    let Err(error) = run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|_| {}) else {
        panic!("a changed file was imported");
    };
    let said = error.to_string();
    assert!(said.contains("is not the file that was planned"), "{said}");
    assert!(said.contains(&claim_path(&claim.id)), "{said}");
    assert!(w.store.list().expect("list").is_empty());
    assert_eq!(leftovers(&w), Vec::<String>::new());
}

#[test]
fn an_import_stopped_after_the_knowledge_change_leaves_nothing_a_reader_can_reach() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let d = bundle(
        "kn-kill-bundle",
        true,
        &[
            claim_file(&settled_claim(1, 1)),
            source_file(&checked_source(1)),
        ],
    );
    let plan = plan_of(&d.0, None);
    let w = world("kn-kill");
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let stopped = catch_unwind(AssertUnwindSafe(|| {
        run(&w, &plan, "orchard", "CC-BY-SA-4.0", &|s| {
            if s == ImportStep::Knowledge {
                panic!("stopped after {s:?}");
            }
        })
    }));
    std::panic::set_hook(hook);
    assert!(stopped.is_err(), "the import was not stopped");
    assert!(w.store.list().expect("list").is_empty());
    assert!(w.store.by_slug(&Slug::new("orchard").unwrap()).is_err());
    assert!(leftovers(&w).iter().any(|p| p.ends_with(".creating")));
}
