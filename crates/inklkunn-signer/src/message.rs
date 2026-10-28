//! The patch message: the attribution contract between the service and prikk's history (external design
//! CT-03; RFC 002 D-5; requirements ID-4; threat model C-1b, C-12a and INV-10).
//!
//! prikk stores the commit message **signed and verbatim** (ledger PK-14: a trailing newline, CRLF and any
//! script survive a round trip, and an empty message is refused), so what is written here is what anyone
//! holding the repository reads with `prikk log`. One grammar, written and read by one module:
//!
//! ```text
//! <summary: 1 to 120 characters, one line>
//! <blank line>
//! Proposal: <proposal id>                       (proposals and rollbacks only)
//! Proposed-By: <account id> (<display name>)    or  anonymous (~<token>)  or  system (<label>)
//! Accepted-By: <account id> (<display name>)    or  system (<label>)
//! Base: <block id, 64 lower-case hex>           (proposals and rollbacks only)
//! Kind: proposal | rollback | import | system
//! Imported-From: <url or bundle digest>         (imports only)
//! ```
//!
//! The trailers appear **once each, in this order, and no others**; every line ends with a line feed (no
//! carriage return) and the message ends with exactly one. The parser is **strict and total**: it accepts
//! only the canonical text a composer writes and returns a typed error, never a panic, for anything else, so
//! the reconciliation can tell a message the service wrote from a change someone else sealed (INV-1,
//! INV-10), and the signer can refuse a message that does not name the acceptance it is bound to (C-1b).
//!
//! This module lives in the signer's crate because the signer is where the contract is **enforced** (it
//! re-parses what it is asked to sign) and `inklkunn-core` may not be depended on from below; `inklkunn-core`
//! re-exports it, and composing a message is core's act.
//!
//! **No value is ever echoed in an error.** A display name, a summary or an import source is untrusted text
//! from a person; an error says which field broke which rule.

use std::fmt;

use inklkunn_store::{AccountId, DisplayName, ProposalId};

/// The longest summary line CT-03 allows (RFC 002 D-5), in characters.
pub const SUMMARY_MAX_CHARS: usize = 120;
/// The longest import source (a URL or a bundle digest), in characters.
pub const IMPORT_SOURCE_MAX_CHARS: usize = 512;
/// The longest pseudonym token after its `~`, in characters.
const PSEUDONYM_MAX: usize = 16;
/// The longest system label, in characters.
const LABEL_MAX: usize = 32;
/// The mark of a summary that was cut.
const ELLIPSIS: char = '…';

/// The trailers, in the order they appear.
const ORDER: [&str; 6] = [
    "Proposal",
    "Proposed-By",
    "Accepted-By",
    "Base",
    "Kind",
    "Imported-From",
];
const PROPOSAL: usize = 0;
const PROPOSED_BY: usize = 1;
const ACCEPTED_BY: usize = 2;
const BASE: usize = 3;
const KIND: usize = 4;
const IMPORTED_FROM: usize = 5;

/// A control character, or a character some line-oriented reader splits on (U+2028, U+2029).
fn is_line_or_control(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Why a message could not be composed or read. No variant carries the offending text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageError {
    /// The summary is empty, or only whitespace.
    EmptySummary,
    /// The summary is longer than [`SUMMARY_MAX_CHARS`] characters.
    SummaryTooLong,
    /// A one-line field has a control character, a line break, or leading or trailing whitespace.
    NotOneLine(&'static str),
    /// A field's value does not have the shape its trailer requires.
    Invalid {
        /// The trailer (or field) name.
        field: &'static str,
        /// The rule it broke, in a few words.
        rule: &'static str,
    },
    /// A trailer this kind of message must carry is absent.
    Missing(&'static str),
    /// A trailer, or an actor, this kind of message may not carry.
    NotAllowed {
        /// The trailer (or field) name.
        trailer: &'static str,
        /// The kind of message.
        kind: Kind,
    },
    /// The text is not in canonical form at this line (1-based).
    Malformed {
        /// The line.
        line: usize,
        /// The rule it broke, in a few words.
        rule: &'static str,
    },
}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageError::EmptySummary => write!(f, "the summary is empty"),
            MessageError::SummaryTooLong => write!(
                f,
                "the summary is longer than {SUMMARY_MAX_CHARS} characters"
            ),
            MessageError::NotOneLine(field) => write!(
                f,
                "the {field} must be one line, with no control characters and no space at either end"
            ),
            MessageError::Invalid { field, rule } => write!(f, "{field}: {rule}"),
            MessageError::Missing(t) => write!(f, "the `{t}` trailer is missing"),
            MessageError::NotAllowed { trailer, kind } => {
                write!(f, "a {} message may not carry {trailer}", kind.as_str())
            }
            MessageError::Malformed { line, rule } => write!(f, "line {line}: {rule}"),
        }
    }
}

impl std::error::Error for MessageError {}

/// What a message records the change as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// An accepted proposal.
    Proposal,
    /// An accepted rollback proposal (an undo is a proposal, PR-8).
    Rollback,
    /// An import, made by the service itself.
    Import,
    /// Any other change the service makes itself (creating a wiki).
    System,
}

impl Kind {
    /// The trailer's value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Proposal => "proposal",
            Kind::Rollback => "rollback",
            Kind::Import => "import",
            Kind::System => "system",
        }
    }

    /// Parse the trailer's value.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "proposal" => Some(Kind::Proposal),
            "rollback" => Some(Kind::Rollback),
            "import" => Some(Kind::Import),
            "system" => Some(Kind::System),
            _ => None,
        }
    }

    /// Does this kind name a proposal and a base (and so need an acceptance record)?
    pub const fn names_a_proposal(self) -> bool {
        matches!(self, Kind::Proposal | Kind::Rollback)
    }
}

/// The first line of a message: one line, 1 to [`SUMMARY_MAX_CHARS`] characters, no control characters, no
/// space at either end.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Summary(String);

impl Summary {
    /// A summary exactly as it must already be (what the parser reads back).
    pub fn new(text: &str) -> Result<Self, MessageError> {
        if text.trim().is_empty() {
            return Err(MessageError::EmptySummary);
        }
        if text.chars().any(is_line_or_control) || text.trim() != text {
            return Err(MessageError::NotOneLine("summary"));
        }
        if text.chars().count() > SUMMARY_MAX_CHARS {
            return Err(MessageError::SummaryTooLong);
        }
        Ok(Summary(text.to_owned()))
    }

    /// A summary made from a person's free text (a proposal's "what and why"): control characters and line
    /// breaks become spaces, the ends are trimmed, and a text over [`SUMMARY_MAX_CHARS`] characters is cut
    /// to that length with a closing `…`. Only an empty text is refused.
    pub fn from_text(text: &str) -> Result<Self, MessageError> {
        let one_line: String = text
            .chars()
            .map(|c| if is_line_or_control(c) { ' ' } else { c })
            .collect();
        let trimmed = one_line.trim();
        if trimmed.is_empty() {
            return Err(MessageError::EmptySummary);
        }
        if trimmed.chars().count() <= SUMMARY_MAX_CHARS {
            return Ok(Summary(trimmed.to_owned()));
        }
        let mut cut: String = trimmed.chars().take(SUMMARY_MAX_CHARS - 1).collect();
        cut.push(ELLIPSIS);
        Ok(Summary(cut))
    }

    /// The summary of a change the service made for itself when its caller gave none to say.
    pub fn system_change() -> Self {
        Summary("System change".to_owned())
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An anonymous proposer's temporary pseudonym (RFC 002 D-10): `~` and 1 to 16 of `a-z`, `0-9`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Pseudonym(String);

impl Pseudonym {
    /// Validate a pseudonym.
    pub fn new(s: &str) -> Result<Self, MessageError> {
        let ok = s.strip_prefix('~').is_some_and(|t| {
            (1..=PSEUDONYM_MAX).contains(&t.len())
                && t.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
        if ok {
            Ok(Pseudonym(s.to_owned()))
        } else {
            Err(MessageError::Invalid {
                field: "pseudonym",
                rule: "`~` and 1 to 16 lower-case letters or digits",
            })
        }
    }

    /// The text, with its `~`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The name of a service-made change's actor: `inklkunn`, `import`, `recovery`. A lower-case letter, then
/// lower-case letters, digits and single hyphens, at most 32 characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SystemLabel(String);

impl SystemLabel {
    /// Validate a label.
    pub fn new(s: &str) -> Result<Self, MessageError> {
        let first_ok = s.bytes().next().is_some_and(|b| b.is_ascii_lowercase());
        let ok = first_ok
            && s.len() <= LABEL_MAX
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !s.ends_with('-')
            && !s.contains("--");
        if ok {
            Ok(SystemLabel(s.to_owned()))
        } else {
            Err(MessageError::Invalid {
                field: "system label",
                rule: "a lower-case letter, then lower-case letters, digits and single hyphens (at most 32)",
            })
        }
    }

    /// The label every change the service makes for itself carries today.
    pub fn inklkunn() -> Self {
        SystemLabel("inklkunn".to_owned())
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A sealed block's id as a message names it: 64 lower-case hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlockId(String);

impl BlockId {
    /// Validate a block id.
    pub fn new(s: &str) -> Result<Self, MessageError> {
        let ok = s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase());
        if ok {
            Ok(BlockId(s.to_owned()))
        } else {
            Err(MessageError::Invalid {
                field: "Base",
                rule: "a block id is 64 lower-case hex characters",
            })
        }
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Where an import came from: a URL or a bundle digest, one line, at most [`IMPORT_SOURCE_MAX_CHARS`]
/// characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportSource(String);

impl ImportSource {
    /// Validate an import source.
    pub fn new(s: &str) -> Result<Self, MessageError> {
        if s.is_empty() || s.chars().any(is_line_or_control) || s.trim() != s {
            return Err(MessageError::NotOneLine("import source"));
        }
        if s.chars().count() > IMPORT_SOURCE_MAX_CHARS {
            return Err(MessageError::Invalid {
                field: "Imported-From",
                rule: "at most 512 characters",
            });
        }
        Ok(ImportSource(s.to_owned()))
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A person, a pseudonym or the service: who proposed or accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Actor {
    /// An account, by its stable id, with the display name it had at the time.
    Account {
        /// The stable id.
        id: AccountId,
        /// The name at the time (RFC 002 D-5: names change, ids do not).
        name: DisplayName,
    },
    /// An anonymous proposer, by temporary pseudonym (only ever a proposer).
    Anonymous(Pseudonym),
    /// The service itself.
    System(SystemLabel),
}

impl Actor {
    /// The trailer value: `<id> (<name>)`, `anonymous (<pseudonym>)` or `system (<label>)`.
    fn render(&self) -> String {
        match self {
            Actor::Account { id, name } => format!("{id} ({name})"),
            Actor::Anonymous(p) => format!("anonymous ({})", p.as_str()),
            Actor::System(l) => format!("system ({})", l.as_str()),
        }
    }

    /// Read a trailer value. The account id has no space or parenthesis, so the first ` (` ends it; the
    /// display name is everything between there and the final `)`, parentheses and all.
    fn parse(field: &'static str, value: &str) -> Result<Self, MessageError> {
        let bad = |rule: &'static str| MessageError::Invalid { field, rule };
        let (head, rest) = value.split_once(" (").ok_or_else(|| {
            bad("`<id> (<name>)`, `anonymous (<pseudonym>)` or `system (<label>)`")
        })?;
        let inner = rest
            .strip_suffix(')')
            .ok_or_else(|| bad("the parenthesis around the name is closed"))?;
        match head {
            "anonymous" => Pseudonym::new(inner).map(Actor::Anonymous),
            "system" => SystemLabel::new(inner).map(Actor::System),
            id => {
                let id = AccountId::parse(id)
                    .ok_or_else(|| bad("an account id, `anonymous` or `system`"))?;
                let name = DisplayName::new(inner).map_err(|_| bad("a valid display name"))?;
                Ok(Actor::Account { id, name })
            }
        }
    }

    fn is_service(&self) -> bool {
        matches!(self, Actor::System(_))
    }

    fn is_account(&self) -> bool {
        matches!(self, Actor::Account { .. })
    }
}

/// A patch message: composed here, written by the signer, read back by the reconciliation and the pages.
/// Every constructor and the parser enforce the same rules, so a `Message` that exists is one that parses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    kind: Kind,
    summary: Summary,
    proposal: Option<ProposalId>,
    proposed_by: Actor,
    accepted_by: Actor,
    base: Option<BlockId>,
    imported_from: Option<ImportSource>,
}

impl Message {
    /// An accepted proposal. The proposer is an account or an anonymous pseudonym; the acceptor is an
    /// account.
    pub fn proposal(
        summary: Summary,
        proposal: ProposalId,
        proposed_by: Actor,
        accepted_by: Actor,
        base: BlockId,
    ) -> Result<Self, MessageError> {
        Self::checked(Message {
            kind: Kind::Proposal,
            summary,
            proposal: Some(proposal),
            proposed_by,
            accepted_by,
            base: Some(base),
            imported_from: None,
        })
    }

    /// An accepted rollback proposal; the actors are as for [`Message::proposal`].
    pub fn rollback(
        summary: Summary,
        proposal: ProposalId,
        proposed_by: Actor,
        accepted_by: Actor,
        base: BlockId,
    ) -> Result<Self, MessageError> {
        Self::checked(Message {
            kind: Kind::Rollback,
            summary,
            proposal: Some(proposal),
            proposed_by,
            accepted_by,
            base: Some(base),
            imported_from: None,
        })
    }

    /// An import the service made, from `source`.
    pub fn import(summary: Summary, label: SystemLabel, source: ImportSource) -> Self {
        Message {
            kind: Kind::Import,
            summary,
            proposal: None,
            proposed_by: Actor::System(label.clone()),
            accepted_by: Actor::System(label),
            base: None,
            imported_from: Some(source),
        }
    }

    /// A change the service made for itself (creating a wiki).
    pub fn system(summary: Summary, label: SystemLabel) -> Self {
        Message {
            kind: Kind::System,
            summary,
            proposal: None,
            proposed_by: Actor::System(label.clone()),
            accepted_by: Actor::System(label),
            base: None,
            imported_from: None,
        }
    }

    /// The kind.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The summary line.
    pub fn summary(&self) -> &Summary {
        &self.summary
    }

    /// The proposal, for a proposal or a rollback.
    pub fn proposal_id(&self) -> Option<&ProposalId> {
        self.proposal.as_ref()
    }

    /// Who proposed.
    pub fn proposed_by(&self) -> &Actor {
        &self.proposed_by
    }

    /// Who accepted.
    pub fn accepted_by(&self) -> &Actor {
        &self.accepted_by
    }

    /// The block the proposal was made against, for a proposal or a rollback.
    pub fn base(&self) -> Option<&BlockId> {
        self.base.as_ref()
    }

    /// Where an import came from.
    pub fn imported_from(&self) -> Option<&ImportSource> {
        self.imported_from.as_ref()
    }

    /// The invariants every `Message` holds, whichever way it was made.
    fn checked(m: Message) -> Result<Message, MessageError> {
        let not_allowed = |trailer: &'static str| MessageError::NotAllowed {
            trailer,
            kind: m.kind,
        };
        if m.kind.names_a_proposal() {
            if m.proposal.is_none() {
                return Err(MessageError::Missing("Proposal"));
            }
            if m.base.is_none() {
                return Err(MessageError::Missing("Base"));
            }
            if m.imported_from.is_some() {
                return Err(not_allowed("Imported-From"));
            }
            if m.proposed_by.is_service() {
                return Err(not_allowed("a system proposer"));
            }
            if !m.accepted_by.is_account() {
                return Err(not_allowed("an acceptor that is not an account"));
            }
        } else {
            if m.proposal.is_some() {
                return Err(not_allowed("Proposal"));
            }
            if m.base.is_some() {
                return Err(not_allowed("Base"));
            }
            if !m.proposed_by.is_service() || !m.accepted_by.is_service() {
                return Err(not_allowed("an actor that is not the service"));
            }
            match (m.kind, &m.imported_from) {
                (Kind::Import, None) => return Err(MessageError::Missing("Imported-From")),
                (Kind::System, Some(_)) => return Err(not_allowed("Imported-From")),
                _ => {}
            }
        }
        Ok(m)
    }

    /// The canonical text: what the signer hands to `prikk commit -m` and what `prikk log` shows back.
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity(256);
        out.push_str(self.summary.as_str());
        out.push_str("\n\n");
        if let Some(p) = &self.proposal {
            out.push_str(&format!("{}: {p}\n", ORDER[PROPOSAL]));
        }
        out.push_str(&format!(
            "{}: {}\n",
            ORDER[PROPOSED_BY],
            self.proposed_by.render()
        ));
        out.push_str(&format!(
            "{}: {}\n",
            ORDER[ACCEPTED_BY],
            self.accepted_by.render()
        ));
        if let Some(b) = &self.base {
            out.push_str(&format!("{}: {}\n", ORDER[BASE], b.as_str()));
        }
        out.push_str(&format!("{}: {}\n", ORDER[KIND], self.kind.as_str()));
        if let Some(s) = &self.imported_from {
            out.push_str(&format!("{}: {}\n", ORDER[IMPORTED_FROM], s.as_str()));
        }
        out
    }

    /// Read a message. Only the canonical text a composer writes is accepted: the same bytes come back from
    /// [`Message::to_text`] for anything that parses.
    pub fn parse(text: &str) -> Result<Self, MessageError> {
        let malformed = |line: usize, rule: &'static str| MessageError::Malformed { line, rule };
        if let Some(i) = text.find('\r') {
            let line = text.get(..i).map_or(1, |t| t.matches('\n').count() + 1);
            return Err(malformed(line, "line breaks are line feeds only"));
        }
        let Some(body) = text.strip_suffix('\n') else {
            let line = text.matches('\n').count() + 1;
            return Err(malformed(line, "the message ends with one line feed"));
        };
        let mut lines = body.split('\n').enumerate().map(|(i, l)| (i + 1, l));
        let summary = match lines.next() {
            Some((_, first)) => Summary::new(first)?,
            None => return Err(MessageError::EmptySummary),
        };
        match lines.next() {
            Some((_, "")) => {}
            Some((n, _)) => return Err(malformed(n, "a blank line follows the summary")),
            None => return Err(MessageError::Missing("Kind")),
        }
        let mut values: [Option<&str>; 6] = [None; 6];
        let mut last: Option<usize> = None;
        for (n, line) in lines {
            let Some((key, value)) = line.split_once(": ") else {
                return Err(malformed(n, "a trailer is `Name: value`"));
            };
            let Some(idx) = ORDER.iter().position(|k| *k == key) else {
                return Err(malformed(n, "an unknown trailer"));
            };
            if last.is_some_and(|prev| idx <= prev) {
                return Err(malformed(
                    n,
                    "each trailer appears once, in the fixed order",
                ));
            }
            last = Some(idx);
            if value.is_empty() || value.trim() != value {
                return Err(malformed(n, "a trailer value has no space at either end"));
            }
            if let Some(slot) = values.get_mut(idx) {
                *slot = Some(value);
            }
        }
        let [
            proposal,
            proposed_by,
            accepted_by,
            base,
            kind,
            imported_from,
        ] = values;
        let kind = kind.ok_or(MessageError::Missing("Kind"))?;
        let kind = Kind::parse(kind).ok_or(MessageError::Invalid {
            field: "Kind",
            rule: "proposal, rollback, import or system",
        })?;
        let proposed_by = Actor::parse(
            "Proposed-By",
            proposed_by.ok_or(MessageError::Missing("Proposed-By"))?,
        )?;
        let accepted_by = Actor::parse(
            "Accepted-By",
            accepted_by.ok_or(MessageError::Missing("Accepted-By"))?,
        )?;
        let proposal = proposal
            .map(|p| {
                ProposalId::parse(p).ok_or(MessageError::Invalid {
                    field: "Proposal",
                    rule: "a proposal id (36 lower-case hex-and-hyphen characters)",
                })
            })
            .transpose()?;
        let base = base.map(BlockId::new).transpose()?;
        let imported_from = imported_from.map(ImportSource::new).transpose()?;
        Self::checked(Message {
            kind,
            summary,
            proposal,
            proposed_by,
            accepted_by,
            base,
            imported_from,
        })
    }
}

#[cfg(test)]
mod tests;
