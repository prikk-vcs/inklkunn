//! The people page of a wiki (RFC 006 D-6; handoff G-T2): the roles in order, what the wiki's rules say each is for, who
//! holds each, what `entry` gives everyone, the person's own standing and why, and the way to the forms that change a role or
//! restrict a person, for those who may use them.
//!
//! **Authorized as a wiki's pages are** (`read`) and existence-neutral: a wiki that is absent and a wiki that may not be
//! read are one page. **Who is named** is the core's rule ([`PeopleView`]): a visitor with no account is told the names of
//! the people who hold the reviewer role and above, and only the number for the lower roles; an account sees every name. A
//! restriction is never named but to the restricted person (their own standing) and to the people who may restrict.

use askama::Template;
use axum::extract::rejection::{PathRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use inklkunn_core::people::{Holders, PeopleError, PeopleView, Person};
use inklkunn_store::{Visibility, WikiRecord};

use super::{fields, paths, sentences};
use crate::catalog::{Key, T};
use crate::discover::Subject;
use crate::pages::{Page, PageState, answer};
use crate::proposals::render::LinkView;
use crate::proposals::{Ctx, serve};
use crate::time::utc_minutes;
use crate::wiki::{Shell, generic_not_found, services_of};

type Params = Result<Query<std::collections::HashMap<String, String>>, QueryRejection>;

/// One person in a role's list.
struct PersonRow {
    /// Their name, with what the page says of them (*you*, *restricted until …*).
    line: String,
    links: Vec<LinkView>,
}

/// One role of the ladder.
struct RoleRow {
    id: &'static str,
    label: String,
    promotion: Option<String>,
    /// For a visitor who is told only how many hold the role.
    count: Option<String>,
    none: Option<String>,
    people: Vec<PersonRow>,
}

struct RestrictedItem {
    line: String,
    reason: String,
    links: Vec<LinkView>,
}

#[derive(Template)]
#[template(path = "people.html")]
struct PeopleTpl<'a> {
    page: &'a Page,
    shell: &'a Shell,
    title: String,
    heading: String,
    done: Option<String>,
    intro: String,
    entry: String,
    promotion_note: String,
    names_note: Option<String>,
    you_heading: String,
    you: Vec<String>,
    grant: Option<LinkView>,
    restrict: Option<LinkView>,
    /// The way to contest the viewer's own restriction, to their own appeals, and, for the people who decide appeals, to the
    /// appeals (RFC 006 D-9).
    appeals: Vec<LinkView>,
    roles_heading: String,
    roles: Vec<RoleRow>,
    restricted_heading: String,
    restricted_intro: String,
    restricted_none: String,
    restricted: Option<Vec<RestrictedItem>>,
}

/// What `entry` gives everyone, in a sentence: never the rule's word.
fn entry_sentence(t: T, view: &PeopleView) -> String {
    let given = view.entry.rung();
    match (view.visibility, given) {
        (Visibility::Restricted, None) => t.get(Key::PeopleEntryNoneRestricted),
        (Visibility::Restricted, Some(role)) => t
            .msg(Key::PeopleEntryNoEffect)
            .with("role", sentences::role_label(t, role))
            .text(),
        (_, None) => t.get(Key::PeopleEntryNone),
        (_, Some(role)) => t
            .msg(Key::PeopleEntryGiven)
            .with("role", sentences::role_label(t, role))
            .text(),
    }
}

/// What was just done, as the page says it (a fixed set of words in the query; anything else says nothing).
fn done_sentence(t: T, word: Option<&str>) -> Option<String> {
    Some(t.get(match word? {
        fields::DONE_GRANTED => Key::PeopleDoneGranted,
        fields::DONE_REVOKED => Key::PeopleDoneRevoked,
        fields::DONE_RESTRICTED => Key::PeopleDoneRestricted,
        fields::DONE_RENEWED => Key::PeopleDoneRenewed,
        fields::DONE_LIFTED => Key::PeopleDoneLifted,
        _ => return None,
    }))
}

fn person_name(t: T, name: Option<&str>) -> String {
    name.map_or_else(|| t.get(Key::PersonUnknown), str::to_owned)
}

fn person_row(t: T, slug: &str, person: &Person) -> PersonRow {
    let mut line = person_name(t, person.name.as_deref());
    if person.is_you {
        line = t.msg(Key::PeopleIsYou).with("person", line).text();
    }
    if let Some(until) = person.restricted_until {
        line = t
            .msg(Key::PeopleRestrictedMark)
            .with("person", line)
            .with("date", utc_minutes(until))
            .text();
    }
    let who = person_name(t, person.name.as_deref());
    let id = person.account.to_string();
    let mut links = Vec::new();
    if person.may_change {
        links.push(LinkView {
            href: paths::grant(slug, Some(&id)),
            label: t
                .msg(Key::PeopleChangeLinkOf)
                .with("person", who.as_str())
                .text(),
        });
    }
    if person.restricted_until.is_some() {
        if person.may_restrict {
            links.push(LinkView {
                href: paths::renew(slug, Some(&id)),
                label: t
                    .msg(Key::PeopleRenewLinkOf)
                    .with("person", who.as_str())
                    .text(),
            });
            links.push(LinkView {
                href: paths::lift(slug, Some(&id)),
                label: t.msg(Key::PeopleLiftLinkOf).with("person", who).text(),
            });
        }
    } else if person.may_restrict {
        links.push(LinkView {
            href: paths::restrict(slug, Some(&id)),
            label: t.msg(Key::PeopleRestrictLinkOf).with("person", who).text(),
        });
    }
    PersonRow { line, links }
}

fn role_rows(t: T, slug: &str, view: &PeopleView) -> Vec<RoleRow> {
    view.rungs
        .iter()
        .map(|row| {
            let (count, none, people) = match &row.holders {
                Holders::Count(0) => (None, Some(t.get(Key::PeopleNoOne)), Vec::new()),
                Holders::Count(n) => (
                    Some(t.msg(Key::PeopleHolderCount).n(*n as u64).text()),
                    None,
                    Vec::new(),
                ),
                Holders::Named(people) if people.is_empty() => {
                    (None, Some(t.get(Key::PeopleNoOne)), Vec::new())
                }
                Holders::Named(people) => (
                    None,
                    None,
                    people.iter().map(|p| person_row(t, slug, p)).collect(),
                ),
            };
            RoleRow {
                id: row.rung.as_str(),
                label: sentences::role_label(t, row.rung),
                promotion: row.promotion.as_ref().map(|text| {
                    t.msg(Key::PeoplePromotion)
                        .with("text", text.as_str())
                        .text()
                }),
                count,
                none,
                people,
            }
        })
        .collect()
}

fn restricted_rows(t: T, slug: &str, view: &PeopleView) -> Vec<RestrictedItem> {
    view.restricted
        .iter()
        .map(|row| {
            let who = person_name(t, row.name.as_deref());
            let id = row.account.to_string();
            let mut links = Vec::new();
            if row.may_act {
                links.push(LinkView {
                    href: paths::renew(slug, Some(&id)),
                    label: t
                        .msg(Key::PeopleRenewLinkOf)
                        .with("person", who.as_str())
                        .text(),
                });
                links.push(LinkView {
                    href: paths::lift(slug, Some(&id)),
                    label: t
                        .msg(Key::PeopleLiftLinkOf)
                        .with("person", who.as_str())
                        .text(),
                });
            }
            RestrictedItem {
                line: t
                    .msg(Key::PeopleRestrictedRow)
                    .with("person", who)
                    .with("date", utc_minutes(row.until))
                    .text(),
                reason: t
                    .msg(Key::RestrictionReason)
                    .with("reason", row.reason.as_str())
                    .text(),
                links,
            }
        })
        .collect()
}

/// *Contest this* for the viewer's own restriction, *My appeals* for a signed-in viewer, and *Appeals* for the people who decide them.
fn appeals_links(t: T, slug: &str, view: &PeopleView, decides: bool) -> Vec<LinkView> {
    let mut out = Vec::new();
    if let Some(you) = &view.you {
        if let Some(notice) = &you.restriction {
            out.push(LinkView {
                href: crate::appeals::words::restriction_choice(t, slug, notice).href,
                label: t.get(Key::ContestLink),
            });
        }
        out.push(LinkView {
            href: crate::appeals::paths::mine(slug),
            label: t.get(Key::AppealsMineLink),
        });
    }
    if decides {
        out.push(LinkView {
            href: crate::appeals::paths::page(slug),
            label: t.get(Key::AppealsHeading),
        });
    }
    out
}

fn render(
    page: &Page,
    wiki: &WikiRecord,
    view: &PeopleView,
    done: Option<&str>,
    decides: bool,
) -> Response {
    let t = page.t();
    let slug = wiki.slug.as_str();
    let shell = Shell::of(wiki);
    let head = page.wiki_head(wiki, Subject::Wiki);
    let anonymous_view = view.you.is_none();
    let rendered = PeopleTpl {
        page: &head,
        shell: &shell,
        title: t
            .msg(Key::TitlePeople)
            .with("wiki", wiki.display_name.as_str())
            .product()
            .text(),
        heading: head.get(Key::PeopleHeading),
        done: done_sentence(t, done),
        intro: head.get(Key::PeopleIntro),
        entry: entry_sentence(t, view),
        promotion_note: head.get(Key::PeoplePromotionNote),
        names_note: anonymous_view.then(|| head.get(Key::PeopleNamesNote)),
        you_heading: head.get(Key::PeopleYouHeading),
        you: view
            .you
            .as_ref()
            .map_or_else(Vec::new, |h| sentences::holding_paragraphs(t, h)),
        grant: view.may_grant_up_to.map(|_| LinkView {
            href: paths::grant(slug, None),
            label: head.get(Key::PeopleGrantLink),
        }),
        restrict: view.may_restrict.then(|| LinkView {
            href: paths::restrict(slug, None),
            label: head.get(Key::PeopleRestrictLink),
        }),
        appeals: appeals_links(t, slug, view, decides),
        roles_heading: head.get(Key::PeopleRolesHeading),
        roles: role_rows(t, slug, view),
        restricted_heading: head.get(Key::PeopleRestrictedHeading),
        restricted_intro: head.get(Key::PeopleRestrictedIntro),
        restricted_none: head.get(Key::PeopleRestrictedNone),
        restricted: view.may_restrict.then(|| restricted_rows(t, slug, view)),
    }
    .render();
    answer(StatusCode::OK, &head, rendered)
}

/// `GET /{slug}/-/people`.
pub(crate) async fn people(
    State(state): State<PageState>,
    page: Page,
    headers: HeaderMap,
    path: Result<Path<String>, PathRejection>,
    params: Params,
) -> Response {
    let (Ok(Path(slug)), Some(services)) = (path, services_of(&state)) else {
        return generic_not_found(&page);
    };
    let done = params.ok().and_then(|Query(mut p)| p.remove(fields::DONE));
    serve(page, move |page| {
        let Some(ctx) = Ctx::open(&services, &headers, &slug) else {
            return generic_not_found(page);
        };
        match services.people.view(&ctx.principal, &ctx.wiki) {
            Ok(view) => render(
                page,
                &ctx.wiki,
                &view,
                done.as_deref(),
                services.appeals.may_decide(&ctx.principal, &ctx.wiki),
            ),
            Err(PeopleError::NotFound) => generic_not_found(page),
            Err(error) => {
                ctx.log(&format!("the people page could not be drawn: {error}"));
                crate::wiki::render_failed(page)
            }
        }
    })
    .await
}
