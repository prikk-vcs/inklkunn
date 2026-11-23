//! The people of a wiki through the real router (RFC 006 D-6, D-7; handoff G-T2; acceptance criterion 6): the people page,
//! the forms that set a role, restrict a person, renew and lift a restriction, and what a restricted person is told on every
//! page that refuses them. The records are the in-memory store's and the repository is a double, so these run in both test
//! runs. Every name and sentence is invented.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::http::StatusCode;
use common::{
    Answer, NOW, World, add_person, get, get_as, home_with, person, post_form, under, with_accounts,
};
use inklkunn_store::{
    AccountId, GovernanceStore, MeasureEvent, RestrictionEvent, Role, RoleStore, Visibility,
};

const PEOPLE: &str = "/handbook/-/people";
const DAY: i64 = 86_400;

struct Env {
    w: World,
    admin: AccountId,
    maintainer: AccountId,
    other_maintainer: AccountId,
    reviewer: AccountId,
    contributor: AccountId,
    observer: AccountId,
    newcomer: AccountId,
}

fn cast(w: World) -> Env {
    let e = Env {
        admin: person(1),
        maintainer: person(2),
        other_maintainer: person(3),
        reviewer: person(4),
        contributor: person(5),
        observer: person(6),
        newcomer: person(7),
        w,
    };
    add_person(&e.w, &e.admin, "Ada Admin", Some(Role::Admin));
    add_person(&e.w, &e.maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(
        &e.w,
        &e.other_maintainer,
        "Max Mori",
        Some(Role::Maintainer),
    );
    add_person(&e.w, &e.reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(&e.w, &e.contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&e.w, &e.observer, "Olu Reader", Some(Role::Observer));
    add_person(&e.w, &e.newcomer, "Nia Newcomer", None);
    e
}

fn base(visibility: Visibility) -> World {
    with_accounts(World::with_visibility(
        "handbook",
        "The Handbook",
        visibility,
        &[("pages/Home.md", &home_with("The bridge opened in 1950."))],
    ))
}

/// A public wiki under the default rules.
fn env() -> Env {
    cast(base(Visibility::Public))
}

/// A public wiki under `policy`.
fn env_under(policy: &str) -> Env {
    cast(under(&base(Visibility::Public), policy))
}

fn main_of(a: &Answer) -> String {
    let body = a.body();
    let start = body.find("<main").expect("a main landmark");
    let end = body.find("</main>").expect("the end of main");
    body.get(start..end).expect("main").to_owned()
}

fn restriction_of(e: &Env, who: &AccountId) -> Vec<RestrictionEvent> {
    e.w.store.restriction_events(&e.w.wiki.id, who).unwrap()
}

fn grant_of(e: &Env, who: &AccountId) -> Option<Role> {
    e.w.store.role(who, &e.w.wiki.id).unwrap()
}

async fn as_(e: &Env, who: &AccountId, uri: &str) -> Answer {
    get_as(&e.w, uri, Some(who)).await
}

/// The same, for a reader who chose Japanese.
async fn as_ja(e: &Env, who: &AccountId, uri: &str) -> Answer {
    common::request(
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

fn set_form<'a>(who: &'a str, role: &'a str, reason: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("who", who), ("role", role), ("reason", reason)]
}

fn restrict_form<'a>(who: &'a str, days: &'a str, reason: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![("who", who), ("days", days), ("reason", reason)]
}

const POLICY: &str = "rungs:\n  grant_up_to: reviewer\npromotion:\n  contributor: \"A few edits that were accepted without a correction.\"\n  reviewer: \"A season of careful reading.\"\n";

mod forms;
mod page;
mod refusals;
