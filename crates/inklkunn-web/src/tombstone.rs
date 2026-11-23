//! What the pages say of what moderation hid, and the acts that hide (RFC 006 D-8, D-12; handoff G-T4; threat model C-14f,
//! C-15d, C-15f, INV-13).
//!
//! **A surface that shows a revision's text, summary or author, or a comment's body or author, shows [`facts`] in place of what
//! is hidden.** The core never gives the hidden part to a surface (its summary is empty, its people are *hidden*, its text is
//! not read); the words here are the one place the tombstone is spelled: what is hidden, the reason category, the role and the
//! date, **never a name** and never the note. The list of the surfaces is `inklkunn_core::tombstone::surfaces`.
//!
//! - [`form`]: the form of the `moderate` rung that hides, the act that restores, and the act that shows hidden content.
//!   **Web only** (RFC 006 D-12): there is no write API for moderation.
//! - [`log`]: the wiki's moderation log: the item, the category, the role and the date, to whoever may read the wiki.
//! - [`paths`]: the addresses.

pub(crate) mod form;
pub(crate) mod log;
pub(crate) mod paths;

use inklkunn_core::tombstone::{Hidden, Mark};
use inklkunn_store::{HiddenParts, ReasonCategory};

use crate::catalog::{Key, T};
use crate::protections::rung_word;
use crate::time::utc_minutes;

/// The reason category, in the reader's language.
pub(crate) fn category_word(t: T, category: ReasonCategory) -> String {
    t.get(match category {
        ReasonCategory::PersonalData => Key::TombstoneCatPersonalData,
        ReasonCategory::Harassment => Key::TombstoneCatHarassment,
        ReasonCategory::Unlawful => Key::TombstoneCatUnlawful,
        ReasonCategory::Copyright => Key::TombstoneCatCopyright,
        ReasonCategory::Spam => Key::TombstoneCatSpam,
        ReasonCategory::Other => Key::TombstoneCatOther,
    })
}

/// The names of the parts a hiding covers, one each, in the order text, summary, author.
pub(crate) fn part_words(t: T, parts: HiddenParts) -> Vec<String> {
    let mut words = Vec::new();
    if parts.content {
        words.push(t.get(Key::TombstonePartContent));
    }
    if parts.summary {
        words.push(t.get(Key::TombstonePartSummary));
    }
    if parts.author {
        words.push(t.get(Key::TombstonePartAuthor));
    }
    words
}

/// The lines of a hiding in force: one for each part it covers, each saying what, why, which role and when.
pub(crate) fn mark_facts(t: T, mark: &Mark) -> Vec<String> {
    part_words(t, mark.parts)
        .into_iter()
        .map(|part| {
            t.msg(Key::TombstoneFact)
                .with("part", part)
                .with("category", category_word(t, mark.category))
                .with("role", rung_word(t, mark.rung))
                .with("date", utc_minutes(mark.at))
                .text()
        })
        .collect()
}

/// The lines for everything hidden of an item: **the tombstone, in place of what it hides.**
pub(crate) fn facts(t: T, hidden: &Hidden) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in hidden.marks().iter().flat_map(|mark| mark_facts(t, mark)) {
        // A change's text is recorded once for each page it wrote: the same line is said once.
        if !lines.contains(&line) {
            lines.push(line);
        }
    }
    lines
}

/// In place of the name of a person whose authorship is hidden.
pub(crate) fn author_word(t: T) -> String {
    t.get(Key::TombstoneAuthorHidden)
}

/// What a comment is shown as, given what moderation hides of it: whether its body is hidden, and the lines of the tombstone.
pub(crate) fn comment_state(t: T, hidden: Option<&Hidden>) -> (bool, Vec<String>) {
    match hidden {
        None => (false, Vec::new()),
        Some(hidden) => (hidden.content().is_some(), facts(t, hidden)),
    }
}

/// The name of a comment's author, or the word that stands for a hidden one.
pub(crate) fn author_of(t: T, name: impl FnOnce() -> String, hidden: Option<&Hidden>) -> String {
    if hidden.is_some_and(|h| h.author().is_some()) {
        author_word(t)
    } else {
        name()
    }
}
