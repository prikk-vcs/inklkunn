//! Test support for the read service: a repository in memory behind [`Storage`], so that the whole path
//! (tip, index, cache, render, sanitize) is tested in both test runs, with and without prikk. Nothing here is
//! compiled outside tests.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use inklkunn_markup::TransclusionRefusal;
use inklkunn_prikk::{
    Change, Diff, ExtractBounds, FileMeta, History, Materialized, Point, Result, Storage,
    StorageError, Tree, Verification, View, Views,
};
use inklkunn_store::{DisplayName, MemoryStore, NewWiki, RootLayout, Slug, WikiRecord, WikiStore};

use super::{Calls, CountingStorage, Presentation, ReadConfig, ReadService};

#[derive(Default)]
struct State {
    files: BTreeMap<String, Vec<u8>>,
    /// Moves whenever a file does: the view's RefState id.
    version: u64,
    extra_views: Vec<(String, u64)>,
    fail_views: Option<StorageError>,
    fail_materialize: Option<StorageError>,
    fail_read: Option<StorageError>,
    delay: Duration,
}

/// A one-view repository. Changing a file moves the tip.
pub(crate) struct Fake {
    state: Mutex<State>,
}

impl Fake {
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
        let fake = Fake {
            state: Mutex::new(State::default()),
        };
        for (path, text) in files {
            fake.state()
                .files
                .insert((*path).to_owned(), text.as_bytes().to_vec());
        }
        fake.state().version = 1;
        fake
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Write a file and move the tip.
    pub(crate) fn set(&self, path: &str, text: &str) {
        self.set_bytes(path, text.as_bytes());
    }

    pub(crate) fn set_bytes(&self, path: &str, bytes: &[u8]) {
        let mut s = self.state();
        s.files.insert(path.to_owned(), bytes.to_vec());
        s.version += 1;
    }

    /// A second view, which has its own state.
    pub(crate) fn add_view(&self, name: &str) {
        let mut s = self.state();
        s.extra_views.push((name.to_owned(), 1));
    }

    pub(crate) fn fail_views(&self, e: Option<StorageError>) {
        self.state().fail_views = e;
    }

    pub(crate) fn fail_materialize(&self, e: Option<StorageError>) {
        self.state().fail_materialize = e;
    }

    pub(crate) fn fail_read(&self, e: Option<StorageError>) {
        self.state().fail_read = e;
    }

    /// Make `materialize` take this long, so that readers meet each other inside it.
    pub(crate) fn slow_materialize(&self, d: Duration) {
        self.state().delay = d;
    }

    pub(crate) fn state_id(&self) -> String {
        state_id(self.state().version)
    }
}

fn state_id(n: u64) -> String {
    format!("{n:064x}")
}

fn unsupported(verb: &'static str) -> StorageError {
    StorageError::Driver {
        verb,
        detail: "the in-memory repository does not implement this verb".to_owned(),
    }
}

impl Storage for Fake {
    fn views(&self, _repo: &Path) -> Result<Views> {
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
        for (name, n) in &s.extra_views {
            views.push(View {
                name: name.clone(),
                state_id: state_id(1_000_000 + n),
                closed: false,
                current: false,
            });
        }
        Ok(Views {
            schema_version: "branch-list-v1".to_owned(),
            views,
        })
    }

    fn tree(&self, _: &Path, _: &Point, _: Option<&str>) -> Result<Tree> {
        Err(unsupported("tree"))
    }

    fn stat(&self, _: &Path, _: &Point, _: &str) -> Result<FileMeta> {
        Err(unsupported("cat"))
    }

    fn read(&self, _repo: &Path, _point: &Point, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let s = self.state();
        if let Some(e) = &s.fail_read {
            return Err(e.clone());
        }
        match s.files.get(path) {
            None => Err(StorageError::Refused {
                verb: "cat",
                message: format!("path {path} not found"),
                kind: None,
                reason: None,
            }),
            Some(bytes) if bytes.len() as u64 > max_bytes => Err(StorageError::Refused {
                verb: "cat",
                message: format!("{path} is above the --max-bytes bound"),
                kind: None,
                reason: None,
            }),
            Some(bytes) => Ok(bytes.clone()),
        }
    }

    fn history(&self, _: &Path, _: &Point, _: Option<u64>) -> Result<History> {
        Err(unsupported("log"))
    }

    fn change(&self, _: &Path, _: &str) -> Result<Change> {
        Err(unsupported("show"))
    }

    fn diff(&self, _: &Path, _: &Point, _: &Point, _: &[&str]) -> Result<Diff> {
        Err(unsupported("diff"))
    }

    fn plan(&self, _: &Path, _: &Point) -> Result<Materialized> {
        Err(unsupported("extract"))
    }

    fn materialize(
        &self,
        _repo: &Path,
        point: &Point,
        scratch: &Path,
        name: &str,
        bounds: ExtractBounds,
    ) -> Result<Materialized> {
        let (files, fail, delay) = {
            let s = self.state();
            (s.files.clone(), s.fail_materialize.clone(), s.delay)
        };
        std::thread::sleep(delay);
        if let Some(e) = fail {
            return Err(e);
        }
        let total: u64 = files.values().map(|b| b.len() as u64).sum();
        let count = files.len() as u64;
        let refuse = |kind: &str| StorageError::Refused {
            verb: "extract",
            message: format!("the point is over the bound ({kind})"),
            kind: Some(kind.to_owned()),
            reason: None,
        };
        if bounds.max_bytes.is_some_and(|m| total > m) {
            return Err(refuse("over-max-bytes"));
        }
        if bounds.max_files.is_some_and(|m| count > m) {
            return Err(refuse("over-max-files"));
        }
        let target = scratch.join(name);
        for (path, bytes) in &files {
            let at = target.join(path);
            if let Some(dir) = at.parent() {
                std::fs::create_dir_all(dir).map_err(|e| StorageError::Damaged {
                    verb: "extract",
                    detail: e.to_string(),
                })?;
            }
            std::fs::write(&at, bytes).map_err(|e| StorageError::Damaged {
                verb: "extract",
                detail: e.to_string(),
            })?;
        }
        if files.is_empty() {
            std::fs::create_dir_all(&target).map_err(|e| StorageError::Damaged {
                verb: "extract",
                detail: e.to_string(),
            })?;
        }
        Ok(Materialized {
            schema_version: "extract-report-v1".to_owned(),
            point: point.to_string(),
            point_kind: "ref".to_owned(),
            target_block_id: "b".repeat(64),
            claim_parent_block_id: None,
            provisional: false,
            written: true,
            file_count: count,
            total_bytes: total,
            largest_file_bytes: files.values().map(|b| b.len() as u64).max().unwrap_or(0),
        })
    }

    fn verify(&self, _: &Path) -> Result<Verification> {
        Err(unsupported("verify"))
    }
}

/// A directory removed when dropped.
pub(crate) struct TempDir(pub(crate) PathBuf);

static NEXT: AtomicUsize = AtomicUsize::new(0);

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let p =
            std::env::temp_dir().join(format!("inklkunn-read-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap_or_else(|e| panic!("temp dir: {e}"));
        TempDir(p)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The reader's presentation, in English and under `/w`.
pub(crate) struct Env {
    pub(crate) lang: &'static str,
    pub(crate) base: &'static str,
}

impl Env {
    pub(crate) const EN: Env = Env {
        lang: "en",
        base: "/w",
    };
    pub(crate) const JA: Env = Env {
        lang: "ja",
        base: "/w",
    };
}

impl Presentation for Env {
    fn lang(&self) -> &str {
        self.lang
    }

    fn key(&self) -> String {
        self.base.to_owned()
    }

    fn page_href(&self, title: &str) -> String {
        format!("{}/{}", self.base, inklkunn_markup::percent_encode(title))
    }

    fn create_href(&self, title: &str) -> Option<String> {
        Some(format!(
            "{}/-/create?title={}",
            self.base,
            inklkunn_markup::percent_encode(title)
        ))
    }

    fn transclusion_label(&self, title: &str) -> String {
        match self.lang {
            "ja" => format!("{title} から"),
            _ => format!("From {title}"),
        }
    }

    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String {
        match refusal {
            TransclusionRefusal::Cycle { chain } => {
                format!(
                    "Not included, it would include itself: {}",
                    chain.join(" → ")
                )
            }
            TransclusionRefusal::TooDeep { title, limit } => {
                format!("Not included, deeper than {limit}: {title}")
            }
            TransclusionRefusal::OverBudget { title } => {
                format!("Not included, a page includes only so much: {title}")
            }
        }
    }
}

/// A wiki over the in-memory repository, a counting storage, a memory store and a read service.
pub(crate) struct World {
    /// Held so that the directory outlives the test.
    #[allow(dead_code)]
    pub(crate) dir: TempDir,
    pub(crate) counting: Arc<CountingStorage<Fake>>,
    pub(crate) store: Arc<MemoryStore>,
    pub(crate) service: ReadService,
    pub(crate) wiki: WikiRecord,
    pub(crate) layout: RootLayout,
}

impl World {
    pub(crate) fn with(files: &[(&str, &str)], config: ReadConfig) -> Self {
        let dir = TempDir::new("world");
        let counting = Arc::new(CountingStorage::new(Fake::new(files)));
        let store = Arc::new(MemoryStore::new());
        let layout = RootLayout::new(dir.0.join("root"));
        let wiki = store
            .create(NewWiki::new(
                Slug::new("read-test").unwrap_or_else(|e| panic!("{e}")),
                DisplayName::new("Read test").unwrap_or_else(|e| panic!("{e}")),
            ))
            .unwrap_or_else(|e| panic!("{e}"));
        let service = Self::service_over(&counting, &store, &layout, config);
        World {
            dir,
            counting,
            store,
            service,
            wiki,
            layout,
        }
    }

    /// Another service over the same repository, store and layout: a restart.
    pub(crate) fn service_over(
        counting: &Arc<CountingStorage<Fake>>,
        store: &Arc<MemoryStore>,
        layout: &RootLayout,
        config: ReadConfig,
    ) -> ReadService {
        ReadService::new(
            counting.clone(),
            store.clone(),
            store.clone(),
            layout.clone(),
            config,
            None,
        )
    }

    /// A world of pages given as `(title, extra frontmatter lines, body)`: `pages/<title>.md` with
    /// `title: <title>` first. The tip is held for an hour: time never decides anything.
    pub(crate) fn pages(list: &[(&str, &[&str], &str)]) -> Self {
        Self::pages_with(
            list,
            ReadConfig {
                tip_ttl: Duration::from_secs(3600),
                ..ReadConfig::default()
            },
        )
    }

    pub(crate) fn pages_with(list: &[(&str, &[&str], &str)], config: ReadConfig) -> Self {
        let files = files_of(list);
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, t)| (p.as_str(), t.as_str()))
            .collect();
        Self::with(&refs, config)
    }

    pub(crate) fn fake(&self) -> &Fake {
        self.counting.inner()
    }

    pub(crate) fn calls(&self) -> Calls {
        self.counting.calls()
    }

    /// The calls made while `f` ran.
    pub(crate) fn counted<T>(&self, f: impl FnOnce() -> T) -> (T, Calls) {
        let before = self.calls();
        let out = f();
        (out, self.calls().since(&before))
    }
}

/// `(path, text)` of each page of `(title, extra frontmatter lines, body)`.
pub(crate) fn files_of(list: &[(&str, &[&str], &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(title, front, body)| {
            let mut lines = vec![format!("title: {title}")];
            lines.extend(front.iter().map(|l| (*l).to_owned()));
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let path =
                inklkunn_markup::title_to_path(title).unwrap_or_else(|e| panic!("{title}: {e}"));
            (path, page(&refs, body))
        })
        .collect()
}

/// A page file: frontmatter lines, a blank line, the body.
pub(crate) fn page(front: &[&str], body: &str) -> String {
    let mut s = String::from("---\n");
    for line in front {
        s.push_str(line);
        s.push('\n');
    }
    s.push_str("---\n\n");
    s.push_str(body);
    s
}

/// What is live or foreign in `html`: an element, attribute or class outside the profile's vocabulary, an
/// event handler, a comment, an address that is `javascript:`, `data:` or `vbscript:`. Empty for sanitized
/// output. A scan made for the output of an HTML serializer (values quoted and escaped), not a parser.
pub(crate) fn findings(html: &str) -> Vec<String> {
    use inklkunn_markup::render::vocabulary::{ELEMENTS, attribute_is_known, class_is_known};
    let b = html.as_bytes();
    let at = |i: usize| b.get(i).copied();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if at(i) != Some(b'<') {
            i += 1;
            continue;
        }
        if html.get(i..).is_some_and(|rest| rest.starts_with("<!--")) {
            out.push("a comment".to_owned());
            i += 4;
            continue;
        }
        let start = i + 1;
        if !at(start).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let mut j = start;
        while at(j).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'-') {
            j += 1;
        }
        let name = html.get(start..j).unwrap_or("").to_ascii_lowercase();
        if !ELEMENTS.contains(&name.as_str()) {
            out.push(format!("element <{name}>"));
        }
        loop {
            while at(j).is_some_and(|c| c.is_ascii_whitespace() || c == b'/') {
                j += 1;
            }
            if j >= b.len() || at(j) == Some(b'>') {
                break;
            }
            let s = j;
            while at(j)
                .is_some_and(|c| !c.is_ascii_whitespace() && !matches!(c, b'=' | b'>' | b'/'))
            {
                j += 1;
            }
            let attr = html.get(s..j).unwrap_or("").to_ascii_lowercase();
            let mut value = String::new();
            if at(j) == Some(b'=') {
                j += 1;
                if let Some(q) = at(j).filter(|q| matches!(q, b'"' | b'\'')) {
                    j += 1;
                    let v = j;
                    while at(j).is_some_and(|c| c != q) {
                        j += 1;
                    }
                    value = html.get(v..j).unwrap_or("").to_owned();
                    j += 1;
                } else {
                    let v = j;
                    while at(j).is_some_and(|c| !c.is_ascii_whitespace() && c != b'>') {
                        j += 1;
                    }
                    value = html.get(v..j).unwrap_or("").to_owned();
                }
            }
            if attr.starts_with("on") {
                out.push(format!("event handler {attr} on <{name}>"));
            }
            // The one control is the checkbox, whose `type` and `disabled` the sanitizer forces; a table cell's
            // alignment is the Markdown writer's `style`, which the CSP blocks (and the sanitizer drops).
            let allowed = attribute_is_known(&name, &attr)
                || (name == "input" && matches!(attr.as_str(), "type" | "disabled" | "checked"));
            if !allowed {
                out.push(format!("attribute {attr} on <{name}>"));
            }
            if attr == "class" {
                for c in value.split_ascii_whitespace() {
                    if !class_is_known(c) {
                        out.push(format!("class {c}"));
                    }
                }
            }
            if matches!(attr.as_str(), "href" | "src") {
                let v = value.trim().to_ascii_lowercase();
                if ["javascript:", "data:", "vbscript:"]
                    .iter()
                    .any(|scheme| v.starts_with(scheme))
                {
                    out.push(format!("address {value}"));
                }
            }
        }
        i = j + 1;
    }
    out
}
