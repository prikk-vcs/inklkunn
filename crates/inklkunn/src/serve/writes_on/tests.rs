#![allow(clippy::expect_used, clippy::unwrap_used)]
//! U6, end to end: the real server (the binary's `prepare`, its real `Identify`, its real acceptor) over a real
//! prikk, a real key store and a real SQLite store, behind a real socket. Nothing here stands in for a
//! person or a credential: accounts are made by the operator's invitation machinery and opened through the
//! invitation page; sign-in goes through `/-/sign-in` with a cookie jar that obeys the `__Host-` rules and
//! forms read off the pages (the hidden `csrf` field, the double-submit cookie, the same-site headers, a
//! text area sent with CR LF as a browser sends it). Every test needs prikk and prints
//! `skipping: prikk not on PATH` without it.

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use inklkunn_core::IdentityAuthority;
use inklkunn_prikk::{Admission, PrikkCli};
use inklkunn_store::{AccountStore, LoginName, Role, Scope, Slug, WikiRecord, WikiStore};
use inklkunn_web::serve::Shutdown;

use super::super::*;
use crate::cli;

/// What the operator's secret file holds in these tests.
const SECRET: &str = "u6-operator-secret-5d1b8e3a7c9f2046-do-not-print";

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn dir(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-u6-{}-{tag}", std::process::id()));
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

fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(60);
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn now() -> i64 {
    inklkunn_store::record::now_secs()
}

/// `YYYY-MM-DD HH:MM UTC`, the way the pages show a moment.
fn utc_minutes(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3600,
        (rem % 3600) / 60
    )
}

// ------------------------------------------------------------------------------------ the deployment

/// A deployment with one public wiki, made by the command line with the real signer. It has no accounts: the
/// people come by invitation.
struct World {
    tmp: Dir,
    _shm: Dir,
    settings: Settings,
    store: Arc<SqliteStore>,
    wiki: WikiRecord,
}

impl World {
    fn new(tag: &str) -> Option<World> {
        let real = real_prikk()?;
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
            prikk: real.into_os_string(),
            dev_unconfined: true,
        };
        let placeholder = SqliteStore::open_in_memory().unwrap();
        let wiki = placeholder
            .create(inklkunn_store::NewWiki::new(
                Slug::new("placeholder").unwrap(),
                inklkunn_store::DisplayName::new("Placeholder").unwrap(),
            ))
            .unwrap();
        let mut world = World {
            tmp,
            _shm: shm,
            store: Arc::new(placeholder),
            wiki,
            settings,
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
        // The creation is verified; age that, so that the check after a change is not turned away as too recent
        // and a fresh one is told from the old one by its time.
        world
            .store
            .set_verification(
                &world.wiki.id,
                inklkunn_store::Verification {
                    state: inklkunn_store::VerificationState::Verified,
                    at: Some(1_000),
                },
            )
            .unwrap();
        Some(world)
    }

    fn env(&self) -> HashMap<&'static str, OsString> {
        let s = &self.settings;
        HashMap::from([
            ("INKLKUNN_DATA_DIR", s.data_dir.clone().into_os_string()),
            (
                "INKLKUNN_SECRET_FILE",
                s.secret_file.clone().into_os_string(),
            ),
            ("INKLKUNN_KEY_DIR", s.key_dir.clone().into_os_string()),
            ("INKLKUNN_SEED_DIR", s.seed_dir.clone().into_os_string()),
            ("INKLKUNN_PRIKK", s.prikk.clone()),
        ])
    }

    /// `inklkunn --dev-unconfined <args>` in this process; (status, stdout, stderr).
    fn cli(&self, args: &[&str]) -> (i32, String, String) {
        let env = self.env();
        let mut argv = vec!["--dev-unconfined".to_owned()];
        argv.extend(args.iter().map(|a| (*a).to_owned()));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = cli::run(&argv, &|k| env.get(k).cloned(), now(), &mut out, &mut err);
        (
            status,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    /// The operator's `account invite --role <role> --wiki handbook`: the link's path (`/-/invite/<secret>`).
    fn invite(&self, role: Role) -> String {
        let env = self.env();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let status = crate::account::run_with(
            &["invite", "--role", role.as_str(), "--wiki", "handbook"],
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
        let link = out
            .lines()
            .find(|l| l.starts_with("/-/invite/"))
            .unwrap_or_else(|| panic!("no link in {out}"));
        link.trim().to_owned()
    }

    fn storage(&self) -> PrikkCli {
        PrikkCli::open(
            &self.settings.prikk,
            inklkunn_prikk::sandbox::Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(500)),
        )
        .unwrap()
    }

    /// The default view's blocks as prikk's `log` gives them, newest first.
    fn blocks(&self) -> Vec<String> {
        use inklkunn_prikk::{Point, Storage};
        let layout = RootLayout::new(self.settings.wikis_root());
        self.storage()
            .history(
                &layout.repo_dir(&self.wiki.id),
                &Point::View(self.wiki.default_view.clone()),
                None,
            )
            .unwrap()
            .blocks
            .into_iter()
            .map(|b| b.block_id)
            .collect()
    }

    fn account_of(&self, login: &str) -> inklkunn_store::AccountId {
        self.store
            .account_by_login(&LoginName::parse(login).unwrap())
            .unwrap()
            .unwrap_or_else(|| panic!("no account {login}"))
            .id
    }

    fn verification(&self) -> inklkunn_store::Verification {
        <dyn WikiStore>::get(&*self.store, &self.wiki.id)
            .unwrap()
            .verification
    }
}

/// The server, started as `serve` starts it: the binary's `prepare` with its own `Identify` (nothing replaced).
struct Running {
    port: u16,
    content_port: u16,
    stop: Option<inklkunn_web::serve::StopHandle>,
    join: Option<std::thread::JoinHandle<(Result<(), String>, String)>>,
    log: Arc<Mutex<Vec<String>>>,
    notes: Vec<String>,
    banner: Vec<String>,
    services: Arc<inklkunn_web::Services>,
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

impl Running {
    fn start(world: &World) -> Running {
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
        let prepared = prepare_with(
            &world.settings,
            &web,
            IdentityAuthority::for_tests(),
            Arc::new(move |line| sink.lock().unwrap().push(line.to_owned())),
        )
        .unwrap();
        let services = prepared._services.clone();
        let (notes, banner) = (prepared.notes.clone(), prepared.banner.clone());
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
            content_port: content,
            stop: Some(stop),
            join: Some(join),
            log,
            notes,
            banner,
            services,
        }
    }

    fn browser(&self) -> Browser {
        Browser::new(self.port)
    }

    fn stop(mut self) -> String {
        self.stop.take().unwrap().stop();
        let (result, err) = self.join.take().unwrap().join().unwrap();
        assert!(result.is_ok(), "{result:?}");
        err
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

// ------------------------------------------------------------------------------------ the wire

/// What an HTTP answer came to.
#[derive(Debug, Clone)]
struct Reply {
    status: u16,
    head: String,
    body: String,
}

impl Reply {
    fn headers(&self, name: &str) -> Vec<String> {
        self.head
            .lines()
            .skip(1)
            .filter_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.trim()
                    .eq_ignore_ascii_case(name)
                    .then(|| v.trim().to_owned())
            })
            .collect()
    }

    fn header(&self, name: &str) -> Option<String> {
        self.headers(name).into_iter().next()
    }

    fn location(&self) -> String {
        self.header("location")
            .unwrap_or_else(|| panic!("no Location in {}\n{}", self.head, self.body))
    }
}

fn dechunk(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(at) = rest.windows(2).position(|w| w == b"\r\n") {
        let size = std::str::from_utf8(rest.get(..at).unwrap_or_default())
            .ok()
            .and_then(|s| usize::from_str_radix(s.split(';').next().unwrap_or("").trim(), 16).ok())
            .unwrap_or(0);
        let start = at + 2;
        if size == 0 {
            break;
        }
        let Some(chunk) = rest.get(start..start + size) else {
            break;
        };
        out.extend_from_slice(chunk);
        rest = rest.get(start + size + 2..).unwrap_or_default();
    }
    out
}

/// One request over a socket: `headers` are written as given (the caller is the browser, or the script).
fn wire(port: u16, method: &str, path: &str, headers: &[(String, String)], body: &[u8]) -> Reply {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(120))).unwrap();
    let mut head = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        head.push_str(&format!("Host: localhost:{port}\r\n"));
    }
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    if method == "POST" {
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    s.write_all(head.as_bytes()).unwrap();
    s.write_all(body).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap_or(raw.len());
    let head = String::from_utf8_lossy(raw.get(..split).unwrap_or_default()).into_owned();
    let mut payload = raw.get(split + 4..).unwrap_or_default().to_vec();
    if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        payload = dechunk(&payload);
    }
    Reply {
        status: head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        head,
        body: String::from_utf8_lossy(&payload).into_owned(),
    }
}

/// `application/x-www-form-urlencoded` percent-encoding.
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn unescape(text: &str) -> String {
    text.replace("&#60;", "<")
        .replace("&#62;", ">")
        .replace("&#34;", "\"")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#38;", "&")
        .replace("&amp;", "&")
}

// ------------------------------------------------------------------------------------ forms

#[derive(Debug, Clone)]
struct Field {
    name: String,
    value: String,
    textarea: bool,
}

/// A form as a browser holds it: where it posts, what it would send, and the named buttons it has.
#[derive(Debug, Clone)]
struct Form {
    action: String,
    fields: Vec<Field>,
    buttons: Vec<(String, String)>,
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let (_, after) = tag.split_once(&format!(" {name}=\""))?;
    let (value, _) = after.split_once('"')?;
    Some(unescape(value))
}

impl Form {
    fn set(&mut self, name: &str, value: &str) -> &mut Self {
        let field = self
            .fields
            .iter_mut()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("the form {} has no field {name}", self.action));
        field.value = value.to_owned();
        self
    }

    fn get(&self, name: &str) -> &str {
        &self
            .fields
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("the form {} has no field {name}", self.action))
            .value
    }

    fn without(&mut self, name: &str) -> &mut Self {
        self.fields.retain(|f| f.name != name);
        self
    }

    /// What the browser sends: every field, a text area with its line breaks as CR LF, and the pressed button.
    fn body(&self, press: Option<(&str, &str)>) -> Vec<u8> {
        let mut pairs: Vec<String> = self
            .fields
            .iter()
            .map(|f| {
                let value = if f.textarea {
                    f.value.replace("\r\n", "\n").replace('\n', "\r\n")
                } else {
                    f.value.clone()
                };
                format!("{}={}", encode(&f.name), encode(&value))
            })
            .collect();
        if let Some((name, value)) = press {
            assert!(
                self.buttons.iter().any(|b| b.0 == name && b.1 == value),
                "the form {} has no button {name}={value}: {:?}",
                self.action,
                self.buttons
            );
            pairs.push(format!("{}={}", encode(name), encode(value)));
        }
        pairs.join("&").into_bytes()
    }
}

fn forms_of(html: &str) -> Vec<Form> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some((_, after)) = rest.split_once("<form") {
        let Some((head, after_head)) = after.split_once('>') else {
            break;
        };
        let Some((inner, remainder)) = after_head.split_once("</form>") else {
            break;
        };
        rest = remainder;
        let mut form = Form {
            action: attr(head, "action").unwrap_or_default(),
            fields: Vec::new(),
            buttons: Vec::new(),
        };
        for chunk in inner.split("<input").skip(1) {
            let Some((tag, _)) = chunk.split_once('>') else {
                continue;
            };
            let tag = format!(" {tag}");
            let kind = attr(&tag, "type").unwrap_or_else(|| "text".to_owned());
            if !matches!(kind.as_str(), "hidden" | "text" | "password") {
                continue;
            }
            if let Some(name) = attr(&tag, "name") {
                form.fields.push(Field {
                    name,
                    value: attr(&tag, "value").unwrap_or_default(),
                    textarea: false,
                });
            }
        }
        for chunk in inner.split("<textarea").skip(1) {
            let Some((tag, after_tag)) = chunk.split_once('>') else {
                continue;
            };
            let content = after_tag.split_once("</textarea>").map_or("", |c| c.0);
            // A browser drops the one line break that follows the opening tag.
            let content = content.strip_prefix('\n').unwrap_or(content);
            if let Some(name) = attr(&format!(" {tag}"), "name") {
                form.fields.push(Field {
                    name,
                    value: unescape(content),
                    textarea: true,
                });
            }
        }
        for chunk in inner.split("<button").skip(1) {
            let Some((tag, _)) = chunk.split_once('>') else {
                continue;
            };
            let tag = format!(" {tag}");
            if let Some(name) = attr(&tag, "name") {
                form.buttons
                    .push((name, attr(&tag, "value").unwrap_or_default()));
            }
        }
        out.push(form);
    }
    out
}

/// The form that posts to `action`, from a page.
fn form_to(page: &Reply, action: &str) -> Form {
    forms_of(&page.body)
        .into_iter()
        .find(|f| f.action == action)
        .unwrap_or_else(|| {
            panic!(
                "no form to {action} on the page ({}):\n{}",
                page.status, page.body
            )
        })
}

fn has_form_to(page: &Reply, action: &str) -> bool {
    forms_of(&page.body).iter().any(|f| f.action == action)
}

// ------------------------------------------------------------------------------------ a browser

/// A browser: a cookie jar that keeps what a browser keeps (a `__Host-` cookie only with `Secure`, `Path=/` and
/// no `Domain`), and the headers a browser sends on a navigation or a form post from the page's own site.
struct Browser {
    port: u16,
    jar: BTreeMap<String, String>,
    /// Every page body it was ever shown, and every session cookie value it was ever given: what a search for
    /// a leaked secret looks through.
    bodies: Vec<String>,
    values: Vec<String>,
}

impl Browser {
    fn new(port: u16) -> Browser {
        Browser {
            port,
            jar: BTreeMap::new(),
            bodies: Vec::new(),
            values: Vec::new(),
        }
    }

    fn cookie(&self, name: &str) -> Option<&String> {
        self.jar.get(name)
    }

    fn visit(
        &mut self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
        overrides: &[(&str, &str)],
    ) -> Reply {
        let mut headers: Vec<(String, String)> = vec![
            ("Sec-Fetch-Site".into(), "same-origin".into()),
            ("Sec-Fetch-Mode".into(), "navigate".into()),
            ("Sec-Fetch-Dest".into(), "document".into()),
            ("Accept".into(), "text/html,application/xhtml+xml".into()),
            ("Accept-Language".into(), "en".into()),
        ];
        if body.is_some() {
            headers.push(("Origin".into(), format!("http://localhost:{}", self.port)));
            headers.push((
                "Content-Type".into(),
                "application/x-www-form-urlencoded".into(),
            ));
        }
        if !self.jar.is_empty() {
            let line: Vec<String> = self.jar.iter().map(|(k, v)| format!("{k}={v}")).collect();
            headers.push(("Cookie".into(), line.join("; ")));
        }
        for (k, v) in overrides {
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case(k));
            headers.push(((*k).to_owned(), (*v).to_owned()));
        }
        let reply = wire(
            self.port,
            method,
            path,
            &headers,
            body.as_deref().unwrap_or_default(),
        );
        for line in reply.headers("set-cookie") {
            self.keep(&line);
        }
        self.bodies.push(reply.body.clone());
        reply
    }

    /// What a browser does with a `Set-Cookie`.
    fn keep(&mut self, line: &str) {
        let mut parts = line.split(';').map(str::trim);
        let Some((name, value)) = parts.next().and_then(|p| p.split_once('=')) else {
            return;
        };
        // (The double-submit value is meant to be in the form it protects; the session's is in no page.)
        if name == "__Host-inklkunn-session" {
            self.values.push(value.to_owned());
        }
        let attrs: Vec<String> = parts.map(str::to_ascii_lowercase).collect();
        if name.starts_with("__Host-")
            && !(attrs.iter().any(|a| a == "secure")
                && attrs.iter().any(|a| a == "path=/")
                && !attrs.iter().any(|a| a.starts_with("domain=")))
        {
            // A browser refuses a `__Host-` cookie that does not meet the prefix's rules.
            return;
        }
        if attrs.iter().any(|a| a == "max-age=0") || value.is_empty() {
            self.jar.remove(name);
        } else {
            self.jar.insert(name.to_owned(), value.to_owned());
        }
    }

    fn get(&mut self, path: &str) -> Reply {
        self.visit("GET", path, None, &[])
    }

    fn submit(&mut self, form: &Form, press: Option<(&str, &str)>) -> Reply {
        self.visit("POST", &form.action, Some(form.body(press)), &[])
    }

    /// The session's CSRF token, off a page of the account: what every form of a signed-in person carries.
    fn csrf(&mut self) -> String {
        let page = self.get("/-/account");
        assert_eq!(page.status, 200, "{}", page.body);
        forms_of(&page.body)
            .iter()
            .find_map(|f| f.fields.iter().find(|x| x.name == "csrf"))
            .unwrap_or_else(|| panic!("no csrf field on the account page: {}", page.body))
            .value
            .clone()
    }
}

// ------------------------------------------------------------------------------------ people

struct Person {
    login: &'static str,
    name: &'static str,
    password: String,
    browser: Browser,
}

fn password_for(login: &str) -> String {
    format!("correct-horse-{login}-battery-staple")
}

/// Make the account by the operator's invitation and the invitation page, then sign in as a browser does.
fn join(
    world: &World,
    running: &Running,
    login: &'static str,
    name: &'static str,
    role: Role,
) -> Person {
    let link = world.invite(role);
    let password = password_for(login);
    // The person opens the link, chooses a sign-in name, a display name and a password.
    let mut opener = running.browser();
    let page = opener.get(&link);
    assert_eq!(page.status, 200, "{}", page.body);
    let mut form = form_to(&page, &link);
    form.set("login", login)
        .set("name", name)
        .set("password", &password);
    let made = opener.submit(&form, None);
    assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
    assert_eq!(made.location(), "/-/account");
    assert!(
        opener.cookie("__Host-inklkunn-session").is_some(),
        "{}",
        made.head
    );
    // The link is used up: the same page as one that never existed.
    let again = running.browser().get(&link);
    assert_eq!(again.status, 404, "{}", again.body);
    // Now the sign-in page, as on any later day.
    let mut browser = running.browser();
    sign_in(&mut browser, login, &password, "/");
    Person {
        login,
        name,
        password,
        browser,
    }
}

/// `/-/sign-in` as a browser uses it; checks how the session cookie was set.
fn sign_in(browser: &mut Browser, login: &str, password: &str, next: &str) {
    let page = browser.get("/-/sign-in");
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(
        browser.cookie("__Host-inklkunn-csrf").is_some(),
        "the sign-in page set no double-submit cookie: {}",
        page.head
    );
    let mut form = form_to(&page, "/-/sign-in");
    form.set("login", login)
        .set("password", password)
        .set("next", next);
    let done = browser.submit(&form, None);
    assert_eq!(done.status, 303, "{}\n{}", done.head, done.body);
    assert_eq!(done.location(), next);
    let session = done
        .headers("set-cookie")
        .into_iter()
        .find(|c| c.starts_with("__Host-inklkunn-session="))
        .unwrap_or_else(|| panic!("no session cookie: {}", done.head));
    for needed in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(session.contains(needed), "{needed} missing in {session}");
    }
    assert!(
        !session.to_ascii_lowercase().contains("domain="),
        "{session}"
    );
    assert!(browser.cookie("__Host-inklkunn-session").is_some());
    // The double-submit cookie is spent.
    assert!(browser.cookie("__Host-inklkunn-csrf").is_none());
}

// ------------------------------------------------------------------------------------ a few readings

const HELP: &str = "/handbook/Help";

/// The text between two markers of a page's body, for a short sentence about what a page said.
fn page_text(browser: &mut Browser, path: &str) -> String {
    let page = browser.get(path);
    assert_eq!(page.status, 200, "{path}: {}", page.body);
    page.body
}

fn proposal_id(location: &str) -> String {
    location
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Open a proposal through the form: the page's own text with `edit` applied, a summary, *Submit*.
fn propose_edit(
    person: &mut Person,
    title: &str,
    summary: &str,
    edit: impl Fn(&str) -> String,
) -> String {
    let page = person
        .browser
        .get(&format!("/handbook/-/proposals/new?page={title}"));
    assert_eq!(page.status, 200, "{}", page.body);
    let mut form = form_to(&page, "/handbook/-/proposals/new");
    let before = form.get("p0_text").to_owned();
    let after = edit(&before);
    assert_ne!(before, after, "the edit changed nothing");
    form.set("p0_text", &after).set("summary", summary);
    let sent = person.browser.submit(&form, Some(("op", "submit")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    proposal_id(&sent.location())
}

fn proposal_page(id: &str) -> String {
    format!("/handbook/-/proposals/{id}")
}

fn comment(person: &mut Person, id: &str, text: &str) -> Reply {
    let page = person.browser.get(&proposal_page(id));
    assert_eq!(page.status, 200, "{}", page.body);
    let mut form = form_to(&page, &format!("/handbook/-/proposals/{id}/comment"));
    form.set("body", text);
    person.browser.submit(&form, None)
}

const READING_BEFORE: &str = "with double brackets";
const READING_AFTER: &str = "with double square brackets";
const READING_OTHER: &str = "with doubled brackets";
const TRUST_BEFORE: &str = "It never claims more than it checked.";
const TRUST_AFTER: &str = "It never claims more than it checked, and says so.";

fn clarify_reading(text: &str) -> String {
    assert!(text.contains(READING_BEFORE), "{text}");
    text.replacen(READING_BEFORE, READING_AFTER, 1)
}

fn reword_reading(text: &str) -> String {
    text.replacen(READING_BEFORE, READING_OTHER, 1)
}

fn say_so(text: &str) -> String {
    assert!(text.contains(TRUST_BEFORE), "{text}");
    text.replacen(TRUST_BEFORE, TRUST_AFTER, 1)
}

// ------------------------------------------------------------------------------------ the web, end to end

#[test]
fn an_invited_person_signs_in_proposes_is_reviewed_and_accepted_and_the_pages_say_so() {
    let Some(world) = World::new("web") else {
        return;
    };
    let started = now();
    let running = Running::start(&world);
    assert!(
        running
            .banner
            .iter()
            .any(|l| l.contains("accepting proposals: the signer is open")),
        "{:?}",
        running.banner
    );
    let first_blocks = world.blocks();

    // ---- a visitor with no account can read, and is told what a write needs; nothing is written
    let mut anonymous = running.browser();
    let home = anonymous.get(HELP);
    assert_eq!(home.status, 200, "{}", home.body);
    let form = anonymous.get("/handbook/-/proposals/new?page=Help");
    assert_eq!(form.status, 403, "{}", form.body);
    assert!(form.body.contains("needs an account"), "{}", form.body);
    let post = anonymous.visit(
        "POST",
        "/handbook/-/proposals/new",
        Some(b"summary=x&op=submit".to_vec()),
        &[],
    );
    assert_eq!(post.status, 403, "{}", post.body);
    assert!(post.body.contains("needs an account"), "{}", post.body);

    // ---- accounts by invitation; the link is single use; a wrong password is the same refusal for everyone
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
    let mut quinn = join(&world, &running, "quinn", "Quinn Reviewer", Role::Reviewer);
    {
        let mut stranger = running.browser();
        let page = stranger.get("/-/sign-in");
        let mut form = form_to(&page, "/-/sign-in");
        form.set("login", "maya")
            .set("password", "not-the-password-at-all-0");
        let refused = stranger.submit(&form, None);
        assert_eq!(refused.status, 401, "{}", refused.body);
        assert!(stranger.cookie("__Host-inklkunn-session").is_none());
        // A sign-in form without the double-submit token is refused before anything is looked at.
        let page = stranger.get("/-/sign-in");
        let mut form = form_to(&page, "/-/sign-in");
        form.set("login", "maya")
            .set("password", &maya.password)
            .set("csrf", "forged");
        let forged = stranger.submit(&form, None);
        assert_eq!(forged.status, 403, "{}", forged.body);
        assert!(stranger.cookie("__Host-inklkunn-session").is_none());
    }
    let header = noor.browser.get(HELP);
    assert!(header.body.contains("Noor Contributor"), "{}", header.body);

    // ---- a wrong or missing CSRF token, or a post from another site, changes nothing
    let page = noor.browser.get("/handbook/-/proposals/new?page=Help");
    let mut form = form_to(&page, "/handbook/-/proposals/new");
    let text = clarify_reading(form.get("p0_text"));
    form.set("p0_text", &text).set("summary", "Not to be sent");
    let own_token = form.get("csrf").to_owned();
    let others_token = maya.browser.csrf();
    assert_ne!(own_token, others_token);
    for (what, token) in [
        ("a forged token", Some("forged-forged-forged".to_owned())),
        ("another session's token", Some(others_token.clone())),
        ("no token", None),
        ("an empty token", Some(String::new())),
    ] {
        let mut attempt = form.clone();
        match &token {
            Some(t) => {
                attempt.set("csrf", t);
            }
            None => {
                attempt.without("csrf");
            }
        }
        let refused = noor.browser.submit(&attempt, Some(("op", "submit")));
        assert_eq!(refused.status, 403, "{what}: {}", refused.body);
        assert!(
            refused
                .body
                .contains("could not be matched to your sign-in"),
            "{what}: {}",
            refused.body
        );
    }
    let cross = noor.browser.visit(
        "POST",
        &form.action,
        Some(form.body(Some(("op", "submit")))),
        &[("Sec-Fetch-Site", "cross-site")],
    );
    assert_eq!(cross.status, 403, "{}", cross.body);
    assert!(
        cross
            .body
            .contains("could not be confirmed as coming from a page of this site"),
        "{}",
        cross.body
    );
    let list = noor.browser.get("/handbook/-/proposals");
    assert!(
        list.body.contains("Nobody has proposed anything yet."),
        "{}",
        list.body
    );
    assert_eq!(world.blocks(), first_blocks);

    // ---- three proposals against the same history: Noor's edit, Quinn's edit of another paragraph, and Noor's
    //      second edit of the paragraph the first one changes
    let p1 = propose_edit(&mut noor, "Help", "Say which brackets", clarify_reading);
    let p2 = propose_edit(&mut quinn, "Help", "Say that it says so", say_so);
    let p3 = propose_edit(&mut noor, "Help", "Reword the brackets", reword_reading);
    assert_ne!(p1, p2);
    assert_ne!(p1, p3);
    let page = page_text(&mut noor.browser, &proposal_page(&p1));
    assert!(page.contains("Noor Contributor"), "{page}");
    assert!(page.contains("Say which brackets"), "{page}");
    assert!(
        page.contains("Held for review") || page.contains("held for review"),
        "{page}"
    );
    assert_eq!(world.blocks(), first_blocks, "proposing writes nothing");
    // INV-6: an open proposal is not the record. The page every reader sees is unchanged, and so is its badge.
    let help = page_text(&mut anonymous, HELP);
    assert!(!help.contains(READING_AFTER), "{help}");
    assert!(!help.contains(READING_OTHER), "{help}");
    assert!(!help.contains(TRUST_AFTER), "{help}");

    // ---- Noor comments; the page shows it
    let commented = comment(
        &mut noor,
        &p1,
        "Please check the wording of this paragraph.",
    );
    assert_eq!(
        commented.status, 303,
        "{}\n{}",
        commented.head, commented.body
    );
    let page = page_text(&mut maya.browser, &proposal_page(&p1));
    assert!(
        page.contains("Please check the wording of this paragraph."),
        "{page}"
    );
    assert!(page.contains("Noor Contributor"), "{page}");

    // ---- what each person is offered: the proposer cannot decide or accept; the maintainer can
    let mine = noor.browser.get(&proposal_page(&p1));
    let accept_action = format!("{}/accept", proposal_page(&p1));
    let decide_action = format!("{}/decide", proposal_page(&p1));
    assert!(!has_form_to(&mine, &accept_action), "{}", mine.body);
    assert!(!has_form_to(&mine, &decide_action), "{}", mine.body);
    let hers = maya.browser.get(&proposal_page(&p1));
    assert!(has_form_to(&hers, &accept_action), "{}", hers.body);
    assert!(has_form_to(&hers, &decide_action), "{}", hers.body);

    // ---- a contributor is not a reviewer: Noor posts the approval herself, with her own valid token, and is
    //      refused; nothing is recorded
    let mut forced = form_to(&hers, &decide_action);
    forced.set("csrf", &noor.browser.csrf());
    let own = noor.browser.submit(&forced, Some(("op", "approve")));
    assert_eq!(own.status, 403, "{}\n{}", own.head, own.body);
    assert!(
        own.body
            .contains("Your role in this wiki does not include this."),
        "{}",
        own.body
    );
    assert!(
        page_text(&mut maya.browser, &proposal_page(&p1)).contains("No reviewer has decided yet."),
        "the refused approval was recorded"
    );
    // D9: a reviewer cannot approve her own proposal either.
    let decide_own = format!("{}/decide", proposal_page(&p2));
    // E1: the page does not offer her the form; where it would be, the sentence of the refusal stands.
    let quinns = quinn.browser.get(&proposal_page(&p2));
    assert!(!has_form_to(&quinns, &decide_own), "{}", quinns.body);
    assert!(
        quinns
            .body
            .contains("You cannot approve or ask for changes on your own proposal"),
        "{}",
        quinns.body
    );
    // The service still refuses it: she posts the maintainer's form of the same shape to her own proposal.
    let mut forced = form_to(&hers, &decide_action);
    forced.action = decide_own.clone();
    forced.set("csrf", &quinn.browser.csrf());
    for op in ["approve", "request-changes"] {
        let own = quinn.browser.submit(&forced, Some(("op", op)));
        assert!(own.status >= 400, "{op}: {}\n{}", own.head, own.body);
        assert!(
            own.body
                .contains("You cannot approve or ask for changes on your own proposal"),
            "{op}: {}\n{}",
            own.head,
            own.body
        );
    }
    assert!(
        page_text(&mut maya.browser, &proposal_page(&p2)).contains("No reviewer has decided yet."),
        "the refused approval was recorded"
    );

    // ---- a contributor cannot accept, even by posting the form herself; an anonymous visitor cannot either
    let mut forged_accept = form_to(&hers, &accept_action);
    forged_accept.set("csrf", &noor.browser.csrf());
    let refused = noor.browser.submit(&forged_accept, None);
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert!(
        refused
            .body
            .contains("does not include accepting proposals"),
        "{}",
        refused.body
    );
    let refused = anonymous.visit("POST", &accept_action, Some(b"revision=1".to_vec()), &[]);
    assert_eq!(refused.status, 403, "{}", refused.body);
    assert_eq!(world.blocks(), first_blocks);

    // ---- Maya approves (a different person from the proposer)
    let page = maya.browser.get(&proposal_page(&p1));
    let mut form = form_to(&page, &decide_action);
    form.set("body", "Looks right to me.");
    let decided = maya.browser.submit(&form, Some(("op", "approve")));
    assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
    let page = page_text(&mut maya.browser, &proposal_page(&p1));
    assert!(page.contains("Looks right to me."), "{page}");
    assert!(page.contains("Maya Maintainer"), "{page}");

    // ---- Maya accepts: the answer, the history, the page, the proposal, the verification
    let page = maya.browser.get(&proposal_page(&p1));
    let form = form_to(&page, &accept_action);
    assert_eq!(world.blocks(), first_blocks);
    let accepted = maya.browser.submit(&form, None);
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert!(
        accepted.body.contains("Signed into the history"),
        "{}",
        accepted.body
    );
    let after_first = world.blocks();
    assert_eq!(
        after_first.len(),
        first_blocks.len() + 1,
        "prikk log: exactly one new block"
    );
    assert_eq!(after_first.get(1), first_blocks.first());
    let help = page_text(&mut anonymous, HELP);
    assert!(help.contains(READING_AFTER), "{help}");
    let proposal = page_text(&mut noor.browser, &proposal_page(&p1));
    assert!(proposal.contains("Signed into the history"), "{proposal}");
    assert!(proposal.contains("Accepted"), "{proposal}");
    let history = page_text(&mut anonymous, "/handbook/-/history/Help");
    assert!(history.contains("Say which brackets"), "{history}");
    assert!(history.contains("Noor Contributor"), "{history}");
    assert!(history.contains("Maya Maintainer"), "{history}");
    let change = page_text(
        &mut anonymous,
        &format!(
            "/handbook/-/change/{}",
            after_first.first().cloned().unwrap_or_default()
        ),
    );
    assert!(change.contains("Say which brackets"), "{change}");
    assert!(change.contains("Signed into the history"), "{change}");
    // A retry (the same form again) finds the record and writes nothing more.
    let again = maya.browser.submit(&form, None);
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(world.blocks(), after_first);

    // The badge comes from a check that ran after the change, off the request path.
    until("the verification after the seal", || {
        world.verification().at.is_some_and(|at| at >= started)
    });
    let check = world.verification();
    assert_eq!(check.state, inklkunn_store::VerificationState::Verified);
    let verification = page_text(&mut anonymous, "/handbook/-/verification");
    assert!(verification.contains("Verified"), "{verification}");
    assert!(
        verification.contains("The last check found nothing wrong."),
        "{verification}"
    );
    let at = check.at.unwrap();
    assert!(
        verification.contains(&format!("Last checked: {}.", utc_minutes(at))),
        "{verification}"
    );
    assert!(at >= started && at <= now(), "the check is recent: {at}");
    let help = page_text(&mut anonymous, HELP);
    assert!(help.contains("Verified"), "{help}");

    // ---- Quinn's proposal was written against the history before Noor's change: it is re-authored and accepted
    let page = maya.browser.get(&proposal_page(&p2));
    let form = form_to(&page, &format!("{}/accept", proposal_page(&p2)));
    let accepted = maya.browser.submit(&form, None);
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert!(
        accepted.body.contains("Signed into the history"),
        "{}",
        accepted.body
    );
    let after_second = world.blocks();
    assert_eq!(after_second.len(), after_first.len() + 1);
    assert_eq!(after_second.get(1), after_first.first());
    let help = page_text(&mut anonymous, HELP);
    assert!(
        help.contains(READING_AFTER),
        "the first change is still there: {help}"
    );
    assert!(
        help.contains(TRUST_AFTER),
        "the second change joined it: {help}"
    );

    // ---- Noor's second edit of the same paragraph overlaps the accepted one: it stops, shows both texts, and
    //      the repository is as it was
    let page = maya.browser.get(&proposal_page(&p3));
    let form = form_to(&page, &format!("{}/accept", proposal_page(&p3)));
    let stopped = maya.browser.submit(&form, None);
    assert_eq!(stopped.status, 409, "{}\n{}", stopped.head, stopped.body);
    assert!(
        stopped.body.contains("Stopped: changes overlap"),
        "{}",
        stopped.body
    );
    assert_eq!(world.blocks(), after_second, "an overlap writes nothing");
    let page = page_text(&mut noor.browser, &proposal_page(&p3));
    assert!(
        page.contains("Needs a revision") || page.contains("needs a revision"),
        "{page}"
    );
    assert!(
        page.contains(READING_AFTER),
        "the wiki's text is shown: {page}"
    );
    assert!(
        page.contains(READING_OTHER),
        "the proposal's text is shown: {page}"
    );
    assert!(
        unescape(&page)
            .contains("The wiki's current text and this proposal both changed the places below."),
        "{page}"
    );
    let help = page_text(&mut anonymous, HELP);
    assert!(!help.contains(READING_OTHER), "{help}");

    // ---- sign-out ends the session at the server too
    let old_session = maya
        .browser
        .cookie("__Host-inklkunn-session")
        .cloned()
        .unwrap();
    let token = maya.browser.csrf();
    let mut out = form_to(&maya.browser.get("/-/sign-out"), "/-/sign-out");
    out.set("csrf", &token);
    let gone = maya.browser.submit(&out, None);
    assert_eq!(gone.status, 303, "{}", gone.body);
    assert!(maya.browser.cookie("__Host-inklkunn-session").is_none());
    let mut replayed = running.browser();
    replayed
        .jar
        .insert("__Host-inklkunn-session".to_owned(), old_session);
    let page = replayed.get("/handbook/-/proposals/new?page=Help");
    assert_eq!(
        page.status, 403,
        "an ended session is no session: {}",
        page.body
    );

    // ---- no password, secret or cookie value reached a log, a note or a page it should not be on
    let browsers = [
        &anonymous,
        &maya.browser,
        &noor.browser,
        &quinn.browser,
        &replayed,
    ];
    let printed = [
        running.notes.join("\n"),
        running.banner.join("\n"),
        running.log.lock().unwrap().join("\n"),
        browsers
            .iter()
            .flat_map(|b| b.bodies.clone())
            .collect::<Vec<_>>()
            .join("\n"),
    ]
    .join("\n");
    let stderr = running.stop();
    let mut secrets = vec![
        SECRET.to_owned(),
        maya.password.clone(),
        noor.password.clone(),
        quinn.password.clone(),
    ];
    // The session values every browser was given (long, random, and in no page or log).
    secrets.extend(
        browsers
            .iter()
            .flat_map(|b| b.values.clone())
            .filter(|v| v.len() >= 20),
    );
    assert!(
        secrets.len() > 4,
        "the search had cookie values to look for"
    );
    for secret in &secrets {
        assert!(!printed.contains(secret), "a secret reached the output");
        assert!(!stderr.contains(secret), "a secret reached stderr");
    }
    let _ = (
        &maya.login,
        &maya.name,
        &noor.login,
        &quinn.name,
        &world.tmp,
    );
}

mod api;
