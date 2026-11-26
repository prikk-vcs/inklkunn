//! Licences on import (RFC 007 D-4; wiki-commons 0.4 chapter 15, LIC-3 – LIC-5): which licence a wiki may have as its default,
//! and which files may go into a wiki that has it. **A licence is never rewritten**; this module only judges.
//!
//! The matrix is wiki-commons' simplified guide (LIC-5), as RFC 007 D-4 states it:
//!
//! | file's licence | goes into a wiki whose default is |
//! |---|---|
//! | CC0 | CC0, CC BY, CC BY-SA |
//! | CC BY | CC BY, CC BY-SA |
//! | CC BY-SA | CC BY-SA |
//! | NC, ND, GFDL, anything else, or no valid licence | none |
//!
//! A licence is read by its SPDX identifier: `CC0-1.0`, `CC-BY-<version>` and `CC-BY-SA-<version>` are the families that a
//! wiki may have. The comparison is by family, not by version: BY-SA 3.0 and BY-SA 4.0 are both *BY-SA* here (the page keeps
//! its own `license` line, so the version it carries is never lost). A licence that is not one of these is *unknown*, and
//! so is a name that merely looks like one (`CC-BY-NC-4.0`, `cc0`, `CC-BY-SA-4.0 OR MIT`).

/// The family a licence is judged as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Family {
    /// `CC0-1.0`.
    Cc0,
    /// `CC-BY-<version>`.
    By,
    /// `CC-BY-SA-<version>`.
    BySa,
    /// Anything else: NonCommercial, NoDerivatives, GFDL, a licence this table does not know, text that is not a licence.
    Unknown,
}

/// The versions of the Creative Commons licences that carry the identifiers `CC-BY-<v>` and `CC-BY-SA-<v>`.
const VERSIONS: &[&str] = &["1.0", "2.0", "2.5", "3.0", "4.0"];

/// The family of the SPDX identifier `id`.
pub fn family(id: &str) -> Family {
    if id == "CC0-1.0" {
        return Family::Cc0;
    }
    if let Some(version) = id.strip_prefix("CC-BY-SA-") {
        return if VERSIONS.contains(&version) {
            Family::BySa
        } else {
            Family::Unknown
        };
    }
    if let Some(version) = id.strip_prefix("CC-BY-") {
        return if VERSIONS.contains(&version) {
            Family::By
        } else {
            Family::Unknown
        };
    }
    Family::Unknown
}

/// May a wiki have the licence `id` as its default? Only if something may go into it: CC0, CC BY and CC BY-SA.
pub fn may_be_a_default(id: &str) -> bool {
    family(id) != Family::Unknown
}

/// May a file under the licence `file` go into a wiki whose default licence is `wiki`?
pub fn goes_into(file: &str, wiki: &str) -> bool {
    match (family(file), family(wiki)) {
        (Family::Unknown, _) | (_, Family::Unknown) => false,
        (Family::Cc0, _) => true,
        (Family::By, Family::By | Family::BySa) => true,
        (Family::By, Family::Cc0) => false,
        (Family::BySa, Family::BySa) => true,
        (Family::BySa, Family::By | Family::Cc0) => false,
    }
}

/// The SPDX identifier a name that is **not** one probably means (`CC0` is `CC0-1.0`, `CC-BY-SA` is `CC-BY-SA-4.0`, `cc by 3.0` is
/// `CC-BY-3.0`), or `None` when the name is one the matrix knows, or is not close to any it knows (NonCommercial and NoDerivatives
/// names, `GFDL`, text that is not a licence). **A name is never taken for the id it resembles**: the answer is only for the sentence
/// that refuses it (*not an SPDX id; did you mean CC0-1.0?*), because a licence is never changed on the way in. Without a version the
/// current one is suggested (4.0).
pub fn did_you_mean(name: &str) -> Option<String> {
    if family(name) != Family::Unknown {
        return None;
    }
    let loud = name.trim().to_ascii_uppercase().replace([' ', '_'], "-");
    let parts: Vec<&str> = loud.split('-').filter(|p| !p.is_empty()).collect();
    let (first, rest) = parts.split_first()?;
    // `CC0`, `CC0-1`, `CC0-1.0`, `CC-0`, `CC-0-1.0`.
    if *first == "CC0" || (*first == "CC" && rest.first() == Some(&"0")) {
        let version = rest.iter().find(|p| p.contains('.') || **p == "1");
        return matches!(version, None | Some(&"1" | &"1.0")).then(|| "CC0-1.0".to_owned());
    }
    // `CC-BY`, `CC-BY-4`, `CC-BY-SA-3.0`, `CCBY`, with no NonCommercial or NoDerivatives part.
    let mut tokens: Vec<&str> = Vec::new();
    if *first == "CC" {
        tokens.extend(rest);
    } else if let Some(after) = first.strip_prefix("CC") {
        tokens.push(after);
        tokens.extend(rest);
    } else {
        return None;
    }
    let mut it = tokens.into_iter().peekable();
    if it.next()? != "BY" {
        return None;
    }
    let share_alike = it.next_if_eq(&"SA").is_some();
    let version = match it.next() {
        None => "4.0",
        Some(v) => VERSIONS
            .iter()
            .find(|known| **known == v || known.strip_suffix(".0") == Some(v))?,
    };
    it.next()
        .is_none()
        .then(|| format!("CC-BY{}-{version}", if share_alike { "-SA" } else { "" }))
}

/// How a refusal says what a licence name that is not an identifier probably meant, or nothing when it is not close to one.
pub fn hint(name: &str) -> String {
    did_you_mean(name).map_or_else(String::new, |id| {
        format!(" (not an SPDX id; did you mean {id}?)")
    })
}

/// Is `text` the shape of an SPDX expression that a wiki record accepts (the same shape its creation checks: at most 128
/// characters of letters, digits, `.`, `+`, `-`, `(`, `)` and spaces, with no space at either end)?
pub fn has_the_shape_of_an_id(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text.trim() == text
        && text.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-' | b'(' | b')' | b' ')
        })
}
