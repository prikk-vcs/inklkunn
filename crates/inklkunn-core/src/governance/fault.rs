//! Why a governance file was refused, or could not be read, in rules and never in the file's own words.
//!
//! A [`Refusal`] is one sentence's worth of what was wrong, with a stable [`name`](Refusal::name) the catalog and the
//! audit trail key on. A [`Fault`] adds the file, the field of the schema it was about (a name from the schema's own
//! fixed vocabulary, never read from the file) and the line of the YAML where the machinery can say it. **Nothing a
//! file says is repeated**: governance files are written by people (or arrive by import), and these words reach logs,
//! terminals and the page that explains why a wiki refuses writes.

use inklkunn_markup::YamlError;

use super::{
    CONSTITUTION_PATH, INTERWIKI_PATH, MAX_CONSTITUTION_BYTES, MAX_INTERWIKI_BYTES,
    MAX_POLICY_BYTES, POLICY_PATH,
};

/// One of the governance files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GovernanceFile {
    /// `governance/constitution.yaml`: who may change `governance/`.
    Constitution,
    /// `governance/policy.yaml`: who may do what.
    Policy,
    /// `governance/interwiki.yaml`: where a link with a prefix leads (RFC 007 D-8). Not a rule of who may do what, and still
    /// a governance file: a change to it is a change to the rules, at the policy-change rung.
    Interwiki,
}

impl GovernanceFile {
    /// All, constitution first.
    pub const ALL: [GovernanceFile; 3] = [
        GovernanceFile::Constitution,
        GovernanceFile::Policy,
        GovernanceFile::Interwiki,
    ];

    /// The file's path in the repository.
    pub const fn path(self) -> &'static str {
        match self {
            GovernanceFile::Constitution => CONSTITUTION_PATH,
            GovernanceFile::Policy => POLICY_PATH,
            GovernanceFile::Interwiki => INTERWIKI_PATH,
        }
    }

    /// The most bytes the file may have.
    pub const fn max_bytes(self) -> usize {
        match self {
            GovernanceFile::Constitution => MAX_CONSTITUTION_BYTES,
            GovernanceFile::Policy => MAX_POLICY_BYTES,
            GovernanceFile::Interwiki => MAX_INTERWIKI_BYTES,
        }
    }

    /// The file a repository path is, if it is one of them.
    pub fn of_path(path: &str) -> Option<GovernanceFile> {
        GovernanceFile::ALL.into_iter().find(|f| f.path() == path)
    }

    /// The file a repository path is, **or whose place it is below** (a directory made where a file belongs): what a block
    /// that changed the shape of one of the paths is recognized by (handoff G-T1 correction C-5).
    pub fn at_or_below(path: &str) -> Option<GovernanceFile> {
        GovernanceFile::ALL.into_iter().find(|f| {
            path == f.path()
                || path
                    .strip_prefix(f.path())
                    .is_some_and(|rest| rest.starts_with('/'))
        })
    }
}

/// Which two rungs of the ordering rule (RFC 006 D-2: `propose` ≤ `review` ≤ `accept`, `moderate` ≥ `reviewer`, and
/// `decide_appeal` ≥ `moderate`) a policy puts the wrong way round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OrderingPair {
    /// `rungs.propose` is above `rungs.review`: someone could open a proposal that nobody they could ask may review.
    ProposeAboveReview,
    /// `rungs.review` is above `rungs.accept`: someone could accept what they may not review.
    ReviewAboveAccept,
    /// `rungs.moderate` is below reviewer: someone who may not review proposals could hide what is in them.
    ModerateBelowReviewer,
    /// `rungs.decide_appeal` is below `rungs.moderate`: someone who may not moderate could, by overturning an appeal, undo what a
    /// moderator did (a hiding, a restriction), which an overturn is the reversal of (A-4).
    DecideAppealBelowModerate,
}

/// A limit of the platform constitution (RFC 006 D-3): one of the operator's two settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Limit {
    /// `INKLKUNN_GOVERNANCE_ENTRY_MAX`: the highest `entry` a wiki may set.
    EntryMax,
    /// `INKLKUNN_GOVERNANCE_LIMIT_DAYS`: the longest temporary protection or restriction. A standing protection has no
    /// expiry, so a policy cannot exceed this one yet (the hook is [`super::PlatformLimits::allows_days`]).
    LimitDays,
}

impl Limit {
    /// The operator's variable that sets the limit.
    pub const fn variable(self) -> &'static str {
        match self {
            Limit::EntryMax => "INKLKUNN_GOVERNANCE_ENTRY_MAX",
            Limit::LimitDays => "INKLKUNN_GOVERNANCE_LIMIT_DAYS",
        }
    }
}

/// What was wrong with a governance file, or with a proposal that touches `governance/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Refusal {
    /// The file is not UTF-8 text.
    NotText,
    /// The file is longer than its bound ([`GovernanceFile::max_bytes`]).
    TooLong,
    /// The text is not in the plain YAML subset (an anchor, an alias, a tag, a malformed line, bad indentation, an
    /// open quote, nesting that is too deep), or a value is of the wrong shape: a list where a word goes, a mapping
    /// where a text goes, a word where a number goes.
    WrongShape,
    /// A key the schema does not have. Nothing is read from a file that says more than the schema does.
    UnknownKey,
    /// The same key twice in one mapping.
    RepeatedKey,
    /// A word outside the closed list its key allows.
    UnknownValue,
    /// A number outside the range its key allows.
    OutOfRange,
    /// A list with more entries than the schema allows, or a text longer than it allows.
    Bounds,
    /// A control character, a direction or format character, or a line separator in the file (the text could read
    /// as something other than it is).
    ForbiddenCharacter,
    /// A key the schema requires is missing, or a text is empty.
    Missing,
    /// The same path or namespace is protected twice.
    Duplicate,
    /// The ordering rule is broken, and the pair says which two rungs.
    Ordering(OrderingPair),
    /// A limit of the platform constitution is exceeded, and the limit says which.
    PlatformLimit(Limit),
    /// A proposal in an alternative view touches `governance/` (RFC 006 D-1: governance forks are not in scope).
    OutsideDefaultView,
    /// A proposal writes a path under `governance/` that is neither of the two files: the set of paths is closed
    /// (exactly `governance/constitution.yaml` and `governance/policy.yaml`), and no other file there is part of the
    /// rules of a wiki.
    UnknownFile,
    /// Where one of the two governance files belongs there is something that is not an ordinary file (a directory, a link,
    /// anything else): the rules of a wiki are read from files, so the wiki refuses every write until it is put right. (Found
    /// in a repository, never written by a proposal; handoff G-T1 correction C-5.)
    NotAFile,
    /// An interwiki prefix is one of the wiki's own namespaces (RFC 007 D-8): a link `[[Help:Contents]]` would mean two things.
    PrefixIsNamespace,
    /// An interwiki template is not `https://`.
    TemplateScheme,
    /// An interwiki template has no host, or one that is not a host name (with an optional port).
    TemplateHost,
    /// An interwiki template names user information (`user@host`).
    TemplateUserInfo,
    /// An interwiki template does not have exactly one `{title}`, in its path or its query, and no other brace.
    TemplatePlaceholder,
    /// An interwiki template holds a character a URL is not written with here (a space, a non-ASCII character, `"<>\^`|`).
    TemplateCharacter,
}

impl Refusal {
    /// Every refusal, in the order of the declaration. [`Refusal::from_name`] reads this list, and a test holds that it
    /// is complete (a refusal that is not here is one the stored name of a snapshot could not be read back as).
    pub const ALL: [Refusal; 26] = [
        Refusal::NotText,
        Refusal::TooLong,
        Refusal::WrongShape,
        Refusal::UnknownKey,
        Refusal::RepeatedKey,
        Refusal::UnknownValue,
        Refusal::OutOfRange,
        Refusal::Bounds,
        Refusal::ForbiddenCharacter,
        Refusal::Missing,
        Refusal::Duplicate,
        Refusal::Ordering(OrderingPair::ProposeAboveReview),
        Refusal::Ordering(OrderingPair::ReviewAboveAccept),
        Refusal::Ordering(OrderingPair::ModerateBelowReviewer),
        Refusal::Ordering(OrderingPair::DecideAppealBelowModerate),
        Refusal::PlatformLimit(Limit::EntryMax),
        Refusal::PlatformLimit(Limit::LimitDays),
        Refusal::OutsideDefaultView,
        Refusal::UnknownFile,
        Refusal::NotAFile,
        Refusal::PrefixIsNamespace,
        Refusal::TemplateScheme,
        Refusal::TemplateHost,
        Refusal::TemplateUserInfo,
        Refusal::TemplatePlaceholder,
        Refusal::TemplateCharacter,
    ];

    /// The refusal a stable [`name`](Refusal::name) is of.
    pub fn from_name(name: &str) -> Option<Refusal> {
        Refusal::ALL.into_iter().find(|r| r.name() == name)
    }

    /// The stable name: what the catalog, the audit trail and the API key on. Never reworded.
    pub const fn name(self) -> &'static str {
        match self {
            Refusal::NotText => "not-text",
            Refusal::TooLong => "too-long",
            Refusal::WrongShape => "wrong-shape",
            Refusal::UnknownKey => "unknown-key",
            Refusal::RepeatedKey => "repeated-key",
            Refusal::UnknownValue => "unknown-value",
            Refusal::OutOfRange => "out-of-range",
            Refusal::Bounds => "bounds",
            Refusal::ForbiddenCharacter => "forbidden-character",
            Refusal::Missing => "missing-key",
            Refusal::Duplicate => "duplicate",
            Refusal::Ordering(OrderingPair::ProposeAboveReview) => "ordering-propose-above-review",
            Refusal::Ordering(OrderingPair::ReviewAboveAccept) => "ordering-review-above-accept",
            Refusal::Ordering(OrderingPair::ModerateBelowReviewer) => {
                "ordering-moderate-below-reviewer"
            }
            Refusal::Ordering(OrderingPair::DecideAppealBelowModerate) => {
                "ordering-decide-appeal-below-moderate"
            }
            Refusal::PlatformLimit(Limit::EntryMax) => "platform-limit-entry-max",
            Refusal::PlatformLimit(Limit::LimitDays) => "platform-limit-limit-days",
            Refusal::OutsideDefaultView => "outside-default-view",
            Refusal::UnknownFile => "unknown-file",
            Refusal::NotAFile => "not-a-file",
            Refusal::PrefixIsNamespace => "prefix-is-namespace",
            Refusal::TemplateScheme => "template-not-https",
            Refusal::TemplateHost => "template-host",
            Refusal::TemplateUserInfo => "template-userinfo",
            Refusal::TemplatePlaceholder => "template-placeholder",
            Refusal::TemplateCharacter => "template-character",
        }
    }

    /// The refusal as an English sentence, for operators and logs. It never holds anything a file said. (The
    /// surfaces keep their own, translated, sentence for each [`name`](Refusal::name).)
    pub const fn sentence(self) -> &'static str {
        match self {
            Refusal::NotText => "the file is not UTF-8 text",
            Refusal::TooLong => "the file is longer than a governance file may be",
            Refusal::WrongShape => {
                "the text is not in the plain YAML subset, or a value is not of the shape its key needs"
            }
            Refusal::UnknownKey => "the file has a key the rules of a wiki do not have",
            Refusal::RepeatedKey => "a key is written twice",
            Refusal::UnknownValue => "a value is not one of the words its key allows",
            Refusal::OutOfRange => "a number is outside the range its key allows",
            Refusal::Bounds => "a list or a text is longer than the schema allows",
            Refusal::ForbiddenCharacter => {
                "the file holds a control, direction or line-separator character"
            }
            Refusal::Missing => "a key the schema requires is missing or empty",
            Refusal::Duplicate => "the same path or namespace is protected twice",
            Refusal::Ordering(OrderingPair::ProposeAboveReview) => {
                "the ordering rule is broken: the rung that proposes is above the rung that reviews"
            }
            Refusal::Ordering(OrderingPair::ReviewAboveAccept) => {
                "the ordering rule is broken: the rung that reviews is above the rung that accepts"
            }
            Refusal::Ordering(OrderingPair::ModerateBelowReviewer) => {
                "the ordering rule is broken: the rung that moderates is below reviewer"
            }
            Refusal::Ordering(OrderingPair::DecideAppealBelowModerate) => {
                "the ordering rule is broken: the rung that decides appeals is below the rung that moderates"
            }
            Refusal::PlatformLimit(Limit::EntryMax) => {
                "the entry rung is above INKLKUNN_GOVERNANCE_ENTRY_MAX, the highest the operator allows"
            }
            Refusal::PlatformLimit(Limit::LimitDays) => {
                "a period is longer than INKLKUNN_GOVERNANCE_LIMIT_DAYS, the longest the operator allows"
            }
            Refusal::OutsideDefaultView => {
                "governance/ can be changed only in the default view, and this proposal is in another view"
            }
            Refusal::UnknownFile => {
                "governance/ holds exactly two files, constitution.yaml and policy.yaml, and this is neither"
            }
            Refusal::NotAFile => {
                "there is something at the file's place that is not an ordinary file"
            }
            Refusal::PrefixIsNamespace => "an interwiki prefix is one of the wiki's own namespaces",
            Refusal::TemplateScheme => "an interwiki template does not start with https://",
            Refusal::TemplateHost => "an interwiki template has no host, or one that is not a host name",
            Refusal::TemplateUserInfo => "an interwiki template names user information",
            Refusal::TemplatePlaceholder => {
                "an interwiki template does not have exactly one {title}, in its path or its query"
            }
            Refusal::TemplateCharacter => {
                "an interwiki template holds a character that a URL is not written with here"
            }
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.sentence())
    }
}

/// A governance file refused: the file, the rule broken, the field of the schema it was about and, where the
/// machinery can say it, the line.
///
/// The line is the YAML parser's own for a refusal of the text, and for the rest the line of the key the refusal is
/// about, found in the file's block structure (the line of its nearest enclosing key when the file writes the
/// mapping on one line). It is a pointer for a person to follow, never a quotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// The file, or `None` for a proposal that touches a file under `governance/` that is none of them
    /// ([`Refusal::OutsideDefaultView`] only).
    pub file: Option<GovernanceFile>,
    /// What was wrong.
    pub refusal: Refusal,
    /// The key of the schema the refusal was about (`rungs.accept`, `protect.reason`), or empty. Always one of the
    /// schema's own words, never a word the file used.
    pub field: &'static str,
    /// The 1-based entry of `protect` the refusal was about, when it was about one.
    pub item: Option<usize>,
    /// The 1-based line of the YAML, when it can be said.
    pub line: Option<usize>,
}

impl Fault {
    /// A refusal of `file` about `field`.
    pub const fn at(file: GovernanceFile, refusal: Refusal, field: &'static str) -> Fault {
        Fault {
            file: Some(file),
            refusal,
            field,
            item: None,
            line: None,
        }
    }

    /// A refusal about the file as a whole.
    pub const fn whole(file: GovernanceFile, refusal: Refusal) -> Fault {
        Fault::at(file, refusal, "")
    }

    /// A refusal of a proposal that touches `governance/` in an alternative view.
    pub const fn outside_default_view(file: Option<GovernanceFile>) -> Fault {
        Fault {
            file,
            refusal: Refusal::OutsideDefaultView,
            field: "",
            item: None,
            line: None,
        }
    }

    /// A refusal of a path under `governance/` that is neither of the two files.
    pub const fn unknown_file() -> Fault {
        Fault {
            file: None,
            refusal: Refusal::UnknownFile,
            field: "",
            item: None,
            line: None,
        }
    }

    /// The path the fault is about: the file's, or `governance/` when it is no file of the two.
    pub fn path(&self) -> &'static str {
        self.file
            .map_or(super::GOVERNANCE_DIR_SLASH, GovernanceFile::path)
    }

    /// The stable name of the refusal ([`Refusal::name`]).
    pub const fn name(&self) -> &'static str {
        self.refusal.name()
    }

    pub(super) fn from_yaml(file: GovernanceFile, e: &YamlError) -> Fault {
        let (refusal, line) = match e {
            YamlError::DuplicateKey { line, .. } => (Refusal::RepeatedKey, *line),
            YamlError::OutsideSubset { line, .. }
            | YamlError::Malformed { line }
            | YamlError::Indentation { line }
            | YamlError::UnterminatedQuote { line }
            | YamlError::TooDeep { line } => (Refusal::WrongShape, *line),
        };
        Fault {
            line: Some(line),
            ..Fault::whole(file, refusal)
        }
    }
}

impl std::fmt::Display for Fault {
    /// `governance/policy.yaml, line 7: <sentence> (`rungs.accept`)`: the file, the line when there is one, the
    /// rule's sentence and the schema's field. Never what the file said.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.path())?;
        if let Some(line) = self.line {
            write!(f, ", line {line}")?;
        }
        write!(f, ": {}", self.refusal.sentence())?;
        if !self.field.is_empty() {
            write!(f, " (`{}`", self.field)?;
            if let Some(item) = self.item {
                write!(f, ", entry {item}")?;
            }
            f.write_str(")")?;
        }
        Ok(())
    }
}

impl std::error::Error for Fault {}

/// A governance file **in the repository** that cannot be read: the wiki then refuses every write, and still reads
/// by its visibility (RFC 006 D-1, threat model C-3d). It is a [`Fault`] of a read: the file, the line where it can be
/// said and the rule, and it never carries what the file said.
///
/// `Display` is `governance/policy.yaml, line N: <rule>` (the line is left out when the machinery cannot give one,
/// and the field follows the rule). Only [`read_policy`](super::read_policy) and
/// [`read_constitution`](super::read_constitution) make one, and a platform limit is not a read fault: a policy above
/// a limit is read as if it were at the limit ([`Policy::as_read_under`](super::Policy::as_read_under)).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFault {
    file: GovernanceFile,
    fault: Fault,
    /// What stands at or below the place of the file, when the rule is [`Refusal::NotAFile`]: the repository paths, at most
    /// [`MAX_FOUND`], so that an admin can say what the repair removes (handoff G-T1 correction E-1). Empty for every other
    /// rule.
    found: Vec<String>,
    /// How many more paths there were than `found` lists.
    more: usize,
}

/// The most paths a [`ReadFault`] lists ([`ReadFault::found`]); the rest are only counted.
pub const MAX_FOUND: usize = inklkunn_store::MAX_FAULT_FOUND;

impl ReadFault {
    pub(super) fn new(file: GovernanceFile, fault: Fault) -> ReadFault {
        ReadFault {
            file,
            fault,
            found: Vec::new(),
            more: 0,
        }
    }

    /// A read fault as a derived snapshot keeps it: the file, the line when there was one and the rule. The snapshot keeps
    /// no field of the schema, so the fault has none (its `Display` is the same sentence without the parenthesis).
    pub fn from_stored(file: GovernanceFile, line: Option<usize>, refusal: Refusal) -> ReadFault {
        ReadFault {
            file,
            fault: Fault {
                line,
                ..Fault::whole(file, refusal)
            },
            found: Vec::new(),
            more: 0,
        }
    }

    /// The same fault **with the paths found at its place**: the first [`MAX_FOUND`] of `found` are kept, and the rest are
    /// counted with `more`. Only the `not-a-file` rule has any.
    #[must_use]
    pub fn with_found(mut self, mut found: Vec<String>, more: usize) -> ReadFault {
        let over = found.len().saturating_sub(MAX_FOUND);
        found.truncate(MAX_FOUND);
        self.found = found;
        self.more = more.saturating_add(over);
        self
    }

    /// The paths found at or below the place of the file, for a `not-a-file` fault: **what a repair removes**. At most
    /// [`MAX_FOUND`]; [`ReadFault::more`] counts the rest. The names are the repository's, never a file's text.
    pub fn found(&self) -> &[String] {
        &self.found
    }

    /// How many more paths than [`ReadFault::found`] lists were at or below the place of the file.
    pub fn more(&self) -> usize {
        self.more
    }

    /// The file that cannot be read.
    pub fn file(&self) -> GovernanceFile {
        self.file
    }

    /// The line of the YAML, when it can be said.
    pub fn line(&self) -> Option<usize> {
        self.fault.line
    }

    /// The rule broken.
    pub fn refusal(&self) -> Refusal {
        self.fault.refusal
    }

    /// The fault, with its field.
    pub fn fault(&self) -> &Fault {
        &self.fault
    }
}

impl std::fmt::Display for ReadFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fault.fmt(f)
    }
}

impl std::error::Error for ReadFault {}
