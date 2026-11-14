//! A claim's thread through the proposal service (RFC 005 part 2 D-11; handoff 005 K-T10), on both stores: who may
//! comment is who may comment on a proposal, at the one comment limit; what is read is the wiki's to read; the comment
//! is recorded and nothing else is, and a discussion changes no claim. The claims are real files in the in-memory
//! repository the read service indexes, so *the claim is there* is the knowledge index's answer. Every sentence is
//! invented.

use std::sync::Arc;
use std::time::Duration;

use inklkunn_store::{CommentId, MemoryAudit, ProposalFilter, Role, Scope};

use super::support::{Actor, Env, T0, now, on_both, on_both_with, set_now};
use crate::authorize::{Authorizer, Principal, StoreAuthz};
use crate::knowledge::{
    Bearing, Claim, ClaimId, CommunityStatus, Expression, Lifecycle, Link, Source, SourceId,
    SourceLink, Standing, StatementKind, block_digest, claim_path, source_path,
};
use crate::proposal::{
    Invalid, MAX_COMMENT_BYTES, NewClaimComment, NewComment, ProposalError, ProposalService,
    QuotaConfig, QuotaLimit,
};
use crate::read::testing::World;
use crate::read::{ReadConfig, ReadService};

const SAID: &str = "Said.";
/// An hour, in seconds.
const HOUR: i64 = 3600;

fn claim_id(n: u8) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-0000000000{n:02x}")).expect("a claim id")
}

fn digest() -> crate::knowledge::Digest {
    block_digest(
        inklkunn_markup::blocks(&format!("{SAID} ^b1"))
            .first()
            .expect("a block"),
    )
}

/// A claim said at `One#b1`, contested, citing one source.
fn claim(n: u8) -> Claim {
    let mut c = Claim::new(claim_id(n));
    c.kind = StatementKind::Observation;
    c.lifecycle = Lifecycle::Proposed;
    c.expressions = vec![Expression::new("p1", "b1").expect("an expression")];
    c.community_status = CommunityStatus {
        status: Standing::Contested,
        proposal: Some("01930000-aaaa-7000-8000-000000000001".to_owned()),
        judged_on: Some(digest()),
    };
    c.sources = vec![SourceLink {
        source: SourceId::parse("01931111-0000-7000-8000-000000000001").expect("a source id"),
        relation: Link::Supports,
        bearing: Bearing::Direct,
        note: None,
        judged_on: digest(),
    }];
    c
}

/// A repository with two claims and the page they are said in.
fn repository() -> World {
    let source = Source::new(SourceId::parse("01931111-0000-7000-8000-000000000001").expect("id"));
    let files = [
        (
            "pages/One.md".to_owned(),
            format!("---\ntitle: One\nid: p1\n---\n\n{SAID} ^b1\n"),
        ),
        (claim_path(&claim_id(1)), claim(1).to_yaml()),
        (claim_path(&claim_id(2)), claim(2).to_yaml()),
        (source_path(&source.id), source.to_yaml()),
    ];
    let refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, t)| (p.as_str(), t.as_str()))
        .collect();
    World::with(
        &refs,
        ReadConfig {
            tip_ttl: Duration::from_secs(3600),
            ..ReadConfig::default()
        },
    )
}

/// The proposal service of `e`'s records, keeping claim threads, with `w`'s repository to see that a claim is there.
fn threaded(e: &Env, w: &World, config: QuotaConfig) -> ProposalService {
    let reads = Arc::new(World::service_over(
        &w.counting,
        &w.store,
        &w.layout,
        ReadConfig {
            tip_ttl: Duration::from_secs(3600),
            ..ReadConfig::default()
        },
    ));
    service_with(e, config, Some(reads))
}

fn service_with(e: &Env, config: QuotaConfig, reads: Option<Arc<ReadService>>) -> ProposalService {
    let store = &e.store;
    let authorizer = Authorizer::new(
        Arc::new(StoreAuthz::new(store.clone(), store.clone())),
        Arc::new(MemoryAudit::new()),
    );
    let service = ProposalService::new(
        store.clone(),
        store.clone(),
        store.clone(),
        authorizer,
        e.bases.clone(),
        config,
    )
    .with_clock(now);
    match reads {
        Some(reads) => service.with_claim_threads(store.clone(), reads),
        None => service,
    }
}

fn say(body: &str) -> NewClaimComment {
    NewClaimComment {
        body: body.to_owned(),
        parent: None,
    }
}

fn replying(body: &str, to: &CommentId) -> NewClaimComment {
    NewClaimComment {
        body: body.to_owned(),
        parent: Some(to.clone()),
    }
}

fn thread_bodies(
    s: &ProposalService,
    e: &Env,
    who: &Actor,
    claim: &ClaimId,
) -> Result<Vec<String>, ProposalError> {
    Ok(s.claim_thread(&who.principal, &e.wiki, claim, None, 100)?
        .rows
        .into_iter()
        .map(|r| r.comment.body)
        .collect())
}

#[test]
fn a_signed_in_person_comments_and_the_thread_is_read_oldest_first_in_pages() {
    on_both(|e| {
        let w = repository();
        let s = threaded(e, &w, QuotaConfig::default());
        let first = s
            .comment_on_claim(
                &e.author.principal,
                &e.wiki,
                None,
                &claim_id(1),
                say("  Where was this measured?  "),
            )
            .unwrap();
        // The text is trimmed; the record is the comment's own.
        assert_eq!(first.body, "Where was this measured?");
        assert_eq!(first.author, e.author.id);
        assert_eq!(first.claim, claim_id(1).as_str());
        assert_eq!(first.wiki, e.wiki.id);
        set_now(T0 + 5);
        let second = s
            .comment_on_claim(
                &e.reviewer.principal,
                &e.wiki,
                None,
                &claim_id(1),
                replying("At the north gauge.", &first.id),
            )
            .unwrap();
        assert_eq!(second.parent.as_ref(), Some(&first.id));
        assert_eq!(second.created_at, T0 + 5);
        // Anyone who may read the wiki reads it, the visitor who signed in nowhere included; oldest first.
        let anon = Actor {
            id: e.outsider.id.clone(),
            principal: Principal::Anonymous,
        };
        assert_eq!(
            thread_bodies(&s, e, &anon, &claim_id(1)).unwrap(),
            ["Where was this measured?", "At the north gauge."]
        );
        let page = s
            .claim_thread(&e.observer.principal, &e.wiki, &claim_id(1), None, 1)
            .unwrap();
        assert_eq!((page.rows.len(), page.total), (1, 2));
        let rest = s
            .claim_thread(
                &e.observer.principal,
                &e.wiki,
                &claim_id(1),
                page.next.as_ref(),
                1,
            )
            .unwrap();
        assert_eq!(rest.rows.first().unwrap().position, 2);
        assert_eq!(rest.rows.first().unwrap().parent_position, Some(1));
        // The other claim's thread is its own.
        assert!(
            thread_bodies(&s, e, &e.author, &claim_id(2))
                .unwrap()
                .is_empty()
        );
        // The whole of the wiki's threads, for the bundle.
        assert_eq!(s.all_claim_comments(&e.wiki).unwrap().len(), 2);
    });
}

#[test]
fn a_discussion_changes_no_claim_no_file_and_makes_no_proposal() {
    on_both(|e| {
        let w = repository();
        let s = threaded(e, &w, QuotaConfig::default());
        let before = (
            w.fake().state_id(),
            w.fake().block_id(),
            w.service
                .knowledge_index(&w.wiki, None)
                .unwrap()
                .rows()
                .clone(),
        );
        let c = s
            .comment_on_claim(
                &e.maintainer.principal,
                &e.wiki,
                None,
                &claim_id(1),
                say("I think this is settled."),
            )
            .unwrap();
        s.comment_on_claim(
            &e.admin.principal,
            &e.wiki,
            None,
            &claim_id(1),
            replying("It is not.", &c.id),
        )
        .unwrap();
        // The repository is the repository it was, the index of its tip is the index it was, the claim is as decided,
        // and the comments are no proposal: a status changes by an accepted curation proposal and by nothing else.
        w.service.forget_wiki(&w.wiki);
        let index = w.service.knowledge_index(&w.wiki, None).unwrap();
        assert_eq!(
            (
                w.fake().state_id(),
                w.fake().block_id(),
                index.rows().clone()
            ),
            before
        );
        let claim_row = index.claim(claim_id(1).as_str()).unwrap();
        assert_eq!(claim_row.standing, "contested");
        assert_eq!(
            claim_row.standing_proposal.as_deref(),
            Some("01930000-aaaa-7000-8000-000000000001")
        );
        assert!(
            e.store
                .proposals(&e.wiki.id, &ProposalFilter::default(), None)
                .unwrap()
                .is_empty()
        );
    });
}

#[test]
fn who_may_comment_is_who_may_comment_on_a_proposal() {
    on_both(|e| {
        let w = repository();
        let s = threaded(e, &w, QuotaConfig::default());
        let questioner = e.person(Some(Role::Questioner));
        let people: Vec<(&str, Principal)> = vec![
            ("anonymous", Principal::Anonymous),
            ("outsider", e.outsider.principal.clone()),
            ("observer", e.observer.principal.clone()),
            ("questioner", questioner.principal.clone()),
            ("author", e.author.principal.clone()),
            ("reviewer", e.reviewer.principal.clone()),
            ("admin", e.admin.principal.clone()),
            ("token that reads", e.token(&e.admin, Scope::Read)),
            ("token that proposes", e.token(&e.admin, Scope::Propose)),
        ];
        let p = e.held(&e.author, "Mine", "pages/a.md");
        for (who, principal) in people {
            let on_a_proposal = s
                .comment(
                    &principal,
                    &e.wiki,
                    &p.id,
                    NewComment {
                        body: "A comment".to_owned(),
                        ..NewComment::default()
                    },
                )
                .is_ok();
            let on_a_claim = s
                .comment_on_claim(&principal, &e.wiki, None, &claim_id(1), say("A comment"))
                .is_ok();
            assert_eq!(on_a_claim, on_a_proposal, "{who}");
            assert_eq!(
                s.may_comment_on_claim(&principal, &e.wiki, &claim_id(1)),
                on_a_proposal,
                "{who}: what the page offers"
            );
        }
        // Spelled out: a signed-in account of any role from questioner up, and nobody else.
        assert!(
            s.comment_on_claim(
                &questioner.principal,
                &e.wiki,
                None,
                &claim_id(1),
                say("Hello")
            )
            .is_ok()
        );
        for refused in [
            &Principal::Anonymous,
            &e.observer.principal,
            &e.token(&e.admin, Scope::Read),
        ] {
            assert_eq!(
                s.comment_on_claim(refused, &e.wiki, None, &claim_id(1), say("Hello")),
                Err(ProposalError::NotAllowed)
            );
        }
    });
}

#[test]
fn a_comment_is_stored_trimmed_so_a_body_never_ends_in_a_line_feed() {
    // K64: the bundle writes a body as a fenced text block, which shows a text that ends in a line feed with a blank line
    // more. The service never stores one: a body has no white space at either end, whatever it was sent with.
    on_both(|e| {
        let w = repository();
        let s = threaded(e, &w, QuotaConfig::default());
        for sent in [
            "Where?\n",
            "Where?\n\n\n",
            "\n\nWhere?\r\n",
            " \t Where? \t\n",
            "Where?\n\u{a0}\n",
            "Where?\u{2028}",
            "Two\nlines\n",
        ] {
            let stored = s
                .comment_on_claim(&e.author.principal, &e.wiki, None, &claim_id(1), say(sent))
                .unwrap();
            assert!(
                !stored.body.ends_with(['\n', '\r']),
                "{sent:?}: {:?}",
                stored.body
            );
            assert!(!stored.body.starts_with(char::is_whitespace), "{sent:?}");
            assert!(!stored.body.ends_with(char::is_whitespace), "{sent:?}");
        }
        for body in thread_bodies(&s, e, &e.author, &claim_id(1)).unwrap() {
            assert!(!body.ends_with('\n'), "{body:?}");
        }
    });
}

#[test]
fn what_is_not_a_comment_or_not_a_thread_is_refused_and_nothing_is_written() {
    on_both(|e| {
        let w = repository();
        let s = threaded(e, &w, QuotaConfig::default());
        let who = &e.author.principal;
        let a = s
            .comment_on_claim(who, &e.wiki, None, &claim_id(1), say("On the first claim."))
            .unwrap();
        let comment = |claim: &ClaimId, new: NewClaimComment| {
            s.comment_on_claim(who, &e.wiki, None, claim, new)
        };
        assert_eq!(
            comment(&claim_id(1), say("")),
            Err(Invalid::CommentEmpty.into())
        );
        assert_eq!(
            comment(&claim_id(1), say(" \n\t ")),
            Err(Invalid::CommentEmpty.into())
        );
        assert_eq!(
            comment(&claim_id(1), say(&"x".repeat(MAX_COMMENT_BYTES + 1))),
            Err(Invalid::CommentTooLong.into())
        );
        // U+0000 is read as U+FFFD by every Markdown reader, so a bundle could not carry it as written: refused (K64).
        for nul in ["a\0b", "\0", "fine\n\0\n"] {
            assert_eq!(
                comment(&claim_id(1), say(nul)),
                Err(Invalid::CommentNul.into()),
                "{nul:?}"
            );
        }
        comment(&claim_id(1), say(&"x".repeat(MAX_COMMENT_BYTES))).expect("the longest comment");
        // A claim the wiki does not hold has no thread to join.
        assert_eq!(
            comment(&claim_id(9), say("Hello")),
            Err(ProposalError::NotFound)
        );
        // A reply to a comment of another thread, or to none, is not a reply.
        assert_eq!(
            comment(&claim_id(2), replying("Misplaced", &a.id)),
            Err(ProposalError::NotFound)
        );
        assert_eq!(
            comment(&claim_id(1), replying("Nowhere", &CommentId::new())),
            Err(ProposalError::NotFound)
        );
        // A view the wiki does not have.
        assert_eq!(
            s.comment_on_claim(who, &e.wiki, Some("heads/nope"), &claim_id(1), say("Hello")),
            Err(ProposalError::NoSuchView("heads/nope".to_owned()))
        );
        assert_eq!(
            thread_bodies(&s, e, &e.author, &claim_id(1)).unwrap().len(),
            2
        );
        assert!(
            thread_bodies(&s, e, &e.author, &claim_id(2))
                .unwrap()
                .is_empty()
        );
        // Reading a thread that nobody has written in, or of a claim the wiki does not hold, is the same empty thread.
        assert_eq!(
            s.claim_thread(who, &e.wiki, &claim_id(2), None, 10)
                .unwrap(),
            s.claim_thread(who, &e.wiki, &claim_id(9), None, 10)
                .unwrap()
        );
        // A place to continue from that is not in the thread.
        assert_eq!(
            s.claim_thread(who, &e.wiki, &claim_id(2), Some(&a.id), 10),
            Err(ProposalError::NotFound)
        );
    });
}

#[test]
fn a_persons_comments_on_proposals_and_in_claim_threads_share_one_hourly_limit() {
    let config = QuotaConfig {
        comments_per_hour_per_principal: 3,
        ..QuotaConfig::default()
    };
    on_both_with(&config, |e| {
        set_now(T0);
        let w = repository();
        let s = threaded(e, &w, config.clone());
        let p = e.held(&e.author, "Mine", "pages/a.md");
        let on_proposal = |n: usize| {
            s.comment(
                &e.reviewer.principal,
                &e.wiki,
                &p.id,
                NewComment {
                    body: format!("On the proposal {n}"),
                    ..NewComment::default()
                },
            )
        };
        let on_claim = |n: usize| {
            s.comment_on_claim(
                &e.reviewer.principal,
                &e.wiki,
                None,
                &claim_id(1),
                say(&format!("On the claim {n}")),
            )
        };
        on_proposal(0).unwrap();
        set_now(T0 + 600);
        on_claim(1).unwrap();
        on_claim(2).unwrap();
        set_now(T0 + 1200);
        // The fourth, wherever it is made, is over the limit, and it says when it will fit.
        for refused in [on_claim(3).map(|_| ()), on_proposal(3).map(|_| ())] {
            let Err(ProposalError::Quota(q)) = refused else {
                panic!("the comment limit: {refused:?}");
            };
            assert_eq!(q.limit, QuotaLimit::CommentsPerHourPerPrincipal);
            assert_eq!((q.allowed, q.current), (3, 3));
            assert_eq!(q.reset, crate::proposal::Reset::At(T0 + HOUR));
        }
        // A refused comment is not stored; somebody else's limit is their own.
        assert_eq!(
            thread_bodies(&s, e, &e.author, &claim_id(1)).unwrap().len(),
            2
        );
        s.comment_on_claim(
            &e.maintainer.principal,
            &e.wiki,
            None,
            &claim_id(1),
            say("Mine"),
        )
        .unwrap();
        // When the first leaves the window, it fits again; a new account has half.
        set_now(T0 + HOUR);
        on_claim(4).unwrap();
        let new = e.person_created(Some(Role::Questioner), T0 + HOUR - 10);
        s.comment_on_claim(&new.principal, &e.wiki, None, &claim_id(1), say("First"))
            .unwrap();
        let Err(ProposalError::Quota(q)) =
            s.comment_on_claim(&new.principal, &e.wiki, None, &claim_id(1), say("Second"))
        else {
            panic!("a new account has half");
        };
        assert_eq!(q.allowed, 1);
    });
}

#[test]
fn a_service_that_keeps_no_threads_says_so_and_offers_no_form() {
    on_both(|e| {
        let s = service_with(e, QuotaConfig::default(), None);
        assert!(matches!(
            s.comment_on_claim(
                &e.author.principal,
                &e.wiki,
                None,
                &claim_id(1),
                say("Hello")
            ),
            Err(ProposalError::Store(_))
        ));
        assert!(matches!(
            s.claim_thread(&e.author.principal, &e.wiki, &claim_id(1), None, 10),
            Err(ProposalError::Store(_))
        ));
        assert!(!s.may_comment_on_claim(&e.author.principal, &e.wiki, &claim_id(1)));
        // A proposal's comment is as it was.
        let p = e.held(&e.author, "Mine", "pages/a.md");
        s.comment(
            &e.reviewer.principal,
            &e.wiki,
            &p.id,
            NewComment {
                body: "Fine".to_owned(),
                ..NewComment::default()
            },
        )
        .unwrap();
    });
}

#[test]
fn the_thread_of_a_wiki_that_may_not_be_read_is_not_there_to_read_or_to_write_in() {
    use inklkunn_store::{DisplayName, NewWiki, RoleGrant, Slug, Visibility};
    on_both(|e| {
        let w = repository();
        let s = threaded(e, &w, QuotaConfig::default());
        let secret = e
            .store
            .create(NewWiki {
                visibility: Visibility::Restricted,
                ..NewWiki::new(
                    Slug::new("secret").unwrap(),
                    DisplayName::new("Secret").unwrap(),
                )
            })
            .unwrap();
        for (who, role) in [(&e.observer, Role::Observer), (&e.reviewer, Role::Reviewer)] {
            e.store
                .grant(RoleGrant {
                    account: who.id.clone(),
                    wiki: secret.id.clone(),
                    role,
                    granted_by: None,
                    granted_at: 0,
                })
                .unwrap();
        }
        // A reader the wiki admits reads; anybody else is refused, the signed-in stranger and the visitor alike.
        for (who, principal, reads) in [
            ("observer", &e.observer.principal, true),
            ("reviewer", &e.reviewer.principal, true),
            ("author with no grant here", &e.author.principal, false),
            ("outsider", &e.outsider.principal, false),
            ("anonymous", &Principal::Anonymous, false),
        ] {
            let read = s.claim_thread(principal, &secret, &claim_id(1), None, 10);
            assert_eq!(read.is_ok(), reads, "{who}");
            if !reads {
                assert_eq!(read, Err(ProposalError::NotAllowed), "{who}");
            }
        }
        // And only an account the wiki lets comment writes: the observer reads and does not.
        assert_eq!(
            s.comment_on_claim(
                &e.observer.principal,
                &secret,
                None,
                &claim_id(1),
                say("Hello")
            ),
            Err(ProposalError::NotAllowed)
        );
        assert_eq!(
            s.comment_on_claim(
                &e.author.principal,
                &secret,
                None,
                &claim_id(1),
                say("Hello")
            ),
            Err(ProposalError::NotAllowed)
        );
    });
}

/// A claim-thread store that counts the lookups made of it, over the real one.
struct Counting {
    inner: Arc<dyn inklkunn_store::ClaimThreadStore>,
    calls: std::sync::atomic::AtomicUsize,
}

impl Counting {
    fn bump(&self) {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn calls(&self) -> usize {
        self.calls.load(std::sync::atomic::Ordering::SeqCst)
    }
}

impl inklkunn_store::ClaimThreadStore for Counting {
    fn add_claim_comment(
        &self,
        comment: &inklkunn_store::ClaimCommentRecord,
    ) -> inklkunn_store::Result<()> {
        self.inner.add_claim_comment(comment)
    }

    fn claim_comments(
        &self,
        wiki: &inklkunn_store::WikiId,
        claim: &str,
        after: Option<&CommentId>,
        limit: usize,
    ) -> inklkunn_store::Result<inklkunn_store::ClaimThreadPage> {
        self.bump();
        self.inner.claim_comments(wiki, claim, after, limit)
    }

    fn claim_comments_since(
        &self,
        wiki: &inklkunn_store::WikiId,
        author: &inklkunn_store::AccountId,
        since: i64,
    ) -> inklkunn_store::Result<Vec<i64>> {
        self.inner.claim_comments_since(wiki, author, since)
    }

    fn claim_comments_in_wiki(
        &self,
        wiki: &inklkunn_store::WikiId,
    ) -> inklkunn_store::Result<Vec<inklkunn_store::ClaimCommentRow>> {
        self.inner.claim_comments_in_wiki(wiki)
    }

    fn claim_comment_at(
        &self,
        wiki: &inklkunn_store::WikiId,
        claim: &str,
        position: u64,
    ) -> inklkunn_store::Result<Option<CommentId>> {
        self.bump();
        self.inner.claim_comment_at(wiki, claim, position)
    }
}

#[test]
fn the_last_page_of_a_thread_is_found_with_two_lookups_however_long_the_thread_is() {
    on_both(|e| {
        let w = repository();
        let reads = Arc::new(World::service_over(
            &w.counting,
            &w.store,
            &w.layout,
            ReadConfig {
                tip_ttl: Duration::from_secs(3600),
                ..ReadConfig::default()
            },
        ));
        let counting = Arc::new(Counting {
            inner: e.store.clone(),
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let s = service_with(e, QuotaConfig::default(), None)
            .with_claim_threads(counting.clone(), reads);
        let id = claim_id(1);
        let last = |page: usize| {
            s.claim_thread_last_page(&e.author.principal, &e.wiki, &id, page)
                .unwrap()
        };
        // Nothing written: one page, which is the first.
        assert_eq!(last(3), None);
        let mut comments: Vec<CommentId> = Vec::new();
        for n in 1..=9_usize {
            set_now(T0 + i64::try_from(n).unwrap());
            comments.push(
                s.comment_on_claim(
                    &e.author.principal,
                    &e.wiki,
                    None,
                    &id,
                    say(&format!("No. {n}")),
                )
                .unwrap()
                .id,
            );
            for page in 1..=4_usize {
                // The comment the last page follows is the one at the end of the pages before it.
                let before = (n - 1) / page * page;
                let expected = before.checked_sub(1).and_then(|i| comments.get(i)).cloned();
                counting.calls.store(0, std::sync::atomic::Ordering::SeqCst);
                assert_eq!(last(page), expected, "{n} comments, {page} to a page");
                assert!(
                    counting.calls() <= 2,
                    "{n} comments, {page} to a page: {} lookups",
                    counting.calls()
                );
            }
        }
        // And it is a place `claim_thread` can continue from: the last page holds the last comment.
        let after = last(4);
        let page = s
            .claim_thread(&e.author.principal, &e.wiki, &id, after.as_ref(), 4)
            .unwrap();
        assert_eq!(
            page.rows.last().map(|r| r.comment.id.clone()),
            comments.last().cloned()
        );
    });
}
