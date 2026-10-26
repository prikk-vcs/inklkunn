#![forbid(unsafe_code)]
//! `inklkunn-pin-check` — RFC 010 D-9 and D-10: nothing in the build chain is mutable, one compiler
//! builds everything (with the single `msrv` exception), and a release rehearsal can never reach the
//! real package. Line-based checks over the workflow and container files; each has a planted failure.

use std::path::{Path, PathBuf};

/// The real container image; a step naming it must be guarded as a tag push.
pub const REAL_IMAGE: &str = "ghcr.io/prikk-vcs/inklkunn";
/// The rehearsal package; a step naming it must be guarded as a rehearsal and may not publish.
pub const REHEARSAL_IMAGE: &str = "ghcr.io/prikk-vcs/inklkunn-rehearsal";

/// The workspace root.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn is_sha40(s: &str) -> bool {
    s.len() == 40
        && s.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

/// `uses:` lines that are not `actions/<name>@<40-hex sha>`.
pub fn unpinned_uses(yaml: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in yaml.lines().enumerate() {
        let t = line.trim();
        let Some(rest) = t
            .strip_prefix("- uses:")
            .or_else(|| t.strip_prefix("uses:"))
        else {
            continue;
        };
        let spec = rest.split('#').next().unwrap_or("").trim();
        let ok = spec.strip_prefix("actions/").is_some_and(|r| {
            let mut it = r.splitn(2, '@');
            let name = it.next().unwrap_or("");
            let sha = it.next().unwrap_or("");
            !name.is_empty() && !name.contains('/') && is_sha40(sha)
        });
        if !ok {
            out.push((i + 1, spec.to_owned()));
        }
    }
    out
}

/// Every `rustup toolchain install X.Y.Z` version in a workflow, in order.
pub fn toolchain_versions(yaml: &str) -> Vec<String> {
    yaml.lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix("rustup toolchain install ")?;
            rest.split_whitespace().next().map(str::to_owned)
        })
        .collect()
}

/// `runs-on:` values that name a moving label.
pub fn moving_runners(yaml: &str) -> Vec<(usize, String)> {
    yaml.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let t = l.trim();
            let v = t.strip_prefix("runs-on:")?.trim();
            (v.contains("latest")).then(|| (i + 1, v.to_owned()))
        })
        .collect()
}

/// `FROM` lines without an `@sha256:` digest.
pub fn undigested_from(dockerfile: &str) -> Vec<(usize, String)> {
    dockerfile
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let t = l.trim();
            let img = t.strip_prefix("FROM ")?;
            (!img.contains("@sha256:")).then(|| (i + 1, img.to_owned()))
        })
        .collect()
}

/// `rust-version = "X.Y.Z"` in the root manifest.
pub fn declared_msrv(cargo_toml: &str) -> Option<String> {
    cargo_toml
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("rust-version"))
        .and_then(|l| l.split('"').nth(1).map(str::to_owned))
}

/// A step of a workflow: its lines, split at `- name:` / `- uses:` / `- run:` list items.
pub fn steps(yaml: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in yaml.lines() {
        let t = line.trim_start();
        if t.starts_with("- name:") || t.starts_with("- uses:") || t.starts_with("- run:") {
            out.push(String::new());
        }
        if let Some(cur) = out.last_mut() {
            cur.push_str(line);
            cur.push('\n');
        }
    }
    out
}

/// Does `step` name the real image (not the rehearsal package, which starts with the same text)?
pub fn names_real_image(step: &str) -> bool {
    let mut from = 0;
    while let Some(i) = step[from..].find(REAL_IMAGE) {
        let end = from + i + REAL_IMAGE.len();
        let next = step[end..].chars().next();
        if next != Some('-') {
            return true;
        }
        from = end;
    }
    false
}

/// Rehearsal-safety violations in a release workflow (RFC 010 D-10):
/// - a step naming the real image lacks `github.event_name == 'push'` in an `if:`;
/// - a step naming the rehearsal image, or conditioned on `inputs.mode == 'rehearsal'`, publishes
///   (`cargo publish`, `gh release`, `git tag`, `git push`) or names the real image.
pub fn rehearsal_violations(release_yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for step in steps(release_yaml) {
        let names_real = names_real_image(&step);
        let is_rehearsal =
            step.contains(REHEARSAL_IMAGE) || step.contains("inputs.mode == 'rehearsal'");
        if names_real && !step.contains("github.event_name == 'push'") {
            out.push(format!(
                "a step names {REAL_IMAGE} without a tag-push guard: {}",
                first(&step)
            ));
        }
        if is_rehearsal {
            for forbidden in ["cargo publish", "gh release", "git tag", "git push"] {
                if step.contains(forbidden) {
                    out.push(format!(
                        "a rehearsal step contains `{forbidden}`: {}",
                        first(&step)
                    ));
                }
            }
            if names_real && !step.contains("github.event_name == 'push'") {
                out.push(format!(
                    "a rehearsal step names the real image: {}",
                    first(&step)
                ));
            }
        }
    }
    out
}

fn first(step: &str) -> String {
    step.lines().next().unwrap_or("").trim().to_owned()
}
