//! The corrections of the I-T2 / I-T3 review (the V round): what the plan judges of a history besides its order and its texts.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::*;

/// A record that is not a text one: `kind` (`delete`) or an edit with the page's path named by the exporting inklkunn.
fn at_path(
    rev: &str,
    parent: Option<&str>,
    kind: &str,
    at: &str,
    path: &str,
    content: Option<&str>,
) -> String {
    let mut v = serde_json::json!({
        "rev": rev,
        "parent": parent,
        "type": kind,
        "at": at,
        "author": {"name": "Ann"},
        "inklkunn": {"path": path},
    });
    if let Some(c) = content {
        set(&mut v, "content", c);
        set(&mut v, "sha256", sha_hex(c.as_bytes()));
    }
    v.to_string()
}

fn gone_text(license: Option<&str>) -> String {
    match license {
        Some(l) => format!("---\nid: gone\ntitle: Gone\nlicense: {l}\n---\n\nonce\n"),
        None => "---\nid: gone\ntitle: Gone\n---\n\nonce\n".to_owned(),
    }
}

/// A bundle whose page `Home` has no history and whose history file `history/gone.jsonl` is of a page **the bundle does not have**
/// (removed since), made and removed.
fn with_a_removed_page(tag: &str, created: &str) -> Dir {
    let d = dir(tag);
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    let lines = [
        at_path(
            "g1",
            None,
            "create",
            "2025-01-01T00:00:00Z",
            "pages/Gone.md",
            Some(created),
        ),
        at_path(
            "g2",
            Some("g1"),
            "delete",
            "2025-01-02T00:00:00Z",
            "pages/Gone.md",
            None,
        ),
    ];
    put(
        &d.0,
        "history/gone.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    seal(&d.0);
    d
}

fn leaving_out() -> BundleOptions {
    BundleOptions {
        leave_out_incompatible: true,
        ..BundleOptions::default()
    }
}

// ---------------------------------------------------------------------------------------------------------------- V-1

#[test]
fn the_history_of_a_removed_page_is_judged_for_its_licences_as_a_pages_is() {
    // A compatible history of a removed page comes in.
    let d = with_a_removed_page("v1-ok", &gone_text(Some("CC0-1.0")));
    let p = plan(&d.0);
    assert_importable(&p);
    assert_eq!(p.histories.len(), 1);
    // The same history, whose text declares a licence the wiki cannot take, refuses, naming the history file and the revision.
    let d = with_a_removed_page("v1-text", &gone_text(Some("CC-BY-NC-4.0")));
    let p = plan(&d.0);
    assert_refused(&p, "history/gone.jsonl", "CC-BY-NC-4.0");
    assert!(
        p.plan
            .refused
            .iter()
            .any(|r| r.reason.contains("at revision `g1`")),
        "{}",
        p.render()
    );
    assert_eq!(p.report.incompatible.len(), 1);
    assert_eq!(
        p.report.incompatible.first().map(|i| i.kind),
        Some(FileKind::History)
    );
    // With the flag it is left out, and named; the page the bundle has comes in.
    let p = plan_with(&d.0, &leaving_out());
    assert_importable(&p);
    assert!(
        p.histories.is_empty() && p.schedule.is_empty(),
        "{}",
        p.render()
    );
    let paths: Vec<&str> = p.plan.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["pages/Home.md"]);
    assert!(p.report.incompatible_left_out);
    assert!(
        p.render()
            .contains("history of a page not in the bundle history/gone.jsonl: CC-BY-NC-4.0"),
        "{}",
        p.render()
    );
    assert!(
        p.report
            .left_out
            .iter()
            .any(|l| l.what == "files with an incompatible licence" && l.files == 1)
    );
    // A record's own `license` member counts the same, for a history whose texts say none.
    let d = with_a_removed_page("v1-member", &gone_text(None));
    let text = std::fs::read_to_string(d.0.join("history/gone.jsonl")).unwrap();
    let mut lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    set(lines.get_mut(1).unwrap(), "license", "CC-BY-ND-4.0");
    let out: Vec<String> = lines.iter().map(ToString::to_string).collect();
    put(
        &d.0,
        "history/gone.jsonl",
        (out.join("\n") + "\n").as_bytes(),
    );
    seal(&d.0);
    assert_refused(&plan(&d.0), "history/gone.jsonl", "CC-BY-ND-4.0");
    // Under `--no-history` no history is read, so none is judged.
    let no_history = BundleOptions {
        no_history: true,
        ..BundleOptions::default()
    };
    assert_importable(&plan_with(&d.0, &no_history));
}

#[test]
fn a_pages_history_with_a_licence_the_wiki_cannot_take_leaves_out_the_page_and_the_history_file_once()
 {
    // The page of the bundle: its history's licence names the page, not the file, as before.
    let d = dir("v1-page");
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
    assert_eq!(p.report.incompatible.len(), 1);
    assert_eq!(
        p.report
            .incompatible
            .first()
            .map(|i| (i.kind, i.path.as_str())),
        Some((FileKind::Page, "pages/Home.md"))
    );
}

// ---------------------------------------------------------------------------------------------------------------- V-3

/// A history of `n` edits of a page whose text starts at `base`, each by a patch that changes the first line.
fn patched_history(n: usize, base: &str) -> (String, String) {
    let first = |text: &str| text.lines().nth(5).unwrap_or_default().to_owned();
    let mut lines = vec![record("r0", None, "2025-01-01T00:00:00Z", Some(base))];
    let mut before = base.to_owned();
    for i in 1..=n {
        let line = first(&before);
        let new_line = format!("Version {i}.");
        let after = before.replacen(&line, &new_line, 1);
        let patch = format!("@@ -6 +6 @@\n-{line}\n+{new_line}\n");
        let mut v = serde_json::json!({
            "rev": format!("r{i}"),
            "parent": format!("r{}", i - 1),
            "type": "edit",
            "at": format!("2025-01-02T00:00:{:02}Z", i.min(59)),
            "patch": patch,
            "sha256": sha_hex(after.as_bytes()),
        });
        set(&mut v, "author", serde_json::json!({"name": "Ann"}));
        lines.push(v.to_string());
        before = after;
    }
    (lines.join("\n") + "\n", before)
}

#[test]
fn the_revision_cap_is_applied_before_any_patch_is_applied() {
    // A history of patches whose later records are broken (a patch with a wrong digest): had the plan applied patches first, the
    // refusal would name that record and the cap would never be said. The cap is counted from what the records say, so it is.
    let base = text(0);
    let (history, _) = patched_history(20, &base);
    // Break the 15th record's digest.
    let mut lines: Vec<String> = history.lines().map(str::to_owned).collect();
    let broken = lines.get(15).unwrap().replace(
        &lines
            .get(15)
            .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .and_then(|v| v.get("sha256").and_then(|d| d.as_str()).map(str::to_owned))
            .unwrap(),
        &"0".repeat(64),
    );
    *lines.get_mut(15).unwrap() = broken;
    let d = dir("v3-order");
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", text(20).as_bytes());
    put(
        &d.0,
        "history/home.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    seal(&d.0);
    // Over the cap: the count is the whole answer.
    let capped = BundleOptions {
        max_revisions: 10,
        ..BundleOptions::default()
    };
    let p = plan_with(&d.0, &capped);
    assert_refused(&p, "history/", "would make 21 blocks, and the cap is 10");
    assert_eq!(
        p.plan.refused.len(),
        1,
        "nothing was judged, so nothing else is said:\n{}",
        p.render()
    );
    // Under the cap the same file is judged, and the broken record is what is said.
    let p = plan(&d.0);
    assert_refused(
        &p,
        "history/home.jsonl",
        "rev `r15` has a `sha256` that is not the text's",
    );
    // The cap counts the replayed revisions and not the records of types that are not replayed.
    let cap_exact = BundleOptions {
        max_revisions: 21,
        ..BundleOptions::default()
    };
    assert!(
        !plan_with(&d.0, &cap_exact)
            .plan
            .refused
            .iter()
            .any(|r| r.name == "history/"),
    );
}

#[test]
fn a_planted_history_over_the_cap_is_refused_in_the_time_it_takes_to_read_it() {
    let base = text(0);
    let (history, _) = patched_history(20_000, &base);
    let d = dir("v3-time");
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", text(1).as_bytes());
    put(&d.0, "history/home.jsonl", history.as_bytes());
    seal(&d.0);
    let capped = BundleOptions {
        max_revisions: 10,
        ..BundleOptions::default()
    };
    let started = std::time::Instant::now();
    let p = plan_with(&d.0, &capped);
    let took = started.elapsed();
    assert_refused(&p, "history/", "the cap is 10");
    assert!(
        took < std::time::Duration::from_secs(5),
        "the refusal took {took:?}"
    );
}

#[test]
fn the_bytes_the_patches_of_a_bundle_handle_are_bounded_apart_from_the_revision_cap() {
    // A text of 40 KiB patched many times: each patch spends the text it is applied to. With the history's byte cap at 64 KiB the
    // patches may handle four times that, 256 KiB: six of them pass, ten do not.
    let big = format!(
        "---\nid: home\ntitle: Home\n---\n\nVersion 0.\n{}",
        "a line of the page\n".repeat(2_100)
    );
    let options = BundleOptions {
        max_history_bytes: 64 << 10,
        ..BundleOptions::default()
    };
    for (n, allowed) in [(5, true), (10, false)] {
        let (history, last) = patched_history(n, &big);
        let d = dir(&format!("v3-bytes-{n}"));
        manifest(
            &d.0,
            &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
        );
        put(&d.0, "pages/Home.md", last.as_bytes());
        put(&d.0, "history/home.jsonl", history.as_bytes());
        seal(&d.0);
        let p = plan_with(&d.0, &options);
        if allowed {
            assert_importable(&p);
        } else {
            assert_refused(
                &p,
                "history/home.jsonl",
                "would take the patches of the bundle past what the plan applies in all",
            );
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------- V-4, V-7

/// A history file of one page at `path`: created at `from`, removed at `until` (open when `None`).
fn held(rev: &str, path: &str, from: &str, until: Option<&str>) -> String {
    let title = path
        .strip_prefix("pages/")
        .and_then(|p| p.strip_suffix(".md"))
        .unwrap();
    let body = format!("---\nid: {rev}\ntitle: {title}\n---\n\n{rev}\n");
    let mut lines = vec![at_path(
        &format!("{rev}1"),
        None,
        "create",
        from,
        path,
        Some(&body),
    )];
    if let Some(until) = until {
        lines.push(at_path(
            &format!("{rev}2"),
            Some(&format!("{rev}1")),
            "delete",
            until,
            path,
            None,
        ));
    }
    lines.join("\n") + "\n"
}

fn histories_only(tag: &str, files: &[(&str, String)]) -> Dir {
    let d = dir(tag);
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    for (name, body) in files {
        put(&d.0, &format!("history/{name}.jsonl"), body.as_bytes());
    }
    seal(&d.0);
    d
}

#[test]
fn two_paths_that_differ_only_in_case_are_refused_when_held_at_the_same_moment() {
    // A removed page's revision at `pages/HOME.md` and the bundle's own `pages/Home.md` (a page with no history is there from the
    // first change to the end, so the removed page's whole life is while it is there).
    let d = histories_only(
        "v4-file",
        &[(
            "aa",
            held(
                "aa",
                "pages/HOME.md",
                "2025-01-01T00:00:00Z",
                Some("2025-01-02T00:00:00Z"),
            ),
        )],
    );
    let p = plan(&d.0);
    assert_refused(&p, "history/aa.jsonl", "ignores case");
    assert!(
        p.plan
            .refused
            .iter()
            .any(|r| r.reason.contains("pages/HOME.md") && r.reason.contains("pages/Home.md")),
        "{}",
        p.render()
    );
    // Two histories, one path in two cases, alive together: one begins while the other holds its path.
    let two = |tag: &str, second_from: &str, second_until: &str| {
        histories_only(
            tag,
            &[
                (
                    "aa",
                    held(
                        "aa",
                        "pages/Same.md",
                        "2025-01-01T00:00:00Z",
                        Some("2025-01-03T00:00:00Z"),
                    ),
                ),
                (
                    "bb",
                    held("bb", "pages/SAME.md", second_from, Some(second_until)),
                ),
            ],
        )
    };
    let d = two("v4-two", "2025-01-02T00:00:00Z", "2025-01-04T00:00:00Z");
    assert_refused(&plan(&d.0), "history/bb.jsonl", "ignores case");
    // ... one begins the second the other ends (the order of two acts of one second is not the file's to say, as V-7 has it).
    let d = two("v4-touch", "2025-01-03T00:00:00Z", "2025-01-04T00:00:00Z");
    assert_refused(&plan(&d.0), "history/bb.jsonl", "ignores case");
    // ... one is still there at the end, so a later one in the other case is alive with it.
    let d = histories_only(
        "v4-open",
        &[
            (
                "aa",
                held("aa", "pages/Same.md", "2025-01-01T00:00:00Z", None),
            ),
            (
                "bb",
                held(
                    "bb",
                    "pages/SAME.md",
                    "2025-02-01T00:00:00Z",
                    Some("2025-02-02T00:00:00Z"),
                ),
            ),
        ],
    );
    assert_refused(&plan(&d.0), "history/bb.jsonl", "ignores case");
    // Never alive together: the first is over before the second begins (a page removed, another made at the path in other case).
    let d = two("v4-apart", "2025-01-03T00:00:01Z", "2025-01-04T00:00:00Z");
    assert_importable(&plan(&d.0));
    // A path that is the same, as written, is no collision: the same page again, later.
    let d = histories_only(
        "v4-same",
        &[
            (
                "aa",
                held(
                    "aa",
                    "pages/Same.md",
                    "2025-01-01T00:00:00Z",
                    Some("2025-01-02T00:00:00Z"),
                ),
            ),
            (
                "bb",
                held(
                    "bb",
                    "pages/Same.md",
                    "2025-01-03T00:00:00Z",
                    Some("2025-01-04T00:00:00Z"),
                ),
            ),
        ],
    );
    assert_importable(&plan(&d.0));
}

/// A bundle whose page `p1` has one history of a create at the first of `titles` and a rename to each of the next, a day apart; its
/// file is at the last.
fn moved_through(tag: &str, titles: &[&str]) -> Dir {
    let d = dir(tag);
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    let text = |title: &str| format!("---\nid: p1\ntitle: {title}\n---\n\nbody\n");
    let last = titles.last().unwrap();
    put(&d.0, &format!("pages/{last}.md"), text(last).as_bytes());
    let mut lines = vec![record(
        "r1",
        None,
        "2025-01-01T00:00:00Z",
        Some(&text(titles.first().unwrap())),
    )];
    for (n, pair) in titles.windows(2).enumerate() {
        lines.push(
            serde_json::json!({
                "rev": format!("r{}", n + 2),
                "parent": format!("r{}", n + 1),
                "type": "rename",
                "at": format!("2025-01-{:02}T00:00:00Z", n + 2),
                "from": pair[0],
                "to": pair[1],
            })
            .to_string(),
        );
    }
    put(
        &d.0,
        "history/p1.jsonl",
        (lines.join("\n") + "\n").as_bytes(),
    );
    seal(&d.0);
    d
}

#[test]
fn a_rename_that_changes_only_the_case_is_a_rename_and_imports() {
    // `Foo bar` → `Foo Bar`: the page holds the first until the move and the second from it.
    let p = plan(&moved_through("w1-one", &["Foo bar", "Foo Bar"]).0);
    assert_importable(&p);
    // ... and back again, and again: the places of one page never overlap.
    let p = plan(&moved_through("w1-back", &["Foo bar", "Foo Bar", "Foo bar", "FOO BAR"]).0);
    assert_importable(&p);
    // Another page that holds a path of the other case while the page is at one is alive with it.
    let d = moved_through("w1-other", &["Foo bar", "Foo Bar"]);
    put(
        &d.0,
        "history/zz.jsonl",
        held("zz", "pages/FOO BAR.md", "2025-01-01T12:00:00Z", None).as_bytes(),
    );
    seal(&d.0);
    let p = plan(&d.0);
    assert_refused(&p, "history/zz.jsonl", "ignores case");
    // ... but not before the page was made.
    let d = moved_through("w1-after", &["Foo bar", "Foo Bar"]);
    put(
        &d.0,
        "history/zz.jsonl",
        held(
            "zz",
            "pages/FOO BAR.md",
            "2024-12-01T00:00:00Z",
            Some("2024-12-02T00:00:00Z"),
        )
        .as_bytes(),
    );
    seal(&d.0);
    assert_importable(&plan(&d.0));
}

#[test]
fn two_histories_that_hold_one_path_at_overlapping_times_are_refused_naming_both() {
    let a = held(
        "aa",
        "pages/Same.md",
        "2025-01-01T00:00:00Z",
        Some("2025-01-05T00:00:00Z"),
    );
    // Begins while the first holds it.
    let d = histories_only(
        "v7-overlap",
        &[
            ("aa", a.clone()),
            (
                "bb",
                held(
                    "bb",
                    "pages/Same.md",
                    "2025-01-03T00:00:00Z",
                    Some("2025-01-04T00:00:00Z"),
                ),
            ),
        ],
    );
    let p = plan(&d.0);
    assert_refused(
        &p,
        "history/bb.jsonl",
        "overlap those of `history/aa.jsonl`",
    );
    assert!(p.render().contains("pages/Same.md"), "{}", p.render());
    // Begins the second the first ends: the order of two acts of one second is not the file's to say.
    let d = histories_only(
        "v7-touch",
        &[
            ("aa", a.clone()),
            (
                "bb",
                held(
                    "bb",
                    "pages/Same.md",
                    "2025-01-05T00:00:00Z",
                    Some("2025-01-06T00:00:00Z"),
                ),
            ),
        ],
    );
    assert_refused(&plan(&d.0), "history/bb.jsonl", "overlap");
    // Begins after the first ended: no overlap.
    let d = histories_only(
        "v7-after",
        &[
            ("aa", a.clone()),
            (
                "bb",
                held(
                    "bb",
                    "pages/Same.md",
                    "2025-01-05T00:00:01Z",
                    Some("2025-01-06T00:00:00Z"),
                ),
            ),
        ],
    );
    assert_importable(&plan(&d.0));
    // A first that is still there at the end holds the path for ever: a later one overlaps it.
    let d = histories_only(
        "v7-open",
        &[
            (
                "aa",
                held("aa", "pages/Same.md", "2025-01-01T00:00:00Z", None),
            ),
            (
                "bb",
                held(
                    "bb",
                    "pages/Same.md",
                    "2025-02-01T00:00:00Z",
                    Some("2025-02-02T00:00:00Z"),
                ),
            ),
        ],
    );
    assert_refused(&plan(&d.0), "history/bb.jsonl", "overlap");
    // Another path is another page.
    let d = histories_only(
        "v7-other",
        &[
            ("aa", a),
            (
                "bb",
                held(
                    "bb",
                    "pages/Other.md",
                    "2025-01-03T00:00:00Z",
                    Some("2025-01-04T00:00:00Z"),
                ),
            ),
        ],
    );
    assert_importable(&plan(&d.0));
}

// ---------------------------------------------------------------------------------------------------------------- V-5

#[test]
fn a_time_after_the_year_9999_is_refused_in_the_plan_and_the_last_second_of_it_is_not() {
    let late = |at: &str| with_history("v5", &[record("r1", None, at, Some(&text(1)))]);
    // Ten thousand, by the clock of the place: 9999-12-31T23:59:59-05:00 is 10000-01-01T04:59:59Z.
    let p = plan(&late("9999-12-31T23:59:59-05:00").0);
    assert_refused(
        &p,
        "history/home.jsonl",
        "rev `r1` has an `at` after the year 9999",
    );
    assert_importable(&plan(&late("9999-12-31T23:59:59Z").0));
    assert_importable(&plan(&late("9999-12-31T23:59:59+09:00").0));
}

// ---------------------------------------------------------------------------------------------------------------- V-6

#[test]
fn the_bundles_identity_is_the_checksums_files_digest_or_a_digest_of_the_files_read() {
    // Verified: the digest of the file of checksums, and a changed file changes it.
    let d = small("v6-sums");
    let p = plan(&d.0);
    assert_eq!(
        p.bundle_digest,
        Sha256::digest(std::fs::read(d.0.join("sha256sums.txt")).unwrap()).as_slice()
    );
    // Two bundles with one manifest and different pages are two bundles, verified or not.
    let other = small("v6-sums-other");
    put(
        &other.0,
        "pages/Home.md",
        "---\nid: home\ntitle: Home\n---\n\nAnother.\n".as_bytes(),
    );
    seal(&other.0);
    assert_eq!(
        std::fs::read(d.0.join("wiki-bundle.yaml")).unwrap(),
        std::fs::read(other.0.join("wiki-bundle.yaml")).unwrap()
    );
    assert_ne!(plan(&other.0).bundle_digest, p.bundle_digest);
    // No checksums: a digest over the sorted (path, digest) list of what is read: pages, attachments, sidecars, the history.
    let bare = |tag: &str, page: &str, attachment: &[u8]| {
        let d = dir(tag);
        manifest(&d.0, &["license: CC-BY-SA-4.0"]);
        put(&d.0, "pages/Home.md", page.as_bytes());
        put(&d.0, "attachments/a.bin", attachment);
        d
    };
    let one = bare("v6-bare-1", HOME, b"one");
    let same = bare("v6-bare-same", HOME, b"one");
    let page = bare("v6-bare-page", &HOME.replace("Hello", "Hullo"), b"one");
    let file = bare("v6-bare-file", HOME, b"two");
    let digest = |d: &Dir| plan(&d.0).bundle_digest;
    assert_eq!(digest(&one), digest(&same));
    assert_ne!(digest(&one), digest(&page));
    assert_ne!(
        digest(&one),
        digest(&file),
        "an attachment's bytes are the bundle's"
    );
    assert_ne!(digest(&one), [0; 32]);
    // The manifest alone no longer says the bundle: the three above have one manifest.
    assert_ne!(
        digest(&one),
        Sha256::digest(std::fs::read(one.0.join("wiki-bundle.yaml")).unwrap()).as_slice()
    );
    // Order and place of files do not matter, their names and digests do.
    put(&one.0, "attachments/b.bin", b"x");
    assert_ne!(digest(&one), digest(&same));
}

// ---------------------------------------------------------------------------------------------------------------- V-10

#[test]
fn a_name_that_is_not_an_spdx_id_is_refused_with_the_id_it_probably_means_and_never_taken_for_it() {
    use licence::{did_you_mean, goes_into};
    for (name, meant) in [
        ("CC0", Some("CC0-1.0")),
        ("cc0", Some("CC0-1.0")),
        ("CC-0", Some("CC0-1.0")),
        ("CC0-1", Some("CC0-1.0")),
        ("CC-BY", Some("CC-BY-4.0")),
        ("CC-BY-SA", Some("CC-BY-SA-4.0")),
        ("cc by sa 3.0", Some("CC-BY-SA-3.0")),
        ("CC BY 2.5", Some("CC-BY-2.5")),
        ("CC-BY-SA-4", Some("CC-BY-SA-4.0")),
        ("CCBY", Some("CC-BY-4.0")),
        // A known id is no near miss; a name that is close to nothing, or to a licence no wiki may take, names nothing.
        ("CC0-1.0", None),
        ("CC-BY-SA-4.0", None),
        ("CC-BY-NC-4.0", None),
        ("CC-BY-ND-4.0", None),
        ("CC-BY-NC-SA", None),
        ("GFDL", None),
        ("MIT", None),
        ("CC-BY-9.9", None),
        ("", None),
    ] {
        assert_eq!(did_you_mean(name).as_deref(), meant, "{name:?}");
    }
    // **Never accepted as that id**: the name does not go into any wiki.
    assert!(!goes_into("CC0", "CC-BY-SA-4.0"));
    assert!(!goes_into("CC-BY-SA", "CC-BY-SA-4.0"));
    assert!(!licence::may_be_a_default("CC0"));
    // In the manifest.
    let d = dir("v10-manifest");
    manifest(&d.0, &["license: CC0"]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    let p = plan(&d.0);
    assert_refused(
        &p,
        "wiki-bundle.yaml",
        "(not an SPDX id; did you mean CC0-1.0?)",
    );
    // In the option.
    let d = dir("v10-option");
    manifest(&d.0, &[]);
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    let given = BundleOptions {
        bundle_license: Some("CC-BY-SA".to_owned()),
        ..BundleOptions::default()
    };
    assert_refused(
        &plan_with(&d.0, &given),
        "--bundle-license",
        "did you mean CC-BY-SA-4.0?",
    );
    // In a page, in a sidecar and in a revision of a history.
    let d = dir("v10-files");
    manifest(
        &d.0,
        &["license: CC-BY-SA-4.0", "checksums: sha256sums.txt"],
    );
    put(&d.0, "pages/Home.md", HOME.as_bytes());
    put(
        &d.0,
        "pages/Near.md",
        b"---\nid: near\ntitle: Near\nlicense: CC0\n---\n\nnear\n",
    );
    put(&d.0, "attachments/a.png", b"png");
    put(&d.0, "attachments/a.png.meta.yaml", b"license: CC-BY\n");
    seal(&d.0);
    let p = plan(&d.0);
    assert_refused(&p, "pages/Near.md", "did you mean CC0-1.0?");
    assert_refused(&p, "attachments/a.png", "did you mean CC-BY-4.0?");
    assert!(
        p.render()
            .contains("(not an SPDX id; did you mean CC0-1.0?)"),
        "{}",
        p.render()
    );
    // Left out under the flag, and still said; the id it resembles is never put on it.
    let p = plan_with(&d.0, &leaving_out());
    assert_importable(&p);
    assert!(
        p.report
            .incompatible
            .iter()
            .any(|i| i.path == "pages/Near.md" && i.meant.as_deref() == Some("CC0-1.0"))
    );
    assert!(p.plan.files.iter().all(|f| f.path != "pages/Near.md"));
    // A licence that is no near miss is refused without a hint.
    let d = dir("v10-nohint");
    manifest(&d.0, &["license: CC-BY-SA-4.0"]);
    put(
        &d.0,
        "pages/Home.md",
        b"---\nid: home\ntitle: Home\nlicense: GFDL\n---\n\nx\n",
    );
    let p = plan(&d.0);
    assert_refused(&p, "pages/Home.md", "GFDL");
    assert!(!p.render().contains("did you mean"), "{}", p.render());
}
