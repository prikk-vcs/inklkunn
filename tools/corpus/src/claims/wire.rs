//! Talking to the wiki's public API: the requests (pure functions), the fetcher (`curl` as a subprocess, one
//! request per second at most) and the reading of the answers.
//!
//! **No personal data is asked for.** `rvprop` is exactly `ids` for the plan and `ids|content` for the fetch:
//! no user, user id, comment, parsed comment, tags, timestamp or IP is ever requested, so none can be kept.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::check_lang;

/// The user agent every request states (the project and a contact).
pub const USER_AGENT: &str = "inklkunn-corpus/0.1 (spike C, RFC 005; contact: nabbisen@scqr.net)";
/// The most revision ids one request names.
pub const MAX_BATCH: usize = 50;
/// The most revisions one plan request asks for (the API's limit for an ordinary client).
pub const MAX_PLAN_LIMIT: usize = 500;
/// `rvprop` of the plan: ids only.
pub const PLAN_RVPROP: &str = "ids";
/// `rvprop` of the fetch: ids and the text.
pub const GET_RVPROP: &str = "ids|content";
/// The most pages of an answer followed for one request.
const MAX_CONTINUES: usize = 200;

/// Percent-encode a query component (unreserved characters stay).
pub fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The API URL of a language with the given query parameters.
pub fn api_url(lang: &str, params: &[(String, String)]) -> Result<String, String> {
    check_lang(lang)?;
    let query: Vec<String> = params
        .iter()
        .map(|(k, v)| format!("{}={}", pct_encode(k), pct_encode(v)))
        .collect();
    Ok(format!(
        "https://{lang}.wikipedia.org/w/api.php?{}",
        query.join("&")
    ))
}

fn p(k: &str, v: impl Into<String>) -> (String, String) {
    (k.to_owned(), v.into())
}

/// The parameters of a plan request: the newest `limit` revisions' ids of one title, newest first, resuming at
/// `cont` (the previous answer's `continue` members).
pub fn plan_params(title: &str, limit: usize, cont: &[(String, String)]) -> Vec<(String, String)> {
    let mut v = vec![
        p("action", "query"),
        p("prop", "revisions"),
        p("titles", title),
        p("rvprop", PLAN_RVPROP),
        p("rvdir", "older"),
        p("rvlimit", limit.min(MAX_PLAN_LIMIT).to_string()),
        p("format", "json"),
        p("formatversion", "2"),
    ];
    v.extend(cont.iter().cloned());
    v
}

/// The parameters of a fetch request for the named revisions (at most [`MAX_BATCH`]).
pub fn get_params(revids: &[u64], cont: &[(String, String)]) -> Vec<(String, String)> {
    let ids: Vec<String> = revids.iter().take(MAX_BATCH).map(u64::to_string).collect();
    let mut v = vec![
        p("action", "query"),
        p("prop", "revisions"),
        p("revids", ids.join("|")),
        p("rvprop", GET_RVPROP),
        p("rvslots", "main"),
        p("format", "json"),
        p("formatversion", "2"),
    ];
    v.extend(cont.iter().cloned());
    v
}

// --------------------------------------------------------------------------------------------- fetching

/// Something that answers a URL with a body. Tests use a scripted fake.
pub trait Fetcher {
    /// GET `url`; the body, or why not.
    fn fetch(&mut self, url: &str) -> Result<String, String>;
}

/// The real fetcher: `curl` run as a subprocess, so no HTTP or TLS crate enters the lock.
pub struct Curl;

impl Fetcher for Curl {
    fn fetch(&mut self, url: &str) -> Result<String, String> {
        let out = crate::sh(
            Path::new("."),
            &[],
            "curl",
            &["-sS", "--max-time", "60", "--fail", "-A", USER_AGENT, url],
        )?;
        String::from_utf8(out).map_err(|_| "the answer is not UTF-8".to_owned())
    }
}

/// A clock a rate limiter can be tested with.
pub trait Clock {
    /// Time since some fixed start.
    fn now(&self) -> Duration;
    /// Wait.
    fn sleep(&mut self, d: Duration);
}

/// The real clock.
pub struct SystemClock(Instant);

impl SystemClock {
    /// A clock starting now.
    pub fn new() -> Self {
        SystemClock(Instant::now())
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }

    fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// At most one request per `gap`.
pub struct RateLimiter {
    gap: Duration,
    last: Option<Duration>,
}

impl RateLimiter {
    /// A limiter that lets requests through `gap` apart.
    pub fn new(gap: Duration) -> Self {
        RateLimiter { gap, last: None }
    }

    /// Wait until a request may be made, then count it.
    pub fn wait(&mut self, clock: &mut impl Clock) {
        if let Some(last) = self.last {
            let since = clock.now().saturating_sub(last);
            if since < self.gap {
                clock.sleep(self.gap - since);
            }
        }
        self.last = Some(clock.now());
    }
}

/// A fetcher that waits for its limiter before every request.
pub struct Limited<F, C> {
    inner: F,
    clock: C,
    limiter: RateLimiter,
}

impl<F: Fetcher, C: Clock> Limited<F, C> {
    /// `inner` limited to one request per `gap` by `clock`.
    pub fn new(inner: F, clock: C, gap: Duration) -> Self {
        Limited {
            inner,
            clock,
            limiter: RateLimiter::new(gap),
        }
    }
}

impl<F: Fetcher, C: Clock> Fetcher for Limited<F, C> {
    fn fetch(&mut self, url: &str) -> Result<String, String> {
        self.limiter.wait(&mut self.clock);
        self.inner.fetch(url)
    }
}

/// The fetcher the commands use: `curl`, one request per second.
pub fn live_fetcher() -> Limited<Curl, SystemClock> {
    Limited::new(Curl, SystemClock::new(), Duration::from_secs(1))
}

// ------------------------------------------------------------------------------------------- the answers

fn parse(json: &str) -> Result<Value, String> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| format!("the answer is not JSON: {e}"))?;
    if let Some(err) = v.get("error") {
        let code = err.get("code").and_then(Value::as_str).unwrap_or("unknown");
        return Err(format!("the API refused the request ({code})"));
    }
    Ok(v)
}

/// The members of an answer's `continue`, as request parameters; `None` when there is nothing more.
fn continuation(v: &Value) -> Option<Vec<(String, String)>> {
    let map = v.get("continue")?.as_object()?;
    let mut out: Vec<(String, String)> = map
        .iter()
        .map(|(k, v)| {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), s)
        })
        .collect();
    out.sort();
    Some(out)
}

/// One page of a plan answer.
#[derive(Debug, PartialEq, Eq)]
pub struct PlanPage {
    /// `(revid, parentid)`, newest first.
    pub revisions: Vec<(u64, u64)>,
    /// The article does not exist.
    pub missing: bool,
    /// Resume parameters when there is more.
    pub next: Option<Vec<(String, String)>>,
}

/// Read a plan answer (`formatversion=2`).
pub fn parse_plan_page(json: &str) -> Result<PlanPage, String> {
    let v = parse(json)?;
    let page = v
        .get("query")
        .and_then(|q| q.get("pages"))
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .ok_or("the answer has no page")?;
    let missing = page
        .get("missing")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut revisions = Vec::new();
    for r in page
        .get("revisions")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
    {
        let id = r
            .get("revid")
            .and_then(Value::as_u64)
            .ok_or("a revision has no id")?;
        let parent = r.get("parentid").and_then(Value::as_u64).unwrap_or(0);
        revisions.push((id, parent));
    }
    Ok(PlanPage {
        revisions,
        missing,
        next: continuation(&v),
    })
}

/// What one fetch answer holds.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Batch {
    /// Revision id to wikitext.
    pub texts: BTreeMap<u64, String>,
    /// Ids the API reported missing, or whose text is hidden (deleted upstream).
    pub deleted: BTreeSet<u64>,
    /// Resume parameters when there is more.
    pub next: Option<Vec<(String, String)>>,
}

/// Read a fetch answer (`formatversion=2`, `rvslots=main`).
pub fn parse_batch(json: &str) -> Result<Batch, String> {
    let v = parse(json)?;
    let mut batch = Batch {
        next: continuation(&v),
        ..Batch::default()
    };
    let query = v.get("query");
    if let Some(bad) = query
        .and_then(|q| q.get("badrevids"))
        .and_then(Value::as_object)
    {
        for k in bad.keys() {
            if let Ok(id) = k.parse::<u64>() {
                batch.deleted.insert(id);
            }
        }
    }
    let pages = query
        .and_then(|q| q.get("pages"))
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    for page in pages {
        for r in page
            .get("revisions")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let Some(id) = r.get("revid").and_then(Value::as_u64) else {
                continue;
            };
            let main = r.get("slots").and_then(|s| s.get("main"));
            let hidden = r
                .get("texthidden")
                .and_then(Value::as_bool)
                .unwrap_or(false)
                || main
                    .and_then(|m| m.get("texthidden"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
            match main.and_then(|m| m.get("content")).and_then(Value::as_str) {
                Some(content) if !hidden => {
                    batch.texts.insert(id, content.to_owned());
                }
                _ => {
                    batch.deleted.insert(id);
                }
            }
        }
    }
    Ok(batch)
}

/// Fetch the named revisions (at most [`MAX_BATCH`]) following `continue` until the answer is complete.
pub fn fetch_batch(fetcher: &mut impl Fetcher, lang: &str, ids: &[u64]) -> Result<Batch, String> {
    let mut all = Batch::default();
    let mut cont: Vec<(String, String)> = Vec::new();
    for _ in 0..MAX_CONTINUES {
        let url = api_url(lang, &get_params(ids, &cont))?;
        let part = parse_batch(&fetcher.fetch(&url)?)?;
        all.texts.extend(part.texts);
        all.deleted.extend(part.deleted);
        match part.next {
            Some(next) => cont = next,
            None => {
                // An id that came back with text is not deleted, whatever an earlier page said.
                let texts: BTreeSet<u64> = all.texts.keys().copied().collect();
                all.deleted.retain(|d| !texts.contains(d));
                return Ok(all);
            }
        }
    }
    Err("the API kept continuing; stopped".to_owned())
}
