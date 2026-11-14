#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::cell::RefCell;

use inklkunn_core::knowledge::{Bearing, Link, SourceLink, block_digest};

use super::*;

fn id(n: u32) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-{n:012}")).unwrap()
}

/// A source named `n`, replaced by `later` when there is one.
fn source(n: u32, later: Option<u32>) -> Source {
    let mut source = Source::new(id(n));
    source.identity.title = Some(format!("Source {n}"));
    source.superseded_by = later.map(id);
    source
}

/// A claim that cites source 1, as read at a base.
fn subject() -> Loaded {
    let mut claim = Claim::new(ClaimId::parse("01930000-0000-7000-8000-000000000001").unwrap());
    let blocks = inklkunn_markup::blocks("It opened in 1950. ^k1");
    claim.sources = vec![SourceLink {
        source: id(1),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: block_digest(blocks.first().unwrap()),
    }];
    Loaded {
        claim,
        cited: vec![Some(source(1, None))],
        wording: None,
    }
}

/// The text of the file a curation would write for `s`.
fn written(s: &Source) -> (String, String) {
    (source_path(&s.id), s.to_yaml())
}

/// Every id that storage was asked for, in order, over a base that holds `sources`.
struct Counting {
    sources: Vec<Source>,
    asked: RefCell<Vec<SourceId>>,
}

impl Counting {
    fn new(sources: Vec<Source>) -> Counting {
        Counting {
            sources,
            asked: RefCell::new(Vec::new()),
        }
    }

    fn reads(&self) -> SourceReads<'_> {
        SourceReads::with(
            |at| {
                self.asked.borrow_mut().push(at.clone());
                Ok(self.sources.iter().find(|s| s.id == *at).cloned())
            },
            &subject(),
        )
    }

    /// The most times storage was asked for one source.
    fn most_asked(&self) -> usize {
        let asked = self.asked.borrow();
        asked
            .iter()
            .map(|one| asked.iter().filter(|other| *other == one).count())
            .max()
            .unwrap_or(0)
    }
}

#[test]
fn a_source_is_read_once_however_often_the_checks_ask_for_it() {
    // Source 1 is the claim's, read with the claim and never read again; 2 is the one source 1 is to be replaced by.
    let base = Counting::new(vec![source(2, None)]);
    let reads = base.reads();
    for _ in 0..3 {
        assert!(reads.get(&id(1)).unwrap().is_some());
        assert!(reads.get(&id(2)).unwrap().is_some());
        assert!(reads.get(&id(9)).unwrap().is_none());
    }
    assert_eq!(
        *base.asked.borrow(),
        [id(2), id(9)],
        "the claim's source is not asked for, and a source the base lacks is asked for once"
    );
}

#[test]
fn the_checks_of_a_curation_share_one_read_of_each_source() {
    // Source 1 is to be replaced by source 2, and the chain goes on through the base: 2 -> 3 -> 4 -> 5.
    let base = Counting::new(vec![
        source(2, Some(3)),
        source(3, Some(4)),
        source(4, Some(5)),
        source(5, None),
    ]);
    let reads = base.reads();
    let subject = subject();
    let mut edit = Curation {
        status: Standing::Unknown,
        links: Vec::new(),
        removed: Vec::new(),
        sources: vec![SourceEdit {
            source: id(1),
            reviewed: String::new(),
            availability: Availability::Unknown,
            work: Some(Work {
                title: "Source 1".to_owned(),
                author: String::new(),
                date: String::new(),
                reference_form: ReferenceForm::Url,
                reference: String::new(),
                superseded_by: Some(id(2)),
            }),
        }],
        confirm: false,
    };
    assert!(!unknown_successor_with(&reads, &subject, &edit).unwrap());
    let files = [written(&source(1, Some(2)))];
    assert_eq!(successor_cycle_with(&reads, &files).unwrap(), None);
    assert_eq!(
        base.most_asked(),
        1,
        "each source is read once for the request: {:?}",
        base.asked
    );
    // A successor the base does not have is said, and read once as well.
    edit.sources
        .first_mut()
        .unwrap()
        .work
        .as_mut()
        .unwrap()
        .superseded_by = Some(id(7));
    assert!(unknown_successor_with(&reads, &subject, &edit).unwrap());
    assert!(unknown_successor_with(&reads, &subject, &edit).unwrap());
    assert_eq!(base.most_asked(), 1);
}

#[test]
fn many_written_sources_that_lead_into_one_long_chain_walk_it_once() {
    // Sources 10 ... 59 each become replaced by 100, which leads on through 100 -> 101 -> ... -> 199 in the base.
    let tail: Vec<Source> = (100..200)
        .map(|n| source(n, (n < 199).then_some(n + 1)))
        .collect();
    let base = Counting::new(tail);
    let reads = base.reads();
    let files: Vec<(String, String)> = (10..60).map(|n| written(&source(n, Some(100)))).collect();
    assert_eq!(successor_cycle_with(&reads, &files).unwrap(), None);
    assert_eq!(
        base.asked.borrow().len(),
        100,
        "one read for each source of the chain"
    );
    assert_eq!(base.most_asked(), 1);
}

#[test]
fn the_source_whose_chain_returns_is_the_one_named() {
    // 3 -> 4 in the base. Writing 4 -> 2 and 2 -> 3 makes 2 -> 3 -> 4 -> 2: the first written, in id order, is named.
    let base = Counting::new(vec![source(3, Some(4))]);
    let reads = base.reads();
    let files = [written(&source(4, Some(2))), written(&source(2, Some(3)))];
    assert_eq!(successor_cycle_with(&reads, &files).unwrap(), Some(id(2)));
    // A source that leads into a circle without being in it returns as well.
    let files = [written(&source(4, Some(3))), written(&source(1, Some(3)))];
    assert_eq!(successor_cycle_with(&reads, &files).unwrap(), Some(id(1)));
    // And one that only joins a chain that ends does not.
    let files = [written(&source(1, Some(3)))];
    let base = Counting::new(vec![source(3, Some(4)), source(4, None)]);
    assert_eq!(successor_cycle_with(&base.reads(), &files).unwrap(), None);
}

#[test]
fn the_refusal_for_a_returning_chain_names_the_source_as_the_legend_does() {
    let t = crate::catalog::T::new(crate::catalog::EN);
    let subject = subject();
    let text = subject.successor_cycle_text(t, &id(1));
    assert!(text.starts_with("Source: Source 1."), "{text}");
    let nameless = subject.successor_cycle_text(t, &id(8));
    assert!(
        nameless.starts_with("Source: a source without a title."),
        "{nameless}"
    );
}
