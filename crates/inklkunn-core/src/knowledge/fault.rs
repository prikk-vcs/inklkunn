//! Why a knowledge file was refused, in rules and never in the file's own words.
//!
//! A [`Rule`] is one sentence's worth of what was wrong; a [`Fault`] adds the field of the format it was about (a
//! name from the format's own fixed vocabulary, never read from the file) and the line of the YAML when the
//! parser gave one. Nothing a file says is repeated: the files are written by people, and these words reach logs
//! and terminals (the import's rule, `import::Refusal`).

use inklkunn_markup::YamlError;

/// What was wrong with a knowledge file. The `code` is stable (the health list's rows and the API carry it); the
/// sentence is English for operators, and the surfaces keep their own, translated, sentence for each code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// The file is not at `knowledge/claims/<id>.yaml` or `knowledge/sources/<id>.yaml`, with a well formed id.
    Path,
    /// The file is not UTF-8 text.
    NotText,
    /// The file is longer than [`MAX_FILE_BYTES`](super::MAX_FILE_BYTES).
    TooLong,
    /// The text is not in the plain YAML subset: an anchor, an alias, a tag, a repeated key, a malformed line, bad
    /// indentation, an open quote, nesting that is too deep.
    Syntax,
    /// A key the format requires is missing.
    Missing,
    /// A key the format does not have (nothing is read from a file that says more than the format does).
    Unknown,
    /// A value of the wrong shape: a list where a word goes, a mapping where a text goes.
    Type,
    /// A number. Nothing in a knowledge file is numeric: dates and years are written as text.
    Number,
    /// A word outside the closed list its key allows, or a text that is not in the form its key needs (a digest,
    /// a date).
    Value,
    /// An identifier that is not well formed.
    Id,
    /// The same expression, relation or source twice in one file.
    Duplicate,
    /// A list with more entries than the format allows, or a text longer than it allows.
    Bounds,
    /// The file names a source or a claim that the view does not have.
    Reference,
    /// An expression names a page the view does not have, or a block the page does not have (or has twice).
    Anchor,
    /// A file would be removed that another file still names.
    InUse,
    /// The change also changes a key of a page that only the service writes.
    Owned,
    /// The change sets what only a curation proposal sets: a claim's community status, a source's `last_reviewed`
    /// or `availability` (part 2 D-3: never by a count, a tally or a timer, and never beside a page edit).
    Status,
}

impl Rule {
    /// Every rule, in the order the surfaces list them.
    pub const ALL: [Rule; 17] = [
        Rule::Path,
        Rule::NotText,
        Rule::TooLong,
        Rule::Syntax,
        Rule::Missing,
        Rule::Unknown,
        Rule::Type,
        Rule::Number,
        Rule::Value,
        Rule::Id,
        Rule::Duplicate,
        Rule::Bounds,
        Rule::Reference,
        Rule::Anchor,
        Rule::InUse,
        Rule::Owned,
        Rule::Status,
    ];

    /// The stable code.
    pub const fn code(self) -> &'static str {
        match self {
            Rule::Path => "path",
            Rule::NotText => "not-text",
            Rule::TooLong => "too-long",
            Rule::Syntax => "syntax",
            Rule::Missing => "missing-key",
            Rule::Unknown => "unknown-key",
            Rule::Type => "wrong-type",
            Rule::Number => "number",
            Rule::Value => "bad-value",
            Rule::Id => "bad-id",
            Rule::Duplicate => "duplicate",
            Rule::Bounds => "bounds",
            Rule::Reference => "reference",
            Rule::Anchor => "anchor",
            Rule::InUse => "in-use",
            Rule::Owned => "owned-key",
            Rule::Status => "status",
        }
    }

    /// The rule a code names.
    pub fn from_code(code: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|r| r.code() == code)
    }

    /// The rule as an English sentence, for operators and logs. It never holds anything a file said.
    pub const fn sentence(self) -> &'static str {
        match self {
            Rule::Path => {
                "the file is not at knowledge/claims/<id>.yaml or knowledge/sources/<id>.yaml with a well formed id"
            }
            Rule::NotText => "the file is not UTF-8 text",
            Rule::TooLong => "the file is longer than a knowledge file may be",
            Rule::Syntax => "the text is not in the plain YAML subset",
            Rule::Missing => "a key the format requires is missing",
            Rule::Unknown => "the file has a key the format does not have",
            Rule::Type => "a value is not of the shape its key needs",
            Rule::Number => "a number where the format has none (write dates and years as text)",
            Rule::Value => "a value is outside the words, or the form, its key allows",
            Rule::Id => "an identifier is not well formed",
            Rule::Duplicate => "the same entry appears twice",
            Rule::Bounds => "a list or a text is longer than the format allows",
            Rule::Reference => "the file names a source or a claim the view does not have",
            Rule::Anchor => {
                "an expression names a page the view does not have, or a block the page does not have once"
            }
            Rule::InUse => "the file is removed while another file still names it",
            Rule::Owned => "the change also changes a page key that only the service writes",
            Rule::Status => {
                "the change sets a community status, a review date or an availability, changes or removes a source link a claim already has, changes who or what a source that was checked is, or where it is found, removes a claim or a source that holds any of these, or replaces or removes a file that cannot be read, outside a curation proposal"
            }
        }
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.sentence())
    }
}

/// A knowledge file read and refused: the rule, the field of the format it was about and, for the YAML parser's
/// own refusals, the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// The rule.
    pub rule: Rule,
    /// The key of the format the fault was about (`kind`, `expressions`, …), or empty. Always one of the format's
    /// own words, never a word the file used.
    pub field: &'static str,
    /// The 1-based line of the YAML, when the parser gave one.
    pub line: Option<usize>,
}

impl Fault {
    /// A fault about `field`.
    pub const fn at(rule: Rule, field: &'static str) -> Fault {
        Fault {
            rule,
            field,
            line: None,
        }
    }

    /// A fault about the file as a whole.
    pub const fn whole(rule: Rule) -> Fault {
        Fault::at(rule, "")
    }
}

impl From<YamlError> for Fault {
    fn from(e: YamlError) -> Fault {
        let line = match e {
            YamlError::OutsideSubset { line, .. }
            | YamlError::Malformed { line }
            | YamlError::DuplicateKey { line, .. }
            | YamlError::Indentation { line }
            | YamlError::UnterminatedQuote { line }
            | YamlError::TooDeep { line } => line,
        };
        Fault {
            rule: Rule::Syntax,
            field: "",
            line: Some(line),
        }
    }
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.rule.sentence())?;
        if !self.field.is_empty() {
            write!(f, " (`{}`)", self.field)?;
        }
        if let Some(line) = self.line {
            write!(f, " (line {line})")?;
        }
        Ok(())
    }
}

impl std::error::Error for Fault {}
