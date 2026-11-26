#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (D): **refusals, each by name, with nothing written** (RFC 007 D-1, D-4, D-5; acceptance criteria 6 and 7). Every bundle is a COPY of a
//! committed fixture, changed in a temporary directory (and sealed again with an independent SHA-256 where the change must not be the
//! checksums' own), and offered to the operator's `wiki import-bundle` beside the running server. Each refusal is asserted by the name
//! the command prints and the rule it gives, and **nothing was written**: the exit status is 1, the store lists the same wikis, no creation
//! mark was made, no directory or key of a half-made wiki exists, the audit trail has no act of an import, and the signer left no seed.

use inklkunn_store::AuditLog;

use super::sha::sha256_hex;
use super::*;

/// What a refused import must leave exactly as it was.
#[derive(Debug, PartialEq, Eq)]
struct Untouched {
    /// The wikis the store lists.
    wikis: Vec<String>,
    /// Every path under the data directory but the store's own files (repositories, keys, scratch directories).
    tree: Vec<String>,
    /// The creation marks.
    marks: Vec<String>,
    /// The audit events that an import's act would have written.
    acts: usize,
}

/// `sha256sums.txt` over every file of the bundle but the manifest and itself, as a converter writes it.
fn seal(root: &Path) {
    let mut lines = Vec::new();
    for (name, bytes) in files_under(root) {
        if name == "wiki-bundle.yaml" || name == "sha256sums.txt" {
            continue;
        }
        lines.push(format!("{}  {name}", sha256_hex(&bytes)));
    }
    std::fs::write(root.join("sha256sums.txt"), lines.join("\n") + "\n").unwrap();
}

/// The file `rel` of the bundle `root`, with `from` replaced by `to` (once; it must be there).
fn replace_in(root: &Path, rel: &str, from: &str, to: &str) {
    let text = std::fs::read_to_string(root.join(rel)).unwrap();
    assert!(text.contains(from), "{rel} has no `{from}`");
    std::fs::write(root.join(rel), text.replacen(from, to, 1)).unwrap();
}

/// The line `n` (from 1) of the JSON Lines file `rel`, changed by `f`, and the file written again.
fn change_line(root: &Path, rel: &str, n: usize, f: impl Fn(&mut serde_json::Value)) {
    let text = std::fs::read_to_string(root.join(rel)).unwrap();
    let lines: Vec<String> = text
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if i + 1 == n {
                let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
                f(&mut v);
                v.to_string()
            } else {
                l.to_owned()
            }
        })
        .collect();
    std::fs::write(root.join(rel), lines.join("\n") + "\n").unwrap();
}

fn set(v: &mut serde_json::Value, key: &str, x: impl Into<serde_json::Value>) {
    v.as_object_mut().unwrap().insert(key.to_owned(), x.into());
}

/// One refusal of the criterion.
struct Case {
    tag: &'static str,
    /// The fixture it is a copy of.
    from: &'static str,
    /// What is done to the copy.
    make: fn(&Path),
    /// Settings the operator's environment gives this run.
    env: &'static [(&'static str, &'static str)],
    /// Options of the command beyond the slug, the directory and the name.
    options: &'static [&'static str],
    /// The name the refusal gives and the rule it names.
    name: &'static str,
    rule: &'static str,
    /// What the story says it is.
    what: &'static str,
}

const MEDIAWIKI: &str = "mediawiki/bundle";
const LICENSED: &[&str] = &["--bundle-license", "CC-BY-SA-4.0"];

fn cases() -> Vec<Case> {
    vec![
        Case {
            tag: "no-licence",
            from: MEDIAWIKI,
            make: |_| {},
            env: &[],
            options: &[],
            name: "wiki-bundle.yaml",
            rule: "declares no `license`",
            what: "a bundle that declares no licence, with no `--bundle-license`",
        },
        Case {
            tag: "checksum-lies",
            from: MEDIAWIKI,
            make: |d| replace_in(d, "pages/Composting.md", "turns kitchen scraps", "turnz kitchen scraps"),
            env: &[],
            options: LICENSED,
            name: "pages/Composting.md",
            rule: "SHA-256 that is not the one the checksums file gives it",
            what: "a checksum that lies (one byte of a page changed in the copy)",
        },
        Case {
            tag: "missing-file",
            from: MEDIAWIKI,
            make: |d| std::fs::remove_file(d.join("pages/Worm bins.md")).unwrap(),
            env: &[],
            options: LICENSED,
            name: "pages/Worm bins.md",
            rule: "missing from the bundle",
            what: "a file the checksums name and the bundle lacks",
        },
        Case {
            tag: "extra-file-read",
            from: MEDIAWIKI,
            make: |d| std::fs::write(d.join("pages/Extra.md"), "---\nid: extra\ntitle: Extra\n---\n\nnot in the checksums\n").unwrap(),
            env: &[],
            options: LICENSED,
            name: "pages/Extra.md",
            rule: "the import reads it, and the checksums file does not name it",
            what: "an extra page the import would read and the checksums do not name",
        },
        Case {
            tag: "extra-history-file",
            from: MEDIAWIKI,
            make: |d| std::fs::write(d.join("history/extra.jsonl"), "").unwrap(),
            env: &[],
            options: LICENSED,
            name: "history/extra.jsonl",
            rule: "the import reads it, and the checksums file does not name it",
            what: "an extra history file",
        },
        Case {
            tag: "symlink",
            from: MEDIAWIKI,
            make: |d| std::os::unix::fs::symlink("/etc/passwd", d.join("pages/Evil.md")).unwrap(),
            env: &[],
            options: LICENSED,
            name: "pages/Evil.md",
            rule: "is a symbolic link",
            what: "a link (a symbolic link in the bundle)",
        },
        Case {
            tag: "escaping-path-in-the-checksums",
            from: MEDIAWIKI,
            make: |d| {
                let sums = std::fs::read_to_string(d.join("sha256sums.txt")).unwrap();
                std::fs::write(
                    d.join("sha256sums.txt"),
                    format!("{sums}{}  ../outside.md\n", "ab".repeat(32)),
                )
                .unwrap();
            },
            env: &[],
            options: LICENSED,
            name: "sha256sums.txt",
            rule: "line ",
            what: "a path that escapes the bundle (`../outside.md` in the checksums file)",
        },
        Case {
            tag: "escaping-checksums-file",
            from: MEDIAWIKI,
            make: |d| replace_in(d, "wiki-bundle.yaml", "checksums: sha256sums.txt", "checksums: ../sums.txt"),
            env: &[],
            options: LICENSED,
            name: "wiki-bundle.yaml",
            rule: "not a path inside the bundle",
            what: "a manifest whose checksums file is outside the bundle (`../sums.txt`)",
        },
        Case {
            tag: "path-over-the-cap",
            from: MEDIAWIKI,
            make: |d| {
                let deep = (0..12)
                    .map(|i| format!("{i}{}", "d".repeat(95)))
                    .collect::<Vec<_>>()
                    .join("/");
                let path = d.join("attachments").join(deep);
                std::fs::create_dir_all(&path).unwrap();
                std::fs::write(path.join("file.bin"), "x").unwrap();
            },
            env: &[],
            options: LICENSED,
            name: "attachments/0ddd",
            rule: "over the 1024-byte cap",
            what: "a path over prikk's cap (more than 1,024 bytes)",
        },
        Case {
            tag: "record-digest",
            from: MEDIAWIKI,
            make: |d| {
                change_line(d, "history/mw:101.jsonl", 2, |v| set(v, "sha256", "a".repeat(64)));
                seal(d);
            },
            env: &[],
            options: LICENSED,
            name: "history/mw:101.jsonl",
            rule: "rev `r1002` has a `sha256` that is not the text's",
            what: "a record whose `sha256` does not match its `content` (the bundle sealed again, so the checksums are not what is wrong)",
        },
        Case {
            tag: "address-in-users",
            from: MEDIAWIKI,
            make: |d| {
                replace_in(d, "users.yaml", "  name: Mira\n", "  name: Mira\n  email: mira@example.org\n");
                seal(d);
            },
            env: &[],
            options: LICENSED,
            name: "users.yaml",
            rule: "entry 1 (id `mira`) has an e-mail address",
            what: "an e-mail address in `users.yaml` (the entry named, the address never repeated)",
        },
        Case {
            tag: "parent-order",
            from: MEDIAWIKI,
            make: |d| {
                change_line(d, "history/mw:101.jsonl", 4, |v| set(v, "parent", "r1001"));
                seal(d);
            },
            env: &[],
            options: LICENSED,
            name: "history/mw:101.jsonl",
            rule: "rev `r1004` has a `parent` that is not the record before it",
            what: "a record whose `parent` is not the record before it in its file",
        },
        Case {
            tag: "time-order",
            from: MEDIAWIKI,
            make: |d| {
                change_line(d, "history/mw:101.jsonl", 2, |v| set(v, "at", "2025-03-01T00:00:00Z"));
                seal(d);
            },
            env: &[],
            options: LICENSED,
            name: "history/mw:101.jsonl",
            rule: "rev `r1002` is earlier than the record before it",
            what: "a record whose `at` is earlier than the record before it (a forged time cannot rearrange a page's own revisions)",
        },
        Case {
            tag: "page-over-the-cap",
            from: MEDIAWIKI,
            make: |d| {
                change_line(d, "history/mw:101.jsonl", 4, |v| {
                    let big = format!("---\ntitle: Composting\n---\n\n{}\n", "x".repeat(3000));
                    set(v, "sha256", sha256_hex(big.as_bytes()));
                    set(v, "content", big);
                });
                seal(d);
            },
            env: &[("INKLKUNN_PAGE_MAX_KIB", "2")],
            options: LICENSED,
            name: "history/mw:101.jsonl",
            rule: "rev `r1004` has a text larger than the 2 KiB cap of a page",
            what: "a page over the cap at an old revision, not only at the last",
        },
        Case {
            tag: "revision-cap",
            from: MEDIAWIKI,
            make: |_| {},
            env: &[("INKLKUNN_IMPORT_MAX_REVISIONS", "3")],
            options: LICENSED,
            name: "history/",
            rule: "and the cap is 3",
            what: "a bundle over the revision cap (set to 3: INKLKUNN_IMPORT_MAX_REVISIONS)",
        },
    ]
}

impl Proof {
    /// `wiki import-bundle` with `extra` set in the operator's environment.
    fn cli_with(&self, extra: &[(&'static str, &str)], args: &[&str]) -> (i32, String, String) {
        let mut env = self.world.env();
        for (k, v) in extra {
            env.insert(k, std::ffi::OsString::from(v));
        }
        let mut argv = vec!["--dev-unconfined".to_owned()];
        argv.extend(args.iter().map(|a| (*a).to_owned()));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = crate::cli::run(&argv, &|k| env.get(k).cloned(), now(), &mut out, &mut err);
        (
            status,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    /// What a refused import must leave as it is.
    fn untouched(&self) -> Untouched {
        fn walk(root: &Path, at: &Path, out: &mut Vec<String>) {
            for entry in std::fs::read_dir(at).unwrap() {
                let path = entry.unwrap().path();
                let name = path.strip_prefix(root).unwrap().display().to_string();
                if name.starts_with("store.sqlite") || name.contains("operator.secret") {
                    continue;
                }
                out.push(name);
                if path.is_dir() {
                    walk(root, &path, out);
                }
            }
        }
        let mut tree = Vec::new();
        let data = &self.world.settings.data_dir;
        walk(data, data, &mut tree);
        tree.sort();
        Untouched {
            wikis: self.listed(),
            tree,
            marks: self
                .layout()
                .creation_marks()
                .unwrap()
                .iter()
                .map(|m| m.as_str().to_owned())
                .collect(),
            acts: self
                .world
                .store
                .read(0, 100_000)
                .unwrap()
                .iter()
                .filter(|r| r.event.action == "wiki-import-bundle")
                .count(),
        }
    }

    /// A copy of the fixture `from` for the case `tag`.
    fn copy_for(&self, tag: &str, from: &str) -> PathBuf {
        let dir = self.world.tmp.0.join("refusals").join(tag);
        copy_tree(&fixture(from), &dir);
        dir
    }

    pub(super) fn refusals_are_named_and_write_nothing(&mut self) {
        self.t.part("(D) refusals, each by name, each with nothing written (RFC 007 D-1, D-4, D-5; criteria 6 and 7)");
        let before = self.untouched();
        let seeds = || std::fs::read_dir(&self.world.settings.seed_dir).unwrap().count();
        self.t.note(&format!(
            "before: {} wikis listed, {} paths under the data directory, {} creation marks, {} import acts in the audit trail",
            before.wikis.len(),
            before.tree.len(),
            before.marks.len(),
            before.acts
        ));
        for case in cases() {
            let dir = self.copy_for(case.tag, case.from);
            (case.make)(&dir);
            let mut args = vec![
                "wiki",
                "import-bundle",
                "refused",
                dir.to_str().unwrap(),
                "--name",
                "Refused",
            ];
            args.extend_from_slice(case.options);
            let (status, out, err) = self.cli_with(case.env, &args);
            self.t.did(&format!("the operator offers {}: `wiki import-bundle refused <the copy>`", case.what));
            self.t.saw(
                status == 1,
                "the command exits with status 1",
                &format!("{status}\n{out}{err}"),
            );
            self.t.saw(
                out.lines()
                    .any(|l| l.trim_start().starts_with(case.name) && l.contains(case.rule)),
                &format!("the plan names `{}` and the rule `{}`", case.name, case.rule),
                &out,
            );
            self.t.saw(
                err.contains("refused, so none of the bundle would be imported; nothing was written"),
                "and the command says that nothing was written",
                &err,
            );
            self.t.saw(
                self.untouched() == before && seeds() == 0,
                "the store lists the same wikis; the data directory has no new path (no repository, key or scratch directory of a half-made wiki); no creation mark; no audit event of an import; no seed of the signer's",
                &format!("{:?}\nvs\n{before:?}", self.untouched()),
            );
            if case.tag == "address-in-users" {
                self.t.saw(
                    !out.contains("mira@example.org") && !err.contains("mira@example.org"),
                    "the address is not repeated in what the command prints",
                    &out,
                );
            }
        }

        // An existing slug.
        let blocks = self.blocks_of("handbook");
        let (status, out, err) = self.import("handbook", "Handbook", &fixture(MEDIAWIKI), LICENSED);
        self.t.did("the operator offers a bundle for the slug of a wiki that exists: `wiki import-bundle handbook <bundle>`");
        self.t.saw(
            status == 1
                && err.contains("a wiki `handbook` exists already; `wiki import-bundle` makes a new wiki and never changes one")
                && err.contains("wiki import-folder handbook <folder>"),
            "it is refused by a sentence that says an existing wiki is never the target and names `import-folder` as the way to add pages",
            &format!("{out}{err}"),
        );
        self.t.saw(
            self.untouched() == before && self.blocks_of("handbook") == blocks,
            "nothing was written, and the existing wiki's history is the history it had",
            &format!("{:?}", self.blocks_of("handbook")),
        );

        // The ways on: `--leave-out-incompatible`, and `--no-history`.
        self.leaving_out_the_incompatible_leaves_out_exactly_those(&before);
        self.no_history_is_the_way_past_the_revision_cap();
    }

    fn leaving_out_the_incompatible_leaves_out_exactly_those(&mut self, before: &Untouched) {
        // An attachment whose sidecar names a licence the wiki cannot take.
        let dir = self.copy_for("licence-sidecar", "sidecars/bundle");
        replace_in(&dir, "attachments/notes.txt.meta.yaml", "license: CC-BY-4.0", "license: CC-BY-NC-4.0");
        seal(&dir);
        let (status, out, err) = self.import("orchard-lite", "Orchard Lite", &dir, &[]);
        self.t.did("the operator offers the hand-written bundle of per-file licences with one sidecar changed to CC-BY-NC-4.0");
        self.t.saw(
            status == 1
                && out.lines().any(|l| l.trim_start().starts_with("attachments/notes.txt") && l.contains("CC-BY-NC-4.0")),
            "it is refused, naming the attachment and the licence",
            &format!("{out}{err}"),
        );
        let given = self.cli(&["wiki", "import-bundle", "orchard-lite", dir.to_str().unwrap(), "--name", "Orchard Lite", "--leave-out-incompatible"]);
        self.t.did("and again with `--leave-out-incompatible`: there is no flag that imports it");
        self.t.saw(
            given.0 == 0
                && given.1.contains("left out for their licence (--leave-out-incompatible):")
                && given.1.contains("attachment attachments/notes.txt: CC-BY-NC-4.0"),
            "the run leaves it out and names it",
            &format!("{}{}", given.1, given.2),
        );
        let export = self.export("orchard-lite", "export-orchard-lite");
        let files: Vec<String> = files_under(&export).into_iter().map(|f| f.0).collect();
        self.t.saw(
            files.contains(&"attachments/plan.svg".to_owned())
                && files.contains(&"attachments/plan.svg.meta.yaml".to_owned())
                && files.contains(&"pages/Orchard Plan.md".to_owned())
                && !files.iter().any(|f| f.contains("notes.txt")),
            "exactly the incompatible attachment and its sidecar are not in the new wiki; the page and the other attachment are",
            &format!("{files:?}"),
        );

        // A page whose own licence the wiki cannot take, with its history.
        let dir = self.copy_for("licence-page", "interwiki/bundle");
        replace_in(&dir, "pages/Seed Library.md", "title: Seed Library\n", "title: Seed Library\nlicense: CC-BY-NC-4.0\n");
        seal(&dir);
        let (status, out, _) = self.import("seeds-lite", "Seeds Lite", &dir, &[]);
        self.t.did("the operator offers the bundle with an interwiki map, one of whose two pages is changed to CC-BY-NC-4.0");
        self.t.saw(
            status == 1
                && out.lines().any(|l| l.trim_start().starts_with("pages/Seed Library.md") && l.contains("CC-BY-NC-4.0")),
            "it is refused, naming the page and the licence",
            &out,
        );
        let given = self.cli(&["wiki", "import-bundle", "seeds-lite", dir.to_str().unwrap(), "--name", "Seeds Lite", "--leave-out-incompatible"]);
        self.t.saw(given.0 == 0, "and `--leave-out-incompatible` imports the other", &format!("{}{}", given.1, given.2));
        let export = self.export("seeds-lite", "export-seeds-lite");
        let files: Vec<String> = files_under(&export).into_iter().map(|f| f.0).collect();
        self.t.saw(
            files.contains(&"pages/Help/Sowing.md".to_owned()) && !files.contains(&"pages/Seed Library.md".to_owned()),
            "exactly that page is left out; `Help/Sowing` is in",
            &format!("{files:?}"),
        );
        let after = self.untouched();
        self.t.saw(
            after.wikis.len() == before.wikis.len() + 2,
            "the two imports with `--leave-out-incompatible` are the only wikis made: every refusal before them made none",
            &format!("{:?}", after.wikis),
        );
    }

    fn no_history_is_the_way_past_the_revision_cap(&mut self) {
        let bundle = fixture(MEDIAWIKI);
        let cap = [("INKLKUNN_IMPORT_MAX_REVISIONS", "3")];
        let (status, out, _) = self.cli_with(&cap, &["wiki", "import-bundle", "garden-flat", bundle.to_str().unwrap(), "--name", "Garden Flat", "--bundle-license", "CC-BY-SA-4.0"]);
        self.t.did("the operator offers the MediaWiki bundle again, with the cap at 3, and then with `--no-history`");
        self.t.saw(
            status == 1 && out.contains("and the cap is 3") && out.contains("`--no-history`"),
            "over the cap it is refused with the count, the cap and the way on (`--no-history`)",
            &out,
        );
        let (status, out, err) = self.cli_with(&cap, &["wiki", "import-bundle", "garden-flat", bundle.to_str().unwrap(), "--name", "Garden Flat", "--bundle-license", "CC-BY-SA-4.0", "--no-history"]);
        self.t.saw(
            status == 0 && out.contains("history: not imported (--no-history)"),
            "with `--no-history` it is imported, and the plan says the history is not",
            &format!("{out}{err}"),
        );
        let blocks = self.blocks_of("garden-flat");
        let changes = self.anonymous.get("/garden-flat/-/changes");
        let said = plain(main_of(&changes));
        self.t.saw(
            blocks.len() == 2 && said.contains("Import 4 page(s) and 0 other file(s) with no history") && !said.contains("Imported. Originally by"),
            "the wiki has two blocks, its creation and one change that holds the pages as they are; no row says it was originally anyone's",
            &said,
        );
        let page = self.anonymous.get("/garden-flat/Composting");
        self.t.saw(
            page.status == 200 && plain(main_of(&page)).contains("Composting turns kitchen scraps into rich soil."),
            "and the pages read as the bundle's files do",
            &plain(main_of(&page)),
        );
    }
}
