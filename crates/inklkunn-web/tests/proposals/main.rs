//! Proposals on the web (RFC 004 D-2, D-3, D-12; handoff 004 T3): the form, the proposal's page, review, the
//! thin forms, the discussion, and what is refused and how. The repository is an in-memory double, so these run
//! in both test runs; accounts are the test identity's (the header `x-test-account`), roles are the store's.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod accept;
#[path = "../common/mod.rs"]
mod common;
mod flows;
mod review;
mod safety;

use axum::http::StatusCode;
use common::{Answer, World, add_person, get_as, page, person, post_form, with_accounts};
use inklkunn_store::{AccountId, Role};

pub(crate) const HOME: &str =
    "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond paragraph.\n\nThird paragraph.\n";

/// A wiki with a few pages and five people: a contributor, a reviewer, a maintainer, an administrator and an
/// observer.
pub(crate) struct Cast {
    pub(crate) w: World,
    pub(crate) contributor: AccountId,
    pub(crate) other: AccountId,
    pub(crate) reviewer: AccountId,
    pub(crate) maintainer: AccountId,
    pub(crate) admin: AccountId,
    pub(crate) observer: AccountId,
}

pub(crate) fn cast() -> Cast {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[
            ("pages/Home.md", HOME),
            (
                "pages/Guide.md",
                &page(
                    &["title: Guide"],
                    "# Setup\n\nInstall it.\n\n## Details\n\nMore.\n\n# Use\n\nUse it.\n",
                ),
            ),
        ],
    );
    let w = with_accounts(w);
    let c = Cast {
        contributor: person(1),
        other: person(2),
        reviewer: person(3),
        maintainer: person(4),
        admin: person(5),
        observer: person(6),
        w,
    };
    add_person(&c.w, &c.contributor, "Aiko Tanaka", Some(Role::Contributor));
    add_person(&c.w, &c.other, "Ben Okoye", Some(Role::Contributor));
    add_person(&c.w, &c.reviewer, "Rin Sato", Some(Role::Reviewer));
    add_person(&c.w, &c.maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&c.w, &c.admin, "Ada Admin", Some(Role::Admin));
    add_person(&c.w, &c.observer, "Olu Reader", Some(Role::Observer));
    c
}

/// The fields a form of the page `Home` posts, with `text` for its text.
pub(crate) fn home_fields<'a>(
    text: &'a str,
    summary: &'a str,
    op: &'a str,
) -> Vec<(&'static str, String)> {
    let tip = common::state_id(1);
    let block = common::block_id(1);
    vec![
        ("op", op.to_owned()),
        ("pages", "1".to_owned()),
        ("renames", "0".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", tip),
        ("base_block", block),
        ("p0_path", "pages/Home.md".to_owned()),
        ("p0_title", "Home".to_owned()),
        ("p0_text", text.to_owned()),
        ("summary", summary.to_owned()),
        ("sources", String::new()),
    ]
}

/// Post `fields` to `uri` as `account`.
pub(crate) async fn send(
    c: &Cast,
    uri: &str,
    account: &AccountId,
    fields: &[(&str, String)],
) -> Answer {
    let borrowed: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
    post_form(&c.w, uri, Some(account), &borrowed).await
}

/// The address a redirect leads to.
pub(crate) fn location(a: &Answer) -> String {
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    a.header("location").unwrap().to_owned()
}

/// Open a proposal that changes the second paragraph of `Home` as `account`, held for review, and give its
/// address.
pub(crate) async fn propose(c: &Cast, account: &AccountId, summary: &str) -> String {
    let text = HOME.replace("Second paragraph.", "Second paragraph, better.");
    let a = send(
        c,
        "/handbook/-/proposals/new",
        account,
        &home_fields(&text, summary, "submit"),
    )
    .await;
    location(&a)
}

pub(crate) async fn read(c: &Cast, uri: &str, account: Option<&AccountId>) -> Answer {
    get_as(&c.w, uri, account).await
}
