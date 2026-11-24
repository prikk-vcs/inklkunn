#![allow(clippy::expect_used, clippy::unwrap_used)]
//! R-3 (0.3.0 readiness; exit criterion 5): the upgrade from 0.2.0, end to end, on a real prikk.
//!
//! **The data directory is made by the 0.2.0 release binary** (the asset `ci.yml` and `release.yml` install and check
//! against a recorded checksum), run as a process: it creates a wiki, two invitations are redeemed over its own
//! server and its own pages (a maintainer and a contributor, who sign in), the contributor proposes two changes, the
//! maintainer accepts one (an accepted change, signed by its key store) and leaves the other open. The server is
//! stopped and the store's schema read: 15. **Then this tree's server opens that same directory**, as `serve` does
//! (the binary's `prepare`, in this process, as U6 does): the store migrates 15 to 19, the wiki serves what 0.2.0
//! wrote, both accounts sign in with the passwords chosen in 0.2.0, the signer opens over the 0.2.0 key store, and
//! the open proposal is accepted into the history.
//!
//! **D-7 against 0.2.0's own output** (RFC 005 part 2 D-7: a page with no claims reads as it did): the pages 0.2.0
//! served, captured from the 0.2.0 process before it stopped, are compared with the pages this tree serves from the
//! same directory, after the one *Why is this here?* line is taken out of the page view and what differs on every
//! page for no reason of the page's (the ports of the two servers, the stylesheet's content-addressed name, a form's
//! per-request token) is masked. The two other places a page differs (a link on the wiki's About, an entry in its
//! health index) are named in `undo_what_0_3_0_adds`, each exactly; and 0.4.0's one line on the About page, the way to the
//! wiki's people, is taken out by `undo_the_people_link`, exactly, once and on that page only. The stylesheet is compared too: this tree's is
//! 0.2.0's, with additions and one changed rule.
//!
//! **0.3.0 on the migrated store** (RC-4): the last steps use the layer the migration (versions 16 - 18) made room
//! for, over data 0.2.0 wrote: a source is attached to a paragraph by proposal, accepted, shown on the claim page, and
//! a comment is written in its thread (version 18's table). A store whose migration lacked a table fails there.
//!
//! The same from **0.3.0** (store schema 18 to 19, RFC 006 D-14, acceptance criterion 14) is `upgrade_0_3_0.rs`, which
//! reuses this file's harness: the one place that starts a release binary as a process.
//!
//! Needs prikk and the 0.2.0 binary. Without prikk the test prints `skipping: prikk not on PATH` (counted by
//! `EXPECTED_SKIPS`); with prikk and without the binary it prints `skipping: inklkunn 0.2.0 not found`, a line the
//! with-prikk run in CI fails on (`WITH_PRIKK_GUARDED`), because CI installs the binary.

use std::fs::File;
use std::process::{Child, Command, Stdio};

use super::knowledge::{
    SURVEY, approve_and_accept, attach, ids_after, main_of, signed_into_history,
};
use super::*;

/// The commit the 0.2.0 release was built from (its `build-info.txt`): what `inklkunn --version` of the real asset
/// says, so that no other binary that calls itself 0.2.0 (this tree's own, before its version moves) is taken for it.
const OLD_COMMIT: &str = "c6c7939c9d9c4ec4eb0982923a767c961c5e7053";

/// The commit the 0.3.0 release was built from (its `build-info.txt`; the release asset's checksum is the one `ci.yml` and
/// `release.yml` record).
pub(super) const COMMIT_0_3_0: &str = "599d4cdc9a4889a332505875307468f6d50fd90d";

/// The first `inklkunn` on `PATH` whose `--version` says it is release `version` built from `commit`, if there is one.
fn find_release(version: &str, commit: &str) -> Option<PathBuf> {
    let wanted = format!("inklkunn {version} (commit {commit};");
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|d| d.join("inklkunn"))
        .filter(|c| c.is_file())
        .find(|c| {
            Command::new(c)
                .arg("--version")
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).starts_with(&wanted))
        })
}

/// The first `inklkunn` on `PATH` that is the 0.2.0 release. With none it prints the line CI fails on.
fn old_binary() -> Option<PathBuf> {
    let Some(found) = find_release("0.2.0", OLD_COMMIT) else {
        println!("skipping: inklkunn 0.2.0 not found");
        return None;
    };
    Some(found)
}

/// The first `inklkunn` on `PATH` that is the 0.3.0 release. With none it prints the line CI fails on.
pub(super) fn binary_0_3_0() -> Option<PathBuf> {
    let Some(found) = find_release("0.3.0", COMMIT_0_3_0) else {
        println!("skipping: inklkunn 0.3.0 not found");
        return None;
    };
    Some(found)
}

/// An earlier release's binary, run as the operator runs it; (status, stdout, stderr).
pub(super) fn old_cli(bin: &Path, world: &World, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(bin)
        .arg("--dev-unconfined")
        .args(args)
        .env_clear()
        .envs(world.env())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

/// An earlier release's server (0.2.0, 0.3.0), a process of its own, its output in a file.
pub(super) struct OldServer {
    child: Option<Child>,
    pub(super) port: u16,
    pub(super) content_port: u16,
    pub(super) log: PathBuf,
}

impl OldServer {
    pub(super) fn start(bin: &Path, world: &World) -> OldServer {
        let (port, content_port) = (free_port(), free_port());
        let log = world.tmp.0.join("old-serve.log");
        let out = File::create(&log).unwrap();
        let mut child = Command::new(bin)
            .arg("--dev-unconfined")
            .arg("serve")
            .env_clear()
            .envs(world.env())
            .env("INKLKUNN_BIND", format!("127.0.0.1:{port}"))
            .env("INKLKUNN_APP_ORIGIN", format!("http://localhost:{port}"))
            .env(
                "INKLKUNN_CONTENT_ORIGIN",
                format!("http://localhost:{content_port}"),
            )
            .stdin(Stdio::null())
            .stdout(out.try_clone().unwrap())
            .stderr(out)
            .spawn()
            .unwrap();
        until("the old release's server to listen", || {
            if let Some(status) = child.try_wait().unwrap() {
                panic!(
                    "the old release's server stopped ({status}):\n{}",
                    std::fs::read_to_string(&log).unwrap_or_default()
                );
            }
            TcpStream::connect(("127.0.0.1", port)).is_ok()
        });
        OldServer {
            child: Some(child),
            port,
            content_port,
            log,
        }
    }

    pub(super) fn browser(&self) -> Browser {
        Browser::new(self.port)
    }

    /// Stop it as an operator does (SIGTERM, to the process this test started) and wait for it to end well; what it
    /// printed.
    pub(super) fn stop(mut self) -> String {
        let mut child = self.child.take().unwrap();
        let pid = rustix::process::Pid::from_child(&child);
        rustix::process::kill_process(pid, rustix::process::Signal::TERM).unwrap();
        until("the old release's server to stop", || {
            child.try_wait().unwrap().is_some()
        });
        let log = std::fs::read_to_string(&self.log).unwrap();
        let status = child.wait().unwrap();
        assert!(
            status.success(),
            "the old release's server ended with {status}:\n{log}"
        );
        log
    }
}

impl Drop for OldServer {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// An account made by the earlier release's invitation page (the link its operator command printed), then signed in.
pub(super) fn redeem(
    server: &OldServer,
    link: &str,
    login: &'static str,
    name: &'static str,
) -> Person {
    let password = password_for(login);
    let mut opener = server.browser();
    let page = opener.get(link);
    assert_eq!(page.status, 200, "{}", page.body);
    let mut form = form_to(&page, link);
    form.set("login", login)
        .set("name", name)
        .set("password", &password);
    let made = opener.submit(&form, None);
    assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
    let mut browser = server.browser();
    sign_in(&mut browser, login, &password, "/");
    Person {
        login,
        name,
        password,
        browser,
    }
}

/// The invitation link the operator command printed (`/-/invite/<secret>`).
pub(super) fn old_invite(bin: &Path, world: &World, role: Role) -> String {
    let (status, out, err) = old_cli(
        bin,
        world,
        &[
            "account",
            "invite",
            "--role",
            role.as_str(),
            "--wiki",
            "handbook",
        ],
    );
    assert_eq!(status, 0, "{err}");
    out.lines()
        .find(|l| l.starts_with("/-/invite/"))
        .unwrap_or_else(|| panic!("no link in {out}"))
        .trim()
        .to_owned()
}

/// The pages of D-7: a page of the wiki (the one every wiki has, changed once), what lists and shows the history
/// beside it, the sections a wiki always has, and a page of the instance. Anonymous: what any reader is shown.
pub(super) fn pages_to_compare(block: &str) -> Vec<String> {
    vec![
        "/handbook".to_owned(),
        HELP.to_owned(),
        format!("/handbook/-/at/{block}/Help"),
        "/handbook/-/history/Help".to_owned(),
        format!("/handbook/-/change/{block}"),
        "/handbook/-/changes".to_owned(),
        "/handbook/-/about".to_owned(),
        "/handbook/-/health".to_owned(),
        "/handbook/-/search".to_owned(),
        "/handbook/-/verification".to_owned(),
        "/handbook/-/discussion/Help".to_owned(),
        "/handbook/-/proposals".to_owned(),
        "/-/wikis".to_owned(),
        "/-/sign-in".to_owned(),
    ]
}

/// What a reader was shown at each path: the status, the body and the `Location` (a redirect's, else none).
pub(super) fn capture(
    mut reader: Browser,
    paths: &[String],
) -> BTreeMap<String, (u16, String, Option<String>)> {
    paths
        .iter()
        .map(|p| {
            let r = reader.get(p);
            let location = r.header("location");
            (p.clone(), (r.status, r.body, location))
        })
        .collect()
}

/// A page as it is compared: the two servers' ports, the stylesheet's name (a hash of its content) and the footer's
/// version masked. The footer says `inklkunn version X.Y.Z`, which differs between the two binaries by definition once this
/// tree is a later release than the one that made the directory; only that version is masked, not the line.
pub(super) fn masked(html: &str, ports: [u16; 2]) -> String {
    let mut out = html
        .replace(&format!("localhost:{}", ports[0]), "localhost:PORT")
        .replace(&format!("localhost:{}", ports[1]), "localhost:CONTENT-PORT")
        .replace("inklkunn version 0.2.0", "inklkunn version VERSION")
        .replace(
            &format!("inklkunn version {}", env!("CARGO_PKG_VERSION")),
            "inklkunn version VERSION",
        )
        // Three differences that are this tree's later work and are named, not hidden: Japanese is no longer labelled a
        // preview in the language switcher and on the About page (the owner signed the catalog off on 2026-11-19, M60), and the
        // About page's accessibility statement gained the sentence saying so.
        .replace("日本語</span> (preview)</button>", "日本語</span></button>")
        .replace(
            "<p>This translation is a preview. Fluent readers have not checked it yet, so some wording may be wrong.</p>\n",
            "",
        )
        .replace(
            "<p>The Japanese translation was reviewed by the owner of the inklkunn project on 2026-11-19 (the messages of 0.3.0); messages added later are reviewed at each release.</p>\n",
            "",
        );
    let mut from = 0;
    while let Some(at) = out
        .get(from..)
        .and_then(|r| r.find("name=\"csrf\" value=\""))
    {
        let start = from + at + "name=\"csrf\" value=\"".len();
        let end = start + out.get(start..).unwrap_or_default().find('"').unwrap_or(0);
        out.replace_range(start..end, "CSRF");
        from = start + "CSRF".len();
    }
    let mut from = 0;
    while let Some(at) = out.get(from..).and_then(|r| r.find("/-/assets/site-")) {
        let start = from + at + "/-/assets/site-".len();
        let end = start
            + out
                .get(start..)
                .unwrap_or_default()
                .find(".css")
                .unwrap_or(0);
        out.replace_range(start..end, "HASH");
        from = start + "HASH".len();
    }
    out
}

/// What 0.2.0 said on the wiki's About, in the line that now also names the views.
const ABOUT_0_2_0: &str = "<p>The view shown when none is named is main.</p>";

/// The one rule of 0.2.0's stylesheet this tree changes, and what it says now.
const CODE_0_2_0: &str = r#"code { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 0.95em; }"#;
const CODE_NOW: &str = r#"code { font-family: ui-monospace, "SFMono-Regular", Menlo, Consolas, monospace; font-size: 0.95em; overflow-wrap: anywhere; }"#;

/// This tree's page with what 0.3.0 adds taken out, and whether anything was: on the page view, the one *Why is this
/// here?* paragraph; on the About page, the link to the views (the line is 0.2.0's again); in the health index, the
/// entry for *Sources to review*. Each is looked for exactly: one such line where it belongs and none anywhere else,
/// so a second link, or one on another page, is not taken out and is found as a difference.
fn undo_what_0_3_0_adds(path: &str, html: &str) -> (String, bool) {
    let marks = [
        "/-/why/",
        "href=\"/handbook/-/views\"",
        "/health/sources-to-review",
    ];
    let lines: Vec<&str> = html.split_inclusive('\n').collect();
    let (mut out, mut touched) = (String::new(), false);
    for line in &lines {
        let hit = marks.iter().position(|m| line.contains(m));
        let wanted = match path {
            HELP => Some(0),
            "/handbook/-/about" => Some(1),
            "/handbook/-/health" => Some(2),
            _ => None,
        };
        match (hit, wanted) {
            (Some(h), Some(w)) if h == w => {
                assert!(!touched, "{path}: more than one line of what 0.3.0 adds");
                touched = true;
                if h == 1 {
                    assert_eq!(
                        line.trim_end(),
                        "<p>The view shown when none is named is main. <a href=\"/handbook/-/views\">All views of this wiki</a></p>",
                        "{path}"
                    );
                    out.push_str(ABOUT_0_2_0);
                    out.push('\n');
                }
                if h == 0 {
                    assert_eq!(
                        line.trim_end(),
                        "<p><a href=\"/handbook/-/why/Help\">Why is this here?</a></p>",
                        "{path}"
                    );
                }
            }
            (Some(h), _) => panic!(
                "{path}: a line of what 0.3.0 adds ({:?}) where it is not expected: {line}",
                marks.get(h)
            ),
            (None, _) => out.push_str(line),
        }
    }
    (out, touched)
}

/// The one line this tree's *About* page adds beside 0.3.0's: the way to the wiki's people (0.4.0's G-T2).
const PEOPLE_LINK: &str = "<p><a href=\"/handbook/-/people\">People and roles of this wiki</a></p>";

/// The second line this tree's *About* page adds: the way to the rules of the wiki (0.4.0's G-T7).
const POLICY_LINK: &str = "<p><a href=\"/handbook/-/policy\">The rules of this wiki</a></p>";

/// The line a change page adds when no record says what an acceptance was made under (0.4.0's G-1).
const UNRECORDED_FACT: &str =
    "<li>The role and the rules this was decided under were not recorded.</li>";

/// This tree's page with the two ways 0.4.0 adds taken out, **exactly**: the way to the people page and the way to the rules, each
/// one line, on the *About* page and nowhere else, so that a second one, or one on another page, is not taken out and is found as
/// a difference. **And, on a change page, the acceptor's line is 0.3.0's again** (G-1: the role and the rules of the record).
pub(super) fn undo_the_people_link(path: &str, html: &str) -> String {
    let mut out = String::new();
    let (mut people, mut rules) = (0, 0);
    let on_a_change = path.starts_with("/handbook/-/change/");
    for line in html.split_inclusive('\n') {
        match line.trim_end() {
            PEOPLE_LINK => people += 1,
            POLICY_LINK => rules += 1,
            // 0.4.0's G-1: the change page's *accepted by* line names the role and the rules of the acceptance record (an
            // acceptance of 0.2.0 or 0.3.0 has the role and the default rules), and says so when there is no record. Taken out
            // exactly, on a change page and nowhere else: the line is 0.3.0's again.
            fact if on_a_change
                && fact.starts_with("<li>Accepted by ")
                && fact.ends_with("), under the default rules.</li>") =>
            {
                let name = fact
                    .strip_prefix("<li>Accepted by ")
                    .and_then(|rest| rest.rsplit_once(" ("))
                    .map(|(name, _)| name)
                    .unwrap();
                out.push_str(&format!("<li>Accepted by {name}</li>\n"));
            }
            UNRECORDED_FACT if on_a_change => {}
            _ => out.push_str(line),
        }
    }
    let expected = usize::from(path == "/handbook/-/about");
    assert_eq!(
        (people, rules),
        (expected, expected),
        "{path}: the lines that lead to the people page and to the rules are on the About page, once each, and nowhere else"
    );
    out
}

/// The first line at which two texts differ, with a few lines around it, for a failure to say where.
pub(super) fn first_difference(old: &str, new: &str) -> String {
    let (o, n): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let at = (0..o.len().max(n.len()))
        .find(|i| o.get(*i) != n.get(*i))
        .unwrap_or(0);
    format!(
        "line {}:\n  0.2.0: {:?}\n  now:   {:?}",
        at + 1,
        o.get(at),
        n.get(at)
    )
}

/// The stylesheet's path in a page.
pub(super) fn stylesheet_of(html: &str) -> String {
    let (_, after) = html.split_once("href=\"/-/assets/site-").unwrap();
    let (name, _) = after.split_once('"').unwrap();
    format!("/-/assets/site-{name}")
}

/// The schema version a store file records.
pub(super) fn schema_of(store: &Path) -> i64 {
    let db =
        rusqlite::Connection::open_with_flags(store, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    db.query_row(
        "SELECT CAST(value AS INTEGER) FROM schema_meta WHERE key = 'schema_version'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

/// Wait for the check after a seal to be recorded, so that stopping the server does not interrupt it.
pub(super) fn settled(reader: &mut Browser, since: i64) {
    until("the check after the change", || {
        let page = reader.get("/handbook/-/verification");
        page.body
            .split_once("Last checked: ")
            .and_then(|(_, after)| after.split_once('.'))
            .is_some_and(|(at, _)| at >= utc_minutes(since).as_str())
    });
}

/// A deployment that has no wiki yet, with the settings an operator keeps in both halves of an upgrade (the earlier release
/// makes the data directory under them, this tree opens it): the `data` directory, the operator's secret, the seed directory.
/// Its store and wiki are a placeholder's until the directory is opened by this tree.
pub(super) fn bare_world(prikk: &Path, tag: &str) -> World {
    let tmp = dir(&std::env::temp_dir(), tag);
    let shm = dir(Path::new("/dev/shm"), tag);
    let data_dir = tmp.0.join("data");
    let secret_file = tmp.0.join("operator.secret");
    std::fs::write(&secret_file, SECRET).unwrap();
    std::fs::set_permissions(&secret_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    let settings = Settings {
        key_dir: data_dir.join("keys"),
        data_dir,
        secret_file,
        seed_dir: shm.0.clone(),
        prikk: prikk.as_os_str().to_owned(),
        dev_unconfined: true,
    };
    let placeholder = SqliteStore::open_in_memory().unwrap();
    let wiki = placeholder
        .create(inklkunn_store::NewWiki::new(
            Slug::new("placeholder").unwrap(),
            inklkunn_store::DisplayName::new("Placeholder").unwrap(),
        ))
        .unwrap();
    World {
        tmp,
        _shm: shm,
        store: Arc::new(placeholder),
        wiki,
        settings,
    }
}

#[test]
fn a_directory_made_by_the_0_2_0_binary_is_opened_migrated_and_used_by_this_tree() {
    let Some(prikk) = crate::keys::tests::prikk_probes::prikk_first() else {
        return;
    };
    let Some(old) = old_binary() else {
        return;
    };

    // ---- the directory, as the operator of 0.2.0 makes it: the settings are the same in both halves
    let mut world = bare_world(&prikk, "upgrade");

    // ---- 0.2.0 makes the wiki, the two invitations, the accounts, the accepted change and the open proposal
    let (status, created, err) = old_cli(
        &old,
        &world,
        &[
            "wiki",
            "create",
            "handbook",
            "--name",
            "Handbook",
            "--visibility",
            "public",
        ],
    );
    assert_eq!(status, 0, "{err}");
    assert!(created.contains("created wiki handbook"), "{created}");
    let maintainer_link = old_invite(&old, &world, Role::Maintainer);
    let contributor_link = old_invite(&old, &world, Role::Contributor);

    let started = now();
    let server = OldServer::start(&old, &world);
    assert!(
        std::fs::read_to_string(&server.log)
            .unwrap()
            .contains("accepting proposals: the signer is open"),
        "the 0.2.0 server did not open its signer"
    );
    let mut maya = redeem(&server, &maintainer_link, "maya", "Maya Maintainer");
    let mut noor = redeem(&server, &contributor_link, "noor", "Noor Contributor");
    let p1 = propose_edit(&mut noor, "Help", "Say which brackets", clarify_reading);
    let p2 = propose_edit(&mut noor, "Help", "Say that it says so", say_so);
    // The proposer does not decide her own proposal: the maintainer approves and accepts the first.
    let page = maya.browser.get(&proposal_page(&p1));
    let mut form = form_to(&page, &format!("{}/decide", proposal_page(&p1)));
    form.set("body", "Looks right to me.");
    let decided = maya.browser.submit(&form, Some(("op", "approve")));
    assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
    let page = maya.browser.get(&proposal_page(&p1));
    let accepted = maya.browser.submit(
        &form_to(&page, &format!("{}/accept", proposal_page(&p1))),
        None,
    );
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert!(
        accepted.body.contains("Signed into the history"),
        "{}",
        accepted.body
    );
    let mut reader = server.browser();
    settled(&mut reader, started);
    let help = reader.get(HELP);
    assert!(help.body.contains(READING_AFTER), "{}", help.body);
    assert!(!help.body.contains(TRUST_AFTER), "{}", help.body);

    // 0.2.0's own output, for D-7: the pages and the stylesheet, as its process served them.
    let block = {
        let page = reader.get("/handbook/-/history/Help");
        let (_, after) = page.body.split_once("/handbook/-/change/").unwrap();
        after.split_once('"').unwrap().0.to_owned()
    };
    let paths = pages_to_compare(&block);
    let before = capture(server.browser(), &paths);
    assert_eq!(
        before.get("/handbook").map(|r| r.0),
        Some(303),
        "0.2.0 answers /handbook with a redirect"
    );
    let old_css_path = stylesheet_of(&before.get(HELP).unwrap().1);
    let old_css = reader.get(&old_css_path).body;
    assert!(!old_css.is_empty());
    let old_ports = [server.port, server.content_port];
    let old_log = server.stop();
    assert_eq!(
        schema_of(&world.settings.store_path()),
        15,
        "0.2.0 writes store schema 15"
    );

    // ---- this tree opens the same directory, as `serve` does
    let running = Running::start(&world);
    assert!(
        running
            .banner
            .iter()
            .any(|l| l.contains("accepting proposals: the signer is open")),
        "the signer did not open over the key store 0.2.0 made: {:?}",
        running.banner
    );
    assert_eq!(
        schema_of(&world.settings.store_path()),
        19,
        "the store migrated from 15 to this tree's schema"
    );
    world.store = Arc::new(SqliteStore::open(&world.settings.store_path()).unwrap());
    world.wiki = world
        .store
        .by_slug(&Slug::new("handbook").unwrap())
        .unwrap();
    let first_blocks = world.blocks();
    assert_eq!(
        first_blocks.len(),
        2,
        "the wiki's first block and the change 0.2.0 accepted"
    );
    assert_eq!(first_blocks.first(), Some(&block));

    // ---- the wiki serves what 0.2.0 wrote
    let mut anonymous = running.browser();
    let help = page_text(&mut anonymous, HELP);
    assert!(help.contains(READING_AFTER), "{help}");
    assert!(!help.contains(TRUST_AFTER), "{help}");
    let history = page_text(&mut anonymous, "/handbook/-/history/Help");
    assert!(history.contains("Say which brackets"), "{history}");
    assert!(history.contains("Noor Contributor"), "{history}");
    assert!(history.contains("Maya Maintainer"), "{history}");

    // ---- D-7: a page with no claims renders as 0.2.0 rendered it, except the one link
    let after = capture(running.browser(), &paths);
    let new_ports = [running.port, running.content_port];
    let mut changed = Vec::new();
    for path in &paths {
        let (old_status, old_html, old_location) = before.get(path).unwrap();
        let (new_status, new_html, new_location) = after.get(path).unwrap();
        assert_eq!(old_status, new_status, "{path}: the status differs");
        // The 303 of /handbook says where it goes in `Location`; that is compared too (with the ports masked).
        assert_eq!(
            old_location.as_deref().map(|l| masked(l, old_ports)),
            new_location.as_deref().map(|l| masked(l, new_ports)),
            "{path}: the Location differs"
        );
        let new_html = &undo_the_people_link(path, new_html);
        let (undone, touched) = undo_what_0_3_0_adds(path, new_html);
        if touched {
            changed.push(path.as_str());
        }
        let (o, n) = (masked(old_html, old_ports), masked(&undone, new_ports));
        assert!(
            o == n,
            "{path}: renders differently from 0.2.0 ({} bytes to {}): {}",
            o.len(),
            n.len(),
            first_difference(&o, &n)
        );
    }
    // Where this tree's pages are not byte for byte 0.2.0's (after the masks): the page view of Help (the one link),
    // and two of the wiki's own sections (a link to the views, an entry in the health index). No other path of the
    // sample differs, and none of the pages of history does.
    assert_eq!(
        changed,
        [HELP, "/handbook/-/about", "/handbook/-/health"],
        "the pages that differ from 0.2.0's"
    );
    // The stylesheet: every line of 0.2.0's is in this tree's, in its order, with additions between them, and with one
    // rule changed (a long `code` may now break anywhere).
    let new_css = anonymous
        .get(&stylesheet_of(&after.get(HELP).unwrap().1))
        .body;
    assert!(
        old_css.lines().any(|l| l == CODE_0_2_0),
        "0.2.0's `code` rule is not the one the check expects"
    );
    let mut rest = new_css.lines();
    for want in old_css
        .lines()
        .map(|l| if l == CODE_0_2_0 { CODE_NOW } else { l })
    {
        assert!(
            rest.any(|l| l == want),
            "a line of 0.2.0's stylesheet is gone or changed (or out of its place): {want}"
        );
    }

    // ---- both accounts sign in with the passwords chosen in 0.2.0
    let mut maya_now = running.browser();
    sign_in(&mut maya_now, "maya", &password_for("maya"), "/");
    let mut noor_now = running.browser();
    sign_in(&mut noor_now, "noor", &password_for("noor"), "/");
    let mine = page_text(&mut noor_now, HELP);
    assert!(mine.contains("Noor Contributor"), "{mine}");
    let hers = page_text(&mut maya_now, HELP);
    assert!(hers.contains("Maya Maintainer"), "{hers}");
    // The accounts are the ones in the store 0.2.0 wrote (the migration kept them, with their grants).
    let _ = (world.account_of("maya"), world.account_of("noor"));

    // ---- the open proposal is there, and the maintainer accepts it into the history through the signer
    let page = noor_now.get(&proposal_page(&p2));
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(page.body.contains("Say that it says so"), "{}", page.body);
    assert!(
        page.body.contains("Held for review") || page.body.contains("held for review"),
        "{}",
        page.body
    );
    let accept_action = format!("{}/accept", proposal_page(&p2));
    assert!(!has_form_to(&page, &accept_action), "{}", page.body);
    let page = maya_now.get(&proposal_page(&p2));
    let accepted = maya_now.submit(&form_to(&page, &accept_action), None);
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert!(
        accepted.body.contains("Signed into the history"),
        "{}",
        accepted.body
    );
    let after_accept = world.blocks();
    assert_eq!(
        after_accept.len(),
        first_blocks.len() + 1,
        "prikk log: exactly one new block"
    );
    assert_eq!(after_accept.get(1), first_blocks.first());
    let help = page_text(&mut anonymous, HELP);
    assert!(
        help.contains(READING_AFTER),
        "0.2.0's change is kept: {help}"
    );
    assert!(
        help.contains(TRUST_AFTER),
        "the open proposal joined it: {help}"
    );

    // ---- 0.3.0's knowledge layer over the migrated store (V16 - V18 on data 0.2.0 made): a source is attached to a
    //      paragraph by proposal, accepted into the history, read by anyone, and a comment is written in its thread
    let mut maya_p = Person {
        login: "maya",
        name: "Maya Maintainer",
        password: password_for("maya"),
        browser: maya_now,
    };
    let mut noor_p = Person {
        login: "noor",
        name: "Noor Contributor",
        password: password_for("noor"),
        browser: noor_now,
    };
    let before_source = world.blocks();
    let attached = attach(
        &mut noor_p,
        None,
        "supports",
        &SURVEY,
        "Cite the link syntax",
    );
    signed_into_history(&approve_and_accept(&mut maya_p, &attached));
    assert_eq!(
        world.blocks().len(),
        before_source.len() + 1,
        "prikk log: the attached source is one new block"
    );
    let help = anonymous.get(HELP);
    assert_eq!(
        help.body.matches("class=\"claim-link\"").count(),
        1,
        "the paragraph the source was attached to now carries the claim: {}",
        help.body
    );
    let claim = ids_after(&help.body, "/handbook/-/claim/")
        .into_iter()
        .next()
        .expect("the claim link names the claim");
    let claim_page = format!("/handbook/-/claim/{claim}");
    let seen = anonymous.get(&claim_page);
    assert_eq!(seen.status, 200, "{}", seen.body);
    assert!(
        main_of(&seen).contains("A survey of wiki link syntax")
            && main_of(&seen).contains("supports, directly"),
        "the claim page, read over the migrated store, lists the source: {}",
        main_of(&seen)
    );
    let page = noor_p.browser.get(&claim_page);
    let discuss = format!("{claim_page}/discuss");
    let mut form = form_to(&page, &discuss);
    form.set("body", "Where in chapter 2 is this said?");
    let said = noor_p.browser.submit(&form, None);
    assert_eq!(said.status, 303, "{}\n{}", said.head, said.body);
    let seen = anonymous.get(&claim_page);
    assert!(
        main_of(&seen).contains("Where in chapter 2 is this said?")
            && main_of(&seen).contains("Noor Contributor"),
        "the comment is in the claim's thread (version 18's table): {}",
        main_of(&seen)
    );

    // ---- nothing secret was printed by either half
    let printed = [
        old_log,
        running.notes.join("\n"),
        running.banner.join("\n"),
        running.log.lock().unwrap().join("\n"),
    ]
    .join("\n");
    let stderr = running.stop();
    for secret in [
        SECRET.to_owned(),
        password_for("maya"),
        password_for("noor"),
    ] {
        assert!(!printed.contains(&secret), "a secret reached a log");
        assert!(!stderr.contains(&secret), "a secret reached stderr");
    }
    let _ = &world.tmp;
}
