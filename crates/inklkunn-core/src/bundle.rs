//! The repository layout (external design CT-02; RFC 001 D-5): a Portable Wiki Bundle. `wiki-bundle.yaml`,
//! `pages/`, `attachments/` with `<name>.meta.yaml` sidecars, `knowledge/` and `governance/` (reserved;
//! schemas by RFCs 005 and 006), `.prikkignore` for the service's scratch paths. Nothing of inklkunn's
//! inside `.prikk/`.

use std::path::Path;

use inklkunn_store::WikiRecord;

/// The manifest file name.
pub const MANIFEST: &str = "wiki-bundle.yaml";
/// The directories a bundle has.
pub const DIRECTORIES: &[&str] = &["pages", "attachments", "knowledge", "governance"];
/// The keep-file that makes an empty directory exist in a repository of files.
pub const KEEP: &str = ".keep";
/// prikk's ignore file: literal path prefixes, one per line.
pub const PRIKKIGNORE: &str = ".prikkignore";
/// The first page of every wiki.
pub const HELP_PAGE: &str = "pages/Help.md";

/// The bundle manifest for a new wiki (wiki-commons chapter 10). `pages_prefix` is `pages`; fidelity is
/// level 3 with nothing degraded, since nothing was converted.
pub fn manifest(record: &WikiRecord, inklkunn_version: &str) -> String {
    let lang = record.languages.first().map_or("en", String::as_str);
    let langs = record.languages.join(", ");
    let lines = [
        "format: portable-wiki-bundle".to_owned(),
        "version: '0.4'".to_owned(),
        "generator:".to_owned(),
        "  name: inklkunn".to_owned(),
        format!("  version: {}", yaml_quote(inklkunn_version)),
        "source:".to_owned(),
        format!("  name: {}", yaml_quote(&record.display_name)),
        "  engine: inklkunn".to_owned(),
        format!("lang: {lang}"),
        format!("languages: [{langs}]"),
        format!("license: {}", record.default_license),
        format!("visibility_default: {}", record.visibility.as_str()),
        "markup:".to_owned(),
        "  profile: portable-wiki-markdown/2".to_owned(),
        "  link_label_order: target-first".to_owned(),
        "  hierarchy_separator: /".to_owned(),
        "  heading_anchor_algorithm: github".to_owned(),
        "  pages_prefix: pages".to_owned(),
        "  directives: [toc, children]".to_owned(),
        "  html_subset: []".to_owned(),
        "contents:".to_owned(),
        "  pages: 1".to_owned(),
        "  attachments: 0".to_owned(),
        "  history: true".to_owned(),
        "  discussions: false".to_owned(),
        "  users: false".to_owned(),
        "  structured: false".to_owned(),
        "fidelity:".to_owned(),
        "  level: 3".to_owned(),
        "  degraded: []".to_owned(),
        "  dropped: []".to_owned(),
        "  note: native; nothing was converted".to_owned(),
        "inklkunn:".to_owned(),
        format!("  default_view: {}", record.default_view),
        format!("  policy_bootstrap: {}", record.policy_bootstrap),
    ];
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Quote a YAML scalar when it needs it.
fn yaml_quote(s: &str) -> String {
    let plain = !s.is_empty()
        && !s.contains([
            ':', '#', '\'', '"', '\n', '[', ']', '{', '}', ',', '&', '*', '!', '|', '>', '%', '@',
            '`',
        ])
        && !s.starts_with(['-', '?', ' '])
        && !s.ends_with(' ');
    if plain {
        s.to_owned()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

/// The first help page: what a reader needs to start, in the profile's own constructs.
pub fn help_page(record: &WikiRecord) -> String {
    let lang = record.languages.first().map_or("en", String::as_str);
    let lines = [
        "---".to_owned(),
        "title: Help".to_owned(),
        "id: 019a0000-0000-7000-8000-0000000he1p0".to_owned(),
        "kind: page".to_owned(),
        "status: stable".to_owned(),
        format!("lang: {lang}"),
        "tags: [help]".to_owned(),
        "description: How this wiki works, in a page.".to_owned(),
        "---".to_owned(),
        String::new(),
        format!("## Welcome to {}", record.display_name),
        String::new(),
        "This wiki keeps every page in a signed, verifiable history. Anyone who may read can see what changed,".to_owned(),
        "when, and who proposed and accepted it.".to_owned(),
        String::new(),
        "## Reading".to_owned(),
        String::new(),
        "Pages link to each other with double brackets, like [[Help]]. A link to a page that does not exist yet".to_owned(),
        "is shown differently: it is an invitation to write it.".to_owned(),
        String::new(),
        "## Changing a page".to_owned(),
        String::new(),
        "A change is proposed, reviewed, and accepted. Until it is accepted it is *held for review*; once".to_owned(),
        "accepted it is *signed into the history* by the wiki service on behalf of the proposer and the acceptor.".to_owned(),
        String::new(),
        "## Trust".to_owned(),
        String::new(),
        "The wiki checks its own history and says one of three things about it: *Verified*, *Verification".to_owned(),
        "failed*, or *Could not be checked now*. It never claims more than it checked.".to_owned(),
        String::new(),
        "## License".to_owned(),
        String::new(),
        format!("Unless a page says otherwise, contributions are licensed {}.", record.default_license),
    ];
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// The ignore file: the service never commits its own scratch.
pub fn prikkignore() -> String {
    ".inklkunn/\n".to_owned()
}

/// Write the bundle files into `repo`'s worktree (the repository already exists).
pub fn write_layout(
    repo: &Path,
    record: &WikiRecord,
    inklkunn_version: &str,
) -> std::io::Result<()> {
    for d in DIRECTORIES {
        let dir = repo.join(d);
        std::fs::create_dir_all(&dir)?;
        if *d != "pages" {
            std::fs::write(dir.join(KEEP), b"")?;
        }
    }
    std::fs::write(repo.join(MANIFEST), manifest(record, inklkunn_version))?;
    std::fs::write(repo.join(PRIKKIGNORE), prikkignore())?;
    std::fs::write(repo.join(HELP_PAGE), help_page(record))?;
    Ok(())
}

/// Why a layout is not CT-02.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// A required entry is missing.
    Missing(String),
    /// Something of inklkunn's is inside `.prikk/`.
    InsidePrikk(String),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LayoutError::Missing(p) => write!(f, "the bundle lacks {p}"),
            LayoutError::InsidePrikk(p) => write!(
                f,
                "{p} is inside .prikk, where nothing of inklkunn's may be"
            ),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Check a worktree against CT-02.
pub fn check_layout(repo: &Path) -> Result<(), LayoutError> {
    for d in DIRECTORIES {
        if !repo.join(d).is_dir() {
            return Err(LayoutError::Missing(format!("{d}/")));
        }
    }
    for f in [MANIFEST, PRIKKIGNORE] {
        if !repo.join(f).is_file() {
            return Err(LayoutError::Missing(f.to_owned()));
        }
    }
    for name in ["inklkunn", ".inklkunn", MANIFEST] {
        if repo.join(".prikk").join(name).exists() {
            return Err(LayoutError::InsidePrikk(name.to_owned()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
