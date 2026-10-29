//! The URL scheme of a wiki's pages (RFC 003 D-1), in both directions.
//!
//! `/<slug>/<Title>` is a page in the wiki's default view and `/<slug>/~<view>/<Title>` a page in the view
//! `heads/<view>`. The title is percent-encoded UTF-8 and may hold `/` (a hierarchy). **The raw path decides
//! which of the two it is**, not the decoded one: a title that begins with `~` is written `%7E…`, so it can
//! never be taken for a view, and a view is always a literal `~`.

/// Decode `%XX` escapes; the bytes must be UTF-8. A `+` stays a plus (this is a path, not a form).
pub(crate) fn decode(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            out.push(value);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A title as it stands in an address: percent-encoded, with a leading `~` escaped so that it is never a view.
pub(crate) fn encode_title(title: &str) -> String {
    let encoded = inklkunn_markup::percent_encode(title);
    match encoded.strip_prefix('~') {
        Some(rest) => format!("%7E{rest}"),
        None => encoded,
    }
}

/// What the part of a path after the wiki's slug asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PageAddress {
    /// The view's name as the address spells it (`main` for `heads/main`), if the address names one.
    pub view: Option<String>,
    /// The title, decoded.
    pub title: String,
}

/// The view named in an address becomes the view of the repository (`heads/<name>`); only a plain name does.
pub(crate) fn view_ref(name: &str) -> Option<String> {
    let plain = !name.is_empty()
        && name.len() <= 100
        && !name.starts_with(['.', '-'])
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    plain.then(|| format!("heads/{name}"))
}

/// Read the raw path after `/<slug>/` (`rest`, not decoded). `None` when it cannot be decoded as UTF-8.
pub(crate) fn page_address(rest: &str) -> Option<PageAddress> {
    match rest.strip_prefix('~') {
        Some(after) => {
            let (view, title) = after.split_once('/').unwrap_or((after, ""));
            Some(PageAddress {
                view: Some(decode(view)?),
                title: decode(title)?,
            })
        }
        None => Some(PageAddress {
            view: None,
            title: decode(rest)?,
        }),
    }
}

// ------------------------------------------------------------------------------------ a page's past

/// The part of an address after `?` that names the view and the place in a history, when they are not the
/// defaults. A view's name is a plain name (`view_ref`), so it needs no escaping.
fn past_query(view: Option<&str>, after: Option<u64>) -> String {
    let mut parts = Vec::new();
    if let Some(view) = view {
        parts.push(format!("view={view}"));
    }
    if let Some(after) = after {
        parts.push(format!("after={after}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("?{}", parts.join("&"))
    }
}

/// `/<slug>/-/history/<Title>`: a page's history, from the change after `after` when given.
pub(crate) fn history_path(
    slug: &str,
    view: Option<&str>,
    title: &str,
    after: Option<u64>,
) -> String {
    format!(
        "/{slug}/-/history/{}{}",
        encode_title(title),
        past_query(view, after)
    )
}

/// `/<slug>/-/change/<block>`: one change.
pub(crate) fn change_path(slug: &str, view: Option<&str>, block: &str) -> String {
    format!("/{slug}/-/change/{block}{}", past_query(view, None))
}

/// `/<slug>/-/diff/<from>/<to>/<Title>`: what changed on a page between two changes.
pub(crate) fn diff_path(
    slug: &str,
    view: Option<&str>,
    from: &str,
    to: &str,
    title: &str,
) -> String {
    format!(
        "/{slug}/-/diff/{from}/{to}/{}{}",
        encode_title(title),
        past_query(view, None)
    )
}

/// `/<slug>/-/at/<block>/<Title>`: a page as it was after a change (a permalink).
pub(crate) fn at_path(slug: &str, view: Option<&str>, block: &str, title: &str) -> String {
    format!(
        "/{slug}/-/at/{block}/{}{}",
        encode_title(title),
        past_query(view, None)
    )
}

#[cfg(test)]
mod tests;
