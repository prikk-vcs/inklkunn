//! The pages over a real prikk (handoff 003 T4): a wiki's repository is the committed fixture of the driver's
//! tests, copied under the root layout, and the pages are read through the whole path — router, authorizer,
//! read service, driver, prikk, sandbox when bubblewrap works — and the verification page runs a real
//! `verify`. Self-skips with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "common/mod.rs"]
mod common;
#[path = "changes_feeds/xml.rs"]
mod xml;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use common::{TempDir, assemble};
use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage};
use inklkunn_store::{
    DisplayName, MemoryStore, NewWiki, RootLayout, Slug, VerificationState, WikiStore,
};
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site_with;
use tower::ServiceExt as _;

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

async fn get(site: &axum::Router, uri: &str) -> (u16, String) {
    let res = site
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::HOST, "localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status().as_u16();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn a_real_repository_is_browsed_and_verified_through_the_whole_path() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let mut policies = vec![Sandbox::Unconfined];
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        policies.push(Sandbox::default());
    } else {
        println!("skipping the confined policy: bubblewrap cannot confine a process here");
    }
    for sandbox in policies {
        let dir = TempDir::new("real");
        let store = Arc::new(MemoryStore::new());
        let wiki = store
            .create(NewWiki::new(
                Slug::new("fixture").unwrap(),
                DisplayName::new("Fixture").unwrap(),
            ))
            .unwrap();
        let layout = RootLayout::new(dir.0.join("root"));
        copy_dir(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../inklkunn-prikk/tests/fixtures/wiki-repo"),
            &layout.repo_dir(&wiki.id),
        );
        let storage: Arc<dyn Storage> = Arc::new(
            PrikkCli::open(
                "prikk",
                sandbox,
                Admission::new(1 << 30, 64 << 20, Duration::from_secs(5)),
            )
            .expect("prikk at the floor"),
        );
        let (services, _, log) = assemble(storage, &store, layout);
        let site = site_with(
            &WebConfig::new(
                "127.0.0.1:8080",
                "http://localhost:8080",
                "http://localhost:8081",
            )
            .unwrap(),
            Some(services),
        )
        .unwrap();

        // The front door, a page, a page in a hierarchy, a title that is not there, an attachment's page does
        // not exist as a page.
        let (status, _) = get(&site, "/fixture").await;
        assert_eq!(status, 303);
        let (status, home) = get(&site, "/fixture/Home").await;
        assert_eq!(status, 200, "{home}");
        assert!(home.contains("<h1>Home</h1>"), "{home}");
        assert!(home.contains("Not checked yet"));
        let (status, guide) = get(&site, "/fixture/Guides/Getting%20Started").await;
        assert_eq!(status, 200, "{guide}");
        let (status, missing) = get(&site, "/fixture/Nothing%20Here").await;
        assert_eq!(status, 404);
        assert!(missing.contains("This page does not exist yet"));
        assert!(log.lock().unwrap().is_empty(), "{:?}", log.lock().unwrap());

        // A real `verify` through the verification page: recorded, and the badge on the pages follows.
        let res = site
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/fixture/-/verification")
                    .header(header::HOST, "localhost:8080")
                    .header("sec-fetch-site", "same-origin")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), 200);
        let bytes = axum::body::to_bytes(res.into_body(), 1 << 22)
            .await
            .unwrap();
        let page = String::from_utf8_lossy(&bytes);
        assert!(page.contains("found nothing wrong"), "{page}");
        assert_eq!(
            store.get(&wiki.id).unwrap().verification.state,
            VerificationState::Verified
        );
        let (_, home) = get(&site, "/fixture/Home").await;
        assert!(home.contains(">Verified</a>"), "{home}");
    }
}

/// The same fixture, read the way T6 reads it: the real log and `show` for the changes and the feed, one
/// extract for search and for the link graph, and the stored file served byte for byte from the content origin.
#[tokio::test]
async fn changes_feeds_search_health_and_the_raw_source_over_a_real_repository() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let dir = TempDir::new("real-t6");
    let store = Arc::new(MemoryStore::new());
    let wiki = store
        .create(NewWiki::new(
            Slug::new("fixture").unwrap(),
            DisplayName::new("Fixture").unwrap(),
        ))
        .unwrap();
    let layout = RootLayout::new(dir.0.join("root"));
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../inklkunn-prikk/tests/fixtures/wiki-repo");
    copy_dir(&fixture, &layout.repo_dir(&wiki.id));
    let storage: Arc<dyn Storage> = Arc::new(
        PrikkCli::open(
            "prikk",
            Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_secs(5)),
        )
        .expect("prikk at the floor"),
    );
    let (services, _, log) = assemble(storage, &store, layout);
    let site = site_with(
        &WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
        Some(services),
    )
    .unwrap();
    let on = |host: &'static str, uri: &'static str| {
        let site = site.clone();
        async move {
            let res = site
                .oneshot(
                    Request::builder()
                        .uri(uri)
                        .header(header::HOST, host)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let (parts, body) = res.into_parts();
            (
                parts.status.as_u16(),
                parts.headers,
                axum::body::to_bytes(body, 1 << 22).await.unwrap().to_vec(),
            )
        }
    };
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();

    // The changes: the real log has two, the newest touched the Glossary and Home.
    let (status, _, body) = on("localhost:8080", "/fixture/-/changes").await;
    let page = text(&body);
    assert_eq!(status, 200, "{page}");
    assert!(page.contains("Create the fixture wiki"), "{page}");
    assert!(
        page.contains("<a href=\"/fixture/Glossary\">Glossary</a>"),
        "{page}"
    );
    assert!(
        page.contains("<a href=\"/fixture/Home\">Home</a>"),
        "{page}"
    );
    // The feed is Atom.
    let (status, _, body) = on("localhost:8080", "/fixture/-/changes.atom").await;
    assert_eq!(status, 200);
    assert_eq!(
        xml::atom_violations(&text(&body)),
        Vec::<String>::new(),
        "{}",
        text(&body)
    );
    // Search by a part of a title, and a list from the link graph.
    let (_, _, body) = on("localhost:8080", "/fixture/-/search?q=gloss").await;
    assert!(text(&body).contains("Glossary</a>"), "{}", text(&body));
    let (status, _, body) = on("localhost:8080", "/fixture/-/health/orphans").await;
    assert_eq!(status, 200, "{}", text(&body));
    let (status, _, body) = on("localhost:8080", "/fixture/-/links/Glossary").await;
    assert_eq!(status, 200);
    assert!(
        text(&body).contains("Welcome to the whole wiki."),
        "{}",
        text(&body)
    );
    // The stored file, byte for byte, only on the content origin.
    let (status, headers, body) = on("localhost:8081", "/fixture/-/raw/Home").await;
    assert_eq!(status, 200);
    assert_eq!(body, std::fs::read(fixture.join("pages/Home.md")).unwrap());
    assert_eq!(
        headers.get("content-type").unwrap(),
        "text/markdown; charset=UTF-8"
    );
    let (status, _, _) = on("localhost:8080", "/fixture/-/raw/Home").await;
    assert_eq!(status, 404);
    assert!(log.lock().unwrap().is_empty(), "{:?}", log.lock().unwrap());
}
