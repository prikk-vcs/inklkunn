//! What the files of a proposal may be and do, judged by the service itself whatever surface sent them (the
//! corrections P4 and P5): every path is a page path, a new path is exactly what its title gives and collides with
//! nothing, and the keys only an administrator may change cannot be changed by an edit, a removal, a rename or a
//! rollback plan.

use inklkunn_store::{FileChange, ProposalState, Rename};

use super::support::{Actor, Env, Kind, base, on_both};
use crate::proposal::{Invalid, ProposalEdit, ProposalError};
use crate::read::RollbackPlan;

fn page(front: &str, body: &str) -> String {
    format!("---\ntitle: T\n{front}---\n\n{body}\n")
}

fn change(path: &str, text: Option<&str>) -> FileChange {
    FileChange {
        path: path.to_owned(),
        text: text.map(str::to_owned),
    }
}

fn rename(from: &str, to: &str) -> Rename {
    Rename {
        from: from.to_owned(),
        to: to.to_owned(),
    }
}

fn proposal(files: Vec<FileChange>, renames: Vec<Rename>) -> ProposalEdit {
    ProposalEdit {
        summary: "A change".to_owned(),
        files,
        renames,
        ..ProposalEdit::default()
    }
}

fn create(e: &Env, who: &Actor, edit: ProposalEdit) -> Result<(), ProposalError> {
    e.service
        .create(&who.principal, &e.wiki, None, edit)
        .map(|_| ())
}

fn invalid(why: Invalid) -> Result<(), ProposalError> {
    Err(ProposalError::Invalid(why))
}

// ---------------------------------------------------------------------------------------------------- P4

#[test]
fn every_file_and_both_ends_of_a_rename_must_be_a_page_inside_pages() {
    on_both(|e| {
        e.bases.put_page("pages/A.md", "x\n");
        for path in [
            "governance/rules.md",
            "pages.md",
            "pagesx/A.md",
            "pages/A.txt",
            "pages/A",
            "pages/",
            "pages/é.md",
        ] {
            assert_eq!(
                create(
                    e,
                    &e.author,
                    proposal(vec![change(path, Some("x\n"))], vec![])
                ),
                invalid(if path == "pages/" {
                    Invalid::BadPath
                } else {
                    Invalid::PathOutsidePages
                }),
                "a file at {path}"
            );
        }
        // Both ends of a rename: the target outside pages/, and a governance file moved into pages/.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/A.md", "governance/A.md")])
            ),
            invalid(Invalid::PathOutsidePages),
            "a page moved out"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![],
                    vec![rename("governance/rules.md", "pages/Rules.md")]
                )
            ),
            invalid(Invalid::PathOutsidePages),
            "a governance file moved in"
        );
        // A good one passes.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/A.md", "pages/B.md")])
            ),
            Ok(())
        );
    });
}

#[test]
fn a_path_that_is_not_at_the_base_must_be_what_its_title_gives() {
    on_both(|e| {
        // `%61` is `a`: the title `a` has the path `pages/a.md`, not this spelling.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/%61.md", Some("x\n"))], vec![])
            ),
            invalid(Invalid::PathNotCanonical)
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/a.md", "pages/%61%62.md")])
            ),
            invalid(Invalid::PathNotCanonical),
            "the target of a rename is a new path"
        );
        // A character the encoding never leaves bare, spelled bare.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/a:b.md", Some("x\n"))], vec![])
            ),
            invalid(Invalid::PathNotCanonical)
        );
        // What the title gives is fine, and so is a path that exists at the base even if its spelling is not
        // the one the wiki would write today (it is not new).
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change("pages/Guides/Set up (2).md", Some("x\n"))],
                    vec![]
                )
            ),
            Ok(())
        );
        e.bases.put_page("pages/%61.md", "old\n");
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/%61.md", Some("new\n"))], vec![])
            ),
            Ok(())
        );
    });
}

#[test]
fn a_new_path_may_not_collide_by_case_with_a_page_that_stays_or_with_another_new_path() {
    on_both(|e| {
        e.bases.put_page("pages/Home.md", "home\n");
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/home.md", Some("x\n"))], vec![])
            ),
            invalid(Invalid::PathCollides),
            "pages/home.md beside pages/Home.md"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![
                        change("pages/Fresh.md", Some("x\n")),
                        change("pages/fresh.md", Some("x\n"))
                    ],
                    vec![]
                )
            ),
            invalid(Invalid::PathCollides),
            "two new paths that differ only by case"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/Home.md", "pages/Other.md")])
            ),
            Ok(()),
            "a plain rename"
        );
        e.bases.put_page("pages/Other.md", "other\n");
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/Home.md", "pages/other.md")])
            ),
            invalid(Invalid::PathCollides),
            "a move onto the case variant of a page that stays"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/Home.md", "pages/Other.md")])
            ),
            invalid(Invalid::PathCollides),
            "a move onto a page that stays replaces it"
        );
        // A page that goes in the same proposal frees its name; a case-only rename moves from itself.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/Home.md", "pages/home.md")])
            ),
            Ok(())
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![
                        change("pages/Home.md", None),
                        change("pages/home.md", Some("x\n"))
                    ],
                    vec![]
                )
            ),
            Ok(())
        );
        // A rename and the edit of the page it lands on name one path and are one new page.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change("pages/Moved.md", Some("x\n"))],
                    vec![rename("pages/Home.md", "pages/Moved.md")]
                )
            ),
            Ok(())
        );
    });
}

#[test]
fn the_path_rules_hold_on_every_way_a_proposal_is_written_and_sent() {
    on_both(|e| {
        e.bases.put_page("pages/Home.md", "home\n");
        let me = &e.author;
        let draft = e
            .service
            .create(
                &me.principal,
                &e.wiki,
                None,
                proposal(vec![change("pages/Fresh.md", Some("x\n"))], vec![]),
            )
            .unwrap();
        // Saving a draft.
        assert_eq!(
            e.service
                .save_draft(
                    &me.principal,
                    &e.wiki,
                    &draft.id,
                    proposal(vec![change("governance/x.md", Some("x\n"))], vec![]),
                )
                .map(|_| ()),
            invalid(Invalid::PathOutsidePages)
        );
        assert_eq!(
            e.service
                .save_draft(
                    &me.principal,
                    &e.wiki,
                    &draft.id,
                    proposal(vec![change("pages/home.md", Some("x\n"))], vec![]),
                )
                .map(|_| ()),
            invalid(Invalid::PathCollides)
        );
        // Submitting: the repository moved on since the draft was saved, and the name is taken now (by case).
        e.bases.put_page("pages/fresh.md", "someone else\n");
        assert_eq!(
            e.service
                .submit(&me.principal, &e.wiki, &draft.id)
                .map(|_| ()),
            invalid(Invalid::PathCollides)
        );
        // Revising a held proposal.
        e.bases.put_page("pages/fresh.md", "someone else\n");
        let ok = e
            .service
            .create(
                &me.principal,
                &e.wiki,
                None,
                proposal(vec![change("pages/Home.md", Some("edited\n"))], vec![]),
            )
            .unwrap();
        let held = e.service.submit(&me.principal, &e.wiki, &ok.id).unwrap();
        assert_eq!(held.state, ProposalState::HeldForReview);
        assert_eq!(
            e.service
                .revise(
                    &me.principal,
                    &e.wiki,
                    &ok.id,
                    proposal(vec![], vec![rename("pages/Home.md", "elsewhere/Home.md")]),
                )
                .map(|_| ()),
            invalid(Invalid::PathOutsidePages)
        );
    });
}

#[test]
fn a_rollback_plan_is_judged_like_any_other_edit() {
    on_both(|e| {
        e.bases.put_page("pages/Home.md", "home\n");
        let plan = |files: Vec<FileChange>| RollbackPlan {
            undoes: "b".repeat(64),
            number: 2,
            undone_summary: "A change".to_owned(),
            base: base(1),
            files,
            summary: "Undo: a change".to_owned(),
        };
        let refused = |files| match e.service.create_rollback(
            &e.author.principal,
            &e.wiki,
            None,
            &plan(files),
        ) {
            Err(error) => error,
            Ok(_) => panic!("a rollback plan that breaks the path rules was taken"),
        };
        assert_eq!(
            refused(vec![change("governance/rules.md", Some("x\n"))]),
            ProposalError::Invalid(Invalid::PathOutsidePages)
        );
        assert_eq!(
            refused(vec![change("pages/home.md", Some("x\n"))]),
            ProposalError::Invalid(Invalid::PathCollides)
        );
    });
}

#[test]
fn a_repository_that_cannot_be_asked_is_not_a_pass() {
    on_both(|e| {
        e.bases
            .fail_with(Some(crate::proposal::BaseError::Unavailable(
                "down".to_owned(),
            )));
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/A.md", Some("x\n"))], vec![])
            ),
            Err(ProposalError::BaseUnavailable("down".to_owned()))
        );
    });
}

// ---------------------------------------------------------------------------------------------------- P5

fn owned(keys: &[&'static str]) -> Result<(), ProposalError> {
    Err(ProposalError::OwnedKeys(keys.to_vec()))
}

#[test]
fn a_contributor_cannot_change_an_owned_key_by_an_edit_and_an_administrator_can() {
    on_both(|e| {
        e.bases.put_page("pages/Plain.md", &page("", "body"));
        e.bases
            .put_page("pages/Secret.md", &page("visibility: private\n", "body"));
        for (front, keys) in [
            ("visibility: private\n", &["visibility"][..]),
            ("system: true\n", &["system"][..]),
            ("kind: category\n", &["kind"][..]),
            (
                "kind: system\nvisibility: members\n",
                &["kind", "visibility"][..],
            ),
        ] {
            let edit = || {
                proposal(
                    vec![change("pages/Plain.md", Some(&page(front, "body")))],
                    vec![],
                )
            };
            assert_eq!(create(e, &e.author, edit()), owned(keys), "{front}");
            assert_eq!(create(e, &e.reviewer, edit()), owned(keys), "{front}");
            assert_eq!(create(e, &e.maintainer, edit()), owned(keys), "{front}");
            assert_eq!(create(e, &e.admin, edit()), Ok(()), "{front}");
        }
        // A key removed is a key changed; a key left as it was is not.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change("pages/Secret.md", Some(&page("", "body")))],
                    vec![]
                )
            ),
            owned(&["visibility"])
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change(
                        "pages/Secret.md",
                        Some(&page("visibility: private\n", "an edit"))
                    )],
                    vec![]
                )
            ),
            Ok(()),
            "the key is as it was"
        );
        // The same value spelled another way is the same value.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change(
                        "pages/Secret.md",
                        Some(&page("visibility: \"private\"\n", "body"))
                    )],
                    vec![]
                )
            ),
            Ok(())
        );
    });
}

#[test]
fn a_redirect_is_ordinary_wiki_work_and_any_other_kind_is_not() {
    on_both(|e| {
        e.bases.put_page("pages/Plain.md", &page("", "body"));
        e.bases.put_page(
            "pages/Moved.md",
            &page("kind: redirect\nredirect: Plain\n", ""),
        );
        e.bases
            .put_page("pages/Category.md", &page("kind: category\n", "body"));
        let redirect = page("kind: redirect\nredirect: Elsewhere\n", "");
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/Plain.md", Some(&redirect))], vec![])
            ),
            Ok(()),
            "a page becomes a redirect"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change("pages/Moved.md", Some(&page("", "back")))],
                    vec![]
                )
            ),
            Ok(()),
            "and stops being one"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/New.md", Some(&redirect))], vec![])
            ),
            Ok(()),
            "a new redirect"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/Moved.md", None)], vec![])
            ),
            Ok(()),
            "a redirect removed"
        );
        // A redirect is not a way to carry another owned key, nor to leave a kind that is owned.
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change(
                        "pages/Plain.md",
                        Some(&page("kind: redirect\nvisibility: private\n", ""))
                    )],
                    vec![]
                )
            ),
            owned(&["visibility"])
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/Category.md", Some(&redirect))], vec![])
            ),
            owned(&["kind"]),
            "an owned kind turned into a redirect"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change("pages/Category.md", Some(&page("", "x")))],
                    vec![]
                )
            ),
            owned(&["kind"]),
            "an owned kind removed"
        );
    });
}

#[test]
fn removing_or_renaming_a_page_that_carries_an_owned_key_counts_as_changing_it() {
    on_both(|e| {
        e.bases
            .put_page("pages/Secret.md", &page("visibility: private\n", "body"));
        e.bases.put_page(
            "pages/System.md",
            &page("system: true\nkind: system\n", "body"),
        );
        e.bases.put_page("pages/Plain.md", &page("", "body"));
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![change("pages/Secret.md", None)], vec![])
            ),
            owned(&["visibility"]),
            "removing"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(vec![], vec![rename("pages/System.md", "pages/Sys.md")])
            ),
            owned(&["kind", "system"]),
            "renaming"
        );
        assert_eq!(
            create(
                e,
                &e.admin,
                proposal(vec![change("pages/Secret.md", None)], vec![])
            ),
            Ok(()),
            "an administrator may"
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change("pages/Plain.md", None)],
                    vec![rename("pages/Plain.md", "pages/Other.md")]
                )
            ),
            Ok(()),
            "an ordinary page may go"
        );
    });
}

#[test]
fn a_new_page_cannot_arrive_with_an_owned_key_and_unreadable_information_is_refused() {
    on_both(|e| {
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change(
                        "pages/Fresh.md",
                        Some(&page("visibility: private\n", "x"))
                    )],
                    vec![]
                )
            ),
            owned(&["visibility"])
        );
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change(
                        "pages/Fresh.md",
                        Some("---\ntitle: &anchor Home\n---\n\nx\n")
                    )],
                    vec![]
                )
            ),
            invalid(Invalid::FrontUnreadable)
        );
        // A text that is the page as it is changes nothing, readable or not.
        e.bases
            .put_page("pages/Broken.md", "---\ntitle: &anchor Home\n---\n\nx\n");
        assert_eq!(
            create(
                e,
                &e.author,
                proposal(
                    vec![change(
                        "pages/Broken.md",
                        Some("---\ntitle: &anchor Home\n---\n\nx\n")
                    )],
                    vec![]
                )
            ),
            Ok(())
        );
    });
}

#[test]
fn the_owned_keys_rule_holds_on_every_way_a_proposal_is_written_and_sent() {
    on_both(|e| {
        e.bases.put_page("pages/Plain.md", &page("", "body"));
        let me = &e.author;
        let draft = e
            .service
            .create(
                &me.principal,
                &e.wiki,
                None,
                proposal(
                    vec![change("pages/Plain.md", Some(&page("", "edited")))],
                    vec![],
                ),
            )
            .unwrap();
        assert_eq!(
            e.service
                .save_draft(
                    &me.principal,
                    &e.wiki,
                    &draft.id,
                    proposal(
                        vec![change("pages/Plain.md", Some(&page("system: true\n", "x")))],
                        vec![]
                    ),
                )
                .map(|_| ()),
            owned(&["system"])
        );
        // The repository changes under the draft: the page now carries a key, and the draft's text removes it.
        e.bases
            .put_page("pages/Plain.md", &page("visibility: private\n", "body"));
        assert_eq!(
            e.service
                .submit(&me.principal, &e.wiki, &draft.id)
                .map(|_| ()),
            owned(&["visibility"])
        );
        e.bases.put_page("pages/Plain.md", &page("", "body"));
        let held = e.service.submit(&me.principal, &e.wiki, &draft.id).unwrap();
        assert_eq!(held.state, ProposalState::HeldForReview);
        assert_eq!(
            e.service
                .revise(
                    &me.principal,
                    &e.wiki,
                    &draft.id,
                    proposal(
                        vec![change(
                            "pages/Plain.md",
                            Some(&page("kind: category\n", "x"))
                        )],
                        vec![]
                    ),
                )
                .map(|_| ()),
            owned(&["kind"])
        );
    });
}

#[test]
fn a_rollback_plan_cannot_change_an_owned_key_for_someone_who_may_not() {
    for kind in [Kind::Memory, Kind::Sqlite] {
        let e = Env::new(kind);
        // The page now carries a key the plan would take away (the rollback restores the earlier text).
        e.bases
            .put_page("pages/Secret.md", &page("visibility: private\n", "body"));
        let plan = |files: Vec<FileChange>| RollbackPlan {
            undoes: "c".repeat(64),
            number: 3,
            undone_summary: "A change".to_owned(),
            base: base(1),
            files,
            summary: "Undo: a change".to_owned(),
        };
        let files = || vec![change("pages/Secret.md", Some(&page("", "body")))];
        assert_eq!(
            e.service
                .create_rollback(&e.author.principal, &e.wiki, None, &plan(files()))
                .map(|_| ())
                .unwrap_err(),
            ProposalError::OwnedKeys(vec!["visibility"])
        );
        assert!(
            e.service
                .create_rollback(&e.admin.principal, &e.wiki, None, &plan(files()))
                .is_ok(),
            "an administrator may undo it"
        );
        // And a plan that undoes the creation of a page that carries one is a removal.
        assert_eq!(
            e.service
                .create_rollback(
                    &e.author.principal,
                    &e.wiki,
                    None,
                    &plan(vec![change("pages/Secret.md", None)])
                )
                .map(|_| ())
                .unwrap_err(),
            ProposalError::OwnedKeys(vec!["visibility"])
        );
    }
}
