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

/// [`checked_scene`], the source last checked on `day` (the tests' clock is 2023-11-14T22:13Z, so a source checked before it
/// can have a new reference checked on a day that has come).
fn checked_scene_on(day: &str) -> Scene {
    let s = scene();
    let mut source = checked_source();
    // Checked by a proposal, as a check made through the service is: the acceptance names it.
    source.last_reviewed = Some(inklkunn_core::knowledge::Reviewed {
        date: day.to_owned(),
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
    });
    let mut later = Source::new(later_id());
    later.identity.title = Some("A later history".to_owned());
    s.w.fake.set_files(&[
        (source_path(&source.id).as_str(), source.to_yaml().as_str()),
        (source_path(&later.id).as_str(), later.to_yaml().as_str()),
    ]);
    s
}

/// What the form of [`checked_scene_on`] sends when nothing is changed.
fn unchanged_on(s: &Scene, day: &str) -> Vec<(&'static str, String)> {
    with(checked_unchanged(s), &[("c0_reviewed", day)])
}

type Case<'a> = (
    &'a str,
    &'a [(&'static str, &'a str)],
    Option<&'a str>,
    Availability,
);

#[tokio::test]
async fn a_changed_reference_ends_the_check_and_the_new_one_has_only_the_check_the_form_states() {
    // K66, K72, K76. The form sends the day and the availability the source has: they are the old reference's, and stay
    // as they are beside a new one (a change is refused, below). The check of the new reference is its own pair of fields,
    // empty as the form opens.
    let cases: [Case; 5] = [
        ("nothing new", &[], None, Availability::Unknown),
        (
            "the check of the new reference",
            &[
                ("c0_checked_day", "2023-11-10"),
                ("c0_checked_availability", "unavailable"),
            ],
            Some("2023-11-10"),
            Availability::Unavailable,
        ),
        (
            "the check of the new reference, on the very date of the old one",
            &[
                ("c0_checked_day", "2023-11-01"),
                ("c0_checked_availability", "available"),
            ],
            Some("2023-11-01"),
            Availability::Available,
        ),
        (
            "a day of the new reference, no answer",
            &[
                ("c0_checked_day", "2023-11-12"),
                ("c0_checked_availability", "unknown"),
            ],
            Some("2023-11-12"),
            Availability::Unknown,
        ),
        (
            "empty fields for the new reference",
            &[
                ("c0_checked_day", "  "),
                ("c0_checked_availability", "unknown"),
            ],
            None,
            Availability::Unknown,
        ),
    ];
    let source_file = source_path(&super::claims::source_id());
    for (what, more, day, availability) in cases {
        let s = checked_scene_on("2023-11-01");
        let fields = with(with(unchanged_on(&s, "2023-11-01"), &elsewhere()), more);
        let address = location(&send(&s, FORM, &s.writer, &fields).await);
        let files = files_of(&s, &address);
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
        // The check of the new reference names the proposal that states it, on whatever date (K77); nothing else does.
        let named = source
            .last_reviewed
            .as_ref()
            .and_then(|r| r.proposal.as_deref());
        let made = address
            .rsplit('/')
            .next()
            .and_then(|last| last.split('?').next())
            .expect("the proposal's id");
        assert_eq!(named, day.map(|_| made), "{what}");
        assert_eq!(
            source.reference.as_ref().map(|r| r.value.as_str()),
            Some("10.1000/182"),
            "{what}"
        );
    }
    // The form opens with the fields for the new reference empty, whatever the source's own check is, and they are
    // there for every source whose file can be read.
    let s = checked_scene_on("2023-11-01");
    let body = open(&s, &claim_uri(&s), &s.writer).await.body();
    assert!(
        body.contains("The day you checked the new reference"),
        "{body}"
    );
    assert!(
        body.contains("name=\"c0_checked_day\" type=\"text\" value=\"\""),
        "{body}"
    );
    assert!(
        body.contains("Whether it can be had at the new reference"),
        "{body}"
    );
    assert!(body.contains("name=\"c0_checked_availability\""), "{body}");
    assert!(
        body.contains("value=\"2023-11-01\""),
        "the old field shows the old check: {body}"
    );
    let ja = page_ja(&s, &claim_uri(&s)).await;
    assert!(ja.contains("新しい参照先を確認した日"), "{ja}");
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
    let hint = "A new place to find it ends the check of the old one: the day it was last checked and whether it can be had, above, stay as they are, are not carried to the new place";
    let body = open(&s, &claim_uri(&s), &s.writer).await.body();
    assert!(body.contains(hint), "{body}");
    let ja = page_ja(&s, &claim_uri(&s)).await;
    assert!(ja.contains("見つかる場所を変えると"), "{ja}");
}

#[tokio::test]
async fn a_check_of_a_new_reference_that_is_not_one_is_refused_on_the_form_naming_the_source_and_nothing_is_made()
 {
    // K72. The source was last checked on 2023-11-01; the clock is 2023-11-14T22:13Z.
    let s = checked_scene_on("2023-11-01");
    let base = |more: &[(&'static str, &str)]| with(unchanged_on(&s, "2023-11-01"), more);
    let moved = |more: &[(&'static str, &str)]| with(base(&elsewhere()), more);
    // The source by its title and its whole id in `<code>` (K78): two sources may share a title.
    let source = format!(
        "Source: A history of the bridge (<code>{}</code>).",
        super::claims::source_id()
    );
    let cases = [
        (
            "a day earlier than the old check",
            moved(&[
                ("c0_checked_day", "2023-10-31"),
                ("c0_checked_availability", "available"),
            ]),
            "cannot be earlier than 2023-11-01, the day the old reference was last checked",
            "value=\"2023-10-31\"",
        ),
        (
            "an answer with no day",
            moved(&[("c0_checked_availability", "available")]),
            "needs the day you checked it. Give the day, or leave the answer as not known.",
            "<option value=\"available\" selected>",
        ),
        (
            "a day that is no day",
            moved(&[
                ("c0_checked_day", "yesterday"),
                ("c0_checked_availability", "available"),
            ]),
            "has to be written year, month, day, like 2026-11-04",
            "value=\"yesterday\"",
        ),
        (
            "a day that has not come",
            moved(&[
                ("c0_checked_day", "2023-11-16"),
                ("c0_checked_availability", "available"),
            ]),
            "The day 2023-11-16 has not come yet in any timezone",
            "value=\"2023-11-16\"",
        ),
        (
            "a check and no new reference",
            base(&[
                ("c0_checked_day", "2023-11-10"),
                ("c0_checked_availability", "available"),
            ]),
            "goes with a change of where the source is found; a change of white space alone does not move the reference. Change where it is found, or leave the day and the answer empty.",
            "value=\"2023-11-10\"",
        ),
    ];
    for (what, fields, said, kept) in cases {
        let a = send(&s, FORM, &s.writer, &fields).await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{what}");
        let body = a.body();
        // Every one names the source by its title and its whole id, a day that has not come too (K80).
        assert!(body.contains(source.as_str()), "{what}: {body}");
        assert!(body.contains(said), "{what}: {body}");
        // What the person sent is still in the fields.
        assert!(body.contains(kept), "{what}: {body}");
    }
    // In Japanese too, with the day it cannot come before.
    let ja = send_ja(
        &s,
        FORM,
        &moved(&[
            ("c0_checked_day", "2023-10-31"),
            ("c0_checked_availability", "available"),
        ]),
    )
    .await;
    assert!(ja.contains("2023-11-01"), "{ja}");
    assert!(ja.contains("より前にはできません"), "{ja}");
    assert!(
        ja.contains(&format!(
            "出典: A history of the bridge（<code>{}</code>）。",
            super::claims::source_id()
        )),
        "{ja}"
    );
    // Nothing was made by any of them.
    assert_eq!(
        inklkunn_store::ProposalStore::open_count(&*s.w.store, &s.w.wiki.id, &s.writer).unwrap(),
        0
    );
}

#[tokio::test]
async fn the_old_fields_beside_a_new_reference_are_refused_on_the_form_by_their_field_and_nothing_is_made()
 {
    // K76. The source was last checked on 2023-11-01 and is available; the clock is 2023-11-14T22:13Z. The form sends the
    // base's day and answer unless a person changes them, and beside a new reference a change is refused: a valid new
    // day, a day that is no day and a day that has not come are refused the same way, never discarded with a sent proposal.
    let s = checked_scene_on("2023-11-01");
    let moved = |more: &[(&'static str, &str)]| {
        with(with(unchanged_on(&s, "2023-11-01"), &elsewhere()), more)
    };
    // The old value must stay as the base has it (the move clears it), and the check of the new place goes in its own field (K80).
    let day = "and that clears the day it was last checked, which is for the old place. Leave that day as the base of the proposal has it (put it back if you cleared or changed it), and give the day you checked the new place in the field for the new reference.";
    let answer = "and that clears whether it can be had, which is for the old place. Leave that answer as the base of the proposal has it (put it back if you changed it), and give what you found at the new place in the field for the new reference.";
    // The source by its title and its whole id in `<code>`, as the other checks do (K80).
    let source = format!(
        "Source: A history of the bridge (<code>{}</code>).",
        super::claims::source_id()
    );
    let cases = [
        (
            "a valid new day",
            moved(&[("c0_reviewed", "2023-11-10")]),
            day,
            "value=\"2023-11-10\"",
        ),
        (
            "a day that is no day",
            moved(&[("c0_reviewed", "yesterday")]),
            day,
            "value=\"yesterday\"",
        ),
        (
            "a day that has not come",
            moved(&[("c0_reviewed", "2023-11-16")]),
            day,
            "value=\"2023-11-16\"",
        ),
        (
            "the day cleared",
            moved(&[("c0_reviewed", "")]),
            day,
            "name=\"c0_reviewed\" type=\"text\" value=\"\"",
        ),
        (
            "another answer",
            moved(&[("c0_availability", "unavailable")]),
            answer,
            "<option value=\"unavailable\" selected>",
        ),
    ];
    for (what, fields, said, kept) in cases {
        let a = send(&s, FORM, &s.writer, &fields).await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{what}");
        let body = a.body();
        assert!(body.contains(source.as_str()), "{what}: {body}");
        assert!(body.contains(said), "{what}: {body}");
        assert!(body.contains(kept), "{what}: {body}");
    }
    // The same in the preview of the changes: the refusal comes before anything is shown.
    let shown = with(
        moved(&[("c0_reviewed", "2023-11-10")]),
        &[("op", "changes"), ("summary", "")],
    );
    let a = send(&s, FORM, &s.writer, &shown).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    // In Japanese too.
    let ja = send_ja(&s, FORM, &moved(&[("c0_reviewed", "2023-11-10")])).await;
    assert!(
        ja.contains("古い場所について最後に確認した日は消えます。その日は、提案の基にした時点のままにしてください"),
        "{ja}"
    );
    // What the base has, sent back, is not a change: the move goes through and clears the check.
    let files = files_of(&s, &location(&send(&s, FORM, &s.writer, &moved(&[])).await));
    let source = source_of(&files);
    assert_eq!(
        (source.last_reviewed, source.availability),
        (None, Availability::Unknown)
    );
    // Nothing else was made: the one proposal above is the only one.
    assert_eq!(
        inklkunn_store::ProposalStore::open_count(&*s.w.store, &s.w.wiki.id, &s.writer).unwrap(),
        1
    );
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
        body.contains("A claim that cites it, and that this proposal does not change:"),
        "{body}"
    );
    assert!(body.contains(&other), "{body}");
    let ja = send_ja(&s, FORM, &shown).await;
    assert!(ja.contains(stays_ja), "{ja}");
    assert!(ja.contains("この提案では変更されない主張"), "{ja}");
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
async fn the_cited_by_line_lists_the_claims_that_cite_the_source_at_the_tip_and_not_at_the_base() {
    // K73: the reviewer accepts at the tip. A claim that began citing the source after the proposal was made is named; one
    // that stopped is not.
    let s = shared_scene();
    let fields = with(
        checked_unchanged(&s),
        &[("c0_title", "A history of the bridge, second edition")],
    );
    let made = location(&send(&s, FORM, &s.writer, &fields).await);
    let kept = other_claim_id();
    let began = inklkunn_core::knowledge::ClaimId::parse("01930000-0000-7000-8000-000000000003")
        .expect("a claim id");
    let mut third = s.claim.clone();
    third.id = began.clone();
    let mut stopped = s.claim.clone();
    stopped.id = kept.clone();
    stopped.sources.clear();
    s.w.fake.set_files(&[
        (claim_path(&began).as_str(), third.to_yaml().as_str()),
        (claim_path(&kept).as_str(), stopped.to_yaml().as_str()),
    ]);
    let page = open(&s, &made, &s.writer).await.body();
    assert!(
        page.contains("A claim that cites it, and that this proposal does not change:"),
        "{page}"
    );
    assert!(page.contains(&began.to_string()), "{page}");
    assert!(!page.contains(&kept.to_string()), "{page}");
    let ja = page_ja(&s, &made).await;
    assert!(ja.contains(&began.to_string()), "{ja}");
    assert!(!ja.contains(&kept.to_string()), "{ja}");
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
        body.contains(&format!(
            "Source: A later history (<code>{}</code>). The sources that replace one another must not come back",
            later_id()
        )),
        "{body}"
    );
    assert!(
        !body.contains("Source: A history of the bridge ("),
        "the other source is not named: {body}"
    );
    let ja = send_ja(&s, FORM, &fields).await;
    assert!(
        ja.contains(&format!(
            "出典: A later history（<code>{}</code>）。置き換えの連なりが",
            later_id()
        )),
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

#[tokio::test]
async fn a_draft_that_moves_a_source_and_leaves_its_check_says_so_on_its_page_before_it_is_accepted()
 {
    // K78 (the review's F-c): the rule the submission and the acceptance have (K71) is said on the page of a draft that holds
    // such a file. The page is not the rule: it only warns, and says it cannot be accepted as it is.
    let s = checked_scene_on("2023-11-01");
    let moved = |check_kept: bool| {
        let mut source = checked_source();
        source.last_reviewed = Some(inklkunn_core::knowledge::Reviewed {
            date: "2023-11-01".to_owned(),
            proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        });
        source.reference = Some(inklkunn_core::knowledge::Reference {
            form: inklkunn_core::knowledge::ReferenceForm::Doi,
            value: "10.1000/182".to_owned(),
        });
        if !check_kept {
            source.last_reviewed = None;
            source.availability = Availability::Unknown;
        }
        vec![(source_path(&source.id), Some(source.to_yaml()))]
    };
    let note = "carries a day it was last checked, or whether it can be had, that this proposal does not state";
    let to = super::claims::held_unjudged(&s, moved(true));
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains(note), "{body}");
    // It names the way to state one (K80).
    assert!(body.contains("`checked`"), "{body}");
    assert!(body.contains("`last_reviewed.proposal`"), "{body}");
    assert!(body.contains("It cannot be accepted as it is."), "{body}");
    // A move that clears the check says nothing of the kind.
    let to = super::claims::held_unjudged(&s, moved(false));
    let body = open(&s, &to, &s.writer).await.body();
    assert!(!body.contains(note), "{body}");
}
