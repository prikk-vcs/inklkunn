#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (G): **a wiki with claims round-trips** (RFC 007 criterion 3; the Z-1 correction of the readiness review, the gap RU-1 found). An
//! inklkunn wiki has a claim on a page that has a history, with a source; **a curation proposal sets its community status, and another a
//! review day and an availability of the source**. It is exported and imported as a new wiki, **which used to be refused** (the claim's
//! page was not there at the first change, and a status is a curation's to set). Now the claim comes back with its sources and its anchor, **unsettled**:
//! the status, the review day and the availability are not imported, the dry run and the run say how many, the history says it, and the
//! claim's status can afterwards be set by a curation proposal like any other (INV-11 is untouched).

use super::super::knowledge::{SURVEY, attach, curate, group_of, ids_after, signed_into_history};
use super::roundtrip::{histories, member};
use super::*;

/// The slug the claims come back in.
const COPY: &str = "handbook-claims";

impl Proof {
    fn signed_in(&self, login: &'static str, name: &'static str) -> Person {
        let password = password_for(login);
        let mut browser = self.running.browser();
        sign_in(&mut browser, login, &password, "/");
        Person {
            login,
            name,
            password,
            browser,
        }
    }

    /// The curation form of `claim` in the wiki `slug`, changed by `edit` and sent for review by `person`: the proposal's id.
    fn curated_in(
        &self,
        slug: &str,
        person: &mut Person,
        claim: &str,
        edit: impl Fn(&mut Form),
        summary: &str,
    ) -> String {
        let uri = format!("/{slug}/-/proposals/curate?claim={claim}");
        let page = person.browser.get(&uri);
        assert_eq!(page.status, 200, "{uri}: {}", page.body);
        let mut form = form_to(&page, &format!("/{slug}/-/proposals/curate"));
        edit(&mut form);
        form.set("summary", summary);
        let sent = person.browser.submit(&form, Some(("op", "submit")));
        assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
        proposal_id(&sent.location())
    }

    /// A reviewer's approval and a maintainer's acceptance of proposal `id` of the wiki `slug`, by `maintainer`.
    fn approved_and_accepted_in(&self, slug: &str, maintainer: &mut Person, id: &str) -> Reply {
        let path = proposal_path(slug, id);
        let page = maintainer.browser.get(&path);
        let mut form = form_to(&page, &format!("{path}/decide"));
        form.set("body", "Read, and it says what it says.");
        let decided = maintainer.browser.submit(&form, Some(("op", "approve")));
        assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
        accept_in(slug, maintainer, id)
    }

    pub(super) fn a_wiki_with_claims_round_trips(&mut self) {
        self.t.part("(G) a wiki with claims round-trips: a claim on a page with a history, a source, a curated status, a review day and an availability; exported, imported as a new wiki, and the claim comes back unsettled (RFC 007 criterion 3; Z-1)");
        let mut noor = self.signed_in("noor", "Noor Contributor");
        let mut maya = self.signed_in("maya", "Maya Maintainer");
        let today = utc_minutes(now()).get(..10).unwrap().to_owned();

        // The claim, its source, the status and the review: all through the forms, accepted by a maintainer.
        let made = attach(&mut noor, None, "supports", &SURVEY, "Cite the link syntax");
        signed_into_history(&approve_and_accept(&mut maya, &made));
        let help = self.anonymous.get(HELP);
        let claim = ids_after(&help.body, "/handbook/-/claim/")
            .into_iter()
            .next()
            .expect("the claim link names the claim");
        let claim_page = format!("/handbook/-/claim/{claim}");
        let seen = self.anonymous.get(&claim_page);
        let source = ids_after(&seen.body, "id=\"source-")
            .into_iter()
            .next()
            .expect("the source's line");
        let status = curate(
            &mut noor,
            &claim,
            None,
            |f| {
                f.set("status", "accepted");
            },
            "Say that this is accepted",
        );
        signed_into_history(&approve_and_accept(&mut maya, &status));
        let review = curate(
            &mut noor,
            &claim,
            None,
            |f| {
                let one = group_of(f, &source);
                f.set(&format!("c{one}_reviewed"), &today)
                    .set(&format!("c{one}_availability"), "available");
            },
            "Say when the source was looked at",
        );
        signed_into_history(&approve_and_accept(&mut maya, &review));
        self.t.did("through the forms: Noor attaches a source to a paragraph of `Help` (a claim on a page with a history), then proposes that the claim is accepted and that its source was reviewed today and is available; Maya reviews and accepts each");
        let now_said = plain(main_of(&self.anonymous.get(&claim_page)));
        self.t.saw(
            now_said.contains("This claim is accepted in this view."),
            "the claim page says it is accepted",
            &now_said,
        );

        // The export: the claim and the source, as the wiki had them.
        let first = self.export("handbook", "export-handbook-claims");
        self.t.did("the operator exports `handbook`");
        let claim_file = format!("knowledge/claims/{claim}.yaml");
        let source_file = format!("knowledge/sources/{source}.yaml");
        let claim_text = std::fs::read_to_string(first.join(&claim_file)).unwrap();
        let source_text = std::fs::read_to_string(first.join(&source_file)).unwrap();
        self.t.saw(
            claim_text.contains("status: accepted")
                && claim_text.contains("proposal:")
                && source_text.contains("last_reviewed:")
                && source_text.contains("availability: available"),
            "the bundle's claim file says `status: accepted` and the proposal that decided it, and its source says when it was reviewed and that it is available",
            &format!("{claim_text}\n{source_text}"),
        );
        let help_history: Vec<serde_json::Value> = histories(&first)
            .into_values()
            .find(|records| {
                records
                    .iter()
                    .any(|r| member(r, "/inklkunn/path") == "pages/Help.md")
            })
            .unwrap_or_default();
        self.t.saw(
            help_history.len() >= 2,
            "the page the claim is anchored to has a history of its own in the bundle (at least two records)",
            &format!("{help_history:?}"),
        );
        self.assert_checksums(&first, "the export with claims");

        // The dry run says what will not be imported, before anything is written.
        let before = self.listed();
        let (status, out, err) = self.import(COPY, "Handbook claims", &first, &["--dry-run"]);
        self.t.did("the operator asks for the plan: `wiki import-bundle handbook-claims <the export> --dry-run`");
        for sentence in [
            "1 community status was not imported: a status is set here only by a curation proposal.",
            "1 review day was not imported: a review day is set here only by a curation proposal.",
            "1 availability was not imported: an availability is set here only by a curation proposal.",
            "A claim or source file that had any of them is written without them; its bytes differ from the bundle's by exactly those keys.",
        ] {
            self.t.saw(
                status == 0 && out.contains(sentence),
                &format!("the dry run says: {sentence}"),
                &format!("{out}{err}"),
            );
        }
        self.t.saw(
            out.contains("dry run: nothing was written") && self.listed() == before,
            "and nothing was written: no new wiki is listed",
            &format!("{out}{err}"),
        );

        // The import, which used to be refused.
        let (status, out, err) = self.import(COPY, "Handbook claims", &first, &[]);
        self.t
            .did("the operator imports it: `wiki import-bundle handbook-claims <the export>`");
        self.t.saw(
            status == 0
                && out.contains("  knowledge:       2 claim or source file(s) in 2 change(s), after the history")
                && out.contains("1 community status was not imported: a status is set here only by a curation proposal."),
            "the import succeeds (it was refused: the claim's page was not there yet, and a status is a curation's); it says the knowledge came in two changes (the source, then the claim) after the history, and what was not imported",
            &format!("{out}{err}"),
        );
        let kept =
            std::fs::read_to_string(self.layout().import_report(&self.record(COPY).id)).unwrap();
        self.t.saw(
            kept.contains("1 availability was not imported: an availability is set here only by a curation proposal.")
                && kept.contains("its bytes differ from the bundle's by exactly those keys."),
            "the report kept beside the repository says the same",
            &kept,
        );

        // What the new wiki has.
        let blocks = self.blocks_of(COPY);
        self.t.saw(
            blocks.len() >= 5,
            "the new wiki's history is its creation, the page's revisions and, last, the knowledge: the source and then the claim",
            &format!("{blocks:?}"),
        );
        let second = self.export(COPY, "export-handbook-claims-2");
        let claim_again = std::fs::read_to_string(second.join(&claim_file)).unwrap();
        let source_again = std::fs::read_to_string(second.join(&source_file)).unwrap();
        self.t.saw(
            claim_again.contains("status: unknown")
                && !claim_again.contains("accepted")
                && !claim_again.contains("proposal:")
                && source_again.contains("availability: unknown")
                && !source_again.contains("last_reviewed"),
            "the claim comes back unsettled (`status: unknown`, no proposal named) and the source without a review day and with an availability of unknown",
            &format!("{claim_again}\n{source_again}"),
        );
        // Its anchor, its source, its kind and its other lines are the bundle's: the files differ by exactly the keys.
        let strip = |text: &str| -> String {
            let mut kept = Vec::new();
            let mut skipping = false;
            for line in text.lines() {
                let top = !line.starts_with(' ');
                if top {
                    skipping =
                        line.starts_with("community_status:") || line.starts_with("last_reviewed:");
                }
                if skipping || (top && line.starts_with("availability:")) {
                    continue;
                }
                kept.push(line);
            }
            kept.join("\n")
        };
        self.t.saw(
            strip(&claim_text) == strip(&claim_again) && strip(&source_text) == strip(&source_again),
            "the two files differ from the bundle's by exactly those keys: the anchor (page id and block), the source's link with its bearing and wording, and the source's title, author and reference are the same",
            &format!("{}\n{}", strip(&claim_again), strip(&source_again)),
        );
        let claim_in_copy = format!("/{COPY}/-/claim/{claim}");
        let page = self.anonymous.get(&claim_in_copy);
        let said = plain(main_of(&page));
        self.t.saw(
            page.status == 200
                && said.contains("This claim has not been decided in this view.")
                && said.contains("A survey of wiki link syntax")
                && said.contains("supports, directly")
                && said.contains("Where it is said"),
            "the new wiki's claim page reads the claim: not decided, with its source and where it is said",
            &said,
        );
        let listed = plain(main_of(
            &self
                .anonymous
                .get(&format!("/{COPY}/-/health/sources-to-review")),
        ));
        self.t.saw(
            listed.contains(&source) && listed.contains("No one has recorded looking at it."),
            "and its source is on the list of sources to review as one no one has looked at: the review day was not imported",
            &listed,
        );
        let changes = plain(main_of(&self.anonymous.get(&format!("/{COPY}/-/changes"))));
        self.t.saw(
            changes.contains("1 community status was not imported: a status is set here only by a curation proposal.")
                && changes.contains("1 review day and 1 availability were not imported: only a curation proposal sets them."),
            "the wiki's history (recent changes) says it, in the summaries of the two changes that wrote the files",
            &changes,
        );

        // INV-11: the status is a curation's, here as anywhere.
        let mut carla = self.join_in(COPY, "carla", "Carla Curator", Role::Maintainer);
        let mut dina = self.join_in(COPY, "dina", "Dina Contributor", Role::Contributor);
        self.t
            .did("Carla (maintainer) and Dina (contributor) join the new wiki");
        let again = self.curated_in(
            COPY,
            &mut dina,
            &claim,
            |f| {
                f.set("status", "contested");
            },
            "Say that this is contested",
        );
        let accepted = self.approved_and_accepted_in(COPY, &mut carla, &again);
        self.t.did("Dina proposes, through the curation form, that the imported claim is contested; Carla approves and accepts");
        let page = self.anonymous.get(&claim_in_copy);
        self.t.saw(
            accepted.status == 200
                && accepted.body.contains("Signed into the history")
                && plain(main_of(&page)).contains("This claim is contested in this view."),
            "a curation proposal sets the imported claim's status like any other claim's: it is contested now",
            &accepted.body,
        );
        let still = self.anonymous.get(&claim_page);
        self.t.saw(
            plain(main_of(&still)).contains("This claim is accepted in this view."),
            "and the first wiki's claim is as it was: accepted",
            &plain(main_of(&still)),
        );
        self.t.note("a retired claim or source still stops an import, by name (`lifecycle`): the importer never un-retires what a curation retired");
    }
}
