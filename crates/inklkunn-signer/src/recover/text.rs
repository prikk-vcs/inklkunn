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
/// The error prikk gives when a `seal` published its block and stopped before it drained the queue: "pointer and log
/// agree but matching active publication state remains", recommending `prikk seal --allow-no-audit` for the ref
/// (measured on prikk 0.55.0; handoff 004, P15).
pub const ACTIVE_CLEANUP_PENDING_CODE: &str = "PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING";
/// The error prikk gives when a ref's publication was begun and not finished: "authoritative pointer leads committed
/// ref log by one transition", recommending ``prikk ref complete <ref>`` (prikk 0.49.0 and later, RFC 165 R4; measured on
/// 0.55.0 for an interrupted `branch create`; handoff 005 K4). While it stands, every other publication refuses.
pub const POINTER_LEADS_LOG_CODE: &str = "PRIKK-VERIFY-REF-POINTER-LEADS-LOG";

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

    /// What a [`PUBLICATION_UNTRUSTED_CODE`] diagnostic names, as `(kind, id)` (`block`, `ref-state` or `ref-update`,
    /// and 64 lower-case hex characters): the message must be exactly `<kind> <id> has no trusted MAINTAINER
    /// signature` (prikk 0.55.0). `None` for any other code and for any other shape.
    pub fn publication_subject(&self) -> Option<(&str, &str)> {
        if self.code != PUBLICATION_UNTRUSTED_CODE {
            return None;
        }
        let (kind, id) = self
            .message
            .strip_suffix(" has no trusted MAINTAINER signature")?
            .split_once(' ')?;
        let hex = id.len() == 64 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        (hex && matches!(kind, "block" | "ref-state" | "ref-update")).then_some((kind, id))
    }

    /// The warning that rides along with an acknowledged damage and asks for nothing.
    pub fn is_metadata_debris(&self) -> bool {
        self.severity == Severity::Warning && self.code == ACTIVE_REF_METADATA_DEBRIS_CODE
    }

    /// The error of exactly [`ACTIVE_CLEANUP_PENDING_CODE`]: a publication whose cleanup a `seal` did not finish.
    pub fn is_cleanup_pending(&self) -> bool {
        self.severity == Severity::Error && self.code == ACTIVE_CLEANUP_PENDING_CODE
    }

    /// An error of exactly [`POINTER_LEADS_LOG_CODE`], whatever its wording: a publication of some ref (a view's
    /// creation, in this service) was begun and not finished (handoff 005 K19). Where [`Diagnostic::lead_ref`] asks for
    /// the wording recovery may *act* on, this is the wider question a key rotation asks: is there one at all?
    pub fn is_publication_lead(&self) -> bool {
        self.severity == Severity::Error && self.code == POINTER_LEADS_LOG_CODE
    }

    /// The ref of an error of exactly [`POINTER_LEADS_LOG_CODE`] whose recommendation is, word for word, prikk's own
    /// ``run `prikk ref complete <ref>`; doctor does not sign or append`` (0.55.0): the interrupted publication of
    /// that one ref. `None` for any other code, any other severity and any other wording.
    pub fn lead_ref(&self) -> Option<&str> {
        if self.severity != Severity::Error || self.code != POINTER_LEADS_LOG_CODE {
            return None;
        }
        let ref_name = self
            .recommendation
            .as_deref()?
            .strip_prefix("run `prikk ref complete ")?
            .strip_suffix("`; doctor does not sign or append")?;
        let named_in_message = self
            .message
            .ends_with(&format!("; run `prikk ref complete {ref_name}`"));
        (named_in_message
            && !ref_name.is_empty()
            && ref_name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'.')))
        .then_some(ref_name)
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

/// The code `doctor` gives a block, ref state or ref update with no trusted signature (the same finding as
/// `verify`'s `publication-trust` line; after a retirement, ledger PK-34, one for each thing the retired key signed).
pub const PUBLICATION_UNTRUSTED_CODE: &str = "PRIKK-TRUST-PUBLICATION-UNTRUSTED";

impl DoctorReport {
    /// The same report without the [`PUBLICATION_UNTRUSTED_CODE`] diagnostics whose subject `excused` accepts (called
    /// with the kind and id each names), the counts recomputed, and how many were taken out. Only those: a
    /// diagnostic of that code that names anything else, or whose message is not of the known shape, stays (handoff
    /// 004, P13: a retirement's record accounts for exactly what it recorded).
    #[must_use]
    pub fn without_excused_publication_findings(
        &self,
        excused: impl Fn(&str, &str) -> bool,
    ) -> (DoctorReport, usize) {
        let kept: Vec<Diagnostic> = self
            .diagnostics
            .iter()
            .filter(|d| !d.publication_subject().is_some_and(|(k, i)| excused(k, i)))
            .cloned()
            .collect();
        let count = |s: Severity| {
            u32::try_from(kept.iter().filter(|d| d.severity == s).count()).unwrap_or(u32::MAX)
        };
        let removed = self.diagnostics.len() - kept.len();
        (
            DoctorReport {
                errors: count(Severity::Error),
                warnings: count(Severity::Warning),
                info: count(Severity::Info),
                diagnostics: kept,
            },
            removed,
        )
    }

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

    /// `doctor` names **exactly** one thing, the error [`ACTIVE_CLEANUP_PENDING_CODE`]: beside it there is only
    /// information and unreached-object notes (which do not block). Any other error or warning, a second one of this
    /// code, or a warning of the same code, is not this.
    pub fn is_only_cleanup_pending(&self) -> bool {
        let mut named = self
            .diagnostics
            .iter()
            .filter(|d| d.severity != Severity::Info && !d.is_unreached_note());
        matches!((named.next(), named.next()), (Some(d), None) if d.is_cleanup_pending())
    }

    /// `doctor` names **exactly** one thing, an interrupted publication of one ref ([`Diagnostic::lead_ref`]), and
    /// returns that ref: beside it there is only information and unreached-object notes. Any other error or warning,
    /// a second lead, or a lead whose wording is not prikk's, is not this.
    pub fn only_ref_lead(&self) -> Option<&str> {
        let mut named = self
            .diagnostics
            .iter()
            .filter(|d| d.severity != Severity::Info && !d.is_unreached_note());
        match (named.next(), named.next()) {
            (Some(d), None) => d.lead_ref(),
            _ => None,
        }
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

/// What ``prikk ref complete <ref> --plan-only`` said it would write (prikk 0.55.0), read strictly: the signer binds the
/// step to a recorded view creation by these fields, so a text of any other shape is an error and nothing is
/// concluded from it. `kind` is kept as prikk prints it (`Branch`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionPlan {
    /// The ref it would complete.
    pub ref_name: String,
    /// The leading RefState (64 hex characters).
    pub leading_ref_state: String,
    /// The target block (64 hex characters).
    pub target: String,
    /// The kind of the ref, as printed.
    pub kind: String,
    /// The ref log's tip for this ref; `None` for a first publication.
    pub log_tip: Option<String>,
    /// The sequence number of the record it would append.
    pub sequence: u64,
    /// Bytes of a partial tail its write would remove first.
    pub partial_tail_bytes: u64,
}

fn is_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Read the stdout of ``prikk ref complete <ref> --plan-only``: eight lines, in prikk's order, nothing else.
pub fn parse_completion_plan(stdout: &str) -> Result<CompletionPlan, String> {
    let bad = |what: &str| format!("unreadable `ref complete --plan-only` output: {what}");
    let mut lines = stdout.lines().filter(|l| !l.trim().is_empty());
    let mut field = |name: &str| -> Result<String, String> {
        lines
            .next()
            .and_then(|l| l.strip_prefix(name))
            .and_then(|l| l.strip_prefix(": "))
            .map(|v| v.trim().to_owned())
            .ok_or_else(|| bad(&format!("expected the line `{name}: …`")))
    };
    let ref_name = field("ref")?;
    let leading = field("leading RefState")?;
    let _original_signer = field("original signer")?;
    let target_line = field("target")?;
    let log_tip = field("log tip")?;
    let sequence = field("sequence")?;
    let _completing_key = field("completing key")?;
    let tail = field("partial tail to remove")?;
    if lines.next().is_some() {
        return Err(bad("text after the last line"));
    }
    let (target, kind) = target_line
        .split_once(" (")
        .and_then(|(t, k)| k.strip_suffix(')').map(|k| (t, k)))
        .ok_or_else(|| bad("the target is not `<block id> (<kind>)`"))?;
    if !is_id(&leading) || !is_id(target) {
        return Err(bad("an id is not 64 lower-case hex characters"));
    }
    let log_tip = if log_tip.starts_with("(none") {
        None
    } else if is_id(&log_tip) {
        Some(log_tip)
    } else {
        return Err(bad("the log tip is neither `(none …)` nor an id"));
    };
    let partial_tail_bytes = tail
        .strip_suffix(" byte(s)")
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| bad("the partial tail is not `<n> byte(s)`"))?;
    Ok(CompletionPlan {
        ref_name,
        leading_ref_state: leading,
        target: target.to_owned(),
        kind: kind.to_owned(),
        log_tip,
        sequence: sequence
            .parse()
            .map_err(|_| bad("the sequence is not a number"))?,
        partial_tail_bytes,
    })
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
