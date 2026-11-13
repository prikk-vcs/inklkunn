//! Claims in the proposal form (RFC 005 part 1 D-1 b; part 2 D-3): the paragraph's marker the form holds and puts
//! back, the removal of a paragraph others rely on as an act the person confirms, and a source attached to a
//! paragraph, all in **the same proposal** as the page edit they belong to.
//!
//! The rules are the core's ([`inklkunn_core::knowledge`]): what a text comes to, what a file is. This is the glue
//! between a form and them, and the words for what went wrong (a catalog message each).

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use inklkunn_core::knowledge::compose::{
    Attachment, Mistake, SourceDraft, attach, clean_reference, clean_text, page_id_of,
    paragraph_id, without_expressions,
};
use inklkunn_core::knowledge::marker::{
    held_part, hide_markers, reattach_all, reattach_part, words_with_id,
};
use inklkunn_core::knowledge::onpage::{blocks_said, claim_blocks, claims_at, claims_in};
use inklkunn_core::knowledge::{Claim, claim_path, family_of};
use inklkunn_core::read::ReadError;
use inklkunn_store::FileChange;

use super::Ctx;
use super::form::AttachInput;
use super::parts::{Selection, excerpt};
use crate::catalog::{Key, T};

/// What the form knows about the claims on one page: the `^id`s of the paragraphs that are a claim's.
pub(super) struct Known {
    ids: BTreeSet<String>,
}

/// The claims of the page whose text the form started from: those the view's tip has and those the proposal's own
/// knowledge files (`own`) add.
pub(super) fn known(
    ctx: &Ctx,
    view: Option<&str>,
    started: &str,
    own: &[Claim],
) -> Result<Known, ReadError> {
    let mut ids = claim_blocks(&ctx.services.read, &ctx.wiki, view, started)?;
    if let Some(page_id) = page_id_of(started) {
        ids.extend(blocks_said(own, &page_id));
    }
    Ok(Known { ids })
}

/// How many paragraphs the form holds markers for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Held {
    /// None.
    #[default]
    None,
    /// The one paragraph the form edits.
    One,
    /// Some of the paragraphs among several.
    Many,
}

/// A paragraph that others rely on and that the text no longer has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Lost {
    /// The paragraph's `^id`.
    pub(super) id: String,
    /// How it began.
    pub(super) excerpt: String,
}

/// What the form shows of `slice` (a part, a section, or a whole page with its information block) and whether it
/// holds any markers.
pub(super) fn show(slice: &str, known: &Known, selection: Selection) -> (String, Held) {
    let whole = matches!(selection, Selection::Whole);
    let shown = hide_markers(slice, &known.ids, whole);
    let held = match (shown == slice, selection) {
        (true, _) => Held::None,
        (false, Selection::Block(_)) => Held::One,
        (false, _) => Held::Many,
    };
    (shown, held)
}

/// What a person's text comes to with the markers put back.
pub(super) struct Restored {
    pub(super) text: String,
    pub(super) lost: Vec<Lost>,
}

/// Put the markers back into what a person sent. `started` is the slice the form started from (markers in it).
pub(super) fn restore(
    started: &str,
    posted: &str,
    known: &Known,
    selection: Selection,
) -> Restored {
    let whole = matches!(selection, Selection::Whole);
    let lost_of = |ids: Vec<String>| {
        ids.into_iter()
            .map(|id| Lost {
                excerpt: words_with_id(started, &id, whole)
                    .map(|w| excerpt(&w))
                    .unwrap_or_default(),
                id,
            })
            .collect()
    };
    match selection {
        Selection::Block(_) => match held_part(started, &known.ids) {
            None => Restored {
                text: posted.to_owned(),
                lost: Vec::new(),
            },
            Some(id) => match reattach_part(started, posted, &id) {
                Some(text) => Restored {
                    text,
                    lost: Vec::new(),
                },
                None => Restored {
                    text: posted.to_owned(),
                    lost: lost_of(vec![id]),
                },
            },
        },
        Selection::Section(_) | Selection::Whole => {
            let back = reattach_all(started, posted, &known.ids, whole);
            Restored {
                text: back.text,
                lost: lost_of(back.lost),
            }
        }
    }
}

/// The words for something the core refused to write.
pub(super) fn mistake_text(t: T, m: Mistake) -> String {
    match m {
        Mistake::NoParagraph => t.get(Key::AttachNoParagraph),
        Mistake::PageUnreadable => t.get(Key::AttachPageUnreadable),
        Mistake::SeveralClaims => t.get(Key::AttachSeveralClaims),
        Mistake::Full => t.get(Key::AttachFull),
        Mistake::SourceSaysNothing => t.get(Key::AttachSaysNothing),
        Mistake::TooLong(_) => t
            .msg(Key::AttachTooLong)
            .with("limit", inklkunn_core::knowledge::MAX_TEXT_CHARS)
            .text(),
        Mistake::ControlCharacter(_) => t.get(Key::AttachControl),
        Mistake::Direction(_) => t.get(Key::AttachDirection),
        Mistake::NotAWebAddress => t.get(Key::AttachNotWeb),
        Mistake::NotADay => t.get(Key::CurateNotADay),
        Mistake::UnknownSource => t.get(Key::CurateUnknownSource),
        Mistake::NoWording => t.get(Key::CurateNoWording),
        Mistake::Unwritable => t.get(Key::AttachUnwritable),
    }
}

/// Why a source could not be attached.
pub(super) enum Failure {
    /// The repository could not be read.
    Read(ReadError),
    /// What the person gave, or the paragraph, is not something a claim can be made of.
    Mistake(Mistake),
}

impl From<ReadError> for Failure {
    fn from(e: ReadError) -> Self {
        Failure::Read(e)
    }
}

impl From<Mistake> for Failure {
    fn from(m: Mistake) -> Self {
        Failure::Mistake(m)
    }
}

/// The source a form attached, as the core's draft.
fn attachment(a: &AttachInput) -> Result<Attachment, Mistake> {
    Ok(Attachment {
        source: SourceDraft {
            kind: a.kind,
            title: clean_text(&a.title, "title")?,
            author: clean_text(&a.author, "author")?,
            date: clean_text(&a.date, "date")?,
            reference: clean_reference(a.reference_form, &a.reference)?,
        },
        relation: a.relation,
        bearing: a.bearing,
        note: clean_text(&a.note, "note")?,
        kind: a.claim_kind,
    })
}

/// The knowledge files of one post: those the proposal already carries, what this post changes in the claims, and the
/// sources it records.
pub(super) struct Work<'a> {
    ctx: &'a Ctx,
    view: Option<&'a str>,
    base_block: &'a str,
    /// The proposal's knowledge files before this post (a revision carries them forward), by path.
    carried: BTreeMap<String, Option<String>>,
    /// Claims this post has read or changed, by id.
    claims: BTreeMap<String, Claim>,
    changed: BTreeSet<String>,
    sources: BTreeMap<String, String>,
}

impl<'a> Work<'a> {
    /// A post against the base block `base_block` of `view`, with the knowledge files the proposal already carries.
    pub(super) fn new(
        ctx: &'a Ctx,
        view: Option<&'a str>,
        base_block: &'a str,
        carried: &[FileChange],
    ) -> Self {
        Work {
            ctx,
            view,
            base_block,
            carried: carried
                .iter()
                .filter(|f| family_of(&f.path).is_some())
                .map(|f| (f.path.clone(), f.text.clone()))
                .collect(),
            claims: BTreeMap::new(),
            changed: BTreeSet::new(),
            sources: BTreeMap::new(),
        }
    }

    /// The claims the proposal already makes or changes.
    pub(super) fn own(&self) -> Vec<Claim> {
        let texts: Vec<(String, String)> = self
            .carried
            .iter()
            .filter_map(|(p, t)| t.clone().map(|t| (p.clone(), t)))
            .collect();
        claims_in(&texts)
    }

    /// The claims said at a block of a page, as the proposal has them: the base's, with the proposal's own versions
    /// (carried, then changed in this post) in place of them.
    fn claims_here(&mut self, page_id: &str, block: &str) -> Result<Vec<Claim>, ReadError> {
        let mut found: BTreeMap<String, Claim> = BTreeMap::new();
        for c in claims_at(
            &self.ctx.services.read,
            &self.ctx.wiki,
            self.view,
            self.base_block,
            page_id,
            block,
        )? {
            found.insert(c.id.to_string(), c);
        }
        let says_here = |c: &Claim| {
            c.expressions
                .iter()
                .any(|e| e.page_id == page_id && e.block == block)
        };
        for c in self.own().into_iter().chain(self.claims.values().cloned()) {
            if says_here(&c) {
                found.insert(c.id.to_string(), c);
            } else {
                found.remove(c.id.as_str());
            }
        }
        Ok(found.into_values().collect())
    }

    fn keep(&mut self, claim: Claim) {
        let id = claim.id.to_string();
        self.changed.insert(id.clone());
        self.claims.insert(id, claim);
    }

    /// The paragraph at `block` of the page `page_id` is removed, and said so: every claim said there stops being
    /// said there. The claim stays, said nowhere, for a reviewer to decide about.
    pub(super) fn drop_expression(&mut self, page_id: &str, block: &str) -> Result<(), ReadError> {
        let gone: BTreeSet<String> = [block.to_owned()].into();
        for claim in self.claims_here(page_id, block)? {
            self.keep(without_expressions(&claim, page_id, &gone));
        }
        Ok(())
    }

    /// Attach the source of `what` to the paragraph at `region` of `page` (the page as the proposal leaves it) and
    /// give back the page's new text: the paragraph's marker and the page's id are in it when they were missing.
    pub(super) fn attach(
        &mut self,
        page: &str,
        region: &Range<usize>,
        what: &AttachInput,
    ) -> Result<String, Failure> {
        let what = attachment(what)?;
        let existing = match (page_id_of(page), paragraph_id(page, region)?) {
            (Some(page_id), Some(block)) => {
                let mut here = self.claims_here(&page_id, &block)?;
                match here.len() {
                    0 => None,
                    1 => here.pop(),
                    _ => return Err(Mistake::SeveralClaims.into()),
                }
            }
            _ => None,
        };
        let done = attach(page, region, existing.as_ref(), &what)?;
        let [(claim_path_, claim_text), (source_path_, source_text)] = done.files;
        let claim = inklkunn_core::knowledge::Claim::parse(done.claim, &claim_text)
            .map_err(|_| Mistake::Unwritable)?;
        debug_assert_eq!(claim_path(&claim.id), claim_path_);
        self.keep(claim);
        self.sources.insert(source_path_, source_text);
        Ok(done.page)
    }

    /// Every knowledge file the proposal has now: the ones it carried, with this post's changes over them.
    pub(super) fn files(mut self) -> Vec<FileChange> {
        for id in &self.changed {
            if let Some(claim) = self.claims.get(id) {
                self.carried
                    .insert(claim_path(&claim.id), Some(claim.to_yaml()));
            }
        }
        for (path, text) in self.sources {
            self.carried.insert(path, Some(text));
        }
        self.carried
            .into_iter()
            .map(|(path, text)| FileChange { path, text })
            .collect()
    }
}
