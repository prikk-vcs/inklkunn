#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;

use inklkunn_prikk::Storage;

use super::*;

fn args(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| (*x).to_owned()).collect()
}

#[test]
fn the_commands_parse() {
    assert_eq!(
        parse(&args(&["create", "docs", "--name", "The Docs"])).unwrap(),
        Command::Create {
            slug: "docs".into(),
            name: "The Docs".into(),
            license: None,
            languages: vec![],
            visibility: None
        }
    );
    assert_eq!(
        parse(&args(&[
            "create",
            "docs",
            "--name",
            "D",
            "--license",
            "CC0-1.0",
            "--lang",
            "en",
            "--lang",
            "ja",
            "--visibility",
            "private"
        ]))
        .unwrap(),
        Command::Create {
            slug: "docs".into(),
            name: "D".into(),
            license: Some("CC0-1.0".into()),
            languages: vec!["en".into(), "ja".into()],
            visibility: Some("private".into())
        }
    );
    assert_eq!(
        parse(&args(&[
            "import-folder",
            "docs",
            "/in",
            "--dry-run",
            "--replace-existing"
        ]))
        .unwrap(),
        Command::ImportFolder {
            wiki: "docs".into(),
            dir: "/in".into(),
            dry_run: true,
            replace_existing: true
        }
    );
    assert_eq!(
        parse(&args(&["import-folder", "docs", "/in"])).unwrap(),
        Command::ImportFolder {
            wiki: "docs".into(),
            dir: "/in".into(),
            dry_run: false,
            replace_existing: false
        }
    );
    assert_eq!(
        parse(&args(&[
            "export",
            "docs",
            "--archive",
            "/a.prikk",
            "--bundle",
            "/b"
        ]))
        .unwrap(),
        Command::Export {
            wiki: "docs".into(),
            bundle: Some("/b".into()),
            archive: Some("/a.prikk".into())
        }
    );
}

#[test]
fn what_is_not_understood_is_a_usage_error_that_shows_the_usage() {
    for bad in [
        vec![],
        vec!["create"],
        vec!["create", "docs"],
        vec!["create", "docs", "--name"],
        vec!["create", "docs", "--name", "D", "--frobnicate"],
        vec!["import-folder", "docs"],
        vec!["import-folder", "docs", "--dry-run"],
        vec!["import-folder", "docs", "/in", "--overwrite"],
        vec!["export", "docs"],
        vec!["export", "docs", "--bundle"],
        vec!["export", "docs", "--zip", "/x"],
        vec!["destroy", "docs"],
    ] {
        let e = parse(&args(&bad)).unwrap_err();
        assert!(e.0.contains("usage:"), "{bad:?}: {}", e.0);
    }
}

/// `import-folder` and the exports are the only ways in and out, and the M0 bootstrap signer is no way at all:
/// the binary never enables the signer's `bootstrap` feature (nor any test knob) in a non-dev dependency. The
/// check is a function so that its failure can be shown.
fn product_features_that_are_test_only(manifest: &str) -> Vec<String> {
    let mut hits = Vec::new();
    let mut table = String::new();
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            table = t.to_owned();
            continue;
        }
        let product = table == "[dependencies]" || table.starts_with("[dependencies.");
        if product
            && ["bootstrap", "test-knobs", "test-authority"]
                .iter()
                .any(|f| t.contains(&format!("\"{f}\"")))
        {
            hits.push(t.to_owned());
        }
    }
    hits
}

#[test]
fn the_binary_never_enables_the_bootstrap_signer_or_a_test_knob() {
    let manifest = include_str!("../../Cargo.toml");
    assert!(
        product_features_that_are_test_only(manifest).is_empty(),
        "the binary's runtime dependencies enable a test-only feature"
    );
    // The check can fail: planted violations in every spelling the manifest allows.
    for plant in [
        "[dependencies]\ninklkunn-signer = { path = \"../inklkunn-signer\", features = [\"bootstrap\"] }\n",
        "[dependencies]\ninklkunn-prikk = { path = \"x\", features = [\"test-knobs\"] }\n",
        "[dependencies.inklkunn-core]\npath = \"x\"\nfeatures = [\"test-authority\"]\n",
    ] {
        assert_eq!(
            product_features_that_are_test_only(plant).len(),
            1,
            "{plant}"
        );
    }
    // A dev-dependency may, as the fetch crate's does for its stand-in curl.
    assert!(
        product_features_that_are_test_only(
            "[dev-dependencies]\ninklkunn-fetch = { path = \"x\", features = [\"test-knobs\"] }\n"
        )
        .is_empty()
    );
    // And the binary does not even depend on the signer crate, which only the core crate may.
    assert!(!manifest.contains("inklkunn-signer"));
}

// ---- the commands against a real prikk, a real key store and a real SQLite store, in this process ----

mod world {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use inklkunn_core::{CreationStep, NoLedger, Signer, SignerConfig, create_wiki_observed};
    use inklkunn_prikk::sandbox::Sandbox;
    use inklkunn_prikk::{Admission, Point, PrikkCli, Storage};
    use inklkunn_store::{
        AuditLog, DisplayName, NewWiki, RootLayout, Slug, SqliteStore, WikiRecord, WikiStore,
    };

    use crate::cli;
    use crate::config::Settings;

    pub struct Dir(pub PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    pub fn dir(base: &Path, tag: &str) -> Dir {
        let p = base.join(format!("inklkunn-adm-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        Dir(p)
    }

    const SECRET: &str = "an operator secret for the admin tests, long enough to be accepted";

    pub fn real_prikk() -> Option<PathBuf> {
        let found = std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("prikk"))
                .find(|c| c.is_file())
        });
        if found.is_none() {
            println!("skipping: prikk not on PATH");
        }
        found
    }

    /// A stand-in `prikk` that runs the real one and, for `key generate --out <path>`, keeps a copy of the seed
    /// the real one wrote, so that a test can look for it everywhere. `fail_verify` makes `verify` fail.
    pub fn spy(dir: &Path, real: &Path, capture: &Path, fail_verify: bool) -> PathBuf {
        let script = format!(
            r#"#!/bin/sh
REAL="{real}"
if [ "$1" = "key" ] && [ "$2" = "generate" ]; then
  "$REAL" "$@"; rc=$?
  cat "$4" >> "{capture}" 2>/dev/null
  exit $rc
fi
if [ "$1" = "verify" ] && [ -e "{fail}" ]; then echo "error: injected failure" >&2; exit 1; fi
if [ "$1" = "verify" ] && [ -e "{report}" ]; then
  echo '{{"schema_version":"verify-report-v1","verdict":{{"ok":false}},"stages":[{{"stage":"objects","status":"failed","message":"integrity error: injected"}}]}}'
  exit 1
fi
exec "$REAL" "$@"
"#,
            real = real.display(),
            capture = capture.display(),
            fail = dir.join("fail-verify").display(),
            report = dir.join("fail-report").display()
        );
        let path = dir.join("prikk-spy");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        if fail_verify {
            std::fs::write(dir.join("fail-verify"), "").unwrap();
        }
        path
    }

    pub struct World {
        pub tmp: Dir,
        _shm: Dir,
        pub settings: Settings,
        pub store: SqliteStore,
        pub storage: PrikkCli,
        pub capture: PathBuf,
    }

    /// A deployment with no wiki yet, whose prikk is the spy.
    pub fn world(tag: &str) -> Option<World> {
        let real = real_prikk()?;
        let tmp = dir(&std::env::temp_dir(), tag);
        let shm = dir(Path::new("/dev/shm"), tag);
        let data_dir = tmp.0.join("data");
        let secret_file = tmp.0.join("operator.secret");
        std::fs::write(&secret_file, SECRET).unwrap();
        let capture = tmp.0.join("seeds-seen");
        let prikk = spy(&tmp.0, &real, &capture, false);
        let settings = Settings {
            key_dir: data_dir.join("keys"),
            data_dir,
            secret_file,
            seed_dir: shm.0.clone(),
            prikk: prikk.clone().into_os_string(),
            dev_unconfined: true,
        };
        // A sibling test may fork while the spy script is still open for writing, and the kernel then answers
        // `ETXTBSY` to our exec (the child never execs this file): ask again, as the driver's own tests do.
        let open = || {
            PrikkCli::open(
                &prikk,
                Sandbox::Unconfined,
                Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
            )
        };
        let mut storage = open();
        for _ in 0..50 {
            let busy = matches!(&storage, Err(inklkunn_prikk::StorageError::Unavailable {
                reason: inklkunn_prikk::Unavailable::Spawn(m), ..
            }) if m.contains("Text file busy"));
            if !busy {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            storage = open();
        }
        let storage = storage.unwrap();
        // The first `wiki create` makes the store; open it afterwards.
        std::fs::create_dir_all(&settings.data_dir).unwrap();
        let store = SqliteStore::open(&settings.store_path()).unwrap();
        Some(World {
            tmp,
            _shm: shm,
            settings,
            store,
            storage,
            capture,
        })
    }

    impl World {
        /// Run `inklkunn --dev-unconfined <args>` in this process; (status, stdout, stderr).
        pub fn cli(&self, args: &[&str]) -> (i32, String, String) {
            let s = &self.settings;
            let env: HashMap<&str, OsString> = HashMap::from([
                ("INKLKUNN_DATA_DIR", s.data_dir.clone().into_os_string()),
                (
                    "INKLKUNN_SECRET_FILE",
                    s.secret_file.clone().into_os_string(),
                ),
                ("INKLKUNN_KEY_DIR", s.key_dir.clone().into_os_string()),
                ("INKLKUNN_SEED_DIR", s.seed_dir.clone().into_os_string()),
                ("INKLKUNN_PRIKK", s.prikk.clone()),
            ]);
            let mut argv = vec!["--dev-unconfined".to_owned()];
            argv.extend(args.iter().map(|a| (*a).to_owned()));
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let status = cli::run(
                &argv,
                &|k| env.get(k).cloned(),
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

        pub fn layout(&self) -> RootLayout {
            RootLayout::new(self.settings.wikis_root())
        }

        pub fn record(&self, slug: &str) -> WikiRecord {
            self.store.by_slug(&Slug::new(slug).unwrap()).unwrap()
        }

        pub fn blocks(&self, slug: &str) -> usize {
            let r = self.record(slug);
            self.storage
                .history(
                    &self.layout().repo_dir(&r.id),
                    &Point::View(r.default_view.clone()),
                    None,
                )
                .unwrap()
                .blocks
                .len()
        }

        pub fn audit(&self) -> Vec<(String, String, String, String)> {
            self.store
                .read(0, 500)
                .unwrap()
                .into_iter()
                .map(|r| {
                    (
                        r.event.principal.clone(),
                        r.event.action.clone(),
                        r.event.outcome.as_str().to_owned(),
                        r.event.resource.clone(),
                    )
                })
                .collect()
        }

        /// The seeds the spy saw prikk write.
        pub fn seeds(&self) -> Vec<String> {
            std::fs::read_to_string(&self.capture)
                .unwrap_or_default()
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()
        }

        /// Leave an unfinished creation, as a kill after `step` would: the same real signer and key store the
        /// commands use, stopped by a panic that runs no cleanup.
        pub fn killed_creation_after(&self, slug: &str, step: CreationStep) {
            let layout = self.layout();
            let signer = Signer::open_creating_store(
                SignerConfig::new(
                    self.settings.prikk.clone(),
                    Sandbox::Unconfined,
                    layout.clone(),
                    &self.settings.key_dir,
                    &self.settings.secret_file,
                    &self.settings.seed_dir,
                ),
                Arc::new(NoLedger),
            )
            .unwrap();
            let hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let r = catch_unwind(AssertUnwindSafe(|| {
                create_wiki_observed(
                    &self.store,
                    &layout,
                    &signer,
                    &self.storage,
                    NewWiki::new(
                        Slug::new(slug).unwrap(),
                        DisplayName::new("Killed").unwrap(),
                    ),
                    "test",
                    crate::keys::key_authority(),
                    &|s| {
                        if s == step {
                            panic!("stopped after {s:?}");
                        }
                    },
                )
            }));
            std::panic::set_hook(hook);
            assert!(r.is_err());
        }
    }

    /// Every file under `root` (not following links), as (path, bytes).
    pub fn all_files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
                let p = e.path();
                let t = e.file_type().unwrap();
                if t.is_dir() {
                    walk(&p, out);
                } else if t.is_file() {
                    out.push((p.clone(), std::fs::read(&p).unwrap_or_default()));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, &mut out);
        out
    }
}

use world::{all_files, dir, world};

fn contains(hay: &[u8], needle: &str) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

const HOME: &str = "---\ntitle: Home\nid: home-1\n---\n\nWelcome. See [[Guide]].\n";
const GUIDE: &str = "---\ntitle: Guide\nid: guide-1\n---\n\nBack to [[Home]].\n";

fn put(root: &std::path::Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

#[test]
fn a_wiki_is_created_filled_exported_and_archived_through_the_commands_and_no_seed_shows_anywhere()
{
    let Some(w) = world("flow") else { return };
    let mut everything = String::new();
    let mut note = |label: &str, status: i32, out: &str, err: &str| {
        everything.push_str(&format!("[{label} -> {status}]\n{out}\n{err}\n"));
    };

    // create
    let (status, out, err) = w.cli(&["wiki", "create", "handbook", "--name", "The Handbook"]);
    note("create", status, &out, &err);
    assert_eq!(status, 0, "{err}");
    assert!(out.contains("created wiki handbook"), "{out}");
    let record = w.record("handbook");
    assert!(record.author_key_id.is_some() && record.maintainer_key_id.is_some());
    assert_eq!(
        record.verification.state,
        inklkunn_store::VerificationState::Verified
    );
    assert_eq!(w.blocks("handbook"), 1);
    // …and the slug is taken the second time, with nothing made.
    let (status, out, err) = w.cli(&["wiki", "create", "handbook", "--name", "Again"]);
    note("create again", status, &out, &err);
    assert_eq!(status, 1);
    assert!(err.contains("is taken"), "{err}");
    assert!(w.layout().creation_marks().unwrap().is_empty());
    assert_eq!(w.store.list().unwrap().len(), 1);
    // a request that cannot be a wiki makes nothing.
    let (status, _, err) = w.cli(&["wiki", "create", "Bad Slug!", "--name", "X"]);
    assert_eq!(status, 1, "{err}");
    let (status, _, err) = w.cli(&[
        "wiki",
        "create",
        "ok",
        "--name",
        "X",
        "--visibility",
        "secret",
    ]);
    assert_eq!(status, 1, "{err}");
    assert_eq!(w.store.list().unwrap().len(), 1);

    // import-folder: a dry run, then the run.
    let folder = dir(&w.tmp.0, "folder");
    put(&folder.0, "pages/Home.md", HOME.as_bytes());
    put(&folder.0, "pages/Guide.md", GUIDE.as_bytes());
    put(
        &folder.0,
        "attachments/logo.bin",
        &(0..=255).collect::<Vec<u8>>(),
    );
    let folder_arg = folder.0.to_str().unwrap();
    let before = w.blocks("handbook");
    let files_before = all_files(w.layout().root()).len();
    let (status, out, err) = w.cli(&["wiki", "import-folder", "handbook", folder_arg, "--dry-run"]);
    note("dry run", status, &out, &err);
    assert_eq!(status, 0, "{err}");
    assert!(
        out.contains("3 file(s) to write (2 page(s), 1 attachment(s)"),
        "{out}"
    );
    assert!(
        out.contains("add     pages/Home.md") && out.contains("dry run: nothing was written"),
        "{out}"
    );
    assert_eq!(w.blocks("handbook"), before, "a dry run wrote a block");
    assert_eq!(
        all_files(w.layout().root()).len(),
        files_before,
        "a dry run wrote a file"
    );
    let (status, out, err) = w.cli(&["wiki", "import-folder", "handbook", folder_arg]);
    note("import", status, &out, &err);
    assert_eq!(status, 0, "{err}");
    assert!(
        out.contains("imported 3 file(s) as one change, block "),
        "{out}"
    );
    assert_eq!(w.blocks("handbook"), before + 1);
    // Importing it again collides with itself and says so; nothing changes.
    let (status, out, err) = w.cli(&["wiki", "import-folder", "handbook", folder_arg]);
    note("import again", status, &out, &err);
    assert_eq!(status, 1);
    assert!(out.contains("already exists in the wiki"), "{out}");
    assert_eq!(w.blocks("handbook"), before + 1);
    // With --replace-existing, an unchanged folder is a named outcome, not a failed commit: exit 0, the report
    // says what was compared, no act is begun, nothing moves.
    let imports_begun = w
        .audit()
        .iter()
        .filter(|(_, a, ..)| a == "wiki-import")
        .count();
    for extra in [
        vec!["--replace-existing"],
        vec!["--replace-existing", "--dry-run"],
    ] {
        let mut argv = vec!["wiki", "import-folder", "handbook", folder_arg];
        argv.extend(extra.iter().copied());
        let (status, out, err) = w.cli(&argv);
        note("import unchanged", status, &out, &err);
        assert_eq!(status, 0, "{extra:?}: {err}");
        assert!(
            out.contains("unchanged (3)")
                && out.contains("nothing changed: the wiki already has all 3 files of the folder exactly as they are"),
            "{extra:?}: {out}"
        );
        assert_eq!(w.blocks("handbook"), before + 1);
    }
    assert_eq!(
        w.audit()
            .iter()
            .filter(|(_, a, ..)| a == "wiki-import")
            .count(),
        imports_begun,
        "an unchanged import began an act"
    );

    // export: a bundle and an archive.
    let out_dir = dir(&w.tmp.0, "exports");
    let bundle = out_dir.0.join("bundle");
    let archive = out_dir.0.join("handbook.prikk");
    let (status, out, err) = w.cli(&[
        "wiki",
        "export",
        "handbook",
        "--bundle",
        bundle.to_str().unwrap(),
        "--archive",
        archive.to_str().unwrap(),
    ]);
    note("export", status, &out, &err);
    assert_eq!(status, 0, "{err}");
    assert!(
        out.contains("exported a bundle of 3 page(s) and 1 attachment(s)"),
        "{out}"
    );
    assert!(
        out.contains("exported prikk's archive of the repository"),
        "{out}"
    );
    assert!(bundle.join("wiki-bundle.yaml").is_file() && bundle.join("users.yaml").is_file());
    assert!(bundle.join("history/home-1.jsonl").is_file());
    let mode = std::fs::metadata(&archive).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
    assert!(std::fs::metadata(&archive).unwrap().len() > 0);
    // Neither destination is overwritten.
    let (status, _, err) = w.cli(&[
        "wiki",
        "export",
        "handbook",
        "--bundle",
        bundle.to_str().unwrap(),
    ]);
    assert_eq!(status, 1, "{err}");
    assert!(err.contains("already exists"), "{err}");
    let (status, _, err) = w.cli(&[
        "wiki",
        "export",
        "handbook",
        "--archive",
        archive.to_str().unwrap(),
    ]);
    assert_eq!(status, 1, "{err}");
    assert!(err.contains("already exists"), "{err}");

    // Every one of those acts is in the trail, by the operator, begun and ended, and the trail has no content.
    let trail = w.audit();
    for action in [
        "wiki-create",
        "wiki-import",
        "wiki-export-bundle",
        "wiki-export-archive",
    ] {
        let mine: Vec<_> = trail.iter().filter(|(_, a, ..)| a == action).collect();
        assert!(mine.len() >= 2, "{action}: {trail:?}");
        assert!(
            mine.iter().all(|(p, ..)| p == "system:operator-cli"),
            "{action}"
        );
        assert!(mine.iter().any(|(_, _, o, _)| o == "allowed"), "{action}");
    }
    // A folder refused at the plan is settled before any act: no import was begun for it.
    assert_eq!(
        trail.iter().filter(|(_, a, ..)| a == "wiki-import").count(),
        2,
        "{trail:?}"
    );
    let (trail_text, _): (String, ()) = {
        let mut buf = Vec::new();
        inklkunn_store::export_jsonl(&w.store, &mut buf).unwrap();
        (String::from_utf8(buf).unwrap(), ())
    };
    assert!(
        !trail_text.contains("Welcome") && !trail_text.contains("logo.bin"),
        "content in the trail"
    );

    // No seed appears in a message, in the bundle, in the archive, in any file of any repository or of the
    // deployment (the sealed key records are ciphertext), or in the trail.
    let seeds = w.seeds();
    assert_eq!(
        seeds.len(),
        2,
        "the spy saw the author and maintainer seeds: {seeds:?}"
    );
    assert!(
        seeds
            .iter()
            .all(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    );
    for seed in &seeds {
        assert!(
            !contains(everything.as_bytes(), seed),
            "a seed is in a command's output"
        );
        assert!(
            !contains(trail_text.as_bytes(), seed),
            "a seed is in the trail"
        );
        for root in [&w.settings.data_dir, &out_dir.0] {
            for (path, bytes) in all_files(root) {
                assert!(!contains(&bytes, seed), "a seed is in {}", path.display());
            }
        }
    }
    // And the seed directory (the tmpfs) holds nothing after the calls.
    assert_eq!(std::fs::read_dir(&w.settings.seed_dir).unwrap().count(), 0);
}

#[test]
fn a_hostile_folder_is_refused_whole_with_every_reason_named_on_a_dry_run_and_on_the_run() {
    let Some(w) = world("hostile") else { return };
    let (status, _, err) = w.cli(&["wiki", "create", "fortress", "--name", "Fortress"]);
    assert_eq!(status, 0, "{err}");
    let f = dir(&w.tmp.0, "hostile");
    let root = &f.0;
    put(root, "pages/Good.md", HOME.as_bytes());
    std::os::unix::fs::symlink("/etc/passwd", root.join("pages/Evil.md")).unwrap();
    std::fs::create_dir_all(root.join("attachments")).unwrap();
    std::os::unix::fs::symlink("/", root.join("attachments/rootdir")).unwrap();
    put(root, "pages/CON.md", b"x");
    put(root, "pages/.hidden.md", b"x");
    put(root, "pages/colon:name.md", b"x");
    put(root, "pages/\u{65e5}\u{672c}.md", b"x");
    put(root, "pages/Latin1.md", b"---\ntitle: Caf\xe9\n---\n");
    put(
        root,
        "pages/BadFront.md",
        b"---\ntitle: &anchor Bad\n---\n\nSECRET-BODY\n",
    );
    put(root, "pages/ZDup.md", HOME.as_bytes());
    let deep = (0..12)
        .map(|i| format!("{i}{}", "d".repeat(95)))
        .collect::<Vec<_>>()
        .join("/");
    put(root, &format!("attachments/{deep}/file.bin"), b"x");
    {
        use std::os::unix::ffi::OsStrExt;
        let bad = std::ffi::OsStr::from_bytes(b"pages/bad\xffname.md");
        std::fs::write(root.join(bad), b"x").unwrap();
    }
    let sock = std::os::unix::net::UnixListener::bind(root.join("attachments/pipe.sock")).unwrap();
    let arg = root.to_str().unwrap();
    let before = w.blocks("fortress");
    for extra in [vec!["--dry-run"], vec![]] {
        let mut argv = vec!["wiki", "import-folder", "fortress", arg];
        argv.extend(extra.iter().copied());
        let (status, out, err) = w.cli(&argv);
        assert_eq!(status, 1, "{extra:?}: {out}{err}");
        for reason in [
            "symbolic link",
            "Windows reserves",
            "does not start with a dot",
            "backslash or colon",
            "ASCII",
            "not UTF-8 text",
            "frontmatter the parser refuses",
            "page id of `pages/Good.md`",
            "not a regular file",
            "over the 1024-byte cap",
            "the name is not UTF-8",
        ] {
            assert!(
                out.contains(reason),
                "{extra:?}: `{reason}` is not named:\n{out}"
            );
        }
        assert!(out.contains("pages/Evil.md") && out.contains("attachments/rootdir"));
        // Content never reaches the report.
        assert!(!out.contains("SECRET-BODY") && !out.contains("root:x:"));
        assert!(err.contains("none of the folder was imported"), "{err}");
        assert_eq!(
            w.blocks("fortress"),
            before,
            "{extra:?}: the hostile folder changed the wiki"
        );
    }
    drop(sock);
    // The wiki is untouched and still verifies.
    let r = w.record("fortress");
    assert!(
        w.storage
            .verify(&w.layout().repo_dir(&r.id))
            .unwrap()
            .verdict
            .ok
    );
    // …and the audit trail shows no import begun for a refusal that was settled before the act.
    assert!(w.audit().iter().all(|(_, a, ..)| a != "wiki-import"));
}

#[test]
fn a_start_removes_a_killed_creation_whole_and_a_create_does_the_same_before_it_begins() {
    let Some(w) = world("sweep") else { return };
    let (status, _, err) = w.cli(&["wiki", "create", "kept", "--name", "Kept"]);
    assert_eq!(status, 0, "{err}");
    // A creation killed after its keys were made, before anything was sealed.
    w.killed_creation_after("lost", inklkunn_core::CreationStep::RepositoryCreated);
    let marks = w.layout().creation_marks().unwrap();
    assert_eq!(marks.len(), 1);
    let lost = marks.first().unwrap().clone();
    assert_eq!(
        w.store.list().unwrap().len(),
        1,
        "the killed creation stored no record"
    );
    let sandbox = inklkunn_prikk::sandbox::Sandbox::Unconfined;
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_000,
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("removed the unfinished creation of wiki")),
        "{notes:?}"
    );
    assert!(w.layout().creation_marks().unwrap().is_empty());
    // Two audit events for the removal, the first before it and the second after it, naming the wiki by its id.
    let swept: Vec<_> = w
        .audit()
        .into_iter()
        .filter(|(_, a, ..)| a == "creation-sweep")
        .collect();
    assert_eq!(swept.len(), 2, "{swept:?}");
    assert!(
        swept.iter().any(|(_, _, _, _)| true)
            && swept.first().map(|e| e.3.as_str()) == Some(&format!("wiki:{lost}")),
        "{swept:?}"
    );
    assert!(
        swept.iter().all(|(p, _, o, r)| p == "system:operator-cli"
            && o == "allowed"
            && *r == format!("wiki:{lost}")),
        "{swept:?}"
    );
    let keys: Vec<_> = std::fs::read_dir(&w.settings.key_dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".key.json"))
        .collect();
    assert_eq!(
        keys.len(),
        1,
        "only the kept wiki's key record is left: {keys:?}"
    );
    assert!(
        keys.first()
            .is_some_and(|k| k.starts_with(w.record("kept").id.as_str()))
    );
    // The kept wiki is untouched and verifies.
    let r = w.record("kept");
    assert!(
        w.storage
            .verify(&w.layout().repo_dir(&r.id))
            .unwrap()
            .verdict
            .ok
    );
    // The same, found by `wiki create` itself.
    w.killed_creation_after("lost2", inklkunn_core::CreationStep::RepositoryCreated);
    let (status, _, err) = w.cli(&["wiki", "create", "second", "--name", "Second"]);
    assert_eq!(status, 0, "{err}");
    assert!(
        err.contains("removed the unfinished creation of wiki"),
        "{err}"
    );
    assert!(w.layout().creation_marks().unwrap().is_empty());
    assert_eq!(
        w.audit()
            .iter()
            .filter(|(_, a, o, _)| a == "creation-sweep" && o == "allowed")
            .count(),
        4,
        "the create's sweep is audited as the start's is: an event before each removal and one after"
    );
    assert_eq!(w.store.list().unwrap().len(), 2);
    // Nothing but the two kept wikis' directories is under the root.
    let dirs = std::fs::read_dir(w.layout().root())
        .unwrap()
        .flat_map(|s| std::fs::read_dir(s.unwrap().path()).unwrap())
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(
        dirs.len(),
        6,
        "two repositories, their scratch directories and their lock files: {dirs:?}"
    );
}

#[test]
fn a_start_removes_nothing_from_a_repository_with_a_sealed_block_or_when_it_made_the_store() {
    let Some(w) = world("sweep-keeps") else {
        return;
    };
    let sandbox = inklkunn_prikk::sandbox::Sandbox::Unconfined;
    // A creation killed after its first change was sealed: a history, which only a person may discard.
    w.killed_creation_after("sealed", inklkunn_core::CreationStep::Sealed);
    let marks = w.layout().creation_marks().unwrap();
    let sealed = marks.first().unwrap().clone();
    let keys = || std::fs::read_dir(&w.settings.key_dir).unwrap().count();
    let (keys_before, repo) = (keys(), w.layout().repo_dir(&sealed));
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_000,
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("could not be removed whole") && n.contains("sealed block")),
        "{notes:?}"
    );
    assert!(
        repo.is_dir(),
        "a repository with a sealed block was removed"
    );
    assert_eq!(keys(), keys_before, "its key record was removed");
    assert_eq!(w.layout().creation_marks().unwrap().len(), 1);
    assert!(
        w.audit().iter().any(|(_, a, o, r)| a == "creation-sweep"
            && o == "failed"
            && *r == format!("wiki:{sealed}")),
        "the refusal is audited"
    );
    assert!(
        w.audit()
            .iter()
            .all(|(_, a, o, _)| a != "creation-sweep" || o != "allowed")
    );
    // A store this start made: nothing is removed even from a repository that holds no block.
    w.killed_creation_after("early", inklkunn_core::CreationStep::RepositoryCreated);
    let early = w
        .layout()
        .creation_marks()
        .unwrap()
        .into_iter()
        .find(|id| *id != sealed)
        .unwrap();
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        true,
        1_800_000_001,
    );
    assert!(
        notes.iter().any(|n| n.contains("made the store")),
        "{notes:?}"
    );
    assert!(w.layout().repo_dir(&early).is_dir());
    assert_eq!(w.layout().creation_marks().unwrap().len(), 2);
    // The next start, with a store that was not made by it, removes the one without a block and only that one.
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_002,
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains(&format!("removed the unfinished creation of wiki {early}"))),
        "{notes:?}"
    );
    assert!(!w.layout().repo_dir(&early).exists());
    assert!(repo.is_dir());
}

#[test]
fn the_start_refreshes_the_checkpoint_where_the_prikk_version_changed_or_a_refresh_was_asked_for() {
    let Some(w) = world("checkpoint") else { return };
    let (status, _, err) = w.cli(&["wiki", "create", "notes", "--name", "Notes"]);
    assert_eq!(status, 0, "{err}");
    let sandbox = inklkunn_prikk::sandbox::Sandbox::Unconfined;
    let version = PrikkVersion::parse(w.storage.version()).unwrap();
    let r = w.record("notes");
    assert_eq!(
        r.checkpoint.refreshed_with, None,
        "a new wiki has had no refresh"
    );
    // First start: refreshed, recorded, audited.
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_001,
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("refreshed prikk's checkpoint record for wiki notes")),
        "{notes:?}"
    );
    assert_eq!(
        w.record("notes").checkpoint.refreshed_with,
        Some(version.clone())
    );
    assert!(
        w.audit()
            .iter()
            .any(|(p, a, o, _)| p == "system:operator-cli"
                && a == "checkpoint-refresh"
                && o == "allowed")
    );
    // The same prikk, nothing asked: nothing to do.
    assert!(
        start_up(
            &w.store,
            &w.settings,
            &sandbox,
            &w.storage,
            false,
            1_800_000_002
        )
        .is_empty()
    );
    // A reader asked for a refresh: done again, and the request is cleared.
    w.store
        .request_checkpoint_refresh(&r.id, 1_800_000_003)
        .unwrap();
    assert!(w.record("notes").checkpoint.refresh_requested_at.is_some());
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_004,
    );
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(w.record("notes").checkpoint.refresh_requested_at.is_none());
    // Another prikk version (as after an upgrade): the record says so, and the start refreshes.
    let old = PrikkVersion::parse("prikk 0.0.1").unwrap();
    w.store.record_checkpoint_refresh(&r.id, &old).unwrap();
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_005,
    );
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(w.record("notes").checkpoint.refreshed_with, Some(version));
}

#[test]
fn a_refresh_that_fails_is_said_recorded_as_failed_and_leaves_the_request_standing() {
    let Some(w) = world("checkpoint-fails") else {
        return;
    };
    let (status, _, err) = w.cli(&["wiki", "create", "notes", "--name", "Notes"]);
    assert_eq!(status, 0, "{err}");
    let r = w.record("notes");
    w.store
        .request_checkpoint_refresh(&r.id, 1_800_000_003)
        .unwrap();
    // prikk's `verify` now fails: the refresh cannot run to a verdict.
    std::fs::write(w.tmp.0.join("fail-verify"), "").unwrap();
    let sandbox = inklkunn_prikk::sandbox::Sandbox::Unconfined;
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_004,
    );
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert!(
        notes
            .first()
            .is_some_and(|n| n.contains("was not refreshed")),
        "{notes:?}"
    );
    let after = w.record("notes");
    assert_eq!(after.checkpoint.refreshed_with, None);
    assert_eq!(after.checkpoint.refresh_requested_at, Some(1_800_000_003));
    assert!(
        w.audit()
            .iter()
            .any(|(_, a, o, _)| a == "checkpoint-refresh" && o == "failed")
    );
    // prikk's `verify` now runs to a complete report that says the repository is damaged: that is a verdict, the
    // checkpoint is not recorded as refreshed, and the request stands.
    std::fs::remove_file(w.tmp.0.join("fail-verify")).unwrap();
    std::fs::write(w.tmp.0.join("fail-report"), "").unwrap();
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_004,
    );
    assert!(
        notes
            .first()
            .is_some_and(|n| n.contains("failed verification")),
        "{notes:?}"
    );
    let after = w.record("notes");
    assert_eq!(after.checkpoint.refreshed_with, None);
    assert_eq!(after.checkpoint.refresh_requested_at, Some(1_800_000_003));
    // Mended, the next start does it.
    std::fs::remove_file(w.tmp.0.join("fail-report")).unwrap();
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_005,
    );
    assert!(
        notes.first().is_some_and(|n| n.contains("refreshed")),
        "{notes:?}"
    );
    assert!(w.record("notes").checkpoint.refresh_requested_at.is_none());
}

#[test]
fn an_unknown_wiki_a_missing_store_and_a_missing_folder_are_refused_in_plain_words() {
    let Some(w) = world("refusals") else { return };
    let (status, _, err) = w.cli(&["wiki", "export", "nowhere", "--bundle", "/tmp/never-made"]);
    assert_eq!(status, 1);
    assert!(err.contains("no wiki `nowhere`"), "{err}");
    let (status, _, err) = w.cli(&["wiki", "create", "notes", "--name", "Notes"]);
    assert_eq!(status, 0, "{err}");
    let (status, _, err) = w.cli(&[
        "wiki",
        "import-folder",
        "notes",
        "/nonexistent-folder-for-the-test",
    ]);
    assert_eq!(status, 1);
    assert!(err.contains("the folder cannot be imported"), "{err}");
    let empty = dir(&w.tmp.0, "empty");
    let (status, _, err) = w.cli(&["wiki", "import-folder", "notes", empty.0.to_str().unwrap()]);
    assert_eq!(status, 1);
    assert!(err.contains("holds no page and no attachment"), "{err}");
    // A usage error is exit 2.
    let (status, _, err) = w.cli(&["wiki", "export", "notes"]);
    assert_eq!(status, 2);
    assert!(err.contains("usage:"), "{err}");
}

fn plant_lock(
    w: &world::World,
    slug: &str,
    relative: &str,
    pid: u32,
    kind: &str,
) -> std::path::PathBuf {
    let repo = w.layout().repo_dir(&w.record(slug).id);
    let path = repo.join(".prikk").join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("pid={pid}\nkind={kind}\nnote=planted\n")).unwrap();
    path
}

fn flag(w: &world::World, slug: &str) {
    use inklkunn_store::ActStore;
    w.store
        .set_recovery_flag(&inklkunn_store::RecoveryFlag {
            wiki: w.record(slug).id,
            reason: inklkunn_store::RecoveryReason::Interrupted,
            act: None,
            since: 1,
        })
        .unwrap();
}

fn flagged(w: &world::World, slug: &str) -> bool {
    use inklkunn_store::ActStore;
    w.store.recovery_flag(&w.record(slug).id).unwrap().is_some()
}

#[test]
fn recover_parses_and_a_usage_error_shows_the_usage() {
    assert_eq!(
        parse(&args(&["recover", "docs"])).unwrap(),
        Command::Recover {
            wiki: "docs".into(),
            plan_only: false
        }
    );
    assert_eq!(
        parse(&args(&["recover", "docs", "--plan"])).unwrap(),
        Command::Recover {
            wiki: "docs".into(),
            plan_only: true
        }
    );
    for bad in [
        vec!["recover"],
        vec!["recover", "--plan"],
        vec!["recover", "docs", "--force"],
        vec!["recover", "docs", "--plan", "--yes"],
    ] {
        let e = parse(&args(&bad)).unwrap_err();
        assert!(
            e.0.contains("usage:") && e.0.contains("wiki recover"),
            "{bad:?}: {}",
            e.0
        );
    }
}

#[test]
fn recover_shows_its_plan_and_changes_nothing_then_runs_it_audited_and_says_nothing_to_recover_after()
 {
    let Some(w) = world("recover") else { return };
    let (status, _, err) = w.cli(&["wiki", "create", "notes", "--name", "Notes"]);
    assert_eq!(status, 0, "{err}");
    // A flagged wiki with a stale lock that prikk thinks is held by a live process (this test's own).
    let lock = plant_lock(
        &w,
        "notes",
        "active/default/active.lock",
        std::process::id(),
        "active",
    );
    flag(&w, "notes");
    let events_before = w.audit().len();

    // --plan: the plan is shown, nothing is done, nothing is audited.
    let (status, out, err) = w.cli(&["wiki", "recover", "notes", "--plan"]);
    assert_eq!(status, 0, "{err}");
    assert!(out.contains("recovery plan for wiki"), "{out}");
    assert!(out.contains("step 1: clear 1 stale lock(s)"), "{out}");
    assert!(out.contains("plan only: nothing was done"), "{out}");
    assert!(lock.exists() && flagged(&w, "notes"));
    assert_eq!(w.audit().len(), events_before, "a plan shown is not an act");

    // The run: exit 0, the plan again and what was done, the lock gone and the flag cleared.
    let (status, out, err) = w.cli(&["wiki", "recover", "notes"]);
    assert_eq!(status, 0, "{out}\n{err}");
    assert!(
        out.contains("done: clear-locks: 1 lock(s) cleared"),
        "{out}"
    );
    assert!(
        out.contains("recovered: the wiki takes writes again"),
        "{out}"
    );
    assert!(!lock.exists() && !flagged(&w, "notes"));
    assert_eq!(w.blocks("notes"), 1, "the history is untouched");
    // Audited: the operator's act before and after, and recovery's own steps.
    let audit = w.audit();
    let mine: Vec<_> = audit
        .iter()
        .filter(|(p, a, ..)| p == "system:operator-cli" && a == "wiki-recover")
        .collect();
    assert_eq!(mine.len(), 2, "{audit:?}");
    assert!(
        audit
            .iter()
            .any(|(p, a, ..)| p == "system:recovery" && a == "recovery-plan")
    );
    assert!(
        audit
            .iter()
            .filter(|(p, a, ..)| p == "system:recovery" && a == "recovery-step")
            .count()
            >= 4
    );

    // Nothing is flagged any more: said so, exit 0, and only the operator's own begin and finish are recorded.
    let before = w.audit().len();
    let (status, out, err) = w.cli(&["wiki", "recover", "notes"]);
    assert_eq!(status, 0, "{err}");
    assert!(out.contains("nothing needed recovering"), "{out}");
    assert_eq!(w.audit().len(), before + 2);
}

#[test]
fn recover_stops_on_what_no_verb_answers_exit_one_and_the_wiki_stays_flagged() {
    let Some(w) = world("recover-stop") else {
        return;
    };
    let (status, _, err) = w.cli(&["wiki", "create", "notes", "--name", "Notes"]);
    assert_eq!(status, 0, "{err}");
    // A lock of a kind an acceptance does not leave.
    let lock = plant_lock(
        &w,
        "notes",
        "trust/policy.lock",
        2_147_483_000,
        "container:trust-policy",
    );
    flag(&w, "notes");
    let (status, out, err) = w.cli(&["wiki", "recover", "notes"]);
    assert_eq!(status, 1, "{out}");
    assert!(out.contains("STOP (unknown-lock)"), "{out}");
    assert!(
        err.contains("recovery stopped and the wiki stays flagged"),
        "{err}"
    );
    assert!(lock.exists() && flagged(&w, "notes"));
    let finished = w
        .audit()
        .into_iter()
        .find(|(p, a, o, _)| p == "system:operator-cli" && a == "wiki-recover" && o == "failed");
    assert!(
        finished.is_some(),
        "the operator's act is recorded as failed"
    );
    // A wiki that does not exist is refused in plain words.
    let (status, _, err) = w.cli(&["wiki", "recover", "nowhere"]);
    assert_eq!(status, 1);
    assert!(err.contains("no wiki `nowhere`"), "{err}");
}

#[test]
fn a_start_recovers_every_wiki_that_needs_it_before_it_serves_and_says_so() {
    let Some(w) = world("recover-start") else {
        return;
    };
    for slug in ["fine", "broken", "stuck"] {
        let (status, _, err) = w.cli(&["wiki", "create", slug, "--name", slug]);
        assert_eq!(status, 0, "{err}");
    }
    let lock = plant_lock(
        &w,
        "broken",
        "active/default/active.lock",
        2_147_483_000,
        "active",
    );
    flag(&w, "broken");
    plant_lock(
        &w,
        "stuck",
        "trust/policy.lock",
        2_147_483_000,
        "container:trust-policy",
    );
    flag(&w, "stuck");
    let sandbox = inklkunn_prikk::sandbox::Sandbox::Unconfined;
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_000,
    );
    assert!(
        notes
            .iter()
            .any(|n| n.contains("wiki broken needed recovery") && n.contains("was recovered")),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n.contains("wiki stuck needs recovery")
            && n.contains("recovery stopped")
            && n.contains("wiki recover stuck --plan")),
        "{notes:?}"
    );
    assert!(
        !notes
            .iter()
            .any(|n| n.contains("wiki fine") && n.contains("recover")),
        "{notes:?}"
    );
    assert!(
        !lock.exists() && !flagged(&w, "broken"),
        "recovered before it serves"
    );
    assert!(
        flagged(&w, "stuck"),
        "a stop is a flag that stays: reads served, writes refused"
    );
    // The start's own recoveries are audited like the command's.
    let audit = w.audit();
    assert_eq!(
        audit
            .iter()
            .filter(|(p, a, ..)| p == "system:operator-cli" && a == "wiki-recover")
            .count(),
        4,
        "{audit:?}"
    );
    // A second start has only the stuck one to say anything about.
    let notes = start_up(
        &w.store,
        &w.settings,
        &sandbox,
        &w.storage,
        false,
        1_800_000_001,
    );
    assert!(notes.iter().all(|n| !n.contains("broken")), "{notes:?}");
    assert_eq!(
        notes
            .iter()
            .filter(|n| n.contains("recovery stopped"))
            .count(),
        1,
        "{notes:?}"
    );
}
