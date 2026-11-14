//! What a reader sees of the knowledge layer (RFC 005 part 2 D-7; handoff K-T6).
//!
//! On an ordinary page: nothing new except **one quiet link on a paragraph that holds a claim** (*Sources and
//! views*) and **one** *Why is this here?* link among the page's tools. A page with no claims is the page it was,
//! byte for byte, plus that one link. The claim page says, in words, what kind of statement it is, its sources
//! with how each bears on it, and its status in this view with the proposal that decided it; it is read from the
//! derived knowledge index and never says, or suggests, that a claim is true.
//!
//! - [`marks`]: the link on a claimed paragraph, cut into the sanitized body without changing a byte of it.
//! - `show`: a claim's page, `/{slug}/-/claim/{id}`.
//! - `why`: `/{slug}/-/why/{title}` (K-T7), the page the *Why is this here?* link leads to: a page's paragraphs,
//!   each with an anchor, and for each the change that last changed it and the one that first wrote it, with the
//!   proposal, the people as the message records them, the review and the sources given, derived from the history
//!   and the proposal records alone, and labelled *traced by matching the text* wherever no anchor carries the chain.
//! - `thread` (K-T10): *Discussion of this claim* on the claim's page and the form that adds a comment to it; a discussion
//!   never changes a claim.
//! - `others`: *Other views:* on a claim's page, each view where the claim reads differently (handoff K-T8).
//! - `paths`: their addresses. `words`: what each word of a claim's file is called.

pub(crate) mod marks;
pub(crate) mod others;
pub(crate) mod paths;
pub(crate) mod show;
pub(crate) mod thread;
pub(crate) mod why;
pub(crate) mod words;
