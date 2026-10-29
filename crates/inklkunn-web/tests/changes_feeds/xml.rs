//! A strict little XML reader and the structural rules of Atom (RFC 4287) it is checked against: what "the
//! feed validates" means here. Not a general XML parser: it accepts what a well-formed, entity-escaped,
//! single-root document is, and says where it is not. (No XML library is in the lock, and none is added for a
//! test.)

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Clone)]
pub struct Element {
    pub name: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<Element>,
    pub text: String,
}

impl Element {
    pub fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn first(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r')
        || ('\u{20}'..='\u{d7ff}').contains(&c)
        || ('\u{e000}'..='\u{fffd}').contains(&c)
        || ('\u{10000}'..='\u{10ffff}').contains(&c)
}

fn unescape(raw: &str, what: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = raw;
    while let Some((before, after)) = rest.split_once('&') {
        out.push_str(before);
        let (entity, tail) = after
            .split_once(';')
            .ok_or_else(|| format!("{what}: an & that starts no entity"))?;
        let reference = |digits: &str, radix: u32| {
            u32::from_str_radix(digits, radix)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| format!("{what}: bad character reference &{entity};"))
        };
        out.push(match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            e if e.starts_with("#x") => reference(e.trim_start_matches("#x"), 16)?,
            e if e.starts_with('#') => reference(e.trim_start_matches('#'), 10)?,
            e => return Err(format!("{what}: unknown entity &{e};")),
        });
        rest = tail;
    }
    out.push_str(rest);
    if let Some(bad) = out.chars().find(|c| !is_xml_char(*c)) {
        return Err(format!(
            "{what}: character U+{:04X} cannot be in XML",
            bad as u32
        ));
    }
    Ok(out)
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, done: Element) {
    match stack.last_mut() {
        Some(parent) => parent.children.push(done),
        None => *root = Some(done),
    }
}

/// Parse a document: an optional declaration, one root element, nothing after it but space.
pub fn parse(doc: &str) -> Result<Element, String> {
    if let Some(bad) = doc.chars().find(|c| !is_xml_char(*c)) {
        return Err(format!("character U+{:04X} cannot be in XML", bad as u32));
    }
    let mut rest = doc;
    if rest.starts_with("<?xml") {
        let (decl, tail) = rest
            .split_once("?>")
            .ok_or("the XML declaration is not closed")?;
        if !decl.contains("encoding=\"utf-8\"") {
            return Err("the declaration does not say utf-8".into());
        }
        rest = tail.trim_start();
    }
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    while !rest.is_empty() {
        if let Some(done) = &root {
            return if rest.trim().is_empty() {
                Ok(done.clone())
            } else {
                Err("content after the root element".into())
            };
        }
        if let Some(after) = rest.strip_prefix("</") {
            let (name, tail) = after.split_once('>').ok_or("an end tag is not closed")?;
            let done = stack.pop().ok_or("an end tag with nothing open")?;
            if done.name != name.trim() {
                return Err(format!("</{name}> closes <{}>", done.name));
            }
            attach(&mut stack, &mut root, done);
            rest = tail;
        } else if let Some(after) = rest.strip_prefix('<') {
            let (inside, tail) = after.split_once('>').ok_or("a tag is not closed")?;
            let (inside, empty) = match inside.strip_suffix('/') {
                Some(i) => (i, true),
                None => (inside, false),
            };
            let (name, mut attrs_text) = match inside.split_once(char::is_whitespace) {
                Some((n, a)) => (n, a.trim()),
                None => (inside, ""),
            };
            if name.is_empty() || name.contains(['<', '"', '\'', '=']) {
                return Err(format!("a tag with the name {name:?}"));
            }
            let mut el = Element {
                name: name.to_owned(),
                ..Element::default()
            };
            while !attrs_text.is_empty() {
                let (key, value) = attrs_text
                    .split_once('=')
                    .ok_or("an attribute without a value")?;
                let key = key.trim().to_owned();
                let quoted = value
                    .trim_start()
                    .strip_prefix('"')
                    .ok_or_else(|| format!("attribute {key} is not double-quoted"))?;
                let (raw, after_value) =
                    quoted.split_once('"').ok_or("an attribute is not closed")?;
                if raw.contains('<') {
                    return Err(format!("attribute {key} holds a raw <"));
                }
                let text = unescape(raw, &format!("attribute {key}"))?;
                if el.attrs.insert(key.clone(), text).is_some() {
                    return Err(format!("attribute {key} twice"));
                }
                attrs_text = after_value.trim_start();
            }
            rest = tail;
            if empty {
                attach(&mut stack, &mut root, el);
            } else {
                stack.push(el);
            }
        } else {
            let (raw, tail) = match rest.find('<') {
                Some(i) => rest.split_at(i),
                None => (rest, ""),
            };
            let text = unescape(raw, "text")?;
            match stack.last_mut() {
                Some(parent) => parent.text.push_str(&text),
                None if text.trim().is_empty() => {}
                None => return Err("text outside the root element".into()),
            }
            rest = tail;
        }
    }
    if let Some(open) = stack.first() {
        return Err(format!("<{}> is never closed", open.name));
    }
    root.ok_or_else(|| "no root element".into())
}

/// RFC 3339 `date-time` in UTC, to the second.
fn is_rfc3339(s: &str) -> bool {
    let shape = "dddd-dd-ddTdd:dd:ddZ";
    s.chars().count() == shape.len()
        && shape.chars().zip(s.chars()).all(
            |(p, c)| {
                if p == 'd' { c.is_ascii_digit() } else { p == c }
            },
        )
}

fn absolute(href: &str) -> bool {
    href.starts_with("http://") || href.starts_with("https://")
}

/// Every violation of Atom's structure (RFC 4287 §4.1, §4.2) in the feed; empty when it conforms.
pub fn atom_violations(doc: &str) -> Vec<String> {
    let root = match parse(doc) {
        Ok(r) => r,
        Err(e) => return vec![format!("not well-formed: {e}")],
    };
    let mut v = Vec::new();
    if root.name != "feed" {
        v.push(format!("the root is <{}>, not <feed>", root.name));
        return v;
    }
    if root.attr("xmlns") != Some("http://www.w3.org/2005/Atom") {
        v.push("the feed is not in the Atom namespace".into());
    }
    for required in ["id", "title", "updated"] {
        match root.all(required).count() {
            1 => {}
            n => v.push(format!("the feed has {n} <{required}> (exactly one)")),
        }
    }
    if let Some(id) = root.first("id")
        && !id.text.contains(':')
    {
        v.push(format!("the feed id {:?} is not an IRI", id.text));
    }
    if let Some(u) = root.first("updated")
        && !is_rfc3339(&u.text)
    {
        v.push(format!("the feed's updated {:?} is not RFC 3339", u.text));
    }
    if root
        .first("title")
        .is_some_and(|t| t.text.trim().is_empty())
    {
        v.push("the feed's title is empty".into());
    }
    let selfs = root
        .all("link")
        .filter(|l| l.attr("rel") == Some("self"))
        .count();
    if selfs != 1 {
        v.push(format!("the feed has {selfs} rel=self links (one)"));
    }
    for l in root.all("link") {
        match l.attr("href") {
            Some(h) if absolute(h) => {}
            other => v.push(format!("a feed link has the href {other:?} (absolute)")),
        }
    }
    let entries: Vec<&Element> = root.all("entry").collect();
    if root.first("author").is_none() && entries.iter().any(|e| e.first("author").is_none()) {
        v.push("an entry has no author and the feed has none".into());
    }
    let mut ids = BTreeSet::new();
    for (i, e) in entries.iter().enumerate() {
        for required in ["id", "title", "updated"] {
            if e.all(required).count() != 1 {
                v.push(format!("entry {i} has no single <{required}>"));
            }
        }
        if let Some(id) = e.first("id")
            && !ids.insert(id.text.clone())
        {
            v.push(format!("entry id {:?} twice", id.text));
        }
        if let Some(u) = e.first("updated")
            && !is_rfc3339(&u.text)
        {
            v.push(format!("entry {i}'s updated {:?} is not RFC 3339", u.text));
        }
        if !e.all("link").any(|l| l.attr("rel") == Some("alternate")) {
            v.push(format!("entry {i} has no alternate link"));
        }
        for l in e.all("link") {
            match l.attr("href") {
                Some(h) if absolute(h) => {}
                other => v.push(format!("entry {i} has a link with the href {other:?}")),
            }
        }
        if let Some(a) = e.first("author")
            && a.first("name").is_none_or(|n| n.text.trim().is_empty())
        {
            v.push(format!("entry {i}'s author has no name"));
        }
    }
    let times: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.first("updated").map(|u| u.text.as_str()))
        .collect();
    if times.iter().zip(times.iter().skip(1)).any(|(a, b)| a < b) {
        v.push("entries are not newest first".into());
    }
    v
}

#[test]
fn the_reader_accepts_what_is_well_formed_and_refuses_what_is_not() {
    let ok =
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<a x=\"1&amp;2\"><b/>t&lt;&#65;<c>d</c></a>\n";
    let el = parse(ok).unwrap();
    assert_eq!(el.attr("x"), Some("1&2"));
    assert_eq!(el.text, "t<A");
    for bad in [
        "<a>",
        "<a></b>",
        "<a>& </a>",
        "<a>&nbsp;</a>",
        "<a x=1></a>",
        "<a x=\"<\"></a>",
        "<a>\u{1}</a>",
        "<a></a><b></b>",
        "<a>text</a> trailing",
        "text<a></a>",
        "<a x=\"1\" x=\"2\"></a>",
    ] {
        assert!(parse(bad).is_err(), "{bad:?} must not parse");
    }
}

#[test]
fn the_atom_rules_catch_their_planted_violations() {
    let good = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<feed xmlns=\"http://www.w3.org/2005/Atom\"><title>T</title><id>urn:x:1</id><updated>2023-11-14T22:13:20Z</updated>\
<link rel=\"self\" href=\"http://h/f\"/><author><name>A</name></author>\
<entry><title>E</title><id>urn:x:2</id><updated>2023-11-14T22:13:20Z</updated><link rel=\"alternate\" href=\"http://h/p\"/></entry></feed>";
    assert_eq!(atom_violations(good), Vec::<String>::new());
    let broken = [
        (
            "<updated>2023-11-14T22:13:20Z</updated><link rel=\"self\"",
            "<link rel=\"self\"",
            "updated",
        ),
        ("<id>urn:x:1</id>", "", "id"),
        ("<link rel=\"self\" href=\"http://h/f\"/>", "", "self"),
        ("http://h/f", "/relative", "absolute"),
        ("<author><name>A</name></author>", "", "author"),
        (
            "</entry></feed>",
            "</entry><entry><title>E</title><id>urn:x:2</id><updated>2023-11-14T22:13:20Z</updated><link rel=\"alternate\" href=\"http://h/p\"/></entry></feed>",
            "twice",
        ),
        (
            "2023-11-14T22:13:20Z</updated><link rel=\"alternate\"",
            "yesterday</updated><link rel=\"alternate\"",
            "RFC 3339",
        ),
        ("xmlns=\"http://www.w3.org/2005/Atom\"", "", "namespace"),
    ];
    for (from, to, expect) in broken {
        let doc = good.replacen(from, to, 1);
        assert_ne!(doc, good, "{expect}: the plant did not apply");
        let found = atom_violations(&doc);
        assert!(
            found.iter().any(|f| f.contains(expect)),
            "planting {expect:?} found {found:?}"
        );
    }
}
