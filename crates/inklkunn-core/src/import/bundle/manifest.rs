//! What a bundle says about itself: the manifest (`wiki-bundle.yaml`), the checksums file the manifest names, and the
//! contributor directory (`users.yaml`). All three are read here, by their **known keys only** (RFC 007 D-5): a key this
//! reader does not know is counted and named in the report and never interpreted, and a known key of the wrong type is a
//! refusal that names the key and never repeats its value.

use inklkunn_markup::Value;

use super::safe::{looks_like_an_address, shown_up_to};
use crate::import::Digest;

/// The manifest keys this reader knows (wiki-commons' manifest schema, and inklkunn's own under `inklkunn`).
const KNOWN_KEYS: &[&str] = &[
    "format",
    "version",
    "generator",
    "exported_at",
    "source",
    "lang",
    "languages",
    "license",
    "visibility_default",
    "markup",
    "namespaces",
    "interwiki",
    "contents",
    "fidelity",
    "extensions_used",
    "pseudonymized",
    "checksums",
    "conformance",
    "inklkunn",
];

/// What the manifest says. Everything here is a claim of the bundle, shown as such (`fidelity` above all: it is **reported and
/// not trusted**).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// `version`, as written (`0.1`, `0.4`).
    pub version: String,
    /// The generator's name and version, when it says them.
    pub generator: Option<String>,
    /// `exported_at`, as written.
    pub exported_at: Option<String>,
    /// `source.name`: the name of the wiki the bundle came from. Text, never a link.
    pub source_name: Option<String>,
    /// `source.url`: the address of the source. **Text, never a link** (an address from a bundle is untrusted, C-17a).
    pub source_url: Option<String>,
    /// `source.engine` and its version.
    pub source_engine: Option<String>,
    /// `lang` and `languages`, in the order given, without repeats.
    pub languages: Vec<String>,
    /// `license`: the bundle's default licence, as written.
    pub license: Option<String>,
    /// `visibility_default`.
    pub visibility_default: Option<String>,
    /// `namespaces`: the prefixes the bundle's titles have.
    pub namespaces: Vec<String>,
    /// `interwiki`: the bundle's map of prefixes to templates, **as written** and in the order given (untrusted text: the plan judges it,
    /// prefix by prefix, with [`crate::governance::judge_interwiki`], RFC 007 D-8). The manifest reader only reads it.
    pub interwiki: Vec<(String, String)>,
    /// `fidelity.level`, as declared.
    pub fidelity_level: Option<u8>,
    /// What the source says it degraded and dropped (`fidelity.degraded`, `fidelity.dropped`): the constructs named, as written.
    pub degraded: Vec<String>,
    /// See [`Manifest::degraded`].
    pub dropped: Vec<String>,
    /// `contents.history`.
    pub declares_history: Option<bool>,
    /// `pseudonymized`.
    pub pseudonymized: bool,
    /// `checksums`: the path of the file of checksums, as written.
    pub checksums: Option<String>,
    /// How many entries the exporting inklkunn says had a part hidden by moderation (`inklkunn.tombstones.entries`).
    pub tombstones: Option<u64>,
    /// The top-level keys this reader does not know, as written, in order.
    pub unknown_keys: Vec<String>,
}

/// Why the manifest cannot be used: each reason is a rule and a key of the vocabulary, never a value.
pub type Reasons = Vec<String>;

/// The member `key` of the map `pairs`, and a refusal when it is given twice.
fn member<'a>(
    pairs: &'a [(String, Value)],
    key: &str,
    path: &str,
    reasons: &mut Reasons,
) -> Option<&'a Value> {
    let mut found = pairs.iter().filter(|(k, _)| k == key);
    let first = found.next().map(|(_, v)| v);
    if found.next().is_some() {
        reasons.push(format!("`{path}{key}` is given twice"));
    }
    first
}

fn text(value: Option<&Value>, name: &str, reasons: &mut Reasons) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::Str(s) | Value::Date(s)) => Some(s.clone()),
        Some(_) => {
            reasons.push(format!("`{name}` is not text"));
            None
        }
    }
}

fn texts(value: Option<&Value>, name: &str, reasons: &mut Reasons) -> Vec<String> {
    match value {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Seq(items)) => items
            .iter()
            .filter_map(|v| match v {
                Value::Str(s) | Value::Date(s) => Some(s.clone()),
                _ => {
                    reasons.push(format!("`{name}` holds something that is not text"));
                    None
                }
            })
            .collect(),
        Some(_) => {
            reasons.push(format!("`{name}` is not a list"));
            Vec::new()
        }
    }
}

fn losses(value: Option<&Value>, name: &str, reasons: &mut Reasons) -> Vec<String> {
    match value {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Seq(items)) => items
            .iter()
            .map(|item| match item.get("construct").and_then(Value::as_str) {
                Some(c) => c.to_owned(),
                None => {
                    reasons.push(format!("`{name}` has an entry with no `construct`"));
                    String::new()
                }
            })
            .collect(),
        Some(_) => {
            reasons.push(format!("`{name}` is not a list"));
            Vec::new()
        }
    }
}

/// Read the manifest's mapping. `Err` holds every rule it breaks.
pub fn read(pairs: &[(String, Value)]) -> Result<Manifest, Reasons> {
    let mut reasons: Reasons = Vec::new();
    let mut m = Manifest::default();
    match member(pairs, "format", "", &mut reasons) {
        Some(Value::Str(f)) if f == "portable-wiki-bundle" => {}
        _ => reasons.push("`format` is not `portable-wiki-bundle`".to_owned()),
    }
    match member(pairs, "version", "", &mut reasons) {
        Some(Value::Str(v)) if is_version(v) => m.version = v.clone(),
        _ => reasons.push("`version` is not text of the form `major.minor`".to_owned()),
    }
    m.exported_at = text(
        member(pairs, "exported_at", "", &mut reasons),
        "exported_at",
        &mut reasons,
    );
    m.license = text(
        member(pairs, "license", "", &mut reasons),
        "license",
        &mut reasons,
    );
    m.checksums = text(
        member(pairs, "checksums", "", &mut reasons),
        "checksums",
        &mut reasons,
    );
    let lang = text(
        member(pairs, "lang", "", &mut reasons),
        "lang",
        &mut reasons,
    );
    for tag in lang.into_iter().chain(texts(
        member(pairs, "languages", "", &mut reasons),
        "languages",
        &mut reasons,
    )) {
        if !m.languages.contains(&tag) {
            m.languages.push(tag);
        }
    }
    m.namespaces = texts(
        member(pairs, "namespaces", "", &mut reasons),
        "namespaces",
        &mut reasons,
    );
    m.visibility_default = text(
        member(pairs, "visibility_default", "", &mut reasons),
        "visibility_default",
        &mut reasons,
    );
    if let Some(v) = &m.visibility_default
        && !matches!(v.as_str(), "public" | "internal" | "restricted")
    {
        reasons.push("`visibility_default` is not public, internal or restricted".to_owned());
    }
    match member(pairs, "pseudonymized", "", &mut reasons) {
        None | Some(Value::Null) => {}
        Some(Value::Bool(b)) => m.pseudonymized = *b,
        Some(_) => reasons.push("`pseudonymized` is not true or false".to_owned()),
    }
    match member(pairs, "interwiki", "", &mut reasons) {
        None | Some(Value::Null) => {}
        Some(Value::Map(entries)) => {
            let mut not_text = false;
            for (prefix, value) in entries {
                match value {
                    Value::Str(template) | Value::Date(template) => {
                        m.interwiki.push((prefix.clone(), template.clone()));
                    }
                    _ => not_text = true,
                }
            }
            if not_text {
                reasons.push("`interwiki` maps a prefix to something that is not text".to_owned());
            }
        }
        Some(_) => reasons.push("`interwiki` is not a map of prefixes to templates".to_owned()),
    }
    if let Some(value) = member(pairs, "generator", "", &mut reasons) {
        match value {
            Value::Map(g) => {
                let name = text(
                    member(g, "name", "generator.", &mut reasons),
                    "generator.name",
                    &mut reasons,
                );
                let version = text(
                    member(g, "version", "generator.", &mut reasons),
                    "generator.version",
                    &mut reasons,
                );
                m.generator = match (name, version) {
                    (Some(n), Some(v)) => Some(format!("{n} {v}")),
                    (Some(n), None) => Some(n),
                    (None, v) => v,
                };
            }
            Value::Null => {}
            _ => reasons.push("`generator` is not a map".to_owned()),
        }
    }
    if let Some(value) = member(pairs, "source", "", &mut reasons) {
        match value {
            Value::Map(s) => {
                m.source_name = text(
                    member(s, "name", "source.", &mut reasons),
                    "source.name",
                    &mut reasons,
                );
                m.source_url = text(
                    member(s, "url", "source.", &mut reasons),
                    "source.url",
                    &mut reasons,
                );
                let engine = text(
                    member(s, "engine", "source.", &mut reasons),
                    "source.engine",
                    &mut reasons,
                );
                let version = text(
                    member(s, "engine_version", "source.", &mut reasons),
                    "source.engine_version",
                    &mut reasons,
                );
                m.source_engine = match (engine, version) {
                    (Some(e), Some(v)) => Some(format!("{e} {v}")),
                    (Some(e), None) => Some(e),
                    (None, v) => v,
                };
            }
            Value::Null => {}
            _ => reasons.push("`source` is not a map".to_owned()),
        }
    }
    if let Some(value) = member(pairs, "contents", "", &mut reasons) {
        match value {
            Value::Map(c) => match member(c, "history", "contents.", &mut reasons) {
                None | Some(Value::Null) => {}
                Some(Value::Bool(b)) => m.declares_history = Some(*b),
                Some(_) => reasons.push("`contents.history` is not true or false".to_owned()),
            },
            Value::Null => {}
            _ => reasons.push("`contents` is not a map".to_owned()),
        }
    }
    if let Some(value) = member(pairs, "fidelity", "", &mut reasons) {
        match value {
            Value::Map(f) => {
                match member(f, "level", "fidelity.", &mut reasons) {
                    None | Some(Value::Null) => {}
                    Some(Value::Int(n)) if (0..=5).contains(n) => {
                        m.fidelity_level = u8::try_from(*n).ok();
                    }
                    Some(_) => reasons
                        .push("`fidelity.level` is not a whole number from 0 to 5".to_owned()),
                }
                m.degraded = losses(
                    member(f, "degraded", "fidelity.", &mut reasons),
                    "fidelity.degraded",
                    &mut reasons,
                );
                m.dropped = losses(
                    member(f, "dropped", "fidelity.", &mut reasons),
                    "fidelity.dropped",
                    &mut reasons,
                );
            }
            Value::Null => {}
            _ => reasons.push("`fidelity` is not a map".to_owned()),
        }
    }
    if let Some(value) = member(pairs, "inklkunn", "", &mut reasons) {
        match value {
            Value::Map(own) => {
                if let Some(Value::Map(t)) = member(own, "tombstones", "inklkunn.", &mut reasons) {
                    m.tombstones = match t.iter().find(|(k, _)| k == "entries").map(|(_, v)| v) {
                        Some(Value::Int(n)) => u64::try_from(*n).ok(),
                        _ => None,
                    };
                }
            }
            Value::Null => {}
            _ => reasons.push("`inklkunn` is not a map".to_owned()),
        }
    }
    m.unknown_keys = pairs
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !KNOWN_KEYS.contains(k))
        .map(|k| shown_up_to(k, 60))
        .collect();
    if reasons.is_empty() {
        Ok(m)
    } else {
        Err(reasons)
    }
}

fn is_version(v: &str) -> bool {
    match v.split_once('.') {
        Some((major, minor)) => {
            !major.is_empty()
                && !minor.is_empty()
                && major.bytes().all(|b| b.is_ascii_digit())
                && minor.bytes().all(|b| b.is_ascii_digit())
        }
        None => false,
    }
}

// ------------------------------------------------------------------------------------------------ the checksums file

/// The entries of the checksums file the manifest names: `(path, digest)` in the order written.
pub type Sums = Vec<(String, Digest)>;

/// A fault in the checksums file: its line (1-based) and the rule. Never the line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SumsFault {
    /// Where.
    pub line: usize,
    /// The rule.
    pub rule: &'static str,
}

/// The digest `hex` writes: 64 lower-case hexadecimal digits, nothing else.
pub fn digest_from_hex(hex: &str) -> Option<Digest> {
    if hex.len() != 64 {
        return None;
    }
    let mut out: Digest = [0; 32];
    let mut bytes = hex.bytes();
    for slot in &mut out {
        let hi = nibble(bytes.next()?)?;
        let lo = nibble(bytes.next()?)?;
        *slot = hi << 4 | lo;
    }
    Some(out)
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Is `path` a path inside the bundle as the checksums file may name one: relative, `/`-separated, with no empty, `.` or `..`
/// component, no backslash trick and no control character?
pub fn is_a_path_inside(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

/// Read the checksums file: lines of `<64 hex digits>  <path>` (two spaces, the form `sha256sum` writes and `sha256sum -c`
/// reads), nothing else. A path named twice is refused: it could say two things.
pub fn read_sums(text: &str) -> Result<Sums, SumsFault> {
    let mut out: Sums = Vec::new();
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() {
        return Ok(out);
    }
    for (n, line) in body.split('\n').enumerate() {
        let at = n + 1;
        let fault = |rule: &'static str| SumsFault { line: at, rule };
        let (hex, path) = line
            .split_once("  ")
            .ok_or_else(|| fault("is not `<sha-256>  <path>`"))?;
        let digest = digest_from_hex(hex)
            .ok_or_else(|| fault("does not start with 64 lower-case hexadecimal digits"))?;
        if !is_a_path_inside(path) {
            return Err(fault("names a path that is not a path inside the bundle"));
        }
        if out.iter().any(|(p, _)| p == path) {
            return Err(fault("names a path that an earlier line names"));
        }
        out.push((path.to_owned(), digest));
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ the contributor directory

/// One entry of `users.yaml`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Person {
    /// The entry's place in the file (from 1).
    pub entry: usize,
    /// `id`, when it is safe to show as the entry's name (see [`Person::label`]).
    pub id: Option<String>,
    /// `name`, and every name of `names`.
    pub names: Vec<String>,
    /// `kind`.
    pub kind: Option<String>,
}

impl Person {
    /// How a refusal names the entry: its place, and its `id` when that is plainly an identifier (letters, digits and `._-:~`, at
    /// most 64 characters) and not an address.
    pub fn label(&self) -> String {
        match &self.id {
            Some(id)
                if id.len() <= 64
                    && !id.is_empty()
                    && id.bytes().all(|b| {
                        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b':' | b'~')
                    }) =>
            {
                format!("entry {} (id `{id}`)", self.entry)
            }
            _ => format!("entry {}", self.entry),
        }
    }
}

/// What `users.yaml` came to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct People {
    /// The entries, in order.
    pub people: Vec<Person>,
    /// How many members of entries this reader does not know (counted, never interpreted).
    pub unknown_members: usize,
}

/// The keys whose name says an address is meant.
fn is_an_address_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    matches!(
        k.as_str(),
        "email" | "e-mail" | "e_mail" | "emails" | "mail" | "mailto" | "address"
    )
}

/// Read `users.yaml`: a list of entries, or a map `users:` of that list (inklkunn's own export writes it so). `Err` holds the
/// rules broken, each naming an entry and never repeating a value; **an e-mail address in any entry refuses** (RFC 007 D-3).
pub fn read_people(value: &Value) -> Result<People, Reasons> {
    let mut reasons: Reasons = Vec::new();
    let list: &[Value] = match value {
        Value::Seq(items) => items,
        Value::Map(m) => match m.as_slice() {
            [(k, Value::Seq(items))] if k.as_str() == "users" => items,
            [(k, Value::Null)] if k.as_str() == "users" => &[],
            [] => &[],
            _ => {
                return Err(vec![
                    "is a map that is not `users:` and a list of entries".to_owned(),
                ]);
            }
        },
        Value::Null => &[],
        _ => return Err(vec!["is not a list of entries".to_owned()]),
    };
    let mut people = People::default();
    for (n, item) in list.iter().enumerate() {
        let mut person = Person {
            entry: n + 1,
            ..Person::default()
        };
        let Value::Map(members) = item else {
            reasons.push(format!("entry {} is not a map of members", n + 1));
            continue;
        };
        let mut address = false;
        for (key, v) in members {
            if is_an_address_key(key) {
                address = true;
                continue;
            }
            let mut scan = |s: &str| address |= looks_like_an_address(s);
            match (key.as_str(), v) {
                ("id", Value::Str(s) | Value::Date(s)) => {
                    scan(s);
                    person.id = Some(s.clone());
                }
                ("name", Value::Str(s) | Value::Date(s)) => {
                    scan(s);
                    person.names.push(s.clone());
                }
                ("names", Value::Seq(items)) => {
                    for s in items.iter().filter_map(Value::as_str) {
                        scan(s);
                        person.names.push(s.to_owned());
                    }
                }
                ("kind", Value::Str(s)) => person.kind = Some(s.clone()),
                ("url", Value::Str(s)) => scan(s),
                ("id" | "name" | "names" | "kind" | "url", Value::Null) => {}
                ("id" | "name" | "names" | "kind" | "url", _) => {
                    reasons.push(format!(
                        "{} has a member `{key}` of the wrong type",
                        person.label()
                    ));
                }
                (_, other) => {
                    people.unknown_members += 1;
                    // An unknown member is not interpreted, but an address in it is still an address.
                    if let Value::Str(s) | Value::Date(s) = other {
                        scan(s);
                    }
                }
            }
        }
        if address {
            // The entry is named by its place (and its id when that is plainly an identifier): never by the address.
            reasons.push(format!(
                "{} has an e-mail address; addresses are not imported, so remove it from the file",
                person.label()
            ));
        }
        people.people.push(person);
    }
    if reasons.is_empty() {
        Ok(people)
    } else {
        Err(reasons)
    }
}
