#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Z-2 (0.5.0 readiness): **a real `SIGKILL` of a running `wiki import-bundle`** (RFC 007 D-1; acceptance criterion 7). `kill.rs` stops the
//! import with a panic in its step observer, which is the repository's precedent; the property the operator relies on is about a process
//! that dies, so this test starts the import as a **child process** and kills that one process with `SIGKILL`, between two blocks of its
//! history, while the server of the deployment is serving beside it.
//!
//! **The child.** The operator's command is `cli::run`, which the binary's `main` calls with the process's own arguments, environment and
//! streams. A unit test of a binary crate cannot start the binary (cargo gives the path of the built binary to integration tests only), so
//! the test starts **its own executable again**, selecting this one test by name, and the test, finding [`CHILD`] in its environment, does
//! what `main` does (`cli::run`, then exit with its status) and nothing else. It is a real process with its own pid, file descriptors,
//! locks and children; what `SIGKILL` stops is the product's import, under the product's `cli::run`; no hook exists in the product for it.
//!
//! **What it shows**, on the data directory the server serves: the wiki the kill left is not listed (the store, the server's index of wikis)
//! and is answered as a wiki that does not exist (a page, the API, the site description); a command that sweeps reports it and keeps it, with
//! every block sealed in it; `wiki discard-unfinished` is refused for the creation while it is alive and removes, once it is dead, exactly what
//! the killed import made (the directories of the data, key and seed directories are what they were before it began, but for what another
//! command made on the way); a completed wiki beside it is untouched; and the same slug imports again, whole.
//!
//! **No vacuous pass.** The import must be caught while it runs: the test watches the repository until the first change and a revision are
//! sealed, and kills only a child that was still running. A child that finished first (a machine faster than the history is long) is not a
//! pass: the next attempt has a longer history, up to the cap's default, and the test fails with that said when none was caught.
//!
//! Needs prikk and prints `skipping: prikk not on PATH` without it (one line, counted by `EXPECTED_SKIPS`).

use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, ExitStatus, Stdio};

use super::sha::sha256_hex;
use super::*;
use inklkunn_prikk::{Point, Storage};
use inklkunn_store::{VerificationState, WikiId, WikiStore};

/// The environment variable that makes the test a child: the operator's arguments, one unit separator (U+001F) between them.
const CHILD: &str = "INKLKUNN_KILL_REAL_CHILD_ARGV";

/// The name of the test, as the test runner selects it.
const TEST: &str = "a_real_sigkill_of_import_bundle_leaves_what_a_person_discards";

/// The revisions of the long history, for each attempt (the page has these on top of the fixture's: the cap's default is 1,000).
const ATTEMPTS: [usize; 3] = [300, 600, 900];

/// The kill: the child's side. It is what `main` does, and never returns.
fn child(argv: &std::ffi::OsStr) -> ! {
    let args: Vec<String> = argv
        .to_string_lossy()
        .split('\u{1f}')
        .map(str::to_owned)
        .collect();
    let status = crate::cli::run(
        &args,
        &|name| std::env::var_os(name),
        now(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    std::process::exit(status)
}

/// `sha256sums.txt` over every file of the bundle but the manifest and itself, with an independent SHA-256.
fn seal(root: &Path) {
    let mut lines = Vec::new();
    for (name, bytes) in files_under(root) {
        if name == "wiki-bundle.yaml" || name == "sha256sums.txt" {
            continue;
        }
        lines.push(format!("{}  {name}", sha256_hex(&bytes)));
    }
    std::fs::write(root.join("sha256sums.txt"), lines.join("\n") + "\n").unwrap();
}

/// A copy of the MediaWiki fixture in `dest` whose page `Composting` has `revisions` revisions, each by the same person, and sealed again.
fn long_bundle(dest: &Path, revisions: usize) {
    copy_tree(&fixture("mediawiki/bundle"), dest);
    let mut lines = Vec::new();
    for i in 1..=revisions {
        let content = format!("---\ntitle: Composting\n---\n\nRevision {i} of the compost notes.");
        let record = serde_json::json!({
            "rev": format!("r{}", 5000 + i),
            "parent": if i == 1 { serde_json::Value::Null } else { format!("r{}", 5000 + i - 1).into() },
            "type": if i == 1 { "create" } else { "edit" },
            "at": format!("2025-06-01T{:02}:{:02}:00Z", i / 60, i % 60),
            "author": {"id": "mira", "name": "Mira"},
            "summary": format!("edit {i}"),
            "sha256": sha256_hex(content.as_bytes()),
            "content": content,
        });
        lines.push(record.to_string());
    }
    std::fs::write(dest.join("history/mw:101.jsonl"), lines.join("\n") + "\n").unwrap();
    seal(dest);
}

/// A running child that is killed if the test ends without having done it: the one process this test started, by its handle.
struct Spawned(Option<Child>);

impl Drop for Spawned {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Every path under `root` (directories and files, not following links), relative to it, sorted; the store's own files are left out (the
/// store changes as it is used; what it holds is asked of the store).
fn names_under(root: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries {
            let entry = entry.unwrap();
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().display().to_string();
            if dir == root && rel.starts_with("store.sqlite") {
                continue;
            }
            out.push(rel);
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// What the attempt saw when it looked for the child at the moment the kill was due.
enum Watched {
    /// The child is running with the creation sealed this many blocks into the history: `(wiki id, blocks sealed)`.
    Running(WikiId, usize),
    /// The child ended before it could be caught.
    Ended(ExitStatus),
}

impl Proof {
    /// The blocks of the default view of the repository `repo` (newest first), at most `limit` of them; none while it is not a repository yet.
    fn sealed_in(&self, repo: &Path, limit: u64) -> usize {
        let view = self.record("handbook").default_view;
        self.world
            .storage()
            .history(repo, &Point::View(view), Some(limit))
            .map(|h| h.blocks.len())
            .unwrap_or(0)
    }

    /// The operator's `wiki import-bundle` as a child process of its own, with its streams in files of the scratch directory: `slug` from `bundle`.
    fn spawn_import(&self, slug: &str, bundle: &Path, tag: &str) -> Spawned {
        let argv = [
            "--dev-unconfined",
            "wiki",
            "import-bundle",
            slug,
            bundle.to_str().unwrap(),
            "--name",
            "Interrupted",
            "--bundle-license",
            "CC-BY-SA-4.0",
        ]
        .join("\u{1f}");
        let log = |name: &str| {
            std::fs::File::create(self.world.tmp.0.join(format!("child-{tag}.{name}"))).unwrap()
        };
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("serve::writes_on::tests::readiness_0_5_0::kill_real::{TEST}"),
            ])
            .args(["--nocapture", "--test-threads=1"])
            .env(CHILD, argv)
            .envs(self.world.env())
            .stdin(Stdio::null())
            .stdout(log("out"))
            .stderr(log("err"))
            .spawn()
            .unwrap();
        Spawned(Some(child))
    }

    /// What the child said, for a failure.
    fn child_said(&self, tag: &str) -> String {
        ["out", "err"]
            .iter()
            .map(|n| {
                std::fs::read_to_string(self.world.tmp.0.join(format!("child-{tag}.{n}")))
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join("\n--- stderr ---\n")
    }

    /// Watch `child` until its creation has sealed the first change and a revision on top of the creation's own blocks (`base` of them), or it
    /// ended. A bounded wait: the test is stopped by the harness's timeout, and this fails with what the child said before that.
    fn watch(&self, child: &mut Spawned, base: usize, tag: &str) -> Watched {
        let end = Instant::now() + Duration::from_secs(120);
        loop {
            if let Some(status) = child.0.as_mut().unwrap().try_wait().unwrap() {
                return Watched::Ended(status);
            }
            assert!(
                Instant::now() < end,
                "the child did not seal a revision in 120 seconds; it said:\n{}",
                self.child_said(tag)
            );
            if let Some(id) = self.layout().creation_marks().unwrap().first() {
                let sealed = self.sealed_in(&self.layout().repo_dir(id), base as u64 + 2);
                if sealed >= base + 2 {
                    return Watched::Running(id.clone(), sealed);
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// The page, the API and the site description of `slug`, each as the reader of a wiki that does not exist sees them (the slug is the only
    /// difference of the answers).
    fn answers_as_absent(&self, slug: &str) -> bool {
        ["/Composting", "/-/api/v1/proposals", "/-/site.json"]
            .iter()
            .all(|tail| {
                let (a, b) = (
                    self.anonymous_get(&format!("/{slug}{tail}")),
                    self.anonymous_get(&format!("/nowhere{tail}")),
                );
                a.status == 404
                    && a.status == b.status
                    && a.header("content-type") == b.header("content-type")
                    && a.body.replace(slug, "nowhere") == b.body
            })
    }

    /// A GET by a reader who is not signed in (a browser of its own each time: nothing is kept).
    fn anonymous_get(&self, path: &str) -> Reply {
        self.running.browser().get(path)
    }

    fn a_real_kill(&mut self) {
        self.t.part("Z-2: a real SIGKILL of a running `wiki import-bundle`, between two blocks of its history (RFC 007 D-1; criterion 7)");
        let handbook = self.blocks_of("handbook");
        let base = handbook.len();
        // A wiki that is complete beside the one that will be killed.
        let garden_bundle = fixture("mediawiki/bundle");
        let (status, out, err) = self.import(
            "garden",
            "Garden",
            &garden_bundle,
            super::refusals::LICENSED,
        );
        self.t.did("the operator imports the MediaWiki bundle as `garden`, whole, before anything is killed");
        self.t
            .saw(status == 0, "it is a wiki", &format!("{out}{err}"));
        let garden = self.blocks_of("garden");
        let reader = self.anonymous_get("/garden/Composting");
        self.t.saw(
            reader.status == 200,
            "and the server serves it",
            &reader.head,
        );

        // The kill: each attempt is a longer history than the one before, until one is caught while it runs.
        let mut caught = None;
        for revisions in ATTEMPTS {
            let slug = format!("interrupted-{revisions}");
            let bundle = self.world.tmp.0.join(format!("long-{revisions}"));
            long_bundle(&bundle, revisions);
            // An attempt that was not caught can leave the mark of a wiki that finished (a probe by another process that holds the mark as the
            // import removes it turns that removal away; the next command's sweep removes it, and says so). That command is run now, so that
            // what is compared below is what the killed import made.
            if !self.layout().creation_marks().unwrap().is_empty() {
                let (status, _, err) = self.cli(&[
                    "wiki",
                    "create",
                    &format!("tidy-{revisions}"),
                    "--name",
                    "Tidy",
                ]);
                assert!(
                    status == 0
                        && err
                            .contains("had finished being created; its creation mark was removed"),
                    "{err}"
                );
                assert!(self.layout().creation_marks().unwrap().is_empty());
            }
            let listed = self.listed();
            let data_before = names_under(&self.world.settings.data_dir);
            let seed_before = names_under(&self.world.settings.seed_dir);
            let tag = format!("{revisions}");
            let mut spawned = self.spawn_import(&slug, &bundle, &tag);
            self.t.did(&format!("the operator starts `wiki import-bundle {slug}` of a history of {revisions} revisions as a process of its own (pid {})", spawned.0.as_ref().unwrap().id()));
            match self.watch(&mut spawned, base, &tag) {
                Watched::Ended(status) => {
                    self.t.note(&format!("the import ended ({status}) before it could be caught; the next attempt has a longer history\n{}", self.child_said(&tag)));
                    assert!(
                        status.success(),
                        "the import that ended was not a success:\n{}",
                        self.child_said(&tag)
                    );
                    continue;
                }
                Watched::Running(id, sealed) => {
                    // A person asks for a discard of it while it is alive. (If it was not alive any more when the answer came, the answer says
                    // nothing of a creation that is alive: that attempt is not a catch.)
                    let (status, out, err) = self.cli(&[
                        "wiki",
                        "discard-unfinished",
                        id.as_str(),
                        "--confirm",
                        id.as_str(),
                    ]);
                    if spawned.0.as_mut().unwrap().try_wait().unwrap().is_some() {
                        self.t.note("the child ended while it was asked about; the next attempt has a longer history");
                        continue;
                    }
                    let repo = self.layout().repo_dir(&id);
                    self.t.did(&format!("the child has sealed {sealed} blocks and is running; a second operator asks `wiki discard-unfinished {id}` for it"));
                    self.t.saw(
                        status == 1
                            && err.contains("that creation is running right now in another process; it is left alone, and nothing was removed")
                            && repo.is_dir()
                            && self.layout().creation_marks().unwrap() == vec![id.clone()],
                        "that is refused for a creation that is alive, and nothing was removed",
                        &format!("{out}{err}"),
                    );
                    // The kill, of that one process, by its handle.
                    let mut child = spawned.0.take().unwrap();
                    child.kill().unwrap();
                    let status = child.wait().unwrap();
                    if status.signal() != Some(9) {
                        // It finished between the look and the signal: not a kill. Whatever it made is a wiki, and the next attempt starts again.
                        assert!(
                            status.success(),
                            "the import ended, but not as a success ({status}):\n{}",
                            self.child_said(&tag)
                        );
                        self.t.note(&format!("the child ended on its own ({status}) as the signal was sent; the next attempt has a longer history"));
                        continue;
                    }
                    self.t.did("the child is sent SIGKILL (std's `Child::kill`, to that one pid) and waited for");
                    self.t.saw(
                        status.signal() == Some(9) && !self.listed().contains(&slug),
                        "it died by signal 9, and its wiki was never stored",
                        &format!("{status}"),
                    );
                    caught = Some((
                        slug,
                        id,
                        revisions,
                        bundle,
                        listed,
                        data_before,
                        seed_before,
                        sealed,
                    ));
                    break;
                }
            }
        }
        let Some((slug, id, revisions, bundle, listed, data_before, seed_before, sealed)) = caught
        else {
            panic!(
                "no attempt caught an import while it ran (histories of {ATTEMPTS:?} revisions); the machine is too fast for the cap's default"
            );
        };
        let repo = self.layout().repo_dir(&id);
        // The child's own processes (prikk) may be a moment behind it; the creation is dead when its mark can be taken.
        let mut asked = (0, String::new(), String::new());
        until("the killed creation to be dead", || {
            asked = self.cli(&["wiki", "discard-unfinished", id.as_str()]);
            asked.1.contains("an unfinished creation of wiki")
        });
        let at_kill = self.sealed_in(&repo, 100_000);
        self.t.note(&format!("{at_kill} blocks are sealed in the killed repository (the child had sealed {sealed} when it was found; {revisions} revisions were to come)"));

        // (4) A reader.
        let after_kill = self.listed();
        self.t.saw(
            after_kill == listed && self.layout().creation_marks().unwrap() == vec![id.clone()],
            "the store does not list the wiki, and its creation mark and repository are what the kill left",
            &format!("{after_kill:?}"),
        );
        let index = self.anonymous_get("/-/wikis");
        self.t.saw(
            index.status == 200 && index.body.contains("handbook") && !index.body.contains(&slug),
            "the server's index of wikis names the wikis that exist and not that one",
            &index.body,
        );
        self.t.saw(
            self.answers_as_absent(&slug),
            "the page, the API and the site description of it answer as for a wiki that does not exist: the generic not-found, the same bytes",
            &slug,
        );

        // (5) The sweep reports it and keeps it.
        let (status, _, err) = self.cli(&["wiki", "create", "swept", "--name", "Swept"]);
        self.t.did(
            "the operator's next command (`wiki create swept`) sweeps what a killed creation left",
        );
        self.t.saw(
            status == 0
                && err.contains(&format!("the unfinished creation of wiki {id} could not be removed whole"))
                && err.contains("its repository holds a sealed block, so it may be a wiki whose record was lost; nothing was removed")
                && err.contains(&format!("inklkunn wiki discard-unfinished {id} --confirm {id}")),
            "the sweep names the creation, says why it kept it (a sealed block) and gives the person's command",
            &err,
        );
        self.t.saw(
            repo.is_dir()
                && self.layout().creation_marks().unwrap() == vec![id.clone()]
                && self.sealed_in(&repo, 100_000) >= at_kill
                && self.world.settings.key_dir.join(format!("{id}.key.json")).is_file(),
            "and kept all of it: the repository with every block sealed in it, the mark and the wiki's key record",
            &err,
        );

        // (6) The discard removes exactly what the import made.
        let (status, out, err) = self.cli(&[
            "wiki",
            "discard-unfinished",
            id.as_str(),
            "--confirm",
            id.as_str(),
        ]);
        self.t
            .did("the person types the id again: `wiki discard-unfinished <id> --confirm <id>`");
        self.t.saw(
            status == 0 && out.contains("removed: nothing of the unfinished creation is left"),
            "it is removed, and says that nothing of it is left",
            &format!("{out}{err}"),
        );
        let swept = self.record("swept").id;
        let (data_after, seed_after) = (
            names_under(&self.world.settings.data_dir),
            names_under(&self.world.settings.seed_dir),
        );
        let made_by_the_sweeping_command: Vec<&String> = data_after
            .iter()
            .filter(|p| !data_before.contains(p))
            .collect();
        let gone: Vec<&String> = data_before
            .iter()
            .filter(|p| !data_after.contains(p))
            .collect();
        // The signer makes a seed directory under a name the sweep ignores (`.new-<32 hex digits>`), locks it, and renames it. A `SIGKILL` that
        // lands between the two leaves that one EMPTY directory (no seed in it), which the next signer's sweep removes once it is old
        // (`key_store/live.rs`, `seeds.rs`); `wiki discard-unfinished` does not look in the seed directory. It is allowed here, said, and nothing
        // else is: no seed directory of a call, no file.
        let birth_leftovers: Vec<&String> = seed_after
            .iter()
            .filter(|p| !seed_before.contains(p))
            .collect();
        let only_birth_leftovers = birth_leftovers.iter().all(|p| {
            p.strip_prefix(".new-")
                .is_some_and(|h| h.len() == 32 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        });
        if !birth_leftovers.is_empty() {
            self.t.note(&format!("the kill landed while the signer was making a seed directory: {birth_leftovers:?} is left, empty, for a later start's sweep (by design: it is removed once it is old)"));
        }
        self.t.saw(
            gone.is_empty()
                && !made_by_the_sweeping_command.is_empty()
                && made_by_the_sweeping_command.iter().all(|p| p.contains(swept.as_str()))
                && only_birth_leftovers
                && seed_before.iter().all(|p| seed_after.contains(p))
                && !data_after.iter().any(|p| p.contains(id.as_str())),
            "the data, key and seed directories hold what they held before the import, and only what `wiki create swept` made since (and, if the kill landed in that instant, one empty half-made seed directory): nothing of the killed wiki is left, and nothing else is gone",
            &format!(
                "new, and not of `swept`: {:?}\ngone: {gone:?}\nleft of the killed wiki: {:?}\nseed before: {seed_before:?}\nseed after: {seed_after:?}",
                made_by_the_sweeping_command
                    .iter()
                    .filter(|p| !p.contains(swept.as_str()))
                    .collect::<Vec<_>>(),
                data_after
                    .iter()
                    .filter(|p| p.contains(id.as_str()))
                    .collect::<Vec<_>>()
            ),
        );
        self.t.saw(
            self.listed().contains(&"swept".to_owned())
                && self.blocks_of("handbook") == handbook
                && self.blocks_of("garden") == garden
                && self.anonymous_get("/garden/Composting").status == 200,
            "and the wikis beside it are what they were: listed, with the history they had, and served",
            &format!("{:?}", self.listed()),
        );
        self.t.saw(
            self.answers_as_absent(&slug),
            "the slug still answers as a wiki that does not exist",
            &slug,
        );

        // (7) The same slug, made again, and let to finish.
        let (status, out, err) =
            self.import(&slug, "Interrupted", &bundle, super::refusals::LICENSED);
        self.t.did(&format!("the operator imports the same bundle as `{slug}` again, and lets it finish: nothing is resumed, the slug is free"));
        self.t.saw(
            status == 0
                && self.listed().contains(&slug)
                && self.layout().creation_marks().unwrap().is_empty(),
            "it comes in whole, and no creation mark is left",
            &format!("{out}{err}"),
        );
        let record = self.record(&slug);
        let history = self.sealed_in(&self.layout().repo_dir(&record.id), 100_000);
        self.t.saw(
            history >= revisions + 3,
            &format!("with its history: {history} blocks, one for each of the {revisions} revisions and more"),
            &format!("{history}"),
        );
        self.t.saw(
            <dyn WikiStore>::get(&*self.world.store, &record.id)
                .unwrap()
                .verification
                .state
                == VerificationState::Verified,
            "and the repository verified",
            &format!(
                "{:?}",
                <dyn WikiStore>::get(&*self.world.store, &record.id)
                    .unwrap()
                    .verification
            ),
        );
        let page = self.anonymous_get(&format!("/{slug}/Composting"));
        self.t.saw(
            page.status == 200 && !self.answers_as_absent(&slug),
            "and the server serves it",
            &page.head,
        );
    }
}

#[test]
fn a_real_sigkill_of_import_bundle_leaves_what_a_person_discards() {
    if let Some(argv) = std::env::var_os(CHILD) {
        child(&argv);
    }
    let Some(world) = World::new("readiness-kill-real") else {
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
    p.a_real_kill();
    p.running.stop();
}
