//! Title ↔ path (RFC 001 D-6; external design CT-01; threat model C-4a). A pure, deterministic function
//! from a page title to its file under `pages/`, and its best-effort inverse. **No input escapes
//! `pages/`**: components never contain a separator or a parent reference, and reserved names are
//! refused. Frontmatter's `title` wins over the inverse where both exist.

use unicode_normalization::UnicodeNormalization;

/// The directory every page lives under.
pub const PAGES_DIR: &str = "pages";
/// The page file extension.
pub const EXTENSION: &str = ".md";
/// The substitute for a character a file name cannot carry.
pub const SUBSTITUTE: char = '_';
/// Characters that common file systems forbid (XFER-3), replaced by [`SUBSTITUTE`].
pub const FORBIDDEN: &[char] = &['\\', ':', '*', '?', '"', '<', '>', '|'];
/// The longest a single path component may be, in bytes.
pub const MAX_COMPONENT_BYTES: usize = 200;

/// Why a title has no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// The title is empty, or a hierarchy component is (`a//b`, `/`).
    Empty,
    /// A component is `.` or `..`, or begins with `.prikk`.
    Reserved(String),
    /// A component is longer than [`MAX_COMPONENT_BYTES`] bytes.
    TooLong(String),
}

impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::Empty => write!(f, "a title or a hierarchy component is empty"),
            PathError::Reserved(c) => write!(f, "`{c}` is a reserved name"),
            PathError::TooLong(c) => write!(f, "`{c}` is too long for a file name"),
        }
    }
}

impl std::error::Error for PathError {}

/// The repository path of the page titled `title`: `pages/<components>.md`.
///
/// NFC-normalized; `/` is hierarchy; forbidden and control characters become `_`; a leading `.` is
/// replaced so no component is hidden; surrounding whitespace is trimmed per component.
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
        let component = sanitize_component(raw)?;
        out.push('/');
        out.push_str(&component);
    }
    out.push_str(EXTENSION);
    Ok(out)
}

fn sanitize_component(raw: &str) -> Result<String, PathError> {
    let t = raw.trim();
    if t.is_empty() {
        return Err(PathError::Empty);
    }
    if t == "." || t == ".." {
        return Err(PathError::Reserved(t.to_owned()));
    }
    let mut c: String = t
        .chars()
        .map(|ch| {
            if FORBIDDEN.contains(&ch) || ch.is_control() || ch == '\0' {
                SUBSTITUTE
            } else {
                ch
            }
        })
        .collect();
    if c.starts_with('.') {
        c.replace_range(..1, &SUBSTITUTE.to_string());
    }
    if c.eq_ignore_ascii_case("_prikk") || c.to_ascii_lowercase().starts_with(".prikk") {
        return Err(PathError::Reserved(c));
    }
    if c.len() > MAX_COMPONENT_BYTES {
        return Err(PathError::TooLong(c));
    }
    Ok(c)
}

/// The title a page path spells, for pages without a `title` in their frontmatter: `pages/` and `.md`
/// removed, the hierarchy kept. Substituted characters cannot be recovered; frontmatter wins when present.
pub fn path_to_title(path: &str) -> Option<String> {
    let rest = path.strip_prefix(PAGES_DIR)?.strip_prefix('/')?;
    let title = rest.strip_suffix(EXTENSION)?;
    if title.is_empty() {
        return None;
    }
    Some(title.to_owned())
}

/// `pages/a/b.md` → `pages/a/b-2.md`: the collision suffix XFER-3 prescribes, on the file name only.
pub fn with_suffix(path: &str, n: usize) -> String {
    match path.strip_suffix(EXTENSION) {
        Some(stem) => format!("{stem}-{n}{EXTENSION}"),
        None => format!("{path}-{n}"),
    }
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
        && !rest.contains('\0')
}

#[cfg(test)]
mod tests;
