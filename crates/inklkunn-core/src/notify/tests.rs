//! Who is told what (handoff 004 T6; RFC 004 D-9) end to end over both stores: the proposal service makes the
//! events when a proposal is reviewed, declined, sent back, withdrawn, superseded, replied to or mentioned,
//! the acceptance's functions make the accepted and watched-page events, and the inbox shows each as the
//! person who opens it may be told, including after the wiki stopped being theirs to read.
#![allow(clippy::expect_used, clippy::unwrap_used)]

mod acceptance;
mod acts;
mod flow;
mod mentions;
mod world;
