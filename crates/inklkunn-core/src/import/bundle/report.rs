//! The report of a bundle import (RFC 007 D-6; wiki-commons XFER-13): what the plan found, said before anything is written.
//!
//! [`BundleReport`] is a **value**: plain data that the act of the import can keep ([`BundleReport::to_json`]), and the one text
//! an operator reads ([`BundleReport::render`]) is made from it and from nothing else. **Every name in it is display-safe when the plan makes it, and
//! bounded** (`safe::shown`: no control or bidirectional character, no name over 120 characters, no list over
//! [`MAX_LISTED`] entries), and **no byte of a refused file's content is in it**: a refused file is named by its path and the
//! rule it broke (the plan's refusals), and what a file that was refused as a whole said is not reported as a fact.

use std::collections::BTreeMap;

/// The most entries of a list that the text shows (the rest is counted).
pub const MAX_LISTED: usize = 25;

/// What the plan found of the bundle's fidelity, as opposed to what the manifest declares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Found {
    /// Pages and attachments, and no revision that can be replayed.
    #[default]
    ContentOnly,
    /// Revisions that can be replayed (level 3 in fact).
    History,
}

/// Where the wiki's licence comes from (RFC 007 D-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseFrom {
    /// The bundle's manifest says it.
    Manifest,
    /// The bundle says none, and the importing admin declared it (`--bundle-license`).
    DeclaredByAdmin,
}

/// What the plan did with the checksums.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Checksums {
    /// The manifest names no file of checksums: *none in the bundle*, and nothing was verified.
    #[default]
    None,
    /// The manifest names a file of checksums and the bundle does not stand up to it (the refusals say how): nothing is
    /// verified.
    Failed {
        /// The file, as the manifest names it.
        file: String,
    },
    /// Every file of the bundle was checked against the file the manifest names (the manifest itself is not covered by a
    /// checksum, by wiki-commons' convention).
    Verified {
        /// The file, as the manifest names it.
        file: String,
        /// How many files it names, all of which were checked.
        entries: usize,
    },
}

/// A kind of file that is judged for its own licence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    /// A page (with its revisions).
    Page,
    /// An attachment.
    Attachment,
}

/// A file whose own licence no wiki with this default may take (LIC-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incompatible {
    /// A page or an attachment.
    pub kind: FileKind,
    /// Its path in the bundle.
    pub path: String,
    /// The licence, as written.
    pub license: String,
    /// The revision that declares it, when it is a revision's and not the page's last text.
    pub rev: Option<String>,
}

/// Something in the bundle that is not imported, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    /// What: a path, or a group (`history/`).
    pub what: String,
    /// How many files.
    pub files: usize,
    /// Why.
    pub why: String,
}

/// One page's history, as the plan counts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageLine {
    /// The page's path in the bundle (or the history file's name when the page is not in the bundle's files).
    pub page: String,
    /// The records in the file.
    pub records: usize,
    /// How many of them would be replayed, one block each.
    pub revisions: usize,
}

/// The people the history names, as the source says them. **Never linked to an account of this server.**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeopleLine {
    /// How many people, counting a source id (or name, or anonymous label) once.
    pub total: usize,
    /// By the kind the source gives.
    pub by_kind: BTreeMap<String, usize>,
    /// Their names, up to [`MAX_LISTED`].
    pub names: Vec<String>,
    /// How many entries `users.yaml` has.
    pub in_users_file: usize,
}

/// The plan's report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BundleReport {
    /// `version` of the manifest.
    pub bundle_version: String,
    /// The generator, as the manifest says.
    pub generator: Option<String>,
    /// When the bundle says it was exported.
    pub exported_at: Option<String>,
    /// The source's name, **as text**.
    pub source_name: Option<String>,
    /// The source's address, **as text, never a link**.
    pub source_url: Option<String>,
    /// The engine the bundle came from.
    pub source_engine: Option<String>,
    /// The bundle says the people's identities were replaced by stable identifiers.
    pub pseudonymized: bool,
    /// The languages the bundle says its pages are in (the wiki's are `--lang`'s, or English).
    pub languages: Vec<String>,
    /// `visibility_default`: what the bundle says who may read it (the wiki's is `--visibility`'s).
    pub visibility_default: Option<String>,
    /// Manifest keys this build does not know (never interpreted).
    pub unknown_manifest_keys: Vec<String>,
    /// `fidelity.level` as declared: **reported, not trusted**.
    pub fidelity_declared: Option<u8>,
    /// What the plan found.
    pub fidelity_found: Found,
    /// What the source says it degraded.
    pub degraded_at_source: Vec<String>,
    /// What the source says it dropped.
    pub dropped_at_source: Vec<String>,
    /// The wiki's default licence.
    pub license: Option<String>,
    /// Where it comes from.
    pub license_from: Option<LicenseFrom>,
    /// The checksums.
    pub checksums: Checksums,
    /// Pages that come in.
    pub pages: usize,
    /// Attachments that come in (a sidecar is not one).
    pub attachments: usize,
    /// Attachment sidecars (`<name>.meta.yaml`) that come in with them.
    pub sidecars: usize,
    /// The licences the sidecars of the attachments that come in declare, with how many attachments each (an attachment's own
    /// licence travels in its sidecar and is never rewritten; the sidecars that say none are not counted).
    pub attachment_licenses: BTreeMap<String, usize>,
    /// Claim and source files of the knowledge layer that come in.
    pub knowledge_files: usize,
    /// `--no-history`: the history was not replayed.
    pub no_history: bool,
    /// Pages and the revisions of each (the pages that have history).
    pub histories: Vec<PageLine>,
    /// Pages that come in with the first change because they have no revision to replay.
    pub pages_without_history: Vec<String>,
    /// The records in all the files read.
    pub records: usize,
    /// The revisions the replay would make, one block each.
    pub revisions: usize,
    /// The cap on revisions the plan was judged under (`INKLKUNN_IMPORT_MAX_REVISIONS`).
    pub max_revisions: u64,
    /// The bytes of the history files read.
    pub history_bytes: u64,
    /// The people.
    pub people: PeopleLine,
    /// Files no wiki with this default licence may take.
    pub incompatible: Vec<Incompatible>,
    /// `--leave-out-incompatible` was given: those files are left out and not a refusal.
    pub incompatible_left_out: bool,
    /// Records that are not replayed, by what they are (`type upload`, `no text`, …).
    pub not_replayed: BTreeMap<String, usize>,
    /// Records whose text the source hid: not replayed.
    pub suppressed_content: usize,
    /// Records replayed with their author hidden at the source.
    pub suppressed_author: usize,
    /// Records replayed with their summary hidden at the source.
    pub suppressed_summary: usize,
    /// Records whose text the exporting inklkunn withheld, by its reason: not replayed.
    pub withheld: BTreeMap<String, usize>,
    /// Record members this build does not know, by name (never interpreted).
    pub unknown_record_fields: Vec<String>,
    /// What is not imported.
    pub left_out: Vec<LeftOut>,
    /// Pages whose final file differs from the text of their last revision: one change at the end puts them right.
    pub final_state_pages: Vec<String>,
    /// Link prefixes of the pages that the wiki cannot resolve, with how many links use each.
    pub unmapped_prefixes: BTreeMap<String, usize>,
    /// How many prefixes the bundle's own interwiki map holds (it is read by the map's own import, not by this plan).
    pub interwiki_in_bundle: usize,
    /// The replay's estimated time in seconds.
    pub estimate_seconds: Option<u64>,
}

fn list<T>(out: &mut String, heading: &str, items: &[T], line: impl Fn(&T) -> String) {
    if items.is_empty() {
        return;
    }
    out.push_str(heading);
    out.push('\n');
    for item in items.iter().take(MAX_LISTED) {
        out.push_str("  ");
        out.push_str(&line(item));
        out.push('\n');
    }
    if items.len() > MAX_LISTED {
        out.push_str(&format!("  … and {} more\n", items.len() - MAX_LISTED));
    }
}

fn duration(seconds: u64) -> String {
    match seconds {
        0..=59 => format!("{seconds} s"),
        60..=3599 => format!("{} min {} s", seconds / 60, seconds % 60),
        _ => format!("{} h {} min", seconds / 3600, seconds % 3600 / 60),
    }
}

impl BundleReport {
    /// The estimate of the replay's time from a count of blocks: **a fit to ledger PK-50's two measurements** of building a
    /// history through the signer (500 blocks: 154 s; 2,000 blocks: 1,438 s), as `0.171 s × n + 0.000274 s × n²`. The cost per
    /// block grows with the log (prikk's), so the estimate is quadratic. It is an estimate of a replay on prikk 0.55 – 0.58; the
    /// import's own cost is measured with the replay.
    pub fn estimate_for(blocks: u64) -> u64 {
        let n = blocks as f64;
        (0.171 * n + 0.000_274 * n * n).round() as u64
    }

    /// The text an operator reads.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("the bundle:\n");
        out.push_str(&format!("  version:         {}\n", self.bundle_version));
        if let Some(g) = &self.generator {
            out.push_str(&format!("  generated by:    {}\n", g));
        }
        if let Some(e) = &self.exported_at {
            out.push_str(&format!("  exported at:     {} (the source's claim)\n", e));
        }
        if let Some(n) = &self.source_name {
            out.push_str(&format!("  source:          {} (text; not checked)\n", n));
        }
        if let Some(u) = &self.source_url {
            out.push_str(&format!("  source address:  {} (text, never a link)\n", u));
        }
        if let Some(e) = &self.source_engine {
            out.push_str(&format!("  source engine:   {}\n", e));
        }
        if self.pseudonymized {
            out.push_str(
                "  the bundle says its people's identities were replaced by stable identifiers\n",
            );
        }
        if !self.languages.is_empty() {
            out.push_str(&format!(
                "  languages:       {} (the wiki's are what --lang says, English without it)\n",
                self.languages.join(", ")
            ));
        }
        if let Some(v) = &self.visibility_default {
            out.push_str(&format!(
                "  visibility:      {v} (the bundle's claim; the wiki's is what --visibility says)\n"
            ));
        }
        match self.fidelity_declared {
            Some(level) => out.push_str(&format!(
                "fidelity declared: level {level} (the bundle's claim; this import trusts what it finds)\n"
            )),
            None => out.push_str("fidelity declared: none (this import trusts what it finds)\n"),
        }
        match self.fidelity_found {
            Found::History => out.push_str(&format!(
                "fidelity found:    pages, attachments and history ({} revision(s) in {} page(s) can be replayed)\n",
                self.revisions,
                self.histories.iter().filter(|h| h.revisions > 0).count()
            )),
            Found::ContentOnly if self.no_history => out.push_str(
                "fidelity found:    pages and attachments (the history was not read: --no-history)\n",
            ),
            Found::ContentOnly => {
                out.push_str("fidelity found:    pages and attachments (no revision that can be replayed)\n");
            }
        }
        list(
            &mut out,
            "what the source says it degraded:",
            &self.degraded_at_source,
            |c| c.clone(),
        );
        list(
            &mut out,
            "what the source says it dropped:",
            &self.dropped_at_source,
            |c| c.clone(),
        );
        match (&self.license, self.license_from) {
            (Some(l), Some(LicenseFrom::Manifest)) => out.push_str(&format!(
                "licence of the wiki: {} (the bundle's own; a page that has none shows under it)\n",
                l
            )),
            (Some(l), Some(LicenseFrom::DeclaredByAdmin)) => out.push_str(&format!(
                "licence of the wiki: {} (the bundle declares none; declared by the importing admin, and the wiki's notice says so)\n",
                l
            )),
            _ => out.push_str("licence of the wiki: none yet (see the refusals)\n"),
        }
        match &self.checksums {
            Checksums::None => out.push_str(
                "checksums: none in the bundle (nothing was verified; the wiki's notice says so)\n",
            ),
            Checksums::Failed { file } => out.push_str(&format!(
                "checksums: the manifest names `{file}`, and the bundle does not stand up to it (see the refusals)\n"
            )),
            Checksums::Verified { file, entries } => out.push_str(&format!(
                "checksums: verified, every file against `{}` ({entries} file(s); they show the bundle is the one that was checked, not who made it)\n",
                file
            )),
        }
        out.push_str(&format!(
            "to come in: {} page(s), {} attachment(s) with {} sidecar(s){}\n",
            self.pages,
            self.attachments,
            self.sidecars,
            if self.knowledge_files > 0 {
                format!(", {} claim or source file(s)", self.knowledge_files)
            } else {
                String::new()
            }
        ));
        if !self.attachment_licenses.is_empty() {
            let licences: Vec<String> = self
                .attachment_licenses
                .iter()
                .map(|(l, n)| format!("{l} ({n})"))
                .collect();
            out.push_str(&format!(
                "attachment licences (each in its sidecar, kept as it is): {}\n",
                licences.join(", ")
            ));
        }
        if self.no_history {
            out.push_str(
                "history: not imported (--no-history): every page comes in with its latest text, in one change\n",
            );
        } else {
            out.push_str(&format!(
                "history: {} record(s) read ({} byte(s)); {} revision(s) would be replayed, one block each (the cap is {})\n",
                self.records, self.history_bytes, self.revisions, self.max_revisions
            ));
            list(&mut out, "revisions by page:", &self.histories, |h| {
                format!(
                    "{}: {} revision(s) of {} record(s)",
                    h.page, h.revisions, h.records
                )
            });
            list(
                &mut out,
                "pages that come in with the first change, having no revision to replay:",
                &self.pages_without_history,
                |p| p.clone(),
            );
            list(
                &mut out,
                "pages whose final file differs from their last revision (one change at the end, `final state of the bundle`):",
                &self.final_state_pages,
                |p| p.clone(),
            );
            if self.suppressed_content + self.suppressed_author + self.suppressed_summary > 0 {
                out.push_str(&format!(
                    "suppressions kept: {} record(s) with their text hidden at the source (not replayed), {} replayed with the author hidden, {} with the summary hidden\n",
                    self.suppressed_content, self.suppressed_author, self.suppressed_summary
                ));
            }
            if !self.withheld.is_empty() {
                let by: Vec<String> = self
                    .withheld
                    .iter()
                    .map(|(why, n)| format!("{n} {}", why))
                    .collect();
                out.push_str(&format!(
                    "withheld by the exporting inklkunn: {} (not replayed)\n",
                    by.join(", ")
                ));
            }
            if !self.not_replayed.is_empty() {
                let by: Vec<String> = self
                    .not_replayed
                    .iter()
                    .map(|(what, n)| format!("{n} {}", what))
                    .collect();
                out.push_str(&format!("records not replayed: {}\n", by.join(", ")));
            }
            if !self.unknown_record_fields.is_empty() {
                let names: Vec<String> = self
                    .unknown_record_fields
                    .iter()
                    .take(MAX_LISTED)
                    .cloned()
                    .collect();
                out.push_str(&format!(
                    "record members this build does not know (kept as written in the bundle, never read): {}\n",
                    names.join(", ")
                ));
            }
            if let Some(s) = self.estimate_seconds {
                out.push_str(&format!(
                    "estimated time of the replay: {} (a fit to the measured cost of building a history through the signer: 500 blocks 154 s, 2,000 blocks 1,438 s; the import's own cost is measured with the replay)\n",
                    duration(s)
                ));
            }
        }
        let people = &self.people;
        out.push_str(&format!(
            "people named: {} (the source's names, as text; none is linked to an account of this server, and they stay in the repository's history for good)\n",
            people.total
        ));
        if !people.by_kind.is_empty() {
            let kinds: Vec<String> = people
                .by_kind
                .iter()
                .map(|(k, n)| format!("{n} {}", k))
                .collect();
            out.push_str(&format!("  by kind: {}\n", kinds.join(", ")));
        }
        if !people.names.is_empty() {
            let names: Vec<String> = people.names.to_vec();
            out.push_str(&format!(
                "  {}{}\n",
                names.join(", "),
                if people.total > people.names.len() {
                    format!(" … and {} more", people.total - people.names.len())
                } else {
                    String::new()
                }
            ));
        }
        list(
            &mut out,
            if self.incompatible_left_out {
                "left out for their licence (--leave-out-incompatible):"
            } else {
                "licences that no wiki with this default may take:"
            },
            &self.incompatible,
            |i| {
                format!(
                    "{} {}: {}{} (the wiki's is {})",
                    match i.kind {
                        FileKind::Page => "page",
                        FileKind::Attachment => "attachment",
                    },
                    i.path,
                    i.license,
                    i.rev
                        .as_ref()
                        .map_or(String::new(), |r| format!(", revision `{}`", r)),
                    self.license.as_deref().unwrap_or("undecided")
                )
            },
        );
        list(&mut out, "left out:", &self.left_out, |l| {
            format!("{} ({} file(s)): {}", l.what, l.files, l.why)
        });
        if !self.unmapped_prefixes.is_empty() {
            out.push_str(
                "link prefixes with no interwiki map to resolve them (the wiki has no map yet: they stay dangling links, not pages to create):\n",
            );
            let mut by_count: Vec<(&String, &usize)> = self.unmapped_prefixes.iter().collect();
            by_count.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
            for (prefix, n) in by_count.into_iter().take(MAX_LISTED) {
                out.push_str(&format!("  {}: {n} link(s)\n", prefix));
            }
            if self.unmapped_prefixes.len() > MAX_LISTED {
                out.push_str(&format!(
                    "  … and {} more prefix(es)\n",
                    self.unmapped_prefixes.len() - MAX_LISTED
                ));
            }
        }
        if self.interwiki_in_bundle > 0 {
            out.push_str(&format!(
                "the bundle's interwiki map has {} prefix(es); this import does not read it yet, so none of the prefixes above is mapped\n",
                self.interwiki_in_bundle
            ));
        }
        out
    }

    /// The report as a JSON value, for the record of the act (`serde_json`; keys are fixed, values are the report's).
    pub fn to_json(&self) -> serde_json::Value {
        let lines = |v: &[String]| serde_json::Value::from(v.to_vec());
        serde_json::json!({
            "bundle": {
                "version": self.bundle_version,
                "generator": self.generator,
                "exported_at": self.exported_at,
                "source_name": self.source_name,
                "source_url": self.source_url,
                "source_engine": self.source_engine,
                "pseudonymized": self.pseudonymized,
                "languages": lines(&self.languages),
                "visibility_default": self.visibility_default,
                "unknown_manifest_keys": lines(&self.unknown_manifest_keys),
            },
            "fidelity": {
                "declared": self.fidelity_declared,
                "found": match self.fidelity_found { Found::History => "history", Found::ContentOnly => "content" },
                "degraded_at_source": lines(&self.degraded_at_source),
                "dropped_at_source": lines(&self.dropped_at_source),
            },
            "license": {
                "wiki_default": self.license,
                "from": self.license_from.map(|f| match f {
                    LicenseFrom::Manifest => "manifest",
                    LicenseFrom::DeclaredByAdmin => "declared-by-admin",
                }),
            },
            "checksums": match &self.checksums {
                Checksums::None => serde_json::json!({"state": "none-in-the-bundle"}),
                Checksums::Failed { file } => {
                    serde_json::json!({"state": "failed", "file": file})
                }
                Checksums::Verified { file, entries } => {
                    serde_json::json!({"state": "verified", "file": file, "entries": entries})
                }
            },
            "contents": {
                "pages": self.pages,
                "attachments": self.attachments,
                "sidecars": self.sidecars,
                "attachment_licenses": self.attachment_licenses,
                "knowledge_files": self.knowledge_files,
            },
            "history": {
                "no_history": self.no_history,
                "records": self.records,
                "revisions": self.revisions,
                "max_revisions": self.max_revisions,
                "bytes": self.history_bytes,
                "pages": self.histories.iter().map(|h| serde_json::json!({
                    "page": h.page, "records": h.records, "revisions": h.revisions,
                })).collect::<Vec<_>>(),
                "pages_without_history": lines(&self.pages_without_history),
                "final_state_pages": lines(&self.final_state_pages),
                "not_replayed": self.not_replayed,
                "suppressed": {
                    "content": self.suppressed_content,
                    "author": self.suppressed_author,
                    "summary": self.suppressed_summary,
                },
                "withheld": self.withheld,
                "unknown_record_fields": lines(&self.unknown_record_fields),
                "estimated_seconds": self.estimate_seconds,
            },
            "people": {
                "total": self.people.total,
                "by_kind": self.people.by_kind,
                "in_users_file": self.people.in_users_file,
                "names": lines(&self.people.names),
            },
            "incompatible": {
                "left_out": self.incompatible_left_out,
                "files": self.incompatible.iter().map(|i| serde_json::json!({
                    "kind": match i.kind { FileKind::Page => "page", FileKind::Attachment => "attachment" },
                    "path": i.path, "license": i.license, "rev": i.rev,
                })).collect::<Vec<_>>(),
            },
            "left_out": self.left_out.iter().map(|l| serde_json::json!({
                "what": l.what, "files": l.files, "why": l.why,
            })).collect::<Vec<_>>(),
            "links": {
                "unmapped_prefixes": self.unmapped_prefixes,
                "interwiki_in_bundle": self.interwiki_in_bundle,
            },
        })
    }
}
