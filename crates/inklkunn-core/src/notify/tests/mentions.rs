//! Replies and mentions: who is told of a comment, once, and never the writer; and the mention syntax.

use inklkunn_store::InboxEventKind as Kind;

use super::world::on_both;
use crate::notify::{MAX_MENTION_EVENTS_PER_HOUR, MAX_MENTIONS, mentions};
use crate::proposal::NewComment;

fn names(text: &str) -> Vec<String> {
    mentions(text)
        .iter()
        .map(|l| l.as_str().to_owned())
        .collect()
}

#[test]
fn the_mention_syntax_is_small_and_bounded() {
    assert_eq!(names("@alice and @bob."), ["alice", "bob"]);
    assert_eq!(
        names("thanks @Alice, @alice again"),
        ["alice"],
        "folded, once"
    );
    assert_eq!(names("mail me at a@bob.example"), Vec::<String>::new());
    assert_eq!(names("(@carol)"), ["carol"]);
    assert_eq!(
        names("@ab @"),
        Vec::<String>::new(),
        "a name has 3 characters or more"
    );
    assert_eq!(names("@dave-"), ["dave"], "a trailing mark is punctuation");
    assert_eq!(names("日本語 @eve です"), ["eve"]);
    let many: String = (0..30).map(|i| format!("@user{i:02} ")).collect();
    assert_eq!(names(&many).len(), MAX_MENTIONS);
}

#[test]
fn a_comment_tells_the_proposer_a_reply_tells_the_one_replied_to_and_nobody_is_told_twice() {
    on_both(|w| {
        let p = w.held(&w.author, "Add a section");
        // A reviewer's comment on the author's proposal tells the author.
        let first = w.comment(&w.reviewer, &p, "Could you cite a source?");
        assert_eq!(w.kinds(&w.author), vec![Kind::Replied]);
        assert!(w.kinds(&w.reviewer).is_empty(), "never the writer");
        // The author replies to the reviewer's comment: the reviewer is told, the author is not (their own).
        w.service
            .comment(
                &w.author.principal,
                &w.wiki,
                &p.id,
                NewComment {
                    body: "Added.".to_owned(),
                    parent: Some(first.id.clone()),
                    anchor: None,
                },
            )
            .expect("a reply");
        assert_eq!(w.kinds(&w.reviewer), vec![Kind::Replied]);
        assert_eq!(w.kinds(&w.author), vec![Kind::Replied]);
        // A third person replies to the reviewer's comment: the reviewer (replied to) and the proposer (whose
        // proposal it is) are each told once.
        w.service
            .comment(
                &w.maintainer.principal,
                &w.wiki,
                &p.id,
                NewComment {
                    body: "Agreed.".to_owned(),
                    parent: Some(first.id),
                    anchor: None,
                },
            )
            .expect("a reply");
        assert_eq!(w.kinds(&w.reviewer), vec![Kind::Replied, Kind::Replied]);
        assert_eq!(w.kinds(&w.author), vec![Kind::Replied, Kind::Replied]);
    });
}

/// Corrections P11: one actor cannot fill one person's inbox with mentions. Past the cap in a rolling hour the
/// mentions are dropped (and counted for the operator); the comment, the other kinds of events, other actors
/// and the next hour are all untouched.
#[test]
fn one_actor_can_send_one_person_only_so_many_mentions_in_an_hour() {
    use super::world::{T0, set_now};
    on_both(|w| {
        set_now(T0);
        let p = w.held(&w.author, "Add a section");
        let mentions_of = |w: &super::world::World, who: &super::world::Person| {
            w.kinds(who)
                .iter()
                .filter(|k| **k == Kind::Mentioned)
                .count()
        };
        for n in 0..MAX_MENTION_EVENTS_PER_HOUR + 5 {
            set_now(T0 + i64::try_from(n).unwrap());
            w.comment(&w.reviewer, &p, "@maintainer look at this");
        }
        assert_eq!(mentions_of(w, &w.maintainer), MAX_MENTION_EVENTS_PER_HOUR);
        assert_eq!(w.notifier.suppressed(), 5);
        assert_eq!(
            w.notifier.failures(),
            0,
            "dropping a flood is not a failure"
        );
        // The comments stood, and told the proposer every time: only the mention is capped.
        assert_eq!(
            w.kinds(&w.author)
                .iter()
                .filter(|k| **k == Kind::Replied)
                .count(),
            MAX_MENTION_EVENTS_PER_HOUR + 5
        );
        // Another actor may still mention the same person, and the same actor may mention another.
        w.comment(&w.author, &p, "@maintainer too");
        assert_eq!(
            mentions_of(w, &w.maintainer),
            MAX_MENTION_EVENTS_PER_HOUR + 1
        );
        w.comment(&w.reviewer, &p, "@author and you");
        assert_eq!(
            mentions_of(w, &w.maintainer),
            MAX_MENTION_EVENTS_PER_HOUR + 1
        );
        // When the hour has passed, the actor is heard again.
        set_now(T0 + 4000);
        w.comment(&w.reviewer, &p, "@maintainer once more");
        assert_eq!(
            mentions_of(w, &w.maintainer),
            MAX_MENTION_EVENTS_PER_HOUR + 2
        );
    });
}

#[test]
fn a_mention_tells_the_person_named_if_they_may_read_and_the_writer_learns_nothing() {
    on_both(|w| {
        let p = w.held(&w.author, "Add a section");
        // The maintainer is mentioned: told once, as mentioned. The proposer is told of the comment.
        w.comment(
            &w.reviewer,
            &p,
            "@maintainer, could you look? cc @nobody-here @author",
        );
        assert_eq!(w.kinds(&w.maintainer), vec![Kind::Mentioned]);
        assert_eq!(
            w.kinds(&w.author),
            vec![Kind::Replied],
            "the proposer is told once, though also mentioned"
        );
        // Someone with no role in an internal wiki may read it, and is told ...
        let before = w.kinds(&w.outsider);
        w.comment(&w.reviewer, &p, "@outsider fyi");
        assert_eq!(w.kinds(&w.outsider).len(), before.len() + 1);
        // ... but once the wiki is restricted to those with a role, they are not, and nothing says so.
        w.set_visibility(inklkunn_store::Visibility::Restricted);
        let told = w.kinds(&w.outsider).len();
        w.comment(&w.reviewer, &p, "@outsider again");
        assert_eq!(
            w.kinds(&w.outsider).len(),
            told,
            "a non-reader is told nothing"
        );
        assert_eq!(w.notifier.failures(), 0);
    });
}
