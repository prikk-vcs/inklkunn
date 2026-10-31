#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::{Action, Guard};

use super::describe::Answer;
use super::*;

#[test]
fn the_addresses_of_the_api_are_told_from_pages() {
    for api in [
        "/-/api/v1/openapi.json",
        "/-/api/v2/anything",
        "/handbook/-/api/v1/pages",
        "/handbook/-/api/v1/pages/Home",
        "/handbook/-/api/v9/x",
        "/handbook/-/api/",
    ] {
        assert!(is_api_path(api), "{api}");
    }
    for page in [
        "/",
        "/-/health",
        "/-/lang",
        "/handbook",
        "/handbook/Home",
        "/handbook/-/about",
        "/handbook/api/v1/pages",
        "/handbook/-/apix/v1",
        "/-/apix",
        "//-/api/v1",
    ] {
        assert!(!is_api_path(page), "{page}");
    }
}

#[test]
fn the_link_to_the_description_names_the_documented_path_and_media_type() {
    assert_eq!(
        reply::SERVICE_DESC,
        format!("<{OPENAPI_PATH}>; rel=\"service-desc\"; type=\"{OPENAPI_MEDIA_TYPE}\"")
    );
    assert!(OPENAPI_PATH.starts_with(SITE_PREFIX));
    assert!(WIKI_PREFIX.starts_with("/{slug}/-/"));
}

#[test]
fn every_endpoint_declares_a_guard_and_a_documented_one_names_its_path() {
    let all = endpoints();
    assert!(all.len() >= 12);
    for endpoint in &all {
        match (endpoint.guard, endpoint.doc.and_then(|d| d.write)) {
            // A read is guarded by `read`; a write by the action its description names, and takes `POST`.
            (Guard::Action(action), None) => {
                assert_eq!(action, Action::Read, "{}", endpoint.path);
                assert_eq!(endpoint.methods, ["GET", "HEAD"], "{}", endpoint.path);
            }
            (Guard::Action(action), Some(write)) => {
                assert_eq!(action, write.action, "{}", endpoint.path);
                assert_ne!(action, Action::Read, "{}", endpoint.path);
                assert_eq!(endpoint.methods, ["POST"], "{}", endpoint.path);
            }
            (Guard::Public { reason }, None) => assert!(reason.len() >= 16, "{}", endpoint.path),
            (Guard::Public { .. }, Some(_)) => {
                panic!("{}: a write cannot be public", endpoint.path)
            }
        }
        if let Some(doc) = endpoint.doc {
            assert_eq!(doc.path, endpoint.path);
        }
    }
    // Only the description and the two catch-alls are public; everything that reads a wiki is `Read`.
    let public: Vec<&str> = all
        .iter()
        .filter(|e| matches!(e.guard, Guard::Public { .. }))
        .map(|e| e.path)
        .collect();
    assert_eq!(
        public,
        [OPENAPI_PATH, "/{slug}/-/api/{*rest}", "/-/api/{*rest}"]
    );
}

#[test]
fn operation_ids_are_unique_and_every_path_begins_under_a_reserved_prefix() {
    let docs: Vec<_> = endpoints().into_iter().filter_map(|e| e.doc).collect();
    let mut ids: Vec<&str> = docs.iter().map(|d| d.operation_id).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), before, "operation ids repeat");
    for doc in &docs {
        assert!(
            doc.path.starts_with(WIKI_PREFIX) || doc.path.starts_with(SITE_PREFIX),
            "{}",
            doc.path
        );
    }
}

#[test]
fn the_documented_bounds_are_the_enforced_ones() {
    let limit = describe::PAGES
        .query
        .iter()
        .find(|p| p.name == "limit")
        .expect("a limit parameter");
    assert_eq!(
        limit.kind,
        describe::Kind::Int {
            min: 1,
            max: i64::try_from(MAX_PAGE_SIZE).unwrap()
        }
    );
    assert!(
        limit.description.contains(&format!("1 to {MAX_PAGE_SIZE}")),
        "{}",
        limit.description
    );
    assert!(
        limit.description.contains(&format!(
            "default is {}",
            inklkunn_core::read::DEFAULT_PAGE_SIZE
        )),
        "{}",
        limit.description
    );
}

#[test]
fn every_endpoint_answers_a_schema_the_document_defines_and_every_problem_it_lists_is_registered() {
    let document: serde_json::Value = serde_json::from_slice(openapi::served().unwrap()).unwrap();
    for endpoint in endpoints() {
        let Some(doc) = endpoint.doc else { continue };
        if let Answer::Schema(name) = doc.answer {
            assert!(
                document
                    .pointer(&format!("/components/schemas/{name}"))
                    .is_some(),
                "{}: the schema {name} is not defined",
                doc.path
            );
        }
        for code in doc.problems {
            assert!(problem::Code::ALL.contains(code), "{}", doc.path);
        }
        // No endpoint is left as a stub: what each one is, it is.
        assert!(
            !doc.description.contains("Not offered by this build"),
            "{}",
            doc.path
        );
    }
}

#[test]
fn a_choice_parameter_documents_its_words_as_an_enum() {
    let document: serde_json::Value = serde_json::from_slice(openapi::served().unwrap()).unwrap();
    let parameters = document
        .pointer("/paths/~1{slug}~1-~1api~1v1~1health/get/parameters")
        .and_then(serde_json::Value::as_array)
        .unwrap();
    let list = parameters
        .iter()
        .find(|p| p.get("name") == Some(&serde_json::json!("list")))
        .unwrap();
    assert_eq!(list.get("required"), Some(&serde_json::json!(true)));
    let words: Vec<&str> = list
        .pointer("/schema/enum")
        .and_then(serde_json::Value::as_array)
        .unwrap()
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert_eq!(
        words,
        [
            "wanted",
            "orphans",
            "dead-ends",
            "redirects",
            "wants-help",
            "stale"
        ]
    );
    for word in &words {
        assert!(
            inklkunn_core::read::HealthList::from_slug(word).is_some(),
            "{word}"
        );
    }
}
