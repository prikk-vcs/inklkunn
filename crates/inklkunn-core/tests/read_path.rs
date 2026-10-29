//! The read path against a real repository (handoff 003 T3): a wiki created and written by the bootstrap
//! signer, read through the real driver, under both sandbox policies where bubblewrap works. The title index
//! comes from a real `extract`, the numbers of prikk calls are counted at the driver, and prikk's own
//! refusal for want of its checkpoint record is met as prikk prints it. Self-skips with a visible line when
//! prikk is not on `PATH`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use inklkunn_core::bundle::HELP_PAGE;
use inklkunn_core::read::{
    Cause, Lookup, PageOutcome, PageView, ReadConfig, ReadError, RedirectedBy,
};
use inklkunn_core::{
    Calls, CountingStorage, Presentation, ReadService, create_wiki, system_message,
};
use inklkunn_markup::TransclusionRefusal;
use inklkunn_prikk::sandbox::probe_bwrap;
use inklkunn_prikk::{Admission, PrikkCli, Sandbox, SizeHints, cost};
use inklkunn_signer::{BootstrapSigner, RepositoryBootstrap};
use inklkunn_store::{
    DisplayName, MemoryStore, NewWiki, PrikkVersion, RootLayout, Slug, WikiRecord, WikiStore,
};

fn prikk_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join("prikk").is_file()))
        .unwrap_or(false)
}

/// Both policies when bubblewrap works (GitHub's runners cannot confine and take the unconfined one).
fn policies() -> Vec<Sandbox> {
    if probe_bwrap(std::ffi::OsStr::new("bwrap")).is_ok() {
        println!("policies: confined (bubblewrap) and unconfined");
        vec![Sandbox::default(), Sandbox::Unconfined]
    } else {
        println!("skipping: bubblewrap cannot confine a process here; running unconfined");
        vec![Sandbox::Unconfined]
    }
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp(tag: &str) -> TempDir {
    let p = std::env::temp_dir().join(format!("inklkunn-readpath-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp");
    TempDir(p)
}

/// English under `/w`.
struct En;

impl Presentation for En {
    fn lang(&self) -> &str {
        "en"
    }
    fn key(&self) -> String {
        "/w".to_owned()
    }
    fn page_href(&self, title: &str) -> String {
        format!("/w/{}", inklkunn_markup::percent_encode(title))
    }
    fn create_href(&self, title: &str) -> Option<String> {
        Some(format!(
            "/w/-/create?title={}",
            inklkunn_markup::percent_encode(title)
        ))
    }
    fn transclusion_label(&self, title: &str) -> String {
        format!("From {title}")
    }
    fn transclusion_notice(&self, refusal: &TransclusionRefusal) -> String {
        format!("Not included: {refusal}")
    }
}

struct Real {
    _tmp: TempDir,
    layout: RootLayout,
    store: Arc<MemoryStore>,
    counting: Arc<CountingStorage<PrikkCli>>,
    hints: SizeHints,
    signer: BootstrapSigner,
    record: WikiRecord,
    repo: PathBuf,
    service: ReadService,
}

impl Real {
    fn new(tag: &str, sandbox: Sandbox) -> Real {
        let tmp = temp(tag);
        let layout = RootLayout::new(tmp.0.join("root"));
        let hints = SizeHints::new();
        let driver = PrikkCli::open(
            "prikk",
            sandbox.clone(),
            Admission::new(1 << 30, 64 << 20, Duration::from_millis(500)),
        )
        .expect("prikk at the floor")
        .with_size_hints(hints.clone());
        let counting = Arc::new(CountingStorage::new(driver));
        let signer =
            BootstrapSigner::new("prikk", sandbox, tmp.0.join("keys")).expect("the signer");
        let store = Arc::new(MemoryStore::new());
        let created = create_wiki(
            &*store,
            &layout,
            &signer,
            counting.inner(),
            NewWiki::new(
                Slug::new("real").expect("slug"),
                DisplayName::new("Real").expect("name"),
            ),
            "0.0.0",
            &inklkunn_core::KeyAuthority::for_tests(),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        let service = ReadService::new(
            counting.clone(),
            store.clone(),
            store.clone(),
            layout.clone(),
            ReadConfig {
                tip_ttl: Duration::from_secs(3600),
                ..ReadConfig::default()
            },
            Some(hints.clone()),
        );
        Real {
            _tmp: tmp,
            layout,
            store,
            counting,
            hints,
            signer,
            record: created.record,
            repo: created.repo,
            service,
        }
    }

    /// Write pages into the worktree and seal them as one system change.
    fn add(&self, pages: &[(&str, &[&str], &str)], summary: &str) {
        for (title, front, body) in pages {
            let path = inklkunn_markup::title_to_path(title).expect("path");
            let mut text = format!("---\ntitle: {title}\n");
            for line in *front {
                text.push_str(line);
                text.push('\n');
            }
            text.push_str("---\n\n");
            text.push_str(body);
            let at = self.repo.join(&path);
            std::fs::create_dir_all(at.parent().expect("parent")).expect("dirs");
            std::fs::write(at, text).expect("write");
        }
        self.signer
            .commit_and_seal(&self.repo, &system_message(summary))
            .unwrap_or_else(|e| panic!("{e}"));
    }

    fn page(&self, title: &str) -> PageView {
        match self
            .service
            .page(&self.record, None, title, &En)
            .unwrap_or_else(|e| panic!("{title}: {e}"))
        {
            PageOutcome::Found(v) => *v,
            other => panic!("{title}: {other:?}"),
        }
    }

    fn calls(&self) -> Calls {
        self.counting.calls()
    }

    fn counted<T>(&self, f: impl FnOnce() -> T) -> (T, Calls) {
        let before = self.calls();
        let out = f();
        (out, self.calls().since(&before))
    }
}

#[test]
fn a_real_wiki_is_read_through_the_whole_path_with_the_numbers_of_calls_the_design_says() {
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    for (n, sandbox) in policies().into_iter().enumerate() {
        let w = Real::new(&format!("full-{n}"), sandbox);
        w.add(
            &[
                (
                    "Home",
                    &["description: The front page"],
                    "Welcome. See [[Guide]], [[How-to]], [[Old Name]] and [[Nowhere]]. ^intro\n\n![[Glossary]]\n\n\
                     <script>alert(1)</script><img src=x onerror=alert(2)>\n\n[click](javascript:alert(3))\n",
                ),
                ("Guide", &["aliases: [How-to]"], "The guide.\n"),
                ("Old Name", &["kind: redirect", "redirect: Guide"], ""),
                ("Glossary", &[], "# Terms\n\nA *term*.\n"),
                ("日本語", &[], "日本語のページ。\n"),
            ],
            "Add pages",
        );

        // The first view: the tip (the list, and one log for its block), one extract of it, the page, and the
        // page it includes. Every read is pinned to the block.
        let (home, cold) = w.counted(|| w.page("Home"));
        assert!(!home.cached);
        assert_eq!(cold.views, 1, "{cold:?}");
        assert_eq!(cold.history, 1, "the block of the tip: {cold:?}");
        assert_eq!(cold.materialize, 1, "the index is one extract: {cold:?}");
        assert_eq!(cold.read, 2, "Home and the Glossary it includes: {cold:?}");
        assert_eq!(
            cold.tree + cold.stat + cold.diff + cold.verify,
            0,
            "{cold:?}"
        );

        // What the real repository said.
        assert_eq!(home.meta.title, "Home");
        assert_eq!(home.meta.description.as_deref(), Some("The front page"));
        let html = home.html.as_str();
        assert!(html.contains("<p id=\"^intro\">Welcome."), "{html}");
        assert!(html.contains("class=\"transclusion\""), "{html}");
        assert!(html.contains("<h2 id=\"t1-terms\">Terms</h2>"), "{html}");
        assert!(html.contains("wikilink-missing"), "{html}");
        assert_eq!(home.links.dangling, ["Nowhere"]);
        for attack in ["<script", "alert", "onerror", "javascript:"] {
            assert!(!html.contains(attack), "{attack} in {html}");
        }

        // A hit asks prikk for nothing.
        let (again, hit) = w.counted(|| w.page("Home"));
        assert!(again.cached);
        assert_eq!(hit, Calls::default());

        // A miss: one `cat`; the index is there and the tip is held.
        let (guide, miss) = w.counted(|| w.page("Guide"));
        assert_eq!((miss.read, miss.total()), (1, 1), "{miss:?}");
        assert_eq!(guide.meta.aliases, ["How-to"]);

        // An alias and a redirect answer with the page and say where from; both are cache hits.
        let (via_alias, c) = w.counted(|| w.page("How-to"));
        assert_eq!(via_alias.meta.title, "Guide");
        assert_eq!(
            via_alias.redirected_from.as_ref().map(|r| r.by),
            Some(RedirectedBy::Alias)
        );
        let via_redirect = w.page("Old Name");
        assert_eq!(via_redirect.meta.title, "Guide");
        assert_eq!(
            via_redirect.redirected_from.as_ref().map(|r| r.by),
            Some(RedirectedBy::Redirect)
        );
        assert_eq!(c.total(), 0);

        // The page the creation wrote is in the index too, with its id, and a title in another script works.
        let help = w.service.lookup(&w.record, None, "Help").expect("lookup");
        match help {
            Lookup::Found(f) => {
                assert_eq!(f.entry.path, HELP_PAGE);
                assert!(f.entry.page_id.is_some());
            }
            other => panic!("{other:?}"),
        }
        assert!(w.page("日本語").html.as_str().contains("日本語のページ"));

        // The driver was told the store's size once, and it is the size of the store.
        assert_eq!(w.hints.get(&w.repo), cost::store_size(&w.repo).ok());

        // The scratch directory holds nothing after the build.
        let scratch = w.layout.scratch_dir(&w.record.id);
        assert_eq!(std::fs::read_dir(&scratch).expect("scratch").count(), 0);

        // The tip moves with a seal. The service forgets what it held (the write path will say so), and the
        // next view is at the new tip: a new index, the new text, and nothing left of the old tip.
        let before: Vec<_> = {
            let tip = w.service.tip(&w.record, None).expect("tip");
            vec![tip.state_id]
        };
        w.add(
            &[("Guide", &["aliases: [How-to]"], "The guide, revised.\n")],
            "Revise the guide",
        );
        w.service.forget_wiki(&w.record);
        let (revised, moved) = w.counted(|| w.page("Guide"));
        assert!(revised.html.as_str().contains("revised"));
        assert_eq!(moved.materialize, 1, "a new tip is a new index: {moved:?}");
        assert_ne!(
            revised.tip.state_id,
            before.first().cloned().unwrap_or_default()
        );
        let (_, other) = w.counted(|| w.page("Home"));
        assert_eq!(other.materialize, 0);
        assert_eq!(
            other.read, 2,
            "Home is re-rendered at the new tip, with its Glossary: {other:?}"
        );

        // ENF-5 against the real repository: clear everything derived, rebuild, compare.
        let titles = ["Home", "Guide", "Glossary", "日本語", "Help"];
        let first: Vec<PageView> = titles.iter().map(|t| w.page(t)).collect();
        w.service.clear_derived(&w.record).expect("clear");
        let rebuilt: Vec<PageView> = titles.iter().map(|t| w.page(t)).collect();
        for (a, b) in first.iter().zip(&rebuilt) {
            assert_eq!(a.html, b.html, "{}", a.meta.title);
            assert_eq!(a.meta, b.meta);
            assert_eq!(a.links, b.links);
            assert_eq!(a.toc, b.toc);
            assert!(!b.cached);
        }
    }
}

#[test]
fn prikks_own_refusal_for_want_of_its_checkpoint_record_is_the_ordinary_path_and_a_refresh_ends_it()
{
    if !prikk_on_path() {
        println!("skipping: prikk not on PATH");
        return;
    }
    let sandbox = Sandbox::Unconfined;
    let w = Real::new("checkpoint", sandbox);
    // A history beyond what `extract` will anchor without a record: 63 blocks and some.
    for i in 0..66 {
        w.add(
            &[(&format!("Page {i}"), &[], "x\n")],
            &format!("Add page {i}"),
        );
    }
    assert!(
        w.page("Page 3").html.as_str().contains('x'),
        "readable while the record is there"
    );

    // The record is lost (a prikk upgrade, a restore, a cleared cache).
    let record_file = w.repo.join(".prikk/cache/replay-verified-blocks.v1");
    assert!(record_file.is_file(), "prikk keeps its checkpoint here");
    std::fs::remove_file(&record_file).expect("remove the checkpoint record");
    w.service.clear_derived(&w.record).expect("clear");

    // The refusal is prikk's, parsed by the driver; the read path calls it by its name, keeps the reason
    // for the operator and flags the wiki.
    let e = w
        .service
        .page(&w.record, None, "Page 3", &En)
        .expect_err("refused");
    match &e {
        ReadError::CouldNotBeReadNow(Cause::CheckpointNotRecorded { refresh_requested }) => {
            assert!(refresh_requested, "{e}");
        }
        other => panic!("{other:?}"),
    }
    let record = w.store.get(&w.record.id).expect("record");
    assert!(record.checkpoint.refresh_requested_at.is_some());
    assert!(
        record
            .checkpoint
            .needs_refresh(&PrikkVersion::parse("prikk 0.54.1").expect("version"))
    );
    // `cat` does not need the record: the pages themselves are still readable by other means, which is why
    // only the index is unavailable.
    let tip = w.service.tip(&w.record, None);
    assert!(tip.is_ok(), "the tip does not need it either");

    // The checkpoint is re-recorded by the next seal (prikk's `seal` and a writable `verify` write it; ledger
    // PK-24), which is how a wiki that is written to heals itself; handoff 003 T7's refresh does the same
    // without waiting for a write, in the signer's keyless writable mode.
    w.add(
        &[("After the loss", &[], "y\n")],
        "Add a page after the record was lost",
    );
    assert!(
        record_file.is_file(),
        "the seal wrote prikk's checkpoint record again"
    );
    w.store
        .record_checkpoint_refresh(
            &w.record.id,
            &PrikkVersion::parse("prikk 0.54.1").expect("version"),
        )
        .expect("recorded");
    w.service.forget_wiki(&w.record);
    let page = w.page("Page 3");
    assert!(page.html.as_str().contains('x'));
    assert_eq!(
        w.store
            .get(&w.record.id)
            .expect("record")
            .checkpoint
            .refresh_requested_at,
        None,
        "answered"
    );
}
