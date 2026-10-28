//! When something cannot be read: the checkpoint refusal and its flag, the other refusals, a page that is
//! there and cannot be shown, a derived store that is down, and the store-size figure the driver is hinted.

use std::sync::Arc;
use std::time::Duration;

use inklkunn_prikk::{SizeHints, StorageError, Unavailable};
use inklkunn_store::{IndexEntry, PageKey, StoreError, WikiStore};

use super::super::testing::{Env, Fake, World};
use super::*;

// ------------------------------------------------------------------------------------ refusals

fn not_recorded() -> StorageError {
    StorageError::Refused {
        verb: "extract",
        message: "precondition not met: no admitted snapshot within 63 blocks of this point".into(),
        kind: Some("no-checkpoint-in-reach".into()),
        reason: Some("not-recorded".into()),
    }
}

#[test]
fn an_extract_refused_for_want_of_the_checkpoint_is_could_not_be_read_now_and_flags_the_wiki() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    w.fake().fail_materialize(Some(not_recorded()));
    assert_eq!(
        w.store
            .get(&w.wiki.id)
            .unwrap()
            .checkpoint
            .refresh_requested_at,
        None
    );

    let e = w
        .service
        .page(&w.wiki, None, "Home", &Env::EN)
        .expect_err("refused");
    match &e {
        ReadError::CouldNotBeReadNow(Cause::CheckpointNotRecorded { refresh_requested }) => {
            assert!(refresh_requested);
        }
        other => panic!("{other:?}"),
    }
    // The operator's text names the reason and what is happening about it.
    let text = e.to_string();
    assert!(text.contains("checkpoint"), "{text}");
    assert!(text.contains("has been requested"), "{text}");
    // The wiki is flagged for the checkpoint refresh.
    let record = w.store.get(&w.wiki.id).unwrap();
    assert!(record.checkpoint.refresh_requested_at.is_some());
    assert!(
        record
            .checkpoint
            .needs_refresh(&inklkunn_store::PrikkVersion::parse("prikk 0.54.1").unwrap())
    );

    // Asking again within the hold does not spawn prikk again for every request.
    let before = w.calls().materialize;
    for _ in 0..5 {
        let e = w
            .service
            .page(&w.wiki, None, "Home", &Env::EN)
            .expect_err("refused");
        assert!(matches!(
            e,
            ReadError::CouldNotBeReadNow(Cause::CheckpointNotRecorded { .. })
        ));
    }
    assert_eq!(
        w.calls().materialize,
        before,
        "the refusal is remembered for the hold"
    );

    // The refresh ran (T7): the binary records it and tells the service; the next request builds the index.
    w.store
        .record_checkpoint_refresh(
            &w.wiki.id,
            &inklkunn_store::PrikkVersion::parse("prikk 0.54.1").unwrap(),
        )
        .unwrap();
    w.fake().fail_materialize(None);
    w.service.forget_wiki(&w.wiki);
    assert!(
        found(w.service.page(&w.wiki, None, "Home", &Env::EN))
            .html
            .as_str()
            .contains('x')
    );
    assert_eq!(
        w.store
            .get(&w.wiki.id)
            .unwrap()
            .checkpoint
            .refresh_requested_at,
        None
    );
}

#[test]
fn the_other_reasons_a_checkpoint_is_missing_do_not_ask_for_a_refresh_and_a_full_replay_is_never_asked()
 {
    for reason in ["signer-not-adopted", "no-checkpoint"] {
        let w = World::pages(&[("Home", &[], "x\n")]);
        w.fake().fail_materialize(Some(StorageError::Refused {
            verb: "extract",
            message: "refused".into(),
            kind: Some("no-checkpoint-in-reach".into()),
            reason: Some(reason.into()),
        }));
        let e = w
            .service
            .page(&w.wiki, None, "Home", &Env::EN)
            .expect_err("refused");
        match &e {
            ReadError::CouldNotBeReadNow(Cause::CheckpointUnavailable { reason: r }) => {
                assert_eq!(r, reason)
            }
            other => panic!("{other:?}"),
        }
        assert!(e.to_string().contains(reason), "{e}");
        assert_eq!(
            w.store
                .get(&w.wiki.id)
                .unwrap()
                .checkpoint
                .refresh_requested_at,
            None,
            "{reason}: a refresh would not help"
        );
    }
}

#[test]
fn other_refusals_and_failures_are_could_not_be_read_now_with_the_outcome_kept() {
    let w = World::pages(&[("Home", &[], "x\n")]);
    // A timeout is tried again by the next request; it is not remembered.
    w.fake().fail_materialize(Some(StorageError::Unavailable {
        verb: "extract",
        reason: Unavailable::TimedOut { seconds: 120 },
    }));
    for _ in 0..2 {
        let e = w
            .service
            .page(&w.wiki, None, "Home", &Env::EN)
            .expect_err("timed out");
        assert!(
            matches!(
                e,
                ReadError::CouldNotBeReadNow(Cause::Storage(StorageError::Unavailable { .. }))
            ),
            "{e:?}"
        );
    }
    assert_eq!(w.calls().materialize, 2, "asked again");
    w.fake().fail_materialize(None);
    assert!(w.service.page(&w.wiki, None, "Home", &Env::EN).is_ok());

    // The tip is larger than the index is built from.
    let big = World::pages_with(
        &[("Home", &[], &"x".repeat(2000))],
        ReadConfig {
            index_max_bytes: 1000,
            ..ReadConfig::default()
        },
    );
    let e = big
        .service
        .page(&big.wiki, None, "Home", &Env::EN)
        .expect_err("too large");
    assert!(
        matches!(e, ReadError::CouldNotBeReadNow(Cause::IndexBounds(_))),
        "{e:?}"
    );
    let few = World::pages_with(
        &[("A", &[], "x\n"), ("B", &[], "x\n"), ("C", &[], "x\n")],
        ReadConfig {
            index_max_files: 2,
            ..ReadConfig::default()
        },
    );
    let e = few
        .service
        .page(&few.wiki, None, "A", &Env::EN)
        .expect_err("too many");
    assert!(
        matches!(e, ReadError::CouldNotBeReadNow(Cause::IndexBounds(_))),
        "{e:?}"
    );

    // Prikk cannot be asked for the tip at all.
    let down = World::pages(&[("Home", &[], "x\n")]);
    down.fake().fail_views(Some(StorageError::Unavailable {
        verb: "branch",
        reason: Unavailable::Spawn("no such file".into()),
    }));
    assert!(matches!(
        down.service.page(&down.wiki, None, "Home", &Env::EN),
        Err(ReadError::CouldNotBeReadNow(Cause::Storage(_)))
    ));
}

#[test]
fn a_page_that_is_there_and_cannot_be_shown_says_why() {
    let w = World::pages(&[("Fine", &[], "x\n")]);
    w.fake()
        .set("pages/Anchored.md", "---\ntitle: &a Anchored\n---\nbody\n");
    w.fake().set_bytes("pages/Binary.md", &[0x66, 0xff, 0xfe]);
    w.service.forget_wiki(&w.wiki);
    let anchored = w
        .service
        .page(&w.wiki, None, "Anchored", &Env::EN)
        .expect_err("unreadable");
    assert!(
        matches!(
            anchored,
            ReadError::PageUnreadable(PageProblem::Frontmatter(_))
        ),
        "{anchored:?}"
    );
    let binary = w
        .service
        .page(&w.wiki, None, "Binary", &Env::EN)
        .expect_err("unreadable");
    assert!(
        matches!(binary, ReadError::PageUnreadable(PageProblem::NotUtf8)),
        "{binary:?}"
    );
    // Both are still in the index, findable to be repaired, and the others still read.
    let ix = w.service.title_index(&w.wiki, None).expect("index");
    assert_eq!(
        ix.entry("pages/Anchored.md")
            .and_then(|e| e.problem.as_deref()),
        Some("frontmatter")
    );
    assert_eq!(
        ix.entry("pages/Binary.md")
            .and_then(|e| e.problem.as_deref()),
        Some("not-utf8")
    );
    assert!(read_page(&w, "Fine").html.as_str().contains('x'));
}

// ------------------------------------------------------------------------------------ the store

/// A derived store that fails every call.
struct BrokenStore;

impl DerivedStore for BrokenStore {
    fn page(&self, _: &PageKey<'_>) -> inklkunn_store::Result<Option<DerivedPage>> {
        Err(StoreError::Backend("down".into()))
    }
    fn put_page(&self, _: &PageKey<'_>, _: &DerivedPage) -> inklkunn_store::Result<()> {
        Err(StoreError::Backend("down".into()))
    }
    fn title_index(&self, _: &WikiId, _: &str) -> inklkunn_store::Result<Option<Vec<IndexEntry>>> {
        Err(StoreError::Backend("down".into()))
    }
    fn put_title_index(
        &self,
        _: &WikiId,
        _: &str,
        _: &[IndexEntry],
        _: i64,
    ) -> inklkunn_store::Result<()> {
        Err(StoreError::Backend("down".into()))
    }
    fn retain_tips(&self, _: &WikiId, _: &[String]) -> inklkunn_store::Result<u64> {
        Err(StoreError::Backend("down".into()))
    }
    fn clear(&self, _: &WikiId) -> inklkunn_store::Result<u64> {
        Err(StoreError::Backend("down".into()))
    }
}

#[test]
fn a_derived_store_that_is_down_costs_speed_and_never_a_page() {
    let w = World::pages(&[("Home", &[], "See [[Guide]].\n"), ("Guide", &[], "x\n")]);
    let service = ReadService::new(
        w.counting.clone(),
        Arc::new(BrokenStore),
        w.store.clone(),
        w.layout.clone(),
        ReadConfig {
            tip_ttl: Duration::from_secs(3600),
            ..ReadConfig::default()
        },
        None,
    );
    let first = found(service.page(&w.wiki, None, "Home", &Env::EN));
    assert!(first.html.as_str().contains("href=\"/w/Guide\""));
    assert!(!first.cached);
    assert!(
        service.store_failures() > 0,
        "the failure is counted for the operator"
    );
    // Without a cache every view is a miss; the index lives in memory, so the extract still happens once.
    let second = found(service.page(&w.wiki, None, "Home", &Env::EN));
    assert!(!second.cached);
    assert_eq!(first.html, second.html);
    assert_eq!(w.calls().materialize, 1);
    // Clearing what cannot be cleared says so.
    assert!(matches!(
        service.clear_derived(&w.wiki),
        Err(ReadError::CouldNotBeReadNow(Cause::Store(_)))
    ));
}

// ------------------------------------------------------------------------------------ the store size

#[test]
fn the_stores_size_is_walked_once_per_tip_and_the_driver_is_hinted() {
    let dir = super::testing::TempDir::new("sizes");
    let counting = Arc::new(CountingStorage::new(Fake::new(&[(
        "pages/Home.md",
        "---\ntitle: Home\n---\nx\n",
    )])));
    let store = Arc::new(inklkunn_store::MemoryStore::new());
    let layout = inklkunn_store::RootLayout::new(dir.0.join("root"));
    let wiki = store
        .create(inklkunn_store::NewWiki::new(
            inklkunn_store::Slug::new("sizes").unwrap(),
            inklkunn_store::DisplayName::new("Sizes").unwrap(),
        ))
        .unwrap();
    let repo = layout.repo_dir(&wiki.id);
    std::fs::create_dir_all(repo.join(".prikk")).unwrap();
    std::fs::write(repo.join(".prikk/blob"), vec![0u8; 2048]).unwrap();
    let hints = SizeHints::new();
    let service = ReadService::new(
        counting.clone(),
        store.clone(),
        store.clone(),
        layout,
        ReadConfig {
            tip_ttl: Duration::ZERO,
            ..ReadConfig::default()
        },
        Some(hints.clone()),
    );

    assert_eq!(service.measured_store_bytes(&wiki), None);
    found(service.page(&wiki, None, "Home", &Env::EN));
    assert_eq!(service.measured_store_bytes(&wiki), Some(2048));
    assert_eq!(hints.get(&repo), Some(2048));

    // The store grows under the same tip, and a thousand requests do not walk it: the figure is the one
    // taken at the tip.
    std::fs::write(repo.join(".prikk/blob"), vec![0u8; 9000]).unwrap();
    for _ in 0..20 {
        found(service.page(&wiki, None, "Home", &Env::EN));
    }
    assert_eq!(hints.get(&repo), Some(2048));

    // A write seals and the tip moves: the next request walks, and the figure is the truth again.
    counting
        .inner()
        .set("pages/Home.md", "---\ntitle: Home\n---\ny\n");
    let view = found(service.page(&wiki, None, "Home", &Env::EN));
    assert!(view.html.as_str().contains('y'));
    assert_eq!(service.measured_store_bytes(&wiki), Some(9000));
    assert_eq!(hints.get(&repo), Some(9000));

    // The write path says so too, and the figure is dropped until the next request.
    service.forget_wiki(&wiki);
    assert_eq!(service.measured_store_bytes(&wiki), None);
    assert_eq!(hints.get(&repo), None);
}
