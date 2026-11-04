//! The knowledge layer's files: the reader, the writer, the digest, the refusals, and (as they land) the
//! validator, the merge and the index.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

/// A claim id with a distinguishing tail (a valid version-7 UUID).
pub(crate) fn claim_id(n: u8) -> ClaimId {
    ClaimId::parse(&format!("01930000-0000-7000-8000-0000000000{n:02x}")).expect("a claim id")
}

/// A source id with a distinguishing tail.
pub(crate) fn source_id(n: u8) -> SourceId {
    SourceId::parse(&format!("01931111-0000-7000-8000-0000000000{n:02x}")).expect("a source id")
}

pub(crate) fn digest_of(text: &str) -> Digest {
    // A real digest of some text: the tests need valid ones, not particular ones.
    let body = format!("{text} ^x");
    let found = inklkunn_markup::blocks(&body);
    block_digest(found.first().expect("a block"))
}

/// The digest of a block as a page has it: the text of the block, its `^id` marker included.
pub(crate) fn digest_of_block(block: &str) -> Digest {
    let found = inklkunn_markup::blocks(block);
    block_digest(found.first().expect("a block"))
}

mod acceptance;
mod check;
mod compare;
mod compare_views;
mod compose;
mod curation;
mod digest;
mod health;
mod index;
mod marker;
mod merge;
mod onpage;
mod refusals;
mod roundtrip;
mod status;
mod summary;
mod touched;
