//! Where an event goes (RFC 004 D-9): **channels are a trait with one implementation in M2**, the in-app
//! inbox. E-mail and digests are a later RFC and the person's choice; a channel added then implements this
//! trait and nothing in the service changes.
//!
//! A channel is handed an event that carries ids and a kind and nothing else, so a channel that sends it
//! somewhere the page's authorization does not reach (an e-mail) has no content to leak; what it may say about
//! the event it has to read, as the recipient, through the same authorization the inbox page uses.

use std::sync::Arc;

use inklkunn_store::{InboxEvent, InboxStore, StoreError};

/// Why a channel could not take an event. The text is for the operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelError(pub String);

impl std::fmt::Display for ChannelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "the channel could not take the event: {}", self.0)
    }
}

impl std::error::Error for ChannelError {}

impl From<StoreError> for ChannelError {
    fn from(e: StoreError) -> Self {
        ChannelError(e.to_string())
    }
}

/// A way to tell someone that something happened.
pub trait Channel: Send + Sync {
    /// The channel's name, for the operator's log (`in-app`).
    fn name(&self) -> &'static str;

    /// Deliver one event to the account it is for.
    fn deliver(&self, event: &InboxEvent) -> Result<(), ChannelError>;
}

/// The in-app inbox (`/-/inbox`): an event is a record in the account's inbox, read when the person looks.
pub struct InApp {
    inbox: Arc<dyn InboxStore>,
}

impl InApp {
    /// The in-app channel over these records.
    pub fn new(inbox: Arc<dyn InboxStore>) -> Self {
        InApp { inbox }
    }
}

impl std::fmt::Debug for InApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InApp")
    }
}

impl Channel for InApp {
    fn name(&self) -> &'static str {
        "in-app"
    }

    fn deliver(&self, event: &InboxEvent) -> Result<(), ChannelError> {
        Ok(self.inbox.push_event(event)?)
    }
}
