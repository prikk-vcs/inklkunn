#![forbid(unsafe_code)]
//! `inklkunn-pin-check` — RFC 010 D-9 and D-10: nothing in the build chain is mutable, one compiler
//! builds everything (with the single `msrv` exception), and a release rehearsal can never reach the
//! real package. Line-based checks over the workflow and container files; each has a planted failure.
//!
//! What is pinned: actions by commit SHA in GitHub's own namespace; runners by named image (no `-latest`,
//! also inside a matrix); the toolchain as `X.Y.Z`; `cargo install`ed tools as `name@X.Y.Z`; container
//! bases by digest (a stage name declared with `AS` may be reused by `FROM` or `COPY --from`). A step that
//! names the real image, or publishes, runs only under the exact tag-push guard, also when the guard sits
//! on the job.

use std::path::{Path, PathBuf};

/// The real container image; a step naming it must be guarded as a tag push.
pub const REAL_IMAGE: &str = "ghcr.io/prikk-vcs/inklkunn";
/// The rehearsal package; a step naming it must be guarded as a rehearsal and may not publish.
pub const REHEARSAL_IMAGE: &str = "ghcr.io/prikk-vcs/inklkunn-rehearsal";
/// The one expression that guards a real-publication step (exact, after whitespace normalization).
pub const PUSH_GUARD: &str = "github.event_name == 'push'";
/// The one expression that marks a rehearsal step.
pub const REHEARSAL_GUARD: &str = "inputs.mode == 'rehearsal'";
/// What a step may not do under a rehearsal.
pub const PUBLISHING: &[&str] = &["cargo publish", "gh release", "git tag", "git push"];

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

fn is_semver(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
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

/// Every `rustup toolchain install <version>` in a workflow, in order; a version that is not `X.Y.Z` is
/// returned as is so the caller can refuse it.
pub fn toolchain_versions(yaml: &str) -> Vec<String> {
    yaml.lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix("rustup toolchain install ")?;
            rest.split_whitespace().next().map(str::to_owned)
        })
        .collect()
}

/// Toolchain versions that are not `X.Y.Z` (a channel name is a moving target).
pub fn moving_toolchains(yaml: &str) -> Vec<String> {
    toolchain_versions(yaml)
        .into_iter()
        .filter(|v| !is_semver(v))
        .collect()
}

/// Every `rustup default <version>` in a workflow, in order.
pub fn default_toolchains(yaml: &str) -> Vec<String> {
    yaml.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("rustup default ")?;
            rest.split_whitespace().next().map(str::to_owned)
        })
        .collect()
}

/// One `rustup toolchain install <version> ...` command of a workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolchainInstall {
    /// The line the command starts on (1-based).
    pub line: usize,
    /// The version it installs.
    pub version: String,
    /// Every `--component <name>` it names, in order.
    pub components: Vec<String>,
}

/// Every `rustup toolchain install` command of a workflow, in order. A command continued over lines with a
/// trailing backslash is read whole, so a `--component` on a continuation line is not missed.
pub fn toolchain_installs(yaml: &str) -> Vec<ToolchainInstall> {
    let mut out = Vec::new();
    let mut lines = yaml.lines().enumerate();
    while let Some((i, l)) = lines.next() {
        let mut text = l.trim().to_owned();
        if !text.starts_with("rustup toolchain install ") {
            continue;
        }
        while text.ends_with('\\') {
            text.pop();
            let Some((_, next)) = lines.next() else {
                break;
            };
            text.push(' ');
            text.push_str(next.trim());
        }
        let mut words = text
            .strip_prefix("rustup toolchain install ")
            .unwrap_or("")
            .split_whitespace();
        let Some(version) = words.next() else {
            continue;
        };
        let mut components = Vec::new();
        while let Some(w) = words.next() {
            if w == "--component"
                && let Some(c) = words.next()
            {
                components.push(c.to_owned());
            }
        }
        out.push(ToolchainInstall {
            line: i + 1,
            version: version.to_owned(),
            components,
        });
    }
    out
}

/// Every `--component <name>` of a `rustup toolchain install` command, with that install's version, in order.
pub fn installed_components(yaml: &str) -> Vec<(String, String)> {
    toolchain_installs(yaml)
        .into_iter()
        .flat_map(|i| {
            let version = i.version;
            i.components.into_iter().map(move |c| (version.clone(), c))
        })
        .collect()
}

/// Installs of `channel` (the pin `rust-toolchain.toml` names) that leave out a component the file names,
/// with the components missing. The file makes the first plain `cargo` of a job install what the install
/// step left out: a download the job did not plan for and a new way for it to fail (maintenance M29). So
/// every install of the pin names all of them.
pub fn installs_missing_components(
    yaml: &str,
    channel: &str,
    components: &[String],
) -> Vec<(usize, Vec<String>)> {
    toolchain_installs(yaml)
        .into_iter()
        .filter(|i| i.version == channel)
        .filter_map(|i| {
            let missing: Vec<String> = components
                .iter()
                .filter(|c| !i.components.contains(c))
                .cloned()
                .collect();
            (!missing.is_empty()).then_some((i.line, missing))
        })
        .collect()
}

/// Jobs under `jobs:` with no `timeout-minutes:` of their own (a positive number of minutes). A job without
/// one runs for up to six hours when a test hangs (maintenance M30).
pub fn jobs_without_timeout(yaml: &str) -> Vec<(usize, String)> {
    let mut jobs: Vec<(usize, String, bool)> = Vec::new();
    let mut in_jobs = false;
    for (i, line) in yaml.lines().enumerate() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 {
            in_jobs = t == "jobs:";
            continue;
        }
        if !in_jobs {
            continue;
        }
        if indent == 2 && t.ends_with(':') && !t.starts_with('-') {
            jobs.push((i + 1, t.trim_end_matches(':').to_owned(), false));
        } else if indent == 4
            && let Some(rest) = t.strip_prefix("timeout-minutes:")
            && let Some(job) = jobs.last_mut()
            && rest
                .split('#')
                .next()
                .unwrap_or("")
                .trim()
                .parse::<u32>()
                .is_ok_and(|n| n > 0)
        {
            job.2 = true;
        }
    }
    jobs.into_iter()
        .filter(|(_, _, has)| !has)
        .map(|(n, name, _)| (n, name))
        .collect()
}

/// `RUST_TOOLCHAIN: "X.Y.Z"` in a workflow's `env:` (the release records the compiler it built with).
pub fn declared_toolchain_env(yaml: &str) -> Option<String> {
    yaml.lines()
        .map(str::trim)
        .find(|l| l.starts_with("RUST_TOOLCHAIN:"))
        .and_then(|l| l.split('"').nth(1).map(str::to_owned))
}

/// `channel = "X.Y.Z"` of a `rust-toolchain.toml`.
pub fn toolchain_file_channel(toml: &str) -> Option<String> {
    toml.lines()
        .map(str::trim)
        .find(|l| l.starts_with("channel"))
        .and_then(|l| l.split('"').nth(1).map(str::to_owned))
}

/// `components = ["a", "b"]` of a `rust-toolchain.toml`, in order.
pub fn toolchain_file_components(toml: &str) -> Vec<String> {
    toml.lines()
        .map(str::trim)
        .find(|l| l.starts_with("components"))
        .map(|l| l.split('"').skip(1).step_by(2).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// Is `s` an exact `X.Y.Z` version (a channel name such as `stable` is not)?
pub fn is_exact_version(s: &str) -> bool {
    is_semver(s)
}

/// A checkout's `rust-toolchain.toml` beats `rustup default`, so a job that installs the declared minimum
/// compiler must name it on every `cargo` and `rustc` it runs (`cargo +1.89.0 check`). These are the lines of
/// such a job that start a `cargo` or `rustc` command without `+<msrv>`. A job is a block under `jobs:` whose
/// text installs `msrv` with `rustup toolchain install`.
pub fn msrv_commands_without_the_toolchain(yaml: &str, msrv: &str) -> Vec<(usize, String)> {
    let mut jobs: Vec<Vec<(usize, &str)>> = Vec::new();
    let mut in_jobs = false;
    for (i, line) in yaml.lines().enumerate() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 {
            in_jobs = t == "jobs:";
            continue;
        }
        if !in_jobs {
            continue;
        }
        if indent == 2 && t.ends_with(':') && !t.starts_with('-') {
            jobs.push(Vec::new());
        }
        if let Some(job) = jobs.last_mut() {
            job.push((i + 1, line));
        }
    }
    let plus = format!("+{msrv} ");
    let mut out = Vec::new();
    for job in jobs {
        let installs = job.iter().any(|(_, l)| {
            l.trim()
                .strip_prefix("rustup toolchain install ")
                .and_then(|r| r.split_whitespace().next())
                == Some(msrv)
        });
        if !installs {
            continue;
        }
        for (n, l) in job {
            let t = l.trim();
            let cmd = t.strip_prefix("run:").map_or(t, str::trim);
            for tool in ["cargo ", "rustc "] {
                if let Some(rest) = cmd.strip_prefix(tool)
                    && !rest.starts_with(&plus)
                {
                    out.push((n, t.to_owned()));
                }
            }
        }
    }
    out
}

/// `runs-on:` values and matrix `os:` values that name a moving label.
pub fn moving_runners(yaml: &str) -> Vec<(usize, String)> {
    yaml.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let t = l.trim();
            let v = t
                .strip_prefix("runs-on:")
                .or_else(|| t.strip_prefix("- os:"))
                .or_else(|| t.strip_prefix("os:"))?
                .trim();
            v.contains("-latest").then(|| (i + 1, v.to_owned()))
        })
        .collect()
}

/// `cargo install` lines whose crates are not pinned as `name@X.Y.Z`.
pub fn unpinned_installs(yaml: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (i, line) in yaml.lines().enumerate() {
        let t = line.trim();
        let Some(idx) = t.find("cargo install ") else {
            continue;
        };
        let rest = t.get(idx + "cargo install ".len()..).unwrap_or("");
        for word in rest.split_whitespace() {
            if word.starts_with('-') {
                continue;
            }
            let pinned = word
                .split_once('@')
                .is_some_and(|(name, v)| !name.is_empty() && is_semver(v));
            if !pinned {
                out.push((i + 1, word.to_owned()));
            }
        }
    }
    out
}

/// Every `name@X.Y.Z` a workflow installs with `cargo install`.
pub fn install_pins(yaml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in yaml.lines() {
        let t = line.trim();
        let Some(idx) = t.find("cargo install ") else {
            continue;
        };
        for word in t
            .get(idx + "cargo install ".len()..)
            .unwrap_or("")
            .split_whitespace()
        {
            if let Some((name, v)) = word.split_once('@')
                && is_semver(v)
            {
                out.push((name.to_owned(), v.to_owned()));
            }
        }
    }
    out
}

/// `FROM` images (and `COPY --from=` sources) without an `@sha256:` digest, excluding stage names an
/// earlier `FROM … AS <name>` declared. Case-insensitive.
pub fn undigested_from(dockerfile: &str) -> Vec<(usize, String)> {
    let mut stages: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (i, l) in dockerfile.lines().enumerate() {
        let t = l.trim();
        let upper = t.to_ascii_uppercase();
        if let Some(rest) = upper.strip_prefix("FROM ") {
            let words: Vec<&str> = t.get(5..).unwrap_or("").split_whitespace().collect();
            let img = words
                .iter()
                .find(|w| !w.starts_with("--"))
                .copied()
                .unwrap_or("");
            if let Some(pos) = rest.split_whitespace().position(|w| w == "AS")
                && let Some(name) = words.get(pos + 1)
            {
                stages.push((*name).to_owned());
            }
            let is_stage = stages.iter().any(|s| s == img) && !img.contains(':');
            if !img.contains("@sha256:") && !is_stage {
                out.push((i + 1, img.to_owned()));
            }
        } else if upper.starts_with("COPY ") {
            for w in t.split_whitespace() {
                if let Some(src) = w
                    .strip_prefix("--from=")
                    .or_else(|| w.strip_prefix("--FROM="))
                    && !stages.iter().any(|s| s == src)
                    && !src.contains("@sha256:")
                {
                    out.push((i + 1, src.to_owned()));
                }
            }
        }
    }
    out
}

/// `rust-version = "X.Y.Z"` in the root manifest.
pub fn declared_msrv(cargo_toml: &str) -> Option<String> {
    cargo_toml
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("rust-version"))
        .and_then(|l| l.split('"').nth(1).map(str::to_owned))
}

/// One step of a workflow, with the `if:` expressions that govern it (its own and its job's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The step's lines.
    pub text: String,
    /// `if:` expressions in force, whitespace-normalized: the job's (if any) and the step's (if any).
    pub guards: Vec<String>,
}

fn normalize(expr: &str) -> String {
    expr.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn if_value(line: &str) -> Option<String> {
    let t = line.trim_start();
    let v = t
        .strip_prefix("- if:")
        .or_else(|| t.strip_prefix("if:"))?
        .trim();
    let v = v
        .strip_prefix("${{")
        .and_then(|r| r.strip_suffix("}}"))
        .unwrap_or(v)
        .trim();
    // A whole-expression quoting (`if: "a == 'b'"`) is removed; a quote that is part of the expression stays.
    let v = ['"', '\'']
        .iter()
        .find_map(|q| v.strip_prefix(*q).and_then(|r| r.strip_suffix(*q)))
        .unwrap_or(v);
    Some(normalize(v))
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The job-level `if:` of every job (any `if:` at the job's key indentation, before or after its steps).
fn job_ifs(yaml: &str) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    let mut in_jobs = false;
    let mut job: Option<String> = None;
    for line in yaml.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 {
            in_jobs = t == "jobs:";
            job = None;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if indent == 2 && t.ends_with(':') && !t.starts_with('-') {
            job = Some(t.trim_end_matches(':').to_owned());
            continue;
        }
        if indent == 4
            && let Some(j) = &job
            && let Some(v) = if_value(line)
        {
            out.insert(j.clone(), v);
        }
    }
    out
}

/// The steps of a workflow: the `- ` items directly under a job's `steps:` key, each with the `if:` in
/// force (the job's, if any, then the step's own).
pub fn steps(yaml: &str) -> Vec<Step> {
    let jobs = job_ifs(yaml);
    let mut out: Vec<Step> = Vec::new();
    let mut in_jobs = false;
    let mut job: Option<String> = None;
    let mut in_steps = false;
    let mut step_indent: Option<usize> = None;
    for line in yaml.lines() {
        let t = line.trim_start();
        if t.is_empty() || t.starts_with('#') {
            if let Some(cur) = out.last_mut().filter(|_| in_steps) {
                cur.text.push_str(line);
                cur.text.push('\n');
            }
            continue;
        }
        let indent = indent_of(line);
        if indent == 0 {
            in_jobs = t == "jobs:";
            job = None;
            in_steps = false;
            step_indent = None;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if indent == 2 && t.ends_with(':') && !t.starts_with('-') {
            job = Some(t.trim_end_matches(':').to_owned());
            in_steps = false;
            step_indent = None;
            continue;
        }
        if indent == 4 {
            in_steps = t == "steps:";
            step_indent = None;
            continue;
        }
        if !in_steps {
            continue;
        }
        let si = *step_indent.get_or_insert(indent);
        if indent == si && t.starts_with("- ") {
            out.push(Step {
                text: String::new(),
                guards: job
                    .as_ref()
                    .and_then(|j| jobs.get(j))
                    .cloned()
                    .into_iter()
                    .collect(),
            });
        }
        if let Some(cur) = out.last_mut() {
            cur.text.push_str(line);
            cur.text.push('\n');
            // The step's own `if:`: the item's first key (`- if:`) or a key of the item's body.
            if (indent == si || indent == si + 2)
                && let Some(v) = if_value(line)
            {
                cur.guards.push(v);
            }
        }
    }
    out
}

/// Does `step` name the real image (not the rehearsal package, which starts with the same text)?
pub fn names_real_image(step: &str) -> bool {
    let mut from = 0;
    while let Some(i) = step.get(from..).and_then(|s| s.find(REAL_IMAGE)) {
        let end = from + i + REAL_IMAGE.len();
        let next = step.get(end..).and_then(|s| s.chars().next());
        if next != Some('-') {
            return true;
        }
        from = end;
    }
    false
}

fn push_guarded(step: &Step) -> bool {
    step.guards.iter().any(|g| g == PUSH_GUARD)
}

fn rehearsal_marked(step: &Step) -> bool {
    step.guards.iter().any(|g| g == REHEARSAL_GUARD)
}

/// Rehearsal-safety violations in a release workflow (RFC 010 D-10):
/// - a step naming the real image, or publishing, lacks the exact tag-push guard (on itself or its job);
/// - a step marked as a rehearsal, or naming the rehearsal image, publishes or names the real image;
/// - a guard that is anything but the one allowed expression (an `||` widening it) counts as no guard.
pub fn rehearsal_violations(release_yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for step in steps(release_yaml) {
        let names_real = names_real_image(&step.text);
        let publishes = PUBLISHING.iter().any(|f| step.text.contains(f));
        let is_rehearsal = step.text.contains(REHEARSAL_IMAGE) || rehearsal_marked(&step);
        if (names_real || publishes) && !push_guarded(&step) {
            out.push(format!(
                "a step names {REAL_IMAGE} or publishes without the exact tag-push guard `{PUSH_GUARD}`: {}",
                first(&step.text)
            ));
        }
        if is_rehearsal {
            if publishes {
                out.push(format!("a rehearsal step publishes: {}", first(&step.text)));
            }
            if names_real {
                out.push(format!(
                    "a rehearsal step names the real image: {}",
                    first(&step.text)
                ));
            }
        }
    }
    out
}

fn first(step: &str) -> String {
    step.lines().next().unwrap_or("").trim().to_owned()
}
