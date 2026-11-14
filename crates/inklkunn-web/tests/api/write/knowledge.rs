//! The write API's knowledge layer (RFC 005 part 2 D-3, D-12; handoff 005 K-T11): the curation proposal, and the explicit
//! drop of a paragraph that claims are said in. Through the real router over an in-memory repository, so these run in both
//! test runs. A curation is a proposal like any other: it changes nothing until people accept it, it is judged by the rules
//! the form is, and what only it may set is set by nothing else. Every sentence is invented.

use inklkunn_core::knowledge::{
    Bearing, File, Link, SourceKind, Standing, claim_path, read_file, source_path,
};
use inklkunn_store::{AccountId, ProposalId, Role};
use serde_json::{Value, json};

use super::super::common::{
    HOME_ID, World, add_person, claim, claim_id, files, home_with, link, person, source, source_id,
    state_id, with_accounts,
};
use super::super::{At, get};
use super::{API, id_of, post, principal, problem_type, session, tip, token, urn};

const SAID: &str = "The bridge opened in 1950.";

struct Cast {
    w: World,
    contributor: AccountId,
    observer: AccountId,
}

/// A wiki whose `Home` says one claim in its first paragraph (undecided, citing two sources, judged on the words as they
/// are), already in version 1, so that a proposal's base is `state_id(1)`.
fn cast() -> Cast {
    cast_of(&[
        source(1, SourceKind::Primary),
        source(2, SourceKind::Secondary),
    ])
}

/// [`cast`], with these two sources.
fn cast_of(sources: &[inklkunn_core::knowledge::Source]) -> Cast {
    let mut c = claim(1, Standing::Unknown, None, SAID);
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, SAID),
        link(2, Link::Challenges, Bearing::Partial, SAID),
    ];
    let mut all = files(&[c], sources);
    all.push(("pages/Home.md".to_owned(), home_with(SAID)));
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let w = with_accounts(World::new("handbook", "The Handbook", &borrowed));
    let cast = Cast {
        contributor: person(1),
        observer: person(6),
        w,
    };
    add_person(
        &cast.w,
        &cast.contributor,
        "Aiko Tanaka",
        Some(Role::Contributor),
    );
    add_person(&cast.w, &cast.observer, "Olu Reader", Some(Role::Observer));
    cast
}

fn curation(body: Value) -> String {
    let mut all = json!({"claim": claim_id(1).to_string(), "summary": "Accept what the survey says", "submit": true});
    if let (Some(into), Value::Object(from)) = (all.as_object_mut(), body) {
        into.extend(from);
    }
    all.to_string()
}

async fn propose(c: &Cast, body: Value) -> super::super::Answer {
    post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[session(&c.contributor), tip()],
        curation(body),
    )
    .await
}

/// The files the proposal carries, by path.
fn stored(c: &Cast, answer: &super::super::Answer) -> Vec<(String, Option<String>)> {
    let id: ProposalId = id_of(&answer.json());
    c.w.services
        .proposals
        .revisions(&principal(&c.contributor), &c.w.wiki, &id)
        .unwrap()
        .pop()
        .map(|r| r.content.files)
        .or_else(|| {
            c.w.services
                .proposals
                .draft(&principal(&c.contributor), &c.w.wiki, &id)
                .ok()
                .map(|d| d.files)
        })
        .unwrap()
        .into_iter()
        .map(|f| (f.path, f.text))
        .collect()
}

fn text_at(files: &[(String, Option<String>)], path: &str) -> String {
    files
        .iter()
        .find(|(p, _)| p == path)
        .and_then(|(_, t)| t.clone())
        .unwrap_or_else(|| panic!("the proposal has no `{path}`: {files:?}"))
}

#[tokio::test]
async fn a_curation_proposal_sets_a_status_a_link_and_a_check_in_a_proposal_that_changes_nothing_until_accepted()
 {
    let c = cast();
    let a = propose(
        &c,
        json!({
            "status": "accepted",
            "links": [{"source": source_id(2).to_string(), "relation": "supports", "bearing": "indirect"}],
            "sources": [{"source": source_id(1).to_string(), "reviewed": "2023-11-01", "availability": "available"}],
        }),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    assert_eq!(a.json().at("/state"), "held-for-review");
    assert_eq!(a.json().at("/revision"), 1);
    assert!(
        a.header("location")
            .is_some_and(|l| l.contains("/proposals/"))
    );
    let files = stored(&c, &a);
    // The claim and the one source whose check was proposed: the other source is not touched.
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths.len(), 2, "{paths:?}");
    let claim_text = text_at(&files, &claim_path(&claim_id(1)));
    let Ok(File::Claim(written)) = read_file(&claim_path(&claim_id(1)), claim_text.as_bytes())
    else {
        panic!("the claim does not read: {claim_text}");
    };
    assert_eq!(written.community_status.status, Standing::Accepted);
    // The deciding proposal is named by the acceptance, not by the one who proposes.
    assert_eq!(written.community_status.proposal, None);
    let link = written
        .sources
        .iter()
        .find(|l| l.source == source_id(2))
        .unwrap();
    assert_eq!(
        (link.relation, link.bearing),
        (Link::Supports, Bearing::Indirect)
    );
    let source_text = text_at(&files, &source_path(&source_id(1)));
    assert!(
        source_text.contains("2023-11-01") && source_text.contains("available"),
        "{source_text}"
    );
    // Nothing in the repository changed: a status is set by a proposal that people accept.
    assert_eq!(
        get(&c.w, &format!("{API}/pages?limit=1"))
            .await
            .json()
            .at("/tip/state_id"),
        &json!(state_id(1))
    );
    let claim_now = get(&c.w, &format!("{API}/claims/{}", claim_id(1)))
        .await
        .json();
    assert_eq!(claim_now.at("/status/standing"), "unknown");
}

#[tokio::test]
async fn a_curation_that_leaves_the_base_as_it_is_or_names_what_is_not_there_is_refused_in_words() {
    let c = cast();
    let refused = |a: &super::super::Answer, reason: &str| {
        assert_eq!(a.status, 422, "{}", a.text());
        assert_eq!(problem_type(a), urn("unacceptable"));
        assert_eq!(a.json().at("/reason"), reason, "{}", a.text());
        assert!(
            a.json()
                .at("/detail")
                .as_str()
                .is_some_and(|e| !e.is_empty()),
            "{}",
            a.text()
        );
    };
    // The base already has these.
    let same = propose(
        &c,
        json!({"status": "unknown", "links": [{"source": source_id(1).to_string(), "relation": "supports", "bearing": "direct"}]}),
    )
    .await;
    refused(&same, "nothing-to-curate");
    // A source the claim does not cite.
    let stranger = propose(
        &c,
        json!({"sources": [{"source": source_id(9).to_string(), "reviewed": "", "availability": "unknown"}]}),
    )
    .await;
    refused(&stranger, "unknown-source");
    // A day that is not a day.
    let day = propose(
        &c,
        json!({"sources": [{"source": source_id(1).to_string(), "reviewed": "last week", "availability": "available"}]}),
    )
    .await;
    refused(&day, "not-a-day");
    // No summary, to send for review.
    let nameless = post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[session(&c.contributor), tip()],
        json!({"claim": claim_id(1).to_string(), "summary": " ", "submit": true, "status": "accepted"}).to_string(),
    )
    .await;
    refused(&nameless, "summary-missing");
    // Nothing was made by any of them.
    assert!(
        c.w.services
            .proposals
            .list(
                &principal(&c.contributor),
                &c.w.wiki,
                &inklkunn_store::ProposalFilter::default(),
            )
            .map(|l| l.is_empty())
            .unwrap_or(false)
    );
}

#[tokio::test]
async fn a_day_that_has_not_come_in_any_timezone_is_refused_at_its_member_before_a_proposal_exists()
{
    // The wiki's clock is 2023-11-14T22:13Z: it is already 2023-11-15 at UTC+14, and 2023-11-16 has not come anywhere (K49).
    let mut kept = source(2, SourceKind::Secondary);
    kept.last_reviewed = Some(inklkunn_core::knowledge::Reviewed {
        date: "2999-01-01".to_owned(),
        proposal: None,
    });
    let c = cast_of(&[source(1, SourceKind::Primary), kept]);
    let reviewed = |source: u8, day: &str| json!({"sources": [{"source": source_id(source).to_string(), "reviewed": day, "availability": "available"}]});
    let not_come = propose(&c, reviewed(1, "2023-11-16")).await;
    assert_eq!(not_come.status, 422, "{}", not_come.text());
    assert_eq!(problem_type(&not_come), urn("unacceptable"));
    assert_eq!(not_come.json().at("/reason"), "day-not-come");
    assert_eq!(not_come.json().at("/member"), "sources[0].reviewed");
    assert_eq!(
        not_come.json().at("/source"),
        &json!(source_id(1).to_string())
    );
    let detail = not_come
        .json()
        .at("/detail")
        .as_str()
        .unwrap_or("")
        .to_owned();
    assert!(detail.contains("2023-11-16"), "{detail}");
    // A day far ahead is the same.
    assert_eq!(propose(&c, reviewed(1, "2999-01-01")).await.status, 422);
    // Nothing was made.
    let list = || {
        c.w.services
            .proposals
            .list(
                &principal(&c.contributor),
                &c.w.wiki,
                &inklkunn_store::ProposalFilter::default(),
            )
            .unwrap()
    };
    assert!(list().is_empty());
    // The date in UTC+14 is not one that has not come: a reviewer there may have written it.
    let today_there = propose(&c, reviewed(1, "2023-11-15")).await;
    assert_eq!(today_there.status, 201, "{}", today_there.text());
    // A day the base already has is not a new record: it is left as it is, whatever it is.
    let unchanged = propose(
        &c,
        json!({"sources": [{"source": source_id(2).to_string(), "reviewed": "2999-01-01", "availability": "unavailable"}]}),
    )
    .await;
    assert_eq!(unchanged.status, 201, "{}", unchanged.text());
    // A different day on that source is a new record.
    let other = propose(&c, reviewed(2, "2999-01-02")).await;
    assert_eq!(other.status, 422, "{}", other.text());
    assert_eq!(other.json().at("/member"), "sources[0].reviewed");
    assert_eq!(list().len(), 2);
}

#[tokio::test]
async fn a_curation_is_judged_as_a_write_is_the_claim_the_words_the_base_the_role_and_the_scope() {
    let c = cast();
    // A claim the base does not hold, and an id that is not an id.
    let none = post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[session(&c.contributor), tip()],
        json!({"claim": claim_id(9).to_string(), "summary": "x", "submit": true, "status": "accepted"}).to_string(),
    )
    .await;
    assert_eq!(none.status, 404);
    assert_eq!(problem_type(&none), urn("not-found"));
    for (body, member) in [
        (
            json!({"claim": "nope", "summary": "x", "submit": true}),
            "claim",
        ),
        (
            json!({"claim": claim_id(1).to_string(), "summary": "x", "submit": true, "status": "true"}),
            "status",
        ),
        (
            json!({"claim": claim_id(1).to_string(), "summary": "x", "submit": true, "links": [{"source": source_id(1).to_string(), "relation": "supports", "bearing": "very"}]}),
            "links[0].bearing",
        ),
        (
            json!({"claim": claim_id(1).to_string(), "summary": "x", "submit": true, "sources": [{"source": source_id(1).to_string(), "reviewed": 20231101}]}),
            "sources[0].reviewed",
        ),
        (
            json!({"claim": claim_id(1).to_string(), "summary": "x", "submit": true, "score": 5}),
            "score",
        ),
    ] {
        let a = post(
            &c.w,
            &format!("{API}/proposals/curation"),
            &[session(&c.contributor), tip()],
            body.to_string(),
        )
        .await;
        assert_eq!(a.status, 400, "{member}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-body"), "{member}");
        assert_eq!(a.json().at("/member"), member);
    }
    // A stale base is a conflict that carries the current one.
    let stale = post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[
            session(&c.contributor),
            ("if-match", format!("\"{}\"", state_id(7))),
        ],
        curation(json!({"status": "accepted"})),
    )
    .await;
    assert_eq!(stale.status, 409);
    assert_eq!(problem_type(&stale), urn("stale-base"));
    // An observer may not propose; a token that cannot propose cannot; a visitor who is nobody is asked to sign in.
    let observer = post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[session(&c.observer), tip()],
        curation(json!({"status": "accepted"})),
    )
    .await;
    assert_eq!(observer.status, 403);
    let read_only = post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[token("read", &c.contributor), tip()],
        curation(json!({"status": "accepted"})),
    )
    .await;
    assert_eq!(read_only.status, 403);
    let anonymous = post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[tip()],
        curation(json!({"status": "accepted"})),
    )
    .await;
    assert_eq!(problem_type(&anonymous), urn("authentication-required"));
    // A draft is the proposer's alone, and a bearer token with the scope does what the session does.
    let draft = post(
        &c.w,
        &format!("{API}/proposals/curation"),
        &[token("propose", &c.contributor), tip()],
        curation(json!({"status": "contested", "submit": false})),
    )
    .await;
    assert_eq!(draft.status, 201, "{}", draft.text());
    assert_eq!(draft.json().at("/state"), "draft");
    assert_eq!(draft.json().at("/revision"), 0);
}

#[tokio::test]
async fn a_curation_sets_a_judgement_only_on_the_wording_the_base_has_and_confirm_judges_it_again()
{
    // The paragraph was reworded after the claim was judged: the claim is decided on the earlier words.
    let c = cast();
    let mut decided = claim(1, Standing::Accepted, None, "The bridge opened in 1949.");
    decided.sources = vec![link(
        1,
        Link::Supports,
        Bearing::Direct,
        "The bridge opened in 1949.",
    )];
    let mut all = files(&[decided], &[source(1, SourceKind::Primary)]);
    all.push(("pages/Home.md".to_owned(), home_with(SAID)));
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let w = with_accounts(World::new("handbook", "The Handbook", &borrowed));
    add_person(&w, &c.contributor, "Aiko Tanaka", Some(Role::Contributor));
    let c = Cast { w, ..c };
    let a = propose(
        &c,
        json!({"confirm": true, "summary": "Confirm it on the words it has"}),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    let files = stored(&c, &a);
    let text = text_at(&files, &claim_path(&claim_id(1)));
    let Ok(File::Claim(written)) = read_file(&claim_path(&claim_id(1)), text.as_bytes()) else {
        panic!("{text}");
    };
    let now = super::super::common::digest_of(&format!("{SAID} ^b1"));
    assert_eq!(written.community_status.judged_on, Some(now.clone()));
    assert!(written.sources.iter().all(|l| l.judged_on == now));
}

// ------------------------------------------------------------------------------------ the explicit drop

/// A paragraph named as removed, with its page.
fn drop(path: &str, block: &str) -> Value {
    json!({"path": path, "marker": block})
}

fn without_the_paragraph() -> String {
    format!("---\ntitle: Home\nid: {HOME_ID}\n---\n\nPlain words, no claim.\n")
}

async fn page_proposal(c: &Cast, text: Value, drops: Value) -> super::super::Answer {
    post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        json!({
            "summary": "Take the first paragraph out",
            "submit": true,
            "files": [{"path": "pages/Home.md", "text": text}],
            "drops": drops,
        })
        .to_string(),
    )
    .await
}

#[tokio::test]
async fn removing_a_paragraph_that_claims_are_said_in_is_an_act_the_body_names() {
    let c = cast();
    // Left out and not named: refused, by the marker and the words it began with.
    let silent = page_proposal(&c, json!(without_the_paragraph()), json!([])).await;
    assert_eq!(silent.status, 422, "{}", silent.text());
    assert_eq!(problem_type(&silent), urn("unacceptable"));
    assert_eq!(silent.json().at("/reason"), "claim-paragraph-removed");
    let words = silent.json().at("/detail").as_str().unwrap().to_owned();
    assert!(
        words.contains("^b1") && words.contains("The bridge opened in 1950"),
        "{words}"
    );
    assert!(words.contains("`drops`"), "{words}");
    // Named: the proposal carries the page and the claim, said nowhere now; the claim stays.
    let named = page_proposal(
        &c,
        json!(without_the_paragraph()),
        json!([drop("pages/Home.md", "b1")]),
    )
    .await;
    assert_eq!(named.status, 201, "{}", named.text());
    let files = stored(&c, &named);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, ["pages/Home.md", claim_path(&claim_id(1)).as_str()]);
    let text = text_at(&files, &claim_path(&claim_id(1)));
    let Ok(File::Claim(written)) = read_file(&claim_path(&claim_id(1)), text.as_bytes()) else {
        panic!("{text}");
    };
    assert!(written.expressions.is_empty(), "{text}");
    assert_eq!(written.sources.len(), 2, "the claim keeps what it cites");
    // The repository is what it was: the reviewer sees the removal, and decides.
    assert_eq!(
        get(&c.w, &format!("{API}/pages?limit=1"))
            .await
            .json()
            .at("/tip/state_id"),
        &json!(state_id(1))
    );
}

#[tokio::test]
async fn a_name_in_drops_that_is_not_a_removed_paragraph_with_a_claim_is_refused_so_the_flag_stands_for_nothing_else()
 {
    let c = cast();
    // The paragraph is kept (reworded, marker kept): nothing is removed, so naming it is refused.
    let reworded = home_with("The bridge opened in the year 1950.");
    let kept = page_proposal(&c, json!(reworded), json!([drop("pages/Home.md", "b1")])).await;
    assert_eq!(kept.status, 422, "{}", kept.text());
    assert_eq!(kept.json().at("/reason"), "drop-not-removed");
    // A paragraph no claim is said in is removed freely, and naming it is refused.
    let plain = format!("---\ntitle: Home\nid: {HOME_ID}\n---\n\nThe bridge opened in 1950. ^b1\n");
    let free = page_proposal(&c, json!(plain.clone()), json!([])).await;
    assert_eq!(free.status, 201, "{}", free.text());
    assert_eq!(stored(&c, &free).len(), 1, "the claim is not touched");
    let odd = page_proposal(&c, json!(plain), json!([drop("pages/Home.md", "nowhere")])).await;
    assert_eq!(odd.json().at("/reason"), "drop-not-removed");
    // Kept as it is and no drops: the ordinary edit, with no claim file in it.
    let ordinary = page_proposal(
        &c,
        json!(home_with("The bridge opened in 1950, they say.")),
        json!([]),
    )
    .await;
    assert_eq!(ordinary.status, 201, "{}", ordinary.text());
    assert_eq!(stored(&c, &ordinary).len(), 1);
    // `drops` is a list of objects like `files` and `renames`: a bare text, or a member missing, is `invalid-body`.
    let bad = page_proposal(&c, json!(without_the_paragraph()), json!(["b1"])).await;
    assert_eq!(bad.status, 400);
    assert_eq!(bad.json().at("/member"), "drops[0]");
    let lacking = page_proposal(
        &c,
        json!(without_the_paragraph()),
        json!([{"path": "pages/Home.md"}]),
    )
    .await;
    assert_eq!(lacking.status, 400);
    assert_eq!(lacking.json().at("/member"), "drops[0].marker");
    // The page is named as well as the paragraph: the right marker on another page removes nothing here.
    let elsewhere = page_proposal(
        &c,
        json!(without_the_paragraph()),
        json!([drop("pages/Other.md", "b1")]),
    )
    .await;
    assert_eq!(elsewhere.status, 422, "{}", elsewhere.text());
    assert_eq!(elsewhere.json().at("/reason"), "claim-paragraph-removed");
}

#[tokio::test]
async fn removing_a_whole_page_that_claims_are_said_in_is_named_too() {
    let c = cast();
    let silent = page_proposal(&c, Value::Null, json!([])).await;
    assert_eq!(silent.status, 422, "{}", silent.text());
    assert_eq!(silent.json().at("/reason"), "claim-paragraph-removed");
    let named = page_proposal(&c, Value::Null, json!([drop("pages/Home.md", "b1")])).await;
    assert_eq!(named.status, 201, "{}", named.text());
    let files = stored(&c, &named);
    assert!(
        files
            .iter()
            .any(|(p, t)| p == "pages/Home.md" && t.is_none())
    );
    assert!(files.iter().any(|(p, _)| *p == claim_path(&claim_id(1))));
}

/// A wiki whose `Home` and `Annex` each say a claim in a paragraph marked `^b1` (the same marker on two pages).
fn two_pages() -> Cast {
    use inklkunn_core::knowledge::Expression;
    let annex_id = "page-annex";
    let mut second = claim(2, Standing::Unknown, None, SAID);
    second.expressions = vec![Expression::new(annex_id, "b1").unwrap()];
    let mut all = files(&[claim(1, Standing::Unknown, None, SAID), second], &[]);
    all.push(("pages/Home.md".to_owned(), home_with(SAID)));
    all.push((
        "pages/Annex.md".to_owned(),
        super::super::common::page(
            &["title: Annex", &format!("id: {annex_id}")],
            &format!("{SAID} ^b1\n\nPlain words, no claim.\n"),
        ),
    ));
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let cast = Cast {
        contributor: person(1),
        observer: person(6),
        w: with_accounts(World::new("handbook", "The Handbook", &borrowed)),
    };
    add_person(
        &cast.w,
        &cast.contributor,
        "Aiko Tanaka",
        Some(Role::Contributor),
    );
    cast
}

// ------------------------------------------------------------------------------------ every change (K44), one step (K47)

/// [`cast`], with the first source **checked** (a review day and an availability): from then on who and what it is
/// belongs to curation.
fn cast_checked() -> Cast {
    cast_checked_titled("The bridge survey")
}

/// [`cast_checked`], the first source titled exactly `title` (a space at its end or a line break in it, as an import or
/// older data may leave it).
fn cast_checked_titled(title: &str) -> Cast {
    let mut c = claim(1, Standing::Unknown, None, SAID);
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, SAID),
        link(2, Link::Challenges, Bearing::Partial, SAID),
    ];
    let mut first = source(1, SourceKind::Primary);
    first.identity.title = Some(title.to_owned());
    first.availability = inklkunn_core::knowledge::Availability::Available;
    first.last_reviewed = Some(inklkunn_core::knowledge::Reviewed {
        date: "2026-11-01".to_owned(),
        proposal: None,
    });
    let mut all = files(&[c], &[first, source(2, SourceKind::Secondary)]);
    all.push(("pages/Home.md".to_owned(), home_with(SAID)));
    let borrowed: Vec<(&str, &str)> = all.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let w = with_accounts(World::new("handbook", "The Handbook", &borrowed));
    let cast = Cast {
        contributor: person(1),
        observer: person(6),
        w,
    };
    add_person(
        &cast.w,
        &cast.contributor,
        "Aiko Tanaka",
        Some(Role::Contributor),
    );
    cast
}

async fn two_page_proposal(c: &Cast, drops: Value) -> super::super::Answer {
    let bare = |title: &str, id: &str| {
        super::super::common::page(
            &[&format!("title: {title}"), &format!("id: {id}")],
            "Plain words, no claim.\n",
        )
    };
    post(
        &c.w,
        &format!("{API}/proposals"),
        &[session(&c.contributor), tip()],
        json!({
            "summary": "Take the first paragraph out of both pages",
            "submit": true,
            "files": [
                {"path": "pages/Home.md", "text": bare("Home", HOME_ID)},
                {"path": "pages/Annex.md", "text": bare("Annex", "page-annex")},
            ],
            "drops": drops,
        })
        .to_string(),
    )
    .await
}

#[tokio::test]
async fn a_drop_names_its_page_so_a_marker_on_two_pages_is_two_acts_and_the_refusal_names_the_page()
{
    let c = two_pages();
    // Home's paragraph named, Annex's not: Annex is refused, by its path and the marker.
    let one = two_page_proposal(&c, json!([drop("pages/Home.md", "b1")])).await;
    assert_eq!(one.status, 422, "{}", one.text());
    assert_eq!(one.json().at("/reason"), "claim-paragraph-removed");
    let words = one.json().at("/detail").as_str().unwrap().to_owned();
    assert!(
        words.contains("^b1")
            && words.contains("pages/Annex.md")
            && !words.contains("pages/Home.md"),
        "{words}"
    );
    // A drop of a page that is not in the proposal is refused by its page.
    let other = two_page_proposal(
        &c,
        json!([
            drop("pages/Home.md", "b1"),
            drop("pages/Annex.md", "b1"),
            drop("pages/Elsewhere.md", "b1"),
        ]),
    )
    .await;
    assert_eq!(other.status, 422, "{}", other.text());
    assert_eq!(other.json().at("/reason"), "drop-not-removed");
    let words = other.json().at("/detail").as_str().unwrap().to_owned();
    assert!(
        words.contains("pages/Elsewhere.md") && words.contains("^b1"),
        "{words}"
    );
    // Both named: both claims lose their expression, in the same proposal.
    let both = two_page_proposal(
        &c,
        json!([drop("pages/Home.md", "b1"), drop("pages/Annex.md", "b1")]),
    )
    .await;
    assert_eq!(both.status, 201, "{}", both.text());
    let files = stored(&c, &both);
    for n in [1, 2] {
        let text = text_at(&files, &claim_path(&claim_id(n)));
        let Ok(File::Claim(written)) = read_file(&claim_path(&claim_id(n)), text.as_bytes()) else {
            panic!("{text}");
        };
        assert!(written.expressions.is_empty(), "{text}");
    }
}

fn source_in(files: &[(String, Option<String>)], n: u8) -> inklkunn_core::knowledge::Source {
    let text = text_at(files, &source_path(&source_id(n)));
    match read_file(&source_path(&source_id(n)), text.as_bytes()) {
        Ok(File::Source(source)) => *source,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_curation_lets_a_link_go_and_corrects_a_checked_source_in_one_proposal() {
    let c = cast_checked();
    let a = propose(
        &c,
        json!({
            "links": [{"source": source_id(2).to_string(), "remove": true}],
            "sources": [{
                "source": source_id(1).to_string(),
                "reviewed": "2026-11-01",
                "availability": "available",
                "title": "The bridge, second edition",
                "author": "Noor Aziz",
                "date": "2024",
                "reference": {"type": "doi", "value": "10.1000/182"},
                "superseded_by": source_id(2).to_string(),
            }],
        }),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    assert_eq!(a.json().at("/state"), "held-for-review");
    let files = stored(&c, &a);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths.len(), 2, "the claim and the one source: {paths:?}");
    let text = text_at(&files, &claim_path(&claim_id(1)));
    let Ok(File::Claim(written)) = read_file(&claim_path(&claim_id(1)), text.as_bytes()) else {
        panic!("{text}");
    };
    assert_eq!(
        written
            .sources
            .iter()
            .map(|l| l.source.clone())
            .collect::<Vec<_>>(),
        [source_id(1)]
    );
    let corrected = source_in(&files, 1);
    assert_eq!(
        (
            corrected.identity.title.as_deref(),
            corrected.identity.author.as_deref(),
            corrected.identity.date.as_deref()
        ),
        (
            Some("The bridge, second edition"),
            Some("Noor Aziz"),
            Some("2024")
        )
    );
    assert_eq!(
        corrected
            .reference
            .as_ref()
            .map(|r| (r.form.word(), r.value.as_str())),
        Some(("doi", "10.1000/182"))
    );
    assert_eq!(corrected.superseded_by, Some(source_id(2)));
    assert_eq!(
        corrected.last_reviewed.as_ref().map(|r| r.date.as_str()),
        Some("2026-11-01")
    );
    // A member of the source left out is left as the base has it: here the title alone, with the check repeated.
    let c = cast_checked();
    let a = propose(
        &c,
        json!({"sources": [{"source": source_id(1).to_string(), "reviewed": "2026-11-01", "availability": "available", "title": "Only the title"}]}),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    let files = stored(&c, &a);
    assert_eq!(files.len(), 1);
    let only = source_in(&files, 1);
    assert_eq!(only.identity.title.as_deref(), Some("Only the title"));
    assert_eq!(only.reference, None);
}

#[tokio::test]
async fn what_a_curation_may_not_say_about_a_link_or_a_source_is_refused_in_words() {
    let c = cast_checked();
    let unacceptable = |a: &super::super::Answer, reason: &str| {
        assert_eq!(a.status, 422, "{}", a.text());
        assert_eq!(problem_type(a), urn("unacceptable"));
        assert_eq!(a.json().at("/reason"), reason, "{}", a.text());
        assert!(
            a.json()
                .at("/detail")
                .as_str()
                .is_some_and(|e| !e.is_empty()),
            "{}",
            a.text()
        );
    };
    let one = source_id(1).to_string();
    let two = source_id(2).to_string();
    // A source that was not checked is corrected like one that was (K60): no page edit writes its file, and the
    // correction asserts no check. (In a wiki of its own: the proposal it makes is not one of "nothing was made" below.)
    let apart = cast_checked();
    let a = propose(
        &apart,
        json!({"sources": [{"source": two, "title": "Another", "superseded_by": one}]}),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    let unchecked = source_in(&stored(&apart, &a), 2);
    assert_eq!(unchecked.identity.title.as_deref(), Some("Another"));
    assert_eq!(unchecked.superseded_by, Some(source_id(1)));
    assert_eq!(
        (unchecked.availability, unchecked.last_reviewed),
        (inklkunn_core::knowledge::Availability::Unknown, None)
    );
    // A link changed and let go.
    let a = propose(
        &c,
        json!({"links": [
            {"source": two, "relation": "supports", "bearing": "direct"},
            {"source": two, "remove": true},
        ]}),
    )
    .await;
    unacceptable(&a, "removed-and-changed");
    // A link the claim does not have.
    let a = propose(
        &c,
        json!({"links": [{"source": source_id(9).to_string(), "remove": true}]}),
    )
    .await;
    unacceptable(&a, "unknown-source");
    // A source that replaces another is a source the wiki has, and not itself.
    let checked = |more: Value| {
        let mut body =
            json!({"source": one, "reviewed": "2026-11-01", "availability": "available"});
        if let (Some(into), Value::Object(from)) = (body.as_object_mut(), more) {
            into.extend(from);
        }
        json!({"sources": [body]})
    };
    let a = propose(
        &c,
        checked(json!({"superseded_by": source_id(9).to_string()})),
    )
    .await;
    unacceptable(&a, "unknown-successor");
    let a = propose(&c, checked(json!({"superseded_by": one.clone()}))).await;
    unacceptable(&a, "replaced-by-itself");
    // A source needs a title or a reference.
    let a = propose(
        &c,
        checked(json!({"title": "", "reference": {"type": "url", "value": ""}})),
    )
    .await;
    unacceptable(&a, "not-curatable");

    // A body that is not the shape: a removed link says nothing else, a successor is an identifier, a reference has
    // both members and a known type.
    for (body, member) in [
        (
            json!({"links": [{"source": two, "remove": true, "relation": "supports"}]}),
            "links[0].relation",
        ),
        (
            json!({"links": [{"source": two, "remove": "yes"}]}),
            "links[0].remove",
        ),
        (
            checked(json!({"superseded_by": "the later one"})),
            "sources[0].superseded_by",
        ),
        (
            checked(json!({"reference": {"type": "doi"}})),
            "sources[0].reference.value",
        ),
        (
            checked(json!({"reference": {"type": "fax", "value": "1"}})),
            "sources[0].reference.type",
        ),
        (checked(json!({"title": 5})), "sources[0].title"),
    ] {
        let a = propose(&c, body).await;
        assert_eq!(a.status, 400, "{member}: {}", a.text());
        assert_eq!(problem_type(&a), urn("invalid-body"), "{member}");
        assert_eq!(a.json().at("/member"), member);
    }
    // Nothing was made by any of them.
    assert!(
        c.w.services
            .proposals
            .list(
                &principal(&c.contributor),
                &c.w.wiki,
                &inklkunn_store::ProposalFilter::default(),
            )
            .map(|l| l.is_empty())
            .unwrap_or(false)
    );
}

#[tokio::test]
async fn a_curation_that_cannot_be_sent_leaves_no_draft_behind() {
    // K47: made and sent in one step, so a send the wiki's hourly limit refuses leaves nothing: not an unsent draft that
    // counts against its author and that the caller would have to find.
    let mut c = cast();
    let services = &c.w.services;
    let proposals = std::sync::Arc::new(
        inklkunn_core::proposal::ProposalService::new(
            c.w.store.clone(),
            c.w.store.clone(),
            c.w.store.clone(),
            (*services.authorizer).clone(),
            services.read.clone(),
            inklkunn_core::proposal::QuotaConfig {
                proposals_per_hour_per_wiki: 1,
                ..inklkunn_core::proposal::QuotaConfig::default()
            },
        )
        .with_clock(|| 1_700_000_000),
    );
    c.w.services = std::sync::Arc::new(inklkunn_web::Services {
        proposals,
        ..(*c.w.services).clone()
    });
    let first = propose(&c, json!({"status": "accepted"})).await;
    assert_eq!(first.status, 201, "{}", first.text());
    let second = propose(&c, json!({"status": "contested"})).await;
    assert_eq!(second.status, 429, "{}", second.text());
    assert!(
        second.json().get("draft").is_none(),
        "no draft is named: there is none: {}",
        second.text()
    );
    let mine = inklkunn_store::ProposalStore::proposals(
        &*c.w.store,
        &c.w.wiki.id,
        &Default::default(),
        Some(&c.contributor),
    )
    .expect("the author's own, drafts included");
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert!(
        mine.iter()
            .all(|p| p.state == inklkunn_store::ProposalState::HeldForReview),
        "{mine:?}"
    );
    assert_eq!(
        inklkunn_store::ProposalStore::open_count(&*c.w.store, &c.w.wiki.id, &c.contributor)
            .unwrap(),
        1
    );
}

#[tokio::test]
async fn a_curation_changes_only_the_members_it_names_and_an_empty_day_or_unknown_clears() {
    // K59: a title with a space at its end and a line break in it is never cleaned, judged or rewritten by a body that
    // does not name it, and the review day stays unless the body says otherwise.
    let untidy = "The bridge \nsurvey ";
    let one = source_id(1).to_string();
    let c = cast_checked_titled(untidy);
    let a = propose(
        &c,
        json!({"sources": [{"source": one, "author": "Noor Aziz"}]}),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    let files = stored(&c, &a);
    assert_eq!(files.len(), 1, "only the source: {files:?}");
    let only = source_in(&files, 1);
    assert_eq!(only.identity.title.as_deref(), Some(untidy));
    assert_eq!(only.identity.author.as_deref(), Some("Noor Aziz"));
    assert_eq!(
        only.last_reviewed.as_ref().map(|r| r.date.as_str()),
        Some("2026-11-01")
    );
    assert_eq!(
        only.availability,
        inklkunn_core::knowledge::Availability::Available
    );

    // Only a title: the review day and the answer stay.
    let c = cast_checked();
    let a = propose(
        &c,
        json!({"sources": [{"source": one, "title": "The bridge, revised"}]}),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    let revised = source_in(&stored(&c, &a), 1);
    assert_eq!(
        revised.identity.title.as_deref(),
        Some("The bridge, revised")
    );
    assert_eq!(
        revised.last_reviewed.as_ref().map(|r| r.date.as_str()),
        Some("2026-11-01")
    );
    assert_eq!(
        revised.availability,
        inklkunn_core::knowledge::Availability::Available
    );

    // `reviewed: ""` and `availability: unknown` clear, and each on its own.
    let a = propose(&c, json!({"sources": [{"source": one, "reviewed": ""}]})).await;
    assert_eq!(a.status, 201, "{}", a.text());
    let cleared = source_in(&stored(&c, &a), 1);
    assert_eq!(
        (cleared.last_reviewed.clone(), cleared.availability),
        (None, inklkunn_core::knowledge::Availability::Available)
    );
    let a = propose(
        &c,
        json!({"sources": [{"source": one, "availability": "unknown"}]}),
    )
    .await;
    assert_eq!(a.status, 201, "{}", a.text());
    let cleared = source_in(&stored(&c, &a), 1);
    assert_eq!(
        (
            cleared.last_reviewed.as_ref().map(|r| r.date.as_str()),
            cleared.availability
        ),
        (
            Some("2026-11-01"),
            inklkunn_core::knowledge::Availability::Unknown
        )
    );
}
