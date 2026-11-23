//! What the people page of a wiki says (RFC 006 D-6): the rungs in order, each with the wiki's promotion text and the people
//! who hold it, the person's own standing, what they may do on the page, and, to the `moderate` rung alone, who is
//! restricted until when and why.
//!
//! **Who is named.** Whoever may read the wiki may open the page (members of an `internal` or `restricted` wiki, everyone
//! for a `public` one). A person is named **by the name the wiki shows people everywhere else** and no more than the
//! proposal and history pages show of them: a visitor who has no account sees the names of the people who hold the
//! reviewer rung or above (reviewing and accepting are public acts that name their doer) and **only the number** of the
//! people who hold a lower one; an account sees every name. **A restriction is never named to anyone but the restricted
//! person and the `moderate` rung.**

use inklkunn_store::{AccountId, Role, Visibility, WikiRecord};

use super::rule::{check_restrict, grants_at_all, highest_grantable};
use super::service::{PeopleError, PeopleService};
use crate::authorize::{Holding, Principal, RestrictionNotice, entry_rung};
use crate::governance::Entry;

/// The most restricted people the page lists to the `moderate` rung.
const RESTRICTED_SHOWN: usize = 100;

/// One person who holds a rung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    /// The account.
    pub account: AccountId,
    /// The name the wiki shows, or `None` for an account that is not known.
    pub name: Option<String>,
    /// Is this the person who is looking?
    pub is_you: bool,
    /// May the person who is looking change this person's rung (there is a form to offer)?
    pub may_change: bool,
    /// May the person who is looking restrict them?
    pub may_restrict: bool,
    /// When the restriction in force ends: **told to the `moderate` rung only**, `None` for everyone else.
    pub restricted_until: Option<i64>,
}

/// Who holds a rung, as much as the person who is looking may know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holders {
    /// Each one, by name.
    Named(Vec<Person>),
    /// Only how many: a visitor who has no account is not told who holds a rung that does nothing in public.
    Count(usize),
}

/// One rung of the ladder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RungRow {
    /// The rung.
    pub rung: Role,
    /// What the rules of the wiki say it looks for before it is granted: guidance, never a count or a score.
    pub promotion: Option<String>,
    /// Who holds it.
    pub holders: Holders,
}

/// One restriction in force, for the `moderate` rung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestrictedRow {
    /// The account.
    pub account: AccountId,
    /// The name the wiki shows.
    pub name: Option<String>,
    /// The reason the person was given.
    pub reason: String,
    /// When it ends, in Unix seconds.
    pub until: i64,
    /// The rung of the person who set it, and the block of the rules in force when they did (RFC 006 D-5; never a name).
    pub by_rung: Role,
    /// The block of the rules in force when it was set, `None` for the defaults.
    pub policy_block: Option<String>,
    /// May the person who is looking renew or lift it?
    pub may_act: bool,
}

/// The people page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeopleView {
    /// Who may read the wiki: what the page says about `entry` depends on it.
    pub visibility: Visibility,
    /// The block that set the rules in force, or `None` for the defaults.
    pub rules_block: Option<String>,
    /// What the rules say `entry` is.
    pub entry: Entry,
    /// Does it do anything here? Never in a `restricted` wiki.
    pub entry_applies: bool,
    /// The rungs, lowest first.
    pub rungs: Vec<RungRow>,
    /// The person who is looking, when they are an account: the rung they hold, what gives it to them, and any restriction.
    pub you: Option<Holding>,
    /// The highest rung the person who is looking may give, when they may change rungs at all.
    pub may_grant_up_to: Option<Role>,
    /// May the person who is looking restrict people?
    pub may_restrict: bool,
    /// **Their restriction is the reason they may not change rungs**, when it is: they would hold a rung that does (a
    /// maintainer whose wiki lets maintainers grant), and a restriction holds them at observer. The reason and the end, for
    /// the page that tells them so instead of *not allowed*.
    pub grant_held_down_by: Option<RestrictionNotice>,
    /// The same for restricting people.
    pub restrict_held_down_by: Option<RestrictionNotice>,
    /// The most days a restriction may last.
    pub limit_days: u32,
    /// The restrictions in force, to the `moderate` rung; empty to everyone else.
    pub restricted: Vec<RestrictedRow>,
}

impl PeopleService {
    /// The people page of `wiki` for `principal`. [`PeopleError::NotFound`] for a wiki that is not there and for one the
    /// principal may not see: one answer.
    pub fn view(
        &self,
        principal: &Principal,
        wiki: &WikiRecord,
    ) -> Result<PeopleView, PeopleError> {
        self.readable(principal, wiki)?;
        let unavailable = |e: &dyn std::fmt::Display| PeopleError::Unavailable(e.to_string());
        let rules = self
            .authorizer
            .rules(&wiki.id)
            .map_err(|e| unavailable(&e))?;
        let policy = rules.policy();
        let you = match principal {
            Principal::Account(_) => Some(
                self.authorizer
                    .holding(principal, &wiki.id)
                    .map_err(|e| unavailable(&e))?,
            ),
            Principal::Anonymous | Principal::System(_) => None,
        };
        let viewer = principal.account_id();
        let viewer_rung = you.as_ref().and_then(|h| h.rung);
        let fault = rules.fault().is_some();
        let up_to = policy.rungs().grant_up_to();
        let may_grant_up_to = viewer_rung
            .filter(|r| !fault && grants_at_all(*r, up_to))
            .and_then(|r| highest_grantable(r, up_to));
        let moderate = policy.rungs().moderate();
        let may_restrict = !fault && viewer_rung.is_some_and(|r| r >= moderate);

        let grants = self
            .roles
            .grants_in(&wiki.id)
            .map_err(|e| unavailable(&e))?;
        let entry = entry_rung(wiki.visibility, &rules);
        // The admins who can sign in, by account: who is left when one of them is changed.
        let mut admins: Vec<&AccountId> = Vec::new();
        for grant in grants.iter().filter(|g| g.role == Role::Admin) {
            let account = self
                .accounts
                .account(&grant.account)
                .map_err(|e| unavailable(&e))?;
            if account.is_some_and(|a| !a.disabled) {
                admins.push(&grant.account);
            }
        }
        let restrictions = if may_restrict {
            self.governance
                .active_restrictions_in(&wiki.id, self.authorizer.now(), RESTRICTED_SHOWN)
                .map_err(|e| unavailable(&e))?
        } else {
            Vec::new()
        };
        let name_of = |account: &AccountId| -> Result<Option<String>, PeopleError> {
            Ok(self
                .accounts
                .account(account)
                .map_err(|e| unavailable(&e))?
                .map(|a| a.display_name.as_str().to_owned()))
        };
        // May the viewer change the rung this person holds through a grant? (The rule's own check over any new rung.)
        let may_change = |account: &AccountId, current: Role| -> bool {
            let Some(rung) = viewer_rung else {
                return false;
            };
            if fault || viewer == Some(account) || !grants_at_all(rung, up_to) {
                return false;
            }
            let reach = rung == Role::Admin || up_to.maintainer_may_grant(current);
            reach && !(current == Role::Admin && !admins.iter().any(|a| *a != account))
        };
        let mut rungs = Vec::with_capacity(Role::ALL.len());
        for rung in Role::ALL {
            let holders: Vec<&AccountId> = grants
                .iter()
                .filter(|g| g.role == rung)
                .map(|g| &g.account)
                .collect();
            let named = viewer.is_some() || rung >= Role::Reviewer;
            let holders = if named {
                let mut people = Vec::with_capacity(holders.len());
                for account in holders {
                    let held = Some(rung).max(entry);
                    let restricted = restrictions.iter().find(|r| r.account == *account);
                    people.push(Person {
                        account: account.clone(),
                        name: name_of(account)?,
                        is_you: viewer == Some(account),
                        may_change: may_change(account, rung),
                        may_restrict: may_restrict
                            && check_restrict(
                                viewer_rung.unwrap_or(Role::Observer),
                                moderate,
                                held,
                                viewer == Some(account),
                            )
                            .is_ok(),
                        restricted_until: restricted.and_then(|r| r.expires_at),
                    });
                }
                people.sort_by_key(|p| p.name.as_deref().map(str::to_lowercase));
                Holders::Named(people)
            } else {
                Holders::Count(holders.len())
            };
            rungs.push(RungRow {
                rung,
                promotion: policy.promotion(rung).map(str::to_owned),
                holders,
            });
        }
        let mut restricted = Vec::with_capacity(restrictions.len());
        for r in &restrictions {
            let held = self
                .roles
                .role(&r.account, &wiki.id)
                .map_err(|e| unavailable(&e))?
                .max(entry);
            restricted.push(RestrictedRow {
                account: r.account.clone(),
                name: name_of(&r.account)?,
                reason: r.reason.clone().unwrap_or_default(),
                until: r.expires_at.unwrap_or(i64::MAX),
                by_rung: r.by.rung,
                policy_block: r.policy_block.clone(),
                may_act: check_restrict(
                    viewer_rung.unwrap_or(Role::Observer),
                    moderate,
                    held,
                    viewer == Some(&r.account),
                )
                .is_ok(),
            });
        }
        // A restriction that holds a person down from acts their held rung includes is told as that, never as *not allowed*.
        let held_down = |would: bool| -> Option<RestrictionNotice> {
            let holding = you.as_ref()?;
            would.then(|| holding.restriction.clone()).flatten()
        };
        let held = you.as_ref().and_then(Holding::held);
        let grant_held_down_by = held_down(
            may_grant_up_to.is_none() && !fault && held.is_some_and(|h| grants_at_all(h, up_to)),
        );
        let restrict_held_down_by =
            held_down(!may_restrict && !fault && held.is_some_and(|h| h >= moderate));
        Ok(PeopleView {
            visibility: wiki.visibility,
            rules_block: rules.block().map(str::to_owned),
            entry: policy.entry(),
            entry_applies: wiki.visibility != Visibility::Restricted && entry.is_some(),
            rungs,
            you,
            may_grant_up_to,
            may_restrict,
            grant_held_down_by,
            restrict_held_down_by,
            limit_days: self.limits.limit_days,
            restricted,
        })
    }
}
