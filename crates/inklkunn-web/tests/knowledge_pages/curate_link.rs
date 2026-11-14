//! The way to the curation form (RFC 005 part 2 D-3; handoff 005 K39), through the real router: the form
//! `…/proposals/curate?claim=<id>` is the only place a community status, a review day or an availability is proposed,
//! so a claim's page and each claim named in a row of *Sources to review* lead to it, for those who may propose and
//! for nobody else, and the link, followed, opens the form. Every name and sentence is invented.

use axum::http::StatusCode;
use inklkunn_core::knowledge::{Bearing, Link, SourceKind, Standing};
use inklkunn_store::{AccountId, Role};

use super::common::{
    World, add_person, claim, get_as, home_with, link, person, put, source, with_accounts,
};
use super::{claim_uri, main_of};

const SAID: &str = "The bridge opened in 1950.";
const WORDS: &str = "Propose a change to its status or sources";

struct Cast {
    w: World,
    contributor: AccountId,
    observer: AccountId,
}

/// A wiki with a claim that cites a source nobody reviewed (so a row of *Sources to review* names the claim), a person
/// who may propose and one who may only read.
fn cast() -> Cast {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &home_with(SAID))],
    );
    let mut c = claim(1, Standing::Contested, None, SAID);
    c.sources = vec![link(1, Link::Supports, Bearing::Direct, SAID)];
    put(
        &w,
        &home_with(SAID),
        &[c],
        &[source(1, SourceKind::Primary)],
    );
    let w = with_accounts(w);
    let cast = Cast {
        contributor: person(2),
        observer: person(4),
        w,
    };
    add_person(
        &cast.w,
        &cast.contributor,
        "Ben Okoye",
        Some(Role::Contributor),
    );
    add_person(&cast.w, &cast.observer, "Olu Reader", Some(Role::Observer));
    cast
}

fn form_of(n: u8) -> String {
    format!(
        "/handbook/-/proposals/curate?claim={}",
        super::common::claim_id(n)
    )
}

async fn main_as(c: &Cast, uri: &str, who: Option<&AccountId>) -> String {
    let a = get_as(&c.w, uri, who).await;
    assert_eq!(a.status, StatusCode::OK, "{uri}");
    main_of(&a)
}

#[tokio::test]
async fn a_claim_s_page_leads_to_its_curation_form_for_who_may_propose_and_to_nobody_else() {
    let c = cast();
    let uri = claim_uri(1);
    let body = main_as(&c, &uri, Some(&c.contributor)).await;
    assert!(
        body.contains(&format!("<a href=\"{}\">{WORDS}</a>", form_of(1))),
        "{body}"
    );
    // A reader who may not propose, a visitor with no account: the page is the page it was.
    for who in [Some(&c.observer), None] {
        let body = main_as(&c, &uri, who).await;
        assert!(!body.contains("proposals/curate"), "{body}");
        assert!(!body.contains(WORDS), "{body}");
    }
}

#[tokio::test]
async fn the_link_on_a_claim_s_page_opens_the_form_of_that_claim() {
    let c = cast();
    let body = main_as(&c, &claim_uri(1), Some(&c.contributor)).await;
    let href = body
        .split("<a href=\"")
        .filter_map(|rest| rest.split_once('"').map(|(href, _)| href))
        .find(|href| href.contains("proposals/curate"))
        .expect("a link to the form");
    let opened = get_as(&c.w, &href.replace("&#38;", "&"), Some(&c.contributor)).await;
    assert_eq!(opened.status, StatusCode::OK);
    let form = opened.body();
    assert!(form.contains("<h1>Curate a claim</h1>"), "{form}");
    assert!(
        form.contains(&format!(
            "name=\"claim\" value=\"{}\"",
            super::common::claim_id(1)
        )),
        "{form}"
    );
}

#[tokio::test]
async fn each_claim_in_a_row_of_sources_to_review_leads_to_its_form_beside_the_claim() {
    let c = cast();
    let list = "/handbook/-/health/sources-to-review";
    let body = main_as(&c, list, Some(&c.contributor)).await;
    // The claim's own link and the way to its form are on one line, in that order.
    let claim_link = format!("<a href=\"{}\">", claim_uri(1));
    let at = body.find(&claim_link).expect("the claim's link");
    let rest = body.get(at..).expect("the rest");
    let line = rest.split("</td>").next().expect("the cell");
    assert!(
        line.contains(&format!(" · <a href=\"{}\">{WORDS}</a>", form_of(1))),
        "{body}"
    );
    for who in [Some(&c.observer), None] {
        let body = main_as(&c, list, who).await;
        assert!(body.contains(&claim_link), "{body}");
        assert!(!body.contains("proposals/curate"), "{body}");
        assert!(!body.contains(WORDS), "{body}");
    }
}

#[tokio::test]
async fn the_link_is_said_in_japanese_too() {
    let c = cast();
    let a = super::common::request(
        &c.w,
        super::common::APP,
        "GET",
        &claim_uri(1),
        &[
            ("x-test-account", c.contributor.as_str()),
            ("cookie", "inklkunn_lang=ja"),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    assert!(
        main_of(&a).contains("この主張の扱いや出典の変更を提案する"),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_claim_read_in_a_view_leads_to_the_form_in_that_view() {
    let c = cast();
    let uri = format!("{}?view=main", claim_uri(1));
    let body = main_as(&c, &uri, Some(&c.contributor)).await;
    let form = format!("{}&#38;view=main", form_of(1));
    assert!(
        body.contains(&format!("<a href=\"{form}\">{WORDS}</a>")),
        "{body}"
    );
    let opened = get_as(
        &c.w,
        &format!("{}&view=main", form_of(1)),
        Some(&c.contributor),
    )
    .await;
    assert_eq!(opened.status, StatusCode::OK);
    assert!(
        opened.body().contains("name=\"view\" value=\"main\""),
        "{}",
        opened.body()
    );
}
