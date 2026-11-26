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
    let jobs = job_blocks(yaml);
    let plus = format!("+{msrv} ");
    let mut out = Vec::new();
    for (_, job) in jobs {
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

/// The jobs of a workflow: each job key (a line at indent 2 under `jobs:`) with the lines of its block (1-based
/// numbers; blank and comment-only lines left out).
fn job_blocks(yaml: &str) -> Vec<(String, Vec<(usize, &str)>)> {
    let mut jobs: Vec<(String, Vec<(usize, &str)>)> = Vec::new();
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
            jobs.push((t.trim_end_matches(':').to_owned(), Vec::new()));
        }
        if let Some((_, job)) = jobs.last_mut() {
            job.push((i + 1, line));
        }
    }
    jobs
}

/// The compiler a `cargo` or `rustc` command names with `+<toolchain>` on a line, in order (`cargo +nightly fmt`
/// names `nightly`). A trailing ` # comment` is not read. The command is the word `cargo` or `rustc` followed by a
/// word that starts with `+`.
fn plus_toolchains(line: &str) -> Vec<String> {
    let code = line.split(" #").next().unwrap_or("");
    let words: Vec<&str> = code
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')' | '"' | '\''))
        .filter(|w| !w.is_empty())
        .collect();
    words
        .windows(2)
        .filter(|w| matches!(w.first(), Some(&"cargo" | &"rustc")))
        .filter_map(|w| w.get(1)?.strip_prefix('+'))
        .map(str::to_owned)
        .collect()
}

/// A `+<toolchain>` on a `cargo` or `rustc` command is allowed in the `msrv` job and only there, and there only
/// with the declared minimum (maintenance M42, review result E-c): anywhere else it would install and use a second
/// compiler that `rustup` never shows in the workflow (rustup 1.28 and later install what a `+X` names), and the
/// canonical reader of M38 sees no `rustup` line to refuse. These are the lines that break it, with the line's text.
pub fn plus_toolchains_outside_the_minimum(yaml: &str, msrv: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for (name, job) in job_blocks(yaml) {
        for (n, l) in job {
            let wrong = plus_toolchains(l)
                .iter()
                .any(|x| name != "msrv" || x != msrv);
            if wrong {
                out.push((n, l.trim().to_owned()));
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
        let publishes = publishes(&step.text);
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

/// Does a step publish? Any of [`PUBLISHING`]'s forms, **except `cargo publish` run with `--dry-run`** (maintenance M63: the
/// rehearsal and the gates package and verify every crate and upload nothing). `cargo publish` is judged **by command**:
/// a line is split at `&&`, `||`, `;` and `|`, a comment is dropped, and a command that names `cargo publish` without its own
/// `--dry-run` token publishes, so `cargo publish --dry-run && cargo publish` is a publication and an `echo` of the words in a
/// string is read as the text it is (as every rule here reads text: this is a guard against a mistake, not against evasion).
fn publishes(step: &str) -> bool {
    if PUBLISHING
        .iter()
        .filter(|f| **f != "cargo publish")
        .any(|f| step.contains(f))
    {
        return true;
    }
    step.lines().any(|line| {
        let code = line.split('#').next().unwrap_or("");
        code.split(['&', '|', ';']).any(|command| {
            command.contains("cargo publish")
                && !command.split_whitespace().any(|t| t == "--dry-run")
        })
    })
}

fn first(step: &str) -> String {
    step.lines().next().unwrap_or("").trim().to_owned()
}

// ---------------------------------------------------------------------------------------------------------
// The canonical form (maintenance M38). No YAML parser is in the lock and none is added: the workflows are
// held to ONE form that these line rules read completely, and anything outside it is REFUSED, not skipped.
// A rule that must understand every shape of YAML can miss one; a form that refuses every other shape cannot.
// ---------------------------------------------------------------------------------------------------------

/// A job of a workflow, as the canonical reader found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRead {
    /// The job key's line (1-based).
    pub line: usize,
    /// The job key, without its colon.
    pub name: String,
    /// `timeout-minutes: <N>` among the job's own keys, before its `steps:`, if it has one.
    pub timeout: Option<u32>,
}

/// A canonical `rustup toolchain install` the reader found, inside a `run: |` block of a job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallRead {
    /// The line (1-based).
    pub line: usize,
    /// The job it stands in.
    pub job: String,
    /// The version it installs: the pin or the declared minimum.
    pub version: String,
}

/// What the canonical reader made of a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reading {
    /// Every job in the canonical form, in order.
    pub jobs: Vec<JobRead>,
    /// Every canonical install, in order.
    pub installs: Vec<InstallRead>,
    /// Everything outside the canonical form, one sentence each, with its line. Empty when the workflow is
    /// canonical.
    pub refusals: Vec<String>,
}

/// The only `rustup toolchain install` lines allowed, as whole lines with the version they install: the
/// pin's, with the components `rust-toolchain.toml` names, and the declared minimum's, without any.
fn canonical_installs(pin: &str, components: &[String], msrv: &str) -> [(String, String); 2] {
    let comps: String = components
        .iter()
        .map(|c| format!(" --component {c}"))
        .collect();
    [
        (
            format!("rustup toolchain install {pin} --profile minimal{comps} --no-self-update"),
            pin.to_owned(),
        ),
        (
            format!("rustup toolchain install {msrv} --profile minimal --no-self-update"),
            msrv.to_owned(),
        ),
    ]
}

/// The other lines that mention `rustup` and are allowed, as whole lines: the default toolchain (the pin or
/// the declared minimum) and the two lines that put `rustup` and the compiler on the shimmed run's `PATH`.
fn canonical_other_rustup(pin: &str, msrv: &str) -> Vec<String> {
    vec![
        format!("rustup default {pin}"),
        format!("rustup default {msrv}"),
        "for tool in cargo rustc rustdoc; do ln -s \"$(rustup which \"$tool\")\" \"$SHIM/$tool\"; done"
            .to_owned(),
        "ln -s \"$(command -v rustup)\" \"$SHIM/rustup\"".to_owned(),
    ]
}

fn mentions_rustup(line: &str) -> bool {
    line.to_ascii_lowercase().contains("rustup")
}

/// Is `raw` (an unstripped line) a job key in the canonical form `^  [a-z0-9-]+:$`?
fn is_canonical_job_key(raw: &str) -> bool {
    raw.strip_prefix("  ")
        .and_then(|r| r.strip_suffix(':'))
        .is_some_and(|name| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// `timeout-minutes: <N>` (a positive number, one trailing ` # comment` read), else `None`.
fn timeout_value(trimmed: &str) -> Option<u32> {
    trimmed
        .strip_prefix("timeout-minutes:")?
        .split(" #")
        .next()
        .unwrap_or("")
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
}

/// The job being read.
struct OpenJob {
    line: usize,
    name: String,
    steps_seen: bool,
    timeouts_seen: usize,
    timeout: Option<u32>,
}

fn close_job(r: &mut Reading, job: Option<OpenJob>) {
    let Some(j) = job else {
        return;
    };
    if !j.steps_seen {
        r.refusals.push(format!(
            "line {}: job `{}` has no `steps:` among its own keys",
            j.line, j.name
        ));
    }
    if j.timeout.is_none() {
        r.refusals.push(format!(
            "line {}: job `{}` has no `timeout-minutes: <N>` among its own keys before `steps:`",
            j.line, j.name
        ));
    }
    r.jobs.push(JobRead {
        line: j.line,
        name: j.name,
        timeout: j.timeout,
    });
}

/// Read a workflow in its canonical form, and record every way it is not (maintenance M38). The form:
/// - `jobs:` is a line of its own, once, at the left edge;
/// - each job key is `^  [a-z0-9-]+:$`, with no comment, quote or trailing space, and no line stands at an
///   odd indent among the job keys;
/// - each job has `steps:` and, among its own keys (indent 4) **before** `steps:`, one `timeout-minutes: <N>`;
/// - every line that mentions `rustup` (in any case; comment lines apart) is, whole, one of the canonical
///   install forms for `pin` (with `components`) or `msrv`, `rustup default <pin or msrv>`, or one of the two
///   shim lines of the shimmed run, and stands inside a block `run: |` of a job: never in a one-line `run:`,
///   never after `;`, `&&` or `||`, never continued over lines.
///
/// Nothing is skipped: a line the reader cannot place is a refusal, and a job whose key is refused is read
/// as no job, so its lines credit nothing to the job before it or after it.
pub fn read_workflow(yaml: &str, pin: &str, components: &[String], msrv: &str) -> Reading {
    let installs = canonical_installs(pin, components, msrv);
    let others = canonical_other_rustup(pin, msrv);
    let mut r = Reading::default();
    let mut jobs_lines = 0;
    let mut in_jobs = false;
    let mut job: Option<OpenJob> = None;
    // The indent of the key of the `run: |` block being read, if any.
    let mut block: Option<usize> = None;
    for (i, raw) in yaml.lines().enumerate() {
        let n = i + 1;
        let t = raw.trim();
        let indent = indent_of(raw);
        if raw.get(..indent).is_some_and(|lead| lead.contains('\t')) {
            r.refusals
                .push(format!("line {n}: a tab in the indentation"));
            continue;
        }
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        // Inside a block scalar: its lines are the block's, until a line at or left of the key.
        if let Some(k) = block {
            if indent > k {
                if mentions_rustup(t) {
                    let install = installs.iter().find(|(text, _)| text == t);
                    match (install, &job) {
                        (Some((_, version)), Some(j)) => r.installs.push(InstallRead {
                            line: n,
                            job: j.name.clone(),
                            version: version.clone(),
                        }),
                        (None, Some(_)) if others.iter().any(|o| o == t) => {}
                        _ => r.refusals.push(format!(
                            "line {n}: `{t}` mentions rustup and is not a canonical form inside a job"
                        )),
                    }
                }
                continue;
            }
            block = None;
        }
        if indent == 0 {
            close_job(&mut r, job.take());
            in_jobs = false;
            let key = raw.split(':').next().unwrap_or("");
            if key.trim().trim_matches(['"', '\'']).trim() == "jobs" {
                if raw == "jobs:" {
                    jobs_lines += 1;
                    in_jobs = true;
                } else {
                    r.refusals.push(format!(
                        "line {n}: `jobs:` is not a line of its own (`{raw}`)"
                    ));
                }
            }
        } else if in_jobs && indent < 4 {
            close_job(&mut r, job.take());
            if is_canonical_job_key(raw) {
                job = Some(OpenJob {
                    line: n,
                    name: t.trim_end_matches(':').to_owned(),
                    steps_seen: false,
                    timeouts_seen: 0,
                    timeout: None,
                });
            } else {
                r.refusals.push(format!(
                    "line {n}: `{raw}` is not a canonical job key (`  [a-z0-9-]+:` and nothing after the colon)"
                ));
            }
        } else if in_jobs
            && indent == 4
            && let Some(j) = job.as_mut()
        {
            if t == "steps:" {
                j.steps_seen = true;
            } else if t.starts_with("timeout-minutes:") {
                j.timeouts_seen += 1;
                match timeout_value(t) {
                    Some(v) if !j.steps_seen && j.timeouts_seen == 1 => j.timeout = Some(v),
                    _ => r.refusals.push(format!(
                        "line {n}: `{t}` is not one `timeout-minutes: <N>` (a positive number) before `steps:`"
                    )),
                }
            }
        }
        // `run: |` opens a block; a `run:` of any other shape that mentions rustup is refused here.
        if t == "run: |" || t == "- run: |" {
            block = Some(indent + if t.starts_with("- ") { 2 } else { 0 });
        } else if mentions_rustup(t) {
            r.refusals.push(format!(
                "line {n}: `{t}` mentions rustup outside a block `run: |`"
            ));
        }
    }
    close_job(&mut r, job.take());
    match jobs_lines {
        1 => {}
        0 => r.refusals.push("no `jobs:` line of its own".to_owned()),
        _ => r.refusals.push("more than one `jobs:` line".to_owned()),
    }
    r
}

/// Jobs with no `timeout-minutes: <N>` of their own before their `steps:` (a job without one runs for up to
/// six hours when a test hangs, maintenance M30), as the canonical reader finds them: a job whose key is
/// refused is no job and credits nothing to the job before it.
pub fn jobs_without_timeout(yaml: &str) -> Vec<(usize, String)> {
    read_workflow(yaml, "0.0.0", &[], "0.0.0")
        .jobs
        .into_iter()
        .filter(|j| j.timeout.is_none())
        .map(|j| (j.line, j.name))
        .collect()
}

/// **A plain value that YAML cannot read** (maintenance B-1; M63's step name, `name: Every crate packages … (a dry run: nothing
/// …)`, made `release.yml` unloadable on `main`: GitHub saw no workflow, so no rehearsal or release could start). In a
/// workflow file, a single-line `key: value` or `- key: value` whose value is **not quoted** must not contain `: ` (a colon
/// and a space), must not end in `:`, and must not begin with a character a plain scalar cannot begin with (`&`, `*`, `!`, `%`,
/// `@`, a backtick, `|`/`>` aside as block-scalar headers, and a flow `[`/`{` aside as a flow collection, which is judged by
/// YAML itself). A `${{ … }}` expression and a trailing ` #` comment are taken out first; **a block scalar's lines (`run: |`,
/// `>`) are not judged**, and neither is a comment line. The line (1-based) and the text of each offender are returned.
pub fn unreadable_plain_values(yaml: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    // The indent of the key line whose block scalar is being skipped.
    let mut block_of: Option<usize> = None;
    for (i, raw) in yaml.lines().enumerate() {
        let trimmed = raw.trim_start();
        let indent = raw.len() - trimmed.len();
        if let Some(key_indent) = block_of {
            if trimmed.is_empty() || indent > key_indent {
                continue;
            }
            block_of = None;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // `- ` list markers (possibly several) are not part of the key.
        let mut rest = trimmed;
        let mut key_indent = indent;
        while let Some(after) = rest.strip_prefix("- ") {
            key_indent += 2;
            rest = after.trim_start();
        }
        // The first `: ` (or a final `:`) splits the key from the value. A key that is quoted or has no colon is not a pair.
        let split = rest.find(": ").map(|at| (at, at + 2)).or_else(|| {
            rest.strip_suffix(':')
                .filter(|k| !k.is_empty())
                .map(|k| (k.len(), rest.len()))
        });
        let Some((key_end, value_start)) = split else {
            continue;
        };
        let key = rest.get(..key_end).unwrap_or("");
        if key.starts_with(['"', '\'']) || key.is_empty() {
            continue;
        }
        let mut value = rest.get(value_start..).unwrap_or("").trim().to_owned();
        // A `${{ … }}` expression is read by GitHub, not by YAML's plain-scalar rules for its inside.
        while let Some(open) = value.find("${{") {
            let Some(close) = value.get(open..).and_then(|v| v.find("}}")) else {
                break;
            };
            value.replace_range(open..open + close + 2, "X");
        }
        if let Some(at) = value.find(" #") {
            value.truncate(at);
        }
        let value = value.trim();
        if value.is_empty() {
            continue; // a mapping or a list follows
        }
        if value.starts_with(['|', '>']) {
            block_of = Some(key_indent);
            continue;
        }
        if value.starts_with(['"', '\'', '[', '{']) {
            continue; // quoted, or a flow collection: YAML reads it by its own rules
        }
        let special_start = value.starts_with(['&', '*', '!', '%', '@', '`']);
        if value.contains(": ") || value.ends_with(':') || special_start {
            out.push((i + 1, trimmed.to_owned()));
        }
    }
    out
}
