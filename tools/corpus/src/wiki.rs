//! Wiki-shaped text and the `wiki` corpus.

use std::path::PathBuf;

use crate::Opts;
use crate::prikk::Prikk;

/// A small deterministic generator (xorshift64*): the corpus is reproducible from its seed.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        let i = usize::try_from(self.below(items.len() as u64)).unwrap_or(0);
        items.get(i)
    }
}

const WORDS: &[&str] = &[
    "page",
    "history",
    "change",
    "proposal",
    "review",
    "accept",
    "signed",
    "verify",
    "view",
    "reader",
    "writer",
    "section",
    "heading",
    "link",
    "block",
    "anchor",
    "title",
    "frontmatter",
    "license",
    "language",
    "wiki",
    "knowledge",
    "inclusive",
    "merge",
    "conflict",
    "patch",
    "repository",
    "seal",
    "author",
    "maintainer",
    "trust",
    "key",
    "pin",
    "floor",
    "sandbox",
    "ceiling",
    "budget",
    "admission",
    "cache",
    "index",
    "search",
    "tag",
    "callout",
    "directive",
    "embed",
    "attachment",
    "diagram",
    "glossary",
    "help",
    "home",
    "guide",
    "編集",
    "競合",
    "知識",
    "共有",
    "ページ",
    "履歴",
    "提案",
    "承認",
    "署名",
    "検証",
];

/// Roughly `bytes` of prose in paragraphs, with links to `titles` and a block id per paragraph.
pub fn prose(rng: &mut Rng, bytes: usize, titles: &[String], page: usize) -> String {
    let mut out = String::with_capacity(bytes + 256);
    let mut section = 1;
    let mut para = 1;
    while out.len() < bytes {
        if para % 3 == 1 {
            out.push_str(&format!("\n## Section {section}\n\n"));
            section += 1;
        }
        let words = 40 + rng.below(60);
        for w in 0..words {
            if w > 0 {
                out.push(' ');
            }
            if w % 23 == 7
                && let Some(t) = rng.pick(titles)
            {
                out.push_str(&format!("[[{t}]]"));
            } else if let Some(word) = rng.pick(WORDS) {
                out.push_str(word);
            }
        }
        out.push_str(&format!(". ^p{page}-{para}\n\n"));
        para += 1;
    }
    out
}

/// prikk refuses a repository path longer than 1024 bytes (measured on 0.54.1: "invalid name: repository
/// path exceeds 1024 bytes"), so the deepest chain of one-character components under `pages/` with `.md`
/// is about 500 levels; the generator caps the chain there and the report records the limit.
pub const MAX_DEPTH: usize = 500;

/// The title of page `i`: most are flat, every 10th is in a two-level hierarchy, and pages
/// `i < depth` form one chain `d/d/d/…` of `i + 1` one-character components.
pub fn title(i: usize, depth: usize) -> String {
    if i < depth.min(MAX_DEPTH) {
        let parts: Vec<&str> = (0..=i).map(|_| "d").collect();
        parts.join("/")
    } else if i.is_multiple_of(10) {
        format!("Guides/Topic {i}")
    } else {
        format!("Page {i}")
    }
}

pub fn page_path(title: &str) -> PathBuf {
    PathBuf::from(format!("pages/{title}.md"))
}

pub fn page_text(rng: &mut Rng, i: usize, t: &str, bytes: usize, titles: &[String]) -> String {
    let id = format!("019a0000-0000-7000-8000-{i:012x}");
    let body = prose(rng, bytes, titles, i);
    format!(
        "---\ntitle: {t}\nid: {id}\ntags: [corpus, group{}]\n---\n{body}",
        i % 7
    )
}

/// The `wiki` subcommand.
pub fn build_wiki(opts: &Opts) -> Result<(), String> {
    let out = opts.path("out")?;
    let keys = opts.path("keys")?;
    let pages = usize::try_from(opts.num("pages", Some(1000))?).unwrap_or(1000);
    let total = usize::try_from(opts.num("total-bytes", Some(1 << 20))?).unwrap_or(1 << 20);
    let depth = usize::try_from(opts.num("depth", Some(0))?)
        .unwrap_or(0)
        .min(pages);
    let blocks = usize::try_from(opts.num("blocks", Some(4))?)
        .unwrap_or(4)
        .max(1);
    let seed = opts.num("seed", Some(1))?;
    if out.exists() {
        return Err(format!("{} exists; refusing to overwrite", out.display()));
    }
    let prikk = Prikk::with_home(&keys)?;
    prikk.setup(&out)?;
    std::fs::write(
        out.join("wiki-bundle.yaml"),
        "format: portable-wiki-bundle\nversion: '0.4'\ngenerator:\n  name: inklkunn-corpus\nlang: en\nlicense: CC-BY-SA-4.0\n",
    )
    .map_err(|e| e.to_string())?;
    let mut rng = Rng::new(seed);
    let titles: Vec<String> = (0..pages).map(|i| title(i, depth)).collect();
    let per_page = (total / pages.max(1)).max(200);
    let per_block = pages.div_ceil(blocks);
    let mut written = 0usize;
    for b in 0..blocks {
        let start = b * per_block;
        let end = ((b + 1) * per_block).min(pages);
        for i in start..end {
            let Some(t) = titles.get(i) else {
                continue;
            };
            let p = out.join(page_path(t));
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let text = page_text(&mut rng, i, t, per_page, &titles);
            written += text.len();
            std::fs::write(&p, text).map_err(|e| e.to_string())?;
        }
        if end > start {
            let tip = prikk.commit_and_seal(
                &out,
                &format!("Corpus block {} of {blocks}: pages {start}..{end}", b + 1),
            )?;
            eprintln!(
                "sealed block {} ({tip}) — {written} bytes of pages so far",
                b + 1
            );
        }
    }
    // One editing block, so `diff` and `show` have content changes to report.
    for i in (0..pages).step_by((pages / 20).max(1)) {
        let Some(t) = titles.get(i) else {
            continue;
        };
        let p = out.join(page_path(t));
        let text = std::fs::read_to_string(&p).map_err(|e| e.to_string())?;
        let edited = text.replacen(
            "## Section 1\n",
            "## Section 1 (revised)\n\nAn inserted paragraph for the editing block. ^edit\n",
            1,
        );
        std::fs::write(&p, edited).map_err(|e| e.to_string())?;
    }
    let tip = prikk.commit_and_seal(&out, "Corpus editing block: revise one page in twenty")?;
    let store = Prikk::store_size(&out);
    println!(
        "{{\"repo\": \"{}\", \"pages\": {pages}, \"page_bytes\": {written}, \"store_bytes\": {store}, \"depth\": {depth}, \"blocks\": {}, \"tip\": \"{tip}\"}}",
        out.display(),
        blocks + 1
    );
    Ok(())
}
