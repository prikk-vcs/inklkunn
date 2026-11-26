#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (C): **a round trip** (RFC 007 D-3, D-7; acceptance criteria 3, 4 and 5). An inklkunn wiki is made through the real server: pages
//! edited, one edited by two accounts, one **renamed**, one **removed**, and the text of a revision that is not the tip **hidden by
//! moderation**. It is exported (`wiki export --bundle`) and the bundle's files are checked: the history records with their fields,
//! the renames and the removals as records, the hidden text in no file, the checksums against an independent SHA-256. The bundle is
//! imported as a new wiki; its pages, history, renames and removals are the first wiki's, its authors are said as D-3 says, and the
//! hidden part is still absent; it is exported again and compared to the first export.

use std::collections::{BTreeMap, BTreeSet};

use super::sha::{known, sha256_hex};
use super::*;

const MODERATE: &str = "/handbook/-/moderate";
const ZORA: &str = "Zora Quokka";
const HIDDEN_TEXT: &str = "QUOKKATEXT";
const HIDDEN_SUMMARY: &str = "QUOKKASUMMARY";

fn page(title: &str, body: &str) -> String {
    format!("---\ntitle: {title}\n---\n\n{body}\n")
}

/// The member of a JSON value at a pointer, `null` when there is none (a test may not index).
pub(super) fn member<'a>(v: &'a serde_json::Value, pointer: &str) -> &'a serde_json::Value {
    v.pointer(pointer).unwrap_or(&serde_json::Value::Null)
}

/// Every history file of a bundle: its name, and its lines as JSON.
fn histories(bundle: &Path) -> BTreeMap<String, Vec<serde_json::Value>> {
    let mut out = BTreeMap::new();
    for (name, bytes) in files_under(&bundle.join("history")) {
        let lines = String::from_utf8(bytes)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        out.insert(name, lines);
    }
    out
}

/// The files of a bundle that carry a word, by path.
fn carrying(bundle: &Path, word: &str) -> Vec<String> {
    files_under(bundle)
        .into_iter()
        .filter(|(_, bytes)| String::from_utf8_lossy(bytes).contains(word))
        .map(|(name, _)| name)
        .collect()
}

/// What a record says of a page's text and who wrote it, without the ids that belong to one wiki's blocks.
fn essence(record: &serde_json::Value) -> String {
    [
        "/type",
        "/at",
        "/summary",
        "/content",
        "/sha256",
        "/author/name",
        "/author/kind",
        "/from",
        "/to",
    ]
    .iter()
    .map(|p| member(record, p).to_string())
    .collect::<Vec<_>>()
    .join("|")
}

impl Proof {
    /// A script's bearer token.
    pub(super) fn token(&self, login: &str, scope: Scope) -> String {
        let auth = self.running.services.auth.clone().expect("sign-in");
        auth.issue_token(
            &self.world.account_of(login),
            scope,
            "r5-proof",
            3600,
            now(),
        )
        .unwrap()
        .secret
        .as_str()
        .to_owned()
    }

    /// The tip's state id, which a proposal sent through the API names.
    pub(super) fn tip(&self) -> String {
        let info = api(
            &self.running,
            "GET",
            &format!("{API}/raw/Help"),
            None,
            None,
            None,
        );
        assert_eq!(info.status, 200, "{}", info.body);
        format!("\"{}\"", json_field(&info.body, "state_id").expect("a tip"))
    }

    /// A proposal of `handbook` sent for review through the API by `who`: its id.
    pub(super) fn sent(&self, who: &str, summary: &str, files: &str, renames: &str) -> String {
        let token = self.token(who, Scope::Propose);
        let body = format!(
            "{{\"summary\":{},\"submit\":true,\"files\":[{files}],\"renames\":[{renames}]}}",
            json_quote(summary)
        );
        let made = api(
            &self.running,
            "POST",
            &format!("{API}/proposals"),
            Some(&token),
            Some(&self.tip()),
            Some(&body),
        );
        assert_eq!(made.status, 201, "{}\n{}\n{body}", made.head, made.body);
        json_field(&made.body, "id").expect("proposal id")
    }

    /// Maya (a maintainer) approves proposal `id` and accepts it.
    pub(super) fn accepted(&self, maya: &mut Person, id: &str) {
        let accepted = approve_and_accept(maya, id);
        assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
        assert!(
            accepted.body.contains("Signed into the history"),
            "{}",
            accepted.body
        );
    }

    /// Hide `parts` of the item whose moderation form is at `query`, as Maya, through the form.
    pub(super) fn hide(&self, maya: &mut Person, query: &str, parts: &[&str]) {
        let page = maya.browser.get(&format!("{MODERATE}?{query}"));
        assert_eq!(page.status, 200, "{query}: {}", page.body);
        let mut form = form_to(&page, MODERATE);
        for part in parts {
            form.check(part, "1");
        }
        let category = form
            .choices
            .iter()
            .find(|c| c.0 == "category")
            .map(|c| c.1.clone())
            .expect("a category");
        form.check("category", &category);
        let hid = maya.browser.submit(&form, None);
        assert_eq!(hid.status, 200, "{query}: {}\n{}", hid.head, hid.body);
    }

    /// Every file of the bundle `dir` is named by its checksums with the SHA-256 it has (the independent function), and every name is a file.
    fn assert_checksums(&mut self, dir: &Path, which: &str) {
        let sums = std::fs::read_to_string(dir.join("sha256sums.txt")).unwrap();
        let mut named: BTreeMap<String, String> = BTreeMap::new();
        for line in sums.lines() {
            let (hex, path) = line.split_once("  ").unwrap();
            named.insert(path.to_owned(), hex.to_owned());
        }
        let mut wrong = Vec::new();
        let mut files = 0;
        for (name, bytes) in files_under(dir) {
            if name == "wiki-bundle.yaml" || name == "sha256sums.txt" {
                continue;
            }
            files += 1;
            if named.get(&name) != Some(&sha256_hex(&bytes)) {
                wrong.push(name);
            }
        }
        self.t.saw(
            wrong.is_empty() && named.len() == files,
            &format!("{which}: `sha256sums.txt` names every one of the {files} files with the SHA-256 it has (checked with an independent SHA-256), and names nothing else"),
            &format!("{wrong:?}\n{sums}"),
        );
    }

    /// Every record of every history file of `bundle` that has a text has the `sha256` of it.
    fn assert_record_digests(&mut self, bundle: &Path, which: &str) {
        let mut checked = 0;
        let mut wrong = Vec::new();
        for (file, lines) in histories(bundle) {
            for line in lines {
                if let Some(content) = line.get("content").and_then(|c| c.as_str()) {
                    checked += 1;
                    if member(&line, "/sha256").as_str()
                        != Some(sha256_hex(content.as_bytes()).as_str())
                    {
                        wrong.push(format!("{file} {}", member(&line, "/rev")));
                    }
                }
            }
        }
        self.t.saw(
            wrong.is_empty() && checked > 0,
            &format!("{which}: each of the {checked} history records that carries a text has the `sha256` of that text"),
            &format!("{wrong:?}"),
        );
    }

    pub(super) fn a_round_trip(&mut self) {
        self.t.part("(C) a round trip: an inklkunn wiki, with a rename, a removal and a hidden revision, exported, imported as a new wiki and exported again (RFC 007 D-3, D-7; criteria 3, 4, 5)");
        self.t.saw(
            known(),
            "the SHA-256 this proof checks the bundle with agrees with the published vectors (it is not the product's own code run again)",
            "",
        );
        let mut maya = join(
            &self.world,
            &self.running,
            "maya",
            "Maya Maintainer",
            Role::Maintainer,
        );
        let _noor = join(
            &self.world,
            &self.running,
            "noor",
            "Noor Contributor",
            Role::Contributor,
        );
        let _zora = join(&self.world, &self.running, "zora", ZORA, Role::Contributor);
        self.t.did("the operator invited Maya (maintainer), Noor and Zora Quokka (contributors) to `handbook`; each signed in");

        // The wiki, made through the server.
        let started = self.sent(
            "noor",
            "Start the notes",
            &format!(
                "{{\"title\":\"Notes\",\"text\":{}}}",
                json_quote(&page("Notes", "The first draft of the notes."))
            ),
            "",
        );
        self.accepted(&mut maya, &started);
        let zoras = self.sent(
            "zora",
            &format!("{HIDDEN_SUMMARY} add a line"),
            &format!(
                "{{\"path\":\"pages/Notes.md\",\"text\":{}}}",
                json_quote(&page(
                    "Notes",
                    &format!(
                        "The first draft of the notes.\n\n{HIDDEN_TEXT} a line only Zora wrote."
                    )
                ))
            ),
            "",
        );
        self.accepted(&mut maya, &zoras);
        let rewritten = self.sent(
            "noor",
            "Rewrite the notes",
            &format!(
                "{{\"path\":\"pages/Notes.md\",\"text\":{}}}",
                json_quote(&page("Notes", "The second draft of the notes."))
            ),
            "",
        );
        self.accepted(&mut maya, &rewritten);
        let draft = self.sent(
            "noor",
            "Start a draft",
            &format!(
                "{{\"title\":\"Draft\",\"text\":{}}}",
                json_quote(&page("Draft", "A plan."))
            ),
            "",
        );
        self.accepted(&mut maya, &draft);
        let moved = self.sent(
            "noor",
            "Name it a plan",
            "",
            "{\"from\":\"pages/Draft.md\",\"to\":\"pages/Plan.md\"}",
        );
        self.accepted(&mut maya, &moved);
        let scratch = self.sent(
            "noor",
            "Start a scratch page",
            &format!(
                "{{\"title\":\"Scratch\",\"text\":{}}}",
                json_quote(&page("Scratch", "Throw away."))
            ),
            "",
        );
        self.accepted(&mut maya, &scratch);
        let gone = self.sent(
            "noor",
            "Remove the scratch page",
            "{\"path\":\"pages/Scratch.md\",\"text\":null}",
            "",
        );
        self.accepted(&mut maya, &gone);
        self.t.did("through the API and the accept form: Noor starts Notes, Zora adds a line to it, Noor rewrites it (two accounts edited it); Noor starts Draft and renames it Plan; she starts Scratch and removes it; Maya reviews and accepts each");
        let notes = self.anonymous.get("/handbook/-/history/Notes");
        let said = plain(main_of(&notes));
        self.t.saw(
            said.contains("Proposed by Zora Quokka")
                && said.contains(&format!("{HIDDEN_SUMMARY} add a line")),
            "before the hide, the history of Notes names Zora and her summary",
            &said,
        );
        let plan = self.anonymous.get("/handbook/-/history/Plan");
        let scratch_page = self.anonymous.get("/handbook/Scratch");
        self.t.saw(
            plain(main_of(&plan)).contains("The page was moved here from Draft")
                && scratch_page.status == 410,
            "Plan says it was moved here from Draft, and Scratch answers 410 Gone",
            &plain(main_of(&plan)),
        );

        // The hide: a revision that is not the tip.
        self.hide(
            &mut maya,
            &format!("kind=proposal-revision&proposal={zoras}&seq=1"),
            &["part_content", "part_summary", "part_author"],
        );
        self.t.did("Maya hides, through the moderation form, the text, summary and author of Zora's revision of Notes, which a later change of Noor's replaced");
        let notes = self.anonymous.get("/handbook/-/history/Notes");
        let said = plain(main_of(&notes));
        self.t.saw(
            !said.contains(ZORA)
                && !said.contains(HIDDEN_SUMMARY)
                && said.contains("Hidden by moderation"),
            "the page's history says it is hidden and does not name her or her summary",
            &said,
        );

        // The export.
        let first = self.export("handbook", "export-handbook-1");
        self.t.did("the operator exports `handbook`: `wiki export handbook --bundle <new directory>`, while the server runs");
        let files: Vec<String> = files_under(&first).into_iter().map(|f| f.0).collect();
        self.t.saw(
            ["pages/Notes.md", "pages/Plan.md", "pages/Help.md", "history/~%4Eotes.jsonl", "history/~%50lan.jsonl", "history/~%53cratch.jsonl", "users.yaml", "sha256sums.txt", "wiki-bundle.yaml"]
                .iter()
                .all(|f| files.iter().any(|g| g == f)),
            "the bundle has the pages (Scratch is not one: it was removed), a history file for each page that has one, the removed page's too, `users.yaml`, the checksums and the manifest",
            &format!("{files:?}"),
        );
        self.t.saw(
            !files.iter().any(|f| f == "pages/Scratch.md")
                && !files.iter().any(|f| f == "pages/Draft.md"),
            "neither the removed page nor the old name of the renamed one is a page of the bundle",
            &format!("{files:?}"),
        );
        let records = histories(&first);
        let notes_records = records.get("~%4Eotes.jsonl").cloned().unwrap_or_default();
        self.t.saw(
            notes_records.len() == 3
                && notes_records.iter().all(|r| {
                    ["/rev", "/at", "/type"].iter().all(|p| member(r, p).is_string())
                        && r.get("parent").is_some()
                }),
            "the history of Notes has three records, each with `rev`, `parent`, `at` and `type`: the edit of two accounts is there",
            &format!("{notes_records:?}"),
        );
        let (create, hidden, last) = (
            notes_records.first().cloned().unwrap_or_default(),
            notes_records.get(1).cloned().unwrap_or_default(),
            notes_records.get(2).cloned().unwrap_or_default(),
        );
        self.t.saw(
            member(&create, "/type") == "create"
                && member(&create, "/parent").is_null()
                && member(&create, "/author/name") == "Noor Contributor"
                && member(&create, "/summary") == "Start the notes"
                && member(&create, "/content") == &serde_json::json!(page("Notes", "The first draft of the notes."))
                && member(&last, "/type") == "edit"
                && member(&last, "/parent") == member(&hidden, "/rev"),
            "the first record is a `create` by Noor with her summary and the page's full text; the third is an `edit` whose `parent` is the hidden revision: the chain is whole",
            &format!("{create}\n{last}"),
        );
        self.t.saw(
            member(&hidden, "/type") == "edit"
                && member(&hidden, "/suppressed") == &serde_json::json!(["content", "summary", "author"])
                && hidden.get("content").is_none()
                && hidden.get("summary").is_none()
                && hidden.get("author").is_none()
                && hidden.get("sha256").is_none()
                && member(&hidden, "/reasons").is_object(),
            "the hidden revision is a record with `suppressed: [content, summary, author]`, a reason for each part and none of the three",
            &hidden.to_string(),
        );
        let plan_records = records.get("~%50lan.jsonl").cloned().unwrap_or_default();
        let rename = plan_records.get(1).cloned().unwrap_or_default();
        self.t.saw(
            plan_records.len() == 2
                && member(&rename, "/type") == "rename"
                && member(&rename, "/from") == "Draft"
                && member(&rename, "/to") == "Plan"
                && member(&rename, "/summary") == "Name it a plan"
                && member(&rename, "/inklkunn/from_path") == "pages/Draft.md"
                && member(&rename, "/inklkunn/path") == "pages/Plan.md",
            "the rename is a record: `type: rename`, `from: Draft`, `to: Plan`, with the summary it was made with",
            &format!("{plan_records:?}"),
        );
        let scratch_records = records.get("~%53cratch.jsonl").cloned().unwrap_or_default();
        let removal = scratch_records.get(1).cloned().unwrap_or_default();
        self.t.saw(
            scratch_records.len() == 2
                && member(&removal, "/type") == "delete"
                && member(&removal, "/summary") == "Remove the scratch page"
                && removal.get("content").is_none(),
            "the removal is a record of the page that is no longer a page: `type: delete`, its summary, no text",
            &format!("{scratch_records:?}"),
        );
        let manifest = std::fs::read_to_string(first.join("wiki-bundle.yaml")).unwrap();
        self.t.saw(
            manifest.contains("  histories_not_written: 0\n") && manifest.contains("  revisions_withheld: 0\n"),
            "the manifest says no history was left unwritten and no revision was withheld by the exporter",
            &manifest,
        );
        self.assert_record_digests(&first, "export 1");

        // What was hidden is in no file.
        let account = self.world.account_of("zora");
        for (word, what) in [
            (HIDDEN_TEXT, "the hidden text"),
            (HIDDEN_SUMMARY, "the hidden summary"),
            (ZORA, "the hidden author's name"),
            (account.as_str(), "the hidden author's account id"),
        ] {
            let found = carrying(&first, word);
            self.t.saw(
                found.is_empty(),
                &format!(
                    "export 1: {what} is in none of the bundle's {} files",
                    files.len()
                ),
                &format!("found in {found:?}"),
            );
        }
        let users = std::fs::read_to_string(first.join("users.yaml")).unwrap();
        self.t.saw(
            users.contains("Maya Maintainer") && users.contains("Noor Contributor"),
            "`users.yaml` names the people whose work is in the bundle, and not the one whose name was hidden",
            &users,
        );
        self.assert_checksums(&first, "export 1");

        // The import.
        let marks = self.layout().creation_marks().unwrap();
        let (status, out, err) = self.import("handbook2", "Handbook again", &first, &[]);
        self.t.did("the operator imports that bundle as a new wiki: `wiki import-bundle handbook2 <the export>`");
        self.t.saw(
            status == 0
                && out.contains("checksums: verified, every file against `sha256sums.txt`")
                && out.contains("licence of the wiki: CC-BY-SA-4.0 (the bundle's own")
                && out.contains("one block each"),
            "the plan verifies the checksums and takes the bundle's own licence; the run replays the history one block each",
            &format!("{out}{err}"),
        );
        self.t.saw(
            self.layout().creation_marks().unwrap() == marks
                && self.listed().contains(&"handbook2".to_owned()),
            "the new wiki is listed, and no creation mark is left",
            &format!("{:?}", self.listed()),
        );

        // The same pages, history, renames and removals; authors said as D-3 says.
        let second = self.export("handbook2", "export-handbook-2");
        self.t
            .did("the operator exports the new wiki, and compares the two bundles");
        let pages_of = |dir: &Path| -> BTreeMap<String, Vec<u8>> {
            files_under(dir)
                .into_iter()
                .filter(|(n, _)| n.starts_with("pages/"))
                .collect()
        };
        self.t.saw(
            pages_of(&first) == pages_of(&second) && pages_of(&first).len() == 3,
            "the pages are the same, byte for byte: Help, Notes, and Plan (under the name the rename gave it)",
            &format!("{:?} {:?}", pages_of(&first).keys(), pages_of(&second).keys()),
        );
        let records2 = histories(&second);
        let names = |h: &BTreeMap<String, Vec<serde_json::Value>>| -> BTreeSet<String> {
            h.keys().cloned().collect()
        };
        self.t.saw(
            names(&records)
                .iter()
                .filter(|n| n.starts_with('~'))
                .all(|n| records2.contains_key(n)),
            "a history file for each page the first wiki had one for, the renamed and the removed page included",
            &format!("{:?} {:?}", names(&records), names(&records2)),
        );
        for file in ["~%4Eotes.jsonl", "~%50lan.jsonl", "~%53cratch.jsonl"] {
            let before: Vec<String> = records
                .get(file)
                .map(|r| {
                    r.iter()
                        .filter(|x| x.get("suppressed").is_none())
                        .map(essence)
                        .collect()
                })
                .unwrap_or_default();
            let after: Vec<String> = records2
                .get(file)
                .map(|r| r.iter().map(essence).collect())
                .unwrap_or_default();
            self.t.saw(
                !before.is_empty() && before == after,
                &format!("history/{file}: the same records in the same order (type, the source's time, summary, text, its SHA-256, the author's name and kind, `from`/`to`), the hidden one not among them"),
                &format!("{before:#?}\n{after:#?}"),
            );
        }
        let first_revs: BTreeSet<String> = records
            .values()
            .flatten()
            .filter_map(|r| member(r, "/rev").as_str().map(str::to_owned))
            .collect();
        let imported: Vec<&serde_json::Value> = records2
            .values()
            .flatten()
            .filter(|r| member(r, "/inklkunn/imported_rev").is_string())
            .collect();
        self.t.saw(
            imported.len() >= 6
                && imported.iter().all(|r| {
                    // The stock Help page of the new wiki's own first block is newer than the source's time for the page, so the export
                    // says the time it gives is the one before it (`ordered`): a time never runs backwards in a file.
                    (member(r, "/inklkunn/at_from") == "source"
                        || (member(r, "/inklkunn/path") == "pages/Help.md" && member(r, "/inklkunn/at_from") == "ordered"))
                        && member(r, "/inklkunn/kind") == "import"
                        && !first_revs.contains(member(r, "/rev").as_str().unwrap_or_default())
                        && first_revs.contains(member(r, "/inklkunn/imported_rev").as_str().unwrap_or_default())
                }),
            &format!("each of the {} imported records says its time is the source's (`at_from: source`; `ordered` for the page the new wiki's own first block wrote before it) and names the first wiki's revision it came from; the revision ids themselves all differ (the new wiki signed its own blocks)", imported.len()),
            &format!("{imported:#?}"),
        );
        self.t.saw(
            member(&records2.get("~%53cratch.jsonl").cloned().unwrap_or_default().get(1).cloned().unwrap_or_default(), "/type") == "delete"
                && member(&records2.get("~%50lan.jsonl").cloned().unwrap_or_default().get(1).cloned().unwrap_or_default(), "/type") == "rename",
            "the removal is a `delete` record and the rename a `rename` record in the second export as in the first",
            "",
        );
        let notes_history = self.anonymous.get("/handbook2/-/history/Notes");
        let said = plain(main_of(&notes_history));
        let at_of = |r: &serde_json::Value| -> String {
            let at = member(r, "/at")
                .as_str()
                .unwrap_or_default()
                .replace('T', " ");
            at.get(..16).unwrap_or_default().to_owned()
        };
        self.t.saw(
            said.contains(&format!(
                "Imported. Originally by Noor Contributor (a name in the wiki it was imported from), {} UTC (originally): Rewrite the notes",
                at_of(&last)
            )) && said.contains("(originally): Start the notes")
                && !said.contains(ZORA)
                && !said.contains(HIDDEN_SUMMARY)
                && said.matches("Imported. Originally by").count() == 2,
            "the second wiki's history of Notes says *Imported. Originally by Noor Contributor (a name in the wiki it was imported from), {the first wiki's time} UTC (originally): …* twice, and names no one else: the hidden revision was not replayed",
            &said,
        );
        for title in ["Notes", "Plan", "Draft", "Scratch", "Help"] {
            let (a, b) = (
                self.anonymous.get(&format!("/handbook/{title}")),
                self.anonymous.get(&format!("/handbook2/{title}")),
            );
            self.t.saw(
                a.status == b.status,
                &format!("/{title}: the same answer in both wikis ({})", a.status),
                &format!("{} {}", a.status, b.status),
            );
        }
        let gone = self.anonymous.get("/handbook2/-/history/Scratch");
        let moved = self.anonymous.get("/handbook2/-/history/Plan");
        self.t.saw(
            plain(main_of(&gone)).contains("The page was removed")
                && plain(main_of(&moved)).contains("The page was moved here from Draft"),
            "the second wiki's histories say the page was removed, and that Plan was moved here from Draft",
            &plain(main_of(&moved)),
        );
        for (word, what) in [
            (HIDDEN_TEXT, "text"),
            (HIDDEN_SUMMARY, "summary"),
            (ZORA, "author"),
        ] {
            let found = carrying(&second, word);
            self.t.saw(
                found.is_empty(),
                &format!("export 2: the hidden {what} is in none of the files either"),
                &format!("found in {found:?}"),
            );
        }
        let api_history = self.anonymous.get("/handbook2/-/api/v1/history/Notes");
        self.t.saw(
            api_history.status == 200
                && !api_history.body.contains(HIDDEN_SUMMARY)
                && !api_history.body.contains(ZORA),
            "the second wiki's API history of Notes carries none of it",
            &api_history.body,
        );
        self.assert_record_digests(&second, "export 2");
        self.assert_checksums(&second, "export 2");
        self.t.note("the revision ids of the two exports differ for every change (each wiki signs its own blocks); the pages, the records' texts, summaries, authors' names, kinds and the source's times are the same");
    }
}
