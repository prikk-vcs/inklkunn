//! The knowledge layer's read API (RFC 005 part 2 D-12; handoff 005 K-T11) through the real router over an in-memory
//! repository: a claim, a source, a page's provenance paragraph by paragraph and the comparison of two views. What
//! each says, that it says nothing that ranks, that an id that names nothing is the one `not-found` an id that is not an
//! id is, that a wiki you may not read answers as one that is not there, and that a proposal is named only to someone who
//! may read it. No prikk is needed, so these run in both test runs. Every sentence is invented.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{Bearing, Link, SourceKind, Standing};
use serde_json::Value;

use super::common::{
    World, block_id, claim, claim_id, home_with, link, op, page, put, shown, source, source_id,
};
use super::{At, get};

const SAID: &str = "The bridge opened in 1950.";
const SAID_NOW: &str = "The bridge was opened in 1950.";

fn as_json(a: super::common::Answer) -> Value {
    serde_json::from_str(&a.body()).expect("JSON")
}

fn problem(a: &super::Answer) -> String {
    a.json().at("/type").as_str().unwrap_or("").to_owned()
}

/// The handbook with a past: version 1 creates `Home` and `Guide`, version 2 gives `Home` a claim (contested, with two
/// sources, judged on the words as they were), version 3 rewords the paragraph (so the judgement is of an earlier
/// wording) and adds a second claim.
pub(crate) fn handbook() -> World {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Home.md", &home_with(SAID)),
            (
                "pages/Guide.md",
                &page(&["title: Guide"], "The guide, with no claim.\n"),
            ),
        ],
    );
    w.fake.say(1, "Create the handbook");
    w.fake.show(
        1,
        shown(vec![
            op("create-file", &["pages/Home.md"]),
            op("create-file", &["pages/Guide.md"]),
        ]),
    );
    let mut c = claim(1, Standing::Contested, None, SAID);
    c.sources = vec![
        link(1, Link::Supports, Bearing::Direct, SAID),
        link(2, Link::Challenges, Bearing::Partial, SAID),
    ];
    let mut s1 = source(1, SourceKind::Primary);
    s1.identity.title = Some("The Bridge Survey".to_owned());
    s1.identity.author = Some("The Harbour Authority".to_owned());
    s1.reference = Some(inklkunn_core::knowledge::Reference {
        form: inklkunn_core::knowledge::ReferenceForm::Url,
        value: "https://example.org/survey".to_owned(),
    });
    put(
        &w,
        &home_with(SAID),
        &[c],
        &[s1, source(2, SourceKind::Secondary)],
    );
    w.fake.say(2, "Add a claim");
    w.fake.show(
        2,
        shown(vec![
            op("create-file", &["knowledge/claims/x.yaml"]),
            op("edit-text", &["pages/Home.md"]),
        ]),
    );
    w
}

/// `handbook`, and the paragraph reworded in version 3, so that the claim was judged on an earlier wording.
pub(crate) fn reworded() -> World {
    let w = handbook();
    w.fake.set("pages/Home.md", &home_with(SAID_NOW));
    w.fake.say(3, "Reword the paragraph");
    w.fake
        .show(3, shown(vec![op("edit-text", &["pages/Home.md"])]));
    w
}

fn claim_uri(n: u8) -> String {
    format!("/handbook/-/api/v1/claims/{}", claim_id(n))
}

fn source_uri(n: u8) -> String {
    format!("/handbook/-/api/v1/sources/{}", source_id(n))
}

#[tokio::test]
async fn a_claim_says_what_kind_it_is_where_it_is_said_its_sources_and_its_status_and_nothing_that_ranks()
 {
    let w = handbook();
    let a = get(&w, &claim_uri(1)).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    let j = a.json();
    assert_eq!(j.at("/id"), &Value::from(claim_id(1).to_string()));
    assert_eq!(j.at("/kind"), "observation");
    assert_eq!(j.at("/lifecycle"), "proposed");
    assert_eq!(j.at("/status/standing"), "contested");
    // Nobody decided it by a proposal the records hold: no proposal is named, and none is said to be hidden.
    assert_eq!(j.at("/status/proposal"), &Value::Null);
    assert_eq!(j.at("/status/proposal_unlisted"), false);
    assert_eq!(j.at("/status/judged_on"), "current");
    let said = j.at("/said").as_array().unwrap();
    assert_eq!(said.len(), 1);
    assert_eq!(said.first().unwrap().at("/title"), "Home");
    assert_eq!(said.first().unwrap().at("/block"), "b1");
    assert_eq!(said.first().unwrap().at("/state"), "present");
    let sources = j.at("/sources").as_array().unwrap();
    assert_eq!(sources.len(), 2);
    let first = sources.first().unwrap();
    assert_eq!(first.at("/source"), &Value::from(source_id(1).to_string()));
    assert_eq!(first.at("/relation"), "supports");
    assert_eq!(first.at("/bearing"), "direct");
    assert_eq!(first.at("/judged_on"), "current");
    assert_eq!(first.at("/readable"), true);
    assert_eq!(sources.get(1).unwrap().at("/relation"), "challenges");
    assert_eq!(sources.get(1).unwrap().at("/bearing"), "partial");
    assert_eq!(j.at("/tip/view"), "main");
    assert_eq!(j.at("/relations").as_array().unwrap().len(), 0);
    // Not a score, a rank or a verdict anywhere in the answer.
    let text = a.text().to_lowercase();
    for word in [
        "score",
        "rank",
        "verified",
        "true",
        "false",
        "canonical",
        "official",
    ] {
        assert!(!text.contains(&format!("\"{word}\"")), "{word}: {text}");
    }
    assert!(a.header("etag").is_some());
}

#[tokio::test]
async fn a_claim_judged_on_an_earlier_wording_says_so_for_its_status_and_each_source() {
    let w = reworded();
    let j = get(&w, &claim_uri(1)).await.json();
    assert_eq!(j.at("/status/judged_on"), "earlier");
    for source in j.at("/sources").as_array().unwrap() {
        assert_eq!(source.at("/judged_on"), "earlier");
    }
}

#[tokio::test]
async fn a_source_says_what_it_says_of_itself_and_the_claims_that_rely_on_it_and_fetches_nothing() {
    let w = handbook();
    let a = get(&w, &source_uri(1)).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    let j = a.json();
    assert_eq!(j.at("/kind"), "primary");
    assert_eq!(j.at("/availability"), "unknown");
    assert_eq!(j.at("/last_reviewed"), &Value::Null);
    assert_eq!(j.at("/title"), "The Bridge Survey");
    assert_eq!(j.at("/author"), "The Harbour Authority");
    assert_eq!(j.at("/reference"), "https://example.org/survey");
    assert_eq!(j.at("/claims_total"), 1);
    let cited = j.at("/claims").as_array().unwrap();
    assert_eq!(
        cited.first().unwrap().at("/claim"),
        &Value::from(claim_id(1).to_string())
    );
    assert_eq!(cited.first().unwrap().at("/relation"), "supports");
    // The second source says nothing of itself, and challenges the claim.
    let j = get(&w, &source_uri(2)).await.json();
    assert_eq!(j.at("/title"), &Value::Null);
    assert_eq!(
        j.at("/claims")
            .as_array()
            .unwrap()
            .first()
            .unwrap()
            .at("/relation"),
        "challenges"
    );
}

#[tokio::test]
async fn an_id_that_names_nothing_is_the_one_not_found_an_id_that_is_not_an_id_is() {
    let w = handbook();
    let mut answers = Vec::new();
    for uri in [
        // A claim id the wiki does not hold, a source id given to claims, a claim id given to sources, and ids that are no ids.
        claim_uri(9),
        format!("/handbook/-/api/v1/claims/{}", source_id(1)),
        format!("/handbook/-/api/v1/sources/{}", claim_id(1)),
        "/handbook/-/api/v1/claims/not-an-id".to_owned(),
        "/handbook/-/api/v1/claims/..%2f..".to_owned(),
        "/handbook/-/api/v1/sources/not-an-id".to_owned(),
    ] {
        let a = get(&w, &uri).await;
        assert_eq!(a.status, StatusCode::NOT_FOUND, "{uri}");
        assert_eq!(problem(&a), "urn:inklkunn:problem:not-found", "{uri}");
        answers.push(a.bytes);
    }
    assert!(answers.windows(2).all(|pair| pair.first() == pair.get(1)));
}

#[tokio::test]
async fn the_view_a_claim_is_read_in_is_named_or_it_is_no_such_view() {
    let w = handbook();
    let a = get(&w, &format!("{}?view=nope", claim_uri(1))).await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(problem(&a), "urn:inklkunn:problem:no-such-view");
    let a = get(&w, &format!("{}?bogus=1", claim_uri(1))).await;
    assert_eq!(problem(&a), "urn:inklkunn:problem:invalid-parameter");
}

#[tokio::test]
async fn provenance_says_for_each_paragraph_which_change_last_changed_it_and_which_wrote_it() {
    let w = reworded();
    let a = get(&w, "/handbook/-/api/v1/provenance/Home").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    let j = a.json();
    assert_eq!(j.at("/title"), "Home");
    assert_eq!(j.at("/reach"), "whole");
    assert_eq!(j.at("/left_out"), 0);
    let paragraphs = j.at("/paragraphs").as_array().unwrap();
    assert_eq!(paragraphs.len(), 2);
    let claimed = paragraphs.first().unwrap();
    // The paragraph with a marker is followed through it, and the claim said in it is named.
    assert_eq!(claimed.at("/id"), "b1");
    assert_eq!(claimed.at("/basis"), "anchor");
    assert_eq!(
        claimed.at("/claims").as_array().unwrap().first().unwrap(),
        &Value::from(claim_id(1).to_string())
    );
    assert_eq!(claimed.at("/last_changed"), 3);
    assert_eq!(claimed.at("/first_written"), 1);
    // The other was not changed after it was written, and is traced by its text.
    let plain = paragraphs.get(1).unwrap();
    assert_eq!(plain.at("/id"), &Value::Null);
    assert_eq!(plain.at("/basis"), "text");
    assert_eq!(plain.at("/claims").as_array().unwrap().len(), 0);
    // The changes the paragraphs name, newest first, with what the messages say; no proposal is tied to them.
    let changes = j.at("/changes").as_array().unwrap();
    let numbers: Vec<u64> = changes
        .iter()
        .filter_map(|c| c.at("/number").as_u64())
        .collect();
    assert_eq!(numbers, [3, 1]);
    assert_eq!(
        changes.first().unwrap().at("/summary"),
        "Reword the paragraph"
    );
    assert_eq!(
        changes.first().unwrap().at("/id"),
        &Value::from(block_id(3))
    );
    for change in changes {
        assert_eq!(change.at("/proposal"), &Value::Null);
    }
    // A title no page has, and one that is not in the view.
    let missing = get(&w, "/handbook/-/api/v1/provenance/Nowhere").await;
    assert_eq!(problem(&missing), "urn:inklkunn:problem:page-not-found");
    let view = get(&w, "/handbook/-/api/v1/provenance/Home?view=nope").await;
    assert_eq!(problem(&view), "urn:inklkunn:problem:no-such-view");
}

/// `main` has moved on from `alt`: version 2 is what `alt` is (the claim as the paragraph first had it, contested, with one
/// source), version 3 rewords, accepts the claim, adds a second source and a second claim. The diff prikk would give is
/// scripted.
pub(crate) fn forked() -> World {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home_with(SAID))],
    );
    let mut old = claim(1, Standing::Contested, None, SAID);
    old.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
    put(
        &w,
        &home_with(SAID),
        &[old],
        &[
            source(1, SourceKind::Primary),
            source(2, SourceKind::Secondary),
        ],
    );
    w.fake.add_view_at("alt", 2);
    let mut now = claim(1, Standing::Accepted, None, SAID_NOW);
    now.sources = vec![
        link(1, Link::Supports, Bearing::Direct, SAID_NOW),
        link(2, Link::Challenges, Bearing::Partial, SAID_NOW),
    ];
    let own = claim(2, Standing::Unresolved, None, SAID_NOW);
    put(
        &w,
        &home_with(SAID_NOW),
        &[now, own],
        &[
            source(1, SourceKind::Primary),
            source(2, SourceKind::Secondary),
        ],
    );
    w.fake.set(
        "pages/Extra.md",
        "---\ntitle: Extra\n---\n\nOnly in the default view.\n",
    );
    let entry = |path: &str, status: &str, hunk: &str| inklkunn_prikk::DiffEntry {
        path: path.to_owned(),
        status: status.to_owned(),
        from: None,
        to: None,
        hunks: vec![hunk.to_owned()],
    };
    w.fake.diff_says(Ok(inklkunn_prikk::Diff {
        schema_version: "diff-report-v1".to_owned(),
        from: inklkunn_prikk::storage::DiffSide {
            point: "a".to_owned(),
            target_block_id: "a".repeat(64),
        },
        to: inklkunn_prikk::storage::DiffSide {
            point: "b".to_owned(),
            target_block_id: "b".repeat(64),
        },
        entries: vec![
            entry(
                "pages/Home.md",
                "modified",
                "@@ -6,1 +6,1 @@\n-The bridge opened in 1950. ^b1\n+The bridge was opened in 1950. ^b1\n",
            ),
            entry("pages/Extra.md", "removed", "@@ -1,1 +0,0 @@\n-Hello.\n"),
        ],
        unsupported_paths: Vec::new(),
    }));
    w
}

#[tokio::test]
async fn two_views_are_compared_claim_by_claim_and_page_by_page_without_a_verdict() {
    let w = forked();
    let a = get(&w, "/handbook/-/api/v1/views/compare?a=main&b=alt").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    let j = a.json();
    assert_eq!(j.at("/same"), false);
    assert_eq!(j.at("/a/view"), "main");
    assert_eq!(j.at("/b/view"), "alt");
    let claims = j.at("/claims").as_array().unwrap();
    assert_eq!(claims.len(), 2);
    // The first differs in all three ways; the second only the default view has.
    let both = claims.first().unwrap();
    assert_eq!(both.at("/presence"), "both");
    assert_eq!(both.at("/wording"), true);
    assert_eq!(both.at("/status/a"), "accepted");
    assert_eq!(both.at("/status/b"), "contested");
    assert_eq!(both.at("/sources"), true);
    assert_eq!(both.at("/said_in"), "Home");
    let only = claims.get(1).unwrap();
    assert_eq!(only.at("/presence"), "only-a");
    assert_eq!(only.at("/status"), &Value::Null);
    let pages = j.at("/pages").as_array().unwrap();
    let statuses: Vec<(&str, &str)> = pages
        .iter()
        .map(|p| {
            (
                p.at("/title").as_str().unwrap_or(""),
                p.at("/status").as_str().unwrap_or(""),
            )
        })
        .collect();
    let mut statuses = statuses;
    statuses.sort_unstable();
    assert_eq!(statuses, [("Extra", "removed"), ("Home", "modified")]);
    assert_eq!(j.at("/claims_omitted"), 0);
    assert_eq!(j.at("/pages_omitted"), 0);
    // The other way round is the same facts from the other side.
    let back = get(&w, "/handbook/-/api/v1/views/compare?a=alt&b=main")
        .await
        .json();
    assert_eq!(back.at("/claims/1/presence"), "only-b");
    assert_eq!(back.at("/claims/0/status/a"), "contested");
}

#[tokio::test]
async fn a_comparison_with_the_default_view_unnamed_is_the_comparison_with_it_named_and_what_is_wrong_is_said()
 {
    let w = forked();
    let named = get(&w, "/handbook/-/api/v1/views/compare?a=main&b=alt")
        .await
        .json();
    let unnamed = get(&w, "/handbook/-/api/v1/views/compare?b=alt")
        .await
        .json();
    assert_eq!(named, unnamed);
    for (uri, kind) in [
        ("/handbook/-/api/v1/views/compare", "invalid-parameter"),
        (
            "/handbook/-/api/v1/views/compare?a=main&b=main",
            "invalid-parameter",
        ),
        (
            "/handbook/-/api/v1/views/compare?b=main",
            "invalid-parameter",
        ),
        (
            "/handbook/-/api/v1/views/compare?a=main&b=nope",
            "no-such-view",
        ),
        (
            "/handbook/-/api/v1/views/compare?a=nope&b=alt",
            "no-such-view",
        ),
        (
            "/handbook/-/api/v1/views/compare?a=main&b=alt&c=1",
            "invalid-parameter",
        ),
    ] {
        let a = get(&w, uri).await;
        assert_eq!(problem(&a), format!("urn:inklkunn:problem:{kind}"), "{uri}");
    }
}

#[tokio::test]
async fn a_wiki_you_may_not_read_has_no_claim_source_provenance_or_comparison_to_tell() {
    use inklkunn_store::Visibility;
    let secret = World::with_visibility(
        "secret",
        "Secret",
        Visibility::Restricted,
        &[("pages/Home.md", &home_with(SAID))],
    );
    let elsewhere = handbook();
    for tail in [
        format!("claims/{}", claim_id(1)),
        format!("sources/{}", source_id(1)),
        "claims/not-an-id".to_owned(),
        "provenance/Home".to_owned(),
        "views/compare?b=alt".to_owned(),
        "views/compare".to_owned(),
    ] {
        let hidden = get(&secret, &format!("/secret/-/api/v1/{tail}")).await;
        let absent = get(&elsewhere, &format!("/never-created/-/api/v1/{tail}")).await;
        assert_eq!(hidden.status, StatusCode::NOT_FOUND, "{tail}");
        assert_eq!(hidden.bytes, absent.bytes, "{tail}");
        assert_eq!(hidden.header_lines(), absent.header_lines(), "{tail}");
    }
}

#[tokio::test]
async fn the_deciding_proposal_is_named_when_the_caller_may_read_it_and_only_its_existence_when_it_is_not_among_the_records()
 {
    let w = super::common::with_accounts(handbook());
    let me = super::common::person(1);
    super::common::let_read(&w, &me);
    let held = super::common::held_proposal(&w, &me, "Settle the date");
    let decided = |proposal: &str| {
        let mut c = claim(1, Standing::Accepted, Some(proposal), SAID);
        c.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
        c
    };
    put(
        &w,
        &home_with(SAID),
        &[decided(held.id.as_str())],
        &[source(1, SourceKind::Primary)],
    );
    // Whoever may read the proposal is told which it is.
    let read = as_json(super::common::get_as(&w, &claim_uri(1), Some(&me)).await);
    assert_eq!(read.at("/status/standing"), "accepted");
    assert_eq!(read.at("/status/proposal"), held.id.as_str());
    assert_eq!(read.at("/status/proposal_unlisted"), false);
    // A proposal held for review is a public record of the wiki, so a visitor is told which as well.
    let visitor = get(&w, &claim_uri(1)).await.json();
    assert_eq!(visitor.at("/status/proposal"), held.id.as_str());
    assert_eq!(visitor.at("/status/proposal_unlisted"), false);
    // A proposal that is not among this wiki's records (an imported wiki's) is the same: one decided it, and it is not named.
    put(
        &w,
        &home_with(SAID),
        &[decided("01930000-aaaa-7000-8000-000000000009")],
        &[source(1, SourceKind::Primary)],
    );
    let unheld = as_json(super::common::get_as(&w, &claim_uri(1), Some(&me)).await);
    assert_eq!(unheld.at("/status/proposal"), &Value::Null);
    assert_eq!(unheld.at("/status/proposal_unlisted"), true);
    // A claim nobody decided names no proposal, and says none is hidden.
    put(
        &w,
        &home_with(SAID),
        &[claim(
            1,
            Standing::Unknown,
            Some("01930000-aaaa-7000-8000-000000000009"),
            SAID,
        )],
        &[],
    );
    let none = get(&w, &claim_uri(1)).await.json();
    assert_eq!(none.at("/status/proposal"), &Value::Null);
    assert_eq!(none.at("/status/proposal_unlisted"), false);
}

/// A wiki whose `Home` is created at version 1 and reworded at version 2.
fn reworded_once() -> World {
    let w = super::common::with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home_with(SAID))],
    ));
    w.fake.say(1, "Create the handbook");
    w.fake
        .show(1, shown(vec![op("create-file", &["pages/Home.md"])]));
    w.fake.set("pages/Home.md", &home_with(SAID_NOW));
    w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
    w
}

/// Make the message of version 2 say it was made from `proposal`.
fn say_made_from(w: &World, proposal: &inklkunn_store::ProposalId) {
    use inklkunn_core::message::{Actor, BlockId, Message, Summary};
    use inklkunn_store::DisplayName;
    let actor = |n: u8, name: &str| Actor::Account {
        id: super::common::person(n),
        name: DisplayName::new(name).expect("name"),
    };
    let message = Message::proposal(
        Summary::new("Reword the first paragraph").expect("summary"),
        proposal.clone(),
        actor(1, "Aiko Tanaka"),
        actor(4, "Mika Ito"),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a proposal message")
    .to_text();
    w.fake.say(2, &message);
}

/// The `proposal` member of the change numbered `number` in a provenance.
fn proposal_of(provenance: &Value, number: u64) -> Value {
    provenance
        .at("/changes")
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c.at("/number").as_u64() == Some(number))
        .unwrap_or_else(|| panic!("no change {number} in {provenance}"))
        .at("/proposal")
        .clone()
}

#[tokio::test]
async fn a_change_made_from_a_proposal_names_it_only_when_the_records_tie_it_to_the_change() {
    let w = reworded_once();
    let proposer = super::common::person(1);
    super::common::let_read(&w, &proposer);
    let held = super::common::held_proposal(&w, &proposer, "Reword the first paragraph");
    say_made_from(&w, &held.id);
    let provenance = || async { get(&w, "/handbook/-/api/v1/provenance/Home").await.json() };

    // The message names a proposal the wiki holds, and no acceptance record seals it as this change: the message alone is
    // no proof, so nothing of the proposal is given.
    assert_eq!(proposal_of(&provenance().await, 2), Value::Null);

    // The acceptance records say it was sealed as this change: the proposal is named, with its summary and its sources.
    let maintainer = super::common::person(4);
    super::common::add_person(
        &w,
        &maintainer,
        "Mika Ito",
        Some(inklkunn_store::Role::Maintainer),
    );
    super::common::accept_by_hand(&w, &held.id, &maintainer, &proposer);
    let tied = provenance().await;
    let named = proposal_of(&tied, 2);
    assert_eq!(named.at("/id"), held.id.as_str());
    assert_eq!(named.at("/summary"), "Reword the first paragraph");
    assert!(named.at("/sources").is_array(), "{named}");
    // The change that created the page was made from none.
    assert_eq!(proposal_of(&tied, 1), Value::Null);
    assert_eq!(tied.at("/changes/0/summary"), "Reword the first paragraph");

    // A message that names a proposal the wiki's records do not hold names nothing of it.
    let other = reworded_once();
    say_made_from(&other, &inklkunn_store::ProposalId::new());
    let unheld = get(&other, "/handbook/-/api/v1/provenance/Home")
        .await
        .json();
    assert_eq!(proposal_of(&unheld, 2), Value::Null);
}
