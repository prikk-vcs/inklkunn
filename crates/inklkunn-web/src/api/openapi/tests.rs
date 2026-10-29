#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_core::{Action, Guard, RouteTable};

use super::super::describe::{Answer, Doc};
use super::*;

/// Look into a JSON value by pointer; absent is `null` (indexing is denied by the workspace's lints).
trait At {
    fn at(&self, pointer: &str) -> &Value;
}

static NULL: Value = Value::Null;

impl At for Value {
    fn at(&self, pointer: &str) -> &Value {
        self.pointer(pointer).unwrap_or(&NULL)
    }
}

fn docs() -> Vec<&'static Doc> {
    endpoints::all().iter().filter_map(|e| e.doc).collect()
}

fn real_table() -> RouteTable {
    crate::routes::application_table().unwrap()
}

#[test]
fn the_document_is_built_from_the_live_registry_and_names_every_api_route() {
    let table = real_table();
    let document = document(&table, &docs()).unwrap();
    let paths = document.at("/paths").as_object().unwrap();
    let from_registry: Vec<String> = table
        .entries()
        .iter()
        .filter(|e| is_api_route(e.path) && !endpoints::NOT_ENDPOINTS.contains(&e.path))
        .map(|e| openapi_path(e.path))
        .collect();
    assert!(from_registry.len() >= 12, "{from_registry:?}");
    for path in &from_registry {
        assert!(
            paths.contains_key(path),
            "{path} is served and not described"
        );
    }
    for path in paths.keys() {
        assert!(
            from_registry.contains(path),
            "{path} is described and not served"
        );
    }
}

#[test]
fn a_route_without_a_description_is_drift() {
    let mut table = real_table();
    table
        .register(
            "/{slug}/-/api/v1/planted",
            &["GET"],
            Guard::Action(Action::Read),
        )
        .unwrap();
    assert_eq!(
        document(&table, &docs()).unwrap_err(),
        Drift::Undocumented("/{slug}/-/api/v1/planted".to_owned())
    );
}

#[test]
fn a_description_without_a_route_is_drift() {
    static PHANTOM: Doc = Doc {
        path: "/{slug}/-/api/v1/phantom",
        operation_id: "phantom",
        tag: "pages",
        summary: "A description that no route serves",
        description: "x",
        query: &[],
        answer: Answer::NotOffered,
        problems: &[],
    };
    let mut all = docs();
    all.push(&PHANTOM);
    assert_eq!(
        document(&real_table(), &all).unwrap_err(),
        Drift::NoRoute("/{slug}/-/api/v1/phantom".to_owned())
    );
}

#[test]
fn removing_a_description_is_drift_too() {
    let mut all = docs();
    let removed = all.remove(3);
    assert_eq!(
        document(&real_table(), &all).unwrap_err(),
        Drift::Undocumented(removed.path.to_owned())
    );
}

#[test]
fn an_api_route_guarded_by_another_action_is_refused() {
    let mut table = RouteTable::new();
    for entry in real_table().entries() {
        let guard = if entry.path == "/{slug}/-/api/v1/pages" {
            Guard::Action(Action::Propose)
        } else {
            entry.guard
        };
        table.register(entry.path, entry.methods, guard).unwrap();
    }
    assert_eq!(
        document(&table, &docs()).unwrap_err(),
        Drift::Unguarded("/{slug}/-/api/v1/pages".to_owned())
    );
}

#[test]
fn every_operation_carries_the_guard_the_registry_declares() {
    let table = real_table();
    let document = document(&table, &docs()).unwrap();
    for (path, item) in document.at("/paths").as_object().unwrap() {
        let op = &item.at("/get");
        let read = op.get("x-inklkunn-action").and_then(Value::as_str);
        let public = op.get("x-inklkunn-public").and_then(Value::as_str);
        match (read, public) {
            (Some("read"), None) => {}
            (None, Some(reason)) => {
                assert_eq!(path, super::super::OPENAPI_PATH);
                assert!(reason.len() >= 16);
            }
            other => panic!("{path}: {other:?}"),
        }
    }
}

#[test]
fn a_router_path_is_spelled_the_openapi_way() {
    assert_eq!(
        openapi_path("/{slug}/-/api/v1/pages/{*title}"),
        "/{slug}/-/api/v1/pages/{title}"
    );
    assert_eq!(
        path_params("/{slug}/-/api/v1/pages/{*title}"),
        ["slug", "title"]
    );
    assert_eq!(path_params("/-/api/v1/openapi.json"), Vec::<&str>::new());
}

#[test]
fn the_document_is_served_the_same_each_time_and_is_the_one_built_here() {
    let served = served().unwrap();
    let again = served;
    assert_eq!(served, again);
    let built = serde_json::to_vec(&document(&real_table(), &docs()).unwrap()).unwrap();
    assert_eq!(served, built.as_slice());
}

#[test]
fn the_page_size_bound_is_in_the_document() {
    let document = document(&real_table(), &docs()).unwrap();
    let limit = document
        .at("/paths/~1{slug}~1-~1api~1v1~1pages/get/parameters")
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p.at("/name") == "limit")
        .unwrap();
    assert_eq!(
        limit.at("/schema/maximum"),
        inklkunn_core::read::MAX_PAGE_SIZE
    );
    assert_eq!(limit.at("/schema/minimum"), 1);
    assert_eq!(
        document.at("/components/schemas/PageList/properties/items/maxItems"),
        inklkunn_core::read::MAX_PAGE_SIZE
    );
}

#[test]
fn the_problem_registry_is_in_the_document() {
    let document = document(&real_table(), &docs()).unwrap();
    let listed = document.at("/x-inklkunn-problems").as_array().unwrap();
    assert_eq!(listed.len(), Code::ALL.len());
    for (listed, code) in listed.iter().zip(Code::ALL) {
        assert_eq!(listed.at("/code"), code.slug());
        assert_eq!(listed.at("/type"), &code.type_uri().as_str());
        assert_eq!(listed.at("/status"), code.status().as_u16());
    }
}
