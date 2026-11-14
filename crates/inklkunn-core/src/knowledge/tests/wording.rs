//! A new source link is judged on a wording the claim has in the result (handoff 005 corrections, third round, K25; RFC 005
//! part 2 D-3, *a new link*): a digest never names wording that is not in the history.

use std::collections::BTreeMap;

use super::check::{Mem, files_of, page, sound_claim};
use super::*;
use crate::knowledge::check::{
    Curated, Touched, Unjudged, new_links_unjudged, unjudged_by, validate,
};
use crate::knowledge::compose::{Attachment, SourceDraft, attach};

/// A wording a paragraph of `p1` can have, as the page has it (the `^b1` marker included).
fn wording(words: &str) -> Digest {
    digest_of_block(&format!("{words} ^b1"))
}

fn page_with(words: &str) -> Mem {
    Mem(vec![(
        "pages/One.md".to_owned(),
        page("p1", &format!("{words} ^b1")),
    )])
}

fn claim_judged_on(on: &[(u8, Digest)]) -> Claim {
    let mut c = sound_claim(1);
    c.sources = on
        .iter()
        .map(|(n, digest)| SourceLink {
            source: source_id(*n),
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            judged_on: digest.clone(),
        })
        .collect();
    c
}

fn written(c: &Claim) -> Vec<(String, Option<String>)> {
    vec![(claim_path(&c.id), Some(c.to_yaml()))]
}

fn nothing(_: &str) -> Option<String> {
    None
}

fn unjudged(claim: &Claim, pages: &Mem) -> Vec<(u8, ())> {
    unjudged_by(&written(claim), &nothing, pages)
        .into_iter()
        .map(|Unjudged { claim: c, source }| {
            assert_eq!(c, claim.id);
            let n = (1..=9).find(|n| source_id(*n) == source).expect("a source");
            (n, ())
        })
        .collect()
}

/// What the acceptance says of the claim written over `pages`: the claim, and the sources it cites, in the view.
fn accepted(
    claim: &Claim,
    pages: &Mem,
    tip: Option<&Claim>,
    curation: bool,
) -> Vec<(Rule, &'static str)> {
    let path = claim_path(&claim.id);
    let mut files: Vec<(String, String)> = (1..=3)
        .map(|n| {
            (
                source_path(&source_id(n)),
                Source::new(source_id(n)).to_yaml(),
            )
        })
        .collect();
    files.push((path.clone(), claim.to_yaml()));
    let mut touched = Touched {
        written: [path.clone()].into(),
        curation,
        ..Touched::default()
    };
    if let Some(tip) = tip {
        touched
            .curated_before
            .insert(path, Curated::of(&File::Claim(Box::new(tip.clone()))));
    }
    validate(&files_of(&files), &touched, pages, pages)
        .into_iter()
        .map(|r| (r.rule, r.field))
        .collect()
}

#[test]
fn a_link_judged_on_a_wording_nobody_has_written_is_refused_on_every_surface() {
    // P-b: the page says W1; the link is judged on the digest of W2. Were it accepted, a later page edit that rewords the
    // paragraph to W2 would make the link read current with no curation and neither page saying so.
    let w1 = page_with("It opened in 1950.");
    let claim = claim_judged_on(&[(1, wording("It opened in 1951."))]);
    assert_eq!(unjudged(&claim, &w1), [(1, ())]);
    assert_eq!(
        accepted(&claim, &w1, None, false),
        [(Rule::Judged, "sources")]
    );
    // A curation proposal is held to it too: it is not a way to judge on words nobody has.
    assert_eq!(
        accepted(&claim, &w1, None, true),
        [(Rule::Judged, "sources")]
    );
    // The service's verdict, at submit and at every revision.
    let was = |_: &str| None;
    assert_eq!(
        crate::proposal::files::knowledge_verdict(
            &written(&claim),
            &was,
            false,
            &crate::knowledge::summary::summarize,
            &w1
        ),
        Err(crate::proposal::Invalid::KnowledgeUnjudged)
    );
    // The same link, with the same proposal writing the words it was judged on: sound (the page says W2 now).
    let w2 = page_with("It opened in 1951.");
    assert!(unjudged(&claim, &w2).is_empty());
    assert!(accepted(&claim, &w2, None, false).is_empty());
    assert_eq!(
        crate::proposal::files::knowledge_verdict(
            &written(&claim),
            &was,
            false,
            &crate::knowledge::summary::summarize,
            &w2
        ),
        Ok(())
    );
}

#[test]
fn what_compose_writes_is_a_wording_the_page_has() {
    const PAGE: &str = "---\ntitle: Bridge\n---\n\nIt opened in 1950.\n\nIt is long.\n";
    let start = PAGE.find("It opened").expect("a paragraph");
    let attached = attach(
        PAGE,
        &(start..start + "It opened in 1950.".len()),
        None,
        &Attachment {
            source: SourceDraft {
                kind: SourceKind::Secondary,
                title: Some("A history".to_owned()),
                author: None,
                date: None,
                reference: None,
            },
            relation: Link::Supports,
            bearing: Bearing::Direct,
            note: None,
            kind: StatementKind::Observation,
        },
    )
    .expect("attached");
    // The page the proposal leaves: the page `attach` returns (it gained its id and the paragraph its marker).
    let result = Mem(vec![("pages/Bridge.md".to_owned(), attached.page.clone())]);
    let [(claim_file, claim_text), (source_file, source_text)] = attached.files;
    let given = vec![
        (claim_file.clone(), Some(claim_text.clone())),
        (source_file.clone(), Some(source_text.clone())),
    ];
    assert!(unjudged_by(&given, &nothing, &result).is_empty());
    // The acceptance, on the files the view would hold.
    let mut touched = Touched {
        written: [claim_file.clone(), source_file.clone()].into(),
        ..Touched::default()
    };
    touched.page_ids.insert(page_id_of_text(&attached.page));
    let files = files_of(&[(claim_file, claim_text), (source_file, source_text)]);
    assert_eq!(validate(&files, &touched, &result, &result), []);
    // The page as it was before the attach does not have that wording (the marker is part of it): the digest is of the
    // page as the proposal leaves it.
    let before = Mem(vec![("pages/Bridge.md".to_owned(), PAGE.to_owned())]);
    let id_less = unjudged_by(&given_only_claim(&given), &nothing, &before);
    assert_eq!(id_less.len(), 1, "{id_less:?}");
}

fn page_id_of_text(text: &str) -> String {
    crate::knowledge::compose::page_id_of(text).expect("a page id")
}

fn given_only_claim(given: &[(String, Option<String>)]) -> Vec<(String, Option<String>)> {
    given.iter().take(1).cloned().collect()
}

#[test]
fn a_new_claim_with_links_is_judged_link_by_link() {
    let pages = page_with("It opened in 1950.");
    let now = wording("It opened in 1950.");
    // Both on the wording it has.
    let fine = claim_judged_on(&[(1, now.clone()), (2, now.clone())]);
    assert!(unjudged(&fine, &pages).is_empty());
    assert!(accepted(&fine, &pages, None, false).is_empty());
    // One on a wording it had: only that link is named.
    let one_old = claim_judged_on(&[(1, now.clone()), (2, wording("It opened in 1949."))]);
    assert_eq!(unjudged(&one_old, &pages), [(2, ())]);
    assert_eq!(
        accepted(&one_old, &pages, None, false),
        [(Rule::Judged, "sources")]
    );
}

#[test]
fn a_link_the_claim_already_has_is_not_judged_again_here_and_a_new_one_beside_it_is() {
    let pages = page_with("It opened in 1950.");
    let now = wording("It opened in 1950.");
    // The tip's claim: one link judged on an earlier wording (it reads *earlier*, which is true).
    let tip = claim_judged_on(&[(1, wording("It opened in 1949."))]);
    let held: BTreeMap<String, String> = [(claim_path(&tip.id), tip.to_yaml())].into();
    let before = |p: &str| held.get(p).cloned();
    // Kept as it is, a second link beside it on the wording the claim has: sound.
    let beside = claim_judged_on(&[(1, wording("It opened in 1949.")), (2, now.clone())]);
    assert!(unjudged_by(&written(&beside), &before, &pages).is_empty());
    assert!(accepted(&beside, &pages, Some(&tip), false).is_empty());
    // ... and on a wording it has not: that link is refused, the first still left alone.
    let stale = claim_judged_on(&[
        (1, wording("It opened in 1949.")),
        (2, wording("It opened in 1948.")),
    ]);
    let got = unjudged_by(&written(&stale), &before, &pages);
    assert_eq!(
        got.iter().map(|u| u.source.clone()).collect::<Vec<_>>(),
        [source_id(2)]
    );
    assert_eq!(
        accepted(&stale, &pages, Some(&tip), false),
        [(Rule::Judged, "sources")]
    );
    // The function itself, for a claim the tip does not have.
    assert_eq!(new_links_unjudged(&stale, None, &pages).len(), 2);
}

#[test]
fn a_wording_is_one_the_claims_own_paragraphs_have() {
    let both = Mem(vec![(
        "pages/One.md".to_owned(),
        page("p1", "First words. ^b1\n\nSecond words. ^b2"),
    )]);
    let second = digest_of_block("Second words. ^b2");
    // Said in the first paragraph only: the second paragraph's wording is somebody else's.
    let mut claim = claim_judged_on(&[(1, second.clone())]);
    assert_eq!(unjudged(&claim, &both), [(1, ())]);
    // Said in both places: either paragraph's wording will do.
    claim.expressions = vec![
        Expression::new("p1", "b1").expect("an expression"),
        Expression::new("p1", "b2").expect("an expression"),
    ];
    assert!(unjudged(&claim, &both).is_empty());
    // A claim said nowhere has no wording to judge a new link on.
    claim.expressions.clear();
    assert_eq!(unjudged(&claim, &both), [(1, ())]);
    // A place whose page is not there, or not there once, is no wording either.
    claim.expressions = vec![Expression::new("nope", "b1").expect("an expression")];
    assert_eq!(unjudged(&claim, &both), [(1, ())]);
    let twice = Mem(vec![
        ("pages/A.md".to_owned(), page("p1", "First words. ^b1")),
        ("pages/B.md".to_owned(), page("p1", "First words. ^b1")),
    ]);
    let claim = claim_judged_on(&[(1, digest_of_block("First words. ^b1"))]);
    assert_eq!(unjudged(&claim, &twice), [(1, ())]);
    // The usual case, for the contrast.
    assert!(unjudged(&sound_claim(1), &both_with_said()).is_empty());
}

fn both_with_said() -> Mem {
    Mem(vec![(
        "pages/One.md".to_owned(),
        page("p1", "Said. ^b1\n\nOther. ^b2"),
    )])
}
