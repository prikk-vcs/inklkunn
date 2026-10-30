//! Drafts (RFC 004 D-1, PR-10): unsent, visible to their author only, kept as a working copy that survives
//! a restart (and, since it belongs to the account and not to a session, a session's expiry).

use std::sync::Arc;

use inklkunn_store::{ProposalFilter, ProposalState, SqliteStore};

use super::support::{Bases, Env, T0, edit, on_both, set_now};
use crate::authorize::Principal;
use crate::proposal::{NewComment, ProposalError, QuotaConfig};

#[test]
fn a_draft_is_invisible_to_every_other_account_and_to_visitors() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let d = e.draft(&e.author, "Secret plans", "pages/a.md");
        // The author sees it, resumes it, and finds it in their list.
        assert_eq!(
            s.get(&e.author.principal, w, &d.id).unwrap().state,
            ProposalState::Draft
        );
        assert_eq!(
            s.draft(&e.author.principal, w, &d.id).unwrap().files.len(),
            1
        );
        assert_eq!(
            s.list(&e.author.principal, w, &ProposalFilter::default())
                .unwrap()
                .len(),
            1
        );
        // Everyone else, whatever their role, finds nothing: not "forbidden", absent.
        for who in [
            &e.reviewer,
            &e.maintainer,
            &e.admin,
            &e.observer,
            &e.outsider,
        ] {
            let p = &who.principal;
            assert_eq!(s.get(p, w, &d.id), Err(ProposalError::NotFound), "get");
            assert_eq!(s.draft(p, w, &d.id), Err(ProposalError::NotFound), "draft");
            assert_eq!(
                s.revisions(p, w, &d.id),
                Err(ProposalError::NotFound),
                "revisions"
            );
            assert_eq!(
                s.comments(p, w, &d.id),
                Err(ProposalError::NotFound),
                "comments"
            );
            assert_eq!(
                s.decisions(p, w, &d.id),
                Err(ProposalError::NotFound),
                "decisions"
            );
            assert_eq!(
                s.transitions(p, w, &d.id),
                Err(ProposalError::NotFound),
                "transitions"
            );
            let note = NewComment {
                body: "hello".into(),
                ..NewComment::default()
            };
            // Acts are authorized first: those who may not do the act are told so (a fact about them), and
            // those who may find the draft absent.
            let may_comment = [&e.reviewer, &e.maintainer, &e.admin]
                .iter()
                .any(|x| x.id == who.id);
            assert_eq!(
                s.comment(p, w, &d.id, note),
                Err(if may_comment {
                    ProposalError::NotFound
                } else {
                    ProposalError::NotAllowed
                }),
                "comment"
            );
            let may_accept = [&e.maintainer, &e.admin].iter().any(|x| x.id == who.id);
            assert_eq!(
                s.decline(p, w, &d.id, "no"),
                Err(if may_accept {
                    ProposalError::NotFound
                } else {
                    ProposalError::NotAllowed
                }),
                "decline"
            );
        }
        assert_eq!(
            s.get(&Principal::Anonymous, w, &d.id),
            Err(ProposalError::NotFound)
        );
        // It is in nobody's list, and in no count of what the wiki holds.
        for who in [&e.reviewer, &e.maintainer, &e.admin, &e.outsider] {
            assert!(
                s.list(&who.principal, w, &ProposalFilter::default())
                    .unwrap()
                    .is_empty()
            );
        }
        // A withdrawn draft was never sent: it stays the author's alone.
        s.withdraw(&e.author.principal, w, &d.id, "Changed my mind.")
            .unwrap();
        assert_eq!(
            s.get(&e.reviewer.principal, w, &d.id),
            Err(ProposalError::NotFound)
        );
        assert_eq!(
            s.get(&e.author.principal, w, &d.id).unwrap().state,
            ProposalState::Withdrawn
        );
        // Submitted, it is everyone's to see.
        let sent = e.held(&e.author, "Public plans", "pages/b.md");
        assert_eq!(
            s.get(&e.outsider.principal, w, &sent.id).unwrap().id,
            sent.id
        );
    });
}

#[test]
fn a_draft_is_saved_and_resumed_as_often_as_wanted_without_becoming_a_revision() {
    on_both(|e| {
        let (w, s, a) = (&e.wiki, &e.service, &e.author);
        let d = e.draft(a, "First words", "pages/a.md");
        for (n, text) in ["one", "two", "three"].iter().enumerate() {
            set_now(T0 + 1 + i64::try_from(n).unwrap());
            let saved = s
                .save_draft(
                    &a.principal,
                    w,
                    &d.id,
                    edit(&format!("Words {n}"), "pages/a.md", text),
                )
                .unwrap();
            assert_eq!(saved.summary, format!("Words {n}"));
        }
        let resumed = s.draft(&a.principal, w, &d.id).unwrap();
        assert_eq!(
            resumed.files.first().and_then(|f| f.text.as_deref()),
            Some("three")
        );
        assert!(
            s.revisions(&a.principal, w, &d.id).unwrap().is_empty(),
            "saving a draft makes no revision: three saves do not use up the revision quota"
        );
        assert_eq!(
            s.transitions(&a.principal, w, &d.id).unwrap().len(),
            1,
            "only the creation is a step"
        );
        // Once sent, there is no draft to save.
        s.submit(&a.principal, w, &d.id).unwrap();
        assert_eq!(
            s.save_draft(&a.principal, w, &d.id, edit("Late", "pages/a.md", "x")),
            Err(ProposalError::NotInThisState {
                op: inklkunn_store::ProposalOp::SaveDraft,
                state: ProposalState::HeldForReview
            })
        );
        assert_eq!(s.revisions(&a.principal, w, &d.id).unwrap().len(), 1);
        assert_eq!(
            s.revisions(&a.principal, w, &d.id)
                .unwrap()
                .first()
                .and_then(|r| r.content.files.first().and_then(|f| f.text.clone())),
            Some("three".to_owned()),
            "revision 1 is the last working copy"
        );
    });
}

struct TempDir(std::path::PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The records survive the process: a service built again over the reopened database finds the draft where
/// its author left it, nobody else sees it, and the author goes on. (A draft belongs to the account, not to a
/// session: its survival of a session's expiry is this, and the page's half is handoff 004 T3.)
#[test]
fn a_draft_survives_a_restart_and_is_still_its_authors_alone() {
    let dir = TempDir(std::env::temp_dir().join(format!(
        "inklkunn-drafts-{}-{}",
        std::process::id(),
        inklkunn_store::AccountId::new()
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    let db = dir.0.join("store.sqlite");
    let (author_id, stranger_id, wiki, proposal) = {
        let store = Arc::new(SqliteStore::open(&db).unwrap());
        let e = Env::over(store, QuotaConfig::default());
        let d = e.draft(&e.author, "Half a thought", "pages/a.md");
        e.service
            .save_draft(
                &e.author.principal,
                &e.wiki,
                &d.id,
                edit("Most of a thought", "pages/a.md", "saved text"),
            )
            .unwrap();
        (
            e.author.id.clone(),
            e.reviewer.id.clone(),
            e.wiki.clone(),
            d.id,
        )
    };

    // A new process: the store reopened, a new service over it, the same accounts signed in again.
    let store = Arc::new(SqliteStore::open(&db).unwrap());
    let bases = Arc::new(Bases::default());
    bases.move_to(1);
    set_now(T0 + 100_000);
    let service = Env::service_over(&store, &bases, QuotaConfig::default());
    let authority = crate::authorize::IdentityAuthority::for_tests();
    let author = Principal::session(&authority, author_id);
    let stranger = Principal::session(&authority, stranger_id);

    let draft = service.get(&author, &wiki, &proposal).unwrap();
    assert_eq!(draft.state, ProposalState::Draft);
    assert_eq!(draft.summary, "Most of a thought");
    let content = service.draft(&author, &wiki, &proposal).unwrap();
    assert_eq!(
        content.files.first().and_then(|f| f.text.as_deref()),
        Some("saved text")
    );
    assert_eq!(
        service.get(&stranger, &wiki, &proposal),
        Err(ProposalError::NotFound)
    );
    // And it goes on to be sent.
    let sent = service.submit(&author, &wiki, &proposal).unwrap();
    assert_eq!(sent.state, ProposalState::HeldForReview);
    assert_eq!(
        service.get(&stranger, &wiki, &proposal).unwrap().id,
        proposal
    );
}
