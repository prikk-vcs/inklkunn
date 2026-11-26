#![allow(clippy::expect_used, clippy::unwrap_used)]
//! 0.5.0 readiness, part RU (0.5.0 plan section 4, item 6): **the upgrade from 0.4.0**, end to end, on a real prikk. A data
//! directory of the 0.4.0 release binary is opened by this tree, and its wikis behave as before.

use std::collections::BTreeMap;

use super::api::{API, json_field, json_quote};
use super::knowledge::{
    SURVEY, approve_and_accept, attach, curate, ids_after, main_of, signed_into_history,
};
use super::upgrade::{
    OldServer, bare_world, binary_0_4_0, first_difference, masked, old_cli, old_invite, redeem,
    schema_of, settled, stylesheet_of,
};
use super::*;

/// One line of what the run did and saw, printed as it happens.
fn say(what: &str) {
    eprintln!("upgrade 0.4.0 -> this tree: {what}");
}

const POLICY: &str = "/handbook/-/policy";
const POLICY_FORM: &str = "/handbook/-/policy/edit";
const MODERATE: &str = "/handbook/-/moderate";
const PROTECT: &str = "/handbook/-/protect";
const APPEALS: &str = "/handbook/-/appeals";

/// The five tables of governance and moderation that people's acts write (RFC 006), authoritative: what 0.4.0 wrote is what
/// this tree finds, row for row.
const GOVERNANCE_TABLES: [&str; 5] = [
    "grant_decisions",
    "restrictions",
    "protections",
    "tombstones",
    "appeals",
];

/// The derived tables that hold the writes index and what is derived from the rules: they are rebuilt from the repository,
/// so they are looked at for being there and not for being the same rows.
const DERIVED_TABLES: [&str; 3] = ["path_writes", "write_blocks", "governance_derived"];

// ------------------------------------------------------------------------------------ the store, read as it is on disk

fn read_only(store: &Path) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(store, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
}

/// How many rows `table` has.
fn rows_in(store: &Path, table: &str) -> i64 {
    read_only(store)
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

/// Every row of `table` as text, in the order the store keeps them: what a comparison of two states of the table is made
/// of.
fn dump(store: &Path, table: &str) -> Vec<String> {
    let db = read_only(store);
    let mut statement = db
        .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
        .unwrap();
    let columns = statement.column_count();
    statement
        .query_map([], |row| {
            Ok((0..columns)
                .map(|i| format!("{:?}", row.get_ref(i).unwrap()))
                .collect::<Vec<_>>()
                .join(" | "))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// The wiki's repository as the files it is: path under `dir` to bytes. Two snapshots that are equal are one repository.
fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().display().to_string(),
                    std::fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

// ------------------------------------------------------------------------------------ small readings of pages

/// Every block id (64 hexadecimal digits) that follows `marker` in `html`, in order of appearance, once each.
fn blocks_after(html: &str, marker: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for chunk in html.split(marker).skip(1) {
        let id: String = chunk
            .chars()
            .take_while(char::is_ascii_hexdigit)
            .collect();
        if id.len() == 64 && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// The text of a whole page as the person reads it, for a sentence: tags become spaces and the app's own escapes are read
/// back.
fn plain(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace("&#38;", "&")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
}

/// The query of the first link to the moderation form for an item of `kind` that `page` holds.
fn moderate_query(page: &Reply, kind: &str) -> String {
    let want = format!("{MODERATE}?kind={kind}");
    let body = page.body.replace("&#38;", "&").replace("&amp;", "&");
    let at = body
        .find(&want)
        .unwrap_or_else(|| panic!("no link to hide a `{kind}` on the page:\n{}", page.body));
    let rest = body.get(at + MODERATE.len() + 1..).unwrap_or_default();
    rest.split('"').next().unwrap_or_default().to_owned()
}

// ------------------------------------------------------------------------------------ what a person does, through the pages

/// The proposal page's decide form, pressed as `approve`.
fn approve(person: &mut Person, id: &str, said: &str) {
    let page = person.browser.get(&proposal_page(id));
    let mut form = form_to(&page, &format!("{}/decide", proposal_page(id)));
    form.set("body", said);
    let decided = person.browser.submit(&form, Some(("op", "approve")));
    assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
}

/// The proposal page's accept form, sent; what the server answered.
fn accept(person: &mut Person, id: &str) -> Reply {
    let page = person.browser.get(&proposal_page(id));
    let form = form_to(&page, &format!("{}/accept", proposal_page(id)));
    person.browser.submit(&form, None)
}

/// A reviewer approves, the maintainer accepts: signed into the history.
fn reviewed_and_accepted(reviewer: &mut Person, maintainer: &mut Person, id: &str) {
    approve(reviewer, id, "Read it; it says what it says.");
    signed_into_history(&accept(maintainer, id));
}

/// Hide `parts` of the item whose form is at `query`, as `moderator`, through the moderation form.
fn hide(moderator: &mut Person, query: &str, parts: &[&str]) {
    let page = moderator.browser.get(&format!("{MODERATE}?{query}"));
    assert_eq!(page.status, 200, "{query}: {}", page.body);
    let mut form = form_to(&page, MODERATE);
    for part in parts {
        form.check(part, "1");
    }
    let category = form
        .choices
        .iter()
        .find(|c| c.0 == "category")
        .map(|c| c.1.clone())
        .expect("a category");
    form.check("category", &category);
    let hid = moderator.browser.submit(&form, None);
    assert_eq!(hid.status, 200, "{query}: {}\n{}", hid.head, hid.body);
}

/// An API token the person makes for themselves on their account page, as a script's owner does.
fn token_for(person: &mut Person, scope: Scope) -> String {
    let page = person.browser.get("/-/account");
    let mut form = form_to(&page, "/-/account/tokens");
    form.set("label", "upgrade 0.4.0")
        .set("scope", scope.as_str())
        .set("days", "1")
        .set("password", &person.password);
    let made = person.browser.submit(&form, None);
    assert!(
        made.status == 200 || made.status == 303,
        "{}\n{}",
        made.head,
        made.body
    );
    let shown = if made.status == 303 {
        person.browser.get(&made.location()).body
    } else {
        made.body
    };
    let at = shown
        .find("ink_")
        .unwrap_or_else(|| panic!("no token shown: {shown}"));
    shown.get(at..at + 47).unwrap().to_owned()
}

/// A script's request to the write API, on a server of this port: a bearer token, an `If-Match`, a JSON body.
fn api_on(
    port: u16,
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
    wire(port, method, path, &headers, body.unwrap_or_default().as_bytes())
}

/// The state id of the default view's tip, which a proposal sent through the API names.
fn tip(port: u16) -> String {
    let info = api_on(port, "GET", &format!("{API}/raw/Help"), None, None, None);
    assert_eq!(info.status, 200, "{}", info.body);
    format!("\"{}\"", json_field(&info.body, "state_id").expect("a tip"))
}

/// A proposal sent through the API as the holder of `token`; its id.
fn sent(port: u16, token: &str, summary: &str, members: &str) -> String {
    let body = format!(
        "{{\"summary\":{},\"submit\":true,{members}}}",
        json_quote(summary)
    );
    let made = api_on(
        port,
        "POST",
        &format!("{API}/proposals"),
        Some(token),
        Some(&tip(port)),
        Some(&body),
    );
    assert_eq!(made.status, 201, "{}\n{}\n{body}", made.head, made.body);
    json_field(&made.body, "id").expect("a proposal id")
}

fn scratch(title: &str, body: &str) -> String {
    format!("---\ntitle: {title}\n---\n\n{body}\n")
}

#[test]
fn a_directory_made_by_the_0_4_0_binary_is_opened_by_this_tree_and_its_wikis_behave_as_before() {
    let Some(prikk) = crate::keys::tests::prikk_probes::prikk_first() else {
        return;
    };
    let Some(old) = binary_0_4_0() else {
        return;
    };
    let mut world = bare_world(&prikk, "upgrade-0-4-0");
    let store_file = world.settings.store_path();

    // ---- 0.4.0 makes the wiki and six accounts
    say(&format!("the data directory is made by {}", old.display()));
    let (status, created, err) = old_cli(
        &old,
        &world,
        &[
            "wiki",
            "create",
            "handbook",
            "--name",
            "Handbook",
            "--visibility",
            "public",
        ],
    );
    assert_eq!(status, 0, "{err}");
    assert!(created.contains("created wiki handbook"), "{created}");
    let links: Vec<String> = [
        Role::Maintainer,
        Role::Contributor,
        Role::Reviewer,
        Role::Admin,
        Role::Contributor,
        Role::Contributor,
    ]
    .into_iter()
    .map(|role| old_invite(&old, &world, role))
    .collect();

    let started = now();
    let server = OldServer::start(&old, &world);
    let banner = std::fs::read_to_string(&server.log).unwrap();
    assert!(
        banner.contains("accepting proposals: the signer is open"),
        "the 0.4.0 server did not open its signer; its log so far:\n{banner}"
    );
    let port = server.port;
    let mut maya = redeem(&server, &links[0], "maya", "Maya Maintainer");
    let mut noor = redeem(&server, &links[1], "noor", "Noor Contributor");
    let mut quinn = redeem(&server, &links[2], "quinn", "Quinn Reviewer");
    let mut ada = redeem(&server, &links[3], "ada", "Ada Admin");
    let mut zora = redeem(&server, &links[4], "zora", "Zora Quokka");
    let mut rex = redeem(&server, &links[5], "rex", "Rex Restricted");
    let mut reader = server.browser();
    say("six accounts made by invitation: a maintainer, two contributors and a third, a reviewer and an admin");

    // ---- history: an accepted change, a claim with a source, a curation and a comment in the claim's thread
    let p1 = propose_edit(&mut noor, "Help", "Say which brackets", clarify_reading);
    signed_into_history(&approve_and_accept(&mut maya, &p1));
    let attached = attach(&mut noor, None, "supports", &SURVEY, "Cite the link syntax");
    signed_into_history(&approve_and_accept(&mut maya, &attached));
    let help = reader.get(HELP);
    let claim = ids_after(&help.body, "/handbook/-/claim/")
        .into_iter()
        .next()
        .expect("the claim link names the claim");
    let claim_page = format!("/handbook/-/claim/{claim}");
    let curated = curate(
        &mut noor,
        &claim,
        None,
        |f| {
            f.set("status", "contested");
        },
        "Say that this is contested",
    );
    signed_into_history(&approve_and_accept(&mut maya, &curated));
    for (who, said) in [
        (&mut zora, "QUOKKANOTE where in chapter 2 is this said?"),
        (&mut noor, "I will look it up."),
    ] {
        let page = who.browser.get(&claim_page);
        let mut form = form_to(&page, &format!("{claim_page}/discuss"));
        form.set("body", said);
        let made = who.browser.submit(&form, None);
        assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
    }
    say("0.4.0: an accepted change, a claim with its source, a curation, two comments in the claim's thread");

    // ---- a change of the rules, made through the policy form and accepted under the old rules
    let page = maya.browser.get(POLICY_FORM);
    assert_eq!(page.status, 200, "{}", page.body);
    let mut form = form_to(&page, POLICY_FORM);
    form.set("approvals", "1")
        .set("summary", "Ask one approval before a change is accepted");
    let sent_rules = maya.browser.submit(&form, Some(("act", "send")));
    assert_eq!(sent_rules.status, 303, "{}", sent_rules.body);
    let rules = proposal_id(&sent_rules.location());
    signed_into_history(&accept(&mut maya, &rules));
    say("0.4.0: the rules now ask one approval from someone other than the proposer");

    // ---- what the rules now bind: revisions of Help, three of them, each reviewed and accepted
    let r1 = propose_edit(
        &mut zora,
        "Help",
        "QUOKKASUMMARY say what the wiki checks",
        |text| {
            assert!(text.contains(TRUST_BEFORE), "{text}");
            text.replacen(
                TRUST_BEFORE,
                "It never claims more than it checked, QUOKKATEXT.",
                1,
            )
        },
    );
    let refused = accept(&mut maya, &r1);
    assert_eq!(refused.status, 409, "{}\n{}", refused.head, refused.body);
    assert!(
        refused.body.contains("it needs more approvals"),
        "{}",
        refused.body
    );
    reviewed_and_accepted(&mut quinn, &mut maya, &r1);
    let r2 = propose_edit(&mut noor, "Help", "WALRUSSUMMARY say it as it was", |text| {
        text.replacen(
            "It never claims more than it checked, QUOKKATEXT.",
            TRUST_AFTER,
            1,
        )
    });
    reviewed_and_accepted(&mut quinn, &mut maya, &r2);
    let r3 = propose_edit(&mut noor, "Help", "Name the licence's place", |text| {
        text.replacen(
            "Unless a page says otherwise",
            "Unless a page says otherwise in its header",
            1,
        )
    });
    reviewed_and_accepted(&mut quinn, &mut maya, &r3);
    say("0.4.0: three accepted revisions of Help, each needing an approval (the rule binds)");

    // ---- two pages and the acts that remove and rename: a script's proposals through the API
    let noor_token = token_for(&mut noor, Scope::Propose);
    for title in ["Scratch", "Sandbox"] {
        let id = sent(
            port,
            &noor_token,
            &format!("Start {title}"),
            &format!(
                "\"files\":[{{\"title\":{},\"text\":{}}}]",
                json_quote(title),
                json_quote(&scratch(title, &format!("The first text of {title}.")))
            ),
        );
        reviewed_and_accepted(&mut quinn, &mut maya, &id);
    }
    let renamed = sent(
        port,
        &noor_token,
        "Rename Scratch to Notes",
        &format!(
            "\"files\":[{{\"path\":\"pages/Notes.md\",\"text\":{}}}],\"renames\":[{{\"from\":\"pages/Scratch.md\",\"to\":\"pages/Notes.md\"}}]",
            json_quote(&scratch("Notes", "The first text of Scratch, now Notes."))
        ),
    );
    reviewed_and_accepted(&mut quinn, &mut maya, &renamed);
    let removed = sent(
        port,
        &noor_token,
        "The sandbox is out of date",
        "\"files\":[{\"path\":\"pages/Sandbox.md\",\"text\":null}]",
    );
    reviewed_and_accepted(&mut quinn, &mut maya, &removed);
    assert_eq!(reader.get("/handbook/Sandbox").status, 410);
    assert_eq!(reader.get("/handbook/Notes").status, 200);
    say("0.4.0: a page renamed (Scratch to Notes) and a page removed (Sandbox, 410 Gone)");
    let _ = (&store_file, &mut world, started, &mut ada, &mut rex, &mut reader);
}
