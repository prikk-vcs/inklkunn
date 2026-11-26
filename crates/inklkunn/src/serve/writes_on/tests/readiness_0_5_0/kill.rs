#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (F): **a killed import** (RFC 007 D-1; acceptance criterion 7), on the deployment the server is serving. The import is the product's
//! own `import_bundle`, over the deployment's real SQLite store, real key store, real signer and real prikk, stopped by a panic in its
//! step observer that runs no cleanup (as `SIGKILL` runs none; `creation_kill.rs` and `import_replay.rs` explain it, and show a stop after
//! every step on a throwaway store). What this adds is the whole operator's story beside a running server: what a reader of the server
//! sees, what the next command's sweep says and leaves alone, what `wiki discard-unfinished` refuses and removes, and the slug made again.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use super::refusals::LICENSED;
use super::*;
use inklkunn_core::import::bundle::{
    BundleOptions, ImportStaging, ImportStep, ImportWorld, import_bundle, plan_bundle,
};
use inklkunn_core::read::ReadConfig;
use inklkunn_core::{Signer, SignerConfig, WriteLeases};
use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_prikk::{Point, Storage};
use inklkunn_store::{AuditLog, AuditSink, DisplayName, NewWiki, WikiId};

impl Proof {
    /// Run the product's import of `bundle` as `slug` and stop it, by a panic, right after `stop`. `during` is called at every step
    /// before the stop, in the process that is importing.
    fn killed_import(
        &self,
        slug: &str,
        bundle: &Path,
        stop: ImportStep,
        during: &dyn Fn(ImportStep),
    ) {
        let settings = &self.world.settings;
        let layout = self.layout();
        let options = BundleOptions {
            bundle_license: Some("CC-BY-SA-4.0".to_owned()),
            ..BundleOptions::default()
        };
        let plan = plan_bundle(bundle, &ReadConfig::default(), &options).unwrap();
        assert!(plan.is_importable(), "{}", plan.render());
        let mut new = NewWiki::new(
            Slug::new(slug).unwrap(),
            DisplayName::new("Interrupted").unwrap(),
        );
        new.languages.clone_from(&plan.report.languages);
        new.default_license.clone_from(&plan.report.license);
        let staging = ImportStaging::new();
        let write_leases = WriteLeases::new(layout.clone());
        let signer = Arc::new(
            Signer::open_creating_store(
                SignerConfig::new(
                    settings.prikk.clone(),
                    Sandbox::Unconfined,
                    layout.clone(),
                    &settings.key_dir,
                    &settings.secret_file,
                    &settings.seed_dir,
                ),
                staging.ledger(),
            )
            .unwrap()
            .with_leases(&write_leases),
        );
        let storage: Arc<dyn Storage> = Arc::new(self.world.storage());
        let leases = staging.leases(write_leases, "readiness-kill");
        let reads = staging.reads(storage.clone(), layout.clone(), ReadConfig::default());
        let audit: Arc<dyn AuditSink> = self.world.store.clone();
        let world = ImportWorld {
            store: &*self.world.store,
            acts: &*self.world.store,
            layout: &layout,
            signer: &signer,
            storage: &storage,
            staging: &staging,
            leases,
            audit,
            reads: Some(reads),
            authority: crate::keys::key_authority(),
            version: env!("CARGO_PKG_VERSION"),
            clock: inklkunn_store::record::now_secs,
        };
        let hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            import_bundle(&world, new, &plan, &plan.render(), &|step| {
                during(step);
                if step == stop {
                    panic!("killed after {step:?}");
                }
            })
        }));
        std::panic::set_hook(hook);
        assert!(
            outcome.is_err(),
            "the import was not stopped after {stop:?}"
        );
    }

    pub(super) fn a_killed_import_is_discarded_by_a_person(&mut self) {
        self.t.part("(F) a killed import leaves what `wiki discard-unfinished` reports and removes exactly that, and nothing of a wiki that was ever stored (RFC 007 D-1; criterion 7)");
        let bundle = fixture("mediawiki/bundle");
        let listed = self.listed();
        let handbook = self.blocks_of("handbook");
        let garden = self.blocks_of("garden");

        // The kill, with a try to discard it while it is alive.
        let alive = std::cell::RefCell::new(None);
        self.killed_import("interrupted", &bundle, ImportStep::Revision(3), &|step| {
            if step == ImportStep::Revision(1) {
                let id = self.layout().creation_marks().unwrap().remove(0);
                let asked = self.cli(&[
                    "wiki",
                    "discard-unfinished",
                    id.as_str(),
                    "--confirm",
                    id.as_str(),
                ]);
                *alive.borrow_mut() = Some((
                    id.as_str().to_owned(),
                    asked,
                    self.layout().repo_dir(&id).is_dir(),
                ));
            }
        });
        self.t.did("the operator imports the MediaWiki bundle as `interrupted`, and the process is killed after the third revision was sealed (a panic that runs no cleanup); while it was running, a second operator asked `wiki discard-unfinished` for it");
        let (id, (status, out, err), repo_kept) = {
            let seen = alive.borrow();
            let (id, asked, kept) = seen
                .as_ref()
                .expect("the observer saw the first revision")
                .clone();
            (id, asked, kept)
        };
        self.t.saw(
            status == 1 && err.contains("that creation is running right now in another process; it is left alone, and nothing was removed") && repo_kept,
            "that was refused for a creation that is alive, and nothing was removed",
            &format!("{out}{err}"),
        );
        let wiki_id = WikiId::parse(&id).expect("an id");
        let repo = self.layout().repo_dir(&wiki_id);
        self.t.saw(
            self.listed() == listed,
            "the store does not list the wiki: a reader has never been able to reach it",
            &format!("{:?}", self.listed()),
        );
        let (made, absent) = (
            self.anonymous.get("/interrupted/Composting"),
            self.anonymous.get("/nowhere/Composting"),
        );
        self.t.saw(
            made.status == 404 && made.status == absent.status,
            "the running server answers for it as it does for a wiki that does not exist",
            &made.head,
        );
        let record = inklkunn_store::WikiRecord::from_new(
            wiki_id.clone(),
            NewWiki::new(
                Slug::new("interrupted").unwrap(),
                DisplayName::new("Interrupted").unwrap(),
            ),
            now(),
        )
        .unwrap();
        let sealed = self
            .world
            .storage()
            .history(&repo, &Point::View(record.default_view.clone()), None)
            .unwrap()
            .blocks
            .len();
        self.t.saw(
            self.layout().creation_marks().unwrap() == vec![wiki_id.clone()] && repo.is_dir() && sealed >= 4,
            &format!("what the kill left is a creation mark and a repository with {sealed} blocks sealed in it"),
            &format!("{:?}", self.layout().creation_marks()),
        );

        // The next command's sweep reports it and keeps it.
        let (status, _, err) = self.cli(&[
            "wiki",
            "create",
            "after-the-kill",
            "--name",
            "After the kill",
        ]);
        self.t.did("the operator's next command (`wiki create after-the-kill`) sweeps what a killed creation left");
        self.t.saw(
            status == 0
                && err.contains(&format!("inklkunn wiki discard-unfinished {id} --confirm {id}"))
                && repo.is_dir()
                && self.layout().creation_marks().unwrap() == vec![wiki_id.clone()],
            "the sweep names the command and the id, twice, and removes nothing: a history is only ever removed by a person",
            &err,
        );

        // The discard, step by step.
        let (status, out, err) = self.cli(&["wiki", "discard-unfinished", &id]);
        self.t.did("the person asks to discard it: without the id typed again; with another id; with the id of a wiki that was stored");
        self.t.saw(
            status == 1
                && out.contains(&format!("an unfinished creation of wiki {id}"))
                && err.contains("nothing was removed")
                && err.contains(&format!("--confirm {id}"))
                && repo.is_dir(),
            "without `--confirm` it says what it found and removes nothing",
            &format!("{out}{err}"),
        );
        let other = WikiId::new();
        let (status, _, err) = self.cli(&[
            "wiki",
            "discard-unfinished",
            &id,
            "--confirm",
            other.as_str(),
        ]);
        self.t.saw(
            status == 1 && err.contains("is not the id of the creation") && repo.is_dir(),
            "with another id typed it is refused, and nothing is removed",
            &err,
        );
        let kept = self.record("garden").id;
        let (status, out, err) = self.cli(&[
            "wiki",
            "discard-unfinished",
            kept.as_str(),
            "--confirm",
            kept.as_str(),
        ]);
        self.t.saw(
            status == 1
                && (err.contains("no unfinished creation of that id exists") || err.contains("the record of that wiki is stored"))
                && !out.contains("unfinished creation of wiki")
                && self.layout().repo_dir(&kept).is_dir()
                && self.blocks_of("garden") == garden,
            "for the id of `garden`, a wiki that was imported whole, it is refused and says no unfinished creation exists: the wiki's history is untouched",
            &format!("{out}{err}"),
        );
        let keys_before = std::fs::read_dir(&self.world.settings.key_dir)
            .unwrap()
            .count();
        let (status, out, err) = self.cli(&["wiki", "discard-unfinished", &id, "--confirm", &id]);
        self.t
            .did("the person types the id again: `wiki discard-unfinished <id> --confirm <id>`");
        self.t.saw(
            status == 0 && out.contains("removed: nothing of the unfinished creation is left"),
            "it is removed, and says that nothing of it is left",
            &format!("{out}{err}"),
        );
        let keys_after = std::fs::read_dir(&self.world.settings.key_dir)
            .unwrap()
            .count();
        self.t.saw(
            !repo.exists()
                && self.layout().creation_marks().unwrap().is_empty()
                && !self.layout().import_report(&wiki_id).exists()
                && keys_after + 1 == keys_before
                && std::fs::read_dir(&self.world.settings.seed_dir).unwrap().next().is_none(),
            "the repository, the creation mark, the kept report and the wiki's key record are gone, and no seed of the signer's",
            &format!("{keys_before} -> {keys_after} key files"),
        );
        self.t.saw(
            self.listed().contains(&"after-the-kill".to_owned())
                && self.blocks_of("handbook") == handbook
                && self.blocks_of("garden") == garden,
            "and nothing else was touched: the other wikis have the history they had",
            &format!("{:?}", self.listed()),
        );
        let trail: Vec<(String, String)> = self
            .world
            .store
            .read(0, 100_000)
            .unwrap()
            .into_iter()
            .filter(|r| r.event.action == "wiki-discard-unfinished")
            .map(|r| (r.event.outcome.as_str().to_owned(), r.event.detail.clone()))
            .collect();
        self.t.saw(
            trail
                == [
                    ("allowed".to_owned(), "removal begun".to_owned()),
                    (
                        "allowed".to_owned(),
                        "unfinished creation removed".to_owned(),
                    ),
                ],
            "the audit trail has the act twice: before it was done, and when it was",
            &format!("{trail:?}"),
        );

        // The slug is free: a wiki no one has read can be made again.
        let (status, out, err) = self.import("interrupted", "Interrupted", &bundle, LICENSED);
        self.t.did("the operator imports the same bundle as `interrupted` again: nothing is resumed, the slug is free");
        self.t.saw(
            status == 0
                && self.listed().contains(&"interrupted".to_owned())
                && self.layout().creation_marks().unwrap().is_empty(),
            "it comes in whole, and no creation mark is left",
            &format!("{out}{err}"),
        );
        self.t.saw(
            self.blocks_of("interrupted").len() + 1 == garden.len(),
            "with the nine blocks the same bundle made in `garden` (which has a tenth now: the acceptance of its interwiki map in (A))",
            &format!("{} {}", self.blocks_of("interrupted").len(), garden.len()),
        );

        // A kill after the record was stored: it is a wiki, and no person may discard it.
        self.killed_import("stored-late", &bundle, ImportStep::Stored, &|_| {});
        self.t.did("another import is killed at the very last step, after its record was stored: only the mark is left");
        self.t.saw(
            self.listed().contains(&"stored-late".to_owned()),
            "the wiki is listed: it is a wiki",
            &format!("{:?}", self.listed()),
        );
        let late = self.record("stored-late").id;
        let (status, out, err) = self.cli(&[
            "wiki",
            "discard-unfinished",
            late.as_str(),
            "--confirm",
            late.as_str(),
        ]);
        self.t.saw(
            status == 1
                && err.contains("the record of that wiki is stored, so it is a wiki and not an unfinished creation")
                && !out.contains("unfinished creation of wiki")
                && self.layout().repo_dir(&late).is_dir(),
            "`wiki discard-unfinished` refuses it by name, whatever id is typed: no command removes a wiki",
            &format!("{out}{err}"),
        );
        let page = self.anonymous.get("/stored-late/Composting");
        self.t
            .saw(page.status == 200, "and the server serves it", &page.head);
        let (status, _, err) = self.cli(&[
            "wiki",
            "create",
            "sweeps-the-mark",
            "--name",
            "Sweeps the mark",
        ]);
        self.t.saw(
            status == 0 && self.layout().creation_marks().unwrap().is_empty() && !err.contains("discard-unfinished"),
            "the next command's sweep finishes it: it removes the mark, and says nothing of discarding",
            &err,
        );
    }
}
