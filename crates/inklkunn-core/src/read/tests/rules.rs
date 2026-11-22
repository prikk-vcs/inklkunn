//! The rules of a wiki read from the tip of its default view (RFC 006 D-5; G-T1 part C): derived, held by the tip, stored by
//! `(wiki, tip)`, rebuilt after the derived store is cleared, read from the default view only, and failing closed.

use std::sync::Arc;

use inklkunn_store::{DerivedStore, GovernanceSnapshotStore};

use super::super::testing::{World, state_id};
use super::*;
use crate::governance::{Entry, Limit, PlatformLimits, Refusal, Rules};
use crate::read::rules::WALK_BLOCKS;
use inklkunn_store::{Role, WikiRecord};

const POLICY: &str = "entry: contributor\nacceptance:\n  approvals: 2\n";
const CONSTITUTION: &str = "policy_change:\n  rung: admin\n  approvals: 1\n";

fn config() -> crate::read::ReadConfig {
    crate::read::ReadConfig {
        tip_ttl: std::time::Duration::from_secs(3600),
        ..crate::read::ReadConfig::default()
    }
}

fn world() -> World {
    let w = World::pages(&[("Home", &[], "Hello.\n")]);
    w.fake().derive_changes();
    w
}

/// Write the files in one block and let the service see the new tip, as the write path does after a seal.
fn write(w: &World, files: &[(&str, &str)]) {
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(p, t)| ((*p).to_owned(), (*t).to_owned()))
        .collect();
    w.fake().set_files(&files);
    w.service.forget_wiki(&w.wiki);
}

fn rules(w: &World) -> Arc<Rules> {
    w.service.rules_of(&w.wiki).expect("the rules")
}

#[test]
fn a_wiki_with_no_governance_file_has_the_defaults_and_pays_for_no_walk() {
    let w = world();
    let (rules, calls) = w.counted(|| rules(&w));
    assert_eq!(*rules, Rules::defaults());
    assert_eq!(rules.block(), None);
    assert_eq!(rules.short_block(), "defaults");
    // The tip (list and log), one listing of `governance/`: no file is read and no block is looked at.
    assert_eq!(
        (calls.tree, calls.read, calls.history, calls.change),
        (1, 0, 1, 0),
        "{calls:?}"
    );
    // The same tip again costs nothing.
    let (again, calls) = w.counted(|| self::rules(&w));
    assert!(Arc::ptr_eq(&rules, &again));
    assert_eq!(calls.total(), 0, "{calls:?}");
}

#[test]
fn the_policy_and_the_constitution_at_the_tip_are_read_and_the_block_is_the_one_that_changed_them()
{
    let w = world();
    write(
        &w,
        &[
            ("governance/policy.yaml", POLICY),
            ("governance/constitution.yaml", CONSTITUTION),
        ],
    );
    let policy_block = w.fake().block_id();
    // Two blocks that change pages and nothing of the rules.
    w.fake()
        .set("pages/Home.md", "---\ntitle: Home\n---\n\nTwo.\n");
    w.fake()
        .set("pages/Home.md", "---\ntitle: Home\n---\n\nThree.\n");
    w.service.forget_wiki(&w.wiki);
    let rules = rules(&w);
    assert_eq!(rules.block(), Some(policy_block.as_str()));
    assert_eq!(rules.policy().entry(), Entry::Contributor);
    assert_eq!(rules.policy().acceptance().approvals(), 2);
    assert_eq!(rules.constitution().policy_change().rung, Role::Admin);
    assert_eq!(rules.constitution().policy_change().approvals, 1);
    assert!(rules.fault().is_none());
    assert_eq!(rules.short_block(), &policy_block[..12]);
    // A later change of one of the files moves the block, and the next tip has the new rules.
    write(&w, &[("governance/policy.yaml", "entry: observer\n")]);
    let newer = self::rules(&w);
    assert_eq!(newer.block(), Some(w.fake().block_id().as_str()));
    assert_eq!(newer.policy().entry(), Entry::Observer);
    // The constitution is still the one the earlier block wrote.
    assert_eq!(newer.constitution().policy_change().rung, Role::Admin);
}

#[test]
fn the_rules_are_rebuilt_to_the_same_rules_after_the_derived_store_is_cleared() {
    let w = world();
    write(
        &w,
        &[
            ("governance/policy.yaml", POLICY),
            ("governance/constitution.yaml", CONSTITUTION),
        ],
    );
    let service = World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_governance_snapshots(w.store.clone());
    let first = service.rules_of(&w.wiki).expect("built");
    let tip = service.tip(&w.wiki, None).expect("tip");
    // The row is in the derived store, keyed by the tip.
    let row = w
        .store
        .governance_snapshot(&w.wiki.id, &tip.state_id)
        .expect("store")
        .expect("a row");
    assert_eq!(row.policy_block.as_deref(), Some(first.block().unwrap()));
    assert!(row.fault.is_none());
    // A service that starts reads it back from the row: no file is read and no block asked about.
    let restarted = World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_governance_snapshots(w.store.clone());
    let (again, calls) = w.counted(|| restarted.rules_of(&w.wiki).expect("stored"));
    assert_eq!(*again, *first);
    assert_eq!((calls.read, calls.change), (0, 0), "{calls:?}");
    // Clear the derived store (the wiki's rows and the memory): the same rules come back, built from the repository.
    let cleared = restarted.clear_derived(&w.wiki).expect("cleared");
    assert!(cleared > 0, "the row was among the derived rows");
    assert!(
        w.store
            .governance_snapshot(&w.wiki.id, &tip.state_id)
            .expect("store")
            .is_none()
    );
    let (rebuilt, calls) = w.counted(|| restarted.rules_of(&w.wiki).expect("rebuilt"));
    assert_eq!(*rebuilt, *first, "the same rules");
    assert!(calls.read >= 2, "read from the repository again: {calls:?}");
    // And the decisions they give are the same ones: the whole value is equal, block included.
    assert_eq!(rebuilt.block(), first.block());
    assert_eq!(rebuilt.policy().to_yaml(), first.policy().to_yaml());
    assert_eq!(
        rebuilt.constitution().to_yaml(),
        first.constitution().to_yaml()
    );
}

#[test]
fn a_tip_that_has_moved_away_loses_its_row_with_the_rest_of_the_derived_rows() {
    let w = world();
    write(&w, &[("governance/policy.yaml", POLICY)]);
    let service = World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_governance_snapshots(w.store.clone());
    let old_tip = service.tip(&w.wiki, None).expect("tip").state_id;
    service.rules_of(&w.wiki).expect("built");
    assert!(
        w.store
            .governance_snapshot(&w.wiki.id, &old_tip)
            .expect("store")
            .is_some()
    );
    // Only the tips the wiki has now are kept.
    let kept = vec![state_id(99)];
    w.store.retain_tips(&w.wiki.id, &kept).expect("retained");
    assert!(
        w.store
            .governance_snapshot(&w.wiki.id, &old_tip)
            .expect("store")
            .is_none()
    );
}

#[test]
fn a_governance_file_that_cannot_be_read_is_a_fault_with_its_file_and_line_and_the_defaults_decide()
{
    let w = world();
    // Line 3 is the second key under `rungs`, which the schema does not have.
    write(
        &w,
        &[(
            "governance/policy.yaml",
            "entry: none\nrungs:\n  accept: maintainer\n  fly: admin\n",
        )],
    );
    let rules = rules(&w);
    let fault = rules.fault().expect("a fault");
    assert_eq!(fault.file().path(), "governance/policy.yaml");
    assert_eq!(fault.refusal(), Refusal::UnknownKey);
    assert!(fault.line().is_some(), "{fault}");
    let said = fault.to_string();
    assert!(said.starts_with("governance/policy.yaml, line "), "{said}");
    // The defaults decide while a file is unreadable, whatever the other file says.
    assert_eq!(*rules.policy(), crate::governance::Policy::default());
    assert!(rules.block().is_some(), "the block that brought the file");
}

#[test]
fn the_fault_is_kept_in_the_row_and_read_back_whole() {
    let w = world();
    write(
        &w,
        &[("governance/constitution.yaml", "policy_change: [1\n")],
    );
    let service = World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_governance_snapshots(w.store.clone());
    let built = service.rules_of(&w.wiki).expect("built");
    let restarted = World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_governance_snapshots(w.store.clone());
    let (read_back, calls) = w.counted(|| restarted.rules_of(&w.wiki).expect("stored"));
    assert_eq!(calls.read, 0, "{calls:?}");
    let (built, read_back) = (
        built.fault().expect("fault"),
        read_back.fault().expect("fault"),
    );
    assert_eq!(
        (built.file(), built.line(), built.refusal()),
        (read_back.file(), read_back.line(), read_back.refusal())
    );
}

#[test]
fn a_file_longer_than_it_may_be_is_a_fault_and_is_not_read() {
    let w = world();
    let long = format!(
        "# {}\n",
        "x".repeat(crate::governance::MAX_CONSTITUTION_BYTES)
    );
    write(&w, &[("governance/constitution.yaml", long.as_str())]);
    let (rules, calls) = w.counted(|| rules(&w));
    assert_eq!(rules.fault().expect("fault").refusal(), Refusal::TooLong);
    assert_eq!(calls.read, 0, "its size said so: {calls:?}");
}

#[test]
fn a_policy_above_a_platform_limit_is_read_as_if_it_were_at_it() {
    let w = world();
    write(&w, &[("governance/policy.yaml", POLICY)]);
    let service = World::service_over(&w.counting, &w.store, &w.layout, config())
        .with_platform_limits(PlatformLimits {
            entry_max: Entry::Observer,
            ..PlatformLimits::default()
        });
    let rules = service.rules_of(&w.wiki).expect("rules");
    assert_eq!(
        rules.policy().entry(),
        Entry::Observer,
        "lowered to the limit"
    );
    assert_eq!(rules.lowered().len(), 1);
    assert_eq!(
        rules.lowered().first().map(|l| l.limit()),
        Some(Limit::EntryMax)
    );
    // The file itself is as it was: a service without the limit reads it whole.
    let plain = World::service_over(&w.counting, &w.store, &w.layout, config());
    assert_eq!(
        plain.rules_of(&w.wiki).expect("rules").policy().entry(),
        Entry::Contributor
    );
}

#[test]
fn only_the_default_views_tip_is_read() {
    let w = world();
    // Another view has governance files of its own; the default view has none.
    w.fake().add_view_with(
        "heads/other",
        &[
            ("pages/Home.md".to_owned(), "x".to_owned()),
            ("governance/policy.yaml".to_owned(), POLICY.to_owned()),
        ],
    );
    w.service.forget_wiki(&w.wiki);
    let rules = rules(&w);
    assert_eq!(*rules, Rules::defaults());
    // And a view that is not the default is no place the rules are read from, whatever view is asked for.
    let tip = w.service.tip(&w.wiki, None).expect("tip");
    assert_eq!(tip.view, w.wiki.default_view);
}

#[test]
fn a_walk_that_ends_before_it_finds_the_block_names_the_oldest_block_it_looked_at() {
    let w = world();
    write(&w, &[("governance/policy.yaml", POLICY)]);
    let first = w.fake().block_id();
    // Far more blocks than the walk looks at, none of which changes a governance file.
    for n in 0..(WALK_BLOCKS + 8) {
        w.fake().set(
            "pages/Home.md",
            &format!("---\ntitle: Home\n---\n\nEdit {n}.\n"),
        );
    }
    w.service.forget_wiki(&w.wiki);
    let (rules, calls) = w.counted(|| rules(&w));
    assert_eq!(rules.policy().entry(), Entry::Contributor);
    let named = rules.block().expect("a block");
    assert_ne!(named, first, "the walk could not reach it");
    // It is a block of the window, and the walk asked about no more than the window.
    assert!(calls.change <= WALK_BLOCKS, "{calls:?}");
}

#[test]
fn a_repository_that_cannot_be_asked_is_could_not_be_read_now_and_never_the_defaults() {
    let w = world();
    write(&w, &[("governance/policy.yaml", POLICY)]);
    w.fake()
        .fail_read(Some(inklkunn_prikk::StorageError::Unavailable {
            verb: "cat",
            reason: inklkunn_prikk::Unavailable::TimedOut { seconds: 1 },
        }));
    let record: WikiRecord = w.wiki.clone();
    assert!(
        matches!(
            w.service.rules_of(&record),
            Err(ReadError::CouldNotBeReadNow(_))
        ),
        "an error, not the defaults"
    );
    // The error is not kept: when the repository answers, the rules are there.
    w.fake().fail_read(None);
    assert_eq!(rules(&w).policy().entry(), Entry::Contributor);
}
