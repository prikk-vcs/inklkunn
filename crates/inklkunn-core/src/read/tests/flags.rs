//! The flags of recent changes (handoff 004 T6; RFC 004 D-10): *minor* and *automated* are the accepted
//! revision's, read from the proposal records through the proposal the message names; the lists filter on
//! them and label machine-assisted content; a filter the records cannot answer is refused.

use std::sync::Arc;

use inklkunn_store::{
    Account, AccountId, AccountStore, Base, DisplayName, FileChange, ProposalChange,
    ProposalFilter, ProposalId, ProposalOp, ProposalRecord, ProposalState, ProposalStore,
    ProposalTransition, RevisionContent, RevisionFlags, RevisionRecord, StoreError,
};

use super::super::testing::{World, block_id};
use super::*;
use crate::message::{Actor, BlockId, Message, Summary};
use crate::read::changes::{ChangeFilter, ChangeKind, Tri};

fn ids(rows: &[crate::read::ChangeRow]) -> Vec<String> {
    rows.iter().map(|r| r.summary.clone()).collect()
}

/// A submitted proposal whose accepted revision has these flags; returns the CT-03 message of its change.
fn accepted(
    w: &World,
    who: &AccountId,
    summary: &str,
    flags: RevisionFlags,
    rollback: bool,
) -> String {
    let id = ProposalId::new();
    let base = Base::new(&format!("{:064x}", 1_u64), &block_id(1)).expect("a base");
    let record = ProposalRecord {
        id: id.clone(),
        wiki: w.wiki.id.clone(),
        view: "heads/main".to_owned(),
        state: ProposalState::Draft,
        author: who.clone(),
        summary: summary.to_owned(),
        sources: Vec::new(),
        superseded_by: None,
        created_at: 1,
        updated_at: 1,
        submitted_at: None,
        closed_at: None,
    };
    let content = RevisionContent {
        base,
        files: vec![FileChange {
            path: "pages/Home.md".to_owned(),
            text: Some("text".to_owned()),
        }],
        renames: Vec::new(),
        minor: flags.minor,
        automated: flags.automated,
    };
    let step = |op, from, to, at| ProposalTransition {
        proposal: id.clone(),
        op,
        from,
        to,
        actor: who.clone(),
        reason: None,
        at,
    };
    w.store
        .create_proposal(
            &record,
            &content,
            &step(ProposalOp::Create, None, ProposalState::Draft, 1),
        )
        .expect("created");
    w.store
        .change_proposal(&ProposalChange {
            transition: step(
                ProposalOp::Submit,
                Some(ProposalState::Draft),
                ProposalState::HeldForReview,
                2,
            ),
            text: None,
            revision: Some(RevisionRecord {
                proposal: id.clone(),
                seq: 1,
                content,
                created_at: 2,
            }),
            superseded_by: None,
        })
        .expect("submitted");
    let actor = Actor::Account {
        id: who.clone(),
        name: DisplayName::new("Ren Sato").expect("name"),
    };
    let make = if rollback {
        Message::rollback
    } else {
        Message::proposal
    };
    make(
        Summary::new(summary).expect("summary"),
        id,
        actor.clone(),
        actor,
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a message")
    .to_text()
}

fn person(w: &World) -> AccountId {
    let account = Account {
        id: AccountId::new(),
        display_name: DisplayName::new("Ren Sato").expect("name"),
        local: None,
        created_at: 1,
        disabled: false,
    };
    w.store.add_account(&account).expect("an account");
    account.id
}

/// Version 1 is the wiki's creation (by the service); then a plain proposal, a minor one, a machine-assisted
/// one, a minor and machine-assisted rollback, and a change somebody else sealed.
fn world() -> World {
    let w = World::pages_with(&[("Home", &[], "Home.\n")], ttl_zero());
    let who = person(&w);
    let flags = |minor, automated| RevisionFlags { minor, automated };
    let system = Message::system(
        Summary::new("Create the wiki").expect("summary"),
        crate::message::SystemLabel::new("inklkunn").expect("label"),
    )
    .to_text();
    w.fake().say(1, &system);
    for (version, text) in [
        (2, accepted(&w, &who, "Plain", flags(false, false), false)),
        (3, accepted(&w, &who, "Minor", flags(true, false), false)),
        (4, accepted(&w, &who, "Machine", flags(false, true), false)),
        (5, accepted(&w, &who, "Both", flags(true, true), true)),
        (6, "Sealed by someone else".to_owned()),
    ] {
        w.fake().set(
            "pages/Home.md",
            &format!("---\ntitle: Home\n---\n\nv{version}.\n"),
        );
        w.fake().say(version, &text);
    }
    w
}

fn service(w: &World, records: Option<Arc<dyn ProposalStore>>) -> ReadService {
    let service = World::service_over(&w.counting, &w.store, &w.layout, ttl_zero());
    match records {
        Some(records) => service.with_revision_flags(records),
        None => service,
    }
}

fn listed(service: &ReadService, w: &World, filter: &ChangeFilter) -> Vec<String> {
    ids(&service.changes(&w.wiki, filter, 50).expect("a list").rows)
}

fn filter(automated: Tri, minor: Tri) -> ChangeFilter {
    ChangeFilter {
        automated,
        minor,
        ..ChangeFilter::default()
    }
}

#[test]
fn the_flags_of_the_accepted_revision_label_and_filter_the_list() {
    let w = world();
    let s = service(&w, Some(w.store.clone()));
    let all = s
        .changes(&w.wiki, &ChangeFilter::default(), 50)
        .unwrap()
        .rows;
    let by_summary = |name: &str| {
        all.iter()
            .find(|r| r.summary == name)
            .unwrap_or_else(|| panic!("{name} in {:?}", ids(&all)))
    };
    assert_eq!(all.len(), 6);
    let plain = by_summary("Plain");
    assert!(!plain.minor && !plain.machine_assisted && !plain.automated());
    let minor = by_summary("Minor");
    assert!(minor.minor && !minor.machine_assisted && !minor.automated());
    let machine = by_summary("Machine");
    assert!(
        machine.machine_assisted && !machine.minor && machine.automated() && !machine.by_service()
    );
    // The person who accepted it is still named: the flag labels, it does not replace.
    assert!(matches!(machine.accepted_by, Some(Person::Account(_))));
    let both = by_summary("Both");
    assert!(both.minor && both.machine_assisted && both.kind == ChangeKind::Rollback);
    // The service's own change and a foreign one carry no flags.
    let created = by_summary("Create the wiki");
    assert!(
        created.by_service() && created.automated() && !created.minor && !created.machine_assisted
    );
    let foreign = by_summary("Sealed by someone else");
    assert!(foreign.kind == ChangeKind::Foreign && !foreign.minor && !foreign.automated());

    // minor: only those, or none of those.
    let only_minor = listed(&s, &w, &filter(Tri::Any, Tri::Only));
    assert_eq!(only_minor.len(), 2);
    assert!(only_minor.contains(&"Minor".to_owned()) && only_minor.contains(&"Both".to_owned()));
    let not_minor = listed(&s, &w, &filter(Tri::Any, Tri::Hide));
    assert_eq!(not_minor.len(), 4);
    assert!(!not_minor.contains(&"Minor".to_owned()) && !not_minor.contains(&"Both".to_owned()));

    // automated: what the service made and what a machine assisted.
    let only_auto = listed(&s, &w, &filter(Tri::Only, Tri::Any));
    assert_eq!(only_auto.len(), 3);
    for name in ["Create the wiki", "Machine", "Both"] {
        assert!(only_auto.contains(&name.to_owned()), "{name}");
    }
    let by_hand = listed(&s, &w, &filter(Tri::Hide, Tri::Any));
    assert_eq!(by_hand.len(), 3);
    for name in ["Plain", "Minor", "Sealed by someone else"] {
        assert!(by_hand.contains(&name.to_owned()), "{name}");
    }

    // Both together.
    assert_eq!(listed(&s, &w, &filter(Tri::Only, Tri::Only)), ["Both"]);
    assert_eq!(listed(&s, &w, &filter(Tri::Hide, Tri::Hide)).len(), 2);
}

#[test]
fn without_records_nothing_is_flagged_as_before() {
    let w = world();
    let s = service(&w, None);
    assert_eq!(listed(&s, &w, &ChangeFilter::default()).len(), 6);
    assert!(listed(&s, &w, &filter(Tri::Any, Tri::Only)).is_empty());
    assert_eq!(listed(&s, &w, &filter(Tri::Any, Tri::Hide)).len(), 6);
    // The service's own change is automated by what it is, not by a flag.
    assert_eq!(
        listed(&s, &w, &filter(Tri::Only, Tri::Any)),
        ["Create the wiki"]
    );
}

/// Records that cannot answer.
struct Down;

fn down<T>() -> Result<T, StoreError> {
    Err(StoreError::Backend("down".to_owned()))
}

impl ProposalStore for Down {
    fn create_proposal(
        &self,
        _: &ProposalRecord,
        _: &RevisionContent,
        _: &ProposalTransition,
    ) -> Result<(), StoreError> {
        down()
    }
    fn create_rollback_proposal(
        &self,
        _: &ProposalRecord,
        _: &RevisionContent,
        _: &ProposalTransition,
        _: &str,
    ) -> Result<(), StoreError> {
        down()
    }
    fn rollback_of(
        &self,
        _: &ProposalId,
        _: Option<&AccountId>,
    ) -> Result<Option<String>, StoreError> {
        down()
    }
    fn revision_flags(
        &self,
        _: &[ProposalId],
    ) -> Result<Vec<(ProposalId, RevisionFlags)>, StoreError> {
        down()
    }
    fn proposal(
        &self,
        _: &ProposalId,
        _: Option<&AccountId>,
    ) -> Result<Option<ProposalRecord>, StoreError> {
        down()
    }
    fn proposals(
        &self,
        _: &inklkunn_store::WikiId,
        _: &ProposalFilter,
        _: Option<&AccountId>,
    ) -> Result<Vec<ProposalRecord>, StoreError> {
        down()
    }
    fn draft(
        &self,
        _: &ProposalId,
        _: Option<&AccountId>,
    ) -> Result<Option<RevisionContent>, StoreError> {
        down()
    }
    fn save_draft(
        &self,
        _: &ProposalId,
        _: &str,
        _: &[String],
        _: &RevisionContent,
        _: i64,
    ) -> Result<(), StoreError> {
        down()
    }
    fn change_proposal(&self, _: &ProposalChange) -> Result<ProposalRecord, StoreError> {
        down()
    }
    fn revisions(
        &self,
        _: &ProposalId,
        _: Option<&AccountId>,
    ) -> Result<Vec<RevisionRecord>, StoreError> {
        down()
    }
    fn transitions(
        &self,
        _: &ProposalId,
        _: Option<&AccountId>,
    ) -> Result<Vec<ProposalTransition>, StoreError> {
        down()
    }
    fn add_comment(
        &self,
        _: &inklkunn_store::CommentRecord,
        _: &[ProposalState],
    ) -> Result<(), StoreError> {
        down()
    }
    fn comments(
        &self,
        _: &ProposalId,
        _: Option<&AccountId>,
    ) -> Result<Vec<inklkunn_store::CommentRecord>, StoreError> {
        down()
    }
    fn add_decision(&self, _: &inklkunn_store::DecisionRecord) -> Result<(), StoreError> {
        down()
    }
    fn decisions(
        &self,
        _: &ProposalId,
        _: Option<&AccountId>,
    ) -> Result<Vec<inklkunn_store::DecisionRecord>, StoreError> {
        down()
    }
    fn wiki_decisions(
        &self,
        _: &inklkunn_store::WikiId,
    ) -> Result<Vec<inklkunn_store::DecisionRecord>, StoreError> {
        down()
    }
    fn open_count(&self, _: &inklkunn_store::WikiId, _: &AccountId) -> Result<u64, StoreError> {
        down()
    }
    fn opened_since(
        &self,
        _: &inklkunn_store::WikiId,
        _: Option<&AccountId>,
        _: i64,
    ) -> Result<Vec<i64>, StoreError> {
        down()
    }
}

#[test]
fn a_filter_the_records_cannot_answer_is_refused_and_a_plain_list_is_still_listed() {
    let w = world();
    let s = service(&w, Some(Arc::new(Down)));
    // Not asked to filter on a flag: the list is there, unflagged, and the failure is counted.
    let before = s.store_failures();
    assert_eq!(listed(&s, &w, &ChangeFilter::default()).len(), 6);
    assert!(s.store_failures() > before);
    // Asked to hide the minor ones: a list that might show them is not given.
    for f in [
        filter(Tri::Any, Tri::Hide),
        filter(Tri::Hide, Tri::Any),
        filter(Tri::Only, Tri::Any),
    ] {
        assert!(
            matches!(
                s.changes(&w.wiki, &f, 50),
                Err(ReadError::CouldNotBeReadNow(Cause::Store(_)))
            ),
            "{f:?}"
        );
    }
}
