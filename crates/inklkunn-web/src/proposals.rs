//! Proposals on the web (RFC 004 D-2, D-3, D-12; handoff 004 T3; external design WEB-03, WEB-04, WEB-05): the
//! list, the form that proposes a change to a page, a section, a part or a new page, the proposal's page with
//! its review, the thin forms that decline, withdraw or ask for a revision, and a page's discussion.
//!
//! **Every handler begins as every handler of a wiki does** (`Ctx::open`): who is asking, which wiki the slug
//! names, whether that principal may read it. A wiki that is absent and a wiki that may not be read are one
//! page; so are a proposal that is absent, one of another wiki, and a draft that is not the visitor's own (the
//! service answers all of them `NotFound`, and `words::refusal` gives that no page of its own). An anonymous
//! visitor who reaches a form is told, in words, that proposing needs an account (RFC 002 D-10: anonymous
//! proposing is off); a signed-in visitor whose role does not allow the act is told *not allowed*.
//!
//! **Every write is a form post from this site** (`crate::lang::from_this_site`) and ends with a redirect
//! (303) to the page that shows the result, so a reload never repeats it. Nothing here needs a script.
//!
//! **Accepting is not served yet** (handoff 004 T4): `review::accept` answers a page that says so, and it is
//! the one function the acceptance orchestration replaces. The words for every outcome of an acceptance are
//! here already (`AcceptOutcome`, `render_accept_outcome`).
//!
//! The pieces: `words` (what each state, outcome and refusal says), `form` (a form read and its bounds
//! checked before anything is parsed), `parts` (the section or part a form edits), `changes` (what a
//! proposal changes, part by part), `anchor` (a comment's anchor and where it lands on a later revision),
//! `render` (the view of those changes), and the handlers in `list`, `new`, `show` and `review`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::{Action, Principal, Resource};
use inklkunn_store::{AccountId, ProposalId, WikiRecord};

use crate::catalog::{Key, T};
use crate::pages::Page;
use crate::services::Services;
use crate::wiki::{Choice, notice, open};

pub(crate) mod anchor;
pub(crate) mod changes;
pub(crate) mod fields;
pub(crate) mod form;
pub(crate) mod lints;
pub(crate) mod list;
pub(crate) mod new;
pub(crate) mod parts;
pub(crate) mod paths;
pub(crate) mod render;
pub(crate) mod review;
pub(crate) mod show;
pub(crate) mod submit;
pub(crate) mod words;

pub use review::{AcceptOutcome, render_accept_outcome};

/// The most bytes a form of a proposal may carry: the bound on its texts, three times over (a text of
/// multi-byte characters is written three times as long in a form), and a little for the other fields.
pub(crate) const PROPOSAL_FORM_LIMIT: usize = 3 * 2 * 1024 * 1024 + 256 * 1024;

/// The most bytes a comment, a decision or a reason may carry.
pub(crate) const SMALL_FORM_LIMIT: usize = 96 * 1024;

/// What a request needs to act on a wiki's proposals.
pub(crate) struct Ctx {
    pub(crate) services: Arc<Services>,
    pub(crate) wiki: WikiRecord,
    pub(crate) principal: Principal,
}

impl Ctx {
    /// The wiki a request is about, if the principal may read it. `None` is the one answer for "no such wiki"
    /// and "not yours to read".
    pub(crate) fn open(services: &Arc<Services>, headers: &HeaderMap, slug: &str) -> Option<Ctx> {
        let wiki = open(services, headers, slug)?;
        Some(Ctx {
            services: services.clone(),
            wiki,
            principal: services.identity.principal(headers),
        })
    }

    /// The account behind the request, if there is one.
    pub(crate) fn account(&self) -> Option<&AccountId> {
        self.principal.account_id()
    }

    /// May the principal do `action` to `resource`?
    pub(crate) fn may(&self, action: Action, resource: &Resource) -> bool {
        self.services
            .authorizer
            .authorize(&self.principal, action, resource)
            .is_allowed()
    }

    /// May the principal do `action` to the proposal?
    pub(crate) fn may_on(&self, action: Action, id: &ProposalId) -> bool {
        self.may(
            action,
            &Resource::Proposal {
                wiki: self.wiki.id.clone(),
                id: id.to_string(),
            },
        )
    }

    /// May the principal do `action` to the wiki as a whole?
    pub(crate) fn may_wiki(&self, action: Action) -> bool {
        self.may(
            action,
            &Resource::Wiki {
                wiki: self.wiki.id.clone(),
            },
        )
    }

    /// The slug, for addresses.
    pub(crate) fn slug(&self) -> &str {
        self.wiki.slug.as_str()
    }

    /// The name a person is shown by: the account's display name, else a word for someone who is not known.
    pub(crate) fn person(&self, t: T, id: &AccountId) -> String {
        match self.services.accounts.account(id) {
            Ok(Some(account)) => account.display_name.as_str().to_owned(),
            _ => t.get(Key::PersonUnknown),
        }
    }

    /// The operator's log.
    pub(crate) fn log(&self, line: &str) {
        (self.services.log)(&format!("wiki `{}`: {line}", self.wiki.slug));
    }
}

/// Run `work` on a blocking thread and answer what it returns; a task that could not run is the page that
/// says the page could not be drawn.
pub(crate) async fn serve(
    page: Page,
    work: impl FnOnce(&Page) -> Response + Send + 'static,
) -> Response {
    let for_work = page.clone();
    match crate::wiki::blocking(move || work(&for_work)).await {
        Some(response) => response,
        None => crate::wiki::render_failed(&page),
    }
}

/// A notice inside the wiki's frame: a heading and paragraphs, perhaps with a way on.
pub(crate) fn notice_page(
    page: &Page,
    ctx: &Ctx,
    status: StatusCode,
    heading: Key,
    paragraphs: Vec<String>,
    choices: Vec<Choice>,
) -> Response {
    notice(page, &ctx.wiki, status, heading, paragraphs, choices, None)
}

/// The page that says a service refusal in words (never the cause), or the generic not-found page for the
/// refusals that are one. A refusal that is the store's or the repository's own failure goes to the log.
pub(crate) fn refused(
    page: &Page,
    ctx: &Ctx,
    error: &inklkunn_core::proposal::ProposalError,
    choices: Vec<Choice>,
) -> Response {
    use inklkunn_core::proposal::ProposalError;
    if let ProposalError::BaseUnavailable(cause) = error {
        ctx.log(&format!("a proposal's base could not be read now: {cause}"));
    }
    if let ProposalError::Store(cause) = error {
        ctx.log(&format!("the proposal records could not be used: {cause}"));
    }
    match words::refusal(page.t(), error) {
        None => crate::wiki::generic_not_found(page),
        Some(r) => {
            let response = notice_page(page, ctx, r.status, r.heading, r.paragraphs, choices);
            if r.retry {
                crate::wiki::with_retry_after(response)
            } else {
                response
            }
        }
    }
}

/// The visitor is not signed in and the act needs an account (RFC 002 D-10): said in words, with no form.
pub(crate) fn needs_account(page: &Page, ctx: &Ctx) -> Option<Response> {
    if ctx.account().is_some() {
        return None;
    }
    let t = page.t();
    Some(notice_page(
        page,
        ctx,
        StatusCode::FORBIDDEN,
        Key::NeedsAccountHeading,
        vec![t.get(Key::NeedsAccountBody), t.get(Key::NeedsAccountAdvice)],
        vec![Choice {
            label: t.get(Key::ProposalsBackToList),
            href: paths::list(ctx.slug()),
        }],
    ))
}

/// A write is a form post from this site, or it is refused (403) before the wiki is even looked up; the same
/// page for a wiki that is there and one that is not.
pub(crate) fn from_this_site(page: &Page, headers: &HeaderMap) -> Option<Response> {
    if crate::lang::from_this_site(headers) {
        return None;
    }
    let page = page.clone().without_switcher();
    Some(crate::pages::answer(
        StatusCode::FORBIDDEN,
        &page,
        crate::pages::render_form_refused(&page),
    ))
}

/// A redirect (303) to the page that shows the result of a write.
pub(crate) fn see(location: &str) -> Response {
    crate::wiki::redirect(location)
}

/// The proposal an address names, if it is an id.
pub(crate) fn parse_id(raw: &str) -> Option<ProposalId> {
    ProposalId::parse(raw)
}

/// A response that ends a request early (boxed: a response is large).
pub(crate) type Stop = Box<Response>;

/// The fields of a form, or the page that says the form was not one (wrong type) or too large.
pub(crate) fn fields_of(
    page: &Page,
    ctx: &Ctx,
    form: Result<
        axum::extract::Form<HashMap<String, String>>,
        axum::extract::rejection::FormRejection,
    >,
) -> Result<HashMap<String, String>, Stop> {
    match form {
        Ok(axum::extract::Form(fields)) => Ok(fields),
        Err(rejection) => {
            let status = rejection.status();
            let key = if status == StatusCode::PAYLOAD_TOO_LARGE {
                Key::InvalidFormTooLarge
            } else {
                Key::InvalidIncomplete
            };
            Err(Box::new(notice_page(
                page,
                ctx,
                status,
                Key::RefusedInvalidHeading,
                vec![page.get(key)],
                vec![Choice {
                    label: page.get(Key::ProposalsBackToList),
                    href: paths::list(ctx.slug()),
                }],
            )))
        }
    }
}
