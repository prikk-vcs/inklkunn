//! Which characters a governance file may not hold, and how a text is written back.

/// Is `c` a character no governance text may hold? A control character, a bidirectional control or format
/// character (it can make a line read as another), a line or paragraph separator, or the byte-order mark.
pub(super) fn is_forbidden(c: char) -> bool {
    c.is_control()
        || inklkunn_signer::recover::text::is_direction_or_format(c)
        || matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}')
}

/// A text as a YAML double-quoted scalar the plain-subset parser reads back to the same text: quotes, backslashes
/// and control characters are escaped, the quote as `"`, because the parser's comment scanner toggles on a raw
/// quote character and an escaped one (`\"`) could hide a `#` from it.
pub(super) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\u0022"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if is_forbidden(c) => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
