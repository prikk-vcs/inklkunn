//! Lint: suggestions, never blockers (AUTH-13), each naming the guideline it comes from so a reader can
//! look the reason up. The rule set is the portable subset of the wiki-commons reference linter that
//! needs only the page (and, when given, the index); bundle-level rules belong to a later RFC.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::Page;
use crate::frontmatter::Value;
use crate::render::SHARED_CALLOUTS;
use crate::resolve::{Index, Via};
use crate::scan::{Envelope, FragmentKind};

/// How much a finding matters. None of them blocks a save.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Worth knowing.
    Info,
    /// Worth doing.
    Suggestion,
    /// Likely a mistake.
    Warning,
}

/// One finding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Severity.
    pub level: Level,
    /// A stable code such as `MKUP-9-duplicate`.
    pub code: &'static str,
    /// The guideline id, such as `MKUP-9`.
    pub guideline: &'static str,
    /// What and why, in one sentence.
    pub message: String,
    /// 1-based file line, when the finding has one.
    pub line: Option<usize>,
}

/// What the linter may know beyond the page.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context<'a> {
    /// The page index, for dangling and ambiguous links.
    pub index: Option<&'a Index>,
    /// The page's own path, for nearest-first resolution.
    pub from_path: Option<&'a str>,
}

const RECOMMENDED_STATUS: &[&str] = &["draft", "wip", "stable", "deprecated", "archived"];
const VISIBILITY: &[&str] = &["public", "internal", "restricted"];
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "api_key",
    "apikey",
    "private_key",
];

/// Lint a parsed page. `source` is the file text (for the MKUP-19 text checks).
pub fn lint(page: &Page, source: &str, ctx: Context<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut add = |level: Level,
                   code: &'static str,
                   guideline: &'static str,
                   message: String,
                   line: Option<usize>| {
        out.push(Finding {
            level,
            code,
            guideline,
            message,
            line,
        });
    };

    // MKUP-19 text conventions.
    if source.starts_with('\u{feff}') {
        add(
            Level::Warning,
            "MKUP-19-bom",
            "MKUP-19",
            "file starts with a byte-order mark".into(),
            Some(1),
        );
    }
    if source.contains("\r\n") {
        add(
            Level::Suggestion,
            "MKUP-19-crlf",
            "MKUP-19",
            "CRLF line endings; LF is recommended for exported files".into(),
            None,
        );
    }
    if !source.is_empty() && !source.ends_with('\n') {
        add(
            Level::Suggestion,
            "MKUP-19-newline",
            "MKUP-19",
            "no trailing newline".into(),
            None,
        );
    }
    if let Some(line) = first_tab_outside_code(&page.body) {
        add(
            Level::Suggestion,
            "MKUP-19-tabs",
            "MKUP-19",
            "tab characters outside code".into(),
            Some(line + page.body_start_line - 1),
        );
    }

    // Frontmatter (chapter 09).
    match &page.frontmatter {
        None => add(
            Level::Suggestion,
            "META-1-missing",
            "META-1",
            "no frontmatter; a `title` block makes the page portable".into(),
            Some(1),
        ),
        Some(fm) => {
            if page.metadata.title.is_none() {
                add(
                    Level::Suggestion,
                    "META-2-title",
                    "META-2",
                    "frontmatter has no `title`".into(),
                    Some(1),
                );
            }
            for key in ["created", "updated"] {
                if let Some(Value::Date(d)) = fm.get(key) {
                    add(
                        Level::Warning,
                        "META-3-coerced",
                        "META-3",
                        format!(
                            "`{key}: {d}` is unquoted and is read by YAML as a date object; quote it"
                        ),
                        Some(1),
                    );
                }
            }
            if matches!(fm.get("tags"), Some(Value::Str(_))) {
                add(
                    Level::Suggestion,
                    "META-5-list",
                    "META-5",
                    "`tags` should be a list of strings".into(),
                    Some(1),
                );
            }
            if matches!(fm.get("aliases"), Some(Value::Str(_))) {
                add(
                    Level::Suggestion,
                    "META-4-list",
                    "META-4",
                    "`aliases` should be a list of strings".into(),
                    Some(1),
                );
            }
            if !RECOMMENDED_STATUS.contains(&page.metadata.status.as_str()) {
                add(
                    Level::Info,
                    "META-6-status",
                    "META-6",
                    format!(
                        "`status: {}` is not one of the recommended values; importers will treat it as stable",
                        page.metadata.status
                    ),
                    Some(1),
                );
            }
            if let Some(v) = &page.metadata.visibility
                && !VISIBILITY.contains(&v.as_str())
            {
                add(
                    Level::Warning,
                    "META-13-visibility",
                    "META-13",
                    format!("`visibility: {v}` is not public, internal, or restricted"),
                    Some(1),
                );
            }
            if page.metadata.kind == "redirect" && page.metadata.redirect.is_none() {
                add(
                    Level::Warning,
                    "META-7-redirect",
                    "META-7",
                    "kind is redirect but there is no `redirect` target".into(),
                    Some(1),
                );
            }
            if let Value::Map(entries) = fm {
                for (k, _) in entries {
                    let lk = k.to_ascii_lowercase();
                    if SECRET_KEYS.iter().any(|s| lk.contains(s)) {
                        add(
                            Level::Warning,
                            "META-14-secret",
                            "META-14",
                            format!(
                                "frontmatter key `{k}` looks like a credential; metadata is content"
                            ),
                            Some(1),
                        );
                    }
                }
            }
        }
    }

    let scan = &page.scan;

    // Block identifiers (MKUP-9).
    let mut seen_ids: BTreeMap<&str, usize> = BTreeMap::new();
    for b in &scan.block_ids {
        if let Some(first) = seen_ids.get(b.id.as_str()) {
            add(
                Level::Warning,
                "MKUP-9-duplicate",
                "MKUP-9",
                format!(
                    "block identifier ^{} appears more than once (first at line {first})",
                    b.id
                ),
                Some(b.line),
            );
        } else {
            seen_ids.insert(&b.id, b.line);
        }
    }

    // Envelopes (chapter 12).
    for e in &scan.envelopes {
        if let Envelope::Unclosed(u) = e {
            add(
                Level::Warning,
                "EXT-4-unclosed",
                "EXT-4",
                format!(
                    "snapshot envelope `{}` is never closed",
                    u.name.as_deref().unwrap_or("")
                ),
                Some(u.line),
            );
        }
    }

    // Callouts (MKUP-15).
    for c in &scan.callouts {
        if !SHARED_CALLOUTS.contains(&c.kind.as_str()) {
            add(
                Level::Info,
                "MKUP-15-callout",
                "MKUP-15",
                format!(
                    "callout type `{}` is not one of the five shared types; renderers will show a generic callout",
                    c.kind
                ),
                Some(c.line),
            );
        }
    }
    for d in scan.directives.iter().filter(|d| !d.inline) {
        let l = d.name.to_ascii_lowercase();
        if SHARED_CALLOUTS.iter().any(|s| s.eq_ignore_ascii_case(&l)) {
            add(
                Level::Suggestion,
                "MKUP-15-container",
                "MKUP-15",
                format!(
                    "`:::{l}` container; the shared callout form is `> [!{}]`",
                    l.to_uppercase()
                ),
                Some(d.line),
            );
        }
    }

    // Headings (MKUP-8, A11Y-3).
    if let Some(first) = scan.headings.first() {
        if first.level == 1
            && let Some(title) = &page.metadata.title
            && first.text != *title
        {
            add(
                Level::Suggestion,
                "MKUP-8-h1",
                "MKUP-8",
                "a level-1 heading that differs from the frontmatter title; body headings should begin at level 2"
                    .into(),
                Some(first.line),
            );
        }
        let mut prev = first.level;
        for h in scan.headings.iter().skip(1) {
            if h.level > prev + 1 {
                add(
                    Level::Suggestion,
                    "A11Y-3-heading-skip",
                    "A11Y-3",
                    format!("heading level jumps from {prev} to {}", h.level),
                    Some(h.line),
                );
            }
            prev = h.level;
        }
    }
    let mut heading_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for h in &scan.headings {
        *heading_counts.entry(h.text.as_str()).or_insert(0) += 1;
    }
    let mut reported: BTreeSet<&str> = BTreeSet::new();
    for h in &scan.headings {
        if heading_counts.get(h.text.as_str()).copied().unwrap_or(0) > 1
            && reported.insert(h.text.as_str())
        {
            add(
                Level::Info,
                "MKUP-8-duplicate-heading",
                "MKUP-8",
                format!(
                    "heading `{}` appears more than once; anchors are disambiguated with -1, -2",
                    h.text
                ),
                None,
            );
        }
    }

    // Tags (MKUP-14).
    let declared: BTreeSet<&str> = scan.tags.frontmatter.iter().map(String::as_str).collect();
    let mut missing: Vec<&str> = Vec::new();
    let mut missing_set: BTreeSet<&str> = BTreeSet::new();
    for t in &scan.tags.inline {
        if !declared.contains(t.as_str()) && missing_set.insert(t.as_str()) {
            missing.push(t);
        }
    }
    if !missing.is_empty() {
        add(
            Level::Suggestion,
            "MKUP-14-tags",
            "MKUP-14",
            format!(
                "inline tags not listed in frontmatter `tags`: {}",
                missing.join(", ")
            ),
            None,
        );
    }

    // Links (MKUP-4, MKUP-5, MKUP-6, MKUP-9, MKUP-13, NAV-3).
    for l in &scan.links {
        if l.raw.ends_with("|]]") {
            add(
                Level::Suggestion,
                "MKUP-4-empty-label",
                "MKUP-4",
                format!("`{}` has an empty label", l.raw),
                Some(l.line),
            );
        }
        if l.target.is_empty() && l.fragment.is_none() {
            add(
                Level::Warning,
                "MKUP-4-empty",
                "MKUP-4",
                format!("`{}` has no target", l.raw),
                Some(l.line),
            );
        }
        if l.fragment_kind == Some(FragmentKind::Block)
            && l.target.is_empty()
            && let Some(f) = &l.fragment
            && !seen_ids.contains_key(f.as_str())
        {
            add(
                Level::Warning,
                "MKUP-9-missing-block",
                "MKUP-9",
                format!("same-page block reference ^{f} does not exist"),
                Some(l.line),
            );
        }
        if let Some(index) = ctx.index
            && !l.target.is_empty()
        {
            let r = index.resolve(&l.target, ctx.from_path);
            match r.via {
                Via::Dangling => {
                    if let Some(prefix) = &l.prefix
                        && !index.interwiki.contains_key(prefix)
                        && !index.namespaces.contains(prefix)
                    {
                        add(
                            Level::Info,
                            "MKUP-13-prefix",
                            "MKUP-13",
                            format!(
                                "`{}` has an undeclared prefix `{prefix}`; declare it in the manifest or it is read as a title",
                                l.raw
                            ),
                            Some(l.line),
                        );
                    } else if let Some(label) = &l.label
                        && index.resolve(label, ctx.from_path).via != Via::Dangling
                    {
                        add(
                            Level::Suggestion,
                            "MKUP-6-order",
                            "MKUP-6",
                            format!(
                                "`{}`: the label resolves and the target does not; the source may be label-first",
                                l.raw
                            ),
                            Some(l.line),
                        );
                    } else {
                        add(
                            Level::Info,
                            "NAV-3-dangling",
                            "NAV-3",
                            format!(
                                "`{}` points at a page that does not exist (an invitation, not an error)",
                                l.raw
                            ),
                            Some(l.line),
                        );
                    }
                }
                Via::Ambiguous => add(
                    Level::Warning,
                    "MKUP-5-ambiguous",
                    "MKUP-5",
                    format!(
                        "`{}` matches several pages: {}",
                        l.raw,
                        r.candidates.join(", ")
                    ),
                    Some(l.line),
                ),
                _ => {}
            }
        }
    }

    out
}

/// The 1-based body line of the first tab outside fenced code, if any.
fn first_tab_outside_code(body: &str) -> Option<usize> {
    let mut fence: Option<char> = None;
    for (i, line) in body.lines().enumerate() {
        let t = line.trim_start();
        let marker = t.chars().next().filter(|c| *c == '`' || *c == '~');
        let is_fence = marker.is_some_and(|c| t.chars().take_while(|x| *x == c).count() >= 3);
        match fence {
            Some(c) => {
                if is_fence && marker == Some(c) {
                    fence = None;
                }
            }
            None => {
                if is_fence {
                    fence = marker;
                } else if line.contains('\t') {
                    return Some(i + 1);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;
