//! A renewal or a lifting that lost to another (handoff G-T4, N-1): the page says *It changed while you acted. Look again.* in the
//! words of the person's language, and nothing is recorded by the act that lost. The race is made exact, as the core's tests make
//! it: the moment the act's event is kept in the trail (after the restriction in force was read, before the record), the other
//! moderator's act is recorded.

use std::sync::Arc;

use inklkunn_core::governance::PlatformLimits;
use inklkunn_core::people::PeopleService;
use inklkunn_store::{
    Actor, AuditEvent, AuditOutcome, AuditSink, GovernanceStore, MemoryAudit, RestrictionEvent,
    RestrictionEventId,
};
use inklkunn_web::Services;

use super::*;

/// A trail that runs `hook` on each event once it is kept.
struct Hooked {
    inner: Arc<MemoryAudit>,
    hook: Box<dyn Fn(&AuditEvent) + Send + Sync>,
}

impl AuditSink for Hooked {
    fn record(&self, event: &AuditEvent) -> inklkunn_store::Result<()> {
        self.inner.record(event)?;
        (self.hook)(event);
        Ok(())
    }
}

const CHANGED_EN: &str = "It changed while you acted. Look again.";
const CHANGED_JA: &str = "操作している間に状態が変わりました。もう一度確認してください。";

/// The people service of the world, serving a moderator whose act is met by `other`'s lifting of the restriction in force.
fn met_by_a_lifting(e: &mut Env, who: &AccountId, action: &'static str) {
    let active =
        e.w.store
            .active_restrictions(&e.w.wiki.id, who, NOW)
            .unwrap()
            .remove(0);
    let ender = RestrictionEvent {
        id: RestrictionEventId::new(),
        event: MeasureEvent::Lifted,
        reason: None,
        expires_at: None,
        of_event: Some(active.id.clone()),
        by: Actor {
            account: e.other_maintainer.clone(),
            rung: Role::Maintainer,
        },
        at: NOW,
        ..active
    };
    let store = e.w.store.clone();
    let sink = Hooked {
        inner: Arc::new(MemoryAudit::new()),
        hook: Box::new(move |event| {
            if event.action == action && event.outcome == AuditOutcome::Allowed {
                store.record_restriction_event(&ender).unwrap();
            }
        }),
    };
    let people = PeopleService::new(
        (*e.w.services.authorizer).clone(),
        e.w.store.clone(),
        e.w.store.clone(),
        e.w.store.clone(),
        Arc::new(sink),
        PlatformLimits::default(),
    );
    e.w.services = Arc::new(Services {
        people: Arc::new(people),
        ..(*e.w.services).clone()
    });
}

/// A form post as `who`, with the language cookie `cookie`.
async fn post_in(
    e: &Env,
    uri: &str,
    who: &AccountId,
    fields: &[(&str, &str)],
    cookie: &str,
) -> Answer {
    let body: String = fields
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                inklkunn_markup::percent_encode(k),
                inklkunn_markup::percent_encode(v)
            )
        })
        .collect::<Vec<_>>()
        .join("&");
    common::send_body(
        &e.w,
        "POST",
        uri,
        Some(who),
        &[("sec-fetch-site", "same-origin"), ("cookie", cookie)],
        body,
    )
    .await
}

#[tokio::test]
async fn a_renewal_and_a_lifting_that_lost_to_another_say_it_changed_in_both_languages_and_record_nothing()
 {
    for (cookie, sentence) in [
        ("inklkunn_lang=en", CHANGED_EN),
        ("inklkunn_lang=ja", CHANGED_JA),
    ] {
        for (act, uri) in [("renew-restriction", RENEW), ("lift-restriction", LIFT)] {
            let mut e = env();
            let who = e.contributor.to_string();
            let a = post_form(
                &e.w,
                RESTRICT,
                Some(&e.admin),
                &restrict_form(&who, "3", "spam"),
            )
            .await;
            assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
            let contributor = e.contributor.clone();
            met_by_a_lifting(&mut e, &contributor, act);
            let form: Vec<(&str, &str)> = if act == "renew-restriction" {
                restrict_form(&who, "5", "still spam")
            } else {
                vec![("who", who.as_str()), ("reason", "settled")]
            };
            let before = restriction_of(&e, &e.contributor).len();
            let a = post_in(&e, uri, &e.admin, &form, cookie).await;
            assert_eq!(a.status, StatusCode::CONFLICT, "{act}: {}", a.body());
            assert!(main_of(&a).contains(sentence), "{act}: {}", a.body());
            // Only the other moderator's lifting was recorded: the act that lost recorded nothing.
            assert_eq!(
                restriction_of(&e, &e.contributor).len(),
                before + 1,
                "{act}"
            );
        }
    }
}
