//! *Why is this here?* over a real prikk (handoff 005 K-T7): the committed fixture repository (two blocks: the wiki's
//! creation, and an edit of `Home` that adds `Glossary`) is read through the whole path: router, authorizer, read
//! service, driver, prikk, sandbox when bubblewrap works. The log, the `show`s and the `cat`s at blocks are real;
//! the paragraphs of `Home` are followed back from the page as it is to the block that created it. Self-skips
//! with a visible line when prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "common/mod.rs"]
mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use common::{TempDir, assemble};
use inklkunn_core::CountingStorage;
use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage};
use inklkunn_store::{DisplayName, MemoryStore, NewWiki, RootLayout, Slug, WikiStore};
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
async fn the_paragraphs_of_a_real_page_are_followed_back_through_a_real_history() {
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
        let dir = TempDir::new("real-why");
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
        let counting = Arc::new(CountingStorage::new(
            PrikkCli::open(
                "prikk",
                sandbox,
                Admission::new(1 << 30, 64 << 20, Duration::from_secs(5)),
            )
            .expect("prikk at the floor"),
        ));
        let storage: Arc<dyn Storage> = counting.clone();
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

        let (status, why) = get(&site, "/fixture/-/why/Home").await;
        assert_eq!(status, 200, "{why}");
        // The page was written by the first block and edited by the second: its paragraphs name both, and the
        // one the edit reworded says it was found by its text (the fixture's page has no anchors).
        assert!(why.contains("<h1>Why is this here?</h1>"), "{why}");
        assert!(why.contains("Change 2</a>"), "{why}");
        assert!(why.contains("Change 1</a>"), "{why}");
        assert!(
            why.contains(
                "Latest change found in <a href=\"/fixture/-/why/Home#c-2\">Change 2</a>."
            ),
            "{why}"
        );
        assert!(why.contains("Traced by matching the text."), "{why}");
        // Both changes are read to the start of the page: nothing is said to be cut off.
        assert!(!why.contains("Only the latest"), "{why}");
        assert!(!why.contains("could not be followed to its start"), "{why}");
        // The people are what the fixture's messages record, which is nothing in the service's form.
        assert!(
            why.contains("The record of this change does not say who made it."),
            "{why}"
        );
        // A page the second block added is written there and nowhere before.
        let (status, glossary) = get(&site, "/fixture/-/why/Glossary").await;
        assert_eq!(status, 200, "{glossary}");
        assert!(!glossary.contains("Change 1<"), "{glossary}");
        assert!(glossary.contains("Change 2</a>"), "{glossary}");
        // A title no page has is the page every missing title gets.
        let (status, none) = get(&site, "/fixture/-/why/Nowhere").await;
        assert_eq!(status, 404, "{none}");
        assert!(none.contains("This page does not exist yet"));

        assert!(log.lock().unwrap().is_empty(), "{:?}", log.lock().unwrap());
    }
}
