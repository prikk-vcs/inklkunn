//! The release body (`.github/release-notes.sh`, docs/RELEASING.md) is a section's lead and a link, and fits a GitHub release
//! (at most 125,000 characters; 0.2.0's whole section was longer, which is why the lead is the body). Checked here on the
//! tree, before a tag exists, and not only by `release.yml`'s gates job at the cut.
// Tests may `expect`/`unwrap`: a failed expectation is the test failing, not a product path.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use inklkunn_floor_check::workspace_root;

/// The line that ends a lead (`release-notes.sh`, docs/RELEASING.md).
const LEAD_END: &str = "The entries below are in the order the work landed";

/// What the script holds the body to: bytes, under GitHub's 125,000 characters (the workflow's own check is the same number).
const LIMIT: usize = 100_000;

/// Run the script in `dir` (where `CHANGELOG.md` is) for `tag`.
fn run(dir: &Path, tag: &str) -> Output {
    let script = workspace_root().join(".github").join("release-notes.sh");
    Command::new("bash")
        .arg(script)
        .arg(tag)
        .arg("owner/repo")
        .current_dir(dir)
        .output()
        .expect("bash runs the script")
}

/// The tag of the first section of the CHANGELOG that has any text: `Unreleased` while a release is being prepared, the
/// release's own once the cut has moved the entries under its heading (and an empty `Unreleased` stands above it).
fn newest_section(changelog: &str) -> (String, String) {
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in changelog.lines() {
        if let Some(rest) = line.strip_prefix("## [") {
            let tag = rest.split(']').next().unwrap_or_default().to_owned();
            sections.push((tag, String::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    sections
        .into_iter()
        .find(|(_, body)| !body.trim().is_empty())
        .expect("a CHANGELOG section with text")
}

#[test]
fn the_newest_section_has_a_lead_and_its_release_body_fits() {
    let root = workspace_root();
    let changelog = fs::read_to_string(root.join("CHANGELOG.md")).expect("CHANGELOG.md");
    let (tag, section) = newest_section(&changelog);
    assert!(
        section.lines().any(|l| l.starts_with(LEAD_END)),
        "the `{tag}` section has no line `{LEAD_END}`: its release body would be the whole section"
    );
    let out = run(&root, &tag);
    assert!(
        out.status.success(),
        "release-notes.sh {tag}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let body = String::from_utf8(out.stdout).expect("UTF-8");
    assert!(
        body.len() <= LIMIT,
        "the release body of `{tag}` is {} bytes",
        body.len()
    );
    // The body is the lead, then the one link to the whole section: none of the entries is in it.
    assert!(!body.contains(LEAD_END), "the body runs past the lead");
    assert!(
        body.trim_end().ends_with(&format!(
            "[CHANGELOG.md at {tag}](https://github.com/owner/repo/blob/{tag}/CHANGELOG.md)."
        )),
        "the body does not end with the link to the whole section"
    );
    // A reader's lead is short: a screen or two, never the length of the first entry's detail.
    assert!(
        body.len() < 12_000,
        "the lead of `{tag}` is {} bytes: it should be one screen or two",
        body.len()
    );
}

#[test]
fn a_body_over_the_limit_is_refused_and_a_lead_is_cut_at_its_line() {
    let dir = std::env::temp_dir().join(format!("inklkunn-release-notes-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("a scratch directory");
    let entries: String = (0..2000)
        .map(|i| format!("- entry {i}: {}\n", "x".repeat(60)))
        .collect();
    assert!(entries.len() > LIMIT + 20_000);

    // No line that ends the lead: the whole section would be the body, so the script refuses (the check at 0.2.0's cut, here).
    fs::write(
        dir.join("CHANGELOG.md"),
        format!("# Changelog\n\n## [9.9.9] — 2099-01-01\n\nThe lead.\n\n{entries}\n## [9.9.8]\n\nOlder.\n"),
    )
    .expect("write");
    let out = run(&dir, "9.9.9");
    assert!(!out.status.success(), "a body over the limit was printed");
    let why = String::from_utf8_lossy(&out.stderr);
    assert!(
        why.contains("bytes") && why.contains("125,000"),
        "the refusal does not say why: {why}"
    );
    assert!(
        out.stdout.is_empty(),
        "something was printed with the refusal"
    );

    // With the line that ends the lead the same section is a small body: the lead and the link, none of the entries.
    fs::write(
        dir.join("CHANGELOG.md"),
        format!("# Changelog\n\n## [9.9.9] — 2099-01-01\n\nThe lead.\n\n{LEAD_END}.\n\n{entries}\n## [9.9.8]\n\nOlder.\n"),
    )
    .expect("write");
    let out = run(&dir, "9.9.9");
    assert!(out.status.success());
    let body = String::from_utf8(out.stdout).expect("UTF-8");
    assert!(body.trim_start().starts_with("The lead."), "{body}");
    assert!(!body.contains("entry 0") && !body.contains("Older."));
    assert!(body.contains("blob/9.9.9/CHANGELOG.md"));

    // A tag with no section is an error, not an empty body.
    let out = run(&dir, "1.2.3");
    assert!(!out.status.success());

    fs::remove_dir_all(&dir).expect("remove the scratch directory");
}

#[test]
fn a_body_of_exactly_the_limit_passes_and_one_byte_over_is_refused() {
    let dir = std::env::temp_dir().join(format!(
        "inklkunn-release-notes-limit-{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("a scratch directory");
    let write = |lead: &str| {
        fs::write(
            dir.join("CHANGELOG.md"),
            format!(
                "# Changelog\n\n## [9.9.9] — 2099-01-01\n\n{lead}\n\n{LEAD_END}.\n\n- an entry\n\n## [9.9.8]\n\nOlder.\n"
            ),
        )
        .expect("write");
    };
    // The body is the lead, a fixed sentence and a link; the script prints it and one newline. Measure the fixed part with a
    // one-byte lead, so the lead that makes the body exactly LIMIT bytes is computed and not guessed.
    write("x");
    let out = run(&dir, "9.9.9");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let fixed = out.stdout.len() - 1 /* the printed newline */ - 1 /* the lead's one byte */;
    let lead_len = LIMIT - fixed;

    // Exactly at the limit: passes (the limit is "at most 100,000").
    write(&"x".repeat(lead_len));
    let out = run(&dir, "9.9.9");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.stdout.len(),
        LIMIT + 1,
        "the body is LIMIT bytes and the printed newline"
    );
    let body = String::from_utf8(out.stdout).expect("UTF-8");
    assert!(body.trim_start().starts_with("xxxx") && body.contains("blob/9.9.9/CHANGELOG.md"));

    // One byte over: refused, with the reason, and nothing printed.
    write(&"x".repeat(lead_len + 1));
    let out = run(&dir, "9.9.9");
    assert!(
        !out.status.success(),
        "a body one byte over the limit was printed"
    );
    let why = String::from_utf8_lossy(&out.stderr);
    assert!(
        why.contains(&format!("{} bytes", LIMIT + 1)) && why.contains("125,000"),
        "the refusal does not say why: {why}"
    );
    assert!(
        out.stdout.is_empty(),
        "something was printed with the refusal"
    );

    // The limit counts bytes, not characters: a Japanese lead of about a third of LIMIT characters is over it in bytes.
    let chars = LIMIT / 3 + 1;
    assert!(chars < LIMIT);
    write(&"あ".repeat(chars));
    let out = run(&dir, "9.9.9");
    assert!(
        !out.status.success(),
        "a body over the limit in bytes (not in characters) was printed"
    );
    assert!(out.stdout.is_empty());

    fs::remove_dir_all(&dir).expect("remove the scratch directory");
}
