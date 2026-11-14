//! What a curation does to a source's check (RFC 005 part 2 D-3, *correcting a source*; handoff 005 K66, K69): a changed
//! reference clears the day it was last checked and whether it can be had, unless the same proposal records new ones; a
//! correction of the title, the author or the date keeps them and the proposal's page says the review stays and whose
//! claims rely on it; a change of white space alone is shown with its white space visible; the cycle refusal names its
//! source. In both languages. The repository is an in-memory double, so these run in both test runs.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{Availability, Source, claim_path, source_path};

use super::claims::{Scene, files_of, location, open, paths, scene, send};
use super::curate::{
    FORM, checked_scene, checked_source, checked_unchanged, claim_uri, later_id, source_of, with,
};
use crate::common::{APP, Answer, request, send_body};

/// [`send`], asking for the Japanese preview.
async fn send_ja(s: &Scene, uri: &str, fields: &[(&'static str, String)]) -> String {
    let body = fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                inklkunn_markup::percent_encode(k),
                inklkunn_markup::percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    let answer: Answer = send_body(
        &s.w,
        "POST",
        uri,
        Some(&s.writer),
        &[
            ("sec-fetch-site", "same-origin"),
            ("cookie", "inklkunn_lang=ja"),
        ],
        body,
    )
    .await;
    answer.body()
}

/// A page in Japanese.
async fn page_ja(s: &Scene, address: &str) -> String {
    request(
        &s.w,
        APP,
        "GET",
        address,
        &[
            ("x-test-account", s.writer.as_str()),
            ("cookie", "inklkunn_lang=ja"),
        ],
    )
    .await
    .body()
}

fn other_claim_id() -> inklkunn_core::knowledge::ClaimId {
    inklkunn_core::knowledge::ClaimId::parse("01930000-0000-7000-8000-000000000002")
        .expect("a claim id")
}

/// The scene with its source checked and a second claim that cites the same source (version 2, as the checked scene is).
fn shared_scene() -> Scene {
    let s = scene();
    let source = checked_source();
    let mut other = s.claim.clone();
    other.id = other_claim_id();
    s.w.fake.set_files(&[
        (source_path(&source.id).as_str(), source.to_yaml().as_str()),
        (claim_path(&other.id).as_str(), other.to_yaml().as_str()),
    ]);
    s
}

fn elsewhere() -> [(&'static str, &'static str); 2] {
    [("c0_ref_form", "doi"), ("c0_ref", "10.1000/182")]
}

type Case<'a> = (
    &'a str,
    &'a [(&'static str, &'a str)],
    Option<&'a str>,
    Availability,
);

#[tokio::test]
async fn a_changed_reference_clears_the_day_and_the_availability_unless_the_form_records_new_ones()
{
    // The form sends the day and the availability the source has (K66): they are not a new record.
    let source_file = source_path(&super::claims::source_id());
    let cases: [Case; 4] = [
        ("nothing new", &[], None, Availability::Unknown),
        (
            "a new day",
            &[("c0_reviewed", "2023-11-10")],
            Some("2023-11-10"),
            Availability::Available,
        ),
        (
            "a new availability",
            &[("c0_availability", "unavailable")],
            None,
            Availability::Unavailable,
        ),
        (
            "both",
            &[
                ("c0_reviewed", "2023-11-10"),
                ("c0_availability", "unavailable"),
            ],
            Some("2023-11-10"),
            Availability::Unavailable,
        ),
    ];
    for (what, more, day, availability) in cases {
        let s = checked_scene();
        let fields = with(with(checked_unchanged(&s), &elsewhere()), more);
        let files = files_of(&s, &location(&send(&s, FORM, &s.writer, &fields).await));
        assert_eq!(paths(&files), [source_file.as_str()], "{what}");
        let source = source_of(&files);
        assert_eq!(
            (
                source.last_reviewed.as_ref().map(|r| r.date.as_str()),
                source.availability
            ),
            (day, availability),
            "{what}"
        );
        assert_eq!(
            source.reference.as_ref().map(|r| r.value.as_str()),
            Some("10.1000/182"),
            "{what}"
        );
    }
    // Shown before it is sent, in both languages, as the check that goes.
    let s = checked_scene();
    let shown = with(
        with(checked_unchanged(&s), &elsewhere()),
        &[("op", "changes"), ("summary", "")],
    );
    let body = send(&s, FORM, &s.writer, &shown).await.body();
    assert!(
        body.contains("It is no longer marked as checked."),
        "{body}"
    );
    assert!(body.contains("10.1000/182"), "{body}");
    assert!(!body.contains("The review of"), "{body}");
    let ja = send_ja(&s, FORM, &shown).await;
    assert!(ja.contains("確認済みの印がなくなります。"), "{ja}");
    // The form says so beside the fields, in both languages.
    let hint =
        "A new place to find it clears the day it was last checked and whether it can be had";
    let body = open(&s, &claim_uri(&s), &s.writer).await.body();
    assert!(body.contains(hint), "{body}");
    let ja = page_ja(&s, &claim_uri(&s)).await;
    assert!(ja.contains("見つかる場所を変えると"), "{ja}");
}

#[tokio::test]
async fn a_correction_of_the_title_keeps_the_check_and_the_proposal_says_the_review_stays_and_whose_claims_rely_on_it()
 {
    let s = shared_scene();
    let fields = with(
        checked_unchanged(&s),
        &[
            ("c0_title", "A history of the bridge, second edition"),
            ("c0_author", "Noor Aziz"),
            ("c0_date", "2024"),
        ],
    );
    let stays = "The review of 2026-11-01 stays: accept only if this is still the same work.";
    let stays_ja =
        "2026-11-01 の確認はそのまま残ります。同じ資料のままである場合にだけ受け入れてください。";
    let other = other_claim_id().to_string();
    // Before it is sent.
    let shown = with(fields.clone(), &[("op", "changes"), ("summary", "")]);
    let body = send(&s, FORM, &s.writer, &shown).await.body();
    assert!(body.contains(stays), "{body}");
    assert!(
        body.contains("Another claim that cites it, and that this proposal does not change:"),
        "{body}"
    );
    assert!(body.contains(&other), "{body}");
    let ja = send_ja(&s, FORM, &shown).await;
    assert!(ja.contains(stays_ja), "{ja}");
    assert!(ja.contains("この提案では変更されない別の主張"), "{ja}");
    // Sent: the day and the availability stay in the file, and the proposal's page says the same.
    let made = location(&send(&s, FORM, &s.writer, &fields).await);
    let files = files_of(&s, &made);
    let source = source_of(&files);
    assert_eq!(
        (
            source.last_reviewed.as_ref().map(|r| r.date.as_str()),
            source.availability
        ),
        (Some("2026-11-01"), Availability::Available)
    );
    let page = open(&s, &made, &s.writer).await.body();
    assert!(page.contains(stays), "{page}");
    assert!(page.contains(&other), "{page}");
    let ja = page_ja(&s, &made).await;
    assert!(ja.contains(stays_ja), "{ja}");
    assert!(ja.contains(&other), "{ja}");
    // A correction that also moves the reference does not say it: the check went with it.
    let moved = with(
        with(fields, &elsewhere()),
        &[("op", "changes"), ("summary", "")],
    );
    let body = send(&s, FORM, &s.writer, &moved).await.body();
    assert!(!body.contains("The review of"), "{body}");
    assert!(
        body.contains("It is no longer marked as checked."),
        "{body}"
    );
}

#[tokio::test]
async fn a_change_of_white_space_alone_is_shown_as_that_with_its_white_space_visible() {
    let s = checked_scene();
    let fields = with(
        checked_unchanged(&s),
        &[("c0_title", "A  history of the bridge")],
    );
    let shown = with(fields.clone(), &[("op", "changes"), ("summary", "")]);
    let body = send(&s, FORM, &s.writer, &shown).await.body();
    let (was, now) = (
        "A\u{2423}history\u{2423}of\u{2423}the\u{2423}bridge",
        "A\u{2423}\u{2423}history\u{2423}of\u{2423}the\u{2423}bridge",
    );
    assert!(
        body.contains("Only the white space in its title changes"),
        "{body}"
    );
    assert!(
        body.contains(&format!("<bdi>{was}</bdi> becomes <bdi>{now}</bdi>")),
        "{body}"
    );
    assert!(
        !body.contains("Who or what it is, as written, becomes"),
        "no line says X is now X: {body}"
    );
    assert!(!body.contains("The review of"), "{body}");
    let ja = send_ja(&s, FORM, &shown).await;
    assert!(ja.contains("タイトルの空白だけが変わります"), "{ja}");
    assert!(ja.contains(now), "{ja}");
    // Sent as the person wrote it, and the proposal's page shows it the same way.
    let made = location(&send(&s, FORM, &s.writer, &fields).await);
    let files = files_of(&s, &made);
    assert_eq!(
        source_of(&files).identity.title.as_deref(),
        Some("A  history of the bridge")
    );
    let page = open(&s, &made, &s.writer).await.body();
    assert!(page.contains(now), "{page}");
}

/// A scene whose claim cites two sources: the first (checked, titled) is replaced by the second in the base, so naming the
/// first as what replaces the second closes a circle.
fn two_sources_scene() -> Scene {
    let s = scene();
    let mut first = checked_source();
    first.superseded_by = Some(later_id());
    let mut second = Source::new(later_id());
    second.identity.title = Some("A later history".to_owned());
    let mut claim = s.claim.clone();
    let mut link = claim.sources.first().expect("a link").clone();
    link.source = later_id();
    claim.sources.push(link);
    claim.normalize();
    s.w.fake.set_files(&[
        (source_path(&first.id).as_str(), first.to_yaml().as_str()),
        (source_path(&second.id).as_str(), second.to_yaml().as_str()),
        (claim_path(&claim.id).as_str(), claim.to_yaml().as_str()),
    ]);
    s
}

#[tokio::test]
async fn the_cycle_refusal_names_the_source_whose_chain_returns_in_a_form_of_several() {
    let s = two_sources_scene();
    let mut fields = checked_unchanged(&s);
    fields.retain(|(k, _)| *k != "links");
    fields.extend([
        ("links", "2".to_owned()),
        ("c1_source", later_id().to_string()),
        ("c1_relation", "supports".to_owned()),
        ("c1_bearing", "direct".to_owned()),
        ("c1_reviewed", String::new()),
        ("c1_availability", "unknown".to_owned()),
        ("c1_title", "A later history".to_owned()),
        ("c1_author", String::new()),
        ("c1_date", String::new()),
        ("c1_ref_form", "url".to_owned()),
        ("c1_ref", String::new()),
        // The first source is named as what replaces the second, which it is replaced by.
        ("c1_superseded_by", super::claims::source_id().to_string()),
    ]);
    // The first source's own fields are the base's: its successor is the second.
    let fields = with(fields, &[("c0_superseded_by", &later_id().to_string())]);
    let a = send(&s, FORM, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = a.body();
    assert!(
        body.contains(
            "Source: A later history. The sources that replace one another must not come back"
        ),
        "{body}"
    );
    assert!(
        !body.contains("Source: A history of the bridge. The sources"),
        "the other source is not named: {body}"
    );
    let ja = send_ja(&s, FORM, &fields).await;
    assert!(
        ja.contains("出典: A later history。置き換えの連なりが"),
        "{ja}"
    );
}

#[tokio::test]
async fn a_change_of_line_breaks_alone_cannot_be_made_in_the_form_and_the_form_says_so() {
    // K69: a browser takes line breaks out of a field's value, so a text sent without them is the base's own.
    let s = scene();
    let mut source = checked_source();
    source.identity.title = Some("Two\nlines".to_owned());
    s.w.fake
        .set_files(&[(source_path(&source.id).as_str(), source.to_yaml().as_str())]);
    let fields = with(checked_unchanged(&s), &[("c0_title", "Twolines")]);
    let a = send(&s, FORM, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body().contains("Nothing in the form differs"),
        "nothing is proposed: {}",
        a.body()
    );
    let body = open(&s, &claim_uri(&s), &s.writer).await.body();
    assert!(
        body.contains("a change of line breaks alone cannot be made here: make it through the API"),
        "{body}"
    );
    let ja = page_ja(&s, &claim_uri(&s)).await;
    assert!(
        ja.contains("改行だけの変更はここではできません。API で行ってください。"),
        "{ja}"
    );
}
