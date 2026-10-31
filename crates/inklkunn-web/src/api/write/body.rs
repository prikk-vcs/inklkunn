//! A write's JSON body, read strictly (RFC 004 D-13): a body is **one JSON object**, every member is named,
//! typed and used, and one that is unknown, repeated, missing or of the wrong type is `invalid-body` naming it.
//! Nothing is guessed, defaulted in silence or ignored, so a client that mistypes `summry` is told, and two
//! readers of one request cannot read two requests.
//!
//! The size was judged before this runs (the caller reads at most the endpoint's bound). Text is data: a string
//! is carried as it is, never trimmed or interpreted here, and what an answer echoes of a name is cut short.

use std::collections::HashSet;

use serde_json::{Map, Value};

use crate::api::problem::{Code, Problem};

/// The most characters of a member's name an answer repeats.
const NAME_SHOWN: usize = 64;

/// The problem for the member at `path`; `$` names the body as a whole.
pub(crate) fn invalid(path: &str) -> Problem {
    let shown: String = path.chars().take(NAME_SHOWN).collect();
    Problem::new(Code::InvalidBody).named("member", shown)
}

/// The body's JSON, if it is one JSON document that is an object in which no name is repeated (at any depth).
pub(crate) fn parse(bytes: &[u8]) -> Result<Fields, Problem> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid("$"))?;
    if let Some(path) = repeated_name(bytes) {
        return Err(invalid(&path));
    }
    Fields::of(value, "$")
}

/// The object's members, taken one by one: what is left when the handler is done is unknown.
#[derive(Debug)]
pub(crate) struct Fields {
    path: String,
    map: Map<String, Value>,
}

impl Fields {
    fn of(value: Value, path: &str) -> Result<Fields, Problem> {
        match value {
            Value::Object(map) => Ok(Fields {
                path: path.to_owned(),
                map,
            }),
            _ => Err(invalid(path)),
        }
    }

    fn at(&self, name: &str) -> String {
        if self.path == "$" {
            name.to_owned()
        } else {
            format!("{}.{name}", self.path)
        }
    }

    /// A required text.
    pub(crate) fn text(&mut self, name: &str) -> Result<String, Problem> {
        match self.map.remove(name) {
            Some(Value::String(s)) => Ok(s),
            _ => Err(invalid(&self.at(name))),
        }
    }

    /// A text that may be left out.
    pub(crate) fn opt_text(&mut self, name: &str) -> Result<Option<String>, Problem> {
        match self.map.remove(name) {
            None => Ok(None),
            Some(Value::String(s)) => Ok(Some(s)),
            Some(_) => Err(invalid(&self.at(name))),
        }
    }

    /// A text that is a text or `null`, and must be there.
    pub(crate) fn text_or_null(&mut self, name: &str) -> Result<Option<String>, Problem> {
        match self.map.remove(name) {
            Some(Value::String(s)) => Ok(Some(s)),
            Some(Value::Null) => Ok(None),
            _ => Err(invalid(&self.at(name))),
        }
    }

    /// A required yes or no.
    pub(crate) fn flag(&mut self, name: &str) -> Result<bool, Problem> {
        match self.map.remove(name) {
            Some(Value::Bool(b)) => Ok(b),
            _ => Err(invalid(&self.at(name))),
        }
    }

    /// A yes or no that may be left out (`false`).
    pub(crate) fn opt_flag(&mut self, name: &str) -> Result<bool, Problem> {
        match self.map.remove(name) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(b),
            Some(_) => Err(invalid(&self.at(name))),
        }
    }

    /// A whole number of 0 to `u32::MAX` that must be there.
    pub(crate) fn count(&mut self, name: &str) -> Result<u32, Problem> {
        match self.map.remove(name) {
            Some(Value::Number(n)) => n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| invalid(&self.at(name))),
            _ => Err(invalid(&self.at(name))),
        }
    }

    /// A list of texts that may be left out (empty), of at most `max`.
    pub(crate) fn texts(&mut self, name: &str, max: usize) -> Result<Vec<String>, Problem> {
        match self.map.remove(name) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) if items.len() <= max => items
                .into_iter()
                .enumerate()
                .map(|(i, item)| match item {
                    Value::String(s) => Ok(s),
                    _ => Err(invalid(&format!("{}[{i}]", self.at(name)))),
                })
                .collect(),
            Some(_) => Err(invalid(&self.at(name))),
        }
    }

    /// A list of objects that may be left out (empty), of at most `max`.
    pub(crate) fn objects(&mut self, name: &str, max: usize) -> Result<Vec<Fields>, Problem> {
        match self.map.remove(name) {
            None => Ok(Vec::new()),
            Some(Value::Array(items)) if items.len() <= max => items
                .into_iter()
                .enumerate()
                .map(|(i, item)| Fields::of(item, &format!("{}[{i}]", self.at(name))))
                .collect(),
            Some(_) => Err(invalid(&self.at(name))),
        }
    }

    /// An object that may be left out.
    pub(crate) fn object(&mut self, name: &str) -> Result<Option<Fields>, Problem> {
        match self.map.remove(name) {
            None => Ok(None),
            Some(value) => Fields::of(value, &self.at(name)).map(Some),
        }
    }

    /// Nothing is left: every member was known. The first one that is not is named.
    pub(crate) fn end(self) -> Result<(), Problem> {
        match self.map.keys().next() {
            None => Ok(()),
            Some(name) => Err(invalid(&self.at(name))),
        }
    }
}

// ------------------------------------------------------------------------------------ repeated names

/// The first name an object of the document has twice, in a document that is known to be well-formed JSON
/// (`serde_json` read it, and keeps the last of a repeated name without a word). A scan of the bytes: strings
/// are skipped whole, and every object keeps the names it has seen.
pub(super) fn repeated_name(bytes: &[u8]) -> Option<String> {
    // One entry per open container: the names an object has seen, or `None` for an array.
    let mut open: Vec<Option<HashSet<String>>> = Vec::new();
    // Is the next string a name?
    let mut name_next = false;
    let mut i = 0;
    while let Some(&byte) = bytes.get(i) {
        match byte {
            b'{' => {
                open.push(Some(HashSet::new()));
                name_next = true;
            }
            b'[' => {
                open.push(None);
                name_next = false;
            }
            b'}' | b']' => {
                open.pop();
                name_next = false;
            }
            b',' => name_next = matches!(open.last(), Some(Some(_))),
            b'"' => {
                let end = string_end(bytes, i)?;
                if name_next {
                    let name: String = serde_json::from_slice(bytes.get(i..=end)?).ok()?;
                    if let Some(Some(seen)) = open.last_mut()
                        && !seen.insert(name.clone())
                    {
                        return Some(name);
                    }
                    name_next = false;
                }
                i = end;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The index of the quote that ends the string that begins at `start`.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while let Some(&byte) = bytes.get(i) {
        match byte {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests;
