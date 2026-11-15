//! Making `curl`'s standard error fit for an operator's eyes. `curl` does not print the request's secrets
//! with the options used here, but "does not today" is not a contract: the excerpt that reaches an error
//! text has every secret (and every piece of one) replaced, control characters, line separators and format
//! characters dropped, and its length bounded.

use inklkunn_prikk::sandbox::is_format_character;

/// The most characters of `curl`'s standard error an error text carries.
const MAX_EXCERPT: usize = 200;
/// A secret's fragment shorter than this (once flattened) is not hunted for (it would blank ordinary words).
const MIN_FRAGMENT: usize = 4;
/// What takes a hunted secret's place.
const REDACTED: &str = "[redacted]";

/// Is `c` a control character, or one that some line-oriented reader splits a line on (U+2028, U+2029)? The excerpt
/// is shown as one line, so neither may stay in it. This is `inklkunn_store::record::is_line_or_control`, the one
/// definition of the rule everywhere else; this crate depends on the driver alone (the boundary gate), so it cannot
/// call it and holds the same two-part test, which a test holds equal to the store's over every code point (the store
/// is a dev-dependency of this crate, for that test).
fn is_line_or_control(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// Is `c` dropped from the excerpt and from every secret before they are compared: a control, a line separator, or a
/// format character (a zero-width space inside a token is not seen, and the token is still the token)? The format
/// characters are the driver's one set, [`is_format_character`] (Unicode category Cf), which a test holds to cover
/// the set the knowledge files refuse (`inklkunn_signer::recover::text::is_direction_or_format`; this crate cannot
/// depend on the signer, so the signer is a dev-dependency for that test).
fn is_dropped(c: char) -> bool {
    is_line_or_control(c) || is_format_character(c)
}

/// `s` without what [`is_dropped`] names.
fn flattened(s: &str) -> String {
    s.chars().filter(|c| !is_dropped(*c)).collect()
}

/// The strings to hunt for, **flattened first and measured after**: each secret whole, and its pieces split at white
/// space, `&`, `=`, `:` and `,`. A piece (or a whole secret that flattening shortened) with fewer than
/// [`MIN_FRAGMENT`] characters left is not hunted: `o<control><control>r` is not the two-character needle `or`, which
/// would blank ordinary text. A secret that nothing is left of is not hunted either (it would match between every
/// two characters).
fn needles(secrets: &[&str]) -> Vec<Vec<char>> {
    let mut out: Vec<Vec<char>> = Vec::new();
    for secret in secrets {
        let whole = flattened(secret);
        let shrunk = whole.chars().count() != secret.chars().count();
        if !whole.is_empty() && (!shrunk || whole.chars().count() >= MIN_FRAGMENT) {
            out.push(whole.chars().collect());
        }
        for piece in secret.split(|c: char| c.is_whitespace() || matches!(c, '&' | '=' | ':' | ','))
        {
            let piece = flattened(piece);
            if piece.chars().count() >= MIN_FRAGMENT {
                out.push(piece.chars().collect());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The first line of `raw` that says anything, with line separators, controls and format characters removed from
/// the **whole text first**, then every secret in `secrets` — whole, and in pieces split at white space, `&`, `=`,
/// `:` and `,` — looked for in that flattened text and replaced by `[redacted]`, then the line chosen and the length
/// bounded. (Redacting first would let a secret split by U+2028 escape, and the filter would then join it back; a
/// secret split by a line break would show its first half on the line that is chosen.) A secret is looked for in
/// the same flattened form, and a match that runs over a line break hides its part of each line. `raw` is lossily
/// read as UTF-8.
pub(crate) fn scrub(raw: &[u8], secrets: &[&str]) -> String {
    let text = String::from_utf8_lossy(raw);
    // The flattened text as one run of characters, and where each of its lines begins and ends in that run (a line
    // is what `\n` ends; the other controls are dropped, not read as line ends).
    let mut flat: Vec<char> = Vec::new();
    let mut lines: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for c in text.chars() {
        if c == '\n' {
            lines.push((start, flat.len()));
            start = flat.len();
        } else if !is_dropped(c) {
            flat.push(c);
        }
    }
    lines.push((start, flat.len()));
    // Every place a needle occurs, overlapping ones too: the union is hidden.
    let mut hidden = vec![false; flat.len()];
    for needle in needles(secrets) {
        if needle.len() > flat.len() {
            continue;
        }
        for at in 0..=(flat.len() - needle.len()) {
            if flat.get(at..at + needle.len()) == Some(needle.as_slice())
                && let Some(span) = hidden.get_mut(at..at + needle.len())
            {
                span.fill(true);
            }
        }
    }
    // The first line that says anything (a line of white space alone does not).
    let Some(&(from, to)) = lines.iter().find(|(a, b)| {
        flat.get(*a..*b)
            .is_some_and(|l| l.iter().any(|c| !c.is_whitespace()))
    }) else {
        return String::new();
    };
    let is_hidden = |i: usize| hidden.get(i).copied().unwrap_or(false);
    let is_blank = |i: usize| flat.get(i).is_some_and(|c| c.is_whitespace());
    // Trimmed at both ends, but never into a hidden run.
    let mut a = from;
    while a < to && !is_hidden(a) && is_blank(a) {
        a += 1;
    }
    let mut b = to;
    while b > a && !is_hidden(b - 1) && is_blank(b - 1) {
        b -= 1;
    }
    let mut out = String::new();
    let mut in_run = false;
    for i in a..b {
        if is_hidden(i) {
            if !in_run {
                out.push_str(REDACTED);
                in_run = true;
            }
        } else {
            in_run = false;
            out.extend(flat.get(i));
        }
    }
    out.chars().take(MAX_EXCERPT).collect()
}

#[cfg(test)]
mod tests;
