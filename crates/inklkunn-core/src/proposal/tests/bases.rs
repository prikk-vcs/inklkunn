//! The base of a proposal comes from the read service, that is from `Storage::views` (the view's RefState id)
//! and the one `log --limit 1` that names the block it belongs to, over the in-memory repository the read
//! tests use; the other proposal tests use a double.

use std::sync::Arc;
use std::time::Duration;

use inklkunn_prikk::StorageError;
use inklkunn_store::{Account, AccountId, AccountStore, DisplayName, Role, RoleGrant, RoleStore};

use super::support::edit;
use crate::authorize::{Authorizer, IdentityAuthority, Principal, StoreAuthz};
use crate::proposal::{BaseError, BaseSource, ProposalService, QuotaConfig};
use crate::read::ReadConfig;
use crate::read::testing::World;

fn world() -> World {
    World::pages_with(
        &[("Home", &[], "Text.\n")],
        ReadConfig {
            tip_ttl: Duration::from_secs(3600),
            ..ReadConfig::default()
        },
    )
}

#[test]
fn the_base_is_the_views_state_and_the_block_it_belongs_to() {
    let w = world();
    let (base, calls) = w.counted(|| w.service.base(&w.wiki, "heads/main").unwrap());
    assert_eq!(base.state_id, w.fake().state_id());
    assert_eq!(base.block_id, w.fake().block_id());
    assert_eq!(calls.views, 1, "the state comes from `branch list`");
    assert_eq!(calls.history, 1, "the block from the one `log --limit 1`");
    // The tip is held: asking again asks prikk nothing.
    let (again, calls) = w.counted(|| w.service.base(&w.wiki, "heads/main").unwrap());
    assert_eq!(again, base);
    assert_eq!((calls.views, calls.history), (0, 0));
    // The view moves and the hold ends (as the write path will end it after a seal): the base moves with it.
    w.fake()
        .set("pages/Home.md", "---\ntitle: Home\n---\n\nNew.\n");
    w.service.forget_wiki(&w.wiki);
    let moved = w.service.base(&w.wiki, "heads/main").unwrap();
    assert_ne!(moved, base);
    assert_eq!(moved.state_id, w.fake().state_id());
    assert_eq!(moved.block_id, w.fake().block_id());
}

#[test]
fn a_view_that_is_not_there_and_a_repository_that_cannot_answer_are_told_apart() {
    let w = world();
    assert_eq!(
        w.service.base(&w.wiki, "heads/nope"),
        Err(BaseError::NoSuchView("heads/nope".into()))
    );
    w.service.forget_wiki(&w.wiki);
    w.fake().fail_views(Some(StorageError::Driver {
        verb: "branch list",
        detail: "boom".into(),
    }));
    assert!(matches!(
        w.service.base(&w.wiki, "heads/main"),
        Err(BaseError::Unavailable(_))
    ));
}

#[test]
fn a_new_proposal_is_written_against_the_tip_the_read_service_names() {
    let w = world();
    let store = w.store.clone();
    let authority = IdentityAuthority::for_tests();
    let id = AccountId::new();
    store
        .add_account(&Account {
            id: id.clone(),
            display_name: DisplayName::new("Somebody").unwrap(),
            local: None,
            created_at: 0,
            disabled: false,
        })
        .unwrap();
    store
        .grant(RoleGrant {
            account: id.clone(),
            wiki: w.wiki.id.clone(),
            role: Role::Contributor,
            granted_by: None,
            granted_at: 0,
        })
        .unwrap();
    let reads = Arc::new(World::service_over(
        &w.counting,
        &w.store,
        &w.layout,
        ReadConfig::default(),
    ));
    let authz = StoreAuthz::new(store.clone(), store.clone());
    let service = ProposalService::new(
        store.clone(),
        store.clone(),
        store.clone(),
        Authorizer::new(
            Arc::new(authz),
            Arc::new(inklkunn_store::MemoryAudit::new()),
        ),
        reads,
        QuotaConfig::default(),
    );
    let me = Principal::session(&authority, id);
    let p = service
        .create(
            &me,
            &w.wiki,
            None,
            edit("From the form", "pages/Home.md", "New text.\n"),
        )
        .unwrap();
    let draft = service.draft(&me, &w.wiki, &p.id).unwrap();
    assert_eq!(draft.base.state_id, w.fake().state_id());
    assert_eq!(draft.base.block_id, w.fake().block_id());
    assert_eq!(p.view, "heads/main");
}
