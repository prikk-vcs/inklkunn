//! What an endpoint does with a request before and after its own work: who is asking and in what language,
//! which wiki the slug names (the same existence-neutral start as the pages), the query checked against the
//! endpoint's own [`Param`]s, and the response built by [`reply::finish`].
//!
//! The order is the contract: **authorize first** (a wiki that is absent and a wiki the caller may not read
//! stop here with the same problem), **then** check the query, **then** read. A caller who may not read a wiki
//! therefore learns nothing from the way a parameter is refused.

use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use inklkunn_core::Principal;
use inklkunn_core::read::ApiError;
use inklkunn_core::read::ReadError;
use inklkunn_store::WikiRecord;

use super::describe::{Doc, Kind, Param};
use super::problem::{Code, Problem};
use super::reply::{self, Reply};
use crate::catalog::T;
use crate::lang;
use crate::pages::PageState;
use crate::services::Services;
use crate::url::decode;
use crate::wiki::{blocking, open};

/// The language of an API answer: the same negotiation as the pages (the language cookie, else
/// `Accept-Language`, else English; a preview only by the reader's own choice).
pub(crate) fn language(headers: &HeaderMap) -> T {
    T::new(lang::negotiate_from_headers(headers))
}

/// A problem answered where no endpoint ran (a method the address does not take, an address the API does not
/// have): a problem detail in the caller's language.
pub(crate) fn answer_plain(headers: &HeaderMap, code: Code) -> Response {
    reply::finish(language(headers), None, Err(Problem::new(code)))
}

/// What the part of the path after `/{slug}/-/api/v1/<endpoint>` is, raw.
#[derive(Debug, Clone)]
pub(crate) struct Ctx {
    pub t: T,
    /// Who is asking: the endpoints that list what several pages hold check each against it.
    pub principal: Principal,
    /// The path after the endpoint's word, as the address spells it (not decoded): a title or an id.
    pub rest: String,
    /// The query string, as sent.
    pub query: Option<String>,
}

impl Ctx {
    /// The title the address names, decoded.
    pub(crate) fn title(&self) -> Result<String, Problem> {
        decode(&self.rest).ok_or_else(|| Problem::new(Code::InvalidTitle))
    }
}

/// `/{slug}/-/api/v1/<endpoint>[/<rest>]`, split into the raw slug, the endpoint's word and the raw rest.
/// `None` when the path does not have that shape.
fn parse_path(path: &str) -> Option<(&str, &str, &str)> {
    let mut parts = path.strip_prefix('/')?.splitn(6, '/');
    let slug = parts.next()?;
    if (parts.next()?, parts.next()?, parts.next()?) != ("-", "api", "v1") {
        return None;
    }
    Some((slug, parts.next()?, parts.next().unwrap_or("")))
}

/// Run an endpoint's work for a request to a wiki's address.
///
/// `work` runs on a blocking thread (the stores and prikk block), only for a caller who may read the wiki, with
/// the query already checked against `doc`.
pub(crate) async fn serve<F>(
    state: PageState,
    headers: HeaderMap,
    uri: Uri,
    doc: &'static Doc,
    work: F,
) -> Response
where
    F: FnOnce(&Services, &WikiRecord, &Params, &Ctx) -> Result<Reply, Problem> + Send + 'static,
{
    let t = language(&headers);
    let conditional = reply::if_none_match(&headers);
    let outcome = run(state, headers, uri, doc, t, work).await;
    reply::finish(t, conditional.as_deref(), outcome)
}

async fn run<F>(
    state: PageState,
    headers: HeaderMap,
    uri: Uri,
    doc: &'static Doc,
    t: T,
    work: F,
) -> Result<Reply, Problem>
where
    F: FnOnce(&Services, &WikiRecord, &Params, &Ctx) -> Result<Reply, Problem> + Send + 'static,
{
    let not_found = || Problem::new(Code::NotFound);
    let services = state.services.ok_or_else(not_found)?;
    let (raw_slug, _word, raw_rest) = parse_path(uri.path()).ok_or_else(not_found)?;
    let slug = decode(raw_slug).ok_or_else(not_found)?;
    let (rest, query) = (raw_rest.to_owned(), uri.query().map(str::to_owned));
    let done = blocking(move || {
        // Existence-neutral: no wiki and no right to read it are the same answer, before anything else.
        let wiki = open(&services, &headers, &slug).ok_or_else(not_found)?;
        let ctx = Ctx {
            t,
            principal: services.identity.principal(&headers),
            rest,
            query,
        };
        let params = Params::parse(ctx.query.as_deref(), doc.query)?;
        work(&services, &wiki, &params, &ctx)
    })
    .await;
    done.unwrap_or_else(|| Err(Problem::new(Code::Failed)))
}

// ------------------------------------------------------------------------------------ the query

/// The query of a request, checked against the endpoint's parameters.
#[derive(Debug, Default)]
pub(crate) struct Params {
    values: Vec<(&'static str, String)>,
}

/// A query component: `+` is a space, `%XX` is a byte.
fn component(raw: &str) -> Option<String> {
    decode(&raw.replace('+', " "))
}

/// The longest a parameter's name is shown in an answer.
const NAME_SHOWN: usize = 64;

fn shown(name: &str) -> String {
    name.chars().take(NAME_SHOWN).collect()
}

fn refused(name: &str) -> Problem {
    Problem::new(Code::InvalidParameter).named("parameter", shown(name))
}

fn value_ok(kind: Kind, value: &str) -> bool {
    match kind {
        Kind::Text { max } => {
            let n = value.chars().count();
            (1..=max).contains(&n) && !value.chars().any(char::is_control)
        }
        Kind::Int { min, max } => value
            .parse::<i64>()
            .is_ok_and(|n| (min..=max).contains(&n) && !value.starts_with('+')),
        Kind::Hex { max } => {
            (1..=max).contains(&value.len())
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }
        Kind::Choice(words) => words.contains(&value),
    }
}

impl Params {
    /// Check `query` against `spec`: only the named parameters, each at most once, each within its kind, the
    /// required ones present.
    pub(crate) fn parse(query: Option<&str>, spec: &'static [Param]) -> Result<Params, Problem> {
        let mut values: Vec<(&'static str, String)> = Vec::new();
        for pair in query.unwrap_or("").split('&').filter(|p| !p.is_empty()) {
            let (raw_name, raw_value) = pair.split_once('=').unwrap_or((pair, ""));
            let name = component(raw_name).ok_or_else(|| refused(raw_name))?;
            let param = spec
                .iter()
                .find(|p| p.name == name)
                .ok_or_else(|| refused(&name))?;
            if values.iter().any(|(n, _)| *n == param.name) {
                return Err(refused(&name));
            }
            let value = component(raw_value).ok_or_else(|| refused(&name))?;
            if value.is_empty() && param.required {
                return Err(missing_param(param));
            }
            if !value_ok(param.kind, &value) {
                return Err(refused(&name));
            }
            values.push((param.name, value));
        }
        if let Some(missing) = spec
            .iter()
            .find(|p| p.required && !values.iter().any(|(n, _)| *n == p.name))
        {
            return Err(missing_param(missing));
        }
        Ok(Params { values })
    }

    /// The value of a text parameter that was given.
    pub(crate) fn text(&self, name: &str) -> Option<&str> {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The value of a whole-number parameter that was given (already checked).
    pub(crate) fn int(&self, name: &str) -> Option<i64> {
        self.text(name).and_then(|v| v.parse().ok())
    }
}

/// The problem for a required parameter that is not there: its own code, and for the general one the name.
fn missing_param(param: &Param) -> Problem {
    let problem = Problem::new(param.missing);
    if param.missing == Code::InvalidParameter {
        problem.named("parameter", param.name)
    } else {
        problem
    }
}

/// A path parameter that is lower-case hex of at most `max` digits (checked here, because it is not in the
/// query).
pub(crate) fn hex_id(value: &str, max: usize, name: &str) -> Result<String, Problem> {
    if value_ok(Kind::Hex { max }, value) {
        Ok(value.to_owned())
    } else {
        Err(refused(name))
    }
}

// ------------------------------------------------------------------------------------ read errors

/// The problem a read error comes to. The cause of an error that is the operator's business is logged, never
/// shown.
pub(crate) fn from_read(services: &Services, wiki: &WikiRecord, error: ReadError) -> Problem {
    match error {
        ReadError::InvalidTitle(_) => Problem::new(Code::InvalidTitle),
        ReadError::NoSuchView(view) => Problem::new(Code::NoSuchView).named(
            "view",
            view.strip_prefix("heads/").unwrap_or(&view).to_owned(),
        ),
        ReadError::PageUnreadable(_) => Problem::new(Code::PageUnreadable),
        ReadError::CouldNotBeReadNow(cause) => {
            (services.log)(&format!(
                "the API could not read wiki `{}` now: {cause}",
                wiki.slug
            ));
            Problem::new(Code::Unavailable)
        }
    }
}

/// The problem an API read error comes to.
pub(crate) fn from_api(services: &Services, wiki: &WikiRecord, error: ApiError) -> Problem {
    match error {
        ApiError::Read(e) => from_read(services, wiki, e),
        ApiError::InvalidCursor => Problem::new(Code::InvalidCursor),
        ApiError::NotAvailable(feature) => {
            Problem::new(Code::NotAvailable).member("feature", feature.name().into())
        }
    }
}
