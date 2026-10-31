#![allow(clippy::expect_used, clippy::unwrap_used)]
//! The server's acceptor against a real prikk, a real key store and a real SQLite store, behind the real
//! router and a real socket. Every test here needs prikk and prints `skipping: prikk not on PATH` without it.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use inklkunn_core::proposal::ProposalEdit;
use inklkunn_core::{IdentityAuthority, Principal};
use inklkunn_prikk::{Admission, PrikkCli};
use inklkunn_store::{
    Account, AccountId, AccountStore, ActStore, DisplayName, FileChange, ProposalRecord,
    ProposalState, ProposalStore, RecoveryFlag, RecoveryReason, Role, RoleGrant, RoleStore, Slug,
    Verification, VerificationState, WikiRecord, WikiStore,
};
use inklkunn_web::serve::Shutdown;
use inklkunn_web::{Identify, Services, routes};

use super::super::*;
use super::*;
use crate::cli;

/// What the operator's secret file holds in these tests: unmistakable, so a search for it means something.
const SECRET: &str = "u5-operator-secret-7f3a9c1e5b2d4e68-do-not-print";
/// A different one, for a server that is started with the wrong secret.
const WRONG_SECRET: &str = "u5-the-wrong-secret-0d9c8b7a6f5e4d3c-nor-this";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-u5-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
    Dir(p)
}

fn real_prikk() -> Option<PathBuf> {
    let found = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("prikk"))
            .find(|c| c.is_file())
    });
    if found.is_none() {
        println!("skipping: prikk not on PATH");
    }
    found
}

/// A stand-in `prikk` that runs the real one after logging the verb, and holds a verb for the seconds a control
/// file names (`delay-<verb>`), leaving `inside-<verb>` while it does.
fn standin(ctl: &Path, real: &Path) -> PathBuf {
    let script = format!(
        r#"#!/bin/sh
CTL="{ctl}"
verb="$1"
echo "$verb" >> "$CTL/calls.log"
if [ -e "$CTL/delay-$verb" ]; then : > "$CTL/inside-$verb"; sleep "$(cat "$CTL/delay-$verb")"; rm -f "$CTL/inside-$verb"; fi
exec "{real}" "$@"
"#,
        ctl = ctl.display(),
        real = real.display()
    );
    let path = ctl.join("prikk");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A request is the account named by `x-test-account` (a session), else anonymous.
struct HeaderAccount(IdentityAuthority);

impl Identify for HeaderAccount {
    fn principal(&self, headers: &inklkunn_web::HeaderMap) -> Principal {
        headers
            .get("x-test-account")
            .and_then(|v| v.to_str().ok())
            .and_then(AccountId::parse)
            .map_or(Principal::Anonymous, |id| Principal::session(&self.0, id))
    }

    fn session(&self, _: &inklkunn_web::HeaderMap) -> Option<inklkunn_web::SessionView> {
        None
    }

    fn for_write(
        &self,
        headers: &inklkunn_web::HeaderMap,
        _: Option<&str>,
    ) -> inklkunn_web::WriteIdentity {
        // A test identity names its account in a header and has no cookie, so no token is asked of it.
        inklkunn_web::WriteIdentity::Principal(self.principal(headers))
    }
}

/// A deployment with one public wiki, made by the command line with the real signer, and two people.
struct World {
    tmp: Dir,
    _shm: Dir,
    settings: Settings,
    store: Arc<SqliteStore>,
    wiki: WikiRecord,
    maintainer: AccountId,
    contributor: AccountId,
    ctl: PathBuf,
}

impl World {
    fn new(tag: &str, with_standin: bool) -> Option<World> {
        let real = real_prikk()?;
        let tmp = dir(&std::env::temp_dir(), tag);
        let shm = dir(Path::new("/dev/shm"), tag);
        let data_dir = tmp.0.join("data");
        let secret_file = tmp.0.join("operator.secret");
        std::fs::write(&secret_file, SECRET).unwrap();
        std::fs::set_permissions(&secret_file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let ctl = tmp.0.join("ctl");
        std::fs::create_dir_all(&ctl).unwrap();
        let prikk = if with_standin {
            standin(&ctl, &real).into_os_string()
        } else {
            OsString::from("prikk")
        };
        let settings = Settings {
            key_dir: data_dir.join("keys"),
            data_dir,
            secret_file,
            seed_dir: shm.0.clone(),
            prikk,
            dev_unconfined: true,
        };
        let mut world = World {
            tmp,
            _shm: shm,
            store: Arc::new(SqliteStore::open_in_memory().unwrap()),
            wiki: placeholder(),
            maintainer: AccountId::new(),
            contributor: AccountId::new(),
            settings,
            ctl,
        };
        let (status, _, err) = world.cli(&[
            "wiki",
            "create",
            "handbook",
            "--name",
            "Handbook",
            "--visibility",
            "public",
        ]);
        assert_eq!(status, 0, "{err}");
        world.store = Arc::new(SqliteStore::open(&world.settings.store_path()).unwrap());
        world.wiki = world
            .store
            .by_slug(&Slug::new("handbook").unwrap())
            .unwrap();
        world.maintainer = world.person("Maya Maintainer", Role::Maintainer);
        world.contributor = world.person("Noor Contributor", Role::Contributor);
        // The creation is verified; age that, so that a verification asked for after a change is not turned
        // away as too recent (and shows when it ran).
        world
            .store
            .set_verification(
                &world.wiki.id,
                Verification {
                    state: VerificationState::Verified,
                    at: Some(1_000),
                },
            )
            .unwrap();
        Some(world)
    }

    fn person(&self, name: &str, role: Role) -> AccountId {
        let id = AccountId::new();
        self.store
            .add_account(&Account {
                id: id.clone(),
                display_name: DisplayName::new(name).unwrap(),
                local: None,
                created_at: 1,
                disabled: false,
            })
            .unwrap();
        self.store
            .grant(RoleGrant {
                account: id.clone(),
                wiki: self.wiki.id.clone(),
                role,
                granted_by: None,
                granted_at: 1,
            })
            .unwrap();
        id
    }

    /// `inklkunn --dev-unconfined <args>` in this process; (status, stdout, stderr).
    fn cli(&self, args: &[&str]) -> (i32, String, String) {
        let s = &self.settings;
        let env: HashMap<&str, OsString> = HashMap::from([
            ("INKLKUNN_DATA_DIR", s.data_dir.clone().into_os_string()),
            (
                "INKLKUNN_SECRET_FILE",
                s.secret_file.clone().into_os_string(),
            ),
            ("INKLKUNN_KEY_DIR", s.key_dir.clone().into_os_string()),
            ("INKLKUNN_SEED_DIR", s.seed_dir.clone().into_os_string()),
            ("INKLKUNN_PRIKK", s.prikk.clone()),
        ]);
        let mut argv = vec!["--dev-unconfined".to_owned()];
        argv.extend(args.iter().map(|a| (*a).to_owned()));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = cli::run(
            &argv,
            &|k| env.get(k).cloned(),
            1_800_000_000,
            &mut out,
            &mut err,
        );
        (
            status,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    fn layout(&self) -> RootLayout {
        RootLayout::new(self.settings.wikis_root())
    }

    fn storage(&self) -> PrikkCli {
        PrikkCli::open(
            &self.settings.prikk,
            inklkunn_prikk::sandbox::Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(500)),
        )
        .unwrap()
    }

    fn blocks(&self) -> usize {
        use inklkunn_prikk::{Point, Storage};
        self.storage()
            .history(
                &self.layout().repo_dir(&self.wiki.id),
                &Point::View(self.wiki.default_view.clone()),
                None,
            )
            .unwrap()
            .blocks
            .len()
    }

    fn remove_the_secret(&self) {
        std::fs::remove_file(&self.settings.secret_file).unwrap();
    }

    fn state_of(&self, p: &ProposalRecord) -> ProposalState {
        self.store
            .proposal(&p.id, Some(&self.contributor))
            .unwrap()
            .unwrap()
            .state
    }
}

/// Only for the field's sake before the wiki exists.
fn placeholder() -> WikiRecord {
    let store = SqliteStore::open_in_memory().unwrap();
    store
        .create(inklkunn_store::NewWiki::new(
            Slug::new("placeholder").unwrap(),
            DisplayName::new("Placeholder").unwrap(),
        ))
        .unwrap()
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// What an HTTP answer came to.
#[derive(Debug)]
struct Reply {
    status: u16,
    head: String,
    body: String,
}

/// One request over a socket, on the application origin.
fn http(port: u16, method: &str, path: &str, account: Option<&AccountId>, body: &str) -> Reply {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(60))).unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\nSec-Fetch-Site: same-origin\r\n"
    );
    if let Some(a) = account {
        req.push_str(&format!("X-Test-Account: {a}\r\n"));
    }
    if method == "POST" {
        req.push_str(&format!(
            "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n",
            body.len()
        ));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    Reply {
        status: head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        head: head.to_owned(),
        body: body.to_owned(),
    }
}

/// A server that is running: the lines it printed and logged, and every answer it gave.
struct Running {
    port: u16,
    stop: Option<inklkunn_web::serve::StopHandle>,
    join: Option<std::thread::JoinHandle<(Result<(), String>, String)>>,
    log: Arc<Mutex<Vec<String>>>,
    notes: Vec<String>,
    banner: Vec<String>,
    services: Arc<Services>,
    answers: Mutex<Vec<String>>,
}

impl Running {
    /// Start the server on `settings` with a test identity. `tweak` may replace parts of the services (an
    /// acceptor with a short wait) before the site is built.
    fn start(world: &World, tweak: impl FnOnce(&mut Services)) -> Running {
        Self::start_on(&world.settings, tweak)
    }

    fn start_on(settings: &Settings, tweak: impl FnOnce(&mut Services)) -> Running {
        let (app, content) = (free_port(), free_port());
        let web = WebSettings {
            bind: format!("127.0.0.1:{app}"),
            app_origin: format!("http://localhost:{app}"),
            content_origin: format!("http://localhost:{content}"),
            admission_budget_mib: 1024,
            admission_wait_ms: 2000,
            read: crate::config::ReadSettings::default(),
            trusted_proxies: Vec::new(),
            join: inklkunn_auth::JoinPolicy::Off,
            oidc: None,
        };
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = log.clone();
        let mut prepared = prepare_with(
            settings,
            &web,
            IdentityAuthority::for_tests(),
            Arc::new(move |line| sink.lock().unwrap().push(line.to_owned())),
        )
        .unwrap();
        let mut services = Services {
            identity: Arc::new(HeaderAccount(IdentityAuthority::for_tests())),
            ..(*prepared._services).clone()
        };
        tweak(&mut services);
        let services = Arc::new(services);
        prepared.site = routes::site_with(&prepared.web, Some(services.clone())).unwrap();
        let notes = prepared.notes.clone();
        let banner = prepared.banner.clone();
        let (shutdown, stop) = Shutdown::manual();
        let (tx, rx) = std::sync::mpsc::channel();
        let join = std::thread::spawn(move || {
            let mut err = Vec::new();
            let result = run(prepared, shutdown, &mut err, move |addr| {
                tx.send(addr).unwrap();
            });
            (result, String::from_utf8(err).unwrap())
        });
        rx.recv_timeout(Duration::from_secs(30))
            .expect("the server bound");
        Running {
            port: app,
            stop: Some(stop),
            join: Some(join),
            log,
            notes,
            banner,
            services,
            answers: Mutex::new(Vec::new()),
        }
    }

    fn get(&self, path: &str, account: Option<&AccountId>) -> Reply {
        self.ask("GET", path, account, "")
    }

    fn post(&self, path: &str, account: Option<&AccountId>, body: &str) -> Reply {
        self.ask("POST", path, account, body)
    }

    fn ask(&self, method: &str, path: &str, account: Option<&AccountId>, body: &str) -> Reply {
        let reply = http(self.port, method, path, account, body);
        self.answers
            .lock()
            .unwrap()
            .push(format!("{}\n{}", reply.head, reply.body));
        reply
    }

    /// Stop it; what it printed at start (`stderr`).
    fn stop(mut self) -> String {
        self.stop.take().unwrap().stop();
        let (result, err) = self.join.take().unwrap().join().unwrap();
        assert!(result.is_ok(), "{result:?}");
        err
    }

    /// Everything the operator could read and everything a visitor was shown, as one text.
    fn everything_seen(&self, printed: &str) -> String {
        let mut all = self.notes.join("\n");
        all.push_str(&self.banner.join("\n"));
        all.push_str(&self.log.lock().unwrap().join("\n"));
        all.push_str(&self.answers.lock().unwrap().join("\n"));
        all.push_str(printed);
        all
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop.stop();
            if let Some(join) = self.join.take() {
                let _ = join.join();
            }
        }
    }
}

const NEW_PAGE: &str = "---\ntitle: Welcome\nid: p-welcome\n---\n\nHello there.\n";

/// A held proposal by the contributor that adds a page, made through the proposal service.
fn propose(running: &Running, world: &World) -> ProposalRecord {
    let authority = IdentityAuthority::for_tests();
    let contributor = Principal::session(&authority, world.contributor.clone());
    let edit = ProposalEdit {
        summary: "Add a welcome page".to_owned(),
        files: vec![FileChange {
            path: "pages/Welcome.md".to_owned(),
            text: Some(NEW_PAGE.to_owned()),
        }],
        ..ProposalEdit::default()
    };
    let draft = running
        .services
        .proposals
        .create(&contributor, &world.wiki, None, edit)
        .unwrap_or_else(|e| panic!("create: {e}"));
    running
        .services
        .proposals
        .submit(&contributor, &world.wiki, &draft.id)
        .unwrap_or_else(|e| panic!("submit: {e}"))
}

fn accept_path(p: &ProposalRecord) -> String {
    format!("/handbook/-/proposals/{}/accept", p.id)
}

fn page_path(p: &ProposalRecord) -> String {
    format!("/handbook/-/proposals/{}", p.id)
}

const THE_SENTENCE: &str = "This server is not set up to sign changes into the history, so nothing was changed and the proposal stays held for review; tell the person who runs this server.";

#[test]
fn a_start_without_the_secret_file_serves_a_page_and_refuses_a_write_with_one_sentence() {
    let Some(world) = World::new("nosecret", false) else {
        return;
    };
    world.remove_the_secret();
    let running = Running::start(&world, |_| {});
    // ONE operator line, naming the setting and the path, saying what the server does instead.
    let lines: Vec<&String> = running
        .notes
        .iter()
        .filter(|l| l.contains("INKLKUNN_SECRET_FILE"))
        .collect();
    assert_eq!(lines.len(), 1, "{:?}", running.notes);
    let line = lines.first().unwrap();
    assert!(!line.contains('\n'));
    assert!(
        line.contains(&world.settings.secret_file.display().to_string())
            && line.contains("does not exist")
            && line.contains("reads pages and refuses every write")
            && line.contains("restarted"),
        "{line}"
    );
    assert!(
        !running
            .banner
            .iter()
            .any(|l| l.contains("accepting proposals")),
        "{:?}",
        running.banner
    );
    // It never asks for the secret anywhere: no file appeared, nothing was made in its place.
    assert!(!world.settings.secret_file.exists());

    // A page is served ...
    let proposal = propose(&running, &world);
    let page = running.get(&page_path(&proposal), Some(&world.maintainer));
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(
        page.body
            .contains("Accepting is not set up on this server."),
        "{}",
        page.body
    );
    // ... the wiki reads ...
    let home = running.get("/handbook/-/proposals", Some(&world.maintainer));
    assert_eq!(home.status, 200, "{}", home.body);
    // ... and a write is refused with the one sentence, through the real router, after the checks.
    let before = world.blocks();
    let accept = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(accept.status, 503, "{}", accept.body);
    assert!(accept.body.contains(THE_SENTENCE), "{}", accept.body);
    let refused = running.post(
        &accept_path(&proposal),
        Some(&world.contributor),
        "revision=1",
    );
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert_eq!(world.state_of(&proposal), ProposalState::HeldForReview);
    assert_eq!(world.blocks(), before, "nothing was written");
    let printed = running.stop();
    assert!(printed.contains("INKLKUNN_SECRET_FILE"), "{printed}");
}

#[test]
fn a_start_with_the_secret_accepts_a_proposal_end_to_end_and_no_secret_reaches_a_log_or_a_page() {
    let Some(world) = World::new("accept", false) else {
        return;
    };
    let running = Running::start(&world, |_| {});
    assert!(
        running
            .banner
            .iter()
            .any(|l| l.contains("accepting proposals: the signer is open")),
        "{:?}",
        running.banner
    );
    assert!(
        !running
            .notes
            .iter()
            .any(|l| l.contains("INKLKUNN_SECRET_FILE")),
        "{:?}",
        running.notes
    );
    let proposal = propose(&running, &world);
    let before = world.blocks();

    // A contributor may not; nothing is signed for them.
    let refused = running.post(
        &accept_path(&proposal),
        Some(&world.contributor),
        "revision=1",
    );
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert_eq!(world.blocks(), before);

    // The maintainer accepts: the page says it is in the history, and the history has the block.
    let accepted = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(accepted.status, 200, "{}", accepted.body);
    assert!(
        accepted.body.contains("Signed into the history"),
        "{}",
        accepted.body
    );
    assert_eq!(world.blocks(), before + 1);
    assert_eq!(world.state_of(&proposal), ProposalState::Accepted);
    let page = running.get(&page_path(&proposal), Some(&world.maintainer));
    assert!(page.body.contains("Accepted"), "{}", page.body);
    let read = running.get("/handbook/Welcome", None);
    assert_eq!(read.status, 200, "{}", read.body);
    assert!(read.body.contains("Hello there."), "{}", read.body);

    // A retry finds the record and writes nothing.
    let again = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(world.blocks(), before + 1);

    // The follow-up ran off the request path: the wiki was verified again after the change.
    until("the verification after the seal", || {
        <dyn WikiStore>::get(&*world.store, &world.wiki.id)
            .unwrap()
            .verification
            .at
            > Some(1_000)
    });

    // The reconciliation after the seal found nothing, and no seed directory was left on the tmpfs.
    assert!(
        !running
            .log
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.contains("disagreement") || l.contains("seeds")),
        "{:?}",
        running.log.lock().unwrap()
    );
    let leftovers: Vec<_> = std::fs::read_dir(&world.settings.seed_dir)
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // The secret is in none of: the operator's lines, the log, any page or answer, what the server printed.
    let printed = running.everything_seen("");
    let total = running.stop();
    for secret in [SECRET, WRONG_SECRET] {
        assert!(!printed.contains(secret), "the secret reached the output");
        assert!(!total.contains(secret), "the secret reached stderr");
    }
    assert!(
        !printed.is_empty() && printed.contains("Signed into the history"),
        "the search looked at what was said"
    );
}

#[test]
fn a_secret_that_does_not_open_the_key_store_is_the_same_server_with_one_line_that_names_no_secret()
{
    let Some(world) = World::new("wrongsecret", false) else {
        return;
    };
    std::fs::write(&world.settings.secret_file, WRONG_SECRET).unwrap();
    let running = Running::start(&world, |_| {});
    let lines: Vec<&String> = running
        .notes
        .iter()
        .filter(|l| l.contains("signer could not be opened"))
        .collect();
    assert_eq!(lines.len(), 1, "{:?}", running.notes);
    assert!(
        lines
            .first()
            .unwrap()
            .contains("reads pages and refuses every write"),
        "{lines:?}"
    );
    let proposal = propose(&running, &world);
    let accept = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(accept.status, 503, "{}", accept.body);
    assert!(accept.body.contains(THE_SENTENCE));
    let printed = running.everything_seen("");
    running.stop();
    assert!(!printed.contains(SECRET) && !printed.contains(WRONG_SECRET));
}

/// An acceptor with a short wait over the same parts, so that a busy wiki is told so in a moment.
fn impatient(world: &World) -> impl FnOnce(&mut Services) + use<> {
    let settings = world.settings.clone();
    let store = world.store.clone();
    let layout = world.layout();
    let sandbox = inklkunn_prikk::sandbox::Sandbox::Unconfined;
    move |services: &mut Services| {
        let assembled = assemble(Parts {
            settings: settings.clone(),
            sandbox,
            layout: layout.clone(),
            store: store.clone(),
            storage: Arc::new(
                PrikkCli::open(
                    &settings.prikk,
                    inklkunn_prikk::sandbox::Sandbox::Unconfined,
                    Admission::new(1 << 30, 64 << 20, Duration::from_millis(500)),
                )
                .unwrap(),
            ),
            read: services.read.clone(),
            proposals: services.proposals.clone(),
            verifier: services.verifier.clone(),
            lease: LeaseConfig {
                max_wait: Duration::from_millis(300),
                poll: Duration::from_millis(10),
                ..LeaseConfig::default()
            },
            log: services.log.clone(),
        });
        assert!(assembled.signing);
        services.acceptor = Some(assembled.acceptor);
    }
}

#[test]
fn a_wiki_the_command_line_holds_makes_the_servers_acceptance_busy_and_it_accepts_when_it_lets_go()
{
    let Some(world) = World::new("cli-holds", false) else {
        return;
    };
    let running = Running::start(&world, impatient(&world));
    let proposal = propose(&running, &world);
    let before = world.blocks();
    // The lease the command line's acts take: the wiki's lock on the root, held by another owner of it.
    let held = inklkunn_core::WriteLeases::new(world.layout())
        .try_acquire(&world.wiki.id)
        .expect("the command line has the wiki");
    let busy = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(busy.status, 503, "{}", busy.body);
    assert!(busy.body.contains("busy"), "{}", busy.body);
    assert_eq!(world.state_of(&proposal), ProposalState::HeldForReview);
    assert_eq!(
        world.blocks(),
        before,
        "nothing was written while it was held"
    );
    drop(held);
    let done = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(done.status, 200, "{}", done.body);
    assert_eq!(world.blocks(), before + 1);
    running.stop();
}

#[test]
fn a_wiki_the_server_is_signing_makes_the_command_line_wait_its_turn() {
    let Some(world) = World::new("server-holds", true) else {
        return;
    };
    let running = Running::start(&world, |_| {});
    let proposal = propose(&running, &world);
    // The server's signing call is held inside `seal` for a few seconds.
    std::fs::write(world.ctl.join("delay-seal"), "4").unwrap();
    let port = running.port;
    let (maintainer, path) = (world.maintainer.clone(), accept_path(&proposal));
    let signing =
        std::thread::spawn(move || http(port, "POST", &path, Some(&maintainer), "revision=1"));
    until("the signing call to be inside seal", || {
        world.ctl.join("inside-seal").exists()
    });
    // The command line beside the running server takes its place in the wiki's line (handoff 004 P9: maintenance
    // goes through the lease): it does not run while the server's signing call holds the wiki, and does after.
    let archive = world.tmp.0.join("busy.prikk");
    let export = {
        let archive = archive.display().to_string();
        std::thread::scope(|scope| {
            let cli =
                scope.spawn(|| world.cli(&["wiki", "export", "handbook", "--archive", &archive]));
            // While the signing call is inside `seal` (it is held there for seconds) nothing of the archive exists.
            std::thread::sleep(Duration::from_millis(1500));
            assert!(
                !archive_exists(&world),
                "the export ran while the server was signing"
            );
            cli.join().unwrap()
        })
    };
    let done = signing.join().unwrap();
    assert_eq!(done.status, 200, "{}", done.body);
    let (status, _, err) = export;
    assert_eq!(status, 0, "{err}");
    assert!(archive.exists(), "the export ran after the server let go");
    running.stop();
}

#[test]
fn a_wiki_flagged_needs_recovery_refuses_the_accept_though_the_signer_is_open() {
    let Some(world) = World::new("flagged", false) else {
        return;
    };
    let running = Running::start(&world, |_| {});
    let proposal = propose(&running, &world);
    let before = world.blocks();
    world
        .store
        .set_recovery_flag(&RecoveryFlag {
            wiki: world.wiki.id.clone(),
            reason: RecoveryReason::Queued,
            act: None,
            since: 1_800_000_000,
        })
        .unwrap();
    let refused = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(refused.status, 503, "{}", refused.body);
    assert!(refused.body.contains("needs recovery"), "{}", refused.body);
    assert_eq!(world.state_of(&proposal), ProposalState::HeldForReview);
    assert_eq!(world.blocks(), before);
    // Reads are served all the same.
    let page = running.get(&page_path(&proposal), Some(&world.maintainer));
    assert_eq!(page.status, 200);
    running.stop();
}

#[test]
fn the_acceptor_is_built_after_the_start_up_work_has_recovered_a_flagged_wiki() {
    let Some(world) = World::new("recovered-first", false) else {
        return;
    };
    // A wiki flagged before the start, with nothing wrong in its repository: start-up recovers it, and only then
    // is the acceptor there to be asked.
    world
        .store
        .set_recovery_flag(&RecoveryFlag {
            wiki: world.wiki.id.clone(),
            reason: RecoveryReason::Queued,
            act: None,
            since: 1_800_000_000,
        })
        .unwrap();
    let running = Running::start(&world, |_| {});
    assert!(
        running
            .notes
            .iter()
            .any(|l| l.contains("needed recovery") && l.contains("was recovered")),
        "{:?}",
        running.notes
    );
    assert_eq!(world.store.recovery_flag(&world.wiki.id).unwrap(), None);
    let proposal = propose(&running, &world);
    let before = world.blocks();
    let accepted = running.post(
        &accept_path(&proposal),
        Some(&world.maintainer),
        "revision=1",
    );
    assert_eq!(accepted.status, 200, "{}", accepted.body);
    assert_eq!(world.blocks(), before + 1);
    running.stop();
}

/// Does the archive the export test waits for exist yet?
fn archive_exists(world: &World) -> bool {
    world.tmp.0.join("busy.prikk").exists()
}
