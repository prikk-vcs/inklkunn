//! Reading and writing the knowledge files' YAML on the frontmatter's machinery
//! ([`inklkunn_markup::frontmatter`]): the plain subset (no anchors, no tags, no repeated keys), and nothing else
//! to trust. These are the small steps both files use to turn a [`Value`] into the format's own types, refusing
//! with a [`Fault`] that names a rule and a field of the format and never what the file said.

use inklkunn_markup::Value;

use super::{Fault, Rule};

/// The most characters of any text in a knowledge file (a note, a title, a citation).
pub const MAX_TEXT_CHARS: usize = 2_000;

/// The most expressions a claim may have.
pub const MAX_EXPRESSIONS: usize = 64;

/// The most relations to other claims, and the most sources, a claim may have.
pub const MAX_LINKS: usize = 256;

/// A mapping's pairs, in file order.
pub(super) type Pairs = [(String, Value)];

/// A value as a mapping whose keys are all in `allowed` and none repeated. A repeated key is refused here too: the
/// parser refuses it in a block mapping, and a flow mapping (`{a: 1, a: 2}`) reaches this point with both.
pub(super) fn mapping<'a>(
    value: &'a Value,
    field: &'static str,
    allowed: &[&str],
) -> Result<&'a Pairs, Fault> {
    let Value::Map(pairs) = value else {
        return Err(Fault::at(Rule::Type, field));
    };
    for (i, (key, _)) in pairs.iter().enumerate() {
        if !allowed.contains(&key.as_str()) {
            return Err(Fault::at(Rule::Unknown, field));
        }
        if pairs.iter().skip(i + 1).any(|(other, _)| other == key) {
            return Err(Fault::at(Rule::Syntax, field));
        }
    }
    Ok(pairs)
}

/// The value of `key`, if the mapping has it with something in it (`key:` and nothing is as good as absent).
pub(super) fn optional<'a>(pairs: &'a Pairs, key: &str) -> Option<&'a Value> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v)
        .filter(|v| !matches!(v, Value::Null))
}

/// The value of `key`, which the format requires.
pub(super) fn required<'a>(pairs: &'a Pairs, key: &'static str) -> Result<&'a Value, Fault> {
    optional(pairs, key).ok_or(Fault::at(Rule::Missing, key))
}

/// A text value: a string, or a date as the parser reads an unquoted one. A number is refused as a number: nothing
/// in a knowledge file is numeric.
pub(super) fn text(value: &Value, field: &'static str) -> Result<String, Fault> {
    match value {
        Value::Str(s) | Value::Date(s) => {
            if s.chars().count() > MAX_TEXT_CHARS {
                return Err(Fault::at(Rule::Bounds, field));
            }
            if s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
                return Err(Fault::at(Rule::Value, field));
            }
            Ok(s.clone())
        }
        Value::Int(_) | Value::Float(_) => Err(Fault::at(Rule::Number, field)),
        _ => Err(Fault::at(Rule::Type, field)),
    }
}

/// A text value that must be a word of a closed list.
pub(super) fn word<E>(
    value: &Value,
    field: &'static str,
    from_word: fn(&str) -> Option<E>,
) -> Result<E, Fault> {
    from_word(&text(value, field)?).ok_or(Fault::at(Rule::Value, field))
}

/// A list: a sequence, or nothing at all (an empty list). At most `max` entries.
pub(super) fn list<'a>(
    value: Option<&'a Value>,
    field: &'static str,
    max: usize,
) -> Result<&'a [Value], Fault> {
    match value {
        None => Ok(&[]),
        Some(Value::Seq(items)) => {
            if items.len() > max {
                Err(Fault::at(Rule::Bounds, field))
            } else {
                Ok(items)
            }
        }
        Some(_) => Err(Fault::at(Rule::Type, field)),
    }
}

/// `YYYY-MM-DD`, the way a day is written in a knowledge file (as text).
pub(super) fn is_day(text: &str) -> bool {
    let b = text.as_bytes();
    let digits = |from: usize, to: usize| {
        b.get(from..to)
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
    };
    if b.len() != 10 || !digits(0, 4) || !digits(5, 7) || !digits(8, 10) {
        return false;
    }
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
        return false;
    }
    let number = |from: usize, to: usize| {
        text.get(from..to)
            .and_then(|s| s.parse::<u8>().ok())
            .unwrap_or(0)
    };
    (1..=12).contains(&number(5, 7)) && (1..=31).contains(&number(8, 10))
}

// ---------------------------------------------------------------------------------------------- writing

/// A text as a YAML double-quoted scalar the plain-subset parser reads back to the same text. Quotes, backslashes
/// and control characters are escaped; the quote as `"`, because the parser's comment scanner toggles on a
/// raw quote character and an escaped one (`\"`) could hide a `#` from it.
pub(super) fn quoted(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\u0022"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{feff}') => {
                out.push_str(&format!("\\u{:04x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
