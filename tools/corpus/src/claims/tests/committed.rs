//! The committed spike folder holds no article text and no editor identity (handoff 005 acceptance). When the
//! cache of a real run is on this machine, every line of the cached text (raw and converted) of at least 40
//! characters is searched for in the committed files; none may be found. Without a cache (a CI run) the test says
//! so and passes: it guards the machine that ran the spike.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn committed_folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rfcs/handoffs/005-knowledge-layer/spikes")
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            files_under(&p, out);
        } else {
            out.push(p);
        }
    }
}

#[test]
fn no_line_of_the_cached_text_is_in_the_committed_folder() {
    let cache = crate::claims::cache_root(&crate::Opts {
        values: Default::default(),
        pairs: Vec::new(),
    });
    let Ok(cache) = cache else {
        println!("skipping: no cache location");
        return;
    };
    let mut cached = Vec::new();
    files_under(&cache.join("raw"), &mut cached);
    files_under(&cache.join("converted"), &mut cached);
    if cached.is_empty() {
        println!("skipping: no cached text on this machine");
        return;
    }
    let mut committed_files = Vec::new();
    files_under(&committed_folder(), &mut committed_files);
    assert!(
        !committed_files.is_empty(),
        "the committed folder was not found"
    );
    let committed: String = committed_files
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect::<Vec<_>>()
        .join("\n");
    let mut seen = HashSet::new();
    for file in cached {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text
            .lines()
            .map(str::trim)
            .filter(|l| l.chars().count() >= 40)
        {
            if seen.insert(line.to_owned()) {
                assert!(
                    !committed.contains(line),
                    "a line of {} is in the committed folder",
                    file.display()
                );
            }
        }
    }
}

#[test]
fn the_committed_folder_names_no_editor_and_no_address() {
    let mut files = Vec::new();
    files_under(&committed_folder(), &mut files);
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        for banned in [
            "\"user\"",
            "\"userid\"",
            "\"comment\"",
            "contributor",
            "User:",
            "Special:Contributions",
        ] {
            assert!(!text.contains(banned), "`{banned}` in {}", f.display());
        }
        // An IPv4 address literal.
        let words: Vec<&str> = text
            .split(|c: char| !(c.is_ascii_digit() || c == '.'))
            .filter(|w| w.matches('.').count() == 3)
            .collect();
        assert!(
            !words.iter().any(|w| w.split('.').all(|p| !p.is_empty()
                && p.len() <= 3
                && p.parse::<u16>().is_ok_and(|n| n <= 255))),
            "an address-like value in {}: {words:?}",
            f.display()
        );
    }
}
