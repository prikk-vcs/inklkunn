//! A page's past over a real prikk (handoff 003 T5): the committed fixture repository (two blocks: the wiki's
//! creation, and an edit of `Home` that adds `Glossary`) is read through the whole path — router, authorizer,
//! read service, driver, prikk, sandbox when bubblewrap works. The history comes from a real `log` and
//! `show`, the difference from a real `diff --path`, the permalink from a real `cat` at a block. Self-skips
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

/// The fixture's two blocks, as `prikk log` prints them.
const CREATED: &str = "c31bb25e4edcb645545aa6f762da352ad4133158ccafc21a70c0d5ea42f9e518";
const EDITED: &str = "7f4208f0ac2bc942c6bcbc1d329bedc9220260cd14be826c08c2c8bb17ee7285";

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
async fn a_real_repositorys_history_changes_differences_and_permalinks_are_read_through_the_whole_path()
 {
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
        let dir = TempDir::new("real-past");
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

        // The history of `Home`: both blocks touched it, newest first. The fixture's messages are not in the
        // service's form, so no one is named for them.
        let (status, history) = get(&site, "/fixture/-/history/Home").await;
        assert_eq!(status, 200, "{history}");
        let (second, first) = (history.find("Change 2"), history.find("Change 1"));
        assert!(second < first && second.is_some(), "{history}");
        assert!(history.contains("The page was edited"), "{history}");
        assert!(history.contains("The page was created"), "{history}");
        assert!(history.contains(": Create the fixture wiki"), "{history}");
        assert!(
            history.contains("The record of this change does not say who made it."),
            "{history}"
        );
        assert!(!history.contains("Signed"));
        // `Glossary` came with the second block only.
        let (_, glossary) = get(&site, "/fixture/-/history/Glossary").await;
        assert!(glossary.contains("Change 2"));
        assert!(!glossary.contains("Change 1"), "{glossary}");

        // The change: signed into the history, and what it did.
        let (status, change) = get(&site, &format!("/fixture/-/change/{EDITED}")).await;
        assert_eq!(status, 200, "{change}");
        assert!(change.contains("Signed into the history"), "{change}");
        assert!(change.contains("Edited the page <a href=\"/fixture/Home\">Home</a>"));
        assert!(change.contains("Added the page <a href=\"/fixture/Glossary\">Glossary</a>"));
        assert!(!change.contains("on behalf of"));

        // The difference: exactly one real `diff`, restricted to the page.
        let before = counting.calls();
        let (status, diff) = get(&site, &format!("/fixture/-/diff/{CREATED}/{EDITED}/Home")).await;
        assert_eq!(status, 200, "{diff}");
        assert_eq!(counting.calls().since(&before).diff, 1, "exactly one diff");
        assert!(
            diff.contains("<ins><span class=\"visually-hidden\">Added: </span>"),
            "{diff}"
        );
        assert!(diff.contains("<del><span class=\"visually-hidden\">Removed: </span>"));
        assert!(diff.contains("1 line added"), "{diff}");
        assert!(diff.contains("1 line removed"));
        assert!(
            !diff.contains("pages/"),
            "no storage path, and no other page of the change"
        );

        // The permalink: the page as the first block had it.
        let (status, at) = get(&site, &format!("/fixture/-/at/{CREATED}/Home")).await;
        assert_eq!(status, 200, "{at}");
        assert!(at.contains("Welcome. See"), "{at}");
        assert!(!at.contains("Welcome to the whole wiki"));
        assert!(at.contains("class=\"at-banner\""));
        assert!(at.contains("<a href=\"/fixture/Home\">the current version of this page</a>"));
        // And before the page existed.
        let (status, early) = get(&site, &format!("/fixture/-/at/{CREATED}/Glossary")).await;
        assert_eq!(status, 404, "{early}");
        assert!(early.contains("The page did not exist then"));

        // A block prikk does not have is no such change, and a malformed one never reaches prikk.
        let (status, none) = get(
            &site,
            &format!("/fixture/-/diff/{CREATED}/{}/Home", "ab".repeat(32)),
        )
        .await;
        assert_eq!(status, 404, "{none}");
        assert!(none.contains("There is no such change"));
        let (status, _) = get(&site, "/fixture/-/change/--all").await;
        assert_eq!(status, 404);

        assert!(log.lock().unwrap().is_empty(), "{:?}", log.lock().unwrap());
    }
}
