//! The driver can never ask prikk to replay a history from its genesis (handoff 003 T3, hold note IS-9;
//! ledger PK-24). `extract` refuses a history it has no checkpoint for unless it is given
//! `--allow-full-replay`, and a request path that could pass it would let a page view cost an unbounded
//! replay of a wiki's whole history. The flag is not in the trait (`ExtractBounds` has no field for it) and
//! this test keeps it out of the driver's sources, so adding it is a decision somebody has to make visibly
//! rather than an argument that slips into an argument vector.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;

/// Lines of `source` that name the flag in code: comments are prose and may explain why it is absent.
fn offending_lines(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            let code = line.split("//").next().unwrap_or("");
            code.contains("allow-full-replay") || code.contains("allow_full_replay")
        })
        .map(|(i, line)| (i + 1, line.trim().to_owned()))
        .collect()
}

fn sources(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("src").flatten() {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_source_of_the_driver_passes_allow_full_replay() {
    let mut files = Vec::new();
    sources(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(
        files.len() >= 8,
        "the driver's sources were found: {files:?}"
    );
    for file in files {
        // The tests of the driver may name the flag to show it is refused; its product code may not.
        if file.components().any(|c| c.as_os_str() == "tests.rs") || file.ends_with("tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&file).expect("source");
        let hits = offending_lines(&text);
        assert!(
            hits.is_empty(),
            "{} would let a request start a full replay: {hits:?}",
            file.display()
        );
    }
}

#[test]
fn the_scan_finds_the_flag_in_code_in_every_spelling_and_ignores_prose() {
    assert_eq!(
        offending_lines("let args = [\"extract\", \"--allow-full-replay\"];").len(),
        1
    );
    assert_eq!(
        offending_lines("args.push(\"--allow-full-replay\");").len(),
        1
    );
    assert_eq!(offending_lines("let allow_full_replay = true;").len(), 1);
    assert!(
        offending_lines(
            "// never passes --allow-full-replay\nlet x = 1; // not --allow-full-replay"
        )
        .is_empty()
    );
}

#[test]
fn the_bounds_a_caller_can_give_extract_have_no_such_field() {
    // Constructing the type with every field it has: if a field for the flag were added, this stops compiling
    // and the person adding it meets this file.
    let bounds = inklkunn_prikk::ExtractBounds {
        max_bytes: Some(1),
        max_files: Some(1),
    };
    assert_eq!((bounds.max_bytes, bounds.max_files), (Some(1), Some(1)));
}
