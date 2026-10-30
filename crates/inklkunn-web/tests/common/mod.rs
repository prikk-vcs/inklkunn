//! What the page tests stand on: a repository in memory behind prikk's `Storage`, a wiki store, the read
//! service, the verifier and an authorizer, assembled into the `Services` the pages ask. No prikk is needed,
//! so these run in both test runs. Included by each test crate with `#[path]`.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_core::read::ReadConfig;
use inklkunn_core::{Authorizer, Inbox, ReadService, StoreAuthz, Verifier};
use inklkunn_prikk::storage::PathRef;
use inklkunn_prikk::{
    Change, Diff, ExtractBounds, FileMeta, History, HistoryBlock, Materialized, Operation, Patch,
    PatchMessage, Point, Result, Stage, Storage, StorageError, Tree, Unavailable, Verdict,
    Verification, View, Views,
};
use inklkunn_store::{
    DisplayName, MemoryAudit, MemoryStore, NewWiki, RootLayout, Slug, Visibility, WikiRecord,
    WikiStore,
};
use inklkunn_web::{AnonymousOnly, Services};

mod inbox;
mod past;
#[allow(unused_imports)]
pub use inbox::*;
#[allow(unused_imports)]
pub use past::*;

#[derive(Default)]
struct State {
    files: BTreeMap<String, Vec<u8>>,
    version: u64,
    snapshots: BTreeMap<u64, BTreeMap<String, Vec<u8>>>,
    fail_views: Option<StorageError>,
    verify: Option<Result<Verification>>,
    verifies: usize,
    /// The message sealed with each version's block: `history` lists one block per version (T5).
    messages: BTreeMap<u64, String>,
    /// What `show` answers for a block id.
    shows: BTreeMap<String, Result<Change>>,
    /// What `diff` answers, and every `diff` asked.
    diff: Option<Result<Diff>>,
    diffs: Vec<(Point, Point, Vec<String>)>,
    /// How many `show`s were asked.
    show_calls: usize,
    /// The message of the newest change (the tip's block).
    tip_message: String,
    /// Older changes of each view, newest first: `(block id, message)`.
    older: BTreeMap<String, Vec<(String, String)>>,
    /// Views besides `heads/main`; all sit at the same state.
    extra_views: Vec<String>,
    /// The pages (paths) each change touched, by block id.
    touched: BTreeMap<String, Vec<String>>,
    /// How many times `show` was asked.
    touch_shows: usize,
    /// Make `show` fail.
    fail_show: bool,
}

/// A one-view repository whose files can be changed, whose `branch list` can fail, and whose `verify` says
/// what it was told to.
pub struct Fake {
    state: Mutex<State>,
}

pub fn block_id(n: u64) -> String {
    format!("{:064x}", 0xb10c_0000 + n)
}

pub fn state_id(n: u64) -> String {
    format!("{n:064x}")
}

impl Fake {
    pub fn new(files: &[(&str, &str)]) -> Arc<Self> {
        let fake = Fake {
            state: Mutex::new(State::default()),
        };
        {
            let mut s = fake.state();
            for (p, t) in files {
                s.files.insert((*p).to_owned(), t.as_bytes().to_vec());
            }
            s.version = 1;
            let f = s.files.clone();
            s.snapshots.insert(1, f);
            s.verify = Some(Ok(clean()));
        }
        Arc::new(fake)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set(&self, path: &str, text: &str) {
        let mut s = self.state();
        s.files.insert(path.to_owned(), text.as_bytes().to_vec());
        s.version += 1;
        let (v, f) = (s.version, s.files.clone());
        s.snapshots.insert(v, f);
    }

    /// Write several files in one block: the tip moves once.
    pub fn set_files(&self, files: &[(&str, &str)]) {
        let mut s = self.state();
        for (path, text) in files {
            s.files.insert((*path).to_owned(), text.as_bytes().to_vec());
        }
        s.version += 1;
        let (v, f) = (s.version, s.files.clone());
        s.snapshots.insert(v, f);
    }

    /// The message the block of `version` was sealed with.
    pub fn say(&self, version: u64, message: &str) {
        self.state().messages.insert(version, message.to_owned());
    }

    /// What `show` answers for the block of `version`.
    pub fn show(&self, version: u64, answer: Result<Change>) {
        self.state().shows.insert(block_id(version), answer);
    }

    /// What `diff` answers.
    pub fn diff_says(&self, answer: Result<Diff>) {
        self.state().diff = Some(answer);
    }

    /// Every `diff` asked: from, to and the paths.
    pub fn diffs_asked(&self) -> Vec<(Point, Point, Vec<String>)> {
        self.state().diffs.clone()
    }

    /// How many `show`s were asked.
    pub fn shows_asked(&self) -> usize {
        self.state().show_calls
    }

    pub fn fail_views(&self, e: Option<StorageError>) {
        self.state().fail_views = e;
    }

    /// How many times `verify` was asked.
    pub fn verifies(&self) -> usize {
        self.state().verifies
    }

    /// What the next `verify` answers.
    pub fn verify_says(&self, answer: Result<Verification>) {
        self.state().verify = Some(answer);
    }

    /// The message of the newest change.
    pub fn tip_message(&self, message: &str) {
        self.state().tip_message = message.to_owned();
    }

    /// An open view besides `heads/main`, at the same state.
    pub fn add_view(&self, name: &str) {
        self.state().extra_views.push(name.to_owned());
    }

    /// An older change of `view`, after the ones already added (so add the newest first).
    pub fn push_change(&self, view: &str, block: &str, message: &str) {
        self.state()
            .older
            .entry(view.to_owned())
            .or_default()
            .push((block.to_owned(), message.to_owned()));
    }

    /// The pages a change touched.
    pub fn touches(&self, block: &str, paths: &[&str]) {
        self.state().touched.insert(
            block.to_owned(),
            paths.iter().map(|p| (*p).to_owned()).collect(),
        );
    }

    /// How many times `show` was asked.
    pub fn shows(&self) -> usize {
        self.state().touch_shows
    }

    /// Make every `show` fail.
    pub fn fail_show(&self, fail: bool) {
        self.state().fail_show = fail;
    }
}

pub fn clean() -> Verification {
    Verification {
        schema_version: "verify-report-v1".to_owned(),
        verdict: Verdict {
            ok: true,
            failed_conditions: Vec::new(),
        },
        stages: vec![Stage {
            stage: "objects".to_owned(),
            status: "evaluated".to_owned(),
            message: None,
            blocked_by: None,
        }],
    }
}

pub fn damaged() -> Verification {
    Verification {
        schema_version: "verify-report-v1".to_owned(),
        verdict: Verdict {
            ok: false,
            failed_conditions: Vec::new(),
        },
        stages: vec![
            Stage {
                stage: "refs".to_owned(),
                status: "failed".to_owned(),
                message: Some("integrity error: damaged".to_owned()),
                blocked_by: None,
            },
            Stage {
                stage: "ref-update-schema-trust".to_owned(),
                status: "not_evaluated".to_owned(),
                message: None,
                blocked_by: Some("refs".to_owned()),
            },
        ],
    }
}

pub fn busy() -> StorageError {
    StorageError::Unavailable {
        verb: "branch",
        reason: Unavailable::Busy,
    }
}

fn unsupported<T>(verb: &'static str) -> Result<T> {
    Err(StorageError::Driver {
        verb,
        detail: "not in this double".to_owned(),
    })
}

impl Storage for Fake {
    fn views(&self, _: &Path) -> Result<Views> {
        let s = self.state();
        if let Some(e) = &s.fail_views {
            return Err(e.clone());
        }
        let mut views = vec![View {
            name: "heads/main".to_owned(),
            state_id: state_id(s.version),
            closed: false,
            current: true,
        }];
        views.extend(s.extra_views.iter().map(|name| View {
            name: format!("heads/{name}"),
            state_id: state_id(s.version),
            closed: false,
            current: false,
        }));
        Ok(Views {
            schema_version: "branch-list-v1".to_owned(),
            views,
        })
    }

    fn tree(&self, _: &Path, _: &Point, _: Option<&str>) -> Result<Tree> {
        unsupported("tree")
    }

    fn stat(&self, _: &Path, point: &Point, path: &str) -> Result<FileMeta> {
        let s = self.state();
        let Point::Block(id) = point else {
            return unsupported("cat");
        };
        s.snapshots
            .iter()
            .find(|(v, _)| block_id(**v) == *id)
            .and_then(|(_, f)| f.get(path))
            .map(|bytes| FileMeta {
                schema_version: "path-content-v1".to_owned(),
                point: point.to_string(),
                target_block_id: id.clone(),
                path: path.to_owned(),
                kind: "file".to_owned(),
                encoding: Some("text".to_owned()),
                mode: None,
                size: bytes.len() as u64,
            })
            .ok_or_else(|| StorageError::Refused {
                verb: "cat",
                message: format!("no {path}"),
                kind: None,
                reason: None,
            })
    }

    fn read(&self, _: &Path, point: &Point, path: &str, _: u64) -> Result<Vec<u8>> {
        let s = self.state();
        let files = match point {
            Point::Block(id) => s
                .snapshots
                .iter()
                .find(|(v, _)| block_id(**v) == *id)
                .map(|(_, f)| f),
            Point::View(_) => Some(&s.files),
        };
        files
            .and_then(|f| f.get(path))
            .cloned()
            .ok_or_else(|| StorageError::Refused {
                verb: "cat",
                message: format!("no {path}"),
                kind: None,
                reason: None,
            })
    }

    fn history(&self, _: &Path, view: &Point, limit: Option<u64>) -> Result<History> {
        let s = self.state();
        let scripted = !s.tip_message.is_empty() || s.older.contains_key(view.as_arg());
        let blocks: Vec<HistoryBlock> = if scripted {
            let block = |id: String, state: String, seq: u64, message: &str| HistoryBlock {
                block_id: id,
                ref_state_id: state,
                update_seq: seq,
                kind: "Normal".to_owned(),
                rollback_block: false,
                patch_count: 1,
                patch_messages: if message.is_empty() {
                    Vec::new()
                } else {
                    vec![PatchMessage {
                        patch_id: format!("{seq:064x}"),
                        message: message.to_owned(),
                    }]
                },
                previous_ref_state_id: None,
            };
            let mut blocks = vec![block(
                block_id(s.version),
                state_id(s.version),
                s.version,
                &s.tip_message,
            )];
            let mut seq = s.version;
            for (id, message) in s.older.get(view.as_arg()).into_iter().flatten() {
                seq = seq.saturating_sub(1);
                blocks.push(block(id.clone(), state_id(seq), seq, message));
            }
            if let Some(limit) = limit {
                blocks.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
            }
            blocks
        } else {
            let limit = limit.map_or(1_000, |l| usize::try_from(l).unwrap_or(usize::MAX));
            (1..=s.version)
                .rev()
                .take(limit)
                .map(|v| HistoryBlock {
                    block_id: block_id(v),
                    ref_state_id: state_id(v),
                    update_seq: v,
                    kind: if v == 1 { "Root" } else { "Normal" }.to_owned(),
                    rollback_block: false,
                    patch_count: 1,
                    patch_messages: s
                        .messages
                        .get(&v)
                        .map(|m| {
                            vec![PatchMessage {
                                patch_id: format!("{v:064x}"),
                                message: m.clone(),
                            }]
                        })
                        .unwrap_or_default(),
                    previous_ref_state_id: (v > 1).then(|| state_id(v - 1)),
                })
                .collect::<Vec<_>>()
        };
        Ok(History {
            schema_version: "log-report-v1".to_owned(),
            view: view.to_string(),
            blocks,
        })
    }

    fn change(&self, _: &Path, id: &str) -> Result<Change> {
        let mut s = self.state();
        s.show_calls += 1;
        if let Some(known) = s.shows.get(id).cloned() {
            return known;
        }
        s.touch_shows += 1;
        if s.fail_show {
            return Err(busy());
        }
        let paths = s.touched.get(id).cloned().unwrap_or_default();
        Ok(Change {
            schema_version: "show-report-v1".to_owned(),
            patches: vec![Patch {
                patch_id: "p".repeat(64),
                queued: false,
                operations: paths
                    .into_iter()
                    .map(|p| Operation {
                        kind: "edit-text".to_owned(),
                        paths: vec![PathRef {
                            path: Some(p),
                            unresolved_node_id: None,
                        }],
                        content: Default::default(),
                    })
                    .collect(),
            }],
        })
    }

    fn diff(&self, _: &Path, from: &Point, to: &Point, paths: &[&str]) -> Result<Diff> {
        let mut s = self.state();
        s.diffs.push((
            from.clone(),
            to.clone(),
            paths.iter().map(|p| (*p).to_owned()).collect(),
        ));
        s.diff.clone().unwrap_or_else(|| unsupported("diff"))
    }

    fn plan(&self, _: &Path, _: &Point) -> Result<Materialized> {
        unsupported("extract")
    }

    fn materialize(
        &self,
        _: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        _: ExtractBounds,
    ) -> Result<Materialized> {
        let files = {
            let s = self.state();
            match point {
                Point::Block(id) => s
                    .snapshots
                    .iter()
                    .find(|(v, _)| block_id(**v) == *id)
                    .map(|(_, f)| f.clone()),
                Point::View(_) => Some(s.files.clone()),
            }
        }
        .ok_or_else(|| StorageError::Refused {
            verb: "extract",
            message: "no such block".to_owned(),
            kind: None,
            reason: None,
        })?;
        let target = scratch.join(name);
        std::fs::create_dir_all(&target).expect("target");
        for (path, bytes) in &files {
            let at = target.join(path);
            std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
            std::fs::write(&at, bytes).expect("file");
        }
        Ok(Materialized {
            schema_version: "extract-report-v1".to_owned(),
            point: point.to_string(),
            point_kind: "ref".to_owned(),
            target_block_id: "b".repeat(64),
            claim_parent_block_id: None,
            provisional: false,
            written: true,
            file_count: files.len() as u64,
            total_bytes: 0,
            largest_file_bytes: 0,
        })
    }

    fn verify(&self, _: &Path) -> Result<Verification> {
        let mut s = self.state();
        s.verifies += 1;
        s.verify.clone().unwrap_or_else(|| unsupported("verify"))
    }
}

pub struct TempDir(pub PathBuf);

static NEXT: AtomicUsize = AtomicUsize::new(0);

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!("inklkunn-web-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).expect("temp");
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A wiki with its repository, and the services over them.
pub struct World {
    pub fake: Arc<Fake>,
    pub store: Arc<MemoryStore>,
    pub wiki: WikiRecord,
    pub services: Arc<Services>,
    pub audit: Arc<MemoryAudit>,
    pub log: Arc<Mutex<Vec<String>>>,
    _dir: TempDir,
}

pub const NOW: i64 = 1_700_000_000;

impl World {
    pub fn new(slug: &str, name: &str, files: &[(&str, &str)]) -> World {
        Self::with_visibility(slug, name, Visibility::Public, files)
    }

    pub fn with_visibility(
        slug: &str,
        name: &str,
        visibility: Visibility,
        files: &[(&str, &str)],
    ) -> World {
        let dir = TempDir::new("world");
        let fake = Fake::new(files);
        let store = Arc::new(MemoryStore::new());
        let wiki = store
            .create(NewWiki {
                visibility,
                ..NewWiki::new(
                    Slug::new(slug).expect("slug"),
                    DisplayName::new(name).expect("name"),
                )
            })
            .expect("wiki");
        let layout = RootLayout::new(dir.0.join("root"));
        let storage: Arc<dyn Storage> = fake.clone();
        let (services, audit, log) = assemble(storage, &store, layout);
        World {
            fake,
            store,
            wiki,
            services,
            audit,
            log,
            _dir: dir,
        }
    }

    /// The operator's log so far.
    pub fn logged(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// A page file with a title and a body.
pub fn page(front: &[&str], body: &str) -> String {
    let mut s = String::from("---\n");
    for l in front {
        s.push_str(l);
        s.push('\n');
    }
    s.push_str("---\n\n");
    s.push_str(body);
    s
}

/// The services over any storage: the read service (no hold on the tip), the verifier, an authorizer over the
/// store and an anonymous identity.
pub fn assemble(
    storage: Arc<dyn Storage>,
    store: &Arc<MemoryStore>,
    layout: RootLayout,
) -> (Arc<Services>, Arc<MemoryAudit>, Arc<Mutex<Vec<String>>>) {
    let read = Arc::new(
        ReadService::new(
            storage.clone(),
            store.clone(),
            store.clone(),
            layout.clone(),
            ReadConfig {
                tip_ttl: std::time::Duration::ZERO,
                ..ReadConfig::default()
            },
            None,
        )
        .with_revision_flags(store.clone()),
    );
    let verifier = Arc::new(Verifier::new(storage, store.clone(), layout).with_clock(|| NOW));
    let audit = Arc::new(MemoryAudit::new());
    let authorizer = Arc::new(Authorizer::new(
        Arc::new(StoreAuthz::new(store.clone(), store.clone())),
        audit.clone(),
    ));
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    let inbox = Arc::new(
        Inbox::new(
            store.clone(),
            store.clone(),
            store.clone(),
            store.clone(),
            (*authorizer).clone(),
        )
        .with_clock(|| NOW),
    );
    let services = Arc::new(Services {
        wikis: store.clone(),
        read,
        verifier,
        authorizer,
        inbox,
        identity: Arc::new(AnonymousOnly),
        log: Arc::new(move |line| {
            sink.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(line.to_owned());
        }),
        clock: || NOW,
    });
    (services, audit, log)
}

// ------------------------------------------------------------------------------------ over HTTP

/// What a request came to.
pub struct Answer {
    pub status: axum::http::StatusCode,
    pub headers: axum::http::HeaderMap,
    pub bytes: Vec<u8>,
}

impl Answer {
    pub fn body(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

pub const APP: &str = "localhost:8080";
pub const CONTENT: &str = "localhost:8081";

/// The site over the world's services: both origins, as the binary builds it.
pub fn site(w: &World) -> axum::Router {
    inklkunn_web::routes::site_with(
        &inklkunn_web::WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
        Some(w.services.clone()),
    )
    .unwrap()
}

/// One request to `host` (`APP` or `CONTENT`).
pub async fn request(
    w: &World,
    host: &str,
    method: &str,
    uri: &str,
    extra: &[(&str, &str)],
) -> Answer {
    use tower::ServiceExt as _;
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header(axum::http::header::HOST, host);
    for (k, v) in extra {
        req = req.header(*k, *v);
    }
    let res = site(w)
        .oneshot(req.body(axum::body::Body::empty()).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    Answer {
        status: parts.status,
        headers: parts.headers,
        bytes: axum::body::to_bytes(body, 1 << 24).await.unwrap().to_vec(),
    }
}

/// `GET` on the application origin.
pub async fn get(w: &World, uri: &str) -> Answer {
    request(w, APP, "GET", uri, &[]).await
}

/// Who a request is, for the tests of readers who are not anonymous: the account named by the header
/// `x-test-account`, else anonymous.
pub struct HeaderAccount(inklkunn_core::IdentityAuthority);

impl inklkunn_web::Identify for HeaderAccount {
    fn principal(&self, headers: &axum::http::HeaderMap) -> inklkunn_core::Principal {
        headers
            .get("x-test-account")
            .and_then(|v| v.to_str().ok())
            .and_then(inklkunn_store::AccountId::parse)
            .map_or(inklkunn_core::Principal::Anonymous, |id| {
                inklkunn_core::Principal::session(&self.0, id)
            })
    }
}

/// The world with requests that carry `x-test-account` taken as that account.
pub fn with_accounts(mut w: World) -> World {
    w.services = Arc::new(Services {
        identity: Arc::new(HeaderAccount(inklkunn_core::IdentityAuthority::for_tests())),
        ..(*w.services).clone()
    });
    w
}

/// Let `account` read the wiki (an observer).
pub fn let_read(w: &World, account: &inklkunn_store::AccountId) {
    use inklkunn_store::{Account, AccountStore, Role, RoleGrant, RoleStore};
    w.store
        .add_account(&Account {
            id: account.clone(),
            display_name: DisplayName::new("A reader").unwrap(),
            local: None,
            created_at: NOW,
            disabled: false,
        })
        .unwrap();
    w.store
        .grant(RoleGrant {
            account: account.clone(),
            wiki: w.wiki.id.clone(),
            role: Role::Observer,
            granted_by: None,
            granted_at: NOW,
        })
        .unwrap();
}

/// A canonical patch message for an accepted proposal.
pub fn proposal_message(summary: &str, proposer: &str, acceptor: &str) -> String {
    format!(
        "{summary}\n\nProposal: 019a0000-0000-7000-8000-00000000aaaa\n\
         Proposed-By: 019a0000-0000-7000-8000-0000000000a1 ({proposer})\n\
         Accepted-By: 019a0000-0000-7000-8000-0000000000a2 ({acceptor})\n\
         Base: {}\nKind: proposal\n",
        "b".repeat(64)
    )
}

/// A canonical patch message for an import the service made itself.
pub fn import_message(summary: &str) -> String {
    format!(
        "{summary}\n\nProposed-By: system (inklkunn)\nAccepted-By: system (inklkunn)\n\
         Kind: import\nImported-From: bundle:sha256:abcd\n"
    )
}
