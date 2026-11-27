//! What an import does not bring (RFC 007 D-1, INV-11; the Z-1 and Z-6 corrections of the 0.5.0 readiness review): **a bundle's claim and
//! source files arrive *unsettled***, and a retired one does not arrive.
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
//! not canonical). **A retirement is a curation's too, and it is not taken out of a file: the file is left out whole** (Z-6): a claim or
//! a source that is `retired` is a decision that it no longer stands, and writing it as if it were not would say what the bundle did
//! not say, while refusing the whole import for it would leave no wiki that ever retired a claim able to round-trip. So it is not
//! written, and it is **counted and said** ([`Unsettled::retired_claims`], [`Unsettled::retired_sources`]): *retiring is a curation
//! here*. A file that names one that was left out is a different matter, judged where the files are planned: a reference to a file the
//! wiki will not have is refused by name. A source link's judgement (`relation`, `bearing`, `judged_on`) is not a departure for a claim
//! that is new: the baseline of a file the wiki does not have holds no link, so a claim arrives with its sources and its anchors as
//! they were.

use super::check::Retirement;
use super::{
    Availability, Claim, ClaimId, CommunityStatus, Family, Fault, Source, SourceId, family_of,
};
use inklkunn_signer::message::SUMMARY_MAX_CHARS;

/// How many of each thing only a curation sets an import left out: three keys of the files it wrote, and the retired files it did not
/// write at all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Unsettled {
    /// Claims whose community status was not *not decided* (a status, or the proposal and wording one was decided on).
    pub statuses: usize,
    /// Sources that had a last review.
    pub review_days: usize,
    /// Sources whose availability was not *unknown*.
    pub availabilities: usize,
    /// Claims whose lifecycle is *retired*: **the file was not written** (Z-6), not written without a key.
    pub retired_claims: usize,
    /// Sources whose lifecycle is *retired*: **the file was not written** (Z-6).
    pub retired_sources: usize,
}

impl Unsettled {
    /// Nothing was left out.
    pub fn is_none(&self) -> bool {
        *self == Unsettled::default()
    }

    /// Was a status, a review day or an availability left out of a file that was written? (A retired file is not that: it was not
    /// written, so no file differs from the bundle's by keys.)
    pub fn has_keys_left_out(&self) -> bool {
        self.statuses > 0 || self.review_days > 0 || self.availabilities > 0
    }

    /// These counts and `other`'s together.
    pub fn plus(self, other: Unsettled) -> Unsettled {
        Unsettled {
            statuses: self.statuses.saturating_add(other.statuses),
            review_days: self.review_days.saturating_add(other.review_days),
            availabilities: self.availabilities.saturating_add(other.availabilities),
            retired_claims: self.retired_claims.saturating_add(other.retired_claims),
            retired_sources: self.retired_sources.saturating_add(other.retired_sources),
        }
    }

    /// The sentences the report and the wiki say, one for each kind of key that has any, in this order: statuses, review days,
    /// availabilities; then **one for the retired files** (*N retired claims and M retired sources were not imported: retiring is a
    /// curation here.*, a kind with none not named). English, the words of the report; **the first is the architect's sentence** (*N
    /// community statuses were not imported: a status is set here only by a curation proposal.*).
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
        let retired: Vec<String> = [
            Self::counted(self.retired_claims, "retired claim", "retired claims"),
            Self::counted(self.retired_sources, "retired source", "retired sources"),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !retired.is_empty() {
            // One thing left out in all: *was*. Two kinds, or many of one: *were*.
            let one = self.retired_claims.saturating_add(self.retired_sources) == 1;
            let verb = if one { "was" } else { "were" };
            out.push(format!(
                "{} {verb} not imported: retiring is a curation here.",
                retired.join(" and ")
            ));
        }
        out
    }

    /// `n` and the noun that agrees with it; `None` for none.
    fn counted(n: usize, one: &str, many: &str) -> Option<String> {
        (n > 0).then(|| format!("{n} {}", if n == 1 { one } else { many }))
    }

    /// What the report adds under the sentences of the keys: what a file that had any of them is written as. (Not said when only retired
    /// files were left out: they were not written at all.)
    pub const DIFFERENCE: &'static str = "A claim or source file that had any of them is written without them; its bytes differ from the bundle's by exactly those keys.";

    /// **The summary of the change that writes the files** (the first line of its message: at most 120 characters, one line): the
    /// sentence itself when one sentence says all that was left out, one sentence naming each kind when several were, and the plain count
    /// when none. `files` is how many files the change holds. **A change holds the files of one family** (the sources first, then the
    /// claims: see [`crate::import::bundle::BundlePlan::knowledge_changes`]), so the change of the claims says the statuses and nothing
    /// else, and the sentence the review of the 0.5.0 readiness asked for is the whole summary; the counts of the retired files are added
    /// to the summary of the change of their family (Z-6). **The counts are never cut, the reason is**: a summary that would pass the
    /// message's 120 characters ends after *were not imported*.
    pub fn summary(&self, files: usize) -> String {
        let sentences = self.sentences();
        match sentences.as_slice() {
            [] => format!("Import {files} claim or source file(s)"),
            [one] => one.clone(),
            _ => {
                let mut parts: Vec<String> = [
                    Self::counted(self.statuses, "community status", "community statuses"),
                    Self::counted(self.review_days, "review day", "review days"),
                    Self::counted(self.availabilities, "availability", "availabilities"),
                    Self::counted(self.retired_claims, "retired claim", "retired claims"),
                    Self::counted(self.retired_sources, "retired source", "retired sources"),
                ]
                .into_iter()
                .flatten()
                .collect();
                let last = parts.pop().unwrap_or_default();
                let counts = format!("{} and {last} were not imported", parts.join(", "));
                let full = format!("{counts}: only a curation proposal sets them.");
                if full.chars().count() <= SUMMARY_MAX_CHARS {
                    full
                } else {
                    format!("{counts}.")
                }
            }
        }
    }
}

/// What an import writes of one knowledge file, and what it left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsettling {
    /// The bytes to write **when they are not the bundle's**: `Some` only when something was left out (the canonical text of the file
    /// without it); `None` means the bundle's own bytes are written (or, when [`Unsettling::retired`], that nothing is).
    pub rewritten: Option<Vec<u8>>,
    /// What was left out.
    pub left_out: Unsettled,
    /// **The file is retired and is not written at all** (Z-6): [`Unsettling::left_out`] counts it, and counts nothing else of it (a
    /// retired claim that also had a status is one retired claim, not also a status: it is not there to have one).
    pub retired: bool,
}

/// Take out of the claim or source file at `path`, whose bytes are `bytes`, what only a curation sets (see the module): a retired file
/// is [`Unsettling::retired`], left out whole. A file the wiki cannot read is the format's [`Fault`], as [`super::read_file`] gives it;
/// nothing is half-read.
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
            if Retirement::of_claim(&claim) != Retirement::Active {
                left_out.retired_claims = 1;
                return Ok(Unsettling {
                    rewritten: None,
                    left_out,
                    retired: true,
                });
            }
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
            if Retirement::of_source(&source) != Retirement::Active {
                left_out.retired_sources = 1;
                return Ok(Unsettling {
                    rewritten: None,
                    left_out,
                    retired: true,
                });
            }
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
        retired: false,
    })
}
