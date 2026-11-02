#![allow(clippy::expect_used, clippy::unwrap_used)]
//! U6 through the write API: a script with scoped bearer tokens does what the pages do, and a token that carries
//! less than the act needs is refused whatever the account is.

use super::*;

/// The string or bare value of the first member `key` in a JSON text (enough for the ids and flags the tests read).
fn json_field(body: &str, key: &str) -> Option<String> {
    let (_, after) = body.split_once(&format!("\"{key}\":"))?;
    let after = after.trim_start();
    let Some(quoted) = after.strip_prefix('"') else {
        let end = after.find([',', '}', ']']).unwrap_or(after.len());
        return Some(after.get(..end)?.trim().to_owned());
    };
    let mut out = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?).unwrap_or('?'));
                }
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    None
}

fn json_quote(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A script's request to the write API: a bearer token (or none), an `If-Match`, a JSON body.
fn api(
    running: &Running,
    method: &str,
    path: &str,
    token: Option<&str>,
    if_match: Option<&str>,
    body: Option<&str>,
) -> Reply {
    let mut headers = vec![("Accept".to_owned(), "application/json".to_owned())];
    if let Some(t) = token {
        headers.push(("Authorization".into(), format!("Bearer {t}")));
    }
    if let Some(m) = if_match {
        headers.push(("If-Match".into(), m.to_owned()));
    }
    if body.is_some() {
        headers.push(("Content-Type".into(), "application/json".into()));
    }
    wire(
        running.port,
        method,
        path,
        &headers,
        body.unwrap_or_default().as_bytes(),
    )
}

const API: &str = "/handbook/-/api/v1";

fn problem(reply: &Reply) -> String {
    json_field(&reply.body, "type").unwrap_or_default()
}

fn proposal_body(summary: &str, text: &str) -> String {
    format!(
        "{{\"summary\":{},\"submit\":true,\"files\":[{{\"path\":\"pages/Help.md\",\"text\":{}}}]}}",
        json_quote(summary),
        json_quote(text)
    )
}

#[test]
fn a_script_with_scoped_tokens_proposes_comments_reviews_and_is_accepted_and_a_narrow_token_is_refused()
 {
    let Some(world) = World::new("api") else {
        return;
    };
    let running = Running::start(&world);
    let before = world.blocks();
    let noor = join(
        &world,
        &running,
        "noor",
        "Noor Contributor",
        Role::Contributor,
    );
    let _maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let auth = running.services.auth.clone().expect("sign-in is served");
    let token = |login: &str, scope: Scope| -> String {
        auth.issue_token(&world.account_of(login), scope, "u6", 3600, now())
            .unwrap()
            .secret
            .as_str()
            .to_owned()
    };
    let (noor_propose, noor_accept) = (token("noor", Scope::Propose), token("noor", Scope::Accept));
    let (maya_propose, maya_review, maya_accept) = (
        token("maya", Scope::Propose),
        token("maya", Scope::Review),
        token("maya", Scope::Accept),
    );

    // What a script reads first: the page's source (on the content origin) and the tip a proposal names.
    let raw = wire(
        running.port,
        "GET",
        "/handbook/-/raw/Help",
        &[("Host".into(), format!("localhost:{}", running.content_port))],
        &[],
    );
    assert_eq!(raw.status, 200, "{}\n{}", raw.head, raw.body);
    let text = raw.body.clone();
    let info = api(
        &running,
        "GET",
        &format!("{API}/raw/Help"),
        None,
        None,
        None,
    );
    assert_eq!(info.status, 200, "{}", info.body);
    let state = json_field(&info.body, "state_id").expect("the tip's state id");
    let base = format!("\"{state}\"");

    // Nobody who is not signed in can open a proposal; a token that is not one is told so.
    let create = format!("{API}/proposals");
    let body = proposal_body("Say which brackets", &clarify_reading(&text));
    let anonymous = api(&running, "POST", &create, None, Some(&base), Some(&body));
    assert_eq!(
        anonymous.status, 401,
        "{}\n{}",
        anonymous.head, anonymous.body
    );
    let invalid = api(
        &running,
        "POST",
        &create,
        Some("not-a-token"),
        Some(&base),
        Some(&body),
    );
    assert_eq!(invalid.status, 401, "{}\n{}", invalid.head, invalid.body);
    assert!(
        problem(&invalid).ends_with("invalid-token"),
        "{}",
        invalid.body
    );
    // A browser session with no CSRF token cannot write through the API either, though the cookie is valid.
    let cookie_line: Vec<String> = noor
        .browser
        .jar
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let forged = wire(
        running.port,
        "POST",
        &create,
        &[
            ("Content-Type".into(), "application/json".into()),
            ("If-Match".into(), base.clone()),
            ("Cookie".into(), cookie_line.join("; ")),
        ],
        body.as_bytes(),
    );
    assert_eq!(forged.status, 403, "{}\n{}", forged.head, forged.body);
    assert_eq!(world.blocks(), before);

    // Noor, with a `propose` token, opens three proposals against the same tip: the edit, an edit of another
    // paragraph, and a second edit of the first paragraph. Maya opens one of her own.
    let open = |who: &str, summary: &str, edit: &str| -> (String, String) {
        let made = api(
            &running,
            "POST",
            &create,
            Some(who),
            Some(&base),
            Some(&proposal_body(summary, edit)),
        );
        assert_eq!(made.status, 201, "{}\n{}", made.head, made.body);
        (
            json_field(&made.body, "id").expect("proposal id"),
            json_field(&made.body, "etag").expect("etag"),
        )
    };
    let (p1, p1_tag) = open(&noor_propose, "Say which brackets", &clarify_reading(&text));
    let (p2, p2_tag) = open(&noor_propose, "Say that it says so", &say_so(&text));
    let (p3, p3_tag) = open(&noor_propose, "Reword the brackets", &reword_reading(&text));
    let (p4, p4_tag) = open(&maya_propose, "Maya's own", &say_so(&text));
    assert_eq!(world.blocks(), before, "proposing writes nothing");

    let decisions = |id: &str| format!("{API}/proposals/{id}/decisions");
    let acceptances = |id: &str| format!("{API}/proposals/{id}/acceptances");
    let approve = "{\"decision\":\"approve\",\"body\":\"Looks right to me.\"}";

    // `propose` comments (the author, on her own proposal).
    let said = api(
        &running,
        "POST",
        &format!("{API}/proposals/{p1}/comments"),
        Some(&noor_propose),
        Some(&p1_tag),
        Some("{\"body\":\"Please check the wording of this paragraph.\"}"),
    );
    assert_eq!(said.status, 201, "{}\n{}", said.head, said.body);

    // Narrow tokens and narrow roles are refused, and nothing changes.
    for (what, who, path, doc) in [
        (
            "propose cannot review",
            &maya_propose,
            decisions(&p1),
            approve,
        ),
        (
            "propose cannot accept",
            &maya_propose,
            acceptances(&p1),
            "{}",
        ),
        ("review cannot accept", &maya_review, acceptances(&p1), "{}"),
        (
            "a contributor's token cannot review",
            &noor_accept,
            decisions(&p1),
            approve,
        ),
        (
            "a contributor's token cannot accept",
            &noor_accept,
            acceptances(&p1),
            "{}",
        ),
    ] {
        let refused = api(&running, "POST", &path, Some(who), Some(&p1_tag), Some(doc));
        assert_eq!(
            refused.status, 403,
            "{what}: {}\n{}",
            refused.head, refused.body
        );
    }
    assert_eq!(world.blocks(), before);
    // D9: Maya may review, but not her own proposal.
    let own = api(
        &running,
        "POST",
        &decisions(&p4),
        Some(&maya_review),
        Some(&p4_tag),
        Some(approve),
    );
    assert_eq!(own.status, 403, "{}\n{}", own.head, own.body);
    assert!(problem(&own).ends_with("own-proposal"), "{}", own.body);

    // Maya, with a `review` token, approves Noor's proposal; with an `accept` token she accepts it.
    let approved = api(
        &running,
        "POST",
        &decisions(&p1),
        Some(&maya_review),
        Some(&p1_tag),
        Some(approve),
    );
    assert_eq!(approved.status, 201, "{}\n{}", approved.head, approved.body);
    let accepted = api(
        &running,
        "POST",
        &acceptances(&p1),
        Some(&maya_accept),
        Some(&p1_tag),
        Some("{}"),
    );
    assert_eq!(accepted.status, 200, "{}\n{}", accepted.head, accepted.body);
    assert_eq!(
        json_field(&accepted.body, "state").as_deref(),
        Some("accepted")
    );
    assert_eq!(
        json_field(&accepted.body, "outcome").as_deref(),
        Some("signed")
    );
    assert_eq!(
        json_field(&accepted.body, "reauthored").as_deref(),
        Some("false")
    );
    let after_first = world.blocks();
    assert_eq!(
        after_first.len(),
        before.len() + 1,
        "prikk log: one new block"
    );
    let help = running.browser().get(HELP);
    assert!(help.body.contains(READING_AFTER), "{}", help.body);
    // A retry writes nothing.
    let again = api(
        &running,
        "POST",
        &acceptances(&p1),
        Some(&maya_accept),
        Some(&p1_tag),
        Some("{}"),
    );
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(
        json_field(&again.body, "outcome").as_deref(),
        Some("already-signed")
    );
    assert_eq!(world.blocks(), after_first);

    // The proposal written against the old tip is re-authored; the one that overlaps stops, with the proposal
    // needing a revision and the history as it was.
    let moved = api(
        &running,
        "POST",
        &acceptances(&p2),
        Some(&maya_accept),
        Some(&p2_tag),
        Some("{}"),
    );
    assert_eq!(moved.status, 200, "{}\n{}", moved.head, moved.body);
    assert_eq!(
        json_field(&moved.body, "outcome").as_deref(),
        Some("signed")
    );
    assert_eq!(
        json_field(&moved.body, "reauthored").as_deref(),
        Some("true")
    );
    let after_second = world.blocks();
    assert_eq!(after_second.len(), after_first.len() + 1);
    let help = running.browser().get(HELP);
    assert!(
        help.body.contains(READING_AFTER) && help.body.contains(TRUST_AFTER),
        "{}",
        help.body
    );
    let overlap = api(
        &running,
        "POST",
        &acceptances(&p3),
        Some(&maya_accept),
        Some(&p3_tag),
        Some("{}"),
    );
    assert_eq!(overlap.status, 409, "{}\n{}", overlap.head, overlap.body);
    assert!(problem(&overlap).ends_with("overlap"), "{}", overlap.body);
    assert_eq!(world.blocks(), after_second, "an overlap writes nothing");
    let held = api(
        &running,
        "GET",
        &format!("{API}/proposals/{p3}"),
        Some(&noor_propose),
        None,
        None,
    );
    assert_eq!(
        json_field(&held.body, "state").as_deref(),
        Some("needs-revision"),
        "{}",
        held.body
    );

    // The verification after the changes is a fresh one.
    until("the verification after the seal", || {
        world.verification().at.is_some_and(|at| at > 1_000)
    });
    assert_eq!(
        world.verification().state,
        inklkunn_store::VerificationState::Verified
    );
    running.stop();
}
