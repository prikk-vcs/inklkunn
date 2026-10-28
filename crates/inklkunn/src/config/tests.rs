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
