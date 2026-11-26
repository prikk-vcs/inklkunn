//! The governance read API (RFC 006 D-12; handoff G-T7) through the real router over an in-memory repository: the rules of a
//! wiki as data with their block, the protections in force, the moderation log, and a person's own appeals. What each says and
//! what it never says (a name, a note, hidden text, anyone else's appeal), that a wiki you may not read answers as one that is
//! not there, that nothing writes, and that every answer is of the shape the document promises.

use axum::http::{Method, StatusCode};
use inklkunn_core::governance::{
    Constitution, Entry, GovernanceFile, PlatformLimits, ReadFault, Refusal, Rules, read_policy,
};
use inklkunn_store::{GovernanceStore, ProposalId, ProposalStore, Role, Visibility};
use serde_json::Value;

use super::common::{
    self, World, add_person, home_with, person, post_form, under_rules, with_accounts,
};
use super::{At, get, send};

const POLICY: &str = "entry: contributor\nrungs:\n  accept: reviewer\nacceptance:\n  approvals: 5\nknowledge:\n  review_after: 400\npromotion:\n  contributor: \"A few accepted edits.\"\nprotect:\n  - namespace: Help\n    require: review\n    reason: \"Read first.\"\n";

/// The wiki under `POLICY`, read under a limit that lowers `entry`.
fn rules() -> Rules {
    Rules::read(
        Some(common::POLICY_BLOCK.to_owned()),
        read_policy(POLICY.as_bytes()).unwrap(),
        Constitution::default(),
        &PlatformLimits {
            entry_max: Entry::Observer,
            ..PlatformLimits::default()
        },
    )
}

fn base(visibility: Visibility) -> World {
    with_accounts(World::with_visibility(
        "handbook",
        "The Handbook",
        visibility,
        &[("pages/Home.md", &home_with("The bridge opened in 1950."))],
    ))
}

struct Cast {
    w: World,
    admin: inklkunn_store::AccountId,
    maintainer: inklkunn_store::AccountId,
    contributor: inklkunn_store::AccountId,
    other: inklkunn_store::AccountId,
}

fn cast(w: World) -> Cast {
    let c = Cast {
        admin: person(1),
        maintainer: person(2),
        contributor: person(5),
        other: person(7),
        w,
    };
    add_person(&c.w, &c.admin, "Ada Admin", Some(Role::Admin));
    add_person(&c.w, &c.maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&c.w, &c.contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&c.w, &c.other, "Nia Newcomer", Some(Role::Contributor));
    c
}

async fn as_(c: &Cast, who: &inklkunn_store::AccountId, tail: &str) -> super::Answer {
    send(
        &c.w,
        Method::GET,
        &format!("/handbook/-/api/v1/{tail}"),
        &[("x-test-account", who.as_str())],
    )
    .await
}

fn code(a: &super::Answer) -> String {
    a.json().at("/type").as_str().unwrap_or("").to_owned()
}

/// A comment hidden with a note, a restriction appealed and decided, and a temporary protection: what the other members list.
async fn lived_in() -> Cast {
    let c = cast(under_rules(&base(Visibility::Public), rules()));
    let propose = post_form(
        &c.w,
        "/handbook/-/proposals/new",
        Some(&c.contributor),
        &[
            ("op", "submit"),
            ("pages", "1"),
            ("renames", "0"),
            ("part", "whole"),
            ("base_state", &common::state_id(1)),
            ("base_block", &common::block_id(1)),
            ("p0_path", "pages/Home.md"),
            ("p0_title", "Home"),
            ("p0_text", &home_with("The bridge opened in 1951.")),
            ("summary", "Correct the year"),
            ("sources", ""),
        ],
    )
    .await;
    let to = propose.header("location").unwrap().to_owned();
    let id = ProposalId::parse(
        to.split("/proposals/")
            .nth(1)
            .unwrap()
            .split(['/', '?'])
            .next()
            .unwrap(),
    )
    .unwrap();
    let said = post_form(
        &c.w,
        &format!("/handbook/-/proposals/{id}/comment"),
        Some(&c.contributor),
        &[("body", "ZEBRACOMMENT on the proposal.")],
    )
    .await;
    assert_eq!(said.status, StatusCode::SEE_OTHER, "{}", said.body());
    let comment =
        c.w.store
            .comments(&id, None)
            .unwrap()
            .first()
            .expect("a comment")
            .id
            .to_string();
    let hid = post_form(
        &c.w,
        "/handbook/-/moderate",
        Some(&c.maintainer),
        &[
            ("kind", "proposal-comment"),
            ("proposal", id.as_str()),
            ("comment", &comment),
            ("part_content", "1"),
            ("part_author", "1"),
            ("category", "harassment"),
            ("note", "ZEBRANOTE for the moderators only"),
        ],
    )
    .await;
    assert_eq!(hid.status, StatusCode::OK, "{}", hid.body());
    // A restriction, appealed by the restricted person, and decided by the admin.
    let a = post_form(
        &c.w,
        "/handbook/-/people/restrict",
        Some(&c.maintainer),
        &[
            ("who", c.contributor.as_str()),
            ("days", "3"),
            ("reason", "Edit-warring."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let event =
        c.w.store
            .restriction_events(&c.w.wiki.id, &c.contributor)
            .unwrap()
            .remove(0);
    let filed = post_form(
        &c.w,
        "/handbook/-/appeals/new",
        Some(&c.contributor),
        &[
            ("kind", "restriction"),
            ("id", event.id.as_str()),
            ("text", "ZEBRAWORDS I did nothing wrong."),
        ],
    )
    .await;
    assert_eq!(filed.status, StatusCode::SEE_OTHER, "{}", filed.body());
    let appeal = c.w.store.open_appeals(&c.w.wiki.id, 5).unwrap().remove(0);
    let decided = post_form(
        &c.w,
        "/handbook/-/appeals/decide",
        Some(&c.admin),
        &[
            ("appeal", appeal.id.as_str()),
            ("decision", "upheld"),
            ("reason", "The record supports the action."),
        ],
    )
    .await;
    assert_eq!(decided.status, StatusCode::SEE_OTHER, "{}", decided.body());
    // A temporary protection.
    let p = post_form(
        &c.w,
        "/handbook/-/protect",
        Some(&c.maintainer),
        &[
            ("target", "page"),
            ("title", "Home"),
            ("require", "review"),
            ("reason", "Vandalism this week."),
            ("days", "7"),
            ("act", "set"),
        ],
    )
    .await;
    assert_eq!(p.status, StatusCode::SEE_OTHER, "{}", p.body());
    c
}

/// The answers the document's shapes are held to: one for each member, from the wiki that has been lived in and from a wiki
/// whose rules cannot be read.
pub(crate) async fn samples() -> Vec<(String, String, Value)> {
    let route = |tail: &str| format!("/{{slug}}/-/api/v1/{tail}");
    let c = lived_in().await;
    let mut out = Vec::new();
    for (template, tail, who) in [
        ("policy", "policy", &c.contributor),
        ("protections", "protections", &c.contributor),
        ("moderation-log", "moderation-log", &c.contributor),
        ("moderation-log", "moderation-log?limit=1", &c.contributor),
        ("appeals/mine", "appeals/mine", &c.contributor),
    ] {
        let a = as_(&c, who, tail).await;
        assert_eq!(a.status, StatusCode::OK, "{tail}: {}", a.text());
        out.push((route(template), tail.to_owned(), a.json()));
    }
    let fault = ReadFault::from_stored(GovernanceFile::Policy, None, Refusal::NotAFile)
        .with_found(vec!["governance/policy.yaml/a.txt".to_owned()], 2);
    let broken = cast(under_rules(
        &base(Visibility::Public),
        Rules::faulted(Some(common::POLICY_BLOCK.to_owned()), fault),
    ));
    let a = as_(&broken, &broken.contributor, "policy").await;
    out.push((route("policy"), "policy (unreadable)".to_owned(), a.json()));
    let plain = cast(base(Visibility::Public));
    let a = as_(&plain, &plain.contributor, "policy").await;
    out.push((route("policy"), "policy (defaults)".to_owned(), a.json()));
    out
}

#[tokio::test]
async fn the_rules_come_as_data_with_their_block_the_lowered_value_the_limits_and_the_warning() {
    let c = cast(under_rules(&base(Visibility::Public), rules()));
    let a = as_(&c, &c.contributor, "policy").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.text());
    let body = a.json();
    assert_eq!(body.at("/block"), common::POLICY_BLOCK);
    assert_eq!(body.at("/defaults"), false);
    assert_eq!(body.at("/fault"), &Value::Null);
    assert_eq!(body.at("/policy/entry"), "observer", "the value in force");
    assert_eq!(body.at("/policy/rungs/accept"), "reviewer");
    assert_eq!(body.at("/policy/rungs/propose"), "contributor");
    assert_eq!(body.at("/policy/acceptance/approvals"), 5);
    assert_eq!(body.at("/policy/acceptance/self"), "small-wiki");
    assert_eq!(body.at("/policy/knowledge/review_after"), 400);
    assert_eq!(body.at("/policy/knowledge/in_force"), 400);
    assert_eq!(
        body.at("/policy/promotion/contributor"),
        "A few accepted edits."
    );
    assert_eq!(body.at("/policy/protect/0/on"), "namespace");
    assert_eq!(body.at("/policy/protect/0/title"), "Help");
    assert_eq!(body.at("/policy/protect/0/requires"), "review");
    assert_eq!(body.at("/constitution/policy_change/rung"), "maintainer");
    assert_eq!(body.at("/constitution/constitution_change/rung"), "admin");
    assert_eq!(body.at("/lowered/0/field"), "entry");
    assert_eq!(body.at("/lowered/0/was"), "contributor");
    assert_eq!(body.at("/lowered/0/now"), "observer");
    assert_eq!(body.at("/lowered/0/limit"), "INKLKUNN_GOVERNANCE_ENTRY_MAX");
    assert_eq!(body.at("/platform_limits/entry_max"), "contributor");
    assert_eq!(body.at("/platform_limits/limit_days"), 90);
    assert_eq!(body.at("/approvals_warning/needed"), 5);
    // No HTML and no raw file text is in it.
    assert!(!a.text().contains('<'), "{}", a.text());
    assert!(!a.text().contains("rungs:"), "{}", a.text());
}

#[tokio::test]
async fn a_wiki_with_no_rules_says_the_defaults_and_one_that_cannot_be_read_says_which_file_and_where()
 {
    let c = cast(base(Visibility::Public));
    let body = as_(&c, &c.contributor, "policy").await.json();
    assert_eq!(body.at("/block"), &Value::Null);
    assert_eq!(body.at("/defaults"), true);
    assert_eq!(body.at("/policy/rungs/accept"), "maintainer");
    assert_eq!(body.at("/policy/entry"), "none");
    assert_eq!(body.at("/approvals_warning"), &Value::Null);
    assert_eq!(body.at("/policy/knowledge/review_after"), &Value::Null);
    assert_eq!(body.at("/policy/knowledge/in_force"), 1096);
    let fault = ReadFault::from_stored(GovernanceFile::Constitution, Some(4), Refusal::WrongShape);
    let broken = cast(under_rules(
        &base(Visibility::Public),
        Rules::faulted(Some(common::POLICY_BLOCK.to_owned()), fault),
    ));
    let a = as_(&broken, &broken.contributor, "policy").await;
    assert_eq!(a.status, StatusCode::OK, "a read is not refused for it");
    let body = a.json();
    assert_eq!(body.at("/fault/file"), "governance/constitution.yaml");
    assert_eq!(body.at("/fault/rule"), "wrong-shape");
    assert_eq!(body.at("/fault/line"), 4);
    assert_eq!(body.at("/defaults"), false);
    assert_eq!(
        body.at("/policy/rungs/accept"),
        "maintainer",
        "the defaults decide"
    );
}

#[tokio::test]
async fn the_protections_in_force_are_the_policys_and_the_temporary_ones_with_who_set_them() {
    let c = lived_in().await;
    let body = as_(&c, &c.contributor, "protections").await.json();
    assert_eq!(body.at("/items/0/kind"), "standing");
    assert_eq!(body.at("/items/0/title"), "Help");
    assert_eq!(body.at("/items/0/set_by"), &Value::Null);
    assert_eq!(body.at("/items/1/kind"), "temporary");
    assert_eq!(body.at("/items/1/title"), "Home");
    assert_eq!(body.at("/items/1/reason"), "Vandalism this week.");
    assert!(body.at("/items/1/until").as_str().unwrap().ends_with('Z'));
    // The role and the rules, never a name.
    assert_eq!(body.at("/items/1/set_by/role"), "maintainer");
    assert_eq!(body.at("/items/1/set_by/block"), common::POLICY_BLOCK);
    assert!(!body.to_string().contains("Mika"), "{body}");
}

#[tokio::test]
async fn the_moderation_log_is_rows_with_no_text_no_name_and_no_note_and_is_read_by_everyone_who_reads_the_wiki()
 {
    let c = lived_in().await;
    // The public wiki's log is read by a visitor and by a member alike, with the same words.
    let visitor = get(&c.w, "/handbook/-/api/v1/moderation-log").await;
    let member = as_(&c, &c.other, "moderation-log").await;
    let moderator = as_(&c, &c.maintainer, "moderation-log").await;
    for a in [&visitor, &member, &moderator] {
        assert_eq!(a.status, StatusCode::OK, "{}", a.text());
        let text = a.text();
        for secret in ["ZEBRA", "Aiko", "Mika", "Tanaka", "note"] {
            assert!(!text.contains(secret), "{secret}: {text}");
        }
    }
    assert_eq!(visitor.bytes, member.bytes);
    let body = visitor.json();
    assert_eq!(body.at("/items/0/act"), "hide");
    assert_eq!(body.at("/items/0/item/kind"), "proposal-comment");
    assert_eq!(body.at("/items/0/parts/0"), "content");
    assert_eq!(body.at("/items/0/parts/1"), "author");
    assert_eq!(body.at("/items/0/category"), "harassment");
    assert_eq!(body.at("/items/0/rung"), "maintainer");
    assert_eq!(body.at("/items/0/policy_block"), common::POLICY_BLOCK);
    assert_eq!(body.at("/items/0/in_force"), true);
    assert_eq!(body.at("/items/0/appeal"), &Value::Null);
    assert_eq!(body.at("/next_cursor"), &Value::Null);
}

#[tokio::test]
async fn the_log_is_read_a_page_at_a_time_by_a_cursor_and_a_cursor_that_is_not_one_is_refused() {
    let c = lived_in().await;
    // Unhide, so that the log has two rows.
    let log = as_(&c, &c.maintainer, "moderation-log").await.json();
    let hide = log.at("/items/0/id").as_str().unwrap().to_owned();
    let a = post_form(
        &c.w,
        "/handbook/-/moderate/unhide",
        Some(&c.maintainer),
        &[("hide", &hide)],
    )
    .await;
    assert!(
        a.status.is_redirection() || a.status == StatusCode::OK,
        "{}",
        a.body()
    );
    let answer = as_(&c, &c.maintainer, "moderation-log?limit=1").await;
    // The log's own cursor is told in the `Link` too (RFC 007 D-9): the target of the first page is the second.
    let target = super::assert_next_link(&answer, "/handbook/-/api/v1/moderation-log?limit=1")
        .expect("a next page");
    let first = answer.json();
    assert_eq!(first.at("/items/0/act"), "unhide");
    let cursor = first.at("/next_cursor").as_str().unwrap().to_owned();
    assert_eq!(
        target,
        format!("/handbook/-/api/v1/moderation-log?limit=1&cursor={cursor}")
    );
    let answer = as_(
        &c,
        &c.maintainer,
        target.strip_prefix("/handbook/-/api/v1/").unwrap(),
    )
    .await;
    assert_eq!(
        super::assert_next_link(&answer, &target),
        None,
        "the last page has no next"
    );
    let second = answer.json();
    assert_eq!(second.at("/items/0/act"), "hide");
    assert_eq!(second.at("/next_cursor"), &Value::Null);
    for bad in ["nonsense", "019a0000-0000-7000-8000-00000000ffff"] {
        let a = as_(&c, &c.maintainer, &format!("moderation-log?cursor={bad}")).await;
        assert_eq!(a.status, StatusCode::BAD_REQUEST, "{bad}: {}", a.text());
        assert_eq!(code(&a), "urn:inklkunn:problem:invalid-cursor");
    }
}

#[tokio::test]
async fn a_person_reads_their_own_appeals_and_nobody_elses() {
    let c = lived_in().await;
    let mine = as_(&c, &c.contributor, "appeals/mine").await;
    assert_eq!(mine.status, StatusCode::OK, "{}", mine.text());
    let body = mine.json();
    assert_eq!(body.at("/items/0/status"), "upheld");
    assert_eq!(body.at("/items/0/action/kind"), "restriction");
    assert_eq!(body.at("/items/0/text"), "ZEBRAWORDS I did nothing wrong.");
    assert_eq!(
        body.at("/items/0/decision/reason"),
        "The record supports the action."
    );
    // The role the decider held and the rules in force are said (G-1, criterion 12): a role, never a name.
    assert_eq!(body.at("/items/0/decision/rung"), "admin");
    assert_eq!(
        body.at("/items/0/decision/policy_block"),
        common::POLICY_BLOCK
    );
    // Who decided is not named, and nobody else's text is in it.
    assert!(!mine.text().contains("Ada"), "{}", mine.text());
    // Someone else, in the same wiki, has none: not the appellant's, not the decider's.
    for who in [&c.other, &c.admin, &c.maintainer] {
        let a = as_(&c, who, "appeals/mine").await;
        assert_eq!(a.status, StatusCode::OK);
        assert_eq!(a.json().at("/items").as_array().unwrap().len(), 0, "{who}");
        assert!(!a.text().contains("ZEBRA"), "{}", a.text());
    }
    // No credential: an account is needed. (A visitor is not told of anyone's.)
    let a = get(&c.w, "/handbook/-/api/v1/appeals/mine").await;
    assert_eq!(a.status, StatusCode::UNAUTHORIZED, "{}", a.text());
    assert_eq!(code(&a), "urn:inklkunn:problem:authentication-required");
}

#[tokio::test]
async fn a_wiki_you_may_not_read_answers_every_member_as_one_that_is_not_there() {
    let w = under_rules(&base(Visibility::Restricted), rules());
    let stranger = person(9);
    add_person(&w, &stranger, "Sue Stranger", None);
    for tail in ["policy", "protections", "moderation-log", "appeals/mine"] {
        for who in [None, Some(&stranger)] {
            let uri = format!("/handbook/-/api/v1/{tail}");
            let gone = uri.replace("handbook", "no-such-wiki");
            let headers: Vec<(&str, &str)> = who
                .map(|a| vec![("x-test-account", a.as_str())])
                .unwrap_or_default();
            let there = send(&w, Method::GET, &uri, &headers).await;
            let absent = send(&w, Method::GET, &gone, &headers).await;
            assert_eq!(there.status, StatusCode::NOT_FOUND, "{tail}");
            assert_eq!(there.status, absent.status, "{tail}");
            assert_eq!(
                there.text().replace("handbook", "no-such-wiki"),
                absent.text(),
                "{tail}"
            );
            assert_eq!(there.header("content-type"), absent.header("content-type"));
        }
    }
}

#[tokio::test]
async fn every_member_has_an_etag_answers_304_to_it_and_takes_no_write() {
    let c = cast(under_rules(&base(Visibility::Public), rules()));
    for tail in ["policy", "protections", "moderation-log"] {
        let uri = format!("/handbook/-/api/v1/{tail}");
        let first = get(&c.w, &uri).await;
        assert_eq!(first.status, StatusCode::OK, "{tail}");
        let etag = first.header("etag").expect("an etag").to_owned();
        let again = send(&c.w, Method::GET, &uri, &[("if-none-match", &etag)]).await;
        assert_eq!(again.status, StatusCode::NOT_MODIFIED, "{tail}");
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            let a = send(
                &c.w,
                method.clone(),
                &uri,
                &[("x-test-account", c.admin.as_str())],
            )
            .await;
            // Refused, and changed nothing: before the method is looked at a write without the site's own token is refused too.
            assert!(a.status.is_client_error(), "{method} {tail}: {}", a.status);
        }
    }
    // The route table has no write for any of them, whatever the method.
    let table = inklkunn_web::routes::application_table().unwrap();
    for entry in table.entries() {
        if ["policy", "protections", "moderation-log", "appeals/mine"]
            .iter()
            .any(|m| entry.path.ends_with(&format!("/-/api/v1/{m}")))
        {
            assert_eq!(entry.methods, ["GET", "HEAD"], "{}", entry.path);
        }
    }
}

#[tokio::test]
async fn the_document_says_moderation_restrictions_protections_grants_and_appeal_decisions_are_web_only()
 {
    let doc = get(
        &common::World::new("handbook", "The Handbook", &[]),
        inklkunn_web::api::OPENAPI_PATH,
    )
    .await
    .json();
    let sentence = "Moderation, restrictions, protections, grants and the decisions of appeals are web-only in 0.4.0";
    assert!(
        doc.at("/info/description")
            .as_str()
            .unwrap()
            .contains(sentence)
    );
    for path in ["policy", "protections", "moderation-log", "appeals/mine"] {
        let op = doc.at(&format!(
            "/paths/~1{{slug}}~1-~1api~1v1~1{}/get",
            path.replace('/', "~1")
        ));
        assert!(
            op.at("/description").as_str().unwrap().contains(sentence),
            "{path}: {op}"
        );
        assert_eq!(op.at("/x-inklkunn-action"), "read");
    }
    // Nothing of the governance is a write in the document.
    for (path, item) in doc.at("/paths").as_object().unwrap() {
        if [
            "/policy",
            "/protections",
            "/moderation-log",
            "/appeals/mine",
        ]
        .iter()
        .any(|m| path.ends_with(m))
        {
            assert!(
                item.get("post").is_none() && item.get("put").is_none(),
                "{path}"
            );
        }
    }
}

#[tokio::test]
async fn every_answer_is_of_the_shape_the_document_promises() {
    let w = World::new("handbook", "The Handbook", &[]);
    let document = get(&w, inklkunn_web::api::OPENAPI_PATH).await.json();
    let mut checker = super::shapes::Checker::new(&document);
    let mut problems = Vec::new();
    for (template, tail, body) in samples().await {
        let schema = super::shapes::schema_of_operation(&document, &template);
        checker.check(&schema, &body, &tail, &mut problems);
    }
    assert!(problems.is_empty(), "{problems:#?}");
    // Every member a schema declares was in some answer.
    let schemas = document.at("/components/schemas").as_object().unwrap();
    for name in [
        "Policy",
        "Protection",
        "ProtectionInForce",
        "Protections",
        "ModerationLogRow",
        "ModerationLog",
        "MyAppeal",
        "MyAppeals",
    ] {
        let declared = schemas
            .get(name)
            .and_then(|s| s.get("properties"))
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{name} is not declared"));
        let seen = checker.seen.get(name).cloned().unwrap_or_default();
        for member in declared.keys() {
            assert!(
                seen.contains(member),
                "{name}.{member} is declared and no answer has it (seen: {seen:?})"
            );
        }
    }
}
