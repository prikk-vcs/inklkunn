//! The foreign-process witness: which processes of the host may be working in a repository (handoff 004 T5).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

use super::*;

const REPO: &str = "/srv/wikis/01/01a1";
const OURS: u32 = 100;

/// The `i`-th of a list that must have it (tests may not index).
fn nth<T>(items: &[T], i: usize) -> &T {
    items.get(i).expect("an element")
}

fn p(pid: u32, parent: u32, argv: &[&str], cwd: Option<&str>) -> ProcessInfo {
    ProcessInfo {
        pid,
        parent,
        argv: argv.iter().map(|s| (*s).to_owned()).collect(),
        cwd: cwd.map(str::to_owned),
        readable: cwd.is_some(),
    }
}

fn found(table: &[ProcessInfo]) -> Vec<u32> {
    foreign(table, Path::new(REPO), OURS)
        .into_iter()
        .map(|f| f.pid)
        .collect()
}

#[test]
fn a_tool_working_in_the_repository_is_foreign_by_its_words_or_by_its_directory() {
    let table = [
        // A confined prikk: bwrap with the repository on its command line.
        p(
            200,
            1,
            &["bwrap", "--bind", REPO, "/r", "prikk", "verify"],
            Some("/"),
        ),
        // An unconfined prikk run in the repository's directory.
        p(
            201,
            1,
            &["/usr/bin/prikk", "seal"],
            Some("/srv/wikis/01/01a1/pages"),
        ),
        // A script standing in for prikk, in the repository.
        p(202, 1, &["/bin/sh", "/tmp/ctl/prikk", "commit"], Some(REPO)),
    ];
    assert_eq!(found(&table), vec![200, 201, 202]);
}

#[test]
fn what_is_not_a_tool_or_not_in_this_repository_is_not_foreign() {
    let table = [
        // A tool working elsewhere (the next wiki's directory shares a prefix of text, not a path).
        p(
            300,
            1,
            &["prikk", "verify"],
            Some("/srv/wikis/01/01a1-other"),
        ),
        p(
            301,
            1,
            &["bwrap", "--bind", "/srv/wikis/01/01a2", "/r", "prikk"],
            Some("/"),
        ),
        // Not a tool at all, though it is in the repository.
        p(302, 1, &["vim", "pages/Home.md"], Some(REPO)),
        p(303, 1, &["/bin/sh"], Some(REPO)),
    ];
    assert!(found(&table).is_empty(), "{:?}", found(&table));
}

#[test]
fn this_process_and_its_descendants_are_ours_and_a_stranger_beside_them_is_not() {
    let table = [
        p(OURS, 1, &["inklkunn", "serve"], Some("/")),
        // The reads the server is making: its children, and theirs.
        p(
            110,
            OURS,
            &["bwrap", "--bind", REPO, "/r", "prikk", "log"],
            Some("/"),
        ),
        p(111, 110, &["prikk", "log"], Some(REPO)),
        // Somebody else's, in the same repository.
        p(400, 1, &["prikk", "commit"], Some(REPO)),
    ];
    assert_eq!(found(&table), vec![400]);
}

#[test]
fn a_tool_that_could_not_be_read_is_counted_and_a_tool_with_no_directory_may_be_here() {
    let unreadable = ProcessInfo {
        pid: 500,
        parent: 0,
        argv: Vec::new(),
        cwd: None,
        readable: false,
    };
    let no_cwd = p(501, 1, &["prikk", "verify"], None);
    // Not being able to see is not seeing nothing.
    let out = foreign(&[unreadable, no_cwd], Path::new(REPO), OURS);
    assert_eq!(out.len(), 2);
    assert_eq!(nth(&out, 0).tool, "unreadable");
    assert_eq!(nth(&out, 1).tool, "prikk");
    // A process with no tool in its words and no directory read is not a tool.
    assert!(found(&[p(502, 1, &["sleep", "60"], None)]).is_empty());
}

#[test]
fn the_real_table_lists_this_process_and_its_parent_chain_ends() {
    let table = ProcFs.processes();
    let me = std::process::id();
    let mine = table.iter().find(|x| x.pid == me).expect("this process");
    assert!(!mine.argv.is_empty());
    assert!(mine.parent != 0, "the parent of a test process is readable");
    // Nothing of this process's own is foreign, whatever repository is asked about.
    let cwd = std::env::current_dir().unwrap();
    assert!(
        foreign(&table, &cwd, me).iter().all(|f| f.pid != me),
        "ourselves are not foreign"
    );
}
