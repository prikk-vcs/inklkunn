#![allow(clippy::expect_used, clippy::unwrap_used)]
//! G-T1 (RFC 006 D-14; acceptance criterion 14): **the upgrade from 0.3.0**, end to end, on a real prikk. A data directory
//! of the 0.3.0 release binary migrates (store schema 18 to 19) at this tree's first start, and its wikis behave as before.
//!
//! **The data directory is made by the 0.3.0 release binary** (the asset `ci.yml` and `release.yml` install and check
//! against a recorded checksum, built from commit `599d4cd`), run as a process, through this file's sibling `upgrade.rs`
//! (the one file that starts a release binary): it creates a wiki, three invitations are redeemed over its own server and
//! its own pages (a maintainer, a contributor and a reviewer), and the **knowledge layer's own rows** are made, so that the
//! tables of schema 18 hold real data: a change is accepted, a source is attached to a paragraph (a claim, its source, its
//! link, and the review record of the change), a curation proposal sets the claim's status, a comment is written in the
//! claim's thread (`claim_comments`, version 18), a view is made (`view_acts`, version 16), and one proposal is left open.
//! The server is stopped and the store's schema read: 18. **Then this tree's server opens that same directory**, as `serve`
//! does: the store migrates 18 to 19 in one step, **nothing is written to the repository** (the blocks are the ones 0.3.0
//! wrote), the five authoritative tables of governance and moderation are there and empty, both halves' accounts sign in with
//! the passwords chosen in 0.3.0, the signer opens over the key store 0.3.0 made, the open proposal is accepted into the
//! history, and a new one is proposed, reviewed and accepted.
//!
//! **A change of the rules takes effect on the migrated directory** (0.4.0 readiness, R-5 item 2; RFC 006 D-4, D-5): through
//! the policy form the maintainer 0.3.0 made asks one approval from someone other than the proposer, and accepts that change
//! under the old rules; the next page proposal is then refused for want of an approval until the reviewer 0.3.0 made gives it;
//! the record of its acceptance names the block of the new rules, and the policy page says the rule and lists the change. The
//! run prints each of its stages (`upgrade 0.3.0 -> this tree: ...`), a transcript under `--nocapture`.
//!
//! **The pages are compared with 0.3.0's own output.** The pages 0.3.0 served, captured from its process before it stopped
//! (a page of the wiki, its history and its change, the wiki's sections, the claim's page and its API member, the views, the
//! *Why is this here?* page, *Sources to review*, the pages of the instance), are compared with the pages this tree serves
//! from the same directory. What differs on every page for no reason of the page's is masked, and only that: the ports of
//! the two servers, the stylesheet's content-addressed name, a form's per-request token, the footer's version, and the
//! sentences of the Japanese-preview notice that 0.3.0 printed and the owner's sign-off removed (the same `masked` as the
//! 0.2.0 test). **One page differs, by one section**: *Why is this here?* now lists the sources the change that attached one
//! created (0.4.0's M56), taken out by `undo_what_this_tree_adds`, which looks for exactly that section; and **one line is added
//! to the *About* page**, the way to the wiki's people (0.4.0's G-T2), taken out by `undo_the_people_link`, exactly, once and
//! on that page only. No other path of the sample differs, and the stylesheet holds every line of 0.3.0's, in order, with two rules changed (the list of attached sources
//! is set and breaks long lines as the page's other lists do).
//!
//! Needs prikk and the 0.3.0 binary. Without prikk the test prints `skipping: prikk not on PATH` (counted by
//! `EXPECTED_SKIPS`); with prikk and without the binary it prints `skipping: inklkunn 0.3.0 not found`, a line the with-prikk
//! run in CI fails on (`WITH_PRIKK_GUARDED`), because CI installs the binary.

use super::knowledge::{
    FIELD_NOTES, SURVEY, approve_and_accept, attach, curate, ids_after, main_of,
    signed_into_history,
};
use super::upgrade::{
    OldServer, bare_world, binary_0_3_0, capture, first_difference, masked, old_cli, old_invite,
    pages_to_compare, redeem, schema_of, settled, stylesheet_of,
};
use super::*;

/// The tables RFC 006 adds in schema 19 that are written by people: nothing may be in them at the upgrade.
const GOVERNANCE_TABLES: [&str; 5] = [
    "grant_decisions",
    "restrictions",
    "protections",
    "tombstones",
    "appeals",
];

/// The two rules of 0.3.0's stylesheet this tree changes, and what each says now: the new list of attached sources is set
/// and broken as the *Why* page's other lists are.
const RULES_CHANGED: [(&str, &str); 2] = [
    (
        ".why-excerpt, .why-chain li, .why-review li, .why-sources li, .why-people li { overflow-wrap: anywhere; }",
        ".why-excerpt, .why-chain li, .why-review li, .why-sources li, .why-attached li, .why-people li { overflow-wrap: anywhere; }",
    ),
    (
        ".why-chain, .why-people, .why-review, .why-sources, .why-links { margin-block: 0 0.5rem; padding-inline-start: 1.25rem; }",
        ".why-chain, .why-people, .why-review, .why-sources, .why-attached, .why-links { margin-block: 0 0.5rem; padding-inline-start: 1.25rem; }",
    ),
];

/// One line of what the run did and saw, printed as it happens (`-- --nocapture` shows the run as a transcript; a failure
/// shows how far it got).
fn say(what: &str) {
    eprintln!("upgrade 0.3.0 -> this tree: {what}");
}

/// How many rows `table` has, read from the store file as it is on disk.
fn rows_in(store: &Path, table: &str) -> i64 {
    let db =
        rusqlite::Connection::open_with_flags(store, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}

/// Does the store file have a table of this name?
fn has_table(store: &Path, table: &str) -> bool {
    let db =
        rusqlite::Connection::open_with_flags(store, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get::<_, i64>(0),
    )
    .unwrap()
        == 1
}

/// This tree's page with what it adds to 0.3.0's taken out, and whether anything was: on *Why is this here?*, the section
/// that lists the sources a change attached (0.4.0's M56: `Sources attached in this change`, its list, one line for each
/// source file the change created, with the way to a claim that cites it). It is looked for exactly: one such section,
/// whose one line is the survey's reference and a link to the claim's own page, on the *Why* page and nowhere else, so
/// a second section or one on another page is not taken out and is found as a difference.
fn undo_what_this_tree_adds(path: &str, html: &str) -> (String, bool) {
    const HEADING: &str = "<h4>Sources attached in this change</h4>";
    let (mut out, mut touched) = (String::new(), false);
    let mut lines = html.split_inclusive('\n');
    while let Some(line) = lines.next() {
        if !line.contains(HEADING) {
            out.push_str(line);
            continue;
        }
        assert_eq!(
            path, "/handbook/-/why/Help",
            "{path}: a section that 0.3.0 did not have, where it is not expected"
        );
        assert!(
            !touched,
            "{path}: more than one section of what this tree adds"
        );
        assert_eq!(line.trim_end(), HEADING, "{path}");
        assert_eq!(
            lines.next().map(str::trim_end),
            Some("<ul class=\"why-attached\">"),
            "{path}"
        );
        let item = lines.next().unwrap_or_default().trim_end();
        assert!(
            item.starts_with("<li>https://example.org/link-syntax <a href=\"/handbook/-/claim/")
                && item.contains("#source-")
                && item.ends_with("</a></li>"),
            "{path}: the one source the change attached: {item}"
        );
        assert_eq!(lines.next().map(str::trim_end), Some("</ul>"), "{path}");
        touched = true;
    }
    (out, touched)
}

#[test]
fn a_directory_made_by_the_0_3_0_binary_is_migrated_to_schema_19_and_its_wikis_behave_as_before() {
    let Some(prikk) = crate::keys::tests::prikk_probes::prikk_first() else {
        return;
    };
    let Some(old) = binary_0_3_0() else {
        return;
    };
    let mut world = bare_world(&prikk, "upgrade-0-3-0");
    let store_file = world.settings.store_path();

    // ---- 0.3.0 makes the wiki, three accounts, and data in the tables of its own schema
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
    let maintainer_link = old_invite(&old, &world, Role::Maintainer);
    let contributor_link = old_invite(&old, &world, Role::Contributor);
    let reviewer_link = old_invite(&old, &world, Role::Reviewer);

    let started = now();
    let server = OldServer::start(&old, &world);
    let banner = std::fs::read_to_string(&server.log).unwrap();
    assert!(
        banner.contains("accepting proposals: the signer is open"),
        "the 0.3.0 server did not open its signer; its log so far:\n{banner}"
    );
    let mut maya = redeem(&server, &maintainer_link, "maya", "Maya Maintainer");
    let mut noor = redeem(&server, &contributor_link, "noor", "Noor Contributor");
    let mut quinn = redeem(&server, &reviewer_link, "quinn", "Quinn Reviewer");

    // An accepted change to a page.
    let p1 = propose_edit(&mut noor, "Help", "Say which brackets", clarify_reading);
    signed_into_history(&approve_and_accept(&mut maya, &p1));
    // A claim: a source attached to the paragraph, accepted (the claim's file, the source's, the link, the review record).
    let attached = attach(&mut noor, None, "supports", &SURVEY, "Cite the link syntax");
    signed_into_history(&approve_and_accept(&mut maya, &attached));
    let mut reader = server.browser();
    let help = reader.get(HELP);
    assert_eq!(
        help.body.matches("class=\"claim-link\"").count(),
        1,
        "0.3.0 shows the claim: {}",
        help.body
    );
    let claim = ids_after(&help.body, "/handbook/-/claim/")
        .into_iter()
        .next()
        .expect("the claim link names the claim");
    let claim_page = format!("/handbook/-/claim/{claim}");
    // A curation: the claim's status is decided.
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
    // A comment in the claim's thread (version 18's table).
    let page = noor.browser.get(&claim_page);
    let mut form = form_to(&page, &format!("{claim_page}/discuss"));
    form.set("body", "Where in chapter 2 is this said?");
    let said = noor.browser.submit(&form, None);
    assert_eq!(said.status, 303, "{}\n{}", said.head, said.body);
    // A view (version 16's tables).
    let page = quinn.browser.get("/handbook/-/views/new");
    let mut form = form_to(&page, "/handbook/-/views/new");
    form.set("name", "reading")
        .check("kind", "interpretation")
        .set("from", "main");
    let made = quinn.browser.submit(&form, None);
    assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
    // An open proposal.
    let p2 = propose_edit(&mut noor, "Help", "Say that it says so", say_so);
    settled(&mut reader, started);

    // 0.3.0's own output, for the comparison: the pages and the stylesheet, as its process served them.
    let block = {
        let page = reader.get("/handbook/-/history/Help");
        let (_, after) = page.body.split_once("/handbook/-/change/").unwrap();
        after.split_once('"').unwrap().0.to_owned()
    };
    let mut paths = pages_to_compare(&block);
    paths.extend([
        claim_page.clone(),
        format!("/handbook/-/api/v1/claims/{claim}"),
        "/handbook/-/views".to_owned(),
        "/handbook/-/why/Help".to_owned(),
        "/handbook/-/health/sources-to-review".to_owned(),
        format!("{claim_page}?view=reading"),
    ]);
    let before = capture(server.browser(), &paths);
    for path in &paths {
        let status = before.get(path).map(|r| r.0);
        assert!(
            status == Some(200) || status == Some(303),
            "0.3.0 answers {path} with {status:?}"
        );
    }
    let old_css = reader
        .get(&stylesheet_of(&before.get(HELP).unwrap().1))
        .body;
    assert!(!old_css.is_empty());
    let old_ports = [server.port, server.content_port];
    let old_log = server.stop();
    assert_eq!(schema_of(&store_file), 18, "0.3.0 writes store schema 18");
    // The tables of that schema hold what 0.3.0 made, and the tables of the next do not exist yet.
    let (comments, views, acceptances) = (
        rows_in(&store_file, "claim_comments"),
        rows_in(&store_file, "view_acts"),
        rows_in(&store_file, "acceptances"),
    );
    assert_eq!(comments, 1, "the claim's thread has the comment");
    assert_eq!(views, 1, "the view's creation is recorded");
    assert_eq!(acceptances, 3, "the three accepted changes' records");
    for table in GOVERNANCE_TABLES.iter().chain(&["governance_derived"]) {
        assert!(!has_table(&store_file, table), "{table} exists at 18");
    }

    say("0.3.0's server is stopped; the store is at schema 18 with the knowledge layer's rows");
    // ---- this tree opens the same directory, as `serve` does
    let running = Running::start(&world);
    assert!(
        running
            .banner
            .iter()
            .any(|l| l.contains("accepting proposals: the signer is open")),
        "the signer did not open over the key store 0.3.0 made: {:?}",
        running.banner
    );
    assert_eq!(
        schema_of(&store_file),
        19,
        "the store migrated from 18 to this tree's schema"
    );
    // The upgrade is a migration and no write: what 0.3.0 recorded is there, and nothing of governance is.
    assert_eq!(rows_in(&store_file, "claim_comments"), comments);
    assert_eq!(rows_in(&store_file, "view_acts"), views);
    assert_eq!(rows_in(&store_file, "acceptances"), acceptances);
    for table in GOVERNANCE_TABLES {
        assert_eq!(
            rows_in(&store_file, table),
            0,
            "{table} is empty at the upgrade"
        );
    }
    world.store = Arc::new(SqliteStore::open(&store_file).unwrap());
    world.wiki = world
        .store
        .by_slug(&Slug::new("handbook").unwrap())
        .unwrap();
    let first_blocks = world.blocks();
    assert_eq!(
        first_blocks.len(),
        4,
        "the repository holds the blocks 0.3.0 wrote and nothing more: the wiki's first, the edit, the claim, the curation"
    );

    say(
        "this tree's server opened the directory: schema 19, the repository untouched, the five governance tables empty",
    );
    // ---- the wiki behaves as before: every page of the sample is 0.3.0's, byte for byte after the masks
    let after = capture(running.browser(), &paths);
    let new_ports = [running.port, running.content_port];
    let mut changed = Vec::new();
    for path in &paths {
        let (old_status, old_html, old_location) = before.get(path).unwrap();
        let (new_status, new_html, new_location) = after.get(path).unwrap();
        assert_eq!(old_status, new_status, "{path}: the status differs");
        assert_eq!(
            old_location.as_deref().map(|l| masked(l, old_ports)),
            new_location.as_deref().map(|l| masked(l, new_ports)),
            "{path}: the Location differs"
        );
        let new_html = &super::upgrade::undo_the_people_link(path, new_html);
        let (undone, touched) = undo_what_this_tree_adds(path, new_html);
        if touched {
            changed.push(path.as_str());
        }
        let (o, n) = (masked(old_html, old_ports), masked(&undone, new_ports));
        assert!(
            o == n,
            "{path}: renders differently from 0.3.0 ({} bytes to {}): {}",
            o.len(),
            n.len(),
            first_difference(&o, &n)
        );
    }
    // The one place this tree's pages are not byte for byte 0.3.0's (after the masks): the *Why* page of the change that
    // attached a source now lists it. No other path of the sample differs.
    assert_eq!(
        changed,
        ["/handbook/-/why/Help"],
        "the pages that differ from 0.3.0's"
    );
    // The stylesheet: every line of 0.3.0's is in this tree's, in its order, with additions between them.
    let mut anonymous = running.browser();
    let new_css = anonymous
        .get(&stylesheet_of(&after.get(HELP).unwrap().1))
        .body;
    for (was, _) in RULES_CHANGED {
        assert!(
            old_css.lines().any(|l| l == was),
            "0.3.0's rule is not the one the check expects: {was}"
        );
    }
    let mut rest = new_css.lines();
    for want in old_css.lines().map(|l| {
        RULES_CHANGED
            .iter()
            .find(|(was, _)| *was == l)
            .map_or(l, |(_, now)| *now)
    }) {
        assert!(
            rest.any(|l| l == want),
            "a line of 0.3.0's stylesheet is gone or changed (or out of its place): {want}"
        );
    }

    say(&format!(
        "{} pages compared with 0.3.0's own output: byte for byte after the masks; one section and one link differ, as named",
        paths.len()
    ));
    // ---- the accounts of 0.3.0 sign in with the passwords chosen there
    let mut maya_now = Person {
        login: "maya",
        name: "Maya Maintainer",
        password: password_for("maya"),
        browser: running.browser(),
    };
    sign_in(&mut maya_now.browser, "maya", &password_for("maya"), "/");
    let mut noor_now = Person {
        login: "noor",
        name: "Noor Contributor",
        password: password_for("noor"),
        browser: running.browser(),
    };
    sign_in(&mut noor_now.browser, "noor", &password_for("noor"), "/");
    let mut quinn_now = running.browser();
    sign_in(&mut quinn_now, "quinn", &password_for("quinn"), "/");
    // The accounts are the ones in the store 0.3.0 wrote (the migration kept them, with their grants).
    let _ = (
        world.account_of("maya"),
        world.account_of("noor"),
        world.account_of("quinn"),
    );
    assert!(
        page_text(&mut noor_now.browser, &claim_page).contains("Where in chapter 2 is this said?"),
        "the comment of 0.3.0 is in the claim's thread, read over schema 19"
    );

    say("the three accounts of 0.3.0 sign in with their passwords");
    // ---- the proposal 0.3.0 left open is accepted into the history, through the signer over 0.3.0's key store
    let page = noor_now.browser.get(&proposal_page(&p2));
    assert_eq!(page.status, 200, "{}", page.body);
    assert!(page.body.contains("Say that it says so"), "{}", page.body);
    let accept_action = format!("{}/accept", proposal_page(&p2));
    assert!(!has_form_to(&page, &accept_action), "{}", page.body);
    let accepted = approve_and_accept(&mut maya_now, &p2);
    signed_into_history(&accepted);
    let after_accept = world.blocks();
    assert_eq!(
        after_accept.len(),
        first_blocks.len() + 1,
        "prikk log: exactly one new block"
    );
    assert_eq!(after_accept.get(1), first_blocks.first());
    let help = page_text(&mut anonymous, HELP);
    assert!(
        help.contains(READING_AFTER),
        "0.3.0's accepted change is kept: {help}"
    );
    assert!(
        help.contains(TRUST_AFTER),
        "the proposal 0.3.0 left open joined it: {help}"
    );

    say("the proposal 0.3.0 left open is accepted, through the signer over 0.3.0's key store");
    // ---- and a new proposal, made on this tree, is reviewed and accepted: a second source against the claim
    let challenge = attach(
        &mut noor_now,
        None,
        "challenges",
        &FIELD_NOTES,
        "A source that speaks against it",
    );
    signed_into_history(&approve_and_accept(&mut maya_now, &challenge));
    assert_eq!(
        world.blocks().len(),
        first_blocks.len() + 2,
        "prikk log: the second source is one more block"
    );
    let seen = anonymous.get(&claim_page);
    assert_eq!(seen.status, 200, "{}", seen.body);
    assert!(
        main_of(&seen).contains("A survey of wiki link syntax")
            && main_of(&seen).contains("Field notes on red links")
            && main_of(&seen).contains("This claim is contested in this view.")
            && main_of(&seen).contains("Where in chapter 2 is this said?"),
        "the claim, as 0.3.0 made it and as this tree changed it: {}",
        main_of(&seen)
    );
    // Reading and accepting wrote nothing of governance either.
    for table in GOVERNANCE_TABLES {
        assert_eq!(rows_in(&store_file, table), 0, "{table} is still empty");
    }

    // ---- a change of the rules, made on the migrated directory, changes who may accept (RFC 006 D-4, D-5; 0.4.0 R-5 item 2)
    say("upgraded: the wiki follows the default rules, as 0.3.0 had it");
    let policy = anonymous.get("/handbook/-/policy");
    assert_eq!(policy.status, 200, "{}", policy.body);
    assert!(
        policy.body.contains("follows the default rules")
            && policy.body.contains("No change to the rules is recorded."),
        "{}",
        policy.body
    );
    say(
        "Maya, who holds the maintainer role 0.3.0 gave her, asks one approval from someone other than the proposer, through the policy form",
    );
    let page = maya_now.browser.get("/handbook/-/policy/edit");
    assert_eq!(page.status, 200, "{}", page.body);
    let mut form = form_to(&page, "/handbook/-/policy/edit");
    assert_eq!(
        form.get("approvals"),
        "0",
        "the form is filled from the rules in force"
    );
    form.set("approvals", "1");
    let sent = maya_now.browser.submit(&form, Some(("act", "send")));
    assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
    let rules = proposal_id(&sent.location());
    let before_rules = world.blocks();
    let shown = maya_now.browser.get(&proposal_page(&rules));
    assert!(
        shown
            .body
            .contains("Accepting this change takes: maintainer or above, with 0 approvals"),
        "the change is accepted under the old rules: {}",
        shown.body
    );
    say("she accepts it under the old rules; one block is signed into the history");
    let signed = maya_now.browser.submit(
        &form_to(&shown, &format!("{}/accept", proposal_page(&rules))),
        None,
    );
    signed_into_history(&signed);
    assert_eq!(
        world.blocks().len(),
        before_rules.len() + 1,
        "prikk log: the rules are one more block"
    );
    let rules_block = world.blocks().first().cloned().expect("the new rules");
    say(
        "Noor proposes a change to a page; Maya tries to accept it and is refused: it needs an approval",
    );
    let third = propose_edit(&mut noor_now, "Help", "Say where the licence is", |t| {
        assert!(t.contains("Unless a page says otherwise"), "{t}");
        t.replacen(
            "Unless a page says otherwise",
            "Unless a page says otherwise in its header",
            1,
        )
    });
    let accept_third = format!("{}/accept", proposal_page(&third));
    let hers = maya_now.browser.get(&proposal_page(&third));
    let held = maya_now
        .browser
        .submit(&form_to(&hers, &accept_third), None);
    assert_eq!(held.status, 409, "{}\n{}", held.head, held.body);
    assert!(
        held.body.contains("it needs more approvals"),
        "{}",
        held.body
    );
    assert_eq!(
        world.blocks().len(),
        before_rules.len() + 1,
        "nothing was signed"
    );
    say("Quinn approves it; Maya accepts it; the record names the block of the new rules");
    let theirs = quinn_now.get(&proposal_page(&third));
    let mut decide = form_to(&theirs, &format!("{}/decide", proposal_page(&third)));
    decide.set("body", "Looks right to me.");
    let decided = quinn_now.submit(&decide, Some(("op", "approve")));
    assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
    let hers = maya_now.browser.get(&proposal_page(&third));
    let accepted = maya_now
        .browser
        .submit(&form_to(&hers, &accept_third), None);
    signed_into_history(&accepted);
    assert_eq!(world.blocks().len(), before_rules.len() + 2);
    let record = maya_now.browser.get(&proposal_page(&third));
    let named = format!(
        "Accepted by Maya Maintainer (maintainer), under the rules as of <a href=\"/handbook/-/change/{rules_block}\">{}</a>.",
        rules_block.get(..12).unwrap_or_default()
    );
    assert!(record.body.contains(&named), "{named}: {}", record.body);
    let policy = anonymous.get("/handbook/-/policy");
    assert!(
        policy
            .body
            .contains("1 approval from someone other than the proposer")
            && policy
                .body
                .contains(&format!("href=\"/handbook/-/proposals/{rules}\"")),
        "the policy page says the rule and lists the change: {}",
        policy.body
    );
    say("the policy page says the rule and lists the change with its proposal");
    for table in GOVERNANCE_TABLES {
        assert_eq!(
            rows_in(&store_file, table),
            0,
            "{table}: a change of the rules is a change of the repository, and writes none of the five tables"
        );
    }

    say("done: nothing of governance written but the rules' own change, and no secret printed");
    // ---- nothing secret was printed by either half
    let printed = [
        old_log,
        running.notes.join("\n"),
        running.banner.join("\n"),
        running.log.lock().unwrap().join("\n"),
    ]
    .join("\n");
    let stderr = running.stop();
    for secret in [
        SECRET.to_owned(),
        password_for("maya"),
        password_for("noor"),
        password_for("quinn"),
    ] {
        assert!(!printed.contains(&secret), "a secret reached a log");
        assert!(!stderr.contains(&secret), "a secret reached stderr");
    }
    let _ = &world.tmp;
}
