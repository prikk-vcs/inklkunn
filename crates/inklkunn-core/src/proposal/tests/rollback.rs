//! *Propose a rollback* through the service (handoff 004 T6; RFC 004 D-8): the proposal is a rollback from
//! its creation and names the block it undoes, it is written against that block, only someone who may propose
//! opens one (before the repository is asked anything), and a draft rollback is its author's alone.

use inklkunn_prikk::storage::PathRef;
use inklkunn_prikk::{Change, Operation, Patch};
use inklkunn_store::{FileChange, ProposalFilter, ProposalState, WikiRecord};

use super::support::{Env, Kind, on_both};
use crate::authorize::Principal;
use crate::proposal::ProposalError;
use crate::proposal::rollback::{RollbackError, RollbackSource, rollback_edit};
use crate::read::testing::{World, block_id, page, state_id};
use crate::read::{NoRollback, ReadConfig, ReadError, RollbackOutcome, RollbackPlan};

/// A repository whose second block changed `Home`: the tip, and the block to undo.
fn world() -> (World, u64) {
    let w = World::pages_with(
        &[("Home", &[], "Home v1.\n")],
        ReadConfig {
            tip_ttl: std::time::Duration::ZERO,
            ..ReadConfig::default()
        },
    );
    w.fake().say(1, "Create the wiki");
    w.fake()
        .set("pages/Home.md", &page(&["title: Home"], "Home, changed.\n"));
    w.fake().say(2, "Change the home page");
    w.fake().show(
        2,
        Ok(Change {
            schema_version: "show-report-v1".to_owned(),
            patches: vec![Patch {
                patch_id: "p".repeat(64),
                queued: false,
                operations: vec![Operation {
                    kind: "edit-text".to_owned(),
                    paths: vec![PathRef {
                        path: Some("pages/Home.md".to_owned()),
                        unresolved_node_id: None,
                    }],
                    content: Default::default(),
                }],
            }],
        }),
    );
    (w, 2)
}

fn plan_of(w: &World, wiki: &WikiRecord, v: u64) -> RollbackPlan {
    match w.service.rollback_plan(wiki, None, &block_id(v)).unwrap() {
        RollbackOutcome::Plan(plan) => *plan,
        other => panic!("{other:?}"),
    }
}

fn nothing_listed(env: &Env) -> bool {
    env.service
        .list(
            &env.maintainer.principal,
            &env.wiki,
            &ProposalFilter::default(),
        )
        .unwrap()
        .is_empty()
}

#[test]
fn a_rollback_proposal_is_a_rollback_from_its_creation_and_names_the_block_it_undoes() {
    on_both(|env| {
        let (w, v) = world();
        let held = env
            .service
            .propose_rollback(
                &env.author.principal,
                &env.wiki,
                None,
                &w.service,
                &block_id(v),
            )
            .expect("a rollback proposal");
        assert_eq!(held.state, ProposalState::HeldForReview);
        assert_eq!(held.summary, "Undo: Change the home page");
        assert_eq!(held.author, env.author.id);

        // It is a rollback, and names the block: for the proposer and for a reviewer alike.
        for who in [&env.author, &env.reviewer, &env.maintainer] {
            assert_eq!(
                env.service
                    .rollback_of(&who.principal, &env.wiki, &held.id)
                    .unwrap()
                    .as_deref(),
                Some(block_id(v).as_str())
            );
        }
        // The revision restores the text before the change and is written against the undone block.
        let revision = env
            .service
            .revisions(&env.author.principal, &env.wiki, &held.id)
            .unwrap()
            .remove(0);
        assert_eq!(
            revision.content.files,
            vec![FileChange {
                path: "pages/Home.md".to_owned(),
                text: Some(page(&["title: Home"], "Home v1.\n")),
            }]
        );
        assert_eq!(revision.content.base.block_id, block_id(v));
        assert_eq!(revision.content.base.state_id, state_id(v));
        assert!(!revision.content.minor && !revision.content.automated);

        // An ordinary proposal is not one.
        let plain = env.held(&env.author, "Fix a typo", "pages/Home.md");
        assert_eq!(
            env.service
                .rollback_of(&env.author.principal, &env.wiki, &plain.id)
                .unwrap(),
            None
        );
    });
}

#[test]
fn only_someone_who_may_propose_opens_a_rollback_and_before_the_repository_is_asked() {
    on_both(|env| {
        let (w, v) = world();
        for who in [&env.observer, &env.outsider] {
            let (refused, calls) = w.counted(|| {
                env.service.propose_rollback(
                    &who.principal,
                    &env.wiki,
                    None,
                    &w.service,
                    &block_id(v),
                )
            });
            assert!(matches!(
                refused,
                Err(RollbackError::Proposal(ProposalError::NotAllowed))
            ));
            assert_eq!(
                (calls.views, calls.history, calls.change, calls.read),
                (0, 0, 0, 0)
            );
        }
        let anonymous = env.service.propose_rollback(
            &Principal::Anonymous,
            &env.wiki,
            None,
            &w.service,
            &block_id(v),
        );
        assert!(matches!(
            anonymous,
            Err(RollbackError::Proposal(ProposalError::NotAllowed))
        ));
        assert!(nothing_listed(env), "nothing was opened");
    });
}

#[test]
fn a_change_that_cannot_be_undone_opens_nothing_and_says_why() {
    on_both(|env| {
        let (w, _) = world();
        for (block, why) in [
            (block_id(1), NoRollback::FirstChange),
            (block_id(77), NoRollback::NoSuchChange),
        ] {
            let refused = env.service.propose_rollback(
                &env.author.principal,
                &env.wiki,
                None,
                &w.service,
                &block,
            );
            assert!(matches!(refused, Err(RollbackError::No(found)) if found == why));
        }
        assert!(nothing_listed(env));
    });
}

/// A source that fails: the repository cannot be asked.
struct Down;

impl RollbackSource for Down {
    fn plan(&self, _: &WikiRecord, _: Option<&str>, _: &str) -> Result<RollbackOutcome, ReadError> {
        Err(ReadError::NoSuchView("heads/gone".to_owned()))
    }
}

#[test]
fn a_repository_that_cannot_be_asked_opens_nothing() {
    on_both(|env| {
        let refused = env.service.propose_rollback(
            &env.author.principal,
            &env.wiki,
            None,
            &Down,
            &block_id(2),
        );
        assert!(matches!(refused, Err(RollbackError::Unavailable(_))));
        assert!(nothing_listed(env));
    });
}

#[test]
fn a_draft_rollback_is_its_authors_alone_and_still_names_what_it_undoes() {
    on_both(|env| {
        let (w, v) = world();
        let plan = plan_of(&w, &env.wiki, v);
        let draft = env
            .service
            .create_rollback(&env.author.principal, &env.wiki, None, &plan)
            .unwrap();
        assert_eq!(draft.state, ProposalState::Draft);
        assert_eq!(
            env.service
                .rollback_of(&env.author.principal, &env.wiki, &draft.id)
                .unwrap()
                .as_deref(),
            Some(block_id(v).as_str())
        );
        assert_eq!(
            env.service
                .rollback_of(&env.maintainer.principal, &env.wiki, &draft.id),
            Err(ProposalError::NotFound)
        );
        // The edit that made it is the plan's, field for field.
        let edit = rollback_edit(&plan);
        assert_eq!(edit.base, Some(plan.base.clone()));
        assert_eq!(edit.files, plan.files);
        assert!(edit.renames.is_empty() && edit.sources.is_empty());
    });
}

#[test]
fn the_pages_a_rollback_writes_are_checked_against_the_proposers_rights() {
    let env = Env::new(Kind::Memory);
    let (w, v) = world();
    let plan = plan_of(&w, &env.wiki, v);
    assert_eq!(
        env.service
            .create_rollback(&env.observer.principal, &env.wiki, None, &plan)
            .err(),
        Some(ProposalError::NotAllowed)
    );
    assert!(nothing_listed(&env));
}
