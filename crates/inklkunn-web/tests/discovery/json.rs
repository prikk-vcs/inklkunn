//! A strict JSON reader for the tests of the documents the site publishes (a site description, JSON-LD): a
//! document that does not parse, has a raw control character in a string, or has anything after the value is
//! refused. (No JSON library is a dependency of this crate; none is added for a test.)

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Json>),
    Map(BTreeMap<String, Json>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Map(m) => m.get(key),
            _ => None,
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Json::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn list(&self) -> Option<&[Json]> {
        match self {
            Json::List(l) => Some(l),
            _ => None,
        }
    }
}

struct Reader<'a> {
    rest: &'a str,
}

impl<'a> Reader<'a> {
    fn skip(&mut self) {
        self.rest = self.rest.trim_start_matches([' ', '\t', '\n', '\r']);
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip();
        match self.rest.strip_prefix(c) {
            Some(r) => {
                self.rest = r;
                true
            }
            None => false,
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        self.skip();
        let first = self.rest.chars().next().ok_or("a value is missing")?;
        match first {
            '{' => {
                self.rest = self.rest.strip_prefix(first).unwrap_or(self.rest);
                let mut map = BTreeMap::new();
                if self.eat('}') {
                    return Ok(Json::Map(map));
                }
                loop {
                    self.skip();
                    let key = self.string()?;
                    if !self.eat(':') {
                        return Err("a : is missing".into());
                    }
                    let value = self.value()?;
                    if map.insert(key.clone(), value).is_some() {
                        return Err(format!("the key {key:?} twice"));
                    }
                    if self.eat('}') {
                        return Ok(Json::Map(map));
                    }
                    if !self.eat(',') {
                        return Err("a , or } is missing".into());
                    }
                }
            }
            '[' => {
                self.rest = self.rest.strip_prefix(first).unwrap_or(self.rest);
                let mut items = Vec::new();
                if self.eat(']') {
                    return Ok(Json::List(items));
                }
                loop {
                    items.push(self.value()?);
                    if self.eat(']') {
                        return Ok(Json::List(items));
                    }
                    if !self.eat(',') {
                        return Err("a , or ] is missing".into());
                    }
                }
            }
            '"' => self.string().map(Json::Text),
            't' | 'f' | 'n' => {
                for (word, value) in [
                    ("true", Json::Bool(true)),
                    ("false", Json::Bool(false)),
                    ("null", Json::Null),
                ] {
                    if let Some(r) = self.rest.strip_prefix(word) {
                        self.rest = r;
                        return Ok(value);
                    }
                }
                Err("a word that is not a value".into())
            }
            _ => {
                let end = self
                    .rest
                    .find(|c: char| {
                        !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E'))
                    })
                    .unwrap_or(self.rest.len());
                let (number, rest) = self.rest.split_at(end);
                let n = number
                    .parse::<f64>()
                    .map_err(|_| format!("{number:?} is not a number"))?;
                self.rest = rest;
                Ok(Json::Number(n))
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.rest = self.rest.strip_prefix('"').ok_or("a string is missing")?;
        let mut out = String::new();
        let mut chars = self.rest.char_indices();
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => {
                    self.rest = self.rest.split_at(at + 1).1;
                    return Ok(out);
                }
                '\\' => match chars.next().ok_or("a string ends in a backslash")?.1 {
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    '/' => out.push('/'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => {
                        let hex: String = chars.by_ref().take(4).map(|(_, c)| c).collect();
                        let code = u32::from_str_radix(&hex, 16)
                            .map_err(|_| format!("\\u{hex} is not a code"))?;
                        out.push(char::from_u32(code).ok_or("a lone surrogate")?);
                    }
                    other => return Err(format!("\\{other} is not an escape")),
                },
                c if (c as u32) < 0x20 => {
                    return Err(format!(
                        "a raw control character U+{:04X} in a string",
                        c as u32
                    ));
                }
                c => out.push(c),
            }
        }
        Err("a string is not closed".into())
    }
}

/// Parse a whole document.
pub fn parse(doc: &str) -> Result<Json, String> {
    let mut reader = Reader { rest: doc };
    let value = reader.value()?;
    reader.skip();
    if reader.rest.is_empty() {
        Ok(value)
    } else {
        Err("something after the value".into())
    }
}

#[test]
fn the_reader_accepts_json_and_refuses_what_is_not() {
    let v = parse(r#" {"a": [1, 2.5, "x\"y\u00e9\n"], "b": {"c": null, "d": true}} "#).unwrap();
    assert_eq!(
        v.get("a").unwrap().list().unwrap().get(2).unwrap().text(),
        Some("x\"yé\n")
    );
    assert_eq!(v.get("b").unwrap().get("d"), Some(&Json::Bool(true)));
    for bad in [
        "{",
        "{\"a\":}",
        "{\"a\":1,}",
        "[1 2]",
        "\"a\nb\"",
        "{\"a\":1,\"a\":2}",
        "{} {}",
        "'a'",
        "",
        "{\"a\":\"\\x\"}",
    ] {
        assert!(parse(bad).is_err(), "{bad:?} must not parse");
    }
}
