#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};

use super::*;

mod corrections;
mod interwiki;
mod knowledge;

// ------------------------------------------------------------------------------------------------ helpers

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/interchange")
        .join(rel)
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        // A test that made a tree read-only gives it back, so that it can go.
        make_writable(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn make_writable(path: &Path) {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if meta.is_dir() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755));
            if let Ok(entries) = std::fs::read_dir(path) {
                for e in entries.flatten() {
                    make_writable(&e.path());
                }
            }
        } else if meta.is_file() {
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644));
        }
    }
}

fn dir(tag: &str) -> Dir {
    let p = std::env::temp_dir().join(format!("inklkunn-bundle-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    Dir(p)
}

fn put(root: &Path, rel: &str, bytes: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
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

/// A copy of a fixture that a test may change.
fn copy_of(tag: &str, fixture_dir: &str) -> Dir {
    let d = dir(tag);
    copy_tree(&fixture(fixture_dir), &d.0);
    d
}

/// Set a member of a JSON object (a test may not index).
fn set(v: &mut serde_json::Value, key: &str, x: impl Into<serde_json::Value>) {
    v.as_object_mut().unwrap().insert(key.to_owned(), x.into());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn files_of(root: &Path) -> Vec<String> {
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
    walk(root, root, &mut out);
    out.sort();
    out
}

/// Write `sha256sums.txt` over every file but the manifest and itself, and return the bundle (the checksums the manifest names).
fn seal(root: &Path) {
    let mut lines = Vec::new();
    for rel in files_of(root) {
        if rel == "wiki-bundle.yaml" || rel == "sha256sums.txt" {
            continue;
        }
        lines.push(format!(
            "{}  {rel}",
            sha_hex(&std::fs::read(root.join(&rel)).unwrap())
        ));
    }
    std::fs::write(root.join("sha256sums.txt"), lines.join("\n") + "\n").unwrap();
}

const HOME: &str = "---\nid: home\ntitle: Home\n---\n\nHello.\n";

fn manifest(root: &Path, lines: &[&str]) {
    let mut text = String::from("format: portable-wiki-bundle\nversion: '0.4'\n");
    for l in lines {
        text.push_str(l);
        text.push('\n');
    }
    std::fs::write(root.join("wiki-bundle.yaml"), text).unwrap();
}

/// A bundle of one page with a licence and checksums, sealed.
fn small(tag: &str) -> Dir {
    let d = dir(tag);
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    seal(&d.0);
    d
}

fn plan_with(root: &Path, options: &BundleOptions) -> BundlePlan {
    plan_bundle(root, &ReadConfig::default(), options).unwrap()
}

fn plan(root: &Path) -> BundlePlan {
    plan_with(root, &BundleOptions::default())
}

/// The plan is refused, and one refusal names `name` with `rule` in it.
#[track_caller]
fn assert_refused(plan: &BundlePlan, name: &str, rule: &str) {
    assert!(!plan.is_importable(), "not refused:\n{}", plan.render());
    assert!(
        plan.plan
            .refused
            .iter()
            .any(|r| r.name == name && r.reason.contains(rule)),
        "no refusal of `{name}` with `{rule}`:\n{}",
        plan.render()
    );
}

#[track_caller]
fn assert_importable(plan: &BundlePlan) {
    assert!(plan.is_importable(), "refused:\n{}", plan.render());
}

/// One history line.
fn record(rev: &str, parent: Option<&str>, at: &str, content: Option<&str>) -> String {
    let mut v = serde_json::json!({
        "rev": rev,
        "parent": parent,
        "type": if parent.is_none() { "create" } else { "edit" },
        "at": at,
        "author": {"id": "ann", "name": "Ann"},
        "summary": format!("change {rev}"),
    });
    if let Some(c) = content {
        set(&mut v, "content", c);
        set(&mut v, "sha256", sha_hex(c.as_bytes()));
    }
    v.to_string()
}

fn text(n: u32) -> String {
    format!("---\nid: home\ntitle: Home\n---\n\nVersion {n}.\n")
}

/// A sealed bundle whose one page `Home` has three revisions.
fn with_history(tag: &str, lines: &[String]) -> Dir {
    let d = dir(tag);
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", text(3).as_bytes());
    put(
        &d.0,
        "history/home.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    seal(&d.0);
    d
}

fn three() -> Vec<String> {
    vec![
        record("r1", None, "2025-01-01T00:00:00Z", Some(&text(1))),
        record("r2", Some("r1"), "2025-01-02T00:00:00Z", Some(&text(2))),
        record("r3", Some("r2"), "2025-01-03T00:00:00Z", Some(&text(3))),
    ]
}

// ------------------------------------------------------------------------------------------------ the small readers

#[test]
fn times_are_instants_and_a_text_that_is_not_a_date_time_is_not_one() {
    use time::{Instant, parse};
    let at = |s: &str| parse(s).unwrap();
    // One moment in three spellings.
    assert_eq!(at("2024-03-02T01:15:00Z"), at("2024-03-02T10:15:00+09:00"));
    assert_eq!(at("2024-03-02T01:15:00Z"), at("2024-03-01T20:15:00-05:00"));
    assert_eq!(at("2024-03-02T01:15:00z"), at("2024-03-02t01:15:00Z"));
    // Fractions order below the second, and leap seconds sort after :59.
    assert!(at("2024-03-02T01:15:00.5Z") > at("2024-03-02T01:15:00Z"));
    assert!(at("2024-03-02T01:15:00.5Z") < at("2024-03-02T01:15:01Z"));
    assert!(at("2016-12-31T23:59:60Z") > at("2016-12-31T23:59:59Z"));
    // The epoch, and a leap day.
    assert_eq!(at("1970-01-01T00:00:00Z"), Instant { secs: 0, nanos: 0 });
    assert_eq!(at("2024-02-29T00:00:00Z").secs, 1_709_164_800);
    for bad in [
        "",
        "2024-03-02",
        "2024-03-02 01:15:00Z",
        "2024-03-02T01:15:00",
        "2023-02-29T00:00:00Z",
        "2024-13-01T00:00:00Z",
        "2024-00-10T00:00:00Z",
        "2024-03-02T24:00:00Z",
        "2024-03-02T01:60:00Z",
        "2024-03-02T01:15:00+9:00",
        "2024-03-02T01:15:00+24:00",
        "2024-3-2T01:15:00Z",
        "24-03-02T01:15:00Z",
        "2024-03-02T01:15:00.Z",
        "2024-03-02T01:15Z",
    ] {
        assert_eq!(parse(bad), None, "{bad}");
    }
}

#[test]
fn a_unified_diff_is_applied_exactly_and_refused_at_the_first_line_that_does_not_fit() {
    let old = "one\ntwo\nthree\nfour\n";
    // Changes in the middle, with context.
    let patch = "--- a\n+++ b\n@@ -2,2 +2,3 @@\n two\n-three\n+3\n+three and a half\n";
    assert_eq!(
        patch::apply(old, patch).unwrap(),
        "one\ntwo\n3\nthree and a half\nfour\n"
    );
    // Two hunks; an insertion at the start and a removal at the end.
    let patch = "@@ -0,0 +1 @@\n+zero\n@@ -4 +4,0 @@\n-four\n";
    assert_eq!(patch::apply(old, patch).unwrap(), "zero\none\ntwo\nthree\n");
    // The last line without a line feed, on both sides.
    assert_eq!(
        patch::apply(
            "a\nb",
            "@@ -2 +2 @@\n-b\n\\ No newline at end of file\n+c\n\\ No newline at end of file\n"
        )
        .unwrap(),
        "a\nc"
    );
    // A line feed appears where there was none.
    assert_eq!(
        patch::apply(
            "a\nb",
            "@@ -2 +2 @@\n-b\n\\ No newline at end of file\n+b\n"
        )
        .unwrap(),
        "a\nb\n"
    );
    // Refusals: a context line that is not the text's, a patch beyond the text, counts that are wrong, a missing hunk, overlap, text
    // after the header, a marker that says what the text does not.
    for bad in [
        "@@ -2,2 +2,2 @@\n two\n-THREE\n+3\n",
        "@@ -9 +9 @@\n-nine\n+9\n",
        "@@ -2,2 +2,2 @@\n two\n-three\n+3\n+extra\n",
        "@@ -2,2 +2,2 @@\n two\n",
        "",
        "--- a\n+++ b\n",
        "@@ -3 +3 @@\n-three\n+3\n@@ -2 +2 @@\n-two\n+2\n",
        "junk\n@@ -1 +1 @@\n-one\n+1\n",
        "@@ -1 +1 @@\n-one\n\\ No newline at end of file\n+1\n",
        "@@ -1 +1 @@\n*one\n+1\n",
    ] {
        assert!(patch::apply(old, bad).is_err(), "{bad:?} was applied");
    }
}

#[test]
fn the_licence_matrix_has_wiki_commons_cells_and_nothing_else_can_be_a_default() {
    use licence::{goes_into, may_be_a_default};
    let (cc0, by, sa) = ("CC0-1.0", "CC-BY-4.0", "CC-BY-SA-4.0");
    // Chapter 15's table, one cell at a time (file → wiki).
    assert!(goes_into(cc0, cc0) && goes_into(cc0, by) && goes_into(cc0, sa));
    assert!(!goes_into(by, cc0) && goes_into(by, by) && goes_into(by, sa));
    assert!(!goes_into(sa, cc0) && !goes_into(sa, by) && goes_into(sa, sa));
    // Versions are one family.
    assert!(goes_into("CC-BY-SA-3.0", sa) && goes_into("CC-BY-2.5", sa));
    // NC, ND, GFDL, the unknown and what only looks like a licence go into none, and none can be a default.
    for bad in [
        "CC-BY-NC-4.0",
        "CC-BY-NC-SA-4.0",
        "CC-BY-ND-4.0",
        "GFDL-1.3-or-later",
        "MIT",
        "cc0-1.0",
        "CC-BY-SA-4.0 OR MIT",
        "CC-BY-SA-9.9",
        "",
    ] {
        assert!(!may_be_a_default(bad), "{bad}");
        for wiki in [cc0, by, sa] {
            assert!(!goes_into(bad, wiki), "{bad} into {wiki}");
            assert!(!goes_into(wiki, bad), "{wiki} into {bad}");
        }
    }
    assert!(may_be_a_default(cc0) && may_be_a_default(by) && may_be_a_default(sa));
}

#[test]
fn a_name_from_a_bundle_is_shown_without_control_or_bidirectional_characters_and_cut() {
    let hostile = "a\u{1b}[31mb\u{202e}c\u{200b}d\u{2066}e\nf\\g\u{7}";
    let s = safe::shown(hostile);
    assert!(
        !s.chars().any(|c| c.is_control()
            || ('\u{202a}'..='\u{202e}').contains(&c)
            || ('\u{2066}'..='\u{2069}').contains(&c)
            || c == '\u{200b}'),
        "{s:?}"
    );
    assert_eq!(
        s,
        "a\\u{1b}[31mb\\u{202e}c\\u{200b}d\\u{2066}e\\u{a}f\\\\g\\u{7}"
    );
    // Bounded, and the cut says how much.
    let long = "x".repeat(500);
    let s = safe::shown(&long);
    assert!(
        s.starts_with(&"x".repeat(120)) && s.contains("and 380 more character(s)"),
        "{s}"
    );
    assert!(safe::shown("日本語の名前").contains("日本語"));
}

#[test]
fn an_address_is_told_from_a_name_that_merely_has_an_at_sign() {
    for yes in [
        "ann@example.org",
        "Ann <ann@example.org>",
        "contact: ann.b+x@mail.example.co.jp.",
        "MAILTO:ann@x.org",
        "mailto:anything",
    ] {
        assert!(looks_like_an_address(yes), "{yes}");
    }
    for no in [
        "Ann",
        "@ann",
        "ann@",
        "ann@localhost",
        "a@b@c.org",
        "email me",
        "https://example.org/@ann",
        "x@y.z",
    ] {
        assert!(!looks_like_an_address(no), "{no}");
    }
}

#[test]
fn a_manifest_is_read_by_its_known_keys_and_a_known_key_of_the_wrong_type_is_a_named_refusal() {
    let parse = |text: &str| {
        let value = inklkunn_markup::frontmatter::parse_yaml(text).unwrap();
        let inklkunn_markup::Value::Map(pairs) = value else {
            panic!("not a map");
        };
        manifest::read(&pairs)
    };
    let ok = parse("format: portable-wiki-bundle\nversion: '0.1'\nlicense: CC0-1.0\nfidelity:\n  level: 3\n  dropped:\n    - construct: media\nfuture_key: 1\nother: x\n").unwrap();
    assert_eq!(ok.fidelity_level, Some(3));
    assert_eq!(ok.dropped, vec!["media"]);
    assert_eq!(ok.unknown_keys, vec!["future_key", "other"]);
    // PyYAML writes a sequence at its key's indentation: the same value.
    let flush = parse("format: portable-wiki-bundle\nversion: '0.1'\nnamespaces:\n- Talk\n- Help\nfidelity:\n  degraded:\n  - construct: x\n    how: y\n").unwrap();
    assert_eq!(flush.namespaces, vec!["Talk", "Help"]);
    assert_eq!(flush.degraded, vec!["x"]);
    // What is wrong, by key, and never the value.
    for (text, key) in [
        ("format: something-else\nversion: '0.1'\n", "`format`"),
        ("format: portable-wiki-bundle\nversion: 0.1\n", "`version`"),
        (
            "format: portable-wiki-bundle\nversion: '0.1'\nlicense: 5\n",
            "`license`",
        ),
        (
            "format: portable-wiki-bundle\nversion: '0.1'\nfidelity:\n  level: 9\n",
            "`fidelity.level`",
        ),
        (
            "format: portable-wiki-bundle\nversion: '0.1'\nvisibility_default: secret\n",
            "`visibility_default`",
        ),
        (
            "format: portable-wiki-bundle\nversion: '0.1'\nsource:\n  url:\n    - x\n",
            "`source.url`",
        ),
        (
            "format: portable-wiki-bundle\nversion: '0.1'\ncontents:\n  history: maybe-not-a-bool-ish\n",
            "`contents.history`",
        ),
    ] {
        let reasons = parse(text).unwrap_err();
        assert!(
            reasons.iter().any(|r| r.contains(key)),
            "{text}: {reasons:?}"
        );
    }
}

#[test]
fn a_checksums_file_is_read_line_by_line_and_a_fault_names_the_line_and_never_its_text() {
    let good = format!(
        "{}  pages/A.md\n{}  history/x.jsonl\n",
        "ab".repeat(32),
        "cd".repeat(32)
    );
    assert_eq!(manifest::read_sums(&good).unwrap().len(), 2);
    assert_eq!(manifest::read_sums("").unwrap().len(), 0);
    assert_eq!(manifest::read_sums("\n").unwrap().len(), 0);
    for bad in [
        format!(
            "{}  pages/A.md\n\n{}  b\n",
            "ab".repeat(32),
            "cd".repeat(32)
        ),
        format!("{} pages/A.md\n", "ab".repeat(32)),
        format!(
            "{}  pages/A.md\n{}  pages/A.md\n",
            "ab".repeat(32),
            "cd".repeat(32)
        ),
        format!("{}  ../outside\n", "ab".repeat(32)),
        format!("{}  /absolute\n", "ab".repeat(32)),
        format!("{}  a//b\n", "ab".repeat(32)),
        format!("{}  ./a\n", "ab".repeat(32)),
        format!("{}  \n", "ab".repeat(32)),
        format!("{}  pages/A.md\n", "AB".repeat(32)),
        format!("{}  pages/A.md\n", "ab".repeat(31)),
        "not a sum  at all\n".to_owned(),
    ] {
        assert!(manifest::read_sums(&bad).is_err(), "{bad:?}");
    }
    let fault = manifest::read_sums(&format!("{}  a\nCANARY\n", "ab".repeat(32))).unwrap_err();
    assert_eq!(fault.line, 2);
    assert!(!fault.rule.contains("CANARY"));
}

#[test]
fn a_history_file_is_named_by_the_page_id_or_by_the_path_its_name_codes() {
    assert_eq!(
        path_of_stem("~Docs%2FDeep%20Page").unwrap(),
        "pages/Docs/Deep Page.md"
    );
    assert_eq!(
        path_of_stem("~Plain-name_1.v2").unwrap(),
        "pages/Plain-name_1.v2.md"
    );
    assert_eq!(path_of_stem("~%41%4a").unwrap(), "pages/AJ.md");
    assert_eq!(path_of_stem("mw:101"), None);
    assert_eq!(path_of_stem("home"), None);
    assert_eq!(path_of_stem("~bad name"), None);
    assert_eq!(path_of_stem("~%zz"), None);
    assert_eq!(path_of_stem("~%4"), None);
}

#[test]
fn the_replays_estimate_is_the_fit_to_the_two_measurements() {
    assert_eq!(BundleReport::estimate_for(500), 154);
    assert_eq!(BundleReport::estimate_for(2000), 1438);
    assert_eq!(BundleReport::estimate_for(0), 0);
    assert!(BundleReport::estimate_for(1000) > 2 * BundleReport::estimate_for(500) - 1);
}

// ------------------------------------------------------------------------------------------------ the fixtures' reports

#[test]
fn the_mediawiki_fixture_reports_its_history_people_suppressions_and_unmapped_prefix() {
    let options = BundleOptions {
        bundle_license: Some("CC-BY-SA-4.0".to_owned()),
        ..BundleOptions::default()
    };
    let p = plan_with(&fixture("mediawiki/bundle"), &options);
    assert_importable(&p);
    let r = &p.report;
    assert_eq!(r.bundle_version, "0.1");
    assert_eq!(r.fidelity_declared, Some(3));
    assert_eq!(r.fidelity_found, Found::History);
    assert_eq!(r.imported_from_name.as_deref(), Some("Garden Notes"));
    assert_eq!(
        r.imported_from_url.as_deref(),
        Some("https://garden.example.org/wiki")
    );
    assert_eq!(r.languages, vec!["en"]);
    // The converter declares no licence: the admin's, said as such.
    assert_eq!(r.license.as_deref(), Some("CC-BY-SA-4.0"));
    assert_eq!(r.license_from, Some(LicenseFrom::DeclaredByAdmin));
    assert_eq!(
        r.checksums,
        Checksums::Verified {
            file: "sha256sums.txt".to_owned(),
            entries: 9
        }
    );
    assert_eq!((r.pages, r.attachments, r.sidecars), (4, 0, 0));
    // 8 records; one has its text hidden at the source and is not replayed, one is replayed with its author hidden.
    assert_eq!((r.records, r.revisions), (8, 7));
    assert_eq!(
        (
            r.suppressed_content,
            r.suppressed_author,
            r.suppressed_summary
        ),
        (1, 1, 0)
    );
    let by_page: Vec<(&str, usize, usize)> = r
        .histories
        .iter()
        .map(|h| (h.page.as_str(), h.records, h.revisions))
        .collect();
    assert_eq!(
        by_page,
        vec![
            ("pages/Composting.md", 4, 3),
            ("pages/Worm bins.md", 2, 2),
            ("pages/Compost heap.md", 1, 1),
            ("pages/Talk/Composting.md", 1, 1),
        ]
    );
    // The converter's final files carry more frontmatter than the last record's text: every page ends with the last change.
    assert_eq!(r.final_state_pages.len(), 4);
    assert!(r.pages_without_history.is_empty());
    // Two people by name and one anonymous label, none an account; kinds as the source says them.
    assert_eq!(r.people.total, 3);
    assert_eq!(r.people.in_users_file, 2);
    assert_eq!(r.people.by_kind.get("person"), Some(&2));
    assert_eq!(r.people.by_kind.get("anonymous"), Some(&1));
    assert!(
        r.people.names.iter().any(|n| n == "Mira")
            && r.people.names.iter().any(|n| n == "~ip-d861b7e910")
    );
    // `Talk` is a namespace of the bundle; `wikipedia` is a prefix no map resolves.
    assert_eq!(r.unmapped_prefixes.len(), 1);
    assert_eq!(r.unmapped_prefixes.get("wikipedia"), Some(&1));
    assert!(r.incompatible.is_empty() && r.left_out.is_empty());
    assert_eq!(r.estimate_seconds, Some(BundleReport::estimate_for(7)));
    assert_eq!(r.degraded_in_the_export.len(), 2);
    assert_eq!(r.dropped_in_the_export.len(), 2);
    // The text says it, and says what the estimate is of.
    let text = p.render();
    for needle in [
        "fidelity declared: level 3",
        "7 revision(s) would be replayed, one block each (the cap is 1000)",
        "declared by the importing admin",
        "wikipedia: 1 link(s)",
        "none is linked to an account of this server",
        "500 blocks 154 s, 2,000 blocks 1,438 s",
    ] {
        assert!(text.contains(needle), "{needle}:\n{text}");
    }
    // The order of the replay is by instant, ties by path, and the same every time.
    assert_eq!(p.schedule.len(), 7);
    let again = plan_with(&fixture("mediawiki/bundle"), &options);
    assert_eq!(p.schedule, again.schedule);
    let instants: Vec<time::Instant> = p
        .schedule
        .iter()
        .map(|&(h, v)| p.histories.get(h).unwrap().revisions.get(v).unwrap().at)
        .collect();
    let mut sorted = instants.clone();
    sorted.sort();
    assert_eq!(instants, sorted);
    // A bundle that declares no licence is refused without the option, by a sentence that names it.
    let refused = plan(&fixture("mediawiki/bundle"));
    assert_refused(&refused, "wiki-bundle.yaml", "--bundle-license");
}

#[test]
fn the_bookstack_fixture_comes_in_as_pages_and_attachments_in_one_change_with_no_history() {
    let options = BundleOptions {
        bundle_license: Some("CC0-1.0".to_owned()),
        ..BundleOptions::default()
    };
    let p = plan_with(&fixture("bookstack/bundle"), &options);
    assert_importable(&p);
    let r = &p.report;
    assert_eq!(r.fidelity_declared, Some(2));
    assert_eq!(r.fidelity_found, Found::ContentOnly);
    assert_eq!((r.pages, r.attachments, r.sidecars), (5, 2, 2));
    assert_eq!((r.records, r.revisions), (0, 0));
    assert!(r.histories.is_empty() && r.final_state_pages.is_empty());
    assert_eq!(r.pages_without_history.len(), 5);
    assert_eq!(r.estimate_seconds, None);
    assert_eq!(r.people.total, 0);
    assert_eq!(r.dropped_in_the_export, vec!["revision history"]);
    assert!(r.checksums != Checksums::None);
    // Every file the first change writes: five pages, two attachments, their two sidecars.
    assert_eq!(p.plan.files.len(), 9);
    assert!(p.histories.is_empty() && p.schedule.is_empty());
    assert!(p.render().contains("no revision that can be replayed"));
}

#[test]
fn inklkunns_own_export_is_read_and_its_pages_end_where_their_last_revision_does() {
    let p = plan(&fixture("inklkunn/bundle"));
    assert_importable(&p);
    let r = &p.report;
    assert_eq!(r.license.as_deref(), Some("CC0-1.0"));
    assert_eq!(r.license_from, Some(LicenseFrom::Manifest));
    assert_eq!(r.fidelity_found, Found::History);
    assert_eq!((r.pages, r.attachments, r.sidecars), (3, 1, 1));
    assert_eq!((r.records, r.revisions), (6, 6));
    // An own export's final files are its last records' texts.
    assert!(r.final_state_pages.is_empty(), "{:?}", r.final_state_pages);
    assert!(r.pages_without_history.is_empty());
    // The service is the one author: a bot with no id, said as the source's.
    assert_eq!(r.people.total, 1);
    assert_eq!(r.people.by_kind.get("bot"), Some(&1));
    // The keep-files and `.prikkignore` are the wiki's own and are left out, said.
    let left: Vec<&str> = r.left_out.iter().map(|l| l.what.as_str()).collect();
    assert_eq!(left, vec![".prikkignore", "keep-files"]);
    assert_eq!(r.unmapped_prefixes.get("wikipedia"), Some(&1));
    // The history of the page that was edited twice is three revisions in the file's order, each parent the one before.
    let pruning = p
        .histories
        .iter()
        .find(|h| h.page.as_deref() == Some("pages/Pruning.md"))
        .unwrap();
    assert_eq!(pruning.revisions.len(), 3);
    assert!(
        pruning
            .revisions
            .iter()
            .all(|v| matches!(v.step, Step::Text { .. }))
    );
    assert!(
        pruning
            .revisions
            .iter()
            .all(|v| v.path.as_deref() == Some("pages/Pruning.md"))
    );
}

#[test]
fn the_checksums_are_the_file_the_manifest_names_and_never_a_fixed_name() {
    let d = copy_of("sums-name", "inklkunn/bundle");
    // The manifest names another file: that one is read.
    std::fs::rename(d.0.join("sha256sums.txt"), d.0.join("sums.txt")).unwrap();
    let manifest_path = d.0.join("wiki-bundle.yaml");
    let m = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(
        &manifest_path,
        m.replace("checksums: sha256sums.txt", "checksums: sums.txt"),
    )
    .unwrap();
    let p = plan(&d.0);
    assert_importable(&p);
    assert!(matches!(&p.report.checksums, Checksums::Verified { file, .. } if file == "sums.txt"));
    // A file by the old name that the manifest does not name is a file the import does not read: left out and said, not refused.
    put(&d.0, "sha256sums.txt", b"anything");
    let p = plan(&d.0);
    assert_importable(&p);
    assert!(
        p.report.left_out.iter().any(|l| l.what == "sha256sums.txt"),
        "{}",
        p.render()
    );
    // Without the key, no file of checksums is read at all, and it is said.
    std::fs::write(&manifest_path, m.replace("checksums: sha256sums.txt\n", "")).unwrap();
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.checksums, Checksums::None);
    assert!(p.render().contains("checksums: none in the bundle"));
}

// ------------------------------------------------------------------------------------------------ refusals (criterion 6)

#[test]
fn a_page_whose_licence_the_wiki_cannot_take_refuses_and_leaving_out_leaves_exactly_it_out() {
    let d = dir("lic-page");
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    put(
        &d.0,
        "pages/Free.md",
        b"---\nid: free\ntitle: Free\nlicense: CC0-1.0\n---\n\nfree\n",
    );
    put(
        &d.0,
        "pages/Tied.md",
        b"---\nid: tied\ntitle: Tied\nlicense: CC-BY-NC-4.0\n---\n\ntied\n",
    );
    put(
        &d.0,
        "pages/Old.md",
        b"---\nid: old\ntitle: Old\nlicense: GFDL-1.3-or-later\n---\n\nold\n",
    );
    put(
        &d.0,
        "history/tied.jsonl",
        (record(
            "t1",
            None,
            "2025-01-01T00:00:00Z",
            Some("---\nid: tied\ntitle: Tied\nlicense: CC-BY-NC-4.0\n---\n\ntied\n"),
        ) + "\n")
            .as_bytes(),
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_refused(&p, "pages/Tied.md", "CC-BY-NC-4.0");
    assert_refused(&p, "pages/Old.md", "GFDL-1.3-or-later");
    assert_eq!(p.plan.refused.len(), 2, "{}", p.render());
    assert!(!p.render().contains("pages/Free.md: "));
    assert_eq!(p.report.incompatible.len(), 2);
    assert!(
        p.render()
            .contains("licences that no wiki with this default may take")
    );
    // Leaving out leaves exactly those out, with their history, and names each.
    let leave = BundleOptions {
        leave_out_incompatible: true,
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &leave);
    assert_importable(&p);
    let paths: Vec<&str> = p.plan.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["pages/Free.md", "pages/Home.md"]);
    assert!(p.histories.is_empty() && p.schedule.is_empty());
    assert_eq!(p.report.pages, 2);
    assert_eq!(p.report.incompatible.len(), 2);
    assert!(p.report.incompatible_left_out);
    assert!(
        p.render()
            .contains("left out for their licence (--leave-out-incompatible)")
    );
    assert!(p.render().contains("pages/Tied.md: CC-BY-NC-4.0"));
}

#[test]
fn an_old_revision_under_a_licence_the_wiki_cannot_take_makes_its_page_incompatible_too() {
    let d = dir("lic-rev");
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", text(2).as_bytes());
    let nc = "---\nid: home\ntitle: Home\nlicense: CC-BY-NC-4.0\n---\n\nonce\n";
    let lines = [
        record("r1", None, "2025-01-01T00:00:00Z", Some(nc)),
        record("r2", Some("r1"), "2025-01-02T00:00:00Z", Some(&text(2))),
    ];
    put(
        &d.0,
        "history/home.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_refused(&p, "pages/Home.md", "CC-BY-NC-4.0");
    assert!(p.render().contains("revision `r1`"), "{}", p.render());
    let leave = BundleOptions {
        leave_out_incompatible: true,
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &leave);
    assert!(p.plan.files.is_empty() && p.histories.is_empty());
    // A record's own licence member counts the same.
    let d = dir("lic-rev-member");
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", text(1).as_bytes());
    let mut v: serde_json::Value =
        serde_json::from_str(&record("r1", None, "2025-01-01T00:00:00Z", Some(&text(1)))).unwrap();
    set(&mut v, "license", "CC-BY-ND-4.0");
    put(
        &d.0,
        "history/home.jsonl",
        (v.to_string() + "\n").as_bytes(),
    );
    seal(&d.0);
    assert_refused(&plan(&d.0), "pages/Home.md", "CC-BY-ND-4.0");
}

#[test]
fn an_attachment_whose_sidecar_licence_the_wiki_cannot_take_refuses_and_goes_with_its_sidecar_when_left_out()
 {
    let d = dir("lic-att");
    manifest(&d.0, &["license: CC-BY-4.0", "checksums: sha256sums.txt"]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    put(&d.0, "attachments/ok.png", b"png");
    put(&d.0, "attachments/ok.png.meta.yaml", b"license: CC0-1.0\n");
    put(&d.0, "attachments/sa.png", b"png");
    put(
        &d.0,
        "attachments/sa.png.meta.yaml",
        b"license: CC-BY-SA-4.0\n",
    );
    put(&d.0, "attachments/none.png", b"png");
    seal(&d.0);
    // BY-SA does not go into a BY wiki; CC0 does; an attachment with no sidecar takes the wiki's.
    let p = plan(&d.0);
    assert_refused(&p, "attachments/sa.png", "CC-BY-SA-4.0");
    assert_eq!(p.plan.refused.len(), 1, "{}", p.render());
    let leave = BundleOptions {
        leave_out_incompatible: true,
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &leave);
    assert_importable(&p);
    let paths: Vec<&str> = p.plan.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "attachments/none.png",
            "attachments/ok.png",
            "attachments/ok.png.meta.yaml",
            "pages/Home.md"
        ]
    );
    assert_eq!((p.report.attachments, p.report.sidecars), (2, 1));
    // A sidecar that is not a map, or whose licence is not text, is a refusal of its own.
    put(&d.0, "attachments/ok.png.meta.yaml", b"license:\n  - a\n");
    seal(&d.0);
    assert_refused(&plan(&d.0), "attachments/ok.png.meta.yaml", "not text");
}

#[test]
fn the_wiki_takes_the_bundles_licence_and_never_another() {
    // The bundle's own, however the admin feels: an option that names another is refused, not applied.
    let d = small("lic-own");
    let other = BundleOptions {
        bundle_license: Some("CC0-1.0".to_owned()),
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &other);
    assert_refused(&p, "wiki-bundle.yaml", "never changed on the way in");
    // The same licence given again is the bundle's.
    let same = BundleOptions {
        bundle_license: Some("CC-BY-SA-4.0".to_owned()),
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &same);
    assert_importable(&p);
    assert_eq!(p.report.license_from, Some(LicenseFrom::Manifest));
    // A licence no wiki may have as its default refuses, wherever it came from; a name that is not an identifier's shape too.
    for (declared, rule) in [
        ("CC-BY-NC-4.0", "no wiki can have as its default"),
        ("GFDL-1.3-or-later", "no wiki can have as its default"),
        ("All rights reserved", "no wiki can have as its default"),
        ("a;b", "not an SPDX identifier"),
    ] {
        let d = dir("lic-default");
        manifest(&d.0, &[&format!("license: \"{declared}\"")]);
        put(&d.0, "pages/Home.md", HOME.as_bytes());
        assert_refused(&plan(&d.0), "wiki-bundle.yaml", rule);
        let d = dir("lic-default-given");
        manifest(&d.0, &[]);
        put(&d.0, "pages/Home.md", HOME.as_bytes());
        let given = BundleOptions {
            bundle_license: Some(declared.to_owned()),
            ..BundleOptions::default()
        };
        assert_refused(&plan_with(&d.0, &given), "--bundle-license", rule);
    }
    // A bundle that declares none and is not given one is refused with the way on.
    let d = dir("lic-none");
    manifest(&d.0, &[]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    assert_refused(&plan(&d.0), "wiki-bundle.yaml", "declares no `license`");
}

#[test]
fn checksums_that_are_present_are_verified_for_every_file_and_a_lie_a_gap_and_a_stranger_each_refuse_by_name()
 {
    // A page that was changed after the checksums were made.
    let d = small("sum-lie");
    put(&d.0, "pages/Home.md", b"---\nid: home\n---\n\nchanged\n");
    assert_refused(
        &plan(&d.0),
        "pages/Home.md",
        "SHA-256 that is not the one the checksums file gives it",
    );
    // A file the checksums name and the bundle lacks.
    let d = small("sum-gap");
    std::fs::remove_file(d.0.join("pages/Home.md")).unwrap();
    put(&d.0, "pages/Other.md", b"---\nid: other\n---\n\nx\n");
    let p = plan(&d.0);
    assert_refused(&p, "pages/Home.md", "missing from the bundle");
    assert_refused(&p, "pages/Other.md", "does not name it");
    // An extra file that the import READS (a page, an attachment, a history file, `users.yaml`): refused, by name. The I-T1 review's
    // ruling on I-T2: the rule is about what is read, and nothing unverified enters the wiki.
    for (rel, bytes) in [
        ("pages/Other.md", &b"---\nid: other\n---\n\nx\n"[..]),
        ("attachments/extra.bin", b"x"),
        ("attachments/extra.bin.meta.yaml", b"license: CC0-1.0\n"),
        ("history/home.jsonl", b""),
        ("users.yaml", b"- id: ann\n  name: Ann\n"),
    ] {
        let d = small("sum-extra");
        put(&d.0, rel, bytes);
        assert_refused(
            &plan(&d.0),
            rel,
            "the import reads it, and the checksums file does not name it",
        );
    }
    // An extra file that the import does not read is no refusal: it is listed as not imported, and the bundle is as importable as it
    // was. Among them the converters' own report, which they write after their checksums, so their default output imports.
    for rel in [
        "extensions/notes.txt",
        "import-report.md",
        "export-report.md",
        "governance/rules.txt",
        "pages/.keep",
        ".prikkignore",
        "README.txt",
    ] {
        let d = small("sum-unread");
        put(&d.0, rel, b"x");
        let p = plan(&d.0);
        assert_importable(&p);
        assert!(
            p.render().contains(rel.split('/').next().unwrap_or(rel)) || rel.ends_with(".keep"),
            "{rel} is not said:\n{}",
            p.render()
        );
        assert_eq!(
            p.report.checksums,
            Checksums::Verified {
                file: "sha256sums.txt".to_owned(),
                entries: 1
            },
            "{rel}"
        );
    }
    // With no history read (`--no-history`), a history file is a file the import does not read.
    let d = small("sum-nohistory");
    put(&d.0, "history/home.jsonl", b"");
    let given = BundleOptions {
        no_history: true,
        ..BundleOptions::default()
    };
    assert_importable(&plan_with(&d.0, &given));
    // Every kind of file is hashed: an attachment, a history file, users.yaml, and a file in an unread directory.
    for rel in [
        "attachments/a.bin",
        "history/home.jsonl",
        "users.yaml",
        "extensions/x/y.bin",
        "governance/g.txt",
    ] {
        let d = dir("sum-kinds");
        manifest(
            &d.0,
            &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
        );
        put(&d.0, "pages/Home.md", HOME.as_bytes());
        put(&d.0, "attachments/a.bin", b"one");
        put(
            &d.0,
            "history/home.jsonl",
            (three().join("\n") + "\n").as_bytes(),
        );
        put(&d.0, "users.yaml", b"- id: ann\n  name: Ann\n");
        put(&d.0, "extensions/x/y.bin", b"one");
        put(&d.0, "governance/g.txt", b"one");
        seal(&d.0);
        assert_importable(&plan(&d.0));
        let before = std::fs::read(d.0.join(rel)).unwrap();
        let mut changed = before.clone();
        changed.push(b' ');
        std::fs::write(d.0.join(rel), &changed).unwrap();
        let p = plan(&d.0);
        assert!(
            p.plan.refused.iter().any(|r| r.name == rel),
            "{rel} was not checked:\n{}",
            p.render()
        );
    }
    // The file the manifest names is not there, or is not a checksums file, or names a path outside the bundle.
    let d = small("sum-missing-file");
    std::fs::remove_file(d.0.join("sha256sums.txt")).unwrap();
    assert_refused(&plan(&d.0), "sha256sums.txt", "not in the bundle");
    let d = small("sum-garbage");
    put(&d.0, "sha256sums.txt", b"this is not a checksums file\n");
    assert_refused(&plan(&d.0), "sha256sums.txt", "line 1");
    let d = dir("sum-outside");
    manifest(&d.0, &["license: CC0-1.0", "checksums: ../sums.txt"]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    assert_refused(
        &plan(&d.0),
        "wiki-bundle.yaml",
        "not a path inside the bundle",
    );
    // And a bundle with none says so and is imported (SEC-7: when present).
    let d = dir("sum-none");
    manifest(&d.0, &["license: CC0-1.0"]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.checksums, Checksums::None);
    assert!(p.render().contains("none in the bundle"));
}

#[test]
fn a_history_files_own_order_is_its_history_and_times_are_compared_as_instants() {
    let rec = record;
    // A parent that is not the record before it.
    let d = with_history(
        "ord-parent",
        &[
            rec("r1", None, "2025-01-01T00:00:00Z", Some(&text(1))),
            rec("r2", Some("r1"), "2025-01-02T00:00:00Z", Some(&text(2))),
            rec("r3", Some("r1"), "2025-01-03T00:00:00Z", Some(&text(3))),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "rev `r3` has a `parent` that is not the record before it",
    );
    // A first record that names a parent the file does not hold begins the history mid-way (V-10, a partial dump); every later
    // record still names the one before it, and a parent that is in the file but later on is an order that is broken.
    let d = with_history(
        "ord-first",
        &[
            rec("r5", Some("r4"), "2025-01-01T00:00:00Z", Some(&text(1))),
            rec("r6", Some("r5"), "2025-01-02T00:00:00Z", Some(&text(2))),
        ],
    );
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(
        p.report.histories.first().and_then(|h| h.begins_at.clone()),
        Some("r5".to_owned())
    );
    assert!(
        p.render().contains("(history begins at r5)"),
        "{}",
        p.render()
    );
    assert_eq!(
        p.histories.first().and_then(|h| h.begins_at.clone()),
        Some("r5".to_owned())
    );
    let d = with_history(
        "ord-first-later",
        &[
            rec("r1", Some("r2"), "2025-01-01T00:00:00Z", Some(&text(1))),
            rec("r2", Some("r1"), "2025-01-02T00:00:00Z", Some(&text(2))),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "names a `parent` that comes after it in the file",
    );
    // The records after the first still name the one before them.
    let d = with_history(
        "ord-mid-broken",
        &[
            rec("r5", Some("r4"), "2025-01-01T00:00:00Z", Some(&text(1))),
            rec("r6", Some("r4"), "2025-01-02T00:00:00Z", Some(&text(2))),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "rev `r6` has a `parent` that is not the record before it",
    );
    // A whole history says nothing of a beginning.
    let whole = with_history("ord-whole", &three());
    assert_eq!(
        plan(&whole.0)
            .report
            .histories
            .first()
            .and_then(|h| h.begins_at.clone()),
        None
    );
    let d = with_history(
        "ord-dup",
        &[
            rec("r1", None, "2025-01-01T00:00:00Z", Some(&text(1))),
            rec("r1", Some("r1"), "2025-01-02T00:00:00Z", Some(&text(2))),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "is used by an earlier record",
    );
    // A time before the record before it: as a string it would be later, as an instant it is earlier.
    let d = with_history(
        "ord-at",
        &[
            rec("r1", None, "2025-01-01T05:00:00Z", Some(&text(1))),
            rec(
                "r2",
                Some("r1"),
                "2025-01-01T10:00:00+09:00",
                Some(&text(2)),
            ),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "rev `r2` is earlier than the record before it",
    );
    // The other way round: the string is smaller and the instant later, which is a good order.
    let d = with_history(
        "ord-at-ok",
        &[
            rec("r1", None, "2025-01-01T04:00:00Z", Some(&text(1))),
            rec(
                "r2",
                Some("r1"),
                "2025-01-01T00:30:00-05:00",
                Some(&text(2)),
            ),
            rec("r3", Some("r2"), "2025-01-01T05:30:00Z", Some(&text(3))),
        ],
    );
    assert_importable(&plan(&d.0));
    // The same instant twice is not earlier.
    let d = with_history(
        "ord-same",
        &[
            rec("r1", None, "2025-01-01T00:00:00Z", Some(&text(1))),
            rec(
                "r2",
                Some("r1"),
                "2025-01-01T09:00:00+09:00",
                Some(&text(2)),
            ),
        ],
    );
    assert_importable(&plan(&d.0));
    // A time that is not a time.
    let d = with_history(
        "ord-bad-at",
        &[rec("r1", None, "yesterday", Some(&text(1)))],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "`at` is not an RFC 3339 date-time",
    );
    // Two history files for one page (by its id and by its path).
    let d = with_history("ord-two", &three());
    put(
        &d.0,
        "history/~Home.jsonl",
        (three().join("\n") + "\n").as_bytes(),
    );
    seal(&d.0);
    assert_refused(
        &plan(&d.0),
        "history/~Home.jsonl",
        "is a second history file of the page",
    );
}

#[test]
fn a_text_is_checked_against_its_sha256_and_a_patch_is_applied_to_the_text_before_it() {
    // A content that is not the digest it says.
    let mut v: serde_json::Value =
        serde_json::from_str(&record("r1", None, "2025-01-01T00:00:00Z", Some(&text(3)))).unwrap();
    set(&mut v, "sha256", sha_hex(b"something else"));
    let d = with_history("sha-bad", &[v.to_string()]);
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "rev `r1` has a `sha256` that is not the text's",
    );
    // A patch that makes the text it says it makes.
    let one = text(1);
    let two = text(2);
    let patch = "@@ -6 +6 @@\n-Version 1.\n+Version 2.\n";
    assert_eq!(patch::apply(&one, patch).unwrap(), two);
    let mut p2: serde_json::Value =
        serde_json::from_str(&record("r2", Some("r1"), "2025-01-02T00:00:00Z", None)).unwrap();
    set(&mut p2, "patch", patch);
    set(&mut p2, "sha256", sha_hex(two.as_bytes()));
    let lines = [
        record("r1", None, "2025-01-01T00:00:00Z", Some(&one)),
        p2.to_string(),
        record("r3", Some("r2"), "2025-01-03T00:00:00Z", Some(&text(3))),
    ];
    let d = with_history("sha-patch", &lines);
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.revisions, 3);
    // …and the digest of what the patch made is what the second revision's text is.
    let h = p.histories.first().unwrap();
    let Step::Text { sha256, bytes } = &h.revisions.get(1).unwrap().step else {
        panic!("not text");
    };
    assert_eq!(hex(sha256), sha_hex(two.as_bytes()));
    assert_eq!(*bytes, two.len() as u64);
    // A patch whose result is not the digest; a patch with no digest at all; a patch with no text before it; a patch that does not fit.
    let mut wrong = p2.clone();
    set(&mut wrong, "sha256", sha_hex(b"not it"));
    let d = with_history(
        "sha-patch-wrong",
        &[
            record("r1", None, "2025-01-01T00:00:00Z", Some(&one)),
            wrong.to_string(),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "rev `r2` has a `sha256` that is not the text's",
    );
    let mut nodigest = p2.clone();
    nodigest.as_object_mut().unwrap().remove("sha256");
    let d = with_history(
        "sha-patch-none",
        &[
            record("r1", None, "2025-01-01T00:00:00Z", Some(&one)),
            nodigest.to_string(),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "no `sha256` to check the result against",
    );
    let mut first = p2.clone();
    set(&mut first, "parent", serde_json::Value::Null);
    let d = with_history("sha-patch-first", &[first.to_string()]);
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "no text before it to apply it to",
    );
    let mut misfit = p2.clone();
    set(
        &mut misfit,
        "patch",
        "@@ -6 +6 @@\n-Version 9.\n+Version 2.\n",
    );
    let d = with_history(
        "sha-patch-misfit",
        &[
            record("r1", None, "2025-01-01T00:00:00Z", Some(&one)),
            misfit.to_string(),
        ],
    );
    assert_refused(
        &plan(&d.0),
        "history/home.jsonl",
        "has a `patch` that has a context or removed line that is not the text's line",
    );
}

#[test]
fn every_revision_text_is_judged_as_a_page_is_by_its_frontmatter_and_the_page_cap() {
    // A revision whose frontmatter the parser refuses, though the page's last text is fine.
    let bad = "---\ntitle: &anchor Bad\n---\n\nCANARY-FRONT\n";
    let d = with_history(
        "rev-front",
        &[
            record("r1", None, "2025-01-01T00:00:00Z", Some(bad)),
            record("r2", Some("r1"), "2025-01-02T00:00:00Z", Some(&text(3))),
        ],
    );
    let p = plan(&d.0);
    assert_refused(
        &p,
        "history/home.jsonl",
        "rev `r1` has a text that has a frontmatter the parser refuses",
    );
    assert!(!p.render().contains("CANARY-FRONT"));
    // A revision over the page cap, while the last page is under it.
    let big = format!("---\nid: home\n---\n\n{}\n", "x".repeat(4000));
    let d = with_history(
        "rev-cap",
        &[
            record("r1", None, "2025-01-01T00:00:00Z", Some(&big)),
            record("r2", Some("r1"), "2025-01-02T00:00:00Z", Some(&text(3))),
        ],
    );
    let small_cap = ReadConfig {
        page_max_bytes: 2048,
        ..ReadConfig::default()
    };
    let p = plan_bundle(&d.0, &small_cap, &BundleOptions::default()).unwrap();
    assert_refused(
        &p,
        "history/home.jsonl",
        "rev `r1` has a text larger than the 2 KiB cap of a page",
    );
    // The page file itself over the cap is the folder import's refusal.
    put(&d.0, "pages/Home.md", big.as_bytes());
    seal(&d.0);
    let p = plan_bundle(&d.0, &small_cap, &BundleOptions::default()).unwrap();
    assert_refused(
        &p,
        "pages/Home.md",
        "is larger than the 2.0 KiB cap of a page",
    );
}

#[test]
fn a_hostile_walk_is_refused_by_the_folder_imports_rules_for_the_files_a_bundle_shares_with_it() {
    let d = small("walk");
    std::os::unix::fs::symlink("/etc/passwd", d.0.join("pages/Evil.md")).unwrap();
    std::fs::create_dir_all(d.0.join("attachments")).unwrap();
    std::os::unix::fs::symlink("/", d.0.join("attachments/rootdir")).unwrap();
    std::os::unix::fs::symlink("/etc", d.0.join("history")).unwrap();
    put(&d.0, "pages/CON.md", b"x");
    let deep = (0..12)
        .map(|i| format!("{i}{}", "d".repeat(95)))
        .collect::<Vec<_>>()
        .join("/");
    put(&d.0, &format!("attachments/{deep}/file.bin"), b"x");
    let p = plan(&d.0);
    assert_refused(&p, "pages/Evil.md", "is a symbolic link");
    assert_refused(&p, "attachments/rootdir", "is a symbolic link");
    assert_refused(&p, "history", "is a symbolic link");
    assert_refused(&p, "pages/CON.md", "Windows reserves");
    assert!(
        p.plan
            .refused
            .iter()
            .any(|r| r.reason.contains("over the 1024-byte cap"))
    );
    // A link inside a directory the import does not read is a link all the same.
    let d = small("walk-other");
    put(&d.0, "extensions/x.txt", b"x");
    std::os::unix::fs::symlink("/etc/passwd", d.0.join("extensions/link")).unwrap();
    assert_refused(&plan(&d.0), "extensions/link", "is a symbolic link");
    // A named file where a directory must be.
    let d = small("walk-file");
    put(&d.0, "history", b"not a directory");
    assert_refused(&plan(&d.0), "history", "is not a directory");
    // A hard link (another name could change it after it was checked).
    let d = small("walk-hard");
    std::fs::hard_link(d.0.join("pages/Home.md"), d.0.join("pages/Twin.md")).unwrap();
    assert!(
        plan(&d.0)
            .plan
            .refused
            .iter()
            .any(|r| r.reason.contains("more than one hard link"))
    );
    // The folder is not a directory.
    let d = dir("walk-not-dir");
    put(&d.0, "file", b"x");
    let err = plan_bundle(
        &d.0.join("file"),
        &ReadConfig::default(),
        &BundleOptions::default(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("not a directory"), "{err}");
    // A bundle with no manifest is refused by name, and nothing else is judged.
    let d = dir("walk-nomanifest");
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    let p = plan(&d.0);
    assert_refused(&p, "wiki-bundle.yaml", "is missing");
    assert_eq!(p.plan.refused.len(), 2);
}

#[test]
fn a_bundle_over_the_revision_cap_is_refused_with_the_count_the_cap_and_the_way_on() {
    let capped = BundleOptions {
        max_revisions: 2,
        ..BundleOptions::default()
    };
    let d = with_history("cap", &three());
    let p = plan_with(&d.0, &capped);
    assert_refused(
        &p,
        "history/",
        "would make 3 blocks, and the cap is 2 (INKLKUNN_IMPORT_MAX_REVISIONS)",
    );
    assert_refused(&p, "history/", "`--no-history`");
    // At the cap is not over it.
    assert_importable(&plan_with(
        &d.0,
        &BundleOptions {
            max_revisions: 3,
            ..BundleOptions::default()
        },
    ));
    // `--no-history` imports the pages as they are, in one change; the report says what it leaves out.
    let none = BundleOptions {
        no_history: true,
        ..capped.clone()
    };
    let p = plan_with(&d.0, &none);
    assert_importable(&p);
    assert!(p.histories.is_empty() && p.schedule.is_empty());
    assert_eq!(p.report.revisions, 0);
    assert_eq!(p.report.pages_without_history, vec!["pages/Home.md"]);
    assert_eq!(p.report.estimate_seconds, None);
    assert!(
        p.report
            .left_out
            .iter()
            .any(|l| l.what == "history/" && l.why.contains("--no-history"))
    );
    assert!(p.render().contains("history: not imported (--no-history)"));
    // The checksums still cover the history that is left out.
    put(&d.0, "history/home.jsonl", b"changed\n");
    assert_refused(
        &plan_with(&d.0, &none),
        "history/home.jsonl",
        "SHA-256 that is not the one",
    );
    // The cap on the bytes of history/ refuses once, with the way on, and is no cap with --no-history.
    let d = with_history("cap-bytes", &three());
    let bytes = BundleOptions {
        max_history_bytes: 100,
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &bytes);
    assert_refused(&p, "history/", "cap on the bytes of the history");
    assert_eq!(
        p.plan
            .refused
            .iter()
            .filter(|r| r.name == "history/")
            .count(),
        1
    );
    assert_importable(&plan_with(
        &d.0,
        &BundleOptions {
            no_history: true,
            ..bytes
        },
    ));
}

#[test]
fn an_address_in_users_yaml_refuses_naming_the_entry_and_never_repeating_the_address() {
    for (tag, users) in [
        (
            "addr-key",
            "- id: ann\n  name: Ann\n  email: CANARY9f3a@example.org\n",
        ),
        (
            "addr-name",
            "- id: ann\n  name: Ann\n- id: bob\n  name: CANARY9f3a@example.org\n",
        ),
        (
            "addr-names",
            "users:\n  - id: bob\n    names: [Bob, \"Bob <CANARY9f3a@example.org>\"]\n",
        ),
        (
            "addr-unknown",
            "- id: ann\n  contact: mailto:CANARY9f3a@example.org\n",
        ),
        ("addr-id", "- id: CANARY9f3a@example.org\n  name: Ann\n"),
    ] {
        let d = dir(tag);
        manifest(&d.0, &["license: CC0-1.0", "checksums: sha256sums.txt"]);
        put(&d.0, "pages/Home.md", HOME.as_bytes());
        put(&d.0, "users.yaml", users.as_bytes());
        seal(&d.0);
        let p = plan(&d.0);
        assert!(!p.is_importable(), "{tag}: {}", p.render());
        let r = p
            .plan
            .refused
            .iter()
            .find(|r| r.name == "users.yaml")
            .unwrap();
        assert!(
            r.reason.contains("entry ") && r.reason.contains("e-mail address"),
            "{tag}: {r:?}"
        );
        assert!(
            !p.render().contains("9f3a"),
            "{tag}: the address is in the report:\n{}",
            p.render()
        );
        // The people of a file that was refused are not reported as people.
        assert_eq!(p.report.people.total, 0, "{tag}");
    }
    // The entry is named by its identifier when that is plainly one.
    let d = dir("addr-named");
    manifest(&d.0, &["license: CC0-1.0"]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    put(
        &d.0,
        "users.yaml",
        b"- id: ann\n  name: Ann\n- id: bob\n  name: Bob\n  email: bob@example.org\n",
    );
    assert_refused(
        &plan(&d.0),
        "users.yaml",
        "entry 2 (id `bob`) has an e-mail address",
    );
    // A directory without one is read, in either shape.
    for users in [
        "- id: ann\n  name: Ann\n  kind: person\n  url: https://example.org/User:Ann\n",
        "users:\n  - id: ann\n    names: [Ann, Annie]\n",
        "users: []\n",
    ] {
        let d = dir("addr-fine");
        manifest(&d.0, &["license: CC0-1.0"]);
        put(&d.0, "pages/Home.md", HOME.as_bytes());
        put(&d.0, "users.yaml", users.as_bytes());
        assert_importable(&plan(&d.0));
    }
    // An address as a history author is the same address.
    let mut v: serde_json::Value =
        serde_json::from_str(&record("r1", None, "2025-01-01T00:00:00Z", Some(&text(3)))).unwrap();
    set(
        &mut v,
        "author",
        serde_json::json!({"name": "CANARY9f3a@example.org"}),
    );
    let d = with_history("addr-author", &[v.to_string()]);
    let p = plan(&d.0);
    assert_refused(
        &p,
        "history/home.jsonl",
        "rev `r1` has an author that is an e-mail address",
    );
    assert!(!p.render().contains("9f3a"));
}

#[test]
fn no_byte_of_a_refused_file_is_repeated_and_every_name_is_display_safe() {
    let canary = "CANARY-7c1e";
    let d = dir("canary");
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    put(
        &d.0,
        "pages/Bad.md",
        format!("---\ntitle: &a Bad\n---\n\n{canary} body\n").as_bytes(),
    );
    put(&d.0, "pages/Latin.md", b"---\ntitle: Caf\xe9\n---\n");
    put(
        &d.0,
        "users.yaml",
        format!("- id: x\n  name: {canary}@example.org\n").as_bytes(),
    );
    // Records that are not JSON, that name the canary as a field's wrong type, and that carry it as a type of event.
    put(
        &d.0,
        "history/a.jsonl",
        format!("{{\"rev\": \"{canary}\", not json\n").as_bytes(),
    );
    put(
        &d.0,
        "history/b.jsonl",
        format!("{{\"rev\":\"r1\",\"at\":\"2025-01-01T00:00:00Z\",\"summary\":[\"{canary}\"]}}\n")
            .as_bytes(),
    );
    put(
        &d.0,
        "history/c.jsonl",
        format!("{{\"rev\":\"{canary}\",\"at\":\"{canary}\"}}\n").as_bytes(),
    );
    put(&d.0, "history/d.jsonl", format!("{{\"rev\":\"r1\",\"at\":\"2025-01-01T00:00:00Z\",\"content\":\"{canary}\",\"sha256\":\"{}\"}}\n", "0".repeat(64)).as_bytes());
    put(
        &d.0,
        "history/e.jsonl",
        format!(
            "{{\"rev\":\"r1\",\"at\":\"2025-01-01T00:00:00Z\",\"suppressed\":[\"{canary}\"]}}\n"
        )
        .as_bytes(),
    );
    // A checksums line that is not a line.
    std::fs::write(d.0.join("sha256sums.txt"), format!("{canary}\n")).unwrap();
    let p = plan(&d.0);
    assert!(!p.is_importable());
    let text = p.render();
    assert!(
        !text.contains(canary),
        "a byte of a refused file is repeated:\n{text}"
    );
    // Each file is refused, by its name and a rule.
    for name in [
        "pages/Bad.md",
        "pages/Latin.md",
        "users.yaml",
        "history/a.jsonl",
        "history/b.jsonl",
        "history/c.jsonl",
        "history/d.jsonl",
        "history/e.jsonl",
        "sha256sums.txt",
    ] {
        assert!(
            p.plan.refused.iter().any(|r| r.name == name),
            "{name} not refused:\n{text}"
        );
    }
}

#[test]
fn names_that_a_bundle_carries_are_escaped_and_bounded_in_the_report() {
    let evil = "Mira\u{202e}gnp.exe\u{1b}[2J";
    let long_name = "N".repeat(400);
    let d = dir("names");
    manifest(
        &d.0,
        &[
            "license: CC-BY-SA-4.0",
            "source:",
            "  name: \"Wiki\\u202eEVIL\\x1b[31m\"",
            "  url: \"https://example.org/\\u2066x\\u2069\"",
            "fidelity:",
            "  dropped:",
            "    - construct: \"hidden \\u200b thing\"",
            "badkey_\\u202e: 1",
        ],
    );
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    let mut v: serde_json::Value =
        serde_json::from_str(&record("r1", None, "2025-01-01T00:00:00Z", Some(&text(3)))).unwrap();
    set(
        &mut v,
        "author",
        serde_json::json!({"id": "m", "name": evil}),
    );
    let mut w: serde_json::Value = serde_json::from_str(&record(
        "r2",
        Some("r1"),
        "2025-01-02T00:00:00Z",
        Some(&text(3)),
    ))
    .unwrap();
    set(
        &mut w,
        "author",
        serde_json::json!({"id": "l", "name": long_name}),
    );
    set(&mut w, "type", "weird\u{202e}type");
    w.as_object_mut().unwrap().remove("content");
    w.as_object_mut().unwrap().remove("sha256");
    set(&mut w, "unknown\u{202e}member", 1);
    put(&d.0, "history/home.jsonl", format!("{v}\n{w}\n").as_bytes());
    let p = plan(&d.0);
    assert_importable(&p);
    let shown_text = p.render();
    let bad = |c: char| {
        c.is_control() && c != '\n'
            || ('\u{202a}'..='\u{202e}').contains(&c)
            || ('\u{2066}'..='\u{2069}').contains(&c)
            || c == '\u{200b}'
    };
    assert!(!shown_text.chars().any(bad), "{shown_text:?}");
    assert!(
        shown_text.contains("Mira\\u{202e}gnp.exe\\u{1b}[2J"),
        "{shown_text}"
    );
    assert!(shown_text.contains("Wiki\\u{202e}EVIL"));
    assert!(shown_text.contains("and 280 more character(s)"));
    assert!(!shown_text.contains(&"N".repeat(121)));
    // The report value is escaped as it is made: a consumer other than the terminal gets the same names.
    assert!(!p.report.to_json().to_string().contains('\u{202e}'));
    assert!(
        p.report
            .people
            .names
            .iter()
            .all(|n| n.chars().count() <= 200)
    );
    assert_eq!(p.report.not_replayed.len(), 1);
    assert!(p.report.not_replayed.keys().all(|k| !k.chars().any(bad)));
}

#[test]
fn what_the_bundle_says_that_this_build_does_not_know_is_kept_in_the_report_and_never_read() {
    let d = dir("unknown");
    manifest(
        &d.0,
        &["license: CC0-1.0", "x_future: [1, 2]", "another: yes"],
    );
    put(&d.0, "pages/Home.md", text(1).as_bytes());
    let mut v: serde_json::Value =
        serde_json::from_str(&record("r1", None, "2025-01-01T00:00:00Z", Some(&text(1)))).unwrap();
    set(&mut v, "x_engine_field", serde_json::json!({"a": 1}));
    set(&mut v, "review", serde_json::json!({"state": "ok"}));
    put(&d.0, "history/home.jsonl", format!("{v}\n").as_bytes());
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.unknown_manifest_keys, vec!["x_future", "another"]);
    assert_eq!(p.report.unknown_record_fields, vec!["x_engine_field"]);
    assert!(p.render().contains("x_engine_field"));
    // A known field of the wrong type is a refusal that says which.
    for (field, value) in [
        ("minor", serde_json::json!("yes")),
        ("summary", serde_json::json!(5)),
        ("author", serde_json::json!("ann")),
        ("author", serde_json::json!({"kind": "alien"})),
        ("suppressed", serde_json::json!("content")),
        ("sha256", serde_json::json!("ABC")),
        ("size", serde_json::json!(-1)),
    ] {
        let mut v: serde_json::Value =
            serde_json::from_str(&record("r1", None, "2025-01-01T00:00:00Z", Some(&text(1))))
                .unwrap();
        set(&mut v, field, value);
        put(&d.0, "history/home.jsonl", format!("{v}\n").as_bytes());
        let p = plan(&d.0);
        assert!(
            p.plan
                .refused
                .iter()
                .any(|r| r.name == "history/home.jsonl" && r.reason.contains(field)),
            "{field}:\n{}",
            p.render()
        );
    }
}

#[test]
fn what_the_source_hid_or_the_exporting_inklkunn_withheld_is_not_replayed_and_is_counted_apart() {
    let one = text(1);
    let hidden = serde_json::json!({
        "rev": "r2", "parent": "r1", "type": "edit", "at": "2025-01-02T00:00:00Z",
        "suppressed": ["content", "summary"], "reason": "personal-data",
    });
    let withheld = serde_json::json!({
        "rev": "r3", "parent": "r2", "type": "edit", "at": "2025-01-03T00:00:00Z",
        "author": {"id": "ann", "name": "Ann"},
        "inklkunn": {"withheld": "opaque-block", "block": "b3"},
    });
    let authorless = serde_json::json!({
        "rev": "r4", "parent": "r3", "type": "edit", "at": "2025-01-04T00:00:00Z",
        "suppressed": ["author"], "content": text(4), "sha256": sha_hex(text(4).as_bytes()),
        "author": {"name": "must not be read"},
    });
    let upload = serde_json::json!({"rev": "r5", "parent": "r4", "type": "upload", "at": "2025-01-05T00:00:00Z"});
    let protect = serde_json::json!({"rev": "r6", "parent": "r5", "type": "protect", "at": "2025-01-06T00:00:00Z"});
    let notext = serde_json::json!({"rev": "r7", "parent": "r6", "type": "edit", "at": "2025-01-07T00:00:00Z"});
    let d = dir("hidden");
    manifest(&d.0, &["license: CC0-1.0"]);
    put(&d.0, "pages/Home.md", text(4).as_bytes());
    let lines = [
        record("r1", None, "2025-01-01T00:00:00Z", Some(&one)),
        hidden.to_string(),
        withheld.to_string(),
        authorless.to_string(),
        upload.to_string(),
        protect.to_string(),
        notext.to_string(),
    ];
    put(
        &d.0,
        "history/home.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    let p = plan(&d.0);
    assert_importable(&p);
    let r = &p.report;
    assert_eq!(r.records, 7);
    assert_eq!(r.revisions, 2); // r1 and r4
    assert_eq!(
        (
            r.suppressed_content,
            r.suppressed_author,
            r.suppressed_summary
        ),
        (1, 1, 0)
    );
    assert_eq!(r.withheld.get("opaque-block"), Some(&1));
    assert_eq!(r.not_replayed.get("of type `upload`"), Some(&1));
    assert_eq!(r.not_replayed.get("of type `protect`"), Some(&1));
    assert_eq!(r.not_replayed.get("with no text"), Some(&1));
    // The author that was hidden is not carried, whatever the record also says.
    let h = p.histories.first().unwrap();
    assert!(h.revisions.get(3).unwrap().author.is_none());
    assert_eq!(h.revisions.get(3).unwrap().hidden, vec![Part::Author]);
    assert!(!p.render().contains("must not be read"));
    // A record that says its text is hidden and carries text is refused: it cannot be told from what was hidden.
    let mut v = hidden.clone();
    set(&mut v, "content", "x");
    put(
        &d.0,
        "history/home.jsonl",
        format!(
            "{}\n{v}\n",
            record("r1", None, "2025-01-01T00:00:00Z", Some(&one))
        )
        .as_bytes(),
    );
    assert_refused(&plan(&d.0), "history/home.jsonl", "carries text");
    let mut w = withheld.clone();
    set(&mut w, "parent", "r1");
    set(&mut w, "content", "x");
    put(
        &d.0,
        "history/home.jsonl",
        format!(
            "{}\n{w}\n",
            record("r1", None, "2025-01-01T00:00:00Z", Some(&one))
        )
        .as_bytes(),
    );
    assert_refused(&plan(&d.0), "history/home.jsonl", "carries text");
}

#[test]
fn a_page_that_was_renamed_and_removed_is_followed_by_its_titles_and_the_paths_the_export_says() {
    let d = dir("moves");
    manifest(&d.0, &["license: CC0-1.0"]);
    put(
        &d.0,
        "pages/New name.md",
        "---\nid: p1\ntitle: New name\n---\n\nbody\n".as_bytes(),
    );
    let t = |title: &str| format!("---\nid: p1\ntitle: {title}\n---\n\nbody\n");
    let rename = serde_json::json!({
        "rev": "r2", "parent": "r1", "type": "rename", "at": "2025-01-02T00:00:00Z",
        "from": "Old name", "to": "New name",
    });
    let lines = [
        record("r1", None, "2025-01-01T00:00:00Z", Some(&t("Old name"))),
        rename.to_string(),
        {
            let mut v: serde_json::Value = serde_json::from_str(&record(
                "r3",
                Some("r2"),
                "2025-01-03T00:00:00Z",
                Some(&t("New name")),
            ))
            .unwrap();
            set(&mut v, "summary", "rewritten");
            v.to_string()
        },
    ];
    put(
        &d.0,
        "history/p1.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    // A page that is gone: its file is named by its id, and its records say where it was.
    let gone = [
        {
            let mut v: serde_json::Value = serde_json::from_str(&record("g1", None, "2025-01-01T12:00:00Z", Some("---\nid: g\ntitle: Gone\n---\n\nx\n"))).unwrap();
            set(&mut v, "inklkunn", serde_json::json!({"path": "pages/Gone.md"}));
            v.to_string()
        },
        serde_json::json!({"rev": "g2", "parent": "g1", "type": "delete", "at": "2025-01-04T00:00:00Z", "inklkunn": {"path": "pages/Gone.md"}}).to_string(),
    ];
    put(&d.0, "history/g.jsonl", (gone.join("\n") + "\n").as_bytes());
    let p = plan(&d.0);
    assert_importable(&p);
    let paths = |file: &str| -> Vec<Option<String>> {
        p.histories
            .iter()
            .find(|h| h.file == file)
            .unwrap()
            .revisions
            .iter()
            .map(|r| r.path.clone())
            .collect()
    };
    assert_eq!(
        paths("history/p1.jsonl"),
        vec![
            Some("pages/Old name.md".to_owned()),
            Some("pages/New name.md".to_owned()),
            Some("pages/New name.md".to_owned())
        ]
    );
    assert_eq!(
        paths("history/g.jsonl"),
        vec![
            Some("pages/Gone.md".to_owned()),
            Some("pages/Gone.md".to_owned())
        ]
    );
    let gone_history = p
        .histories
        .iter()
        .find(|h| h.file == "history/g.jsonl")
        .unwrap();
    assert_eq!(gone_history.page, None);
    assert!(matches!(
        gone_history.revisions.get(1).unwrap().step,
        Step::Delete
    ));
    // The order across pages is by instant: the page that was removed is interleaved by time.
    let order: Vec<&str> = p
        .schedule
        .iter()
        .map(|&(h, v)| {
            p.histories
                .get(h)
                .unwrap()
                .revisions
                .get(v)
                .unwrap()
                .rev
                .as_str()
        })
        .collect();
    assert_eq!(order, vec!["r1", "g1", "r2", "r3", "g2"]);
    // A rename whose `to` is not where the page is, or a title that is not one, is refused naming the revision.
    let mut bad = rename.clone();
    set(&mut bad, "to", "Elsewhere");
    put(
        &d.0,
        "history/p1.jsonl",
        format!("{}\n{bad}\n{}\n", lines[0], lines[2]).as_bytes(),
    );
    assert_refused(
        &plan(&d.0),
        "history/p1.jsonl",
        "rev `r2` has a rename whose `to` is not the path the page has afterwards",
    );
    let mut bad = rename.clone();
    bad.as_object_mut().unwrap().remove("from");
    put(
        &d.0,
        "history/p1.jsonl",
        format!("{}\n{bad}\n{}\n", lines[0], lines[2]).as_bytes(),
    );
    assert_refused(
        &plan(&d.0),
        "history/p1.jsonl",
        "is a rename without a `from` and a `to`",
    );
}

#[test]
fn a_page_that_has_no_replayable_revision_comes_in_with_the_first_change_and_a_removed_page_does_not_matter()
 {
    let d = small("nohist");
    put(
        &d.0,
        "pages/Second.md",
        b"---\nid: second\ntitle: Second\n---\n\nx\n",
    );
    put(
        &d.0,
        "history/home.jsonl",
        format!(
            "{}\n",
            record("r1", None, "2025-01-01T00:00:00Z", Some(HOME))
        )
        .as_bytes(),
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_importable(&p);
    // `Home`'s last text is its file; `Second` has no history.
    assert_eq!(p.report.pages_without_history, vec!["pages/Second.md"]);
    assert!(p.report.final_state_pages.is_empty());
    // Change `Home`'s file: it now needs the change at the end.
    put(
        &d.0,
        "pages/Home.md",
        b"---\nid: home\ntitle: Home\n---\n\nEdited after.\n",
    );
    seal(&d.0);
    assert_eq!(plan(&d.0).report.final_state_pages, vec!["pages/Home.md"]);
}

#[test]
fn the_plan_reads_and_judges_and_writes_nothing_at_all() {
    // A bundle that nothing may be written into: every directory and file read-only, and the plan still comes out whole.
    let d = copy_of("readonly", "mediawiki/bundle");
    fn lock(path: &Path) {
        let meta = std::fs::symlink_metadata(path).unwrap();
        if meta.is_dir() {
            for e in std::fs::read_dir(path).unwrap() {
                lock(&e.unwrap().path());
            }
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555)).unwrap();
        } else {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o444)).unwrap();
        }
    }
    let snapshot = |root: &Path| -> Vec<(String, String)> {
        files_of(root)
            .into_iter()
            .map(|f| {
                let bytes = std::fs::read(root.join(&f)).unwrap();
                (f, sha_hex(&bytes))
            })
            .collect()
    };
    let before = snapshot(&d.0);
    lock(&d.0);
    let options = BundleOptions {
        bundle_license: Some("CC0-1.0".to_owned()),
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &options);
    assert_importable(&p);
    assert_eq!(p.report.revisions, 7);
    make_writable(&d.0);
    assert_eq!(snapshot(&d.0), before);
}

#[test]
fn the_manifest_that_cannot_be_read_stops_the_plan_with_a_refusal_and_nothing_judged() {
    for (tag, body) in [
        ("man-not-map", "- a\n- b\n"),
        ("man-yaml", "format: [unclosed\n"),
        ("man-format", "format: other\nversion: '1.0'\n"),
        (
            "man-anchor",
            "format: &a portable-wiki-bundle\nversion: '1.0'\n",
        ),
    ] {
        let d = dir(tag);
        put(&d.0, "wiki-bundle.yaml", body.as_bytes());
        put(&d.0, "pages/Home.md", HOME.as_bytes());
        let p = plan(&d.0);
        assert!(!p.is_importable(), "{tag}");
        assert!(
            p.plan.refused.iter().any(|r| r.name == "wiki-bundle.yaml"),
            "{tag}: {}",
            p.render()
        );
        assert!(p.plan.files.is_empty(), "{tag}: nothing else was walked");
    }
}

#[test]
fn the_knowledge_directory_is_read_only_where_the_manifest_declares_the_extension() {
    let claim = "kind: observation\nlifecycle: adopted\ncommunity_status:\n  status: accepted\nexpressions: []\nrelations: []\nsources: []\n";
    let d = dir("knowledge");
    manifest(&d.0, &["license: CC0-1.0", "checksums: sha256sums.txt"]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    put(
        &d.0,
        "knowledge/claims/01930000-0000-7000-8000-000000000001.yaml",
        claim.as_bytes(),
    );
    put(&d.0, "knowledge/.keep", b"");
    seal(&d.0);
    // Not declared: left out, as the folder import leaves it.
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.report.knowledge_files, 0);
    assert!(p.report.left_out.iter().any(|l| l.what == "knowledge/"));
    // Declared: each file is read by the knowledge reader, and one that does not read refuses by name and rule.
    manifest(
        &d.0,
        &[
            "license: CC0-1.0",
            "checksums: sha256sums.txt",
            "extensions_used:",
            "  - name: inklkunn-knowledge",
            "    version: \"1\"",
        ],
    );
    assert_eq!(plan(&d.0).report.knowledge_files, 1);
    put(
        &d.0,
        "knowledge/claims/01930000-0000-7000-8000-000000000002.yaml",
        b"kind: [unclosed\n",
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert!(
        p.plan
            .refused
            .iter()
            .any(|r| r.name == "knowledge/claims/01930000-0000-7000-8000-000000000002.yaml"),
        "{}",
        p.render()
    );
}

#[test]
fn the_report_is_a_value_that_can_be_kept_and_says_what_it_is_made_of() {
    let options = BundleOptions {
        bundle_license: Some("CC-BY-SA-4.0".to_owned()),
        ..BundleOptions::default()
    };
    let p = plan_with(&fixture("mediawiki/bundle"), &options);
    let again = plan_with(&fixture("mediawiki/bundle"), &options);
    // The same bundle gives the same report, equal and clonable, and the same JSON.
    assert_eq!(p.report, again.report);
    assert_eq!(p.report.clone(), p.report);
    let json = p.report.to_json();
    assert_eq!(json, again.report.to_json());
    let at = |pointer: &str| json.pointer(pointer).cloned().unwrap();
    assert_eq!(at("/fidelity/declared"), 3);
    assert_eq!(at("/fidelity/found"), "history");
    assert_eq!(at("/license/from"), "declared-by-admin");
    assert_eq!(at("/checksums/state"), "verified");
    assert_eq!(at("/history/revisions"), 7);
    assert_eq!(at("/history/suppressed/content"), 1);
    assert_eq!(at("/people/total"), 3);
    assert_eq!(at("/links/unmapped_prefixes/wikipedia"), 1);
    // The checksums' absence is part of the value.
    let none = small("value-none");
    manifest(&none.0, &["license: CC0-1.0"]);
    assert_eq!(
        plan(&none.0)
            .report
            .to_json()
            .pointer("/checksums/state")
            .cloned()
            .unwrap(),
        "none-in-the-bundle"
    );
}

#[test]
fn the_history_of_a_page_the_bundle_does_not_have_and_cannot_place_is_left_out_and_said() {
    let d = small("ghost");
    // Records of a page with no file and no path of their own: nothing says where the replay would put it.
    put(
        &d.0,
        "history/ghost.jsonl",
        (record(
            "g1",
            None,
            "2025-01-01T00:00:00Z",
            Some("---\nid: ghost\ntitle: Ghost\n---\n\nx\n"),
        ) + "\n")
            .as_bytes(),
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_importable(&p);
    assert!(p.histories.is_empty());
    assert_eq!(p.report.revisions, 0);
    assert!(
        p.report
            .left_out
            .iter()
            .any(|l| l.what == "history files of pages the bundle does not have" && l.files == 1),
        "{}",
        p.render()
    );
}

/// Acceptance criterion 2's *sidecars' licences*, shown by a bundle written by hand (neither converter writes one): each
/// attachment's own licence is in its sidecar, comes in with it as it is, and the report says which licences came.
#[test]
fn a_hand_written_bundle_shows_the_licence_of_each_sidecar() {
    let p = plan(&fixture("sidecars/bundle"));
    assert_importable(&p);
    let r = &p.report;
    assert_eq!(r.license.as_deref(), Some("CC-BY-SA-4.0"));
    assert_eq!(r.license_from, Some(LicenseFrom::Manifest));
    assert_eq!((r.pages, r.attachments, r.sidecars), (1, 2, 2));
    assert_eq!(
        r.attachment_licenses.iter().collect::<Vec<_>>(),
        [(&"CC-BY-4.0".to_owned(), &1), (&"CC0-1.0".to_owned(), &1)]
    );
    assert!(r.incompatible.is_empty());
    // One change: nothing has a history, so the page and both attachments with their sidecars are the first change.
    assert_eq!(p.plan.files.len(), 5);
    assert_eq!(p.first_pages, ["pages/Orchard Plan.md"]);
    assert!(p.schedule.is_empty());
    assert!(
        p.render().contains(
            "attachment licences (each in its sidecar, kept as it is): CC-BY-4.0 (1), CC0-1.0 (1)"
        ),
        "{}",
        p.render()
    );
    // A sidecar's licence that the wiki's cannot take refuses it by name, and `--leave-out-incompatible` leaves the pair out.
    let d = copy_of("sidecar-nc", "sidecars/bundle");
    let sidecar = d.0.join("attachments/notes.txt.meta.yaml");
    let text = std::fs::read_to_string(&sidecar).unwrap();
    std::fs::write(&sidecar, text.replace("CC-BY-4.0", "CC-BY-NC-4.0")).unwrap();
    // The checksums name the sidecar, so they are made again.
    seal(&d.0);
    assert_refused(&plan(&d.0), "attachments/notes.txt", "CC-BY-NC-4.0");
    let given = BundleOptions {
        leave_out_incompatible: true,
        ..BundleOptions::default()
    };
    let left = plan_with(&d.0, &given);
    assert_importable(&left);
    assert_eq!(left.plan.files.len(), 3);
}

/// The converters write their report after their checksums, so the checksums do not name it. A file the import does not read does
/// not refuse (the I-T1 review's ruling on I-T2): wiki-commons' default output imports without `--no-report`.
#[test]
fn a_converters_own_report_beside_the_bundle_is_left_out_and_said_and_refuses_nothing() {
    for (name, fixture_dir, license) in [
        ("report-mw", "mediawiki/bundle", Some("CC-BY-SA-4.0")),
        ("report-bs", "bookstack/bundle", Some("CC0-1.0")),
    ] {
        let d = copy_of(name, fixture_dir);
        put(
            &d.0,
            "import-report.md",
            b"# Import report\n\nwritten after the checksums\n",
        );
        let given = BundleOptions {
            bundle_license: license.map(str::to_owned),
            ..BundleOptions::default()
        };
        let p = plan_with(&d.0, &given);
        assert_importable(&p);
        assert!(matches!(p.report.checksums, Checksums::Verified { .. }));
        let left = p
            .report
            .left_out
            .iter()
            .find(|l| l.what == "import-report.md")
            .unwrap_or_else(|| panic!("not said:\n{}", p.render()));
        assert!(left.why.contains("converter"), "{}", left.why);
    }
}

/// **A page's own order is its history, whatever the order of the paths it passes through** (RFC 007 D-2): two revisions of one page with the
/// same instant, the first at a path that sorts after the path of the second (a creation at `Zed` and its move to `Alpha`), are replayed in
/// the file's order, never in the order of the paths.
#[test]
fn the_order_of_one_pages_revisions_survives_a_tie_in_time_across_a_rename() {
    let d = dir("tie-rename");
    manifest(&d.0, &["license: CC0-1.0"]);
    put(
        &d.0,
        "pages/Alpha.md",
        b"---\nid: p1\ntitle: Alpha\n---\n\nbody\n",
    );
    let created = serde_json::json!({
        "rev": "r1", "parent": null, "type": "create", "at": "2025-01-01T00:00:00Z",
        "content": "---\nid: p1\ntitle: Zed\n---\n\nbody\n",
    });
    let moved = serde_json::json!({
        "rev": "r2", "parent": "r1", "type": "rename", "at": "2025-01-01T00:00:00Z",
        "from": "Zed", "to": "Alpha",
    });
    put(
        &d.0,
        "history/p1.jsonl",
        format!("{created}\n{moved}\n").as_bytes(),
    );
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.schedule, [(0, 0), (0, 1)], "{}", p.render());
    // Two pages with the same instants are in the order of their pages' paths, and each keeps its own.
    put(
        &d.0,
        "pages/Beta.md",
        b"---\nid: p2\ntitle: Beta\n---\n\nb\n",
    );
    let other = serde_json::json!({
        "rev": "q1", "parent": null, "type": "create", "at": "2025-01-01T00:00:00Z",
        "content": "---\nid: p2\ntitle: Beta\n---\n\nb\n",
    });
    put(&d.0, "history/p2.jsonl", format!("{other}\n").as_bytes());
    let p = plan(&d.0);
    assert_importable(&p);
    let order: Vec<&str> = p
        .schedule
        .iter()
        .map(|(h, r)| {
            p.histories
                .get(*h)
                .and_then(|x| x.revisions.get(*r))
                .map(|v| v.rev.as_str())
                .unwrap()
        })
        .collect();
    assert_eq!(
        order,
        ["r1", "r2", "q1"],
        "the page Alpha, then the page Beta"
    );
}
