//! `wiki import-bundle` on the command line (RFC 007 D-1, D-4 – D-6; handoff I-T2): the plan and the report as a dry run, and the
//! run that refuses with nothing written. **None of these needs prikk**: the plan reads a directory, and nothing else.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use inklkunn_store::{NewWiki, Slug, SqliteStore, WikiStore};

use super::*;
use crate::cli;

fn args(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| (*x).to_owned()).collect()
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-bcli-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Dir(p)
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../inklkunn-core/tests/fixtures/interchange")
        .join(rel)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn all_files(root: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(at).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                walk(root, &e.path(), out);
            } else {
                out.push(
                    e.path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                );
            }
        }
    }
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, &mut out);
    }
    out.sort();
    out
}

/// Set a member of a JSON object (a test may not index).
fn set(v: &mut serde_json::Value, key: &str, x: impl Into<serde_json::Value>) {
    v.as_object_mut().unwrap().insert(key.to_owned(), x.into());
}

/// A bundle with a licence, a page and three revisions of it (and no checksums: the cases that need them start from a fixture).
fn good(tag: &str) -> Dir {
    let d = dir(tag);
    put(
        &d.0,
        "wiki-bundle.yaml",
        b"format: portable-wiki-bundle\nversion: '0.4'\nlicense: CC-BY-SA-4.0\n",
    );
    put(&d.0, "pages/Home.md", text(3).as_bytes());
    let lines: Vec<String> = (1..=3)
        .map(|n| {
            serde_json::json!({
                "rev": format!("r{n}"),
                "parent": if n == 1 { serde_json::Value::Null } else { format!("r{}", n - 1).into() },
                "type": "edit", "at": format!("2025-01-0{n}T00:00:00Z"),
                "author": {"id": "ann", "name": "Ann"},
                "content": text(n),
            })
            .to_string()
        })
        .collect();
    put(
        &d.0,
        "history/home.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    d
}

/// The lines of a file, each changed by `f` when it is the `n`th (from 1), and the file written again.
fn change_line(root: &Path, rel: &str, n: usize, f: impl Fn(&str) -> String) {
    let text = std::fs::read_to_string(root.join(rel)).unwrap();
    let lines: Vec<String> = text
        .lines()
        .enumerate()
        .map(|(i, l)| if i + 1 == n { f(l) } else { l.to_owned() })
        .collect();
    put(root, rel, (lines.join("\n") + "\n").as_bytes());
}

fn text(n: u32) -> String {
    format!("---\nid: home\ntitle: Home\n---\n\nVersion {n}.\n")
}

/// The command line against a data directory that need not exist (the plan needs no prikk, no store and no key).
struct Run {
    data: PathBuf,
    secret: PathBuf,
}

impl Run {
    fn new(base: &Path) -> Run {
        Run {
            data: base.join("data"),
            secret: base.join("secret"),
        }
    }

    fn cli(&self, env: &[(&str, &str)], argv: &[&str]) -> (i32, String, String) {
        let mut vars: HashMap<&str, OsString> = HashMap::from([
            ("INKLKUNN_DATA_DIR", self.data.clone().into_os_string()),
            ("INKLKUNN_SECRET_FILE", self.secret.clone().into_os_string()),
        ]);
        for (k, v) in env {
            vars.insert(k, OsString::from(v));
        }
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = cli::run(
            &args(argv),
            &|k| vars.get(k).cloned(),
            1_800_000_000,
            &mut out,
            &mut err,
        );
        (
            status,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }
}

#[test]
fn import_bundle_parses_with_create_options_and_its_own_and_has_no_way_to_skip_checksums_or_change_the_licence()
 {
    let a = parse(&args(&[
        "import-bundle",
        "garden",
        "/in/bundle",
        "--name",
        "The Garden",
        "--lang",
        "en",
        "--lang",
        "ja",
        "--visibility",
        "restricted",
        "--bundle-license",
        "CC0-1.0",
        "--leave-out-incompatible",
        "--no-history",
        "--dry-run",
        "--config",
        "/etc/inklkunn.conf",
    ]))
    .unwrap();
    assert_eq!(
        a,
        Command::ImportBundle(ImportBundleArgs {
            slug: "garden".into(),
            dir: "/in/bundle".into(),
            name: "The Garden".into(),
            languages: vec!["en".into(), "ja".into()],
            visibility: Some("restricted".into()),
            bundle_license: Some("CC0-1.0".into()),
            leave_out_incompatible: true,
            no_history: true,
            dry_run: true,
            config: Some("/etc/inklkunn.conf".into()),
        })
    );
    assert_eq!(a.config_file(), Some("/etc/inklkunn.conf"));
    let minimal = parse(&args(&["import-bundle", "g", "/b", "--name", "G"])).unwrap();
    let Command::ImportBundle(m) = minimal else {
        panic!("not an import-bundle");
    };
    assert!(!m.dry_run && !m.no_history && !m.leave_out_incompatible && m.bundle_license.is_none());
    // What is not understood is a usage error that shows the usage: no name, no bundle, an unknown option, and the two options a
    // careful reader looks for and that do not exist (there is no way to skip checksums, and the licence is the bundle's).
    for (bad, said) in [
        (vec!["import-bundle", "g", "/b"], "--name"),
        (vec!["import-bundle", "g", "--name", "G"], "which bundle"),
        (
            vec!["import-bundle", "g", "/b", "--name", "G", "--frobnicate"],
            "not an option of `import-bundle`",
        ),
        (
            vec!["import-bundle", "g", "/b", "--name", "G", "--no-checksums"],
            "not an option of `import-bundle`",
        ),
        (
            vec![
                "import-bundle",
                "g",
                "/b",
                "--name",
                "G",
                "--skip-checksums",
            ],
            "not an option of `import-bundle`",
        ),
        (
            vec![
                "import-bundle",
                "g",
                "/b",
                "--name",
                "G",
                "--license",
                "CC0-1.0",
            ],
            "--bundle-license",
        ),
        (
            vec![
                "import-bundle",
                "g",
                "/b",
                "--name",
                "G",
                "--bundle-license",
            ],
            "needs a value",
        ),
        (
            vec![
                "import-bundle",
                "g",
                "/b",
                "--name",
                "G",
                "--bundle-license",
                "a",
                "--bundle-license",
                "b",
            ],
            "given once",
        ),
        (
            vec![
                "import-bundle",
                "g",
                "/b",
                "--name",
                "G",
                "--config",
                "--dry-run",
            ],
            "is an option",
        ),
    ] {
        let Err(Usage(why)) = parse(&args(&bad)) else {
            panic!("{bad:?} was accepted");
        };
        assert!(
            why.contains(said) && why.contains("usage:"),
            "{bad:?}: {why}"
        );
        assert!(
            why.contains("wiki import-bundle <new-wiki> <bundle-dir>"),
            "{why}"
        );
    }
    // The command is in the list of commands, and the process tells a mistaken one apart from it.
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = cli::run(
        &args(&["wiki", "frobnicate"]),
        &|_| None,
        0,
        &mut out,
        &mut err,
    );
    assert_eq!(status, 2);
    assert!(
        String::from_utf8(err)
            .unwrap()
            .contains("`wiki import-bundle`")
    );
}

#[test]
fn a_dry_run_prints_the_plan_of_each_fixture_and_writes_nothing_anywhere() {
    let base = dir("dry");
    let run = Run::new(&base.0);
    let before_bundle = all_files(&fixture("mediawiki/bundle"));
    let (status, out, err) = run.cli(
        &[],
        &[
            "wiki",
            "import-bundle",
            "garden",
            fixture("mediawiki/bundle").to_str().unwrap(),
            "--name",
            "Garden",
            "--bundle-license",
            "CC-BY-SA-4.0",
            "--dry-run",
        ],
    );
    assert_eq!(status, 0, "{out}{err}");
    for needle in [
        "plan for a new wiki `garden` (Garden), visibility public",
        "fidelity declared: level 3",
        "fidelity found:    pages, attachments and history (7 revision(s) in 4 page(s)",
        "licence of the wiki: CC-BY-SA-4.0 (the bundle declares none; declared by the importing admin",
        "checksums: verified, every file against `sha256sums.txt` (9 file(s)",
        "7 revision(s) would be replayed, one block each (the cap is 1000)",
        "suppressions kept: 1 record(s) with their text hidden at the wiki it was imported from",
        "people named: 3",
        "wikipedia: 1 link(s)",
        "dry run: nothing was written",
    ] {
        assert!(out.contains(needle), "`{needle}` is not in:\n{out}");
    }
    // Nothing was made: not the data directory, not the store, not a key, not a scratch, and the bundle is as it was.
    assert!(!run.data.exists() && !run.secret.exists());
    assert_eq!(all_files(&fixture("mediawiki/bundle")), before_bundle);
    assert_eq!(all_files(&base.0), Vec::<String>::new());
    // The BookStack bundle and inklkunn's own, with the licence they have.
    let (status, out, err) = run.cli(
        &[],
        &[
            "wiki",
            "import-bundle",
            "handbook",
            fixture("bookstack/bundle").to_str().unwrap(),
            "--name",
            "Handbook",
            "--bundle-license",
            "CC0-1.0",
            "--dry-run",
        ],
    );
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains("to come in: 5 page(s), 2 attachment(s) with 2 sidecar(s)"),
        "{out}"
    );
    assert!(
        out.contains("fidelity declared: level 2")
            && out.contains("no revision that can be replayed"),
        "{out}"
    );
    let (status, out, err) = run.cli(
        &[],
        &[
            "wiki",
            "import-bundle",
            "orchard",
            fixture("inklkunn/bundle").to_str().unwrap(),
            "--name",
            "Orchard",
            "--dry-run",
        ],
    );
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains("licence of the wiki: CC0-1.0 (the bundle's own"),
        "{out}"
    );
    assert!(out.contains("6 revision(s) would be replayed"), "{out}");
    assert!(!run.data.exists());
}

/// Every refusal of the acceptance criterion's list, as a bundle, the rule it must name and (where the bundle alone does not
/// do it) the settings that bring it about.
struct Case {
    tag: &'static str,
    /// Start from a copy of inklkunn's own export (which has its checksums) and not from [`good`].
    from_fixture: bool,
    make: fn(&Path),
    env: &'static [(&'static str, &'static str)],
    name: &'static str,
    rule: &'static str,
}

fn replace_manifest(root: &Path, from: &str, to: &str) {
    let text = std::fs::read_to_string(root.join("wiki-bundle.yaml")).unwrap();
    std::fs::write(root.join("wiki-bundle.yaml"), text.replace(from, to)).unwrap();
}

#[test]
fn every_refusal_of_the_criterion_is_named_the_same_on_a_dry_run_and_on_a_run_and_nothing_is_written()
 {
    let cases = [
        Case {
            tag: "licence",
            from_fixture: false,
            make: |r| {
                put(
                    r,
                    "pages/Tied.md",
                    b"---\nid: tied\ntitle: Tied\nlicense: CC-BY-NC-4.0\n---\n\nx\n",
                );
            },
            env: &[],
            name: "pages/Tied.md",
            rule: "CC-BY-NC-4.0",
        },
        Case {
            tag: "sum-lie",
            from_fixture: true,
            make: |r| {
                put(
                    r,
                    "pages/Pruning.md",
                    b"---\nid: pruning\n---\n\nchanged after the sums\n",
                )
            },
            env: &[],
            name: "pages/Pruning.md",
            rule: "SHA-256 that is not the one the checksums file gives it",
        },
        Case {
            tag: "sum-gap",
            from_fixture: true,
            make: |r| std::fs::remove_file(r.join("attachments/cut.txt")).unwrap(),
            env: &[],
            name: "attachments/cut.txt",
            rule: "missing from the bundle",
        },
        Case {
            tag: "sum-extra",
            from_fixture: true,
            make: |r| put(r, "pages/Extra.md", b"---\nid: extra\n---\n\nx\n"),
            env: &[],
            name: "pages/Extra.md",
            rule: "the checksums file does not name it",
        },
        Case {
            tag: "sha-content",
            from_fixture: false,
            make: |r| {
                change_line(r, "history/home.jsonl", 2, |l| {
                    let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
                    set(&mut v, "sha256", "a".repeat(64));
                    v.to_string()
                });
            },
            env: &[],
            name: "history/home.jsonl",
            rule: "rev `r2` has a `sha256` that is not the text's",
        },
        Case {
            tag: "link",
            from_fixture: false,
            make: |r| {
                std::os::unix::fs::symlink("/etc/passwd", r.join("pages/Evil.md")).unwrap();
            },
            env: &[],
            name: "pages/Evil.md",
            rule: "is a symbolic link",
        },
        Case {
            tag: "path-cap",
            from_fixture: false,
            make: |r| {
                let deep = (0..12)
                    .map(|i| format!("{i}{}", "d".repeat(95)))
                    .collect::<Vec<_>>()
                    .join("/");
                put(r, &format!("attachments/{deep}/file.bin"), b"x");
            },
            env: &[],
            name: "attachments/0ddd",
            rule: "over the 1024-byte cap",
        },
        Case {
            tag: "page-cap",
            from_fixture: false,
            make: |r| {
                change_line(r, "history/home.jsonl", 1, |l| {
                    let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
                    set(
                        &mut v,
                        "content",
                        format!("---\nid: home\n---\n\n{}\n", "x".repeat(3000)),
                    );
                    v.to_string()
                });
            },
            env: &[("INKLKUNN_PAGE_MAX_KIB", "2")],
            name: "history/home.jsonl",
            rule: "rev `r1` has a text larger than the 2 KiB cap of a page",
        },
        Case {
            tag: "revision-cap",
            from_fixture: false,
            make: |_| {},
            env: &[("INKLKUNN_IMPORT_MAX_REVISIONS", "2")],
            name: "history/",
            rule: "would make 3 blocks, and the cap is 2",
        },
        Case {
            tag: "address",
            from_fixture: false,
            make: |r| {
                put(
                    r,
                    "users.yaml",
                    b"- id: ann\n  name: Ann\n  email: ann@example.org\n",
                );
            },
            env: &[],
            name: "users.yaml",
            rule: "entry 1 (id `ann`) has an e-mail address",
        },
        Case {
            tag: "no-licence",
            from_fixture: false,
            make: |r| replace_manifest(r, "license: CC-BY-SA-4.0\n", ""),
            env: &[],
            name: "wiki-bundle.yaml",
            rule: "declares no `license`",
        },
        Case {
            tag: "order",
            from_fixture: false,
            make: |r| {
                change_line(r, "history/home.jsonl", 3, |l| {
                    l.replace("\"parent\":\"r2\"", "\"parent\":\"r1\"")
                });
            },
            env: &[],
            name: "history/home.jsonl",
            rule: "rev `r3` has a `parent` that is not the record before it",
        },
        Case {
            tag: "time-order",
            from_fixture: false,
            make: |r| {
                change_line(r, "history/home.jsonl", 3, |l| {
                    l.replace("2025-01-03T00:00:00Z", "2025-01-01T12:00:00Z")
                });
            },
            env: &[],
            name: "history/home.jsonl",
            rule: "rev `r3` is earlier than the record before it",
        },
    ];
    for case in cases {
        let bundle = if case.from_fixture {
            let d = dir(case.tag);
            copy_tree(&fixture("inklkunn/bundle"), &d.0);
            d
        } else {
            good(case.tag)
        };
        (case.make)(&bundle.0);
        let base = dir(&format!("{}-run", case.tag));
        let run = Run::new(&base.0);
        let mut outputs = Vec::new();
        for dry_run in [true, false] {
            let mut argv = vec![
                "wiki",
                "import-bundle",
                "garden",
                bundle.0.to_str().unwrap(),
                "--name",
                "Garden",
            ];
            if dry_run {
                argv.push("--dry-run");
            }
            let (status, out, err) = run.cli(case.env, &argv);
            assert_eq!(status, 1, "{}: dry_run={dry_run}\n{out}{err}", case.tag);
            assert!(
                out.lines()
                    .any(|l| l.trim_start().starts_with(case.name) && l.contains(case.rule)),
                "{}: `{}… {}` is not in\n{out}",
                case.tag,
                case.name,
                case.rule
            );
            assert!(
                err.contains(
                    "refused, so none of the bundle would be imported; nothing was written"
                ),
                "{}: {err}",
                case.tag
            );
            assert!(!err.contains("not built yet"), "{}: {err}", case.tag);
            // Nothing is made, whichever way it was asked.
            assert!(
                !run.data.exists(),
                "{}: the data directory was made",
                case.tag
            );
            outputs.push(out);
        }
        // The refusals are the same lines on the dry run and on the run.
        let refusals = |out: &str| -> Vec<String> {
            let mut on = false;
            out.lines()
                .filter(|l| {
                    if l.starts_with("refused (") {
                        on = true;
                    } else if !l.starts_with(' ') {
                        on = false;
                    }
                    on && l.starts_with("  ")
                })
                .map(str::to_owned)
                .collect()
        };
        let mut both = outputs.iter().map(|o| refusals(o));
        assert_eq!(both.next(), both.next(), "{}", case.tag);
    }
}

#[test]
fn leaving_out_and_no_history_are_the_ways_on_and_the_run_still_writes_nothing() {
    let base = dir("ways-on");
    let run = Run::new(&base.0);
    let bundle = good("ways-on-bundle");
    put(
        &bundle.0,
        "pages/Tied.md",
        b"---\nid: tied\ntitle: Tied\nlicense: CC-BY-NC-4.0\n---\n\nx\n",
    );
    let path = bundle.0.to_str().unwrap();
    let base_argv = [
        "wiki",
        "import-bundle",
        "garden",
        path,
        "--name",
        "Garden",
        "--dry-run",
    ];
    let (status, out, _) = run.cli(&[], &base_argv);
    assert_eq!(status, 1, "{out}");
    let mut leave = base_argv.to_vec();
    leave.push("--leave-out-incompatible");
    let (status, out, err) = run.cli(&[], &leave);
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains("left out for their licence (--leave-out-incompatible):")
            && out.contains("page pages/Tied.md: CC-BY-NC-4.0"),
        "{out}"
    );
    assert!(out.contains("to come in: 1 page(s)"), "{out}");
    // Over the cap, with the way on said in the refusal and taken.
    let capped = [("INKLKUNN_IMPORT_MAX_REVISIONS", "1")];
    let (status, out, _) = run.cli(&capped, &leave);
    assert_eq!(status, 1);
    assert!(
        out.contains("would make 3 blocks, and the cap is 1") && out.contains("`--no-history`"),
        "{out}"
    );
    let mut none = leave.clone();
    none.push("--no-history");
    let (status, out, err) = run.cli(&capped, &none);
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains("history: not imported (--no-history)"),
        "{out}"
    );
    assert!(!run.data.exists());
}

#[test]
fn an_existing_slug_is_refused_by_a_sentence_that_names_import_folder_and_the_store_is_left_as_it_was()
 {
    let base = dir("slug");
    let run = Run::new(&base.0);
    std::fs::create_dir_all(&run.data).unwrap();
    let store_path = run.data.join("store.sqlite");
    {
        let store = SqliteStore::open(&store_path).unwrap();
        store
            .create(NewWiki::new(
                Slug::new("garden").unwrap(),
                inklkunn_store::DisplayName::new("The first Garden").unwrap(),
            ))
            .unwrap();
    }
    let before = (
        all_files(&base.0),
        std::fs::read(&store_path).unwrap().len(),
    );
    let bundle = good("slug-bundle");
    for dry in [true, false] {
        let mut argv = vec![
            "wiki",
            "import-bundle",
            "garden",
            bundle.0.to_str().unwrap(),
            "--name",
            "Garden",
        ];
        if dry {
            argv.push("--dry-run");
        }
        let (status, out, err) = run.cli(&[], &argv);
        assert_eq!(status, 1, "{out}{err}");
        assert!(
            err.contains("a wiki `garden` exists already")
                && err.contains("never changes one")
                && err.contains("wiki import-folder garden"),
            "{err}"
        );
        // Judged before the bundle is read.
        assert!(out.is_empty(), "{out}");
    }
    assert_eq!(
        (
            all_files(&base.0),
            std::fs::read(&store_path).unwrap().len()
        ),
        before
    );
    // Another slug in the same deployment is free.
    let (status, out, err) = run.cli(
        &[],
        &[
            "wiki",
            "import-bundle",
            "orchard",
            bundle.0.to_str().unwrap(),
            "--name",
            "Orchard",
            "--dry-run",
        ],
    );
    assert_eq!(status, 0, "{out}{err}");
}

#[test]
fn a_request_that_could_not_be_a_wiki_is_refused_before_the_bundle_is_read() {
    let base = dir("request");
    let run = Run::new(&base.0);
    let bundle = good("request-bundle");
    let path = bundle.0.to_str().unwrap();
    for (extra, said) in [
        (vec!["--name", ""], "not a usable name"),
        (vec!["--name", "G", "--lang", "not a tag"], "BCP 47"),
        (
            vec!["--name", "G", "--visibility", "secret"],
            "not a visibility",
        ),
    ] {
        let mut argv = vec!["wiki", "import-bundle", "garden", path, "--dry-run"];
        argv.extend(extra.iter().copied());
        let (status, out, err) = run.cli(&[], &argv);
        assert_eq!(status, 1, "{extra:?}: {out}{err}");
        assert!(err.contains(said), "{extra:?}: {err}");
        assert!(out.is_empty());
    }
    let (status, _, err) = run.cli(
        &[],
        &[
            "wiki",
            "import-bundle",
            "Not A Slug!",
            path,
            "--name",
            "G",
            "--dry-run",
        ],
    );
    assert_eq!(status, 1);
    assert!(err.contains("not a usable slug"), "{err}");
    // A bundle that is not there, or is a file.
    let (status, _, err) = run.cli(
        &[],
        &[
            "wiki",
            "import-bundle",
            "garden",
            "/nonexistent-bundle-for-the-test",
            "--name",
            "G",
        ],
    );
    assert_eq!(status, 1);
    assert!(err.contains("the folder cannot be imported"), "{err}");
}

#[test]
fn a_bundle_that_says_it_is_not_public_does_not_become_a_public_wiki_unless_the_admin_says_so() {
    let base = dir("visibility");
    let run = Run::new(&base.0);
    let bundle = good("visibility-bundle");
    replace_manifest(
        &bundle.0,
        "license:",
        "visibility_default: restricted\nlicense:",
    );
    let path = bundle.0.to_str().unwrap();
    let argv = [
        "wiki",
        "import-bundle",
        "garden",
        path,
        "--name",
        "G",
        "--dry-run",
    ];
    let (status, out, err) = run.cli(&[], &argv);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.contains("visibility restricted (the bundle says it is not public; --visibility public makes it public)"), "{out}");
    assert!(
        out.contains("visibility:      restricted (the bundle's claim"),
        "{out}"
    );
    let mut public = argv.to_vec();
    public.extend(["--visibility", "public"]);
    let (status, out, err) = run.cli(&[], &public);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.contains("visibility public,"), "{out}");
}

#[test]
fn the_two_caps_are_settings_that_a_file_sets_and_a_bad_value_names() {
    let base = dir("settings");
    let run = Run::new(&base.0);
    let bundle = good("settings-bundle");
    let path = bundle.0.to_str().unwrap();
    let argv = [
        "wiki",
        "import-bundle",
        "garden",
        path,
        "--name",
        "G",
        "--dry-run",
    ];
    // The default, with the cap said in the report.
    let (status, out, err) = run.cli(&[], &argv);
    assert_eq!(status, 0, "{out}{err}");
    assert!(out.contains("(the cap is 1000)"), "{out}");
    // The environment, and the file, the environment first.
    let (status, out, _) = run.cli(&[("INKLKUNN_IMPORT_MAX_REVISIONS", "3")], &argv);
    assert_eq!(status, 0, "{out}");
    assert!(out.contains("(the cap is 3)"));
    let conf = base.0.join("import.conf");
    std::fs::write(
        &conf,
        "INKLKUNN_IMPORT_MAX_REVISIONS=2\nINKLKUNN_IMPORT_MAX_HISTORY_MIB=1\n",
    )
    .unwrap();
    let mut with_file = argv.to_vec();
    with_file.extend(["--config", conf.to_str().unwrap()]);
    let (status, out, _) = run.cli(&[], &with_file);
    assert_eq!(status, 1);
    assert!(
        out.contains("would make 3 blocks, and the cap is 2"),
        "{out}"
    );
    let (status, out, _) = run.cli(&[("INKLKUNN_IMPORT_MAX_REVISIONS", "3")], &with_file);
    assert_eq!(status, 0, "the environment wins: {out}");
    // A value that is not a number in range stops the command before the bundle is opened, and names the setting.
    for value in ["many", "0", "-1", "1000001"] {
        let (status, out, err) = run.cli(&[("INKLKUNN_IMPORT_MAX_REVISIONS", value)], &argv);
        assert_eq!(status, 1, "{value}");
        assert!(
            err.contains("INKLKUNN_IMPORT_MAX_REVISIONS") && out.is_empty(),
            "{value}: {err}"
        );
    }
    let (status, _, err) = run.cli(&[("INKLKUNN_IMPORT_MAX_HISTORY_MIB", "0")], &argv);
    assert_eq!(status, 1);
    assert!(err.contains("INKLKUNN_IMPORT_MAX_HISTORY_MIB"), "{err}");
    // Both are settings the configuration file knows.
    assert!(crate::config::KNOWN.contains(&"INKLKUNN_IMPORT_MAX_REVISIONS"));
    assert!(crate::config::KNOWN.contains(&"INKLKUNN_IMPORT_MAX_HISTORY_MIB"));
}

#[test]
fn the_knowledge_lines_say_what_came_in_and_what_was_left_out_and_say_so_when_every_file_was_retired()
 {
    use inklkunn_core::knowledge::Unsettled;
    // Files and nothing left out: the one line. Nothing at all: no line.
    assert_eq!(
        knowledge_lines(3, 2, &Unsettled::default()),
        "  knowledge:       3 claim or source file(s) in 2 change(s), after the history\n"
    );
    assert_eq!(knowledge_lines(0, 0, &Unsettled::default()), "");
    // Files, a status left out and retired claims: the line, then one sentence a line.
    let some = Unsettled {
        statuses: 1,
        retired_claims: 2,
        ..Unsettled::default()
    };
    assert_eq!(
        knowledge_lines(1, 1, &some),
        "  knowledge:       1 claim or source file(s) in 1 change(s), after the history\n                   1 community status was not imported: a status is set here only by a curation proposal.\n                   2 retired claims were not imported: retiring is a curation here.\n"
    );
    // Every file retired: no change was made, and the line says so before the sentence.
    let all = Unsettled {
        retired_claims: 1,
        retired_sources: 1,
        ..Unsettled::default()
    };
    assert_eq!(
        knowledge_lines(0, 0, &all),
        "  knowledge:       no claim or source file came in\n                   1 retired claim and 1 retired source were not imported: retiring is a curation here.\n"
    );
}
