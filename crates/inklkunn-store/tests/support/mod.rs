//! Fixtures the proposal, acceptance and inbox record tests share (handoff 004 T1): a wiki, accounts, and
//! the smallest valid records, built the same way for both stores. Each test crate uses a different part.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

use inklkunn_store::ProposalChange;
use inklkunn_store::{
    Account, AccountId, AccountStore, Base, DisplayName, FileChange, NewWiki, ProposalId,
    ProposalOp, ProposalRecord, ProposalState, ProposalStore, ProposalTransition, RevisionContent,
    RevisionRecord, Slug, WikiId, WikiRecord, WikiStore,
};

pub struct TempDir(pub PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn temp(tag: &str) -> TempDir {
    let dir = std::env::temp_dir().join(format!(
        "inklkunn-proposals-{tag}-{}-{}",
        std::process::id(),
        AccountId::new()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir");
    TempDir(dir)
}

pub fn wiki(store: &impl WikiStore, slug: &str) -> WikiRecord {
    store
        .create(NewWiki::new(
            Slug::new(slug).expect("a slug"),
            DisplayName::new(slug).expect("a name"),
        ))
        .expect("a wiki")
}

pub fn account(store: &impl AccountStore) -> AccountId {
    let account = Account {
        id: AccountId::new(),
        display_name: DisplayName::new("Somebody").expect("a name"),
        local: None,
        created_at: 1,
        disabled: false,
    };
    store.add_account(&account).expect("an account");
    account.id
}

/// A base made from one number: both ids 64 lower-case hex characters.
pub fn base(n: u64) -> Base {
    Base::new(&format!("{n:064x}"), &format!("{:064x}", 0xb10c_0000 + n)).expect("a base")
}

pub fn content(n: u64, text: &str) -> RevisionContent {
    RevisionContent {
        base: base(n),
        files: vec![FileChange {
            path: "pages/home.md".to_owned(),
            text: Some(text.to_owned()),
        }],
        renames: Vec::new(),
        minor: false,
        automated: false,
    }
}

/// The record of a fresh draft.
pub fn draft_record(wiki: &WikiId, author: &AccountId, at: i64) -> ProposalRecord {
    ProposalRecord {
        id: ProposalId::new(),
        wiki: wiki.clone(),
        view: "heads/main".to_owned(),
        state: ProposalState::Draft,
        author: author.clone(),
        summary: "Fix a typo".to_owned(),
        sources: vec!["https://example.org/a".to_owned()],
        superseded_by: None,
        created_at: at,
        updated_at: at,
        submitted_at: None,
        closed_at: None,
    }
}

pub fn step(
    p: &ProposalId,
    op: ProposalOp,
    from: Option<ProposalState>,
    to: ProposalState,
    actor: &AccountId,
    reason: Option<&str>,
    at: i64,
) -> ProposalTransition {
    ProposalTransition {
        proposal: p.clone(),
        op,
        from,
        to,
        actor: actor.clone(),
        reason: reason.map(str::to_owned),
        at,
    }
}

/// Open a draft for `author`.
pub fn open_draft(
    store: &impl ProposalStore,
    wiki: &WikiId,
    author: &AccountId,
    at: i64,
) -> ProposalRecord {
    let record = draft_record(wiki, author, at);
    let created = step(
        &record.id,
        ProposalOp::Create,
        None,
        ProposalState::Draft,
        author,
        None,
        at,
    );
    store
        .create_proposal(&record, &content(1, "draft text"), &created)
        .expect("create");
    record
}

pub fn revision(p: &ProposalId, seq: u32, text: &str, at: i64) -> RevisionRecord {
    RevisionRecord {
        proposal: p.clone(),
        seq,
        content: content(u64::from(seq), text),
        created_at: at,
    }
}

/// The `n`th of a list (tests may not index).
pub fn nth<T: Clone>(items: &[T], n: usize) -> T {
    items.get(n).cloned().expect("the list is long enough")
}

/// Submit a draft with revision 1.
pub fn submit(store: &impl ProposalStore, p: &ProposalRecord, at: i64) -> ProposalRecord {
    store
        .change_proposal(&ProposalChange {
            transition: step(
                &p.id,
                ProposalOp::Submit,
                Some(ProposalState::Draft),
                ProposalState::HeldForReview,
                &p.author,
                None,
                at,
            ),
            text: None,
            revision: Some(revision(&p.id, 1, "first text", at)),
            superseded_by: None,
        })
        .expect("submit")
}

/// Move a proposal from `from` to `to` with `op`, by `actor`.
pub fn mv(
    store: &impl ProposalStore,
    p: &ProposalId,
    op: ProposalOp,
    from: ProposalState,
    to: ProposalState,
    actor: &AccountId,
    at: i64,
) -> ProposalRecord {
    store
        .change_proposal(&ProposalChange {
            transition: step(p, op, Some(from), to, actor, Some("a reason"), at),
            text: None,
            revision: None,
            superseded_by: None,
        })
        .expect("move")
}
