#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use crate::accept::PreparedFile;

fn file(path: &str, content: Option<&str>) -> PreparedFile {
    PreparedFile {
        path: PathBuf::from(path),
        content: content.map(|c| c.as_bytes().to_vec()),
    }
}

fn change(files: Vec<PreparedFile>, renames: Vec<(&str, &str)>) -> PreparedChange {
    PreparedChange {
        files,
        renames: renames
            .into_iter()
            .map(|(a, b)| (PathBuf::from(a), PathBuf::from(b)))
            .collect(),
        message: String::new(),
    }
}

struct Dir(PathBuf);
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn temp(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-wt-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Dir(p)
}

#[test]
fn paths_are_ascii_relative_under_a_root_and_obey_prikks_rules() {
    for ok in [
        "pages/Home.md",
        "pages/Guides/Getting Started.md",
        "pages/%E6%97%A5%E6%9C%AC.md",
        "attachments/diagram.svg.meta.yaml",
        "knowledge/.keep",
        "governance/policy.yaml",
        "wiki-bundle.yaml",
    ] {
        assert!(repo_path(Path::new(ok)).is_ok(), "{ok}");
    }
    let long_component = format!("pages/{}.md", "a".repeat(260));
    let long_path = format!("pages/{}", "a/".repeat(600));
    for (bad, why) in [
        ("", "empty"),
        ("/pages/x.md", "absolute"),
        ("pages/../x.md", "dotdot"),
        ("pages/./x.md", "dot"),
        ("pages//x.md", "a doubled slash"),
        ("pages/x.md/", "a trailing slash"),
        ("x.md", "outside the roots"),
        ("pages", "a root alone"),
        (".prikk/x", "inside prikk"),
        ("pages/.hidden.md", "leading dot"),
        ("pages/日本語.md", "non-ASCII"),
        ("pages/a\\b.md", "backslash"),
        ("pages/a:b.md", "colon"),
        ("pages/a\tb.md", "control"),
        ("pages/trail.", "trailing dot"),
        ("pages/trail ", "trailing space"),
        ("pages/CON.md", "windows reserved"),
        ("pages/nul", "windows reserved"),
        ("pages/lpt1.txt", "windows reserved"),
        ("wiki-bundle.yaml/x", "under the manifest"),
        (".inklkunn/x", "service prefix"),
        (long_component.as_str(), "component too long"),
        (long_path.as_str(), "path too long"),
    ] {
        let r = repo_path(Path::new(bad));
        assert!(r.is_err(), "{why}: {bad:?}");
        // A refusal states a rule and never echoes the path.
        let text = r.unwrap_err();
        // (A short input such as `pages` is a word the rule itself uses; the long ones are the test.)
        assert!(!(bad.len() > 12 && text.contains(bad)), "{text}");
    }
}

#[test]
fn a_plan_is_all_or_nothing_and_collisions_are_refused() {
    let ok = change(
        vec![file("pages/A.md", Some("a")), file("pages/B.md", None)],
        vec![("pages/C.md", "pages/D.md")],
    );
    let plan = Plan::new(&ok).unwrap();
    assert_eq!(plan.writes.len(), 1);
    assert_eq!(plan.deletes, ["pages/B.md"]);
    assert_eq!(
        plan.renames,
        [("pages/C.md".to_owned(), "pages/D.md".to_owned())]
    );
    // A renamed page's new path may carry new content.
    assert!(
        Plan::new(&change(
            vec![file("pages/D.md", Some("new text"))],
            vec![("pages/C.md", "pages/D.md")]
        ))
        .is_ok()
    );
    for (what, c) in [
        ("empty", change(vec![], vec![])),
        (
            "a bad path among good ones",
            change(
                vec![file("pages/A.md", Some("a")), file("x/../y", Some("b"))],
                vec![],
            ),
        ),
        (
            "a path twice",
            change(
                vec![file("pages/A.md", Some("a")), file("pages/A.md", Some("b"))],
                vec![],
            ),
        ),
        (
            "write and remove",
            change(
                vec![file("pages/A.md", Some("a")), file("pages/A.md", None)],
                vec![],
            ),
        ),
        (
            "rename onto itself",
            change(vec![], vec![("pages/A.md", "pages/A.md")]),
        ),
        (
            "old name also written",
            change(
                vec![file("pages/C.md", Some("x"))],
                vec![("pages/C.md", "pages/D.md")],
            ),
        ),
        (
            "two renames to one target",
            change(
                vec![],
                vec![("pages/A.md", "pages/C.md"), ("pages/B.md", "pages/C.md")],
            ),
        ),
        (
            "one source renamed twice",
            change(
                vec![],
                vec![("pages/A.md", "pages/B.md"), ("pages/A.md", "pages/C.md")],
            ),
        ),
        (
            "a chain",
            change(
                vec![],
                vec![("pages/A.md", "pages/B.md"), ("pages/B.md", "pages/C.md")],
            ),
        ),
        (
            "a swap",
            change(
                vec![],
                vec![("pages/A.md", "pages/B.md"), ("pages/B.md", "pages/A.md")],
            ),
        ),
        (
            "renaming the manifest",
            change(vec![], vec![("wiki-bundle.yaml", "pages/X.md")]),
        ),
    ] {
        assert!(Plan::new(&c).is_err(), "{what}");
    }
    let many = change(
        (0..=MAX_FILES)
            .map(|i| file(&format!("pages/P{i}.md"), Some("x")))
            .collect(),
        vec![],
    );
    assert!(
        Plan::new(&many).is_err(),
        "more files than one acceptance may carry"
    );
}

#[test]
fn writes_are_atomic_create_parents_and_are_undone_exactly() {
    let tmp = temp("apply");
    let root = &tmp.0;
    std::fs::create_dir_all(root.join("pages")).unwrap();
    std::fs::write(root.join("pages/Keep.md"), "keep").unwrap();
    std::fs::write(root.join("pages/Edit.md"), "before").unwrap();
    std::fs::write(root.join("pages/Gone.md"), "gone").unwrap();
    let mut applied = Applied::default();
    applied.write(root, "pages/Edit.md", b"after").unwrap();
    applied
        .write(root, "pages/New/Deep/Page.md", b"new")
        .unwrap();
    applied.delete(root, "pages/Gone.md").unwrap();
    assert_eq!(std::fs::read(root.join("pages/Edit.md")).unwrap(), b"after");
    assert_eq!(
        std::fs::read(root.join("pages/New/Deep/Page.md")).unwrap(),
        b"new"
    );
    assert!(!root.join("pages/Gone.md").exists());
    // No temporary file is left beside a written one.
    let names: Vec<_> = std::fs::read_dir(root.join("pages"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        !names.iter().any(|n| n.contains("inklkunn-tmp")),
        "{names:?}"
    );
    assert!(applied.undo());
    assert_eq!(
        std::fs::read(root.join("pages/Edit.md")).unwrap(),
        b"before"
    );
    assert_eq!(std::fs::read(root.join("pages/Gone.md")).unwrap(), b"gone");
    assert_eq!(std::fs::read(root.join("pages/Keep.md")).unwrap(), b"keep");
    assert!(
        !root.join("pages/New").exists(),
        "the directories it made are gone"
    );
}

#[test]
fn removing_the_last_file_of_a_directory_takes_the_directory_and_the_file_can_then_be_written_where_it_stood()
 {
    // Handoff G-T1 correction D-1: a directory stands where `governance/policy.yaml` belongs; the change removes the files
    // below it and writes the file.
    let tmp = temp("dir-to-file");
    let root = &tmp.0;
    std::fs::create_dir_all(root.join("governance/policy.yaml/sub")).unwrap();
    std::fs::write(root.join("governance/policy.yaml/inner.txt"), "x").unwrap();
    std::fs::write(root.join("governance/policy.yaml/sub/deeper.txt"), "y").unwrap();
    std::fs::write(root.join("governance/constitution.yaml"), "keep").unwrap();
    let mut applied = Applied::default();
    // Written before the directory is gone: refused, and nothing is touched.
    assert!(applied.write(root, "governance/policy.yaml", b"p").is_err());
    applied
        .delete(root, "governance/policy.yaml/sub/deeper.txt")
        .unwrap();
    assert!(
        !root.join("governance/policy.yaml/sub").exists(),
        "the directory the removal emptied is gone"
    );
    assert!(root.join("governance/policy.yaml/inner.txt").exists());
    applied
        .delete(root, "governance/policy.yaml/inner.txt")
        .unwrap();
    assert!(!root.join("governance/policy.yaml").exists());
    // Never a directory of the bundle's roots, and never what holds another file.
    assert!(root.join("governance").is_dir());
    assert!(root.join("governance/constitution.yaml").is_file());
    applied.write(root, "governance/policy.yaml", b"p").unwrap();
    assert_eq!(
        std::fs::read(root.join("governance/policy.yaml")).unwrap(),
        b"p"
    );
    // Undone: the file is gone and the directory and the files below it are back, as they were.
    assert!(applied.undo());
    assert!(root.join("governance/policy.yaml").is_dir());
    assert_eq!(
        std::fs::read(root.join("governance/policy.yaml/inner.txt")).unwrap(),
        b"x"
    );
    assert_eq!(
        std::fs::read(root.join("governance/policy.yaml/sub/deeper.txt")).unwrap(),
        b"y"
    );
    assert_eq!(
        std::fs::read(root.join("governance/constitution.yaml")).unwrap(),
        b"keep"
    );
}

#[test]
fn removing_a_file_leaves_a_directory_that_still_holds_something_and_never_a_root() {
    let tmp = temp("dir-kept");
    let root = &tmp.0;
    std::fs::create_dir_all(root.join("pages/Sub")).unwrap();
    std::fs::write(root.join("pages/Sub/A.md"), "a").unwrap();
    std::fs::write(root.join("pages/Sub/B.md"), "b").unwrap();
    std::fs::write(root.join("pages/Only.md"), "o").unwrap();
    let mut applied = Applied::default();
    applied.delete(root, "pages/Sub/A.md").unwrap();
    assert!(root.join("pages/Sub/B.md").exists(), "a neighbour is kept");
    applied.delete(root, "pages/Only.md").unwrap();
    assert!(
        root.join("pages").is_dir(),
        "a root of the bundle is never taken"
    );
    assert!(applied.undo());
    assert_eq!(std::fs::read(root.join("pages/Sub/A.md")).unwrap(), b"a");
    assert_eq!(std::fs::read(root.join("pages/Only.md")).unwrap(), b"o");
}

#[test]
fn a_link_is_never_followed_and_nothing_is_written_through_one() {
    let tmp = temp("links");
    let root = tmp.0.join("wt");
    let outside = tmp.0.join("outside");
    std::fs::create_dir_all(root.join("pages")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("target.txt"), "untouched").unwrap();
    // A directory that is a link, a file that is a link, and a path under a file.
    std::os::unix::fs::symlink(&outside, root.join("pages/Dir")).unwrap();
    std::os::unix::fs::symlink(outside.join("target.txt"), root.join("pages/Link.md")).unwrap();
    std::fs::write(root.join("pages/File.md"), "f").unwrap();
    let mut applied = Applied::default();
    assert!(applied.write(&root, "pages/Dir/x.md", b"x").is_err());
    assert!(applied.write(&root, "pages/Link.md", b"x").is_err());
    assert!(
        applied
            .write(&root, "pages/File.md/under.md", b"x")
            .is_err()
    );
    assert!(applied.delete(&root, "pages/Link.md").is_err());
    assert!(applied.delete(&root, "pages/Dir").is_err());
    assert_eq!(
        std::fs::read(outside.join("target.txt")).unwrap(),
        b"untouched"
    );
    assert!(!outside.join("x.md").exists());
    assert!(applied.undo());
}

#[test]
fn removing_what_is_not_there_or_not_a_file_is_refused() {
    let tmp = temp("rm");
    std::fs::create_dir_all(tmp.0.join("pages/Sub")).unwrap();
    let mut applied = Applied::default();
    assert!(applied.delete(&tmp.0, "pages/Missing.md").is_err());
    assert!(applied.delete(&tmp.0, "pages/Sub").is_err());
}
