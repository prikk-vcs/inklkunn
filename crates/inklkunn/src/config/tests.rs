#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;
use std::ffi::OsString;

use super::*;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let map: HashMap<String, OsString> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
        .collect();
    move |k| map.get(k).cloned()
}

#[test]
fn the_required_settings_and_the_defaults() {
    let s = Settings::load(
        env(&[
            ("INKLKUNN_DATA_DIR", "/var/lib/inklkunn"),
            ("INKLKUNN_SECRET_FILE", "/run/secrets/inklkunn"),
        ]),
        false,
    )
    .unwrap();
    assert_eq!(s.key_dir, PathBuf::from("/var/lib/inklkunn/keys"));
    assert_eq!(s.seed_dir, PathBuf::from("/dev/shm/inklkunn"));
    assert_eq!(s.prikk, OsString::from("prikk"));
    assert_eq!(
        s.store_path(),
        PathBuf::from("/var/lib/inklkunn/store.sqlite")
    );
    assert_eq!(s.wikis_root(), PathBuf::from("/var/lib/inklkunn/wikis"));
    assert!(!s.dev_unconfined);
}

#[test]
fn every_setting_can_be_overridden() {
    let s = Settings::load(
        env(&[
            ("INKLKUNN_DATA_DIR", "/data"),
            ("INKLKUNN_SECRET_FILE", "/secret"),
            ("INKLKUNN_KEY_DIR", "/keys"),
            ("INKLKUNN_SEED_DIR", "/run/seeds"),
            ("INKLKUNN_PRIKK", "/opt/prikk/bin/prikk"),
        ]),
        true,
    )
    .unwrap();
    assert_eq!(
        (s.key_dir, s.seed_dir, s.prikk, s.dev_unconfined),
        (
            PathBuf::from("/keys"),
            PathBuf::from("/run/seeds"),
            OsString::from("/opt/prikk/bin/prikk"),
            true
        )
    );
}

#[test]
fn a_refusal_names_the_setting_and_what_is_wanted() {
    let base = [
        ("INKLKUNN_DATA_DIR", "/data"),
        ("INKLKUNN_SECRET_FILE", "/secret"),
    ];
    let with = |extra: (&str, &str)| {
        let mut all = base.to_vec();
        all.push(extra);
        Settings::load(env(&all), false).unwrap_err().0
    };
    let none = Settings::load(env(&[]), false).unwrap_err().0;
    assert!(
        none.contains("INKLKUNN_DATA_DIR") && none.contains("required"),
        "{none}"
    );
    let no_secret = Settings::load(env(&[("INKLKUNN_DATA_DIR", "/data")]), false)
        .unwrap_err()
        .0;
    assert!(no_secret.contains("INKLKUNN_SECRET_FILE"), "{no_secret}");
    let relative = Settings::load(
        env(&[
            ("INKLKUNN_DATA_DIR", "data"),
            ("INKLKUNN_SECRET_FILE", "/secret"),
        ]),
        false,
    )
    .unwrap_err()
    .0;
    assert!(
        relative.contains("INKLKUNN_DATA_DIR") && relative.contains("absolute"),
        "{relative}"
    );
    assert!(with(("INKLKUNN_KEY_DIR", "keys")).contains("INKLKUNN_KEY_DIR"));
    assert!(with(("INKLKUNN_SEED_DIR", "")).contains("INKLKUNN_SEED_DIR"));
    assert!(with(("INKLKUNN_PRIKK", "")).contains("INKLKUNN_PRIKK"));
}

#[test]
fn unconfined_is_a_flag_and_the_sandbox_follows_it() {
    let s = Settings::load(
        env(&[
            ("INKLKUNN_DATA_DIR", "/data"),
            ("INKLKUNN_SECRET_FILE", "/secret"),
        ]),
        true,
    )
    .unwrap();
    assert!(matches!(s.sandbox(), Ok(Sandbox::Unconfined)));
    assert!(UNCONFINED_BANNER.contains("WITHOUT"));
    // The environment cannot turn it on: there is no variable for it.
    let s = Settings::load(
        env(&[
            ("INKLKUNN_DATA_DIR", "/data"),
            ("INKLKUNN_SECRET_FILE", "/secret"),
            ("INKLKUNN_DEV_UNCONFINED", "1"),
        ]),
        false,
    )
    .unwrap();
    assert!(!s.dev_unconfined);
}

#[test]
fn the_web_settings_have_defaults_and_named_refusals() {
    let w = WebSettings::load(&env(&[
        ("INKLKUNN_APP_ORIGIN", "https://wiki.example"),
        ("INKLKUNN_CONTENT_ORIGIN", "https://files.example"),
    ]))
    .unwrap();
    assert_eq!(
        (w.bind.as_str(), w.admission_budget_mib, w.admission_wait_ms),
        ("127.0.0.1:8080", 1024, 2000)
    );
    let e = |extra: &[(&str, &str)]| {
        let mut all = vec![
            ("INKLKUNN_APP_ORIGIN", "https://a.example"),
            ("INKLKUNN_CONTENT_ORIGIN", "https://b.example"),
        ];
        all.extend_from_slice(extra);
        WebSettings::load(&env(&all)).unwrap_err().0
    };
    assert!(
        WebSettings::load(&env(&[]))
            .unwrap_err()
            .0
            .contains("INKLKUNN_APP_ORIGIN")
    );
    assert!(
        WebSettings::load(&env(&[("INKLKUNN_APP_ORIGIN", "https://a.example")]))
            .unwrap_err()
            .0
            .contains("INKLKUNN_CONTENT_ORIGIN")
    );
    assert!(
        e(&[("INKLKUNN_ADMISSION_BUDGET_MIB", "lots")]).contains("INKLKUNN_ADMISSION_BUDGET_MIB")
    );
    assert!(e(&[("INKLKUNN_ADMISSION_BUDGET_MIB", "10")]).contains("256 to"));
    assert!(e(&[("INKLKUNN_ADMISSION_WAIT_MS", "0")]).contains("INKLKUNN_ADMISSION_WAIT_MS"));
}

#[test]
fn a_configuration_file_is_name_equals_value_and_a_typo_is_refused() {
    let file = read_file(
        "# a comment\n\nINKLKUNN_DATA_DIR = /var/lib/inklkunn\nINKLKUNN_BIND=0.0.0.0:80\n",
    )
    .unwrap();
    assert_eq!(
        file.get("INKLKUNN_DATA_DIR"),
        Some(&OsString::from("/var/lib/inklkunn"))
    );
    assert_eq!(file.len(), 2);
    for (bad, why) in [
        (
            "INKLKUNN_BNID=1",
            "line 1: `INKLKUNN_BNID` is not a setting",
        ),
        ("INKLKUNN_BIND", "line 1: expected NAME=value"),
        (
            "INKLKUNN_BIND=a\nINKLKUNN_BIND=b",
            "line 2: `INKLKUNN_BIND` is set twice",
        ),
        ("export INKLKUNN_BIND=a", "is not a setting"),
    ] {
        assert!(read_file(bad).unwrap_err().0.contains(why), "{bad:?}");
    }
}

#[test]
fn the_environment_wins_over_the_file() {
    let file = read_file("INKLKUNN_BIND=0.0.0.0:80\nINKLKUNN_PRIKK=/opt/prikk\n").unwrap();
    let environment = env(&[("INKLKUNN_BIND", "127.0.0.1:9000")]);
    let get = layered(&environment, &file);
    assert_eq!(get("INKLKUNN_BIND"), Some(OsString::from("127.0.0.1:9000")));
    assert_eq!(get("INKLKUNN_PRIKK"), Some(OsString::from("/opt/prikk")));
    assert_eq!(get("INKLKUNN_DATA_DIR"), None);
}
