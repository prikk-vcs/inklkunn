//! Making claims and attaching sources in the proposal form (RFC 005 part 2 D-3; part 1 D-1 b): the part menu, the
//! form that attaches or challenges, the paragraph's marker the form holds, the removal of a paragraph that others
//! rely on as an act the person confirms, and what the proposal's page says of the claims and sources it records.
//! The repository is an in-memory double, so these run in both test runs.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{
    Bearing, Claim, ClaimId, Expression, File, Lifecycle, Link, Source, SourceId, SourceLink,
    StatementKind, block_digest, claim_path, read_file, source_path,
};
use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_store::{AccountId, ProposalId, Role};

use crate::common::{
    Answer, World, add_person, block_id, get_as, person, post_form, state_id, with_accounts,
};

pub(crate) const BRIDGE: &str = "---\ntitle: Bridge\nid: bridge-page\n---\n\nIt opened in 1950. ^k1\n\nIt is long.\n\n# Notes\n\nA note.\n";
const PLAIN: &str = "---\ntitle: Plain\n---\n\nNo identifier here.\n";

pub(crate) struct Scene {
    pub(crate) w: World,
    pub(crate) writer: AccountId,
    pub(crate) observer: AccountId,
    pub(crate) claim: Claim,
}

pub(crate) fn claim_id() -> ClaimId {
    ClaimId::parse("01930000-0000-7000-8000-000000000001").expect("a claim id")
}

pub(crate) fn source_id() -> SourceId {
    SourceId::parse("01931111-0000-7000-8000-000000000001").expect("a source id")
}

/// A wiki with a page that holds a claim, its source, a page with no identifier, and two people.
pub(crate) fn scene() -> Scene {
    let mut claim = Claim::new(claim_id());
    claim.kind = StatementKind::Observation;
    claim.lifecycle = Lifecycle::Proposed;
    claim.expressions = vec![Expression::new("bridge-page", "k1").expect("an expression")];
    let blocks = inklkunn_markup::blocks("It opened in 1950. ^k1");
    claim.sources = vec![SourceLink {
        source: source_id(),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: block_digest(blocks.first().expect("a block")),
    }];
    let mut source = Source::new(source_id());
    source.identity.title = Some("A history of the bridge".to_owned());
    let (claim_file, source_file) = (claim_path(&claim.id), source_path(&source.id));
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Bridge.md", BRIDGE),
            ("pages/Plain.md", PLAIN),
            (claim_file.as_str(), &claim.to_yaml()),
            (source_file.as_str(), &source.to_yaml()),
        ],
    ));
    let (writer, observer) = (person(1), person(6));
    add_person(&w, &writer, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&w, &observer, "Olu Reader", Some(Role::Observer));
    add_person(&w, &person(4), "Mika Ito", Some(Role::Maintainer));
    Scene {
        w,
        writer,
        observer,
        claim,
    }
}

pub(crate) fn principal(id: &AccountId) -> Principal {
    Principal::session(&IdentityAuthority::for_tests(), id.clone())
}

pub(crate) async fn open(s: &Scene, uri: &str, account: &AccountId) -> Answer {
    get_as(&s.w, uri, Some(account)).await
}

pub(crate) async fn send(
    s: &Scene,
    uri: &str,
    account: &AccountId,
    fields: &[(&str, String)],
) -> Answer {
    let borrowed: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    post_form(&s.w, uri, Some(account), &borrowed).await
}

/// The fields of the form for the `ordinal`-th part of a page, with `text`.
pub(crate) fn part(
    path: &str,
    title: &str,
    ordinal: usize,
    text: &str,
    op: &str,
) -> Vec<(&'static str, String)> {
    vec![
        ("op", op.to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "block".to_owned()),
        ("ordinal", ordinal.to_string()),
        ("base_state", state_id(1)),
        ("base_block", block_id(1)),
        ("p0_path", path.to_owned()),
        ("p0_title", title.to_owned()),
        ("p0_text", text.to_owned()),
        ("summary", "A source for the bridge".to_owned()),
        ("sources", String::new()),
    ]
}

/// The fields a source form sends.
fn source_fields(relation: &str, title: &str) -> Vec<(&'static str, String)> {
    vec![
        ("attach", relation.to_owned()),
        ("a_kind", "anecdotal".to_owned()),
        ("a_title", title.to_owned()),
        ("a_author", String::new()),
        ("a_date", "1962".to_owned()),
        ("a_ref_form", "url".to_owned()),
        ("a_ref", "https://example.org/history".to_owned()),
        ("a_bearing", "partial".to_owned()),
        ("a_note", "page 4".to_owned()),
        ("a_claim_kind", "opinion".to_owned()),
    ]
}

/// `fields` with these replaced or added.
fn with(
    mut fields: Vec<(&'static str, String)>,
    more: Vec<(&'static str, String)>,
) -> Vec<(&'static str, String)> {
    for (k, v) in more {
        fields.retain(|(name, _)| *name != k);
        fields.push((k, v));
    }
    fields
}

pub(crate) const NEW: &str = "/handbook/-/proposals/new";

/// The address a redirect leads to.
pub(crate) fn location(a: &Answer) -> String {
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    a.header("location").expect("a location").to_owned()
}

pub(crate) type Files = Vec<(String, Option<String>)>;

/// The files of the latest revision (or the draft) of the proposal an address names.
pub(crate) fn files_of(s: &Scene, address: &str) -> Files {
    let raw = address
        .rsplit('/')
        .next()
        .and_then(|last| last.split('?').next())
        .expect("an id");
    let id = ProposalId::parse(raw).unwrap_or_else(|| {
        // `…/proposals/<id>/edit?saved=1`
        let parts: Vec<&str> = address.split('/').collect();
        let at = parts
            .iter()
            .position(|p| *p == "proposals")
            .expect("proposals");
        ProposalId::parse(parts.get(at + 1).expect("an id")).expect("a proposal id")
    });
    let who = principal(&s.writer);
    let service = &s.w.services.proposals;
    let revisions = service.revisions(&who, &s.w.wiki, &id).expect("revisions");
    let content = match revisions.last() {
        Some(latest) => latest.content.clone(),
        None => service.draft(&who, &s.w.wiki, &id).expect("a draft"),
    };
    content
        .files
        .into_iter()
        .map(|f| (f.path, f.text))
        .collect()
}

pub(crate) fn file<'a>(files: &'a Files, path: &str) -> &'a str {
    files
        .iter()
        .find(|(p, _)| p == path)
        .and_then(|(_, t)| t.as_deref())
        .unwrap_or_else(|| panic!("no file {path} in {files:?}"))
}

pub(crate) fn claims_in(files: &Files) -> Vec<Claim> {
    files
        .iter()
        .filter_map(|(p, t)| match read_file(p, t.as_deref()?.as_bytes()) {
            Ok(File::Claim(c)) => Some(*c),
            _ => None,
        })
        .collect()
}

pub(crate) fn sources_in(files: &Files) -> Vec<Source> {
    files
        .iter()
        .filter_map(|(p, t)| match read_file(p, t.as_deref()?.as_bytes()) {
            Ok(File::Source(s)) => Some(*s),
            _ => None,
        })
        .collect()
}

pub(crate) fn paths(files: &Files) -> Vec<&str> {
    files.iter().map(|(p, _)| p.as_str()).collect()
}

// ---------------------------------------------------------------------------------------------- the part menu

#[tokio::test]
async fn the_part_menu_offers_a_source_for_each_paragraph_and_for_nothing_else() {
    let s = scene();
    let a = open(
        &s,
        "/handbook/-/proposals/new?page=Bridge&parts=1",
        &s.writer,
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    // Three paragraphs; the heading has none.
    assert_eq!(body.matches(">Attach a source</a>").count(), 3, "{body}");
    assert_eq!(body.matches(">Challenge this</a>").count(), 3, "{body}");
    assert!(
        body.contains("aria-label=\"Attach a source to part 2\""),
        "{body}"
    );
    assert!(
        body.contains("aria-label=\"Challenge this: part 2\""),
        "{body}"
    );
    assert!(
        body.contains("attach=supports") && body.contains("attach=challenges"),
        "{body}"
    );
    let heading = body
        .lines()
        .find(|l| l.contains("Part 3: # Notes"))
        .expect("the heading's line");
    assert!(!heading.contains("Attach a source"), "{heading}");
}

#[tokio::test]
async fn the_form_that_attaches_a_source_asks_for_the_source_and_nothing_about_identity() {
    let s = scene();
    let uri = "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=1&attach=supports";
    let a = open(&s, uri, &s.writer).await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(
        body.contains("<h1>Attach a source to Bridge</h1>"),
        "{body}"
    );
    assert!(body.contains("A source is what a statement rests on"));
    for name in [
        "attach",
        "a_kind",
        "a_title",
        "a_author",
        "a_date",
        "a_ref_form",
        "a_ref",
        "a_bearing",
        "a_note",
        "a_claim_kind",
    ] {
        assert!(body.contains(&format!("name=\"{name}\"")), "{name}: {body}");
    }
    assert!(
        body.contains("<input type=\"hidden\" name=\"attach\" value=\"supports\">"),
        "{body}"
    );
    // The paragraph is in the text box, as it is; the form needs no script.
    assert!(body.contains("It is long."), "{body}");
    assert!(!body.contains("<script"));
    // The choices are in words, and none of them ranks.
    for word in [
        "A first-hand source",
        "It speaks to the whole of it",
        "It speaks to it only by implication",
        "An observation",
    ] {
        assert!(body.contains(word), "{word}: {body}");
    }
    for ranked in ["reliable", "credible", "authoritative", "weak", "strong"] {
        assert!(!body.to_lowercase().contains(ranked), "{ranked}");
    }
    // The other way in.
    let challenge = "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=1&attach=challenges";
    let body = open(&s, challenge, &s.writer).await.body();
    assert!(
        body.contains("<h1>Challenge a part of Bridge</h1>"),
        "{body}"
    );
    assert!(body.contains("A challenging source speaks against"));
}

#[tokio::test]
async fn a_source_form_is_refused_for_what_is_not_a_paragraph_or_not_one_part() {
    let s = scene();
    for uri in [
        // The heading (a part that is no paragraph).
        "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=2&attach=supports",
        // The whole page, a section.
        "/handbook/-/proposals/new?page=Bridge&attach=supports",
        "/handbook/-/proposals/new?page=Bridge&part=section&ordinal=0&attach=supports",
        // A word the form does not have.
        "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=1&attach=proves",
    ] {
        let a = open(&s, uri, &s.writer).await;
        let to = location(&a);
        assert!(to.contains("parts=1"), "{uri}: {to}");
    }
}

#[tokio::test]
async fn someone_who_may_not_propose_is_not_given_the_form() {
    let s = scene();
    let uri = "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=1&attach=supports";
    let a = open(&s, uri, &s.observer).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(!a.body().contains("name=\"a_title\""));
    let a = get_as(&s.w, uri, None).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
    assert!(!a.body().contains("name=\"a_title\""));
}

// ---------------------------------------------------------------------------------------------- the marker

#[tokio::test]
async fn the_form_for_a_claim_paragraph_holds_its_marker_and_says_that_others_rely_on_it() {
    let s = scene();
    let uri = "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=0";
    let body = open(&s, uri, &s.writer).await.body();
    assert!(
        body.contains("Others rely on this paragraph: sources are attached to it."),
        "{body}"
    );
    assert!(body.contains("It opened in 1950."), "{body}");
    assert!(
        !body.contains("^k1"),
        "the marker is never in the text box: {body}"
    );
    // A paragraph that is not a claim shows what it is and says nothing of the kind.
    let plain = "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=1";
    let body = open(&s, plain, &s.writer).await.body();
    assert!(!body.contains("Others rely on"), "{body}");
}

#[tokio::test]
async fn rewording_a_claim_paragraph_keeps_its_marker_and_costs_nothing_else() {
    let s = scene();
    let fields = part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1951.",
        "submit",
    );
    let a = send(&s, NEW, &s.writer, &fields).await;
    let files = files_of(&s, &location(&a));
    assert_eq!(paths(&files), ["pages/Bridge.md"]);
    assert_eq!(
        file(&files, "pages/Bridge.md"),
        BRIDGE.replace("It opened in 1950. ^k1", "It opened in 1951. ^k1")
    );
}

#[tokio::test]
async fn a_text_nobody_touched_is_the_page_byte_for_byte_and_no_change() {
    let s = scene();
    // The part as the form showed it, sent back: the page is unchanged, so there is nothing to propose.
    let fields = part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1950.",
        "submit",
    );
    let a = send(&s, NEW, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body().contains("The text is the same as the page"),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn emptying_a_claim_paragraph_is_an_explicit_act_and_the_claim_loses_the_expression() {
    let s = scene();
    let fields = part("pages/Bridge.md", "Bridge", 0, "", "submit");
    let a = send(&s, NEW, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = a.body();
    assert!(
        body.contains("The paragraph that begins “It opened in 1950.” has sources attached"),
        "{body}"
    );
    assert!(body.contains("name=\"drop_k1\""), "{body}");
    assert!(
        !body.contains("name=\"drop_k1\" value=\"1\" checked"),
        "{body}"
    );

    // Confirmed: the proposal removes the paragraph and says the claim is said there no more.
    let confirmed = with(fields, vec![("drop_k1", "1".to_owned())]);
    let a = send(&s, NEW, &s.writer, &confirmed).await;
    let to = location(&a);
    let files = files_of(&s, &to);
    assert_eq!(
        paths(&files),
        ["pages/Bridge.md", claim_path(&s.claim.id).as_str(),]
    );
    assert!(!file(&files, "pages/Bridge.md").contains("It opened in 1950"));
    let claims = claims_in(&files);
    assert!(claims.iter().all(|c| c.expressions.is_empty()));
    assert_eq!(claims.len(), 1);
    // The reviewer is shown it.
    let page = as_text(open(&s, &to, &s.writer).await.body());
    assert!(
        page.contains("It is no longer said in the paragraph “<bdi>It opened in 1950.</bdi>”"),
        "{page}"
    );
}

#[tokio::test]
async fn a_form_for_several_paragraphs_keeps_the_markers_it_held_and_refuses_to_reword_a_claim_paragraph()
 {
    let s = scene();
    let uri = "/handbook/-/proposals/new?page=Bridge";
    let body = open(&s, uri, &s.writer).await.body();
    assert!(
        body.contains("Some paragraphs here have sources attached"),
        "{body}"
    );
    assert!(!body.contains("^k1"), "{body}");
    let fields = |text: &str| {
        vec![
            ("op", "submit".to_owned()),
            ("pages", "1".to_owned()),
            ("renames", "0".to_owned()),
            ("part", "whole".to_owned()),
            ("base_state", state_id(1)),
            ("base_block", block_id(1)),
            ("p0_path", "pages/Bridge.md".to_owned()),
            ("p0_title", "Bridge".to_owned()),
            ("p0_text", text.to_owned()),
            ("summary", "Edit".to_owned()),
            ("sources", String::new()),
        ]
    };
    let shown = BRIDGE.replace(" ^k1", "");
    // Another paragraph changes: the claim paragraph keeps its marker as it was.
    let edited = shown.replace("It is long.", "It is very long.");
    let a = send(&s, NEW, &s.writer, &fields(&edited)).await;
    let files = files_of(&s, &location(&a));
    assert_eq!(
        file(&files, "pages/Bridge.md"),
        BRIDGE.replace("It is long.", "It is very long.")
    );
    // The claim paragraph's words change here: the marker is not guessed back.
    let reworded = shown.replace("in 1950.", "in 1951.");
    let a = send(&s, NEW, &s.writer, &fields(&reworded)).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = a.body();
    assert!(body.contains("change that part on its own"), "{body}");
    assert!(body.contains("name=\"drop_k1\""), "{body}");
}

// ---------------------------------------------------------------------------------------------- attaching

#[tokio::test]
async fn attaching_a_source_to_a_paragraph_makes_the_claim_and_the_source_in_the_same_proposal() {
    let s = scene();
    let fields = with(
        part("pages/Bridge.md", "Bridge", 1, "It is long.", "submit"),
        source_fields("supports", "A survey of bridges"),
    );
    let a = send(&s, NEW, &s.writer, &fields).await;
    let to = location(&a);
    let files = files_of(&s, &to);
    // The page (marked), the claim and the source: three files in one proposal.
    assert_eq!(files.len(), 3, "{files:?}");
    let page = file(&files, "pages/Bridge.md");
    assert!(page.starts_with("---\ntitle: Bridge\nid: bridge-page\n---\n"));
    let claims = claims_in(&files);
    let [claim] = claims.as_slice() else {
        panic!("{claims:?}")
    };
    let [expression] = claim.expressions.as_slice() else {
        panic!("{claim:?}")
    };
    assert_eq!(expression.page_id, "bridge-page");
    assert!(page.contains(&format!("It is long. ^{}\n", expression.block)));
    assert_eq!(claim.kind, StatementKind::Opinion);
    assert_eq!(claim.lifecycle, Lifecycle::Proposed);
    let [link] = claim.sources.as_slice() else {
        panic!("{claim:?}")
    };
    assert_eq!(
        (link.relation, link.bearing),
        (Link::Supports, Bearing::Partial)
    );
    assert_eq!(link.note.as_deref(), Some("page 4"));
    let sources = sources_in(&files);
    let [source] = sources.as_slice() else {
        panic!("{sources:?}")
    };
    assert_eq!(source.id, link.source);
    assert_eq!(
        source.identity.title.as_deref(),
        Some("A survey of bridges")
    );
    // The proposal page tells the reviewer, in words and without a verdict.
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(
        body.contains("Claims and sources in this proposal"),
        "{body}"
    );
    // One page changes; the claim and the source are not pages and are not drawn as text.
    assert!(body.contains("It changes 1 page."), "{body}");
    assert_eq!(body.matches("Page: ").count(), 1, "{body}");
    assert!(!body.contains("lifecycle: proposed"), "{body}");
    // Each is told by its identifier too, shortened (K24); the service made them.
    let (claim_tag, source_tag) = (tag("claim", &claim.id), tag("source", &source.id));
    assert!(
        body.contains(&format!(
            "Records the claim “<bdi>It is long.</bdi>” {claim_tag} (an opinion), said in 1 place. Its community status: not decided."
        )),
        "{body}"
    );
    assert!(
        body.contains(&format!(
            "Cites the source <cite><bdi>“A survey of bridges”, dated 1962</bdi></cite> {source_tag}: it supports the claim in part, judged on the wording it has now."
        )),
        "{body}"
    );
    assert!(
        body.contains(&format!(
            "Records the source <cite><bdi>“A survey of bridges”, dated 1962</bdi></cite> {source_tag} (a person&#39;s account)."
        )),
        "{body}"
    );
}

/// How the page tells a subject by its identifier (K24, K32): the whole of it, in `<code>`.
fn tag(kind: &str, id: &impl std::fmt::Display) -> String {
    format!("[{kind} <code>{id}</code>]")
}

#[tokio::test]
async fn challenging_a_paragraph_attaches_a_source_that_challenges() {
    let s = scene();
    let fields = with(
        part("pages/Bridge.md", "Bridge", 1, "It is long.", "submit"),
        source_fields("challenges", "A correction"),
    );
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    let files = files_of(&s, &to);
    let claims = claims_in(&files);
    let link = claims
        .first()
        .and_then(|c| c.sources.first())
        .expect("a link");
    assert_eq!(link.relation, Link::Challenges);
    let body = as_text(open(&s, &to, &s.writer).await.body());
    let source_tag = tag("source", &link.source);
    assert!(
        body.contains(&format!(
            "Cites the source <cite><bdi>“A correction”, dated 1962</bdi></cite> {source_tag}: it challenges the claim in part, judged on the wording it has now."
        )),
        "{body}"
    );
}

#[tokio::test]
async fn a_page_that_has_no_identifier_is_given_one_with_the_claim() {
    let s = scene();
    let fields = with(
        part(
            "pages/Plain.md",
            "Plain",
            0,
            "No identifier here.",
            "submit",
        ),
        source_fields("supports", "A note"),
    );
    let files = files_of(&s, &location(&send(&s, NEW, &s.writer, &fields).await));
    let page = file(&files, "pages/Plain.md");
    let id = inklkunn_core::knowledge::compose::page_id_of(page).expect("an identifier");
    let claims = claims_in(&files);
    assert_eq!(
        claims
            .first()
            .and_then(|c| c.expressions.first())
            .map(|e| e.page_id.as_str()),
        Some(id.as_str())
    );
}

#[tokio::test]
async fn a_source_for_a_paragraph_that_is_a_claim_goes_to_that_claim() {
    let s = scene();
    // The part as the form showed it (marker held), with a source; the page does not change.
    let fields = with(
        part(
            "pages/Bridge.md",
            "Bridge",
            0,
            "It opened in 1950.",
            "submit",
        ),
        source_fields("challenges", "Another history"),
    );
    // The claim kind is not asked for a claim that exists, and is not sent.
    let fields: Vec<_> = fields
        .into_iter()
        .filter(|(k, _)| *k != "a_claim_kind")
        .collect();
    let files = files_of(&s, &location(&send(&s, NEW, &s.writer, &fields).await));
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(!paths(&files).contains(&"pages/Bridge.md"));
    let claims = claims_in(&files);
    let [claim] = claims.as_slice() else {
        panic!("{claims:?}")
    };
    assert_eq!(claim.id, s.claim.id, "no new claim");
    assert_eq!(claim.sources.len(), 2);
    assert_eq!(claim.kind, StatementKind::Observation, "the kind stands");
    // The form for it asks no kind and says the claim is there.
    let uri = "/handbook/-/proposals/new?page=Bridge&part=block&ordinal=0&attach=supports";
    let body = open(&s, uri, &s.writer).await.body();
    assert!(!body.contains("name=\"a_claim_kind\""), "{body}");
    assert!(
        body.contains("This paragraph is a claim already."),
        "{body}"
    );
}

#[tokio::test]
async fn what_is_wrong_with_a_source_is_said_on_the_form_with_the_text_still_in_it() {
    let s = scene();
    let wrong = |title: &str, extra: Vec<(&'static str, String)>| {
        with(
            with(
                part(
                    "pages/Bridge.md",
                    "Bridge",
                    1,
                    "It is long, really.",
                    "submit",
                ),
                source_fields("supports", title),
            ),
            extra,
        )
    };
    let nothing = wrong("", vec![("a_ref", String::new())]);
    let a = send(&s, NEW, &s.writer, &nothing).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = a.body();
    assert!(
        body.contains("A source needs a title or a reference."),
        "{body}"
    );
    assert!(
        body.contains("It is long, really."),
        "the person's text stays: {body}"
    );
    let web = wrong("T", vec![("a_ref", "javascript:alert(1)".to_owned())]);
    let body = send(&s, NEW, &s.writer, &web).await.body();
    assert!(
        body.contains("A web address has to start with https or http"),
        "{body}"
    );
    // The value the person wrote is printed escaped, never as markup.
    assert!(!body.contains("value=\"javascript:alert(1)\"<"), "{body}");
    let long = wrong("T", vec![("a_note", "x".repeat(2_001))]);
    let body = send(&s, NEW, &s.writer, &long).await.body();
    assert!(body.contains("longer than 2000 characters"), "{body}");
    // Nothing was made by any of them.
    let all =
        s.w.services
            .proposals
            .list(&principal(&s.writer), &s.w.wiki, &Default::default())
            .expect("a list");
    assert!(all.is_empty());
}

#[tokio::test]
async fn a_source_form_posted_for_more_than_one_paragraph_is_not_one_a_page_offered() {
    let s = scene();
    let mut fields = with(
        part("pages/Bridge.md", "Bridge", 1, "It is long.", "submit"),
        source_fields("supports", "T"),
    );
    fields = with(fields, vec![("part", "whole".to_owned())]);
    let a = send(&s, NEW, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let all =
        s.w.services
            .proposals
            .list(&principal(&s.writer), &s.w.wiki, &Default::default())
            .expect("a list");
    assert!(all.is_empty());
}

// ---------------------------------------------------------------------------------------------- drafts and revisions

#[tokio::test]
async fn a_draft_keeps_the_claim_and_a_revision_carries_it_forward() {
    let s = scene();
    let fields = with(
        part("pages/Bridge.md", "Bridge", 1, "It is long.", "draft"),
        source_fields("supports", "A survey"),
    );
    let to = location(&send(&s, NEW, &s.writer, &fields).await);
    assert!(to.contains("/edit"), "{to}");
    let files = files_of(&s, &to);
    assert_eq!(files.len(), 3);
    // The form that resumes it lists the claim and the source, and has the page to edit.
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(
        body.contains("Claims and sources in this proposal"),
        "{body}"
    );
    assert!(body.contains("Records the claim"), "{body}");
    assert!(body.contains("name=\"p0_text\""), "{body}");
    assert!(
        !body.contains("p1_text"),
        "the knowledge files are not text boxes: {body}"
    );
    assert!(
        body.contains("It is long.\n") && !body.contains(" ^k"),
        "the marker the proposal made is held too: {body}"
    );
    // Sent for review as it stands: the three files go.
    let id = to.split('/').nth(4).expect("an id").to_owned();
    let page = files
        .iter()
        .find(|(p, _)| p == "pages/Bridge.md")
        .and_then(|(_, t)| t.clone())
        .expect("the page");
    let blocks: std::collections::BTreeSet<String> = claims_in(&files)
        .iter()
        .flat_map(|c| c.expressions.iter().map(|e| e.block.clone()))
        .collect();
    let hidden_page = inklkunn_core::knowledge::marker::hide_markers(&page, &blocks, true);
    assert!(hidden_page.contains("It is long.\n"), "{hidden_page}");
    let resumed = vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", state_id(1)),
        ("base_block", block_id(1)),
        ("p0_path", "pages/Bridge.md".to_owned()),
        ("p0_title", "Bridge".to_owned()),
        ("p0_text", hidden_page),
        ("summary", "A source for the bridge".to_owned()),
        ("sources", String::new()),
    ];
    let a = send(
        &s,
        &format!("/handbook/-/proposals/{id}/edit"),
        &s.writer,
        &resumed,
    )
    .await;
    let to = location(&a);
    let sent = files_of(&s, &to);
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert_eq!(sources_in(&sent).len(), 1);
    assert_eq!(claims_in(&sent).len(), 1);
    assert_eq!(
        file(&sent, "pages/Bridge.md"),
        page,
        "the marker went back where it was"
    );
}

// ---------------------------------------------------------------------------------------------- what only curation sets

/// A proposal of the writer's with these files (pages and knowledge files), held for review.
pub(crate) fn held_with(s: &Scene, files: Vec<(String, Option<String>)>) -> String {
    let edit = inklkunn_core::proposal::ProposalEdit {
        summary: "A change".to_owned(),
        files: files
            .into_iter()
            .map(|(path, text)| inklkunn_store::FileChange { path, text })
            .collect(),
        ..Default::default()
    };
    let who = principal(&s.writer);
    let draft =
        s.w.services
            .proposals
            .create(&who, &s.w.wiki, None, edit)
            .expect("a draft");
    s.w.services
        .proposals
        .submit(&who, &s.w.wiki, &draft.id)
        .expect("held for review");
    format!("/handbook/-/proposals/{}", draft.id)
}

/// A proposal of the writer's held for review **without the service's judgement of its knowledge files**: what the
/// service refuses when it is sent (K7) can still be in the records (an older proposal), and its page says so.
pub(crate) fn held_unjudged(s: &Scene, files: Vec<(String, Option<String>)>) -> String {
    let record = s.w.hold_unjudged(
        &s.writer,
        "A change",
        files
            .into_iter()
            .map(|(path, text)| inklkunn_store::FileChange { path, text })
            .collect(),
    );
    format!("/handbook/-/proposals/{}", record.id)
}

#[tokio::test]
async fn a_status_beside_a_page_edit_is_said_to_be_refused_and_the_change_to_it_is_listed() {
    let s = scene();
    let mut decided = s.claim.clone();
    decided.community_status.status = inklkunn_core::knowledge::Standing::Accepted;
    let files = vec![
        (claim_path(&decided.id), Some(decided.to_yaml())),
        (
            "pages/Bridge.md".to_owned(),
            Some(BRIDGE.replace("A note.", "A longer note.")),
        ),
    ];
    // The service does not let it be sent (K7) ...
    let who = principal(&s.writer);
    let draft =
        s.w.services
            .proposals
            .create(
                &who,
                &s.w.wiki,
                None,
                inklkunn_core::proposal::ProposalEdit {
                    summary: "A change".to_owned(),
                    files: files
                        .iter()
                        .cloned()
                        .map(|(path, text)| inklkunn_store::FileChange { path, text })
                        .collect(),
                    ..Default::default()
                },
            )
            .expect("a draft");
    assert!(matches!(
        s.w.services.proposals.submit(&who, &s.w.wiki, &draft.id),
        Err(inklkunn_core::proposal::ProposalError::Invalid(
            inklkunn_core::proposal::Invalid::KnowledgeCurated
        ))
    ));
    // ... and a record that is there anyway says so on its page.
    let to = held_unjudged(&s, files);
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(
        body.contains("Its community status changes from not decided to accepted."),
        "{body}"
    );
    assert!(body.contains("only a curation proposal may do"), "{body}");
    assert!(body.contains("It cannot be accepted as it is."), "{body}");
    // A claim file that changes nothing about it says nothing of the kind.
    let to = held_with(
        &s,
        vec![
            (claim_path(&s.claim.id), Some(s.claim.to_yaml())),
            (
                "pages/Bridge.md".to_owned(),
                Some(BRIDGE.replace("A note.", "A longer note.")),
            ),
        ],
    );
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(!body.contains("only a curation proposal may do"), "{body}");
    // The section is there to say so (K5), with no line under it: an empty list is said, not left out.
    assert!(body.contains("they change nothing"), "{body}");
    assert!(!body.contains("Changes the claim"), "{body}");
}

#[tokio::test]
async fn a_new_link_is_said_to_be_judged_on_the_wording_it_has_now_only_when_it_is() {
    let s = scene();
    let second = SourceId::parse("01931111-0000-7000-8000-000000000002").expect("a source id");
    let mut source = Source::new(second.clone());
    source.identity.title = Some("A second history".to_owned());
    let link = |on: &str| SourceLink {
        source: second.clone(),
        relation: Link::Challenges,
        bearing: Bearing::Partial,
        note: None,
        judged_on: block_digest(inklkunn_markup::blocks(on).first().expect("a block")),
    };
    let with_link = |on: &str| {
        let mut more = s.claim.clone();
        more.sources.push(link(on));
        more.normalize();
        vec![
            (claim_path(&more.id), Some(more.to_yaml())),
            (source_path(&second), Some(source.to_yaml())),
        ]
    };
    let now = &format!(
        "Cites the source <cite><bdi>“A second history”</bdi></cite> {}: it challenges the claim in part, judged on the wording it has now.",
        tag("source", &second)
    );
    let unjudged = "judged on a wording the claim does not have in this proposal&#39;s result.";
    let note = "judged on a wording that no paragraph of that claim has once the proposal is made";
    // The wording the paragraph has: said so, with no warning.
    let to = held_unjudged(&s, with_link("It opened in 1950. ^k1"));
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(body.contains(now), "{body}");
    assert!(!body.contains(unjudged) && !body.contains(note), "{body}");
    // A wording nobody has written: the line does not say it, and the page says it cannot be accepted as it is.
    let to = held_unjudged(&s, with_link("It opened in 1951. ^k1"));
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(!body.contains(now), "{body}");
    assert!(body.contains(unjudged), "{body}");
    assert!(body.contains(note), "{body}");
    assert!(body.contains("It cannot be accepted as it is."), "{body}");
    // The same proposal writing the words the link was judged on is sound again: the paragraph then has them.
    let mut files = with_link("It opened in 1951. ^k1");
    files.push((
        "pages/Bridge.md".to_owned(),
        Some(BRIDGE.replace("It opened in 1950.", "It opened in 1951.")),
    ));
    let to = held_unjudged(&s, files);
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(body.contains(now), "{body}");
    assert!(!body.contains(note), "{body}");
}

#[tokio::test]
async fn a_status_judged_on_a_wording_nobody_has_written_is_said_to_be_so_and_the_service_refuses_it()
 {
    // K33 (RFC 005 part 2 D-3, *every judgement*): the community status's `judged_on` is held to the rule of a new
    // link's, in a curation proposal too.
    let s = scene();
    let note = "judged on a wording that no paragraph of that claim has once the proposal is made";
    let status = |on: &str| {
        let mut decided = s.claim.clone();
        decided.community_status = inklkunn_core::knowledge::CommunityStatus {
            status: inklkunn_core::knowledge::Standing::Contested,
            proposal: None,
            judged_on: Some(block_digest(
                inklkunn_markup::blocks(on).first().expect("a block"),
            )),
        };
        vec![(claim_path(&decided.id), Some(decided.to_yaml()))]
    };
    // The service does not let it be sent, and says it is the status.
    let who = principal(&s.writer);
    let made = s.w.services.proposals.create_and_submit(
        &who,
        &s.w.wiki,
        None,
        inklkunn_core::proposal::ProposalEdit {
            summary: "A status".to_owned(),
            files: status("It opened in 1951. ^k1")
                .into_iter()
                .map(|(path, text)| inklkunn_store::FileChange { path, text })
                .collect(),
            ..Default::default()
        },
    );
    assert!(
        matches!(
            made,
            Err(inklkunn_core::proposal::ProposalError::Invalid(
                inklkunn_core::proposal::Invalid::KnowledgeStatusUnjudged
            ))
        ),
        "{made:?}"
    );
    // A record that is there anyway says so on its page; on the wording the paragraph has, it says nothing.
    let to = held_unjudged(&s, status("It opened in 1951. ^k1"));
    let body = open(&s, &to, &s.writer).await.body();
    assert!(body.contains(note), "{body}");
    assert!(body.contains("It cannot be accepted as it is."), "{body}");
    let to = held_unjudged(&s, status("It opened in 1950. ^k1"));
    let body = open(&s, &to, &s.writer).await.body();
    assert!(!body.contains(note), "{body}");
}

#[tokio::test]
async fn the_page_warns_of_a_decided_claim_removed_and_of_a_source_link_changed() {
    let s = scene();
    let note = "only a curation proposal may do";
    // A link the claim has, borne differently: curation (K6), and the page says it cannot be accepted as it is.
    let mut rebeared = s.claim.clone();
    rebeared.sources.first_mut().expect("a link").bearing = Bearing::Indirect;
    let to = held_unjudged(
        &s,
        vec![(claim_path(&rebeared.id), Some(rebeared.to_yaml()))],
    );
    let body = as_text(open(&s, &to, &s.writer).await.body());
    // A proposal of knowledge files alone is a curation proposal: no warning there.
    assert!(!body.contains(note), "{body}");
    let to = held_unjudged(
        &s,
        vec![
            (claim_path(&rebeared.id), Some(rebeared.to_yaml())),
            (
                "pages/Bridge.md".to_owned(),
                Some(BRIDGE.replace("A note.", "A longer note.")),
            ),
        ],
    );
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(body.contains(note), "{body}");
    // The link taken away.
    let mut unlinked = s.claim.clone();
    unlinked.sources.clear();
    let to = held_unjudged(
        &s,
        vec![
            (claim_path(&unlinked.id), Some(unlinked.to_yaml())),
            (
                "pages/Bridge.md".to_owned(),
                Some(BRIDGE.replace("A note.", "A longer note.")),
            ),
        ],
    );
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(body.contains(note), "{body}");
    // A claim whose standing was decided, removed beside a page edit; one nobody decided is not warned about.
    let mut decided = s.claim.clone();
    decided.community_status.status = inklkunn_core::knowledge::Standing::Contested;
    s.w.fake.set(&claim_path(&decided.id), &decided.to_yaml());
    let to = held_unjudged(
        &s,
        vec![
            (claim_path(&decided.id), None),
            (
                "pages/Bridge.md".to_owned(),
                Some(BRIDGE.replace("A note.", "A longer note.")),
            ),
        ],
    );
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(body.contains(note), "{body}");
}

#[tokio::test]
async fn removing_a_claim_with_a_link_or_a_file_that_cannot_be_read_beside_a_page_edit_is_curation()
{
    let s = scene();
    let note = "only a curation proposal may do";
    let page_edit = || {
        (
            "pages/Bridge.md".to_owned(),
            Some(BRIDGE.replace("A note.", "A longer note.")),
        )
    };
    // The claim has a source link and nobody decided its standing: removing it takes the link with it (K16). The
    // sentence says removals are what it is about, and the service does not let the proposal be sent.
    let removal = vec![(claim_path(&s.claim.id), None), page_edit()];
    let who = principal(&s.writer);
    let draft =
        s.w.services
            .proposals
            .create(
                &who,
                &s.w.wiki,
                None,
                inklkunn_core::proposal::ProposalEdit {
                    summary: "A change".to_owned(),
                    files: removal
                        .iter()
                        .cloned()
                        .map(|(path, text)| inklkunn_store::FileChange { path, text })
                        .collect(),
                    ..Default::default()
                },
            )
            .expect("a draft");
    assert!(matches!(
        s.w.services.proposals.submit(&who, &s.w.wiki, &draft.id),
        Err(inklkunn_core::proposal::ProposalError::Invalid(
            inklkunn_core::proposal::Invalid::KnowledgeCurated
        ))
    ));
    let body = open(&s, &held_unjudged(&s, removal), &s.writer)
        .await
        .body();
    assert!(body.contains(note), "{body}");
    assert!(
        body.contains("removes a claim or a source that holds any of these"),
        "{body}"
    );

    // A claim that holds nothing curated goes with a page edit as before, and its page does not warn.
    let mut bare = s.claim.clone();
    bare.id = inklkunn_core::knowledge::ClaimId::parse("01930000-0000-7000-8000-000000000002")
        .expect("a claim id");
    bare.sources.clear();
    s.w.fake.set(&claim_path(&bare.id), &bare.to_yaml());
    let to = held_with(&s, vec![(claim_path(&bare.id), None), page_edit()]);
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(!body.contains(note), "{body}");

    // A file the wiki has and cannot read: replaced or removed, it has a line of its own and is curation's.
    let unread = inklkunn_core::knowledge::ClaimId::parse("01930000-0000-7000-8000-000000000003")
        .expect("a claim id");
    s.w.fake
        .set(&claim_path(&unread), "kind: observation\nsecret: words\n");
    let body = open(
        &s,
        &held_unjudged(&s, vec![(claim_path(&unread), None), page_edit()]),
        &s.writer,
    )
    .await
    .body();
    assert!(
        body.contains("Replaces or removes a claim file that cannot be read"),
        "{body}"
    );
    assert!(body.contains(note), "{body}");
    assert!(!body.contains("Removes the claim"), "{body}");
    assert!(!body.contains("secret"), "{body}");
    // Written over with a claim that reads: the same line, and the new claim beside it.
    let mut over = s.claim.clone();
    over.id = unread.clone();
    let body = open(
        &s,
        &held_unjudged(
            &s,
            vec![(claim_path(&unread), Some(over.to_yaml())), page_edit()],
        ),
        &s.writer,
    )
    .await
    .body();
    assert!(
        body.contains("Replaces or removes a claim file that cannot be read"),
        "{body}"
    );
    assert!(body.contains("Records the claim"), "{body}");
    // Alone it is a curation proposal: no warning, and the line stays.
    let body = open(
        &s,
        &held_unjudged(&s, vec![(claim_path(&unread), None)]),
        &s.writer,
    )
    .await
    .body();
    assert!(!body.contains(note), "{body}");
    assert!(body.contains("This is a curation proposal"), "{body}");
    assert!(
        body.contains("Replaces or removes a claim file that cannot be read"),
        "{body}"
    );
}

#[tokio::test]
async fn revising_a_proposal_that_sets_what_only_curation_sets_is_refused_in_the_pages_words() {
    let s = scene();
    let mut decided = s.claim.clone();
    decided.community_status.status = inklkunn_core::knowledge::Standing::Accepted;
    let to = held_unjudged(
        &s,
        vec![
            (claim_path(&decided.id), Some(decided.to_yaml())),
            (
                "pages/Bridge.md".to_owned(),
                Some(BRIDGE.replace("A note.", "A longer note.")),
            ),
        ],
    );
    let revise = vec![
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
            BRIDGE.replace("A note.", "A longer note, again."),
        ),
        ("summary", "A longer note".to_owned()),
        ("sources", String::new()),
    ];
    let a = send(&s, &format!("{to}/edit"), &s.writer, &revise).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    let body = a.body();
    // The sentence the proposal page's warning has: one rule, one set of words.
    assert!(body.contains("only a curation proposal may do"), "{body}");
    // The proposal is as it was: no second revision.
    let page = as_text(open(&s, &to, &s.writer).await.body());
    assert!(!page.contains("longer note, again"), "{page}");
}

// ---------------------------------------------------------------------------------------------- K5: trust never moves unseen

/// The claim as the base has it, decided and judged on an older wording, and a curation proposal that judges it again
/// on the wording the paragraph has now.
fn rejudging(s: &Scene, page: &str, wording: &str) -> String {
    use inklkunn_core::knowledge::{CommunityStatus, Standing};
    let older = block_digest(
        inklkunn_markup::blocks("An older wording. ^k1")
            .first()
            .unwrap(),
    );
    let mut had = s.claim.clone();
    had.community_status = CommunityStatus {
        status: Standing::Accepted,
        proposal: Some("01930000-0000-7000-8000-00000000aaaa".to_owned()),
        judged_on: Some(older.clone()),
    };
    had.sources.first_mut().expect("a link").judged_on = older;
    s.w.fake.set("pages/Bridge.md", page);
    s.w.fake.set(&claim_path(&had.id), &had.to_yaml());
    let now = block_digest(inklkunn_markup::blocks(wording).first().unwrap());
    let mut judged = had.clone();
    judged.community_status.judged_on = Some(now.clone());
    judged.sources.first_mut().expect("a link").judged_on = now;
    // Since K33 the service refuses a judgement on a wording the claim does not have; a record an older release made
    // can hold one, and the page says what it shows then.
    held_unjudged(s, vec![(claim_path(&judged.id), Some(judged.to_yaml()))])
}

#[tokio::test]
async fn a_curation_that_judges_again_shows_each_re_judgement_with_the_wording_it_moves_to() {
    let s = scene();
    let to = rejudging(&s, BRIDGE, "It opened in 1950. ^k1");
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(body.contains("This is a curation proposal"), "{body}");
    // Both the status's and the source's: two lines, neither of them *accepted to accepted*.
    assert!(
        body.contains(
            "What its community status was judged on becomes this wording: “<bdi>It opened in 1950.</bdi>”"
        ),
        "{body}"
    );
    assert!(
        body.contains(
            "What the source <cite><bdi>“A history of the bridge”</bdi></cite> [source <code>01931111-0000-7000-8000-000000000001</code>] that it cites was judged on becomes this wording: “<bdi>It opened in 1950.</bdi>”"
        ),
        "{body}"
    );
    assert!(
        !body.contains("changes from accepted to accepted"),
        "{body}"
    );
}

#[tokio::test]
async fn a_re_judgement_to_wording_no_paragraph_has_says_so_and_wording_is_escaped_and_bounded() {
    let s = scene();
    // The paragraph holds markup and is long; the claim is judged on a wording the page does not have.
    let long = "word ".repeat(200);
    let page = BRIDGE.replace(
        "It opened in 1950. ^k1",
        &format!("It opened <b>in</b> 1950 & more. {long}^k1"),
    );
    let to = rejudging(
        &s,
        &page,
        &format!("It opened <b>in</b> 1950 & more. {long}^k1"),
    );
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(
        body.contains("&lt;b&gt;in&lt;/b&gt; 1950 &amp; more."),
        "{body}"
    );
    assert!(!body.contains("<b>in</b>"), "{body}");
    assert!(body.contains("…</bdi>”"), "the long wording is cut: {body}");
    assert!(!body.contains(&long), "{body}");
    // Judged on a wording that is not on the page: said so, never shown as a wording.
    let to = rejudging(&s, &page, "A wording the page never had. ^k1");
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(
        body.contains("none of the claim&#39;s paragraphs has once this proposal is made"),
        "{body}"
    );
}

#[tokio::test]
async fn a_page_with_knowledge_files_that_change_nothing_says_so() {
    let s = scene();
    let to = held_with(
        &s,
        vec![
            (claim_path(&s.claim.id), Some(s.claim.to_yaml())),
            (
                "pages/Bridge.md".to_owned(),
                Some(BRIDGE.replace("A note.", "A longer note.")),
            ),
        ],
    );
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(
        body.contains("they change nothing"),
        "an empty list is said, not left out: {body}"
    );
}

#[tokio::test]
async fn a_marker_on_a_line_of_its_own_stays_there_in_the_form_for_one_paragraph() {
    let s = scene();
    let page = BRIDGE.replace("It opened in 1950. ^k1", "It opened in 1950.\n^k1");
    s.w.fake.set("pages/Bridge.md", &page);
    // The form is opened on the page as it now is (the second version of the repository).
    let now = |fields| {
        with(
            fields,
            vec![("base_state", state_id(2)), ("base_block", block_id(2))],
        )
    };
    // Nothing touched: not a whitespace-only edit of the page (the marker was not moved onto the words' line).
    let fields = now(part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1950.",
        "submit",
    ));
    let a = send(&s, NEW, &s.writer, &fields).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body().contains("The text is the same as the page"),
        "{}",
        a.body()
    );
    // The words changed: the marker is where it was.
    let fields = now(part(
        "pages/Bridge.md",
        "Bridge",
        0,
        "It opened in 1951.",
        "submit",
    ));
    let a = send(&s, NEW, &s.writer, &fields).await;
    let files = files_of(&s, &location(&a));
    assert_eq!(
        file(&files, "pages/Bridge.md"),
        page.replace("It opened in 1950.\n^k1", "It opened in 1951.\n^k1")
    );
}

#[tokio::test]
async fn deleting_a_claim_paragraph_among_equal_words_is_asked_about_and_never_moved() {
    let s = scene();
    // The claim's paragraph is the second of two with the same words.
    let page = BRIDGE.replace(
        "It opened in 1950. ^k1\n\n",
        "It opened in 1950.\n\nIt opened in 1950. ^k1\n\n",
    );
    s.w.fake.set("pages/Bridge.md", &page);
    let shown = vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", state_id(2)),
        ("base_block", block_id(2)),
        ("p0_path", "pages/Bridge.md".to_owned()),
        ("p0_title", "Bridge".to_owned()),
        (
            "p0_text",
            inklkunn_core::knowledge::marker::hide_markers(
                &page,
                &["k1".to_owned()].into_iter().collect(),
                true,
            ),
        ),
        ("summary", "Tidy".to_owned()),
        ("sources", String::new()),
    ];
    // Sent back untouched with one more edit elsewhere: the marker is on the second, where it was.
    let edited = with(
        shown.clone(),
        vec![(
            "p0_text",
            shown
                .iter()
                .find(|(k, _)| *k == "p0_text")
                .map(|(_, v)| v.replace("A note.", "A longer note."))
                .unwrap_or_default(),
        )],
    );
    let a = send(&s, NEW, &s.writer, &edited).await;
    let files = files_of(&s, &location(&a));
    assert_eq!(
        file(&files, "pages/Bridge.md"),
        page.replace("A note.", "A longer note.")
    );
    // One of the two taken away: which one is not in the text, so the claim's paragraph is asked about.
    let gone = with(
        shown.clone(),
        vec![(
            "p0_text",
            shown
                .iter()
                .find(|(k, _)| *k == "p0_text")
                .map(|(_, v)| v.replacen("It opened in 1950.\n\n", "", 1))
                .unwrap_or_default(),
        )],
    );
    let a = send(&s, NEW, &s.writer, &gone).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(a.body().contains("name=\"drop_k1\""), "{}", a.body());
}

#[tokio::test]
async fn a_source_text_with_a_direction_character_is_refused_on_the_form_with_a_message_of_its_own()
{
    let s = scene();
    for c in ['\u{202e}', '\u{2066}', '\u{200f}', '\u{061c}'] {
        let fields = with(
            part(
                "pages/Bridge.md",
                "Bridge",
                1,
                "It is long, really.",
                "submit",
            ),
            source_fields("supports", &format!("A title {c}reversed")),
        );
        let a = send(&s, NEW, &s.writer, &fields).await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{c:?}");
        let body = a.body();
        assert!(
            body.contains("changes the direction of text or is not shown"),
            "{c:?}: {body}"
        );
        assert!(
            body.contains("It is long, really."),
            "the person's text stays: {body}"
        );
    }
}

/// The page as text: an identifier that leads to its subject's page (K32) is shown as the `<code>` it is, so that a
/// test of a sentence need not spell out the address. (The links have tests of their own.)
pub(crate) fn as_text(html: String) -> String {
    let mut out = String::new();
    let mut rest = html.as_str();
    while let Some((before, after)) = rest.split_once("<a href=\"") {
        out.push_str(before);
        let linked = after
            .split_once("\"><code>")
            .filter(|(href, _)| !href.contains(['"', '<', '>']))
            .and_then(|(_, tail)| tail.split_once("</code></a>"));
        match linked {
            Some((id, more)) => {
                out.push_str("<code>");
                out.push_str(id);
                out.push_str("</code>");
                rest = more;
            }
            None => {
                out.push_str("<a href=\"");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A claim file that gives the claim a second source, judged on the paragraph's present wording, and that source's file.
fn second_link_judged_on_the_paragraph(s: &Scene) -> Vec<(String, Option<String>)> {
    let second = SourceId::parse("01931111-0000-7000-8000-000000000002").expect("a source id");
    let mut source = Source::new(second.clone());
    source.identity.title = Some("A second history".to_owned());
    let mut more = s.claim.clone();
    more.sources.push(SourceLink {
        source: second.clone(),
        relation: Link::Challenges,
        bearing: Bearing::Partial,
        note: None,
        judged_on: block_digest(
            inklkunn_markup::blocks("It opened in 1950. ^k1")
                .first()
                .expect("a block"),
        ),
    });
    more.normalize();
    vec![
        (claim_path(&more.id), Some(more.to_yaml())),
        (source_path(&second), Some(source.to_yaml())),
    ]
}

#[tokio::test]
async fn the_page_judges_against_the_pages_the_service_reads_a_removed_page_and_a_changed_id() {
    let s = scene();
    let now = "judged on the wording it has now.";
    let unjudged = "judged on a wording the claim does not have in this proposal&#39;s result.";
    let removed = ("pages/Bridge.md".to_owned(), None);
    let reid = (
        "pages/Bridge.md".to_owned(),
        Some(BRIDGE.replace("id: bridge-page", "id: another-page")),
    );
    // Control: the page stays as it is, the wording is there, and the page and the service agree that it is sound.
    let sound = second_link_judged_on_the_paragraph(&s);
    let to = held_unjudged(&s, sound.clone());
    let body = as_text(open(&s, &to, &s.writer).await.body());
    assert!(body.contains(now) && !body.contains(unjudged), "{body}");
    let who = principal(&s.writer);
    let send = |files: Vec<(String, Option<String>)>| {
        let draft =
            s.w.services
                .proposals
                .create(
                    &who,
                    &s.w.wiki,
                    None,
                    inklkunn_core::proposal::ProposalEdit {
                        summary: "A change".to_owned(),
                        files: files
                            .into_iter()
                            .map(|(path, text)| inklkunn_store::FileChange { path, text })
                            .collect(),
                        ..Default::default()
                    },
                )
                .expect("a draft");
        s.w.services.proposals.submit(&who, &s.w.wiki, &draft.id)
    };
    assert!(send(sound).is_ok());
    // The page the paragraph is in is taken out, or is given another `id:`: the claim's paragraph is nowhere in the result.
    for (how, page) in [("removed", removed), ("another id", reid)] {
        let mut files = second_link_judged_on_the_paragraph(&s);
        files.push(page);
        let to = held_unjudged(&s, files.clone());
        let body = as_text(open(&s, &to, &s.writer).await.body());
        assert!(!body.contains(now), "{how}: {body}");
        assert!(body.contains(unjudged), "{how}: {body}");
        // The page and the submission say the same.
        assert!(
            matches!(
                send(files),
                Err(inklkunn_core::proposal::ProposalError::Invalid(
                    inklkunn_core::proposal::Invalid::KnowledgeUnjudged
                ))
            ),
            "{how}"
        );
    }
}
