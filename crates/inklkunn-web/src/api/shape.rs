//! The JSON shapes of the read API's answers that more than one endpoint gives, and the codes in them.
//!
//! **A shape is decided here, once, and described in the OpenAPI document** (`openapi.rs`); the endpoints only
//! fill it in. The rules every shape follows:
//!
//! * Words for people come from the change's message and nothing else. A change is signed into the history by
//!   the wiki service; the people of a change are *proposed by* and *accepted by*, and no shape has a member
//!   that says a person signed. `people` are the sentences the pages show, in the caller's language, produced
//!   by the very functions the pages use; `provenance` and the person objects are the same facts as codes.
//! * A code is a code: `kind`, `action`, `flags`, `events` are stable words of the API, not catalog messages.
//!   They do not change with the language, and a client builds its own words from them.
//! * Text of an author (a summary, a name, a title, a line of a diff, a snippet) is a JSON string, plain text.
//!   A shape that carries HTML says so (`html`, from the sanitizer) and is the only one that does.

use inklkunn_core::read::{ChangeInfo, HEADING_DEMOTION};
use inklkunn_core::read::{
    ChangeKind, ChangeRow, Did, DiffLine, HistoryChangeKind, HunkView, LineKind, PageEvent,
    PageView, Person, Provenance, SnippetSegment, Subject, Who,
};
use inklkunn_store::WikiRecord;
use serde_json::{Map, Value, json};

use crate::catalog::T;
use crate::history::{Flag, people};

/// A person of the recent changes: an account by the name it had then, an anonymous proposer by pseudonym, the
/// service by its label.
pub(super) fn person(person: &Person) -> Value {
    match person {
        Person::Account(name) => json!({"kind": "account", "name": name}),
        Person::Anonymous(token) => json!({"kind": "anonymous", "name": token}),
        Person::Service(label) => json!({"kind": "service", "name": label}),
    }
}

/// A person of a page's history, as its message names them.
pub(super) fn who(who: &Who) -> Value {
    match who {
        Who::Person { name } => json!({"kind": "account", "name": name}),
        Who::Anonymous { token } => json!({"kind": "anonymous", "name": token}),
        Who::Service => json!({"kind": "service", "name": Value::Null}),
    }
}

const fn history_kind(kind: HistoryChangeKind) -> &'static str {
    match kind {
        HistoryChangeKind::Proposal => "proposal",
        HistoryChangeKind::Rollback => "rollback",
        HistoryChangeKind::Import => "import",
        HistoryChangeKind::System => "system",
    }
}

/// What a change's message says it is and who it names, for a page's history.
pub(super) fn provenance(provenance: &Provenance) -> Value {
    match provenance {
        Provenance::Recorded {
            kind,
            proposed_by,
            accepted_by,
        } => json!({
            "kind": history_kind(*kind),
            "proposed_by": who(proposed_by),
            "accepted_by": who(accepted_by),
        }),
        Provenance::NotRecorded => not_recorded(),
    }
}

/// A message that is not in the service's form: it names nobody, and nobody is assumed.
fn not_recorded() -> Value {
    json!({"kind": "not-recorded", "proposed_by": Value::Null, "accepted_by": Value::Null})
}

/// What a change's message says it is and who it names, for the recent changes.
pub(super) fn row_provenance(row: &ChangeRow) -> Value {
    let kind = match row.kind {
        ChangeKind::Proposal => "proposal",
        ChangeKind::Rollback => "rollback",
        ChangeKind::Import => "import",
        ChangeKind::System => "system",
        ChangeKind::Foreign => return not_recorded(),
    };
    json!({
        "kind": kind,
        "proposed_by": row.proposed_by.as_ref().map(person),
        "accepted_by": row.accepted_by.as_ref().map(person),
    })
}

/// The members every description of a change has, from the history's record of it.
pub(super) fn change_head(t: T, info: &ChangeInfo) -> Map<String, Value> {
    let mut head = Map::new();
    head.insert("id".to_owned(), json!(info.block_id));
    head.insert("number".to_owned(), json!(info.number));
    head.insert("parent".to_owned(), json!(info.parent));
    head.insert("first".to_owned(), json!(info.first));
    head.insert("undoes".to_owned(), json!(info.undoes));
    head.insert("summary".to_owned(), json!(info.summary));
    head.insert("provenance".to_owned(), provenance(&info.provenance));
    head.insert("people".to_owned(), json!(people(t, &info.provenance)));
    head.insert(
        "flags".to_owned(),
        json!(
            Flag::of(info)
                .into_iter()
                .map(Flag::code)
                .collect::<Vec<_>>()
        ),
    );
    head
}

/// What a change did to the page a history is of.
pub(super) fn event(event: &PageEvent) -> Value {
    match event {
        PageEvent::Created => json!({"kind": "created"}),
        PageEvent::Edited => json!({"kind": "edited"}),
        PageEvent::Deleted => json!({"kind": "deleted"}),
        PageEvent::MovedFrom { title } => json!({"kind": "moved-from", "title": title}),
        PageEvent::MovedTo { title } => json!({"kind": "moved-to", "title": title}),
        PageEvent::NotInspected => json!({"kind": "not-inspected"}),
    }
}

fn subject(subject: &Subject) -> Value {
    match subject {
        Subject::Page { title } => json!({"kind": "page", "title": title}),
        Subject::File => json!({"kind": "file", "title": Value::Null}),
    }
}

/// One thing a change did.
pub(super) fn did(did: &Did) -> Value {
    match did {
        Did::Added(what) => json!({"action": "added", "subject": subject(what)}),
        Did::Edited(what) => json!({"action": "edited", "subject": subject(what)}),
        Did::Removed(what) => json!({"action": "removed", "subject": subject(what)}),
        Did::Moved { from, to } => {
            json!({"action": "moved", "from": subject(from), "to": subject(to)})
        }
    }
}

/// The runs of a text, `marked` where it is what matched (a snippet) or what the refinement found different (a
/// line of a diff). Plain text: the client escapes it.
pub(super) fn segments<'a>(runs: impl IntoIterator<Item = (&'a str, bool)>) -> Value {
    Value::Array(
        runs.into_iter()
            .map(|(text, marked)| json!({"text": text, "marked": marked}))
            .collect(),
    )
}

/// A snippet of the search: the text that matched, cut into the matching part and the rest.
pub(super) fn snippet(runs: &[SnippetSegment]) -> Value {
    segments(runs.iter().map(|s| (s.text.as_str(), s.matched)))
}

fn line(line: &DiffLine) -> Value {
    let kind = match line.kind {
        LineKind::Context => "context",
        LineKind::Removed => "removed",
        LineKind::Added => "added",
        LineKind::Marker => "no-newline-marker",
    };
    json!({
        "kind": kind,
        "segments": segments(line.segments.iter().map(|s| (s.text.as_str(), s.marked))),
    })
}

/// One hunk of a diff: as lines, or, when it is not in the shape of a unified hunk, as the text it is.
pub(super) fn hunk(hunk: &HunkView) -> Value {
    match hunk {
        HunkView::Lines(h) => json!({
            "kind": "lines",
            "old_start": h.old_start,
            "new_start": h.new_start,
            "lines": h.lines.iter().map(line).collect::<Vec<_>>(),
        }),
        HunkView::Raw(text) => json!({"kind": "raw", "text": text}),
    }
}

/// A page as the page endpoint answers, and as a permalink does: its information, its table of contents and its
/// sanitized body.
pub(super) fn page(wiki: &WikiRecord, page: &PageView) -> Value {
    let license = page
        .meta
        .license
        .clone()
        .unwrap_or_else(|| wiki.default_license.clone());
    json!({
        "title": page.meta.title,
        "id": page.meta.id,
        "kind": page.meta.kind,
        "status": page.meta.status,
        "lang": page.meta.lang,
        "license": license,
        "description": page.meta.description,
        "tags": page.meta.tags,
        "aliases": page.meta.aliases,
        "toc": page.toc.iter().map(|h| json!({
            // The level the heading has in `html`, where the page's own title is the one `h1`.
            "level": h.level.saturating_add(HEADING_DEMOTION).min(6),
            "id": h.id,
            "text": h.text,
        })).collect::<Vec<_>>(),
        "html": page.html.as_str(),
        "redirected_from": page.redirected_from.as_ref().map(|from| json!({
            "title": from.title,
            "by": match from.by {
                inklkunn_core::read::RedirectedBy::Alias => "alias",
                inklkunn_core::read::RedirectedBy::Redirect => "redirect",
            },
        })),
        "matched_by": page.via.as_str(),
        "stale": page.stale,
        "tip": super::endpoints::tip_json(&page.tip),
    })
}
