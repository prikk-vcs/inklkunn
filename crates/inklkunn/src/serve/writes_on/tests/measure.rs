#![allow(clippy::expect_used, clippy::unwrap_used)]
//! R-5 item 6 (0.4.0 readiness): **the writes index measured on a long history.** Not a check: a measurement, so the test is
//! `#[ignore]`d (no `skipping:` line in a normal run, no `EXPECTED_SKIPS`), and it prints its numbers.
//!
//! ```text
//! PATH=$HOME/.pgtmp/rel-0.58.0/x:$PATH TMPDIR=<a directory on a disk, not a tmpfs> \
//!   INKLKUNN_MEASURE_SIZES=500,2000,5000 INKLKUNN_MEASURE_RUNS=3 \
//!   INKLKUNN_MEASURE_FIXTURES=<a directory that keeps the built histories> \
//!   cargo test -p inklkunn --locked -- --ignored --nocapture the_writes_index_on_a_long_history
//! ```
//!
//! The knobs (all optional): `INKLKUNN_MEASURE_SIZES` (blocks of the default view's log, comma separated; default `500`),
//! `INKLKUNN_MEASURE_RUNS` (default `1`), `INKLKUNN_MEASURE_FIXTURES` (where a built history is kept, default
//! `$TMPDIR/inklkunn-measure-fixtures`; a history is built once per size, prikk version and size, and reused),
//! `INKLKUNN_MEASURE_SCENARIOS` (a subset of `natural,cold,warm,views`; default all but `views`, which is slow: it opens
//! twenty views), `INKLKUNN_MEASURE_BUILD_ONLY` (build the histories and stop).
//!
//! # The history
//!
//! One public wiki through the real server, the real signer and the real prikk, **every block an accepted proposal of a person,
//! reviewed and accepted through the pages** (the forms the end-to-end tests use): the `K` blocks of the default view are the
//! wiki's first change, four new pages (*Alpha* to *Delta*) and `K - 5` edits, each a one-line change of a counter paragraph,
//! **half of them to *Help*** and the rest round-robin to the four other pages. The edit at the middle of the history (to *Help*)
//! is the one that is hidden; the third edit (block 8) is a second hide's target (the `warm` scenario) and the block the
//! *warmed* request asks for. Nothing is hidden when the history is built, so the writes index is not built by it.
//!
//! A history is built once per size and prikk version and kept (a copy of the data directory, a few MiB per thousand blocks;
//! building costs about 0.3 s a block at 500 blocks and grows with the length of the log, about 0.7 s a block at 2,000). Each
//! run works on a fresh copy. **The quotas** (thirty proposals an hour for a person, half of that for an account a week old; eight
//! MiB of accepted changes a day) would make the build wait for hours of a clock that cannot be moved, so the build does what
//! the end-to-end tests do for a protection (`pass_time`): from time to time every time that the quotas count is moved two
//! days back in the store file. No limit is changed and no check is skipped.
//!
//! # What is counted
//!
//! The server's prikk is a **counting wrapper** (a shell script that appends the verb to a file and `exec`s the real prikk):
//! every process the server spawns is counted, by verb, whether the read service or the signer made it; the number of `show`s is
//! the count of that verb. The wrapper's own cost (a `sh` start, 0.4 to 0.5 ms) is measured and printed. Times are wall times of
//! the request over the socket, as seen by the client, through the real server (debug build of the test profile). The requests
//! of a reader are anonymous (no account). The load average is read from `/proc/loadavg` before and after each run.
//!
//! # The scenarios (each on a fresh copy of the built history; the derived store empty at the start)
//!
//! - **natural**: a moderator opens the moderation form for the middle edit and hides its text. The form's page lists the later
//!   changes of the text, which builds the index toward the tip within its bound (`ReadService::later_writers`), and the hide's
//!   post warms it again (`ReadService::warm_writes`): both are counted as the hide's own cost (the form's latency). Then
//!   readers ask for *Help* at the hidden block (a permalink at an old block) until the answer is not *not checked yet*, then
//!   for *Help* at the newest block, then for *Help* at the early edit's block (the warmed request: the index holds it, the page is
//!   not held) and again (the page is held).
//! - **cold**: the same hide, then the derived store cleared (`clear_derived`) and the server started again: the first request
//!   meets an empty index and a process that has read nothing.
//! - **warm**: the index is built to the tip first (a decoy hide, of the early edit's text, then `warm_writes` until it reaches
//!   the tip); then the middle edit is hidden and the same requests are made.
//! - **views**: twenty views open (`v01` to `v20`), each holding one edit of its own, and an open proposal in the last one; the
//!   middle edit is hidden, the derived store cleared and the server started again, and the page of that proposal is asked, which
//!   reads the text the proposal was written against: a block that the default view's log does not hold, found among the
//!   other views (correction L-1, `MAX_VIEWS_ASKED`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use super::*;

const SIZES: &str = "INKLKUNN_MEASURE_SIZES";
const RUNS: &str = "INKLKUNN_MEASURE_RUNS";
const FIXTURES: &str = "INKLKUNN_MEASURE_FIXTURES";
const SCENARIOS: &str = "INKLKUNN_MEASURE_SCENARIOS";
const BUILD_ONLY: &str = "INKLKUNN_MEASURE_BUILD_ONLY";

/// The most requests one settling may take before the measurement says it did not settle.
const SETTLE_CAP: usize = 1_000;
/// The wiki's pages the edits go to besides *Help* (round robin); the ones that exist in a new wiki.
const OTHER_PAGES: [&str; 4] = ["Alpha", "Beta", "Gamma", "Delta"];
const HELP_TITLE: &str = "Help";
const COUNTER: &str = "Measurement edit counter ";
/// The words the page says in place of a hidden text.
const HIDDEN_WORDS: &str = "Hidden by moderation";
const NOT_YET_WORDS: &str = "cannot be checked yet";

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_owned())
}

fn loadavg() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(str::to_owned))
        .unwrap_or_else(|| "?".to_owned())
}

// ------------------------------------------------------------------------------------ counting prikk

/// A prikk that counts: `<log>` gets one line, the verb, for each process.
struct Counting {
    log: PathBuf,
    script: PathBuf,
}

impl Counting {
    fn install(world: &mut World, real: &std::ffi::OsStr) -> Counting {
        let log = world.tmp.0.join("prikk-calls.log");
        let script = world.tmp.0.join("prikk-counting");
        std::fs::write(&log, "").unwrap();
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho \"$1\" >> '{}'\nexec '{}' \"$@\"\n",
                log.display(),
                real.to_string_lossy()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        world.settings.prikk = script.clone().into_os_string();
        Counting { log, script }
    }

    /// The calls since the last `take`, by verb; the log is emptied.
    fn take(&self) -> Calls {
        let text = std::fs::read_to_string(&self.log).unwrap_or_default();
        std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.log)
            .unwrap();
        let mut by_verb = BTreeMap::new();
        for verb in text.lines().filter(|l| !l.is_empty()) {
            *by_verb.entry(verb.to_owned()).or_insert(0) += 1;
        }
        Calls(by_verb)
    }

    /// What one start of the wrapper costs: `prikk --version` through it, minus the real one, in microseconds (a median of 50).
    fn overhead_us(&self, real: &std::ffi::OsStr) -> u128 {
        let median = |program: &std::ffi::OsStr| {
            let mut times: Vec<u128> = (0..50)
                .map(|_| {
                    let start = Instant::now();
                    let status = Command::new(program).arg("--version").output().unwrap();
                    assert!(status.status.success());
                    start.elapsed().as_micros()
                })
                .collect();
            times.sort_unstable();
            times.get(25).copied().unwrap_or(0)
        };
        let through = median(self.script.as_os_str());
        let direct = median(real);
        through.saturating_sub(direct)
    }
}

#[derive(Debug, Clone, Default)]
struct Calls(BTreeMap<String, usize>);

impl Calls {
    fn total(&self) -> usize {
        self.0.values().sum()
    }

    fn of(&self, verb: &str) -> usize {
        self.0.get(verb).copied().unwrap_or(0)
    }

    fn describe(&self) -> String {
        if self.0.is_empty() {
            return "none".to_owned();
        }
        self.0
            .iter()
            .map(|(verb, n)| format!("{verb}:{n}"))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

// ------------------------------------------------------------------------------------ the built history

/// What a built history knows of itself, kept beside it.
#[derive(Debug, Clone)]
struct Built {
    /// The blocks of the default view's log.
    blocks: usize,
    /// The middle edit: its proposal, its block and its number in the log (the first block is 1).
    middle: (String, String, usize),
    /// The edit near the start.
    decoy: (String, String, usize),
    /// The newest block.
    tip: String,
}

impl Built {
    fn save(&self, to: &std::path::Path) {
        let text = format!(
            "blocks={}\nmiddle={},{},{}\ndecoy={},{},{}\ntip={}\n",
            self.blocks,
            self.middle.0,
            self.middle.1,
            self.middle.2,
            self.decoy.0,
            self.decoy.1,
            self.decoy.2,
            self.tip
        );
        std::fs::write(to, text).unwrap();
    }

    fn load(from: &std::path::Path) -> Option<Built> {
        let text = std::fs::read_to_string(from).ok()?;
        let field = |key: &str| -> Option<String> {
            text.lines()
                .find_map(|l| l.strip_prefix(&format!("{key}=")))
                .map(str::to_owned)
        };
        let triple = |key: &str| -> Option<(String, String, usize)> {
            let value = field(key)?;
            let mut parts = value.split(',');
            Some((
                parts.next()?.to_owned(),
                parts.next()?.to_owned(),
                parts.next()?.parse().ok()?,
            ))
        };
        Some(Built {
            blocks: field("blocks")?.parse().ok()?,
            middle: triple("middle")?,
            decoy: triple("decoy")?,
            tip: field("tip")?,
        })
    }
}

fn prikk_version(real: &std::ffi::OsStr) -> String {
    let out = Command::new(real).arg("--version").output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .replace(' ', "-")
}

fn fixtures_dir() -> PathBuf {
    let dir = PathBuf::from(env_or(
        FIXTURES,
        &std::env::temp_dir()
            .join("inklkunn-measure-fixtures")
            .to_string_lossy(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
    let status = Command::new("cp")
        .arg("-a")
        .arg(from)
        .arg(to)
        .status()
        .unwrap();
    assert!(status.success(), "cp {from:?} {to:?}");
}

/// The wiki at the end of a build or of a copy: signed in as the person it needs.
fn sign_in_as(running: &Running, login: &'static str, name: &'static str) -> Person {
    let password = password_for(login);
    let mut browser = running.browser();
    sign_in(&mut browser, login, &password, "/");
    Person {
        login,
        name,
        password,
        browser,
    }
}

/// An edit of `title` through the pages in `view`: proposed by `noor`, approved and accepted by `maya`; the proposal's id.
fn edit_page(
    noor: &mut Person,
    maya: &mut Person,
    title: &str,
    view: Option<&str>,
    n: usize,
    accept: bool,
) -> String {
    let mut uri = format!("/handbook/-/proposals/new?page={title}");
    if let Some(view) = view {
        uri.push_str(&format!("&view={view}"));
    }
    let page = noor.browser.get(&uri);
    assert_eq!(page.status, 200, "{uri}: {}", page.body);
    let mut form = form_to(&page, "/handbook/-/proposals/new");
    let before = form.get("p0_text").to_owned();
    let line = format!("{COUNTER}{n}.");
    let after = match before.find(COUNTER) {
        Some(at) => {
            let end = before
                .get(at..)
                .unwrap()
                .find('\n')
                .map_or(before.len(), |e| at + e);
            format!(
                "{}{line}{}",
                before.get(..at).unwrap(),
                before.get(end..).unwrap()
            )
        }
        None => format!("{}\n\n{line}\n", before.trim_end()),
    };
    assert_ne!(before, after, "the edit changed nothing");
    form.set("p0_text", &after)
        .set("summary", &format!("Edit {n}."));
    let sent = noor.browser.submit(&form, Some(("op", "submit")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let id = proposal_id(&sent.location());
    if accept {
        let accepted = super::knowledge::approve_and_accept(maya, &id);
        super::knowledge::signed_into_history(&accepted);
    }
    id
}

/// **What time passing does to the records the quotas read**, so that the build is not made to wait the hours the limits of a
/// person's proposals ask for (thirty an hour, half of that for a week-old account; eight MiB of accepted changes a day): the
/// accounts are made old once, and from time to time every time the quotas count is moved two days back (the server's clock
/// cannot be moved). No limit is changed, no check skipped: the checks run on every proposal and find what two days passing would
/// leave them. The acceptance records are append-only by trigger, which is lifted for the one statement and put back as it was.
fn pass_time(store: &std::path::Path, accounts_too: bool) {
    let db = rusqlite::Connection::open(store).unwrap();
    db.busy_timeout(Duration::from_secs(30)).unwrap();
    if accounts_too {
        db.execute("UPDATE accounts SET created_at = created_at - 864000", [])
            .unwrap();
    }
    db.execute(
        "UPDATE proposals SET created_at = created_at - 172800, \
         submitted_at = submitted_at - 172800",
        [],
    )
    .unwrap();
    let trigger: String = db
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'acceptances_no_update'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    db.execute_batch("DROP TRIGGER acceptances_no_update")
        .unwrap();
    db.execute(
        "UPDATE acceptances SET created_at = created_at - 172800",
        [],
    )
    .unwrap();
    db.execute_batch(&trigger).unwrap();
}

/// A new page through the pages: proposed by `noor`, approved and accepted by `maya`.
fn create_page(noor: &mut Person, maya: &mut Person, title: &str) {
    let uri = format!("/handbook/-/proposals/new?new=1&title={title}");
    let page = noor.browser.get(&uri);
    assert_eq!(page.status, 200, "{uri}: {}", page.body);
    let mut form = form_to(&page, "/handbook/-/proposals/new");
    form.set(
        "p0_text",
        &format!("The page {title}, made for the measurement.\n"),
    )
    .set("summary", &format!("The page {title}."));
    let sent = noor.browser.submit(&form, Some(("op", "submit")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let id = proposal_id(&sent.location());
    let accepted = super::knowledge::approve_and_accept(maya, &id);
    super::knowledge::signed_into_history(&accepted);
}

/// The history of `blocks` blocks, built once and kept under `fixtures`; the path of its `data` directory and what it knows.
fn build_history(blocks: usize, fixtures: &std::path::Path, version: &str) -> (PathBuf, Built) {
    let home = fixtures.join(format!("{version}-{blocks}"));
    let meta = home.join("built.txt");
    if let Some(built) = Built::load(&meta) {
        eprintln!("history of {blocks} blocks: kept at {}", home.display());
        return (home.join("data"), built);
    }
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let started = Instant::now();
    let world = World::new(&format!("measure-build-{blocks}"))
        .expect("the measurement needs a real prikk on PATH");
    let running = Running::start(&world);
    let mut maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let mut noor = join(
        &world,
        &running,
        "noor",
        "Noor Contributor",
        Role::Contributor,
    );
    // The other pages are made first (they are blocks of the history: the first one, and one for each of them).
    for title in OTHER_PAGES {
        create_page(&mut noor, &mut maya, title);
    }
    let store_file = world.settings.store_path();
    pass_time(&store_file, true);
    let edits = blocks.saturating_sub(1 + OTHER_PAGES.len());
    let (middle_at, decoy_at) = (edits / 2, 2.min(edits.saturating_sub(1)));
    let (mut middle, mut decoy) = (None, None);
    let mut other = 0;
    for i in 0..edits {
        // Half of the edits are to Help; the others go round the other pages.
        let title = if i % 2 == 0 {
            HELP_TITLE
        } else {
            other += 1;
            OTHER_PAGES.get(other % OTHER_PAGES.len()).copied().unwrap()
        };
        // The middle edit and the decoy are Help's.
        let title = if i == middle_at || i == decoy_at {
            HELP_TITLE
        } else {
            title
        };
        let id = edit_page(&mut noor, &mut maya, title, None, i + 1, true);
        if i == middle_at || i == decoy_at {
            let newest = world.blocks().first().cloned().unwrap();
            let kept = (id, newest, i + 2 + OTHER_PAGES.len());
            if i == middle_at {
                middle = Some(kept);
            } else {
                decoy = Some(kept);
            }
        }
        if (i + 1) % 12 == 0 {
            pass_time(&store_file, false);
        }
        if (i + 1) % 100 == 0 {
            eprintln!(
                "build {blocks}: {} of {edits} edits in {:.0} s ({:.2} s each over the last 100) load {}",
                i + 1,
                started.elapsed().as_secs_f64(),
                started.elapsed().as_secs_f64() / (i + 1) as f64,
                loadavg()
            );
        }
    }
    // `World::blocks` reads the log's default window; the whole log is asked for here.
    let log: Vec<String> = {
        use inklkunn_prikk::{Point, Storage};
        let layout = RootLayout::new(world.settings.wikis_root());
        world
            .storage()
            .history(
                &layout.repo_dir(&world.wiki.id),
                &Point::View(world.wiki.default_view.clone()),
                Some(10_000),
            )
            .unwrap()
            .blocks
            .into_iter()
            .map(|b| b.block_id)
            .collect()
    };
    assert_eq!(log.len(), blocks, "the log holds the blocks asked for");
    let built = Built {
        blocks,
        middle: middle.expect("a middle edit"),
        decoy: decoy.expect("a decoy edit"),
        tip: log.first().cloned().unwrap(),
    };
    running.stop();
    copy_tree(&world.tmp.0.join("data"), &home.join("data"));
    built.save(&meta);
    eprintln!(
        "built a history of {blocks} blocks in {:.0} s, kept at {}",
        started.elapsed().as_secs_f64(),
        home.display()
    );
    (home.join("data"), built)
}

// ------------------------------------------------------------------------------------ one run

/// A copy of a built history as a world, with the counting prikk in front of the real one.
fn copy_world(tag: &str, data: &std::path::Path) -> (World, Counting, std::ffi::OsString) {
    let tmp = dir(&std::env::temp_dir(), tag);
    let shm = dir(Path::new("/dev/shm"), tag);
    copy_tree(data, &tmp.0.join("data"));
    let secret_file = tmp.0.join("operator.secret");
    std::fs::write(&secret_file, SECRET).unwrap();
    std::fs::set_permissions(&secret_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let data_dir = tmp.0.join("data");
    let real = crate::keys::tests::prikk_probes::prikk_first()
        .expect("the measurement needs a real prikk on PATH")
        .into_os_string();
    let settings = Settings {
        key_dir: data_dir.join("keys"),
        data_dir,
        secret_file,
        seed_dir: shm.0.clone(),
        prikk: real.clone(),
        dev_unconfined: true,
    };
    let store = Arc::new(SqliteStore::open(&settings.store_path()).unwrap());
    let wiki = store.by_slug(&Slug::new("handbook").unwrap()).unwrap();
    let mut world = World {
        tmp,
        _shm: shm,
        settings,
        store,
        wiki,
    };
    let counting = Counting::install(&mut world, &real);
    (world, counting, real)
}

/// One request of a settling: what it cost and what it said.
#[derive(Debug, Clone)]
struct Step {
    ms: f64,
    calls: Calls,
    status: u16,
    hidden: bool,
}

impl Step {
    fn answer(&self) -> &'static str {
        match (self.status, self.hidden) {
            (200, true) => "200 hidden-with-tombstone",
            (200, false) => "200 shown",
            (503, _) => "503 not checked yet",
            _ => "other",
        }
    }
}

fn ask(anonymous: &mut Browser, counting: &Counting, uri: &str) -> Step {
    counting.take();
    let started = Instant::now();
    let reply = anonymous.get(uri);
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let calls = counting.take();
    if reply.status == 503 {
        assert!(
            reply.body.contains(NOT_YET_WORDS),
            "a 503 that is not the index's: {}",
            reply.body
        );
    }
    Step {
        ms,
        calls,
        status: reply.status,
        hidden: reply.body.contains(HIDDEN_WORDS),
    }
}

/// Ask until the answer is not *not checked yet*.
fn settle(anonymous: &mut Browser, counting: &Counting, uri: &str) -> Vec<Step> {
    let mut steps = Vec::new();
    loop {
        let step = ask(anonymous, counting, uri);
        let again = step.status == 503;
        steps.push(step);
        if !again || steps.len() >= SETTLE_CAP {
            return steps;
        }
    }
}

#[derive(Debug, Clone)]
struct Outcome {
    k: usize,
    scenario: &'static str,
    run: usize,
    load: String,
    /// The hide's own cost (the form's latency) and its calls, when it was made in the scenario.
    hide: Option<Hide>,
    /// The requests of the middle block's settling.
    middle: Vec<Step>,
    /// The further requests until the newest block's text settles (the tip's own).
    tip: Vec<Step>,
    warmed: Option<Step>,
    repeat: Option<Step>,
    /// The decoy hide's cost and the rounds of warming before (the warm scenario).
    note: String,
}

/// A moderator's hide of the text of a proposal's revision: the form's page (which lists the later changes of the text, so
/// builds the index toward the tip, within its bound) and the form's POST (which records the hide and warms the index again).
#[derive(Debug, Clone)]
struct Hide {
    form: Step,
    post: Step,
}

fn hide_content(maya: &mut Person, counting: &Counting, proposal: &str) -> Hide {
    let query = format!("kind=proposal-revision&proposal={proposal}&seq=1");
    counting.take();
    let started = Instant::now();
    let page = maya.browser.get(&format!("{MODERATE}?{query}"));
    let form_ms = started.elapsed().as_secs_f64() * 1000.0;
    let form_step = Step {
        ms: form_ms,
        calls: counting.take(),
        status: page.status,
        hidden: false,
    };
    assert_eq!(page.status, 200, "{query}: {}", page.body);
    let mut form = form_to(&page, MODERATE);
    form.check("part_content", "1");
    let category = form
        .choices
        .iter()
        .find(|c| c.0 == "category")
        .map(|c| c.1.clone())
        .expect("a category");
    form.check("category", &category);
    counting.take();
    let started = Instant::now();
    let hid = maya.browser.submit(&form, None);
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(hid.status, 200, "{query}: {}\n{}", hid.head, hid.body);
    Hide {
        form: form_step,
        post: Step {
            ms,
            calls: counting.take(),
            status: hid.status,
            hidden: false,
        },
    }
}

const MODERATE: &str = "/handbook/-/moderate";

fn permalink(block: &str) -> String {
    format!("/handbook/-/at/{block}/{HELP_TITLE}")
}

/// The two settlings and the warmed request, on a running server whose hide is in force.
fn requests(anonymous: &mut Browser, counting: &Counting, built: &Built, out: &mut Outcome) {
    out.middle = settle(anonymous, counting, &permalink(&built.middle.1));
    out.tip = settle(anonymous, counting, &permalink(&built.tip));
    // A request for another old block (the early edit's) once the index holds it: the page is not held, the index is.
    out.warmed = Some(ask(anonymous, counting, &permalink(&built.decoy.1)));
    // And the same request again: the page is held now.
    out.repeat = Some(ask(anonymous, counting, &permalink(&built.decoy.1)));
}

fn run_once(
    k: usize,
    run: usize,
    scenario: &'static str,
    data: &std::path::Path,
    built: &Built,
) -> Outcome {
    let (mut world, counting, real) = copy_world(&format!("measure-{scenario}-{k}-{run}"), data);
    let mut out = Outcome {
        k,
        scenario,
        run,
        load: loadavg(),
        hide: None,
        middle: Vec::new(),
        tip: Vec::new(),
        warmed: None,
        repeat: None,
        note: String::new(),
    };
    let mut running = Running::start(&world);
    counting.take();
    let mut maya = sign_in_as(&running, "maya", "Maya Maintainer");
    let mut anonymous = running.browser();
    match scenario {
        "natural" => {
            out.hide = Some(hide_content(&mut maya, &counting, &built.middle.0));
            requests(&mut anonymous, &counting, built, &mut out);
        }
        "cold" => {
            hide_content(&mut maya, &counting, &built.middle.0);
            running.services.read.clear_derived(&world.wiki).unwrap();
            running.stop();
            running = Running::start(&world);
            counting.take();
            anonymous = running.browser();
            requests(&mut anonymous, &counting, built, &mut out);
        }
        "warm" => {
            // The index is built to the tip by an earlier hide (an early edit's), then the middle edit is hidden.
            let decoy = hide_content(&mut maya, &counting, &built.decoy.0);
            let mut rounds = 0;
            counting.take();
            while !running.services.read.warm_writes(&world.wiki).unwrap() {
                rounds += 1;
                assert!(rounds < 1_000, "the index did not reach the tip");
            }
            let warming = counting.take();
            out.note = format!(
                "decoy hide {:.0} ms ({} shows); then {} further warm_writes calls ({} shows) to the tip",
                decoy.form.ms + decoy.post.ms,
                decoy.form.calls.of("show") + decoy.post.calls.of("show"),
                rounds + 1,
                warming.of("show")
            );
            out.hide = Some(hide_content(&mut maya, &counting, &built.middle.0));
            requests(&mut anonymous, &counting, built, &mut out);
        }
        "views" => {
            let mut noor = sign_in_as(&running, "noor", "Noor Contributor");
            let mut last = String::new();
            let started = Instant::now();
            for i in 1..=20 {
                let name = format!("v{i:02}");
                let page = maya.browser.get("/handbook/-/views/new");
                let mut form = form_to(&page, "/handbook/-/views/new");
                form.set("name", &name)
                    .check("kind", "interpretation")
                    .set("from", "main");
                let made = maya.browser.submit(&form, None);
                assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
                edit_page(
                    &mut noor,
                    &mut maya,
                    HELP_TITLE,
                    Some(&name),
                    1_000_000 + i,
                    true,
                );
                if i == 20 {
                    // A proposal left open in the last view: its base is the block that only that view holds, and its page
                    // reads the text it was written against.
                    last = edit_page(
                        &mut noor,
                        &mut maya,
                        HELP_TITLE,
                        Some(&name),
                        2_000_000,
                        false,
                    );
                }
            }
            out.note = format!(
                "twenty views opened in {:.0} s",
                started.elapsed().as_secs_f64()
            );
            hide_content(&mut maya, &counting, &built.middle.0);
            running.services.read.clear_derived(&world.wiki).unwrap();
            running.stop();
            running = Running::start(&world);
            counting.take();
            anonymous = running.browser();
            // The page of that proposal: the first request asks the default view for the block and then up to sixteen others.
            let uri = format!("/handbook/-/proposals/{last}");
            out.middle = settle(&mut anonymous, &counting, &uri);
            out.warmed = Some(ask(&mut anonymous, &counting, &uri));
        }
        other => panic!("no scenario {other}"),
    }
    out.note = format!(
        "{}{}wrapper {} us/call",
        out.note,
        if out.note.is_empty() { "" } else { "; " },
        counting.overhead_us(&real)
    );
    let _ = (&mut world, &anonymous);
    running.stop();
    out
}

// ------------------------------------------------------------------------------------ the report

fn sum_ms(steps: &[Step]) -> f64 {
    steps.iter().map(|s| s.ms).sum()
}

fn shows(steps: &[Step]) -> usize {
    steps.iter().map(|s| s.calls.of("show")).sum()
}

fn calls(steps: &[Step]) -> usize {
    steps.iter().map(|s| s.calls.total()).sum()
}

fn print_outcome(o: &Outcome) {
    println!(
        "RUN k={} scenario={} run={} load-before={} load-after={}",
        o.k,
        o.scenario,
        o.run,
        o.load,
        loadavg()
    );
    if let Some(h) = &o.hide {
        for (label, step) in [("form page", &h.form), ("post", &h.post)] {
            println!(
                "  hide, {label}: {:.0} ms, prikk calls {} ({}), shows {}",
                step.ms,
                step.calls.total(),
                step.calls.describe(),
                step.calls.of("show")
            );
        }
    }
    for (label, steps) in [("middle block", &o.middle), ("newest block", &o.tip)] {
        if steps.is_empty() {
            continue;
        }
        println!(
            "  {label}: settled in {} requests, {:.0} ms in all, {} prikk calls, {} shows",
            steps.len(),
            sum_ms(steps),
            calls(steps),
            shows(steps)
        );
        for (i, s) in steps.iter().enumerate() {
            if i < 3 || i + 1 == steps.len() {
                println!(
                    "    request {}: {:.0} ms, calls {} ({}), shows {}, {}",
                    i + 1,
                    s.ms,
                    s.calls.total(),
                    s.calls.describe(),
                    s.calls.of("show"),
                    s.answer()
                );
            } else if i == 3 {
                println!("    ...");
            }
        }
    }
    for (label, step) in [
        ("warmed request (another old block)", &o.warmed),
        ("the same again", &o.repeat),
    ] {
        if let Some(w) = step {
            println!(
                "  {label}: {:.0} ms, calls {} ({}), shows {}, {}",
                w.ms,
                w.calls.total(),
                w.calls.describe(),
                w.calls.of("show"),
                w.answer()
            );
        }
    }
    if !o.note.is_empty() {
        println!("  note: {}", o.note);
    }
    let last_middle = o.middle.last().map(|s| s.answer());
    println!(
        "  the middle block settled as: {}",
        last_middle.unwrap_or("not asked")
    );
}

fn min_median(mut values: Vec<f64>) -> (f64, f64) {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let min = values.first().copied().unwrap_or(0.0);
    let median = values.get(values.len() / 2).copied().unwrap_or(0.0);
    (min, median)
}

fn print_table(version: &str, outcomes: &[Outcome]) {
    println!(
        "\nTABLE (prikk {version}; times are min / median over the runs; counts are of the first run, and the same in every run unless said)"
    );
    println!(
        "K | scenario | runs | first request: ms / prikk calls / shows / answer | requests to settle: middle block + newest block (more) | settling in all: middle ms, newest ms | warmed request: ms / prikk calls | hide: form ms, post ms / shows in all"
    );
    let mut keys: Vec<(usize, &'static str)> = Vec::new();
    for o in outcomes {
        if !keys.contains(&(o.k, o.scenario)) {
            keys.push((o.k, o.scenario));
        }
    }
    for (k, scenario) in keys {
        let runs: Vec<&Outcome> = outcomes
            .iter()
            .filter(|o| o.k == k && o.scenario == scenario)
            .collect();
        let Some(first) = runs.first().copied() else {
            continue;
        };
        let ms = |pick: &dyn Fn(&Outcome) -> Option<f64>| {
            let (min, median) = min_median(runs.iter().filter_map(|o| pick(o)).collect());
            format!("{min:.0} / {median:.0}")
        };
        let first_step = first.middle.first();
        let counts: Vec<String> = runs
            .iter()
            .map(|o| format!("{}+{}", o.middle.len(), o.tip.len()))
            .collect();
        let same = counts.iter().all(|c| Some(c) == counts.first());
        println!(
            "{k} | {scenario} | {} | {} ms ({} calls, {} shows, {}) | {} + {} more{} | {}, {} | {} ms ({} calls) | {}",
            runs.len(),
            ms(&|o| o.middle.first().map(|s| s.ms)),
            first_step.map_or(0, |s| s.calls.total()),
            first_step.map_or(0, |s| s.calls.of("show")),
            first_step.map_or("none", Step::answer),
            first.middle.len(),
            first.tip.len(),
            if same {
                String::new()
            } else {
                format!(" (runs: {})", counts.join(", "))
            },
            ms(&|o| (!o.middle.is_empty()).then(|| sum_ms(&o.middle))),
            ms(&|o| (!o.tip.is_empty()).then(|| sum_ms(&o.tip))),
            ms(&|o| o.warmed.as_ref().map(|s| s.ms)),
            first.warmed.as_ref().map_or(0, |s| s.calls.total()),
            first.hide.as_ref().map_or_else(
                || "none".to_owned(),
                |h| format!(
                    "form {}, post {} / {} shows ({} + {})",
                    ms(&|o| o.hide.as_ref().map(|s| s.form.ms)),
                    ms(&|o| o.hide.as_ref().map(|s| s.post.ms)),
                    h.form.calls.of("show") + h.post.calls.of("show"),
                    h.form.calls.of("show"),
                    h.post.calls.of("show")
                )
            ),
        );
    }
}

#[test]
#[ignore = "a measurement, not a check: run by hand (see the file's head)"]
fn the_writes_index_on_a_long_history() {
    let sizes: Vec<usize> = env_or(SIZES, "500")
        .split(',')
        .map(|s| s.trim().parse().expect("a size"))
        .collect();
    let runs: usize = env_or(RUNS, "1").parse().expect("a number of runs");
    let wanted = env_or(SCENARIOS, "natural,cold,warm");
    let scenarios: Vec<&'static str> = ["natural", "cold", "warm", "views"]
        .into_iter()
        .filter(|s| wanted.split(',').any(|w| w.trim() == *s))
        .collect();
    let real = crate::keys::tests::prikk_probes::prikk_first()
        .expect("the measurement needs a real prikk on PATH")
        .into_os_string();
    let version = prikk_version(&real);
    println!("prikk: {version} ({})", real.to_string_lossy());
    println!(
        "load average at the start: {}",
        std::fs::read_to_string("/proc/loadavg")
            .unwrap_or_default()
            .trim()
    );
    let fixtures = fixtures_dir();
    let mut outcomes: Vec<Outcome> = Vec::new();
    for k in sizes {
        let (data, built) = build_history(k, &fixtures, &version);
        println!(
            "history K={k}: middle edit is block {} of {}, the newest block is {}",
            built.middle.2, built.blocks, built.tip
        );
        if std::env::var(BUILD_ONLY).is_ok() {
            continue;
        }
        for run in 1..=runs {
            for scenario in &scenarios {
                let outcome = run_once(k, run, scenario, &data, &built);
                print_outcome(&outcome);
                outcomes.push(outcome);
            }
        }
    }
    if !outcomes.is_empty() {
        print_table(&version, &outcomes);
    }
}
