//! The message catalog (RFC 003 D-6; planeter RFC 012 D-3): every sentence a page says, keyed by a stable
//! identifier, one compiled-in table per language, whole sentences with **named placeholders**. No
//! dependency: a table, a lookup and a few rules.
//!
//! * **Keys** are an enum ([`Key`]): a template or handler that names a key that does not exist does not
//!   compile; the identifier string (`"not_found.heading"`) is what the tables, the glossary and the tests use.
//! * **Languages** are [`LangDef`] values (tag, name in itself, direction, plural rule, number format, review
//!   state, table). English is [`EN_DEF`], the language the messages are written in; Japanese is [`JA_DEF`], a
//!   **preview** until its reviewers sign off (DR-7): compiled in, checked by the tests, labelled so wherever
//!   it is offered, and **chosen only by the reader** (the switcher's cookie), never for them from
//!   `Accept-Language` (see [`Lang::negotiate`]). Adding a language is a new table, a new `LangDef` and a line
//!   in [`languages`].
//! * **Placeholders** are `{name}`. A sentence that holds a link, or a name in another language, takes a typed
//!   [`Slot`]: text is HTML-escaped, a link is built from an escaped label and an `href` the **code** supplies
//!   (never the catalog), and the result ([`Rich`]) is a type askama prints without a second escape. A catalog
//!   entry cannot introduce a tag, an attribute or a URL by itself: its own text is escaped like any other, and
//!   the tests refuse markup in it.
//! * **Plurals** follow the language's rule ([`Plural`]): a counted message has a singular and an other form
//!   and takes `{count}`.
//! * **No concatenation**: a sentence is one message; two sentences next to each other are two messages.
//! * A missing key is never a crash and never a silent English fallback: it renders as a visible marker
//!   (`[the.key]`), and the audit ([`audit`]) fails the build first for every language.
//!
//! The wording itself is governed by `docs/glossary.md`: change the words there and here together (a test
//! holds the two equal).

use std::fmt;

use askama::filters::HtmlSafe;

pub mod en;
pub mod ja;

/// The product name: the same in every language, so it is a value and not a message.
pub const PRODUCT: &str = "inklkunn";

/// The most `Accept-Language` entries read from one header; the rest are ignored.
const MAX_ACCEPTED: usize = 32;

/// Text direction of a language (`dir` on `<html>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    /// Left to right.
    Ltr,
    /// Right to left.
    Rtl,
}

/// The plural categories this scheme knows. CLDR has more (`zero`, `two`, `few`, `many`); a language that needs
/// them is where a fluent-style library is reconsidered. Callers use [`Msg::n`] and never name a category, so
/// adding one does not rewrite them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plural {
    /// The singular form.
    One,
    /// Every other count.
    Other,
}

/// One message: the key's identifier and its text for the singular and the other form. A message without a
/// count has `one == other`.
#[derive(Debug)]
pub struct Entry {
    /// The stable identifier (`area.name`).
    pub key: &'static str,
    /// The text for a count the language's rule calls [`Plural::One`].
    pub one: &'static str,
    /// The text for every other count.
    pub other: &'static str,
}

/// A message with no plural forms.
pub const fn msg(key: &'static str, text: &'static str) -> Entry {
    Entry {
        key,
        one: text,
        other: text,
    }
}

/// A message with a singular and an other form (it must contain `{count}`).
pub const fn plural_msg(key: &'static str, one: &'static str, other: &'static str) -> Entry {
    Entry { key, one, other }
}

/// Whether a fluent reader has gone through a language (RFC 003 D-6; DR-7). The switcher and the accessibility
/// statement say so for every language that is not reviewed, so no reader takes a draft for a finished
/// translation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Review {
    /// The language the messages are written in.
    Source,
    /// A fluent reader's pass is recorded: who, and when.
    Reviewed {
        /// The reviewers and the date, as the glossary records them.
        by: &'static str,
    },
    /// Compiled in and checked by the tests, but not yet read by a fluent reviewer.
    Preview,
}

impl Review {
    /// Is this language still a preview?
    pub const fn is_preview(self) -> bool {
        matches!(self, Review::Preview)
    }
}

/// Everything that makes a language: how to name it, write it, count in it, whether it was reviewed, and where
/// its messages are.
#[derive(Debug)]
pub struct LangDef {
    /// The BCP 47 primary tag (`en`): the `<html lang>` value, the cookie value and the `Content-Language`.
    pub tag: &'static str,
    /// The language's name in itself (`English`), for the switcher.
    pub endonym: &'static str,
    /// The text direction.
    pub dir: Dir,
    /// Which plural form a count takes.
    pub plural: fn(u64) -> Plural,
    /// The digit group separator of a counted number.
    pub group_separator: char,
    /// Whether the language is the source, reviewed, or a preview.
    pub review: Review,
    /// The messages.
    pub entries: &'static [Entry],
}

/// English: `one` for exactly 1, `other` for everything else.
fn plural_en(n: u64) -> Plural {
    if n == 1 { Plural::One } else { Plural::Other }
}

/// Japanese has no grammatical plural: every count takes the `other` form.
fn plural_ja(_: u64) -> Plural {
    Plural::Other
}

/// English, the language the messages are written in, the default and the fallback of negotiation.
pub static EN_DEF: LangDef = LangDef {
    tag: "en",
    endonym: "English",
    dir: Dir::Ltr,
    plural: plural_en,
    group_separator: ',',
    review: Review::Source,
    entries: en::CATALOG,
};

/// Japanese: **a preview** (DR-7). Compiled in and offered, labelled as a preview, until the reviewers the
/// owner names have gone through it; then `review` becomes [`Review::Reviewed`] with who and when, in one
/// commit with the glossary and the accessibility statement.
pub static JA_DEF: LangDef = LangDef {
    tag: "ja",
    endonym: ja::ENDONYM,
    dir: Dir::Ltr,
    plural: plural_ja,
    group_separator: ',',
    review: Review::Preview,
    entries: ja::CATALOG,
};

static LANGUAGES: [&LangDef; 2] = [&EN_DEF, &JA_DEF];

/// Every language this build can serve, English first. The switcher lists exactly these.
pub fn languages() -> &'static [&'static LangDef] {
    &LANGUAGES
}

/// A language handle: cheap to copy, carried by every page.
#[derive(Clone, Copy)]
pub struct Lang(&'static LangDef);

impl fmt::Debug for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lang({})", self.0.tag)
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Self) -> bool {
        self.0.tag == other.0.tag
    }
}

impl Eq for Lang {}

/// English.
pub const EN: Lang = Lang(&EN_DEF);

impl Lang {
    /// The language's definition.
    pub fn def(self) -> &'static LangDef {
        self.0
    }

    /// The BCP 47 primary tag.
    pub fn tag(self) -> &'static str {
        self.0.tag
    }

    /// `ltr` or `rtl`, as `dir` says it.
    pub fn dir(self) -> &'static str {
        match self.0.dir {
            Dir::Ltr => "ltr",
            Dir::Rtl => "rtl",
        }
    }

    /// Is this language still a preview?
    pub fn is_preview(self) -> bool {
        self.0.review.is_preview()
    }

    /// A handle for a definition (tests build their own languages).
    pub fn from_def(def: &'static LangDef) -> Lang {
        Lang(def)
    }

    /// The language for a language code (`ja`, `ja-JP`, `JA_jp`): the primary subtag decides. `None` for a code
    /// no language of this build answers to.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        Lang::from_tag_in(languages(), tag)
    }

    fn from_tag_in(available: &[&'static LangDef], tag: &str) -> Option<Lang> {
        let primary = tag.trim().split(['-', '_']).next()?.to_ascii_lowercase();
        available.iter().find(|d| d.tag == primary).map(|d| Lang(d))
    }

    /// Negotiate the page language: the `lang` cookie when it names a language of this build, else the best
    /// `Accept-Language` entry (q-values; `q=0` refused; `*` ignored), else English.
    ///
    /// **A preview is never chosen for a reader, only by the reader.** The cookie is the reader's own choice
    /// and names any language of the build; `Accept-Language` is a browser's setting, and a translation no
    /// fluent reader has checked is not served on its strength (DR-7; the owner's "no display says more than is
    /// known"). A preview that is signed off ([`Review::Reviewed`]) is negotiated like any other language.
    pub fn negotiate(accept_language: Option<&str>, cookie: Option<&str>) -> Lang {
        Lang::negotiate_in(languages(), accept_language, cookie)
    }

    fn negotiate_in(
        available: &[&'static LangDef],
        accept_language: Option<&str>,
        cookie: Option<&str>,
    ) -> Lang {
        if let Some(l) = cookie.and_then(|c| Lang::from_tag_in(available, c)) {
            return l;
        }
        let mut best: Option<(f32, usize, Lang)> = None;
        for (i, part) in accept_language
            .unwrap_or("")
            .split(',')
            .take(MAX_ACCEPTED)
            .enumerate()
        {
            let mut fields = part.trim().split(';');
            let tag = fields.next().unwrap_or("").trim();
            let q = fields
                .find_map(|p| p.trim().strip_prefix("q=").map(str::trim))
                .map_or(Some(1.0), |v| v.parse::<f32>().ok());
            // An unreadable or out-of-range weight makes the entry unusable, not preferred.
            let Some(q) = q.filter(|q| q.is_finite() && *q > 0.0 && *q <= 1.0) else {
                continue;
            };
            if tag == "*" {
                continue;
            }
            if let Some(l) = Lang::from_tag_in(available, tag).filter(|l| !l.is_preview())
                && best.is_none_or(|(bq, bi, _)| q > bq || (q == bq && i < bi))
            {
                best = Some((q, i, l));
            }
        }
        best.map_or(EN, |(_, _, l)| l)
    }

    fn entry(self, key: Key) -> Option<&'static Entry> {
        self.0.entries.iter().find(|e| e.key == key.id())
    }

    /// A number with the language's digit grouping.
    pub fn number(self, n: u64) -> String {
        let digits = n.to_string();
        let mut out = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(self.0.group_separator);
            }
            out.push(c);
        }
        out
    }
}

macro_rules! keys {
    ($($variant:ident => $id:literal,)*) => {
        /// A message key. The enum is the compile-time check: naming a key that does not exist does not
        /// compile; the identifier is what the tables and the tests use.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Key {
            $(
                #[doc = $id]
                $variant,
            )*
        }

        impl Key {
            /// Every key, in declaration order (the order of the tables).
            pub const ALL: &'static [Key] = &[$(Key::$variant),*];

            /// The stable identifier.
            pub fn id(self) -> &'static str {
                match self { $(Key::$variant => $id),* }
            }

            /// The variant's name as Rust and the templates write it (`Key::NotFoundBody` is `NotFoundBody`).
            pub fn variant(self) -> &'static str {
                match self { $(Key::$variant => stringify!($variant)),* }
            }
        }
    };
}

keys! {
    // layout
    LayoutSkip => "layout.skip",
    LayoutLanguage => "layout.language",
    LayoutFooter => "layout.footer",
    // the language switcher and its page
    TitleLanguage => "title.language",
    LanguageHeading => "language.heading",
    LanguageIntro => "language.intro",
    LanguageInUse => "language.in_use",
    LanguagePreviewChoice => "language.preview_choice",
    LanguagePreviewNote => "language.preview_note",
    LanguageBack => "language.back",
    // not found
    TitleNotFound => "title.not_found",
    NotFoundHeading => "not_found.heading",
    NotFoundBody => "not_found.body",
    // a switch that was refused
    TitleLanguageRefused => "title.language_refused",
    LanguageRefusedHeading => "language_refused.heading",
    LanguageRefusedBody => "language_refused.body",
    LanguageRefusedLink => "language_refused.link",
    TitleFormRefused => "title.form_refused",
    FormRefusedHeading => "form_refused.heading",
    FormRefusedBody => "form_refused.body",
    // answers that are not pages
    PageRenderFailed => "page.render_failed",
    MisdirectedBody => "misdirected.body",
}

/// A typed value for a placeholder. Text is HTML-escaped; a link is an escaped label plus an `href` the
/// **code** supplies; a name in another language is escaped text marked with its `lang`. There is no way to
/// hand the catalog's output raw markup.
#[derive(Clone, Debug)]
pub enum Slot {
    /// Plain text.
    Text(String),
    /// A link: the label is text, the `href` comes from code (a local path, or an `http(s)` URL; anything else
    /// renders as the label alone).
    Link {
        /// What the link says.
        label: String,
        /// Where it goes.
        href: String,
    },
    /// Text in another language than the sentence's (a language's name in itself): `<span lang="…">`.
    InLang {
        /// The BCP 47 tag of the text.
        tag: String,
        /// The text.
        text: String,
    },
}

fn escape_into(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
}

/// An `href` the code may put in a link: a local path (not protocol-relative) or an `http(s)` URL, without
/// spaces or control characters.
fn href_ok(href: &str) -> bool {
    let local = href.starts_with('/') && !href.starts_with("//") && !href.starts_with("/\\");
    let remote = href.starts_with("https://") || href.starts_with("http://");
    (local || remote) && !href.chars().any(|c| c.is_control() || c.is_whitespace())
}

/// A language tag fit for an attribute value: letters, digits and hyphens, 1 to 35 characters.
fn tag_ok(tag: &str) -> bool {
    (1..=35).contains(&tag.len()) && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

impl Slot {
    fn plain(&self) -> &str {
        match self {
            Slot::Text(s) => s,
            Slot::Link { label, .. } => label,
            Slot::InLang { text, .. } => text,
        }
    }

    fn html_into(&self, out: &mut String) {
        match self {
            Slot::Text(s) => escape_into(out, s),
            Slot::Link { label, href } if href_ok(href) => {
                out.push_str("<a href=\"");
                escape_into(out, href);
                out.push_str("\">");
                escape_into(out, label);
                out.push_str("</a>");
            }
            Slot::Link { label, .. } => escape_into(out, label),
            Slot::InLang { tag, text } if tag_ok(tag) => {
                out.push_str("<span lang=\"");
                out.push_str(tag);
                out.push_str("\">");
                escape_into(out, text);
                out.push_str("</span>");
            }
            Slot::InLang { text, .. } => escape_into(out, text),
        }
    }
}

/// Markup this module built from escaped parts: askama prints it as it is. Only [`Msg::html`] and
/// [`in_lang`] make one, so nothing else can be passed off as safe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rich(String);

impl Rich {
    /// The markup.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Rich {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl HtmlSafe for Rich {}

/// `text` marked as being in the language `tag` (`<span lang="ja">…</span>`), escaped. For a name that
/// stands alone, such as a language's name in itself; inside a sentence use [`Msg::in_lang`].
pub fn in_lang(tag: &str, text: &str) -> Rich {
    let mut out = String::new();
    Slot::InLang {
        tag: tag.to_owned(),
        text: text.to_owned(),
    }
    .html_into(&mut out);
    Rich(out)
}

/// A message being filled in.
#[derive(Clone, Debug)]
pub struct Msg {
    lang: Lang,
    key: Key,
    args: Vec<(&'static str, Slot)>,
    count: Option<u64>,
}

impl Msg {
    /// A plain-text placeholder.
    #[must_use]
    pub fn with(mut self, name: &'static str, value: impl ToString) -> Msg {
        self.args.push((name, Slot::Text(value.to_string())));
        self
    }

    /// A placeholder that is a link: the label is text, the `href` comes from code.
    #[must_use]
    pub fn link(mut self, name: &'static str, label: impl ToString, href: impl ToString) -> Msg {
        self.args.push((
            name,
            Slot::Link {
                label: label.to_string(),
                href: href.to_string(),
            },
        ));
        self
    }

    /// A placeholder holding text in another language, marked with its `lang`.
    #[must_use]
    pub fn in_lang(mut self, name: &'static str, tag: impl ToString, text: impl ToString) -> Msg {
        self.args.push((
            name,
            Slot::InLang {
                tag: tag.to_string(),
                text: text.to_string(),
            },
        ));
        self
    }

    /// The product name as `{product}`.
    #[must_use]
    pub fn product(self) -> Msg {
        self.with("product", PRODUCT)
    }

    /// A count: picks the plural form for the language and fills `{count}` with the grouped number.
    #[must_use]
    pub fn n(mut self, n: u64) -> Msg {
        self.count = Some(n);
        self.args.push(("count", Slot::Text(self.lang.number(n))));
        self
    }

    fn template(&self) -> String {
        match self.lang.entry(self.key) {
            Some(e) => match self.count.map(|n| (self.lang.0.plural)(n)) {
                Some(Plural::One) => e.one.to_owned(),
                _ => e.other.to_owned(),
            },
            // Never a crash, never a silent fallback to another language: a visible marker. The audit fails
            // the build before a shipped language can come here.
            None => format!("[{}]", self.key.id()),
        }
    }

    fn render(&self, html: bool) -> String {
        let text = self.template();
        let mut out = String::with_capacity(text.len() + 16);
        let literal = |out: &mut String, s: &str| {
            if html {
                escape_into(out, s);
            } else {
                out.push_str(s);
            }
        };
        let mut rest = text.as_str();
        while let Some(open) = rest.find('{') {
            let (before, from_open) = rest.split_at(open);
            literal(&mut out, before);
            let Some(close) = from_open.find('}') else {
                literal(&mut out, from_open);
                rest = "";
                break;
            };
            let (token, after) = from_open.split_at(close + 1);
            let name = token.get(1..close).unwrap_or("");
            match self.args.iter().find(|(k, _)| *k == name) {
                Some((_, slot)) if html => slot.html_into(&mut out),
                Some((_, slot)) => out.push_str(slot.plain()),
                // An unfilled placeholder stays visible, and the tests catch it.
                None => literal(&mut out, token),
            }
            rest = after;
        }
        literal(&mut out, rest);
        out
    }

    /// The message as plain text (`<title>`, attributes, plain-text answers); whoever prints it escapes it.
    pub fn text(&self) -> String {
        self.render(false)
    }

    /// The message as HTML made of escaped parts and the typed slots.
    pub fn html(&self) -> Rich {
        Rich(self.render(true))
    }
}

impl fmt::Display for Msg {
    /// Plain text: askama escapes it on output.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text())
    }
}

/// What a page holds: its language, and a builder per message.
#[derive(Clone, Copy, Debug)]
pub struct T {
    lang: Lang,
}

impl T {
    /// Messages in `lang`.
    pub fn new(lang: Lang) -> T {
        T { lang }
    }

    /// The page's language.
    pub fn lang(&self) -> Lang {
        self.lang
    }

    /// The message `key`, ready to be filled in.
    pub fn msg(&self, key: Key) -> Msg {
        Msg {
            lang: self.lang,
            key,
            args: Vec::new(),
            count: None,
        }
    }

    /// A message with no placeholders, as plain text.
    pub fn get(&self, key: Key) -> String {
        self.msg(key).text()
    }

    /// The running version, for the footer: a number, not a translatable word.
    pub fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

/// Every placeholder name in a message text, sorted and de-duplicated.
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut v = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let (_, from_open) = rest.split_at(open);
        match from_open.find('}') {
            Some(close) => {
                if let Some(name) = from_open.get(1..close) {
                    v.push(name);
                }
                rest = from_open.get(close + 1..).unwrap_or("");
            }
            None => break,
        }
    }
    v.sort_unstable();
    v.dedup();
    v
}

/// Are the braces of `text` exactly its placeholders: each `{` closed by a `}` with a name of lower-case
/// letters, digits and underscores between, and no other brace?
fn braces_ok(text: &str) -> bool {
    let mut open = false;
    let mut name = String::new();
    for c in text.chars() {
        match c {
            '{' if !open => {
                open = true;
                name.clear();
            }
            '}' if open => {
                let first_ok = name.chars().next().is_some_and(|f| f.is_ascii_lowercase());
                let all_ok = name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
                if !(first_ok && all_ok) {
                    return false;
                }
                open = false;
            }
            '{' | '}' => return false,
            c if open => name.push(c),
            _ => {}
        }
    }
    !open
}

/// What an [`audit`] can find wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A key of [`Key::ALL`] a language has no entry for.
    Missing,
    /// An entry no [`Key`] variant names.
    Orphan,
    /// A key a table lists twice.
    Duplicate,
    /// Placeholder names that differ from the first language's.
    PlaceholdersDiffer,
    /// Plural forms that do not fit the language's rule (a counted message needs its two forms where the
    /// language has two, and one where it has one; an uncounted one has no second form).
    PluralFormsDisagree,
    /// Markup, an entity or a URL in a message.
    Markup,
    /// A brace that is not part of a well-formed `{name}` placeholder.
    Braces,
}

/// One thing wrong with a set of tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Problem {
    /// What is wrong.
    pub kind: Kind,
    /// The language's tag.
    pub lang: &'static str,
    /// The message's identifier.
    pub key: &'static str,
}

impl Problem {
    /// A problem of `kind` in the language `lang`, about `key`.
    pub fn new(kind: Kind, lang: &'static str, key: &'static str) -> Problem {
        Problem { kind, lang, key }
    }
}

fn placeholder_set(e: &Entry) -> Vec<&'static str> {
    let mut v = placeholders(e.one);
    v.extend(placeholders(e.other));
    v.sort_unstable();
    v.dedup();
    v
}

/// The completeness and consistency checks over `langs` (the first is the base the others are compared with):
/// every key in every language, no entry the enum does not name, no duplicate, the same placeholder names
/// everywhere, well-formed braces, plural forms that fit the language's rule, and no markup, entity or URL in
/// any message.
pub fn audit(langs: &[&'static LangDef]) -> Vec<Problem> {
    let mut problems = Vec::new();
    let base = langs.first().copied();
    for def in langs {
        for k in Key::ALL {
            if !def.entries.iter().any(|e| e.key == k.id()) {
                problems.push(Problem::new(Kind::Missing, def.tag, k.id()));
            }
        }
        let mut seen: Vec<&str> = Vec::new();
        for e in def.entries {
            if !Key::ALL.iter().any(|k| k.id() == e.key) {
                problems.push(Problem::new(Kind::Orphan, def.tag, e.key));
            }
            if seen.contains(&e.key) {
                problems.push(Problem::new(Kind::Duplicate, def.tag, e.key));
            }
            seen.push(e.key);
            if [e.one, e.other]
                .iter()
                .any(|text| text.contains(['<', '>', '&']) || text.contains("://"))
            {
                problems.push(Problem::new(Kind::Markup, def.tag, e.key));
            }
            if ![e.one, e.other].iter().all(|text| braces_ok(text)) {
                problems.push(Problem::new(Kind::Braces, def.tag, e.key));
            }
            // A counted message says both forms where the language has two; a language whose rule has only
            // `other` must not pretend to have two; an uncounted message has one text.
            let counted = placeholder_set(e).contains(&"count");
            let has_one_form = (0..=2).any(|n| (def.plural)(n) == Plural::One);
            let one_text = e.one == e.other;
            let fits = match (counted, has_one_form) {
                (false, _) | (true, false) => one_text,
                (true, true) => !one_text,
            };
            if !fits {
                problems.push(Problem::new(Kind::PluralFormsDisagree, def.tag, e.key));
            }
            if let Some(b) = base.filter(|b| b.tag != def.tag)
                && let Some(be) = b.entries.iter().find(|x| x.key == e.key)
                && placeholder_set(be) != placeholder_set(e)
            {
                problems.push(Problem::new(Kind::PlaceholdersDiffer, def.tag, e.key));
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests;
