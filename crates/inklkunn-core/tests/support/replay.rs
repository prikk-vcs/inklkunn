//! What the replay's tests share: a world over real directories with the real signer and a real prikk, the plan of a bundle, the run of an
//! import, and what an unfinished creation leaves on disk.
#![allow(dead_code, clippy::expect_used, clippy::unwrap_used)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::import::bundle::{
    BundleOptions, BundlePlan, ImportBundleError, ImportStaging, ImportStep, ImportWorld,
    ImportedWiki, import_bundle, plan_bundle,
};
use inklkunn_core::message::Message;
use inklkunn_core::read::ReadConfig;
use inklkunn_core::{KeyAuthority, Signer, SignerConfig};
use inklkunn_prikk::{Admission, PrikkCli, Sandbox, Storage};
use inklkunn_signer::WriteLeases;
use inklkunn_store::{DisplayName, MemoryAudit, MemoryStore, NewWiki, RootLayout, Slug, WikiId};

pub struct Dir(pub PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn dir_in(base: &Path, tag: &str) -> Dir {
    let p = base.join(format!("inklkunn-replay-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("dir");
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).expect("private");
    Dir(p)
}

pub fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

pub fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/interchange")
        .join(rel)
}

/// Everything a replay needs, over real directories.
pub struct World {
    _tmp: Dir,
    _shm: Dir,
    pub layout: RootLayout,
    pub key_dir: PathBuf,
    pub shm: PathBuf,
    pub signer: Arc<Signer>,
    pub storage: Arc<dyn Storage>,
    pub staging: ImportStaging,
    pub leases: Arc<inklkunn_core::WikiLeases>,
    pub audit: Arc<MemoryAudit>,
    pub store: Arc<MemoryStore>,
}

pub fn world(tag: &str) -> World {
    let tmp = dir_in(&std::env::temp_dir(), tag);
    let shm = dir_in(Path::new("/dev/shm"), tag);
    let secret = tmp.0.join("secret");
    std::fs::write(&secret, "an operator secret long enough for the tests").expect("secret");
    let layout = RootLayout::new(tmp.0.join("root"));
    let key_dir = tmp.0.join("keys");
    let mut cfg = SignerConfig::new(
        "prikk",
        Sandbox::Unconfined,
        layout.clone(),
        &key_dir,
        &secret,
        &shm.0,
    );
    cfg.cheap_derivation = true;
    let staging = ImportStaging::new();
    let write_leases = WriteLeases::new(layout.clone());
    let signer = Arc::new(
        Signer::open_creating_store(cfg, staging.ledger())
            .expect("signer")
            .with_leases(&write_leases),
    );
    let leases = staging.leases(write_leases, "replay-test");
    let storage: Arc<dyn Storage> = Arc::new(
        PrikkCli::open(
            "prikk",
            Sandbox::Unconfined,
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(2000)),
        )
        .expect("prikk at the floor"),
    );
    World {
        key_dir,
        shm: shm.0.clone(),
        layout,
        signer,
        storage,
        staging,
        leases,
        audit: Arc::new(MemoryAudit::default()),
        store: Arc::new(MemoryStore::new()),
        _tmp: tmp,
        _shm: shm,
    }
}

pub fn new_wiki(slug: &str, license: &str) -> NewWiki {
    let mut new = NewWiki::new(
        Slug::new(slug).expect("slug"),
        DisplayName::new(slug).expect("name"),
    );
    new.default_license = Some(license.to_owned());
    new
}

pub fn plan_of(bundle: &Path, license: Option<&str>) -> BundlePlan {
    let options = BundleOptions {
        bundle_license: license.map(str::to_owned),
        ..BundleOptions::default()
    };
    let plan = plan_bundle(bundle, &ReadConfig::default(), &options).expect("plan");
    assert!(plan.is_importable(), "{}", plan.render());
    plan
}

pub fn run(
    w: &World,
    plan: &BundlePlan,
    slug: &str,
    license: &str,
    observe: &dyn Fn(ImportStep),
) -> Result<ImportedWiki, ImportBundleError> {
    let key = KeyAuthority::for_tests();
    let reads = w
        .staging
        .reads(w.storage.clone(), w.layout.clone(), ReadConfig::default());
    let world = ImportWorld {
        store: &*w.store,
        acts: &*w.store,
        layout: &w.layout,
        signer: &w.signer,
        storage: &w.storage,
        staging: &w.staging,
        leases: w.leases.clone(),
        audit: w.audit.clone(),
        reads: Some(reads),
        authority: &key,
        version: "0.0.0",
        clock: inklkunn_store::record::now_secs,
    };
    import_bundle(
        &world,
        new_wiki(slug, license),
        plan,
        &plan.render(),
        observe,
    )
}

/// What an unfinished creation leaves on disk: anything under the root but the shard directories, any key record, any seed directory.
pub fn leftovers(w: &World) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let rel = e.path().strip_prefix(root).unwrap().display().to_string();
            let is_shard = e.path().parent() == Some(root) && e.path().is_dir();
            if is_shard {
                walk(&e.path(), root, out);
            } else {
                out.push(rel);
            }
        }
    }
    let mut out = Vec::new();
    walk(w.layout.root(), w.layout.root(), &mut out);
    for entry in std::fs::read_dir(&w.key_dir)
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".key.json") {
            out.push(format!("keys/{name}"));
        }
    }
    for entry in std::fs::read_dir(&w.shm).into_iter().flatten().flatten() {
        out.push(format!("seeds/{}", entry.file_name().to_string_lossy()));
    }
    out.sort();
    out
}

/// The messages of the wiki's blocks, oldest first, parsed.
pub fn messages(w: &World, id: &WikiId) -> Vec<Message> {
    let repo = w.layout.repo_dir(id);
    let history = w
        .storage
        .history(
            &repo,
            &inklkunn_prikk::Point::View("heads/main".to_owned()),
            None,
        )
        .expect("history");
    history
        .blocks
        .iter()
        .rev()
        .map(|b| {
            let text = &b.patch_messages.first().expect("a patch").message;
            Message::parse(text).unwrap_or_else(|e| panic!("{e}: {text:?}"))
        })
        .collect()
}

pub fn read_text(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
