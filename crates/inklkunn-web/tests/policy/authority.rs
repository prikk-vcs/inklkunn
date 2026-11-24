//! **Every decision page shows the rung and the policy block** (RFC 006 D-5, acceptance criterion 12; threat model GV-3; readiness
//! gap 3): the one table of the pages that name a decision, each held to the one phrase (`crate::authority`) in a world under the
//! rules of a block, in a world under the default rules, and in a world whose records say neither (a record without its rung and
//! block says *not recorded*, and nothing is guessed).
//!
//! **A new decision page must be added to [`PAGES`].** Two tests hold the table to the code: every source file of the web crate that
//! says an authority line (it calls `crate::authority`) is named by a row, so a page that begins to name a decision and is in no
//! row fails the second test; and each row is shown on the page it names, in both languages.

use std::path::Path;

use inklkunn_store::{
    AppealDecision, AppealOutcome, DecisionId, DecisionKind, DecisionRecord, GovernanceStore,
    ProposalStore,
};

use super::decisions::{link_of, page_proposal, sealed_change};
use super::page::RICH;
use super::*;
use common::{NOW, accept_by_hand_under, block_id};

/// Who is looking.
#[derive(Clone, Copy)]
enum Viewer {
    Anyone,
    Maintainer,
    Contributor,
    Admin,
}

/// Which of the three worlds a sentence is asked of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Authority {
    /// Under the rules a block set: the block is linked.
    Block,
    /// Under no rules of the wiki's own.
    Default,
    /// A record that says neither.
    NotRecorded,
}

/// One page that names a decision.
struct Page {
    /// What it is, for the failure.
    name: &'static str,
    /// Its address, from the proposal the world holds.
    uri: fn(&ProposalId) -> String,
    viewer: Viewer,
    /// The source files of the web crate whose authority line this page shows (relative to `crates/inklkunn-web/src/`).
    sources: &'static [&'static str],
    /// The sentences the page says in English, for each world (empty: the page has no such record in that world).
    says: fn(Authority, &str) -> Vec<String>,
    /// Whether the block is a link to its change on this page (a page that is only paragraphs names it by its first characters).
    linked: bool,
}

fn proposal_page(id: &ProposalId) -> String {
    format!("/handbook/-/proposals/{id}")
}

fn change_page(_: &ProposalId) -> String {
    format!("/handbook/-/change/{}", block_id(2))
}

const NOT_RECORDED: &str = "The role and the rules this was decided under were not recorded.";

fn under(authority: Authority) -> &'static str {
    match authority {
        Authority::Block => "under the rules as of ",
        _ => "under the default rules",
    }
}

/// **The table.** Add a row for a page that names a decision (an act with a rung and a block); the second test fails for a source
/// file that says an authority line and is named by no row.
const PAGES: &[Page] = &[
    Page {
        name: "the proposal page: the acceptance",
        uri: proposal_page,
        viewer: Viewer::Anyone,
        sources: &["proposals/show.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!(
                "Accepted by Mika Ito (maintainer), under the rules as of {link}."
            )],
            Authority::Default => {
                vec!["Accepted by Mika Ito (maintainer), under the default rules.".to_owned()]
            }
            Authority::NotRecorded => Vec::new(),
        },
        linked: true,
    },
    Page {
        name: "the proposal page: a reviewer's decision",
        uri: proposal_page,
        viewer: Viewer::Anyone,
        sources: &["proposals/show.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!(
                "Decided as reviewer, under the rules as of {link}."
            )],
            Authority::Default => {
                vec!["Decided as reviewer, under the default rules.".to_owned()]
            }
            Authority::NotRecorded => vec![NOT_RECORDED.to_owned()],
        },
        linked: true,
    },
    Page {
        name: "the change page: accepted by",
        uri: change_page,
        viewer: Viewer::Anyone,
        sources: &["history.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!(
                "Accepted by Mika Ito (maintainer), under the rules as of {link}."
            )],
            Authority::Default => {
                vec!["Accepted by Mika Ito (maintainer), under the default rules.".to_owned()]
            }
            Authority::NotRecorded => vec![NOT_RECORDED.to_owned()],
        },
        linked: true,
    },
    Page {
        name: "the moderation log",
        uri: |_| "/handbook/-/moderation".to_owned(),
        viewer: Viewer::Anyone,
        sources: &["tombstone/log.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!("Done under the rules as of {link}.")],
            Authority::Default => vec!["Done under the default rules.".to_owned()],
            Authority::NotRecorded => Vec::new(),
        },
        linked: true,
    },
    Page {
        name: "the people page, as the person who is restricted",
        uri: |_| "/handbook/-/people".to_owned(),
        viewer: Viewer::Contributor,
        sources: &["people/sentences.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!(
                "Set by a person holding maintainer, under the rules as of {link}."
            )],
            Authority::Default => {
                vec!["Set by a person holding maintainer, under the default rules.".to_owned()]
            }
            Authority::NotRecorded => Vec::new(),
        },
        linked: false,
    },
    Page {
        name: "the people page, as a moderator",
        uri: |_| "/handbook/-/people".to_owned(),
        viewer: Viewer::Maintainer,
        sources: &["people/roster.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!(
                "Set by a person holding maintainer, under the rules as of {link}."
            )],
            Authority::Default => {
                vec!["Set by a person holding maintainer, under the default rules.".to_owned()]
            }
            Authority::NotRecorded => Vec::new(),
        },
        linked: true,
    },
    Page {
        name: "the appeals the people who decide see",
        uri: |_| "/handbook/-/appeals".to_owned(),
        viewer: Viewer::Admin,
        sources: &["appeals/words.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!("Decided as admin, under the rules as of {link}.")],
            Authority::Default => vec!["Decided as admin, under the default rules.".to_owned()],
            Authority::NotRecorded => vec![NOT_RECORDED.to_owned()],
        },
        linked: true,
    },
    Page {
        name: "the appeals of a person's own",
        uri: |_| "/handbook/-/appeals/mine".to_owned(),
        viewer: Viewer::Contributor,
        sources: &["appeals/words.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!("Decided as admin, under the rules as of {link}.")],
            Authority::Default => vec!["Decided as admin, under the default rules.".to_owned()],
            Authority::NotRecorded => vec![NOT_RECORDED.to_owned()],
        },
        linked: true,
    },
    Page {
        name: "the page of the rules: a temporary protection",
        uri: |_| "/handbook/-/policy".to_owned(),
        viewer: Viewer::Contributor,
        sources: &["protections.rs"],
        says: |a, link| match a {
            Authority::Block => vec![format!(
                "Put in place by a person holding maintainer, under the rules as of {link}."
            )],
            Authority::Default => vec![
                "Put in place by a person holding maintainer, under the default rules.".to_owned(),
            ],
            Authority::NotRecorded => Vec::new(),
        },
        linked: true,
    },
];

/// The decisions that have **no page of their own**, and why (so that a reader of the table does not wonder): a grant or a taking
/// away of a role is a recorded decision naming who acted, their rung and the block (`core: people::tests::grants`), and no page of
/// the site lists the decisions of grants; the people page lists the roles as they are.
const WITHOUT_A_PAGE: &[&str] = &["a grant or a taking away of a role"];

struct Lived {
    e: Env,
    proposal: ProposalId,
    link: String,
}

fn viewer(l: &Lived, who: Viewer) -> Option<AccountId> {
    match who {
        Viewer::Anyone => None,
        Viewer::Maintainer => Some(l.e.maintainer.clone()),
        Viewer::Contributor => Some(l.e.contributor.clone()),
        Viewer::Admin => Some(l.e.admin.clone()),
    }
}

async fn restrict(e: &Env) -> inklkunn_store::RestrictionEvent {
    let a = post_form(
        &e.w,
        "/handbook/-/people/restrict",
        Some(&e.maintainer),
        &[
            ("who", e.contributor.as_str()),
            ("days", "3"),
            ("reason", "Too many edits."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    e.w.store
        .restriction_events(&e.w.wiki.id, &e.contributor)
        .unwrap()
        .remove(0)
}

async fn file_and_get_the_appeal(e: &Env, event: &inklkunn_store::RestrictionEvent) -> String {
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/new",
        Some(&e.contributor),
        &[
            ("kind", "restriction"),
            ("id", event.id.as_str()),
            ("text", "I did nothing wrong."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    e.w.store
        .open_appeals(&e.w.wiki.id, 5)
        .unwrap()
        .remove(0)
        .id
        .to_string()
}

/// A world in which every kind of decision of the table was made: a comment hidden, a review, a restriction that was appealed and
/// decided, a temporary protection, and an acceptance that was sealed (the change). Under the rules of `block`, or the default.
async fn lived(e: Env, block: Option<&str>) -> Lived {
    let id = page_proposal(&e).await;
    // A comment by the contributor, hidden by the maintainer.
    let a = post_form(
        &e.w,
        &format!("/handbook/-/proposals/{id}/comment"),
        Some(&e.contributor),
        &[("body", "A comment to hide.")],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    let comment =
        e.w.store
            .comments(&id, None)
            .unwrap()
            .first()
            .expect("a comment")
            .id
            .to_string();
    let a = post_form(
        &e.w,
        "/handbook/-/moderate",
        Some(&e.maintainer),
        &[
            ("kind", "proposal-comment"),
            ("proposal", id.as_str()),
            ("comment", &comment),
            ("part_content", "1"),
            ("category", "spam"),
            ("note", ""),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    // A review.
    let a = post_form(
        &e.w,
        &format!("/handbook/-/proposals/{id}/decide"),
        Some(&e.reviewer),
        &[
            ("op", "approve"),
            ("revision", "1"),
            ("body", "Looks right."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    // A temporary protection.
    let a = post_form(
        &e.w,
        "/handbook/-/protect",
        Some(&e.maintainer),
        &[
            ("target", "page"),
            ("title", "Home"),
            ("require", "review"),
            ("reason", "Vandalism this week."),
            ("days", "7"),
            ("act", "set"),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    // A restriction, appealed and decided by the admin.
    let event = restrict(&e).await;
    let appeal = file_and_get_the_appeal(&e, &event).await;
    let a = post_form(
        &e.w,
        "/handbook/-/appeals/decide",
        Some(&e.admin),
        &[
            ("appeal", &appeal),
            ("decision", "upheld"),
            ("reason", "The record supports it."),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    // The acceptance and the change it sealed.
    accept_by_hand_under(&e.w, &id, &e.maintainer, &e.contributor, block, false);
    sealed_change(&e, &id);
    let link = match block {
        Some(block) => link_of(block),
        None => String::new(),
    };
    Lived {
        e,
        proposal: id,
        link,
    }
}

/// A world whose records say neither the rung nor the block: a review, an acceptance that no record names, and an appeal's decision,
/// each as they were written before the columns existed.
async fn unrecorded() -> Lived {
    let e = env();
    let id = page_proposal(&e).await;
    e.w.store
        .add_decision(&DecisionRecord {
            id: DecisionId::new(),
            proposal: id.clone(),
            revision: 1,
            reviewer: e.maintainer.clone(),
            kind: DecisionKind::Approve,
            body: String::new(),
            reviewer_rung: None,
            policy_block: None,
            created_at: NOW,
        })
        .unwrap();
    sealed_change(&e, &id);
    let event = restrict(&e).await;
    let appeal = file_and_get_the_appeal(&e, &event).await;
    let appeal =
        e.w.store
            .open_appeals(&e.w.wiki.id, 5)
            .unwrap()
            .into_iter()
            .find(|a| a.id.to_string() == appeal)
            .expect("the appeal");
    e.w.store
        .decide_appeal(
            &appeal.id,
            &AppealDecision {
                at: NOW,
                by: e.admin.clone(),
                reason: "The record supports it.".to_owned(),
                rung: None,
                policy_block: None,
                outcome: AppealOutcome::Upheld,
            },
        )
        .unwrap();
    Lived {
        e,
        proposal: id,
        link: String::new(),
    }
}

async fn worlds() -> [(Authority, Lived); 3] {
    [
        (
            Authority::Block,
            lived(env_under(RICH), Some(common::POLICY_BLOCK)).await,
        ),
        (Authority::Default, lived(env(), None).await),
        (Authority::NotRecorded, unrecorded().await),
    ]
}

#[tokio::test]
async fn every_page_that_names_a_decision_says_the_role_and_the_rules_or_that_they_were_not_recorded()
 {
    assert!(!WITHOUT_A_PAGE.is_empty());
    for (authority, l) in worlds().await {
        for page in PAGES {
            let uri = (page.uri)(&l.proposal);
            let who = viewer(&l, page.viewer);
            let a = match &who {
                Some(who) => as_(&l.e, who, &uri).await,
                None => common::get(&l.e.w, &uri).await,
            };
            assert_eq!(
                a.status,
                StatusCode::OK,
                "{} ({authority:?}): {uri}",
                page.name
            );
            let main = main_of(&a);
            // The block as this page names it: the link to its change, or its first characters.
            let mark = if page.linked {
                l.link.clone()
            } else {
                common::POLICY_BLOCK[..12].to_owned()
            };
            let wanted = (page.says)(authority, &mark);
            for sentence in &wanted {
                assert!(
                    main.contains(sentence.as_str()),
                    "{} ({authority:?}): {sentence}\n{main}",
                    page.name
                );
            }
            // A record that names neither is never said to have been made under the default rules: nothing is guessed.
            if authority == Authority::NotRecorded && !wanted.is_empty() {
                assert!(
                    !main.contains(under(Authority::Default)),
                    "{} says a record that names neither was made under the default rules: {main}",
                    page.name
                );
            }
            // In Japanese, the block is the same link, and the default is said in words.
            if !wanted.is_empty() {
                let ja = as_ja(&l.e, who.as_ref(), &uri).await;
                let ja = main_of(&ja);
                match authority {
                    Authority::Block => assert!(
                        ja.contains(&mark),
                        "{} (ja): the block's link is not there: {ja}",
                        page.name
                    ),
                    Authority::Default => assert!(
                        ja.contains("既定のルール"),
                        "{} (ja): the default rules are not said: {ja}",
                        page.name
                    ),
                    Authority::NotRecorded => assert!(
                        ja.contains("記録されていません"),
                        "{} (ja): the missing record is not said: {ja}",
                        page.name
                    ),
                }
            }
        }
    }
}

/// The `.rs` files below `dir`, relative to `root`, that call `crate::authority` (outside that module itself).
fn sources_that_say_an_authority_line(root: &Path, dir: &Path, found: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources_that_say_an_authority_line(root, &path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            if rel != "authority.rs" && text.contains("crate::authority::") {
                found.push(rel);
            }
        }
    }
}

#[test]
fn every_source_that_says_an_authority_line_is_a_row_of_the_table() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = Vec::new();
    sources_that_say_an_authority_line(&root, &root, &mut found);
    found.sort();
    assert!(
        found.len() >= 6,
        "the scan found only {found:?}: is the source tree being read?"
    );
    let mut covered: Vec<&str> = PAGES
        .iter()
        .flat_map(|p| p.sources.iter().copied())
        .collect();
    covered.sort_unstable();
    covered.dedup();
    let missing: Vec<&String> = found
        .iter()
        .filter(|f| !covered.contains(&f.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "a page names a decision and is in no row of PAGES (add it: criterion 12): {missing:?}"
    );
    let stale: Vec<&&str> = covered
        .iter()
        .filter(|c| !found.iter().any(|f| f == **c))
        .collect();
    assert!(
        stale.is_empty(),
        "a row names a source that says no authority line: {stale:?}"
    );
}
