//! The records behind the signer's guard and behind the reconciliation, read from the service store (RFC 004
//! D-4, D-14; RFC 002 D-4, D-7; threat model C-1b).
//!
//! **The signer reads the record the token names, and a caller cannot invent what it says** ([`StoreLedger`]):
//! an acceptance is *pending seal* only while its record has no end and its intent is open, and every fact the
//! seal is bound to (who proposed, who accepted, the kind, the base, the files' digest, the tip the change must
//! still be written on) is read back from those records, the message from the intent's own text. Nothing is kept
//! in memory between the records and the signer, so a restart loses nothing and a test double cannot stand in
//! for the store without the test saying so.
//!
//! The reconciliation compares the history with **the same records** ([`StoreActs`]): a sealed acceptance names
//! its block, and a system act (an import) names its block or, while it is open, none. A failed act took
//! nothing in and is no record of one.

use std::sync::Arc;

use inklkunn_signer::accept::{ChangeDigest, ExpectedMessage};
use inklkunn_signer::message::{BlockId, Message};
use inklkunn_signer::{
    AcceptanceLedger, AcceptanceToken, LedgerError, RecoveryFacts, RecoveryToken, ViewCreationAct,
    ViewCreationFacts,
};
use inklkunn_store::{
    AcceptanceId, AcceptanceOutcome, AcceptanceStore, ActStore, RecoveryActId, SystemActId, WikiId,
};
use sha2::{Digest as _, Sha256};

use crate::reconcile::{AcceptedRecord, RecordedActs, RecordsError, SystemActRecord};

/// The SHA-256 of a text in lower-case hex: the digest an intent keeps of its message.
pub fn message_digest(message: &str) -> String {
    hex(&Sha256::digest(message.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A [`ChangeDigest`] from the 64 hex characters a record keeps.
pub fn digest_from_hex(text: &str) -> Option<ChangeDigest> {
    if text.len() != 64
        || !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let mut out = [0_u8; 32];
    for (byte, pair) in out.iter_mut().zip(text.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(ChangeDigest::from_bytes(out))
}

/// What an act's records say was accepted, as the signer's guard and the reconciliation read it. The message
/// is parsed, never trusted: its digest must be the one the record kept, and it must be a patch message.
pub fn facts_of(
    wiki: &WikiId,
    view: &str,
    message: &str,
    message_digest_hex: &str,
    base_block: &str,
    change_digest_hex: &str,
) -> Result<ExpectedMessage, String> {
    if self::message_digest(message) != message_digest_hex {
        return Err("the intent's message is not the one its digest was made from".to_owned());
    }
    let parsed = Message::parse(message).map_err(|e| format!("the intent's message: {e}"))?;
    let change_digest = digest_from_hex(change_digest_hex)
        .ok_or_else(|| "the record's change digest is not a digest".to_owned())?;
    let pinned = BlockId::new(base_block).map_err(|e| format!("the intent's base: {e}"))?;
    Ok(ExpectedMessage {
        wiki: wiki.clone(),
        view: view.to_owned(),
        kind: parsed.kind(),
        proposal: parsed.proposal_id().cloned(),
        proposed_by: parsed.proposed_by().clone(),
        accepted_by: parsed.accepted_by().clone(),
        base: parsed.base().cloned(),
        imported_from: parsed.imported_from().cloned(),
        pinned_tip: Some(pinned),
        change_digest,
    })
}

/// The signer's ledger, over the service store: the acceptance records and the system acts.
pub struct StoreLedger {
    acceptances: Arc<dyn AcceptanceStore>,
    acts: Arc<dyn ActStore>,
}

impl std::fmt::Debug for StoreLedger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StoreLedger")
    }
}

impl StoreLedger {
    /// The ledger over these records (one SQLite store serves both in service).
    pub fn new(acceptances: Arc<dyn AcceptanceStore>, acts: Arc<dyn ActStore>) -> Self {
        StoreLedger { acceptances, acts }
    }
}

impl AcceptanceLedger for StoreLedger {
    fn pending(&self, token: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        let unreadable =
            |what: &str, e: &dyn std::fmt::Display| LedgerError(format!("{what}: {e}"));
        if let Some(id) = AcceptanceId::parse(token.as_str())
            && let Some((a, i)) = self
                .acceptances
                .pending_acceptance(&id)
                .map_err(|e| unreadable("the acceptance record", &e))?
        {
            return facts_of(
                &a.wiki,
                &a.view,
                &i.message,
                &i.message_digest,
                &i.base_block,
                &a.digest,
            )
            .map(Some)
            .map_err(LedgerError);
        }
        if let Some(id) = SystemActId::parse(token.as_str())
            && let Some(act) = self
                .acts
                .open_system_act(&id)
                .map_err(|e| unreadable("the system act record", &e))?
        {
            return facts_of(
                &act.wiki,
                &act.view,
                &act.message,
                &act.message_digest,
                &act.base_block,
                &act.digest,
            )
            .map(Some)
            .map_err(LedgerError);
        }
        Ok(None)
    }

    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        // View creation has its own recorded act (a later handoff); until it does, none is pending.
        Ok(None)
    }

    fn recovery(&self, act: &RecoveryToken) -> Result<Option<RecoveryFacts>, LedgerError> {
        let Some(id) = RecoveryActId::parse(act.as_str()) else {
            return Ok(None);
        };
        let open = self
            .acts
            .open_recovery_act(&id)
            .map_err(|e| LedgerError(format!("the recovery act record: {e}")))?;
        Ok(open.and_then(|record| crate::recovery::facts_of(&record.wiki, &record.plan)))
    }
}

/// A ledger with nothing pending: for a signer that is only opened to create wikis or to clear what a killed
/// creation left (the bootstrap and the start-up sweep), which are bound to no acceptance. Every `accept` of
/// such a signer is refused as not pending.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoLedger;

impl AcceptanceLedger for NoLedger {
    fn pending(&self, _: &AcceptanceToken) -> Result<Option<ExpectedMessage>, LedgerError> {
        Ok(None)
    }

    fn view_creation(&self, _: &ViewCreationAct) -> Result<Option<ViewCreationFacts>, LedgerError> {
        Ok(None)
    }
}

/// The reconciliation's records, over the service store.
pub struct StoreActs {
    acceptances: Arc<dyn AcceptanceStore>,
    acts: Arc<dyn ActStore>,
}

impl std::fmt::Debug for StoreActs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StoreActs")
    }
}

impl StoreActs {
    /// The records of these stores.
    pub fn new(acceptances: Arc<dyn AcceptanceStore>, acts: Arc<dyn ActStore>) -> Self {
        StoreActs { acceptances, acts }
    }
}

impl RecordedActs for StoreActs {
    fn accepted(&self, wiki: &WikiId) -> Result<Vec<AcceptedRecord>, RecordsError> {
        let sealed = self
            .acceptances
            .sealed_acceptances(wiki)
            .map_err(|e| RecordsError(e.to_string()))?;
        sealed
            .into_iter()
            .map(|s| {
                let facts = facts_of(
                    &s.acceptance.wiki,
                    &s.acceptance.view,
                    &s.intent.message,
                    &s.intent.message_digest,
                    &s.intent.base_block,
                    &s.acceptance.digest,
                )
                .map_err(RecordsError)?;
                Ok(AcceptedRecord {
                    id: s.acceptance.id.to_string(),
                    block_id: s.block_id,
                    facts,
                })
            })
            .collect()
    }

    fn system_acts(&self, wiki: &WikiId) -> Result<Vec<SystemActRecord>, RecordsError> {
        let entries = self
            .acts
            .system_acts(wiki)
            .map_err(|e| RecordsError(e.to_string()))?;
        let mut out = Vec::with_capacity(entries.len());
        for entry in entries {
            let block_id = match entry.end.as_ref().map(|e| &e.outcome) {
                // A failed act took nothing in.
                Some(AcceptanceOutcome::Failed { .. }) => continue,
                Some(AcceptanceOutcome::Sealed { block_id, .. }) => Some(block_id.clone()),
                None => None,
            };
            let a = &entry.act;
            let facts = facts_of(
                &a.wiki,
                &a.view,
                &a.message,
                &a.message_digest,
                &a.base_block,
                &a.digest,
            )
            .map_err(RecordsError)?;
            out.push(SystemActRecord {
                id: a.id.to_string(),
                block_id,
                facts,
            });
        }
        Ok(out)
    }
}
