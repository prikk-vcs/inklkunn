//! `authorize()` reading the rules of a wiki (RFC 006 D-5; G-T1 part C): the rung is the higher of the grant and `entry`
//! (never in a restricted wiki), capped; the policy's rungs decide; a governance file that cannot be read refuses every
//! write but an admin's repair; the audit record names the rung and the block; the rules are asked for only when they can
//! matter.

use super::*;
use crate::governance::{
    Constitution, Entry, PlatformLimits, Policy, ReadFault, Refusal, short_block,
};

const BLOCK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// A source with the table's facts and a rules answer that can be set, failed and counted.
struct Governed {
    facts: Facts,
    rules: Mutex<Result<Arc<Rules>, SourceError>>,
    asked: AtomicUsize,
}

impl Governed {
    fn new(facts: Facts, rules: Rules) -> Governed {
        Governed {
            facts,
            rules: Mutex::new(Ok(Arc::new(rules))),
            asked: AtomicUsize::new(0),
        }
    }
}

impl AuthzSource for Governed {
    fn visibility(&self, wiki: &WikiId) -> Result<Option<Visibility>, SourceError> {
        self.facts.visibility(wiki)
    }

    fn role(&self, account: &AccountId, wiki: &WikiId) -> Result<Option<Role>, SourceError> {
        self.facts.role(account, wiki)
    }

    fn rules(&self, _: &WikiId) -> Result<Arc<Rules>, SourceError> {
        self.asked.fetch_add(1, Ordering::SeqCst);
        self.rules.lock().unwrap().clone()
    }
}

fn policy(text: &str) -> Policy {
    crate::governance::read_policy(text.as_bytes()).expect("a policy")
}

fn rules_with(text: &str) -> Rules {
    Rules::read(
        Some(BLOCK.to_owned()),
        policy(text),
        Constitution::default(),
        &PlatformLimits::default(),
    )
}

fn faulted() -> Rules {
    Rules::faulted(
        Some(BLOCK.to_owned()),
        ReadFault::from_stored(GovernanceFile::Policy, Some(4), Refusal::UnknownKey),
    )
}

struct Wikis {
    public: WikiId,
    internal: WikiId,
    restricted: WikiId,
}

fn wikis() -> Wikis {
    Wikis {
        public: WikiId::new(),
        internal: WikiId::new(),
        restricted: WikiId::new(),
    }
}

fn facts(w: &Wikis, grants: &[(&AccountId, Role)]) -> Facts {
    let mut facts = Facts::default();
    facts.wikis.insert(w.public.clone(), Visibility::Public);
    facts.wikis.insert(w.internal.clone(), Visibility::Internal);
    facts
        .wikis
        .insert(w.restricted.clone(), Visibility::Restricted);
    for (account, role) in grants {
        for wiki in [&w.public, &w.internal, &w.restricted] {
            facts
                .roles
                .insert(((*account).clone(), wiki.clone()), *role);
        }
    }
    facts
}

fn over(governed: Governed) -> (Authorizer, Arc<Governed>, Arc<Sink>) {
    let governed = Arc::new(governed);
    let sink = Arc::new(Sink::default());
    (
        Authorizer::new(governed.clone(), sink.clone()).with_clock(clock),
        governed,
        sink,
    )
}

fn session(id: &AccountId) -> Principal {
    Principal::session(&IdentityAuthority::for_tests(), id.clone())
}

fn on(wiki: &WikiId) -> Resource {
    Resource::Wiki { wiki: wiki.clone() }
}

fn page(wiki: &WikiId, path: &str) -> Resource {
    Resource::Page {
        wiki: wiki.clone(),
        path: path.into(),
    }
}

// ---- the rung: the higher of the grant and `entry`, capped

#[test]
fn the_rung_is_the_higher_of_the_grant_and_entry_and_entry_gives_nothing_in_a_restricted_wiki() {
    let rules = rules_with("entry: questioner\n");
    for visibility in VISIBILITIES {
        for grant in std::iter::once(None).chain(Role::ALL.into_iter().map(Some)) {
            let got = rung_of(snap(Some(visibility), grant), &rules);
            let entry = (visibility != Visibility::Restricted).then_some(Role::Questioner);
            assert_eq!(got, grant.max(entry), "{visibility:?} {grant:?}");
        }
    }
    // No wiki, no rung.
    assert_eq!(rung_of(snap(None, Some(Role::Admin)), &rules), None);
    // `none` gives nothing, anywhere.
    let none = rules_with("entry: none\n");
    for visibility in VISIBILITIES {
        assert_eq!(rung_of(snap(Some(visibility), None), &none), None);
    }
}

#[test]
fn a_cap_lowers_the_rung_after_entry_and_none_caps_nothing() {
    let rules = rules_with("entry: contributor\n");
    for grant in [None, Some(Role::Observer), Some(Role::Maintainer)] {
        let mut s = snap(Some(Visibility::Internal), grant);
        let uncapped = rung_of(s, &rules);
        s.cap = Some(Role::Observer);
        let capped = rung_of(s, &rules);
        assert_eq!(capped, Some(Role::Observer), "{grant:?}");
        assert!(capped <= uncapped);
        s.cap = None;
        assert_eq!(rung_of(s, &rules), uncapped);
    }
    // A cap above the rung changes nothing.
    let mut s = snap(Some(Visibility::Internal), Some(Role::Questioner));
    s.cap = Some(Role::Maintainer);
    assert_eq!(rung_of(s, &Rules::defaults()), Some(Role::Questioner));
}

#[test]
fn entry_lets_an_account_without_a_grant_do_what_the_rung_allows_in_a_public_or_internal_wiki_only()
{
    let w = wikis();
    let stranger = account();
    let (a, _, _) = over(Governed::new(
        facts(&w, &[]),
        rules_with("entry: contributor\n"),
    ));
    let p = session(&stranger);
    for (wiki, visibility) in [
        (&w.public, Visibility::Public),
        (&w.internal, Visibility::Internal),
        (&w.restricted, Visibility::Restricted),
    ] {
        for action in [
            Action::Comment,
            Action::Propose,
            Action::Review,
            Action::Accept,
        ] {
            let want = visibility != Visibility::Restricted
                && matches!(action, Action::Comment | Action::Propose);
            assert_eq!(
                a.authorize(&p, action, &on(wiki)).is_allowed(),
                want,
                "{visibility:?} {}",
                action.name()
            );
        }
    }
}

#[test]
fn entry_does_not_raise_a_member_of_a_restricted_wiki() {
    // A restricted wiki's members are the accounts the operator granted a rung (RFC 006 D-3): `entry` is for the others.
    let w = wikis();
    let member = account();
    let mut f = facts(&w, &[]);
    f.roles
        .insert((member.clone(), w.restricted.clone()), Role::Observer);
    f.roles
        .insert((member.clone(), w.internal.clone()), Role::Observer);
    let (a, _, _) = over(Governed::new(f, rules_with("entry: contributor\n")));
    let p = session(&member);
    assert!(
        !a.authorize(&p, Action::Propose, &on(&w.restricted))
            .is_allowed(),
        "an observer of a restricted wiki stays one"
    );
    assert!(
        a.authorize(&p, Action::Propose, &on(&w.internal))
            .is_allowed(),
        "and the same account in an internal wiki is raised"
    );
}

#[test]
fn a_grant_above_entry_stays_and_entry_above_the_grant_raises_it() {
    let w = wikis();
    let (observer, maintainer) = (account(), account());
    let mut f = facts(&w, &[]);
    f.roles
        .insert((observer.clone(), w.internal.clone()), Role::Observer);
    f.roles
        .insert((maintainer.clone(), w.internal.clone()), Role::Maintainer);
    let (a, _, _) = over(Governed::new(f, rules_with("entry: contributor\n")));
    // The observer is raised to what the wiki gives everyone; the maintainer is not lowered to it.
    assert!(
        a.authorize(&session(&observer), Action::Propose, &on(&w.internal))
            .is_allowed()
    );
    assert!(
        !a.authorize(&session(&observer), Action::Review, &on(&w.internal))
            .is_allowed()
    );
    assert!(
        a.authorize(&session(&maintainer), Action::Accept, &on(&w.internal))
            .is_allowed()
    );
}

// ---- the policy's rungs decide

#[test]
fn the_policys_rungs_decide_what_each_rung_may_do() {
    let w = wikis();
    let (questioner, contributor, reviewer) = (account(), account(), account());
    let mut f = facts(&w, &[]);
    for (who, role) in [
        (&questioner, Role::Questioner),
        (&contributor, Role::Contributor),
        (&reviewer, Role::Reviewer),
    ] {
        f.roles.insert((who.clone(), w.internal.clone()), role);
    }
    // Anyone who may comment may propose; reviewers accept.
    let text = "rungs:\n  propose: questioner\n  review: reviewer\n  accept: reviewer\n";
    let (a, _, _) = over(Governed::new(f, rules_with(text)));
    let r = on(&w.internal);
    assert!(
        a.authorize(&session(&questioner), Action::Propose, &r)
            .is_allowed()
    );
    assert!(
        !a.authorize(&session(&questioner), Action::Review, &r)
            .is_allowed()
    );
    assert!(
        a.authorize(&session(&reviewer), Action::Accept, &r)
            .is_allowed()
    );
    assert!(
        !a.authorize(&session(&contributor), Action::Accept, &r)
            .is_allowed()
    );
    // The admin actions stay admin and reading stays observer, whatever a policy says.
    for who in [&questioner, &contributor, &reviewer] {
        for action in [Action::Admin, Action::Export, Action::Import] {
            assert!(!a.authorize(&session(who), action, &r).is_allowed());
        }
        assert!(a.authorize(&session(who), Action::Read, &r).is_allowed());
    }
    // A token's scope still intersects: a `propose` token of a reviewer cannot accept.
    let token = Principal::token(
        &IdentityAuthority::for_tests(),
        reviewer.clone(),
        Scope::Propose,
    );
    assert!(!a.authorize(&token, Action::Accept, &r).is_allowed());
}

// ---- a governance file that cannot be read

#[test]
fn a_wiki_whose_governance_cannot_be_read_refuses_every_write_and_reads_on() {
    let w = wikis();
    let (maintainer, observer, admin) = (account(), account(), account());
    let mut f = facts(&w, &[]);
    f.roles
        .insert((maintainer.clone(), w.internal.clone()), Role::Maintainer);
    f.roles
        .insert((observer.clone(), w.internal.clone()), Role::Observer);
    f.roles
        .insert((admin.clone(), w.internal.clone()), Role::Admin);
    let (a, _, _) = over(Governed::new(f, faulted()));
    let r = on(&w.internal);
    for who in [&maintainer, &observer] {
        for action in Action::ALL {
            assert_eq!(
                a.authorize(&session(who), action, &r).is_allowed(),
                action == Action::Read,
                "{}",
                action.name()
            );
        }
    }
    // Even the admin's other writes: comment, review, create a view, moderate, administer, export, import.
    for action in Action::ALL {
        let allowed = a.authorize(&session(&admin), action, &r).is_allowed();
        assert_eq!(
            allowed,
            matches!(action, Action::Read | Action::Accept),
            "{}",
            action.name()
        );
    }
    // The system's acts are not a person's writes and are decided as before.
    let system = Principal::system(SystemLabel::inklkunn());
    assert!(a.authorize(&system, Action::Accept, &r).is_allowed());
}

#[test]
fn an_admin_may_propose_to_the_two_governance_files_in_that_state_and_to_nothing_else() {
    let w = wikis();
    let (admin, maintainer) = (account(), account());
    let mut f = facts(&w, &[]);
    f.roles
        .insert((admin.clone(), w.internal.clone()), Role::Admin);
    f.roles
        .insert((maintainer.clone(), w.internal.clone()), Role::Maintainer);
    let (a, _, _) = over(Governed::new(f, faulted()));
    for path in ["governance/policy.yaml", "governance/constitution.yaml"] {
        assert!(
            a.authorize(&session(&admin), Action::Propose, &page(&w.internal, path))
                .is_allowed(),
            "{path}"
        );
        // Not a maintainer: the repair is an admin's.
        assert!(
            !a.authorize(
                &session(&maintainer),
                Action::Propose,
                &page(&w.internal, path)
            )
            .is_allowed(),
            "{path}"
        );
    }
    for path in [
        "pages/Home.md",
        "governance/other.yaml",
        "governance/",
        "Governance/policy.yaml",
    ] {
        assert!(
            !a.authorize(&session(&admin), Action::Propose, &page(&w.internal, path))
                .is_allowed(),
            "{path}"
        );
    }
    // On the wiki as a whole there is no governance file to name.
    assert!(
        !a.authorize(&session(&admin), Action::Propose, &on(&w.internal))
            .is_allowed()
    );
    // A token that cannot accept cannot, whoever holds it.
    let weak = Principal::token(&IdentityAuthority::for_tests(), admin.clone(), Scope::Read);
    assert!(
        !a.authorize(
            &weak,
            Action::Propose,
            &page(&w.internal, "governance/policy.yaml")
        )
        .is_allowed()
    );
}

#[test]
fn only_someone_who_may_read_the_wiki_is_told_why_its_writes_refuse() {
    let w = wikis();
    let member = account();
    let mut f = facts(&w, &[]);
    f.roles
        .insert((member.clone(), w.restricted.clone()), Role::Observer);
    let (a, _, _) = over(Governed::new(f, faulted()));
    let reader = session(&account());
    // An account may read an internal wiki, and is told.
    let fault = a.write_refusal(&reader, &w.internal).expect("told");
    assert_eq!(
        fault.to_string(),
        "governance/policy.yaml, line 4: the file has a key the rules of a wiki do not have"
    );
    // A member of a restricted wiki is told; a stranger to it is told nothing, as for a wiki that is not there.
    assert!(a.write_refusal(&session(&member), &w.restricted).is_some());
    assert!(a.write_refusal(&reader, &w.restricted).is_none());
    assert!(a.write_refusal(&reader, &WikiId::new()).is_none());
    // Nobody is told of a fault there is not.
    let (healthy, _, _) = over(Governed::new(facts(&w, &[]), Rules::defaults()));
    assert!(healthy.write_refusal(&reader, &w.internal).is_none());
}

// ---- the audit record names the rung and the block

#[test]
fn the_audit_record_of_a_consequential_decision_names_the_rung_and_the_block() {
    let w = wikis();
    let maintainer = account();
    let (a, _, sink) = over(Governed::new(
        facts(&w, &[(&maintainer, Role::Maintainer)]),
        rules_with("entry: observer\n"),
    ));
    let _ = a.authorize(&session(&maintainer), Action::Accept, &on(&w.internal));
    let _ = a.authorize(&session(&maintainer), Action::Admin, &on(&w.internal));
    let _ = a.authorize(&Principal::Anonymous, Action::Accept, &on(&w.internal));
    let stranger = account();
    let _ = a.authorize(&session(&stranger), Action::Accept, &on(&w.internal));
    let events = sink.events.lock().unwrap().clone();
    let details: Vec<&str> = events.iter().map(|e| e.detail.as_str()).collect();
    let short = short_block(BLOCK);
    assert_eq!(short.len(), 12);
    assert_eq!(
        details,
        [
            format!("rung=maintainer; policy={short}"),
            format!("rung=maintainer; policy={short}"),
            "rung=none; policy=defaults".to_owned(),
            // `entry: observer` gives the stranger a rung, and the decision names it.
            format!("rung=observer; policy={short}"),
        ]
    );
    // The record never carries a path.
    assert!(events.iter().all(|e| !e.detail.contains('/')));
}

#[test]
fn a_wiki_with_no_governance_is_recorded_under_defaults() {
    let w = wikis();
    let admin = account();
    let (a, _, sink) = over(Governed::new(
        facts(&w, &[(&admin, Role::Admin)]),
        Rules::defaults(),
    ));
    let _ = a.authorize(&session(&admin), Action::Accept, &on(&w.public));
    let events = sink.events.lock().unwrap().clone();
    assert_eq!(
        events.first().map(|e| e.detail.as_str()),
        Some("rung=admin; policy=defaults")
    );
}

#[test]
fn an_absent_wiki_and_a_restricted_one_the_caller_is_no_member_of_are_recorded_alike() {
    let w = wikis();
    let (a, _, sink) = over(Governed::new(
        facts(&w, &[]),
        rules_with("entry: contributor\n"),
    ));
    let stranger = session(&account());
    let _ = a.authorize(&stranger, Action::Accept, &on(&w.restricted));
    let _ = a.authorize(&stranger, Action::Accept, &on(&WikiId::new()));
    let events = sink.events.lock().unwrap().clone();
    let [hidden, gone] = events.as_slice() else {
        panic!("two events")
    };
    assert_eq!(hidden.detail, gone.detail);
    assert_eq!(hidden.detail, "rung=none; policy=defaults");
}

#[test]
fn the_standing_of_an_account_is_its_rung_and_the_block_of_the_rules() {
    let w = wikis();
    let reviewer = account();
    let (a, _, _) = over(Governed::new(
        facts(&w, &[(&reviewer, Role::Reviewer)]),
        rules_with("entry: contributor\n"),
    ));
    let s = a
        .standing(&session(&reviewer), &w.internal)
        .expect("standing");
    assert_eq!(
        s,
        Standing {
            rung: Some(Role::Reviewer),
            policy_block: Some(BLOCK.to_owned())
        }
    );
    // Entry counts for an account with no grant; a restricted wiki's non-member has none.
    let stranger = session(&account());
    assert_eq!(
        a.standing(&stranger, &w.internal).expect("standing").rung,
        Some(Role::Contributor)
    );
    assert_eq!(
        a.standing(&stranger, &w.restricted).expect("standing").rung,
        None
    );
    assert_eq!(
        a.standing(&Principal::Anonymous, &w.public)
            .expect("standing")
            .rung,
        None
    );
}

// ---- when the rules are asked for, and when they cannot be

#[test]
fn the_rules_are_asked_for_only_by_a_write_of_an_account_that_can_be_a_member() {
    let w = wikis();
    let member = account();
    let (a, source, _) = over(Governed::new(
        facts(&w, &[(&member, Role::Maintainer)]),
        rules_with("entry: contributor\n"),
    ));
    let asked = || source.asked.load(Ordering::SeqCst);
    // Reads never ask: nothing in a policy changes who reads (and a read must not cost a walk of the repository).
    for p in [Principal::Anonymous, session(&member), session(&account())] {
        let _ = a.authorize(&p, Action::Read, &on(&w.public));
    }
    assert_eq!(asked(), 0);
    // Neither does anonymous, the system, an absent wiki or a stranger to a restricted wiki.
    let _ = a.authorize(&Principal::Anonymous, Action::Propose, &on(&w.public));
    let _ = a.authorize(
        &Principal::system(SystemLabel::inklkunn()),
        Action::Accept,
        &on(&w.public),
    );
    let _ = a.authorize(&session(&member), Action::Propose, &on(&WikiId::new()));
    let _ = a.authorize(&session(&account()), Action::Propose, &on(&w.restricted));
    assert_eq!(asked(), 0);
    // A write of an account in a wiki it may be a member of does.
    let _ = a.authorize(&session(&member), Action::Propose, &on(&w.internal));
    assert_eq!(asked(), 1);
    let _ = a.authorize(&session(&account()), Action::Propose, &on(&w.internal));
    assert_eq!(asked(), 2);
}

#[test]
fn rules_that_cannot_be_read_deny_a_write_and_say_so_in_the_trail_and_leave_a_read_alone() {
    let w = wikis();
    let admin = account();
    let (a, source, sink) = over(Governed::new(
        facts(&w, &[(&admin, Role::Admin)]),
        Rules::defaults(),
    ));
    *source.rules.lock().unwrap() = Err(SourceError("the repository is busy".into()));
    let p = session(&admin);
    for action in Action::ALL {
        assert_eq!(
            a.authorize(&p, action, &on(&w.internal)).is_allowed(),
            action == Action::Read,
            "{}",
            action.name()
        );
    }
    let events = sink.events.lock().unwrap().clone();
    // The five consequential actions, each failed with a detail that carries no cause.
    assert_eq!(events.len(), 5);
    assert!(events.iter().all(|e| e.outcome == AuditOutcome::Failed));
    assert!(events.iter().all(|e| !e.detail.contains("busy")));
    // `standing` says the same: could not be read.
    assert!(a.standing(&p, &w.internal).is_err());
    assert!(a.write_refusal(&p, &w.internal).is_none());
}

#[test]
fn a_source_with_no_governance_decides_as_a_wiki_with_none() {
    // `Facts` does not implement `rules`: the default answer is the defaults, which are 0.3.0's table.
    let w = wikis();
    let maintainer = account();
    let (a, _, _) = authorizer(
        facts(&w, &[(&maintainer, Role::Maintainer)]),
        Sink::default(),
    );
    assert!(
        a.authorize(&session(&maintainer), Action::Accept, &on(&w.internal))
            .is_allowed()
    );
    assert_eq!(*a.rules(&w.internal).expect("rules"), Rules::defaults());
    assert_eq!(Entry::None, Rules::defaults().policy().entry());
}
