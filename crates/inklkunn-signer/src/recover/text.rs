//! The text prikk prints for `doctor` and `unlock`, read into typed values (RFC 004 D-7; handoff 004 T5).
//!
//! Recovery removes locks of the storage engine and, once, a damaged commit, so what it reads must be exactly what it
//! thinks it is. Neither verb has a JSON form (prikk 0.55.0), so both readers are strict: an output of a shape they do
//! not know is an **error and nothing is concluded**.
//!
//! * `prikk doctor` prints `doctor repository: <path>`, many `name: value` lines, the diagnostics
//!   (`error|warning|info [CODE]: message`, each optionally followed by an indented `recommendation: …`) and, last,
//!   `issue summary: errors=E, warnings=W, info=I`. The diagnostics must add up to the summary.
//! * `prikk unlock` prints either `no locks currently held`, or for each lock an absolute path line followed by
//!   indented `kind:`, `recorded pid:` and `liveness:` lines, and then an advisory paragraph. **The liveness line is
//!   read and kept, and never used** (ledger PL-8: inside the sandbox's PID namespace it means nothing).

/// The code prikk gives to an object that nothing refers to (`verify` counts the same as `unreached objects`;
/// prikk 0.54 and later; ledger PL-12). A warning, not damage.
pub const UNREACHED_OBJECT_CODE: &str = "PRIKK-DOCTOR-UNREACHED-OBJECT";
/// A commit the repository acknowledged and the active WAL no longer holds soundly (RFC 166 row 4 of prikk).
pub const ACKNOWLEDGED_DAMAGE_CODE: &str = "PRIKK-DOCTOR-COMMIT-WITNESS-ACKNOWLEDGED-DAMAGE";
/// A commit the repository acknowledged and the active WAL no longer holds at all (RFC 166 row 5 of prikk).
pub const ACKNOWLEDGED_LOSS_CODE: &str = "PRIKK-DOCTOR-COMMIT-WITNESS-ACKNOWLEDGED-LOSS";
/// The warning prikk gives beside an acknowledged damage: stale ref metadata for an empty WAL, "no repair is
/// required" (measured on prikk 0.55.0).
pub const ACTIVE_REF_METADATA_DEBRIS_CODE: &str = "PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS";

/// The kinds of lock an acceptance leaves behind: `commit` takes the active session's lock and the object store's,
/// and `seal` a ref's lock and the ref pointer index's and ref log's. A lock of any other kind (the trust policy's,
/// the received index's, one this build has not seen) is not an acceptance's, and is evidence we do not have.
pub const ACCEPTANCE_LOCK_KINDS: [&str; 5] = [
    "active",
    "ref",
    "container:object-store",
    "container:ref-pointer-index",
    "container:ref-log",
];

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// `error [CODE]`.
    Error,
    /// `warning [CODE]`.
    Warning,
    /// `info [CODE]`.
    Info,
}

/// One diagnostic of `doctor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// How serious.
    pub severity: Severity,
    /// `PRIKK-DOCTOR-VERIFY-OK`, …
    pub code: String,
    /// prikk's message: **operator text**, which may name a path.
    pub message: String,
    /// The indented `recommendation:` lines that followed, joined by a space.
    pub recommendation: Option<String>,
}

impl Diagnostic {
    /// A **warning of exactly [`UNREACHED_OBJECT_CODE`] that names no repair** ("the bytes stay; no command removes
    /// them yet"): there is nothing to do and nothing to wait for, so recovery notes it and goes on. An error of that
    /// code, a differently coded warning, or one that names a repair is not this.
    pub fn is_unreached_note(&self) -> bool {
        self.severity == Severity::Warning
            && self.code == UNREACHED_OBJECT_CODE
            && self.named_flags().is_empty()
    }

    /// An error of one of the two codes that `--discard-damaged-commits` answers.
    pub fn is_acknowledged_damage(&self) -> bool {
        self.severity == Severity::Error
            && (self.code == ACKNOWLEDGED_DAMAGE_CODE || self.code == ACKNOWLEDGED_LOSS_CODE)
    }

    /// The warning that rides along with an acknowledged damage and asks for nothing.
    pub fn is_metadata_debris(&self) -> bool {
        self.severity == Severity::Warning && self.code == ACTIVE_REF_METADATA_DEBRIS_CODE
    }

    /// The `--flags` this diagnostic's message and recommendation name (without `--plan-only`), in order, without
    /// repeats. Text for the plan; what may be *run* is decided by recovery, never by this.
    pub fn named_flags(&self) -> Vec<String> {
        let text = format!(
            "{} {}",
            self.message,
            self.recommendation.as_deref().unwrap_or("")
        );
        let mut out: Vec<String> = Vec::new();
        for tok in text.split(|c: char| {
            c.is_whitespace() || matches!(c, '`' | ',' | ';' | '(' | ')' | '\'' | '"')
        }) {
            let tok = tok.trim_end_matches('.');
            if tok.starts_with("--")
                && tok.len() > 2
                && tok != "--plan-only"
                && !out.iter().any(|t| t == tok)
            {
                out.push(tok.to_owned());
            }
        }
        out
    }
}

/// What `doctor` reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    /// The error count prikk summarized.
    pub errors: u32,
    /// The warning count prikk summarized.
    pub warnings: u32,
    /// The information count prikk summarized.
    pub info: u32,
    /// The diagnostics, in prikk's order. They add up to the summary (or the report was not read).
    pub diagnostics: Vec<Diagnostic>,
}

impl DoctorReport {
    /// How many diagnostics are [`Diagnostic::is_unreached_note`]s.
    pub fn unreached_notes(&self) -> u32 {
        let n = self
            .diagnostics
            .iter()
            .filter(|d| d.is_unreached_note())
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    }

    /// Nothing but information and unreached-object notes: recovery may go on (the notes are recorded and do not
    /// block, ledger PL-12).
    pub fn is_clean_for_recovery(&self) -> bool {
        self.diagnostics
            .iter()
            .all(|d| d.severity == Severity::Info || d.is_unreached_note())
    }

    /// Whether `doctor` names an acknowledged commit that the WAL no longer holds soundly.
    pub fn has_acknowledged_damage(&self) -> bool {
        self.diagnostics
            .iter()
            .any(Diagnostic::is_acknowledged_damage)
    }

    /// The diagnostics that stand in the way of every automatic step: errors and warnings that are not an unreached
    /// note, not an acknowledged damage (which `--discard-damaged-commits` answers) and not the metadata debris that
    /// rides along with one.
    pub fn others(&self) -> Vec<&Diagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity != Severity::Info)
            .filter(|d| !d.is_unreached_note() && !d.is_acknowledged_damage())
            .filter(|d| !(d.is_metadata_debris() && self.has_acknowledged_damage()))
            .collect()
    }
}

fn unreadable(what: &str) -> String {
    format!("unreadable `doctor`/`unlock` output: {what}")
}

/// Read the stdout of `prikk doctor` (no repair flag).
pub fn parse_doctor_output(stdout: &str) -> Result<DoctorReport, String> {
    let mut lines = stdout.lines().filter(|l| !l.trim().is_empty());
    match lines.next() {
        Some(first) if first.starts_with("doctor repository: ") => {}
        _ => return Err(unreadable("the first line is not `doctor repository: …`")),
    }
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut summary: Option<(u32, u32, u32)> = None;
    for line in lines {
        if let Some(rec) = line.trim_start().strip_prefix("recommendation:") {
            if !line.starts_with(' ') {
                continue;
            }
            match diagnostics.last_mut() {
                Some(d) => {
                    let text = rec.trim();
                    d.recommendation = Some(match d.recommendation.take() {
                        Some(prev) => format!("{prev} {text}"),
                        None => text.to_owned(),
                    });
                }
                None => return Err(unreadable("a recommendation with no diagnostic")),
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("issue summary: ") {
            let (mut e, mut w, mut i) = (None, None, None);
            for part in rest.split(", ") {
                let (k, v) = part
                    .split_once('=')
                    .ok_or_else(|| unreadable("the issue summary has a part without `=`"))?;
                let v: u32 = v
                    .trim()
                    .parse()
                    .map_err(|_| unreadable("a count in the issue summary is not a number"))?;
                match k {
                    "errors" => e = Some(v),
                    "warnings" => w = Some(v),
                    "info" => i = Some(v),
                    _ => return Err(unreadable("the issue summary names something else")),
                }
            }
            summary = Some((
                e.ok_or_else(|| unreadable("no errors count"))?,
                w.ok_or_else(|| unreadable("no warnings count"))?,
                i.ok_or_else(|| unreadable("no info count"))?,
            ));
            continue;
        }
        for (prefix, severity) in [
            ("error [", Severity::Error),
            ("warning [", Severity::Warning),
            ("info [", Severity::Info),
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let (code, message) = rest
                    .split_once("]: ")
                    .ok_or_else(|| unreadable("a diagnostic has no `]: `"))?;
                diagnostics.push(Diagnostic {
                    severity,
                    code: code.to_owned(),
                    message: message.trim().to_owned(),
                    recommendation: None,
                });
                break;
            }
        }
    }
    let (errors, warnings, info) = summary.ok_or_else(|| unreadable("no issue summary"))?;
    let count = |s: Severity| {
        u32::try_from(diagnostics.iter().filter(|d| d.severity == s).count()).unwrap_or(u32::MAX)
    };
    if (
        count(Severity::Error),
        count(Severity::Warning),
        count(Severity::Info),
    ) != (errors, warnings, info)
    {
        return Err(unreadable(
            "the diagnostics do not add up to the issue summary",
        ));
    }
    Ok(DoctorReport {
        errors,
        warnings,
        info,
        diagnostics,
    })
}

/// One lock `unlock` lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldLock {
    /// The lock file's absolute path, as prikk prints it.
    pub path: String,
    /// `active`, `container:object-store`, …
    pub kind: String,
    /// The process id the lock file records.
    pub recorded_pid: u32,
    /// prikk's own words about whether that process is running: kept for the record, **never used** (ledger PL-8).
    pub liveness: String,
}

/// Read the stdout of `prikk unlock` (the listing; it clears nothing).
pub fn parse_unlock_output(stdout: &str) -> Result<Vec<HeldLock>, String> {
    let mut out: Vec<HeldLock> = Vec::new();
    let mut lines = stdout.lines().peekable();
    let mut saw_anything = false;
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        saw_anything = true;
        if line.trim() == "no locks currently held" {
            if !out.is_empty() {
                return Err(unreadable("locks and `no locks currently held` together"));
            }
            continue;
        }
        if line.starts_with("liveness is advisory only") {
            // The advisory paragraph closes the listing; nothing may follow it but blank lines.
            if lines.any(|l| !l.trim().is_empty()) {
                return Err(unreadable("text after the advisory paragraph"));
            }
            break;
        }
        if !line.starts_with('/') || line.contains(' ') || line.bytes().any(|b| b < 0x20) {
            return Err(unreadable(
                "a line that is neither a lock path nor the known text",
            ));
        }
        let mut field = |name: &str| -> Result<String, String> {
            let next = lines
                .next()
                .ok_or_else(|| unreadable("a lock without its detail lines"))?;
            next.strip_prefix("  ")
                .and_then(|l| l.strip_prefix(name))
                .and_then(|l| l.strip_prefix(": "))
                .map(|v| v.trim().to_owned())
                .ok_or_else(|| {
                    unreadable("a lock's detail lines are not kind, recorded pid, liveness")
                })
        };
        let kind = field("kind")?;
        let pid = field("recorded pid")?;
        let liveness = field("liveness")?;
        out.push(HeldLock {
            path: line.to_owned(),
            kind,
            recorded_pid: pid
                .parse()
                .map_err(|_| unreadable("a recorded pid is not a number"))?,
            liveness,
        });
    }
    if !saw_anything {
        return Err(unreadable("empty unlock output"));
    }
    Ok(out)
}

/// What `doctor --discard-damaged-commits` said it did, as far as it could be read. Every field is a courtesy for
/// the record: none of them decides anything.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Discarded {
    /// The patch id of the acknowledged commit that was removed, when prikk named it.
    pub patch_id: Option<String>,
    /// The id of the recovery run prikk saved the removed bytes under (`doctor --recovery-list` shows it), when named.
    pub recovery_run: Option<String>,
    /// prikk said the content may still be in the worktree.
    pub worktree_may_hold_content: bool,
}

/// Read what `doctor --discard-damaged-commits` printed. `None` unless it said `damaged commit discarded`, which is
/// the only line that means it happened.
pub fn parse_discard_output(stdout: &str) -> Option<Discarded> {
    if !stdout
        .lines()
        .any(|l| l.trim() == "damaged commit discarded")
    {
        return None;
    }
    let mut out = Discarded::default();
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("acknowledged commit at sequence ")
            && let Some((_, patch)) = rest.split_once("(patch ")
        {
            let id = patch.trim_end_matches(')').trim();
            if id.bytes().all(|b| b.is_ascii_hexdigit()) && !id.is_empty() {
                out.patch_id = Some(id.to_owned());
            }
        }
        if let Some((_, run)) = line.split_once("recovery/log, run ") {
            let id = run.split_whitespace().next().unwrap_or("");
            if id.bytes().all(|b| b.is_ascii_hexdigit()) && !id.is_empty() {
                out.recovery_run = Some(id.to_owned());
            }
        }
        if line == "this content may still be in your working tree" {
            out.worktree_may_hold_content = true;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
