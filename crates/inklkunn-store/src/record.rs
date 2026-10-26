//! The wiki record (DM-1) and its vocabulary.

use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The opaque wiki id: a UUID in the version-7 layout (time-ordered), rendered lower-case with hyphens. It
/// names the directory the repository lives in and never changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WikiId(String);

impl WikiId {
    /// A fresh id: 48 bits of Unix milliseconds, then version and variant bits, then 74 bits drawn from
    /// the standard library's randomly seeded hasher state (OS entropy, not a cryptographic generator;
    /// the id is an opaque name, not a secret, and the slug is unique by the store's rule).
    pub fn new() -> Self {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        let millis = u64::try_from(millis).unwrap_or(u64::MAX) & 0x0000_FFFF_FFFF_FFFF;
        let r1 = entropy();
        let r2 = entropy();
        let time_high = u32::try_from(millis >> 16).unwrap_or(0);
        let time_low = u16::try_from(millis & 0xFFFF).unwrap_or(0);
        let ver_rand = 0x7000 | u16::try_from(r1 & 0x0FFF).unwrap_or(0);
        let var_rand = 0x8000 | u16::try_from((r1 >> 16) & 0x3FFF).unwrap_or(0);
        let tail = r2 & 0x0000_FFFF_FFFF_FFFF;
        WikiId(format!(
            "{time_high:08x}-{time_low:04x}-{ver_rand:04x}-{var_rand:04x}-{tail:012x}"
        ))
    }

    /// Parse a stored id; refused unless it is 36 lower-case hex-and-hyphen characters in UUID layout.
    pub fn parse(s: &str) -> Option<Self> {
        let b = s.as_bytes();
        if b.len() != 36 {
            return None;
        }
        for (i, c) in b.iter().enumerate() {
            let ok = match i {
                8 | 13 | 18 | 23 => *c == b'-',
                _ => c.is_ascii_hexdigit() && !c.is_ascii_uppercase(),
            };
            if !ok {
                return None;
            }
        }
        Some(WikiId(s.to_owned()))
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

fn entropy() -> u64 {
    std::collections::hash_map::RandomState::new()
        .build_hasher()
        .finish()
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
        }
    }
}

impl std::error::Error for SlugError {}

/// Names no wiki may take: the service's own first path segments (external design WEB-01).
pub const RESERVED_SLUGS: &[&str] = &[
    "api",
    "admin",
    "static",
    "assets",
    "auth",
    "login",
    "logout",
    "new",
    "wikis",
    "health",
    "metrics",
    "w",
    "u",
    "users",
    "search",
    "about",
    "help",
    "docs",
    "robots.txt",
    "favicon.ico",
];

/// A URL slug: 1–64 of `a-z`, `0-9`, `-`; no leading, trailing or double hyphen; not reserved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    /// Validate a slug.
    pub fn new(s: &str) -> Result<Self, SlugError> {
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
        if RESERVED_SLUGS.contains(&s) {
            return Err(SlugError::Reserved);
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

/// Who may read (requirements ID-4 … ID-7; META-13 is the page-level advisory signal, this is the wiki's rule).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

/// The Assurance value a verification yields (external design, ID-9; planeter's vocabulary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationState {
    /// Never verified yet.
    Unknown,
    /// `verify` ran and every condition held.
    Verified,
    /// `verify` ran and a condition failed.
    Failed,
    /// `verify` could not run to completion (busy, ceiling, timeout): not a verdict.
    CouldNotCheck,
}

impl VerificationState {
    /// The stored name.
    pub fn as_str(self) -> &'static str {
        match self {
            VerificationState::Unknown => "unknown",
            VerificationState::Verified => "verified",
            VerificationState::Failed => "failed",
            VerificationState::CouldNotCheck => "could-not-check",
        }
    }

    /// Parse the stored name.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "unknown" => Some(VerificationState::Unknown),
            "verified" => Some(VerificationState::Verified),
            "failed" => Some(VerificationState::Failed),
            "could-not-check" => Some(VerificationState::CouldNotCheck),
            _ => None,
        }
    }
}

/// A verification outcome and when it was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// The outcome.
    pub state: VerificationState,
    /// Unix seconds, `None` while unknown.
    pub at: Option<i64>,
}

impl Default for Verification {
    fn default() -> Self {
        Verification {
            state: VerificationState::Unknown,
            at: None,
        }
    }
}

/// What a caller supplies to create a wiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewWiki {
    /// The URL slug.
    pub slug: Slug,
    /// The display name.
    pub display_name: String,
    /// The view readers see by default; `heads/main` when `None`.
    pub default_view: Option<String>,
    /// SPDX id; `CC-BY-SA-4.0` when `None` (roadmap DR-8).
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
    pub fn new(slug: Slug, display_name: impl Into<String>) -> Self {
        NewWiki {
            slug,
            display_name: display_name.into(),
            default_view: None,
            default_license: None,
            languages: Vec::new(),
            visibility: Visibility::Public,
            policy_bootstrap: None,
        }
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
    pub display_name: String,
    /// The default view.
    pub default_view: String,
    /// SPDX id of the default license.
    pub default_license: String,
    /// BCP 47 tags.
    pub languages: Vec<String>,
    /// Who may read.
    pub visibility: Visibility,
    /// The named policy bootstrap.
    pub policy_bootstrap: String,
    /// prikk's repository format version, once the repository exists.
    pub prikk_format_version: Option<String>,
    /// The latest verification.
    pub verification: Verification,
    /// The wiki's author key id (never a seed).
    pub author_key_id: Option<String>,
    /// The wiki's maintainer key id (never a seed).
    pub maintainer_key_id: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
}

impl WikiRecord {
    /// Build a record from a creation request with the defaults filled in.
    pub fn from_new(id: WikiId, new: NewWiki, created_at: i64) -> Self {
        WikiRecord {
            id,
            slug: new.slug,
            display_name: new.display_name,
            default_view: new.default_view.unwrap_or_else(|| "heads/main".to_owned()),
            default_license: new
                .default_license
                .unwrap_or_else(|| "CC-BY-SA-4.0".to_owned()),
            languages: if new.languages.is_empty() {
                vec!["en".to_owned()]
            } else {
                new.languages
            },
            visibility: new.visibility,
            policy_bootstrap: new.policy_bootstrap.unwrap_or_else(|| "default".to_owned()),
            prikk_format_version: None,
            verification: Verification::default(),
            author_key_id: None,
            maintainer_key_id: None,
            created_at,
        }
    }
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
