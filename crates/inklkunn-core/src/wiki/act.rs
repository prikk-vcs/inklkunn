//! The record of a wiki's creation (handoff 004 T4b; RFC 004 D-14; RFC 002 D-7): **every block of a history has
//! a recorded act behind it**, and the first block, the creation, is a system change like any other. It is
//! kept as the same record [`SystemActs`](crate::acceptance::SystemActs) writes (`Kind: system`, begun and then
//! ended as sealed), so the reconciliation matches the first block to it and reports nothing.
//!
//! **Two things a creation does not have** are written as 64 zeros, and nothing reads them: a *base block* (it is
//! the first block) and a *digest of the files* (the bootstrap lays the bundle files down and the signer's
//! first-change entry commits whatever is there; no change was prepared against a tip, which is what the
//! digest binds). The reconciliation compares the message, not these.
//!
//! **The record and the act are one transaction** (corrections P8, D6): the act refers to the wiki's record, so
//! the store takes both together ([`ActStore::insert_wiki_with_creation_act`]) and a kill cannot leave one
//! without the other. The sweep's repair ([`repair`]) stays as the second line, for a wiki that an earlier build
//! stored in two steps and a kill split: it writes the act from the history, and the mark is removed only when it
//! exists.

use inklkunn_prikk::{Point, Storage};
use inklkunn_signer::message::{Kind, Message};
use inklkunn_store::{
    AcceptanceOutcome, ActStore, RootLayout, StoreError, SystemActEnd, SystemActId, SystemActKind,
    SystemActRecord, WikiRecord,
};

use crate::acceptance::message_digest;

/// What a creation's record keeps where an act of another kind keeps a base block and a digest of files.
const NONE: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The creation of `wiki` as a sealed system act, start and end: `block_id` is the first block, `patch_id` its
/// patch, `message` the text it was sealed with.
pub(super) fn entries(
    wiki: &WikiRecord,
    message: &str,
    block_id: &str,
    patch_id: &str,
    at: i64,
) -> (SystemActRecord, SystemActEnd) {
    let id = SystemActId::new();
    let begin = SystemActRecord {
        id: id.clone(),
        wiki: wiki.id.clone(),
        view: wiki.default_view.clone(),
        kind: SystemActKind::System,
        base_block: NONE.to_owned(),
        message: message.to_owned(),
        message_digest: message_digest(message),
        digest: NONE.to_owned(),
        bytes: 0,
        created_at: at,
    };
    let end = SystemActEnd {
        act: id,
        outcome: AcceptanceOutcome::Sealed {
            block_id: block_id.to_owned(),
            patch_id: patch_id.to_owned(),
        },
        at,
    };
    (begin, end)
}

/// Keep the creation of a wiki whose record is stored already as a sealed system act (the sweep's repair, for a
/// creation of an earlier build; a creation now stores the record and the act together).
fn record(
    acts: &dyn ActStore,
    wiki: &WikiRecord,
    message: &str,
    block_id: &str,
    patch_id: &str,
    at: i64,
) -> Result<(), StoreError> {
    let (begin, end) = entries(wiki, message, block_id, patch_id, at);
    acts.begin_system_act(&begin)?;
    acts.finish_system_act(&end)
}

/// Write the record of a creation that was stored and not recorded (a creation killed between the two), from
/// the history. `Ok(true)`: it was written; `Ok(false)`: the wiki has an act already, and nothing is changed.
/// `Err` is the reason it could not be done, for the operator; the caller leaves the mark so that the next
/// start tries again.
pub(super) fn repair(
    acts: &dyn ActStore,
    storage: &dyn Storage,
    layout: &RootLayout,
    wiki: &WikiRecord,
    at: i64,
) -> Result<bool, String> {
    let have = acts
        .system_acts(&wiki.id)
        .map_err(|e| format!("the wiki's acts could not be read ({e})"))?;
    if !have.is_empty() {
        return Ok(false);
    }
    let repo = layout.repo_dir(&wiki.id);
    let history = storage
        .history(&repo, &Point::View(wiki.default_view.clone()), None)
        .map_err(|e| format!("the wiki's history could not be read ({e})"))?;
    // The oldest block that carries a message is the creation.
    let (block, patch) = history
        .blocks
        .iter()
        .rev()
        .find_map(|b| b.patch_messages.first().map(|p| (b, p)))
        .ok_or_else(|| "the wiki's history has no first change to record".to_owned())?;
    let is_system = Message::parse(&patch.message).is_ok_and(|m| m.kind() == Kind::System);
    if !is_system {
        return Err(
            "the wiki's first change is not a system change; nothing was recorded".to_owned(),
        );
    }
    record(
        acts,
        wiki,
        &patch.message,
        &block.block_id,
        &patch.patch_id,
        at,
    )
    .map_err(|e| format!("the creation's record could not be kept ({e})"))?;
    Ok(true)
}
