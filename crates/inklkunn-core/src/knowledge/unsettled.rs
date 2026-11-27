//! What an import does not bring (RFC 007 D-1, INV-11; the Z-1 correction of the 0.5.0 readiness review): **a bundle's claim and source
//! files arrive *unsettled***.
//!
//! A claim's **community status**, a source's **last review** and its **availability** are set only by an accepted curation proposal
//! ([`super::curation`], [`super::check::Curated`]): an import is the service writing, never a curation, so it may not set them, and
//! it is not made one to carry a wiki's own export back. [`unsettle`] is the one step that takes them out: it reads a knowledge file's
//! bytes and returns what an import writes of it and what it left out, so that the plan (which *counts*), the replay (which *writes*)
//! and the report (which *says*) cannot disagree.
//!
//! **Only what a curation sets is taken out, and nothing is taken out silently.** A file that had none of the three keys is written
//! with the bundle's exact bytes; a file that had any of them is written as the canonical text of the file read ([`Claim::to_yaml`],
//! [`Source::to_yaml`]), so its bytes differ from the bundle's by those keys (and by the canonical form, where the bundle's text was
//! not canonical). **A retirement is not taken out**: a claim or a source that is `retired` is a decision that it no longer stands,
//! and writing it as if it were not would say what the bundle did not say, so it stays what acceptance refuses of an import (the field
//! `lifecycle`, rule `status`). A source link's judgement (`relation`, `bearing`, `judged_on`) is not a departure for a claim that is
//! new: the baseline of a file the wiki does not have holds no link, so a claim arrives with its sources and its anchors as they were.

use super::{
    Availability, Claim, ClaimId, CommunityStatus, Family, Fault, Source, SourceId, family_of,
};

/// How many of each thing only a curation sets an import left out of the files it wrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Unsettled {
    /// Claims whose community status was not *not decided* (a status, or the proposal and wording one was decided on).
    pub statuses: usize,
    /// Sources that had a last review.
    pub review_days: usize,
    /// Sources whose availability was not *unknown*.
    pub availabilities: usize,
}

impl Unsettled {
    /// Nothing was left out.
    pub fn is_none(&self) -> bool {
        *self == Unsettled::default()
    }

    /// These counts and `other`'s together.
    pub fn plus(self, other: Unsettled) -> Unsettled {
        Unsettled {
            statuses: self.statuses.saturating_add(other.statuses),
            review_days: self.review_days.saturating_add(other.review_days),
            availabilities: self.availabilities.saturating_add(other.availabilities),
        }
    }

    /// The sentences the report and the wiki say, one for each kind that has any, in this order: statuses, review days, availabilities.
    /// English, the words of the report; **the first is the architect's sentence** (*N community statuses were not imported: a status
    /// is set here only by a curation proposal.*).
    pub fn sentences(&self) -> Vec<String> {
        let mut out = Vec::new();
        let kinds = [
            (
                self.statuses,
                "community status",
                "community statuses",
                "a status",
            ),
            (
                self.review_days,
                "review day",
                "review days",
                "a review day",
            ),
            (
                self.availabilities,
                "availability",
                "availabilities",
                "an availability",
            ),
        ];
        for (n, one, many, what) in kinds {
            if n == 0 {
                continue;
            }
            let (noun, verb) = if n == 1 { (one, "was") } else { (many, "were") };
            out.push(format!(
                "{n} {noun} {verb} not imported: {what} is set here only by a curation proposal."
            ));
        }
        out
    }

    /// What the report adds under those sentences: what a file that had any of them is written as.
    pub const DIFFERENCE: &'static str = "A claim or source file that had any of them is written without them; its bytes differ from the bundle's by exactly those keys.";

    /// **The summary of the change that writes the files** (the first line of its message: at most 120 characters, one line): the
    /// sentence itself when one kind was left out, one sentence naming each kind when several were, and the plain count when none.
    /// `files` is how many files the change holds. **A change holds the files of one family** (the sources first, then the claims: see
    /// [`crate::import::bundle::BundlePlan::knowledge_changes`]), so the change of the claims says the statuses and nothing else, and the
    /// sentence the review of the 0.5.0 readiness asked for is the whole summary.
    pub fn summary(&self, files: usize) -> String {
        let sentences = self.sentences();
        match sentences.as_slice() {
            [] => format!("Import {files} claim or source file(s)"),
            [one] => one.clone(),
            _ => {
                let part = |n: usize, one: &str, many: &str| {
                    (n > 0).then(|| format!("{n} {}", if n == 1 { one } else { many }))
                };
                let mut parts: Vec<String> = [
                    part(self.statuses, "community status", "community statuses"),
                    part(self.review_days, "review day", "review days"),
                    part(self.availabilities, "availability", "availabilities"),
                ]
                .into_iter()
                .flatten()
                .collect();
                let last = parts.pop().unwrap_or_default();
                format!(
                    "{} and {last} were not imported: only a curation proposal sets them.",
                    parts.join(", ")
                )
            }
        }
    }
}

/// What an import writes of one knowledge file, and what it left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsettling {
    /// The bytes to write **when they are not the bundle's**: `Some` only when something was left out (the canonical text of the file
    /// without it); `None` means the bundle's own bytes are written.
    pub rewritten: Option<Vec<u8>>,
    /// What was left out.
    pub left_out: Unsettled,
}

/// Take out of the claim or source file at `path`, whose bytes are `bytes`, what only a curation sets (see the module). A file the wiki
/// cannot read is the format's [`Fault`], as [`super::read_file`] gives it; nothing is half-read.
pub fn unsettle(path: &str, bytes: &[u8]) -> Result<Unsettling, Fault> {
    // Read by the format's one reader first: the path, the size, the text and every rule of the file.
    super::read_file(path, bytes)?;
    let name = path
        .rsplit('/')
        .next()
        .and_then(|n| n.strip_suffix(".yaml"))
        .unwrap_or("");
    let text = std::str::from_utf8(bytes).map_err(|_| Fault::whole(super::Rule::NotText))?;
    let mut left_out = Unsettled::default();
    let rewritten = match family_of(path) {
        Some(Family::Claim) => {
            let id = ClaimId::parse(name).ok_or(Fault::whole(super::Rule::Path))?;
            let mut claim = Claim::parse(id, text)?;
            if claim.community_status == CommunityStatus::undecided() {
                None
            } else {
                claim.community_status = CommunityStatus::undecided();
                left_out.statuses = 1;
                Some(claim.to_yaml().into_bytes())
            }
        }
        Some(Family::Source) => {
            let id = SourceId::parse(name).ok_or(Fault::whole(super::Rule::Path))?;
            let mut source = Source::parse(id, text)?;
            if source.last_reviewed.is_some() {
                source.last_reviewed = None;
                left_out.review_days = 1;
            }
            if source.availability != Availability::Unknown {
                source.availability = Availability::Unknown;
                left_out.availabilities = 1;
            }
            (!left_out.is_none()).then(|| source.to_yaml().into_bytes())
        }
        None => return Err(Fault::whole(super::Rule::Path)),
    };
    Ok(Unsettling {
        rewritten,
        left_out,
    })
}
