//! Appeals through the real router (RFC 006 D-9, D-12; handoff G-T5; acceptance criterion 9; threat model C-14g): *contest this* on
//! the refusal a restriction gives and on a declined proposal, the form, the person's own page, the page of the people who decide
//! and its decision form, the refusals by name, the limits, and both languages. The records are the in-memory store's and the
//! repository is a double, so these run in both test runs. Every name and sentence is invented, and the words of an appeal
//! carry the word `ZEBRA`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::http::StatusCode;
use common::{
    Answer, World, add_person, get_as, home_with, person, post_form, request, under, with_accounts,
};
use inklkunn_store::{
    AccountId, AppealRecord, AppealState, GovernanceStore, ProposalId, ProposalState,
    ProposalStore, RestrictionEvent, Role, RoleStore, Visibility,
};

const GRIEVANCE: &str = "ZEBRAWORDS I did nothing that the rules of this wiki forbid.";
const REASON: &str = "The record supports the appeal.";

struct Env {
    w: World,
    admin: AccountId,
    maintainer: AccountId,
    contributor: AccountId,
    bystander: AccountId,
}

fn cast(w: World) -> Env {
    let e = Env {
        admin: person(1),
        maintainer: person(2),
        contributor: person(5),
        bystander: person(7),
        w,
    };
    add_person(&e.w, &e.admin, "Ada Admin", Some(Role::Admin));
    add_person(&e.w, &e.maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&e.w, &e.contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&e.w, &e.bystander, "Nia Newcomer", Some(Role::Contributor));
    e
}

fn base() -> World {
    with_accounts(World::with_visibility(
        "handbook",
        "The Handbook",
        Visibility::Public,
        &[("pages/Home.md", &home_with("The bridge opened in 1950."))],
    ))
}

fn env() -> Env {
    cast(base())
}

fn env_under(policy: &str) -> Env {
    cast(under(&base(), policy))
}

async fn as_(e: &Env, who: &AccountId, uri: &str) -> Answer {
    get_as(&e.w, uri, Some(who)).await
}

async fn as_ja(e: &Env, who: &AccountId, uri: &str) -> Answer {
    request(
        &e.w,
        common::APP,
        "GET",
        uri,
        &[
            ("x-test-account", who.as_str()),
            ("cookie", "inklkunn_lang=ja"),
        ],
    )
    .await
}

fn main_of(a: &Answer) -> String {
    let body = a.body();
    let start = body.find("<main").expect("a main landmark");
    let end = body.find("</main>").expect("the end of main");
    body.get(start..end).expect("main").to_owned()
}

/// Restrict `who` for `days` as the maintainer, through the people form.
async fn restrict(e: &Env, who: &AccountId, days: &str) {
    let who = who.to_string();
    let a = post_form(
        &e.w,
        "/handbook/-/people/restrict",
        Some(&e.maintainer),
        &[("who", &who), ("days", days), ("reason", "Edit-warring.")],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
}

fn restriction_of(e: &Env, who: &AccountId) -> RestrictionEvent {
    e.w.store
        .restriction_events(&e.w.wiki.id, who)
        .unwrap()
        .into_iter()
        .next()
        .expect("a restriction")
}

fn open_appeals(e: &Env) -> Vec<AppealRecord> {
    e.w.store.open_appeals(&e.w.wiki.id, 50).unwrap()
}

async fn file(e: &Env, who: &AccountId, kind: &str, id: &str) -> Answer {
    post_form(
        &e.w,
        "/handbook/-/appeals/new",
        Some(who),
        &[("kind", kind), ("id", id), ("text", GRIEVANCE)],
    )
    .await
}

async fn decide(e: &Env, who: &AccountId, appeal: &AppealRecord, decision: &str) -> Answer {
    post_form(
        &e.w,
        "/handbook/-/appeals/decide",
        Some(who),
        &[
            ("appeal", appeal.id.as_str()),
            ("decision", decision),
            ("reason", REASON),
        ],
    )
    .await
}

fn proposal_fields(summary: &str) -> Vec<(&'static str, String)> {
    vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", common::state_id(1)),
        ("base_block", common::block_id(1)),
        ("p0_path", "pages/Home.md".to_owned()),
        ("p0_title", "Home".to_owned()),
        (
            "p0_text",
            home_with("The bridge opened in 1951, after the floods."),
        ),
        ("summary", summary.to_owned()),
        ("sources", String::new()),
    ]
}

async fn propose(e: &Env, who: &AccountId, summary: &str) -> ProposalId {
    let f = proposal_fields(summary);
    let borrowed: Vec<(&str, &str)> = f.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let a = post_form(&e.w, "/handbook/-/proposals/new", Some(who), &borrowed).await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let to = a.header("location").unwrap().to_owned();
    let tail = to.split("/proposals/").nth(1).expect("an address");
    ProposalId::parse(tail.split(['/', '?', '#']).next().unwrap()).expect("an id")
}

async fn decline(e: &Env, who: &AccountId, id: &ProposalId) {
    let a = post_form(
        &e.w,
        &format!("/handbook/-/proposals/{id}/decline"),
        Some(who),
        &[("reason", "Thank you. Not now.")],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
}

mod decisions;
mod pages;
