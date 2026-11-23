//! The records of governance and moderation (RFC 006 D-6 - D-10; handoff 006 G-T1; store schema 19), through the traits,
//! against both stores: grant decisions that change the role grants in the same step, restrictions and temporary
//! protections as chains of events that end by the clock, tombstones, appeals decided once with the reversal in the same
//! step, and the derived governance snapshot. The SQLite database's own refusals and the migration are in
//! `governance_schema.rs`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod support;

use inklkunn_store::governance::{MAX_APPEAL_BYTES, MAX_REASON_BYTES, MAX_SNAPSHOT_FILE_BYTES};
use inklkunn_store::{
    AccountId, AccountStore, Actor, AppealDecision, AppealId, AppealOutcome, AppealRecord,
    AppealState, AppealedAction, AppealedKind, Decided, DerivedStore, GovernanceFault,
    GovernanceSnapshot, GovernanceSnapshotStore, GovernanceStore, GrantDecisionId,
    GrantDecisionRecord, HiddenItem, HiddenItemKind, HiddenParts, MeasureEvent, MemoryStore,
    ProposalChange, ProposalId, ProposalOp, ProposalState, ProposalStore, ProtectionEvent,
    ProtectionEventId, ProtectionKind, ProtectionRequirement, ReasonCategory, RestrictionEvent,
    RestrictionEventId, Reversal, Role, RoleStore, SqliteStore, StoreError, TombstoneAct,
    TombstoneActId, TombstoneEvent, TombstoneEventId, TombstoneReversal, WikiId, WikiStore,
    hidden_parts,
};
use support::{account, mv, open_draft, step, submit, wiki};

const T0: i64 = 1_000;

fn block(n: u64) -> String {
    format!("{n:064x}")
}

fn actor(account: &AccountId, rung: Role) -> Actor {
    Actor {
        account: account.clone(),
        rung,
    }
}

fn backend<T: std::fmt::Debug>(r: Result<T, StoreError>) -> String {
    match r {
        Err(StoreError::Backend(why)) => why,
        other => panic!("expected a refusal in the backend's words, got {other:?}"),
    }
}

// ------------------------------------------------------------------------------------------------ fixtures

fn grant(
    w: &WikiId,
    by: Option<(&AccountId, Role)>,
    target: &AccountId,
    before: Option<Role>,
    after: Option<Role>,
    at: i64,
) -> GrantDecisionRecord {
    GrantDecisionRecord {
        id: GrantDecisionId::new(),
        wiki: w.clone(),
        at,
        by: by.map(|(a, r)| actor(a, r)),
        policy_block: None,
        target: target.clone(),
        before,
        after,
        reason: None,
        appeal: None,
    }
}

/// A restriction event of `event`, following `of`; a `Set` or `Renewed` one ends at `ends`.
fn restriction(
    w: &WikiId,
    event: MeasureEvent,
    account: &AccountId,
    by: (&AccountId, Role),
    at: i64,
    ends: Option<i64>,
    of: Option<&RestrictionEvent>,
) -> RestrictionEvent {
    RestrictionEvent {
        id: RestrictionEventId::new(),
        wiki: w.clone(),
        at,
        event,
        account: account.clone(),
        by: actor(by.0, by.1),
        policy_block: Some(block(1)),
        reason: event
            .puts_in_place()
            .then(|| "Repeated personal attacks".to_owned()),
        expires_at: ends,
        of_event: of.map(|e| e.id.clone()),
        appeal: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn protection(
    w: &WikiId,
    event: MeasureEvent,
    kind: ProtectionKind,
    target: &str,
    by: (&AccountId, Role),
    at: i64,
    ends: Option<i64>,
    of: Option<&ProtectionEvent>,
) -> ProtectionEvent {
    ProtectionEvent {
        id: ProtectionEventId::new(),
        wiki: w.clone(),
        at,
        event,
        kind,
        target: target.to_owned(),
        requirement: event
            .puts_in_place()
            .then_some(ProtectionRequirement::Review),
        by: actor(by.0, by.1),
        policy_block: None,
        reason: event.puts_in_place().then(|| "An edit war".to_owned()),
        expires_at: ends,
        of_event: of.map(|e| e.id.clone()),
        appeal: None,
    }
}

fn comment_item() -> HiddenItem {
    HiddenItem {
        kind: HiddenItemKind::ProposalComment,
        id: AccountId::new().to_string(),
        reference: None,
    }
}

fn hide(
    w: &WikiId,
    item: &HiddenItem,
    parts: HiddenParts,
    by: (&AccountId, Role),
    at: i64,
) -> TombstoneEvent {
    TombstoneEvent {
        id: TombstoneEventId::new(),
        wiki: w.clone(),
        at,
        act: TombstoneAct::Hide,
        act_id: TombstoneActId::new(),
        item: item.clone(),
        parts,
        category: Some(ReasonCategory::Harassment),
        note: None,
        affected: None,
        by: actor(by.0, by.1),
        policy_block: None,
        of_event: None,
        appeal: None,
        declined_transition: None,
    }
}

fn unhide(of: &TombstoneEvent, by: (&AccountId, Role), at: i64) -> TombstoneEvent {
    TombstoneEvent {
        id: TombstoneEventId::new(),
        act: TombstoneAct::Unhide,
        act_id: TombstoneActId::new(),
        at,
        category: None,
        note: None,
        by: actor(by.0, by.1),
        of_event: Some(of.id.clone()),
        declined_transition: None,
        ..of.clone()
    }
}

const CONTENT: HiddenParts = HiddenParts {
    content: true,
    summary: false,
    author: false,
};
const CONTENT_AND_AUTHOR: HiddenParts = HiddenParts {
    content: true,
    summary: false,
    author: true,
};

fn appeal(
    w: &WikiId,
    kind: AppealedKind,
    action_id: &str,
    action_actor: Option<&AccountId>,
    appellant: &AccountId,
    at: i64,
) -> AppealRecord {
    AppealRecord {
        id: AppealId::new(),
        wiki: w.clone(),
        at,
        action: AppealedAction {
            kind,
            id: action_id.to_owned(),
        },
        action_actor: action_actor.cloned(),
        appellant: appellant.clone(),
        text: "I think this was a mistake.".to_owned(),
        state: AppealState::Open,
    }
}

fn decision(by: &AccountId, at: i64, outcome: AppealOutcome) -> AppealDecision {
    AppealDecision {
        at,
        by: by.clone(),
        reason: "Looked at it again.".to_owned(),
        outcome,
    }
}

/// The reversal of a hiding.
fn tombstone(r: TombstoneReversal) -> Reversal {
    Reversal::Tombstone(Box::new(r))
}

fn overturn(reversal: Reversal) -> AppealOutcome {
    AppealOutcome::Overturned(Box::new(reversal))
}

/// A wiki with an admin, a maintainer and a person to act on.
struct Rig<S> {
    store: S,
    wiki: WikiId,
    admin: AccountId,
    maintainer: AccountId,
    person: AccountId,
}

fn rig<S>(store: S) -> Rig<S>
where
    S: GovernanceStore + WikiStore + AccountStore,
{
    let w = wiki(&store, "governed");
    let (admin, maintainer, person) = (account(&store), account(&store), account(&store));
    Rig {
        store,
        wiki: w.id,
        admin,
        maintainer,
        person,
    }
}

// ------------------------------------------------------------------------------------------------ grants

fn grants_are_recorded_and_applied_in_one_step<S>(r: &Rig<S>)
where
    S: GovernanceStore + RoleStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let d1 = grant(
        w,
        Some((&r.admin, Role::Admin)),
        &r.person,
        None,
        Some(Role::Reviewer),
        T0,
    );
    s.record_grant_decision(&d1).unwrap();
    assert_eq!(s.role(&r.person, w).unwrap(), Some(Role::Reviewer));
    let held = s.grants_in(w).unwrap();
    assert_eq!(held.len(), 1);
    let held_first = held.first().expect("a grant");
    assert_eq!(held_first.granted_by.as_ref(), Some(&r.admin));
    assert_eq!(held_first.granted_at, T0);
    assert_eq!(s.grant_decision(&d1.id).unwrap(), Some(d1.clone()));

    // A decision says what the rung was, and applies only to a grant that still is that: nothing changes otherwise.
    let stale = grant(
        w,
        Some((&r.admin, Role::Admin)),
        &r.person,
        None,
        Some(Role::Maintainer),
        T0 + 1,
    );
    assert_eq!(
        s.record_grant_decision(&stale),
        Err(StoreError::StateChanged)
    );
    assert_eq!(s.role(&r.person, w).unwrap(), Some(Role::Reviewer));
    assert_eq!(s.grant_decision(&stale.id).unwrap(), None);

    // A promotion, with its reason and the rules it was made under.
    let mut d2 = grant(
        w,
        Some((&r.admin, Role::Admin)),
        &r.person,
        Some(Role::Reviewer),
        Some(Role::Maintainer),
        T0 + 2,
    );
    d2.reason = Some("Has reviewed forty proposals.".to_owned());
    d2.policy_block = Some(block(7));
    s.record_grant_decision(&d2).unwrap();
    assert_eq!(s.role(&r.person, w).unwrap(), Some(Role::Maintainer));
    assert_eq!(s.grant_decision(&d2.id).unwrap(), Some(d2.clone()));

    // A revocation is a decision whose rung after is none, and it takes the grant away.
    let d3 = grant(
        w,
        Some((&r.admin, Role::Admin)),
        &r.person,
        Some(Role::Maintainer),
        None,
        T0 + 3,
    );
    s.record_grant_decision(&d3).unwrap();
    assert_eq!(s.role(&r.person, w).unwrap(), None);
    assert!(s.grants_in(w).unwrap().is_empty());

    // The operator, from the command line, is not an account: no actor and no rung.
    let d4 = grant(w, None, &r.person, None, Some(Role::Contributor), T0 + 4);
    s.record_grant_decision(&d4).unwrap();
    assert_eq!(
        s.grants_in(w)
            .unwrap()
            .first()
            .map(|g| g.granted_by.clone()),
        Some(None)
    );
    assert_eq!(s.grant_decision(&d4.id).unwrap().unwrap().by, None);

    // Newest first, for the wiki or for one person, never more than asked.
    let ids = |v: Vec<GrantDecisionRecord>| v.into_iter().map(|d| d.id).collect::<Vec<_>>();
    assert_eq!(
        ids(s.grant_decisions(w, None, 10).unwrap()),
        [&d4, &d3, &d2, &d1].map(|d| d.id.clone())
    );
    assert_eq!(
        ids(s.grant_decisions(w, None, 1).unwrap()),
        vec![d4.id.clone()]
    );
    assert_eq!(
        ids(s.grant_decisions(w, None, 0).unwrap()),
        vec![d4.id.clone()]
    );
    assert_eq!(
        ids(s.grant_decisions(w, Some(&r.admin), 10).unwrap()),
        Vec::<GrantDecisionId>::new()
    );
    assert_eq!(s.grant_decisions(w, Some(&r.person), 10).unwrap().len(), 4);
}

fn a_malformed_or_unknown_grant_decision_is_refused<S>(r: &Rig<S>)
where
    S: GovernanceStore + RoleStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let base = grant(
        w,
        Some((&r.admin, Role::Admin)),
        &r.person,
        None,
        Some(Role::Observer),
        T0,
    );
    // A decision that changes nothing is not a decision.
    let mut noop = base.clone();
    noop.after = None;
    assert!(backend(s.record_grant_decision(&noop)).contains("changes the rung"));
    // The wiki and the accounts are there.
    let mut elsewhere = base.clone();
    elsewhere.wiki = WikiId::new();
    assert_eq!(
        s.record_grant_decision(&elsewhere),
        Err(StoreError::NotFound)
    );
    let mut nobody = base.clone();
    nobody.target = AccountId::new();
    assert_eq!(
        s.record_grant_decision(&nobody),
        Err(StoreError::AccountNotFound)
    );
    let mut ghost = base.clone();
    ghost.by = Some(actor(&AccountId::new(), Role::Admin));
    assert_eq!(
        s.record_grant_decision(&ghost),
        Err(StoreError::AccountNotFound)
    );
    // A reason is 1 to 4096 bytes; a policy block is a block id.
    for reason in [String::new(), "x".repeat(MAX_REASON_BYTES + 1)] {
        let mut bad = base.clone();
        bad.reason = Some(reason);
        assert!(backend(s.record_grant_decision(&bad)).contains("a reason"));
    }
    let mut bad_block = base.clone();
    bad_block.policy_block = Some("NOT-A-BLOCK".to_owned());
    assert!(backend(s.record_grant_decision(&bad_block)).contains("block id"));
    // A reason at the bound is kept; an id is used once.
    let mut at_bound = base.clone();
    at_bound.reason = Some("x".repeat(MAX_REASON_BYTES));
    s.record_grant_decision(&at_bound).unwrap();
    assert_eq!(
        s.record_grant_decision(&at_bound),
        Err(StoreError::AlreadyRecorded)
    );
    // Nothing that was refused changed a grant.
    assert_eq!(s.role(&r.person, w).unwrap(), Some(Role::Observer));
    assert_eq!(s.grant_decisions(w, None, 10).unwrap().len(), 1);
}

// ------------------------------------------------------------------------------------------------ restrictions

fn a_restriction_ends_by_the_clock_and_its_chain_is_a_line<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let set = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0,
        Some(T0 + 100),
        None,
    );
    s.record_restriction_event(&set).unwrap();
    assert_eq!(s.restriction_event(&set.id).unwrap(), Some(set.clone()));

    let active = |at: i64| s.active_restrictions(w, &r.person, at).unwrap();
    assert!(active(T0 - 1).is_empty(), "it has not begun");
    assert_eq!(active(T0), vec![set.clone()]);
    assert_eq!(active(T0 + 99), vec![set.clone()]);
    assert!(
        active(T0 + 100).is_empty(),
        "it ends at its end, and nothing was written for it to end"
    );
    assert!(
        s.active_restrictions(w, &r.maintainer, T0 + 1)
            .unwrap()
            .is_empty()
    );
    assert!(
        s.active_restrictions(&WikiId::new(), &r.person, T0 + 1)
            .unwrap()
            .is_empty()
    );

    // Renewing is a new event that follows the first; the first is then not in force, and the renewal is.
    let renewed = restriction(
        w,
        MeasureEvent::Renewed,
        &r.person,
        (&r.admin, Role::Admin),
        T0 + 50,
        Some(T0 + 300),
        Some(&set),
    );
    s.record_restriction_event(&renewed).unwrap();
    assert_eq!(active(T0 + 120), vec![renewed.clone()]);
    assert_eq!(
        active(T0 + 60),
        vec![renewed.clone()],
        "the renewal replaced it"
    );

    // An event is followed by at most one.
    let again = restriction(
        w,
        MeasureEvent::Renewed,
        &r.person,
        by,
        T0 + 55,
        Some(T0 + 400),
        Some(&set),
    );
    assert_eq!(
        s.record_restriction_event(&again),
        Err(StoreError::AlreadyRecorded)
    );

    // Lifting ends the chain early.
    let lifted = restriction(
        w,
        MeasureEvent::Lifted,
        &r.person,
        by,
        T0 + 60,
        None,
        Some(&renewed),
    );
    s.record_restriction_event(&lifted).unwrap();
    assert!(active(T0 + 61).is_empty());
    assert_eq!(
        s.restriction_events(w, &r.person).unwrap(),
        [set.clone(), renewed, lifted.clone()]
    );
    // Nothing follows a lift.
    let after = restriction(
        w,
        MeasureEvent::Renewed,
        &r.person,
        by,
        T0 + 70,
        Some(T0 + 500),
        Some(&lifted),
    );
    assert!(backend(s.record_restriction_event(&after)).contains("follows a set or renewed event"));

    // Two restrictions in force at once: the one that ends last first.
    let other = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0 + 200,
        Some(T0 + 900),
        None,
    );
    let short = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0 + 200,
        Some(T0 + 250),
        None,
    );
    s.record_restriction_event(&short).unwrap();
    s.record_restriction_event(&other).unwrap();
    assert_eq!(active(T0 + 210), [other, short]);
}

fn the_restrictions_in_force_in_a_wiki_are_read_in_one_query_and_end_by_the_clock<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    // Two people restricted, one of them twice over (the first is renewed, so only the renewal is in force), and a
    // restriction that has ended.
    let first = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0,
        Some(T0 + 100),
        None,
    );
    let renewed = restriction(
        w,
        MeasureEvent::Renewed,
        &r.person,
        by,
        T0 + 10,
        Some(T0 + 500),
        Some(&first),
    );
    let other = restriction(
        w,
        MeasureEvent::Set,
        &r.admin,
        (&r.maintainer, Role::Maintainer),
        T0,
        Some(T0 + 300),
        None,
    );
    for e in [&first, &renewed, &other] {
        s.record_restriction_event(e).unwrap();
    }
    let in_force = |at: i64, limit: usize| s.active_restrictions_in(w, at, limit).unwrap();
    assert_eq!(in_force(T0 + 20, 10), [renewed.clone(), other.clone()]);
    assert_eq!(
        in_force(T0 + 299, 10),
        [renewed.clone(), other.clone()],
        "the second before the end of one"
    );
    assert_eq!(
        in_force(T0 + 300, 10),
        std::slice::from_ref(&renewed),
        "at the end of one it is not in force, and nothing was written for that"
    );
    assert!(in_force(T0 + 500, 10).is_empty());
    assert!(in_force(T0 - 1, 10).is_empty(), "before they begin");
    assert_eq!(in_force(T0 + 20, 1), [renewed], "bounded by the limit");
    assert!(
        s.active_restrictions_in(&WikiId::new(), T0 + 20, 10)
            .unwrap()
            .is_empty(),
        "another wiki has none"
    );
}

fn a_restriction_chain_keeps_to_its_account_and_its_time<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let set = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0,
        Some(T0 + 100),
        None,
    );
    s.record_restriction_event(&set).unwrap();
    // An event follows an event that is there, of the same account, no later than itself.
    let mut phantom = restriction(
        w,
        MeasureEvent::Lifted,
        &r.person,
        by,
        T0 + 1,
        None,
        Some(&set),
    );
    phantom.of_event = Some(RestrictionEventId::new());
    assert_eq!(
        s.record_restriction_event(&phantom),
        Err(StoreError::NotFound)
    );
    let someone_else = restriction(
        w,
        MeasureEvent::Lifted,
        &r.admin,
        (&r.maintainer, Role::Maintainer),
        T0 + 1,
        None,
        Some(&set),
    );
    assert!(
        backend(s.record_restriction_event(&someone_else))
            .contains("the same account in the same wiki")
    );
    let early = restriction(
        w,
        MeasureEvent::Lifted,
        &r.person,
        by,
        T0 - 1,
        None,
        Some(&set),
    );
    assert!(backend(s.record_restriction_event(&early)).contains("follows a set or renewed event"));

    // The shapes: a set has a reason and an end after its beginning, and follows nothing; what ends one has no end of its
    // own; a reversal names an appeal; nobody restricts themselves.
    let mut no_reason = restriction(w, MeasureEvent::Set, &r.person, by, T0, Some(T0 + 5), None);
    no_reason.reason = None;
    assert!(backend(s.record_restriction_event(&no_reason)).contains("has a reason"));
    let backwards = restriction(w, MeasureEvent::Set, &r.person, by, T0, Some(T0), None);
    assert!(backend(s.record_restriction_event(&backwards)).contains("ends after it begins"));
    let mut sets_after = restriction(w, MeasureEvent::Set, &r.person, by, T0, Some(T0 + 5), None);
    sets_after.of_event = Some(set.id.clone());
    assert!(
        backend(s.record_restriction_event(&sets_after)).contains("names the event it follows")
    );
    let mut lift_with_end = restriction(
        w,
        MeasureEvent::Lifted,
        &r.person,
        by,
        T0 + 1,
        Some(T0 + 9),
        Some(&set),
    );
    assert!(backend(s.record_restriction_event(&lift_with_end)).contains("no end of its own"));
    lift_with_end.expires_at = None;
    lift_with_end.appeal = Some(AppealId::new());
    assert!(backend(s.record_restriction_event(&lift_with_end)).contains("names the appeal"));
    let reversed_without = restriction(
        w,
        MeasureEvent::Reversed,
        &r.person,
        by,
        T0 + 1,
        None,
        Some(&set),
    );
    assert!(backend(s.record_restriction_event(&reversed_without)).contains("names the appeal"));
    let own = restriction(
        w,
        MeasureEvent::Set,
        &r.maintainer,
        (&r.maintainer, Role::Maintainer),
        T0,
        Some(T0 + 5),
        None,
    );
    assert!(backend(s.record_restriction_event(&own)).contains("nobody acts on their own"));
    // The wiki and the accounts are there; an id is used once.
    let mut elsewhere = restriction(w, MeasureEvent::Set, &r.person, by, T0, Some(T0 + 5), None);
    elsewhere.wiki = WikiId::new();
    assert_eq!(
        s.record_restriction_event(&elsewhere),
        Err(StoreError::NotFound)
    );
    let mut ghost = restriction(w, MeasureEvent::Set, &r.person, by, T0, Some(T0 + 5), None);
    ghost.account = AccountId::new();
    assert_eq!(
        s.record_restriction_event(&ghost),
        Err(StoreError::AccountNotFound)
    );
    assert_eq!(
        s.record_restriction_event(&set),
        Err(StoreError::AlreadyRecorded)
    );
    assert_eq!(s.restriction_events(w, &r.person).unwrap(), [set]);
}

// ------------------------------------------------------------------------------------------------ protections

fn a_temporary_protection_ends_by_the_clock_and_is_found_by_exactly_its_target<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let page = protection(
        w,
        MeasureEvent::Set,
        ProtectionKind::Page,
        "Help/x",
        by,
        T0,
        Some(T0 + 100),
        None,
    );
    let mut namespace = protection(
        w,
        MeasureEvent::Set,
        ProtectionKind::Namespace,
        "Hel",
        by,
        T0,
        Some(T0 + 500),
        None,
    );
    namespace.requirement = Some(ProtectionRequirement::Rung(Role::Maintainer));
    s.record_protection_event(&page).unwrap();
    s.record_protection_event(&namespace).unwrap();
    assert_eq!(
        s.protection_event(&namespace.id).unwrap(),
        Some(namespace.clone())
    );

    // In force, the one that ends last first; and by exactly the target asked for: a namespace `Hel` is not a page
    // `Help/x`, and which paths a namespace covers is not the store's question.
    assert_eq!(
        s.active_protections(w, T0 + 10).unwrap(),
        [namespace.clone(), page.clone()]
    );
    assert_eq!(
        s.active_protections_on(w, ProtectionKind::Page, "Help/x", T0 + 10)
            .unwrap(),
        vec![page.clone()]
    );
    assert_eq!(
        s.active_protections_on(w, ProtectionKind::Namespace, "Hel", T0 + 10)
            .unwrap(),
        vec![namespace.clone()]
    );
    for (kind, target) in [
        (ProtectionKind::Namespace, "Help/x"),
        (ProtectionKind::Page, "Hel"),
        (ProtectionKind::Page, "Help/"),
        (ProtectionKind::Page, "help/x"),
    ] {
        assert!(
            s.active_protections_on(w, kind, target, T0 + 10)
                .unwrap()
                .is_empty(),
            "{kind:?} {target}"
        );
    }
    assert_eq!(
        s.active_protections(w, T0 + 100).unwrap(),
        vec![namespace.clone()]
    );
    assert!(s.active_protections(w, T0 - 1).unwrap().is_empty());
    assert!(
        s.active_protections(&WikiId::new(), T0 + 10)
            .unwrap()
            .is_empty()
    );

    // Renewing and lifting follow the one before, of the same page, and say what they require or nothing.
    let renewed = protection(
        w,
        MeasureEvent::Renewed,
        ProtectionKind::Page,
        "Help/x",
        by,
        T0 + 90,
        Some(T0 + 700),
        Some(&page),
    );
    s.record_protection_event(&renewed).unwrap();
    assert_eq!(
        s.active_protections_on(w, ProtectionKind::Page, "Help/x", T0 + 150)
            .unwrap(),
        vec![renewed.clone()]
    );
    let elsewhere = protection(
        w,
        MeasureEvent::Lifted,
        ProtectionKind::Page,
        "Help/y",
        by,
        T0 + 91,
        None,
        Some(&renewed),
    );
    assert!(backend(s.record_protection_event(&elsewhere)).contains("the same page or namespace"));
    let lifted = protection(
        w,
        MeasureEvent::Lifted,
        ProtectionKind::Page,
        "Help/x",
        by,
        T0 + 120,
        None,
        Some(&renewed),
    );
    s.record_protection_event(&lifted).unwrap();
    assert!(
        s.active_protections_on(w, ProtectionKind::Page, "Help/x", T0 + 130)
            .unwrap()
            .is_empty()
    );
    let log = s.protection_events(w, 10).unwrap();
    assert_eq!(log.first(), Some(&lifted), "newest first");
    assert_eq!(log.len(), 4);
    assert_eq!(s.protection_events(w, 2).unwrap().len(), 2);
}

fn a_protection_event_is_well_formed_or_refused<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let set = || {
        protection(
            w,
            MeasureEvent::Set,
            ProtectionKind::Page,
            "Help/x",
            by,
            T0,
            Some(T0 + 100),
            None,
        )
    };
    let mut no_requirement = set();
    no_requirement.requirement = None;
    assert!(backend(s.record_protection_event(&no_requirement)).contains("says what it requires"));
    let lifted_with_requirement = {
        let first = set();
        s.record_protection_event(&first).unwrap();
        let mut lift = protection(
            w,
            MeasureEvent::Lifted,
            ProtectionKind::Page,
            "Help/x",
            by,
            T0 + 1,
            None,
            Some(&first),
        );
        lift.requirement = Some(ProtectionRequirement::Review);
        lift
    };
    assert!(
        backend(s.record_protection_event(&lifted_with_requirement))
            .contains("says what it requires")
    );
    for target in [String::new(), "x".repeat(1025)] {
        let mut bad = set();
        bad.target = target;
        assert!(backend(s.record_protection_event(&bad)).contains("a protected page or namespace"));
    }
    let mut no_reason = set();
    no_reason.reason = None;
    assert!(backend(s.record_protection_event(&no_reason)).contains("has a reason"));
    let mut elsewhere = set();
    elsewhere.wiki = WikiId::new();
    assert_eq!(
        s.record_protection_event(&elsewhere),
        Err(StoreError::NotFound)
    );
    let mut ghost = set();
    ghost.by = actor(&AccountId::new(), Role::Maintainer);
    assert_eq!(
        s.record_protection_event(&ghost),
        Err(StoreError::AccountNotFound)
    );
    // The requirement round-trips, a rung with it.
    let mut ranked = set();
    ranked.target = "Help/z".to_owned();
    ranked.requirement = Some(ProtectionRequirement::Rung(Role::Admin));
    s.record_protection_event(&ranked).unwrap();
    assert_eq!(s.protection_event(&ranked.id).unwrap(), Some(ranked));
}

// ------------------------------------------------------------------------------------------------ tombstones

/// The events of one act (A-2) share its id, and with it one time, one person, one wiki and one kind: another time, another
/// person or the other kind under the same id is a different act and is refused; the act id comes back as it went in.
fn the_events_of_one_act_share_one_time_one_person_and_one_kind<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let (one, two, other) = (comment_item(), comment_item(), comment_item());
    let first = hide(w, &one, CONTENT, by, T0);
    s.record_tombstone_event(&first).unwrap();
    let same_act = |item: &HiddenItem, by: (&AccountId, Role), at: i64| {
        let mut e = hide(w, item, CONTENT, by, at);
        e.act_id = first.act_id.clone();
        e
    };
    let rule = "the events of one act are of one wiki, one time, one person and one kind";
    // Another second, another person, the other kind of event: not one act.
    assert!(backend(s.record_tombstone_event(&same_act(&two, by, T0 + 1))).contains(rule));
    assert!(
        backend(s.record_tombstone_event(&same_act(&two, (&r.admin, Role::Admin), T0)))
            .contains(rule)
    );
    let mut unhiding = unhide(&first, by, T0);
    unhiding.act_id = first.act_id.clone();
    assert!(backend(s.record_tombstone_event(&unhiding)).contains(rule));
    assert_eq!(s.tombstone_events_of(w, &two).unwrap(), []);
    // The same second and the same person under the id is the same act, and it is read back with it.
    let second = same_act(&two, by, T0);
    s.record_tombstone_event(&second).unwrap();
    assert_eq!(
        s.tombstone_event(&second.id).unwrap().map(|e| e.act_id),
        Some(first.act_id.clone())
    );
    // A hide of its own, the same second and person, is another act.
    let third = hide(w, &other, CONTENT, by, T0);
    assert_ne!(third.act_id, first.act_id);
    s.record_tombstone_event(&third).unwrap();
    let in_act: Vec<TombstoneEventId> = s
        .tombstone_log(w, None, 10)
        .unwrap()
        .into_iter()
        .filter(|e| e.act_id == first.act_id)
        .map(|e| e.id)
        .collect();
    assert_eq!(in_act, [second.id, first.id]);
}

fn a_tombstone_hides_and_an_unhide_restores_exactly<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let item = comment_item();
    assert_eq!(
        hidden_parts(&s.tombstone_events_of(w, &item).unwrap()),
        HiddenParts::default()
    );
    let mut first = hide(w, &item, CONTENT_AND_AUTHOR, by, T0);
    first.note = Some("Names a private person.".to_owned());
    first.affected = Some(r.person.clone());
    first.policy_block = Some(block(3));
    s.record_tombstone_event(&first).unwrap();
    assert_eq!(s.tombstone_event(&first.id).unwrap(), Some(first.clone()));
    assert_eq!(
        hidden_parts(&s.tombstone_events_of(w, &item).unwrap()),
        CONTENT_AND_AUTHOR
    );

    // An unhide follows that hide, of the same item and the same parts, once.
    let mut wrong_parts = unhide(&first, by, T0 + 1);
    wrong_parts.parts = CONTENT;
    assert!(
        backend(s.record_tombstone_event(&wrong_parts))
            .contains("an unhide follows a hide of the same item and parts")
    );
    let mut wrong_item = unhide(&first, by, T0 + 1);
    wrong_item.item = comment_item();
    assert!(backend(s.record_tombstone_event(&wrong_item)).contains("an unhide follows a hide"));
    let mut early = unhide(&first, by, T0 - 1);
    early.at = T0 - 1;
    assert!(backend(s.record_tombstone_event(&early)).contains("an unhide follows a hide"));
    let mut phantom = unhide(&first, by, T0 + 1);
    phantom.of_event = Some(TombstoneEventId::new());
    assert_eq!(
        s.record_tombstone_event(&phantom),
        Err(StoreError::NotFound)
    );
    let restore = unhide(&first, by, T0 + 2);
    s.record_tombstone_event(&restore).unwrap();
    assert_eq!(
        s.tombstone_events_of(w, &item).unwrap(),
        [first.clone(), restore.clone()]
    );
    assert_eq!(
        hidden_parts(&s.tombstone_events_of(w, &item).unwrap()),
        HiddenParts::default()
    );
    // Once: a hide has at most one unhide.
    assert_eq!(
        s.record_tombstone_event(&unhide(&first, by, T0 + 3)),
        Err(StoreError::AlreadyRecorded)
    );
    // A hide of the same item can follow, and two hides of different parts are hidden together until each is unhidden.
    let content_again = hide(w, &item, CONTENT, by, T0 + 4);
    let author_only = hide(
        w,
        &item,
        HiddenParts {
            content: false,
            summary: false,
            author: true,
        },
        by,
        T0 + 5,
    );
    s.record_tombstone_event(&content_again).unwrap();
    s.record_tombstone_event(&author_only).unwrap();
    assert_eq!(
        hidden_parts(&s.tombstone_events_of(w, &item).unwrap()),
        CONTENT_AND_AUTHOR
    );
    s.record_tombstone_event(&unhide(&content_again, by, T0 + 6))
        .unwrap();
    assert_eq!(
        hidden_parts(&s.tombstone_events_of(w, &item).unwrap()),
        HiddenParts {
            content: false,
            summary: false,
            author: true
        }
    );
    // Another item is untouched.
    assert!(
        s.tombstone_events_of(w, &comment_item())
            .unwrap()
            .is_empty()
    );
}

fn the_moderation_log_is_newest_first_and_pages_by_event<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let events: Vec<TombstoneEvent> = (0..5)
        .map(|n| hide(w, &comment_item(), CONTENT, by, T0 + n))
        .collect();
    for e in &events {
        s.record_tombstone_event(e).unwrap();
    }
    let newest_first: Vec<TombstoneEventId> = events.iter().rev().map(|e| e.id.clone()).collect();
    let ids = |v: Vec<TombstoneEvent>| v.into_iter().map(|e| e.id).collect::<Vec<_>>();
    assert_eq!(ids(s.tombstone_log(w, None, 10).unwrap()), newest_first);
    let page = s.tombstone_log(w, None, 2).unwrap();
    assert_eq!(ids(page.clone()), newest_first.get(..2).unwrap());
    let older = s.tombstone_log(w, page.last().map(|e| &e.id), 2).unwrap();
    assert_eq!(ids(older.clone()), newest_first.get(2..4).unwrap());
    assert_eq!(
        ids(s.tombstone_log(w, older.last().map(|e| &e.id), 10).unwrap()),
        newest_first.get(4..).unwrap()
    );
    // A cursor that is not an event of this wiki is not found; the log of another wiki is empty.
    assert_eq!(
        s.tombstone_log(w, Some(&TombstoneEventId::new()), 2),
        Err(StoreError::NotFound)
    );
    assert!(s.tombstone_log(&WikiId::new(), None, 2).unwrap().is_empty());
    assert_eq!(
        s.tombstone_log(w, None, 0).unwrap().len(),
        1,
        "at least one"
    );
}

fn the_items_whose_content_was_ever_hidden_are_listed_once_each_in_the_order_they_were_first_hidden<
    S,
>(
    r: &Rig<S>,
) where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let revision = |n: u64, reference: Option<&str>| HiddenItem {
        kind: HiddenItemKind::PageRevision,
        id: block(n),
        reference: reference.map(str::to_owned),
    };
    let ids = |limit: usize| {
        s.content_hidden_ids(w, HiddenItemKind::PageRevision, limit)
            .unwrap()
    };
    assert!(ids(10).is_empty(), "nothing hidden: nothing listed");
    // Block 7 is hidden per page (two events), block 5 once; the second of 7 is not a second entry.
    let seven_a = hide(w, &revision(7, Some("pages/A.md")), CONTENT, by, T0);
    let five = hide(w, &revision(5, None), CONTENT_AND_AUTHOR, by, T0 + 1);
    let seven_b = hide(w, &revision(7, Some("pages/B.md")), CONTENT, by, T0 + 2);
    for e in [&seven_a, &five, &seven_b] {
        s.record_tombstone_event(e).unwrap();
    }
    assert_eq!(ids(10), vec![block(7), block(5)]);
    assert_eq!(ids(1), vec![block(7)], "a limit cuts the list");
    assert_eq!(ids(0), vec![block(7)], "at least one");
    // A hide of the summary or the author alone is not a hide of the content; another kind and another wiki are not listed.
    let summary_only = HiddenParts {
        content: false,
        summary: true,
        author: false,
    };
    s.record_tombstone_event(&hide(w, &revision(9, None), summary_only, by, T0 + 3))
        .unwrap();
    s.record_tombstone_event(&hide(w, &comment_item(), CONTENT, by, T0 + 4))
        .unwrap();
    assert_eq!(ids(10), vec![block(7), block(5)]);
    assert!(
        s.content_hidden_ids(&WikiId::new(), HiddenItemKind::PageRevision, 10)
            .unwrap()
            .is_empty()
    );
    // An unhide does not take an item off the list: the list is a superset, what is hidden now is read from the events.
    s.record_tombstone_event(&unhide(&five, by, T0 + 5))
        .unwrap();
    assert_eq!(ids(10), vec![block(7), block(5)]);
}

fn a_tombstone_names_its_item_the_way_its_kind_does<S>(r: &Rig<S>)
where
    S: GovernanceStore + ProposalStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let proposal = |reference: Option<&str>| HiddenItem {
        kind: HiddenItemKind::ProposalRevision,
        id: ProposalId::new().to_string(),
        reference: reference.map(str::to_owned),
    };
    // A proposal's revision is named by the revision's number; a comment by its id alone.
    assert!(
        backend(s.record_tombstone_event(&hide(w, &proposal(None), CONTENT, by, T0)))
            .contains("revision's number")
    );
    assert!(
        backend(s.record_tombstone_event(&hide(w, &proposal(Some("one")), CONTENT, by, T0)))
            .contains("decimal digits")
    );
    let mut referenced = comment_item();
    referenced.reference = Some("1".to_owned());
    assert!(
        backend(s.record_tombstone_event(&hide(w, &referenced, CONTENT, by, T0)))
            .contains("identifier alone")
    );
    let revision = proposal(Some("2"));
    s.record_tombstone_event(&hide(w, &revision, CONTENT_AND_AUTHOR, by, T0))
        .unwrap();
    assert_eq!(s.tombstone_events_of(w, &revision).unwrap().len(), 1);
    assert!(
        s.tombstone_events_of(
            w,
            &HiddenItem {
                reference: Some("3".to_owned()),
                ..revision.clone()
            }
        )
        .unwrap()
        .is_empty(),
        "another revision of the proposal is another item"
    );
    // A page's revision is a block, and a page of it when the change touched several; the summary is a revision's.
    let page = HiddenItem {
        kind: HiddenItemKind::PageRevision,
        id: block(9),
        reference: Some("Help/x".to_owned()),
    };
    let summary_and_author = HiddenParts {
        content: false,
        summary: true,
        author: true,
    };
    s.record_tombstone_event(&hide(w, &page, summary_and_author, by, T0))
        .unwrap();
    assert_eq!(
        hidden_parts(&s.tombstone_events_of(w, &page).unwrap()),
        summary_and_author
    );
    let whole_block = HiddenItem {
        reference: None,
        ..page.clone()
    };
    assert!(s.tombstone_events_of(w, &whole_block).unwrap().is_empty());
    // A comment has no summary; something is hidden; an item is named by a plain id; a hide has a category.
    let summary = HiddenParts {
        content: false,
        summary: true,
        author: false,
    };
    assert!(
        backend(s.record_tombstone_event(&hide(w, &comment_item(), summary, by, T0)))
            .contains("no summary")
    );
    assert!(
        backend(s.record_tombstone_event(&hide(
            w,
            &comment_item(),
            HiddenParts::default(),
            by,
            T0
        )))
        .contains("at least one part")
    );
    let mut loud = comment_item();
    loud.id = "NOT AN ID".to_owned();
    assert!(
        backend(s.record_tombstone_event(&hide(w, &loud, CONTENT, by, T0))).contains("identifier")
    );
    let mut no_category = hide(w, &comment_item(), CONTENT, by, T0);
    no_category.category = None;
    assert!(backend(s.record_tombstone_event(&no_category)).contains("reason category"));
    let first = hide(w, &comment_item(), CONTENT, by, T0);
    s.record_tombstone_event(&first).unwrap();
    let mut with_category = unhide(&first, by, T0 + 1);
    with_category.category = Some(ReasonCategory::Spam);
    assert!(backend(s.record_tombstone_event(&with_category)).contains("no category of its own"));
    let mut too_long = hide(w, &comment_item(), CONTENT, by, T0);
    too_long.note = Some("x".repeat(MAX_REASON_BYTES + 1));
    assert!(backend(s.record_tombstone_event(&too_long)).contains("a note"));
    // Every category is kept as it is.
    for category in ReasonCategory::ALL {
        let mut e = hide(w, &comment_item(), CONTENT, by, T0);
        e.category = Some(category);
        s.record_tombstone_event(&e).unwrap();
        assert_eq!(
            s.tombstone_event(&e.id).unwrap().unwrap().category,
            Some(category)
        );
    }
    // The person the item is about is an account.
    let mut ghost = hide(w, &comment_item(), CONTENT, by, T0);
    ghost.affected = Some(AccountId::new());
    assert_eq!(
        s.record_tombstone_event(&ghost),
        Err(StoreError::AccountNotFound)
    );
}

fn a_hide_that_declined_a_proposal_names_the_decline<S>(r: &Rig<S>)
where
    S: GovernanceStore + ProposalStore + WikiStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let p = open_draft(s, w, &r.person, T0);
    let p = submit(s, &p, T0 + 1);
    // The proposal log: create (1), submit (2), decline (3).
    mv(
        s,
        &p.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &r.maintainer,
        T0 + 2,
    );
    let item = HiddenItem {
        kind: HiddenItemKind::ProposalRevision,
        id: p.id.to_string(),
        reference: Some("1".to_owned()),
    };
    let mut hidden = hide(w, &item, CONTENT, by, T0 + 3);
    hidden.declined_transition = Some(3);
    s.record_tombstone_event(&hidden).unwrap();
    assert_eq!(
        s.tombstone_event(&hidden.id)
            .unwrap()
            .unwrap()
            .declined_transition,
        Some(3)
    );
    // A position that is not in the log, or is another wiki's, is not found; only a revision's hide declines.
    let mut nowhere = hide(w, &item, CONTENT, by, T0 + 4);
    nowhere.declined_transition = Some(99);
    assert_eq!(
        s.record_tombstone_event(&nowhere),
        Err(StoreError::NotFound)
    );
    let other = wiki(s, "another");
    let mut foreign = hide(&other.id, &item, CONTENT, by, T0 + 4);
    foreign.declined_transition = Some(3);
    assert_eq!(
        s.record_tombstone_event(&foreign),
        Err(StoreError::NotFound)
    );
    let mut comment = hide(w, &comment_item(), CONTENT, by, T0 + 4);
    comment.declined_transition = Some(3);
    assert!(
        backend(s.record_tombstone_event(&comment))
            .contains("only the hiding of a proposal's revision")
    );
    // An unhide declines nothing.
    let mut restore = unhide(&hidden, by, T0 + 5);
    restore.declined_transition = Some(3);
    assert!(backend(s.record_tombstone_event(&restore)).contains("no category of its own"));
}

// ------------------------------------------------------------------------------------------------ appeals

fn an_appeal_is_filed_once_per_action_per_person_and_counted<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let set = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0,
        Some(T0 + 1000),
        None,
    );
    s.record_restriction_event(&set).unwrap();
    let id = set.id.to_string();
    let first = appeal(
        w,
        AppealedKind::Restriction,
        &id,
        Some(&r.maintainer),
        &r.person,
        T0 + 1,
    );
    s.file_appeal(&first).unwrap();
    assert_eq!(s.appeal(&first.id).unwrap(), Some(first.clone()));
    assert_eq!(s.open_appeal_count(w, &r.person).unwrap(), 1);

    // One open appeal per action per person; another person's own appeal of the same action is theirs.
    let second = appeal(
        w,
        AppealedKind::Restriction,
        &id,
        Some(&r.maintainer),
        &r.person,
        T0 + 2,
    );
    assert_eq!(s.file_appeal(&second), Err(StoreError::AlreadyRecorded));
    assert_eq!(
        s.file_appeal(&first),
        Err(StoreError::AlreadyRecorded),
        "an id is used once"
    );
    let by_admin = appeal(
        w,
        AppealedKind::Restriction,
        &id,
        Some(&r.maintainer),
        &r.admin,
        T0 + 3,
    );
    s.file_appeal(&by_admin).unwrap();
    // The same person may appeal another action.
    let other_action = appeal(
        w,
        AppealedKind::Protection,
        &ProtectionEventId::new().to_string(),
        None,
        &r.person,
        T0 + 4,
    );
    s.file_appeal(&other_action).unwrap();

    assert_eq!(s.open_appeal_count(w, &r.person).unwrap(), 2);
    assert_eq!(s.open_appeal_count(w, &r.admin).unwrap(), 1);
    assert_eq!(s.open_appeal_count(w, &r.maintainer).unwrap(), 0);
    let ids = |v: Vec<AppealRecord>| v.into_iter().map(|a| a.id).collect::<Vec<_>>();
    assert_eq!(
        ids(s.open_appeals(w, 10).unwrap()),
        [&first, &by_admin, &other_action].map(|a| a.id.clone()),
        "the queue is oldest first"
    );
    assert_eq!(s.open_appeals(w, 2).unwrap().len(), 2);
    assert_eq!(
        ids(s.recent_appeals(w, 10).unwrap()),
        [&other_action, &by_admin, &first].map(|a| a.id.clone()),
        "the wiki's appeals of every state, newest first"
    );
    assert_eq!(s.recent_appeals(w, 2).unwrap().len(), 2);
    assert!(s.recent_appeals(&WikiId::new(), 10).unwrap().is_empty());
    assert_eq!(
        ids(s.appeals_by(w, &r.person).unwrap()),
        [&other_action, &first].map(|a| a.id.clone()),
        "a person's own, newest first"
    );
    assert_eq!(
        ids(s.appeals_about(w, &first.action).unwrap()),
        [&first, &by_admin].map(|a| a.id.clone())
    );
    // The quotas count by time, for a person or for the wiki.
    assert_eq!(
        s.appeals_since(w, Some(&r.person), T0).unwrap(),
        [T0 + 1, T0 + 4]
    );
    assert_eq!(
        s.appeals_since(w, Some(&r.person), T0 + 2).unwrap(),
        [T0 + 4]
    );
    assert_eq!(
        s.appeals_since(w, None, T0).unwrap(),
        [T0 + 1, T0 + 3, T0 + 4]
    );
    assert!(
        s.appeals_since(&WikiId::new(), None, T0)
            .unwrap()
            .is_empty()
    );
    assert!(s.open_appeals(&WikiId::new(), 10).unwrap().is_empty());

    // Once it is decided it is not open, a person may appeal the same action again, and the count falls.
    s.decide_appeal(
        &first.id,
        &decision(&r.admin, T0 + 5, AppealOutcome::Upheld),
    )
    .unwrap();
    assert_eq!(s.open_appeal_count(w, &r.person).unwrap(), 1);
    let again = appeal(
        w,
        AppealedKind::Restriction,
        &id,
        Some(&r.maintainer),
        &r.person,
        T0 + 6,
    );
    s.file_appeal(&again).unwrap();
    assert_eq!(s.appeals_about(w, &first.action).unwrap().len(), 3);
}

fn a_malformed_appeal_is_refused<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let base = || {
        appeal(
            w,
            AppealedKind::Grant,
            &GrantDecisionId::new().to_string(),
            Some(&r.admin),
            &r.person,
            T0,
        )
    };
    for text in [String::new(), "x".repeat(MAX_APPEAL_BYTES + 1)] {
        let mut bad = base();
        bad.text = text;
        assert!(backend(s.file_appeal(&bad)).contains("an appeal"));
    }
    let mut at_bound = base();
    at_bound.text = "x".repeat(MAX_APPEAL_BYTES);
    s.file_appeal(&at_bound).unwrap();
    let mut decided = base();
    decided.state = AppealState::Upheld(Decided {
        at: T0,
        by: r.admin.clone(),
        reason: "x".to_owned(),
    });
    assert!(backend(s.file_appeal(&decided)).contains("filed open"));
    let mut bad_id = base();
    bad_id.action.id = "Not An Id".to_owned();
    assert!(backend(s.file_appeal(&bad_id)).contains("a record's id"));
    let mut elsewhere = base();
    elsewhere.wiki = WikiId::new();
    assert_eq!(s.file_appeal(&elsewhere), Err(StoreError::NotFound));
    let mut nobody = base();
    nobody.appellant = AccountId::new();
    assert_eq!(s.file_appeal(&nobody), Err(StoreError::AccountNotFound));
    let mut ghost = base();
    ghost.action_actor = Some(AccountId::new());
    assert_eq!(s.file_appeal(&ghost), Err(StoreError::AccountNotFound));
}

fn an_appeal_is_decided_once_by_someone_who_neither_appealed_nor_acted<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let set = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0,
        Some(T0 + 1000),
        None,
    );
    s.record_restriction_event(&set).unwrap();
    let filed = appeal(
        w,
        AppealedKind::Restriction,
        set.id.as_str(),
        Some(&r.maintainer),
        &r.person,
        T0 + 1,
    );
    s.file_appeal(&filed).unwrap();

    // Not the appellant, and not the one who took the action: the appeal stays open.
    for decider in [&r.person, &r.maintainer] {
        backend(s.decide_appeal(&filed.id, &decision(decider, T0 + 2, AppealOutcome::Upheld)));
        assert_eq!(
            s.appeal(&filed.id).unwrap().unwrap().state,
            AppealState::Open
        );
    }
    assert_eq!(
        s.decide_appeal(
            &filed.id,
            &decision(&AccountId::new(), T0 + 2, AppealOutcome::Upheld)
        ),
        Err(StoreError::AccountNotFound)
    );
    assert_eq!(
        s.decide_appeal(
            &AppealId::new(),
            &decision(&r.admin, T0 + 2, AppealOutcome::Upheld)
        ),
        Err(StoreError::NotFound)
    );
    assert!(
        backend(s.decide_appeal(
            &filed.id,
            &AppealDecision {
                reason: String::new(),
                ..decision(&r.admin, T0 + 2, AppealOutcome::Upheld)
            }
        ))
        .contains("decision's reason")
    );
    // Decided before it was filed is not a decision.
    backend(s.decide_appeal(
        &filed.id,
        &decision(&r.admin, T0 - 5, AppealOutcome::Upheld),
    ));
    assert_eq!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Open
    );

    s.decide_appeal(
        &filed.id,
        &decision(&r.admin, T0 + 3, AppealOutcome::Upheld),
    )
    .unwrap();
    assert_eq!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Upheld(Decided {
            at: T0 + 3,
            by: r.admin.clone(),
            reason: "Looked at it again.".to_owned(),
        })
    );
    // Decided once, for good; and the restriction stands.
    assert_eq!(
        s.decide_appeal(
            &filed.id,
            &decision(&r.admin, T0 + 4, AppealOutcome::Upheld)
        ),
        Err(StoreError::StateChanged)
    );
    assert_eq!(s.active_restrictions(w, &r.person, T0 + 10).unwrap(), [set]);
    assert!(s.open_appeals(w, 10).unwrap().is_empty());
}

fn an_overturned_restriction_is_reversed_in_the_same_step<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let set = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0,
        Some(T0 + 1000),
        None,
    );
    s.record_restriction_event(&set).unwrap();
    let filed = appeal(
        w,
        AppealedKind::Restriction,
        set.id.as_str(),
        Some(&r.maintainer),
        &r.person,
        T0 + 1,
    );
    s.file_appeal(&filed).unwrap();
    let admin = (&r.admin, Role::Admin);
    let reversal = |of: &RestrictionEvent, appeal_id: Option<&AppealId>| {
        let mut e = restriction(
            w,
            MeasureEvent::Reversed,
            &r.person,
            admin,
            T0 + 5,
            None,
            Some(of),
        );
        e.appeal = appeal_id.cloned();
        e
    };

    // Each wrong reversal is refused and changes nothing: not the appeal's own, another appeal, the wrong kind, made by
    // someone other than the decider, a position that something follows already.
    let someone_elses = reversal(&set, Some(&AppealId::new()));
    assert!(
        backend(s.decide_appeal(
            &filed.id,
            &decision(
                &r.admin,
                T0 + 5,
                overturn(Reversal::Restriction(someone_elses))
            )
        ))
        .contains("names the appeal it overturns")
    );
    let unnamed = reversal(&set, None);
    assert!(
        backend(s.decide_appeal(
            &filed.id,
            &decision(&r.admin, T0 + 5, overturn(Reversal::Restriction(unnamed)))
        ))
        .contains("names the appeal")
    );
    let wrong_kind = hide(w, &comment_item(), CONTENT, by, T0 + 5);
    backend(s.decide_appeal(
        &filed.id,
        &decision(
            &r.admin,
            T0 + 5,
            overturn(tombstone(TombstoneReversal::of(unhide(
                &wrong_kind,
                admin,
                T0 + 5,
            )))),
        ),
    ));
    let not_by_decider = reversal(&set, Some(&filed.id));
    backend(s.decide_appeal(
        &filed.id,
        &decision(
            &r.maintainer,
            T0 + 5,
            overturn(Reversal::Restriction(not_by_decider)),
        ),
    ));
    let lifted = restriction(
        w,
        MeasureEvent::Lifted,
        &r.person,
        admin,
        T0 + 4,
        None,
        Some(&set),
    );
    s.record_restriction_event(&lifted).unwrap();
    let followed = reversal(&set, Some(&filed.id));
    assert_eq!(
        s.decide_appeal(
            &filed.id,
            &decision(
                &r.admin,
                T0 + 5,
                overturn(Reversal::Restriction(followed.clone()))
            )
        ),
        Err(StoreError::AlreadyRecorded)
    );
    assert_eq!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Open
    );
    assert_eq!(
        s.restriction_event(&followed.id).unwrap(),
        None,
        "no reversal was written"
    );
    assert_eq!(s.restriction_events(w, &r.person).unwrap().len(), 2);

    // A restriction whose chain is open: the overturn writes the reversal and decides the appeal together.
    let set2 = restriction(
        w,
        MeasureEvent::Set,
        &r.person,
        by,
        T0 + 10,
        Some(T0 + 2000),
        None,
    );
    s.record_restriction_event(&set2).unwrap();
    let filed2 = appeal(
        w,
        AppealedKind::Restriction,
        set2.id.as_str(),
        Some(&r.maintainer),
        &r.person,
        T0 + 11,
    );
    s.file_appeal(&filed2).unwrap();
    assert_eq!(
        s.active_restrictions(w, &r.person, T0 + 12).unwrap(),
        vec![set2.clone()]
    );
    let mut reversed = restriction(
        w,
        MeasureEvent::Reversed,
        &r.person,
        admin,
        T0 + 12,
        None,
        Some(&set2),
    );
    reversed.appeal = Some(filed2.id.clone());
    s.decide_appeal(
        &filed2.id,
        &decision(
            &r.admin,
            T0 + 12,
            overturn(Reversal::Restriction(reversed.clone())),
        ),
    )
    .unwrap();
    assert!(
        s.active_restrictions(w, &r.person, T0 + 13)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        s.restriction_event(&reversed.id).unwrap(),
        Some(reversed.clone())
    );
    assert!(matches!(
        s.appeal(&filed2.id).unwrap().unwrap().state,
        AppealState::Overturned(_, ref record) if *record == reversed.id.to_string()
    ));
    // A reversal needs an appeal that is open: this one is decided now.
    let late = {
        let mut e = restriction(
            w,
            MeasureEvent::Reversed,
            &r.person,
            admin,
            T0 + 14,
            None,
            Some(&reversed),
        );
        e.appeal = Some(filed2.id.clone());
        e
    };
    backend(s.record_restriction_event(&late));
}

fn an_overturned_grant_restores_the_rung_and_a_stale_one_changes_nothing<S>(r: &Rig<S>)
where
    S: GovernanceStore + RoleStore + AccountStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let second_admin = account(s);
    let given = grant(
        w,
        Some((&r.admin, Role::Admin)),
        &r.person,
        None,
        Some(Role::Reviewer),
        T0,
    );
    s.record_grant_decision(&given).unwrap();
    let demotion = grant(
        w,
        Some((&r.admin, Role::Admin)),
        &r.person,
        Some(Role::Reviewer),
        None,
        T0 + 1,
    );
    s.record_grant_decision(&demotion).unwrap();
    let filed = appeal(
        w,
        AppealedKind::Grant,
        demotion.id.as_str(),
        Some(&r.admin),
        &r.person,
        T0 + 2,
    );
    s.file_appeal(&filed).unwrap();

    let restore = |before: Option<Role>| {
        let mut d = grant(
            w,
            Some((&second_admin, Role::Admin)),
            &r.person,
            before,
            Some(Role::Reviewer),
            T0 + 3,
        );
        d.appeal = Some(filed.id.clone());
        d.reason = Some("The appeal was right.".to_owned());
        d
    };
    // A rung before that is not the one held now: nothing is changed, the appeal stays open.
    assert_eq!(
        s.decide_appeal(
            &filed.id,
            &decision(
                &second_admin,
                T0 + 3,
                overturn(Reversal::Grant(restore(Some(Role::Maintainer))))
            )
        ),
        Err(StoreError::StateChanged)
    );
    assert_eq!(s.role(&r.person, w).unwrap(), None);
    assert_eq!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Open
    );
    // The one who took the action does not decide.
    let own = {
        let mut d = restore(None);
        d.by = Some(actor(&r.admin, Role::Admin));
        d
    };
    backend(s.decide_appeal(
        &filed.id,
        &decision(&r.admin, T0 + 3, overturn(Reversal::Grant(own))),
    ));
    assert_eq!(s.role(&r.person, w).unwrap(), None);

    let good = restore(None);
    s.decide_appeal(
        &filed.id,
        &decision(
            &second_admin,
            T0 + 3,
            overturn(Reversal::Grant(good.clone())),
        ),
    )
    .unwrap();
    assert_eq!(s.role(&r.person, w).unwrap(), Some(Role::Reviewer));
    assert_eq!(s.grant_decision(&good.id).unwrap(), Some(good.clone()));
    assert!(matches!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Overturned(_, ref record) if *record == good.id.to_string()
    ));
}

fn an_overturned_tombstone_and_protection_are_undone_with_the_decision<S>(r: &Rig<S>)
where
    S: GovernanceStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let admin = (&r.admin, Role::Admin);
    // A tombstone: the unhide is the reversal.
    let item = comment_item();
    let mut hidden = hide(w, &item, CONTENT, by, T0);
    hidden.affected = Some(r.person.clone());
    s.record_tombstone_event(&hidden).unwrap();
    let filed = appeal(
        w,
        AppealedKind::Tombstone,
        hidden.id.as_str(),
        Some(&r.maintainer),
        &r.person,
        T0 + 1,
    );
    s.file_appeal(&filed).unwrap();
    let mut restore = unhide(&hidden, admin, T0 + 2);
    restore.appeal = Some(filed.id.clone());
    s.decide_appeal(
        &filed.id,
        &decision(
            &r.admin,
            T0 + 2,
            overturn(tombstone(TombstoneReversal::of(restore.clone()))),
        ),
    )
    .unwrap();
    assert_eq!(
        hidden_parts(&s.tombstone_events_of(w, &item).unwrap()),
        HiddenParts::default()
    );
    assert_eq!(
        s.tombstone_event(&restore.id).unwrap().unwrap().appeal,
        Some(filed.id.clone())
    );
    // A hide cannot name an appeal.
    let mut hide_by_appeal = hide(w, &comment_item(), CONTENT, by, T0 + 3);
    hide_by_appeal.appeal = Some(filed.id.clone());
    assert!(backend(s.record_tombstone_event(&hide_by_appeal)).contains("follows nothing"));

    // A protection: the reversed event is the reversal.
    let set = protection(
        w,
        MeasureEvent::Set,
        ProtectionKind::Page,
        "Help/x",
        by,
        T0,
        Some(T0 + 1000),
        None,
    );
    s.record_protection_event(&set).unwrap();
    let filed = appeal(
        w,
        AppealedKind::Protection,
        set.id.as_str(),
        Some(&r.maintainer),
        &r.person,
        T0 + 4,
    );
    s.file_appeal(&filed).unwrap();
    let mut reversed = protection(
        w,
        MeasureEvent::Reversed,
        ProtectionKind::Page,
        "Help/x",
        admin,
        T0 + 5,
        None,
        Some(&set),
    );
    reversed.appeal = Some(filed.id.clone());
    assert_eq!(s.active_protections(w, T0 + 5).unwrap(), [set]);
    s.decide_appeal(
        &filed.id,
        &decision(&r.admin, T0 + 5, overturn(Reversal::Protection(reversed))),
    )
    .unwrap();
    assert!(s.active_protections(w, T0 + 6).unwrap().is_empty());
}

/// The reopening of a declined proposal, by `by`.
fn reopening(p: &ProposalId, from: ProposalState, by: &AccountId, at: i64) -> ProposalChange {
    ProposalChange {
        transition: step(
            p,
            ProposalOp::Reopen,
            Some(from),
            ProposalState::HeldForReview,
            by,
            Some("Looked at it again."),
            at,
        ),
        text: None,
        revision: None,
        superseded_by: None,
    }
}

fn an_overturned_decline_reopens_the_proposal_and_names_the_proposal_logs_own_record<S>(r: &Rig<S>)
where
    S: GovernanceStore + ProposalStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let p = open_draft(s, w, &r.person, T0);
    let p = submit(s, &p, T0 + 1);
    // The proposal log: create (1), submit (2), decline (3); the reopening is its own transition (4), written by the decision.
    mv(
        s,
        &p.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &r.maintainer,
        T0 + 2,
    );
    let filed = appeal(
        w,
        AppealedKind::Decline,
        "3",
        Some(&r.maintainer),
        &r.person,
        T0 + 3,
    );
    s.file_appeal(&filed).unwrap();
    let decide = |change: ProposalChange| {
        s.decide_appeal(
            &filed.id,
            &decision(
                &r.admin,
                T0 + 4,
                overturn(Reversal::Decline {
                    reopen: Box::new(change),
                }),
            ),
        )
    };
    let untouched = |why: &str, steps: usize| {
        assert_eq!(
            s.appeal(&filed.id).unwrap().unwrap().state,
            AppealState::Open,
            "{why}"
        );
        assert_eq!(
            s.proposal(&p.id, None).unwrap().unwrap().state,
            ProposalState::Declined,
            "{why}"
        );
        assert_eq!(s.transitions(&p.id, None).unwrap().len(), steps, "{why}");
    };
    // The reopening is of a proposal of this wiki.
    let elsewhere = ProposalId::new();
    assert_eq!(
        decide(reopening(
            &elsewhere,
            ProposalState::Declined,
            &r.admin,
            T0 + 4
        )),
        Err(StoreError::NotFound)
    );
    untouched("a proposal that is not there", 3);
    // It is the decider's, a reopening, out of declined, and nothing else.
    assert!(
        backend(decide(reopening(
            &p.id,
            ProposalState::Declined,
            &r.maintainer,
            T0 + 4
        )))
        .contains("made by the person who decided")
    );
    let mut with_text = reopening(&p.id, ProposalState::Declined, &r.admin, T0 + 4);
    with_text.text = Some(("Another summary".to_owned(), Vec::new()));
    assert!(backend(decide(with_text)).contains("changes nothing else"));
    let mut not_a_reopening = reopening(&p.id, ProposalState::Declined, &r.admin, T0 + 4);
    not_a_reopening.transition.op = ProposalOp::Decide;
    assert!(backend(decide(not_a_reopening)).contains("reopens a declined proposal"));
    untouched("a malformed reopening", 3);
    // A proposal that is not declined any more is the proposal's own StateChanged: nothing is decided.
    mv(
        s,
        &p.id,
        ProposalOp::Decide,
        ProposalState::Declined,
        ProposalState::HeldForReview,
        &r.admin,
        T0 + 4,
    );
    assert_eq!(
        decide(reopening(&p.id, ProposalState::Declined, &r.admin, T0 + 4)),
        Err(StoreError::StateChanged)
    );
    mv(
        s,
        &p.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &r.maintainer,
        T0 + 4,
    );
    untouched("a proposal that moved on", 5);
    // A decline is undone by reopening the proposal that it declined.
    let other = open_draft(s, w, &r.person, T0);
    let other = submit(s, &other, T0 + 1);
    mv(
        s,
        &other.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &r.maintainer,
        T0 + 2,
    );
    assert!(
        backend(decide(reopening(
            &other.id,
            ProposalState::Declined,
            &r.admin,
            T0 + 4
        )))
        .contains("reopening the proposal it declined")
    );
    untouched("another proposal's reopening", 5);
    // The decision itself is the last record, and refused (the decider is the one who declined): the reopening is not kept.
    backend(s.decide_appeal(
        &filed.id,
        &decision(
            &r.maintainer,
            T0 + 4,
            overturn(Reversal::Decline {
                reopen: Box::new(reopening(
                    &p.id,
                    ProposalState::Declined,
                    &r.maintainer,
                    T0 + 4,
                )),
            }),
        ),
    ));
    untouched("a decision refused after the reopening", 5);
    decide(reopening(&p.id, ProposalState::Declined, &r.admin, T0 + 4)).unwrap();
    let after = s.proposal(&p.id, None).unwrap().unwrap();
    assert_eq!(after.state, ProposalState::HeldForReview);
    assert_eq!(after.closed_at, None);
    let log = s.transitions(&p.id, None).unwrap();
    assert_eq!(log.last().map(|t| t.op), Some(ProposalOp::Reopen));
    let position = s.last_transition_position(&p.id).unwrap().unwrap();
    assert!(matches!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Overturned(_, ref record) if *record == position.to_string()
    ));
}

/// **An overturn and its decision are one transaction** (A-1): the unhide the appeal names, the unhides of the rest of its act and the
/// proposal's reopening land together or not at all.
fn an_overturned_hiding_writes_its_unhides_and_the_reopening_in_the_decision_or_not_at_all<S>(
    r: &Rig<S>,
) where
    S: GovernanceStore + ProposalStore,
{
    let (s, w) = (&r.store, &r.wiki);
    let by = (&r.maintainer, Role::Maintainer);
    let admin = (&r.admin, Role::Admin);
    let p = open_draft(s, w, &r.person, T0);
    let p = submit(s, &p, T0 + 1);
    mv(
        s,
        &p.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &r.maintainer,
        T0 + 2,
    );
    let (first, second, third) = (comment_item(), comment_item(), comment_item());
    let mut hides = Vec::new();
    for item in [&first, &second, &third] {
        let mut h = hide(w, item, CONTENT, by, T0 + 2);
        h.affected = Some(r.person.clone());
        s.record_tombstone_event(&h).unwrap();
        hides.push(h);
    }
    let [main, sibling, last] = hides.as_slice() else {
        panic!("three hides");
    };
    let filed = appeal(
        w,
        AppealedKind::Tombstone,
        main.id.as_str(),
        Some(&r.maintainer),
        &r.person,
        T0 + 3,
    );
    s.file_appeal(&filed).unwrap();
    let named = |e: TombstoneEvent| {
        let mut e = unhide(&e, admin, T0 + 4);
        e.appeal = Some(filed.id.clone());
        e
    };
    let reversal = |unhid_last: bool, reopen: ProposalChange| {
        let mut siblings = vec![named(sibling.clone())];
        if unhid_last {
            siblings.push(named(last.clone()));
        }
        overturn(tombstone(TombstoneReversal {
            main: named(main.clone()),
            siblings,
            reopen: Some(reopen),
        }))
    };
    let still_hidden = |what: &TombstoneEvent| {
        hidden_parts(&s.tombstone_events_of(w, &what.item).unwrap()) == CONTENT
    };
    let nothing_written = |why: &str, steps: usize| {
        assert!(
            still_hidden(main) && still_hidden(sibling) && still_hidden(last),
            "{why}"
        );
        assert_eq!(
            s.appeal(&filed.id).unwrap().unwrap().state,
            AppealState::Open,
            "{why}"
        );
        assert_eq!(
            s.proposal(&p.id, None).unwrap().unwrap().state,
            ProposalState::Declined,
            "{why}"
        );
        assert_eq!(s.transitions(&p.id, None).unwrap().len(), steps, "{why}");
    };
    let reopen = |at: i64| reopening(&p.id, ProposalState::Declined, &r.admin, at);

    // The last record refused (the proposal is not declined any more): the unhides before it are not left written.
    mv(
        s,
        &p.id,
        ProposalOp::Decide,
        ProposalState::Declined,
        ProposalState::HeldForReview,
        &r.admin,
        T0 + 3,
    );
    assert_eq!(
        s.decide_appeal(
            &filed.id,
            &decision(&r.admin, T0 + 4, reversal(false, reopen(T0 + 4)))
        ),
        Err(StoreError::StateChanged)
    );
    mv(
        s,
        &p.id,
        ProposalOp::Decline,
        ProposalState::HeldForReview,
        ProposalState::Declined,
        &r.maintainer,
        T0 + 3,
    );
    nothing_written("the reopening refused", 5);

    // A moderator unhid one of the act's hides a moment ago: the decision is refused whole, and the main unhide is not written.
    let by_hand = unhide(last, by, T0 + 3);
    s.record_tombstone_event(&by_hand).unwrap();
    assert_eq!(
        s.decide_appeal(
            &filed.id,
            &decision(&r.admin, T0 + 4, reversal(true, reopen(T0 + 4)))
        ),
        Err(StoreError::AlreadyRecorded)
    );
    assert!(still_hidden(main) && still_hidden(sibling));
    assert_eq!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Open
    );
    assert_eq!(
        s.proposal(&p.id, None).unwrap().unwrap().state,
        ProposalState::Declined
    );
    assert_eq!(s.transitions(&p.id, None).unwrap().len(), 5);

    // An unhide among them that does not name this appeal, or is not the decider's, is refused before anything is written.
    let mut unnamed = named(sibling.clone());
    unnamed.appeal = None;
    let mixed = overturn(tombstone(TombstoneReversal {
        main: named(main.clone()),
        siblings: vec![unnamed],
        reopen: Some(reopen(T0 + 4)),
    }));
    assert!(
        backend(s.decide_appeal(&filed.id, &decision(&r.admin, T0 + 4, mixed)))
            .contains("one appeal")
    );
    let mut by_someone_else = named(sibling.clone());
    by_someone_else.by = actor(&r.maintainer, Role::Maintainer);
    let theirs = overturn(tombstone(TombstoneReversal {
        main: named(main.clone()),
        siblings: vec![by_someone_else],
        reopen: None,
    }));
    assert!(
        backend(s.decide_appeal(&filed.id, &decision(&r.admin, T0 + 4, theirs)))
            .contains("made by the person who decided")
    );
    assert!(still_hidden(main) && still_hidden(sibling));

    // All of it, together.
    s.decide_appeal(
        &filed.id,
        &decision(&r.admin, T0 + 4, reversal(false, reopen(T0 + 4))),
    )
    .unwrap();
    assert!(!still_hidden(main) && !still_hidden(sibling));
    assert_eq!(
        s.proposal(&p.id, None).unwrap().unwrap().state,
        ProposalState::HeldForReview
    );
    assert_eq!(s.transitions(&p.id, None).unwrap().len(), 6);
    assert!(matches!(
        s.appeal(&filed.id).unwrap().unwrap().state,
        AppealState::Overturned(..)
    ));
}

// ------------------------------------------------------------------------------------------------ the snapshot

fn the_latest_governance_snapshot_is_the_one_put_last_of_the_wiki_whatever_its_tip<S>(r: &Rig<S>)
where
    S: GovernanceSnapshotStore + DerivedStore,
{
    let (s, w) = (&r.store, &r.wiki);
    assert_eq!(s.latest_governance_snapshot(w).unwrap(), None);
    let snapshot = |n: u64| GovernanceSnapshot {
        policy_block: Some(block(n)),
        policy: Some(format!("entry: observer\n# {n}\n")),
        ..GovernanceSnapshot::default()
    };
    // The order is the order they were put in, not the order of the tips' names, and the same second breaks to the later.
    s.put_governance_snapshot(w, "tip-z", &snapshot(1), T0)
        .unwrap();
    s.put_governance_snapshot(w, "tip-a", &snapshot(2), T0)
        .unwrap();
    assert_eq!(
        s.latest_governance_snapshot(w).unwrap(),
        Some(("tip-a".to_owned(), snapshot(2)))
    );
    // A tip put again is the latest again.
    s.put_governance_snapshot(w, "tip-z", &snapshot(3), T0)
        .unwrap();
    assert_eq!(
        s.latest_governance_snapshot(w).unwrap(),
        Some(("tip-z".to_owned(), snapshot(3)))
    );
    // Another wiki's snapshots are another wiki's.
    let other = WikiId::new();
    assert_eq!(s.latest_governance_snapshot(&other).unwrap(), None);
    // Dropped with the rest of the derived rows: the older one when its tip is no longer kept, all of them when cleared.
    assert_eq!(s.retain_tips(w, &["tip-a".to_owned()]).unwrap(), 1);
    assert_eq!(
        s.latest_governance_snapshot(w).unwrap(),
        Some(("tip-a".to_owned(), snapshot(2)))
    );
    s.clear(w).unwrap();
    assert_eq!(s.latest_governance_snapshot(w).unwrap(), None);
}

fn the_governance_snapshot_is_derived_one_row_per_tip_and_dropped_with_the_rest<S>(r: &Rig<S>)
where
    S: GovernanceSnapshotStore + DerivedStore,
{
    let (s, w) = (&r.store, &r.wiki);
    assert_eq!(s.governance_snapshot(w, "tip-a").unwrap(), None);
    // A wiki with no governance files is a snapshot of nothing, kept all the same.
    s.put_governance_snapshot(w, "tip-a", &GovernanceSnapshot::default(), T0)
        .unwrap();
    assert_eq!(
        s.governance_snapshot(w, "tip-a").unwrap(),
        Some(GovernanceSnapshot::default())
    );
    let full = GovernanceSnapshot {
        policy_block: Some(block(5)),
        constitution: Some("policy:\n  rung: maintainer\n".to_owned()),
        policy: Some("entry: observer\n".to_owned()),
        fault: Some(GovernanceFault {
            file: "governance/policy.yaml".to_owned(),
            line: Some(3),
            rule: "unknown-key".to_owned(),
            found: Vec::new(),
            found_more: 0,
        }),
    };
    s.put_governance_snapshot(w, "tip-b", &full, T0).unwrap();
    assert_eq!(
        s.governance_snapshot(w, "tip-b").unwrap(),
        Some(full.clone())
    );
    let whole_file = GovernanceSnapshot {
        fault: Some(GovernanceFault {
            file: "governance/constitution.yaml".to_owned(),
            line: None,
            rule: "not-a-file".to_owned(),
            found: vec![
                "governance/constitution.yaml/a.txt".to_owned(),
                "governance/constitution.yaml/b/c.txt".to_owned(),
            ],
            found_more: 7,
        }),
        ..GovernanceSnapshot::default()
    };
    s.put_governance_snapshot(w, "tip-b", &whole_file, T0 + 1)
        .unwrap();
    assert_eq!(
        s.governance_snapshot(w, "tip-b").unwrap(),
        Some(whole_file),
        "a snapshot replaces the one of its tip"
    );
    s.put_governance_snapshot(w, "tip-b", &full, T0 + 2)
        .unwrap();
    let other = WikiId::new();
    assert_eq!(s.governance_snapshot(&other, "tip-b").unwrap(), None);

    // Dropped with the rest of the derived rows: by tip, and as a whole.
    assert_eq!(s.retain_tips(w, &["tip-b".to_owned()]).unwrap(), 1);
    assert_eq!(s.governance_snapshot(w, "tip-a").unwrap(), None);
    assert_eq!(
        s.governance_snapshot(w, "tip-b").unwrap(),
        Some(full.clone())
    );
    assert_eq!(s.clear(w).unwrap(), 1);
    assert_eq!(s.governance_snapshot(w, "tip-b").unwrap(), None);
    assert_eq!(s.clear(w).unwrap(), 0);

    // The shapes: a block id, a line from one, a file within the bound, names within theirs.
    let refuse = |snapshot: GovernanceSnapshot| {
        backend(s.put_governance_snapshot(w, "tip-c", &snapshot, T0))
    };
    assert!(
        refuse(GovernanceSnapshot {
            policy_block: Some("xyz".to_owned()),
            ..GovernanceSnapshot::default()
        })
        .contains("block id")
    );
    assert!(
        refuse(GovernanceSnapshot {
            policy: Some("x".repeat(MAX_SNAPSHOT_FILE_BYTES + 1)),
            ..GovernanceSnapshot::default()
        })
        .contains("1 MiB")
    );
    let fault = |file: &str, line: Option<u32>, rule: &str| GovernanceSnapshot {
        fault: Some(GovernanceFault {
            file: file.to_owned(),
            line,
            rule: rule.to_owned(),
            found: Vec::new(),
            found_more: 0,
        }),
        ..GovernanceSnapshot::default()
    };
    assert!(refuse(fault("f", Some(0), "r")).contains("from 1"));
    assert!(refuse(fault("", None, "r")).contains("fault's file"));
    assert!(refuse(fault("f", None, "")).contains("fault's rule"));
    // What a fault lists (E-1): at most 50 paths, each of 1 - 1024 bytes, and a count only beside a list.
    let listing = |found: Vec<String>, more: u32| GovernanceSnapshot {
        fault: Some(GovernanceFault {
            file: "governance/policy.yaml".to_owned(),
            line: None,
            rule: "not-a-file".to_owned(),
            found,
            found_more: more,
        }),
        ..GovernanceSnapshot::default()
    };
    let path = |n: usize| format!("governance/policy.yaml/{n}");
    assert!(refuse(listing((0..51).map(path).collect(), 0)).contains("at most 50"));
    assert!(refuse(listing(vec![String::new()], 0)).contains("a path a fault lists"));
    assert!(refuse(listing(vec!["p".repeat(1025)], 0)).contains("a path a fault lists"));
    assert!(refuse(listing(Vec::new(), 3)).contains("lists some"));
    let fifty = listing((0..50).map(path).collect(), 12);
    s.put_governance_snapshot(w, "tip-d", &fifty, T0).unwrap();
    assert_eq!(s.governance_snapshot(w, "tip-d").unwrap(), Some(fifty));
    // Names that are not plain text come back as they went in (the list is JSON in the row).
    let odd = listing(
        vec![
            "governance/policy.yaml/a \"b\"\n\u{202e}c.txt".to_owned(),
            "governance/policy.yaml/日本語.txt".to_owned(),
        ],
        0,
    );
    s.put_governance_snapshot(w, "tip-e", &odd, T0).unwrap();
    assert_eq!(s.governance_snapshot(w, "tip-e").unwrap(), Some(odd));
    assert!(refuse(fault(&"f".repeat(257), None, "r")).contains("fault's file"));
    assert_eq!(s.governance_snapshot(w, "tip-c").unwrap(), None);
    let at_bound = GovernanceSnapshot {
        policy: Some("x".repeat(MAX_SNAPSHOT_FILE_BYTES)),
        ..GovernanceSnapshot::default()
    };
    s.put_governance_snapshot(w, "tip-c", &at_bound, T0)
        .unwrap();
    assert_eq!(s.governance_snapshot(w, "tip-c").unwrap(), Some(at_bound));
}

// ------------------------------------------------------------------------------------------------ both stores

macro_rules! both_stores {
    ($($name:ident),+ $(,)?) => {
        mod memory {
            use super::*;
            $(#[test] fn $name() { super::$name(&rig(MemoryStore::new())); })+
        }
        mod sqlite {
            use super::*;
            $(#[test] fn $name() { super::$name(&rig(SqliteStore::open_in_memory().expect("a store"))); })+
        }
    };
}

both_stores!(
    grants_are_recorded_and_applied_in_one_step,
    a_malformed_or_unknown_grant_decision_is_refused,
    a_restriction_ends_by_the_clock_and_its_chain_is_a_line,
    the_restrictions_in_force_in_a_wiki_are_read_in_one_query_and_end_by_the_clock,
    a_restriction_chain_keeps_to_its_account_and_its_time,
    a_temporary_protection_ends_by_the_clock_and_is_found_by_exactly_its_target,
    a_protection_event_is_well_formed_or_refused,
    a_tombstone_hides_and_an_unhide_restores_exactly,
    the_events_of_one_act_share_one_time_one_person_and_one_kind,
    the_moderation_log_is_newest_first_and_pages_by_event,
    the_items_whose_content_was_ever_hidden_are_listed_once_each_in_the_order_they_were_first_hidden,
    a_tombstone_names_its_item_the_way_its_kind_does,
    a_hide_that_declined_a_proposal_names_the_decline,
    an_appeal_is_filed_once_per_action_per_person_and_counted,
    a_malformed_appeal_is_refused,
    an_appeal_is_decided_once_by_someone_who_neither_appealed_nor_acted,
    an_overturned_restriction_is_reversed_in_the_same_step,
    an_overturned_grant_restores_the_rung_and_a_stale_one_changes_nothing,
    an_overturned_tombstone_and_protection_are_undone_with_the_decision,
    an_overturned_decline_reopens_the_proposal_and_names_the_proposal_logs_own_record,
    an_overturned_hiding_writes_its_unhides_and_the_reopening_in_the_decision_or_not_at_all,
    the_governance_snapshot_is_derived_one_row_per_tip_and_dropped_with_the_rest,
    the_latest_governance_snapshot_is_the_one_put_last_of_the_wiki_whatever_its_tip,
);
