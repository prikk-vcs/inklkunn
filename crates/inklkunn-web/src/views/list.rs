//! The wiki's views (RFC 005 part 2 D-9; handoff 005 K-T8): the default view first, called *the default view*, then
//! the alternative views by name, each with what the service recorded of it (its kind, and the view it was forked
//! from). A view the service did not make has no recorded origin, and the list says so in one plain sentence
//! rather than leave it out or invent one. Bounded: what the list leaves out is counted in words.
//!
//! Authorized as a wiki's pages are (`read`), and existence-neutral. The link to the form that creates a view is
//! shown to whoever may create one; whoever may not sees the list and nothing else.

use askama::Template;
use axum::extract::Path;
use axum::extract::State;
use axum::extract::rejection::PathRejection;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::views::{ViewKind, ViewList, ViewRow, list as list_views, short_name};
use inklkunn_core::{Action, Resource};
use inklkunn_store::WikiRecord;

use super::{named, paths};
use crate::catalog::{Key, T};
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::services::Services;
use crate::url::view_ref;
use crate::wiki::{
    Shell, Shown, blocking, generic_not_found, open, present, render_failed, services_of,
};

/// One view in the list, in words.
struct Item {
    id: String,
    /// The view, named in words.
    label: String,
    /// The link that opens it (where its name can be an address).
    read: Option<LinkView>,
    notes: Vec<String>,
    compare: Option<LinkView>,
}

#[derive(Template)]
#[template(path = "views.html")]
struct ViewsTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    intro: String,
    create: Option<LinkView>,
    compare: Option<LinkView>,
    only_default: Option<String>,
    items: Vec<Item>,
    omitted: Option<String>,
}

/// What a request for the list came to, before any template.
enum Loaded {
    Shown(Shown),
    List {
        wiki: Box<WikiRecord>,
        list: ViewList,
        may_create: bool,
    },
}

fn load(services: &Services, headers: &HeaderMap, slug: &str) -> Loaded {
    let Some(wiki) = open(services, headers, slug) else {
        return Loaded::Shown(Shown::NotFound);
    };
    match list_views(&services.read, &*services.acts, &wiki) {
        Ok(list) => {
            let principal = services.identity.principal(headers);
            let may_create = services
                .authorizer
                .authorize(
                    &principal,
                    Action::CreateView,
                    &Resource::Wiki {
                        wiki: wiki.id.clone(),
                    },
                )
                .is_allowed();
            Loaded::List {
                wiki: Box::new(wiki),
                list,
                may_create,
            }
        }
        Err(e) => {
            (services.log)(&format!(
                "the views of wiki `{}` could not be read now: {e}",
                wiki.slug
            ));
            Loaded::Shown(Shown::Unavailable {
                wiki: Box::new(wiki),
            })
        }
    }
}

/// `GET /{slug}/-/views`.
pub(crate) async fn list(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    match blocking(move || load(&services, &headers, &slug)).await {
        None => render_failed(&page),
        Some(Loaded::Shown(shown)) => present(&page, shown),
        Some(Loaded::List {
            wiki,
            list,
            may_create,
        }) => render(&page, &wiki, &list, may_create),
    }
}

/// What the note under an alternative view says: its kind and what that kind is, and where it came from.
fn notes_of(t: T, wiki: &WikiRecord, row: &ViewRow) -> Vec<String> {
    if row.is_default {
        return vec![t.get(Key::ViewsDefaultNote)];
    }
    let Some(origin) = &row.origin else {
        return vec![t.get(Key::ViewsNoOrigin)];
    };
    let (kind, note) = match origin.kind {
        ViewKind::Experimental => (Key::ViewsKindExperimental, Key::ViewsKindExperimentalNote),
        ViewKind::Interpretation => (
            Key::ViewsKindInterpretation,
            Key::ViewsKindInterpretationNote,
        ),
    };
    vec![
        t.get(kind),
        t.get(note),
        t.msg(Key::ViewsForkedFrom)
            .with("view", named(t, wiki, &origin.forked_from))
            .text(),
    ]
}

fn items(t: T, wiki: &WikiRecord, list: &ViewList) -> Vec<Item> {
    let slug = wiki.slug.as_str();
    let default = short_name(&wiki.default_view);
    list.rows
        .iter()
        .map(|row| {
            // A view whose name cannot be an address (made some other way) is listed and cannot be followed.
            let addressable = row.is_default || view_ref(&row.name).is_some();
            let read = row
                .is_default
                .then(|| paths::read(slug, None))
                .or_else(|| addressable.then(|| paths::read(slug, Some(&row.name))));
            Item {
                id: format!("view-{}", row.name),
                label: named(t, wiki, &row.name),
                read: read.map(|href| LinkView {
                    href,
                    label: t
                        .msg(Key::ViewsReadLink)
                        .with("view", named(t, wiki, &row.name))
                        .text(),
                }),
                notes: notes_of(t, wiki, row),
                compare: (!row.is_default && addressable).then(|| LinkView {
                    href: paths::compare(slug, Some(default), Some(&row.name)),
                    label: t
                        .msg(Key::ViewsCompareWithDefault)
                        .with("view", named(t, wiki, &row.name))
                        .text(),
                }),
            }
        })
        .collect()
}

fn render(page: &Page, wiki: &WikiRecord, list: &ViewList, may_create: bool) -> Response {
    let t = page.t();
    let slug = wiki.slug.as_str();
    let shell = Shell::of(wiki);
    let head = page.wiki_head(wiki, Subject::Surface("-/views"));
    // A list past its bound holds the bound's worth of rows, so one row (or none) is the default view alone.
    let only_default = list.rows.len() <= 1;
    let rendered = ViewsTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitleViews)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: head.get(Key::ViewsHeading),
        intro: head.get(Key::ViewsIntro),
        create: may_create.then(|| LinkView {
            href: paths::new(slug),
            label: head.get(Key::ViewsCreateLink),
        }),
        compare: (!only_default).then(|| LinkView {
            href: paths::compare(slug, None, None),
            label: head.get(Key::ViewsCompareLink),
        }),
        only_default: only_default.then(|| head.get(Key::ViewsOnlyDefault)),
        items: items(t, wiki, list),
        omitted: (list.omitted > 0).then(|| {
            t.msg(Key::ViewsOmitted)
                .with("shown", list.rows.len())
                .with("omitted", list.omitted)
                .text()
        }),
    }
    .render();
    answer(StatusCode::OK, &head, rendered)
}
