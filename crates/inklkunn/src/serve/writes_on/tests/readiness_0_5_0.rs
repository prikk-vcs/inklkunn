#![allow(clippy::expect_used, clippy::unwrap_used)]
//! R-5 (0.5.0 readiness; the plan's exit criterion 4): **the end-to-end proof of the interchange on a real prikk**, in the shape of
//! 0.4.0's (`readiness.rs`). The real server (the binary's `prepare`) over a real prikk, a real key store and a real SQLite store,
//! behind a real socket; the people come by invitation and sign in; the operator's `wiki import-bundle`, `wiki export --bundle` and
//! `wiki discard-unfinished` run in this process **against the same data directory the server is serving**, as an operator runs them
//! beside a running server (the command line and the server take turns on a wiki by lease, and a wiki that does not exist yet is
//! nobody's: the new wiki appears to the server when its record is stored, which is last).
//!
//! **One scripted run, and its transcript.** Every step is numbered, printed as it goes (`eprintln!`, so
//! `cargo test -p inklkunn --bins readiness_0_5_0 -- --nocapture` shows the story and a failure shows how far it got) and asserts at
//! once what the story says it saw. The story, in order:
//!
//! - (A) **a MediaWiki dump** (the converter's bundle, at level 3) comes in as a NEW wiki with its history: every revision is a block
//!   in D-2's order, each row of the history says *Imported. Originally by …* with the source's date as *originally*, the revision
//!   the source deleted is counted and not replayed, the notice names the wiki it was imported from as text and the day, the pages
//!   end as the bundle's files, participation counts leave the import out, and a source name equal to a local account's is not
//!   linked to it; its `wikipedia:` links dangle until the admin proposes an interwiki map through the policy form, which is
//!   accepted at the rung the policy says, and then the link is an external one on the page, on a permalink of an old revision, in
//!   `site.json` and in the export's manifest; and a bundle that carries its own map (the hand-written one) has it written by the import,
//!   a prefix that is a namespace of its pages left out and a prefix it lacks left dangling;
//! - (B) **a BookStack export** (level 2, no history) comes in as a second new wiki: pages and attachments in one change, each
//!   attachment's sidecar licence kept (the converter writes none, so the hand-written bundle of per-file licences is imported too),
//!   the licence notice of an attachment where the product shows one, and an attachment a reader can fetch;
//! - (C) **a round trip**: an inklkunn wiki made through the server (pages edited, one renamed, one removed, one edited by two accounts,
//!   a non-tip revision's text hidden by moderation) is exported, the files checked (history records, renames and removals as
//!   records, the hidden text in no file, the checksums), imported as a new wiki, and exported again;
//! - (D) **refusals**, each by name, each with nothing written;
//! - (E) **the new surfaces**: `listProposals` and `listProposalComments` walked by `Link rel="next"`, the proposals feed, none of them
//!   showing what a moderator hid, and the generic not-found for an anonymous reader of a restricted wiki;
//! - (F) **a kill**: an import killed after a step leaves what `wiki discard-unfinished` reports and removes exactly that.
//! - (G) **a wiki with claims round-trips**: a claim on a page with a history, its source, a status and a review day set by curation
//!   proposals; exported and imported as a new wiki, which used to be refused; the claim comes back unsettled, with its anchor and its
//!   source, the dry run, the run, the kept report and the wiki's history say what was not imported, and a curation proposal sets the
//!   status afterwards like any other (Z-1; INV-11).
//!
//! **What stands in for nothing:** every account is made by the invitation machinery, every rule by the policy form, every hide by the
//! moderation form, every import and export by the operator's command; the bundles are COPIES of the committed fixtures (converter
//! output at wiki-commons 19f021c), changed in a temporary directory where a refusal needs a changed one. The kill in (F) stops the
//! import with a panic that runs no cleanup (as `SIGKILL` runs none), the way the core's kill tests do.
//!
//! Needs prikk and prints `skipping: prikk not on PATH` without it (one line per test, counted by `EXPECTED_SKIPS`).

use super::api::{API, api, json_field, json_quote};
use super::knowledge::approve_and_accept;
use super::*;

mod bookstack;
mod claims;
mod kill;
mod kill_real;
mod mediawiki;
mod refusals;
mod roundtrip;
mod sha;
mod surfaces;

const FIXTURES: &str = "../inklkunn-core/tests/fixtures/interchange";

/// A fixture directory of the committed interchange fixtures.
fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(FIXTURES)
        .join(rel)
}

// ------------------------------------------------------------------------------------ the transcript

/// The step-numbered, human-readable log of the run. Every line is printed when it is written.
struct Transcript {
    lines: Vec<String>,
    steps: usize,
}

impl Transcript {
    fn new() -> Transcript {
        Transcript {
            lines: Vec::new(),
            steps: 0,
        }
    }

    fn say(&mut self, line: String) {
        eprintln!("{line}");
        self.lines.push(line);
    }

    /// A part of the story.
    fn part(&mut self, name: &str) {
        self.say(format!("\n== {name}"));
    }

    /// A step: what was done.
    fn did(&mut self, what: &str) {
        self.steps += 1;
        self.say(format!("step {:>3}: {what}", self.steps));
    }

    /// What was seen, asserted. `context` is shown when it is not so.
    fn saw(&mut self, ok: bool, what: &str, context: &str) {
        assert!(
            ok,
            "step {}: expected to see: {what}\n{context}",
            self.steps
        );
        self.say(format!("          saw: {what}"));
    }

    /// A fact that is not an assertion of its own: a number, an address.
    fn note(&mut self, what: &str) {
        self.say(format!("          note: {what}"));
    }
}

// ------------------------------------------------------------------------------------ helpers

/// The text of a whole page as the person reads it: tags become spaces and the app's own escaping is read back.
fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("&#38;", "&")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
}

/// The part of a page between `<main` and `</main>`: what the page says, not its chrome.
fn main_of(reply: &Reply) -> &str {
    let from = reply.body.find("<main").unwrap_or(0);
    let to = reply.body.rfind("</main>").unwrap_or(reply.body.len());
    reply.body.get(from..to).unwrap_or_default()
}

/// Every file under `root` (not following links) as (path under the root, bytes), in path order.
fn files_under(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                walk(root, &path, out);
            } else if kind.is_file() {
                out.push((
                    path.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(&path).unwrap(),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// The address of a proposal's page in the wiki `slug`.
fn proposal_path(slug: &str, id: &str) -> String {
    format!("/{slug}/-/proposals/{id}")
}

/// The accept form of a proposal of the wiki `slug`, sent by `person`: the reply (200, *Signed into the history*).
fn accept_in(slug: &str, person: &mut Person, id: &str) -> Reply {
    let page = person.browser.get(&proposal_path(slug, id));
    assert_eq!(page.status, 200, "{}", page.body);
    let form = form_to(&page, &format!("{}/accept", proposal_path(slug, id)));
    person.browser.submit(&form, None)
}

// ------------------------------------------------------------------------------------ the proof

struct Proof {
    world: World,
    running: Running,
    t: Transcript,
    anonymous: Browser,
}

impl Proof {
    /// The operator's command, in this process, against the data directory the server is serving.
    fn cli(&self, args: &[&str]) -> (i32, String, String) {
        self.world.cli(args)
    }

    /// The default view's blocks of the wiki `slug`, newest first.
    fn blocks_of(&self, slug: &str) -> Vec<String> {
        use inklkunn_prikk::{Point, Storage};
        let record = self.world.store.by_slug(&Slug::new(slug).unwrap()).unwrap();
        let layout = RootLayout::new(self.world.settings.wikis_root());
        self.world
            .storage()
            .history(
                &layout.repo_dir(&record.id),
                &Point::View(record.default_view.clone()),
                None,
            )
            .unwrap()
            .blocks
            .into_iter()
            .map(|b| b.block_id)
            .collect()
    }

    /// The operator's `account invite --role <role> --wiki <slug>`: the link's path.
    fn invite_to(&self, slug: &str, role: Role) -> String {
        let env = self.world.env();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = crate::account::run_with(
            &["invite", "--role", role.as_str(), "--wiki", slug],
            &|k| env.get(k).cloned(),
            now(),
            &mut out,
            &mut err,
            || Some(IdentityAuthority::for_tests()),
        );
        let (out, err) = (
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        );
        assert_eq!(status, 0, "{err}");
        out.lines()
            .find(|l| l.starts_with("/-/invite/"))
            .unwrap_or_else(|| panic!("no link in {out}"))
            .trim()
            .to_owned()
    }

    /// A person who comes by invitation to the wiki `slug` and signs in (`join`, for a wiki that is not `handbook`).
    fn join_in(&self, slug: &str, login: &'static str, name: &'static str, role: Role) -> Person {
        let link = self.invite_to(slug, role);
        let password = password_for(login);
        let mut opener = self.running.browser();
        let page = opener.get(&link);
        assert_eq!(page.status, 200, "{}", page.body);
        let mut form = form_to(&page, &link);
        form.set("login", login)
            .set("name", name)
            .set("password", &password);
        let made = opener.submit(&form, None);
        assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
        let mut browser = self.running.browser();
        sign_in(&mut browser, login, &password, "/");
        Person {
            login,
            name,
            password,
            browser,
        }
    }

    /// What the operator's `wiki import-bundle` says and does for `slug` from `bundle`, with `extra` options.
    fn import(
        &self,
        slug: &str,
        name: &str,
        bundle: &Path,
        extra: &[&str],
    ) -> (i32, String, String) {
        let mut args = vec![
            "wiki",
            "import-bundle",
            slug,
            bundle.to_str().unwrap(),
            "--name",
            name,
        ];
        args.extend_from_slice(extra);
        self.cli(&args)
    }

    fn layout(&self) -> RootLayout {
        RootLayout::new(self.world.settings.wikis_root())
    }

    fn record(&self, slug: &str) -> WikiRecord {
        self.world.store.by_slug(&Slug::new(slug).unwrap()).unwrap()
    }

    /// The operator's `wiki export <slug> --bundle <new directory in the scratch directory>`: where it was written.
    fn export(&self, slug: &str, name: &str) -> PathBuf {
        let dest = self.world.tmp.0.join(name);
        let (status, out, err) =
            self.cli(&["wiki", "export", slug, "--bundle", dest.to_str().unwrap()]);
        assert_eq!(status, 0, "{out}{err}");
        dest
    }

    /// The wikis the store lists, by slug.
    fn listed(&self) -> Vec<String> {
        let mut slugs: Vec<String> = self
            .world
            .store
            .list()
            .unwrap()
            .into_iter()
            .map(|w| w.slug.to_string())
            .collect();
        slugs.sort();
        slugs
    }
}

#[test]
fn the_interchange_works_end_to_end_on_a_real_prikk() {
    let Some(world) = World::new("readiness-0-5-0") else {
        return;
    };
    let running = Running::start(&world);
    let anonymous = running.browser();
    let mut p = Proof {
        world,
        running,
        t: Transcript::new(),
        anonymous,
    };
    p.t.part("The deployment: a public wiki `handbook` on prikk, and a server that is already serving it");
    let listed = p.listed();
    p.t.did("the operator made the wiki `handbook` and started the server; the store lists it, and nothing else");
    p.t.saw(
        listed == ["handbook"],
        "the store lists one wiki",
        &format!("{listed:?}"),
    );

    p.a_mediawiki_dump_comes_in_with_its_history();
    p.a_bundle_with_its_own_interwiki_map_comes_in_with_it();
    p.a_bookstack_export_comes_in();
    p.a_round_trip();
    p.refusals_are_named_and_write_nothing();
    p.the_new_surfaces();
    p.a_killed_import_is_discarded_by_a_person();
    p.a_wiki_with_claims_round_trips();

    let steps = p.t.steps;
    p.t.say(format!("\n== end of the transcript: {steps} steps"));
    p.running.stop();
}
