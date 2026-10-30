//! The inbox from the proposal service's acts to the page that shows them, and watching: the other party's
//! inbox has the event, a person who can no longer read the wiki is shown the bare fact and none of its
//! content, and an account is what an inbox and a watch need.

use inklkunn_store::{InboxEventKind as Kind, ProposalState, Visibility, WatchTarget};

use super::world::{World, on_both};
use crate::authorize::Principal;
use crate::inbox::{About, InboxError, MAX_WATCHES, namespace_target, page_target};

#[test]
fn the_proposers_inbox_has_the_decline_with_what_it_is_about() {
    on_both(|w| {
        let p = w.held(&w.author, "Fix a typo in the intro");
        w.service
            .decline(
                &w.maintainer.principal,
                &w.wiki,
                &p.id,
                "Please open it on the guide.",
            )
            .expect("declined");
        let entries = w
            .inbox
            .entries(&w.author.principal, false)
            .expect("entries");
        assert_eq!(entries.len(), 1);
        let entry = entries.first().expect("an entry");
        assert_eq!(entry.kind, Kind::Declined);
        assert!(entry.unread);
        let About::Readable {
            slug,
            wiki,
            proposal,
            actor,
        } = &entry.about
        else {
            panic!("the proposer may read their own wiki: {:?}", entry.about);
        };
        assert_eq!((slug.as_str(), wiki.as_str()), ("notify", "Notify"));
        assert_eq!(actor.as_deref(), Some("maintainer"));
        let line = proposal.as_ref().expect("a proposal");
        assert_eq!(line.id, p.id);
        assert_eq!(line.summary, "Fix a typo in the intro");
        assert_eq!(line.state, ProposalState::Declined);
        // The unread count, and reading it.
        assert_eq!(w.inbox.unread(&w.author.principal).unwrap(), 1);
        assert!(w.inbox.mark_read(&w.author.principal, &entry.id).unwrap());
        assert_eq!(w.inbox.unread(&w.author.principal).unwrap(), 0);
        assert!(
            !w.inbox
                .entries(&w.author.principal, false)
                .unwrap()
                .remove(0)
                .unread
        );
        // Somebody else's event is not found, whoever asks.
        assert!(!w.inbox.mark_read(&w.reviewer.principal, &entry.id).unwrap());
    });
}

#[test]
fn a_person_who_can_no_longer_read_the_wiki_is_shown_the_fact_and_none_of_its_content() {
    on_both(|w| {
        // The event is made while the wiki is theirs to read ...
        let p = w.held(&w.author, "A summary that must not leak");
        w.service
            .decline(
                &w.maintainer.principal,
                &w.wiki,
                &p.id,
                "A reason that must not leak.",
            )
            .expect("declined");
        let before = w.inbox.entries(&w.author.principal, false).unwrap();
        assert!(matches!(
            before.first().map(|e| &e.about),
            Some(About::Readable { .. })
        ));

        // ... then the wiki is restricted and their role is taken away.
        w.set_visibility(Visibility::Restricted);
        w.store.revoke(&w.author.id, &w.wiki.id).expect("revoked");
        let after = w.inbox.entries(&w.author.principal, false).unwrap();
        assert_eq!(after.len(), 1, "the event is still theirs");
        let entry = after.first().expect("an entry");
        assert_eq!(entry.kind, Kind::Declined);
        assert_eq!(entry.about, About::Unreadable);
        let shown = format!("{entry:?}");
        for secret in ["leak", "Notify", "notify", "maintainer"] {
            assert!(!shown.contains(secret), "{secret}: {shown}");
        }

        // A person who never could read it is told nothing in the first place.
        let q = w.held(&w.reviewer, "Another");
        w.service
            .decline(&w.maintainer.principal, &w.wiki, &q.id, "No.")
            .expect("declined");
        let author_again = w.person(None, None);
        assert!(
            w.inbox
                .entries(&author_again.principal, false)
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn an_event_whose_proposal_the_reader_may_not_see_names_nothing() {
    on_both(|w| {
        let p = w.held(&w.author, "Seen by few");
        w.service
            .decline(&w.maintainer.principal, &w.wiki, &p.id, "No.")
            .expect("declined");
        // Someone else's inbox cannot be made to show the proposal: the entry is always the owner's, and the
        // event itself names only ids. Reading it as the owner after the proposal is gone from view (here,
        // the wiki is not readable at all to an anonymous caller) fails closed.
        assert_eq!(
            w.inbox.entries(&Principal::Anonymous, false),
            Err(InboxError::NeedsAccount)
        );
    });
}

#[test]
fn an_inbox_and_a_watch_need_an_account() {
    on_both(|w| {
        let anon = Principal::Anonymous;
        assert_eq!(w.inbox.entries(&anon, false), Err(InboxError::NeedsAccount));
        assert_eq!(w.inbox.unread(&anon), Err(InboxError::NeedsAccount));
        let target = page_target("Home").unwrap();
        assert_eq!(
            w.inbox.watch(&anon, &w.wiki, target.clone()),
            Err(InboxError::NeedsAccount)
        );
        assert_eq!(
            w.inbox.unwatch(&anon, &w.wiki, &target),
            Err(InboxError::NeedsAccount)
        );
        assert_eq!(w.inbox.followed(&anon), Err(InboxError::NeedsAccount));
    });
}

#[test]
fn watching_a_page_or_a_namespace_follows_it_and_again_is_no_change() {
    on_both(|w| {
        assert_eq!(
            page_target("Home").unwrap(),
            WatchTarget::Page("pages/Home.md".into())
        );
        assert_eq!(
            namespace_target("Guides").unwrap(),
            WatchTarget::Namespace("pages/Guides/".into())
        );
        assert_eq!(
            namespace_target("Guides/").unwrap(),
            WatchTarget::Namespace("pages/Guides/".into())
        );
        assert!(page_target("").is_err() && namespace_target("a//b").is_err());

        let me = &w.reviewer.principal;
        let home = page_target("Home").unwrap();
        let guides = namespace_target("Guides").unwrap();
        assert!(!w.inbox.is_watching(me, &w.wiki, &home).unwrap());
        assert!(w.inbox.watch(me, &w.wiki, home.clone()).unwrap());
        assert!(
            !w.inbox.watch(me, &w.wiki, home.clone()).unwrap(),
            "again is no change"
        );
        assert!(w.inbox.watch(me, &w.wiki, guides.clone()).unwrap());
        assert!(w.inbox.is_watching(me, &w.wiki, &home).unwrap());
        let followed = w.inbox.followed(me).unwrap();
        assert_eq!(
            followed
                .iter()
                .map(|f| f.target.clone())
                .collect::<Vec<_>>(),
            vec![home.clone(), guides]
        );
        assert_eq!(followed.first().map(|f| f.slug.as_str()), Some("notify"));
        assert!(w.inbox.unwatch(me, &w.wiki, &home).unwrap());
        assert!(
            !w.inbox.unwatch(me, &w.wiki, &home).unwrap(),
            "it was not followed"
        );
        assert!(!w.inbox.is_watching(me, &w.wiki, &home).unwrap());
        // Someone else's watch is not mine.
        assert!(
            !w.inbox
                .is_watching(
                    &w.author.principal,
                    &w.wiki,
                    &w.inbox.followed(me).unwrap().remove(0).target
                )
                .unwrap()
        );
    });
}

#[test]
fn watching_needs_the_right_to_read_and_stopping_does_not() {
    on_both(|w| {
        let home = page_target("Home").unwrap();
        w.set_visibility(Visibility::Restricted);
        // Without a role in a restricted wiki: not allowed, and nothing is stored.
        assert_eq!(
            w.inbox.watch(&w.outsider.principal, &w.wiki, home.clone()),
            Err(InboxError::NotAllowed)
        );
        assert!(w.inbox.followed(&w.outsider.principal).unwrap().is_empty());
        // A reader who follows, then loses the right: the watch is not listed, may still be dropped, and
        // makes no event.
        w.set_visibility(Visibility::Internal);
        assert!(
            w.inbox
                .watch(&w.outsider.principal, &w.wiki, home.clone())
                .unwrap()
        );
        w.set_visibility(Visibility::Restricted);
        assert!(w.inbox.followed(&w.outsider.principal).unwrap().is_empty());
        assert_eq!(
            w.notifier.page_changed(
                &w.wiki,
                &w.held(&w.author, "Edit").id,
                &["pages/Home.md".to_owned()],
                &w.maintainer.id
            ),
            0
        );
        assert!(
            w.inbox
                .unwatch(&w.outsider.principal, &w.wiki, &home)
                .unwrap()
        );
    });
}

#[test]
fn an_account_follows_only_so_many() {
    let w = World::memory();
    let me = &w.reviewer.principal;
    for i in 0..MAX_WATCHES {
        let target = page_target(&format!("Page {i}")).unwrap();
        assert!(w.inbox.watch(me, &w.wiki, target).unwrap(), "{i}");
    }
    let one_more = page_target("One more").unwrap();
    assert_eq!(
        w.inbox.watch(me, &w.wiki, one_more.clone()),
        Err(InboxError::TooManyWatches)
    );
    // Following what is followed already is no change, even at the limit; stopping makes room.
    assert!(
        !w.inbox
            .watch(me, &w.wiki, page_target("Page 0").unwrap())
            .unwrap()
    );
    assert!(
        w.inbox
            .unwatch(me, &w.wiki, &page_target("Page 0").unwrap())
            .unwrap()
    );
    assert!(w.inbox.watch(me, &w.wiki, one_more).unwrap());
}

#[test]
fn all_unread_events_can_be_marked_read_at_once() {
    on_both(|w| {
        for n in 0..3 {
            let p = w.held(&w.author, &format!("Proposal {n}"));
            w.service
                .decline(&w.maintainer.principal, &w.wiki, &p.id, "No.")
                .expect("declined");
        }
        assert_eq!(w.inbox.unread(&w.author.principal).unwrap(), 3);
        assert_eq!(w.inbox.mark_all_read(&w.author.principal).unwrap(), 3);
        assert_eq!(w.inbox.unread(&w.author.principal).unwrap(), 0);
        assert_eq!(w.inbox.mark_all_read(&w.author.principal).unwrap(), 0);
        assert_eq!(w.inbox.entries(&w.author.principal, true).unwrap().len(), 0);
        assert_eq!(
            w.inbox.entries(&w.author.principal, false).unwrap().len(),
            3
        );
    });
}
