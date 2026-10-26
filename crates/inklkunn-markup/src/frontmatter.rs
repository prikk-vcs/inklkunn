//! Frontmatter in the plain YAML subset of META-1: scalars, sequences and mappings; no anchors, no
//! tags, no multi-document streams. The parser is deliberately small and strict about the things the
//! subset excludes (an anchor or a tag is a refusal, threat model C-4b), and deliberately faithful
//! to YAML's coercions where they are a known pitfall: an unquoted `2026-06-01` is a [`Value::Date`],
//! an unquoted `no` is `false`, so that the pitfall is visible rather than silently flattened
//! (wiki-commons corpus case 013).

use serde_json::json;

/// A frontmatter value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `null`, `~` or an empty value.
    Null,
    /// `true`/`false`, `yes`/`no`, `on`/`off`, in any case.
    Bool(bool),
    /// A decimal integer.
    Int(i64),
    /// A decimal float.
    Float(f64),
    /// A string, quoted or plain.
    Str(String),
    /// An unquoted ISO date or datetime, kept as text so the coercion is visible.
    Date(String),
    /// A sequence (block `- a` or flow `[a, b]`).
    Seq(Vec<Value>),
    /// A mapping, in source order.
    Map(Vec<(String, Value)>),
}

impl Value {
    /// The JSON shape the wiki-commons corpus uses for comparison: a date becomes
    /// `{"$type": "date", "iso": "…"}`.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => json!(b),
            Value::Int(i) => json!(i),
            Value::Float(f) => json!(f),
            Value::Str(s) => json!(s),
            Value::Date(d) => json!({"$type": "date", "iso": d}),
            Value::Seq(v) => serde_json::Value::Array(v.iter().map(Value::to_json).collect()),
            Value::Map(m) => {
                let mut o = serde_json::Map::new();
                for (k, v) in m {
                    o.insert(k.clone(), v.to_json());
                }
                serde_json::Value::Object(o)
            }
        }
    }

    /// Look a key up in a mapping.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The value as a string, when it is one (dates count as their text).
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) | Value::Date(s) => Some(s),
            _ => None,
        }
    }

    /// A list of strings: a sequence of strings, or one string split on commas (META-5 tolerates both).
    pub fn as_string_list(&self) -> Vec<String> {
        match self {
            Value::Seq(items) => items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect(),
            Value::Str(s) => s
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// Why frontmatter was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YamlError {
    /// An anchor (`&`), alias (`*`) or tag (`!`) — outside the plain subset.
    OutsideSubset {
        /// 1-based line within the frontmatter.
        line: usize,
        /// What was found.
        what: &'static str,
    },
    /// A line that is neither a key, a sequence item nor a continuation.
    Malformed {
        /// 1-based line within the frontmatter.
        line: usize,
    },
    /// The same key twice in one mapping.
    DuplicateKey {
        /// 1-based line within the frontmatter.
        line: usize,
        /// The repeated key.
        key: String,
    },
    /// Indentation that does not match any open block.
    Indentation {
        /// 1-based line within the frontmatter.
        line: usize,
    },
    /// A quoted string that never closes.
    UnterminatedQuote {
        /// 1-based line within the frontmatter.
        line: usize,
    },
}

impl std::fmt::Display for YamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YamlError::OutsideSubset { line, what } => {
                write!(f, "line {line}: {what} is outside the plain YAML subset")
            }
            YamlError::Malformed { line } => write!(f, "line {line}: malformed"),
            YamlError::DuplicateKey { line, key } => {
                write!(f, "line {line}: duplicate key `{key}`")
            }
            YamlError::Indentation { line } => write!(f, "line {line}: bad indentation"),
            YamlError::UnterminatedQuote { line } => write!(f, "line {line}: unterminated quote"),
        }
    }
}

impl std::error::Error for YamlError {}

/// Split a page into its frontmatter (the text between the opening and closing `---` lines, when the
/// file begins with one), its body, and the 1-based line the body starts on.
pub fn split(text: &str) -> (Option<&str>, &str, usize) {
    let mut lines = text.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return (None, text, 1);
    };
    if first.trim_end() != "---" {
        return (None, text, 1);
    }
    let mut offset = first.len();
    let mut line_no = 1;
    let yaml_start = offset;
    for line in lines {
        line_no += 1;
        if line.trim_end() == "---" || line.trim_end() == "..." {
            let yaml = text.get(yaml_start..offset).unwrap_or("");
            let body_start = offset + line.len();
            return (
                Some(yaml),
                text.get(body_start..).unwrap_or(""),
                line_no + 1,
            );
        }
        offset += line.len();
    }
    // An opening fence with no closing one is not frontmatter: the page is body only.
    (None, text, 1)
}

struct Line<'a> {
    no: usize,
    indent: usize,
    text: &'a str,
}

/// Parse a YAML text in the plain subset into a [`Value`] (a mapping at the top level, or a scalar or
/// sequence when the text is one).
pub fn parse_yaml(yaml: &str) -> Result<Value, YamlError> {
    let mut lines: Vec<Line<'_>> = Vec::new();
    for (i, raw) in yaml.lines().enumerate() {
        let no = i + 1;
        let without_comment = strip_comment(raw);
        if without_comment.trim().is_empty() {
            continue;
        }
        let indent = without_comment.len() - without_comment.trim_start().len();
        lines.push(Line {
            no,
            indent,
            text: without_comment.trim_end(),
        });
    }
    if lines.is_empty() {
        return Ok(Value::Map(Vec::new()));
    }
    let mut pos = 0;
    let v = parse_block(&lines, &mut pos, lines.first().map_or(0, |l| l.indent))?;
    if pos < lines.len() {
        return Err(YamlError::Malformed {
            line: lines.get(pos).map_or(0, |l| l.no),
        });
    }
    Ok(v)
}

/// Remove a `#` comment that starts the line or follows whitespace, outside quotes.
fn strip_comment(line: &str) -> &str {
    let mut in_single = false;
    let mut in_double = false;
    let mut prev_space = true;
    let mut cut = line.len();
    for (i, c) in line.char_indices() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '#' if !in_single && !in_double && prev_space => {
                cut = i;
                break;
            }
            _ => {}
        }
        prev_space = c == ' ' || c == '\t';
    }
    line.get(..cut).unwrap_or(line)
}

fn parse_block(lines: &[Line<'_>], pos: &mut usize, indent: usize) -> Result<Value, YamlError> {
    let Some(first) = lines.get(*pos) else {
        return Ok(Value::Null);
    };
    if first.indent != indent {
        return Err(YamlError::Indentation { line: first.no });
    }
    let body = first.text.trim_start();
    if body == "-" || body.starts_with("- ") {
        parse_sequence(lines, pos, indent)
    } else if key_split(body).is_some() {
        parse_mapping(lines, pos, indent)
    } else {
        // A lone scalar at the top level.
        *pos += 1;
        parse_scalar(body, first.no)
    }
}

fn parse_sequence(lines: &[Line<'_>], pos: &mut usize, indent: usize) -> Result<Value, YamlError> {
    let mut items = Vec::new();
    while let Some(line) = lines.get(*pos) {
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(YamlError::Indentation { line: line.no });
        }
        let body = line.text.trim_start();
        let Some(rest) = body.strip_prefix('-') else {
            break;
        };
        let rest = rest.trim_start();
        *pos += 1;
        if rest.is_empty() {
            // The item is the nested block on the following lines.
            match lines.get(*pos) {
                Some(next) if next.indent > indent => {
                    items.push(parse_block(lines, pos, next.indent)?);
                }
                _ => items.push(Value::Null),
            }
        } else if let Some((k, v)) = key_split(rest) {
            // `- key: value` begins an inline mapping whose further keys are indented past the dash.
            let item_indent = indent + 2;
            let mut map = vec![(
                unquote_key(k),
                inline_value(lines, pos, v, line.no, item_indent)?,
            )];
            while let Some(next) = lines.get(*pos) {
                if next.indent != item_indent {
                    break;
                }
                let nb = next.text.trim_start();
                let Some((nk, nv)) = key_split(nb) else {
                    break;
                };
                *pos += 1;
                let key = unquote_key(nk);
                if map.iter().any(|(existing, _)| *existing == key) {
                    return Err(YamlError::DuplicateKey { line: next.no, key });
                }
                let val = inline_value(lines, pos, nv, next.no, item_indent)?;
                map.push((key, val));
            }
            items.push(Value::Map(map));
        } else {
            items.push(parse_scalar(rest, line.no)?);
        }
    }
    Ok(Value::Seq(items))
}

fn parse_mapping(lines: &[Line<'_>], pos: &mut usize, indent: usize) -> Result<Value, YamlError> {
    let mut map: Vec<(String, Value)> = Vec::new();
    while let Some(line) = lines.get(*pos) {
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(YamlError::Indentation { line: line.no });
        }
        let body = line.text.trim_start();
        let Some((k, v)) = key_split(body) else {
            return Err(YamlError::Malformed { line: line.no });
        };
        *pos += 1;
        let key = unquote_key(k);
        if map.iter().any(|(existing, _)| *existing == key) {
            return Err(YamlError::DuplicateKey { line: line.no, key });
        }
        let val = inline_value(lines, pos, v, line.no, indent)?;
        map.push((key, val));
    }
    Ok(Value::Map(map))
}

/// The value after `key:` — inline on the same line, or the nested block below.
fn inline_value(
    lines: &[Line<'_>],
    pos: &mut usize,
    v: &str,
    line_no: usize,
    indent: usize,
) -> Result<Value, YamlError> {
    let v = v.trim();
    if v.is_empty() {
        return match lines.get(*pos) {
            Some(next) if next.indent > indent => parse_block(lines, pos, next.indent),
            // `key:` followed by a sequence at the same indentation is YAML's usual shape for a list.
            Some(next) if next.indent == indent && next.text.trim_start().starts_with("- ") => {
                parse_sequence(lines, pos, indent)
            }
            _ => Ok(Value::Null),
        };
    }
    if v == "|" || v == ">" || v == "|-" || v == ">-" {
        let folded = v.starts_with('>');
        let strip = v.ends_with('-');
        let mut parts = Vec::new();
        let block_indent = lines.get(*pos).map(|l| l.indent).unwrap_or(indent + 1);
        while let Some(next) = lines.get(*pos) {
            if next.indent <= indent {
                break;
            }
            parts.push(
                next.text
                    .get(block_indent.min(next.indent)..)
                    .unwrap_or("")
                    .to_owned(),
            );
            *pos += 1;
        }
        let joined = if folded {
            parts.join(" ")
        } else {
            parts.join("\n")
        };
        let s = if strip || folded {
            joined
        } else {
            format!("{joined}\n")
        };
        return Ok(Value::Str(s));
    }
    parse_scalar(v, line_no)
}

/// Split `key: value` at the first unquoted `: ` (or a trailing `:`). Returns `None` if the text is not
/// a key line.
fn key_split(body: &str) -> Option<(&str, &str)> {
    if body.starts_with('[')
        || body.starts_with('{')
        || body.starts_with('"') && !body.contains("\":")
    {
        // A quoted key is `"k": v`; handle below. A flow collection is not a key line.
        if !body.starts_with('"') {
            return None;
        }
    }
    let mut in_single = false;
    let mut in_double = false;
    let bytes = body.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !in_double => in_single = !in_single,
            b'"' if !in_single => in_double = !in_double,
            b':' if !in_single && !in_double => {
                let after = bytes.get(i + 1).copied();
                if after.is_none() || after == Some(b' ') || after == Some(b'\t') {
                    let key = body.get(..i).unwrap_or("").trim();
                    if key.is_empty() {
                        return None;
                    }
                    return Some((key, body.get(i + 1..).unwrap_or("")));
                }
            }
            _ => {}
        }
    }
    None
}

fn unquote_key(k: &str) -> String {
    let k = k.trim();
    if (k.starts_with('"') && k.ends_with('"') || k.starts_with('\'') && k.ends_with('\''))
        && k.len() >= 2
    {
        k[1..k.len() - 1].to_owned()
    } else {
        k.to_owned()
    }
}

/// Parse one scalar or flow collection.
fn parse_scalar(v: &str, line_no: usize) -> Result<Value, YamlError> {
    let v = v.trim();
    if let Some(first) = v.chars().next() {
        match first {
            '&' | '*' => {
                return Err(YamlError::OutsideSubset {
                    line: line_no,
                    what: "an anchor or alias",
                });
            }
            '!' => {
                return Err(YamlError::OutsideSubset {
                    line: line_no,
                    what: "a tag",
                });
            }
            _ => {}
        }
    }
    if v.starts_with('[') && v.ends_with(']') {
        let inner = v.get(1..v.len() - 1).unwrap_or("");
        let mut items = Vec::new();
        for part in split_flow(inner, line_no)? {
            let part = part.trim();
            if !part.is_empty() {
                items.push(parse_scalar(part, line_no)?);
            }
        }
        return Ok(Value::Seq(items));
    }
    if v.starts_with('{') && v.ends_with('}') {
        let inner = v.get(1..v.len() - 1).unwrap_or("");
        let mut map = Vec::new();
        for part in split_flow(inner, line_no)? {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((k, val)) = key_split(part) else {
                return Err(YamlError::Malformed { line: line_no });
            };
            map.push((unquote_key(k), parse_scalar(val, line_no)?));
        }
        return Ok(Value::Map(map));
    }
    if v.starts_with('"') {
        return parse_double_quoted(v, line_no);
    }
    if v.starts_with('\'') {
        if v.len() < 2 || !v.ends_with('\'') {
            return Err(YamlError::UnterminatedQuote { line: line_no });
        }
        return Ok(Value::Str(
            v.get(1..v.len() - 1).unwrap_or("").replace("''", "'"),
        ));
    }
    Ok(coerce_plain(v))
}

fn split_flow(inner: &str, line_no: usize) -> Result<Vec<&str>, YamlError> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut start = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            '[' | '{' if !in_single && !in_double => depth += 1,
            ']' | '}' if !in_single && !in_double => depth = depth.saturating_sub(1),
            ',' if !in_single && !in_double && depth == 0 => {
                parts.push(inner.get(start..i).unwrap_or(""));
                start = i + 1;
            }
            _ => {}
        }
    }
    if in_single || in_double {
        return Err(YamlError::UnterminatedQuote { line: line_no });
    }
    parts.push(inner.get(start..).unwrap_or(""));
    Ok(parts)
}

fn parse_double_quoted(v: &str, line_no: usize) -> Result<Value, YamlError> {
    let mut out = String::new();
    let mut chars = v.get(1..).unwrap_or("").chars();
    loop {
        match chars.next() {
            None => return Err(YamlError::UnterminatedQuote { line: line_no }),
            Some('"') => break,
            Some('\\') => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                        out.push(c);
                    }
                }
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => return Err(YamlError::UnterminatedQuote { line: line_no }),
            },
            Some(c) => out.push(c),
        }
    }
    // Anything after the closing quote is a malformed line; YAML would refuse it too.
    if chars.next().is_some() {
        return Err(YamlError::Malformed { line: line_no });
    }
    Ok(Value::Str(out))
}

/// YAML 1.1's coercions, kept visible: booleans, null, integers, floats, dates. Everything else is a
/// string. A number with a leading zero (`0123`) stays a string, since YAML 1.1 would read it as octal
/// and the vocabulary's identifiers are strings.
fn coerce_plain(v: &str) -> Value {
    match v.to_ascii_lowercase().as_str() {
        "null" | "~" | "" => return Value::Null,
        "true" | "yes" | "on" => return Value::Bool(true),
        "false" | "no" | "off" => return Value::Bool(false),
        _ => {}
    }
    if is_date(v) {
        return Value::Date(v.to_owned());
    }
    let digits = v.strip_prefix('-').unwrap_or(v);
    if !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        if digits.len() > 1 && digits.starts_with('0') {
            return Value::Str(v.to_owned());
        }
        if let Ok(i) = v.parse::<i64>() {
            return Value::Int(i);
        }
    }
    if digits.contains('.')
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && digits.matches('.').count() == 1
        && !digits.starts_with('.')
        && !digits.ends_with('.')
        && let Ok(f) = v.parse::<f64>()
    {
        return Value::Float(f);
    }
    Value::Str(v.to_owned())
}

/// `YYYY-MM-DD`, optionally followed by `T…` or ` …` time and offset text.
fn is_date(v: &str) -> bool {
    let b = v.as_bytes();
    let digit_at = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    let dash_at = |i: usize| b.get(i) == Some(&b'-');
    let date_ok = (0..4).all(digit_at)
        && dash_at(4)
        && (5..7).all(digit_at)
        && dash_at(7)
        && (8..10).all(digit_at);
    if !date_ok {
        return false;
    }
    match b.get(10) {
        None => true,
        Some(b'T') | Some(b' ') | Some(b't') => b.get(11..).is_some_and(|rest| {
            rest.iter()
                .all(|c| c.is_ascii_digit() || matches!(c, b':' | b'.' | b'+' | b'-' | b'Z' | b'z'))
        }),
        _ => false,
    }
}

/// The Portable Page Metadata vocabulary (wiki-commons chapter 09), as inklkunn reads it. Every field
/// is optional except that a page should have a `title`; unknown keys stay in [`Metadata::extra`]
/// (META-11).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metadata {
    /// The human-readable title (META-2). `None` when the frontmatter has none.
    pub title: Option<String>,
    /// The stable identifier (META-2).
    pub id: Option<String>,
    /// Other titles that resolve to this page (META-4).
    pub aliases: Vec<String>,
    /// Free-form labels (META-5).
    pub tags: Vec<String>,
    /// What sort of page (META-7); `page` when absent.
    pub kind: String,
    /// Lifecycle state (META-6); `stable` when absent.
    pub status: String,
    /// BCP 47 language (META-10).
    pub lang: Option<String>,
    /// SPDX license (META-9).
    pub license: Option<String>,
    /// Stewards (COLL-11).
    pub maintainers: Vec<String>,
    /// One or two sentences for previews.
    pub description: Option<String>,
    /// Advisory visibility signal (META-13).
    pub visibility: Option<String>,
    /// A redirect target, when this page only redirects (META-4).
    pub redirect: Option<String>,
    /// Keys the vocabulary does not name, preserved as data (META-11).
    pub extra: Vec<(String, Value)>,
}

impl Metadata {
    /// The known keys of the vocabulary that [`Metadata`] lifts out of the mapping.
    pub const KNOWN_KEYS: &'static [&'static str] = &[
        "title",
        "id",
        "aliases",
        "tags",
        "kind",
        "status",
        "lang",
        "license",
        "maintainers",
        "description",
        "visibility",
        "redirect",
    ];

    /// Read the vocabulary out of a frontmatter value (`None` yields the defaults).
    pub fn from_value(v: Option<&Value>) -> Metadata {
        let mut m = Metadata {
            kind: "page".to_owned(),
            status: "stable".to_owned(),
            ..Metadata::default()
        };
        let Some(Value::Map(entries)) = v else {
            return m;
        };
        for (k, val) in entries {
            match k.as_str() {
                "title" => m.title = val.as_str().map(str::to_owned),
                "id" => m.id = val.as_str().map(str::to_owned),
                "aliases" => m.aliases = val.as_string_list(),
                "tags" => m.tags = val.as_string_list(),
                "kind" => {
                    if let Some(s) = val.as_str() {
                        m.kind = s.to_owned();
                    }
                }
                "status" => {
                    if let Some(s) = val.as_str() {
                        m.status = s.to_owned();
                    }
                }
                "lang" => m.lang = val.as_str().map(str::to_owned),
                "license" => m.license = val.as_str().map(str::to_owned),
                "maintainers" => m.maintainers = val.as_string_list(),
                "description" => {
                    m.description = val.as_str().map(str::to_owned);
                }
                "visibility" => m.visibility = val.as_str().map(str::to_owned),
                "redirect" => m.redirect = val.as_str().map(str::to_owned),
                _ => m.extra.push((k.clone(), val.clone())),
            }
        }
        m
    }
}

#[cfg(test)]
mod tests;
