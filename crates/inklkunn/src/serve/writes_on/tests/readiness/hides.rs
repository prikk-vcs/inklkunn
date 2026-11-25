#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (c) of the readiness story, on a real prikk, with the hides in place (the readiness review's P-6): **the export** of the
//! wiki (RFC 006 D-13; criterion 13), **the *show hidden content* act** (D-8; criterion 8) and **a hide that declines an open
//! proposal** (D-8; criterion 8). Each is a step of the one scripted run in `readiness.rs`, told in its transcript.

use super::*;

/// What (c) has hidden when the wiki is exported: no byte of a bundle may carry any of it.
const HIDDEN_AT_EXPORT: [Mark; 8] = [
    REV_TEXT,
    REV_SUMMARY,
    REV_AUTHOR,
    COMMENT_BODY,
    COMMENT_AUTHOR,
    NOTE_BODY,
    NOTE_AUTHOR,
    RULES_SUMMARY,
];

/// Every file of a bundle: its path under the bundle's directory, and what it holds.
fn bundle_files(root: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path.strip_prefix(root).unwrap().display().to_string();
                out.push((
                    name,
                    String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out
}

/// The lines of the bundle's JSON Lines files whose names start with `prefix`, as JSON.
fn bundle_lines(files: &[(String, String)], prefix: &str) -> Vec<serde_json::Value> {
    files
        .iter()
        .filter(|(name, _)| name.starts_with(prefix) && name.ends_with(".jsonl"))
        .flat_map(|(_, text)| text.lines())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

/// The member of a JSON value at a pointer, `null` when there is none (tests do not index).
fn member<'a>(line: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    line.pointer(pointer).unwrap_or(&serde_json::Value::Null)
}

impl Story {
    /// The operator's `wiki export --bundle`, to a directory of this name: what it said, and the bundle's files.
    fn export_bundle(&mut self, name: &str) -> (String, Vec<(String, String)>) {
        let dest = self.world.tmp.0.join(name);
        let (status, out, err) = self.world.cli(&[
            "wiki",
            "export",
            "handbook",
            "--bundle",
            dest.to_str().unwrap(),
        ]);
        assert_eq!(status, 0, "{err}");
        (out, bundle_files(&dest))
    }

    /// The export: a bundle written while the hides are in place carries none of what was hidden, and carries the tombstones.
    pub(super) fn export_with_the_hides_in_place(&mut self) {
        self.t.did("the operator exports the wiki as a bundle (`wiki export --bundle`) while the hides are in place");
        let (out, files) = self.export_bundle("export-hidden");
        self.t.saw(
            out.contains(
                "hidden by moderation (entries marked `suppressed`, none carrying what was hidden): ",
            ),
            "the command says that entries are marked hidden by moderation",
            &out,
        );
        for mark in HIDDEN_AT_EXPORT {
            let found: Vec<&str> = files
                .iter()
                .filter(|(_, text)| text.contains(mark.needle))
                .map(|(name, _)| name.as_str())
                .collect();
            self.t.saw(
                found.is_empty(),
                &format!(
                    "none of the {} files of the bundle carries `{}` ({:?} of {:?})",
                    files.len(),
                    mark.needle,
                    mark.part,
                    mark.item
                ),
                &format!("found in {found:?}"),
            );
        }
        // The tombstones: the revision's line in the page's history says what was left out and why, and carries neither.
        let history = bundle_lines(&files, "history/");
        let line = history
            .iter()
            .find(|l| *member(l, "/block") == self.revision_block.as_str())
            .unwrap_or_else(|| panic!("no history line of {}: {history:?}", self.revision_block));
        let names = ["content", "summary", "author"];
        self.t.saw(
            *member(line, "/suppressed") == serde_json::json!(names),
            "the history line of the hidden revision says `suppressed`: content, summary, author",
            &line.to_string(),
        );
        self.t.saw(
            names.iter().all(|n| {
                member(line, &format!("/reasons/{n}"))
                    .as_str()
                    .is_some_and(|reason| !reason.is_empty())
            }),
            "and `reasons` names a category for each part",
            &line.to_string(),
        );
        self.t.saw(
            line.get("summary").is_none() && line.get("authors").is_none(),
            "the line has no summary and no authors: what is hidden is left out, not blanked",
            &line.to_string(),
        );
        // The hidden comment in the claim's thread: its place is kept, its body and its author are not.
        let thread = bundle_lines(&files, "extensions/inklkunn-knowledge/threads/");
        let hidden: Vec<_> = thread
            .iter()
            .filter(|l| l.get("suppressed").is_some())
            .collect();
        self.t.saw(
            hidden.len() == 1
                && hidden
                    .iter()
                    .all(|l| *member(l, "/suppressed") == serde_json::json!(["content", "author"]))
                && hidden
                    .iter()
                    .all(|l| l.get("body").is_none() && l.get("author").is_none()),
            "the claim's thread has one tombstone, with its body and its author left out",
            &format!("{hidden:?}"),
        );
        let manifest = files
            .iter()
            .find(|(name, _)| name == "wiki-bundle.yaml")
            .map(|(_, text)| text.as_str())
            .unwrap_or_default();
        self.t.saw(
            manifest.contains("  tombstones:\n    entries: ")
                && manifest.contains(
                    "    marked_by: suppressed\n    reasons_in: reasons\n    hidden_content: omitted\n",
                ),
            "the manifest's `inklkunn.tombstones` says how the entries are marked and that the hidden content is omitted",
            manifest,
        );
    }

    /// After the unhide: the same export carries what was hidden and no tombstone, so that the absence above is the hide's.
    pub(super) fn export_with_the_hides_undone(&mut self) {
        self.t
            .did("the operator exports again, after Maya restored every hide");
        let (out, files) = self.export_bundle("export-shown");
        self.t.saw(
            !out.contains("hidden by moderation")
                && files
                    .iter()
                    .all(|(_, text)| !text.contains("\"suppressed\"")),
            "no entry is marked hidden",
            &out,
        );
        for mark in [REV_SUMMARY, NOTE_BODY, NOTE_AUTHOR] {
            self.t.saw(
                files.iter().any(|(_, text)| text.contains(mark.needle)),
                &format!(
                    "`{}` is in the bundle again: the earlier absence was the hide's",
                    mark.needle
                ),
                "",
            );
        }
    }

    /// The rows of the audit trail of the *show hidden content* act: (principal, outcome, detail).
    fn shown_in_the_audit(&self) -> Vec<(String, String, String)> {
        let db = rusqlite::Connection::open(self.world.settings.store_path()).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        let mut query = db
            .prepare("SELECT principal, outcome, detail FROM audit_log WHERE action = 'tombstone-show' ORDER BY seq")
            .unwrap();
        query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    /// *Show hidden content*, the moderator's audited act: the audit line carries the rung and no content, the answer is never
    /// stored, a person who may not moderate is refused, and what readers are served does not change.
    pub(super) fn show_hidden_content(&mut self, revision_query: &str) {
        self.t.did("Maya asks to see what she hid of Zora's revision: *show hidden content*, on the item's moderation form");
        let readers = self.reader_pages_of_the_revision();
        let audited = self.shown_in_the_audit();
        let page = self
            .maya
            .browser
            .get(&format!("{MODERATE}?{revision_query}"));
        let shows: Vec<Form> = forms_of(&page.body)
            .into_iter()
            .filter(|f| f.action == format!("{MODERATE}/show"))
            .collect();
        self.t.saw(
            !shows.is_empty(),
            &format!(
                "the form offers *show hidden content* for {} hide(s) in force",
                shows.len()
            ),
            &page.body,
        );
        let mut seen = String::new();
        for form in &shows {
            let revealed = self.maya.browser.submit(form, None);
            assert_eq!(revealed.status, 200, "{}\n{}", revealed.head, revealed.body);
            self.t.saw(
                revealed
                    .header("cache-control")
                    .is_some_and(|v| v.contains("no-store")),
                "the answer carries `Cache-Control: no-store`",
                &revealed.head,
            );
            seen.push_str(&revealed.body);
        }
        for mark in [REV_TEXT, REV_SUMMARY, REV_AUTHOR] {
            self.t.saw(
                seen.contains(mark.needle),
                &format!("the moderator is shown `{}` ({:?})", mark.needle, mark.part),
                &plain(&seen),
            );
        }
        let rows = self.shown_in_the_audit();
        let new: Vec<_> = rows.iter().skip(audited.len()).collect();
        self.t.saw(
            new.len() == shows.len()
                && new.iter().all(|(_, outcome, detail)| {
                    outcome == "allowed"
                        && detail.contains("rung=maintainer;")
                        && detail.contains("show; item=page-revision:")
                }),
            "each act is one line of the audit trail, allowed, naming the rung it was done at and the item",
            &format!("{new:?}"),
        );
        self.t.saw(
            new.iter().all(|(_, _, detail)| {
                HIDDEN_AT_EXPORT
                    .iter()
                    .all(|mark| !detail.contains(mark.needle))
            }),
            "and no line carries a word of what was shown",
            &format!("{new:?}"),
        );
        // A person who may not moderate sends the same act with her own token.
        let mut forged = shows.first().cloned().expect("a form");
        let token = self.noor.browser.csrf();
        forged.set("csrf", &token);
        let refused = self.noor.browser.submit(&forged, None);
        self.t.saw(
            refused.status == 403 && !refused.body.contains(REV_TEXT.needle),
            "Noor, a contributor, sending the same act is refused (403) and is shown nothing",
            &refused.body,
        );
        self.t.saw(
            self.shown_in_the_audit().len() == rows.len(),
            "and the refused attempt is no line of the show act",
            "",
        );
        let after = self.reader_pages_of_the_revision();
        self.t.saw(
            after == readers,
            "the pages a reader is served are the same bytes as before the moderator looked",
            "",
        );
    }

    /// A hide that declines: hiding the text of the latest revision of an open proposal declines it in the same act, tells its
    /// author in a sentence that names no person, and says so in the moderation log.
    pub(super) fn a_hide_declines_an_open_proposal(&mut self) {
        self.t.did("Noor opens one more proposal, which stays open");
        let id = propose_edit(&mut self.noor, "Help", "Say it plainly", |text| {
            assert!(text.contains(TRUST_AFTER), "{text}");
            text.replacen(
                TRUST_AFTER,
                "It never claims more than it checked, and says so plainly.",
                1,
            )
        });
        let state_of = |story: &Story, id: &str| {
            inklkunn_store::ProposalStore::proposal(
                &*story.world.store,
                &inklkunn_store::ProposalId::parse(id).unwrap(),
                None,
            )
            .unwrap()
            .expect("the proposal")
            .state
        };
        self.t.saw(
            state_of(self, &id).is_open(),
            "it is held for review",
            &format!("{:?}", state_of(self, &id)),
        );
        let query = format!("kind=proposal-revision&proposal={id}&seq=1");
        let form = self.maya.browser.get(&format!("{MODERATE}?{query}"));
        self.t.saw(
            plain(&form.body).contains("also declines the proposal"),
            "the moderation form of its text says that hiding it also declines the proposal",
            &plain(&form.body),
        );
        self.t
            .did("Maya hides the text of its latest revision, through the form");
        let done = self.hide(&query, &["part_content"]);
        self.t.saw(
            plain(&done.body).contains("Hidden, and the proposal was declined with it."),
            "the form says the proposal was declined with it",
            &plain(&done.body),
        );
        self.t.saw(
            state_of(self, &id) == inklkunn_store::ProposalState::Declined,
            "the proposal is declined",
            &format!("{:?}", state_of(self, &id)),
        );
        let opened = plain(&self.noor.browser.get(&proposal_page(&id)).body);
        self.t.saw(
            opened.contains("Moderation declined this proposal (") && !opened.contains("Maya"),
            "its author reads the neutral sentence on the proposal: moderation declined it, with a category and no name",
            &opened,
        );
        let inbox = plain(&self.noor.browser.get("/-/inbox").body);
        self.t.saw(
            inbox.contains("Moderation declined your proposal Say it plainly in Handbook ("),
            "and the same, which says no person's name, in her inbox",
            &inbox,
        );
        let log = plain(&self.anonymous.get(LOG).body);
        self.t.saw(
            log.contains("The proposal was declined in the same act."),
            "the moderation log says: the proposal was declined in the same act",
            &log,
        );
        let open = self.open.clone();
        self.t.saw(
            state_of(self, &open).is_open(),
            "the proposal whose comment was hidden, and not its text, stays open",
            &format!("{:?}", state_of(self, &open)),
        );
    }
}
