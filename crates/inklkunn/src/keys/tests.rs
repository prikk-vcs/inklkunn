#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

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
        let settings = Settings {
            key_dir: data_dir.join("keys"),
            data_dir,
            secret_file,
            seed_dir: shm.0.clone(),
            prikk: OsString::from("prikk"),
            dev_unconfined: true,
        };
        let layout = RootLayout::new(settings.wikis_root());
        let store = SqliteStore::open(&settings.store_path()).unwrap();
        let signer = Signer::open(
            SignerConfig::new(
                "prikk",
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
            &layout,
            &signer,
            &storage,
            NewWiki::new(
                Slug::new("handbook").unwrap(),
                DisplayName::new("Handbook").unwrap(),
            ),
            "test",
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
