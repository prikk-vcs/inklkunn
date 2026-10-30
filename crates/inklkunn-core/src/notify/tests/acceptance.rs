//! The events of an acceptance, which the acceptance calls once the seal is done (handoff 004 T4): the
//! proposal accepted, and the watched pages that changed. Tested against a channel that only remembers.
//! **A watch is told about a change that is in the history and about nothing else.**

use std::sync::Arc;

use inklkunn_store::{InboxEventKind as Kind, WatchRecord, WatchTarget};

use super::world::{Recorder, World};
use crate::proposal::ProposalError;

fn watch(w: &World, who: &super::world::Person, target: WatchTarget) {
    w.store
        .watch(&WatchRecord {
            account: who.id.clone(),
            wiki: w.wiki.id.clone(),
            target,
            created_at: 1,
        })
        .expect("a watch");
}

fn page(path: &str) -> WatchTarget {
    WatchTarget::Page(path.to_owned())
}

fn namespace(prefix: &str) -> WatchTarget {
    WatchTarget::Namespace(prefix.to_owned())
}

#[test]
fn nothing_but_an_acceptance_tells_a_watch() {
    let recorder = Arc::new(Recorder::default());
    let w = World::recording(recorder.clone());
    watch(&w, &w.reviewer, page("pages/Home.md"));
    watch(&w, &w.maintainer, namespace("pages/"));

    // Everything the service does to a proposal of the watched page, short of accepting it.
    let p = w.held(&w.author, "Edit the home page");
    w.comment(&w.reviewer, &p, "Fine.");
    w.service
        .decide(
            &w.reviewer.principal,
            &w.wiki,
            &p.id,
            1,
            inklkunn_store::DecisionKind::Approve,
            "",
        )
        .unwrap();
    w.service
        .ask_for_revision(&w.maintainer.principal, &w.wiki, &p.id, "Add a source.")
        .unwrap();
    w.service
        .withdraw(&w.author.principal, &w.wiki, &p.id, "Not now.")
        .unwrap();
    let q = w.held(&w.author, "Edit it again");
    w.service
        .decline(&w.maintainer.principal, &w.wiki, &q.id, "Not this way.")
        .unwrap();
    assert!(matches!(
        w.service
            .decline(&w.maintainer.principal, &w.wiki, &q.id, "Twice."),
        Err(ProposalError::NotInThisState { .. })
    ));

    let kinds: Vec<Kind> = recorder.seen().into_iter().map(|e| e.kind).collect();
    assert!(!kinds.is_empty(), "the acts did tell the people involved");
    assert!(!kinds.contains(&Kind::WatchedPageChanged));
    assert!(!kinds.contains(&Kind::Accepted));
}

#[test]
fn the_function_the_acceptance_calls_tells_each_watcher_once_and_not_the_acceptor() {
    let recorder = Arc::new(Recorder::default());
    let w = World::recording(recorder.clone());
    let p = w.held(&w.author, "Edit the home page");
    // Watching the page and its namespace is one watcher, told once; an unrelated page is not told; the
    // acceptor is not told of their own act.
    watch(&w, &w.reviewer, page("pages/Home.md"));
    watch(&w, &w.reviewer, namespace("pages/"));
    watch(&w, &w.author, page("pages/Other.md"));
    watch(&w, &w.maintainer, page("pages/Home.md"));
    watch(&w, &w.observer, namespace("pages/Guides/"));
    let before = recorder.seen().len();

    let told = w.notifier.page_changed(
        &w.wiki,
        &p.id,
        &["pages/Home.md".to_owned()],
        &w.maintainer.id,
    );
    assert_eq!(told, 1);
    let events = recorder.seen().split_off(before);
    assert_eq!(events.len(), 1);
    let event = events.first().expect("an event");
    assert_eq!(event.account, w.reviewer.id);
    assert_eq!(event.kind, Kind::WatchedPageChanged);
    assert_eq!(event.proposal.as_ref(), Some(&p.id));
    assert_eq!(event.actor.as_ref(), Some(&w.maintainer.id));
    assert_eq!(event.wiki, w.wiki.id);

    // A change to several pages is still one event for a watcher of two of them.
    watch(&w, &w.observer, page("pages/Guides/Setup.md"));
    let before = recorder.seen().len();
    let told = w.notifier.page_changed(
        &w.wiki,
        &p.id,
        &[
            "pages/Guides/Setup.md".to_owned(),
            "pages/Guides/Use.md".to_owned(),
        ],
        &w.maintainer.id,
    );
    assert_eq!(
        told, 2,
        "the reviewer's namespace and the observer's two watches, once each"
    );
    let who: Vec<_> = recorder
        .seen()
        .split_off(before)
        .into_iter()
        .map(|e| e.account)
        .collect();
    assert!(who.contains(&w.observer.id) && who.contains(&w.reviewer.id));
}

#[test]
fn a_watcher_who_may_not_read_the_page_is_not_told() {
    let recorder = Arc::new(Recorder::default());
    let w = World::recording(recorder.clone());
    let p = w.held(&w.author, "Edit the home page");
    watch(&w, &w.outsider, page("pages/Home.md"));
    watch(&w, &w.reviewer, page("pages/Home.md"));
    // The wiki is restricted after they began to watch: the one with no role reads nothing.
    w.set_visibility(inklkunn_store::Visibility::Restricted);
    let before = recorder.seen().len();
    let told = w.notifier.page_changed(
        &w.wiki,
        &p.id,
        &["pages/Home.md".to_owned()],
        &w.maintainer.id,
    );
    assert_eq!(told, 1);
    let who: Vec<_> = recorder
        .seen()
        .split_off(before)
        .into_iter()
        .map(|e| e.account)
        .collect();
    assert_eq!(who, vec![w.reviewer.id.clone()]);
}

#[test]
fn an_accepted_proposal_tells_its_proposer_and_not_the_acceptor() {
    let recorder = Arc::new(Recorder::default());
    let w = World::recording(recorder.clone());
    let p = w.held(&w.author, "Edit the home page");
    let before = recorder.seen().len();
    w.notifier.accepted(&w.wiki, &p, &w.maintainer.id);
    let events = recorder.seen().split_off(before);
    assert_eq!(events.len(), 1);
    assert_eq!(events.first().map(|e| e.kind), Some(Kind::Accepted));
    assert_eq!(
        events.first().map(|e| e.account.clone()),
        Some(w.author.id.clone())
    );
    // The acceptor accepting their own proposal is told nothing about it.
    let own = w.held(&w.maintainer, "My own edit");
    let before = recorder.seen().len();
    w.notifier.accepted(&w.wiki, &own, &w.maintainer.id);
    assert_eq!(recorder.seen().len(), before);
}

#[test]
fn a_channel_that_fails_is_counted_and_never_fails_the_act() {
    struct Broken;
    impl crate::notify::Channel for Broken {
        fn name(&self) -> &'static str {
            "broken"
        }
        fn deliver(
            &self,
            _: &inklkunn_store::InboxEvent,
        ) -> Result<(), crate::notify::ChannelError> {
            Err(crate::notify::ChannelError("down".to_owned()))
        }
    }
    let w = World::memory();
    // Replace the notifier by one whose only channel is broken.
    let authorizer = crate::authorize::Authorizer::new(
        w.facts.clone(),
        Arc::new(inklkunn_store::MemoryAudit::new()),
    );
    let broken = Arc::new(
        crate::notify::Notifier::new(
            w.store.clone(),
            w.store.clone(),
            w.store.clone(),
            w.store.clone(),
            authorizer,
        )
        .with_channels(vec![Arc::new(Broken)]),
    );
    let p = w.held(&w.author, "Fix a typo");
    broken.declined(&w.wiki, &p, &w.maintainer.id);
    assert_eq!(broken.failures(), 1);
    assert!(w.kinds(&w.author).is_empty());
}
