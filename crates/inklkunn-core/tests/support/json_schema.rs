//! A small JSON Schema (2020-12) checker for the bundle manifest tests, written by hand because no JSON Schema
//! crate is in the lock and a dependency is not worth one schema. It covers the keywords the wiki-commons
//! manifest schema uses (`$ref` into `$defs`, `type`, `const`, `enum`, `required`, `properties`,
//! `additionalProperties`, `items`, `minimum`, `maximum`, `pattern`, `format`) and **fails on any other keyword,
//! on a `pattern` it does not know and on a `format` it would have to check**, so a newer schema copy cannot be
//! half-checked without the test saying so.
#![allow(dead_code)]

use serde_json::Value;

/// Keywords that annotate and never constrain.
const ANNOTATIONS: &[&str] = &["$schema", "$id", "title", "description", "$defs"];
/// Keywords this checker applies.
const APPLIED: &[&str] = &[
    "$ref",
    "type",
    "const",
    "enum",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "minimum",
    "maximum",
    "pattern",
    "format",
];

/// Every way `instance` fails `schema` (empty when it conforms). Paths are JSON pointers into the instance.
pub fn violations(schema: &Value, instance: &Value) -> Vec<String> {
    let mut out = Vec::new();
    check(schema, schema, instance, "", &mut out);
    out
}

fn type_matches(name: &str, v: &Value) -> bool {
    match name {
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "boolean" => v.is_boolean(),
        "null" => v.is_null(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        _ => false,
    }
}

/// The one `pattern` the manifest schema has: the bundle layout version, such as `0.4`.
fn pattern_matches(pattern: &str, s: &str) -> Option<bool> {
    if pattern == "^[0-9]+\\.[0-9]+$" {
        let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
        Some(
            s.split_once('.')
                .is_some_and(|(a, b)| digits(a) && digits(b)),
        )
    } else {
        None
    }
}

fn check(root: &Value, schema: &Value, inst: &Value, at: &str, out: &mut Vec<String>) {
    let Some(map) = schema.as_object() else {
        // `true` accepts, `false` rejects.
        if schema == &Value::Bool(false) {
            out.push(format!("{at}: not allowed"));
        }
        return;
    };
    for key in map.keys() {
        if !APPLIED.contains(&key.as_str()) && !ANNOTATIONS.contains(&key.as_str()) {
            out.push(format!(
                "{at}: the checker does not support the keyword `{key}`"
            ));
        }
    }
    if let Some(reference) = map.get("$ref").and_then(Value::as_str) {
        match reference
            .strip_prefix('#')
            .and_then(|pointer| root.pointer(pointer))
        {
            Some(target) => check(root, target, inst, at, out),
            None => out.push(format!("{at}: unresolvable $ref {reference}")),
        }
    }
    if let Some(t) = map.get("type") {
        let names: Vec<&str> = match t {
            Value::String(s) => vec![s.as_str()],
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !names.iter().any(|n| type_matches(n, inst)) {
            out.push(format!("{at}: is not of type {names:?}: {inst}"));
            return;
        }
    }
    if let Some(c) = map.get("const")
        && c != inst
    {
        out.push(format!("{at}: must be {c}, is {inst}"));
    }
    if let Some(Value::Array(options)) = map.get("enum")
        && !options.contains(inst)
    {
        out.push(format!("{at}: is {inst}, not one of {options:?}"));
    }
    if let Some(n) = inst.as_f64() {
        if let Some(min) = map.get("minimum").and_then(Value::as_f64)
            && n < min
        {
            out.push(format!("{at}: {n} is below {min}"));
        }
        if let Some(max) = map.get("maximum").and_then(Value::as_f64)
            && n > max
        {
            out.push(format!("{at}: {n} is above {max}"));
        }
    }
    if let Some(s) = inst.as_str() {
        if let Some(pattern) = map.get("pattern").and_then(Value::as_str) {
            match pattern_matches(pattern, s) {
                Some(true) => {}
                Some(false) => out.push(format!("{at}: {s:?} does not match {pattern}")),
                None => out.push(format!(
                    "{at}: the checker does not know the pattern {pattern}"
                )),
            }
        }
        match map.get("format").and_then(Value::as_str) {
            None => {}
            Some("date-time") => {
                if !is_date_time(s) {
                    out.push(format!("{at}: {s:?} is not an RFC 3339 date-time"));
                }
            }
            Some(format) => out.push(format!(
                "{at}: the checker does not check the format `{format}`"
            )),
        }
    }
    if let Some(object) = inst.as_object() {
        if let Some(Value::Array(required)) = map.get("required") {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    out.push(format!("{at}: `{name}` is required"));
                }
            }
        }
        let properties = map.get("properties").and_then(Value::as_object);
        for (name, value) in object {
            let here = format!("{at}/{name}");
            match properties.and_then(|p| p.get(name)) {
                Some(sub) => check(root, sub, value, &here, out),
                None => {
                    if let Some(extra) = map.get("additionalProperties") {
                        check(root, extra, value, &here, out);
                    }
                }
            }
        }
    }
    if let (Some(items), Some(array)) = (map.get("items"), inst.as_array()) {
        for (i, value) in array.iter().enumerate() {
            check(root, items, value, &format!("{at}/{i}"), out);
        }
    }
}

/// RFC 3339 `date-time`: `YYYY-MM-DDTHH:MM:SS`, optional fraction, then `Z` or `±HH:MM`, with the date and time in
/// range (the month's length and leap years included; a leap second is allowed).
fn is_date_time(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |from: usize, n: usize| -> Option<u32> {
        let part = b.get(from..from + n)?;
        if !part.iter().all(u8::is_ascii_digit) {
            return None;
        }
        std::str::from_utf8(part).ok()?.parse().ok()
    };
    let at = |i: usize| b.get(i).copied();
    let (Some(year), Some(month), Some(day), Some(hour), Some(minute), Some(second)) = (
        digits(0, 4),
        digits(5, 2),
        digits(8, 2),
        digits(11, 2),
        digits(14, 2),
        digits(17, 2),
    ) else {
        return false;
    };
    if at(4) != Some(b'-') || at(7) != Some(b'-') || !matches!(at(10), Some(b'T' | b't')) {
        return false;
    }
    if at(13) != Some(b':') || at(16) != Some(b':') {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    if day == 0 || day > days || hour > 23 || minute > 59 || second > 60 {
        return false;
    }
    let mut i = 19;
    if at(i) == Some(b'.') {
        let start = i + 1;
        i = start;
        while at(i).is_some_and(|c| c.is_ascii_digit()) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match at(i) {
        Some(b'Z' | b'z') => i + 1 == b.len(),
        Some(b'+' | b'-') => {
            let (Some(h), Some(m)) = (digits(i + 1, 2), digits(i + 4, 2)) else {
                return false;
            };
            at(i + 3) == Some(b':') && h <= 23 && m <= 59 && i + 6 == b.len()
        }
        _ => false,
    }
}
