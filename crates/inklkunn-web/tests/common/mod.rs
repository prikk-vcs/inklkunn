//! What the page tests stand on: a repository in memory behind prikk's `Storage`, a wiki store, the read
//! service, the verifier and an authorizer, assembled into the `Services` the pages ask. No prikk is needed,
//! so these run in both test runs. Included by each test crate with `#[path]`.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use inklkunn_core::read::ReadConfig;
use inklkunn_core::{Authorizer, ReadService, StoreAuthz, Verifier};
use inklkunn_prikk::{
    Change, Diff, ExtractBounds, FileMeta, History, HistoryBlock, Materialized, Point, Result,
    Stage, Storage, StorageError, Tree, Unavailable, Verdict, Verification, View, Views,
};
use inklkunn_store::{
    DisplayName, MemoryAudit, MemoryStore, NewWiki, RootLayout, Slug, Visibility, WikiRecord,
    WikiStore,
};
use inklkunn_web::{AnonymousOnly, Services};

#[derive(Default)]
struct State {
    files: BTreeMap<String, Vec<u8>>,
    version: u64,
    snapshots: BTreeMap<u64, BTreeMap<String, Vec<u8>>>,
    fail_views: Option<StorageError>,
    verify: Option<Result<Verification>>,
    verifies: usize,
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
        Ok(Views {
            schema_version: "branch-list-v1".to_owned(),
            views: vec![View {
                name: "heads/main".to_owned(),
                state_id: state_id(s.version),
                closed: false,
                current: true,
            }],
        })
    }

    fn tree(&self, _: &Path, _: &Point, _: Option<&str>) -> Result<Tree> {
        unsupported("tree")
    }

    fn stat(&self, _: &Path, _: &Point, _: &str) -> Result<FileMeta> {
        unsupported("cat")
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

    fn history(&self, _: &Path, view: &Point, _: Option<u64>) -> Result<History> {
        let s = self.state();
        Ok(History {
            schema_version: "log-report-v1".to_owned(),
            view: view.to_string(),
            blocks: vec![HistoryBlock {
                block_id: block_id(s.version),
                ref_state_id: state_id(s.version),
                update_seq: s.version,
                kind: "Normal".to_owned(),
                rollback_block: false,
                patch_count: 1,
                patch_messages: Vec::new(),
                previous_ref_state_id: None,
            }],
        })
    }

    fn change(&self, _: &Path, _: &str) -> Result<Change> {
        unsupported("show")
    }

    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> Result<Diff> {
        unsupported("diff")
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
    let read = Arc::new(ReadService::new(
        storage.clone(),
        store.clone(),
        store.clone(),
        layout.clone(),
        ReadConfig {
            tip_ttl: std::time::Duration::ZERO,
            ..ReadConfig::default()
        },
        None,
    ));
    let verifier = Arc::new(Verifier::new(storage, store.clone(), layout).with_clock(|| NOW));
    let audit = Arc::new(MemoryAudit::new());
    let authorizer = Arc::new(Authorizer::new(
        Arc::new(StoreAuthz::new(store.clone(), store.clone())),
        audit.clone(),
    ));
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = log.clone();
    let services = Arc::new(Services {
        wikis: store.clone(),
        read,
        verifier,
        authorizer,
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
