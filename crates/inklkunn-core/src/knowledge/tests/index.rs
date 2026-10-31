//! The derived knowledge index (K-T4): built from the same extract as the title index, bounded, stored, and
//! rebuilt identically with the derived store cleared.

use std::sync::Arc;
use std::time::Duration;

use inklkunn_store::{DerivedStore as _, MemoryStore};

use super::*;
use crate::knowledge::index as kix;
use crate::read::testing::{Env, World};
use crate::read::{Cause, PageOutcome, ReadConfig, ReadError, ReadService};

const ONE: &str = "---\ntitle: One\nid: p1\n---\n\nSaid. ^b1\n\nOther.\n";
const TWO: &str = "---\ntitle: Two\nid: p2\n---\n\nPlain.\n";

fn config() -> ReadConfig {
    ReadConfig {
        tip_ttl: Duration::from_secs(3600),
        ..ReadConfig::default()
    }
}

/// A claim anchored at `p1#b1`, judged on the wording `judged` (a block as the page writes it).
fn claim_on(n: u8, judged: &str, source: u8) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![Expression::new("p1", "b1").expect("an expression")];
    c.community_status = CommunityStatus {
        status: Standing::Contested,
        proposal: Some("01930000-aaaa-7000-8000-000000000001".to_owned()),
        judged_on: Some(digest_of_block(judged)),
    };
    c.sources = vec![SourceLink {
        source: source_id(source),
        relation: Link::Supports,
        bearing: Bearing::Partial,
        note: None,
        judged_on: digest_of_block(judged),
    }];
    c
}

fn world(extra: &[(String, String)]) -> World {
    let mut files: Vec<(String, String)> = vec![
        ("pages/One.md".to_owned(), ONE.to_owned()),
        ("pages/Two.md".to_owned(), TWO.to_owned()),
    ];
    files.extend_from_slice(extra);
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::with(&refs, config())
}

fn files(claims: &[Claim], sources: &[Source]) -> Vec<(String, String)> {
    claims
        .iter()
        .map(|c| (claim_path(&c.id), c.to_yaml()))
        .chain(sources.iter().map(|s| (source_path(&s.id), s.to_yaml())))
        .collect()
}

fn knowledge(w: &World) -> Arc<kix::KnowledgeIndex> {
    w.service.knowledge_index(&w.wiki, None).expect("an index")
}

#[test]
fn the_index_holds_claims_sources_relations_anchors_and_links() {
    let mut a = claim_on(1, "Said. ^b1", 1);
    a.relations = vec![Relation {
        kind: RelationType::Refines,
        claim: claim_id(2),
    }];
    let b = claim_on(2, "Said. ^b1", 1);
    let mut s = Source::new(source_id(1));
    s.availability = Availability::Available;
    s.last_reviewed = Some(Reviewed {
        date: "2026-10-31".to_owned(),
        proposal: None,
    });
    let w = world(&files(&[a.clone(), b.clone()], std::slice::from_ref(&s)));
    let k = knowledge(&w);
    assert_eq!(k.claims().len(), 2);
    assert_eq!(k.sources().len(), 1);
    let row = k.claim(a.id.as_str()).expect("claim a");
    assert_eq!(
        (row.kind.as_str(), row.lifecycle.as_str()),
        ("observation", "proposed")
    );
    assert_eq!(row.standing, "contested");
    assert_eq!(
        k.source(s.id.as_str()).map(|r| r.last_reviewed.clone()),
        Some(Some("2026-10-31".to_owned()))
    );
    assert_eq!(k.relations_of(a.id.as_str()).count(), 1);
    // The claims that rely on the source, and the claims said at the block.
    assert_eq!(k.claims_citing(s.id.as_str()).count(), 2);
    assert_eq!(k.claims_at("p1", "b1").count(), 2);
    assert_eq!(k.claims_at("p1", "other").count(), 0);
    let anchor = k.anchors_of(a.id.as_str()).next().expect("an anchor");
    assert_eq!(anchor.state, kix::PRESENT);
    assert_eq!(anchor.page_path.as_deref(), Some("pages/One.md"));
    assert_eq!(
        anchor.digest.as_deref(),
        Some(digest_of_block("Said. ^b1").as_str())
    );
    // Nothing was judged on another wording.
    assert!(
        k.links_of(a.id.as_str())
            .all(|l| l.judgement == kix::CURRENT)
    );
    assert_eq!(row.standing_judgement, kix::CURRENT);
    assert!(k.invalid().is_empty());
}

#[test]
fn a_relation_is_current_exactly_while_the_anchors_text_equals_what_it_was_judged_on() {
    let on_old = claim_on(1, "Said. ^b1", 1);
    let source = Source::new(source_id(1));
    let w = world(&files(
        std::slice::from_ref(&on_old),
        std::slice::from_ref(&source),
    ));
    let before = knowledge(&w);
    assert!(
        before
            .links_of(on_old.id.as_str())
            .all(|l| l.judgement == "current")
    );

    // The page's words change (the id stays): the relation, and the standing, were judged on an earlier wording.
    w.fake()
        .set("pages/One.md", &ONE.replace("Said.", "Said otherwise."));
    w.service.forget_wiki(&w.wiki);
    let after = knowledge(&w);
    assert_ne!(before.tip(), after.tip());
    assert!(
        after
            .links_of(on_old.id.as_str())
            .all(|l| l.judgement == kix::EARLIER)
    );
    assert_eq!(
        after
            .claim(on_old.id.as_str())
            .map(|c| c.standing_judgement.as_str()),
        Some(kix::EARLIER)
    );
    // A reviewer's confirmation (the claim rewritten on the new wording) clears it.
    let confirmed = claim_on(1, "Said otherwise. ^b1", 1);
    w.fake()
        .set(&claim_path(&confirmed.id), &confirmed.to_yaml());
    w.service.forget_wiki(&w.wiki);
    let again = knowledge(&w);
    assert!(
        again
            .links_of(on_old.id.as_str())
            .all(|l| l.judgement == kix::CURRENT)
    );
    // Going back to the old words is current again for the old judgement.
    w.fake().set(&claim_path(&on_old.id), &on_old.to_yaml());
    w.fake().set("pages/One.md", ONE);
    w.service.forget_wiki(&w.wiki);
    assert!(
        knowledge(&w)
            .links_of(on_old.id.as_str())
            .all(|l| l.judgement == kix::CURRENT)
    );
}

#[test]
fn a_standing_that_was_never_decided_is_not_judged_on_anything() {
    let mut c = claim_on(1, "Said. ^b1", 1);
    c.community_status = CommunityStatus::undecided();
    let w = world(&files(
        std::slice::from_ref(&c),
        &[Source::new(source_id(1))],
    ));
    assert_eq!(
        knowledge(&w)
            .claim(c.id.as_str())
            .map(|r| r.standing_judgement.clone()),
        Some(kix::NOT_JUDGED.to_owned())
    );
}

#[test]
fn what_each_anchor_is_now_is_stated() {
    let mut c = claim_on(1, "Said. ^b1", 1);
    c.expressions = vec![
        Expression::new("p1", "b1").expect("e"),
        Expression::new("p1", "gone").expect("e"),
        Expression::new("nowhere", "b1").expect("e"),
        Expression::new("dup", "b1").expect("e"),
        Expression::new("rep", "twice").expect("e"),
    ];
    c.normalize();
    let mut extra = files(std::slice::from_ref(&c), &[Source::new(source_id(1))]);
    extra.push((
        "pages/D1.md".to_owned(),
        "---\ntitle: D1\nid: dup\n---\n\nx ^b1\n".to_owned(),
    ));
    extra.push((
        "pages/D2.md".to_owned(),
        "---\ntitle: D2\nid: dup\n---\n\ny ^b1\n".to_owned(),
    ));
    extra.push((
        "pages/R.md".to_owned(),
        "---\ntitle: R\nid: rep\n---\n\nx ^twice\n\ny ^twice\n".to_owned(),
    ));
    let w = world(&extra);
    let k = knowledge(&w);
    let state = |page: &str, block: &str| {
        k.claims_at(page, block)
            .next()
            .map(|a| a.state.clone())
            .expect("an anchor")
    };
    assert_eq!(state("p1", "b1"), kix::PRESENT);
    assert_eq!(state("p1", "gone"), kix::BLOCK_MISSING);
    assert_eq!(state("nowhere", "b1"), kix::PAGE_MISSING);
    assert_eq!(state("dup", "b1"), kix::PAGE_REPEATED);
    assert_eq!(state("rep", "twice"), kix::BLOCK_REPEATED);
    // The link, judged on a wording whose block is still found, is current; the standing too.
    assert!(
        k.links_of(c.id.as_str())
            .all(|l| l.judgement == kix::CURRENT)
    );
}

#[test]
fn a_file_that_cannot_be_read_is_listed_and_nothing_is_read_from_it() {
    let good = claim_on(1, "Said. ^b1", 1);
    let mut extra = files(std::slice::from_ref(&good), &[Source::new(source_id(1))]);
    // A claim with a number, a source with an unknown word, a stray file, a name that is no id, a long file.
    extra.push((
        claim_path(&claim_id(5)),
        "kind: observation\nlifecycle: proposed\ncommunity_status:\n  status: unknown\nscore: 3\n"
            .to_owned(),
    ));
    extra.push((
        source_path(&source_id(6)),
        "kind: reliable\navailability: unknown\n".to_owned(),
    ));
    extra.push(("knowledge/notes.txt".to_owned(), "hello".to_owned()));
    extra.push(("knowledge/claims/not-an-id.yaml".to_owned(), "x".to_owned()));
    extra.push((
        claim_path(&claim_id(8)),
        format!("# {}\n", "x".repeat(MAX_FILE_BYTES + 10)),
    ));
    let w = world(&extra);
    let k = knowledge(&w);
    let listed: Vec<(&str, &str)> = k
        .invalid()
        .iter()
        .map(|r| (r.path.as_str(), r.rule.as_str()))
        .collect();
    assert_eq!(listed.len(), 5, "{listed:?}");
    assert!(listed.contains(&(claim_path(&claim_id(5)).as_str(), "unknown-key")));
    assert!(listed.contains(&(source_path(&source_id(6)).as_str(), "bad-value")));
    assert!(listed.contains(&("knowledge/notes.txt", "path")));
    assert!(listed.contains(&("knowledge/claims/not-an-id.yaml", "path")));
    assert!(listed.contains(&(claim_path(&claim_id(8)).as_str(), "too-long")));
    // No claim or source from any of them; the good ones are there.
    assert_eq!(k.claims().len(), 1);
    assert_eq!(k.sources().len(), 1);
    assert!(k.claim(claim_id(5).as_str()).is_none());
    assert!(k.source(source_id(6).as_str()).is_none());
    // And the pages are served all the same.
    let page = w
        .service
        .page(&w.wiki, None, "One", &Env::EN)
        .expect("a page");
    assert!(matches!(page, PageOutcome::Found(_)));
}

#[test]
fn the_title_index_and_the_knowledge_index_come_from_one_extract() {
    let w = world(&files(
        &[claim_on(1, "Said. ^b1", 1)],
        &[Source::new(source_id(1))],
    ));
    let ((), calls) = w.counted(|| {
        let _ = w.service.lookup(&w.wiki, None, "One").expect("lookup");
        let _ = knowledge(&w);
        let _ = knowledge(&w);
        let _ = w.service.lookup(&w.wiki, None, "Two").expect("lookup");
    });
    assert_eq!(calls.materialize, 1, "one extract for both");
}

#[test]
fn rebuilt_with_the_derived_store_cleared_it_is_the_same_index() {
    let w = world(&files(
        &[claim_on(1, "Said. ^b1", 1), claim_on(2, "Other. ^zz", 1)],
        &[Source::new(source_id(1))],
    ));
    let first = knowledge(&w);
    // The stored index serves a restart: a service over the same store and no memory asks prikk for nothing.
    let restarted = World::service_over(&w.counting, &w.store, &w.layout, config());
    let (served, calls) = w.counted(|| restarted.knowledge_index(&w.wiki, None).expect("an index"));
    assert_eq!(served.rows(), first.rows());
    assert_eq!(calls.materialize, 0, "the store had it");

    // Cleared: everything derived is gone, and the next ask is one extract and the same rows.
    let removed = w.service.clear_derived(&w.wiki).expect("cleared");
    assert!(removed > 0);
    assert_eq!(
        w.store
            .knowledge_index(&w.wiki.id, first.tip())
            .expect("store"),
        None,
        "cleared from the store too"
    );
    let (rebuilt, calls) = w.counted(|| knowledge(&w));
    assert_eq!(calls.materialize, 1);
    assert_eq!(rebuilt.rows(), first.rows());
    // A fresh store, as after a restore: the same again.
    let fresh = Arc::new(MemoryStore::new());
    let other = ReadService::new(
        w.counting.clone(),
        fresh.clone(),
        w.store.clone(),
        w.layout.clone(),
        config(),
        None,
    );
    assert_eq!(
        other
            .knowledge_index(&w.wiki, None)
            .expect("an index")
            .rows(),
        first.rows()
    );
}

#[test]
fn a_title_index_stored_before_the_knowledge_index_existed_gets_one_on_demand() {
    let w = world(&files(
        &[claim_on(1, "Said. ^b1", 1)],
        &[Source::new(source_id(1))],
    ));
    let titles = w.service.title_index(&w.wiki, None).expect("titles");
    let tip = titles.tip().to_owned();
    // A store as an upgrade leaves it: the title index is there, the knowledge index is not.
    let upgraded = Arc::new(MemoryStore::new());
    upgraded
        .put_title_index(&w.wiki.id, &tip, titles.entries(), 1)
        .expect("title index");
    let service = ReadService::new(
        w.counting.clone(),
        upgraded.clone(),
        w.store.clone(),
        w.layout.clone(),
        config(),
        None,
    );
    let (k, calls) = w.counted(|| service.knowledge_index(&w.wiki, None).expect("an index"));
    assert_eq!(k.claims().len(), 1);
    assert_eq!(calls.materialize, 1);
    assert!(
        upgraded
            .knowledge_index(&w.wiki.id, &tip)
            .expect("store")
            .is_some()
    );
}

#[test]
fn the_index_is_bounded_and_a_tip_over_the_bound_still_serves_its_pages() {
    // Three files at the tip (the extract's own bound is three files), so prikk lets the tip be extracted; but the
    // knowledge directory is a chain of directories, and the walk counts the directories it visits too.
    let deep = "knowledge/a/b/c/d/e/f/g/h/i/j/file.txt";
    let borrowed: Vec<(&str, &str)> =
        vec![("pages/One.md", ONE), ("pages/Two.md", TWO), (deep, "x")];
    let w = World::with(
        &borrowed,
        ReadConfig {
            index_max_files: 3,
            ..config()
        },
    );
    let page = w
        .service
        .page(&w.wiki, None, "One", &Env::EN)
        .expect("a page");
    assert!(matches!(page, PageOutcome::Found(_)));
    match w.service.knowledge_index(&w.wiki, None) {
        Err(ReadError::CouldNotBeReadNow(Cause::IndexBounds(_))) => {}
        other => panic!(
            "expected the bound, got {:?}",
            other.map(|k| k.claims().len())
        ),
    }
    // Inside the bound the same files are indexed (the stray one listed as unreadable).
    let w = World::with(&borrowed, config());
    let k = knowledge(&w);
    assert_eq!(k.invalid().len(), 1);
}

#[test]
fn nothing_in_the_index_is_a_number_or_a_rank() {
    let w = world(&files(
        &[claim_on(1, "Said. ^b1", 1)],
        &[Source::new(source_id(1))],
    ));
    let k = knowledge(&w);
    // Every word the index carries is one of the format's own words, or an id, a path or a digest.
    for c in k.claims() {
        assert!(StatementKind::from_word(&c.kind).is_some());
        assert!(Lifecycle::from_word(&c.lifecycle).is_some());
        assert!(Standing::from_word(&c.standing).is_some());
        assert!(kix::Judgement::from_code(&c.standing_judgement).is_some());
    }
    for l in k.rows().links.iter() {
        assert!(Link::from_word(&l.relation).is_some());
        assert!(Bearing::from_word(&l.bearing).is_some());
        assert!(kix::Judgement::from_code(&l.judgement).is_some());
    }
}

#[test]
fn the_keep_file_of_a_new_wiki_is_no_knowledge_file() {
    // Every wiki starts with `knowledge/.keep` (it makes the empty directory exist); it is not listed as unreadable.
    let w = world(&[
        ("knowledge/.keep".to_owned(), String::new()),
        ("knowledge/claims/.keep".to_owned(), String::new()),
    ]);
    let k = knowledge(&w);
    assert!(k.invalid().is_empty(), "{:?}", k.invalid());
    assert!(k.claims().is_empty() && k.sources().is_empty());
    assert!(kix::build(std::path::Path::new("/nonexistent"), &[], 10, 10).is_ok());
}
