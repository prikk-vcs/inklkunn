//! The derived knowledge index of a tip (RFC 005 part 2, D-6; handoff K-T4).
//!
//! **Never a second source of truth.** The claim and source files are; this is what is read out of them at one tip,
//! so that *which claims rely on this source*, *where is this claim said* and *was this relation judged on the
//! wording the claim has now* are answered without reading the files again. It is built from **the same
//! `extract` of the tip** that builds the title index (the read service does both in one pass over one extracted
//! tree), stored in the derived store (schema version 13; 17 adds the sources' names) and dropped and rebuilt with the rest of it.
//!
//! What is in it:
//!
//! - the claims and sources that **can be read** (a file that cannot is listed apart, with the rule it broke, and
//!   nothing is read from it: a claim is never half-read);
//! - the relations between claims, and the links from claims to sources;
//! - the expressions: where each claim is said, with what the block is now (`present` and the digest of its text,
//!   or why not);
//! - and, per link and for the standing, whether it was **judged on the wording the claim has now**: `current` when
//!   the digest it was judged on is the digest of one of the claim's expressions as the page has it now, else
//!   `earlier`. The index states the fact; it never says which wording is right.
//!
//! **Bounded like the title index.** The walk counts the files and directories it visits against the same bound as
//! the pages' (`index_max_files`); the extract itself is bounded in bytes by `INKLKUNN_INDEX_MAX_MIB`; a file is
//! read to at most [`MAX_FILE_BYTES`](super::MAX_FILE_BYTES) and one at a time, and a page is read once, to at
//! most the page bound.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use inklkunn_store::{
    IndexEntry, KnowledgeAnchorRow, KnowledgeClaimRow, KnowledgeInvalidRow, KnowledgeLinkRow,
    KnowledgeRelationRow, KnowledgeRows, KnowledgeSourceRow,
};

use super::tree::{TreeError, list_tree, read_head};
use super::{AnchorState, Claim, Digest, File, Rule, Source, anchor_state, read_file};

/// The wording a relation or a standing was judged on still reads as it does.
pub const CURRENT: &str = "current";
/// It was judged on an earlier wording of the claim.
pub const EARLIER: &str = "earlier";
/// There is nothing it was judged on (a standing that was never decided).
pub const NOT_JUDGED: &str = "none";

/// The block is in the page, once.
pub const PRESENT: &str = "present";
/// No page has the id.
pub const PAGE_MISSING: &str = "page-missing";
/// More than one page has the id.
pub const PAGE_REPEATED: &str = "page-repeated";
/// The page has no block with that `^id`.
pub const BLOCK_MISSING: &str = "block-missing";
/// The page has the `^id` on more than one block.
pub const BLOCK_REPEATED: &str = "block-repeated";

/// Whether a relation (or a standing) was judged on the wording its claim has now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judgement {
    /// Judged on the wording as it is.
    Current,
    /// Judged on an earlier wording: shown so, until a reviewer confirms it.
    Earlier,
    /// Nothing was judged.
    NotJudged,
}

impl Judgement {
    /// The judgement a stored code names.
    pub fn from_code(code: &str) -> Option<Judgement> {
        match code {
            CURRENT => Some(Judgement::Current),
            EARLIER => Some(Judgement::Earlier),
            NOT_JUDGED => Some(Judgement::NotJudged),
            _ => None,
        }
    }
}

/// The knowledge index of one tip.
#[derive(Debug, Clone)]
pub struct KnowledgeIndex {
    tip: String,
    rows: KnowledgeRows,
    claims: HashMap<String, usize>,
    sources: HashMap<String, usize>,
}

impl KnowledgeIndex {
    /// The index of these rows at the tip `tip`.
    pub fn new(tip: impl Into<String>, rows: KnowledgeRows) -> Self {
        let rows = rows.sorted();
        let claims = rows
            .claims
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), i))
            .collect();
        let sources = rows
            .sources
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();
        KnowledgeIndex {
            tip: tip.into(),
            rows,
            claims,
            sources,
        }
    }

    /// The tip's state id.
    pub fn tip(&self) -> &str {
        &self.tip
    }

    /// The rows as the store keeps them.
    pub fn rows(&self) -> &KnowledgeRows {
        &self.rows
    }

    /// Every readable claim, by id.
    pub fn claims(&self) -> &[KnowledgeClaimRow] {
        &self.rows.claims
    }

    /// Every readable source, by id.
    pub fn sources(&self) -> &[KnowledgeSourceRow] {
        &self.rows.sources
    }

    /// The claim with this id.
    pub fn claim(&self, id: &str) -> Option<&KnowledgeClaimRow> {
        self.claims.get(id).and_then(|i| self.rows.claims.get(*i))
    }

    /// The source with this id.
    pub fn source(&self, id: &str) -> Option<&KnowledgeSourceRow> {
        self.sources.get(id).and_then(|i| self.rows.sources.get(*i))
    }

    /// The claim-to-claim relations of a claim.
    pub fn relations_of<'a>(
        &'a self,
        claim: &'a str,
    ) -> impl Iterator<Item = &'a KnowledgeRelationRow> {
        self.rows.relations.iter().filter(move |r| r.claim == claim)
    }

    /// The relations of a claim to its sources.
    pub fn links_of<'a>(&'a self, claim: &'a str) -> impl Iterator<Item = &'a KnowledgeLinkRow> {
        self.rows.links.iter().filter(move |l| l.claim == claim)
    }

    /// The claims that rely on a source (cite it, supporting or challenging), by claim id.
    pub fn claims_citing<'a>(
        &'a self,
        source: &'a str,
    ) -> impl Iterator<Item = &'a KnowledgeLinkRow> {
        self.rows.links.iter().filter(move |l| l.source == source)
    }

    /// Where a claim is said.
    pub fn anchors_of<'a>(
        &'a self,
        claim: &'a str,
    ) -> impl Iterator<Item = &'a KnowledgeAnchorRow> {
        self.rows.anchors.iter().filter(move |a| a.claim == claim)
    }

    /// The claims said at a block of a page (by the page's id and the block's `^id`).
    pub fn claims_at<'a>(
        &'a self,
        page_id: &'a str,
        block: &'a str,
    ) -> impl Iterator<Item = &'a KnowledgeAnchorRow> {
        self.rows
            .anchors
            .iter()
            .filter(move |a| a.page_id == page_id && a.block == block)
    }

    /// The claims said in a page, at blocks the page really has (once): where a reader of the page is offered the
    /// claim. An anchor whose page or block is missing or repeated is not among them, because there is no
    /// paragraph to offer it on; the index still states it ([`KnowledgeIndex::anchors_of`]).
    pub fn claims_on_page<'a>(
        &'a self,
        page_id: &'a str,
    ) -> impl Iterator<Item = &'a KnowledgeAnchorRow> {
        self.rows
            .anchors
            .iter()
            .filter(move |a| a.page_id == page_id && a.state == PRESENT)
    }

    /// The files under `knowledge/` that cannot be read, by path, each with the rule it broke.
    pub fn invalid(&self) -> &[KnowledgeInvalidRow] {
        &self.rows.invalid
    }
}

/// Read the knowledge index out of an extracted tree. `pages` are the title index's entries of the same tree (the
/// pages a claim's expression can name, by their ids); `max_entries` is the bound on the walk and `page_max_bytes`
/// on a page read.
pub fn build(
    root: &Path,
    pages: &[IndexEntry],
    max_entries: u64,
    page_max_bytes: u64,
) -> Result<KnowledgeRows, TreeError> {
    let mut rows = KnowledgeRows::default();
    let mut claims: Vec<Claim> = Vec::new();
    // One file at a time: only what is read out of it is kept.
    for (path, file) in list_tree(root, max_entries)? {
        let bytes = read_head(&file).map_err(TreeError::Io)?;
        match read_file(&path, &bytes) {
            Ok(File::Claim(claim)) => claims.push(*claim),
            Ok(File::Source(source)) => rows.sources.push(source_row(&source)),
            Err(fault) => rows.invalid.push(KnowledgeInvalidRow {
                path,
                rule: fault.rule.code().to_owned(),
            }),
        }
    }

    // The pages a claim is said in, by id.
    let mut by_id: BTreeMap<&str, Vec<&IndexEntry>> = BTreeMap::new();
    for page in pages.iter().filter(|p| p.problem.is_none()) {
        if let Some(id) = &page.page_id {
            by_id.entry(id.as_str()).or_default().push(page);
        }
    }
    let mut texts: HashMap<String, Option<String>> = HashMap::new();
    let mut read_page = |path: &str| -> Option<String> {
        texts
            .entry(path.to_owned())
            .or_insert_with(|| read_bounded(&root.join(path), page_max_bytes))
            .clone()
    };

    for claim in claims {
        let mut now: Vec<Digest> = Vec::new();
        for e in &claim.expressions {
            let (state, path, digest) = match by_id.get(e.page_id.as_str()).map(Vec::as_slice) {
                None | Some([]) => (PAGE_MISSING, None, None),
                Some([one]) => match read_page(&one.path) {
                    None => (PAGE_MISSING, None, None),
                    Some(text) => match anchor_state(&text, &e.block) {
                        AnchorState::Present(d) => (PRESENT, Some(one.path.clone()), Some(d)),
                        AnchorState::BlockMissing => (BLOCK_MISSING, Some(one.path.clone()), None),
                        AnchorState::BlockRepeated => {
                            (BLOCK_REPEATED, Some(one.path.clone()), None)
                        }
                    },
                },
                Some(_) => (PAGE_REPEATED, None, None),
            };
            if let Some(d) = &digest {
                now.push(d.clone());
            }
            rows.anchors.push(KnowledgeAnchorRow {
                claim: claim.id.to_string(),
                page_id: e.page_id.clone(),
                block: e.block.clone(),
                page_path: path,
                state: state.to_owned(),
                digest: digest.map(|d| d.to_string()),
            });
        }
        let judged = |on: &Digest| if now.contains(on) { CURRENT } else { EARLIER };
        for r in &claim.relations {
            rows.relations.push(KnowledgeRelationRow {
                claim: claim.id.to_string(),
                kind: r.kind.word().to_owned(),
                other: r.claim.to_string(),
            });
        }
        for l in &claim.sources {
            rows.links.push(KnowledgeLinkRow {
                claim: claim.id.to_string(),
                source: l.source.to_string(),
                relation: l.relation.word().to_owned(),
                bearing: l.bearing.word().to_owned(),
                judgement: judged(&l.judged_on).to_owned(),
            });
        }
        rows.claims.push(KnowledgeClaimRow {
            id: claim.id.to_string(),
            kind: claim.kind.word().to_owned(),
            lifecycle: claim.lifecycle.word().to_owned(),
            standing: claim.community_status.status.word().to_owned(),
            standing_proposal: claim.community_status.proposal.clone(),
            standing_judgement: claim
                .community_status
                .judged_on
                .as_ref()
                .map_or(NOT_JUDGED, judged)
                .to_owned(),
        });
    }
    Ok(rows.sorted())
}

/// The most characters of a source's title, author, date or reference the index keeps.
pub const MAX_SOURCE_NAME_CHARS: usize = 300;

/// `text` as it was written, cut to [`MAX_SOURCE_NAME_CHARS`] characters (on a character boundary, never inside a
/// code point) with an ellipsis where it was cut, so that a reader can see that it was.
fn kept(text: &str) -> String {
    if text.chars().count() <= MAX_SOURCE_NAME_CHARS {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(MAX_SOURCE_NAME_CHARS - 1).collect();
    cut.push('…');
    cut
}

fn source_row(source: &Source) -> KnowledgeSourceRow {
    KnowledgeSourceRow {
        title: source.identity.title.as_deref().map(kept),
        author: source.identity.author.as_deref().map(kept),
        date: source.identity.date.as_deref().map(kept),
        reference: source.reference.as_ref().map(|r| kept(&r.value)),
        id: source.id.to_string(),
        kind: source.kind.word().to_owned(),
        availability: source.availability.word().to_owned(),
        last_reviewed: source.last_reviewed.as_ref().map(|r| r.date.clone()),
        superseded_by: source.superseded_by.as_ref().map(ToString::to_string),
    }
}

/// A page's text if it can be read under the bound and is UTF-8.
fn read_bounded(path: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    String::from_utf8(std::fs::read(path).ok()?).ok()
}

/// The rule a stored code names, for a surface that words it.
pub fn rule_of(code: &str) -> Option<Rule> {
    Rule::from_code(code)
}
