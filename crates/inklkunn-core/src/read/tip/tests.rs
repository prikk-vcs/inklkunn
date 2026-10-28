#![allow(clippy::expect_used, clippy::unwrap_used)]
use std::time::Duration;

use inklkunn_prikk::{SizeHints, View, Views};
use inklkunn_store::WikiId;

use super::*;
use crate::read::testing::TempDir;

fn views(list: &[(&str, &str)]) -> Views {
    Views {
        schema_version: "branch-list-v1".to_owned(),
        views: list
            .iter()
            .map(|(name, state)| View {
                name: (*name).to_owned(),
                state_id: (*state).to_owned(),
                closed: false,
                current: false,
            })
            .collect(),
    }
}

/// A repository directory whose store holds `bytes`.
fn repo_with_store(tmp: &TempDir, bytes: usize) -> std::path::PathBuf {
    let repo = tmp.0.join("repo");
    std::fs::create_dir_all(repo.join(".prikk")).expect("store dir");
    std::fs::write(repo.join(".prikk/blob"), vec![0u8; bytes]).expect("blob");
    repo
}

#[test]
fn an_answer_is_held_for_the_hold_and_then_asked_for_again() {
    let tips = Tips::new(Duration::from_millis(40), None);
    let wiki = WikiId::new();
    let repo = std::path::Path::new("/nowhere");
    assert_eq!(tips.fresh(&wiki, "heads/main"), None, "nothing is held yet");
    tips.hold(
        &wiki,
        repo,
        &views(&[("heads/main", "s1"), ("heads/other", "s2")]),
    );
    assert_eq!(
        tips.fresh(&wiki, "heads/main"),
        Some(Some(Tip {
            view: "heads/main".into(),
            state_id: "s1".into()
        }))
    );
    // One answer serves every view; a view it does not list is known to be absent while it is held.
    assert_eq!(
        tips.fresh(&wiki, "heads/other")
            .map(|t| t.map(|t| t.state_id)),
        Some(Some("s2".to_owned()))
    );
    assert_eq!(tips.fresh(&wiki, "heads/none"), Some(None));
    // Another wiki has nothing held.
    assert_eq!(tips.fresh(&WikiId::new(), "heads/main"), None);
    std::thread::sleep(Duration::from_millis(120));
    assert_eq!(tips.fresh(&wiki, "heads/main"), None, "the hold is over");
    // Even then the last answer names the tips every view has (for pruning the cache).
    assert_eq!(tips.current_tips(&wiki), ["s1", "s2"]);
}

#[test]
fn a_closed_view_is_not_a_view() {
    let tips = Tips::new(Duration::from_secs(60), None);
    let wiki = WikiId::new();
    let mut v = views(&[("heads/main", "s1"), ("heads/old", "s0")]);
    v.views.get_mut(1).expect("two views").closed = true;
    tips.hold(&wiki, std::path::Path::new("/nowhere"), &v);
    assert_eq!(tips.fresh(&wiki, "heads/old"), Some(None));
    assert_eq!(tips.current_tips(&wiki), ["s1"]);
}

#[test]
fn the_store_is_walked_once_per_change_of_the_views_and_the_driver_is_told() {
    let tmp = TempDir::new("tip-size");
    let repo = repo_with_store(&tmp, 1000);
    let hints = SizeHints::new();
    let tips = Tips::new(Duration::from_secs(60), Some(hints.clone()));
    let wiki = WikiId::new();

    // First sight of the views: the walk is the truth.
    tips.hold(&wiki, &repo, &views(&[("heads/main", "s1")]));
    assert_eq!(tips.store_bytes(&wiki), Some(1000));
    assert_eq!(hints.get(&repo), Some(1000));

    // The store grows under the same views (a change nobody announced): the figure stays, because the
    // views have not moved. This is the point of the hint: no walk between writes.
    std::fs::write(repo.join(".prikk/blob"), vec![0u8; 5000]).expect("grow");
    tips.hold(&wiki, &repo, &views(&[("heads/main", "s1")]));
    assert_eq!(tips.store_bytes(&wiki), Some(1000));
    assert_eq!(hints.get(&repo), Some(1000));

    // A view moves: the figure is walked again and is true.
    tips.hold(&wiki, &repo, &views(&[("heads/main", "s2")]));
    assert_eq!(tips.store_bytes(&wiki), Some(5000));
    assert_eq!(hints.get(&repo), Some(5000));

    // A view that is not the one read moves too: the store is shared, so the figure is walked again.
    std::fs::write(repo.join(".prikk/blob"), vec![0u8; 7000]).expect("grow");
    tips.hold(
        &wiki,
        &repo,
        &views(&[("heads/main", "s2"), ("heads/other", "o1")]),
    );
    assert_eq!(tips.store_bytes(&wiki), Some(7000));
    assert_eq!(hints.get(&repo), Some(7000));

    // Forgetting drops the answer, the figure and the driver's hint; the next sight walks.
    tips.forget(&wiki, &repo);
    assert_eq!(tips.fresh(&wiki, "heads/main"), None);
    assert_eq!(tips.store_bytes(&wiki), None);
    assert_eq!(hints.get(&repo), None);
    std::fs::write(repo.join(".prikk/blob"), vec![0u8; 100]).expect("shrink");
    tips.hold(
        &wiki,
        &repo,
        &views(&[("heads/main", "s2"), ("heads/other", "o1")]),
    );
    assert_eq!(hints.get(&repo), Some(100));
}

#[test]
fn a_hint_that_expired_is_walked_again_even_though_the_views_did_not_move() {
    let tmp = TempDir::new("tip-expiry");
    let repo = repo_with_store(&tmp, 10);
    let hints = SizeHints::with_max_age(Duration::from_millis(30));
    let tips = Tips::new(Duration::from_secs(60), Some(hints.clone()));
    let wiki = WikiId::new();
    let same = views(&[("heads/main", "s1")]);
    tips.hold(&wiki, &repo, &same);
    assert_eq!(hints.get(&repo), Some(10));
    std::fs::write(repo.join(".prikk/blob"), vec![0u8; 20]).expect("grow");
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(hints.get(&repo), None, "the driver no longer trusts it");
    tips.hold(&wiki, &repo, &same);
    assert_eq!(
        hints.get(&repo),
        Some(20),
        "so the service walks and hands it a fresh one"
    );
}

#[test]
fn a_repository_that_cannot_be_measured_gets_no_hint() {
    let tmp = TempDir::new("tip-missing");
    let repo = tmp.0.join("not-a-repo");
    let hints = SizeHints::new();
    hints.set(&repo, 99);
    let tips = Tips::new(Duration::from_secs(60), Some(hints.clone()));
    let wiki = WikiId::new();
    tips.hold(&wiki, &repo, &views(&[("heads/main", "s1")]));
    assert_eq!(
        hints.get(&repo),
        None,
        "the driver walks and says what is wrong itself"
    );
    assert_eq!(tips.store_bytes(&wiki), None);
}

#[test]
fn without_hints_nothing_is_walked() {
    let tmp = TempDir::new("tip-nohints");
    let repo = repo_with_store(&tmp, 10);
    let tips = Tips::new(Duration::from_secs(60), None);
    let wiki = WikiId::new();
    tips.hold(&wiki, &repo, &views(&[("heads/main", "s1")]));
    assert_eq!(tips.store_bytes(&wiki), None);
}
