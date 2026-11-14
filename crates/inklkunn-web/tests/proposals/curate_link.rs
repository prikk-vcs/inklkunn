//! The way from a refusal to the curation form (RFC 005 part 2 D-3; handoff 005 K39): where a proposal's page, or a
//! refusal, says a change needs a curation proposal, it leads to the form of each claim the change touches, for those
//! who may propose and for nobody else. The repository is an in-memory double, so these run in both test runs.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{
    Claim, ClaimId, Expression, Lifecycle, Standing, StatementKind, claim_path,
};

use super::claims::{BRIDGE, Scene, held_unjudged, open, principal, scene, send};
use crate::common::{block_id, state_id};

const WORDS: &str = "Propose a change to its status or sources";

/// The link's words on a page that is not the claim's: they name the claim, by its whole identifier in `<code>` (U-i).
fn words_of(claim: &ClaimId) -> String {
    format!("Propose a change to the status or sources of the claim <code>{claim}</code>")
}

fn form_of(claim: &ClaimId) -> String {
    format!("/handbook/-/proposals/curate?claim={claim}")
}

/// `claim` with a community status decided: what only a curation proposal may set.
fn decided(claim: &Claim) -> (String, Option<String>) {
    let mut decided = claim.clone();
    decided.community_status.status = Standing::Accepted;
    (claim_path(&decided.id), Some(decided.to_yaml()))
}

/// An edit of a page beside it: the proposal is then not a curation proposal.
fn page_edit() -> (String, Option<String>) {
    (
        "pages/Bridge.md".to_owned(),
        Some(BRIDGE.replace("A note.", "A longer note.")),
    )
}

/// A second claim, said in the same paragraph, written into the wiki.
fn second_claim(s: &Scene) -> Claim {
    numbered_claim(s, 2)
}

/// Claim number `n`, said in the same paragraph, written into the wiki.
fn numbered_claim(s: &Scene, n: u8) -> Claim {
    let id =
        ClaimId::parse(&format!("01930000-0000-7000-8000-00000000000{n}")).expect("a claim id");
    let mut claim = Claim::new(id);
    claim.kind = StatementKind::Observation;
    claim.lifecycle = Lifecycle::Proposed;
    claim.expressions = vec![Expression::new("bridge-page", "k1").expect("an expression")];
    s.w.fake
        .set_files(&[(claim_path(&claim.id).as_str(), claim.to_yaml().as_str())]);
    claim
}

#[tokio::test]
async fn a_proposal_that_needs_curation_leads_to_the_form_of_the_claim_for_who_may_propose() {
    let s = scene();
    let to = held_unjudged(&s, vec![decided(&s.claim), page_edit()]);
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    // One claim too is named: the proposal's page is not the claim's, so *its* would have nothing to refer to (U-i).
    assert!(
        body.contains(&format!(
            "<a href=\"{}\">{}</a>",
            form_of(&s.claim.id),
            words_of(&s.claim.id)
        )),
        "{body}"
    );
    assert!(!body.contains(WORDS), "{body}");
    // Someone who may not propose is told the same and is not offered a form they cannot use.
    let body = open(&s, &to, &s.observer).await.body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(!body.contains("proposals/curate"), "{body}");
    // The link opens the form of that claim.
    let form = open(&s, &form_of(&s.claim.id), &s.writer).await;
    assert_eq!(form.status, StatusCode::OK);
    assert!(form.body().contains("<h1>Curate a claim</h1>"));
    // A proposal that is a curation proposal, or changes no status, offers nothing: it has nothing to be told.
    let body = open(
        &s,
        &held_unjudged(&s, vec![(claim_path(&s.claim.id), Some(s.claim.to_yaml()))]),
        &s.writer,
    )
    .await
    .body();
    assert!(!body.contains("proposals/curate"), "{body}");
}

#[tokio::test]
async fn several_claims_are_each_offered_a_link_that_names_the_claim() {
    let s = scene();
    let second = second_claim(&s);
    let to = held_unjudged(&s, vec![decided(&s.claim), decided(&second), page_edit()]);
    let body = open(&s, &to, &s.writer).await.body();
    for claim in [&s.claim.id, &second.id] {
        assert!(
            body.contains(&format!(
                "<a href=\"{}\">{}</a>",
                form_of(claim),
                words_of(claim)
            )),
            "{body}"
        );
    }
    assert!(!body.contains(WORDS), "{body}");
}

#[tokio::test]
async fn a_refused_submission_of_a_change_that_needs_curation_leads_to_the_form() {
    let s = scene();
    // A draft that holds a decided status (the service keeps what a person made, and refuses it when it is sent).
    let draft =
        s.w.services
            .proposals
            .create(
                &principal(&s.writer),
                &s.w.wiki,
                None,
                inklkunn_core::proposal::ProposalEdit {
                    summary: "A status".to_owned(),
                    files: [decided(&s.claim), page_edit()]
                        .into_iter()
                        .map(|(path, text)| inklkunn_store::FileChange { path, text })
                        .collect(),
                    ..Default::default()
                },
            )
            .expect("a draft");
    let blocks: std::collections::BTreeSet<String> = ["k1".to_owned()].into();
    let fields = vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", state_id(1)),
        ("base_block", block_id(1)),
        ("p0_path", "pages/Bridge.md".to_owned()),
        ("p0_title", "Bridge".to_owned()),
        (
            "p0_text",
            inklkunn_core::knowledge::marker::hide_markers(
                &page_edit().1.expect("a page"),
                &blocks,
                true,
            ),
        ),
        ("summary", "A status".to_owned()),
        ("sources", String::new()),
    ];
    let uri = format!("/handbook/-/proposals/{}/edit", draft.id);
    let a = send(&s, &uri, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    let body = a.body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(
        body.contains(&format!(
            "<a href=\"{}\">{}</a>",
            form_of(&s.claim.id),
            words_of(&s.claim.id)
        )),
        "{body}"
    );
}

#[tokio::test]
async fn more_claims_than_are_offered_a_form_are_counted_and_not_cut_in_silence() {
    // U-i: at most five links; the rest are said, with how many, in the singular and the plural, in both languages.
    let s = scene();
    let claims: Vec<Claim> = (2..=7).map(|n| numbered_claim(&s, n)).collect();
    let mut files = vec![decided(&s.claim), page_edit()];
    files.extend(claims.iter().map(decided));
    let to = held_unjudged(&s, files);
    let body = open(&s, &to, &s.writer).await.body();
    assert_eq!(body.matches("proposals/curate?claim=").count(), 5, "{body}");
    assert!(
        body.contains(
            "2 more claims are touched: each claim&#39;s own page leads to its curation form."
        ),
        "{body}"
    );
    // Six claims: one more, in the singular. Five: nothing is cut, nothing is said.
    let mut files = vec![decided(&s.claim), page_edit()];
    files.extend(claims.iter().take(5).map(decided));
    let body = open(&s, &held_unjudged(&s, files), &s.writer).await.body();
    assert!(
        body.contains("1 more claim is touched: its own page leads to its curation form."),
        "{body}"
    );
    let mut files = vec![decided(&s.claim), page_edit()];
    files.extend(claims.iter().take(4).map(decided));
    let body = open(&s, &held_unjudged(&s, files), &s.writer).await.body();
    assert_eq!(body.matches("proposals/curate?claim=").count(), 5, "{body}");
    assert!(!body.contains(" more claim"), "{body}");
    // In Japanese.
    let ja = crate::common::request(
        &s.w,
        crate::common::APP,
        "GET",
        &to,
        &[
            ("x-test-account", s.writer.as_str()),
            ("cookie", "inklkunn_lang=ja"),
        ],
    )
    .await
    .body();
    assert!(ja.contains("ほかに 2 件の主張にも関わります。"), "{ja}");
}

// ---------------------------------------------------------------------------------------------- every change (K44)

use super::claims::{file, files_of, location};
use super::curate::{FORM, checked_scene, checked_source, checked_unchanged, later_id, with};
use inklkunn_core::knowledge::{Reference, ReferenceForm, source_path};

const BY_REVIEWER: &str = "is decided by a reviewer: the curation form does not offer it";

/// The checked source with `change` made to it: its path and the text the wiki would write.
fn source_changed(change: impl Fn(&mut inklkunn_core::knowledge::Source)) -> (String, String) {
    let mut source = checked_source();
    change(&mut source);
    (source_path(&source.id), source.to_yaml())
}

#[tokio::test]
async fn each_change_only_curation_makes_leads_from_its_refusal_to_a_form_that_makes_it() {
    // Each case: the refused change (beside a page edit, so it is not a curation proposal), and the form's fields that
    // make the same change. The link must be offered, the form must open, and the file it sends must be the file the
    // refused proposal carried.
    type Fields = Vec<(&'static str, String)>;
    type Refused = Box<dyn Fn(&Scene) -> (String, Option<String>)>;
    struct Case {
        name: &'static str,
        refused: Refused,
        fields: Box<dyn Fn(&Scene) -> Fields>,
    }
    let cases = vec![
        Case {
            name: "a link taken away",
            refused: Box::new(|s| {
                let mut without = s.claim.clone();
                without.sources.clear();
                (claim_path(&without.id), Some(without.to_yaml()))
            }),
            fields: Box::new(|s| with(checked_unchanged(s), &[("c0_remove", "1")])),
        },
        Case {
            name: "a checked source's title, author and date",
            refused: Box::new(|_| {
                let (path, text) = source_changed(|x| {
                    x.identity.title = Some("A history, corrected".to_owned());
                    x.identity.author = Some("Noor Aziz".to_owned());
                    x.identity.date = Some("2024".to_owned());
                });
                (path, Some(text))
            }),
            fields: Box::new(|s| {
                with(
                    checked_unchanged(s),
                    &[
                        ("c0_title", "A history, corrected"),
                        ("c0_author", "Noor Aziz"),
                        ("c0_date", "2024"),
                    ],
                )
            }),
        },
        Case {
            name: "a checked source's reference",
            refused: Box::new(|_| {
                let (path, text) = source_changed(|x| {
                    x.reference = Some(Reference {
                        form: ReferenceForm::Doi,
                        value: "10.1000/182".to_owned(),
                    });
                });
                (path, Some(text))
            }),
            fields: Box::new(|s| {
                with(
                    checked_unchanged(s),
                    &[("c0_ref_form", "doi"), ("c0_ref", "10.1000/182")],
                )
            }),
        },
        Case {
            name: "the source a checked source is replaced by",
            refused: Box::new(|_| {
                let later = later_id();
                let (path, text) = source_changed(|x| x.superseded_by = Some(later.clone()));
                (path, Some(text))
            }),
            fields: Box::new(|s| {
                with(
                    checked_unchanged(s),
                    &[("c0_superseded_by", &later_id().to_string())],
                )
            }),
        },
    ];
    for case in cases {
        let s = checked_scene();
        let (path, text) = (case.refused)(&s);
        let to = held_unjudged(&s, vec![(path.clone(), text.clone()), page_edit()]);
        let body = open(&s, &to, &s.writer).await.body();
        assert!(
            body.contains("only a curation proposal may do"),
            "{}: {body}",
            case.name
        );
        assert!(
            !body.contains(BY_REVIEWER),
            "{}: nothing here is a reviewer's: {body}",
            case.name
        );
        let link = format!("<a href=\"{}\">{WORDS}</a>", form_of(&s.claim.id));
        assert!(body.contains(&link), "{}: {body}", case.name);
        // The link opens the form, and the form makes the same change.
        let form = open(&s, &form_of(&s.claim.id), &s.writer).await;
        assert_eq!(form.status, StatusCode::OK, "{}", case.name);
        let sent = send(&s, FORM, &s.writer, &(case.fields)(&s)).await;
        let made = files_of(&s, &location(&sent));
        assert_eq!(
            file(&made, &path),
            text.as_deref().expect("a text"),
            "{}: the form writes the file the refused proposal carried",
            case.name
        );
        // And what it made is a curation proposal that can be accepted as it is.
        let page = open(&s, &location(&sent), &s.writer).await.body();
        assert!(
            page.contains("This is a curation proposal"),
            "{}: {page}",
            case.name
        );
        assert!(
            !page.contains("It cannot be accepted as it is."),
            "{}: {page}",
            case.name
        );
    }
}

fn second_claim_with_a_link(s: &Scene) -> Claim {
    let id = ClaimId::parse("01930000-0000-7000-8000-000000000002").expect("a claim id");
    let mut claim = Claim::new(id);
    claim.kind = StatementKind::Observation;
    claim.lifecycle = Lifecycle::Proposed;
    claim.expressions = vec![Expression::new("bridge-page", "k1").expect("an expression")];
    claim.sources = s.claim.sources.clone();
    s.w.fake.set(&claim_path(&claim.id), &claim.to_yaml());
    claim
}

#[tokio::test]
async fn what_no_form_makes_is_a_reviewers_and_the_page_says_so_without_a_link_it_cannot_use() {
    let s = scene();
    // A claim taken away with its link, beside a page edit: no form removes a file.
    let removal = vec![(claim_path(&s.claim.id), None), page_edit()];
    let body = open(&s, &held_unjudged(&s, removal.clone()), &s.writer)
        .await
        .body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(body.contains(BY_REVIEWER), "{body}");
    assert!(!body.contains("proposals/curate"), "{body}");

    // A file the wiki cannot read, written over: the same.
    let unread = ClaimId::parse("01930000-0000-7000-8000-000000000003").expect("a claim id");
    s.w.fake
        .set(&claim_path(&unread), "kind: observation\nsecret: words\n");
    let body = open(
        &s,
        &held_unjudged(&s, vec![(claim_path(&unread), None), page_edit()]),
        &s.writer,
    )
    .await
    .body();
    assert!(body.contains(BY_REVIEWER), "{body}");
    assert!(!body.contains("proposals/curate"), "{body}");

    // A change a form makes beside one it does not: the form is offered for the first, the second is said to be a
    // reviewer's.
    let other = second_claim_with_a_link(&s);
    let mut decided = s.claim.clone();
    decided.community_status.status = Standing::Accepted;
    let body = open(
        &s,
        &held_unjudged(
            &s,
            vec![
                decided_pair(&decided),
                (claim_path(&other.id), None),
                page_edit(),
            ],
        ),
        &s.writer,
    )
    .await
    .body();
    assert!(body.contains(BY_REVIEWER), "{body}");
    assert!(
        body.contains(&format!("<a href=\"{}\">{WORDS}</a>", form_of(&s.claim.id))),
        "{body}"
    );
    assert!(
        !body.contains(&form_of(&other.id)),
        "the claim whose file is removed is not offered a form: {body}"
    );

    // The refused send says it too, and leads nowhere for it.
    let draft =
        s.w.services
            .proposals
            .create(
                &principal(&s.writer),
                &s.w.wiki,
                None,
                inklkunn_core::proposal::ProposalEdit {
                    summary: "A removal".to_owned(),
                    files: vec![
                        inklkunn_store::FileChange {
                            path: claim_path(&s.claim.id),
                            text: None,
                        },
                        inklkunn_store::FileChange {
                            path: page_edit().0,
                            text: page_edit().1,
                        },
                    ],
                    ..Default::default()
                },
            )
            .expect("a draft");
    let blocks: std::collections::BTreeSet<String> = ["k1".to_owned()].into();
    let fields = vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", state_id(3)),
        ("base_block", block_id(3)),
        ("p0_path", "pages/Bridge.md".to_owned()),
        ("p0_title", "Bridge".to_owned()),
        (
            "p0_text",
            inklkunn_core::knowledge::marker::hide_markers(
                &page_edit().1.expect("a page"),
                &blocks,
                true,
            ),
        ),
        ("summary", "A removal".to_owned()),
        ("sources", String::new()),
    ];
    let a = send(
        &s,
        &format!("/handbook/-/proposals/{}/edit", draft.id),
        &s.writer,
        &fields,
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    let body = a.body();
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(body.contains(BY_REVIEWER), "{body}");
    assert!(!body.contains("proposals/curate"), "{body}");
}

/// `claim` as a file of a proposal.
fn decided_pair(claim: &Claim) -> (String, Option<String>) {
    (claim_path(&claim.id), Some(claim.to_yaml()))
}
