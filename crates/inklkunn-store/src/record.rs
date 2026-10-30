//! The wiki record (DM-1) and its vocabulary. Every value a caller supplies is a validated type or is
//! validated by [`WikiRecord::from_new`]: nothing user-shaped reaches a path, a signed message or a YAML
//! file unchecked.

use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A fresh UUID in the version-7 layout, rendered lower-case with hyphens: 48 bits of Unix milliseconds,
/// the version and variant bits, then 74 random bits. The bits come from the operating system
/// (`/dev/urandom`); when that cannot be read, from the standard library's randomly seeded hasher state.
/// An id is an opaque name, not a secret.
pub fn uuid_v7() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let millis = u64::try_from(millis).unwrap_or(u64::MAX) & 0x0000_FFFF_FFFF_FFFF;
    let (r1, r2) = entropy();
    let time_high = u32::try_from(millis >> 16).unwrap_or(0);
    let time_low = u16::try_from(millis & 0xFFFF).unwrap_or(0);
    let ver_rand = 0x7000 | u16::try_from(r1 & 0x0FFF).unwrap_or(0);
    let var_rand = 0x8000 | u16::try_from((r1 >> 16) & 0x3FFF).unwrap_or(0);
    let tail = r2 & 0x0000_FFFF_FFFF_FFFF;
    format!("{time_high:08x}-{time_low:04x}-{ver_rand:04x}-{var_rand:04x}-{tail:012x}")
}

/// Two random words: the operating system's, else the hasher's.
fn entropy() -> (u64, u64) {
    use std::io::Read;
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let mut buf = [0u8; 16];
        if f.read_exact(&mut buf).is_ok() {
            let (a, b) = buf.split_at(8);
            let a = u64::from_le_bytes(a.try_into().unwrap_or([0; 8]));
            let b = u64::from_le_bytes(b.try_into().unwrap_or([0; 8]));
            if a != 0 || b != 0 {
                return (a, b);
            }
        }
    }
    let h = || {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    };
    (h(), h())
}

/// Is `s` 36 lower-case hex-and-hyphen characters in UUID layout?
fn is_uuid_shape(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => *c == b'-',
            _ => c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
        })
}

/// The opaque wiki id: a UUID in the version-7 layout (time-ordered), rendered lower-case with hyphens. It
/// names the directory the repository lives in and never changes. Deserialization goes through
/// [`WikiId::parse`], so a malformed id is refused wherever it arrives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WikiId(String);

impl WikiId {
    /// A fresh id.
    pub fn new() -> Self {
        WikiId(uuid_v7())
    }

    /// Parse a stored id; refused unless it is 36 lower-case hex-and-hyphen characters in UUID layout.
    pub fn parse(s: &str) -> Option<Self> {
        is_uuid_shape(s).then(|| WikiId(s.to_owned()))
    }

    /// The text form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The shard: the first two hex characters.
    pub fn shard(&self) -> &str {
        self.0.get(..2).unwrap_or("00")
    }
}

impl Default for WikiId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for WikiId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for WikiId {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        WikiId::parse(&s)
            .ok_or_else(|| "not a wiki id (36 lower-case hex-and-hyphen characters)".to_owned())
    }
}

impl From<WikiId> for String {
    fn from(id: WikiId) -> String {
        id.0
    }
}

/// Define an opaque UUID-v7 identifier with the same rules as [`WikiId`]: drawn fresh with `new`, accepted
/// back only through `parse` (36 lower-case hex-and-hyphen characters in UUID layout), and deserialized only
/// through that validation, so a malformed id is refused wherever it arrives and can never reach a path, a
/// signed message or a query unchecked.
macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// A fresh id.
            pub fn new() -> Self {
                $name(uuid_v7())
            }

            /// Parse a stored id; refused unless it is 36 lower-case hex-and-hyphen characters in UUID
            /// layout.
            pub fn parse(s: &str) -> Option<Self> {
                is_uuid_shape(s).then(|| $name(s.to_owned()))
            }

            /// The text form.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = String;
            fn try_from(s: String) -> Result<Self, String> {
                $name::parse(&s).ok_or_else(|| {
                    "not an id (36 lower-case hex-and-hyphen characters)".to_owned()
                })
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }
    };
}

uuid_id! {
    /// The opaque account id (RFC 002 D-1): stable for the life of the account, whatever its display name
    /// becomes. It is the person's name inside signed patch messages (CT-03), so it is an identifier and
    /// never a name, an address or anything a person chose.
    AccountId
}

uuid_id! {
    /// The opaque proposal id (RFC 004's records; named now because the signed patch message carries it).
    ProposalId
}

uuid_id! {
    /// The opaque id of an API token: what a listing and a revocation name. It is not the token, which is
    /// shown once at issue and kept only as a hash.
    TokenId
}

/// Why a slug was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlugError {
    /// Empty, or longer than 64 characters.
    Length,
    /// A character other than `a-z`, `0-9` or `-`, or a leading/trailing/double hyphen.
    Characters,
    /// One of the names the service keeps for its own routes.
    Reserved,
    /// Shaped like a wiki id, which a route accepting either must not be able to confuse.
    LooksLikeId,
}

impl fmt::Display for SlugError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SlugError::Length => write!(f, "a slug has 1 to 64 characters"),
            SlugError::Characters => write!(
                f,
                "a slug is lower-case letters, digits and single inner hyphens"
            ),
            SlugError::Reserved => write!(f, "that name is reserved for the service"),
            SlugError::LooksLikeId => write!(f, "a slug may not have the shape of a wiki id"),
        }
    }
}

impl std::error::Error for SlugError {}

/// Names no wiki may take: the service's own first path segments (external design WEB-01). RFC 003 D-1
/// puts the wiki's own surfaces under `-/`, which no slug can spell; this list keeps the instance-level
/// words that a reader would expect to mean the service.
pub const RESERVED_SLUGS: &[&str] = &[
    "api", "admin", "static", "assets", "auth", "login", "logout", "new", "wikis", "health",
    "metrics", "w", "u", "users", "search", "about", "help", "docs", "feed", "feeds", "sitemap",
    "robots",
];

/// A URL slug: 1–64 of `a-z`, `0-9`, `-`; no leading, trailing or double hyphen; not reserved; not shaped
/// like a wiki id. Deserialization goes through [`Slug::new`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    /// Validate a slug a caller supplies (creation and rename).
    pub fn new(s: &str) -> Result<Self, SlugError> {
        if RESERVED_SLUGS.contains(&s) {
            return Err(SlugError::Reserved);
        }
        let slug = Self::from_stored(s)?;
        if is_uuid_shape(s) {
            return Err(SlugError::LooksLikeId);
        }
        Ok(slug)
    }

    /// Read a slug back from a store: the character and length rules only. The reserved list is a rule
    /// for new names; a wiki that holds a name later added to it stays readable (and is reported by the
    /// store's own listing, not made to vanish).
    pub fn from_stored(s: &str) -> Result<Self, SlugError> {
        if s.is_empty() || s.len() > 64 {
            return Err(SlugError::Length);
        }
        if !s
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || s.starts_with('-')
            || s.ends_with('-')
            || s.contains("--")
        {
            return Err(SlugError::Characters);
        }
        Ok(Slug(s.to_owned()))
    }

    /// The text form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Slug {
    type Error = SlugError;
    fn try_from(s: String) -> Result<Self, SlugError> {
        Slug::new(&s)
    }
}

impl From<Slug> for String {
    fn from(s: Slug) -> String {
        s.0
    }
}

/// Why a display name was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayNameError {
    /// Empty, or longer than 120 characters.
    Length,
    /// A control character (a newline, a tab, an escape) is in the name.
    Control,
    /// Leading or trailing whitespace.
    Whitespace,
}

impl fmt::Display for DisplayNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayNameError::Length => write!(f, "a display name has 1 to 120 characters"),
            DisplayNameError::Control => {
                write!(f, "a display name has no control characters or line breaks")
            }
            DisplayNameError::Whitespace => {
                write!(f, "a display name does not start or end with whitespace")
            }
        }
    }
}

impl std::error::Error for DisplayNameError {}

/// A display name (a wiki's, and a person's in a signed message): one free-text field in any script
/// (RFC 002 D-1), 1–120 characters, no control characters or line breaks (U+2028 and U+2029 included),
/// trimmed. It enters the signed creation message's first line and the bundle manifest, so
/// the type is the guarantee that neither can be broken by a name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DisplayName(String);

impl DisplayName {
    /// Validate a display name.
    pub fn new(s: &str) -> Result<Self, DisplayNameError> {
        let n = s.chars().count();
        if n == 0 || n > 120 {
            return Err(DisplayNameError::Length);
        }
        // A line break is more than LF: U+2028 and U+2029 end a line for readers that split on them (and a
        // name reaches a signed message that anyone may read with any tool), so they are refused with the
        // control characters.
        if s.chars()
            .any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
        {
            return Err(DisplayNameError::Control);
        }
        if s.trim() != s {
            return Err(DisplayNameError::Whitespace);
        }
        Ok(DisplayName(s.to_owned()))
    }

    /// The text form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DisplayName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for DisplayName {
    type Error = DisplayNameError;
    fn try_from(s: String) -> Result<Self, DisplayNameError> {
        DisplayName::new(&s)
    }
}

impl From<DisplayName> for String {
    fn from(d: DisplayName) -> String {
        d.0
    }
}

/// Who may read (requirements ID-4 … ID-7; META-13 is the page-level advisory signal, this is the wiki's rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Anyone, signed in or not.
    Public,
    /// Signed-in accounts.
    Internal,
    /// Members only.
    Restricted,
}

impl Visibility {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::Internal => "internal",
            Visibility::Restricted => "restricted",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Visibility::Public),
            "internal" => Some(Visibility::Internal),
            "restricted" => Some(Visibility::Restricted),
            _ => None,
        }
    }
}

/// The Assurance value a verification **verdict** yields (external design, ID-9; planeter's vocabulary). Only a
/// check that ran to completion is a verdict: one that could not run is an [`Attempt`], kept apart so that it
/// can never replace what the last verdict said (handoff 003, review R1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationState {
    /// Never verified yet.
    Unknown,
    /// `verify` ran and every condition held.
    Verified,
    /// `verify` ran and a condition failed.
    Failed,
}

impl VerificationState {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            VerificationState::Unknown => "unknown",
            VerificationState::Verified => "verified",
            VerificationState::Failed => "failed",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unknown" => Some(VerificationState::Unknown),
            "verified" => Some(VerificationState::Verified),
            "failed" => Some(VerificationState::Failed),
            _ => None,
        }
    }
}

/// A verification verdict and when it was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// The verdict.
    pub state: VerificationState,
    /// Unix seconds, `None` while unknown.
    pub at: Option<i64>,
}

/// Why a verification could not run to completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NotRun {
    /// The server was busy with other work.
    Busy,
    /// Stopped for want of memory or time.
    Limit,
    /// Could not be started, or its answer could not be read.
    Unavailable,
}

impl NotRun {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            NotRun::Busy => "busy",
            NotRun::Limit => "limit",
            NotRun::Unavailable => "unavailable",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "busy" => Some(NotRun::Busy),
            "limit" => Some(NotRun::Limit),
            "unavailable" => Some(NotRun::Unavailable),
            _ => None,
        }
    }
}

/// A verification that could not run to completion: **not a verdict**, and never a replacement for one. A
/// wiki's record holds the latest such attempt only while it is the latest thing that happened; the next
/// verdict clears it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    /// Unix seconds.
    pub at: i64,
    /// Why it could not run.
    pub why: NotRun,
}

impl Default for Verification {
    fn default() -> Self {
        Verification {
            state: VerificationState::Unknown,
            at: None,
        }
    }
}

/// What a caller supplies to create a wiki. The free-text fields are validated by
/// [`WikiRecord::from_new`]; the typed ones by their types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWiki {
    /// The URL slug.
    pub slug: Slug,
    /// The display name.
    pub display_name: DisplayName,
    /// The view readers see by default; `heads/main` when `None`. Shape: `heads/<name>`.
    pub default_view: Option<String>,
    /// SPDX license expression; `CC-BY-SA-4.0` when `None` (roadmap DR-8).
    pub default_license: Option<String>,
    /// BCP 47 tags; `["en"]` when empty.
    pub languages: Vec<String>,
    /// Who may read.
    pub visibility: Visibility,
    /// The named policy bootstrap (RFC 006 fills the names); `default` when `None`.
    pub policy_bootstrap: Option<String>,
}

impl NewWiki {
    /// A public wiki with the defaults.
    pub fn new(slug: Slug, display_name: DisplayName) -> Self {
        NewWiki {
            slug,
            display_name,
            default_view: None,
            default_license: None,
            languages: Vec::new(),
            visibility: Visibility::Public,
            policy_bootstrap: None,
        }
    }
}

/// Why a creation request was refused before anything was stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewWikiError {
    /// `default_view` is not `heads/<name>` with a plain name.
    DefaultView,
    /// `default_license` is not an SPDX expression shape.
    License,
    /// A language tag is not a BCP 47 shape.
    Language(String),
    /// `policy_bootstrap` is not a plain lower-case name.
    PolicyBootstrap,
}

impl fmt::Display for NewWikiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NewWikiError::DefaultView => write!(
                f,
                "the default view is `heads/<name>`, the name in letters, digits, `.`, `_` and `-`"
            ),
            NewWikiError::License => write!(
                f,
                "the license is an SPDX expression (letters, digits, `.`, `+`, `-`, spaces and parentheses)"
            ),
            NewWikiError::Language(t) => write!(f, "`{t}` is not a BCP 47 language tag"),
            NewWikiError::PolicyBootstrap => write!(
                f,
                "the policy bootstrap is a plain name: lower-case letters, digits and hyphens"
            ),
        }
    }
}

impl std::error::Error for NewWikiError {}

fn is_view_name(s: &str) -> bool {
    let Some(name) = s.strip_prefix("heads/") else {
        return false;
    };
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['.', '-'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

fn is_spdx_shape(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.trim() == s
        && s.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-' | b'(' | b')' | b' ')
        })
}

fn is_bcp47_shape(s: &str) -> bool {
    let mut parts = s.split('-');
    let Some(primary) = parts.next() else {
        return false;
    };
    (2..=8).contains(&primary.len())
        && primary.bytes().all(|b| b.is_ascii_alphabetic())
        && parts.all(|p| (1..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn is_plain_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

/// The text `prikk --version` prints (`prikk 0.54.1`), as the service records it: trimmed, 1 to 64 printable
/// ASCII characters. It names the prikk a checkpoint record was refreshed under, so a prikk upgrade is seen
/// as the change of one value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PrikkVersion(String);

impl PrikkVersion {
    /// Validate a version text (surrounding whitespace is dropped); `None` when it is then empty, longer than
    /// 64 bytes or holds anything but printable ASCII (a version is read from a program's output and goes
    /// into a record and a log line).
    pub fn parse(s: &str) -> Option<Self> {
        let text = s.trim();
        let printable = text.bytes().all(|b| (0x20..0x7f).contains(&b));
        (printable && !text.is_empty() && text.len() <= 64).then(|| PrikkVersion(text.to_owned()))
    }

    /// The text form.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PrikkVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for PrikkVersion {
    type Error = String;
    fn try_from(s: String) -> Result<Self, String> {
        PrikkVersion::parse(&s).ok_or_else(|| "not a prikk version text".to_owned())
    }
}

impl From<PrikkVersion> for String {
    fn from(v: PrikkVersion) -> String {
        v.0
    }
}

/// What the service knows about prikk's **checkpoint record** for a wiki (handoff 003 hold note IS-9; ledger
/// PK-24). `extract` of a history beyond 63 blocks anchors on a record that only `seal` and a *writable*
/// `verify` write and that is keyed to the prikk version; after an upgrade, a restore or a cleared cache it
/// is gone, and `extract` (which builds the title index) refuses until it is refreshed.
///
/// Two facts, both set by someone else: the **last refresh's prikk version** (the binary compares it with the
/// prikk it runs at start) and a **request** a reader raises when it meets the refusal.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Checkpoint {
    /// The prikk the record was last refreshed under, `None` until a refresh has been recorded.
    pub refreshed_with: Option<PrikkVersion>,
    /// Unix seconds of the first unanswered request for a refresh, `None` when nobody has asked since the
    /// last one.
    pub refresh_requested_at: Option<i64>,
}

impl Checkpoint {
    /// Does the record need a refresh before the title index can be built under `running`: someone asked,
    /// or it was last refreshed under another prikk (or never)?
    pub fn needs_refresh(&self, running: &PrikkVersion) -> bool {
        self.refresh_requested_at.is_some() || self.refreshed_with.as_ref() != Some(running)
    }
}

/// The wiki record (DM-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiRecord {
    /// The opaque id; names the on-disk directory.
    pub id: WikiId,
    /// The URL slug.
    pub slug: Slug,
    /// The display name.
    pub display_name: DisplayName,
    /// The default view.
    pub default_view: String,
    /// SPDX expression of the default license.
    pub default_license: String,
    /// BCP 47 tags.
    pub languages: Vec<String>,
    /// Who may read.
    pub visibility: Visibility,
    /// The named policy bootstrap.
    pub policy_bootstrap: String,
    /// prikk's repository format version, once known.
    pub prikk_format_version: Option<String>,
    /// The last verification **verdict** (a check that ran to completion).
    pub verification: Verification,
    /// A later attempt that could not run, if the last thing that happened was that; the next verdict clears
    /// it. It never replaces `verification`.
    pub last_attempt: Option<Attempt>,
    /// The wiki's author key id (never a seed).
    pub author_key_id: Option<String>,
    /// The wiki's maintainer key id (never a seed).
    pub maintainer_key_id: Option<String>,
    /// prikk's checkpoint record for this wiki: when it was last refreshed and whether a reader asked.
    pub checkpoint: Checkpoint,
    /// Unix seconds.
    pub created_at: i64,
}

impl WikiRecord {
    /// Build a record from a creation request with the defaults filled in and the free-text fields
    /// validated.
    pub fn from_new(id: WikiId, new: NewWiki, created_at: i64) -> Result<Self, NewWikiError> {
        let default_view = new.default_view.unwrap_or_else(|| "heads/main".to_owned());
        if !is_view_name(&default_view) {
            return Err(NewWikiError::DefaultView);
        }
        let default_license = new
            .default_license
            .unwrap_or_else(|| "CC-BY-SA-4.0".to_owned());
        if !is_spdx_shape(&default_license) {
            return Err(NewWikiError::License);
        }
        let languages = if new.languages.is_empty() {
            vec!["en".to_owned()]
        } else {
            new.languages
        };
        if let Some(bad) = languages.iter().find(|t| !is_bcp47_shape(t)) {
            return Err(NewWikiError::Language(bad.clone()));
        }
        let policy_bootstrap = new.policy_bootstrap.unwrap_or_else(|| "default".to_owned());
        if !is_plain_name(&policy_bootstrap) {
            return Err(NewWikiError::PolicyBootstrap);
        }
        Ok(WikiRecord {
            id,
            slug: new.slug,
            display_name: new.display_name,
            default_view,
            default_license,
            languages,
            visibility: new.visibility,
            policy_bootstrap,
            prikk_format_version: None,
            verification: Verification::default(),
            last_attempt: None,
            author_key_id: None,
            maintainer_key_id: None,
            checkpoint: Checkpoint::default(),
            created_at,
        })
    }
}

/// `secs` (Unix seconds) as RFC 3339 in UTC, to the second: `2026-10-29T14:03:09Z` (proleptic Gregorian;
/// hand-written, so no date library enters the lock for it).
pub fn rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Unix seconds now.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
