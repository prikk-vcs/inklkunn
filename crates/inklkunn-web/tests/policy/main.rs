//! The rules of a wiki through the real router (RFC 006 D-4, D-11; handoff G-T7; acceptance criteria 2, 12 and 15): the policy page,
//! the form that proposes a change to them, the sentences of a change on a proposal's page, and the rung and the rules on every
//! decision. The records are the in-memory store's and the repository is a double, so these run in both test runs; the proof on a
//! real prikk is `real_prikk_policy.rs`. Every name and sentence is invented.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::http::StatusCode;
use common::{
    Answer, World, add_person, get_as, home_with, person, post_form, under, under_rules,
    with_accounts,
};
use inklkunn_store::{AccountId, ProposalId, ProposalStore, Role, Visibility};

mod authority;
mod closed;
mod decisions;
mod form;
mod page;

pub(crate) const PAGE: &str = "/handbook/-/policy";
pub(crate) const EDIT: &str = "/handbook/-/policy/edit";

pub(crate) struct Env {
    pub(crate) w: World,
    pub(crate) admin: AccountId,
    pub(crate) maintainer: AccountId,
    pub(crate) reviewer: AccountId,
    pub(crate) contributor: AccountId,
    pub(crate) observer: AccountId,
    pub(crate) newcomer: AccountId,
}

pub(crate) fn cast(w: World) -> Env {
    let e = Env {
        admin: person(1),
        maintainer: person(2),
        reviewer: person(4),
        contributor: person(5),
        observer: person(6),
        newcomer: person(7),
        w,
    };
    add_person(&e.w, &e.admin, "Ada Admin", Some(Role::Admin));
    add_person(&e.w, &e.maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&e.w, &e.reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(&e.w, &e.contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&e.w, &e.observer, "Olu Reader", Some(Role::Observer));
    add_person(&e.w, &e.newcomer, "Nia Newcomer", None);
    e
}

pub(crate) fn base(visibility: Visibility) -> World {
    with_accounts(World::with_visibility(
        "handbook",
        "The Handbook",
        visibility,
        &[("pages/Home.md", &home_with("The bridge opened in 1950."))],
    ))
}

/// A public wiki under the default rules.
pub(crate) fn env() -> Env {
    cast(base(Visibility::Public))
}

/// A public wiki under `policy`.
pub(crate) fn env_under(policy: &str) -> Env {
    cast(under(&base(Visibility::Public), policy))
}

/// A public wiki under any rules.
pub(crate) fn env_rules(rules: inklkunn_core::governance::Rules) -> Env {
    cast(under_rules(&base(Visibility::Public), rules))
}

pub(crate) fn main_of(a: &Answer) -> String {
    let body = a.body();
    let start = body.find("<main").expect("a main landmark");
    let end = body.find("</main>").expect("the end of main");
    body.get(start..end).expect("main").to_owned()
}

pub(crate) async fn as_(e: &Env, who: &AccountId, uri: &str) -> Answer {
    get_as(&e.w, uri, Some(who)).await
}

/// The same, for a reader who chose Japanese.
pub(crate) async fn as_ja(e: &Env, who: Option<&AccountId>, uri: &str) -> Answer {
    let account = who.map(|w| w.as_str().to_owned());
    let mut headers = vec![("cookie", "inklkunn_lang=ja")];
    if let Some(a) = &account {
        headers.push(("x-test-account", a.as_str()));
    }
    common::request(&e.w, common::APP, "GET", uri, &headers).await
}

/// The words a page must never say: a key of the files as the files write it, and *official*.
pub(crate) const RAW_YAML: &[&str] = &[
    "rungs.",
    "acceptance.",
    "create_view",
    "decide_appeal",
    "grant_up_to",
    "review_after",
    "small-wiki",
    "policy_change",
    "constitution_change",
    "entry:",
    "rungs:",
    "acceptance:",
    "protect:",
];

/// **Criterion 15**: nothing the reader is shown is a key as a file writes it, and nothing is called official, in either language.
pub(crate) fn assert_plain_words(what: &str, html: &str) {
    for raw in RAW_YAML {
        assert!(
            !html.contains(raw),
            "{what} holds the raw key `{raw}`: {html}"
        );
    }
    let lower = html.to_lowercase();
    assert!(!lower.contains("official"), "{what}: {html}");
    assert!(!html.contains("公式"), "{what}: {html}");
}

/// The proposal a redirect to `/handbook/-/proposals/<id>` names.
pub(crate) fn proposal_of(a: &Answer) -> ProposalId {
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let to = a.header("location").unwrap().to_owned();
    let tail = to.split("/proposals/").nth(1).expect("an address");
    ProposalId::parse(tail.split(['/', '?', '#']).next().unwrap()).expect("an id")
}

/// What the latest revision of a proposal writes: the path and the text (`None`: removes it).
pub(crate) fn files_of(w: &World, id: &ProposalId) -> Vec<(String, Option<String>)> {
    let revisions = w.store.revisions(id, None).unwrap();
    let revision = revisions.last().expect("a revision");
    revision
        .content
        .files
        .iter()
        .map(|f| (f.path.clone(), f.text.clone()))
        .collect()
}

fn unescape(text: &str) -> String {
    text.replace("&#34;", "\"")
        .replace("&#38;", "&")
        .replace("&#39;", "'")
        .replace("&#60;", "<")
        .replace("&#62;", ">")
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    let end = at + tag.get(at..)?.find('"')?;
    Some(unescape(tag.get(at..end)?))
}

/// The fields a form page would send as it is: every input's value and every select's chosen option (the first when none is).
pub(crate) fn form_fields(html: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        rest = rest.get(at..).unwrap_or("");
        if rest.starts_with("<input ") {
            let end = rest.find('>').unwrap_or(rest.len());
            let tag = rest.get(..end).unwrap_or("");
            if let Some(name) = attr(tag, "name").filter(|n| n != "csrf") {
                out.push((name, attr(tag, "value").unwrap_or_default()));
            }
            rest = rest.get(end..).unwrap_or("");
        } else if rest.starts_with("<select ") {
            let end = rest.find("</select>").unwrap_or(rest.len());
            let block = rest.get(..end).unwrap_or("");
            let name = attr(
                block.get(..block.find('>').unwrap_or(0)).unwrap_or(""),
                "name",
            );
            let mut chosen: Option<String> = None;
            let mut first: Option<String> = None;
            for option in block.split("<option ").skip(1) {
                let tag = option.get(..option.find('>').unwrap_or(0)).unwrap_or("");
                let value = attr(&format!(" {tag}"), "value").unwrap_or_default();
                first.get_or_insert(value.clone());
                if tag.contains(" selected") {
                    chosen = Some(value);
                }
            }
            if let (Some(name), Some(value)) = (name, chosen.or(first)) {
                out.push((name, value));
            }
            rest = rest.get(end..).unwrap_or("");
        } else {
            rest = rest.get(1..).unwrap_or("");
        }
    }
    out
}

/// `fields` with `name` set to `value` (added when the form has none).
pub(crate) fn with(fields: &[(String, String)], name: &str, value: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fields.to_vec();
    match out.iter_mut().find(|(n, _)| n == name) {
        Some(field) => field.1 = value.to_owned(),
        None => out.push((name.to_owned(), value.to_owned())),
    }
    out
}

pub(crate) async fn post_fields(
    w: &World,
    who: Option<&AccountId>,
    fields: &[(String, String)],
) -> Answer {
    let borrowed: Vec<(&str, &str)> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    post_form(w, EDIT, who, &borrowed).await
}

/// The form as `who` is shown it, filled; the field `act` set to what a button sends.
pub(crate) async fn opened(e: &Env, who: &AccountId, act: &str) -> Vec<(String, String)> {
    let a = as_(e, who, EDIT).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    with(&form_fields(&main_of(&a)), "act", act)
}
