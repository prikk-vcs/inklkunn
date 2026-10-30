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
    Change, Diff, ExtractBounds, FileMeta, History, HistoryBlock, Materialized, Point, Result,
    Storage, StorageError, Tree, Verification, View, Views,
};
use inklkunn_store::{DisplayName, MemoryStore, NewWiki, RootLayout, Slug, WikiRecord, WikiStore};

use super::{Calls, CountingStorage, Presentation, ReadConfig, ReadService};

#[derive(Default)]
struct State {
    files: BTreeMap<String, Vec<u8>>,
    /// Moves whenever a file does: the view's RefState id.
    version: u64,
    /// The files at every version, so that a read pinned to a block sees that block's files.
    snapshots: BTreeMap<u64, BTreeMap<String, Vec<u8>>>,
    /// A file change applied right after the next `branch list` answers (the view moves between two calls).
    move_after_views: Option<(String, Vec<u8>)>,
    extra_views: Vec<(String, u64)>,
    fail_views: Option<StorageError>,
    fail_materialize: Option<StorageError>,
    fail_read: Option<StorageError>,
    delay: Duration,
    /// The message sealed with each version's block (T5): `history` lists one block per version.
    messages: BTreeMap<u64, String>,
    /// What `show` answers for a block id.
    shows: BTreeMap<String, Result<Change>>,
    /// What `diff` answers, and every `diff` asked.
    diff: Option<Result<Diff>>,
    diffs: Vec<(Point, Point, Vec<String>)>,
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
        {
            let mut s = fake.state();
            s.version = 1;
            let files = s.files.clone();
            s.snapshots.insert(1, files);
        }
        fake
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Write a file and move the tip.
    pub(crate) fn set(&self, path: &str, text: &str) {
        self.set_bytes(path, text.as_bytes());
    }

    /// Write several files in one block: the tip moves once.
    pub(crate) fn set_files(&self, files: &[(String, String)]) {
        let mut s = self.state();
        for (path, text) in files {
            s.files.insert(path.clone(), text.as_bytes().to_vec());
        }
        s.version += 1;
        let (version, files) = (s.version, s.files.clone());
        s.snapshots.insert(version, files);
    }

    /// Write some files and remove others in one block: the tip moves once (a change that moves a page).
    pub(crate) fn change_files(&self, write: &[(String, String)], remove: &[&str]) {
        let mut s = self.state();
        for (path, text) in write {
            s.files.insert(path.clone(), text.as_bytes().to_vec());
        }
        for path in remove {
            s.files.remove(*path);
        }
        s.version += 1;
        let (version, files) = (s.version, s.files.clone());
        s.snapshots.insert(version, files);
    }

    pub(crate) fn set_bytes(&self, path: &str, bytes: &[u8]) {
        let mut s = self.state();
        s.files.insert(path.to_owned(), bytes.to_vec());
        s.version += 1;
        let (version, files) = (s.version, s.files.clone());
        s.snapshots.insert(version, files);
    }

    /// Write this file right after the next `branch list` has answered: the view moves between a request's
    /// first call and its second.
    pub(crate) fn move_after_next_views(&self, path: &str, text: &str) {
        self.state().move_after_views = Some((path.to_owned(), text.as_bytes().to_vec()));
    }

    /// The block the tip is at.
    pub(crate) fn block_id(&self) -> String {
        block_id(self.state().version)
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

    /// The message the block of `version` was sealed with.
    pub(crate) fn say(&self, version: u64, message: &str) {
        self.state().messages.insert(version, message.to_owned());
    }

    /// What `show` answers for the block of `version`.
    pub(crate) fn show(&self, version: u64, answer: Result<Change>) {
        self.state().shows.insert(block_id(version), answer);
    }

    /// What `diff` answers.
    pub(crate) fn diff_says(&self, answer: Result<Diff>) {
        self.state().diff = Some(answer);
    }

    /// Every `diff` asked: from, to and the paths.
    pub(crate) fn diffs_asked(&self) -> Vec<(Point, Point, Vec<String>)> {
        self.state().diffs.clone()
    }
}

pub(crate) fn state_id(n: u64) -> String {
    format!("{n:064x}")
}

pub(crate) fn block_id(n: u64) -> String {
    format!("{:064x}", 0xb10c_0000 + n)
}

/// The files a point names: the block's own files for a block, the current ones for a view.
fn files_at(s: &State, point: &Point) -> Option<BTreeMap<String, Vec<u8>>> {
    match point {
        Point::View(_) => Some(s.files.clone()),
        Point::Block(id) => s
            .snapshots
            .iter()
            .find(|(version, _)| block_id(**version) == *id)
            .map(|(_, files)| files.clone())
            // An extra view shares the files of the fake (it has its own state, not its own content).
            .or_else(|| {
                s.extra_views
                    .iter()
                    .any(|(_, n)| block_id(1_000_000 + n) == *id)
                    .then(|| s.files.clone())
            }),
    }
}

fn unsupported(verb: &'static str) -> StorageError {
    StorageError::Driver {
        verb,
        detail: "the in-memory repository does not implement this verb".to_owned(),
    }
}

impl Storage for Fake {
    fn views(&self, _repo: &Path) -> Result<Views> {
        let answer = {
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
            Views {
                schema_version: "branch-list-v1".to_owned(),
                views,
            }
        };
        let pending = self.state().move_after_views.take();
        if let Some((path, bytes)) = pending {
            self.set_bytes(&path, &bytes);
        }
        Ok(answer)
    }

    fn tree(&self, _: &Path, point: &Point, prefix: Option<&str>) -> Result<Tree> {
        // Only a block's files are listed (the acceptance's merge asks); a view's are not needed by any read.
        if matches!(point, Point::View(_)) {
            return Err(unsupported("tree"));
        }
        let s = self.state();
        let files = files_at(&s, point).ok_or_else(|| StorageError::Refused {
            verb: "tree",
            message: format!("no such block: {point}"),
            kind: None,
            reason: None,
        })?;
        Ok(Tree {
            schema_version: "tree-listing-v1".to_owned(),
            point: point.to_string(),
            target_block_id: point.to_string(),
            prefix: prefix.map(str::to_owned),
            entries: files
                .iter()
                .filter(|(path, _)| prefix.is_none_or(|p| path.starts_with(p)))
                .map(|(path, bytes)| inklkunn_prikk::Entry {
                    path: path.clone(),
                    kind: "file".to_owned(),
                    encoding: Some("text".to_owned()),
                    mode: None,
                    size: Some(bytes.len() as u64),
                })
                .collect(),
        })
    }

    fn stat(&self, _: &Path, point: &Point, path: &str) -> Result<FileMeta> {
        // Only a block's files are described (a permalink asks); a view's are not needed by any read.
        if matches!(point, Point::View(_)) {
            return Err(unsupported("cat"));
        }
        let s = self.state();
        let files = files_at(&s, point).ok_or_else(|| StorageError::Refused {
            verb: "cat",
            message: format!("no such block: {point}"),
            kind: None,
            reason: None,
        })?;
        match files.get(path) {
            Some(bytes) => Ok(FileMeta {
                schema_version: "path-content-v1".to_owned(),
                point: point.to_string(),
                target_block_id: point.to_string(),
                path: path.to_owned(),
                kind: "file".to_owned(),
                encoding: Some("text".to_owned()),
                mode: None,
                size: bytes.len() as u64,
            }),
            None => Err(StorageError::Refused {
                verb: "cat",
                message: format!("path {path} not found"),
                kind: None,
                reason: None,
            }),
        }
    }

    fn read(&self, _repo: &Path, point: &Point, path: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let s = self.state();
        if let Some(e) = &s.fail_read {
            return Err(e.clone());
        }
        let files = files_at(&s, point).ok_or_else(|| StorageError::Refused {
            verb: "cat",
            message: format!("no such block: {point}"),
            kind: None,
            reason: None,
        })?;
        match files.get(path) {
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

    fn history(&self, _: &Path, view: &Point, limit: Option<u64>) -> Result<History> {
        let s = self.state();
        let Point::View(name) = view else {
            return Err(unsupported("log"));
        };
        let n = match name.as_str() {
            "heads/main" => s.version,
            other => match s.extra_views.iter().find(|(v, _)| v == other) {
                Some((_, n)) => 1_000_000 + n,
                None => {
                    return Err(StorageError::Refused {
                        verb: "log",
                        message: format!("no such view: {other}"),
                        kind: None,
                        reason: None,
                    });
                }
            },
        };
        if n == s.version && name == "heads/main" {
            // One block per version, newest first, each with the message it was sealed with.
            let limit = limit.map_or(1_000, |l| usize::try_from(l).unwrap_or(usize::MAX));
            let blocks = (1..=n)
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
                            vec![inklkunn_prikk::PatchMessage {
                                patch_id: format!("{v:064x}"),
                                message: m.clone(),
                            }]
                        })
                        .unwrap_or_default(),
                    previous_ref_state_id: (v > 1).then(|| state_id(v - 1)),
                })
                .collect();
            return Ok(History {
                schema_version: "log-report-v1".to_owned(),
                view: name.clone(),
                blocks,
            });
        }
        Ok(History {
            schema_version: "log-report-v1".to_owned(),
            view: name.clone(),
            blocks: vec![HistoryBlock {
                block_id: block_id(n),
                ref_state_id: state_id(n),
                update_seq: n,
                kind: "Normal".to_owned(),
                rollback_block: false,
                patch_count: 1,
                patch_messages: Vec::new(),
                previous_ref_state_id: None,
            }],
        })
    }

    fn change(&self, _: &Path, id: &str) -> Result<Change> {
        self.state()
            .shows
            .get(id)
            .cloned()
            .unwrap_or_else(|| Err(unsupported("show")))
    }

    fn diff(&self, _: &Path, from: &Point, to: &Point, paths: &[&str]) -> Result<Diff> {
        let mut s = self.state();
        s.diffs.push((
            from.clone(),
            to.clone(),
            paths.iter().map(|p| (*p).to_owned()).collect(),
        ));
        s.diff.clone().unwrap_or_else(|| Err(unsupported("diff")))
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
            let files = files_at(&s, point).ok_or_else(|| StorageError::Refused {
                verb: "extract",
                message: format!("no such block: {point}"),
                kind: None,
                reason: None,
            })?;
            (files, s.fail_materialize.clone(), s.delay)
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
