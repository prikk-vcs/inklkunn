//! Accept (RFC 004 D-4, D-12; handoff 004 T4b): the form posts the revision it shows, the acceptor is asked
//! only after the checks that precede it, every answer of the acceptance is a page in the words of D-12 (in
//! both languages), what is for the operator goes to the log and not to the page, and the overlaps an
//! acceptance recorded are shown side by side where the proposal needs a revision. The acceptor is a double
//! that answers what it is told; the real one is tested against the repository in `inklkunn-core`.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::http::StatusCode;
use inklkunn_core::acceptance::{Reconciled, words};
use inklkunn_core::lease::{Busy, BusyWhy};
use inklkunn_core::proposal::{ProposalEdit, ProposalError, QuotaLimit, QuotaRefusal, Reset};
use inklkunn_core::{AcceptFailure, Accepted, IdentityAuthority, Principal, Signed};
use inklkunn_store::{
    AcceptanceId, AccountId, ActStore, FileChange, OverlapRecord, OverlapText, ProposalId,
    ProposalState, RecoveryFlag, RecoveryReason, StoreError, WikiRecord,
};
use inklkunn_web::{Acceptor, Services};

use super::{Cast, HOME, cast, propose, read};
use crate::common::{Answer, NOW, block_id, post_form, send_body};

type Answered = Result<Accepted, AcceptFailure>;

/// An acceptor that answers what it was told and keeps every call.
struct Scripted {
    answer: Mutex<Option<Answered>>,
    calls: Mutex<Vec<(Option<AccountId>, ProposalId, u32)>>,
}

impl Acceptor for Scripted {
    fn accept(
        &self,
        principal: &Principal,
        _: &WikiRecord,
        id: &ProposalId,
        revision: u32,
    ) -> Answered {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((principal.account_id().cloned(), id.clone(), revision));
        self.answer
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .unwrap_or(Err(AcceptFailure::NotAllowed))
    }
}

impl Scripted {
    fn calls(&self) -> usize {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

/// The wiki served by an acceptor that will answer `answer`.
fn serving(c: &mut Cast, answer: Answered) -> Arc<Scripted> {
    let double = Arc::new(Scripted {
        answer: Mutex::new(Some(answer)),
        calls: Mutex::new(Vec::new()),
    });
    c.w.services = Arc::new(Services {
        acceptor: Some(double.clone()),
        ..(*c.w.services).clone()
    });
    double
}

fn signed() -> Signed {
    Signed {
        acceptance: AcceptanceId::new(),
        block_id: block_id(2),
        patch_id: "a".repeat(64),
        reauthored: false,
        unconfirmed: None,
        seeds_left: false,
        ids_attached: 0,
        reconciliation: Reconciled::Clean,
        paths: vec!["pages/Home.md".to_owned()],
        notified: 0,
    }
}

fn failed(word: &'static str, detail: &str) -> AcceptFailure {
    AcceptFailure::Failed {
        acceptance: AcceptanceId::new(),
        word,
        detail: detail.to_owned(),
    }
}

/// Post an accept as `account` with this body and these headers.
async fn accept(
    c: &Cast,
    uri: &str,
    account: Option<&AccountId>,
    extra: &[(&str, &str)],
    body: &str,
) -> Answer {
    send_body(
        &c.w,
        "POST",
        &format!("{uri}/accept"),
        account,
        extra,
        body.to_owned(),
    )
    .await
}

/// The hidden field of the accept form (not of the decision form, which carries one too).
const ACCEPT_FORM_REVISION: &str =
    "/accept\"><input type=\"hidden\" name=\"revision\" value=\"1\">";

const SAME_SITE: &[(&str, &str)] = &[("sec-fetch-site", "same-origin")];
const JAPANESE: &[(&str, &str)] = &[
    ("sec-fetch-site", "same-origin"),
    ("cookie", "inklkunn_lang=ja"),
];

fn id_of(uri: &str) -> ProposalId {
    ProposalId::parse(uri.rsplit('/').next().unwrap()).unwrap()
}

fn queue_refusal() -> QuotaRefusal {
    QuotaRefusal {
        limit: QuotaLimit::AcceptanceQueue,
        allowed: 64,
        current: 64,
        asked: 1,
        reset: Reset::WhenTheQueueMoves,
    }
}

fn flag() -> RecoveryFlag {
    RecoveryFlag {
        wiki: inklkunn_store::WikiId::new(),
        reason: RecoveryReason::Queued,
        act: None,
        since: NOW,
    }
}

#[tokio::test]
async fn the_accept_form_posts_the_revision_it_shows_and_says_nothing_of_unavailability_when_accepting_is_served()
 {
    let mut c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    // Without an acceptor the button is there and says so.
    let page = read(&c, &p, Some(&c.maintainer)).await.body();
    assert!(page.contains("Accepting is not available yet on this server."));
    assert!(
        page.contains(ACCEPT_FORM_REVISION),
        "the form carries the revision it shows: {page}"
    );
    let double = serving(&mut c, Err(AcceptFailure::NotAllowed));
    let page = read(&c, &p, Some(&c.maintainer)).await.body();
    assert!(page.contains(">Accept</button>"), "{page}");
    assert!(!page.contains("not available yet"), "{page}");
    assert!(page.contains(ACCEPT_FORM_REVISION), "{page}");
    assert_eq!(double.calls(), 0, "reading a page accepts nothing");
}

#[tokio::test]
async fn the_acceptor_is_asked_as_the_visitor_for_the_proposal_and_the_revision_the_form_names() {
    let mut c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let double = serving(&mut c, Ok(Accepted::Signed(signed())));
    let a = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let calls = double.calls.lock().unwrap().clone();
    assert_eq!(calls, vec![(Some(c.maintainer.clone()), id_of(&p), 1)]);
    // A revision the form names is asked for as it is: the acceptance, not the page, decides it is stale.
    let double = serving(
        &mut c,
        Err(AcceptFailure::Proposal(ProposalError::StaleRevision {
            latest: 3,
        })),
    );
    let a = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=2").await;
    assert_eq!(a.status, StatusCode::CONFLICT);
    let body = a.body();
    assert!(
        body.contains("This proposal has a newer revision, number 3."),
        "{body}"
    );
    assert_eq!(double.calls.lock().unwrap().first().unwrap().2, 2);
}

#[tokio::test]
async fn a_missing_or_unreadable_revision_is_a_refusal_in_words_and_asks_nobody() {
    let mut c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let double = serving(&mut c, Ok(Accepted::Signed(signed())));
    for body in [
        "",
        "revision=",
        "revision=abc",
        "revision=-1",
        "revision=99999999999",
        "other=1",
    ] {
        let a = accept(&c, &p, Some(&c.maintainer), SAME_SITE, body).await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{body:?}");
        let page = a.body();
        assert!(
            page.contains("The form was not complete. Open it again from the page and try again."),
            "{body:?}: {page}"
        );
        assert!(page.contains("Back to the proposal"), "{page}");
    }
    assert_eq!(double.calls(), 0);
    // And the same in Japanese.
    let a = accept(&c, &p, Some(&c.maintainer), JAPANESE, "").await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(a.body().contains("フォームの内容が不完全でした"));
}

#[tokio::test]
async fn nothing_runs_for_a_visitor_who_may_not_accept_a_form_from_elsewhere_or_a_proposal_that_is_not_there()
 {
    let mut c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let double = serving(&mut c, Ok(Accepted::Signed(signed())));
    // A contributor and an observer may read it and may not accept it.
    for who in [&c.contributor, &c.other, &c.observer, &c.reviewer] {
        let a = accept(&c, &p, Some(who), SAME_SITE, "revision=1").await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{who}");
        assert!(a.body().contains("<h1>Not allowed</h1>"));
    }
    // Not signed in: told that an account is needed.
    let a = accept(&c, &p, None, SAME_SITE, "revision=1").await;
    assert!(a.body().contains("Proposing needs an account"));
    // A form from another site is refused before the wiki is looked up.
    for headers in [
        &[("sec-fetch-site", "cross-site")][..],
        &[("sec-fetch-site", "same-site")][..],
        &[("origin", "http://evil.example")][..],
    ] {
        let a = accept(&c, &p, Some(&c.maintainer), headers, "revision=1").await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{headers:?}");
        assert!(
            a.body().contains("The form was not accepted"),
            "{}",
            a.body()
        );
    }
    // No such proposal: the generic page.
    let nowhere = "/handbook/-/proposals/019a0000-0000-7000-8000-00000000dddd";
    let a = accept(&c, nowhere, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    let a = accept(
        &c,
        "/nowhere/-/proposals/x",
        Some(&c.maintainer),
        SAME_SITE,
        "revision=1",
    )
    .await;
    assert_eq!(a.status, StatusCode::NOT_FOUND);
    assert_eq!(double.calls(), 0, "none of them reached the acceptor");
    assert_eq!(
        c.w.services
            .proposals
            .get(
                &Principal::session(&IdentityAuthority::for_tests(), c.maintainer.clone()),
                &c.w.wiki,
                &id_of(&p)
            )
            .unwrap()
            .state,
        ProposalState::HeldForReview
    );
}

#[tokio::test]
async fn a_proposal_the_acceptance_cannot_find_looks_exactly_like_one_the_visitor_may_not_accept() {
    let mut c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    serving(&mut c, Err(AcceptFailure::NotFound));
    let missing = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    serving(&mut c, Err(AcceptFailure::NotAllowed));
    let refused = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    assert_eq!(missing.status, StatusCode::FORBIDDEN);
    assert_eq!(missing.status, refused.status);
    assert_eq!(missing.body(), refused.body());
}

/// One answer of the acceptance, the status it comes to, and the words of its page in each language.
struct Case {
    name: &'static str,
    answer: fn() -> Answered,
    status: StatusCode,
    english: &'static [&'static str],
    japanese: &'static [&'static str],
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "signed",
            answer: || Ok(Accepted::Signed(signed())),
            status: StatusCode::OK,
            english: &[
                "<h1>Signed into the history</h1>",
                "The proposal is part of the history now.",
            ],
            japanese: &[
                "<h1>署名して履歴に記録済み</h1>",
                "提案は履歴に取り込まれました。",
            ],
        },
        Case {
            name: "already signed (a retry finds the record)",
            answer: || {
                Ok(Accepted::AlreadySigned {
                    acceptance: AcceptanceId::new(),
                    block_id: block_id(2),
                    patch_id: "a".repeat(64),
                })
            },
            status: StatusCode::OK,
            english: &["<h1>Signed into the history</h1>"],
            japanese: &["<h1>署名して履歴に記録済み</h1>"],
        },
        Case {
            name: "overlap",
            answer: || Err(AcceptFailure::Overlap { overlaps: 2 }),
            status: StatusCode::CONFLICT,
            english: &[
                "<h1>Stopped: changes overlap</h1>",
                "both texts are shown on the proposal",
            ],
            japanese: &["<h1>停止: 変更が重なっています</h1>"],
        },
        Case {
            name: "not allowed",
            answer: || Err(AcceptFailure::NotAllowed),
            status: StatusCode::FORBIDDEN,
            english: &["<h1>Not allowed</h1>"],
            japanese: &["<h1>許可されていません</h1>"],
        },
        Case {
            name: "busy with a depth",
            answer: || {
                Err(AcceptFailure::Busy(Busy {
                    depth: 2,
                    estimate: Duration::from_secs(45),
                    why: BusyWhy::Waited,
                }))
            },
            status: StatusCode::SERVICE_UNAVAILABLE,
            english: &[
                "<h1>The wiki is busy</h1>",
                "2 acceptances are waiting ahead of yours.",
            ],
            japanese: &[
                "<h1>ウィキは混み合っています</h1>",
                "あなたの前に 2 件の受け入れが待っています。",
            ],
        },
        Case {
            name: "refused by a named limit",
            answer: || Err(AcceptFailure::Refused(queue_refusal())),
            status: StatusCode::TOO_MANY_REQUESTS,
            english: &["<h1>Refused</h1>", "too many acceptances waiting"],
            japanese: &["<h1>拒否されました</h1>", "待機中の受け入れが多すぎます"],
        },
        Case {
            name: "needs recovery",
            answer: || Err(AcceptFailure::NeedsRecovery(flag())),
            status: StatusCode::SERVICE_UNAVAILABLE,
            english: &["<h1>The wiki needs recovery</h1>"],
            japanese: &["<h1>ウィキの復旧が必要です</h1>"],
        },
        Case {
            name: "incomplete",
            answer: || {
                Err(AcceptFailure::Incomplete {
                    acceptance: AcceptanceId::new(),
                    reason: RecoveryReason::WorktreeDirty,
                    flagged: true,
                    detail: "prikk said /srv/wikis/x/worktree is dirty".to_owned(),
                })
            },
            status: StatusCode::SERVICE_UNAVAILABLE,
            english: &["<h1>The wiki needs recovery</h1>"],
            japanese: &["<h1>ウィキの復旧が必要です</h1>"],
        },
        Case {
            name: "in progress",
            answer: || {
                Err(AcceptFailure::InProgress {
                    acceptance: AcceptanceId::new(),
                })
            },
            status: StatusCode::SERVICE_UNAVAILABLE,
            english: &["<h1>This revision is being signed</h1>"],
            japanese: &["<h1>この修正版は署名の最中です</h1>"],
        },
        Case {
            name: "nothing to change",
            answer: || Err(AcceptFailure::NothingToChange),
            status: StatusCode::CONFLICT,
            english: &["<h1>Nothing to change</h1>"],
            japanese: &["<h1>変更するものがありません</h1>"],
        },
        Case {
            name: "failed before the change: refused",
            answer: || {
                Err(failed(
                    words::REFUSED,
                    "prikk said: the patch does not apply",
                ))
            },
            status: StatusCode::UNPROCESSABLE_ENTITY,
            english: &[
                "<h1>Not signed: the history refused it</h1>",
                "Nothing was changed",
            ],
            japanese: &["<h1>署名されませんでした: 履歴が受け付けませんでした</h1>"],
        },
        Case {
            name: "failed before the change: too large",
            answer: || Err(failed(words::MEMORY_LIMIT, "ceiling 64 MiB exceeded")),
            status: StatusCode::PAYLOAD_TOO_LARGE,
            english: &["<h1>Too large to sign now</h1>"],
            japanese: &["<h1>大きすぎて今は署名できません</h1>"],
        },
        Case {
            name: "failed before the change: the signer was busy",
            answer: || Err(failed(words::BUSY, "the signer holds the lock")),
            status: StatusCode::SERVICE_UNAVAILABLE,
            english: &["<h1>The wiki is busy</h1>"],
            japanese: &["<h1>ウィキは混み合っています</h1>"],
        },
        Case {
            name: "failed before the change: unavailable",
            answer: || Err(failed(words::UNAVAILABLE, "no route to the repository")),
            status: StatusCode::SERVICE_UNAVAILABLE,
            english: &["<h1>Could not be done now</h1>", "Nothing was changed."],
            japanese: &["<h1>今は実行できません</h1>"],
        },
        Case {
            name: "unavailable before anything was written",
            answer: || {
                Err(AcceptFailure::Unavailable(
                    "the lock file /srv/x/lock".to_owned(),
                ))
            },
            status: StatusCode::SERVICE_UNAVAILABLE,
            english: &["<h1>Could not be done now</h1>"],
            japanese: &["<h1>今は実行できません</h1>"],
        },
        Case {
            name: "the records failed",
            answer: || Err(AcceptFailure::Store(StoreError::NotFound)),
            status: StatusCode::INTERNAL_SERVER_ERROR,
            english: &[
                "<h1>The result is not certain</h1>",
                "read the history before you try again",
            ],
            japanese: &["<h1>結果を確定できません</h1>"],
        },
        Case {
            name: "the proposal's state does not allow it",
            answer: || {
                Err(AcceptFailure::Proposal(ProposalError::NotInThisState {
                    op: inklkunn_store::ProposalOp::Accept,
                    state: ProposalState::Declined,
                }))
            },
            status: StatusCode::CONFLICT,
            english: &["This proposal is Declined, so this cannot be done now."],
            japanese: &["<h1>"],
        },
    ]
}

#[tokio::test]
async fn every_answer_of_the_acceptance_is_a_page_in_the_words_of_the_glossary_in_both_languages() {
    for case in cases() {
        for (lang, headers, words) in [
            ("English", SAME_SITE, case.english),
            ("Japanese", JAPANESE, case.japanese),
        ] {
            let mut c = cast();
            let p = propose(&c, &c.contributor, "Improve").await;
            serving(&mut c, (case.answer)());
            let a = accept(&c, &p, Some(&c.maintainer), headers, "revision=1").await;
            let body = a.body();
            assert_eq!(a.status, case.status, "{} ({lang}): {body}", case.name);
            for word in words {
                assert!(
                    body.contains(word),
                    "{} ({lang}) lacks {word}: {body}",
                    case.name
                );
            }
            // The way back is there, and nothing that is for the operator.
            assert!(
                body.contains(&format!("href=\"{p}\"")),
                "{}: {body}",
                case.name
            );
            for secret in [
                "prikk said",
                "/srv/",
                "ceiling",
                "no route",
                "the signer holds",
            ] {
                assert!(
                    !body.contains(secret),
                    "{} ({lang}) leaks {secret}",
                    case.name
                );
            }
        }
    }
}

#[tokio::test]
async fn busy_says_when_to_ask_again_and_never_a_depth_of_zero() {
    let mut c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let busy = |depth, secs| {
        Err(AcceptFailure::Busy(Busy {
            depth,
            estimate: Duration::from_secs(secs),
            why: BusyWhy::OtherProcess { holder: None },
        }))
    };
    serving(&mut c, busy(2, 45));
    let a = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    assert_eq!(a.header("retry-after"), Some("45"));
    serving(&mut c, busy(0, 0));
    let a = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    assert_eq!(a.header("retry-after"), Some("1"));
    assert!(!a.body().contains("waiting ahead"), "{}", a.body());
    serving(&mut c, busy(1, 90_000));
    let a = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    assert_eq!(a.header("retry-after"), Some("3600"));
    assert!(
        a.body()
            .contains("One acceptance is waiting ahead of yours.")
    );
}

#[tokio::test]
async fn what_is_for_the_operator_goes_to_the_log_and_nowhere_else() {
    let mut c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    serving(
        &mut c,
        Err(failed(
            words::REFUSED,
            "prikk said: the patch does not apply",
        )),
    );
    let a = accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    assert!(!a.body().contains("prikk said"));
    serving(
        &mut c,
        Err(AcceptFailure::Unavailable("lock /srv/x/lock".to_owned())),
    );
    accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    let mut done = signed();
    done.reconciliation = Reconciled::NotRun("the history could not be read".to_owned());
    serving(&mut c, Ok(Accepted::Signed(done)));
    accept(&c, &p, Some(&c.maintainer), SAME_SITE, "revision=1").await;
    let log = c.w.logged().join("\n");
    for line in [
        "the patch does not apply",
        "lock /srv/x/lock",
        "the reconciliation could not run now: the history could not be read",
    ] {
        assert!(log.contains(line), "{line} is not in the log: {log}");
    }
    assert!(log.contains("wiki `handbook`"), "{log}");
}

// ------------------------------------------------------------------------------------ the overlaps

fn overlaps_of(c: &Cast, p: &str, revision: u32) -> OverlapRecord {
    let text = |place: &str, theirs: Option<&str>, ours: Option<&str>| OverlapText {
        page: "pages/Home.md".to_owned(),
        place: place.to_owned(),
        base: Some("Second paragraph.".to_owned()),
        theirs: theirs.map(str::to_owned),
        ours: ours.map(str::to_owned),
    };
    let _ = c;
    OverlapRecord {
        proposal: id_of(p),
        revision,
        tip_block: block_id(1),
        texts: vec![
            text("block 2", Some("Second, by them."), Some("Second, by us.")),
            text("frontmatter title", Some("Start"), Some("Home page")),
            text("page", None, Some("<script>alert(1)</script>")),
        ],
        created_at: NOW,
    }
}

fn principal(id: &AccountId) -> Principal {
    Principal::session(&IdentityAuthority::for_tests(), id.clone())
}

async fn sent_back(c: &Cast, p: &str) {
    c.w.services
        .proposals
        .ask_for_revision(&principal(&c.maintainer), &c.w.wiki, &id_of(p), "Overlaps.")
        .unwrap();
}

#[tokio::test]
async fn a_proposal_sent_back_for_an_overlap_shows_both_texts_side_by_side_in_both_languages() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    c.w.store.record_overlap(&overlaps_of(&c, &p, 1)).unwrap();
    // While it is held for review, the record is not shown: it is not what the proposal is waiting for.
    let held = read(&c, &p, Some(&c.contributor)).await.body();
    assert!(!held.contains("Second, by them."), "{held}");
    sent_back(&c, &p).await;
    let page = read(&c, &p, Some(&c.contributor)).await.body();
    assert!(
        page.contains("<h2 id=\"overlaps\">Where the changes overlap</h2>"),
        "{page}"
    );
    assert!(
        page.contains("<caption>Places changed both in the wiki and in this proposal</caption>"),
        "{page}"
    );
    // Each row: where, the wiki's text, the proposal's text, in that order.
    for row in [
        "<th scope=\"row\">Home: Part 2</th><td>Second, by them.</td><td>Second, by us.</td>",
        "<th scope=\"row\">Home: Information at the top of the page: title</th><td>Start</td><td>Home page</td>",
        "<th scope=\"row\">Home: The whole page</th><td>(removed: nothing is there)</td><td>&#60;script&#62;alert(1)&#60;/script&#62;</td>",
    ] {
        assert!(page.contains(row), "{row}\n{page}");
    }
    assert!(!page.contains("<script>alert"), "{page}");
    // Anyone who may read the proposal sees them.
    assert!(read(&c, &p, None).await.body().contains("Second, by them."));
    // The same, in Japanese.
    let a = crate::common::request(
        &c.w,
        crate::common::APP,
        "GET",
        &p,
        &[
            ("x-test-account", c.contributor.as_str()),
            ("cookie", "inklkunn_lang=ja"),
        ],
    )
    .await;
    let page = a.body();
    assert!(page.contains("変更が重なっている箇所"), "{page}");
    assert!(
        page.contains(
            "<th scope=\"row\">Home: 部分 2</th><td>Second, by them.</td><td>Second, by us.</td>"
        ),
        "{page}"
    );
    assert!(
        page.contains("（削除されています: 何もありません）"),
        "{page}"
    );
}

#[tokio::test]
async fn the_overlaps_of_an_older_revision_are_not_shown_on_a_newer_one() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    c.w.store.record_overlap(&overlaps_of(&c, &p, 1)).unwrap();
    sent_back(&c, &p).await;
    // The proposer revises (revision 2); it is sent back again, for a reason of the reviewer's own.
    c.w.services
        .proposals
        .revise(
            &principal(&c.contributor),
            &c.w.wiki,
            &id_of(&p),
            ProposalEdit {
                summary: "Improve".to_owned(),
                files: vec![FileChange {
                    path: "pages/Home.md".to_owned(),
                    text: Some(HOME.replace("Second paragraph.", "Second, again.")),
                }],
                ..ProposalEdit::default()
            },
        )
        .unwrap();
    sent_back(&c, &p).await;
    let page = read(&c, &p, Some(&c.contributor)).await.body();
    assert!(page.contains("<strong>Needs a revision</strong>"), "{page}");
    assert!(!page.contains("Where the changes overlap"), "{page}");
    assert!(!page.contains("Second, by them."), "{page}");
}

#[tokio::test]
async fn a_proposal_sent_back_for_another_reason_has_no_overlaps_section() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    sent_back(&c, &p).await;
    let page = read(&c, &p, Some(&c.contributor)).await.body();
    assert!(page.contains("<strong>Needs a revision</strong>"), "{page}");
    assert!(!page.contains("Where the changes overlap"), "{page}");
}

#[tokio::test]
async fn a_post_without_an_acceptor_still_checks_first_and_says_it_is_not_available() {
    let c = cast();
    let p = propose(&c, &c.contributor, "Improve").await;
    let a = post_form(&c.w, &format!("{p}/accept"), Some(&c.maintainer), &[]).await;
    assert_eq!(a.status, StatusCode::NOT_IMPLEMENTED);
    let a = post_form(&c.w, &format!("{p}/accept"), Some(&c.contributor), &[]).await;
    assert_eq!(a.status, StatusCode::FORBIDDEN);
}
