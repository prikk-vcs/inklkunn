//! **A hide on a proposal's revision reaches the page change it became** (RFC 006 D-8; INV-13; handoff G-T4, correction H-2). The
//! proposer of a proposal is one person: when the author of its *first* revision is hidden and the proposal is accepted at its
//! *second*, every surface of the page change that the acceptance sealed says *hidden* in the proposer's place, and no byte of any of
//! them names the person. The content and the summary of a revision that was not the one sealed hide nothing of the change. The
//! records are the in-memory store's and the repository is a double. The person's name carries `ZEBRA`.

use axum::http::StatusCode;
use inklkunn_core::proposal::ProposalEdit;
use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, AcceptanceRecord, AcceptanceResult, AcceptanceStore,
    AccountId, FileChange, ProposalChange, ProposalId, ProposalOp, ProposalState, ProposalStore,
    ProposalTransition, Role,
};

use super::{AUTHOR, HIDING, HOME, borrowed, proposal_item, propose};
use crate::common::{
    Answer, NOW, World, add_person, block_id, get_as, op, person, post_form, shown, with_accounts,
};

const TEXT_ONE: &str = "---\ntitle: Home\n---\n\nFirst paragraph.\n\nOne more paragraph.\n";
const TEXT_TWO: &str = "---\ntitle: Home\n---\n\nFirst paragraph.\n\nTwo more paragraphs.\n";
const FIRST: &str = "A first try at the paragraph";
const SECOND: &str = "The paragraph, said better";

struct Seen {
    w: World,
    maintainer: AccountId,
    author: AccountId,
    reader: AccountId,
    proposal: ProposalId,
}

/// The accepted proposal's message, naming the proposer by `AUTHOR`.
fn message(id: &ProposalId, proposer: &AccountId, acceptor: &AccountId) -> String {
    use inklkunn_core::message::{Actor, BlockId, Message, Summary};
    use inklkunn_store::DisplayName;
    let actor = |id: &AccountId, name: &str| Actor::Account {
        id: id.clone(),
        name: DisplayName::new(name).expect("name"),
    };
    Message::proposal(
        Summary::new(SECOND).expect("summary"),
        id.clone(),
        actor(proposer, AUTHOR),
        actor(acceptor, "Mika Ito"),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a message")
    .to_text()
}

/// The acceptance of revision `revision`, sealed as change 2, by hand (the records of it, as the acceptor writes them).
fn accept_revision(
    w: &World,
    id: &ProposalId,
    by: &AccountId,
    proposer: &AccountId,
    revision: u32,
) {
    w.store
        .change_proposal(&ProposalChange {
            transition: ProposalTransition {
                proposal: id.clone(),
                op: ProposalOp::Accept,
                from: Some(ProposalState::HeldForReview),
                to: ProposalState::Accepted,
                actor: by.clone(),
                reason: None,
                at: NOW,
            },
            text: None,
            revision: None,
            superseded_by: None,
        })
        .unwrap();
    let acceptance = AcceptanceId::new();
    w.store
        .begin_acceptance(&AcceptanceRecord {
            policy_block: None,
            hatch: false,
            id: acceptance.clone(),
            proposal: id.clone(),
            revision,
            wiki: w.wiki.id.clone(),
            view: "heads/main".to_owned(),
            acceptor: by.clone(),
            acceptor_role: Role::Maintainer,
            policy_version: "bootstrap".to_owned(),
            proposer: proposer.clone(),
            digest: "d".repeat(64),
            bytes: 10,
            created_at: NOW,
        })
        .unwrap();
    w.store
        .finish_acceptance(&AcceptanceResult {
            acceptance,
            outcome: AcceptanceOutcome::Sealed {
                block_id: block_id(2),
                patch_id: "a".repeat(64),
            },
            at: NOW,
        })
        .unwrap();
}

/// The wiki: the author proposes (revision 1), revises (revision 2), and revision 2 is accepted as change 2, the tip.
async fn seen() -> Seen {
    let w = with_accounts(World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", HOME)],
    ));
    let (maintainer, author, reader) = (person(4), person(1), person(6));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &author, AUTHOR, Some(Role::Contributor));
    add_person(&w, &reader, "Olu Reader", Some(Role::Observer));
    let proposal = propose(&w, &author, TEXT_ONE, FIRST).await;
    let principal = Principal::session(&IdentityAuthority::for_tests(), author.clone());
    w.services
        .proposals
        .revise(
            &principal,
            &w.wiki,
            &proposal,
            ProposalEdit {
                summary: SECOND.to_owned(),
                files: vec![FileChange {
                    path: "pages/Home.md".to_owned(),
                    text: Some(TEXT_TWO.to_owned()),
                }],
                ..ProposalEdit::default()
            },
        )
        .expect("a second revision");
    accept_revision(&w, &proposal, &maintainer, &author, 2);
    w.fake.say(1, "Create the handbook");
    w.fake
        .show(1, shown(vec![op("create-file", &["pages/Home.md"])]));
    w.fake.set_files(&[("pages/Home.md", TEXT_TWO)]);
    w.fake.say(2, &message(&proposal, &author, &maintainer));
    w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
    Seen {
        w,
        maintainer,
        author,
        reader,
        proposal,
    }
}

impl Seen {
    fn viewers(&self) -> Vec<Option<AccountId>> {
        vec![
            None,
            Some(self.reader.clone()),
            Some(self.author.clone()),
            Some(self.maintainer.clone()),
        ]
    }

    async fn hide(&self, seq: u32, parts: &[&str]) -> Answer {
        let item = proposal_item(&self.proposal, seq);
        let mut fields: Vec<(&str, &str)> = borrowed(&item);
        for p in parts {
            fields.push((p, "1"));
        }
        fields.push(("category", "harassment"));
        fields.push(("note", ""));
        post_form(
            &self.w,
            "/handbook/-/moderate",
            Some(&self.maintainer),
            &fields,
        )
        .await
    }

    /// Every surface of the page change, HTML and API.
    fn surfaces(&self) -> Vec<String> {
        let b = block_id(2);
        [
            "/handbook/-/history/Home".to_owned(),
            format!("/handbook/-/change/{b}"),
            "/handbook/-/changes".to_owned(),
            "/handbook/-/changes.atom".to_owned(),
            "/handbook/-/why/Home".to_owned(),
            "/handbook/-/api/v1/history/Home".to_owned(),
            "/handbook/-/api/v1/changes".to_owned(),
            format!("/handbook/-/api/v1/revisions/{b}"),
            "/handbook/-/api/v1/provenance/Home".to_owned(),
        ]
        .into()
    }
}

#[tokio::test]
async fn an_author_hidden_on_the_first_revision_is_hidden_on_the_change_the_second_became_everywhere()
 {
    let s = seen().await;
    // Before: the surfaces name the proposer (the fixture is not vacuous).
    let mut naming = 0;
    for uri in s.surfaces() {
        let a = read_as(&s, &uri, None).await;
        assert_eq!(a.status, StatusCode::OK, "{uri}: {}", a.body());
        if a.body().contains(AUTHOR) {
            naming += 1;
        }
    }
    assert!(naming >= 4, "{naming} surfaces name the proposer");
    let a = s.hide(1, &["part_author"]).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    for uri in s.surfaces() {
        for who in s.viewers() {
            let a = read_as(&s, &uri, who.as_ref()).await;
            assert_eq!(a.status, StatusCode::OK, "{uri} as {who:?}: {}", a.body());
            assert!(
                !a.body().contains(AUTHOR) && !a.body().contains("Zed"),
                "{uri} as {who:?} names the proposer: {}",
                a.body()
            );
        }
    }
    // Where the person was named there is a tombstone in the person's place.
    let history = read_as(&s, "/handbook/-/history/Home", None).await.body();
    assert!(history.contains(HIDING), "{history}");
    // The summary of the change is the one that was sealed, and is shown: the hide was of the author alone.
    assert!(history.contains(SECOND), "{history}");
}

async fn read_as(s: &Seen, uri: &str, who: Option<&AccountId>) -> Answer {
    let uri = uri.replace("{b}", &block_id(2));
    get_as(&s.w, &uri, who).await
}

#[tokio::test]
async fn an_unhide_gives_the_name_back_on_every_surface_exactly() {
    let s = seen().await;
    let mut before = Vec::new();
    for uri in s.surfaces() {
        before.push(read_as(&s, &uri, None).await.bytes);
    }
    s.hide(1, &["part_author"]).await;
    let log = inklkunn_store::GovernanceStore::tombstone_log(&*s.w.store, &s.w.wiki.id, None, 50)
        .expect("the log");
    for hide in log
        .iter()
        .filter(|e| e.act == inklkunn_store::TombstoneAct::Hide)
    {
        let a = post_form(
            &s.w,
            "/handbook/-/moderate/unhide",
            Some(&s.maintainer),
            &[("hide", hide.id.as_str())],
        )
        .await;
        assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
    }
    let mut after = Vec::new();
    for uri in s.surfaces() {
        after.push(read_as(&s, &uri, None).await.bytes);
    }
    assert_eq!(before, after);
}

#[tokio::test]
async fn the_content_and_the_summary_of_a_revision_that_was_not_sealed_hide_nothing_of_the_change()
{
    let s = seen().await;
    // Revision 1's summary cannot be hidden by the form (an earlier revision has none to hide), and its content was not what the
    // change wrote: hiding the content of revision 1 changes no surface of the change.
    let mut before = Vec::new();
    for uri in s.surfaces() {
        before.push(read_as(&s, &uri, None).await.bytes);
    }
    let a = s.hide(1, &["part_content"]).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let mut after = Vec::new();
    for uri in s.surfaces() {
        after.push(read_as(&s, &uri, None).await.bytes);
    }
    assert_eq!(before, after);
}
