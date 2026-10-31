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
    // about, and the list of wikis
    TitleAbout => "title.about",
    AboutHeading => "about.heading",
    AboutLicense => "about.license",
    AboutView => "about.view",
    AccessibilityHeading => "about.accessibility_heading",
    AccessibilityTools => "about.accessibility_tools",
    AccessibilityPeople => "about.accessibility_people",
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
    OutcomeTooLarge => "outcome.too_large",
    OutcomeNoteTooLarge => "outcome_note.too_large",
    OutcomeUnavailable => "outcome.unavailable",
    OutcomeNoteUnavailable => "outcome_note.unavailable",
    OutcomeInProgress => "outcome.in_progress",
    OutcomeNoteInProgress => "outcome_note.in_progress",
    OutcomeNothingToChange => "outcome.nothing_to_change",
    OutcomeNoteNothingToChange => "outcome_note.nothing_to_change",
    OutcomeStore => "outcome.store",
    OutcomeNoteStore => "outcome_note.store",
    AcceptHeading => "accept.heading",
    AcceptNotYet => "accept.not_yet",
    AcceptNotYetKept => "accept.not_yet_kept",
    AcceptNotYetNote => "accept.not_yet_note",
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
    RefusedStaleRevision => "refused.stale_revision",
    RefusedSuperseder => "refused.superseder",
    RefusedChanged => "refused.changed",
    InvalidIncomplete => "invalid.incomplete",
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
