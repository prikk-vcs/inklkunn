//! A world for the appeal tests: one wiki under chosen rules, the real authorizer over the store's grants and restrictions with a
//! clock the test sets, the appeal service, and builders for each of the five kinds of action an appeal is against. Built over
//! either store, so each test body runs on both.

use std::cell::Cell;
use std::sync::Arc;

use inklkunn_store::{
    AcceptanceStore, Account, AccountId, AccountStore, Actor, AppealId, AuditEvent, Base,
    ClaimThreadStore, CommentId, DisplayName, FileChange, GovernanceStore, GrantDecisionId,
    GrantDecisionRecord, HiddenItem, HiddenItemKind, HiddenParts, InboxStore, MeasureEvent,
    MemoryAudit, MemoryStore, NewWiki, ProposalChange, ProposalId, ProposalOp, ProposalRecord,
    ProposalState, ProposalStore, ProposalTransition, ProtectionEvent, ProtectionEventId,
    ProtectionKind, ProtectionRequirement, ReasonCategory, RestrictionEvent, RestrictionEventId,
    RevisionContent, RevisionRecord, Role, RoleGrant, RoleStore, Slug, SqliteStore, TombstoneAct,
    TombstoneActId, TombstoneEvent, TombstoneEventId, Visibility, WikiRecord, WikiStore,
};

use crate::appeal::{AppealLimits, AppealService, ContestRef};
use crate::authorize::{
    Authorizer, AuthzSource, IdentityAuthority, Principal, RulesSource, SourceError, StoreAuthz,
};
use crate::governance::{
    Constitution, GovernanceFile, PlatformLimits, ReadFault, Refusal, Rules, read_policy,
};
use crate::tombstone::Tombstones;

thread_local! {
    static NOW: Cell<i64> = const { Cell::new(0) };
}

/// The time every test starts at.
pub(super) const T0: i64 = 10_000_000;

/// One day, in seconds.
pub(super) const DAY: i64 = 86_400;

pub(super) fn now() -> i64 {
    NOW.with(Cell::get)
}

pub(super) fn set_now(t: i64) {
    NOW.with(|n| n.set(t));
}

/// A clock every thread reads alike: the time every test starts at, which does not move.
pub(super) fn steady() -> i64 {
    T0
}

/// The block the rules of a world with a policy file were set by.
pub(super) const BLOCK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A store that is every kind of store the services read.
pub(super) trait Everything:
    ProposalStore
    + AcceptanceStore
    + InboxStore
    + ClaimThreadStore
    + WikiStore
    + RoleStore
    + AccountStore
    + GovernanceStore
{
}

impl<T> Everything for T where
    T: ProposalStore
        + AcceptanceStore
        + InboxStore
        + ClaimThreadStore
        + WikiStore
        + RoleStore
        + AccountStore
        + GovernanceStore
{
}

/// The rules a test fixes.
struct Fixed(Arc<Rules>);

impl RulesSource for Fixed {
    fn rules(&self, _: &inklkunn_store::WikiId) -> Result<Arc<Rules>, SourceError> {
        Ok(self.0.clone())
    }
}

/// A person: the account, and a session principal for it.
#[derive(Clone)]
pub(super) struct Person {
    pub(super) id: AccountId,
    pub(super) principal: Principal,
    name: String,
}

impl Person {
    /// The name the account is shown by.
    pub(super) fn display(&self) -> &str {
        &self.name
    }
}

/// Which store a world is built over.
#[derive(Debug, Clone, Copy)]
pub(super) enum Kind {
    Memory,
    Sqlite,
}

pub(super) const KINDS: [Kind; 2] = [Kind::Memory, Kind::Sqlite];

pub(super) struct World {
    pub(super) appeals: AppealService,
    pub(super) authorizer: Authorizer,
    pub(super) audit: Arc<MemoryAudit>,
    pub(super) store: Arc<dyn Everything>,
    pub(super) wiki: WikiRecord,
    pub(super) authority: IdentityAuthority,
    pub(super) tombstones: Arc<Tombstones>,
}

impl World {
    /// A public wiki under the default rules.
    pub(super) fn new(kind: Kind) -> World {
        World::with(kind, Visibility::Public, None, AppealLimits::default(), now)
    }

    /// A public wiki under the policy `policy` (a policy file's text).
    pub(super) fn under(kind: Kind, policy: &str) -> World {
        World::with(
            kind,
            Visibility::Public,
            Some(policy),
            AppealLimits::default(),
            now,
        )
    }

    /// The same, with the limits and the clock a test needs.
    pub(super) fn with(
        kind: Kind,
        visibility: Visibility,
        policy: Option<&str>,
        limits: AppealLimits,
        clock: fn() -> i64,
    ) -> World {
        let platform = PlatformLimits::default();
        let rules = Arc::new(match policy {
            Some(text) => Rules::read(
                Some(BLOCK.to_owned()),
                read_policy(text.as_bytes()).expect("a policy"),
                Constitution::default(),
                &platform,
            ),
            None => Rules::defaults(),
        });
        World::ruled(kind, visibility, rules, limits, clock)
    }

    /// A public wiki one of whose governance files cannot be read: every write refuses.
    pub(super) fn faulted(kind: Kind) -> World {
        let rules = Arc::new(Rules::faulted(
            Some(BLOCK.to_owned()),
            ReadFault::from_stored(GovernanceFile::Policy, Some(4), Refusal::UnknownKey),
        ));
        World::ruled(
            kind,
            Visibility::Public,
            rules,
            AppealLimits::default(),
            now,
        )
    }

    fn ruled(
        kind: Kind,
        visibility: Visibility,
        rules: Arc<Rules>,
        limits: AppealLimits,
        clock: fn() -> i64,
    ) -> World {
        match kind {
            Kind::Memory => World::over(
                Arc::new(MemoryStore::new()),
                visibility,
                rules,
                limits,
                clock,
            ),
            Kind::Sqlite => World::over(
                Arc::new(SqliteStore::open_in_memory().expect("a store")),
                visibility,
                rules,
                limits,
                clock,
            ),
        }
    }

    fn over<S: Everything + 'static>(
        store: Arc<S>,
        visibility: Visibility,
        rules: Arc<Rules>,
        limits: AppealLimits,
        clock: fn() -> i64,
    ) -> World {
        set_now(T0);
        let mut new = NewWiki::new(
            Slug::new("appeals").expect("slug"),
            DisplayName::new("Appeals").expect("name"),
        );
        new.visibility = visibility;
        let wiki = store.create(new).expect("a wiki");
        let platform = PlatformLimits::default();
        let audit = Arc::new(MemoryAudit::new());
        let authz = StoreAuthz::new(store.clone(), store.clone())
            .with_rules(Arc::new(Fixed(rules)))
            .with_restrictions(store.clone());
        let source: Arc<dyn AuthzSource> = Arc::new(authz);
        let authorizer = Authorizer::new(source, audit.clone())
            .with_clock(clock)
            .with_protections(store.clone(), platform);
        let tombstones = Arc::new(Tombstones::new(store.clone(), store.clone()));
        let appeals = AppealService::new(
            authorizer.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            tombstones.clone(),
            audit.clone(),
            limits,
        );
        World {
            appeals,
            authorizer,
            audit,
            store,
            wiki,
            authority: IdentityAuthority::for_tests(),
            tombstones,
        }
    }

    /// A new account named `name` holding `role` in the wiki through a grant (none: no grant).
    pub(super) fn person(&self, name: &str, role: Option<Role>) -> Person {
        let id = AccountId::new();
        self.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new(name).expect("a name"),
                local: None,
                created_at: 0,
                disabled: false,
            })
            .expect("an account");
        if let Some(role) = role {
            self.store
                .grant(RoleGrant {
                    account: id.clone(),
                    wiki: self.wiki.id.clone(),
                    role,
                    granted_by: None,
                    granted_at: 0,
                })
                .expect("a grant");
        }
        let principal = Principal::session(&self.authority, id.clone());
        Person {
            id,
            principal,
            name: name.to_owned(),
        }
    }

    /// The rung `who` holds in the wiki through a grant now.
    pub(super) fn grant_of(&self, who: &Person) -> Option<Role> {
        self.store.role(&who.id, &self.wiki.id).expect("a role")
    }

    /// The audit events named `act`, oldest first.
    pub(super) fn events(&self, act: &str) -> Vec<AuditEvent> {
        self.audit
            .events()
            .into_iter()
            .filter(|e| e.action == act)
            .collect()
    }

    // ------------------------------------------------------------------------------------------ the five kinds

    /// A proposal of `author`, held for review, whose latest revision writes the page `title`.
    pub(super) fn proposal(&self, author: &Person, title: &str) -> ProposalId {
        let record = ProposalRecord {
            id: ProposalId::new(),
            wiki: self.wiki.id.clone(),
            view: "heads/main".to_owned(),
            state: ProposalState::Draft,
            author: author.id.clone(),
            summary: "Fix a typo".to_owned(),
            sources: Vec::new(),
            superseded_by: None,
            created_at: T0 - 1000,
            updated_at: T0 - 1000,
            submitted_at: None,
            closed_at: None,
        };
        let content = |text: &str| RevisionContent {
            base: Base::new(&format!("{:064x}", 1_u64), &format!("{:064x}", 0xb10c_u64))
                .expect("a base"),
            files: vec![FileChange {
                path: format!("pages/{title}.md"),
                text: Some(text.to_owned()),
            }],
            renames: Vec::new(),
            minor: false,
            automated: false,
        };
        self.store
            .create_proposal(
                &record,
                &content("draft"),
                &ProposalTransition {
                    proposal: record.id.clone(),
                    op: ProposalOp::Create,
                    from: None,
                    to: ProposalState::Draft,
                    actor: author.id.clone(),
                    reason: None,
                    at: T0 - 1000,
                },
            )
            .expect("a proposal");
        self.store
            .change_proposal(&ProposalChange {
                transition: ProposalTransition {
                    proposal: record.id.clone(),
                    op: ProposalOp::Submit,
                    from: Some(ProposalState::Draft),
                    to: ProposalState::HeldForReview,
                    actor: author.id.clone(),
                    reason: None,
                    at: T0 - 999,
                },
                text: None,
                revision: Some(RevisionRecord {
                    proposal: record.id.clone(),
                    seq: 1,
                    content: content("text"),
                    created_at: T0 - 999,
                }),
                superseded_by: None,
            })
            .expect("submitted");
        record.id
    }

    /// The state of a proposal.
    pub(super) fn state_of(&self, proposal: &ProposalId) -> ProposalState {
        self.store
            .proposal(proposal, None)
            .expect("a read")
            .expect("a proposal")
            .state
    }

    /// Decline a proposal as `by`, and say where the decline stands in the proposal log: **the id an appeal against it names**.
    pub(super) fn decline(&self, proposal: &ProposalId, by: &Person) -> ContestRef {
        self.store
            .change_proposal(&ProposalChange {
                transition: ProposalTransition {
                    proposal: proposal.clone(),
                    op: ProposalOp::Decline,
                    from: Some(ProposalState::HeldForReview),
                    to: ProposalState::Declined,
                    actor: by.id.clone(),
                    reason: Some("Not now.".to_owned()),
                    at: T0 - 500,
                },
                text: None,
                revision: None,
                superseded_by: None,
            })
            .expect("declined");
        let position = self
            .store
            .last_transition_position(proposal)
            .expect("a read")
            .expect("a position");
        ContestRef {
            kind: inklkunn_store::AppealedKind::Decline,
            id: position.to_string(),
        }
    }

    /// A restriction of `who` set by `by` now, until `days` days from now.
    pub(super) fn restrict(&self, by: &Person, who: &Person, days: i64) -> ContestRef {
        let id = RestrictionEventId::new();
        self.store
            .record_restriction_event(&RestrictionEvent {
                id: id.clone(),
                wiki: self.wiki.id.clone(),
                at: now(),
                event: MeasureEvent::Set,
                account: who.id.clone(),
                by: Actor {
                    account: by.id.clone(),
                    rung: Role::Maintainer,
                },
                policy_block: None,
                reason: Some("Edit-warring.".to_owned()),
                expires_at: Some(now() + days * DAY),
                of_event: None,
                appeal: None,
            })
            .expect("a restriction");
        ContestRef {
            kind: inklkunn_store::AppealedKind::Restriction,
            id: id.to_string(),
        }
    }

    /// Renew the restriction `of` (an event id) until `days` days from now.
    pub(super) fn renew(
        &self,
        by: &Person,
        who: &Person,
        of: &ContestRef,
        days: i64,
    ) -> ContestRef {
        let id = RestrictionEventId::new();
        self.store
            .record_restriction_event(&RestrictionEvent {
                id: id.clone(),
                wiki: self.wiki.id.clone(),
                at: now(),
                event: MeasureEvent::Renewed,
                account: who.id.clone(),
                by: Actor {
                    account: by.id.clone(),
                    rung: Role::Maintainer,
                },
                policy_block: None,
                reason: Some("Still edit-warring.".to_owned()),
                expires_at: Some(now() + days * DAY),
                of_event: RestrictionEventId::parse(&of.id),
                appeal: None,
            })
            .expect("a renewal");
        ContestRef {
            kind: inklkunn_store::AppealedKind::Restriction,
            id: id.to_string(),
        }
    }

    /// A temporary protection of the page `title`, set by `by` now, for `days` days.
    pub(super) fn protect(&self, by: &Person, title: &str, days: i64) -> ContestRef {
        let id = ProtectionEventId::new();
        self.store
            .record_protection_event(&ProtectionEvent {
                id: id.clone(),
                wiki: self.wiki.id.clone(),
                at: now(),
                event: MeasureEvent::Set,
                kind: ProtectionKind::Page,
                target: title.to_owned(),
                requirement: Some(ProtectionRequirement::Review),
                by: Actor {
                    account: by.id.clone(),
                    rung: Role::Maintainer,
                },
                policy_block: None,
                reason: Some("Vandalism.".to_owned()),
                expires_at: Some(now() + days * DAY),
                of_event: None,
                appeal: None,
            })
            .expect("a protection");
        ContestRef {
            kind: inklkunn_store::AppealedKind::Protection,
            id: id.to_string(),
        }
    }

    /// A hiding of the body of a comment of `author`, by `by`, now.
    pub(super) fn hide(&self, by: &Person, author: &Person) -> (ContestRef, CommentId) {
        let comment = CommentId::new();
        let id = self.hide_item(by, Some(author), &comment, now());
        (
            ContestRef {
                kind: inklkunn_store::AppealedKind::Tombstone,
                id: id.to_string(),
            },
            comment,
        )
    }

    /// A hide of a comment `comment`, recorded at `at`.
    pub(super) fn hide_item(
        &self,
        by: &Person,
        author: Option<&Person>,
        comment: &CommentId,
        at: i64,
    ) -> TombstoneEventId {
        let id = TombstoneEventId::new();
        self.store
            .record_tombstone_event(&TombstoneEvent {
                id: id.clone(),
                wiki: self.wiki.id.clone(),
                at,
                act: TombstoneAct::Hide,
                act_id: TombstoneActId::new(),
                item: HiddenItem {
                    kind: HiddenItemKind::ProposalComment,
                    id: comment.to_string(),
                    reference: None,
                },
                parts: HiddenParts {
                    content: true,
                    summary: false,
                    author: false,
                },
                category: Some(ReasonCategory::Harassment),
                note: None,
                affected: author.map(|a| a.id.clone()),
                by: Actor {
                    account: by.id.clone(),
                    rung: Role::Maintainer,
                },
                policy_block: None,
                of_event: None,
                appeal: None,
                declined_transition: None,
            })
            .expect("a hide");
        id
    }

    /// Is the comment's body hidden now?
    pub(super) fn hidden(&self, comment: &CommentId) -> bool {
        self.tombstones
            .proposal_comment(&self.wiki.id, comment)
            .expect("a read")
            .content()
            .is_some()
    }

    /// A decision of `by` (none: the operator) that changed `target`'s role from `before` to `after`, applied.
    pub(super) fn regrade(
        &self,
        by: Option<&Person>,
        target: &Person,
        before: Option<Role>,
        after: Option<Role>,
    ) -> ContestRef {
        let id = GrantDecisionId::new();
        self.store
            .record_grant_decision(&GrantDecisionRecord {
                id: id.clone(),
                wiki: self.wiki.id.clone(),
                at: now(),
                by: by.map(|p| Actor {
                    account: p.id.clone(),
                    rung: Role::Admin,
                }),
                policy_block: None,
                target: target.id.clone(),
                before,
                after,
                reason: Some("Quiet for a year.".to_owned()),
                appeal: None,
            })
            .expect("a decision");
        ContestRef {
            kind: inklkunn_store::AppealedKind::Grant,
            id: id.to_string(),
        }
    }

    /// Is the person restricted now?
    pub(super) fn restricted(&self, who: &Person) -> bool {
        !self
            .store
            .active_restrictions(&self.wiki.id, &who.id, now())
            .expect("a read")
            .is_empty()
    }

    /// Is the protection of `title` in force now?
    pub(super) fn protected(&self) -> bool {
        !self
            .store
            .active_protections(&self.wiki.id, now())
            .expect("a read")
            .is_empty()
    }

    /// The appeal with this id.
    pub(super) fn appeal(&self, id: &AppealId) -> inklkunn_store::AppealRecord {
        self.store.appeal(id).expect("a read").expect("an appeal")
    }
}

/// An audit trail that tells a test **each event as it is written**, after it is kept: the moment between an act's event and
/// its record, where a test makes the record fail.
pub(super) struct HookedAudit {
    pub(super) inner: Arc<MemoryAudit>,
    pub(super) hook: Box<dyn Fn(&AuditEvent) + Send + Sync>,
}

impl inklkunn_store::AuditSink for HookedAudit {
    fn record(&self, event: &AuditEvent) -> inklkunn_store::Result<()> {
        self.inner.record(event)?;
        (self.hook)(event);
        Ok(())
    }
}

impl World {
    /// The service of the world, writing its act events to a trail that runs `hook` on each one once it is kept.
    pub(super) fn service_with(
        &self,
        hook: impl Fn(&AuditEvent) + Send + Sync + 'static,
    ) -> (AppealService, Arc<MemoryAudit>) {
        let trail = Arc::new(MemoryAudit::new());
        let sink = HookedAudit {
            inner: trail.clone(),
            hook: Box::new(hook),
        };
        let service = AppealService::new(
            self.authorizer.clone(),
            self.store.clone(),
            self.store.clone(),
            self.store.clone(),
            self.store.clone(),
            self.tombstones.clone(),
            Arc::new(sink),
            AppealLimits::default(),
        );
        (service, trail)
    }
}
