//! **The rule above every task of RFC 006: acceptance criterion 1.** A wiki with no `governance/` decides every action
//! exactly as 0.3.0 does.
//!
//! The 0.3.0 rule is **frozen here as plain test code**, taken verbatim from `git show
//! 0.3.0:crates/inklkunn-core/src/authorize.rs` (`Snapshot`, `decide`, `Action::minimum_role`, `Action::required_scope`)
//! and from `git show 0.3.0:crates/inklkunn-core/src/acceptance.rs` (`SELF_ACCEPTANCE_BELOW` and its use). Nothing in it is
//! derived from the code under test: if the product's rule drifts, this copy does not. The product's real
//! [`Authorizer::authorize`], over a real store with no governance files, is then compared with it for every principal
//! kind x every action x every grant (and none) x every visibility x every scope x the wiki there or not x every kind of
//! resource.

use std::sync::Arc;

use inklkunn_store::{
    Account, AccountStore, DisplayName, MemoryAudit, MemoryStore, NewWiki, RoleGrant, Slug,
};

use super::*;

/// Frozen from 0.3.0 (`crates/inklkunn-core/src/authorize.rs`): do not edit to follow the product.
pub(super) mod v0_3_0 {
    use inklkunn_signer::message::SystemLabel;
    use inklkunn_store::{Role, Scope, Visibility};

    // `Action` and `Principal` are the product's own types (the question is what is *decided* about them); the rule over
    // them, below, is the frozen text.
    use super::super::super::{Action, Principal};

    /// What the rule needs to know about the wiki and the caller's place in it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Snapshot {
        /// The wiki's visibility, or `None` when there is no such wiki.
        pub visibility: Option<Visibility>,
        /// The role the account holds in the wiki (always `None` for a principal that is not an account).
        pub role: Option<Role>,
    }

    /// 0.3.0's `Action::minimum_role`.
    pub const fn minimum_role(action: Action) -> Role {
        match action {
            Action::Read => Role::Observer,
            Action::Comment => Role::Questioner,
            Action::Propose => Role::Contributor,
            Action::Review | Action::CreateView => Role::Reviewer,
            Action::Accept | Action::Moderate => Role::Maintainer,
            Action::Admin | Action::Export | Action::Import => Role::Admin,
        }
    }

    /// 0.3.0's `Action::required_scope`.
    pub const fn required_scope(action: Action) -> Scope {
        match action {
            Action::Read => Scope::Read,
            Action::Comment | Action::Propose => Scope::Propose,
            Action::Review | Action::CreateView => Scope::Review,
            Action::Accept | Action::Moderate => Scope::Accept,
            Action::Admin | Action::Export | Action::Import => Scope::Admin,
        }
    }

    /// 0.3.0's `decide`: true for allowed.
    pub fn decide(snapshot: Snapshot, principal: &Principal, action: Action) -> bool {
        let Some(visibility) = snapshot.visibility else {
            return false;
        };
        match principal {
            Principal::System(_) => {
                matches!(action, Action::Read | Action::Accept | Action::Import)
            }
            Principal::Anonymous => action == Action::Read && visibility == Visibility::Public,
            Principal::Account(a) => {
                let scope = a.scope();
                scope >= required_scope(action)
                    && match snapshot.role {
                        Some(role) => role >= minimum_role(action),
                        None => action == Action::Read && visibility != Visibility::Restricted,
                    }
            }
        }
    }

    /// 0.3.0's `SELF_ACCEPTANCE_BELOW`: how many maintainers a wiki may have for one of them to accept their own proposal.
    pub const SELF_ACCEPTANCE_BELOW: usize = 3;

    /// 0.3.0's rule for accepting one's own proposal: `maintainers` counts the wiki's grants at maintainer or above.
    pub fn may_accept_own(maintainers: usize) -> bool {
        maintainers < SELF_ACCEPTANCE_BELOW
    }

    /// A system principal, for the table.
    pub fn system() -> Principal {
        Principal::system(SystemLabel::inklkunn())
    }
}

fn grants() -> Vec<Option<Role>> {
    std::iter::once(None)
        .chain(Role::ALL.into_iter().map(Some))
        .collect()
}

/// A store with a wiki of each visibility and, in each, one account per grant (and one with none).
struct Table {
    store: Arc<MemoryStore>,
    wikis: Vec<(Visibility, WikiId)>,
    accounts: Vec<(Option<Role>, AccountId)>,
}

fn table() -> Table {
    let store = Arc::new(MemoryStore::new());
    let mut accounts = Vec::new();
    for role in grants() {
        let id = account();
        store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new("Someone").unwrap(),
                local: None,
                created_at: 1,
                disabled: false,
            })
            .unwrap();
        accounts.push((role, id));
    }
    let mut wikis = Vec::new();
    for (n, visibility) in VISIBILITIES.into_iter().enumerate() {
        let mut new = NewWiki::new(
            Slug::new(&format!("frozen-{n}")).unwrap(),
            DisplayName::new("Frozen").unwrap(),
        );
        new.visibility = visibility;
        let wiki = store.create(new).unwrap();
        for (role, id) in &accounts {
            if let Some(role) = role {
                store
                    .grant(RoleGrant {
                        account: id.clone(),
                        wiki: wiki.id.clone(),
                        role: *role,
                        granted_by: None,
                        granted_at: 1,
                    })
                    .unwrap();
            }
        }
        wikis.push((visibility, wiki.id));
    }
    Table {
        store,
        wikis,
        accounts,
    }
}

/// The principals of the table: anonymous, the system, and every account with every scope.
fn principals(table: &Table) -> Vec<(Option<Role>, Principal)> {
    let authority = IdentityAuthority::for_tests();
    let mut out: Vec<(Option<Role>, Principal)> =
        vec![(None, Principal::Anonymous), (None, v0_3_0::system())];
    for (role, id) in &table.accounts {
        for scope in Scope::ALL {
            out.push((*role, Principal::token(&authority, id.clone(), scope)));
        }
    }
    out
}

/// Every decision of the product's `Authorizer` over a wiki with no governance files is 0.3.0's: for each principal, each
/// of the ten actions, each grant, each visibility, each scope, the wiki present or absent, and each kind of resource.
fn every_decision_is_0_3_0s(authorizer: &Authorizer, table: &Table) -> usize {
    let mut compared = 0;
    let absent = WikiId::new();
    for (visibility, wiki) in table
        .wikis
        .iter()
        .map(|(v, w)| (Some(*v), w.clone()))
        .chain(std::iter::once((None, absent)))
    {
        for (grant, principal) in principals(table) {
            // The grant the principal holds *in this wiki*: an account holds its grant in every wiki of the table, and
            // none in a wiki that is not there.
            let held = match (&principal, visibility) {
                (Principal::Account(_), Some(_)) => grant,
                _ => None,
            };
            for action in Action::ALL {
                let want = v0_3_0::decide(
                    v0_3_0::Snapshot {
                        visibility,
                        role: held,
                    },
                    &principal,
                    action,
                );
                for resource in resources(&wiki) {
                    let got = authorizer.authorize(&principal, action, &resource);
                    assert_eq!(
                        got.is_allowed(),
                        want,
                        "{principal:?} on a {visibility:?} wiki, grant {held:?}, {} on {resource:?}",
                        action.name()
                    );
                    compared += 1;
                }
            }
        }
    }
    compared
}

#[test]
fn criterion_1_a_wiki_with_no_governance_decides_every_action_as_0_3_0_does() {
    let table = table();
    let audit = Arc::new(MemoryAudit::new());
    let source = StoreAuthz::new(table.store.clone(), table.store.clone());
    let authorizer = Authorizer::new(Arc::new(source), audit).with_clock(super::clock);
    let compared = every_decision_is_0_3_0s(&authorizer, &table);
    // 4 wikis (3 visibilities and one that is absent) x (anonymous + system + 7 grants x 5 scopes) x 10 actions x 6 resources.
    assert_eq!(compared, 4 * (2 + 7 * 5) * 10 * 6);
}

#[test]
fn the_default_policy_and_the_defaults_of_the_rule_are_0_3_0s_table() {
    let rules = Rules::defaults();
    for action in Action::ALL {
        assert_eq!(
            rules.policy().minimum_role(action),
            v0_3_0::minimum_role(action),
            "{}",
            action.name()
        );
        assert_eq!(action.minimum_role(), v0_3_0::minimum_role(action));
        assert_eq!(action.required_scope(), v0_3_0::required_scope(action));
    }
    assert_eq!(
        crate::governance::defaults::SELF_ACCEPTANCE_BELOW,
        v0_3_0::SELF_ACCEPTANCE_BELOW
    );
    for maintainers in 0..8 {
        assert_eq!(
            rules.policy().acceptance().may_self_accept(maintainers),
            v0_3_0::may_accept_own(maintainers),
            "{maintainers} maintainers"
        );
    }
    assert_eq!(rules.policy().acceptance().approvals(), 0);
}

#[test]
fn the_pure_rule_with_no_governance_is_0_3_0s_for_every_snapshot() {
    for visibility in std::iter::once(None).chain(VISIBILITIES.into_iter().map(Some)) {
        for role in grants() {
            for scope in Scope::ALL {
                for principal in [
                    Principal::Anonymous,
                    v0_3_0::system(),
                    Principal::token(&IdentityAuthority::for_tests(), account(), scope),
                ] {
                    for action in Action::ALL {
                        assert_eq!(
                            decide(snap(visibility, role), &principal, action).is_allowed(),
                            v0_3_0::decide(
                                v0_3_0::Snapshot { visibility, role },
                                &principal,
                                action
                            ),
                            "{principal:?} {visibility:?} {role:?} {}",
                            action.name()
                        );
                    }
                }
            }
        }
    }
}

/// The same table, with the rules read by the real loader from a real (in-memory) repository that has no `governance/`:
/// the whole path a decision takes in service, the read service's snapshot included.
#[test]
fn criterion_1_holds_through_the_loader_over_a_repository_with_no_governance() {
    use crate::read::testing::{Fake, TempDir};
    use crate::read::{ReadConfig, ReadService};

    let table = table();
    let dir = TempDir::new("frozen");
    let fake = Arc::new(Fake::new(&[(
        "pages/Home.md",
        "---\ntitle: Home\n---\n\nHi.\n",
    )]));
    fake.derive_changes();
    let reads = Arc::new(
        ReadService::new(
            fake.clone(),
            table.store.clone(),
            table.store.clone(),
            inklkunn_store::RootLayout::new(dir.0.join("root")),
            ReadConfig::default(),
            None,
        )
        .with_governance_snapshots(table.store.clone()),
    );
    let source = StoreAuthz::new(table.store.clone(), table.store.clone()).with_rules(reads);
    let audit = Arc::new(MemoryAudit::new());
    let authorizer = Authorizer::new(Arc::new(source), audit).with_clock(super::clock);
    let compared = every_decision_is_0_3_0s(&authorizer, &table);
    assert_eq!(compared, 4 * (2 + 7 * 5) * 10 * 6);
}
