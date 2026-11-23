//! The address of the form that replaces a file the wiki cannot read, under the reserved prefix (`/<slug>/-/`). Code, not words;
//! scoped in `tests/literals.allow`.

use super::fields;

/// `/<slug>/-/health/knowledge-files`: the list the form is reached from, and goes back to.
pub(crate) fn files_that_cannot_be_read(slug: &str) -> String {
    format!("/{slug}/-/health/knowledge-files")
}

/// The identifier a knowledge file's name gives: its name without the extension.
pub(crate) fn name_of(file: &str) -> Option<&str> {
    file.rsplit('/').next()?.strip_suffix(".yaml")
}

/// `/<slug>/-/knowledge/replace?file=<the file's place>[&view=<name>]`.
pub(crate) fn replace(slug: &str, file: &str, view: Option<&str>) -> String {
    let mut out = format!(
        "/{slug}/-/knowledge/replace?{}={}",
        fields::FILE,
        inklkunn_markup::percent_encode(file)
    );
    if let Some(view) = view {
        out.push('&');
        out.push_str(fields::VIEW);
        out.push('=');
        out.push_str(&inklkunn_markup::percent_encode(view));
    }
    out
}
