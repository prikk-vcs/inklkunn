#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (B): a BookStack export, converted by wiki-commons (level 2, no history), comes in as a second new wiki: pages and attachments in
//! **one change**, each attachment's sidecar kept as it is (RFC 007 D-1, D-2, D-4; acceptance criterion 2).
//!
//! **Neither converter writes a licence in an attachment's sidecar** (the fixtures' README), so the sidecars' licences are shown by the
//! hand-written bundle of per-file licences, imported as a third wiki. That is said in the transcript.

use super::*;

impl Proof {
    pub(super) fn a_bookstack_export_comes_in(&mut self) {
        self.t.part("(B) a BookStack export, converted by wiki-commons (level 2, no history), comes in as a second NEW wiki: pages and attachments in one change (RFC 007 D-2, D-4; criterion 2)");
        let bundle = fixture("bookstack/bundle");
        let (status, out, err) = self.import(
            "allotment",
            "Allotment Handbook",
            &bundle,
            &["--bundle-license", "CC-BY-SA-4.0"],
        );
        self.t.did("the operator imports the BookStack bundle as `allotment`, declaring the licence (the converter wrote none), with the server running");
        self.t.saw(
            status == 0
                && out.contains("fidelity found:    pages and attachments (no revision that can be replayed)")
                && out.contains("to come in: 5 page(s), 2 attachment(s) with 2 sidecar(s)")
                && out.contains("history:         0 revision(s), one block each, after a first change of the files that have none"),
            "the plan says: level 2, five pages and two attachments with their sidecars, no revision to replay; the run did it in a first change",
            &format!("{out}{err}"),
        );
        let blocks = self.blocks_of("allotment");
        self.t.saw(
            blocks.len() == 2,
            "the wiki has two blocks: its creation and the one change that holds everything the bundle had",
            &format!("{blocks:?}"),
        );
        let changes = self.anonymous.get("/allotment/-/changes");
        let said = plain(main_of(&changes));
        self.t.saw(
            said.contains("Import 5 page(s) and 4 other file(s) with no history")
                && said.contains("Imported from Allotment Handbook on ")
                && said.contains("The license was declared by the importing admin; the bundle declared none."),
            "recent changes names that change (5 pages and 4 other files: the two attachments and their two sidecars) and carries the notice",
            &said,
        );
        for (path, what) in [
            (
                "/allotment/Allotment%20Handbook/Sowing/Seed%20trays",
                "a Markdown page in a chapter of a book",
            ),
            ("/allotment/Calendar", "the HTML page, kept as HTML"),
            ("/allotment/Index", "the first page of the book"),
        ] {
            let page = self.anonymous.get(path);
            self.t.saw(
                page.status == 200 && page.body.contains("License: CC-BY-SA-4.0"),
                &format!("{what} is served under the wiki's licence"),
                &page.body,
            );
        }
        let seed = self.anonymous.get("/allotment/Seed%20trays");
        let calendar = self.anonymous.get("/allotment/Calendar");
        self.t.saw(
            plain(main_of(&seed)).contains("Fill the tray, press the seed in")
                && plain(main_of(&calendar)).contains("Sow peas first.")
                && seed.body.contains("href=\"/allotment/Calendar\""),
            "the pages read as the book wrote them, and a link inside the book resolves",
            &plain(main_of(&seed)),
        );

        // The attachments and their sidecars, kept as they are.
        let export = self.export("allotment", "export-allotment");
        self.t
            .did("the operator exports it: the attachments and their sidecars are in the bundle");
        for rel in [
            "attachments/tray.png",
            "attachments/tray.png.meta.yaml",
            "attachments/notes.txt",
            "attachments/notes.txt.meta.yaml",
            "pages/Allotment Handbook/Sowing/Seed trays.md",
            "pages/Allotment Handbook/Sowing/Calendar.md",
        ] {
            let given = std::fs::read(bundle.join(rel)).unwrap();
            let kept = std::fs::read(export.join(rel)).unwrap();
            self.t.saw(
                given == kept,
                &format!("{rel}: the exported file is the converter's file, byte for byte"),
                &String::from_utf8_lossy(&kept),
            );
        }
        let sidecar =
            std::fs::read_to_string(export.join("attachments/tray.png.meta.yaml")).unwrap();
        self.t.saw(
            !sidecar.contains("license"),
            "the BookStack converter wrote no licence in the sidecar, and the import did not write one: a licence is never invented, the attachment is the wiki's",
            &sidecar,
        );
        let manifest = std::fs::read_to_string(export.join("wiki-bundle.yaml")).unwrap();
        self.t.saw(
            manifest.contains("license: CC-BY-SA-4.0") && manifest.contains("history: true"),
            "the export of the new wiki declares the licence the admin gave it",
            &manifest,
        );

        // Per-file licences: the hand-written bundle, because no converter writes one.
        let orchard = fixture("sidecars/bundle");
        let (status, out, err) = self.import("orchard", "Orchard Plans", &orchard, &[]);
        self.t.did("the operator imports the hand-written bundle of per-file licences as `orchard`: the converters write none, the README says so");
        self.t.saw(
            status == 0
                && out.contains("attachment licences (each in its sidecar, kept as it is): CC-BY-4.0 (1), CC0-1.0 (1)")
                && out.contains("licence of the wiki: CC-BY-SA-4.0 (the bundle's own"),
            "the plan lists the licence of each attachment as its sidecar says it, and the wiki takes the bundle's own licence (no declaration needed)",
            &format!("{out}{err}"),
        );
        let orchard_export = self.export("orchard", "export-orchard");
        for (rel, licence) in [
            ("attachments/plan.svg.meta.yaml", "license: CC0-1.0"),
            ("attachments/notes.txt.meta.yaml", "license: CC-BY-4.0"),
        ] {
            let kept = std::fs::read_to_string(orchard_export.join(rel)).unwrap();
            self.t.saw(
                kept.contains(licence)
                    && kept.as_bytes() == std::fs::read(orchard.join(rel)).unwrap().as_slice(),
                &format!("{rel} still says `{licence}`: the licence is neither rewritten nor dropped, in a wiki of CC-BY-SA-4.0"),
                &kept,
            );
        }
        let changes = self.anonymous.get("/orchard/-/changes");
        let said = plain(main_of(&changes));
        self.t.saw(
            said.contains("Import 1 page(s) and 4 other file(s) with no history")
                && !said.contains("The license was declared by the importing admin"),
            "one change, and the notice does not say the licence was the admin's word: the bundle declared it",
            &said,
        );

        // What a reader can do with an attachment in this build.
        let img = seed
            .body
            .contains("<img src=\"../../../attachments/tray.png\"");
        let tray = self.anonymous.get("/allotment/attachments/tray.png");
        let raw = self.anonymous.get("/allotment/-/raw/attachments/tray.png");
        self.t.saw(
            img,
            "the page's picture is the book's own relative path to the attachment, written as the bundle wrote it",
            &seed.body,
        );
        self.t.note(&format!(
            "FINDING: no route of this build serves an attachment or shows its licence: `/allotment/attachments/tray.png` answers {}, `/allotment/-/raw/attachments/tray.png` answers {} (the raw route is for pages by title; RFC 007 adds none: requirements WEB-10 plans attachments on the content origin). The attachments are in the repository and in every export, with their sidecars; a reader cannot fetch one yet",
            tray.status, raw.status
        ));
    }
}
