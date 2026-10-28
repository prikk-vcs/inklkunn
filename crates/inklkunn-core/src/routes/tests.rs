#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

const REASON: &str = "the page stylesheet: static, the same for everyone, no wiki data";

fn table() -> RouteTable {
    let mut t = RouteTable::new();
    t.register("/{slug}/{*title}", &["GET"], Guard::Action(Action::Read))
        .unwrap();
    t.register(
        "/-/static/app.css",
        &["GET"],
        Guard::Public { reason: REASON },
    )
    .unwrap();
    t
}

#[test]
fn a_valid_table_keeps_its_order_and_is_searchable() {
    let t = table();
    assert_eq!(t.entries().len(), 2);
    assert_eq!(
        t.entries().first().map(|e| e.path),
        Some("/{slug}/{*title}")
    );
    assert_eq!(t.find("/-/static/app.css").unwrap().methods, ["GET"]);
    assert!(t.find("/nope").is_none());
    let guarded: Vec<_> = t.guarded().map(|(e, a)| (e.path, a)).collect();
    assert_eq!(guarded, [("/{slug}/{*title}", Action::Read)]);
}

#[test]
fn a_public_entry_without_a_real_reason_is_refused() {
    let mut t = RouteTable::new();
    for thin in ["", "   ", "static", "no wiki data", "fifteen chars!!"] {
        assert_eq!(
            t.register("/x", &["GET"], Guard::Public { reason: thin }),
            Err(RouteError::ThinReason),
            "{thin:?}"
        );
    }
    assert!(
        t.entries().is_empty(),
        "a refused entry leaves nothing behind"
    );
    t.register(
        "/x",
        &["GET"],
        Guard::Public {
            reason: "sixteen chars ok",
        },
    )
    .unwrap();
}

#[test]
fn malformed_entries_are_refused_and_leave_nothing() {
    let mut t = table();
    let before = t.clone();
    for (path, methods, want) in [
        ("", &["GET"][..], RouteError::BadPath),
        ("x", &["GET"][..], RouteError::BadPath),
        ("/a b", &["GET"][..], RouteError::BadPath),
        ("/a\n", &["GET"][..], RouteError::BadPath),
        ("/a//b", &["GET"][..], RouteError::BadPath),
        ("/ok", &[][..], RouteError::NoMethods),
        ("/ok", &["get"][..], RouteError::BadMethod),
        ("/ok", &["GET", "GET"][..], RouteError::BadMethod),
        ("/ok", &[""][..], RouteError::BadMethod),
        ("/ok", &["G ET"][..], RouteError::BadMethod),
    ] {
        assert_eq!(
            t.register(path, methods, Guard::Action(Action::Read)),
            Err(want),
            "{path:?} {methods:?}"
        );
    }
    assert_eq!(t, before);
}

#[test]
fn a_path_is_registered_once() {
    let mut t = table();
    assert_eq!(
        t.register(
            "/{slug}/{*title}",
            &["POST"],
            Guard::Action(Action::Propose)
        ),
        Err(RouteError::Duplicate)
    );
    assert_eq!(t.entries().len(), 2);
}

#[test]
fn errors_say_what_to_do() {
    for e in [
        RouteError::BadPath,
        RouteError::NoMethods,
        RouteError::BadMethod,
        RouteError::ThinReason,
        RouteError::Duplicate,
    ] {
        assert!(!e.to_string().is_empty());
    }
}
