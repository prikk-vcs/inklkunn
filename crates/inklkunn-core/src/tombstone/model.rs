//! What a surface is told about a hiding: the marks of the parts that are hidden **now**, and nothing a tombstone must not
//! show (RFC 006 D-8). **There is no person in a [`Mark`]**: the category, the rung and the date, never a name, and never the
//! free-text note (which is for the `moderate` rung, through the acts that authorize it).

use inklkunn_store::{
    HiddenParts, ReasonCategory, Role, TombstoneAct, TombstoneEvent, TombstoneEventId,
};

/// One hiding in force, as every surface shows it in place of what it hides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// The record of the hide: the hook for *contest this* and for *show hidden content*.
    pub event: TombstoneEventId,
    /// The parts this hide covers.
    pub parts: HiddenParts,
    /// Why, in the closed set of six.
    pub category: ReasonCategory,
    /// The rung that hid it.
    pub rung: Role,
    /// When: Unix seconds.
    pub at: i64,
}

impl Mark {
    /// The mark of a hide event; `None` for an unhide, or for a hide that lacks its category (the store refuses one).
    pub fn of(event: &TombstoneEvent) -> Option<Mark> {
        if event.act != TombstoneAct::Hide {
            return None;
        }
        Some(Mark {
            event: event.id.clone(),
            parts: event.parts,
            category: event.category?,
            rung: event.by.rung,
            at: event.at,
        })
    }
}

/// What is hidden of one item now: the hides no unhide has followed, in the order they were made.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Hidden {
    marks: Vec<Mark>,
}

impl Hidden {
    /// Nothing is hidden.
    pub fn none() -> Hidden {
        Hidden::default()
    }

    /// What is hidden of an item, from every event of its chain (oldest first, as the store gives them).
    pub fn of_events(events: &[TombstoneEvent]) -> Hidden {
        let marks = events
            .iter()
            .filter(|hide| hide.act == TombstoneAct::Hide)
            .filter(|hide| {
                !events
                    .iter()
                    .any(|e| e.act == TombstoneAct::Unhide && e.of_event.as_ref() == Some(&hide.id))
            })
            .filter_map(Mark::of)
            .collect();
        Hidden { marks }
    }

    /// What is hidden, from marks already read.
    pub fn from_marks(marks: impl IntoIterator<Item = Mark>) -> Hidden {
        Hidden {
            marks: marks.into_iter().collect(),
        }
    }

    /// The same, with everything `other` hides too.
    #[must_use]
    pub fn with(mut self, other: Hidden) -> Hidden {
        self.marks.extend(other.marks);
        self
    }

    /// Is anything hidden?
    pub fn is_hidden(&self) -> bool {
        !self.marks.is_empty()
    }

    /// Every hide in force.
    pub fn marks(&self) -> &[Mark] {
        &self.marks
    }

    /// Every part hidden.
    pub fn parts(&self) -> HiddenParts {
        self.marks
            .iter()
            .fold(HiddenParts::default(), |all, m| all.union(m.parts))
    }

    /// The newest hide that covers the content, if the content is hidden.
    pub fn content(&self) -> Option<&Mark> {
        self.marks.iter().rev().find(|m| m.parts.content)
    }

    /// The newest hide that covers the summary.
    pub fn summary(&self) -> Option<&Mark> {
        self.marks.iter().rev().find(|m| m.parts.summary)
    }

    /// The newest hide that covers the author.
    pub fn author(&self) -> Option<&Mark> {
        self.marks.iter().rev().find(|m| m.parts.author)
    }
}

/// What is hidden of a proposal wherever the proposal is named: its summary and its author (the content and the pages are
/// the revisions').
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProposalHidden {
    /// The summary of the proposal's latest revision is hidden.
    pub summary: Option<Mark>,
    /// The author of **a** revision is hidden: the proposal names its author nowhere.
    pub author: Option<Mark>,
}

impl ProposalHidden {
    /// Is anything of the proposal hidden?
    pub fn is_hidden(&self) -> bool {
        self.summary.is_some() || self.author.is_some()
    }
}
