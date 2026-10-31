//! Where the knowledge files are, and reading one from its path and bytes.

use super::{Claim, ClaimId, Fault, Rule, Source, SourceId};

/// The top directory of the knowledge layer (RFC 001's reserved `knowledge/`).
pub const KNOWLEDGE_DIR: &str = "knowledge";

/// Where the claim files are.
pub const CLAIMS_DIR: &str = "knowledge/claims";

/// Where the source files are.
pub const SOURCES_DIR: &str = "knowledge/sources";

/// The most bytes a knowledge file may have (the bound the derived index reads under).
pub const MAX_FILE_BYTES: usize = 64 * 1024;

const EXTENSION: &str = ".yaml";

/// Which kind of knowledge file a path is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// `knowledge/claims/….yaml`.
    Claim,
    /// `knowledge/sources/….yaml`.
    Source,
}

/// Is `path` a keep-file ([`crate::bundle::KEEP`])? The one file a new wiki's `knowledge/` holds: it makes the empty
/// directory exist and is no knowledge file, so nothing reads it, judges it or lists it.
pub fn is_keep(path: &str) -> bool {
    path.rsplit('/').next() == Some(crate::bundle::KEEP)
}

/// Is `path` under the knowledge directory?
pub fn is_knowledge_path(path: &str) -> bool {
    path.strip_prefix(KNOWLEDGE_DIR)
        .is_some_and(|rest| rest.starts_with('/'))
}

/// The family a path belongs to by where it is and how it ends, whether or not its name is a well formed id:
/// the question *which kind of file is this meant to be* (the merge routes by it; the validator then judges the
/// name).
pub fn family_of(path: &str) -> Option<Family> {
    let name = |dir: &str| {
        path.strip_prefix(dir)?
            .strip_prefix('/')?
            .strip_suffix(EXTENSION)
    };
    if name(CLAIMS_DIR).is_some_and(|n| !n.contains('/')) {
        Some(Family::Claim)
    } else if name(SOURCES_DIR).is_some_and(|n| !n.contains('/')) {
        Some(Family::Source)
    } else {
        None
    }
}

/// The path of a claim's file.
pub fn claim_path(id: &ClaimId) -> String {
    format!("{CLAIMS_DIR}/{id}{EXTENSION}")
}

/// The path of a source's file.
pub fn source_path(id: &SourceId) -> String {
    format!("{SOURCES_DIR}/{id}{EXTENSION}")
}

/// A knowledge file, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum File {
    /// A claim.
    Claim(Box<Claim>),
    /// A source.
    Source(Box<Source>),
}

/// Read the knowledge file at `path` with these `bytes`: the path must be a claim's or a source's with a well
/// formed id, the bytes within [`MAX_FILE_BYTES`] and UTF-8, and the text the format. A file that is not wholly
/// well formed is a [`Fault`]; **no claim is half-read from it**.
pub fn read_file(path: &str, bytes: &[u8]) -> Result<File, Fault> {
    let family = family_of(path).ok_or(Fault::whole(Rule::Path))?;
    let name = path
        .rsplit('/')
        .next()
        .and_then(|n| n.strip_suffix(EXTENSION))
        .unwrap_or("");
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Fault::whole(Rule::TooLong));
    }
    let id_fault = Fault::whole(Rule::Path);
    let text = std::str::from_utf8(bytes).map_err(|_| Fault::whole(Rule::NotText))?;
    match family {
        Family::Claim => {
            let id = ClaimId::parse(name).ok_or(id_fault)?;
            Claim::parse(id, text).map(|c| File::Claim(Box::new(c)))
        }
        Family::Source => {
            let id = SourceId::parse(name).ok_or(id_fault)?;
            Source::parse(id, text).map(|s| File::Source(Box::new(s)))
        }
    }
}
