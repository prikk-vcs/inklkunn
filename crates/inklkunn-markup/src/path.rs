//! Title ↔ path (RFC 001 D-6; external design CT-01; threat model C-4a). A pure, deterministic function
//! from a page title to its file under `pages/`, and its **exact** inverse.
//!
//! **Paths are ASCII** (prikk 0.54 refuses non-ASCII repository paths: "deferred until Unicode NFC
//! normalization is implemented", its `path-safety` reference; upstream ledger PK-21). Every character a
//! path cannot carry — anything non-ASCII, a control, `%` itself, and the characters file systems or
//! prikk forbid — is percent-encoded as UTF-8, so a title in any script has a path, two titles never share
//! one, and [`path_to_title`] recovers the (NFC, per-component trimmed) title byte for byte. The
//! frontmatter's `title` stays authoritative; readers see titles in URLs (RFC 003 D-1), never paths.
//!
//! **No input escapes `pages/`**: `/` is the hierarchy, components are never empty, `.` or `..`, a
//! leading `.` is encoded so no file is hidden, a trailing `.` is encoded (prikk refuses it), Windows
//! device names are encoded, and prikk's length caps (255 bytes per component, 1024 in all) are refused.

use unicode_normalization::UnicodeNormalization;

/// The directory every page lives under.
pub const PAGES_DIR: &str = "pages";
/// The page file extension.
pub const EXTENSION: &str = ".md";
/// ASCII characters a component carries as themselves (besides letters and digits).
pub const SAFE_PUNCTUATION: &str = " -_.(),'+=@~![]{}^";
/// The longest a single path component may be, in bytes (prikk's cap, `NAME_MAX`).
pub const MAX_COMPONENT_BYTES: usize = 255;
/// The longest a whole repository path may be, in bytes (prikk's cap, macOS `PATH_MAX`).
pub const MAX_PATH_BYTES: usize = 1024;

const DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why a title has no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The title is empty, or a hierarchy component is (`a//b`, `/`).
    Empty,
    /// A component is `.` or `..`.
    Reserved(String),
    /// A component is longer than [`MAX_COMPONENT_BYTES`] bytes once encoded.
    TooLong(String),
    /// The whole path is longer than [`MAX_PATH_BYTES`] bytes once encoded (too deep or too long a title).
    PathTooLong(usize),
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::Empty => write!(f, "a title or a hierarchy component is empty"),
            PathError::Reserved(c) => write!(f, "`{c}` is a reserved name"),
            PathError::TooLong(c) => write!(
                f,
                "`{c}` is longer than {MAX_COMPONENT_BYTES} bytes as a file name"
            ),
            PathError::PathTooLong(n) => write!(
                f,
                "the page path would be {n} bytes, more than the {MAX_PATH_BYTES} a repository path may have"
            ),
        }
    }
}

impl std::error::Error for PathError {}

/// The repository path of the page titled `title`: `pages/<components>.md`.
pub fn title_to_path(title: &str) -> Result<String, PathError> {
    let nfc: String = title.nfc().collect();
    let trimmed = nfc.trim();
    if trimmed.is_empty() {
        return Err(PathError::Empty);
    }
    let trimmed = trimmed.trim_matches('/');
    if trimmed.is_empty() {
        return Err(PathError::Empty);
    }
    let mut out = String::from(PAGES_DIR);
    for raw in trimmed.split('/') {
        let component = encode_component(raw)?;
        out.push('/');
        out.push_str(&component);
    }
    out.push_str(EXTENSION);
    if out.len() > MAX_PATH_BYTES {
        return Err(PathError::PathTooLong(out.len()));
    }
    Ok(out)
}

fn is_safe(c: char) -> bool {
    c.is_ascii_alphanumeric() || SAFE_PUNCTUATION.contains(c)
}

fn push_encoded(c: char, out: &mut String) {
    crate::text::percent_encode_char(c, out);
}

fn encode_component(raw: &str) -> Result<String, PathError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(PathError::Empty);
    }
    if t == "." || t == ".." {
        return Err(PathError::Reserved(t.to_owned()));
    }
    let chars: Vec<char> = t.chars().collect();
    let last = chars.len().saturating_sub(1);
    // A Windows device name is the stem before the first dot, case-insensitively.
    let stem: String = t.split('.').next().unwrap_or("").to_ascii_uppercase();
    let device = DEVICE_NAMES.contains(&stem.as_str());
    let mut out = String::with_capacity(t.len() + 8);
    for (i, c) in chars.iter().enumerate() {
        let must_encode =
            !is_safe(*c) || (i == 0 && (*c == '.' || device)) || (i == last && *c == '.');
        if must_encode {
            push_encoded(*c, &mut out);
        } else {
            out.push(*c);
        }
    }
    if out.len() > MAX_COMPONENT_BYTES {
        return Err(PathError::TooLong(t.to_owned()));
    }
    Ok(out)
}

/// The title a page path spells: `pages/` and `.md` removed, every component percent-decoded, the
/// hierarchy kept. `None` when the path is not under `pages/`, has no `.md`, or carries an encoding that
/// is not valid (`%` not followed by two hex digits, or bytes that are not UTF-8).
pub fn path_to_title(path: &str) -> Option<String> {
    let rest = path.strip_prefix(PAGES_DIR)?.strip_prefix('/')?;
    let encoded = rest.strip_suffix(EXTENSION)?;
    if encoded.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for component in encoded.split('/') {
        if component.is_empty() {
            return None;
        }
        parts.push(decode_component(component)?);
    }
    Some(parts.join("/"))
}

fn decode_component(c: &str) -> Option<String> {
    let bytes = c.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hi = bytes.get(i + 1).and_then(|h| (*h as char).to_digit(16))?;
            let lo = bytes.get(i + 2).and_then(|l| (*l as char).to_digit(16))?;
            out.push(u8::try_from(hi * 16 + lo).ok()?);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// `pages/a/b.md` → `pages/a/b-2.md`: the collision suffix XFER-3 prescribes, on the file name only.
pub fn with_suffix(path: &str, n: usize) -> String {
    match path.strip_suffix(EXTENSION) {
        Some(stem) => format!("{stem}-{n}{EXTENSION}"),
        None => format!("{path}-{n}"),
    }
}

/// The key two paths collide on a case-insensitive file system (prikk refuses such a pair in one path
/// set): the path lower-cased. A caller that writes two pages whose keys are equal gives the second a
/// [`with_suffix`] name.
pub fn collision_key(path: &str) -> String {
    path.to_ascii_lowercase()
}

/// Does `path` stay inside `pages/`? Every output of [`title_to_path`] does; this is the property the
/// fuzz test asserts and callers may re-check on paths read back from a repository.
pub fn is_inside_pages(path: &str) -> bool {
    let Some(rest) = path.strip_prefix("pages/") else {
        return false;
    };
    !rest.is_empty()
        && rest
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
        && rest.is_ascii()
        && !rest.bytes().any(|b| b < 0x20 || b == 0x7f)
}

#[cfg(test)]
mod tests;
