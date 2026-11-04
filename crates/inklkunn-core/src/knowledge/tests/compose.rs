//! Making a claim and attaching a source (RFC 005 part 2 D-3): the files and the page text an attachment comes to.

use std::collections::BTreeSet;

use super::*;
use crate::knowledge::compose::{
    Attachment, Curation, LinkEdit, Mistake, SourceDraft, SourceEdit, attach, clean_reference,
    clean_text, curate, page_id_of, paragraph_id, without_expressions,
};

const PAGE: &str = "---\ntitle: Bridge\n---\n\nIt opened in 1950.\n\nIt is long.\n";

fn attachment() -> Attachment {
    Attachment {
        source: SourceDraft {
            kind: SourceKind::Secondary,
            title: Some("A history".to_owned()),
            author: None,
            date: Some("1962".to_owned()),
            reference: Some(Reference {
                form: ReferenceForm::Url,
                value: "https://example.org/history".to_owned(),
            }),
        },
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: Some("page 4".to_owned()),
        kind: StatementKind::Observation,
    }
}

/// The region of the first paragraph of `PAGE` after the information block.
fn first_paragraph(page: &str) -> std::ops::Range<usize> {
    let start = page.find("It opened").expect("a paragraph");
    start..start + "It opened in 1950.".len()
}

fn read_claim(files: &[(String, String)]) -> Claim {
    let (path, text) = files.first().expect("a claim file");
    match read_file(path, text.as_bytes()).expect("a file") {
        File::Claim(c) => *c,
        File::Source(_) => panic!("a source"),
    }
}

#[test]
fn a_paragraph_without_a_claim_becomes_one_with_its_source_in_one_step() {
    let got = attach(PAGE, &first_paragraph(PAGE), None, &attachment()).expect("attached");
    assert!(got.created);
    // The page: gained an id, and the paragraph its marker; nothing else moved.
    let id = page_id_of(&got.page).expect("a page id");
    assert!(
        got.page
            .starts_with(&format!("---\nid: {id}\ntitle: Bridge\n---\n"))
    );
    assert!(
        got.page
            .contains(&format!("It opened in 1950. ^{}\n", got.block))
    );
    assert!(got.page.ends_with("It is long.\n"));
    // The claim: said at that paragraph, put forward, cites the source judged on the paragraph as the page has it.
    let claim = read_claim(&got.files);
    assert_eq!(claim.id, got.claim);
    assert_eq!(claim.kind, StatementKind::Observation);
    assert_eq!(claim.lifecycle, Lifecycle::Proposed);
    assert_eq!(claim.community_status, CommunityStatus::undecided());
    assert_eq!(
        claim.expressions,
        [Expression::new(&id, &got.block).expect("an expression")]
    );
    let link = claim.sources.first().expect("a link");
    assert_eq!(
        (link.relation, link.bearing),
        (Link::Supports, Bearing::Direct)
    );
    assert_eq!(link.note.as_deref(), Some("page 4"));
    let AnchorState::Present(now) = anchor_state(&got.page, &got.block) else {
        panic!("present");
    };
    assert_eq!(link.judged_on, now);
    // The source: as written, nothing checked.
    let source = match read_file(&got.files[1].0, got.files[1].1.as_bytes()).expect("a file") {
        File::Source(s) => *s,
        File::Claim(_) => panic!("a claim"),
    };
    assert_eq!(source.id, got.source);
    assert_eq!(source.identity.title.as_deref(), Some("A history"));
    assert_eq!(source.availability, Availability::Unknown);
    assert_eq!(source.last_reviewed, None);
    // The files are where the layer reads them.
    assert_eq!(got.files[0].0, claim_path(&claim.id));
    assert_eq!(got.files[1].0, source_path(&source.id));
}

#[test]
fn a_page_that_has_an_id_and_a_paragraph_that_has_a_marker_keep_them() {
    let page = "---\nid: bridge\n---\n\nIt opened in 1950. ^mine\n";
    let region = 0..page.len();
    assert_eq!(
        paragraph_id(page, &region).expect("a paragraph").as_deref(),
        Some("mine")
    );
    let got = attach(page, &region, None, &attachment()).expect("attached");
    assert_eq!(got.page, page, "the page is not touched");
    assert_eq!(got.block, "mine");
    assert_eq!(
        read_claim(&got.files).expressions,
        [Expression::new("bridge", "mine").expect("an expression")]
    );
}

#[test]
fn a_second_source_goes_to_the_claim_that_is_said_there_and_makes_no_new_claim() {
    let first = attach(PAGE, &first_paragraph(PAGE), None, &attachment()).expect("attached");
    let claim = read_claim(&first.files);
    let mut challenge = attachment();
    challenge.relation = Link::Challenges;
    challenge.bearing = Bearing::Partial;
    let region = first_paragraph(&first.page);
    let second = attach(&first.page, &region, Some(&claim), &challenge).expect("attached");
    assert!(!second.created);
    assert_eq!(second.claim, claim.id);
    assert_eq!(second.page, first.page);
    let after = read_claim(&second.files);
    assert_eq!(after.sources.len(), 2);
    assert_eq!(after.lifecycle, Lifecycle::Proposed);
    assert_eq!(after.expressions, claim.expressions);
    let new = after
        .sources
        .iter()
        .find(|l| l.source == second.source)
        .expect("the new link");
    assert_eq!(
        (new.relation, new.bearing),
        (Link::Challenges, Bearing::Partial)
    );
}

#[test]
fn the_claim_found_must_be_the_one_said_in_that_paragraph() {
    let first = attach(PAGE, &first_paragraph(PAGE), None, &attachment()).expect("attached");
    let mut other = read_claim(&first.files);
    other.expressions = vec![Expression::new("elsewhere", "x").expect("an expression")];
    let region = first_paragraph(&first.page);
    assert_eq!(
        attach(&first.page, &region, Some(&other), &attachment()),
        Err(Mistake::Unwritable)
    );
}

#[test]
fn a_page_without_an_information_block_is_given_one_and_the_bom_stays_first() {
    let page = "\u{feff}It opened in 1950.\n";
    let got = attach(page, &(3..page.len() - 1), None, &attachment()).expect("attached");
    assert!(got.page.starts_with("\u{feff}---\nid: "), "{:?}", got.page);
    assert!(page_id_of(&got.page).is_some());
    // CRLF information block: the new line is written the way the block is.
    let crlf = "---\r\ntitle: B\r\n---\r\n\r\nIt opened.\r\n";
    let region = crlf.find("It opened").expect("p")..crlf.len() - 2;
    let got = attach(crlf, &region, None, &attachment()).expect("attached");
    assert!(got.page.starts_with("---\r\nid: "), "{:?}", got.page);
    let id = page_id_of(&got.page).expect("a page id");
    assert!(
        got.page.contains(&format!("{id}\r\ntitle: B")),
        "{:?}",
        got.page
    );
}

#[test]
fn the_part_must_hold_a_paragraph() {
    let page = "---\nid: p\n---\n\n# A heading\n\n- an item\n\nA paragraph.\n";
    let heading = page.find("# A heading").expect("h")..page.find("# A heading").expect("h") + 11;
    assert_eq!(
        attach(page, &heading, None, &attachment()),
        Err(Mistake::NoParagraph)
    );
    assert_eq!(
        attach(page, &(5..5), None, &attachment()),
        Err(Mistake::NoParagraph)
    );
    assert_eq!(paragraph_id(page, &heading), Err(Mistake::NoParagraph));
    // A part that spans a heading and a paragraph: the paragraph is the one.
    let both = heading.start..page.len();
    assert!(attach(page, &both, None, &attachment()).is_ok());
}

#[test]
fn a_page_whose_information_block_cannot_be_read_cannot_be_given_an_id() {
    let page = "---\ntitle: A\ntitle: B\n---\n\nIt opened.\n";
    let region = page.find("It opened").expect("p")..page.len() - 1;
    assert_eq!(
        attach(page, &region, None, &attachment()),
        Err(Mistake::PageUnreadable)
    );
}

#[test]
fn a_source_has_to_say_something_and_what_it_says_is_checked() {
    let nothing = |f: fn(&mut Attachment)| {
        let mut a = attachment();
        f(&mut a);
        attach(PAGE, &first_paragraph(PAGE), None, &a)
    };
    assert_eq!(
        nothing(|a| {
            a.source.title = None;
            a.source.reference = None;
        }),
        Err(Mistake::SourceSaysNothing)
    );
    // A reference alone, or a title alone, is something.
    assert!(nothing(|a| a.source.title = None).is_ok());
    assert!(nothing(|a| a.source.reference = None).is_ok());
    assert_eq!(
        nothing(|a| a.note = Some("x".repeat(2_001))),
        Err(Mistake::TooLong("note"))
    );
    assert_eq!(
        nothing(|a| a.note = Some("a\u{7}b".to_owned())),
        Err(Mistake::ControlCharacter("note"))
    );
}

#[test]
fn texts_and_references_are_checked_before_they_are_files() {
    assert_eq!(clean_text("  ", "title"), Ok(None));
    assert_eq!(
        clean_text(" A title ", "title"),
        Ok(Some("A title".to_owned()))
    );
    assert_eq!(
        clean_text("two\nlines", "title"),
        Err(Mistake::ControlCharacter("title"))
    );
    assert_eq!(
        clean_text(&"x".repeat(2_001), "title"),
        Err(Mistake::TooLong("title"))
    );
    assert_eq!(clean_reference(ReferenceForm::Url, ""), Ok(None));
    assert!(matches!(
        clean_reference(ReferenceForm::Url, "HTTPS://example.org/a"),
        Ok(Some(_))
    ));
    for bad in [
        "javascript:alert(1)",
        "ftp://example.org/a",
        "example.org",
        "https://example.org/a b",
        "data:text/html,x",
    ] {
        assert_eq!(
            clean_reference(ReferenceForm::Url, bad),
            Err(Mistake::NotAWebAddress),
            "{bad}"
        );
    }
    // A citation in words is words.
    assert!(matches!(
        clean_reference(ReferenceForm::Citation, "Smith 1962, p. 4"),
        Ok(Some(_))
    ));
}

#[test]
fn a_claim_that_cites_as_many_sources_as_it_may_takes_no_more() {
    let first = attach(PAGE, &first_paragraph(PAGE), None, &attachment()).expect("attached");
    let mut claim = read_claim(&first.files);
    let link = claim.sources.first().expect("a link").clone();
    claim.sources = (0..256)
        .map(|_| SourceLink {
            source: SourceId::new(),
            ..link.clone()
        })
        .collect();
    let region = first_paragraph(&first.page);
    assert_eq!(
        attach(&first.page, &region, Some(&claim), &attachment()),
        Err(Mistake::Full)
    );
}

#[test]
fn removing_a_paragraph_takes_only_its_expression_out_of_the_claim() {
    let mut claim = Claim::new(claim_id(1));
    claim.expressions = vec![
        Expression::new("p1", "a").expect("e"),
        Expression::new("p1", "b").expect("e"),
        Expression::new("p2", "a").expect("e"),
    ];
    let gone: BTreeSet<String> = ["a".to_owned()].into();
    let after = without_expressions(&claim, "p1", &gone);
    assert_eq!(
        after.expressions,
        [
            Expression::new("p1", "b").expect("e"),
            Expression::new("p2", "a").expect("e")
        ]
    );
}

// ---------------------------------------------------------------------------------------------- curation

fn cited_claim() -> (Claim, Source) {
    let mut claim = Claim::new(claim_id(1));
    claim.expressions = vec![Expression::new("p1", "b").expect("e")];
    claim.sources = vec![SourceLink {
        source: source_id(1),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: digest_of("old"),
    }];
    (claim, Source::new(source_id(1)))
}

fn unchanged(claim: &Claim, source: &Source) -> Curation {
    Curation {
        status: claim.community_status.status,
        links: vec![LinkEdit {
            source: source.id.clone(),
            relation: Link::Supports,
            bearing: Bearing::Direct,
        }],
        sources: vec![SourceEdit {
            source: source.id.clone(),
            reviewed: String::new(),
            availability: source.availability,
        }],
        confirm: false,
    }
}

#[test]
fn a_curation_that_changes_nothing_writes_nothing() {
    let (claim, source) = cited_claim();
    let now = digest_of("new");
    let edit = unchanged(&claim, &source);
    assert_eq!(curate(&claim, &[source], Some(&now), &edit), Ok(vec![]));
}

#[test]
fn a_new_status_is_judged_on_the_wording_the_claim_has_now_and_names_no_proposal_yet() {
    let (claim, source) = cited_claim();
    let now = digest_of("new");
    let mut edit = unchanged(&claim, &source);
    edit.status = Standing::Contested;
    let files = curate(&claim, &[source], Some(&now), &edit).expect("curated");
    assert_eq!(files.len(), 1);
    let after = read_claim(&files);
    assert_eq!(after.community_status.status, Standing::Contested);
    assert_eq!(after.community_status.judged_on, Some(now));
    assert_eq!(after.community_status.proposal, None);
    // Back to not decided: nothing is kept of it.
    let mut again = unchanged(&after, &Source::new(source_id(1)));
    again.status = Standing::Unknown;
    let files = curate(&after, &[Source::new(source_id(1))], None, &again).expect("curated");
    assert_eq!(
        read_claim(&files).community_status,
        CommunityStatus::undecided()
    );
}

#[test]
fn a_changed_relation_or_bearing_is_judged_on_the_wording_now_and_an_untouched_link_is_not() {
    let (mut claim, source) = cited_claim();
    claim.sources.push(SourceLink {
        source: source_id(2),
        relation: Link::Challenges,
        bearing: Bearing::Indirect,
        note: Some("kept".to_owned()),
        judged_on: digest_of("old"),
    });
    let second = Source::new(source_id(2));
    let now = digest_of("new");
    let mut edit = unchanged(&claim, &source);
    edit.links = vec![
        LinkEdit {
            source: source_id(1),
            relation: Link::Challenges,
            bearing: Bearing::Partial,
        },
        LinkEdit {
            source: source_id(2),
            relation: Link::Challenges,
            bearing: Bearing::Indirect,
        },
    ];
    edit.sources = vec![];
    let files = curate(&claim, &[source, second], Some(&now), &edit).expect("curated");
    let after = read_claim(&files);
    let one = after
        .sources
        .iter()
        .find(|l| l.source == source_id(1))
        .expect("l");
    assert_eq!(
        (one.relation, one.bearing),
        (Link::Challenges, Bearing::Partial)
    );
    assert_eq!(one.judged_on, now);
    let two = after
        .sources
        .iter()
        .find(|l| l.source == source_id(2))
        .expect("l");
    assert_eq!(two.judged_on, digest_of("old"), "not asked, not re-judged");
    assert_eq!(two.note.as_deref(), Some("kept"));
}

#[test]
fn confirming_judges_every_link_and_a_decided_status_on_the_wording_now_once() {
    let (mut claim, source) = cited_claim();
    claim.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(digest_of("old")),
    };
    let now = digest_of("new");
    let mut edit = unchanged(&claim, &source);
    edit.confirm = true;
    let files = curate(&claim, std::slice::from_ref(&source), Some(&now), &edit).expect("curated");
    let after = read_claim(&files);
    assert_eq!(after.community_status.status, Standing::Accepted);
    assert_eq!(after.community_status.judged_on, Some(now.clone()));
    assert!(after.sources.iter().all(|l| l.judged_on == now));
    // Already current: nothing to write, and the deciding proposal is not renamed.
    assert_eq!(
        curate(&after, &[source], Some(&now), &edit),
        Ok(vec![]),
        "a confirmation of what is current changes nothing"
    );
}

#[test]
fn a_claim_said_nowhere_has_no_wording_to_judge_on() {
    let (claim, source) = cited_claim();
    let mut edit = unchanged(&claim, &source);
    edit.status = Standing::Accepted;
    assert_eq!(
        curate(&claim, std::slice::from_ref(&source), None, &edit),
        Err(Mistake::NoWording)
    );
    let mut edit = unchanged(&claim, &source);
    edit.confirm = true;
    assert_eq!(
        curate(&claim, &[source], None, &edit),
        Err(Mistake::NoWording)
    );
}

#[test]
fn a_source_the_claim_does_not_cite_is_refused_and_a_review_date_is_a_day() {
    let (claim, source) = cited_claim();
    let now = digest_of("new");
    let mut edit = unchanged(&claim, &source);
    edit.links.first_mut().expect("a link").source = source_id(9);
    assert_eq!(
        curate(&claim, std::slice::from_ref(&source), Some(&now), &edit),
        Err(Mistake::UnknownSource)
    );
    let mut edit = unchanged(&claim, &source);
    edit.sources.first_mut().expect("a source").source = source_id(9);
    assert_eq!(
        curate(&claim, std::slice::from_ref(&source), Some(&now), &edit),
        Err(Mistake::UnknownSource)
    );
    let mut edit = unchanged(&claim, &source);
    edit.sources.first_mut().expect("a source").reviewed = "yesterday".to_owned();
    assert_eq!(
        curate(&claim, std::slice::from_ref(&source), Some(&now), &edit),
        Err(Mistake::NotADay)
    );
    let mut edit = unchanged(&claim, &source);
    let first = edit.sources.first_mut().expect("a source");
    first.reviewed = "2026-11-04".to_owned();
    first.availability = Availability::Available;
    let files = curate(&claim, &[source], Some(&now), &edit).expect("curated");
    let (path, text) = files.first().expect("a source file");
    assert_eq!(path, &source_path(&source_id(1)));
    let File::Source(after) = read_file(path, text.as_bytes()).expect("a file") else {
        panic!("a source");
    };
    assert_eq!(after.availability, Availability::Available);
    assert_eq!(
        after.last_reviewed,
        Some(Reviewed {
            date: "2026-11-04".to_owned(),
            proposal: None
        })
    );
}
