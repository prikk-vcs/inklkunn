//! The rules of a wiki are changed in its default view only, and the write API says so when a proposal is made, saved, sent and
//! revised (C-3e at proposing; the readiness review's P-6).

use serde_json::{Value, json};

use super::super::{Answer, At};
use super::{API, Cast, cast, files_of_home, id_of, post, principal, session, tag_header, tip};

/// A proposal of `files` in `view`, by the contributor, sent for review at once or kept as a draft.
async fn open_in(c: &Cast, view: &str, files: Value, submit: bool) -> Answer {
    let uri = format!("{API}/proposals");
    let headers = [session(&c.contributor), tip()];
    let body = json!({"summary": "x", "submit": submit, "view": view, "files": files});
    post(&c.w, &uri, &headers, body.to_string()).await
}

#[tokio::test]
async fn a_governance_file_in_another_view_is_refused_by_the_api_when_saved_sent_and_revised_as_the_acceptance_refuses_it()
 {
    // The same word the acceptance says (`governance-invalid`, the rule `outside-default-view`) is the answer when a draft is
    // made, saved, sent and revised.
    let c = cast();
    c.w.fake.add_view("fork");
    let file = |path: &str| json!([{"path": path, "text": "entry: none\n"}]);
    let refused = |a: &Answer, path: &str, what: &str| {
        assert_eq!(a.status, 422, "{what}: {}", a.text());
        assert_eq!(a.json().at("/reason"), "governance-invalid", "{what}");
        assert_eq!(a.json().at("/refusals/0/rule"), "outside-default-view");
        assert_eq!(a.json().at("/refusals/0/file"), path, "{what}");
        assert!(
            a.json()
                .at("/detail")
                .as_str()
                .is_some_and(|d| d.contains("another view") && !d.contains("accept again")),
            "{what}: {}",
            a.text()
        );
    };
    for path in ["governance/policy.yaml", "governance/constitution.yaml"] {
        // Made as a draft, and made already sent.
        for submit in [false, true] {
            let a = open_in(&c, "fork", file(path), submit).await;
            refused(&a, path, &format!("opened (submit: {submit})"));
            // The control: the default view takes it.
            let a = open_in(&c, "main", file(path), submit).await;
            assert_eq!(a.status, 201, "{path} in the default view: {}", a.text());
        }
    }
    // A page in another view is as it was.
    let a = open_in(&c, "fork", files_of_home("Second."), false).await;
    assert_eq!(a.status, 201, "{}", a.text());
    let draft = a.json();
    assert_eq!(draft.at("/view"), "fork");
    // The draft is saved with a governance file: refused.
    let saved = post(
        &c.w,
        &format!("{API}/proposals/{}/draft", id_of(&draft)),
        &[session(&c.contributor), tag_header(&draft)],
        json!({"summary": "x", "files": file("governance/policy.yaml")}).to_string(),
    )
    .await;
    refused(&saved, "governance/policy.yaml", "a draft saved");
    // A proposal held for review in another view is revised to name one: refused, and no revision is made.
    let held = open_in(&c, "fork", files_of_home("Third."), true).await;
    assert_eq!(held.status, 201, "{}", held.text());
    let held = held.json();
    let revised = post(
        &c.w,
        &format!("{API}/proposals/{}/revisions", id_of(&held)),
        &[session(&c.contributor), tag_header(&held)],
        json!({"summary": "x", "files": file("governance/constitution.yaml")}).to_string(),
    )
    .await;
    refused(&revised, "governance/constitution.yaml", "a revision");
    let revisions =
        c.w.services
            .proposals
            .revisions(&principal(&c.contributor), &c.w.wiki, &id_of(&held))
            .unwrap();
    assert_eq!(revisions.len(), 1);
}
