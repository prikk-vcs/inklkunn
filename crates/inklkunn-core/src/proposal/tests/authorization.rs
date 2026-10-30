//! Every act is authorized first, through the one `authorize()`, before any record is read; only an account
//! acts; a proposal of another wiki, or someone else's draft, is as absent as one that does not exist.

use inklkunn_store::{DecisionKind, ProposalFilter, Role, Scope};

use super::support::{edit, on_both};
use crate::authorize::Principal;
use crate::proposal::{NewComment, ProposalError};

#[test]
fn each_act_needs_the_action_the_rfc_names_and_no_less() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        // Propose: a contributor and above. An observer and a person with no role do not.
        for who in [&e.observer, &e.outsider] {
            assert_eq!(
                s.create(&who.principal, w, None, edit("One", "pages/a.md", "x")),
                Err(ProposalError::NotAllowed)
            );
        }
        let p = e.held(&e.author, "One", "pages/a.md");

        // Comment: a questioner and above (an observer follows, and does not take part).
        let note = || NewComment {
            body: "A question".into(),
            ..NewComment::default()
        };
        assert_eq!(
            s.comment(&e.observer.principal, w, &p.id, note()),
            Err(ProposalError::NotAllowed)
        );
        assert_eq!(
            s.comment(&e.outsider.principal, w, &p.id, note()),
            Err(ProposalError::NotAllowed)
        );
        let questioner = e.person(Some(Role::Questioner));
        s.comment(&questioner.principal, w, &p.id, note()).unwrap();
        // A questioner asks; a contributor does not review.
        assert_eq!(
            s.decide(&e.author.principal, w, &p.id, 1, DecisionKind::Approve, ""),
            Err(ProposalError::NotAllowed)
        );
        assert_eq!(
            s.decide(
                &questioner.principal,
                w,
                &p.id,
                1,
                DecisionKind::Approve,
                ""
            ),
            Err(ProposalError::NotAllowed)
        );
        s.decide(
            &e.reviewer.principal,
            w,
            &p.id,
            1,
            DecisionKind::Approve,
            "",
        )
        .unwrap();

        // Accept (decline, send back): a maintainer and above, not a reviewer.
        for result in [
            s.decline(&e.reviewer.principal, w, &p.id, "no"),
            s.ask_for_revision(&e.reviewer.principal, w, &p.id, "no"),
            s.decline(&e.author.principal, w, &p.id, "no"),
        ] {
            assert_eq!(result, Err(ProposalError::NotAllowed));
        }
        s.decline(&e.maintainer.principal, w, &p.id, "yes").unwrap();
    });
}

#[test]
fn only_an_account_acts_and_a_token_is_limited_to_its_scope() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let p = e.held(&e.author, "One", "pages/a.md");
        // The visitor with nothing, and the service itself, are not proposers, reviewers or acceptors.
        let system = Principal::system(inklkunn_signer::message::SystemLabel::inklkunn());
        for who in [&Principal::Anonymous, &system] {
            assert_eq!(
                s.create(who, w, None, edit("One", "pages/a.md", "x")),
                Err(ProposalError::NotAllowed)
            );
            assert_eq!(
                s.decline(who, w, &p.id, "no"),
                Err(ProposalError::NotAllowed)
            );
            assert_eq!(
                s.withdraw(who, w, &p.id, "no"),
                Err(ProposalError::NotAllowed)
            );
            assert_eq!(
                s.decide(who, w, &p.id, 1, DecisionKind::Approve, ""),
                Err(ProposalError::NotAllowed)
            );
        }
        // A token carries no more than its scope: a `read` token of a maintainer does not decline, a `propose`
        // token of a maintainer does not accept, and an `accept` token does.
        let read_only = e.token(&e.maintainer, Scope::Read);
        assert_eq!(
            s.decline(&read_only, w, &p.id, "no"),
            Err(ProposalError::NotAllowed)
        );
        assert_eq!(
            s.create(&read_only, w, None, edit("One", "pages/b.md", "x")),
            Err(ProposalError::NotAllowed)
        );
        let propose_only = e.token(&e.maintainer, Scope::Propose);
        assert_eq!(
            s.decline(&propose_only, w, &p.id, "no"),
            Err(ProposalError::NotAllowed)
        );
        assert!(
            s.create(&propose_only, w, None, edit("Two", "pages/b.md", "x"))
                .is_ok()
        );
        let accept = e.token(&e.maintainer, Scope::Accept);
        assert!(s.decline(&accept, w, &p.id, "yes").is_ok());
    });
}

#[test]
fn only_the_proposer_revises_withdraws_or_submits() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let p = e.held(&e.author, "One", "pages/a.md");
        let other = e.person(Some(Role::Contributor));
        for result in [
            s.revise(
                &other.principal,
                w,
                &p.id,
                edit("One", "pages/a.md", "mine"),
            )
            .map(|_| ()),
            s.withdraw(&other.principal, w, &p.id, "not yours")
                .map(|_| ()),
            // Not even a maintainer withdraws what is not theirs: they decline it.
            s.withdraw(&e.maintainer.principal, w, &p.id, "not yours")
                .map(|_| ()),
            s.revise(
                &e.maintainer.principal,
                w,
                &p.id,
                edit("One", "pages/a.md", "mine"),
            )
            .map(|_| ()),
        ] {
            assert_eq!(result, Err(ProposalError::NotAllowed));
        }
        let draft = e.draft(&e.author, "Two", "pages/b.md");
        // Another person's draft is absent, not forbidden.
        assert_eq!(
            s.submit(&other.principal, w, &draft.id),
            Err(ProposalError::NotFound)
        );
        assert_eq!(
            s.save_draft(
                &other.principal,
                w,
                &draft.id,
                edit("Two", "pages/b.md", "mine")
            ),
            Err(ProposalError::NotFound)
        );
        // Someone else's submitted proposal exists for them, and is not theirs to submit.
        assert_eq!(
            s.submit(&other.principal, w, &p.id),
            Err(ProposalError::NotAllowed)
        );
    });
}

#[test]
fn a_proposal_of_another_wiki_is_as_absent_as_one_that_does_not_exist() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let p = e.held(&e.author, "One", "pages/a.md");
        let other_wiki = e
            .store
            .create(inklkunn_store::NewWiki::new(
                inklkunn_store::Slug::new("elsewhere").unwrap(),
                inklkunn_store::DisplayName::new("Elsewhere").unwrap(),
            ))
            .unwrap();
        // The same people have the same roles nowhere else; make them maintainers there to isolate the check.
        e.store
            .grant(inklkunn_store::RoleGrant {
                account: e.maintainer.id.clone(),
                wiki: other_wiki.id.clone(),
                role: Role::Maintainer,
                granted_by: None,
                granted_at: 0,
            })
            .unwrap();
        assert_eq!(
            s.decline(&e.maintainer.principal, &other_wiki, &p.id, "no"),
            Err(ProposalError::NotFound)
        );
        assert_eq!(
            s.get(&e.maintainer.principal, &other_wiki, &p.id),
            Err(ProposalError::NotFound)
        );
        assert_eq!(
            s.get(
                &e.maintainer.principal,
                w,
                &inklkunn_store::ProposalId::new()
            ),
            Err(ProposalError::NotFound),
            "a proposal that does not exist answers the same"
        );
        assert!(
            s.list(
                &e.maintainer.principal,
                &other_wiki,
                &ProposalFilter::default()
            )
            .unwrap()
            .is_empty()
        );
    });
}

#[test]
fn reading_follows_the_wikis_visibility() {
    on_both(|e| {
        let (w, s) = (&e.wiki, &e.service);
        let p = e.held(&e.author, "One", "pages/a.md");
        let draft = e.draft(&e.author, "Two", "pages/b.md");
        // The wiki is public: a visitor reads what was submitted, and sees no draft at all.
        assert_eq!(s.get(&Principal::Anonymous, w, &p.id).unwrap().id, p.id);
        assert_eq!(
            s.get(&Principal::Anonymous, w, &draft.id),
            Err(ProposalError::NotFound)
        );
        let listed = s
            .list(&Principal::Anonymous, w, &ProposalFilter::default())
            .unwrap();
        assert_eq!(
            listed.iter().map(|x| &x.id).collect::<Vec<_>>(),
            vec![&p.id]
        );
        // A reviewer sees the same; the author sees their draft too.
        let mine = s
            .list(&e.author.principal, w, &ProposalFilter::default())
            .unwrap();
        assert_eq!(mine.len(), 2);
        let theirs = s
            .list(&e.reviewer.principal, w, &ProposalFilter::default())
            .unwrap();
        assert_eq!(theirs.len(), 1);
    });
}
