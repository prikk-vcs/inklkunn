//! `tests/literals.allow`: the allowlist of literal text, its parser and its dead-line check.

use std::cell::RefCell;
use std::collections::BTreeSet;

use super::{file_name, files_under, read, root};

/// `tests/literals.allow`: kind :: file :: literal :: reason. An entry nothing matched fails the test.
pub(crate) struct Allow {
    used: RefCell<BTreeSet<String>>,
    scope: Vec<(String, String)>,
    exact: Vec<(String, String, String)>,
    prefix: Vec<(String, String, String)>,
    safe: Vec<(String, String, String)>,
}

pub(crate) fn parse_allowlist(text: &str) -> Allow {
    let mut a = Allow {
        used: RefCell::default(),
        scope: Vec::new(),
        exact: Vec::new(),
        prefix: Vec::new(),
        safe: Vec::new(),
    };
    for line in text.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(4, " :: ").collect();
        let [kind, file, literal, reason] = parts.as_slice() else {
            panic!("allowlist line needs kind :: file :: literal :: reason: {line}");
        };
        assert!(
            !reason.trim().is_empty(),
            "allowlist line without a reason: {line}"
        );
        let entry = (file.to_string(), literal.to_string(), reason.to_string());
        match *kind {
            "scope" => a.scope.push((entry.0, entry.2)),
            "exact" => a.exact.push(entry),
            "prefix" => a.prefix.push(entry),
            "safe" => a.safe.push(entry),
            other => panic!("unknown allowlist kind {other}"),
        }
    }
    a
}

pub(crate) fn allowlist() -> Allow {
    parse_allowlist(&read(&root().join("tests/literals.allow")))
}

/// An allowlist that excuses nothing, for the planted cases of the scanners.
pub(crate) fn none() -> Allow {
    parse_allowlist("")
}

impl Allow {
    pub(crate) fn mark(&self, key: String) {
        self.used.borrow_mut().insert(key);
    }

    pub(crate) fn scoped(&self, file: &str) -> bool {
        let hit = self.scope.iter().any(|(f, _)| f == file);
        if hit {
            self.mark(format!("scope::{file}"));
        }
        hit
    }

    pub(crate) fn literal_ok(&self, file: &str, lit: &str) -> bool {
        let lit = lit.trim();
        if self
            .exact
            .iter()
            .any(|(f, l, _)| (f == "*" || f == file) && l == lit)
        {
            self.mark(format!("exact::{file}::{lit}"));
            self.mark(format!("exact::*::{lit}"));
            return true;
        }
        if let Some((_, p, _)) = self
            .prefix
            .iter()
            .find(|(f, p, _)| (f == "*" || f == file) && lit.starts_with(p.as_str()))
        {
            self.mark(format!("prefix::{p}"));
            return true;
        }
        false
    }

    pub(crate) fn safe_ok(&self, file: &str, expr: &str) -> bool {
        let hit = self.safe.iter().any(|(f, e, _)| f == file && e == expr);
        if hit {
            self.mark(format!("safe::{file}::{expr}"));
        }
        hit
    }

    /// Entries nothing matched: an allowlist that only grows hides what it once excused.
    pub(crate) fn unused(&self) -> Vec<String> {
        let used = self.used.borrow();
        let mut v: Vec<String> = self
            .scope
            .iter()
            .map(|(f, _)| format!("scope::{f}"))
            .chain(self.exact.iter().map(|(f, l, _)| {
                if f == "*" {
                    format!("exact::*::{l}")
                } else {
                    format!("exact::{f}::{l}")
                }
            }))
            .chain(self.prefix.iter().map(|(_, p, _)| format!("prefix::{p}")))
            .chain(self.safe.iter().map(|(f, e, _)| format!("safe::{f}::{e}")))
            .filter(|k| !used.contains(k))
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

#[test]
fn the_allowlist_file_is_well_formed() {
    // Every line has a kind, a file, a literal and a reason, and `parse_allowlist` refuses anything else.
    let allow = allowlist();
    for (f, l, r) in allow.exact.iter().chain(&allow.prefix).chain(&allow.safe) {
        assert!(
            !f.is_empty() && !l.is_empty() && r.trim().len() >= 8,
            "{f} :: {l} :: {r}"
        );
    }
    for (f, r) in &allow.scope {
        assert!(!f.is_empty() && r.trim().len() >= 8, "{f} :: {r}");
        assert!(
            root().join("src").join(f).is_file()
                || files_under(&root().join("src"), "rs")
                    .iter()
                    .any(|p| file_name(p) == *f),
            "scope names a file that does not exist: {f}"
        );
    }
    for bad in [
        "exact :: a.rs :: x",
        "exact :: a.rs :: x ::",
        "unknown :: a.rs :: x :: reason",
    ] {
        assert!(
            std::panic::catch_unwind(|| parse_allowlist(bad)).is_err(),
            "{bad}"
        );
    }
}
