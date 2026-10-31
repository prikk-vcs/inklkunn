//! A write form's body is parsed after the checks (corrections P11): the handlers take the bytes, and nothing
//! reads them as fields until the form is known to come from this site, the wiki is looked up and the visitor holds
//! an account.

use std::path::Path;

use axum::http::StatusCode;

use super::{cast, home_fields};
use crate::common::send_body;

/// The lines of `source` that name axum's `Form` extractor or its rejection (which parse a body when a handler is
/// entered), by line number.
fn form_extractors(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let code = line.split("//").next().unwrap_or("");
            code.contains("extract::Form")
                || code.contains("Form<")
                || code.contains("FormRejection")
                || code.contains("Form(")
        })
        .map(|(n, _)| n + 1)
        .collect()
}

#[test]
fn no_write_form_takes_axums_form_extractor_which_parses_before_the_handler_begins() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = vec![src.join("watch.rs"), src.join("proposals.rs")];
    for entry in std::fs::read_dir(src.join("proposals")).unwrap().flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    assert!(files.len() > 10, "{files:?}");
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        assert_eq!(
            form_extractors(&source),
            Vec::<usize>::new(),
            "{} takes a body parsed at extraction; take the bytes (`urlencoded::Raw`) and parse them after the checks",
            file.display()
        );
    }
    // The check can fail.
    assert_eq!(
        form_extractors(
            "fn a() {}\n    form: Result<Form<HashMap<String, String>>, FormRejection>,\n"
        ),
        vec![2]
    );
    assert_eq!(
        form_extractors("use axum::extract::Form;\n// a Form<T> in a comment\n"),
        vec![1]
    );
}

/// A body that is not a form once decoded, and the headers of a form from this site.
const GARBAGE: &str = "a=%FF&pages=1";

async fn post_garbage(
    c: &super::Cast,
    account: Option<&inklkunn_store::AccountId>,
    site: &str,
) -> (StatusCode, String) {
    let a = send_body(
        &c.w,
        "POST",
        "/handbook/-/proposals/new",
        account,
        &[("sec-fetch-site", site)],
        GARBAGE.to_owned(),
    )
    .await;
    (a.status, a.body())
}

#[tokio::test]
async fn a_body_that_is_no_form_is_judged_after_the_site_the_wiki_and_the_account() {
    let c = cast();
    // From another site: refused as a form from another site, whatever the body is.
    let (status, body) = post_garbage(&c, Some(&c.contributor), "cross-site").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!body.contains("The form was not complete."), "{body}");
    // Signed out: told proposing needs an account, not that the form was bad.
    let (status, body) = post_garbage(&c, None, "same-origin").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!body.contains("The form was not complete."), "{body}");
    // A wiki that is not there is one page for every body.
    let a = send_body(
        &c.w,
        "POST",
        "/nowhere/-/proposals/new",
        Some(&c.contributor),
        &[("sec-fetch-site", "same-origin")],
        GARBAGE.to_owned(),
    )
    .await;
    let b = send_body(
        &c.w,
        "POST",
        "/nowhere/-/proposals/new",
        Some(&c.contributor),
        &[("sec-fetch-site", "same-origin")],
        "x=1".to_owned(),
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(a.status, b.status);
    // Only for someone who has passed all of that is the body read, and then it is judged in words.
    let (status, body) = post_garbage(&c, Some(&c.contributor), "same-origin").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.contains("The form was not complete."), "{body}");
    // …and a good body from the same person is taken.
    let good = home_fields(
        &super::HOME.replace("Second", "Second, and"),
        "Fix",
        "submit",
    );
    let a = super::send(&c, "/handbook/-/proposals/new", &c.contributor, &good).await;
    assert!(a.status.is_redirection(), "{}", a.body());
}
