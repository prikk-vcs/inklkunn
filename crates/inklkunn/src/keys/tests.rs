#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;
use std::os::unix::fs::PermissionsExt;

fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_owned).collect()
}

#[test]
fn the_commands_parse() {
    assert_eq!(
        parse(&args("status docs")).unwrap(),
        Command::Status {
            wiki: "docs".into()
        }
    );
    assert_eq!(
        parse(&args("create docs")).unwrap(),
        Command::Create {
            wiki: "docs".into()
        }
    );
    assert_eq!(
        parse(&args("finish-retirement docs")).unwrap(),
        Command::FinishRetirement {
            wiki: "docs".into()
        }
    );
    assert_eq!(
        parse(&args("abandon-retirement docs --confirm docs")).unwrap(),
        Command::AbandonRetirement {
            wiki: "docs".into(),
            confirm: "docs".into()
        }
    );
    // Abandoning needs the slug typed, as rotating does, and takes no other option.
    for bad in [
        "abandon-retirement docs",
        "abandon-retirement docs --confirm",
        "abandon-retirement docs --confirm docs --retire-old",
        "abandon-retirement --confirm docs",
    ] {
        assert!(parse(&args(bad)).is_err(), "{bad}");
    }
    assert_eq!(
        parse(&args("rotate docs --confirm docs")).unwrap(),
        Command::Rotate {
            wiki: "docs".into(),
            confirm: "docs".into(),
            retire_old: false,
            accept_unverifiable_history: false
        }
    );
    assert_eq!(
        parse(&args(
            "rotate docs --retire-old --confirm docs --accept-unverifiable-history"
        ))
        .unwrap(),
        Command::Rotate {
            wiki: "docs".into(),
            confirm: "docs".into(),
            retire_old: true,
            accept_unverifiable_history: true
        }
    );
}

#[test]
fn what_is_not_understood_is_a_usage_error_that_shows_the_usage() {
    for bad in [
        "",
        "status",
        "status --confirm x",
        "status docs extra",
        "create docs --retire-old",
        "finish-retirement",
        "finish-retirement docs extra",
        "finish-retirement docs --confirm docs",
        "rotate docs",
        "rotate docs --confirm",
        "rotate docs --confirm docs --frobnicate",
        "destroy docs",
    ] {
        let e = parse(&args(bad)).unwrap_err();
        assert!(e.0.contains("usage:"), "{bad:?}: {}", e.0);
    }
    assert!(
        parse(&args("rotate docs"))
            .unwrap_err()
            .0
            .contains("--confirm")
    );
}

#[test]
fn times_are_printed_as_utc_rfc_3339() {
    assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
    assert_eq!(rfc3339(86_399), "1970-01-01T23:59:59Z");
    assert_eq!(rfc3339(951_782_400), "2000-02-29T00:00:00Z", "a leap day");
    assert_eq!(rfc3339(1_782_691_200 + 3_661), "2026-06-29T01:01:01Z");
    assert_eq!(rfc3339(1_793_145_600), "2026-10-28T00:00:00Z");
    assert_eq!(rfc3339(4_102_444_799), "2099-12-31T23:59:59Z");
    assert_eq!(rfc3339(-1), "1969-12-31T23:59:59Z");
}

// ---- the commands against a real prikk, a real key store and a real SQLite store, in this process ----

mod world {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;

    use inklkunn_core::wiki::create_wiki;
    use inklkunn_core::{Signer, SignerConfig};
    use inklkunn_prikk::sandbox::Sandbox;
    use inklkunn_prikk::{Admission, PrikkCli, Storage};
    use inklkunn_store::{
        AuditLog, DisplayName, NewWiki, RootLayout, Slug, SqliteStore, WikiRecord, WikiStore,
    };

    use crate::cli;
    use crate::config::Settings;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn dir(base: &Path, tag: &str) -> Dir {
        let p = base.join(format!("inklkunn-cli-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        Dir(p)
    }

    const SECRET: &str = "an operator secret for the command-line tests, long enough";

    pub struct World {
        _tmp: Dir,
        _shm: Dir,
        pub settings: Settings,
        pub store: SqliteStore,
        pub record: WikiRecord,
        pub storage: PrikkCli,
    }

    /// A deployment with one wiki, `handbook`, created through the product's own path with the production key
    /// derivation (the commands open the key store the same way).
    pub fn world(tag: &str) -> Option<World> {
        build(tag, false)
    }

    /// As [`world`], over a `prikk` that copies the seeds of the first keys when it first sees them (a leak), into
    /// `<data dir>/loot`: what a thief of the first maintainer key holds. It also cannot run `verify` once a key was
    /// removed from trust while the file `<data dir>/ctl/verify-dies` exists.
    pub fn leaky_world(tag: &str) -> Option<World> {
        build(tag, true)
    }

    fn build(tag: &str, leaky: bool) -> Option<World> {
        let on_path = std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()));
        if !on_path {
            println!("skipping: prikk not on PATH");
            return None;
        }
        let tmp = dir(&std::env::temp_dir(), tag);
        let shm = dir(Path::new("/dev/shm"), tag);
        let data_dir = tmp.0.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let secret_file = tmp.0.join("operator.secret");
        std::fs::write(&secret_file, SECRET).unwrap();
        let prikk = if leaky {
            let real = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|d| d.join("prikk"))
                .find(|c| c.is_file())
                .unwrap();
            let loot = data_dir.join("loot");
            let path = data_dir.join("prikk-leaky");
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nif [ \"$1 $2 $3\" = \"trust maintainer remove\" ]; then {real} \"$@\"; rc=$?; touch {ctl}/removed; exit $rc; fi\nif [ \"$1\" = verify ] && [ -e {ctl}/removed ] && [ -e {ctl}/verify-dies ]; then echo 'error: injected' >&2; exit 2; fi\nif [ \"$1\" = verify ] && [ -e {ctl}/forge-act ]; then read A M ARGS < {ctl}/forge-act; rm {ctl}/forge-act; PRIKK_AUTHOR_KEY_ID=$A PRIKK_AUTHOR_SEED_FILE={loot}/author.seed PRIKK_MAINTAINER_KEY_ID=$M PRIKK_MAINTAINER_SEED_FILE={loot}/maintainer.seed {real} $ARGS >&2 || exit 9; fi\nif [ \"$1\" = verify ] && [ -e {ctl}/forge ]; then read A M < {ctl}/forge; rm {ctl}/forge; echo forged > Forged.md; PRIKK_AUTHOR_KEY_ID=$A PRIKK_AUTHOR_SEED_FILE={loot}/author.seed PRIKK_MAINTAINER_KEY_ID=$M PRIKK_MAINTAINER_SEED_FILE={loot}/maintainer.seed {real} commit -m forged >&2 || exit 9; PRIKK_AUTHOR_KEY_ID=$A PRIKK_AUTHOR_SEED_FILE={loot}/author.seed PRIKK_MAINTAINER_KEY_ID=$M PRIKK_MAINTAINER_SEED_FILE={loot}/maintainer.seed {real} seal --allow-no-audit >&2 || exit 9; fi\nif [ -n \"$PRIKK_MAINTAINER_SEED_FILE\" ] && [ -f \"$PRIKK_MAINTAINER_SEED_FILE\" ] && [ ! -e {loot}/maintainer.seed ]; then mkdir -p {loot}; cp \"$PRIKK_MAINTAINER_SEED_FILE\" {loot}/maintainer.seed; fi\nif [ -n \"$PRIKK_AUTHOR_SEED_FILE\" ] && [ -f \"$PRIKK_AUTHOR_SEED_FILE\" ] && [ ! -e {loot}/author.seed ]; then mkdir -p {loot}; cp \"$PRIKK_AUTHOR_SEED_FILE\" {loot}/author.seed; fi\nexec {real} \"$@\"\n",
                    real = real.display(),
                    loot = loot.display(),
                    ctl = data_dir.join("ctl").display()
                ),
            )
            .unwrap();
            std::fs::create_dir_all(data_dir.join("ctl")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path.into_os_string()
        } else {
            OsString::from("prikk")
        };
        let settings = Settings {
            key_dir: data_dir.join("keys"),
            data_dir,
            secret_file,
            seed_dir: shm.0.clone(),
            prikk: prikk.clone(),
            dev_unconfined: true,
        };
        let layout = RootLayout::new(settings.wikis_root());
        let store = SqliteStore::open(&settings.store_path()).unwrap();
        let signer = Signer::open_creating_store(
            SignerConfig::new(
                prikk,
                Sandbox::Unconfined,
                layout.clone(),
                &settings.key_dir,
                &settings.secret_file,
                &settings.seed_dir,
            ),
            Arc::new(super::NoAcceptances),
        )
        .unwrap();
        let storage = PrikkCli::open(
            "prikk",
            Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(200)),
        )
        .unwrap();
        let created = create_wiki(
            &store,
            &store,
            &layout,
            &signer,
            &storage,
            NewWiki::new(
                Slug::new("handbook").unwrap(),
                DisplayName::new("Handbook").unwrap(),
            ),
            "test",
            super::super::key_authority(),
        )
        .expect("a wiki");
        let record = store.get(&created.record.id).unwrap();
        Some(World {
            _tmp: tmp,
            _shm: shm,
            settings,
            store,
            record,
            storage,
        })
    }

    impl World {
        /// Run `inklkunn --dev-unconfined <args>` in this process; (status, stdout, stderr).
        pub fn cli(&self, args: &str) -> (i32, String, String) {
            self.cli_with_prikk(args, &self.settings.prikk.clone())
        }

        /// As [`World::cli`], with `INKLKUNN_PRIKK` naming this binary.
        pub fn cli_with_prikk(&self, args: &str, prikk: &std::ffi::OsStr) -> (i32, String, String) {
            self.run_cli(args, prikk, &self.settings.key_dir)
        }

        /// As [`World::cli`], with `INKLKUNN_KEY_DIR` pointing somewhere else (a mistyped setting).
        pub fn cli_with_key_dir(&self, args: &str, key_dir: &Path) -> (i32, String, String) {
            self.run_cli(args, &self.settings.prikk.clone(), key_dir)
        }

        fn run_cli(
            &self,
            args: &str,
            prikk: &std::ffi::OsStr,
            key_dir: &Path,
        ) -> (i32, String, String) {
            let s = &self.settings;
            let env: HashMap<&str, OsString> = HashMap::from([
                ("INKLKUNN_DATA_DIR", s.data_dir.clone().into_os_string()),
                (
                    "INKLKUNN_SECRET_FILE",
                    s.secret_file.clone().into_os_string(),
                ),
                ("INKLKUNN_KEY_DIR", key_dir.to_path_buf().into_os_string()),
                ("INKLKUNN_SEED_DIR", s.seed_dir.clone().into_os_string()),
                ("INKLKUNN_PRIKK", prikk.to_os_string()),
            ]);
            let args: Vec<String> = format!("--dev-unconfined {args}")
                .split_whitespace()
                .map(str::to_owned)
                .collect();
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let status = cli::run(
                &args,
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

        pub fn verifies(&self) -> bool {
            let repo = RootLayout::new(self.settings.wikis_root()).repo_dir(&self.record.id);
            self.storage.verify(&repo).unwrap().verdict.ok
        }

        pub fn audit_events(&self) -> Vec<(String, String, String)> {
            self.store
                .read(0, 100)
                .unwrap()
                .into_iter()
                .map(|r| {
                    (
                        r.event.action,
                        r.event.outcome.as_str().to_owned(),
                        r.event.detail,
                    )
                })
                .collect()
        }

        /// A `prikk` that fails `trust maintainer remove` and passes everything else to the real one.
        pub fn standin_that_cannot_remove_a_key(&self) -> PathBuf {
            let path = self.settings.data_dir.join("prikk-standin");
            // The unconfined policy clears the environment, so the real prikk is named in full.
            let real = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|d| d.join("prikk"))
                .find(|c| c.is_file())
                .unwrap();
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nif [ \"$1 $2 $3\" = \"trust maintainer remove\" ]; then echo 'error: precondition not met: injected' >&2; exit 1; fi\nexec {} \"$@\"\n",
                    real.display()
                ),
            )
            .unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        /// A `prikk` that cannot run `verify` once a key was removed from trust (the file `removed` in the returned
        /// directory is made by the removal), and passes everything else to the real one.
        pub fn standin_whose_verify_dies_after_a_removal(&self) -> PathBuf {
            let ctl = self.settings.data_dir.join("ctl");
            std::fs::create_dir_all(&ctl).unwrap();
            let path = self.settings.data_dir.join("prikk-dies");
            let real = std::env::split_paths(&std::env::var_os("PATH").unwrap())
                .map(|d| d.join("prikk"))
                .find(|c| c.is_file())
                .unwrap();
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\nif [ \"$1 $2 $3\" = \"trust maintainer remove\" ]; then {real} \"$@\"; rc=$?; touch {ctl}/removed; exit $rc; fi\nif [ \"$1\" = verify ] && [ -e {ctl}/removed ]; then echo 'error: injected' >&2; exit 2; fi\nexec {real} \"$@\"\n",
                    real = real.display(),
                    ctl = ctl.display()
                ),
            )
            .unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        /// A thief with the seeds of the first keys (`leaky_world`) seals a change the next time `prikk verify` is run
        /// in the repository (by whichever command): the stand-in does it, as a process of its own. (Only the driver, the
        /// signer and the fetcher spawn processes in this product, tests of the binary included.)
        pub fn arm_forgery(&self, author_key_id: &str, maintainer_key_id: &str) {
            std::fs::write(
                self.settings.data_dir.join("ctl").join("forge"),
                format!("{author_key_id} {maintainer_key_id}\n"),
            )
            .unwrap();
        }

        /// As [`World::arm_forgery`], for an act that seals no block: the thief runs `prikk <args>` (a branch close, a
        /// tag).
        pub fn arm_forged_act(&self, author_key_id: &str, maintainer_key_id: &str, args: &str) {
            std::fs::write(
                self.settings.data_dir.join("ctl").join("forge-act"),
                format!("{author_key_id} {maintainer_key_id} {args}\n"),
            )
            .unwrap();
        }

        pub fn recorded_ids(&self) -> (Option<String>, Option<String>) {
            let r = self.store.get(&self.record.id).unwrap();
            (r.author_key_id, r.maintainer_key_id)
        }
    }
}

#[test]
fn status_prints_ids_and_dates_and_never_a_secret() {
    let Some(w) = world::world("status") else {
        return;
    };
    let (status, out, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 0, "{err}");
    let id = w.record.id.to_string();
    assert!(out.contains(&format!("inklkunn-{id}-author")), "{out}");
    assert!(out.contains(&format!("inklkunn-{id}-maintainer")), "{out}");
    assert!(out.contains("generation:      1"), "{out}");
    assert!(out.contains("rotated:         never"), "{out}");
    assert!(
        out.contains("created:         20"),
        "an RFC 3339 date: {out}"
    );
    // The same by id; and nothing that looks like a seed or a public key (64 hex digits), nor the secret.
    let (status, by_id, _) = w.cli(&format!("wiki keys status {id}"));
    assert_eq!(status, 0);
    assert_eq!(by_id, out);
    for text in [&out, &err] {
        assert!(!text.contains("operator secret"), "{text}");
        let hex_run = text
            .split(|c: char| !c.is_ascii_hexdigit())
            .map(str::len)
            .max()
            .unwrap_or(0);
        assert!(hex_run < 64, "a seed-sized hex run in the output: {text}");
    }
    // The development flag says so every time.
    assert!(err.contains("WITHOUT its sandbox"), "{err}");
    // An unknown wiki is a clear refusal.
    let (status, _, err) = w.cli("wiki keys status nosuch");
    assert_eq!(status, 1);
    assert!(err.contains("no wiki `nosuch`"), "{err}");
}

#[test]
fn a_rotation_is_confirmed_applied_recorded_audited_and_the_repository_verifies() {
    let Some(w) = world::world("rotate") else {
        return;
    };
    let before = w.recorded_ids();
    assert!(w.verifies());

    // Not this wiki: nothing changes.
    let (status, out, err) = w.cli("wiki keys rotate handbook --confirm other");
    assert_eq!((status, out.as_str()), (1, ""));
    assert!(err.contains("nothing was changed"), "{err}");
    assert_eq!(w.recorded_ids(), before);

    let (status, out, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    assert!(out.contains("generation:      2"), "{out}");
    assert!(out.contains("still trusted"), "{out}");
    let after = w.recorded_ids();
    assert_ne!(after, before, "the wiki record follows the key store");
    assert!(
        after
            .1
            .as_deref()
            .is_some_and(|m| m.contains("-maintainer-2-")),
        "{after:?}"
    );
    let (_, status_out, _) = w.cli("wiki keys status handbook");
    assert!(
        status_out.contains(after.1.as_deref().unwrap()),
        "{status_out}"
    );
    assert!(
        !status_out.contains("rotated:         never"),
        "{status_out}"
    );
    assert!(
        w.verifies(),
        "the old key is kept, so the whole history verifies"
    );

    let events = w.audit_events();
    assert!(
        events.iter().any(|(a, o, d)| a == "keys-rotate"
            && o == "allowed"
            && d.contains("generation 2")
            && d.contains("old key kept")),
        "{events:?}"
    );
    // Written before and after: an act that stopped in the middle shows as begun and never finished.
    let rotate: Vec<_> = events
        .iter()
        .filter(|(a, _, _)| a == "keys-rotate")
        .collect();
    assert_eq!(rotate.len(), 2, "{events:?}");
    assert!(
        rotate
            .first()
            .is_some_and(|(_, _, d)| d.starts_with("begun")),
        "{events:?}"
    );
}

#[test]
fn retiring_the_old_key_needs_both_flags_and_makes_prikk_unable_to_verify_what_it_sealed() {
    let Some(w) = world::world("retire") else {
        return;
    };
    let before = w.recorded_ids();
    // The cost is named, and nothing is changed without the second flag.
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook --retire-old");
    assert_eq!(status, 1);
    assert!(
        err.contains("--accept-unverifiable-history") && err.contains("Nothing was changed"),
        "{err}"
    );
    assert_eq!(w.recorded_ids(), before);

    let (status, out, err) = w.cli(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
    );
    assert_eq!(status, 0, "{err}");
    assert!(out.contains("no longer verifies"), "{out}");
    // PK-34, seen through the product: the wiki's history is now unverifiable. When prikk can keep a retired
    // key's history verifiable this flips, and RFC 002 D-8's "the repository still verifies" holds.
    assert!(
        !w.verifies(),
        "prikk now verifies history sealed by a retired key; see PK-34"
    );
    let events = w.audit_events();
    assert!(
        events
            .iter()
            .any(|(a, o, d)| a == "keys-rotate" && o == "allowed" && d.contains("old key retired")),
        "{events:?}"
    );
}

#[test]
fn create_for_a_wiki_that_has_keys_is_refused_and_a_lost_key_record_is_reported_not_papered_over() {
    let Some(w) = world::world("create") else {
        return;
    };
    let (status, _, err) = w.cli("wiki keys create handbook");
    assert_eq!(status, 1);
    assert!(err.contains("already has keys"), "{err}");
    assert!(
        w.audit_events()
            .iter()
            .any(|(a, o, _)| a == "keys-create" && o == "failed"),
        "a refused act is in the trail too"
    );

    // The key store loses the wiki's record (a restore that missed it). Status says so; `create` cannot
    // quietly make new keys under the old ids, because prikk binds an id to its first key (PK-35).
    let record_file = std::fs::read_dir(&w.settings.key_dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.to_string_lossy().ends_with(".key.json"))
        .expect("the wiki's key record");
    std::fs::remove_file(record_file).unwrap();
    let (status, _, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 1);
    assert!(
        err.contains("no keys") || err.contains("has no keys"),
        "{err}"
    );
    let (status, _, err) = w.cli("wiki keys create handbook");
    assert_eq!(status, 1, "{err}");
    assert!(
        err.contains("already adopted with a different public key"),
        "{err}"
    );
    let (status, _, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 1, "still no keys: {err}");
}

#[test]
fn a_key_directory_with_no_store_in_it_is_refused_by_status_and_rotate_and_nothing_is_made() {
    let Some(w) = world::world("mistyped") else {
        return;
    };
    // A directory that does not exist is the command line's own refusal; one that exists and is empty (made by
    // hand, or the wrong mount) is the signer's: no store there, the path named, nothing created in it.
    let (status, _, err) = w.cli_with_key_dir(
        "wiki keys status handbook",
        &w.settings.data_dir.join("kyes"),
    );
    assert_eq!(status, 1);
    assert!(
        err.contains("no key directory") && err.contains("kyes"),
        "{err}"
    );
    let empty = w.settings.data_dir.join("empty-keys");
    std::fs::create_dir(&empty).unwrap();
    std::fs::set_permissions(&empty, std::fs::Permissions::from_mode(0o700)).unwrap();
    for command in [
        "wiki keys status handbook",
        "wiki keys rotate handbook --confirm handbook",
    ] {
        let (status, _, err) = w.cli_with_key_dir(command, &empty);
        assert_eq!(status, 1, "{command}: {err}");
        assert!(
            err.contains("there is no key store") && err.contains("empty-keys"),
            "{command}: {err}"
        );
        assert_eq!(
            std::fs::read_dir(&empty).unwrap().count(),
            0,
            "{command} made something in the key directory"
        );
    }
}

#[test]
fn status_lists_the_superseded_keys_and_checks_for_seals_made_after_their_rotation() {
    let Some(w) = world::world("superseded-status") else {
        return;
    };
    // Never rotated: nothing to list, and the check has nothing to say.
    let (status, out, _) = w.cli("wiki keys status handbook");
    assert_eq!(status, 0);
    assert!(!out.contains("superseded"), "{out}");
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    let (status, out, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 0, "{err}");
    assert!(
        out.contains("superseded maintainer key inklkunn-")
            && out.contains("generation 1")
            && out.contains("still trusted")
            && out.contains("no block was sealed by a superseded key after its rotation")
            && out.contains("does not help if the host itself was taken over"),
        "{out}"
    );
    // Nothing was found, so nothing was written to the trail.
    assert!(
        !w.audit_events()
            .iter()
            .any(|(a, _, _)| a == "keys-late-seal"),
        "{:?}",
        w.audit_events()
    );
}

#[test]
fn a_rotation_whose_retirement_fails_is_recorded_as_a_rotation_and_says_the_old_key_is_still_trusted()
 {
    let Some(w) = world::world("retire-fails") else {
        return;
    };
    let before = w.recorded_ids();
    let standin = w.standin_that_cannot_remove_a_key();
    let (status, out, err) = w.cli_with_prikk(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
        standin.as_os_str(),
    );
    assert_eq!(status, 1, "the operator must notice: {out}{err}");
    // It names the two commands that end it, and never tells the operator to run prikk by hand (W2).
    assert!(
        err.contains("STILL TRUSTED")
            && err.contains(&format!("wiki keys finish-retirement {}", w.record.id))
            && err.contains(&format!("wiki keys abandon-retirement {}", w.record.id))
            && !err.contains("prikk trust maintainer"),
        "{err}"
    );
    // The keys did rotate, so the wiki record follows them and the output shows them…
    assert_ne!(w.recorded_ids(), before);
    assert!(out.contains("generation:      2"), "{out}");
    // …the trail says what happened rather than "failed"…
    let events = w.audit_events();
    assert!(
        events.iter().any(|(a, o, d)| a == "keys-rotate"
            && o == "allowed"
            && d.contains("retirement FAILED")),
        "{events:?}"
    );
    // …and the old key is still trusted, so the repository still verifies.
    assert!(w.verifies());
}

#[test]
fn a_retirement_that_ends_without_its_record_exits_non_zero_names_the_command_and_finish_retirement_finishes_it()
 {
    let Some(w) = world::world("retire-unrecorded") else {
        return;
    };
    let standin = w.standin_whose_verify_dies_after_a_removal();
    let (status, out, err) = w.cli_with_prikk(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
        standin.as_os_str(),
    );
    // The keys rotated and the old key left the trust store; the record of what that made fail was not made.
    assert_eq!(status, 1, "the operator must notice: {out}{err}");
    let command = format!("wiki keys finish-retirement {}", w.record.id);
    let abandon = format!("wiki keys abandon-retirement {}", w.record.id);
    assert!(
        err.contains("NOT recorded")
            && err.contains(&command)
            && err.contains(&abandon)
            && err.contains("nothing new is sealed"),
        "{err}"
    );
    assert!(out.contains("generation:      2"), "{out}");
    assert!(!w.verifies(), "the key is out of the trust store");
    let events = w.audit_events();
    assert!(
        events.iter().any(|(a, o, d)| a == "keys-rotate"
            && o == "allowed"
            && d.contains("NOT recorded")
            && d.contains("finish-retirement")),
        "{events:?}"
    );
    // `status` says it, with the command, and exits non-zero: writes are paused (Z7), nothing untraceable needed.
    let (status, out, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 1, "{out}{err}");
    assert!(
        out.contains("UNFINISHED") && out.contains(&command) && out.contains(&abandon),
        "{out}"
    );
    assert!(
        err.contains("is not finished") && err.contains("nothing new is sealed"),
        "{err}"
    );
    // Nothing else can be done to the keys meanwhile.
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 1);
    assert!(err.contains(&command), "{err}");
    // It is finished with the command, audited before and after, and only once.
    let (status, out, err) = w.cli("wiki keys finish-retirement handbook");
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains("is finished") && out.contains("3 finding(s)"),
        "{out}"
    );
    let events = w.audit_events();
    let finishing: Vec<_> = events
        .iter()
        .filter(|(a, _, _)| a == "keys-finish-retirement")
        .collect();
    assert_eq!(finishing.len(), 2, "{events:?}");
    assert!(
        finishing
            .first()
            .is_some_and(|(_, _, d)| d.starts_with("begun")),
        "{events:?}"
    );
    assert!(
        finishing.get(1).is_some_and(|(_, o, d)| o == "allowed"
            && d.contains("finished")
            && d.contains("3 finding(s)")),
        "{events:?}"
    );
    let (status, out, _) = w.cli("wiki keys status handbook");
    assert_eq!(status, 0, "{out}");
    assert!(!out.contains("UNFINISHED"), "{out}");
    let (status, _, err) = w.cli("wiki keys finish-retirement handbook");
    assert_eq!(status, 1);
    assert!(err.contains("no key retirement"), "{err}");
    let events = w.audit_events();
    assert!(
        events
            .iter()
            .any(|(a, o, _)| a == "keys-finish-retirement" && o == "failed"),
        "{events:?}"
    );
}

#[test]
fn finish_retirement_removes_a_key_that_is_still_trusted_itself() {
    // W2 at the command line: the removal failed (the stand-in), the key is still trusted. Nothing tells the operator to
    // run prikk; `finish-retirement`, run with a prikk that can remove, does the removal and records the rest.
    let Some(w) = world::world("w2-cli") else {
        return;
    };
    let standin = w.standin_that_cannot_remove_a_key();
    let (status, _, err) = w.cli_with_prikk(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
        standin.as_os_str(),
    );
    assert_eq!(status, 1, "{err}");
    assert!(w.verifies(), "the key is still trusted");
    // With the same broken prikk the command says why and changes nothing.
    let (status, _, err) =
        w.cli_with_prikk("wiki keys finish-retirement handbook", standin.as_os_str());
    assert_eq!(status, 1, "{err}");
    assert!(
        err.contains("removing the key") && !err.contains("prikk trust maintainer"),
        "{err}"
    );
    assert!(w.verifies(), "nothing was changed");
    let (status, out, err) = w.cli("wiki keys finish-retirement handbook");
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains("this command removed the key") && out.contains("3 finding(s)"),
        "{out}"
    );
    assert!(!w.verifies(), "the key is out of the trust store");
    let events = w.audit_events();
    assert!(
        events.iter().any(|(a, o, d)| a == "keys-finish-retirement"
            && o == "allowed"
            && d.contains("removed from the repository's trust by this command")),
        "{events:?}"
    );
}

#[test]
fn abandon_retirement_puts_the_old_key_back_and_says_so_in_one_sentence() {
    // W3 at the command line, both branches: the key still trusted (the removal failed) and the key already removed
    // (the look after it could not be made). Audited before and after; the typed slug is checked first; the state is
    // gone, the wiki verifies, and a second run has nothing to give up.
    for (tag, removed) in [("w3-cli-trusted", false), ("w3-cli-removed", true)] {
        let Some(w) = world::world(tag) else {
            return;
        };
        let standin = if removed {
            w.standin_whose_verify_dies_after_a_removal()
        } else {
            w.standin_that_cannot_remove_a_key()
        };
        let (status, _, _) = w.cli_with_prikk(
            "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
            standin.as_os_str(),
        );
        assert_eq!(status, 1);
        assert_eq!(w.verifies(), !removed);
        // The wrong slug, and no slug, change nothing.
        let (status, _, err) = w.cli("wiki keys abandon-retirement handbook --confirm other");
        assert_eq!(status, 1);
        assert!(
            err.contains("is not the slug") && err.contains("nothing was changed"),
            "{err}"
        );
        let (status, _, err) = w.cli("wiki keys abandon-retirement handbook");
        assert_eq!(status, 2, "{err}");
        assert_eq!(w.verifies(), !removed);
        let (status, out, err) = w.cli("wiki keys abandon-retirement handbook --confirm handbook");
        assert_eq!(status, 0, "{tag}: {out}{err}");
        // The sentence says what abandoning costs and how to retire the key later (X1); it says *again* only when the
        // key was put back.
        assert!(
            out.contains(if removed {
                "is trusted again; nothing is retired; to retire it later run `inklkunn wiki keys retire "
            } else {
                "is trusted; nothing is retired; to retire it later run `inklkunn wiki keys retire "
            }) && out.contains(&format!("wiki keys retire {} --key ", w.record.id))
                && out.contains("--accept-unverifiable-history"),
            "{out}"
        );
        assert!(
            w.verifies(),
            "{tag}: the old key is trusted, so its history verifies"
        );
        let events = w.audit_events();
        let abandoning: Vec<_> = events
            .iter()
            .filter(|(a, _, _)| a == "keys-abandon-retirement")
            .collect();
        assert_eq!(abandoning.len(), 2, "{events:?}");
        let (begun, ended) = (abandoning.first().unwrap(), abandoning.get(1).unwrap());
        assert!(begun.2.starts_with("begun"), "{events:?}");
        assert!(
            ended.1 == "allowed"
                && ended.2.contains("given up")
                && ended.2.contains(if removed {
                    "adopted the key in the repository's trust again"
                } else {
                    "was trusted in the repository when this ran"
                })
                && !ended.2.contains("never left"),
            "{events:?}"
        );
        let (status, out, _) = w.cli("wiki keys status handbook");
        assert_eq!(status, 0);
        assert!(!out.contains("UNFINISHED"), "{out}");
        let (status, _, err) = w.cli("wiki keys abandon-retirement handbook --confirm handbook");
        assert_eq!(status, 1);
        assert!(err.contains("no key retirement"), "{err}");
    }
}

#[test]
fn every_end_of_a_retirement_records_a_fresh_verdict_or_clears_the_stored_explanation() {
    // W6. K1 is retired and recorded: the verdict is recorded *Failed, explained by K1* (by the command itself, through
    // the check the server uses). Then K2's retirement is left unfinished (the check cannot run: the stand-in's `verify`
    // dies once a key was removed): the explanation is cleared, so the verification page no longer says that the only
    // failures are K1's. Finishing K2, or giving it up, records a fresh verdict again (K2 sealed nothing, so what fails is K1's alone).
    for give_up in [false, true] {
        let Some(w) = world::world(&format!("w6-{give_up}")) else {
            return;
        };
        let read = || w.store.get(&w.record.id).unwrap();
        let (status, out, err) = w.cli(
            "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
        );
        assert_eq!(status, 0, "{out}{err}");
        assert!(out.contains("a fresh verification was recorded"), "{out}");
        let k1 = read();
        assert_eq!(
            k1.verification.state,
            inklkunn_store::VerificationState::Failed
        );
        assert_eq!(k1.verification_explained_by.len(), 1, "{k1:?}");
        let first_key = k1.verification_explained_by.first().unwrap().clone();
        // K2: removed, and the look after the removal cannot be made; the check cannot run either.
        let standin = w.standin_whose_verify_dies_after_a_removal();
        let (status, _, err) = w.cli_with_prikk(
            "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
            standin.as_os_str(),
        );
        assert_eq!(status, 1, "{err}");
        assert!(
            err.contains("a fresh verification could not be recorded now")
                && err.contains("explanation"),
            "{err}"
        );
        let k2 = read();
        assert_eq!(
            k2.verification, k1.verification,
            "the verdict stays what it was (it is not replaced by a guess)"
        );
        assert!(
            k2.verification_explained_by.is_empty(),
            "the page may no longer say the only failures are K1's: {k2:?}"
        );
        // The end of K2's retirement records a verdict again.
        let command = if give_up {
            "wiki keys abandon-retirement handbook --confirm handbook"
        } else {
            "wiki keys finish-retirement handbook"
        };
        let (status, out, err) = w.cli(command);
        assert_eq!(status, 0, "{out}{err}");
        assert!(out.contains("a fresh verification was recorded"), "{out}");
        let after = read();
        assert_eq!(
            after.verification.state,
            inklkunn_store::VerificationState::Failed
        );
        // K2 sealed nothing, so what fails is still K1's alone, whichever way K2's retirement ended.
        assert_eq!(after.verification_explained_by, [first_key], "{after:?}");
    }
}

#[test]
fn an_act_that_cannot_be_written_to_the_audit_trail_is_not_begun() {
    use inklkunn_store::{AuditEvent, AuditSink, StoreError};
    struct Broken;
    impl AuditSink for Broken {
        fn record(&self, _: &AuditEvent) -> inklkunn_store::Result<()> {
            Err(StoreError::Backend("the disk is full".into()))
        }
    }
    let Some(w) = world::world("no-trail") else {
        return;
    };
    let before = w.recorded_ids();
    for line in [
        "rotate handbook --confirm handbook",
        "retire handbook --key inklkunn-whatever --confirm handbook --accept-unverifiable-history",
    ] {
        let command = parse(&args(line)).unwrap();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = execute_with(
            &command,
            &w.settings,
            1_800_000_000,
            &mut out,
            &mut err,
            Some(&Broken),
        );
        let err = String::from_utf8(err).unwrap();
        assert_eq!(status, 1, "{line}: {err}");
        assert!(
            err.contains("audit trail could not be written") && err.contains("nothing was done"),
            "{line}: {err}"
        );
    }
    assert_eq!(w.recorded_ids(), before);
    let (_, status_out, _) = w.cli("wiki keys status handbook");
    assert!(
        status_out.contains("generation:      1"),
        "no key changed: {status_out}"
    );
}

// ---- retire by id, and abandon against the audit trail (handoff 004, X1, X2)

#[test]
fn the_retire_command_parses_and_asks_for_the_key_and_the_slug() {
    // The flag is read in any place; without it the command parses and is refused with the cost (Y6).
    for (line, accepted) in [
        ("retire docs --key inklkunn-k --confirm docs", false),
        (
            "retire docs --key inklkunn-k --confirm docs --accept-unverifiable-history",
            true,
        ),
        (
            "retire docs --accept-unverifiable-history --confirm docs --key inklkunn-k",
            true,
        ),
    ] {
        assert_eq!(
            parse(&args(line)).unwrap(),
            Command::Retire {
                wiki: "docs".into(),
                key: "inklkunn-k".into(),
                confirm: "docs".into(),
                accept_unverifiable_history: accepted
            },
            "{line}"
        );
    }
    for bad in [
        "retire docs",
        "retire docs --key inklkunn-k",
        "retire docs --confirm docs",
        "retire docs --key",
        "retire docs --key inklkunn-k --confirm",
        "retire docs --key inklkunn-k --confirm docs --retire-old",
        "retire --key inklkunn-k --confirm docs",
    ] {
        let e = parse(&args(bad)).unwrap_err();
        assert!(e.0.contains("usage:"), "{bad}: {}", e.0);
    }
    assert!(USAGE.contains(
        "wiki keys retire <wiki> --key <maintainer key id> --confirm <slug> --accept-unverifiable-history"
    ));
}

#[test]
fn retire_is_refused_without_the_flag_that_accepts_the_cost_and_nothing_changes_and_works_with_it()
{
    // Y6: `retire` has the cost of `rotate --retire-old` and the same flag.
    let Some(w) = world::world("y6-retire-flag") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    let ids_before = w.recorded_ids();
    let trail_before = w.audit_events().len();
    let (status, out, err) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook"
    ));
    assert_eq!(status, 1, "{out}{err}");
    // The sentence is the one `rotate --retire-old` says, and it says the cost.
    assert!(
        err.contains("--accept-unverifiable-history")
            && err.contains("everything it sealed then fails prikk's verification")
            && err.contains("Verification failed")
            && err.contains("Nothing was changed"),
        "{err}"
    );
    let (_, _, rotate_err) = w.cli("wiki keys rotate handbook --confirm handbook --retire-old");
    assert_eq!(
        err.split_once("from the repository's trust")
            .map(|(_, rest)| rest),
        rotate_err
            .split_once("from the repository's trust")
            .map(|(_, rest)| rest),
        "one sentence about the cost for both commands"
    );
    // Nothing changed: not the keys, not the trust, not the trail (refused before the act is begun).
    assert_eq!(w.recorded_ids(), ids_before);
    assert!(w.verifies(), "the key is still trusted");
    assert_eq!(w.audit_events().len(), trail_before);
    // The wrong slug is still refused first, and with the flag the command works.
    let (status, out, err) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
    ));
    assert_eq!(status, 0, "{out}{err}");
    assert!(!w.verifies(), "the key is out of the trust store");
}

#[test]
fn a_superseded_key_is_retired_at_the_command_line_by_its_id_audited_and_the_refusals_change_nothing()
 {
    let Some(w) = world::world("x1-cli") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, out, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{out}{err}");
    // The rotation said how to retire the kept key later.
    assert!(
        out.contains(&format!("wiki keys retire {} --key {k1}", w.record.id))
            && out.contains("--accept-unverifiable-history"),
        "{out}"
    );
    let k2 = w.recorded_ids().1.unwrap();
    assert_ne!(k1, k2);
    assert!(w.verifies());
    // Refusals first, each changing nothing and audited as failed.
    for (what, line, said) in [
        (
            "the wrong slug",
            format!("wiki keys retire handbook --key {k1} --confirm other --accept-unverifiable-history"),
            "is not the slug",
        ),
        (
            "the current key",
            format!("wiki keys retire handbook --key {k2} --confirm handbook --accept-unverifiable-history"),
            "rotate first",
        ),
        (
            "an id nobody knows",
            "wiki keys retire handbook --key inklkunn-nobody --confirm handbook --accept-unverifiable-history".to_owned(),
            "does not know",
        ),
    ] {
        let (status, _, err) = w.cli(&line);
        assert_eq!(status, 1, "{what}: {err}");
        assert!(
            err.contains(said) && err.contains("nothing was changed"),
            "{what}: {err}"
        );
        assert!(w.verifies(), "{what}: nothing was retired");
    }
    // Retired by its id.
    let (status, out, err) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
    ));
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains(&format!("{k1} was removed from the repository's trust")),
        "{out}"
    );
    // Nothing untraceable, so nothing is said of it (Z1).
    assert!(
        !out.contains("WARNING") && !out.contains(inklkunn_core::UNTRACEABLE_SENTENCE),
        "{out}"
    );
    assert!(!w.verifies(), "the key is out of the trust store");
    // Audited before and after; the end carries the key's id in the fixed form abandon reads back.
    let events = w.audit_events();
    let retiring: Vec<_> = events
        .iter()
        .filter(|(a, _, _)| a == "keys-retire")
        .collect();
    assert_eq!(retiring.len(), 6, "{events:?}");
    assert!(
        retiring.iter().filter(|(_, o, _)| o == "failed").count() == 2,
        "the two refusals that reached the signer: {events:?}"
    );
    let (begun, ended) = (retiring.get(4).unwrap(), retiring.get(5).unwrap());
    assert!(begun.2.starts_with("begun"), "{events:?}");
    assert!(
        ended.1 == "allowed"
            && ended
                .2
                .starts_with(&format!("retirement recorded: key {k1}")),
        "{events:?}"
    );
    // `status` lists it as retired with its findings.
    let (status, out, _) = w.cli("wiki keys status handbook");
    assert_eq!(status, 0, "{out}");
    assert!(
        out.contains(&k1) && out.contains("retired, 3 finding(s) of publication trust recorded"),
        "{out}"
    );
    // A key retired already is refused.
    let (status, _, err) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
    ));
    assert_eq!(status, 1);
    assert!(err.contains("retired already"), "{err}");
    // The verdict was recorded afresh (W6): the failures are the retirement's.
    let record = w.store.get(&w.record.id).unwrap();
    assert!(
        record.verification_explained_by.contains(&k1),
        "{:?}",
        record.verification_explained_by
    );
}

#[test]
fn a_rotation_that_retires_writes_the_replaced_keys_id_first_in_the_audit_detail() {
    let Some(w) = world::world("x2-rotate-detail") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, _, err) = w.cli(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
    );
    assert_eq!(status, 0, "{err}");
    let events = w.audit_events();
    let ended = events
        .iter()
        .rfind(|(a, o, _)| a == "keys-rotate" && o == "allowed")
        .unwrap();
    assert!(
        ended
            .2
            .starts_with(&format!("retirement recorded: key {k1}; generation 2")),
        "{events:?}"
    );
    // Read back the way abandon reads it.
    assert!(
        inklkunn_core::retirement_finished_in(&w.store, &w.record.id, &k1).unwrap(),
        "{events:?}"
    );
    // A rotation that kept the key says nothing of the sort.
    let Some(kept) = world::world("x2-rotate-kept") else {
        return;
    };
    let k1 = kept.recorded_ids().1.unwrap();
    let (status, _, _) = kept.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0);
    assert!(!inklkunn_core::retirement_finished_in(&kept.store, &kept.record.id, &k1).unwrap());
}

/// Copy a directory tree (files and directories, mode kept).
fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    std::fs::set_permissions(to, std::fs::metadata(from).unwrap().permissions()).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn abandon_refuses_when_the_key_record_is_older_than_the_audit_trail_and_changes_nothing() {
    // X2 at the command line. A retirement stops half way (the key is removed, the look after it cannot be made); the
    // key directory is copied aside (a backup taken meanwhile); the retirement is finished (the trail says so); the
    // old copy is put back. The key record then carries the unfinished state of a retirement that finished, and
    // abandoning it would trust a key that was retired on purpose.
    let Some(w) = world::world("x2-cli") else {
        return;
    };
    let standin = w.standin_whose_verify_dies_after_a_removal();
    let (status, _, err) = w.cli_with_prikk(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
        standin.as_os_str(),
    );
    assert_eq!(status, 1, "{err}");
    assert!(!w.verifies());
    let backup = w.settings.data_dir.join("keys-backup");
    copy_tree(&w.settings.key_dir, &backup);
    let (status, out, err) = w.cli("wiki keys finish-retirement handbook");
    assert_eq!(status, 0, "{out}{err}");
    // The backup is put back over the newer key directory.
    std::fs::remove_dir_all(&w.settings.key_dir).unwrap();
    std::fs::rename(&backup, &w.settings.key_dir).unwrap();
    let (status, out, _) = w.cli("wiki keys status handbook");
    assert_eq!(
        status, 1,
        "the record says unfinished, so writes are paused: {out}"
    );
    assert!(out.contains("UNFINISHED"), "the old record: {out}");
    let trusted_before = w.verifies();
    let (status, out, err) = w.cli("wiki keys abandon-retirement handbook --confirm handbook");
    assert_eq!(status, 1, "{out}{err}");
    assert!(
        err.contains("the key record looks older than the audit trail")
            && err.contains("restore the key directory from a later backup")
            && err.contains("nothing was changed"),
        "{err}"
    );
    assert!(
        !out.contains("is trusted"),
        "nothing was said to be trusted: {out}"
    );
    assert_eq!(w.verifies(), trusted_before);
    assert!(!w.verifies(), "the retired key was not trusted again");
    // Still unfinished in that record (nothing was changed), and the trail says the act failed.
    let (_, out, _) = w.cli("wiki keys status handbook");
    assert!(out.contains("UNFINISHED"), "{out}");
    let events = w.audit_events();
    let abandoning: Vec<_> = events
        .iter()
        .filter(|(a, _, _)| a == "keys-abandon-retirement")
        .collect();
    assert_eq!(abandoning.len(), 2, "{events:?}");
    assert_eq!(abandoning.get(1).unwrap().1, "failed");
}

// ---- a retirement records only what it can trace, or nothing (handoff 004, Z1)

/// The id of the late seal `status` shows (`WARNING: block <id> ...`).
fn late_block_in(out: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix("WARNING: block "))
        .and_then(|l| l.split_whitespace().next())
        .expect("the late seal is shown")
        .to_owned()
}

#[test]
fn retire_with_a_late_block_leaves_the_retirement_unfinished_names_it_and_status_and_finish_say_the_same()
 {
    let Some(w) = world::leaky_world("z1-cli") else {
        return;
    };
    let (author, maintainer) = w.recorded_ids();
    let (author, k1) = (author.unwrap(), maintainer.unwrap());
    let (status, out, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{out}{err}");
    // A thief with the first keys' seeds seals a block (when `status` runs `verify`); `status` shows it as a late seal.
    w.arm_forgery(&author, &k1);
    let (status, out, _) = w.cli("wiki keys status handbook");
    assert_eq!(status, 1, "{out}");
    let forged = late_block_in(&out);
    assert!(w.verifies(), "the key is still trusted");

    // The retirement removes the key, records nothing, stays unfinished, and exits 1 with the block named.
    let (status, out, err) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
    ));
    assert_eq!(status, 1, "{out}{err}");
    assert!(
        err.contains(&format!(
            "block {forged} (sealed with the key after it was replaced)"
        )) && err.contains(inklkunn_core::UNTRACEABLE_SENTENCE),
        "{err}"
    );
    assert!(
        !err.contains("wiki recover") && !out.contains("wiki recover"),
        "{out}{err}"
    );
    assert!(!w.verifies(), "the key is out of the trust store");
    assert!(!inklkunn_core::retirement_finished_in(&w.store, &w.record.id, &k1).unwrap());
    // The verdict stored afterwards is a failure nothing explains: the page carries no *retired key* sentence.
    let record = w.store.get(&w.record.id).unwrap();
    assert!(
        record.verification_explained_by.is_empty(),
        "{:?}",
        record.verification_explained_by
    );

    // `status` says the retirement is unfinished, names the block and carries the one sentence, and no command that
    // cannot work. It exits non-zero (Z7): writes are paused, and the late-seal check cannot see a key that is out of
    // trust, so that check alone would have said 0.
    let (status, out, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 1, "{out}{err}");
    assert!(err.contains("is not finished"), "{err}");
    let unfinished = out
        .lines()
        .find(|l| l.starts_with("UNFINISHED"))
        .expect("the line");
    assert!(
        unfinished.contains(&forged)
            && unfinished.contains(inklkunn_core::UNTRACEABLE_SENTENCE)
            && unfinished.contains("abandon-retirement")
            && !unfinished.contains("finish it with"),
        "{unfinished}"
    );
    // ... and does not say that nothing was sealed after the rotation: the check cannot see a key that is out of trust.
    assert!(
        !out.contains("no block was sealed by a superseded key after its rotation")
            && out.contains(&format!("this check cannot see {k1}")),
        "{out}"
    );
    // `finish-retirement` refuses with the same findings and records nothing.
    let (status, out, err) = w.cli("wiki keys finish-retirement handbook");
    assert_eq!(status, 1, "{out}{err}");
    assert!(
        err.contains(&forged)
            && err.contains(inklkunn_core::UNTRACEABLE_SENTENCE)
            && err.contains("nothing was recorded"),
        "{err}"
    );
    assert!(!inklkunn_core::retirement_finished_in(&w.store, &w.record.id, &k1).unwrap());
    // The way out the sentence names: give the retirement up, and the key is trusted again.
    let (status, out, err) = w.cli("wiki keys abandon-retirement handbook --confirm handbook");
    assert_eq!(status, 0, "{out}{err}");
    assert!(w.verifies(), "the key is trusted again");
}

#[test]
fn status_exits_non_zero_while_a_retirement_is_unfinished_after_a_close_or_a_tag_by_the_superseded_key()
 {
    // Z7. A branch closed or a tag made with the superseded key leaves the retirement unfinished (nothing traces it),
    // and the late-seal check cannot see either: `status` still exits non-zero and prints the UNFINISHED line.
    for (tag, act) in [
        ("z7-close", "branch close heads/main"),
        ("z7-tag", "tag create tags/stolen --target heads/main"),
    ] {
        let Some(w) = world::leaky_world(tag) else {
            return;
        };
        let (author, maintainer) = w.recorded_ids();
        let (author, k1) = (author.unwrap(), maintainer.unwrap());
        // A wiki with no retirement unfinished keeps its exits.
        let (status, out, err) = w.cli("wiki keys status handbook");
        assert_eq!(status, 0, "{tag}: {out}{err}");
        let (status, out, err) = w.cli("wiki keys rotate handbook --confirm handbook");
        assert_eq!(status, 0, "{tag}: {out}{err}");
        w.arm_forged_act(&author, &k1, act);
        let (status, out, err) = w.cli("wiki keys status handbook");
        assert_eq!(status, 0, "{tag}: the act is not a late seal: {out}{err}");
        let (status, out, err) = w.cli(&format!(
            "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
        ));
        assert_eq!(status, 1, "{tag}: {out}{err}");
        assert!(
            err.contains(inklkunn_core::UNTRACEABLE_SENTENCE),
            "{tag}: {err}"
        );
        let (status, out, err) = w.cli("wiki keys status handbook");
        assert_eq!(status, 1, "{tag}: {out}{err}");
        assert!(
            out.lines()
                .any(|l| l.starts_with("UNFINISHED")
                    && l.contains(inklkunn_core::UNTRACEABLE_SENTENCE)),
            "{tag}: {out}"
        );
        assert!(err.contains("is not finished"), "{tag}: {err}");
        // Giving the retirement up is the way out, and `status` exits 0 again.
        let (status, out, err) = w.cli("wiki keys abandon-retirement handbook --confirm handbook");
        assert_eq!(status, 0, "{tag}: {out}{err}");
        let (status, out, err) = w.cli("wiki keys status handbook");
        assert_eq!(status, 0, "{tag}: {out}{err}");
    }
}

// ---- status says what the repository trusts, and names the command (handoff 004, Y2, Y3)

/// The status line of the superseded key `key` (and the line under it, if it starts with two spaces).
fn status_of_superseded(out: &str, key: &str) -> String {
    let mut lines = out.lines();
    let first = lines
        .find(|l| l.starts_with(&format!("superseded maintainer key {key} ")))
        .unwrap_or_else(|| panic!("no line for {key}: {out}"));
    let next = lines.next().filter(|l| l.starts_with("  ")).unwrap_or("");
    format!("{first}\n{next}")
}

#[test]
fn status_says_not_retired_and_names_the_retire_command_for_a_superseded_key_that_is_still_trusted()
{
    let Some(w) = world::world("y2-kept") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, out, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{out}{err}");
    let (status, out, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 0, "{out}{err}");
    let shown = status_of_superseded(&out, &k1);
    assert!(
        shown.contains("not retired") && shown.contains("still trusted"),
        "{shown}"
    );
    // The command, with the final spelling of the flags (Y6), beside the key.
    assert!(
        shown.contains(&format!(
            "`inklkunn wiki keys retire {} --key {k1} --confirm <the wiki's slug> --accept-unverifiable-history`",
            w.record.id
        )),
        "{shown}"
    );
    // Once retired: the record says so, the repository no longer trusts the key, and no command is offered.
    let (status, out, err) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
    ));
    assert_eq!(status, 0, "{out}{err}");
    let (_, out, _) = w.cli("wiki keys status handbook");
    let shown = status_of_superseded(&out, &k1);
    assert!(
        shown.contains("retired, 3 finding(s) of publication trust recorded")
            && shown.contains("out of the trust store")
            && !shown.contains("not retired")
            && !shown.contains("still trusted")
            && !shown.contains("to retire it"),
        "{shown}"
    );
}

#[test]
fn status_reads_the_trust_list_so_a_key_an_unfinished_retirement_removed_is_not_said_to_be_trusted()
{
    let Some(w) = world::world("y2-unfinished") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let standin = w.standin_whose_verify_dies_after_a_removal();
    let (status, _, err) = w.cli_with_prikk(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
        standin.as_os_str(),
    );
    assert_eq!(status, 1, "{err}");
    let (_, out, _) = w.cli("wiki keys status handbook");
    assert!(out.contains("UNFINISHED"), "{out}");
    let shown = status_of_superseded(&out, &k1);
    // The record's flag says not retired; the repository's trust list says the key is gone.
    assert!(
        shown.contains("not retired")
            && shown.contains("out of the trust store")
            && !shown.contains("still trusted")
            && !shown.contains("to retire it"),
        "{shown}"
    );
}

#[test]
fn status_that_cannot_read_the_trust_list_says_it_cannot_tell() {
    let Some(w) = world::world("y2-unreadable") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    let real = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join("prikk"))
        .find(|c| c.is_file())
        .unwrap();
    let standin = w.settings.data_dir.join("prikk-no-list");
    std::fs::write(
        &standin,
        format!(
            "#!/bin/sh\nif [ \"$1 $2 $3\" = \"trust maintainer list\" ]; then echo 'error: injected' >&2; exit 2; fi\nexec {} \"$@\"\n",
            real.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&standin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (_, out, _) = w.cli_with_prikk("wiki keys status handbook", standin.as_os_str());
    let shown = status_of_superseded(&out, &k1);
    assert!(
        shown.contains("not retired")
            && shown.contains("cannot be told whether the key is still trusted")
            && !shown.contains(", still trusted")
            && !shown.contains("out of the trust store")
            && !shown.contains("to retire it"),
        "{shown}"
    );
}

#[test]
fn status_names_no_retire_command_for_the_key_whose_retirement_is_unfinished_and_says_when_it_works_for_another()
 {
    let Some(w) = world::world("y3-unfinished") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    let k2 = w.recorded_ids().1.unwrap();
    // The second rotation's retirement stops at the removal: K2 stays trusted, its retirement begun.
    let standin = w.standin_that_cannot_remove_a_key();
    let (status, _, err) = w.cli_with_prikk(
        "wiki keys rotate handbook --confirm handbook --retire-old --accept-unverifiable-history",
        standin.as_os_str(),
    );
    assert_eq!(status, 1, "{err}");
    let (_, out, _) = w.cli("wiki keys status handbook");
    assert!(out.contains("UNFINISHED"), "{out}");
    // K2 is trusted and not retired, and `retire` would refuse it: what ends it is named above, not `retire`.
    let shown = status_of_superseded(&out, &k2);
    assert!(
        shown.contains("not retired")
            && shown.contains("still trusted")
            && !shown.contains("to retire it"),
        "{shown}"
    );
    // K1 is trusted and not retired: its command is named, with the condition that makes it work.
    let shown = status_of_superseded(&out, &k1);
    assert!(
        shown.contains("to retire it")
            && shown.contains(&format!("--key {k1} "))
            && shown.contains("once the unfinished retirement above is finished or given up"),
        "{shown}"
    );
}
// ---- status after a retirement (handoff 004, Z4, Z-b)

#[test]
fn status_does_not_say_that_no_block_was_sealed_after_the_rotation_of_a_key_it_cannot_see() {
    // Z-b. prikk lists who sealed a block only for the keys it trusts; once the key is out of the trust store the
    // check has nothing to look at, and must not report that it found nothing.
    let Some(w) = world::world("z-b") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    let (status, out, _) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
    ));
    assert_eq!(status, 0, "{out}");
    let (status, out, err) = w.cli("wiki keys status handbook");
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        !out.contains("no block was sealed by a superseded key after its rotation"),
        "{out}"
    );
    assert!(
        out.contains(&format!("this check cannot see {k1}"))
            && out.contains("only for the keys it still trusts"),
        "{out}"
    );
    // The same when the check does run (a block signed by a key that is trusted is listed): it looked at nothing that
    // could be K1's, so it still must not say that nothing was found for it.
    let real = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join("prikk"))
        .find(|c| c.is_file())
        .unwrap();
    let standin = w.settings.data_dir.join("prikk-lists-a-signer");
    std::fs::write(
        &standin,
        format!(
            "#!/bin/sh\nif [ \"$1\" = verify ] && [ $# -eq 1 ]; then {real} verify; rc=$?; echo 'sealed-block {block}: a-trusted-key'; exit $rc; fi\nexec {real} \"$@\"\n",
            real = real.display(),
            block = "a".repeat(64)
        ),
    )
    .unwrap();
    std::fs::set_permissions(&standin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (status, out, err) = w.cli_with_prikk("wiki keys status handbook", standin.as_os_str());
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        !out.contains("no block was sealed by a superseded key after its rotation")
            && out.contains(&format!("this check cannot see {k1}")),
        "{out}"
    );
    // With a second superseded key that is still trusted, the check says what it did see, and what it could not.
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    let (status, out, err) = w.cli_with_prikk("wiki keys status handbook", standin.as_os_str());
    assert_eq!(status, 0, "{out}{err}");
    assert!(
        out.contains(
            "no block was sealed after its rotation by a superseded key that is still trusted;"
        ) && out.contains(&format!("this check cannot see {k1}")),
        "{out}"
    );
}

#[test]
fn status_says_when_the_record_calls_a_key_retired_but_the_repository_trusts_it_again() {
    // Z4. The repository is restored on its own from before the retirement: the key record says *retired*, the
    // repository's trust list holds the key. The stand-in records the trust list while the key is trusted and shows
    // that list again once the key has been retired (only the driver, the signer and the fetcher spawn a process).
    let Some(w) = world::world("z4") else {
        return;
    };
    let k1 = w.recorded_ids().1.unwrap();
    let (status, _, err) = w.cli("wiki keys rotate handbook --confirm handbook");
    assert_eq!(status, 0, "{err}");
    let real = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|d| d.join("prikk"))
        .find(|c| c.is_file())
        .unwrap();
    let ctl = w.settings.data_dir.join("trust-list");
    let standin = w.settings.data_dir.join("prikk-old-repository");
    std::fs::write(
        &standin,
        format!(
            "#!/bin/sh\nif [ \"$1 $2 $3\" = \"trust maintainer list\" ]; then if [ -e {ctl}.replay ]; then cat {ctl}; exit 0; fi; if [ -e {ctl} ]; then exec {real} \"$@\"; fi; {real} \"$@\" | tee {ctl}; exit 0; fi\nexec {real} \"$@\"\n",
            real = real.display(),
            ctl = ctl.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&standin, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (status, out, _) = w.cli_with_prikk("wiki keys status handbook", standin.as_os_str());
    assert_eq!(status, 0, "{out}");
    assert!(
        !out.contains("looks older"),
        "a key that is not retired draws no such line: {out}"
    );
    let (status, out, err) = w.cli(&format!(
        "wiki keys retire handbook --key {k1} --confirm handbook --accept-unverifiable-history"
    ));
    assert_eq!(status, 0, "{out}{err}");
    // As the retired wiki is: the key out of the trust store, nothing said about an older repository.
    let (_, out, _) = w.cli_with_prikk("wiki keys status handbook", standin.as_os_str());
    assert!(!w.verifies());
    assert!(!out.contains("looks older"), "{out}");
    // The repository now shows the key trusted again (the replayed list), the record still says retired.
    std::fs::write(format!("{}.replay", ctl.display()), "").unwrap();
    let (status, out, err) = w.cli_with_prikk("wiki keys status handbook", standin.as_os_str());
    assert_eq!(status, 0, "{out}{err}");
    let shown = status_of_superseded(&out, &k1);
    assert!(
        shown.contains("retired, 3 finding(s) of publication trust recorded")
            && shown.contains("still trusted")
            && shown.contains(
                "the repository looks older than the key record: restore the key directory from the same backup, then retire it again"
            ),
        "{shown}"
    );
}
