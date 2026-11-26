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

/// Japanese: **signed off by the owner, 2026-11-19** (DR-7; maintenance M60), the messages of 0.3.0. It is
/// negotiated like any other language. Later messages are reviewed at each release, and the accessibility
/// statement says so.
pub static JA_DEF: LangDef = LangDef {
    tag: "ja",
    endonym: ja::ENDONYM,
    dir: Dir::Ltr,
    plural: plural_ja,
    group_separator: ',',
    review: Review::Reviewed {
        by: "the owner, 2026-11-19",
    },
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
    // wiki shell
    WikiNav => "wiki.nav",
    WikiNavAbout => "wiki.nav_about",
    WikiNavVerification => "wiki.nav_verification",
    WikiNavWikis => "wiki.nav_wikis",
    WikiViewInUse => "wiki.view_in_use",
    TitlePage => "title.page",
    // a page
    PageRedirectedFrom => "page.redirected_from",
    PageAliasFrom => "page.alias_from",
    PageMatchedLoosely => "page.matched_loosely",
    PageStale => "page.stale",
    PageContents => "page.contents",
    PageLicense => "page.license",
    // a page that is not there, or not one
    MissingHeading => "missing.heading",
    MissingBody => "missing.body",
    MissingInvite => "missing.invite",
    AmbiguousHeading => "ambiguous.heading",
    AmbiguousBody => "ambiguous.body",
    RedirectHeading => "redirect.heading",
    RedirectDangling => "redirect.dangling",
    RedirectLoop => "redirect.loop",
    RedirectTooLong => "redirect.too_long",
    UnreadableHeading => "unreadable.heading",
    UnreadableNotText => "unreadable.not_text",
    UnreadableInfo => "unreadable.info",
    UnreadableInfoLong => "unreadable.info_long",
    UnreadableAdvice => "unreadable.advice",
    UnavailableHeading => "unavailable.heading",
    UnavailableBody => "unavailable.body",
    UncheckedHeading => "unchecked.heading",
    UncheckedBody => "unchecked.body",
    NoViewHeading => "no_view.heading",
    NoViewBody => "no_view.body",
    InvalidTitleHeading => "invalid_title.heading",
    InvalidTitleBody => "invalid_title.body",
    // verification
    AssuranceVerified => "assurance.verified",
    AssuranceVerificationFailed => "assurance.verification_failed",
    AssuranceCouldNotBeCheckedNow => "assurance.could_not_be_checked_now",
    AssuranceNotCheckedYet => "assurance.not_checked_yet",
    BadgeSentence => "badge.sentence",
    TitleVerification => "title.verification",
    VerificationHeading => "verification.heading",
    ExplainVerification => "explain.verification",
    VerificationExplainSummary => "verification.explain_summary",
    VerificationStateVerified => "verification.state_verified",
    VerificationStateFailed => "verification.state_failed",
    VerificationStateCouldNot => "verification.state_could_not",
    VerificationStateUnknown => "verification.state_unknown",
    VerificationCheckedAt => "verification.checked_at",
    VerificationCheckNow => "verification.check_now",
    VerificationRecent => "verification.recent",
    VerificationFoundHeading => "verification.found_heading",
    VerificationFoundIntro => "verification.found_intro",
    VerificationNowVerified => "verification.now_verified",
    VerificationNotSaved => "verification.not_saved",
    VerificationBusy => "verification.busy",
    VerificationLimit => "verification.limit",
    VerificationUnavailable => "verification.unavailable",
    VerificationAttemptAt => "verification.attempt_at",
    VerificationRunning => "verification.running",
    VerificationExplainedByRetirement => "verification.explained_by_retirement",
    // about, and the list of wikis
    TitleAbout => "title.about",
    AboutHeading => "about.heading",
    AboutLicense => "about.license",
    AboutView => "about.view",
    AccessibilityHeading => "about.accessibility_heading",
    AccessibilityTools => "about.accessibility_tools",
    AccessibilityPeople => "about.accessibility_people",
    AccessibilityLanguages => "about.accessibility_languages",
    TitleWikis => "title.wikis",
    WikisHeading => "wikis.heading",
    WikisIntro => "wikis.intro",
    WikisNone => "wikis.none",
    // transclusion (a page that includes another)
    TransclusionFrom => "transclusion.from",
    TransclusionCycle => "transclusion.cycle",
    TransclusionTooDeep => "transclusion.too_deep",
    TransclusionOverBudget => "transclusion.over_budget",
    // a request this address does not take
    TitleMethodNotAllowed => "title.method_not_allowed",
    MethodNotAllowedHeading => "method_not_allowed.heading",
    MethodNotAllowedBody => "method_not_allowed.body",
    // the read API's problem details (handoff 003 T8): a title, a detail and a hint for each stable code
    ProblemNotFoundTitle => "problem.not_found.title",
    ProblemNotFoundDetail => "problem.not_found.detail",
    ProblemNotFoundHint => "problem.not_found.hint",
    ProblemPageNotFoundTitle => "problem.page_not_found.title",
    ProblemPageNotFoundDetail => "problem.page_not_found.detail",
    ProblemPageNotFoundHint => "problem.page_not_found.hint",
    ProblemAmbiguousTitleTitle => "problem.ambiguous_title.title",
    ProblemAmbiguousTitleDetail => "problem.ambiguous_title.detail",
    ProblemAmbiguousTitleHint => "problem.ambiguous_title.hint",
    ProblemRedirectBrokenTitle => "problem.redirect_broken.title",
    ProblemRedirectBrokenDetail => "problem.redirect_broken.detail",
    ProblemRedirectBrokenHint => "problem.redirect_broken.hint",
    ProblemPageUnreadableTitle => "problem.page_unreadable.title",
    ProblemPageUnreadableDetail => "problem.page_unreadable.detail",
    ProblemPageUnreadableHint => "problem.page_unreadable.hint",
    ProblemInvalidTitleTitle => "problem.invalid_title.title",
    ProblemInvalidTitleDetail => "problem.invalid_title.detail",
    ProblemInvalidTitleHint => "problem.invalid_title.hint",
    ProblemNoSuchViewTitle => "problem.no_such_view.title",
    ProblemNoSuchViewDetail => "problem.no_such_view.detail",
    ProblemNoSuchViewHint => "problem.no_such_view.hint",
    ProblemInvalidParameterTitle => "problem.invalid_parameter.title",
    ProblemInvalidParameterDetail => "problem.invalid_parameter.detail",
    ProblemInvalidParameterHint => "problem.invalid_parameter.hint",
    ProblemDiffPathRequiredTitle => "problem.diff_path_required.title",
    ProblemDiffPathRequiredDetail => "problem.diff_path_required.detail",
    ProblemDiffPathRequiredHint => "problem.diff_path_required.hint",
    ProblemInvalidCursorTitle => "problem.invalid_cursor.title",
    ProblemInvalidCursorDetail => "problem.invalid_cursor.detail",
    ProblemInvalidCursorHint => "problem.invalid_cursor.hint",
    ProblemNotAvailableTitle => "problem.not_available.title",
    ProblemNotAvailableDetail => "problem.not_available.detail",
    ProblemNotAvailableHint => "problem.not_available.hint",
    ProblemUnavailableTitle => "problem.unavailable.title",
    ProblemUnavailableDetail => "problem.unavailable.detail",
    ProblemUnavailableHint => "problem.unavailable.hint",
    ProblemUnavailableUncheckedDetail => "problem.unavailable.unchecked_detail",
    ProblemFailedTitle => "problem.failed.title",
    ProblemFailedDetail => "problem.failed.detail",
    ProblemFailedHint => "problem.failed.hint",
    ProblemMethodNotAllowedTitle => "problem.method_not_allowed.title",
    ProblemMethodNotAllowedDetail => "problem.method_not_allowed.detail",
    ProblemMethodNotAllowedHint => "problem.method_not_allowed.hint",
    // history, change, diff and permalinks (handoff 003 T5)
    PageLinkHistory => "page.link_history",
    TitleHistory => "title.history",
    HistoryHeading => "history.heading",
    HistoryIntro => "history.intro",
    HistoryCurrent => "history.current",
    HistoryNone => "history.none",
    HistoryNoneSoFar => "history.none_so_far",
    HistoryOlder => "history.older",
    HistoryBeyond => "history.beyond",
    HistoryChangeLabel => "history.change_label",
    HistoryLinkDiff => "history.link_diff",
    HistoryLinkVersion => "history.link_version",
    PeopleProposedBy => "people.proposed_by",
    PeopleAcceptedBy => "people.accepted_by",
    PeopleMadeBy => "people.made_by",
    PeopleService => "people.service",
    PeopleAnonymous => "people.anonymous",
    PeopleUnrecorded => "people.unrecorded",
    PeopleImported => "people.imported",
    PeopleImportedNoSummary => "people.imported_no_summary",
    PeopleImportedName => "people.imported_name",
    PeopleImportedHidden => "people.imported_hidden",
    PeopleImportedUnnamed => "people.imported_unnamed",
    PeopleImportedDate => "people.imported_date",
    ImportNotice => "import.notice",
    ImportNoticeOrigin => "import.notice_origin",
    ImportNoticeOriginUnnamed => "import.notice_origin_unnamed",
    ImportNoticeChecksVerified => "import.notice_checks_verified",
    ImportNoticeChecksNone => "import.notice_checks_none",
    ImportNoticeLicenseDeclared => "import.notice_license_declared",
    FlagUndo => "flag.undo",
    FlagImport => "flag.import",
    FlagService => "flag.service",
    FlagFirst => "flag.first",
    EventCreated => "event.created",
    EventEdited => "event.edited",
    EventDeleted => "event.deleted",
    EventMovedFrom => "event.moved_from",
    EventMovedTo => "event.moved_to",
    EventNotInspected => "event.not_inspected",
    TitleChange => "title.change",
    ChangeHeading => "change.heading",
    ChangeSigned => "change.signed",
    ChangeSignedBy => "change.signed_by",
    ChangeSignedService => "change.signed_service",
    ChangeDidHeading => "change.did_heading",
    ChangeDidNone => "change.did_none",
    ChangeDidAdded => "change.did_added",
    ChangeDidEdited => "change.did_edited",
    ChangeDidRemoved => "change.did_removed",
    ChangeDidMoved => "change.did_moved",
    ChangeDidFileAdded => "change.did_file_added",
    ChangeDidFileEdited => "change.did_file_edited",
    ChangeDidFileRemoved => "change.did_file_removed",
    ChangeDidFileMoved => "change.did_file_moved",
    ChangeDidMore => "change.did_more",
    ChangeTooLarge => "change.too_large",
    ChangeLinkDiff => "change.link_diff",
    ChangeNoneHeading => "change_none.heading",
    ChangeNoneBody => "change_none.body",
    TitleDiff => "title.diff",
    DiffHeading => "diff.heading",
    DiffFromTo => "diff.from_to",
    DiffSummaryHeading => "diff.summary_heading",
    DiffSummaryAdded => "diff.summary_added",
    DiffSummaryRemoved => "diff.summary_removed",
    DiffSummaryNone => "diff.summary_none",
    DiffStatusAdded => "diff.status_added",
    DiffStatusRemoved => "diff.status_removed",
    DiffStatusUnchanged => "diff.status_unchanged",
    DiffNoLines => "diff.no_lines",
    DiffTruncated => "diff.truncated",
    DiffRefinement => "diff.refinement",
    DiffHunkNear => "diff.hunk_near",
    DiffHunkPlain => "diff.hunk_plain",
    DiffHunkRaw => "diff.hunk_raw",
    DiffAdded => "diff.added_label",
    DiffRemoved => "diff.removed_label",
    DiffNoNewline => "diff.no_newline",
    TitleAt => "title.at",
    AtBanner => "at.banner",
    AtCurrent => "at.current",
    AtCurrentLabel => "at.current_label",
    AtLinks => "at.links",
    AtMissingHeading => "at.missing_heading",
    AtMissingBody => "at.missing_body",
    // handoff 003 T6: changes, feeds, search, health, backlinks, the head of a page
    WikiNavChanges => "wiki.nav_changes",
    WikiNavSearch => "wiki.nav_search",
    WikiNavHealth => "wiki.nav_health",
    TitleChanges => "title.changes",
    ChangesHeading => "changes.heading",
    ChangesIntro => "changes.intro",
    ChangesSeenNote => "changes.seen_note",
    ChangesFilterLegend => "changes.filter_legend",
    ChangesFilterNamespace => "changes.filter_namespace",
    ChangesFilterNamespaceHint => "changes.filter_namespace_hint",
    ChangesFilterView => "changes.filter_view",
    ChangesAllViews => "changes.all_views",
    ChangesFilterAutomated => "changes.filter_automated",
    ChangesOptShow => "changes.opt_show",
    ChangesOptHide => "changes.opt_hide",
    ChangesOptOnly => "changes.opt_only",
    ChangesFilterApply => "changes.filter_apply",
    ChangesActiveHeading => "changes.active_heading",
    ChangesActiveNamespace => "changes.active_namespace",
    ChangesActivePage => "changes.active_page",
    ChangesActiveView => "changes.active_view",
    ChangesActiveHide => "changes.active_hide",
    ChangesActiveOnly => "changes.active_only",
    ChangesClear => "changes.clear",
    ChangesCaption => "changes.caption",
    ChangesColChange => "changes.col_change",
    ChangesColPeople => "changes.col_people",
    ChangesColPages => "changes.col_pages",
    ChangesColSeen => "changes.col_seen",
    ChangesProposedBy => "changes.proposed_by",
    ChangesAcceptedBy => "changes.accepted_by",
    ChangesMadeBy => "changes.made_by",
    ChangesPersonAnonymous => "changes.person_anonymous",
    ChangesPersonService => "changes.person_service",
    ChangesForeign => "changes.foreign",
    ChangesRollback => "changes.rollback",
    ChangesNoPages => "changes.no_pages",
    ChangesPagesUnavailable => "changes.pages_unavailable",
    ChangesPageGone => "changes.page_gone",
    ChangesNone => "changes.none",
    ChangesMore => "changes.more",
    ChangesPagesMissing => "changes.pages_missing",
    ChangesHidden => "changes.hidden",
    ChangesFeed => "changes.feed",
    FeedTitleWiki => "feed.title_wiki",
    FeedTitleNamespace => "feed.title_namespace",
    FeedTitlePage => "feed.title_page",
    FeedPages => "feed.pages",
    FeedProposalsTitle => "feed.proposals_title",
    FeedProposalOpened => "feed.proposal_opened",
    FeedProposalDecided => "feed.proposal_decided",
    FeedProposalBy => "feed.proposal_by",
    TitleSearch => "title.search",
    SearchHeading => "search.heading",
    SearchLabel => "search.label",
    SearchButton => "search.button",
    SearchScope => "search.scope",
    SearchEmpty => "search.empty",
    SearchResultsHeading => "search.results_heading",
    SearchMatchedTitle => "search.matched_title",
    SearchMatchedAlias => "search.matched_alias",
    SearchNone => "search.none",
    SearchCut => "search.cut",
    SearchExact => "search.exact",
    SearchSimilarHeading => "search.similar_heading",
    SearchCreateHeading => "search.create_heading",
    SearchCreate => "search.create",
    SearchOpenSearch => "opensearch.description",
    TitleHealth => "title.health",
    HealthHeading => "health.heading",
    HealthIntro => "health.intro",
    HealthWantedName => "health.wanted_name",
    HealthWantedDesc => "health.wanted_desc",
    HealthOrphansName => "health.orphans_name",
    HealthOrphansDesc => "health.orphans_desc",
    HealthDeadEndsName => "health.dead_ends_name",
    HealthDeadEndsDesc => "health.dead_ends_desc",
    HealthRedirectsName => "health.redirects_name",
    HealthRedirectsDesc => "health.redirects_desc",
    HealthHelpName => "health.help_name",
    HealthHelpDesc => "health.help_desc",
    HealthStaleName => "health.stale_name",
    HealthStaleDesc => "health.stale_desc",
    HealthBack => "health.back",
    HealthColPage => "health.col_page",
    HealthColWanted => "health.col_wanted",
    HealthColLinked => "health.col_linked",
    HealthColAction => "health.col_action",
    HealthColTarget => "health.col_target",
    HealthColProblem => "health.col_problem",
    HealthColMark => "health.col_mark",
    HealthColChanged => "health.col_changed",
    HealthSeeLinks => "health.see_links",
    HealthProposePage => "health.propose_page",
    HealthWantedAction => "health.wanted_action",
    HealthOrphansAction => "health.orphans_action",
    HealthDeadEndsAction => "health.dead_ends_action",
    HealthProblemBroken => "health.problem_broken",
    HealthProblemLoop => "health.problem_loop",
    HealthProblemLong => "health.problem_long",
    HealthProblemDouble => "health.problem_double",
    HealthActionBroken => "health.action_broken",
    HealthActionLoop => "health.action_loop",
    HealthActionLong => "health.action_long",
    HealthActionDouble => "health.action_double",
    HealthMarkWip => "health.mark_wip",
    HealthMarkDraft => "health.mark_draft",
    HealthMarkStub => "health.mark_stub",
    HealthHelpAction => "health.help_action",
    HealthStaleNever => "health.stale_never",
    HealthStaleSeen => "health.stale_seen",
    HealthStaleAction => "health.stale_action",
    HealthStaleWindow => "health.stale_window",
    HealthNoneWanted => "health.none_wanted",
    HealthNoneOrphans => "health.none_orphans",
    HealthNoneDeadEnds => "health.none_dead_ends",
    HealthNoneRedirects => "health.none_redirects",
    HealthNoneHelp => "health.none_help",
    HealthNoneStale => "health.none_stale",
    HealthKnowledgeName => "health.knowledge_name",
    HealthKnowledgeDesc => "health.knowledge_desc",
    HealthColFile => "health.col_file",
    HealthColRule => "health.col_rule",
    HealthKnowledgeAction => "health.knowledge_action",
    HealthNoneKnowledge => "health.none_knowledge",
    HealthRulePath => "health.rule_path",
    HealthRuleNotText => "health.rule_not_text",
    HealthRuleTooLong => "health.rule_too_long",
    HealthRuleSyntax => "health.rule_syntax",
    HealthRuleMissing => "health.rule_missing",
    HealthRuleUnknown => "health.rule_unknown",
    HealthRuleType => "health.rule_type",
    HealthRuleNumber => "health.rule_number",
    HealthRuleValue => "health.rule_value",
    HealthRuleId => "health.rule_id",
    HealthRuleDuplicate => "health.rule_duplicate",
    HealthRuleBounds => "health.rule_bounds",
    HealthRuleReference => "health.rule_reference",
    HealthRuleAnchor => "health.rule_anchor",
    HealthRuleInUse => "health.rule_in_use",
    HealthRuleOwned => "health.rule_owned",
    HealthRuleStatus => "health.rule_status",
    HealthRuleJudged => "health.rule_judged",
    TitleLinks => "title.links",
    LinksHeading => "links.heading",
    LinksMissing => "links.missing",
    LinksGoTo => "links.go_to",
    LinksNone => "links.none",
    LinksKindInclude => "links.kind_include",
    LinksKindRedirect => "links.kind_redirect",
    LinksCut => "links.cut",
    HeadFeedWiki => "head.feed_wiki",
    HeadFeedPage => "head.feed_page",
    HeadMarkdown => "head.markdown",
    HeadSearch => "head.search",
    // the API's problem for a change that is not in the history (handoff 003 T8, completion)
    ProblemChangeNotFoundTitle => "problem.change_not_found.title",
    ProblemChangeNotFoundDetail => "problem.change_not_found.detail",
    ProblemChangeNotFoundHint => "problem.change_not_found.hint",
    // the write API's problems (handoff 004 T7)
    ProblemAuthenticationRequiredTitle => "problem.authentication_required.title",
    ProblemAuthenticationRequiredDetail => "problem.authentication_required.detail",
    ProblemAuthenticationRequiredHint => "problem.authentication_required.hint",
    ProblemInvalidTokenTitle => "problem.invalid_token.title",
    ProblemInvalidTokenDetail => "problem.invalid_token.detail",
    ProblemInvalidTokenHint => "problem.invalid_token.hint",
    ProblemForbiddenTitle => "problem.forbidden.title",
    ProblemForbiddenDetail => "problem.forbidden.detail",
    ProblemForbiddenHint => "problem.forbidden.hint",
    ProblemBaseRequiredTitle => "problem.base_required.title",
    ProblemBaseRequiredDetail => "problem.base_required.detail",
    ProblemBaseRequiredHint => "problem.base_required.hint",
    ProblemStaleBaseTitle => "problem.stale_base.title",
    ProblemStaleBaseDetail => "problem.stale_base.detail",
    ProblemStaleBaseHint => "problem.stale_base.hint",
    ProblemInvalidBodyTitle => "problem.invalid_body.title",
    ProblemInvalidBodyDetail => "problem.invalid_body.detail",
    ProblemInvalidBodyHint => "problem.invalid_body.hint",
    ProblemBodyTooLargeTitle => "problem.body_too_large.title",
    ProblemBodyTooLargeDetail => "problem.body_too_large.detail",
    ProblemBodyTooLargeHint => "problem.body_too_large.hint",
    ProblemUnsupportedMediaTypeTitle => "problem.unsupported_media_type.title",
    ProblemUnsupportedMediaTypeDetail => "problem.unsupported_media_type.detail",
    ProblemUnsupportedMediaTypeHint => "problem.unsupported_media_type.hint",
    ProblemUnacceptableTitle => "problem.unacceptable.title",
    ProblemUnacceptableDetail => "problem.unacceptable.detail",
    ProblemUnacceptableHint => "problem.unacceptable.hint",
    ProblemNotInThisStateTitle => "problem.not_in_this_state.title",
    ProblemNotInThisStateDetail => "problem.not_in_this_state.detail",
    ProblemNotInThisStateHint => "problem.not_in_this_state.hint",
    ProblemAlreadyDecidedTitle => "problem.already_decided.title",
    ProblemAlreadyDecidedDetail => "problem.already_decided.detail",
    ProblemAlreadyDecidedHint => "problem.already_decided.hint",
    ProblemOwnProposalTitle => "problem.own_proposal.title",
    ProblemOwnProposalDetail => "problem.own_proposal.detail",
    ProblemOwnProposalHint => "problem.own_proposal.hint",
    ProblemRefusedLimitTitle => "problem.refused_limit.title",
    ProblemRefusedLimitDetail => "problem.refused_limit.detail",
    ProblemRefusedLimitHint => "problem.refused_limit.hint",
    ProblemBusyTitle => "problem.busy.title",
    ProblemBusyDetail => "problem.busy.detail",
    ProblemBusyHint => "problem.busy.hint",
    ProblemNeedsRecoveryTitle => "problem.needs_recovery.title",
    ProblemNeedsRecoveryDetail => "problem.needs_recovery.detail",
    ProblemNeedsRecoveryHint => "problem.needs_recovery.hint",
    ProblemOverlapTitle => "problem.overlap.title",
    ProblemOverlapDetail => "problem.overlap.detail",
    ProblemOverlapHint => "problem.overlap.hint",
    ProblemNothingToChangeTitle => "problem.nothing_to_change.title",
    ProblemNothingToChangeDetail => "problem.nothing_to_change.detail",
    ProblemNothingToChangeHint => "problem.nothing_to_change.hint",
    ProblemAcceptanceInProgressTitle => "problem.acceptance_in_progress.title",
    ProblemAcceptanceInProgressDetail => "problem.acceptance_in_progress.detail",
    ProblemAcceptanceInProgressHint => "problem.acceptance_in_progress.hint",
    ProblemAcceptanceFailedTitle => "problem.acceptance_failed.title",
    ProblemAcceptanceFailedDetail => "problem.acceptance_failed.detail",
    ProblemAcceptanceFailedHint => "problem.acceptance_failed.hint",
    ProblemSigningNotConfiguredTitle => "problem.signing_not_configured.title",
    ProblemSigningNotConfiguredDetail => "problem.signing_not_configured.detail",
    ProblemSigningNotConfiguredHint => "problem.signing_not_configured.hint",
    ProblemRetirementUnfinishedTitle => "problem.retirement_unfinished.title",
    ProblemRetirementUnfinishedDetail => "problem.retirement_unfinished.detail",
    ProblemRetirementUnfinishedHint => "problem.retirement_unfinished.hint",
    // the inbox, following a page, and the flags of changes (handoff 004 T6)
    TitleInbox => "title.inbox",
    InboxHeading => "inbox.heading",
    InboxIntro => "inbox.intro",
    InboxNone => "inbox.none",
    InboxNew => "inbox.new",
    InboxMarkAllRead => "inbox.mark_all_read",
    InboxSomeone => "inbox.someone",
    InboxEventReviewed => "inbox.event_reviewed",
    InboxEventAccepted => "inbox.event_accepted",
    InboxEventDeclined => "inbox.event_declined",
    InboxEventDeclinedByModeration => "inbox.event_declined_by_moderation",
    InboxEventDeclinedByModerationNoCategory => "inbox.event_declined_by_moderation_no_category",
    InboxEventNeedsRevision => "inbox.event_needs_revision",
    InboxEventWithdrawn => "inbox.event_withdrawn",
    InboxEventSuperseded => "inbox.event_superseded",
    InboxEventReplied => "inbox.event_replied",
    InboxEventMentioned => "inbox.event_mentioned",
    InboxEventWatched => "inbox.event_watched",
    InboxEventUnreadable => "inbox.event_unreadable",
    InboxAccountHeading => "inbox.account_heading",
    InboxAccountBody => "inbox.account_body",
    InboxFollowingHeading => "inbox.following_heading",
    InboxFollowingNone => "inbox.following_none",
    InboxFollowingPage => "inbox.following_page",
    InboxFollowingNamespace => "inbox.following_namespace",
    InboxUnfollow => "inbox.unfollow",
    WatchHeadingFollowed => "watch.heading_followed",
    WatchHeadingStopped => "watch.heading_stopped",
    WatchHeadingFull => "watch.heading_full",
    WatchPageFollowed => "watch.page_followed",
    WatchNamespaceFollowed => "watch.namespace_followed",
    WatchPageStopped => "watch.page_stopped",
    WatchNamespaceStopped => "watch.namespace_stopped",
    WatchAccountBody => "watch.account_body",
    WatchFullBody => "watch.full_body",
    WatchBack => "watch.back",
    WatchToInbox => "watch.to_inbox",
    ChangesFilterMinor => "changes.filter_minor",
    ChangesActiveMinorHide => "changes.active_minor_hide",
    ChangesActiveMinorOnly => "changes.active_minor_only",
    ChangesMinorLabel => "changes.minor_label",
    ChangesMachineAssisted => "changes.machine_assisted",
    ChangesMachineAssistedOnly => "changes.machine_assisted_only",
    // proposals (handoff 004 T3): begin
    WikiNavProposals => "wiki.nav_proposals",
    PageProposeChange => "page.propose_change",
    PageProposePart => "page.propose_part",
    PageDiscussion => "page.discussion",
    ProposeNewPageLink => "propose.new_page_link",
    StateDraft => "state.draft",
    StateHeld => "state.held",
    StateNeedsRevision => "state.needs_revision",
    StateAccepted => "state.accepted",
    StateDeclined => "state.declined",
    StateWithdrawn => "state.withdrawn",
    StateSuperseded => "state.superseded",
    NoteDraft => "state_note.draft",
    NoteHeld => "state_note.held",
    NoteNeedsRevision => "state_note.needs_revision",
    NoteAccepted => "state_note.accepted",
    NoteDeclined => "state_note.declined",
    NoteWithdrawn => "state_note.withdrawn",
    NoteSuperseded => "state_note.superseded",
    SupersededBy => "proposal.superseded_by",
    ProposalAnother => "proposal.another",
    ReasonDeclined => "proposal.reason_declined",
    ReasonDeclinedByModeration => "proposal.reason_declined_by_moderation",
    ReasonWithdrawn => "proposal.reason_withdrawn",
    ReasonAsked => "proposal.reason_asked",
    OutcomeSigned => "outcome.signed",
    OutcomeOverlap => "outcome.overlap",
    OutcomeNotAllowed => "outcome.not_allowed",
    OutcomeBusy => "outcome.busy",
    OutcomeRecovery => "outcome.recovery",
    OutcomeRefused => "outcome.refused",
    OutcomeRefusedHeading => "outcome.refused_heading",
    OutcomeNoteSigned => "outcome_note.signed",
    OutcomeNoteOverlap => "outcome_note.overlap",
    OutcomeNoteNotAllowed => "outcome_note.not_allowed",
    OutcomeNoteBusy => "outcome_note.busy",
    OutcomeNoteBusyDepth => "outcome_note.busy_depth",
    OutcomeNoteRecovery => "outcome_note.recovery",
    OutcomeUnchangedRefused => "outcome.unchanged_refused",
    OutcomeNoteUnchangedRefused => "outcome_note.unchanged_refused",
    OutcomeKnowledgeInvalid => "outcome.knowledge_invalid",
    OutcomeNoteKnowledgeInvalid => "outcome_note.knowledge_invalid",
    OutcomeTooLarge => "outcome.too_large",
    OutcomeNoteTooLarge => "outcome_note.too_large",
    OutcomeRetirementUnfinished => "outcome.retirement_unfinished",
    OutcomeNoteRetirementUnfinished => "outcome_note.retirement_unfinished",
    OutcomeUnavailable => "outcome.unavailable",
    OutcomeNoteUnavailable => "outcome_note.unavailable",
    OutcomeInProgress => "outcome.in_progress",
    OutcomeNoteInProgress => "outcome_note.in_progress",
    OutcomeNothingToChange => "outcome.nothing_to_change",
    OutcomeNoteNothingToChange => "outcome_note.nothing_to_change",
    OutcomeStore => "outcome.store",
    OutcomeNoteStore => "outcome_note.store",
    AcceptHeading => "accept.heading",
    AcceptNotServed => "accept.not_served",
    AcceptNotServedKept => "accept.not_served_kept",
    AcceptNotServedNote => "accept.not_served_note",
    AcceptUnconfiguredHeading => "accept.unconfigured_heading",
    AcceptUnconfigured => "accept.unconfigured",
    AcceptUnconfiguredNote => "accept.unconfigured_note",
    OverlapHeading => "overlap.heading",
    OverlapIntro => "overlap.intro",
    OverlapCaption => "overlap.caption",
    OverlapColWhere => "overlap.col_where",
    OverlapColTheirs => "overlap.col_theirs",
    OverlapColOurs => "overlap.col_ours",
    OverlapNothing => "overlap.nothing",
    OverlapRow => "overlap.row",
    OverlapPlaceTop => "overlap.place_top",
    OverlapPlaceKey => "overlap.place_key",
    OverlapPlacePart => "overlap.place_part",
    OverlapPlaceNumber => "overlap.place_number",
    OverlapPlaceNamed => "overlap.place_named",
    OverlapPlacePage => "overlap.place_page",
    OverlapPlacePath => "overlap.place_path",
    QuotaOpenProposals => "quota.open_proposals",
    QuotaRevisions => "quota.revisions",
    QuotaAcceptedBytes => "quota.accepted_bytes",
    QuotaHourPerson => "quota.hour_person",
    QuotaHourWiki => "quota.hour_wiki",
    QuotaQueue => "quota.queue",
    QuotaCommentsHourPerson => "quota.comments_hour_person",
    QuotaCounts => "quota.counts",
    ResetAt => "quota.reset_at",
    ResetProposalCloses => "quota.reset_proposal_closes",
    ResetQueue => "quota.reset_queue",
    ResetNeverRevisions => "quota.reset_never_revisions",
    ResetNeverTooBig => "quota.reset_never_too_big",
    SizeMib => "size.mib",
    SizeKib => "size.kib",
    SizeBytes => "size.bytes",
    NeedsAccountHeading => "needs_account.heading",
    NeedsAccountBody => "needs_account.body",
    NeedsAccountAdvice => "needs_account.advice",
    RefusedNotAllowedBody => "refused.not_allowed",
    RefusedNotNowHeading => "refused.not_now_heading",
    RefusedInvalidHeading => "refused.invalid_heading",
    RefusedNotInThisState => "refused.not_in_this_state",
    RefusedAlreadyDecided => "refused.already_decided",
    RefusedOwnProposal => "refused.own_proposal",
    RefusedStaleRevision => "refused.stale_revision",
    RefusedSuperseder => "refused.superseder",
    RefusedChanged => "refused.changed",
    InvalidIncomplete => "invalid.incomplete",
    InvalidPathCollides => "invalid.path_collides",
    InvalidFormTooLarge => "invalid.form_too_large",
    InvalidSummaryMissing => "invalid.summary_missing",
    InvalidSummaryTooLong => "invalid.summary_too_long",
    InvalidSummaryNotOneLine => "invalid.summary_not_one_line",
    InvalidTooManySources => "invalid.too_many_sources",
    InvalidBadSource => "invalid.bad_source",
    InvalidNothingProposed => "invalid.nothing_proposed",
    InvalidTooLarge => "invalid.too_large",
    InvalidPageTooLarge => "invalid.page_too_large",
    InvalidReasonMissing => "invalid.reason_missing",
    InvalidReasonTooLong => "invalid.reason_too_long",
    InvalidCommentEmpty => "invalid.comment_empty",
    InvalidCommentTooLong => "invalid.comment_too_long",
    InvalidCommentNul => "invalid.comment_nul",
    TitleProposals => "title.proposals",
    ProposalsHeading => "proposals.heading",
    ProposalsIntro => "proposals.intro",
    ProposalsEmpty => "proposals.empty",
    ProposalsNewPage => "proposals.new_page",
    ProposalsDrafts => "proposals.drafts",
    ProposalsOpen => "proposals.open",
    ProposalsClosed => "proposals.closed",
    ProposalsRowBy => "proposals.row_by",
    ProposalNoSummary => "proposal.no_summary",
    ProposalsBackToList => "proposals.back",
    ProposalBack => "proposal.back",
    PersonUnknown => "person.unknown",
    TitleProposeParts => "title.propose_parts",
    PartsHeading => "parts.heading",
    PartsIntro => "parts.intro",
    PartsWhole => "parts.whole",
    PartsSectionsHeading => "parts.sections_heading",
    PartsPartsHeading => "parts.parts_heading",
    PartsNoSections => "parts.no_sections",
    PartsSectionLabel => "parts.section_label",
    PartsPartLabel => "parts.part_label",
    PartsCut => "parts.cut",
    PartsBack => "parts.back",
    TitleProposePage => "title.propose_page",
    TitleProposeNew => "title.propose_new",
    TitleProposeRevise => "title.propose_revise",
    ProposeHeadingPage => "propose.heading_page",
    ProposeHeadingNew => "propose.heading_new",
    ProposeHeadingRevise => "propose.heading_revise",
    ProposeIntro => "propose.intro",
    ProposeScopeWhole => "propose.scope_whole",
    ProposeScopeSection => "propose.scope_section",
    ProposeScopePart => "propose.scope_part",
    ProposeChoose => "propose.choose",
    ProposeLabelText => "propose.label_text",
    ProposeLabelSection => "propose.label_section",
    ProposeLabelPart => "propose.label_part",
    ProposeLabelTitle => "propose.label_title",
    ProposeTitleHint => "propose.title_hint",
    ProposeNewPageName => "propose.new_page_name",
    ProposeLabelSummary => "propose.label_summary",
    ProposeSummaryHint => "propose.summary_hint",
    ProposeLabelSources => "propose.label_sources",
    ProposeSourcesHint => "propose.sources_hint",
    ProposePreview => "propose.preview",
    ProposeShowChanges => "propose.show_changes",
    ProposeSaveDraft => "propose.save_draft",
    ProposeSubmit => "propose.submit",
    ProposeSubmitRevision => "propose.submit_revision",
    ProposeSaved => "propose.saved",
    ProposeProblems => "propose.problems",
    ProposeContinueDraft => "propose.continue_draft",
    ProposeRemovedNote => "propose.removed_note",
    ProposePreviewHeading => "propose.preview_heading",
    ProposePreviewNote => "propose.preview_note",
    ProposeSuggestions => "propose.suggestions",
    ProposeFrontUnreadable => "propose.front_unreadable",
    ProposeFrontOwned => "propose.front_owned",
    ProposeTitleEmpty => "propose.title_empty",
    ProposeTitleReserved => "propose.title_reserved",
    ProposeTitleTooLong => "propose.title_too_long",
    ProposeTitleTooDeep => "propose.title_too_deep",
    ProposeTitleTaken => "propose.title_taken",
    ProposalChangesHeading => "changes.proposal_heading",
    ProposalChangesNone => "changes.proposal_none",
    ChangesPageHeading => "changes.page_heading",
    ChangesStatusAdded => "changes.status_added",
    ChangesStatusRemoved => "changes.status_removed",
    ChangesStatusModified => "changes.status_modified",
    ChangesInfoHeading => "changes.info_heading",
    ChangesPartChanged => "changes.part_changed",
    ChangesPartAdded => "changes.part_added",
    ChangesPartRemoved => "changes.part_removed",
    ChangesUnchangedParts => "changes.unchanged_parts",
    TitleProposal => "title.proposal",
    FactProposedBy => "proposal.proposed_by",
    FactPages => "proposal.pages",
    FactRevision => "proposal.revision",
    FactView => "proposal.view",
    SourcesHeading => "proposal.sources_heading",
    SignedChangeLink => "proposal.signed_change",
    ActionsHeading => "proposal.actions_heading",
    ActionContinue => "action.continue",
    ActionRevise => "action.revise",
    ActionWithdraw => "action.withdraw",
    ActionAsk => "action.ask",
    ActionDecline => "action.decline",
    ActionAccept => "action.accept",
    ThreadHeading => "thread.heading",
    ThreadNone => "thread.none",
    CommentMeta => "comment.meta",
    CommentOnPart => "comment.on_part",
    CommentOrphaned => "comment.orphaned",
    CommentOnThisPart => "comment.on_this_part",
    CommentReply => "comment.reply",
    CommentFormLabel => "comment_form.label",
    CommentFormButton => "comment_form.button",
    CommentFormAboutPart => "comment_form.about_part",
    CommentFormReplyTo => "comment_form.reply_to",
    DecisionsHeading => "decisions.heading",
    DecisionsNone => "decisions.none",
    DecisionApproved => "decision.approved",
    DecisionRequested => "decision.requested",
    DecisionOlder => "decision.older",
    DecideLabel => "decide.label",
    DecideApprove => "decide.approve",
    DecideRequest => "decide.request",
    TitleDecline => "title.decline",
    TitleWithdraw => "title.withdraw",
    TitleAsk => "title.ask",
    ReasonDeclineHeading => "reason.decline_heading",
    ReasonDeclineIntro => "reason.decline_intro",
    ReasonDeclineButton => "reason.decline_button",
    ReasonWithdrawHeading => "reason.withdraw_heading",
    ReasonWithdrawIntro => "reason.withdraw_intro",
    ReasonWithdrawButton => "reason.withdraw_button",
    ReasonAskHeading => "reason.ask_heading",
    ReasonAskIntro => "reason.ask_intro",
    ReasonAskButton => "reason.ask_button",
    ReasonLabel => "reason.label",
    TitleDiscussion => "title.discussion",
    DiscussionHeading => "discussion.heading",
    DiscussionIntro => "discussion.intro",
    DiscussionEmpty => "discussion.empty",
    DiscussionCut => "discussion.cut",
    DiscussionPage => "discussion.page",
    DiscussionPropose => "discussion.propose",
    LintBom => "lint.bom",
    LintCrlf => "lint.crlf",
    LintNewline => "lint.newline",
    LintTabs => "lint.tabs",
    LintNoInfo => "lint.no_info",
    LintNoTitle => "lint.no_title",
    LintCoerced => "lint.coerced",
    LintList => "lint.list",
    LintStatus => "lint.status",
    LintRedirect => "lint.redirect",
    LintVisibility => "lint.visibility",
    LintSecret => "lint.secret",
    LintDuplicateId => "lint.duplicate_id",
    LintUnclosed => "lint.unclosed",
    LintCallout => "lint.callout",
    LintContainer => "lint.container",
    LintHeadingH1 => "lint.heading_top",
    LintHeadingDup => "lint.heading_dup",
    LintTags => "lint.tags",
    LintEmptyLabel => "lint.empty_label",
    LintEmptyTarget => "lint.empty_target",
    LintMissingPartRef => "lint.missing_part_ref",
    LintPrefix => "lint.prefix",
    LintOrder => "lint.order",
    LintDangling => "lint.dangling",
    LintAmbiguous => "lint.ambiguous",
    LintOther => "lint.other",
    // proposals (handoff 004 T3): end
    // sign-in and accounts (the sign-in handoff U1, U2): begin
    HeaderAccountNav => "header.account_nav",
    HeaderSignedIn => "header.signed_in",
    HeaderSignOut => "header.sign_out",
    HeaderSignIn => "header.sign_in",
    TitleFormExpired => "title.form_expired",
    FormExpiredHeading => "form_expired.heading",
    FormExpiredBody => "form_expired.body",
    TitleWebOnly => "title.web_only",
    WebOnlyHeading => "web_only.heading",
    WebOnlyBody => "web_only.body",
    ProblemUnavailable => "problem.unavailable",
    TitleSignIn => "title.sign_in",
    SignInHeading => "sign_in.heading",
    SignInLoginLabel => "sign_in.login_label",
    SignInPasswordLabel => "sign_in.password_label",
    SignInButton => "sign_in.button",
    SignInFailed => "sign_in.failed",
    SignInLocked => "sign_in.locked",
    SignInBusy => "sign_in.busy",
    SignInAlready => "sign_in.already",
    SignInToAccount => "sign_in.to_account",
    SignInInvitationOnly => "sign_in.invitation_only",
    SignInJoin => "sign_in.join",
    TitleSignOut => "title.sign_out",
    SignOutHeading => "sign_out.heading",
    SignOutBody => "sign_out.body",
    SignOutNotSignedIn => "sign_out.not_signed_in",
    TitleJoin => "title.join",
    JoinHeading => "join.heading",
    JoinIntro => "join.intro",
    JoinButton => "join.button",
    JoinClosedHeading => "join_closed.heading",
    JoinClosedBody => "join_closed.body",
    TitleInvite => "title.invite",
    InviteHeading => "invite.heading",
    InviteIntro => "invite.intro",
    InviteButton => "invite.button",
    InviteUnusableHeading => "invite_unusable.heading",
    InviteUnusableBody => "invite_unusable.body",
    AccountFormNameLabel => "account_form.name_label",
    AccountFormNameHint => "account_form.name_hint",
    AccountFormLoginHint => "account_form.login_hint",
    AccountFormPasswordLabel => "account_form.password_label",
    AccountFormPasswordHint => "account_form.password_hint",
    AccountFormLoginTaken => "account_form.login_taken",
    AccountFormLoginLength => "account_form.login_length",
    AccountFormLoginCharacters => "account_form.login_characters",
    AccountFormNameLength => "account_form.name_length",
    AccountFormNameControl => "account_form.name_control",
    AccountFormNameSpace => "account_form.name_space",
    AccountFormPasswordShort => "account_form.password_short",
    AccountFormPasswordLong => "account_form.password_long",
    TitleAccount => "title.account",
    AccountHeading => "account.heading",
    AccountNameHeading => "account.name_heading",
    AccountNameLabel => "account.name_label",
    AccountNameHint => "account.name_hint",
    AccountNameButton => "account.name_button",
    AccountNameSaved => "account.name_saved",
    AccountSessionsHeading => "account.sessions_heading",
    AccountSessionsIntro => "account.sessions_intro",
    AccountSessionLine => "account.session_line",
    AccountSessionCurrent => "account.session_current",
    AccountSessionEnd => "account.session_end",
    AccountSessionEnded => "account.session_ended",
    AccountTokensHeading => "account.tokens_heading",
    AccountTokensIntro => "account.tokens_intro",
    AccountTokensNone => "account.tokens_none",
    AccountTokenUnlabelled => "account.token_unlabelled",
    AccountTokenScope => "account.token_scope",
    AccountTokenLive => "account.token_live",
    AccountTokenExpired => "account.token_expired",
    AccountTokenRevoked => "account.token_revoked",
    AccountTokenRevoke => "account.token_revoke",
    AccountTokenRevokedNote => "account.token_revoked_note",
    AccountTokenNewHeading => "account.token_new_heading",
    AccountTokenLabel => "account.token_label",
    AccountTokenLabelHint => "account.token_label_hint",
    AccountTokenScopeLabel => "account.token_scope_label",
    AccountScopeRead => "account.scope_read",
    AccountScopePropose => "account.scope_propose",
    AccountScopeReview => "account.scope_review",
    AccountScopeAccept => "account.scope_accept",
    AccountScopeAdmin => "account.scope_admin",
    AccountTokenLifetimeLabel => "account.token_lifetime_label",
    AccountLifetimeDay => "account.lifetime_day",
    AccountLifetimeWeek => "account.lifetime_week",
    AccountLifetimeMonth => "account.lifetime_month",
    AccountLifetimeQuarter => "account.lifetime_quarter",
    AccountLifetimeYear => "account.lifetime_year",
    AccountTokenButton => "account.token_button",
    AccountTokenMadeHeading => "account.token_made_heading",
    AccountTokenMadeBody => "account.token_made_body",
    AccountTokenMadeUse => "account.token_made_use",
    AccountTokenTooMany => "account.token_too_many",
    AccountTokenLabelLong => "account.token_label_long",
    // sign-in and accounts (the sign-in handoff U1, U2): end
    // sign-in through a provider (the OpenID Connect handoff U3): begin
    SignInProvider => "sign_in.provider",
    SignInProviderJoins => "sign_in.provider_joins",
    SignInProviderFailed => "sign_in.provider_failed",
    SignInProviderState => "sign_in.provider_state",
    SignInProviderNotLinked => "sign_in.provider_not_linked",
    SignInProviderUnreachable => "sign_in.provider_unreachable",
    InviteProviderIntro => "invite.provider_intro",
    InviteProviderLink => "invite.provider_link",
    InviteProviderTaken => "invite.provider_taken",
    TitleLanding => "title.landing",
    LandingBody => "landing.body",
    LandingContinue => "landing.continue",
    SignInProviderNotNavigation => "sign_in.provider_not_navigation",
    AccountTokenPasswordLabel => "account.token_password_label",
    AccountTokenPasswordHint => "account.token_password_hint",
    AccountTokenPasswordWrong => "account.token_password_wrong",
    AccountTokenProviderHint => "account.token_provider_hint",
    AccountTokenSignInAgain => "account.token_sign_in_again",
    AccountPasswordHeading => "account.password_heading",
    AccountPasswordIntro => "account.password_intro",
    AccountPasswordNone => "account.password_none",
    AccountPasswordCurrentLabel => "account.password_current_label",
    AccountPasswordNewLabel => "account.password_new_label",
    AccountPasswordButton => "account.password_button",
    AccountPasswordWrong => "account.password_wrong",
    AccountPasswordChanged => "account.password_changed",
    // sign-in through a provider (the OpenID Connect handoff U3): end
    // the knowledge layer's reader (RFC 005 part 2 D-7; handoff 005 K-T6)
    PageLinkWhy => "page.link_why",
    ClaimLink => "claim.link",
    ClaimLinkName => "claim.link_name",
    ClaimLinkNameNth => "claim.link_name_nth",
    ClaimParagraphStart => "claim.paragraph_start",
    ClaimParagraphEmpty => "claim.paragraph_empty",
    TitleClaim => "title.claim",
    ClaimHeading => "claim.heading",
    ClaimKind => "claim.kind",
    ClaimKindObservation => "claim.kind_observation",
    ClaimKindInterpretation => "claim.kind_interpretation",
    ClaimKindHypothesis => "claim.kind_hypothesis",
    ClaimKindOpinion => "claim.kind_opinion",
    ClaimKindDecision => "claim.kind_decision",
    ClaimKindPolicy => "claim.kind_policy",
    ClaimKindQuestion => "claim.kind_question",
    ClaimKindUnknown => "claim.kind_unknown",
    ClaimSaidHeading => "claim.said_heading",
    ClaimSaidNowhere => "claim.said_nowhere",
    ClaimSourcesHeading => "claim.sources_heading",
    ClaimSourcesNone => "claim.sources_none",
    ClaimSourceLine => "claim.source_line",
    ClaimSourcePrimary => "claim.source_primary",
    ClaimSourceSecondary => "claim.source_secondary",
    ClaimSourceObservational => "claim.source_observational",
    ClaimSourceComputational => "claim.source_computational",
    ClaimSourceAnecdotal => "claim.source_anecdotal",
    ClaimSourceOther => "claim.source_other",
    ClaimSourceUnreadable => "claim.source_unreadable",
    ClaimSourceLineNamed => "claim.source_line_named",
    ClaimSourceTitled => "claim.source_titled",
    ClaimSourceUntitled => "claim.source_untitled",
    ClaimSourceBy => "claim.source_by",
    ClaimSourceDated => "claim.source_dated",
    ClaimSourceJoin => "claim.source_join",
    ClaimSourceFoundAt => "claim.source_found_at",
    ClaimSourceIdentifier => "claim.source_identifier",
    ClaimHowSupportsDirect => "claim.how_supports_direct",
    ClaimHowSupportsPartial => "claim.how_supports_partial",
    ClaimHowSupportsIndirect => "claim.how_supports_indirect",
    ClaimHowChallengesDirect => "claim.how_challenges_direct",
    ClaimHowChallengesPartial => "claim.how_challenges_partial",
    ClaimHowChallengesIndirect => "claim.how_challenges_indirect",
    ClaimJudgedEarlier => "claim.judged_earlier",
    ClaimStatusHeading => "claim.status_heading",
    ClaimStatusAccepted => "claim.status_accepted",
    ClaimStatusContested => "claim.status_contested",
    ClaimStatusUnresolved => "claim.status_unresolved",
    ClaimStatusUnknown => "claim.status_unknown",
    ClaimDecidedBy => "claim.decided_by",
    ClaimDecidedLink => "claim.decided_link",
    ClaimDecidedUnlisted => "claim.decided_unlisted",
    ClaimNoneHeading => "claim.none_heading",
    ClaimNoneBody => "claim.none_body",
    WhyHeading => "why.heading",
    WhyBack => "why.back",
    // views: creating and comparing (K-T8)
    TitleViews => "title.views",
    ViewsHeading => "views.heading",
    ViewsIntro => "views.intro",
    ViewsDefaultName => "views.default_name",
    ViewsNamed => "views.named",
    ViewsDefaultNote => "views.default_note",
    ViewsKindExperimental => "views.kind_experimental",
    ViewsKindInterpretation => "views.kind_interpretation",
    ViewsKindExperimentalNote => "views.kind_experimental_note",
    ViewsKindInterpretationNote => "views.kind_interpretation_note",
    ViewsForkedFrom => "views.forked_from",
    ViewsNoOrigin => "views.no_origin",
    ViewsReadLink => "views.read_link",
    ViewsCompareWithDefault => "views.compare_with_default",
    ViewsOmitted => "views.omitted",
    ViewsOnlyDefault => "views.only_default",
    ViewsCreateLink => "views.create_link",
    ViewsCompareLink => "views.compare_link",
    ViewsAllLink => "views.all_link",
    TitleViewsNew => "title.views_new",
    ViewsNewHeading => "views_new.heading",
    ViewsNewIntro => "views_new.intro",
    ViewsNewNameLabel => "views_new.name_label",
    ViewsNewNameHint => "views_new.name_hint",
    ViewsNewKindLegend => "views_new.kind_legend",
    ViewsNewFromLabel => "views_new.from_label",
    ViewsNewSubmit => "views_new.submit",
    ViewsNewProblemNameEmpty => "views_new.problem_name_empty",
    ViewsNewProblemNameStart => "views_new.problem_name_start",
    ViewsNewProblemNameChars => "views_new.problem_name_chars",
    ViewsNewProblemNameLong => "views_new.problem_name_long",
    ViewsNewProblemNameRetired => "views_new.problem_name_retired",
    ViewsNewProblemKind => "views_new.problem_kind",
    ViewsNewProblemFrom => "views_new.problem_from",
    ViewsNewProblemTaken => "views_new.problem_taken",
    ViewsNewNotAllowed => "views_new.not_allowed",
    ViewsNewBusy => "views_new.busy",
    ViewsNewTooManyHeading => "views_new.too_many_heading",
    ViewsNewTooMany => "views_new.too_many",
    ViewsNewRetirementHeading => "views_new.retirement_heading",
    ViewsNewRetirement => "views_new.retirement",
    ViewsNewFailedHeading => "views_new.failed_heading",
    ViewsNewFailed => "views_new.failed",
    ViewsNewUncertain => "views_new.uncertain",
    ViewsNewNotKeptHeading => "views_new.not_kept_heading",
    ViewsNewNotKept => "views_new.not_kept",
    ViewsNewUnconfiguredHeading => "views_new.unconfigured_heading",
    ViewsNewUnconfigured => "views_new.unconfigured",
    TitleCompare => "title.compare",
    CompareHeading => "compare.heading",
    CompareIntro => "compare.intro",
    CompareALabel => "compare.a_label",
    CompareBLabel => "compare.b_label",
    CompareGo => "compare.go",
    ComparePickTwo => "compare.pick_two",
    ComparePair => "compare.pair",
    CompareSame => "compare.same",
    CompareClaimsHeading => "compare.claims_heading",
    CompareClaimsNone => "compare.claims_none",
    CompareClaimSaidIn => "compare.claim_said_in",
    CompareClaimUnplaced => "compare.claim_unplaced",
    CompareDiffWording => "compare.diff_wording",
    CompareDiffStatus => "compare.diff_status",
    CompareDiffSources => "compare.diff_sources",
    CompareOnlyIn => "compare.only_in",
    CompareUnreadableIn => "compare.unreadable_in",
    CompareStatusAccepted => "compare.status_accepted",
    CompareStatusContested => "compare.status_contested",
    CompareStatusUnresolved => "compare.status_unresolved",
    CompareStatusUnknown => "compare.status_unknown",
    CompareClaimsOmitted => "compare.claims_omitted",
    ComparePagesHeading => "compare.pages_heading",
    ComparePagesNone => "compare.pages_none",
    ComparePageDiffers => "compare.page_differs",
    ComparePageLink => "compare.page_link",
    ComparePagesOmitted => "compare.pages_omitted",
    CompareBack => "compare.back",
    TitleComparePage => "title.compare_page",
    ComparePageHeading => "compare.page_heading",
    ComparePageRange => "compare.page_range",
    ComparePageRead => "compare.page_read",
    ComparePageBack => "compare.page_back",
    ComparePageAbsent => "compare.page_absent",
    ClaimOthersHeading => "claim.others_heading",
    ClaimOthersNone => "claim.others_none",
    ClaimOthersCompare => "claim.others_compare",
    ClaimOthersLimit => "claim.others_limit",
    ClaimOthersUnchecked => "claim.others_unchecked",
    // views: creating and comparing (K-T8): end
    // the knowledge layer's reader (K-T6): end
    // Why is this here? (RFC 005 part 2 D-8; handoff 005 K-T7)
    TitleWhy => "title.why",
    WhyIntro => "why.intro",
    WhyHistoryLink => "why.history_link",
    WhyBounded => "why.bounded",
    WhyBoundedNone => "why.bounded_none",
    WhySpentNone => "why.spent_none",
    WhyUnfollowed => "why.unfollowed",
    WhyLeftOut => "why.left_out",
    WhyParagraphsHeading => "why.paragraphs_heading",
    WhyNoParagraphs => "why.no_paragraphs",
    WhyParagraphLabel => "why.paragraph_label",
    WhyWrittenUnchanged => "why.written_unchanged",
    WhyLastChanged => "why.last_changed",
    WhyFirstWritten => "why.first_written",
    WhyFirstBefore => "why.first_before",
    WhyUnchangedRead => "why.unchanged_read",
    WhyFoundWrittenUnchanged => "why.found_written_unchanged",
    WhyFoundLast => "why.found_last",
    WhyFoundFirst => "why.found_first",
    WhyBasisAnchor => "why.basis_anchor",
    WhyBasisText => "why.basis_text",
    WhyChangesHeading => "why.changes_heading",
    WhyChangesNone => "why.changes_none",
    WhyChangeOpen => "why.change_open",
    WhyProposal => "why.proposal",
    WhyProposalNamed => "why.proposal_named",
    WhyProposalUnlisted => "why.proposal_unlisted",
    WhyProposalUnconfirmed => "why.proposal_unconfirmed",
    WhyReviewHeading => "why.review_heading",
    WhyReviewNone => "why.review_none",
    WhyDecisionWords => "why.decision_words",
    WhyComment => "why.comment",
    WhyReviewMore => "why.review_more",
    WhySourcesHeading => "why.sources_heading",
    WhyAttachedHeading => "why.attached_heading",
    WhyAttachedNoReference => "why.attached_no_reference",
    WhyAttachedCited => "why.attached_cited",
    WhyAttachedTooLarge => "why.attached_too_large",
    WhyAttachedUnreadable => "why.attached_unreadable",
    WhyAttachedHidden => "why.attached_hidden",
    WhyAttachedMore => "why.attached_more",
    // Why is this here? (K-T7): end
    // knowledge: claims and sources in the proposal path (handoff 005 K-T5): start
    PartsAttach => "parts.attach",
    PartsChallenge => "parts.challenge",
    PartsAttachFor => "parts.attach_for",
    PartsChallengeFor => "parts.challenge_for",
    TitleAttach => "title.attach",
    TitleChallenge => "title.challenge",
    AttachHeadingSupports => "attach.heading_supports",
    AttachHeadingChallenges => "attach.heading_challenges",
    AttachIntroSupports => "attach.intro_supports",
    AttachIntroChallenges => "attach.intro_challenges",
    AttachClaimExists => "attach.claim_exists",
    AttachLegend => "attach.legend",
    AttachLabelKind => "attach.label_kind",
    AttachKindPrimary => "attach.kind_primary",
    AttachKindSecondary => "attach.kind_secondary",
    AttachKindObservational => "attach.kind_observational",
    AttachKindComputational => "attach.kind_computational",
    AttachKindAnecdotal => "attach.kind_anecdotal",
    AttachKindOther => "attach.kind_other",
    AttachLabelTitle => "attach.label_title",
    AttachLabelAuthor => "attach.label_author",
    AttachLabelDate => "attach.label_date",
    AttachHintDate => "attach.hint_date",
    AttachLabelRefForm => "attach.label_ref_form",
    AttachRefUrl => "attach.ref_url",
    AttachRefDoi => "attach.ref_doi",
    AttachRefIsbn => "attach.ref_isbn",
    AttachRefPage => "attach.ref_page",
    AttachRefCitation => "attach.ref_citation",
    AttachLabelRef => "attach.label_ref",
    AttachHintRef => "attach.hint_ref",
    AttachLabelBearing => "attach.label_bearing",
    AttachBearingDirect => "attach.bearing_direct",
    AttachBearingPartial => "attach.bearing_partial",
    AttachBearingIndirect => "attach.bearing_indirect",
    AttachLabelNote => "attach.label_note",
    AttachHintNote => "attach.hint_note",
    AttachLabelClaimKind => "attach.label_claim_kind",
    AttachClaimKindHint => "attach.claim_kind_hint",
    AttachClaimKindObservation => "attach.claim_kind_observation",
    AttachClaimKindInterpretation => "attach.claim_kind_interpretation",
    AttachClaimKindHypothesis => "attach.claim_kind_hypothesis",
    AttachClaimKindOpinion => "attach.claim_kind_opinion",
    AttachClaimKindDecision => "attach.claim_kind_decision",
    AttachClaimKindPolicy => "attach.claim_kind_policy",
    AttachClaimKindQuestion => "attach.claim_kind_question",
    AttachClaimKindUnknown => "attach.claim_kind_unknown",
    AttachNoParagraph => "attach.no_paragraph",
    AttachPageUnreadable => "attach.page_unreadable",
    AttachSeveralClaims => "attach.several_claims",
    AttachFull => "attach.full",
    AttachSaysNothing => "attach.says_nothing",
    AttachTooLong => "attach.too_long",
    AttachControl => "attach.control",
    AttachNotWeb => "attach.not_web",
    AttachUnwritable => "attach.unwritable",
    CurateNotADay => "curate.not_a_day",
    CurateUnknownSource => "curate.unknown_source",
    CurateNoWording => "curate.no_wording",
    CurateDayNotCome => "curate.day_not_come",
    ProposeClaimHeldOne => "propose.claim_held_one",
    ProposeClaimHeldMany => "propose.claim_held_many",
    ProposeClaimLost => "propose.claim_lost",
    ProposeClaimDropLegend => "propose.claim_drop_legend",
    ProposeClaimDrop => "propose.claim_drop",
    KnowHeading => "know.heading",
    KnowClaimMade => "know.claim_made",
    KnowCite => "know.cite",
    KnowCiteUnjudged => "know.cite_unjudged",
    KnowSourceRecorded => "know.source_recorded",
    KnowClaimChanged => "know.claim_changed",
    KnowSourceChanged => "know.source_changed",
    KnowRemovedClaim => "know.removed_claim",
    KnowRemovedSource => "know.removed_source",
    KnowUnreadable => "know.unreadable",
    KnowChangeKind => "know.change_kind",
    KnowChangeLifecycle => "know.change_lifecycle",
    KnowChangeStanding => "know.change_standing",
    KnowChangeSaid => "know.change_said",
    KnowChangeUnsaid => "know.change_unsaid",
    KnowChangeRelated => "know.change_related",
    KnowChangeUnrelated => "know.change_unrelated",
    KnowChangeUncited => "know.change_uncited",
    KnowChangeRecited => "know.change_recited",
    KnowChangeNoted => "know.change_noted",
    KnowSourceChangeKind => "know.source_change_kind",
    KnowSourceChangeIdentity => "know.source_change_identity",
    KnowSourceChangeReference => "know.source_change_reference",
    KnowSourceChangeSnapshot => "know.source_change_snapshot",
    KnowSourceChangeReviewed => "know.source_change_reviewed",
    KnowSourceChangeUnreviewed => "know.source_change_unreviewed",
    KnowSourceChangeAvailability => "know.source_change_availability",
    KnowSourceChangeSuperseded => "know.source_change_superseded",
    KnowIsCuration => "know.is_curation",
    KnowNotCuration => "know.not_curation",
    KnowNotJudged => "know.not_judged",
    KnowKindObservation => "know.kind_observation",
    KnowKindInterpretation => "know.kind_interpretation",
    KnowKindHypothesis => "know.kind_hypothesis",
    KnowKindOpinion => "know.kind_opinion",
    KnowKindDecision => "know.kind_decision",
    KnowKindPolicy => "know.kind_policy",
    KnowKindQuestion => "know.kind_question",
    KnowKindUnknown => "know.kind_unknown",
    KnowSourcePrimary => "know.source_primary",
    KnowSourceSecondary => "know.source_secondary",
    KnowSourceObservational => "know.source_observational",
    KnowSourceComputational => "know.source_computational",
    KnowSourceAnecdotal => "know.source_anecdotal",
    KnowSourceOther => "know.source_other",
    KnowSupports => "know.supports",
    KnowChallenges => "know.challenges",
    KnowDirectly => "know.directly",
    KnowInPart => "know.in_part",
    KnowByImplication => "know.by_implication",
    KnowLifeProposed => "know.life_proposed",
    KnowLifeAdopted => "know.life_adopted",
    KnowLifeRetired => "know.life_retired",
    KnowLifeUnknown => "know.life_unknown",
    KnowStandingAccepted => "know.standing_accepted",
    KnowStandingContested => "know.standing_contested",
    KnowStandingUnresolved => "know.standing_unresolved",
    KnowStandingUnknown => "know.standing_unknown",
    KnowAvailable => "know.available",
    KnowUnavailable => "know.unavailable",
    KnowAvailabilityUnknown => "know.availability_unknown",
    KnowRelRefines => "know.rel_refines",
    KnowRelSupersedes => "know.rel_supersedes",
    KnowRelContradicts => "know.rel_contradicts",
    KnowRelSplitFrom => "know.rel_split_from",
    KnowRelMergedInto => "know.rel_merged_into",
    // knowledge: claims and sources in the proposal path (handoff 005 K-T5): end
    // knowledge: the curation proposal (handoff 005 K-T5): start
    TitleCurate => "title.curate",
    CurateHeading => "curate.heading",
    CurateIntro => "curate.intro",
    CurateWording => "curate.wording",
    CurateWordingNone => "curate.wording_none",
    CurateLabelStatus => "curate.label_status",
    CurateStatusHint => "curate.status_hint",
    CurateEarlier => "curate.earlier",
    CurateSourceLegend => "curate.source_legend",
    CurateSourceUntitled => "curate.source_untitled",
    CurateLabelRelation => "curate.label_relation",
    CurateLabelBearing => "curate.label_bearing",
    CurateLabelReviewed => "curate.label_reviewed",
    CurateReviewedHint => "curate.reviewed_hint",
    CurateLabelAvailability => "curate.label_availability",
    CurateAvailabilityHint => "curate.availability_hint",
    CurateConfirm => "curate.confirm",
    CurateConfirmHint => "curate.confirm_hint",
    CurateSubmit => "curate.submit",
    CurateNothing => "curate.nothing",
    // knowledge: the curation proposal (handoff 005 K-T5): end
    // knowledge: the claims a change touches (handoff 005 K-T5): start
    TouchHeading => "touch.heading",
    TouchIntro => "touch.intro",
    TouchReworded => "touch.reworded",
    TouchRemoved => "touch.removed",
    TouchLinksEarlier => "touch.links_earlier",
    TouchStandingEarlier => "touch.standing_earlier",
    TouchNothingEarlier => "touch.nothing_earlier",
    TouchReconfirmHint => "touch.reconfirm_hint",
    TouchReconfirmButton => "touch.reconfirm_button",
    TouchReconfirmLater => "touch.reconfirm_later",
    TouchReconfirmSummary => "touch.reconfirm_summary",
    TouchNothingToJudge => "touch.nothing_to_judge",
    TouchReconfirmOpen => "touch.reconfirm_open",
    TouchReconfirmDone => "touch.reconfirm_done",
    TouchReconfirmOf => "touch.reconfirm_of",
    // knowledge: the claims a change touches (handoff 005 K-T5): end
    InvalidKnowledgeNotShown => "invalid.knowledge_not_shown",
    InvalidKnowledgeUnjudged => "invalid.knowledge_unjudged",
    InvalidKnowledgeStatusUnjudged => "invalid.knowledge_status_unjudged",
    KnowChangeRejudged => "know.change_rejudged",
    KnowChangeRejudgedUnseen => "know.change_rejudged_unseen",
    KnowChangeRejudgedCleared => "know.change_rejudged_cleared",
    KnowRejudgedStatus => "know.rejudged_status",
    KnowRejudgedSource => "know.rejudged_source",
    KnowNothingChanges => "know.nothing_changes",
    // knowledge: re-judgements shown (handoff 005 corrections K5): end
    AttachDirection => "attach.direction",
    // knowledge: direction characters refused (handoff 005 corrections K14): end
    KnowReplacedUnreadableClaim => "know.replaced_unreadable_claim",
    KnowReplacedUnreadableSource => "know.replaced_unreadable_source",
    KnowMadeLifecycle => "know.made_lifecycle",
    KnowMadeRelated => "know.made_related",
    KnowSourceMadeAvailability => "know.source_made_availability",
    KnowSourceWhere => "know.source_where",
    KnowSourceCopy => "know.source_copy",
    KnowSourceChangeUnsuperseded => "know.source_change_unsuperseded",
    KnowSourceChangeReferenceCleared => "know.source_change_reference_cleared",
    KnowSourceNameless => "know.source_nameless",
    // knowledge: a file that cannot be read is replaced (handoff 005 corrections, second round K16): end
    KnowSubjectClaim => "know.subject_claim",
    KnowSubjectClaimId => "know.subject_claim_id",
    KnowSubjectSourceId => "know.subject_source_id",
    KnowSubjectWords => "know.subject_words",
    KnowSubjectParagraph => "know.subject_paragraph",
    KnowSubjectParagraphBlock => "know.subject_paragraph_block",
    // knowledge: subjects told apart (handoff 005 corrections, third round K24): end
    KnowRemovedHadStanding => "know.removed_had_standing",
    KnowRemovedHadLink => "know.removed_had_link",
    KnowRemovedHadReviewed => "know.removed_had_reviewed",
    KnowRemovedHadAvailability => "know.removed_had_availability",
    // knowledge: what leaves with a removal (handoff 005 corrections, third round K27): end
    HealthSourcesName => "health.sources_name",
    HealthSourcesDesc => "health.sources_desc",
    HealthColSource => "health.col_source",
    HealthColWhy => "health.col_why",
    HealthColClaims => "health.col_claims",
    HealthSourcesAction => "health.sources_action",
    HealthNoneSources => "health.none_sources",
    HealthSourcesUnavailable => "health.sources_unavailable",
    HealthSourcesNever => "health.sources_never",
    HealthSourcesLast => "health.sources_last",
    HealthSourcesOverdue => "health.sources_overdue",
    HealthSourcesClaims => "health.sources_claims",
    HealthSourcesCut => "health.sources_cut",
    HealthSourcesNoClaims => "health.sources_no_claims",
    HealthSourcesClaim => "health.sources_claim",
    HealthSourcesClaimId => "health.sources_claim_id",
    HealthSourcesClaimNowhere => "health.sources_claim_nowhere",
    HealthSourcesSetting => "health.sources_setting",
    HealthSourcesFuture => "health.sources_future",
    HealthSourcesShowing => "health.sources_showing",
    HealthSourcesMore => "health.sources_more",
    HealthSourcesFirst => "health.sources_first",
    HealthSourcesNoMore => "health.sources_no_more",
    KnowCurateLink => "know.curate_link",
    KnowCurateLinkOf => "know.curate_link_of",
    KnowCurateMore => "know.curate_more",
    // knowledge: the freshness list's pages, a future day, the curation form's link (handoff 005 corrections, fourth round K39, K40): end
    ClaimThreadHeading => "claim_thread.heading",
    ClaimThreadNeverChanges => "claim_thread.never_changes",
    ClaimThreadTotal => "claim_thread.total",
    ClaimThreadNone => "claim_thread.none",
    ClaimThreadMeta => "claim_thread.meta",
    ClaimThreadInReplyTo => "claim_thread.in_reply_to",
    ClaimThreadMore => "claim_thread.more",
    ClaimThreadFirst => "claim_thread.first",
    ClaimThreadUnavailable => "claim_thread.unavailable",
    ClaimThreadFormLabel => "claim_thread.form_label",
    ClaimThreadFormReply => "claim_thread.form_reply",
    ClaimThreadBack => "claim_thread.back",
    ApiClaimLost => "api.claim_lost",
    ApiDropUnused => "api.drop_unused",
    ApiBaseHidden => "api.base_hidden",
    ClaimThreadInReplyToLink => "claim_thread.in_reply_to_link",
    ClaimThreadCommentLabel => "claim_thread.comment_label",
    CurateLabelRemove => "curate.label_remove",
    CurateRemoveHint => "curate.remove_hint",
    CurateWorkHint => "curate.work_hint",
    CurateLabelSuperseded => "curate.label_superseded",
    CurateSupersededHint => "curate.superseded_hint",
    CurateRemovedAndChanged => "curate.removed_and_changed",
    CurateReplacedByItself => "curate.replaced_by_itself",
    CurateUnknownSuccessor => "curate.unknown_successor",
    CurateSuccessorCycle => "curate.successor_cycle",
    KnowNotByForm => "know.not_by_form",
    KnowSourceSpacingTitle => "know.source_spacing_title",
    KnowSourceSpacingAuthor => "know.source_spacing_author",
    KnowSourceSpacingDate => "know.source_spacing_date",
    KnowSourceSpacingReference => "know.source_spacing_reference",
    KnowSourceSpacingNone => "know.source_spacing_none",
    KnowSourceReviewStays => "know.source_review_stays",
    KnowSourceCitedBy => "know.source_cited_by",
    KnowSourceCitedByMore => "know.source_cited_by_more",
    CurateSuccessorCycleAt => "curate.successor_cycle_at",
    InvalidKnowledgeCheckUnstated => "invalid.knowledge_check_unstated",
    CurateLabelCheckedDay => "curate.label_checked_day",
    CurateCheckedDayHint => "curate.checked_day_hint",
    CurateLabelCheckedAvailability => "curate.label_checked_availability",
    CurateCheckedAvailabilityHint => "curate.checked_availability_hint",
    CurateCheckNoDay => "curate.check_no_day",
    CurateCheckNotADay => "curate.check_not_a_day",
    CurateCheckBeforeReview => "curate.check_before_review",
    CurateCheckNoNewReference => "curate.check_no_new_reference",
    KnowCheckUnstated => "know.check_unstated",
    OutcomeNoteKnowledgeKept => "outcome_note.knowledge_kept",
    // knowledge: a changed reference clears the check; the cycle names its source (handoff 005 corrections, seventh round K66, K69): end
    CurateCheckOldDay => "curate.check_old_day",
    CurateCheckOldAnswer => "curate.check_old_answer",
    // knowledge: the old fields beside a moved reference are refused (handoff 005 corrections, ninth round K76): end
    KnowCheckBeforeReview => "know.check_before_review",
    KnowCheckNotCome => "know.check_not_come",
    // knowledge: a bound refusal at acceptance says its own sentence (handoff 005 corrections, tenth round K79): end
    OutcomeGovernanceInvalid => "outcome.governance_invalid",
    OutcomeNoteGovernanceInvalid => "outcome_note.governance_invalid",
    GovernanceFaultLine => "governance.fault_line",
    GovernanceFaultFile => "governance.fault_file",
    GovernanceRuleNotText => "governance.rule_not_text",
    GovernanceRuleTooLong => "governance.rule_too_long",
    GovernanceRuleWrongShape => "governance.rule_wrong_shape",
    GovernanceRuleUnknownKey => "governance.rule_unknown_key",
    GovernanceRuleRepeatedKey => "governance.rule_repeated_key",
    GovernanceRuleUnknownValue => "governance.rule_unknown_value",
    GovernanceRuleOutOfRange => "governance.rule_out_of_range",
    GovernanceRuleBounds => "governance.rule_bounds",
    GovernanceRuleForbiddenCharacter => "governance.rule_forbidden_character",
    GovernanceRuleMissingKey => "governance.rule_missing_key",
    GovernanceRuleDuplicate => "governance.rule_duplicate",
    GovernanceRuleOrderingProposeAboveReview => "governance.rule_ordering_propose_above_review",
    GovernanceRuleOrderingReviewAboveAccept => "governance.rule_ordering_review_above_accept",
    GovernanceRuleOrderingModerateBelowReviewer => "governance.rule_ordering_moderate_below_reviewer",
    GovernanceRuleOrderingDecideAppealBelowModerate => "governance.rule_ordering_decide_appeal_below_moderate",
    GovernanceRulePlatformLimitEntryMax => "governance.rule_platform_limit_entry_max",
    GovernanceRulePlatformLimitLimitDays => "governance.rule_platform_limit_limit_days",
    GovernanceRuleOutsideDefaultView => "governance.rule_outside_default_view",
    GovernanceRuleUnknownFile => "governance.rule_unknown_file",
    GovernanceRuleNotAFile => "governance.rule_not_a_file",
    GovernanceRulePrefixIsNamespace => "governance.rule_prefix_is_namespace",
    GovernanceRuleTemplateNotHttps => "governance.rule_template_not_https",
    GovernanceRuleTemplateHost => "governance.rule_template_host",
    GovernanceRuleTemplateUserinfo => "governance.rule_template_userinfo",
    GovernanceRuleTemplatePlaceholder => "governance.rule_template_placeholder",
    GovernanceRuleTemplateCharacter => "governance.rule_template_character",
    OutcomeGovernanceRung => "outcome.governance_rung",
    OutcomeNoteGovernanceRungMaintainer => "outcome_note.governance_rung_maintainer",
    OutcomeNoteGovernanceRungAdmin => "outcome_note.governance_rung_admin",
    OutcomeApprovalsMissing => "outcome.approvals_missing",
    OutcomeNoteApprovalsMissing => "outcome_note.approvals_missing",
    OutcomeNoteAuthorRestricted => "outcome_note.author_restricted",
    OutcomeNoteAuthorNotAllowed => "outcome_note.author_not_allowed",
    OutcomeRulesUnreadable => "outcome.rules_unreadable",
    OutcomeNoteRulesUnreadable => "outcome_note.rules_unreadable",
    OutcomeNoteRulesUnreadableFound => "outcome_note.rules_unreadable_found",
    GovernanceFoundMore => "governance.found_more",
    InvalidGovernancePath => "invalid.governance_path",
    InvalidGovernanceMixed => "invalid.governance_mixed",
    ProposalApprovalsUnreachable => "proposal.approvals_unreachable",
    // governance: the refusals of acceptance and the approvals warning (RFC 006 D-1, D-4; handoff G-T1 part C): end
    // protections (RFC 006 D-7; handoff G-T3): the one display, the refusals and the moderation form
    ProtectionHeading => "protection.heading",
    ProtectionNote => "protection.note",
    ProtectionKindStanding => "protection.kind_standing",
    ProtectionKindTemporary => "protection.kind_temporary",
    ProtectionOnPage => "protection.on_page",
    ProtectionOnNamespace => "protection.on_namespace",
    ProtectionRequiresReview => "protection.requires_review",
    ProtectionRequiresRung => "protection.requires_rung",
    ProtectionReason => "protection.reason",
    OutcomeProtected => "outcome.protected",
    OutcomeNoteProtectedRung => "outcome_note.protected_rung",
    OutcomeNoteProtectedReview => "outcome_note.protected_review",
    TitleProtect => "title.protect",
    ProtectLink => "protect.link",
    ProtectHeading => "protect.heading",
    ProtectIntro => "protect.intro",
    ProtectTargetLegend => "protect.target_legend",
    ProtectTargetPage => "protect.target_page",
    ProtectTargetNamespace => "protect.target_namespace",
    ProtectTitleLabel => "protect.title_label",
    ProtectRequireLegend => "protect.require_legend",
    ProtectRequireReview => "protect.require_review",
    ProtectRequireRung => "protect.require_rung",
    ProtectRungLabel => "protect.rung_label",
    ProtectReasonLabel => "protect.reason_label",
    ProtectDaysLabel => "protect.days_label",
    ProtectInForce => "protect.in_force",
    ProtectSet => "protect.set",
    ProtectRenew => "protect.renew",
    ProtectLift => "protect.lift",
    ProtectProblemTitle => "protect.problem_title",
    ProtectProblemRequire => "protect.problem_require",
    ProtectProblemRung => "protect.problem_rung",
    ProtectProblemReason => "protect.problem_reason",
    ProtectProblemDays => "protect.problem_days",
    ProtectProblemTooLong => "protect.problem_too_long",
    ProtectProblemInForce => "protect.problem_in_force",
    ProtectProblemNotInForce => "protect.problem_not_in_force",
    ProtectProblemChanged => "protect.problem_changed",
    PeopleChanged => "people.changed",
    ProtectNotAllowed => "protect.not_allowed",
    ProtectNotKept => "protect.not_kept",
    ProtectUnavailable => "protect.unavailable",
    ProtectBack => "protect.back",
    // protections: end
    // people: roles, grants, restrictions and the people page (RFC 006 D-6, D-7; handoff G-T2)
    RoleObserver => "role.observer",
    RoleQuestioner => "role.questioner",
    RoleContributor => "role.contributor",
    RoleReviewer => "role.reviewer",
    RoleMaintainer => "role.maintainer",
    RoleAdmin => "role.admin",
    RestrictionNotice => "restriction.notice",
    RestrictionReason => "restriction.reason",
    RestrictionUntil => "restriction.until",
    RestrictionNothingChanged => "restriction.nothing_changed",
    TitlePeople => "title.people",
    PeopleHeading => "people.heading",
    PeopleIntro => "people.intro",
    PeopleRolesHeading => "people.roles_heading",
    PeopleEntryNone => "people.entry_none",
    PeopleEntryGiven => "people.entry_given",
    PeopleEntryNoneRestricted => "people.entry_none_restricted",
    PeopleEntryNoEffect => "people.entry_no_effect",
    PeoplePromotionNote => "people.promotion_note",
    PeopleNamesNote => "people.names_note",
    PeopleYouHeading => "people.you_heading",
    PeopleYouGrant => "people.you_grant",
    PeopleYouEntry => "people.you_entry",
    PeopleYouNone => "people.you_none",
    PeopleGrantLink => "people.grant_link",
    PeopleRestrictLink => "people.restrict_link",
    PeoplePromotion => "people.promotion",
    PeopleNoOne => "people.no_one",
    PeopleHolderCount => "people.holder_count",
    PeopleIsYou => "people.is_you",
    PeopleRestrictedMark => "people.restricted_mark",
    PeopleChangeLinkOf => "people.change_link_of",
    PeopleRestrictLinkOf => "people.restrict_link_of",
    PeopleRenewLinkOf => "people.renew_link_of",
    PeopleLiftLinkOf => "people.lift_link_of",
    PeopleRestrictedHeading => "people.restricted_heading",
    PeopleRestrictedIntro => "people.restricted_intro",
    PeopleRestrictedNone => "people.restricted_none",
    PeopleRestrictedRow => "people.restricted_row",
    PeopleDoneGranted => "people.done_granted",
    PeopleDoneRevoked => "people.done_revoked",
    PeopleDoneRestricted => "people.done_restricted",
    PeopleDoneRenewed => "people.done_renewed",
    PeopleDoneLifted => "people.done_lifted",
    PeopleBack => "people.back",
    AboutPeopleLink => "about.people_link",
    TitlePeopleGrant => "title.people_grant",
    TitlePeopleRestrict => "title.people_restrict",
    TitlePeopleRenew => "title.people_renew",
    TitlePeopleLift => "title.people_lift",
    PeopleGrantHeading => "people_grant.heading",
    PeopleGrantIntro => "people_grant.intro",
    PeopleGrantSubmit => "people_grant.submit",
    PeopleGrantRoleLegend => "people_grant.role_legend",
    PeopleGrantReach => "people_grant.reach",
    PeopleGrantNone => "people_grant.none",
    PeopleRestrictHeading => "people_restrict.heading",
    PeopleRestrictIntro => "people_restrict.intro",
    PeopleRestrictSubmit => "people_restrict.submit",
    PeopleRenewHeading => "people_renew.heading",
    PeopleRenewIntro => "people_renew.intro",
    PeopleRenewSubmit => "people_renew.submit",
    PeopleLiftHeading => "people_lift.heading",
    PeopleLiftIntro => "people_lift.intro",
    PeopleLiftSubmit => "people_lift.submit",
    PeopleWhoLabel => "people.who_label",
    PeopleWhoHint => "people.who_hint",
    PeopleDaysLabel => "people.days_label",
    PeopleDaysHint => "people.days_hint",
    PeopleReasonLabel => "people.reason_label",
    PeopleReasonLabelRequired => "people.reason_label_required",
    PeopleReasonHint => "people.reason_hint",
    PeopleProblemRoleMissing => "people.problem_role_missing",
    PeopleProblemNoAccount => "people.problem_no_account",
    PeopleProblemOwnRole => "people.problem_own_role",
    PeopleProblemNothingChanges => "people.problem_nothing_changes",
    PeopleProblemAboveReach => "people.problem_above_reach",
    PeopleProblemLastAdmin => "people.problem_last_admin",
    PeopleProblemOwnRestriction => "people.problem_own_restriction",
    PeopleProblemAdmin => "people.problem_admin",
    PeopleProblemNotBelow => "people.problem_not_below",
    PeopleProblemNotRestricted => "people.problem_not_restricted",
    PeopleProblemAlreadyRestricted => "people.problem_already_restricted",
    PeopleProblemDays => "people.problem_days",
    PeopleProblemReasonMissing => "people.problem_reason_missing",
    PeopleProblemReasonLong => "people.problem_reason_long",
    PeopleProblemReasonLines => "people.problem_reason_lines",
    AccountWikisHeading => "account.wikis_heading",
    AccountWikisIntro => "account.wikis_intro",
    AccountWikisNone => "account.wikis_none",
    TombstoneFact => "tombstone.fact",
    TombstoneRestoredFact => "tombstone.restored_fact",
    TombstonePartContent => "tombstone.part_content",
    TombstonePartSummary => "tombstone.part_summary",
    TombstonePartAuthor => "tombstone.part_author",
    TombstoneCatPersonalData => "tombstone.cat_personal_data",
    TombstoneCatHarassment => "tombstone.cat_harassment",
    TombstoneCatUnlawful => "tombstone.cat_unlawful",
    TombstoneCatCopyright => "tombstone.cat_copyright",
    TombstoneCatSpam => "tombstone.cat_spam",
    TombstoneCatOther => "tombstone.cat_other",
    TombstoneAuthorHidden => "tombstone.author_hidden",
    TombstoneCommentHidden => "tombstone.comment_hidden",
    TombstoneDiffFrom => "tombstone.diff_from",
    TombstoneDiffTo => "tombstone.diff_to",
    TombstoneDiffHeading => "tombstone.diff_heading",
    TombstoneAtHeading => "tombstone.at_heading",
    TombstoneAtHidden => "tombstone.at_hidden",
    TombstoneRevisionHidden => "tombstone.revision_hidden",
    TombstoneSummaryHidden => "tombstone.summary_hidden",
    TombstoneModerateLink => "tombstone.moderate_link",
    TitleModerate => "title.moderate",
    ModerateHeading => "moderate.heading",
    ModerateIntro => "moderate.intro",
    ModerateStillHeld => "moderate.still_held",
    ModerateItemPageRevision => "moderate.item_page_revision",
    ModerateItemProposalRevision => "moderate.item_proposal_revision",
    ModerateItemProposalComment => "moderate.item_proposal_comment",
    ModerateItemClaimComment => "moderate.item_claim_comment",
    ModeratePartsLegend => "moderate.parts_legend",
    ModerateLabelContent => "moderate.label_content",
    ModerateLabelSummary => "moderate.label_summary",
    ModerateLabelAuthor => "moderate.label_author",
    ModerateCategoryLegend => "moderate.category_legend",
    ModerateNoteLabel => "moderate.note_label",
    ModerateHide => "moderate.hide",
    ModerateInForceHeading => "moderate.in_force_heading",
    ModerateUnhide => "moderate.unhide",
    ModerateShow => "moderate.show",
    ModerateShowNote => "moderate.show_note",
    ModerateTipRefused => "moderate.tip_refused",
    ModerateTipUnchecked => "moderate.tip_unchecked",
    ModerateTipUnresolved => "moderate.tip_unresolved",
    ModerateTipLink => "moderate.tip_link",
    ModerateDeclines => "moderate.declines",
    ModerateSurvives => "moderate.survives",
    ModerateSurvivesAtLeast => "moderate.survives_at_least",
    ModerateLaterChange => "moderate.later_change",
    ModerateLaterIntro => "moderate.later_intro",
    ModerateLaterItem => "moderate.later_item",
    ModerateLaterMore => "moderate.later_more",
    ModerateLaterHidden => "moderate.later_hidden",
    ModerateLaterUnchecked => "moderate.later_unchecked",
    ModerateLaterUncheckedAtLeast => "moderate.later_unchecked_at_least",
    ModerateLaterPagesUnchecked => "moderate.later_pages_unchecked",
    ModerateLaterUntraced => "moderate.later_untraced",
    ModerateLaterUntracedAtLeast => "moderate.later_untraced_at_least",
    ModerateLaterMovesUnfollowed => "moderate.later_moves_unfollowed",
    ModerateLaterUnsettled => "moderate.later_unsettled",
    ModerateBack => "moderate.back",
    ModerateProblemNoPart => "moderate.problem_no_part",
    ModerateProblemNoSuchPart => "moderate.problem_no_such_part",
    ModerateProblemAlready => "moderate.problem_already",
    ModerateProblemNote => "moderate.problem_note",
    ModerateProblemChanged => "moderate.problem_changed",
    ModerateNotAllowed => "moderate.not_allowed",
    ModerateUnavailable => "moderate.unavailable",
    ModerateDoneHidden => "moderate.done_hidden",
    ModerateDoneDeclined => "moderate.done_declined",
    TitleModerationLog => "title.moderation_log",
    LogHeading => "log.heading",
    LogIntro => "log.intro",
    LogNone => "log.none",
    LogHidden => "log.hidden",
    LogRestored => "log.restored",
    LogDeclined => "log.declined",
    LogNote => "log.note",
    LogOlder => "log.older",
    TitleModerationShow => "title.moderation_show",
    ShowHeading => "show.heading",
    ShowIntro => "show.intro",
    ShowTextLabel => "show.text_label",
    ShowAuthorLabel => "show.author_label",
    ShowFilesLabel => "show.files_label",
    ShowNothing => "show.nothing",
    RemovedHeading => "removed.heading",
    RemovedBody => "removed.body",
    RemovedByChange => "removed.by_change",
    RemovedAcceptedAt => "removed.accepted_at",
    RemovedReason => "removed.reason",
    RemovedNoReason => "removed.no_reason",
    RemovedLinkChange => "removed.link_change",
    RemovedLinkProposal => "removed.link_proposal",
    RemovedRestoreLink => "removed.restore_link",
    RemovedRestoreNotText => "removed.restore_not_text",
    RemovedCannotPropose => "removed.cannot_propose",
    RemovedRaw => "removed.raw",
    RestoreHeading => "restore.heading",
    RestoreSummary => "restore.summary",
    RestoreIntroRollback => "restore.intro_rollback",
    RestoreIntroLastText => "restore.intro_last_text",
    RestoreOpenDraft => "restore.open_draft",
    RestoreChanged => "restore.changed",
    RestoreBack => "restore.back",
    RestoreTextHidden => "restore.text_hidden",
    ProblemPageGoneTitle => "problem.page_gone.title",
    ProblemPageGoneDetail => "problem.page_gone.detail",
    ProblemPageGoneHint => "problem.page_gone.hint",
    ClaimRetired => "claim.retired",
    ClaimRetiredReason => "claim.retired_reason",
    ClaimRetiredNoReason => "claim.retired_no_reason",
    ClaimSourceRetired => "claim.source_retired",
    ClaimSourceRetiredNoReason => "claim.source_retired_no_reason",
    CurateLabelRetireClaim => "curate.label_retire_claim",
    CurateLabelRetireSource => "curate.label_retire_source",
    CurateRetireLeave => "curate.retire_leave",
    CurateRetireNow => "curate.retire_now",
    CurateRetireBack => "curate.retire_back",
    CurateRetireClaimHint => "curate.retire_claim_hint",
    CurateRetireSourceHint => "curate.retire_source_hint",
    CurateLabelRetireReason => "curate.label_retire_reason",
    CurateRetireReasonHint => "curate.retire_reason_hint",
    CurateRetireIs => "curate.retire_is",
    CurateRetireIsNoReason => "curate.retire_is_no_reason",
    CurateRetireReasonMissing => "curate.retire_reason_missing",
    KnowChangeRetiredReason => "know.change_retired_reason",
    KnowChangeRetiredReasonCleared => "know.change_retired_reason_cleared",
    KnowMadeRetiredReason => "know.made_retired_reason",
    KnowSourceLifeActive => "know.source_life_active",
    KnowSourceLifeRetired => "know.source_life_retired",
    InvalidKnowledgeRetiredUnexplained => "invalid.knowledge_retired_unexplained",
    KnowUnreadableByModerate => "know.unreadable_by_moderate",
    HealthKnowledgeReplaceLink => "health.knowledge_replace_link",
    ReplaceHeading => "replace.heading",
    ReplaceIntro => "replace.intro",
    ReplaceWhy => "replace.why",
    ReplaceWhyTooLong => "replace.why_too_long",
    ReplaceChoiceLegend => "replace.choice_legend",
    ReplaceChoiceWrite => "replace.choice_write",
    ReplaceChoiceRetire => "replace.choice_retire",
    ReplaceLabelText => "replace.label_text",
    ReplaceTextHint => "replace.text_hint",
    ReplaceRetireHint => "replace.retire_hint",
    ReplaceSend => "replace.send",
    ReplaceProblemChoice => "replace.problem_choice",
    ReplaceProblemText => "replace.problem_text",
    ReplaceProblemReason => "replace.problem_reason",
    ReplaceProblemNothing => "replace.problem_nothing",
    QuotaAppealsDay => "quota.appeals_day",
    QuotaAppealsOpen => "quota.appeals_open",
    QuotaAppealsOpenPerson => "quota.appeals_open_person",
    ResetAppealDecided => "quota.reset_appeal_decided",
    ResetAppealYoursDecided => "quota.reset_appeal_yours_decided",
    ContestLink => "contest.link",
    ContestFiled => "contest.filed",
    TitleContest => "title.contest",
    ContestHeading => "contest.heading",
    ContestIntro => "contest.intro",
    ContestTextLabel => "contest.text_label",
    ContestTextHint => "contest.text_hint",
    ContestSubmit => "contest.submit",
    ContestBack => "contest.back",
    ContestProposalLink => "contest.proposal_link",
    ContestDoneFiled => "contest.done_filed",
    ContestProblemEmpty => "contest.problem_empty",
    ContestProblemLong => "contest.problem_long",
    ContestProblemChars => "contest.problem_chars",
    ContestNothingHeading => "contest.nothing_heading",
    ContestNothingBody => "contest.nothing_body",
    ContestAlreadyOpen => "contest.already_open",
    ContestWhatDecline => "contest.what_decline",
    ContestWhatTombstone => "contest.what_tombstone",
    ContestWhatRestriction => "contest.what_restriction",
    ContestWhatProtection => "contest.what_protection",
    ContestWhatGrant => "contest.what_grant",
    ContestNoRole => "contest.no_role",
    AppealsMineLink => "appeals.mine_link",
    TitleAppealsMine => "title.appeals_mine",
    MineIntro => "mine.intro",
    MineContestHeading => "mine.contest_heading",
    MineContestNone => "mine.contest_none",
    MineAppealsHeading => "mine.appeals_heading",
    MineAppealsNone => "mine.appeals_none",
    MineFiledOn => "mine.filed_on",
    MineOpen => "mine.open",
    TitleAppeals => "title.appeals",
    AppealsHeading => "appeals.heading",
    AppealsIntro => "appeals.intro",
    AppealsOpenHeading => "appeals.open_heading",
    AppealsOpenNone => "appeals.open_none",
    AppealsOpenMore => "appeals.open_more",
    AppealsDecidedHeading => "appeals.decided_heading",
    AppealsDecidedNone => "appeals.decided_none",
    AppealsRowHead => "appeals.row_head",
    AppealsKindDecline => "appeals.kind_decline",
    AppealsKindTombstone => "appeals.kind_tombstone",
    AppealsKindRestriction => "appeals.kind_restriction",
    AppealsKindProtection => "appeals.kind_protection",
    AppealsKindGrant => "appeals.kind_grant",
    AppealsFiledBy => "appeals.filed_by",
    AppealsActionBy => "appeals.action_by",
    AppealsActionByOperator => "appeals.action_by_operator",
    AppealsSaidLabel => "appeals.said_label",
    AppealsDecisionUpheld => "appeals.decision_upheld",
    AppealsDecisionOverturned => "appeals.decision_overturned",
    AppealsDecideLegend => "appeals.decide_legend",
    AppealsDecideUpheld => "appeals.decide_upheld",
    AppealsDecideOverturned => "appeals.decide_overturned",
    AppealsDecideReason => "appeals.decide_reason",
    AppealsDecideSubmit => "appeals.decide_submit",
    AppealsBlockedAppellant => "appeals.blocked_appellant",
    AppealsBlockedActor => "appeals.blocked_actor",
    AppealsDoneUpheld => "appeals.done_upheld",
    AppealsDoneOverturned => "appeals.done_overturned",
    AppealsDoneReopened => "appeals.done_reopened",
    AppealsBack => "appeals.back",
    AppealsProblemDecision => "appeals.problem_decision",
    AppealsRefusedHeading => "appeals.refused_heading",
    AppealsRefusedNoSuch => "appeals.refused_no_such",
    AppealsRefusedDecided => "appeals.refused_decided",
    AppealsMootRestriction => "appeals.moot_restriction",
    AppealsMootProtection => "appeals.moot_protection",
    AppealsMootRole => "appeals.moot_role",
    AppealsMootReach => "appeals.moot_reach",
    AppealsMootProposal => "appeals.moot_proposal",
    AppealsMootHidden => "appeals.moot_hidden",
    AppealsMootHiding => "appeals.moot_hiding",
    LogAppealed => "log.appealed",
    LogAppealUpheld => "log.appeal_upheld",
    LogAppealOverturned => "log.appeal_overturned",
    LogRestoredByAppeal => "log.restored_by_appeal",
    TitlePolicy => "title.policy",
    PolicyHeading => "policy.heading",
    PolicyIntro => "policy.intro",
    PolicyDefaultsNote => "policy.defaults_note",
    PolicyAsOf => "policy.as_of",
    PolicyUnreadableHeading => "policy.unreadable_heading",
    PolicyUnreadableDefaults => "policy.unreadable_defaults",
    PolicyWhoHeading => "policy.who_heading",
    PolicyLabelComment => "policy.label_comment",
    PolicyLabelPropose => "policy.label_propose",
    PolicyLabelReview => "policy.label_review",
    PolicyLabelCreateView => "policy.label_create_view",
    PolicyLabelAccept => "policy.label_accept",
    PolicyLabelModerate => "policy.label_moderate",
    PolicyLabelDecideAppeal => "policy.label_decide_appeal",
    PolicyLabelGrant => "policy.label_grant",
    PolicyLabelEntry => "policy.label_entry",
    PolicyValueRung => "policy.value_rung",
    PolicyValueRungTop => "policy.value_rung_top",
    PolicyValueEntryNone => "policy.value_entry_none",
    PolicyValueGrantNone => "policy.value_grant_none",
    PolicyValueGrantUpTo => "policy.value_grant_up_to",
    PolicyEntryNoEffect => "policy.entry_no_effect",
    PolicyAcceptHeading => "policy.accept_heading",
    PolicyLabelApprovals => "policy.label_approvals",
    PolicyValueApprovals => "policy.value_approvals",
    PolicyValueApprovalsNone => "policy.value_approvals_none",
    PolicyLabelSelf => "policy.label_self",
    PolicyValueSelfNever => "policy.value_self_never",
    PolicyValueSelfSmall => "policy.value_self_small",
    PolicyLabelReviewAfter => "policy.label_review_after",
    PolicyValueReviewDays => "policy.value_review_days",
    PolicyValueReviewServer => "policy.value_review_server",
    PolicyPromotionHeading => "policy.promotion_heading",
    PolicyPromotionNote => "policy.promotion_note",
    PolicyLabelPromotion => "policy.label_promotion",
    PolicyPromotionNone => "policy.promotion_none",
    PolicyChangeHeading => "policy.change_heading",
    PolicyLabelChangeRules => "policy.label_change_rules",
    PolicyLabelChangeWho => "policy.label_change_who",
    PolicyValueChangeRules => "policy.value_change_rules",
    PolicyValueChangeWho => "policy.value_change_who",
    PolicyHatchNote => "policy.hatch_note",
    PolicyProtectionsHeading => "policy.protections_heading",
    PolicyProtectionsIntro => "policy.protections_intro",
    PolicyProtectionsNone => "policy.protections_none",
    PolicyProtectionsSelfNote => "policy.protections_self_note",
    PolicyInterwikiHeading => "policy.interwiki_heading",
    PolicyInterwikiIntro => "policy.interwiki_intro",
    PolicyInterwikiNone => "policy.interwiki_none",
    PolicyInterwikiUnavailable => "policy.interwiki_unavailable",
    PolicyLimitsHeading => "policy.limits_heading",
    PolicyLimitsIntro => "policy.limits_intro",
    PolicyLimitEntry => "policy.limit_entry",
    PolicyLimitDays => "policy.limit_days",
    PolicyLoweredEntry => "policy.lowered_entry",
    PolicyHistoryHeading => "policy.history_heading",
    PolicyHistoryIntro => "policy.history_intro",
    PolicyHistoryNone => "policy.history_none",
    PolicyHistoryRow => "policy.history_row",
    PolicyHistoryRowAlone => "policy.history_row_alone",
    PolicyHistoryRepair => "policy.history_repair",
    PolicyHistoryHidden => "policy.history_hidden",
    PolicyHistoryMore => "policy.history_more",
    PolicyHistoryWindow => "policy.history_window",
    PolicyHistoryUnavailable => "policy.history_unavailable",
    PolicyProtectionsUnavailable => "policy.protections_unavailable",
    PolicyLinkLog => "policy.link_log",
    PolicyLinkChange => "policy.link_change",
    AboutPolicyLink => "about.policy_link",
    PolicyChangeLine => "policy.change_line",
    PolicyChangePromotionNone => "policy.change_promotion_none",
    PolicyChangeProtectionAdded => "policy.change_protection_added",
    PolicyChangeProtectionRemoved => "policy.change_protection_removed",
    PolicyChangeProtectionChanged => "policy.change_protection_changed",
    PolicyChangeInterwikiAdded => "policy.change_interwiki_added",
    PolicyChangeInterwikiRemoved => "policy.change_interwiki_removed",
    PolicyChangeInterwikiChanged => "policy.change_interwiki_changed",
    PolicyProtectionPage => "policy.protection_page",
    PolicyProtectionNamespace => "policy.protection_namespace",
    PolicyProtectionReview => "policy.protection_review",
    PolicyProtectionRung => "policy.protection_rung",
    GovernanceChangeHeading => "governance.change_heading",
    GovernanceChangeNone => "governance.change_none",
    GovernanceChangeDefaults => "governance.change_defaults",
    GovernanceChangeUnreadable => "governance.change_unreadable",
    GovernanceNeeds => "governance.needs",
    GovernanceHatchYou => "governance.hatch_you",
    TitlePolicyEdit => "title.policy_edit",
    PolicyEditHeading => "policy_edit.heading",
    PolicyEditIntro => "policy_edit.intro",
    PolicyEditWhoNote => "policy_edit.who_note",
    PolicyEditAcceptNote => "policy_edit.accept_note",
    PolicyEditRepairNote => "policy_edit.repair_note",
    PolicyEditRepairNotAdmin => "policy_edit.repair_not_admin",
    PolicyEditEntryHint => "policy_edit.entry_hint",
    PolicyEditApprovalsHint => "policy_edit.approvals_hint",
    PolicyEditReviewHint => "policy_edit.review_hint",
    PolicyEditPromotionHint => "policy_edit.promotion_hint",
    PolicyEditProtectIntro => "policy_edit.protect_intro",
    PolicyEditProtectRow => "policy_edit.protect_row",
    PolicyEditProtectKindLabel => "policy_edit.protect_kind_label",
    PolicyEditProtectKindPage => "policy_edit.protect_kind_page",
    PolicyEditProtectKindNamespace => "policy_edit.protect_kind_namespace",
    PolicyEditProtectTitleLabel => "policy_edit.protect_title_label",
    PolicyEditProtectAsksLabel => "policy_edit.protect_asks_label",
    PolicyEditProtectReasonLabel => "policy_edit.protect_reason_label",
    PolicyEditProtectRemove => "policy_edit.protect_remove",
    PolicyEditProtectAdd => "policy_edit.protect_add",
    PolicyEditInterwikiIntro => "policy_edit.interwiki_intro",
    PolicyEditInterwikiRow => "policy_edit.interwiki_row",
    PolicyEditInterwikiPrefixLabel => "policy_edit.interwiki_prefix_label",
    PolicyEditInterwikiTemplateLabel => "policy_edit.interwiki_template_label",
    PolicyEditInterwikiRemove => "policy_edit.interwiki_remove",
    PolicyEditInterwikiAdd => "policy_edit.interwiki_add",
    PolicyEditInterwikiPrefixCharacters => "policy_edit.interwiki_prefix_characters",
    PolicyEditInterwikiPrefixLong => "policy_edit.interwiki_prefix_long",
    PolicyEditInterwikiTemplateLong => "policy_edit.interwiki_template_long",
    PolicyEditInterwikiRepeated => "policy_edit.interwiki_repeated",
    PolicyEditInterwikiTooMany => "policy_edit.interwiki_too_many",
    PolicyEditSummaryDefault => "policy_edit.summary_default",
    PolicyEditSummaryDefaults => "policy_edit.summary_defaults",
    PolicyEditSend => "policy_edit.send",
    PolicyEditDefaultsButton => "policy_edit.defaults_button",
    PolicyEditUnchanged => "policy_edit.unchanged",
    PolicyEditDefaultsUnchanged => "policy_edit.defaults_unchanged",
    PolicyEditProblemNumber => "policy_edit.problem_number",
    PolicyEditRefusalWrongShape => "policy_edit.refusal_wrong_shape",
    PolicyEditRefusalUnknownValue => "policy_edit.refusal_unknown_value",
    PolicyEditRefusalOutOfRange => "policy_edit.refusal_out_of_range",
    PolicyEditRefusalBounds => "policy_edit.refusal_bounds",
    PolicyEditRefusalForbidden => "policy_edit.refusal_forbidden",
    PolicyEditRefusalMissing => "policy_edit.refusal_missing",
    PolicyEditRefusalDuplicate => "policy_edit.refusal_duplicate",
    PolicyEditRefusalOrderPropose => "policy_edit.refusal_order_propose",
    PolicyEditRefusalOrderReview => "policy_edit.refusal_order_review",
    PolicyEditRefusalOrderModerate => "policy_edit.refusal_order_moderate",
    PolicyEditRefusalOrderDecideAppeal => "policy_edit.refusal_order_decide_appeal",
    PolicyEditRefusalLimitEntry => "policy_edit.refusal_limit_entry",
    PolicyEditRefusalLimitDays => "policy_edit.refusal_limit_days",
    AuthorityUnderRules => "authority.under_rules",
    AuthorityUnderDefaults => "authority.under_defaults",
    AuthorityUnrecorded => "authority.unrecorded",
    AuthorityAccepted => "authority.accepted",
    AuthorityAcceptedHatch => "authority.accepted_hatch",
    AuthorityReviewed => "authority.reviewed",
    AuthorityDone => "authority.done",
    AuthoritySetBy => "authority.set_by",
    AuthorityRestriction => "authority.restriction",
    PolicyEditLabelChangeApprovals => "policy_edit.label_change_approvals",
    PolicyEditLabelChangeWhoApprovals => "policy_edit.label_change_who_approvals",
    PolicyEditRefusalTitle => "policy_edit.refusal_title",
    OutcomeSummaryHidden => "outcome.summary_hidden",
    OutcomeNoteSummaryHidden => "outcome_note.summary_hidden",
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
    /// Markup this module built from escaped parts ([`Rich`]): [`bdi`], [`cite`], [`code`] or another message's output. It
    /// can be nothing a caller wrote.
    Rich(String),
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
            Slot::Rich(markup) => markup,
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
            Slot::Rich(markup) => out.push_str(markup),
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

/// `text` isolated from the direction of the sentence around it (`<bdi>`), escaped: a text a person wrote (a
/// paragraph's excerpt, a name) that may hold characters of the other direction, so that it cannot reorder the words
/// beside it.
pub fn bdi(text: &str) -> Rich {
    let mut out = String::from("<bdi>");
    escape_into(&mut out, text);
    out.push_str("</bdi>");
    Rich(out)
}

/// `text` as the name of a work (`<cite>`, direction-isolated and escaped): a source's name has a visible boundary,
/// so that a title written to read like another source's line is the name it is.
pub fn cite(text: &str) -> Rich {
    let mut out = String::from("<cite><bdi>");
    escape_into(&mut out, text);
    out.push_str("</bdi></cite>");
    Rich(out)
}

/// `text` as words, escaped: a line of a list whose other lines hold markup (a problem that names a token in `<code>`).
pub fn plain(text: &str) -> Rich {
    let mut out = String::new();
    escape_into(&mut out, text);
    Rich(out)
}

/// `text` as a token to be read exactly (`<code>`, escaped): an identifier.
pub fn code(text: &str) -> Rich {
    let mut out = String::from("<code>");
    escape_into(&mut out, text);
    out.push_str("</code>");
    Rich(out)
}

/// [`code`] linked to `href` (a link only to an address of this site or an absolute one, as [`Msg::link`] is): the
/// whole identifier of a subject, which leads to the subject's page.
pub fn code_link(text: &str, href: &str) -> Rich {
    if !href_ok(href) {
        return code(text);
    }
    let mut out = String::from("<a href=\"");
    escape_into(&mut out, href);
    out.push_str("\">");
    out.push_str(code(text).as_str());
    out.push_str("</a>");
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

    /// A placeholder that is markup this module built ([`bdi`], [`cite`], or another message's [`Msg::html`]).
    #[must_use]
    pub fn rich(mut self, name: &'static str, value: Rich) -> Msg {
        self.args.push((name, Slot::Rich(value.0)));
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
