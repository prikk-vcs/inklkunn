//! The linter's findings, named by their stable codes (RFC 004 D-2; PR-9): which catalog message says each
//! one. Codes are identifiers the markup crate fixes, not words; the file is scoped in `tests/literals.allow`.

use crate::catalog::Key;

/// The catalog message for a finding of the linter (by its stable code); a code this build has no message for
/// is the general one, which still names the guideline.
pub(crate) fn lint_key(code: &str) -> Key {
    match code {
        "MKUP-19-bom" => Key::LintBom,
        "MKUP-19-crlf" => Key::LintCrlf,
        "MKUP-19-newline" => Key::LintNewline,
        "MKUP-19-tabs" => Key::LintTabs,
        "META-1-missing" => Key::LintNoInfo,
        "META-2-title" => Key::LintNoTitle,
        "META-3-coerced" => Key::LintCoerced,
        "META-4-list" | "META-5-list" => Key::LintList,
        "META-6-status" => Key::LintStatus,
        "META-7-redirect" => Key::LintRedirect,
        "META-13-visibility" => Key::LintVisibility,
        "META-14-secret" => Key::LintSecret,
        "MKUP-9-duplicate" => Key::LintDuplicateId,
        "EXT-4-unclosed" => Key::LintUnclosed,
        "MKUP-15-callout" => Key::LintCallout,
        "MKUP-15-container" => Key::LintContainer,
        "MKUP-8-h1" => Key::LintHeadingH1,
        "MKUP-8-duplicate-heading" => Key::LintHeadingDup,
        "MKUP-14-tags" => Key::LintTags,
        "MKUP-4-empty-label" => Key::LintEmptyLabel,
        "MKUP-4-empty" => Key::LintEmptyTarget,
        "MKUP-9-missing-block" => Key::LintMissingPartRef,
        "MKUP-13-prefix" => Key::LintPrefix,
        "MKUP-6-order" => Key::LintOrder,
        "NAV-3-dangling" => Key::LintDangling,
        "MKUP-5-ambiguous" => Key::LintAmbiguous,
        _ => Key::LintOther,
    }
}
