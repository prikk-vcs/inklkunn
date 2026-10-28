#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;

use super::*;

fn run_with(args: &str, env: &[(&str, &str)]) -> (i32, String, String) {
    let map: HashMap<String, OsString> = env
        .iter()
        .map(|(k, v)| ((*k).to_owned(), OsString::from(v)))
        .collect();
    let args: Vec<String> = args.split_whitespace().map(str::to_owned).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let status = run(
        &args,
        &|k| map.get(k).cloned(),
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

#[test]
fn the_version_names_the_build_and_the_floor() {
    for args in ["", "--version", "-V"] {
        let (status, out, _) = run_with(args, &[]);
        assert_eq!(status, 0, "{args:?}");
        assert!(out.starts_with("inklkunn "), "{out}");
        assert!(out.contains("requires prikk >= 0.54.0"), "{out}");
    }
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let (status, out, err) = run_with("frobnicate now", &[]);
    assert_eq!((status, out.as_str()), (2, ""));
    assert!(
        err.contains("frobnicate now") && err.contains("usage:"),
        "{err}"
    );
    // The dev flag alone is not a command either.
    assert_eq!(run_with("--dev-unconfined", &[]).0, 2);
}

#[test]
fn a_keys_command_line_that_is_not_understood_says_so_before_reading_any_setting() {
    let (status, _, err) = run_with("wiki keys rotate docs", &[]);
    assert_eq!(status, 2);
    assert!(err.contains("--confirm"), "{err}");
}

#[test]
fn missing_settings_are_named_and_nothing_is_touched() {
    let (status, out, err) = run_with("wiki keys status docs", &[]);
    assert_eq!((status, out.as_str()), (1, ""));
    assert!(err.contains("INKLKUNN_DATA_DIR"), "{err}");
    let (status, _, err) = run_with(
        "wiki keys status docs",
        &[
            ("INKLKUNN_DATA_DIR", "/nonexistent/inklkunn"),
            ("INKLKUNN_SECRET_FILE", "/nonexistent/secret"),
        ],
    );
    assert_eq!(status, 1);
    assert!(
        err.contains("no store at /nonexistent/inklkunn/store.sqlite"),
        "{err}"
    );
}
