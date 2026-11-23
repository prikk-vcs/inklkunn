//! The address of the form that proposes restoring a removed page, under the reserved prefix (`/<slug>/-/`), so that no title
//! can shadow it. Code, not words; scoped in `tests/literals.allow`.

use super::fields;
use crate::url::encode_title;

/// The address of the form for the removed page `title` in `view` (`None`: the default view).
pub(crate) fn path(slug: &str, title: &str, view: Option<&str>) -> String {
    let mut out = format!("/{slug}/-/restore/{}", encode_title(title));
    if let Some(view) = view {
        out.push('?');
        out.push_str(fields::VIEW);
        out.push('=');
        out.push_str(&inklkunn_markup::percent_encode(view));
    }
    out
}
