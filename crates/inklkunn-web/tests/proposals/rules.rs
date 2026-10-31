//! The service's own rules for the files of a proposal (corrections P4 and P5), reached through the form: a
//! removal, a rename and a new path are checked by the service whatever the form checked first, and the
//! ruling on redirects (D8) holds on the form.

use axum::http::StatusCode;

use super::{HOME, cast, home_fields, send};
use crate::common;

/// The fields of a form over version 2 of the repository (`Hidden` exists there) that touches one page and,
/// when `rename` is given, declares one rename.
fn fields_at_two(
    page: (&str, &str, bool),
    rename: Option<(&str, &str)>,
    summary: &str,
) -> Vec<(&'static str, String)> {
    let (path, text, removed) = page;
    let mut fields = vec![
        ("op", "submit".to_owned()),
        ("pages", "1".to_owned()),
        (
            "renames",
            if rename.is_some() { "1" } else { "0" }.to_owned(),
        ),
        ("part", "whole".to_owned()),
        ("base_state", common::state_id(2)),
        ("base_block", common::block_id(2)),
        ("p0_path", path.to_owned()),
        ("p0_title", path.to_owned()),
        ("p0_text", text.to_owned()),
        ("summary", summary.to_owned()),
        ("sources", String::new()),
    ];
    if removed {
        fields.push(("p0_removed", "1".to_owned()));
    }
    if let Some((from, to)) = rename {
        fields.push(("rn0_from", from.to_owned()));
        fields.push(("rn0_to", to.to_owned()));
    }
    fields
}

const HIDDEN: &str = "---\ntitle: Hidden\nvisibility: restricted\n---\n\nSecret.\n";
const URL: &str = "/handbook/-/proposals/new";
const OWNED_SENTENCE: &str = "can only be changed by an administrator";

#[tokio::test]
async fn the_form_leaves_removals_renames_and_new_paths_to_the_service_and_it_refuses_them() {
    let c = cast();
    c.w.fake.set("pages/Hidden.md", HIDDEN);
    // Removing a page that carries an owned key counts as changing it.
    let a = send(
        &c,
        URL,
        &c.contributor,
        &fields_at_two(("pages/Hidden.md", "", true), None, "Remove it"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(
        a.body().contains(&format!(
            "The lines visibility at the top of the page {OWNED_SENTENCE}"
        )),
        "{}",
        a.body()
    );
    // So does renaming it.
    let a = send(
        &c,
        URL,
        &c.contributor,
        &fields_at_two(
            ("pages/Home.md", HOME, false),
            Some(("pages/Hidden.md", "pages/Moved.md")),
            "Move it",
        ),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(a.body().contains(OWNED_SENTENCE), "{}", a.body());
    // An administrator may do both.
    let a = send(
        &c,
        URL,
        &c.admin,
        &fields_at_two(("pages/Hidden.md", "", true), None, "Remove it"),
    )
    .await;
    assert!(a.status.is_redirection(), "{}", a.body());
    // A governance file cannot be moved into pages/, nor a page out of them.
    for rename in [
        ("governance/rules.md", "pages/Rules.md"),
        ("pages/Home.md", "governance/Home.md"),
    ] {
        let a = send(
            &c,
            URL,
            &c.contributor,
            &fields_at_two(("pages/Home.md", HOME, false), Some(rename), "Move it"),
        )
        .await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{rename:?}");
        assert!(
            a.body().contains("The form was not complete."),
            "{rename:?}: {}",
            a.body()
        );
    }
    // pages/home.md beside pages/Home.md, by a move.
    let a = send(
        &c,
        URL,
        &c.contributor,
        &fields_at_two(
            ("pages/Home.md", HOME, false),
            Some(("pages/Guide.md", "pages/home.md")),
            "Move it",
        ),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(
        a.body()
            .contains("differs from this one only in capital letters"),
        "{}",
        a.body()
    );
}

#[tokio::test]
async fn a_contributor_may_make_a_page_a_redirect_through_the_form_and_the_ruling_stops_there() {
    let c = cast();
    let redirect = HOME.replace(
        "title: Home",
        "title: Home\nkind: redirect\nredirect: Guide",
    );
    let a = send(
        &c,
        URL,
        &c.contributor,
        &home_fields(&redirect, "Point Home at the guide", "submit"),
    )
    .await;
    assert!(a.status.is_redirection(), "{}", a.body());
    let other = HOME.replace("title: Home", "title: Home\nkind: category");
    let a = send(
        &c,
        URL,
        &c.contributor,
        &home_fields(&other, "Make it a category", "submit"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body().contains(&format!(
            "The lines kind at the top of the page {OWNED_SENTENCE}"
        )),
        "{}",
        a.body()
    );
}
