//! Shared helpers: a temporary directory, a scripted fetcher, a fake clock.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use crate::claims::events::{Event, Kind};
use crate::claims::wire::{Clock, Fetcher};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A directory under the system temp directory, removed on drop.
pub struct Tmp(PathBuf);

impl Tmp {
    pub fn new(tag: &str) -> Tmp {
        let dir = std::env::temp_dir().join(format!(
            "inklkunn-corpus-test-{}-{tag}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Tmp(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Tmp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

type Handler = Box<dyn FnMut(&str) -> Result<String, String>>;

/// A fetcher that answers from a closure and remembers the URLs it was asked.
pub struct Scripted {
    pub urls: Vec<String>,
    handler: Handler,
}

impl Scripted {
    pub fn new(handler: impl FnMut(&str) -> Result<String, String> + 'static) -> Scripted {
        Scripted {
            urls: Vec::new(),
            handler: Box::new(handler),
        }
    }
}

impl Fetcher for Scripted {
    fn fetch(&mut self, url: &str) -> Result<String, String> {
        self.urls.push(url.to_owned());
        (self.handler)(url)
    }
}

/// A clock that only moves when told, and records how long it was asked to sleep (the record is shared, so a
/// test can read it after the clock moved into a limited fetcher).
#[derive(Default, Clone)]
pub struct FakeClock {
    pub now: Duration,
    pub slept: Rc<RefCell<Vec<Duration>>>,
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.now
    }

    fn sleep(&mut self, d: Duration) {
        self.slept.borrow_mut().push(d);
        self.now += d;
    }
}

/// The value of a query parameter of a URL, percent-decoded.
pub fn param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| decode(v))
    })
}

pub fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3).unwrap()).unwrap();
            out.push(u8::from_str_radix(hex, 16).unwrap());
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap()
}

/// An event of a made-up history (metrics are placeholders; the tests that use them do not read them).
pub fn event(
    kind: Kind,
    lang: &str,
    article: &str,
    rev: u64,
    blocks: (Option<usize>, Option<usize>),
    id_before: Option<&str>,
) -> Event {
    Event {
        kind,
        lang: lang.to_owned(),
        article: article.to_owned(),
        rev,
        parent_rev: rev - 1,
        prev_block: blocks.0,
        new_block: blocks.1,
        id_before: id_before.map(str::to_owned),
        id_after: id_before.map(str::to_owned),
        id_survived: kind == Kind::Changed,
        chars_changed: 5,
        words_changed: 1,
        unrefined: false,
        text_differs: true,
        others_changed_in_revision: 0,
        is_revert: false,
    }
}
