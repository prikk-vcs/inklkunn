//! What a proposer may send: the summary, the sources, the pages, the renames, and the bounds that hold
//! before anything is stored. A refusal names the rule and never repeats a path or a text.

use inklkunn_store::{FileChange, ProposalFilter, Rename};

use super::support::{edit, on_both};
use crate::proposal::{
    Invalid, MAX_PAGE_BYTES, MAX_PROPOSAL_BYTES, MAX_SOURCE_BYTES, MAX_SOURCES, MAX_SUMMARY_CHARS,
    ProposalEdit, ProposalError,
};

fn refused(e: &super::support::Env, edit: ProposalEdit) -> Invalid {
    match e.service.create(&e.author.principal, &e.wiki, None, edit) {
        Err(ProposalError::Invalid(why)) => why,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_summary_is_one_line_of_at_most_120_characters_and_is_required_to_submit() {
    on_both(|e| {
        let long = "あ".repeat(MAX_SUMMARY_CHARS + 1);
        assert_eq!(
            refused(e, edit(&long, "pages/a.md", "x")),
            Invalid::SummaryTooLong
        );
        // The bound is in characters, not bytes: 120 multibyte characters are fine.
        let exact = "あ".repeat(MAX_SUMMARY_CHARS);
        assert!(
            e.service
                .create(
                    &e.author.principal,
                    &e.wiki,
                    None,
                    edit(&exact, "pages/a.md", "x")
                )
                .is_ok()
        );
        for bad in ["two\nlines", "tab\there", "carriage\rreturn"] {
            assert_eq!(
                refused(e, edit(bad, "pages/a.md", "x")),
                Invalid::SummaryNotOneLine
            );
        }
        // A draft may be unfinished; sending it for review may not.
        let d = e
            .service
            .create(
                &e.author.principal,
                &e.wiki,
                None,
                edit("  ", "pages/b.md", "x"),
            )
            .unwrap();
        assert_eq!(
            e.service.submit(&e.author.principal, &e.wiki, &d.id),
            Err(ProposalError::Invalid(Invalid::SummaryMissing))
        );
        let ok = e
            .service
            .save_draft(
                &e.author.principal,
                &e.wiki,
                &d.id,
                edit("  Now it says what  ", "pages/b.md", "x"),
            )
            .unwrap();
        assert_eq!(
            ok.summary, "Now it says what",
            "the summary is kept trimmed"
        );
        assert!(
            e.service
                .submit(&e.author.principal, &e.wiki, &d.id)
                .is_ok()
        );
    });
}

#[test]
fn sources_are_few_short_and_single_lines() {
    on_both(|e| {
        let mut many = edit("One", "pages/a.md", "x");
        many.sources = (0..=MAX_SOURCES)
            .map(|i| format!("https://example.org/{i}"))
            .collect();
        assert_eq!(refused(e, many), Invalid::TooManySources);
        for bad in ["", "  ", "a\nb", &"x".repeat(MAX_SOURCE_BYTES + 1)] {
            let mut one = edit("One", "pages/a.md", "x");
            one.sources = vec![bad.to_owned()];
            assert_eq!(refused(e, one), Invalid::BadSource);
        }
        let mut fine = edit("One", "pages/a.md", "x");
        fine.sources = (0..MAX_SOURCES)
            .map(|i| format!("https://example.org/{i}"))
            .collect();
        assert!(
            e.service
                .create(&e.author.principal, &e.wiki, None, fine)
                .is_ok()
        );
    });
}

#[test]
fn paths_are_repository_paths_and_each_is_named_once() {
    on_both(|e| {
        for bad in [
            "",
            "/etc/passwd",
            "../up.md",
            "a/../b.md",
            "a//b.md",
            "a/",
            "tab\there.md",
            &"x".repeat(1025),
        ] {
            assert_eq!(
                refused(e, edit("One", bad, "x")),
                Invalid::BadPath,
                "{bad:?}"
            );
        }
        let twice = ProposalEdit {
            summary: "One".into(),
            files: vec![
                FileChange {
                    path: "pages/a.md".into(),
                    text: Some("x".into()),
                },
                FileChange {
                    path: "pages/a.md".into(),
                    text: None,
                },
            ],
            ..ProposalEdit::default()
        };
        assert_eq!(refused(e, twice), Invalid::DuplicatePath);
        let to_self = ProposalEdit {
            summary: "One".into(),
            renames: vec![Rename {
                from: "pages/a.md".into(),
                to: "pages/a.md".into(),
            }],
            ..ProposalEdit::default()
        };
        assert_eq!(refused(e, to_self), Invalid::RenameToSelf);
        let swapped = ProposalEdit {
            summary: "One".into(),
            renames: vec![
                Rename {
                    from: "pages/a.md".into(),
                    to: "pages/c.md".into(),
                },
                Rename {
                    from: "pages/b.md".into(),
                    to: "pages/c.md".into(),
                },
            ],
            ..ProposalEdit::default()
        };
        assert_eq!(refused(e, swapped), Invalid::DuplicatePath);
        // A page removed, a page renamed, and the renamed page edited are all fine together.
        let all = ProposalEdit {
            summary: "Tidy".into(),
            files: vec![
                FileChange {
                    path: "pages/gone.md".into(),
                    text: None,
                },
                FileChange {
                    path: "pages/new-name.md".into(),
                    text: Some("edited".into()),
                },
            ],
            renames: vec![Rename {
                from: "pages/old-name.md".into(),
                to: "pages/new-name.md".into(),
            }],
            ..ProposalEdit::default()
        };
        assert!(
            e.service
                .create(&e.author.principal, &e.wiki, None, all)
                .is_ok()
        );
    });
}

#[test]
fn the_size_bounds_hold_before_anything_is_stored() {
    on_both(|e| {
        let page = "x".repeat(MAX_PAGE_BYTES + 1);
        assert_eq!(
            refused(e, edit("One", "pages/a.md", &page)),
            Invalid::PageTooLarge
        );
        // A page at the bound is fine; several pages that together pass the proposal's bound are not.
        let at_bound = "x".repeat(MAX_PAGE_BYTES);
        assert!(
            e.service
                .create(
                    &e.author.principal,
                    &e.wiki,
                    None,
                    edit("One", "pages/a.md", &at_bound)
                )
                .is_ok()
        );
        let files = (0..3)
            .map(|i| FileChange {
                path: format!("pages/p{i}.md"),
                text: Some("y".repeat(800 * 1024)),
            })
            .collect();
        let big = ProposalEdit {
            summary: "Big".into(),
            files,
            ..ProposalEdit::default()
        };
        const { assert!(MAX_PROPOSAL_BYTES < 3 * 800 * 1024) };
        assert_eq!(refused(e, big), Invalid::TooLarge);
        // Only the accepted one exists.
        let listed = e
            .service
            .list(&e.author.principal, &e.wiki, &ProposalFilter::default())
            .unwrap();
        assert_eq!(listed.len(), 1);
    });
}

#[test]
fn a_submitted_proposal_changes_a_page_or_renames_one() {
    on_both(|e| {
        let empty = ProposalEdit {
            summary: "Nothing".into(),
            ..ProposalEdit::default()
        };
        // An empty draft may be saved (a person starting a form) ...
        let d = e
            .service
            .create(&e.author.principal, &e.wiki, None, empty.clone())
            .unwrap();
        // ... but not sent, and not revised into.
        assert_eq!(
            e.service.submit(&e.author.principal, &e.wiki, &d.id),
            Err(ProposalError::Invalid(Invalid::NothingProposed))
        );
        let held = e.held(&e.author, "Real", "pages/a.md");
        assert_eq!(
            e.service
                .revise(&e.author.principal, &e.wiki, &held.id, empty),
            Err(ProposalError::Invalid(Invalid::NothingProposed))
        );
    });
}

#[test]
fn a_refusal_names_the_rule_and_never_repeats_what_was_sent() {
    on_both(|e| {
        let secret = "pages/très-secret-path.md";
        let err = e
            .service
            .create(
                &e.author.principal,
                &e.wiki,
                None,
                edit("a\nb", secret, "x"),
            )
            .unwrap_err();
        assert!(!format!("{err} {err:?}").contains("secret"));
        let err = e
            .service
            .create(
                &e.author.principal,
                &e.wiki,
                None,
                edit("One", &format!("../{secret}"), "x"),
            )
            .unwrap_err();
        assert!(!format!("{err} {err:?}").contains("secret"));
    });
}
