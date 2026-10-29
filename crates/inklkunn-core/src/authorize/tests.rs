#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use inklkunn_store::{
    Account, AccountStore, DisplayName, MemoryAudit, MemoryStore, NewWiki, RoleGrant, Slug,
    WikiRecord,
};

use super::*;

const VISIBILITIES: [Visibility; 3] = [
    Visibility::Public,
    Visibility::Internal,
    Visibility::Restricted,
];

fn account() -> AccountId {
    AccountId::new()
}

fn resources(wiki: &WikiId) -> Vec<Resource> {
    vec![
        Resource::Wiki { wiki: wiki.clone() },
        Resource::Namespace {
            wiki: wiki.clone(),
            prefix: "Guides".into(),
        },
        Resource::Page {
            wiki: wiki.clone(),
            path: "pages/Home.md".into(),
        },
        Resource::View {
            wiki: wiki.clone(),
            name: "heads/main".into(),
        },
        Resource::Proposal {
            wiki: wiki.clone(),
            id: "p-1".into(),
        },
        Resource::Discussion {
            wiki: wiki.clone(),
            id: "d-1".into(),
        },
    ]
}

/// The documented ladder, written out independently of `Action::minimum_role`: what each role may do.
fn documented(role: Role) -> Vec<Action> {
    use Action::*;
    match role {
        Role::Observer => vec![Read],
        Role::Questioner => vec![Read, Comment],
        Role::Contributor => vec![Read, Comment, Propose],
        Role::Reviewer => vec![Read, Comment, Propose, Review],
        Role::Maintainer => vec![Read, Comment, Propose, Review, Accept, CreateView, Moderate],
        Role::Admin => Action::ALL.to_vec(),
    }
}

/// The documented scopes: what a token of each scope may reach at most.
fn scope_reaches(scope: Scope) -> Vec<Action> {
    use Action::*;
    match scope {
        Scope::Read => vec![Read],
        Scope::Propose => vec![Read, Comment, Propose],
        Scope::Review => vec![Read, Comment, Propose, Review],
        Scope::Accept => vec![Read, Comment, Propose, Review, Accept, CreateView, Moderate],
        Scope::Admin => Action::ALL.to_vec(),
    }
}

fn snap(visibility: Option<Visibility>, role: Option<Role>) -> Snapshot {
    Snapshot { visibility, role }
}

#[test]
fn a_role_may_do_what_the_ladder_says_whatever_the_visibility() {
    for role in Role::ALL {
        for vis in VISIBILITIES {
            for action in Action::ALL {
                let p = Principal::session(&IdentityAuthority::for_tests(), account());
                let got = decide(snap(Some(vis), Some(role)), &p, action);
                let want = documented(role).contains(&action);
                assert_eq!(got.is_allowed(), want, "{role:?} {vis:?} {}", action.name());
            }
        }
    }
}

#[test]
fn a_principal_with_no_grant_may_only_read_what_the_visibility_lets_it() {
    for vis in VISIBILITIES {
        for action in Action::ALL {
            let anon = decide(snap(Some(vis), None), &Principal::Anonymous, action);
            assert_eq!(
                anon.is_allowed(),
                action == Action::Read && vis == Visibility::Public,
                "anonymous {vis:?} {}",
                action.name()
            );
            for scope in Scope::ALL {
                let acct = decide(
                    snap(Some(vis), None),
                    &Principal::token(&IdentityAuthority::for_tests(), account(), scope),
                    action,
                );
                assert_eq!(
                    acct.is_allowed(),
                    action == Action::Read && vis != Visibility::Restricted,
                    "account without a role, scope {scope:?}, {vis:?} {}",
                    action.name()
                );
            }
        }
    }
}

#[test]
fn a_wiki_that_is_not_there_denies_everyone_everything() {
    for action in Action::ALL {
        for role in std::iter::once(None).chain(Role::ALL.into_iter().map(Some)) {
            for p in [
                Principal::Anonymous,
                Principal::session(&IdentityAuthority::for_tests(), account()),
                Principal::system(SystemLabel::inklkunn()),
            ] {
                assert_eq!(
                    decide(snap(None, role), &p, action),
                    Decision::Deny,
                    "{p:?} {}",
                    action.name()
                );
            }
        }
    }
}

#[test]
fn a_token_never_does_more_than_its_scope_or_its_account_allows() {
    for scope in Scope::ALL {
        for role in Role::ALL {
            for action in Action::ALL {
                let p = Principal::token(&IdentityAuthority::for_tests(), account(), scope);
                let got = decide(snap(Some(Visibility::Public), Some(role)), &p, action);
                let want =
                    documented(role).contains(&action) && scope_reaches(scope).contains(&action);
                assert_eq!(
                    got.is_allowed(),
                    want,
                    "scope {scope:?}, role {role:?}, {}",
                    action.name()
                );
            }
        }
    }
    // The intersection in two concrete sentences: a maintainer's `read` token cannot accept, and an
    // observer's `admin` token cannot either.
    let read_token = Principal::token(&IdentityAuthority::for_tests(), account(), Scope::Read);
    let maintainer = snap(Some(Visibility::Public), Some(Role::Maintainer));
    assert!(decide(maintainer, &read_token, Action::Read).is_allowed());
    assert!(!decide(maintainer, &read_token, Action::Accept).is_allowed());
    let admin_token = Principal::token(&IdentityAuthority::for_tests(), account(), Scope::Admin);
    let observer = snap(Some(Visibility::Public), Some(Role::Observer));
    assert!(!decide(observer, &admin_token, Action::Accept).is_allowed());
}

#[test]
fn the_ladders_of_roles_and_scopes_agree() {
    // If one action needs no more role than another, it needs no more scope either: the two ladders
    // never disagree about which action is higher.
    for a in Action::ALL {
        for b in Action::ALL {
            if a.minimum_role() <= b.minimum_role() {
                assert!(
                    a.required_scope() <= b.required_scope(),
                    "{} vs {}",
                    a.name(),
                    b.name()
                );
            }
        }
    }
}

#[test]
fn the_system_principal_reads_accepts_and_imports_and_does_nothing_else() {
    let system = Principal::system(SystemLabel::new("import").unwrap());
    for vis in VISIBILITIES {
        for action in Action::ALL {
            let want = matches!(action, Action::Read | Action::Accept | Action::Import);
            assert_eq!(
                decide(snap(Some(vis), None), &system, action).is_allowed(),
                want,
                "{vis:?} {}",
                action.name()
            );
        }
    }
    assert_eq!(system.audit_name(), "system:import");
    let Principal::System(inner) = &system else {
        panic!("not a system principal")
    };
    assert_eq!(inner.label().as_str(), "import");
}

// ---- the Authorizer ----

/// A source with fixed answers that counts how often it was asked.
#[derive(Default)]
struct Facts {
    wikis: HashMap<WikiId, Visibility>,
    roles: HashMap<(AccountId, WikiId), Role>,
    fail: bool,
    visibility_calls: AtomicUsize,
    role_calls: AtomicUsize,
}

impl AuthzSource for Facts {
    fn visibility(&self, wiki: &WikiId) -> Result<Option<Visibility>, SourceError> {
        self.visibility_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(SourceError("down".into()));
        }
        Ok(self.wikis.get(wiki).copied())
    }

    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>, SourceError> {
        self.role_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(SourceError("down".into()));
        }
        Ok(self.roles.get(&(account.clone(), wiki.clone())).copied())
    }
}

/// A sink that can be told to fail.
#[derive(Default)]
struct Sink {
    events: Mutex<Vec<AuditEvent>>,
    fail: bool,
}

impl AuditSink for Sink {
    fn record(&self, event: &AuditEvent) -> inklkunn_store::Result<()> {
        if self.fail {
            return Err(StoreError::Backend("full".into()));
        }
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }
}

fn clock() -> i64 {
    1_700_000_000
}

fn authorizer(facts: Facts, sink: Sink) -> (Authorizer, Arc<Facts>, Arc<Sink>) {
    let facts = Arc::new(facts);
    let sink = Arc::new(sink);
    (
        Authorizer::new(facts.clone(), sink.clone()).with_clock(clock),
        facts,
        sink,
    )
}

#[test]
fn the_authorizer_reads_the_wiki_itself_so_a_handler_cannot_say_it_is_public() {
    let (public, restricted) = (WikiId::new(), WikiId::new());
    let mut facts = Facts::default();
    facts.wikis.insert(public.clone(), Visibility::Public);
    facts
        .wikis
        .insert(restricted.clone(), Visibility::Restricted);
    let (a, _, _) = authorizer(facts, Sink::default());
    for r in resources(&public) {
        assert_eq!(
            a.authorize(&Principal::Anonymous, Action::Read, &r),
            Decision::Allow
        );
    }
    for r in resources(&restricted) {
        assert_eq!(
            a.authorize(&Principal::Anonymous, Action::Read, &r),
            Decision::Deny
        );
    }
}

#[test]
fn a_restricted_wiki_and_an_absent_one_are_indistinguishable_to_every_caller() {
    let (restricted, absent) = (WikiId::new(), WikiId::new());
    let (member, stranger) = (account(), account());
    let mut facts = Facts::default();
    facts
        .wikis
        .insert(restricted.clone(), Visibility::Restricted);
    facts
        .roles
        .insert((member.clone(), restricted.clone()), Role::Observer);
    let (a, f, sink) = authorizer(facts, Sink::default());
    // For a stranger, an anonymous caller and a token: the same decision for every action on every kind of
    // resource, and for the consequential ones the same audit event but for the wiki id.
    let callers = [
        Principal::Anonymous,
        Principal::session(&IdentityAuthority::for_tests(), stranger.clone()),
        Principal::token(&IdentityAuthority::for_tests(), stranger, Scope::Admin),
    ];
    for caller in &callers {
        for action in Action::ALL {
            for (hidden, gone) in resources(&restricted).iter().zip(resources(&absent).iter()) {
                let before = sink.events.lock().unwrap().len();
                let lookups = f.role_calls.load(Ordering::SeqCst);
                let d_hidden = a.authorize(caller, action, hidden);
                let roles_hidden = f.role_calls.load(Ordering::SeqCst) - lookups;
                let d_gone = a.authorize(caller, action, gone);
                let roles_gone = f.role_calls.load(Ordering::SeqCst) - lookups - roles_hidden;
                assert_eq!(d_hidden, Decision::Deny);
                assert_eq!(d_hidden, d_gone, "{caller:?} {}", action.name());
                assert_eq!(
                    roles_hidden, roles_gone,
                    "asking about an absent wiki costs the same lookups as asking about a hidden one"
                );
                let events = sink.events.lock().unwrap();
                let fresh = events.get(before..).unwrap_or_default();
                if action.is_consequential() {
                    assert_eq!(fresh.len(), 2, "{caller:?} {}", action.name());
                    let [h, g] = fresh else {
                        panic!("two events for the hidden wiki and the absent one")
                    };
                    assert_eq!(
                        (
                            h.principal.as_str(),
                            h.action.as_str(),
                            h.outcome,
                            h.detail.as_str()
                        ),
                        (
                            g.principal.as_str(),
                            g.action.as_str(),
                            g.outcome,
                            g.detail.as_str()
                        )
                    );
                } else {
                    assert!(fresh.is_empty());
                }
            }
        }
    }
    // A member is told apart from a stranger by what they may do, never by how the wiki answers.
    assert_eq!(
        a.authorize(
            &Principal::session(&IdentityAuthority::for_tests(), member),
            Action::Read,
            &Resource::Wiki { wiki: restricted }
        ),
        Decision::Allow
    );
}

#[test]
fn consequential_decisions_are_recorded_allowed_or_not_and_the_others_are_not() {
    let wiki = WikiId::new();
    let admin = account();
    let mut facts = Facts::default();
    facts.wikis.insert(wiki.clone(), Visibility::Public);
    facts
        .roles
        .insert((admin.clone(), wiki.clone()), Role::Admin);
    let (a, _, sink) = authorizer(facts, Sink::default());
    let page = Resource::Page {
        wiki: wiki.clone(),
        path: "pages/Secret plans.md".into(),
    };
    let proposal = Resource::Proposal {
        wiki: wiki.clone(),
        id: "prop-9".into(),
    };
    for action in Action::ALL {
        let _ = a.authorize(
            &Principal::session(&IdentityAuthority::for_tests(), admin.clone()),
            action,
            &page,
        );
        let _ = a.authorize(&Principal::Anonymous, action, &proposal);
    }
    let events = sink.events.lock().unwrap().clone();
    let consequential: Vec<&str> = Action::ALL
        .iter()
        .filter(|x| x.is_consequential())
        .map(|x| x.name())
        .collect();
    assert_eq!(
        consequential,
        ["accept", "moderate", "admin", "export", "import"]
    );
    assert_eq!(
        events.len(),
        consequential.len() * 2,
        "two callers, each action once"
    );
    for e in &events {
        assert_eq!(e.at, 1_700_000_000);
        assert!(consequential.contains(&e.action.as_str()), "{e:?}");
    }
    let by_admin: Vec<&AuditEvent> = events
        .iter()
        .filter(|e| e.principal == format!("account:{admin}"))
        .collect();
    assert_eq!(by_admin.len(), 5);
    assert!(by_admin.iter().all(|e| e.outcome == AuditOutcome::Allowed));
    assert!(
        by_admin
            .iter()
            .all(|e| e.resource == format!("page:{wiki}"))
    );
    let by_anon: Vec<&AuditEvent> = events
        .iter()
        .filter(|e| e.principal == "anonymous")
        .collect();
    assert_eq!(by_anon.len(), 5);
    assert!(by_anon.iter().all(|e| e.outcome == AuditOutcome::Denied));
    assert!(
        by_anon
            .iter()
            .all(|e| e.resource == format!("proposal:{wiki}:prop-9"))
    );
    // The trail holds events, not titles.
    assert!(events.iter().all(|e| !e.resource.contains("Secret plans")));
}

#[test]
fn a_source_that_cannot_answer_denies_and_says_so_in_the_trail() {
    let wiki = WikiId::new();
    let (a, _, sink) = authorizer(
        Facts {
            fail: true,
            ..Facts::default()
        },
        Sink::default(),
    );
    let r = Resource::Wiki { wiki };
    for action in Action::ALL {
        for p in [
            Principal::Anonymous,
            Principal::session(&IdentityAuthority::for_tests(), account()),
        ] {
            assert_eq!(
                a.authorize(&p, action, &r),
                Decision::Deny,
                "{}",
                action.name()
            );
        }
    }
    let events = sink.events.lock().unwrap().clone();
    assert_eq!(events.len(), 10);
    assert!(
        events
            .iter()
            .all(|e| e.outcome == AuditOutcome::Failed && !e.detail.is_empty())
    );
}

#[test]
fn nothing_consequential_is_allowed_unrecorded() {
    let wiki = WikiId::new();
    let admin = account();
    let mut facts = Facts::default();
    facts.wikis.insert(wiki.clone(), Visibility::Public);
    facts
        .roles
        .insert((admin.clone(), wiki.clone()), Role::Admin);
    let (a, _, _) = authorizer(
        facts,
        Sink {
            fail: true,
            ..Sink::default()
        },
    );
    let r = Resource::Wiki { wiki };
    let p = Principal::session(&IdentityAuthority::for_tests(), admin);
    for action in Action::ALL {
        let d = a.authorize(&p, action, &r);
        if action.is_consequential() {
            assert_eq!(
                d,
                Decision::Deny,
                "{} must not be allowed when it cannot be recorded",
                action.name()
            );
        } else {
            assert_eq!(
                d,
                Decision::Allow,
                "{} is not audited, so a full trail does not stop it",
                action.name()
            );
        }
    }
}

#[test]
fn the_store_backed_source_reads_the_wiki_record_and_the_role_grants() {
    let store = Arc::new(MemoryStore::new());
    let mut new = NewWiki::new(
        Slug::new("authz").unwrap(),
        DisplayName::new("Authz").unwrap(),
    );
    new.visibility = Visibility::Restricted;
    let wiki: WikiRecord = store.create(new).unwrap();
    let member = account();
    // A grant needs a real account (a foreign key in the SQLite store, a check in this one).
    store
        .add_account(&Account {
            id: member.clone(),
            display_name: DisplayName::new("Member").unwrap(),
            local: None,
            created_at: 1,
            disabled: false,
        })
        .unwrap();
    let source = StoreAuthz::new(store.clone(), store.clone());
    let audit = Arc::new(MemoryAudit::new());
    let a = Authorizer::new(Arc::new(source), audit.clone());
    let r = Resource::Wiki {
        wiki: wiki.id.clone(),
    };
    let p = Principal::session(&IdentityAuthority::for_tests(), member.clone());
    assert_eq!(
        a.authorize(&p, Action::Read, &r),
        Decision::Deny,
        "no grant yet"
    );
    store
        .grant(RoleGrant {
            account: member.clone(),
            wiki: wiki.id.clone(),
            role: Role::Reviewer,
            granted_by: None,
            granted_at: 1,
        })
        .unwrap();
    assert_eq!(a.authorize(&p, Action::Review, &r), Decision::Allow);
    assert_eq!(a.authorize(&p, Action::Accept, &r), Decision::Deny);
    assert_eq!(
        a.authorize(&Principal::Anonymous, Action::Read, &r),
        Decision::Deny
    );
    // Revoking takes effect at the next decision: nothing is cached.
    store.revoke(&member, &wiki.id).unwrap();
    assert_eq!(a.authorize(&p, Action::Review, &r), Decision::Deny);
    // A wiki id nobody made is denied like a hidden one.
    assert_eq!(
        a.authorize(
            &p,
            Action::Read,
            &Resource::Wiki {
                wiki: WikiId::new()
            }
        ),
        Decision::Deny
    );
    // Of everything asked above only the refused `accept` was consequential, and it is on the trail, denied.
    let events = audit.events();
    assert_eq!(events.len(), 1, "{events:?}");
    let e = events.first().unwrap();
    assert_eq!(
        (e.action.as_str(), e.outcome, e.principal.as_str()),
        (
            "accept",
            AuditOutcome::Denied,
            format!("account:{member}").as_str()
        )
    );
}

#[test]
fn audit_names_carry_ids_and_never_titles() {
    let wiki = WikiId::new();
    for (resource, want) in [
        (
            Resource::Wiki { wiki: wiki.clone() },
            format!("wiki:{wiki}"),
        ),
        (
            Resource::Namespace {
                wiki: wiki.clone(),
                prefix: "Private/Plans".into(),
            },
            format!("namespace:{wiki}"),
        ),
        (
            Resource::Page {
                wiki: wiki.clone(),
                path: "pages/Plans.md".into(),
            },
            format!("page:{wiki}"),
        ),
        (
            Resource::View {
                wiki: wiki.clone(),
                name: "heads/draft".into(),
            },
            format!("view:{wiki}:heads/draft"),
        ),
        (
            Resource::Proposal {
                wiki: wiki.clone(),
                id: "p".into(),
            },
            format!("proposal:{wiki}:p"),
        ),
        (
            Resource::Discussion {
                wiki: wiki.clone(),
                id: "d".into(),
            },
            format!("discussion:{wiki}:d"),
        ),
    ] {
        assert_eq!(resource.audit_name(), want);
        assert_eq!(resource.wiki(), &wiki);
    }
    assert_eq!(Principal::Anonymous.audit_name(), "anonymous");
    let id = account();
    assert_eq!(
        Principal::session(&IdentityAuthority::for_tests(), id.clone()).audit_name(),
        format!("account:{id}")
    );
    assert_eq!(
        Principal::token(&IdentityAuthority::for_tests(), id.clone(), Scope::Read).audit_name(),
        format!("account:{id}")
    );
}

#[test]
fn the_identity_authority_is_claimed_once_per_process_and_whoever_asks_second_gets_nothing() {
    // The only test of this binary that claims it: the statics are per process.
    let first = IdentityAuthority::claim();
    assert!(first.is_some(), "the first claim");
    assert!(IdentityAuthority::claim().is_none(), "the second claim");
    assert!(IdentityAuthority::claim().is_none(), "and every one after");
    // Tests build their own, as many as they like, and a principal built with one is an ordinary account.
    let p = Principal::session(&IdentityAuthority::for_tests(), account());
    assert!(matches!(&p, Principal::Account(a) if a.scope() == Scope::Admin));
    assert!(p.audit_name().starts_with("account:"));
}
