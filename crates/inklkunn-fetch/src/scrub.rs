//! Making `curl`'s standard error fit for an operator's eyes. `curl` does not print the request's secrets
//! with the options used here, but "does not today" is not a contract: the excerpt that reaches an error
//! text has every secret (and every piece of one) replaced, control characters dropped, and its length
//! bounded.

/// The most characters of `curl`'s standard error an error text carries.
const MAX_EXCERPT: usize = 200;
/// A secret's fragment shorter than this is not hunted for (it would blank ordinary words).
const MIN_FRAGMENT: usize = 4;

/// The first line of `raw` that says anything, with every secret in `secrets` — whole, and in pieces split at
/// whitespace, `&`, `=`, `:` and `,` — replaced by `[redacted]`, control characters removed and the length
/// bounded. `raw` is lossily read as UTF-8.
pub(crate) fn scrub(raw: &[u8], secrets: &[&str]) -> String {
    let text = String::from_utf8_lossy(raw);
    let mut needles: Vec<&str> = Vec::new();
    for secret in secrets {
        if secret.is_empty() {
            continue;
        }
        needles.push(secret);
        for piece in secret.split(|c: char| c.is_whitespace() || matches!(c, '&' | '=' | ':' | ','))
        {
            if piece.chars().count() >= MIN_FRAGMENT {
                needles.push(piece);
            }
        }
    }
    // Longest first, so a whole secret goes before its pieces.
    needles.sort_by_key(|n| std::cmp::Reverse(n.len()));
    needles.dedup();
    let mut line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned();
    for needle in needles {
        line = line.replace(needle, "[redacted]");
    }
    line.chars()
        .filter(|c| !c.is_control())
        .take(MAX_EXCERPT)
        .collect()
}

#[cfg(test)]
mod tests;
