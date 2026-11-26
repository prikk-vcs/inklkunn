#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (A): a MediaWiki dump, converted by wiki-commons, comes in as a new wiki with its history, and its interwiki links resolve once
//! the wiki's admin proposes a map through the policy form (RFC 007 D-1 – D-3, D-8; acceptance criteria 1, 8 and 9).

use super::*;

/// The template the story gives the `wikipedia` prefix: its query has an `&`, which a page must write as `&amp;`.
const WIKIPEDIA: &str = "https://en.wikipedia.org/w/index.php?search={title}&ns0=1";
const WIKIPEDIA_COMPOST: &str =
    "href=\"https://en.wikipedia.org/w/index.php?search=Compost&amp;ns0=1\"";

/// The minute-precision times a text gives as `… UTC (originally)`, in the order the text has them.
fn originally(text: &str) -> Vec<String> {
    text.match_indices(" UTC (originally)")
        .filter_map(|(at, _)| text.get(at.checked_sub(16)?..at))
        .map(str::to_owned)
        .collect()
}

/// The blocks a page's history links to as *the page after change N*, newest first.
fn permalinks(history: &str, slug: &str, title: &str) -> Vec<String> {
    let head = format!("href=\"/{slug}/-/at/");
    let tail = format!("/{title}\"");
    history
        .split(&head)
        .skip(1)
        .filter_map(|rest| rest.split_once(&tail).map(|(block, _)| block.to_owned()))
        .collect()
}

/// The policy form of `slug` with the map rows `rows` typed, and sent (a row is added for each one that is wanted).
fn send_map(person: &mut Person, slug: &str, rows: &[(&str, &str)]) -> Reply {
    let form_path = format!("/{slug}/-/policy/edit");
    let mut page = person.browser.get(&form_path);
    assert_eq!(page.status, 200, "{}", page.body);
    let have: usize = form_to(&page, &form_path)
        .get("map_rows")
        .parse()
        .expect("a count");
    for _ in have..rows.len() {
        let form = form_to(&page, &form_path);
        page = person.browser.submit(&form, Some(("act", "map-add")));
        assert_eq!(page.status, 200, "{}", page.body);
    }
    let mut form = form_to(&page, &form_path);
    for (n, (prefix, template)) in rows.iter().enumerate() {
        form.set(&format!("map-{}-prefix", n + 1), prefix);
        form.set(&format!("map-{}-template", n + 1), template);
    }
    person.browser.submit(&form, Some(("act", "send")))
}

impl Proof {
    pub(super) fn a_mediawiki_dump_comes_in_with_its_history(&mut self) {
        self.t.part("(A) a MediaWiki dump, converted by wiki-commons, comes in as a NEW wiki with its history (RFC 007 D-1 – D-3, D-8; criteria 1, 8, 9)");
        let bundle = fixture("mediawiki/bundle");
        let marks_before = self.layout().creation_marks().unwrap();

        // The plan first, as an operator reads it; nothing is written.
        let listed = self.listed();
        let (status, out, err) = self.import(
            "garden",
            "Garden Notes",
            &bundle,
            &["--bundle-license", "CC-BY-SA-4.0", "--dry-run"],
        );
        self.t.did("the operator reads the plan: `wiki import-bundle garden <the converter's bundle> --dry-run --bundle-license CC-BY-SA-4.0`, with the server running");
        self.t.saw(
            status == 0 && out.contains("dry run: nothing was written"),
            "the plan is printed and says nothing was written",
            &format!("{out}{err}"),
        );
        for (what, line) in [
            (
                "the revisions it would replay, one block each",
                "7 revision(s) would be replayed, one block each",
            ),
            (
                "the revision the source hid, counted and not replayed",
                "suppressions kept: 1 record(s) with their text hidden at the wiki it was imported from (not replayed)",
            ),
            (
                "the licence, declared by the admin because the converter wrote none",
                "licence of the wiki: CC-BY-SA-4.0 (the bundle declares none; declared by the importing admin",
            ),
            (
                "the checksums, verified",
                "checksums: verified, every file against `sha256sums.txt`",
            ),
            (
                "the prefix with no map, with how many links use it",
                "wikipedia: 1 link(s)",
            ),
        ] {
            self.t.saw(out.contains(line), what, &out);
        }
        self.t.saw(
            self.listed() == listed && self.layout().creation_marks().unwrap() == marks_before,
            "the store lists the same wikis and no creation mark was made",
            &format!("{:?}", self.listed()),
        );

        // The run.
        let day_before = utc_minutes(now()).get(..10).unwrap_or_default().to_owned();
        let (status, out, err) = self.import(
            "garden",
            "Garden Notes",
            &bundle,
            &["--bundle-license", "CC-BY-SA-4.0"],
        );
        let day_after = utc_minutes(now()).get(..10).unwrap_or_default().to_owned();
        self.t.did("the operator runs it, with the server still running: the converter's bundle becomes the new wiki `garden`");
        self.t.saw(
            status == 0
                && out.contains("history:         7 revision(s), one block each")
                && out.contains("licence:         CC-BY-SA-4.0"),
            "the command says: 7 revisions, one block each, and the licence the admin declared",
            &format!("{out}{err}"),
        );
        let mut slugs = self.listed();
        slugs.sort();
        self.t.saw(
            slugs == ["garden", "handbook"]
                && self.layout().creation_marks().unwrap() == marks_before
                && std::fs::read_dir(&self.world.settings.seed_dir).unwrap().next().is_none(),
            "the store lists the new wiki beside the old one; the creation mark and the signer's seeds are gone",
            &format!("{slugs:?}"),
        );
        let record = self.record("garden");
        let kept = std::fs::read_to_string(self.layout().import_report(&record.id)).unwrap();
        self.t.saw(
            kept.contains("7 revision(s) would be replayed")
                && kept.contains("suppressions kept: 1 record(s) with their text hidden at the wiki it was imported from (not replayed)"),
            "the report is kept beside the repository and says what was not imported (the source's deleted revision)",
            &kept,
        );
        let page = self.anonymous.get("/garden/Composting");
        self.t.saw(
            page.status == 200,
            "the server, which was running before the wiki existed, serves its pages at once",
            &page.head,
        );

        // Every revision a block, in D-2's order.
        let blocks = self.blocks_of("garden");
        self.t.did("the history of the new wiki, block by block");
        self.t.saw(
            blocks.len() == 9,
            "nine blocks: the creation, the seven revisions the bundle can replay (the eighth, the source's deleted one, has no text), and the final state of the bundle",
            &format!("{blocks:?}"),
        );
        let changes = self.anonymous.get("/garden/-/changes");
        let changes_text = plain(main_of(&changes));
        let dates = originally(&changes_text);
        self.t.saw(
            dates == [
                "2025-05-11 15:20",
                "2025-05-10 11:00",
                "2025-05-02 08:10",
                "2025-05-02 07:45",
                "2025-04-09 12:15",
                "2025-04-03 09:30",
                "2025-04-01 08:00",
            ],
            "recent changes lists the seven imported changes newest first by the SOURCE's time, across pages (D-2: merged by `at`, ties in path order)",
            &changes_text,
        );
        self.t.saw(
            changes_text.contains("Import: the final state of the bundle (4 page(s) written, 0 removed)")
                && !changes_text.contains("2025-04-05"),
            "the last change is the wiki's own, `final state of the bundle`; the source's deleted revision (2025-04-05) is nowhere",
            &changes_text,
        );

        // The words of a row, on the page's history.
        let history = self.anonymous.get("/garden/-/history/Composting");
        let said = plain(main_of(&history));
        self.t.did("a reader opens the history of Composting");
        let rows = [
            "Imported. Originally by Jun (a name in the wiki it was imported from), 2025-04-09 12:15 UTC (originally): add sections and links",
            "Imported. Originally by ~ip-d861b7e910 (a name in the wiki it was imported from), 2025-04-03 09:30 UTC (originally): fix a word",
            "Imported. Originally by Mira (a name in the wiki it was imported from), 2025-04-01 08:00 UTC (originally): first draft",
        ];
        let places: Vec<Option<usize>> = rows.iter().map(|r| said.find(r)).collect();
        self.t.saw(
            places.iter().all(Option::is_some) && places.windows(2).all(|w| matches!(w, [a, b] if a < b)),
            "its rows, newest first, each say *Imported. Originally by {name} (a name in the wiki it was imported from), {the source's date} (originally): {the source's summary}*",
            &said,
        );
        self.t.saw(
            said.contains("~ip-d861b7e910 (a name in the wiki it was imported from)"),
            "the anonymous edit says the pseudonym the converter made, as a name of the other wiki, and nothing else of the editor",
            &said,
        );
        self.t.saw(
            said.contains("Made by the wiki service") && said.matches("Imported. Originally by").count() == 3,
            "the wiki's own last change says it was made by the wiki service; three rows are the source's, the deleted revision has none",
            &said,
        );
        let notice_day = [day_before.as_str(), day_after.as_str()]
            .into_iter()
            .map(|d| {
                format!("Imported from Garden Notes (https://garden.example.org/wiki) on {d} ")
            })
            .find(|n| said.contains(n.as_str()));
        self.t.saw(
            notice_day.is_some()
                && said.contains("The files of the bundle were checked against its checksums.")
                && said.contains("The license was declared by the importing admin; the bundle declared none."),
            "the notice names the wiki it was imported from and its address, the day of the import, that the checksums were checked and that the licence is the admin's word",
            &said,
        );
        self.t.saw(
            !history.body.contains("href=\"https://garden.example.org"),
            "the source's address is text, never a link",
            &history.body,
        );
        self.t.note("the page itself carries no notice: the history and recent changes do (the V round deferred a page-view notice: the history is the notice)");

        // A source name that is a local account's is not linked to it.
        let mira = self.join_in("garden", "mira", "Mira", Role::Contributor);
        let mut gwen = self.join_in("garden", "gwen", "Gwen Gardener", Role::Maintainer);
        self.t.did("a local account called Mira joins the new wiki (the source has a Mira too), and Gwen, its maintainer");
        let history = self.anonymous.get("/garden/-/history/Composting");
        let people = gwen.browser.get("/garden/-/people");
        let people_text = plain(main_of(&people));
        self.t.saw(
            people_text.contains("contributor Mira")
                && !people_text.contains("Jun")
                && !history.body.contains("Mira</a>")
                && !history.body.to_lowercase().contains("/-/people/mira")
                && !history.body.contains("href=\"/-/account"),
            "the people page lists the local Mira, never Jun; no name in the history is a link, and the imported Mira is not matched with the local one",
            &people_text,
        );
        let mut mira = mira;
        let theirs = mira.browser.get("/-/account");
        self.t.saw(
            theirs.status == 200 && !plain(&theirs.body).contains("Imported. Originally"),
            "the local Mira's own account page knows nothing of the imported changes",
            &plain(&theirs.body),
        );
        let (status, out, err) = self.cli(&["wiki", "metrics", "garden", "--days", "3650"]);
        self.t.saw(
            status == 0
                && out.contains("\"submitted\":0")
                && out.contains("\"distinct_proposers\":0")
                && out.contains("\"first_time_proposers\":0"),
            "the participation counts (`wiki metrics`) leave the imported changes (the seven and the final state) out: nothing was submitted, nobody proposed",
            &format!("{out}{err}"),
        );

        // The final pages are the bundle's files.
        let export = self.export("garden", "export-garden-1");
        self.t.did("the operator exports the new wiki as a bundle");
        for page in [
            "Composting.md",
            "Worm bins.md",
            "Compost heap.md",
            "Talk/Composting.md",
        ] {
            let given = std::fs::read(bundle.join("pages").join(page)).unwrap();
            let kept = std::fs::read(export.join("pages").join(page)).unwrap();
            self.t.saw(
                given == kept,
                &format!("pages/{page}: the exported file is the converter's file, byte for byte"),
                &String::from_utf8_lossy(&kept),
            );
        }
        let manifest = std::fs::read_to_string(export.join("wiki-bundle.yaml")).unwrap();
        self.t.saw(
            manifest.contains("interwiki: {}"),
            "the export's manifest has an empty interwiki map: the wiki has none yet",
            &manifest,
        );

        // The links dangle until the map exists.
        let before = self.anonymous.get("/garden/Composting");
        let dangling = "class=\"wikilink wikilink-missing\" href=\"/garden/-/proposals/new?new=1&amp;title=wikipedia%3ACompost\"";
        self.t.did("a reader opens Composting, whose text links [[wikipedia:Compost]], before the wiki has an interwiki map");
        self.t.saw(
            before.body.contains(dangling) && !before.body.contains("wikilink-interwiki"),
            "the link is a dangling one (an invitation to write a page called wikipedia:Compost), not an external link",
            &before.body,
        );
        let site = self.anonymous.get("/garden/-/site.json");
        self.t.saw(
            site.body.contains("\"interwiki\":{}"),
            "site.json has an empty map",
            &site.body,
        );

        // The map, through the policy form, at the rung the policy says.
        let sent = send_map(&mut gwen, "garden", &[("wikipedia", WIKIPEDIA)]);
        self.t.did("Gwen opens the policy form and adds one row of the interwiki map: wikipedia → a template with {title} in its query; she sends it for review");
        self.t.saw(
            sent.status == 303,
            "the form is accepted as a proposal",
            &format!("{}\n{}", sent.head, sent.body),
        );
        let id = proposal_id(&sent.location());
        let proposal = gwen.browser.get(&proposal_path("garden", &id));
        let proposal_said = plain(main_of(&proposal));
        self.t.saw(
            proposal_said.contains(&format!("Interwiki prefix added: wikipedia → {WIKIPEDIA}"))
                && !proposal.body.contains("wikipedia: "),
            "the proposal says the change as a sentence, and shows no YAML",
            &proposal_said,
        );
        self.t.saw(
            proposal_said.contains("Accepting this change takes: maintainer or above, with 0 approvals"),
            "and it says at which rung it is accepted: the policy's (maintainer or above, no approvals on a wiki that follows the default rules)",
            &proposal_said,
        );
        let still = self.anonymous.get("/garden/Composting");
        self.t.saw(
            still.body.contains(dangling),
            "until it is accepted the link is still a dangling one: a proposal writes nothing",
            &still.body,
        );
        let blocks_before = self.blocks_of("garden").len();
        let accepted = accept_in("garden", &mut gwen, &id);
        self.t
            .did("Gwen accepts it (the rung the policy names: a maintainer)");
        self.t.saw(
            accepted.status == 200
                && accepted.body.contains("Signed into the history")
                && self.blocks_of("garden").len() == blocks_before + 1,
            "it is signed into the history: one block more",
            &accepted.body,
        );

        // The link now resolves: on the page, on a permalink of an old revision, in site.json and in the export.
        let after = self.anonymous.get("/garden/Composting");
        self.t.did("a reader opens Composting again");
        self.t.saw(
            after.body.contains(WIKIPEDIA_COMPOST)
                && after.body.contains("wikilink wikilink-interwiki")
                && after.body.contains("rel=\"nofollow ugc\"")
                && !after.body.contains("wikilink-missing"),
            "the link is an external one to the template's address with the title percent-encoded and the `&` escaped, rel=\"nofollow ugc\"; no dangling link is left",
            &after.body,
        );
        let history = self.anonymous.get("/garden/-/history/Composting");
        let links = permalinks(&history.body, "garden", "Composting");
        let old = self.anonymous.get(&format!(
            "/garden/-/at/{}/Composting",
            links.get(1).cloned().unwrap_or_default()
        ));
        self.t.did("a reader opens the permalink of change 4, an old revision made before the map existed (W-4: one map per wiki, from the default tip)");
        self.t.saw(
            links.len() == 4
                && old.status == 200
                && old.body.contains(WIKIPEDIA_COMPOST)
                && old.body.contains("rel=\"nofollow ugc\""),
            "the old revision's link resolves through the wiki's map too",
            &format!("{links:?}\n{}", old.body),
        );
        let site = self.anonymous.get("/garden/-/site.json");
        self.t.saw(
            site.body.contains(&format!(
                "\"interwiki\":{{\"wikipedia\":\"{}\"}}",
                WIKIPEDIA.replace('&', "\\u0026")
            )),
            "site.json carries the map (the JSON writes `&` as \\u0026)",
            &site.body,
        );
        let policy = self.anonymous.get("/garden/-/policy");
        self.t.saw(
            plain(main_of(&policy)).contains(WIKIPEDIA),
            "the policy page lists the prefix and its address",
            &plain(main_of(&policy)),
        );
        let export = self.export("garden", "export-garden-2");
        let manifest = std::fs::read_to_string(export.join("wiki-bundle.yaml")).unwrap();
        self.t.did("the operator exports again");
        self.t.saw(
            manifest.contains("interwiki:")
                && manifest.contains("wikipedia")
                && manifest.contains("en.wikipedia.org/w/index.php?search={title}&ns0=1"),
            "the export's manifest carries the map",
            &manifest,
        );
        self.t.saw(
            !std::fs::read_to_string(export.join("governance/interwiki.yaml"))
                .unwrap_or_default()
                .is_empty(),
            "and the bundle carries the governance file of the map",
            &format!(
                "{:?}",
                files_under(&export)
                    .iter()
                    .map(|f| &f.0)
                    .collect::<Vec<_>>()
            ),
        );
    }

    /// A bundle that carries its own interwiki map (the hand-written one: neither converter writes one): the import writes the good
    /// prefixes, leaves out and names the one that is a namespace of its pages, and what the map lacks stays dangling.
    pub(super) fn a_bundle_with_its_own_interwiki_map_comes_in_with_it(&mut self) {
        let bundle = fixture("interwiki/bundle");
        let (status, out, err) = self.import("seeds", "Seed Library", &bundle, &[]);
        self.t.did("the operator imports the hand-written bundle whose manifest has an interwiki map of three prefixes, one of them a namespace of its pages (`help`)");
        self.t.saw(
            status == 0
                && out.contains("forum: 1 link(s)")
                && out.contains("help"),
            "the plan says which prefix the map lacks (`forum`, one link) and names the one it leaves out (`help`)",
            &format!("{out}{err}"),
        );
        let page = self.anonymous.get("/seeds/Seed%20Library");
        self.t.saw(
            page.body.contains("href=\"https://en.wikipedia.org/wiki/Seed%20bank\"")
                && page.body.contains("href=\"https://docs.example.org/read?page=Sowing%20guide\"")
                && page.body.matches("rel=\"nofollow ugc\"").count() == 2
                && page.body.contains("wikilink-missing")
                && page.body.contains("title=forum%3ASeed%20swap"),
            "the two good prefixes are external links at once, under their templates; `forum:` is a dangling link; nothing was proposed or accepted for it",
            &page.body,
        );
        let policy = self.anonymous.get("/seeds/-/policy");
        let said = plain(main_of(&policy));
        self.t.saw(
            said.contains("https://en.wikipedia.org/wiki/{title}")
                && said.contains("https://docs.example.org/read?page={title}")
                && !said.contains("help.example.org"),
            "the policy page lists the two prefixes and not `help`",
            &said,
        );
        let export = self.export("seeds", "export-seeds");
        let manifest = std::fs::read_to_string(export.join("wiki-bundle.yaml")).unwrap();
        self.t.saw(
            manifest.contains("wikipedia")
                && manifest.contains("docs")
                && !manifest.contains("help.example.org"),
            "and the export's manifest carries the map the wiki has",
            &manifest,
        );
    }
}
