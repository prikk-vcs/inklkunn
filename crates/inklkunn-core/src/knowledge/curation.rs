//! The curation proposal (RFC 005 part 2 D-3, D-10): a proposal of its own, with no page text, that is where a
//! claim's **community status**, a source's **last review** and its **availability** are set.
//!
//! *Curation* is a fact about the proposal's content and nothing else: every file of its revision is a knowledge
//! file, there is at least one, and no page is renamed ([`is_curation`]). There is no flag to set and no field to
//! forge: a proposal that writes a page, however it was made (the form, the write API, a script), is not one.
//! The acceptance asks the question before the signer is asked ([`super::check`]: a change that is not a curation
//! proposal and sets what only curation sets is refused, rule [`super::Rule::Status`]), and names the proposal
//! that decided: [`stamp`] writes the accepting proposal's id into the standing and the review it set, whatever
//! the files said, so the status always carries the decision that made it ([`super::CommunityStatus::proposal`]).
//!
//! Nothing changes a standing by a count, a vote tally or a timer: the only way is a proposal that a person
//! accepts.

use std::path::Path;

use inklkunn_prikk::{Point, Storage, StorageError};
use inklkunn_signer::PreparedFile;
use inklkunn_store::RevisionContent;

use super::{Claim, ClaimId, File, MAX_FILE_BYTES, Source, SourceId, family_of, read_file};

/// Is this revision a curation proposal: knowledge files only (at least one), and no rename?
pub fn is_curation(content: &RevisionContent) -> bool {
    !content.files.is_empty()
        && content.renames.is_empty()
        && content.files.iter().all(|f| family_of(&f.path).is_some())
}

/// The file at `path` at the tip, when the tip has one.
fn at_tip(
    storage: &dyn Storage,
    repo: &Path,
    tip: &str,
    path: &str,
) -> Result<Option<File>, StorageError> {
    match storage.read(
        repo,
        &Point::Block(tip.to_owned()),
        path,
        MAX_FILE_BYTES as u64,
    ) {
        Ok(bytes) => Ok(read_file(path, &bytes).ok()),
        Err(StorageError::Refused { .. }) => Ok(None),
        Err(other) => Err(other),
    }
}

/// Name `proposal` as the decision of what a curation proposal sets, in the claim and source files it writes: a
/// standing that differs from the tip's (its status, or the wording it was judged on) is decided by this proposal;
/// one that does not is left as the tip had it, whatever the file says; and a source's review date likewise. The
/// files are written back canonically. A file that does not read is left for the validator to refuse.
pub fn stamp(
    storage: &dyn Storage,
    repo: &Path,
    tip: &str,
    files: &mut [PreparedFile],
    proposal: &str,
) -> Result<(), StorageError> {
    for file in files.iter_mut() {
        let Some(path) = file.path.to_str().map(str::to_owned) else {
            continue;
        };
        let Some(family) = family_of(&path) else {
            continue;
        };
        let Some(text) = file
            .content
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
        else {
            continue;
        };
        let name = path
            .rsplit('/')
            .next()
            .and_then(|n| n.strip_suffix(".yaml"))
            .unwrap_or("");
        let before = at_tip(storage, repo, tip, &path)?;
        match (family, before) {
            (super::Family::Claim, was) => {
                let Some(id) = ClaimId::parse(name) else {
                    continue;
                };
                let Ok(mut claim) = Claim::parse(id, text) else {
                    continue;
                };
                let (was_status, was_judged, was_proposal) = match &was {
                    Some(File::Claim(c)) => (
                        c.community_status.status,
                        c.community_status.judged_on.clone(),
                        c.community_status.proposal.clone(),
                    ),
                    _ => (super::Standing::Unknown, None, None),
                };
                let status = &mut claim.community_status;
                let decided_now = status.status != was_status || status.judged_on != was_judged;
                // A standing that changes is decided by this proposal **whichever way it changes**: setting it back to
                // *not decided* is a decision too, and the record says which proposal made it (handoff 005
                // corrections K14).
                status.proposal = if decided_now {
                    Some(proposal.to_owned())
                } else {
                    was_proposal
                };
                file.content = Some(claim.to_yaml().into_bytes());
            }
            (super::Family::Source, was) => {
                let Some(id) = SourceId::parse(name) else {
                    continue;
                };
                let Ok(mut source) = Source::parse(id, text) else {
                    continue;
                };
                let was_review = match &was {
                    Some(File::Source(s)) => s.last_reviewed.clone(),
                    _ => None,
                };
                if let Some(review) = &mut source.last_reviewed {
                    let same_day = was_review.as_ref().map(|r| &r.date) == Some(&review.date);
                    review.proposal = if same_day {
                        was_review.and_then(|r| r.proposal)
                    } else {
                        Some(proposal.to_owned())
                    };
                }
                file.content = Some(source.to_yaml().into_bytes());
            }
        }
    }
    Ok(())
}
