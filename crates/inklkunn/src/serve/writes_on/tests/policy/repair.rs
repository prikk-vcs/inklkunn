#![allow(clippy::expect_used, clippy::unwrap_used)]
//! **The broken interwiki map, repaired through the policy form, end to end** (0.5.0 readiness R-5, Y-1; RFC 006 INV-14, RFC 007
//! D-8). The real server over a real prikk, a real signer and a real store. A map the wiki cannot read makes it refuse every
//! write, and **the form's repair is the way out**: if it could not be sent, accepted and followed by writes that work, the
//! wiki would stay as it is.
//!
//! **The break is planted the way an import or a hand edit would make it**: a system act of the signer (`SystemActs`, the path
//! `wiki import-folder` takes), which nothing judges, in a second signer over the same keys while the server runs (as the
//! command line's own acts do). Nothing is written to the repository but by that signer and by the server's own acceptance.
//!
//! Each test plants one way the file can be broken, and shows, in order: (1) the wiki refuses a write, naming the map's file,
//! and still reads; (2) the policy page names the fault; (3) the form, opened by an admin, starts with no map rows and says why,
//! and a maintainer is told that only an admin can send the repair; (4) the repair is sent (typed rows, or the form with no row:
//! the file is removed) as one ordinary proposal and accepted by the admin, by the hatch; (5) writes work again, the policy page
//! shows the map there is (or none), and the next change to the map is an ordinary change.

use inklkunn_core::governance::{MAX_INTERWIKI_BYTES, validate_interwiki_bytes};
use inklkunn_core::message::SystemLabel;
use inklkunn_core::{
    Authorizer, LeaseConfig, PreparedFile, Signer, SignerConfig, StoreAuthz, StoreLedger,
    SystemActs, SystemRequest, WikiLeases, WriteLeases,
};
use inklkunn_prikk::sandbox::Sandbox;
use inklkunn_store::SystemActKind;

use super::*;

const MAP: &str = "governance/interwiki.yaml";
/// The sentence a refused write says, as the catalog gives it.
const REFUSED: &str = "Not changed: the rules of this wiki cannot be read";
const WIKI: &str = "https://en.wikipedia.org/wiki/{title}";

/// Write `files` into the wiki's default view **as a system act of a second signer** (one the server does not know of: the
/// command line's acts and an import are made so), and say the block. A file with `None` is removed. Nothing judges them.
fn plant(world: &World, files: &[(&str, Option<Vec<u8>>)]) -> String {
    let settings = &world.settings;
    let layout = RootLayout::new(settings.wikis_root());
    let write_leases = WriteLeases::new(layout.clone());
    let ledger = Arc::new(StoreLedger::new(world.store.clone(), world.store.clone()));
    let signer = Arc::new(
        Signer::open(
            SignerConfig::new(
                settings.prikk.clone(),
                Sandbox::Unconfined,
                layout.clone(),
                &settings.key_dir,
                &settings.secret_file,
                &settings.seed_dir,
            ),
            ledger,
        )
        .expect("the signer opens")
        .with_leases(&write_leases),
    );
    let leases = WikiLeases::new(
        write_leases,
        world.store.clone(),
        "y1-plant",
        LeaseConfig::default(),
    );
    let acts = SystemActs::new(
        Authorizer::new(
            Arc::new(StoreAuthz::new(world.store.clone(), world.store.clone())),
            world.store.clone(),
        ),
        world.store.clone(),
        world.store.clone(),
        Arc::new(world.storage()),
        layout,
        signer,
        leases,
    );
    let request = SystemRequest {
        kind: SystemActKind::System,
        summary: "Written outside the proposals".to_owned(),
        files: files
            .iter()
            .map(|(path, content)| PreparedFile {
                path: std::path::PathBuf::from(path),
                content: content.clone(),
            })
            .collect(),
        renames: Vec::new(),
        imported_from: None,
        source: None,
        notice: None,
        planned_against: None,
    };
    acts.run(&world.wiki, &SystemLabel::inklkunn(), request)
        .unwrap_or_else(|e| panic!("the plant was refused: {e}"))
        .block_id
}

/// A wiki with its people (a maintainer, an admin, a contributor), a proposal of a page made while it was well, and the map
/// broken by `plant`.
struct Broken {
    world: World,
    running: Running,
    maya: Person,
    ada: Person,
    noor: Person,
    /// A page proposal opened before the break: it is what the wiki cannot accept until the repair.
    open: String,
    /// The block of the break.
    broke: String,
}

fn broken(tag: &str, files: &[(&str, Option<Vec<u8>>)]) -> Option<Broken> {
    let world = World::new(tag)?;
    let running = Running::start(&world);
    let maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let ada = join(&world, &running, "ada", "Ada Admin", Role::Admin);
    let mut noor = join(
        &world,
        &running,
        "noor",
        "Noor Contributor",
        Role::Contributor,
    );
    let open = propose_edit(&mut noor, "Help", "Say which brackets", clarify_reading);
    let broke = plant(&world, files);
    Some(Broken {
        world,
        running,
        maya,
        ada,
        noor,
        open,
        broke,
    })
}

/// What `person` does when she presses *Accept* on proposal `id` where her page offers no button (the wiki refuses what she asks): the
/// POST of the form as it would be, with her own CSRF token.
fn press_accept(person: &mut Person, id: &str) -> Reply {
    let csrf = person.browser.csrf();
    let body = format!("csrf={}&revision=1", encode(&csrf)).into_bytes();
    person.browser.visit(
        "POST",
        &format!("{}/accept", proposal_page(id)),
        Some(body),
        &[],
    )
}

impl Broken {
    /// The break was written by another signer, so the running server learns of it when its view of the tip ends (a few seconds):
    /// wait until the policy page says it, as a person would see it.
    fn server_sees_the_break(&mut self) {
        let browser = &mut self.noor.browser;
        until_seeing("the policy page to name the map that cannot be read", || {
            let page = browser.get(POLICY);
            if page.body.contains("The rules cannot be read") {
                Ok(())
            } else {
                Err(page.body)
            }
        });
    }

    /// Every kind of write the wiki refuses while the map cannot be read, each said by the sentence and naming the map's file;
    /// and what it still does: read. Nothing is written by any of them.
    fn refuses_writes_and_reads(&mut self) {
        self.server_sees_the_break();
        let before = self.world.blocks();
        assert_eq!(before.first(), Some(&self.broke), "the break is the tip");

        // A maintainer accepting the proposal that waited: refused, whatever the rungs.
        let refused = press_accept(&mut self.maya, &self.open);
        assert!(refused.status >= 400, "{}\n{}", refused.head, refused.body);
        assert!(
            refused.body.contains(REFUSED) && refused.body.contains(MAP),
            "{}",
            refused.body
        );
        // The admin cannot use the repair for anything else: the page proposal is refused to her too.
        let refused = press_accept(&mut self.ada, &self.open);
        assert!(refused.status >= 400, "{}\n{}", refused.head, refused.body);
        assert!(refused.body.contains(REFUSED), "{}", refused.body);

        // A new proposal of a page is refused when it is sent (the form of a contributor is no way to the repair).
        let page = self.noor.browser.get("/handbook/-/proposals/new?page=Help");
        if page.status == 200 {
            let mut form = form_to(&page, "/handbook/-/proposals/new");
            let text = say_so(form.get("p0_text"));
            form.set("p0_text", &text).set("summary", "Say it again");
            let sent = self.noor.browser.submit(&form, Some(("op", "submit")));
            assert!(sent.status >= 400, "{}\n{}", sent.head, sent.body);
            assert!(sent.body.contains(REFUSED), "{}", sent.body);
        } else {
            assert!(
                page.body.contains(REFUSED) || page.body.contains(MAP),
                "{}\n{}",
                page.head,
                page.body
            );
        }
        assert_eq!(self.world.blocks(), before, "nothing was written");

        // Reads work: the page, the proposal, and the policy page, which says what is wrong with the map and no more.
        let help = self.noor.browser.get(HELP);
        assert_eq!(help.status, 200, "{}", help.body);
        let policy = self.noor.browser.get(POLICY);
        assert_eq!(policy.status, 200, "{}", policy.body);
        assert!(
            policy.body.contains("The rules cannot be read") && policy.body.contains(MAP),
            "{}",
            policy.body
        );
    }

    /// The form as the admin opens it: no map row, the repair's note and the fault's words above it. As the maintainer opens it:
    /// refused, with the reason.
    fn form_is_the_repair(&mut self) {
        let told = self.maya.browser.get(FORM);
        assert_eq!(told.status, 403, "{}", told.body);
        assert!(
            told.body.contains("Only an admin of the wiki can propose the repair"),
            "{}",
            told.body
        );
        let page = self.ada.browser.get(FORM);
        assert_eq!(page.status, 200, "{}", page.body);
        assert!(
            page.body.contains("The rules cannot be read, so the form starts from the default rules")
                && page.body.contains(MAP),
            "{}",
            page.body
        );
        assert_eq!(form_to(&page, FORM).get("map_rows"), "0");
    }

    /// The admin sends the map `rows` (none: the form as it is) and gets one proposal, which she accepts: the block it makes is
    /// signed by the hatch (an admin, the rules unreadable), and is the next block of the wiki.
    fn is_repaired_by(&mut self, rows: &[(&str, &str)]) {
        let before = self.world.blocks();
        let sent = send_map(&mut self.ada, rows);
        assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
        let repair = proposal_id(&sent.location());
        assert_eq!(self.world.blocks(), before, "proposing writes nothing");
        // A maintainer cannot accept it: the repair is an admin's.
        let by_maya = press_accept(&mut self.maya, &repair);
        assert!(
            by_maya.status >= 400 && by_maya.body.contains(REFUSED),
            "{}\n{}",
            by_maya.head,
            by_maya.body
        );
        assert_eq!(self.world.blocks(), before);
        accept(&self.world, &mut self.ada, &repair);
        let record = self.ada.browser.get(&proposal_page(&repair));
        assert!(
            record.body.contains("Accepted by Ada Admin (admin)"),
            "{}",
            record.body
        );
    }

    /// Writes work: the proposal that waited is accepted by the maintainer, and the page says what it wrote (`marker`); the policy
    /// page has no fault.
    fn writes_work(&mut self, marker: &str) {
        let before = self.world.blocks().len();
        accept(&self.world, &mut self.maya, &self.open);
        assert_eq!(self.world.blocks().len(), before + 1);
        assert!(page_text(&mut self.noor.browser, HELP).contains(marker));
        let policy = self.noor.browser.get(POLICY);
        assert_eq!(policy.status, 200, "{}", policy.body);
        assert!(
            !policy.body.contains("The rules cannot be read"),
            "{}",
            policy.body
        );
    }
}

/// A map file that is not sound: a row whose address is not `https`.
const UNSOUND: &[u8] = b"wikipedia: http://en.wikipedia.org/wiki/{title}\n";

#[test]
fn a_map_that_is_not_sound_refuses_every_write_and_the_form_repairs_it_with_rows_and_then_with_none() {
    let Some(mut w) = broken("y1-unsound", &[(MAP, Some(UNSOUND.to_vec()))]) else {
        return;
    };
    assert!(validate_interwiki_bytes(UNSOUND, &Default::default()).is_err());
    w.refuses_writes_and_reads();
    w.form_is_the_repair();
    // Typed rows: the file is written in its canonical text, and the wiki has that map.
    w.is_repaired_by(&[("wikipedia", WIKI)]);
    w.writes_work(READING_AFTER);
    let policy = w.noor.browser.get(POLICY);
    assert!(
        policy.body.contains("<dt>wikipedia</dt>") && policy.body.contains(WIKI),
        "{}",
        policy.body
    );
    let site = w.noor.browser.get("/handbook/-/site.json");
    assert!(
        site.body.contains("\"interwiki\":{\"wikipedia\":"),
        "{}",
        site.body
    );

    // Broken again, the same way: this time the form with no row (the file is taken away).
    let broke = plant(&w.world, &[(MAP, Some(UNSOUND.to_vec()))]);
    assert_eq!(w.world.blocks().first(), Some(&broke));
    w.broke = broke;
    w.server_sees_the_break();
    let page = w.ada.browser.get(FORM);
    assert_eq!(page.status, 200, "{}", page.body);
    assert_eq!(form_to(&page, FORM).get("map_rows"), "0");
    let before = w.world.blocks();
    w.is_repaired_by(&[]);
    assert_eq!(w.world.blocks().len(), before.len() + 1);
    let policy = w.noor.browser.get(POLICY);
    assert!(
        policy.body.contains("This wiki has no interwiki map.")
            && !policy.body.contains("The rules cannot be read"),
        "{}",
        policy.body
    );
    // The wiki is writable: a page proposal of its own is accepted by the maintainer.
    let again = propose_edit(&mut w.noor, "Help", "Say that it says so", say_so);
    accept(&w.world, &mut w.maya, &again);
    w.running.stop();
}

#[test]
fn a_map_that_is_not_text_or_is_too_long_is_repaired_by_the_form_like_one_that_is_not_sound() {
    let Some(mut w) = broken("y1-bytes", &[(MAP, Some(vec![0xff, 0xfe, b'a', b':', b' ']))]) else {
        return;
    };
    w.refuses_writes_and_reads();
    w.form_is_the_repair();
    // The file taken away by the form with no row.
    w.is_repaired_by(&[]);
    w.writes_work(READING_AFTER);
    assert!(
        w.noor
            .browser
            .get(POLICY)
            .body
            .contains("This wiki has no interwiki map.")
    );

    // A file one byte over the bound: a proposal is made while the wiki is well, the file arrives, and typed rows repair it.
    w.open = propose_edit(&mut w.noor, "Help", "Say that it says so", say_so);
    w.broke = plant(&w.world, &[(MAP, Some(vec![b'#'; MAX_INTERWIKI_BYTES + 1]))]);
    w.refuses_writes_and_reads();
    w.form_is_the_repair();
    w.is_repaired_by(&[("wikipedia", WIKI)]);
    w.writes_work(TRUST_AFTER);
    assert!(
        w.noor
            .browser
            .get(POLICY)
            .body
            .contains("<dt>wikipedia</dt>")
    );
    w.running.stop();
}
