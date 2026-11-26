//! What an import says about the bundle it came from, on every change it makes (RFC 007 D-3, LIC-4's *imported from*; handoff
//! I-T3): the facts that a wiki's notice (*Imported from {source} on {date}*) is made of, as typed trailers, so that the notice is
//! read from the same messages the history is and needs nothing else to be kept.
//!
//! ```text
//! Imported-On: <RFC 3339 in UTC to the second: the day the import ran, by this service's own clock>
//! Imported-Source-Name: <the name the bundle's manifest gives its source, one line>      (when it gives one)
//! Imported-Source-Address: <the address the manifest gives, one line, as text>           (when it gives one)
//! Imported-Checks: <verified | none>          (were the bundle's checksums verified, or were there none)
//! Imported-License: <manifest | declared-by-admin>      (whose word the wiki's licence is)
//! ```
//!
//! They follow `Imported-From`, once each, in this order, **on every change an import of a bundle makes** (the first change, each
//! replayed revision, the change at the end). The name and the address are the source's: **text, never a link**, made one line and
//! bounded; nothing reads them as an account or a place to go.

use inklkunn_store::record::rfc3339;

use super::MessageError;
use super::source::{SOURCE_NAME_MAX_CHARS, one_line, parse_at};

/// The longest source address, in characters.
pub const SOURCE_ADDRESS_MAX_CHARS: usize = 300;

/// Were the bundle's checksums verified?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChecksState {
    /// The manifest named a file of checksums and every file matched.
    Verified,
    /// The bundle had none: the import said so and went ahead (SEC-7: *when present*).
    None,
}

impl ChecksState {
    /// The trailer's word.
    pub const fn as_str(self) -> &'static str {
        match self {
            ChecksState::Verified => "verified",
            ChecksState::None => "none",
        }
    }
}

/// Whose word the wiki's licence is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LicenseWord {
    /// The bundle's manifest declared it.
    Manifest,
    /// The bundle declared none, and the admin who ran the import did (`--bundle-license`).
    DeclaredByAdmin,
}

impl LicenseWord {
    /// The trailer's word.
    pub const fn as_str(self) -> &'static str {
        match self {
            LicenseWord::Manifest => "manifest",
            LicenseWord::DeclaredByAdmin => "declared-by-admin",
        }
    }
}

/// The facts of an import of a bundle that a notice says (see the module).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportNotice {
    on: i64,
    source_name: Option<String>,
    source_address: Option<String>,
    checks: ChecksState,
    license: LicenseWord,
}

impl ImportNotice {
    /// The import that ran at `on` (Unix seconds, UTC), with the bundle's checksums and licence as they were.
    pub fn new(on: i64, checks: ChecksState, license: LicenseWord) -> Result<Self, MessageError> {
        // The text of a date this message can write and read back.
        if !(-62_135_596_800..=253_402_300_799).contains(&on) {
            return Err(MessageError::Invalid {
                field: "Imported-On",
                rule: "a time between the years 0001 and 9999",
            });
        }
        Ok(ImportNotice {
            on,
            source_name: None,
            source_address: None,
            checks,
            license,
        })
    }

    /// The name the bundle's manifest gives its source, made one line and bounded (nothing left is none).
    #[must_use]
    pub fn with_source_name(mut self, name: &str) -> Self {
        self.source_name = one_line(name, SOURCE_NAME_MAX_CHARS);
        self
    }

    /// The address the manifest gives its source, made one line and bounded. **Text**: nothing visits it.
    #[must_use]
    pub fn with_source_address(mut self, address: &str) -> Self {
        self.source_address = one_line(address, SOURCE_ADDRESS_MAX_CHARS);
        self
    }

    /// The day the import ran, in Unix seconds (UTC).
    pub fn on(&self) -> i64 {
        self.on
    }

    /// The source's name, as text.
    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    /// The source's address, as text.
    pub fn source_address(&self) -> Option<&str> {
        self.source_address.as_deref()
    }

    /// Were the checksums verified?
    pub fn checks(&self) -> ChecksState {
        self.checks
    }

    /// Whose word the licence is.
    pub fn license(&self) -> LicenseWord {
        self.license
    }

    /// The trailer lines, `(name, value)` in the order the message writes them.
    pub(super) fn trailers(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![(super::ORDER[super::IMPORTED_ON], rfc3339(self.on))];
        if let Some(n) = &self.source_name {
            out.push((super::ORDER[super::IMPORTED_SOURCE_NAME], n.clone()));
        }
        if let Some(a) = &self.source_address {
            out.push((super::ORDER[super::IMPORTED_SOURCE_ADDRESS], a.clone()));
        }
        out.push((
            super::ORDER[super::IMPORTED_CHECKS],
            self.checks.as_str().to_owned(),
        ));
        out.push((
            super::ORDER[super::IMPORTED_LICENSE],
            self.license.as_str().to_owned(),
        ));
        out
    }

    /// Read the trailers' values (the slots of [`super::ORDER`] from `Imported-On` to `Imported-License`), strictly.
    pub(super) fn parse(values: &[Option<&str>]) -> Result<Option<Self>, MessageError> {
        let get = |i: usize| values.get(i - super::IMPORTED_ON).copied().flatten();
        if values.iter().all(Option::is_none) {
            return Ok(None);
        }
        let on = get(super::IMPORTED_ON).ok_or(MessageError::Missing("Imported-On"))?;
        let on = parse_at(on).map_err(|_| MessageError::Invalid {
            field: "Imported-On",
            rule: "RFC 3339 in UTC to the second, like 1999-12-31T23:59:59Z",
        })?;
        let checks = match get(super::IMPORTED_CHECKS) {
            Some("verified") => ChecksState::Verified,
            Some("none") => ChecksState::None,
            Some(_) => {
                return Err(MessageError::Invalid {
                    field: "Imported-Checks",
                    rule: "verified or none",
                });
            }
            None => return Err(MessageError::Missing("Imported-Checks")),
        };
        let license = match get(super::IMPORTED_LICENSE) {
            Some("manifest") => LicenseWord::Manifest,
            Some("declared-by-admin") => LicenseWord::DeclaredByAdmin,
            Some(_) => {
                return Err(MessageError::Invalid {
                    field: "Imported-License",
                    rule: "manifest or declared-by-admin",
                });
            }
            None => return Err(MessageError::Missing("Imported-License")),
        };
        let mut out = ImportNotice::new(on, checks, license)?;
        for (slot, field, max) in [
            (
                super::IMPORTED_SOURCE_NAME,
                "Imported-Source-Name",
                SOURCE_NAME_MAX_CHARS,
            ),
            (
                super::IMPORTED_SOURCE_ADDRESS,
                "Imported-Source-Address",
                SOURCE_ADDRESS_MAX_CHARS,
            ),
        ] {
            if let Some(value) = get(slot) {
                // Only a text that is already one line and bounded is canonical: what was written is what is read back.
                if one_line(value, max).as_deref() != Some(value) {
                    return Err(MessageError::Invalid {
                        field,
                        rule: "a text of one line, within its bound",
                    });
                }
                if slot == super::IMPORTED_SOURCE_NAME {
                    out.source_name = Some(value.to_owned());
                } else {
                    out.source_address = Some(value.to_owned());
                }
            }
        }
        Ok(Some(out))
    }
}

#[cfg(test)]
mod tests;
