#![allow(clippy::expect_used, clippy::unwrap_used)]
//! R-5 item 1 (0.4.0 readiness; exit criterion 4): **the end-to-end proof of governance and moderation on a real prikk**,
//! in the shape of 0.3.0's R-2 (`knowledge.rs`). The real server (the binary's `prepare`) over a real prikk, a real key
//! store and a real SQLite store, behind a real socket, driven the way U6 drives it: the invitation machinery, the sign-in
//! page, forms read off the pages with their CSRF tokens. Nobody is given a header-named identity and no record is written
//! but by the server (one exception, said where it is: the clock, in part (b)).
//!
//! **One scripted run, and its transcript.** The test numbers every step, prints it as it goes (`eprintln!`, so
//! `cargo test ... readiness -- --nocapture` shows the whole story and a failure shows how far it got) and asserts at every
//! step what the story says it saw. The story, in order:
//!
//! - (a) a change of the rules, made through the policy form and accepted under the old rules, changes who may accept:
//!   a page proposal is refused for want of approvals until two people other than the proposer have given theirs; the
//!   record of its acceptance names the block of the new rules; the policy page says the new rule in a sentence and lists
//!   the change with the proposal that made it;
//! - (b) a temporary protection is put in place through the moderation form, shown on the policy page and on the page,
//!   refuses the acceptance it asks a role for, and ends: the page and the policy page stop showing it and the acceptance
//!   goes through;
//! - (c) a hidden revision, a hidden proposal comment, a hidden comment in a claim's thread and a hidden summary of a
//!   change of the rules are **not served on any surface of the table** (`inklkunn_core::tombstone::surfaces::SURFACES`,
//!   iterated here: every row, each address of it, as an anonymous reader and as a moderator, in the HTML and in the API
//!   where the row has one), every one says why in its place, and the unhide brings each back on the surfaces that show it;
//! - (d) the author of a hidden revision appeals, a person who did not hide it reverses it, the appeal pages say which rung
//!   decided under which rules, and the content is back;
//! - (e) a page is removed (`410`, with the removal's summary), the two rows of the table that speak of a removed page are
//!   checked with the removal's summary and the page's last text hidden and shown, the offer to restore it never copies what
//!   moderation hid (a form opened before the hiding is refused), and the restore, accepted, brings back the last text it
//!   had;
//! - (f) a claim is retired through the curation proposal, with its reason: it leaves the paragraph and keeps its page, its
//!   sources and its thread, and a reader with no account reads that it is retired and why.
//!
//! **What stands in for nothing:** the accounts are made by the invitation machinery, the rules by the policy form, the
//! protection and every hide by their forms, the appeal by its form and its decision by the admin's. The one thing the test
//! does to a record is in (b): the server's clock is the system's and cannot be moved, so the protection's two times are
//! moved back two days in the store file (the way `World::new` ages the verification record), which is what two days passing
//! does to the record; the server's own reading then judges it ended.
//!
//! Needs prikk and prints `skipping: prikk not on PATH` without it (one line, counted by `EXPECTED_SKIPS`).

use super::api::{API, api, json_field, json_quote};
use super::knowledge::{
    SURVEY, approve_and_accept, attach, curate, ids_after, main_of, signed_into_history,
};
use super::*;
use inklkunn_core::tombstone::surfaces::SURFACES;

const POLICY: &str = "/handbook/-/policy";
const POLICY_FORM: &str = "/handbook/-/policy/edit";
const MODERATE: &str = "/handbook/-/moderate";
const LOG: &str = "/handbook/-/moderation";

// ------------------------------------------------------------------------------------ the transcript

/// The step-numbered, human-readable log of the run. Every line is printed when it is written.
struct Transcript {
    lines: Vec<String>,
    steps: usize,
}

impl Transcript {
    fn new() -> Transcript {
        Transcript {
            lines: Vec::new(),
            steps: 0,
        }
    }

    fn say(&mut self, line: String) {
        eprintln!("{line}");
        self.lines.push(line);
    }

    /// A part of the story.
    fn part(&mut self, name: &str) {
        self.say(format!("\n== {name}"));
    }

    /// A step: what was done.
    fn did(&mut self, what: &str) {
        self.steps += 1;
        self.say(format!("{:>3}. {what}", self.steps));
    }

    /// What was seen, asserted. `context` is shown when it is not so.
    fn saw(&mut self, ok: bool, what: &str, context: &str) {
        assert!(
            ok,
            "step {}: expected to see: {what}\n{context}",
            self.steps
        );
        self.say(format!("       saw: {what}"));
    }

    /// A fact that is not an assertion of its own: a number, an address.
    fn note(&mut self, what: &str) {
        self.say(format!("       note: {what}"));
    }
}

// ------------------------------------------------------------------------------------ what is hidden

/// What a hide can hide of an item.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    Content,
    Summary,
    Author,
}

/// The items the story hides, each by one form.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    /// An accepted proposal's revision and the change it made: text, summary, author (by Zora).
    Revision,
    /// A comment of Zora's on an open proposal: body, author.
    Comment,
    /// A comment of Zora's in a claim's thread: body, author.
    Note,
    /// The summary of the proposal that changed the rules.
    Rules,
    /// The summary of the proposal that removed a page.
    Removal,
    /// The last text of the removed page.
    LastText,
}

/// One thing that must not be served when its item is hidden, and must be where it is shown: a word nothing else says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Mark {
    item: Item,
    part: Part,
    needle: &'static str,
}

const ZORA: &str = "Zora Quokka";
const REV_TEXT: Mark = Mark {
    item: Item::Revision,
    part: Part::Content,
    needle: "QUOKKATEXT",
};
const REV_SUMMARY: Mark = Mark {
    item: Item::Revision,
    part: Part::Summary,
    needle: "QUOKKASUMMARY",
};
const REV_AUTHOR: Mark = Mark {
    item: Item::Revision,
    part: Part::Author,
    needle: ZORA,
};
const COMMENT_BODY: Mark = Mark {
    item: Item::Comment,
    part: Part::Content,
    needle: "QUOKKACOMMENT",
};
const COMMENT_AUTHOR: Mark = Mark {
    item: Item::Comment,
    part: Part::Author,
    needle: ZORA,
};
const NOTE_BODY: Mark = Mark {
    item: Item::Note,
    part: Part::Content,
    needle: "QUOKKANOTE",
};
const NOTE_AUTHOR: Mark = Mark {
    item: Item::Note,
    part: Part::Author,
    needle: ZORA,
};
const RULES_SUMMARY: Mark = Mark {
    item: Item::Rules,
    part: Part::Summary,
    needle: "QUOKKARULES",
};
const REMOVAL_SUMMARY: Mark = Mark {
    item: Item::Removal,
    part: Part::Summary,
    needle: "QUOKKAGONE",
};
const LAST_TEXT: Mark = Mark {
    item: Item::LastText,
    part: Part::Content,
    needle: "QUOKKALAST",
};
const MARKS: [Mark; 10] = [
    REV_TEXT,
    REV_SUMMARY,
    REV_AUTHOR,
    COMMENT_BODY,
    COMMENT_AUTHOR,
    NOTE_BODY,
    NOTE_AUTHOR,
    RULES_SUMMARY,
    REMOVAL_SUMMARY,
    LAST_TEXT,
];

/// How an address is asked.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    /// A page of the app origin.
    Html,
    /// The API (JSON; the Atom feed is asked the same way).
    Api,
    /// The raw source, on the content origin.
    Raw,
}

/// One address of one surface of the table, and what it carries of the hidden items.
#[derive(Clone, Debug)]
struct Address {
    surface: &'static str,
    url: String,
    kind: Kind,
    status: u16,
    /// Who asks: `anonymous`, `maya` (the moderator), or a person whose page it is (`zora`, `noor`).
    viewers: &'static [&'static str],
    /// What the address shows of the hidden items, where they are shown.
    carries: Vec<Mark>,
    /// Words the viewer's own page says whatever is hidden (their own name), not looked for.
    ignore: &'static [&'static str],
}

impl Address {
    fn new(surface: &'static str, url: impl Into<String>, kind: Kind, carries: &[Mark]) -> Address {
        Address {
            surface,
            url: url.into(),
            kind,
            status: 200,
            viewers: &["anonymous", "maya"],
            carries: carries.to_vec(),
            ignore: &[],
        }
    }

    fn status(mut self, status: u16) -> Address {
        self.status = status;
        self
    }

    fn viewers(mut self, viewers: &'static [&'static str]) -> Address {
        self.viewers = viewers;
        self
    }

    fn ignoring(mut self, ignore: &'static [&'static str]) -> Address {
        self.ignore = ignore;
        self
    }
}

fn addr(surface: &'static str, url: impl Into<String>, kind: Kind, carries: &[Mark]) -> Address {
    Address::new(surface, url, kind, carries)
}

/// The words a surface says in place of what is hidden (`TOMBSTONE_WORDS` in the HTML, the API's members).
const TOMBSTONE_WORDS: [&str; 5] = [
    "Hidden by moderation",
    "(summary hidden)",
    "This comment is hidden.",
    "tombstones",
    "\"hidden\"",
];

// ------------------------------------------------------------------------------------ the story

const SAY_TRUST: &str = "It never claims more than it checked, QUOKKATEXT.";

struct Story {
    world: World,
    running: Running,
    t: Transcript,
    anonymous: Browser,
    maya: Person,
    noor: Person,
    quinn: Person,
    ada: Person,
    zora: Person,
    /// The proposal that changed the rules; the block that did.
    rules: String,
    rules_block: String,
    /// The proposal whose revision is hidden (Zora's), and the block of its change, and the block of the change after it.
    revision: String,
    revision_block: String,
    later_block: String,
    /// The open proposal that holds Zora's comment.
    open: String,
    /// The claim, whose thread holds a comment of Zora's.
    claim: String,
    /// The removal story, once it is told.
    removal: Option<Removal>,
    /// Which surfaces were checked hidden, and checked shown.
    hidden_checked: Vec<&'static str>,
    shown_checked: Vec<&'static str>,
}

/// The page that was removed.
struct Removal {
    title: &'static str,
}

impl Story {
    fn person(&mut self, who: &str) -> &mut Browser {
        match who {
            "anonymous" => &mut self.anonymous,
            "maya" => &mut self.maya.browser,
            "noor" => &mut self.noor.browser,
            "quinn" => &mut self.quinn.browser,
            "ada" => &mut self.ada.browser,
            "zora" => &mut self.zora.browser,
            other => panic!("nobody called {other}"),
        }
    }

    /// A reviewer's approval of a proposal, through the page's own form.
    fn approve(&mut self, who: &str, id: &str, said: &str) {
        let browser = self.person(who);
        let page = browser.get(&proposal_page(id));
        let mut form = form_to(&page, &format!("{}/decide", proposal_page(id)));
        form.set("body", said);
        let decided = browser.submit(&form, Some(("op", "approve")));
        assert_eq!(decided.status, 303, "{}\n{}", decided.head, decided.body);
    }

    /// The maintainer's accept form on the proposal's page, sent.
    fn accept(&mut self, id: &str) -> Reply {
        let page = self.maya.browser.get(&proposal_page(id));
        let form = form_to(&page, &format!("{}/accept", proposal_page(id)));
        self.maya.browser.submit(&form, None)
    }

    /// What the rules ask now of every proposal: two approvals besides the proposer's. Quinn gives one, Maya the other and
    /// accepts.
    fn accepted_by_two(&mut self, id: &str) {
        self.approve("quinn", id, "Read it; it says what it says.");
        let accepted = approve_and_accept(&mut self.maya, id);
        signed_into_history(&accepted);
    }

    /// The newest block of the default view.
    fn newest(&self) -> String {
        self.world.blocks().first().cloned().expect("a block")
    }

    /// A script's bearer token.
    fn token(&self, login: &str, scope: Scope) -> String {
        let auth = self.running.services.auth.clone().expect("sign-in");
        auth.issue_token(&self.world.account_of(login), scope, "r5", 3600, now())
            .unwrap()
            .secret
            .as_str()
            .to_owned()
    }

    /// The tip's state id, which a proposal sent through the API names.
    fn tip(&self) -> String {
        let info = api(
            &self.running,
            "GET",
            &format!("{API}/raw/Help"),
            None,
            None,
            None,
        );
        assert_eq!(info.status, 200, "{}", info.body);
        format!("\"{}\"", json_field(&info.body, "state_id").expect("a tip"))
    }

    /// A proposal sent through the API by `who` (a contributor's token): its id and etag.
    fn sent(&mut self, who: &str, summary: &str, files: &str) -> (String, String) {
        let token = self.token(who, Scope::Propose);
        let body = format!(
            "{{\"summary\":{},\"submit\":true,\"files\":[{files}]}}",
            json_quote(summary)
        );
        let made = api(
            &self.running,
            "POST",
            &format!("{API}/proposals"),
            Some(&token),
            Some(&self.tip()),
            Some(&body),
        );
        assert_eq!(made.status, 201, "{}\n{}\n{body}", made.head, made.body);
        (
            json_field(&made.body, "id").expect("proposal id"),
            json_field(&made.body, "etag").expect("etag"),
        )
    }
}

/// The text of a whole page as the person reads it, for a sentence: tags become spaces and entities of the app's own
/// escaping are read back.
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

#[test]
fn governance_and_moderation_work_end_to_end_on_a_real_prikk() {
    let Some(world) = World::new("readiness") else {
        return;
    };
    let running = Running::start(&world);
    let anonymous = running.browser();
    let maya = join(
        &world,
        &running,
        "maya",
        "Maya Maintainer",
        Role::Maintainer,
    );
    let noor = join(
        &world,
        &running,
        "noor",
        "Noor Contributor",
        Role::Contributor,
    );
    let quinn = join(&world, &running, "quinn", "Quinn Reviewer", Role::Reviewer);
    let ada = join(&world, &running, "ada", "Ada Admin", Role::Admin);
    let zora = join(&world, &running, "zora", ZORA, Role::Contributor);
    let mut s = Story {
        world,
        running,
        t: Transcript::new(),
        anonymous,
        maya,
        noor,
        quinn,
        ada,
        zora,
        rules: String::new(),
        rules_block: String::new(),
        revision: String::new(),
        revision_block: String::new(),
        later_block: String::new(),
        open: String::new(),
        claim: String::new(),
        removal: None,
        hidden_checked: Vec::new(),
        shown_checked: Vec::new(),
    };
    s.t.part("The wiki: a public wiki `handbook` on prikk, five people who came by invitation");
    let blocks = s.world.blocks();
    s.t.did("the operator made the wiki and invited five people; each redeemed an invitation and signed in");
    s.t.saw(
        blocks.len() == 1,
        "the repository holds the wiki's first change and nothing else",
        &format!("{blocks:?}"),
    );
    s.t.note("people: Maya (maintainer), Noor (contributor), Quinn (reviewer), Ada (admin), Zora Quokka (contributor)");

    s.rules_change_decides_who_may_accept();
    s.a_protection_is_shown_and_ends();
    s.hidden_items_are_not_served();
    s.an_appeal_is_decided();
    s.a_page_is_removed_and_restored();
    s.a_claim_is_retired();

    s.t.part("The table");
    let parts = s.t.lines.iter().filter(|l| l.starts_with("\n== (")).count();
    s.t.saw(
        parts == 6,
        "the six parts of the story, (a) to (f), were told",
        &format!("{parts}"),
    );
    let surfaces: Vec<&str> = SURFACES.iter().map(|x| x.id).collect();
    for id in &surfaces {
        let (h, v) = (s.hidden_checked.contains(id), s.shown_checked.contains(id));
        s.t.saw(
            h && v,
            &format!("surface {id}: checked with its item hidden and again shown"),
            &format!("hidden {:?}\nshown {:?}", s.hidden_checked, s.shown_checked),
        );
    }
    let steps = s.t.steps;
    s.t.say(format!(
        "\n== end of the transcript: {steps} steps, {} surfaces of the table",
        surfaces.len()
    ));
    s.running.stop();
}

impl Story {
    /// (a)
    fn rules_change_decides_who_may_accept(&mut self) {
        self.t
            .part("(a) an accepted change of the rules changes who may accept (RFC 006 D-4, D-5, D-11; criterion 2)");
        let before = self.anonymous.get(POLICY);
        self.t.did("a reader opens the policy page of the new wiki");
        self.t.saw(
            before.status == 200 && before.body.contains("follows the default rules"),
            "it says the wiki follows the default rules",
            &before.body,
        );
        self.t.saw(
            before.body.contains("No change to the rules is recorded."),
            "and that no change to the rules is recorded",
            &before.body,
        );

        let page = self.maya.browser.get(POLICY_FORM);
        self.t
            .did("Maya, a maintainer, opens the form that changes the rules");
        assert_eq!(page.status, 200, "{}", page.body);
        let mut form = form_to(&page, POLICY_FORM);
        self.t.saw(
            form.get("approvals") == "0",
            "the form is filled from the rules in force: no approvals asked",
            &page.body,
        );
        form.set("approvals", "2").set(
            "summary",
            &format!(
                "{} two approvals before a change is accepted",
                RULES_SUMMARY.needle
            ),
        );
        let sent = self.maya.browser.submit(&form, Some(("act", "send")));
        self.t.did(
            "she asks two approvals from people other than the proposer, and sends it for review",
        );
        assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
        self.rules = proposal_id(&sent.location());
        let first_blocks = self.world.blocks();
        let shown = self.maya.browser.get(&proposal_page(&self.rules));
        let said = plain(&shown.body);
        self.t.saw(
            said.contains("Approvals needed before a proposal is accepted: none → 2 approvals from people other than the proposer")
                && said.contains("Accepting this change takes: maintainer or above, with 0 approvals"),
            "the proposal says the change as a before and an after sentence, and that it is accepted under the old rules (maintainer, no approvals)",
            &said,
        );
        self.t.saw(
            !shown.body.contains("acceptance:") && !shown.body.contains("approvals: 2"),
            "no YAML is shown",
            &shown.body,
        );

        let accept = format!("{}/accept", proposal_page(&self.rules));
        let signed = self.maya.browser.submit(&form_to(&shown, &accept), None);
        self.t.did("Maya accepts it, under the rules there are");
        assert_eq!(signed.status, 200, "{}\n{}", signed.head, signed.body);
        self.t.saw(
            self.world.blocks().len() == first_blocks.len() + 1,
            "one block was signed into the history",
            &format!("{:?}", self.world.blocks()),
        );
        self.rules_block = self.newest();
        self.t
            .note(&format!("the block of the new rules: {}", self.rules_block));

        // A page proposal by Zora, who will be the author of what is hidden later.
        self.t
            .did("Zora, a contributor, proposes a change to the page Help");
        self.revision = propose_edit(
            &mut self.zora,
            "Help",
            &format!("{} say what the wiki checks", REV_SUMMARY.needle),
            |text| {
                assert!(text.contains(TRUST_BEFORE), "{text}");
                text.replacen(TRUST_BEFORE, SAY_TRUST, 1)
            },
        );
        let refused = self.accept(&self.revision.clone());
        self.t.did("Maya tries to accept it at once");
        self.t.saw(
            refused.status == 409 && refused.body.contains("it needs more approvals"),
            "refused: it needs more approvals (409); nothing was signed",
            &refused.body,
        );
        self.t.saw(
            self.world.blocks().len() == first_blocks.len() + 1,
            "the repository is as it was",
            &format!("{:?}", self.world.blocks()),
        );
        // The same, through the API, by its reason.
        let maya_accept = self.token("maya", Scope::Accept);
        let etag = {
            let got = api(
                &self.running,
                "GET",
                &format!("{API}/proposals/{}", self.revision),
                Some(&maya_accept),
                None,
                None,
            );
            assert_eq!(got.status, 200, "{}\n{}", got.head, got.body);
            json_field(&got.body, "etag").expect("etag")
        };
        let by_api = api(
            &self.running,
            "POST",
            &format!("{API}/proposals/{}/acceptances", self.revision),
            Some(&maya_accept),
            Some(&etag),
            Some("{}"),
        );
        self.t
            .did("a script holding Maya's accept token tries the same through the API");
        self.t.saw(
            by_api.status == 403
                && json_field(&by_api.body, "reason").as_deref() == Some("approvals-missing"),
            "refused with the reason `approvals-missing` (403)",
            &format!("{}\n{}", by_api.head, by_api.body),
        );
        self.approve(
            "quinn",
            &self.revision.clone(),
            "Read it; it says what it says.",
        );
        self.t.did("Quinn, a reviewer, approves it");
        let still = self.accept(&self.revision.clone());
        self.t.saw(
            still.status == 409 && still.body.contains("it needs more approvals"),
            "one approval of two is still refused (409)",
            &still.body,
        );
        self.approve("ada", &self.revision.clone(), "Agreed.");
        self.t.did("Ada, an admin, gives the second approval");
        let accepted = self.accept(&self.revision.clone());
        self.t.did("Maya accepts it");
        signed_into_history(&accepted);
        self.t.saw(
            self.world.blocks().len() == first_blocks.len() + 2,
            "it is signed into the history: one more block",
            &format!("{:?}", self.world.blocks()),
        );
        self.revision_block = self.newest();
        let help = page_text(&mut self.anonymous, HELP);
        self.t
            .saw(help.contains(SAY_TRUST), "the page is the page now", &help);

        // The record names the block of the rules.
        let record = self.maya.browser.get(&proposal_page(&self.revision));
        let named = format!(
            "Accepted by Maya Maintainer (maintainer), under the rules as of <a href=\"/handbook/-/change/{}\">{}</a>.",
            self.rules_block,
            self.rules_block.get(..12).unwrap_or_default()
        );
        self.t.saw(
            record.body.contains(&named),
            "the record of that acceptance names the block of the new rules, and links it",
            &record.body,
        );
        let first_record = self.maya.browser.get(&proposal_page(&self.rules));
        self.t.saw(
            first_record
                .body
                .contains("Accepted by Maya Maintainer (maintainer), under the default rules."),
            "and the record of the change of the rules says it was made under the default rules",
            &first_record.body,
        );

        // The policy page says the new rule and lists the change with its proposal.
        let policy = self.anonymous.get(POLICY);
        self.t.did("a reader opens the policy page again");
        let said = plain(&policy.body);
        self.t.saw(
            said.contains("2 approvals from people other than the proposer")
                && said.contains("These are the rules as set by the change"),
            "it says the rule in a sentence, as set by a change",
            &said,
        );
        self.t.saw(
            policy
                .body
                .contains(&format!("href=\"/handbook/-/proposals/{}\"", self.rules))
                && policy
                    .body
                    .contains(&format!("href=\"/handbook/-/change/{}\"", self.rules_block)),
            "and lists the change, linking the proposal that made it and the block",
            &policy.body,
        );
        self.t.saw(
            policy.body.contains(RULES_SUMMARY.needle),
            "the history entry names the proposal by its summary",
            &policy.body,
        );
        self.t.saw(
            !policy.body.contains("acceptance:") && !policy.body.contains("rungs:"),
            "no YAML is shown",
            &policy.body,
        );
    }
}

// ------------------------------------------------------------------------------------ (b) a protection

const PROTECT: &str = "/handbook/-/protect";
const PROTECTED_REASON: &str = "Edit war this week.";

impl Story {
    /// What two days passing does to the record of a protection: both of its times are two days earlier. The events are
    /// append-only (triggers refuse an update), so the trigger is lifted for this one statement and put back as it was.
    fn two_days_pass(&self) {
        let db = rusqlite::Connection::open(self.world.settings.store_path()).unwrap();
        db.busy_timeout(Duration::from_secs(10)).unwrap();
        let trigger: String = db
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'protections_no_update'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        db.execute_batch("DROP TRIGGER protections_no_update")
            .unwrap();
        let moved = db
            .execute(
                "UPDATE protections SET at = at - 172800, expires_at = expires_at - 172800 WHERE expires_at IS NOT NULL",
                [],
            )
            .unwrap();
        db.execute_batch(&trigger).unwrap();
        assert!(moved >= 1, "no protection to age");
    }

    /// (b)
    fn a_protection_is_shown_and_ends(&mut self) {
        self.t.part(
            "(b) a temporary protection is shown, refuses what it asks a role for, and ends (RFC 006 D-7; criterion 3)",
        );
        let policy = self.anonymous.get(POLICY);
        self.t.did("a reader opens the policy page");
        self.t.saw(
            policy.body.contains("No protection is in force."),
            "it says no protection is in force",
            &policy.body,
        );
        let page = self.maya.browser.get(&format!("{PROTECT}?page=Help"));
        self.t
            .did("Maya opens the protection form for the page Help");
        assert_eq!(page.status, 200, "{}", page.body);
        let mut form = form_to(&page, PROTECT);
        form.set("target", "page");
        form.check("require", "rung")
            .set("role", "admin")
            .set("reason", PROTECTED_REASON)
            .set("days", "1");
        let set = self.maya.browser.submit(&form, Some(("act", "set")));
        self.t
            .did("she protects it for one day: only an admin may accept a change to it");
        assert_eq!(set.status, 303, "{}\n{}", set.head, set.body);
        self.t.saw(
            set.location() == format!("{PROTECT}?page=Help"),
            "the form answers with its own page",
            &set.location(),
        );
        for (who, uri) in [("a reader", HELP), ("a reader", POLICY)] {
            let shown = self.anonymous.get(uri);
            let said = plain(&shown.body);
            self.t.did(&format!("{who} opens {uri}"));
            self.t.saw(
                shown.status == 200
                    && said.contains("Temporarily, until")
                    && said.contains(&format!("Reason: {PROTECTED_REASON}")),
                "the protection is shown, as temporary, with its end and its reason",
                &said,
            );
        }
        let shown = self.anonymous.get(HELP);
        let said = plain(&shown.body);
        self.t.saw(
            said.contains("On the page Help.") && shown.body.contains("id=\"protections-heading\""),
            "on the page itself, under Protections, naming the page",
            &said,
        );
        let said = plain(&self.anonymous.get(POLICY).body);
        self.t.saw(
            said.contains("Put in place by a person holding maintainer"),
            "the policy page names the role that set it",
            &said,
        );

        // Noor rewrites what Zora's change wrote.
        self.t
            .did("Noor proposes a later change to the same sentence of Help");
        let later = propose_edit(&mut self.noor, "Help", "Say it as it was", |text| {
            assert!(text.contains(SAY_TRUST), "{text}");
            text.replacen(SAY_TRUST, TRUST_AFTER, 1)
        });
        self.approve("quinn", &later, "Better.");
        self.approve("maya", &later, "Yes.");
        self.t.did("Quinn and Maya approve it");
        let held = self.accept(&later);
        self.t
            .did("Maya, who holds the role the rules ask, tries to accept it");
        let said = plain(&held.body);
        self.t.saw(
            held.status == 403
                && said.contains("Not signed: a protection holds it")
                && said.contains("is accepted only by admin or a higher role"),
            "refused (403): not signed, a protection holds it, and it asks an admin or a higher role",
            &said,
        );
        self.t.saw(
            self.world.blocks().first() == Some(&self.revision_block),
            "nothing was signed: the tip is still Zora's change",
            &format!("{:?}", self.world.blocks()),
        );

        self.two_days_pass();
        self.t
            .did("two days pass (the protection's record is aged: see the file's head)");
        for uri in [HELP, POLICY] {
            let shown = self.anonymous.get(uri);
            let said = plain(&shown.body);
            self.t.saw(
                !said.contains("Temporarily, until") && !said.contains(PROTECTED_REASON),
                &format!("{uri} shows no protection now"),
                &said,
            );
        }
        let said = plain(&self.anonymous.get(POLICY).body);
        self.t.saw(
            said.contains("No protection is in force."),
            "the policy page says none is in force",
            &said,
        );
        let accepted = self.accept(&later);
        self.t
            .did("Maya accepts the same proposal again, with nothing changed but the time");
        signed_into_history(&accepted);
        self.later_block = self.newest();
        self.t.saw(
            self.world.blocks().len() == 4,
            "it is signed into the history",
            &format!("{:?}", self.world.blocks()),
        );
        let help = page_text(&mut self.anonymous, HELP);
        self.t.saw(
            help.contains(TRUST_AFTER) && !help.contains("QUOKKATEXT"),
            "the page no longer holds Zora's words",
            &help,
        );
    }
}

// ------------------------------------------------------------------------------------ the table

/// The query of the first link to the moderation form for an item of `kind` that `page` holds, in the form's own address.
fn moderate_query(page: &Reply, kind: &str) -> String {
    let want = format!("{MODERATE}?kind={kind}");
    let page_body = page.body.replace("&#38;", "&").replace("&amp;", "&");
    let at = page_body
        .find(&want)
        .unwrap_or_else(|| panic!("no link to hide a `{kind}` on the page:\n{}", page.body));
    let rest = page_body.get(at + MODERATE.len() + 1..).unwrap_or_default();
    rest.split('"').next().unwrap_or_default().to_owned()
}

impl Story {
    /// Every address of every row of the table that the story can ask about now, with what each carries.
    fn addresses(&self) -> Vec<Address> {
        use Kind::{Api, Html, Raw};
        let (b0, b1, b2) = (&self.rules_block, &self.revision_block, &self.later_block);
        let (p1, open, claim) = (&self.revision, &self.open, &self.claim);
        let rev = [REV_TEXT, REV_SUMMARY, REV_AUTHOR];
        let rev_who = [REV_SUMMARY, REV_AUTHOR];
        let note = [NOTE_BODY, NOTE_AUTHOR];
        let mut out = vec![
            addr("history-row", "/handbook/-/history/Help", Html, &rev_who),
            addr("history-row", format!("{API}/history/Help"), Api, &rev_who),
            addr(
                "change-view",
                format!("/handbook/-/change/{b1}"),
                Html,
                &rev_who,
            ),
            addr(
                "change-view",
                format!("{API}/revisions/{b1}"),
                Api,
                &rev_who,
            ),
            addr(
                "change-view",
                format!("{API}/revisions/{b1}?title=Help"),
                Api,
                &[REV_TEXT],
            ),
            // The change after it holds the same line, removed: it shows no part of the hidden one.
            addr("change-view", format!("/handbook/-/change/{b2}"), Html, &[]),
            addr(
                "diff",
                format!("/handbook/-/diff/{b0}/{b1}/Help"),
                Html,
                &[REV_TEXT],
            ),
            addr(
                "diff",
                format!("/handbook/-/diff/{b1}/{b2}/Help"),
                Html,
                &[REV_TEXT],
            ),
            addr(
                "diff",
                format!("{API}/diff?from={b0}&to={b1}&path=Help"),
                Api,
                &[REV_TEXT],
            ),
            addr(
                "page-as-of",
                format!("/handbook/-/at/{b1}/Help"),
                Html,
                &[REV_TEXT],
            ),
            addr("why-is-this-here", "/handbook/-/why/Help", Html, &[]),
            addr(
                "why-is-this-here",
                format!("{API}/provenance/Help"),
                Api,
                &[],
            ),
            addr(
                "compare-views",
                "/handbook/-/views/compare?a=main&b=alt",
                Html,
                &[],
            ),
            addr(
                "compare-views",
                format!("{API}/views/compare?b=alt"),
                Api,
                &[],
            ),
            addr("recent-changes", "/handbook/-/changes", Html, &rev_who),
            addr(
                "recent-changes",
                "/handbook/-/changes.atom",
                Api,
                &[REV_SUMMARY],
            ),
            addr("recent-changes", format!("{API}/changes"), Api, &rev_who),
            addr(
                "proposal-page",
                format!("/handbook/-/proposals/{p1}"),
                Html,
                &rev,
            ),
            addr(
                "proposal-page",
                format!("{API}/proposals/{p1}"),
                Api,
                &[REV_SUMMARY],
            ),
            addr(
                "proposal-comments",
                format!("/handbook/-/proposals/{open}"),
                Html,
                &[COMMENT_BODY, COMMENT_AUTHOR],
            ),
            addr("proposal-list", "/handbook/-/proposals", Html, &rev_who),
            addr(
                "proposal-list",
                "/handbook/-/discussion/Help",
                Html,
                &[COMMENT_AUTHOR],
            ),
            addr(
                "claim-thread",
                format!("/handbook/-/claim/{claim}"),
                Html,
                &note,
            ),
            addr("inbox", "/-/inbox", Html, &[REV_SUMMARY])
                .viewers(&["zora"])
                .ignoring(&[ZORA]),
            addr("inbox", "/-/inbox", Html, &[COMMENT_AUTHOR]).viewers(&["noor"]),
            addr("rules-history", POLICY, Html, &[RULES_SUMMARY]),
        ];
        if let Some(gone) = &self.removal {
            let title = gone.title;
            out.extend([
                addr(
                    "removed-page",
                    format!("/handbook/{title}"),
                    Html,
                    &[REMOVAL_SUMMARY],
                )
                .status(410),
                addr("removed-page", format!("/handbook/-/raw/{title}"), Raw, &[]).status(410),
                addr(
                    "removed-page",
                    format!("{API}/pages/{title}"),
                    Api,
                    &[REMOVAL_SUMMARY],
                )
                .status(410),
                addr("removed-page", format!("{API}/raw/{title}"), Api, &[]).status(410),
                addr(
                    "removed-page",
                    format!("{API}/fragment/{title}"),
                    Api,
                    &[REMOVAL_SUMMARY],
                )
                .status(410),
                addr(
                    "restore-offer",
                    format!("/handbook/-/restore/{title}"),
                    Html,
                    &[],
                )
                .viewers(&["noor"]),
            ]);
        }
        out
    }

    /// One address asked as one viewer.
    fn fetch(&mut self, who: &str, a: &Address) -> Reply {
        let json = [("Accept", "application/json")];
        match a.kind {
            Kind::Raw => wire(
                self.running.port,
                "GET",
                &a.url,
                &[(
                    "Host".into(),
                    format!("localhost:{}", self.running.content_port),
                )],
                &[],
            ),
            Kind::Api => self.person(who).visit("GET", &a.url, None, &json),
            Kind::Html => self.person(who).get(&a.url),
        }
    }

    /// **The table, iterated.** Every address that can carry something of `items` (and, when `only` is not empty, of those
    /// rows), asked by each viewer it names. Hidden: not one byte of the answer or its headers holds a mark of the items, and
    /// each address that would show one says why in its place. Shown: each mark the address carries is there.
    fn check_table(&mut self, items: &[Item], hidden: bool, only: &[&str]) {
        let marks: Vec<Mark> = MARKS
            .iter()
            .copied()
            .filter(|m| items.contains(&m.item))
            .collect();
        let todo: Vec<Address> = self
            .addresses()
            .into_iter()
            .filter(|a| only.is_empty() || only.contains(&a.surface))
            .collect();
        let state = if hidden { "hidden" } else { "shown" };
        let mut rows: Vec<(&'static str, usize, usize)> = Vec::new();
        let mut problems: Vec<String> = Vec::new();
        for a in &todo {
            let carried: Vec<Mark> = a
                .carries
                .iter()
                .copied()
                .filter(|m| items.contains(&m.item))
                .collect();
            // A word that is also the name of someone an item that is not under test shows here is not looked for.
            let others: Vec<&str> = a
                .carries
                .iter()
                .filter(|m| !items.contains(&m.item))
                .map(|m| m.needle)
                .collect();
            let mut fetched = 0;
            for who in a.viewers {
                let r = self.fetch(who, a);
                fetched += 1;
                let all = format!("{}\n{}", r.head, r.body);
                if r.status != a.status {
                    problems.push(format!(
                        "surface {}: {} as {who} answers {} and not {}",
                        a.surface, a.url, r.status, a.status
                    ));
                    continue;
                }
                if hidden {
                    for m in &marks {
                        if a.ignore.contains(&m.needle) || others.contains(&m.needle) {
                            continue;
                        }
                        if let Some(at) = all.find(m.needle) {
                            let from = at.saturating_sub(120);
                            problems.push(format!(
                                "surface {}: {} as {who} serves `{}` ({:?} of {:?}) while it is hidden: ...{}...",
                                a.surface,
                                a.url,
                                m.needle,
                                m.part,
                                m.item,
                                plain(all.get(from..(at + 120).min(all.len())).unwrap_or_default())
                            ));
                        }
                    }
                    if !carried.is_empty() && !TOMBSTONE_WORDS.iter().any(|w| all.contains(w)) {
                        problems.push(format!(
                            "surface {}: {} as {who} does not say why in the place of what is hidden",
                            a.surface, a.url
                        ));
                    }
                } else {
                    for m in &carried {
                        if !all.contains(m.needle) {
                            problems.push(format!(
                                "surface {}: {} as {who} does not show `{}` ({:?} of {:?}) where it is shown",
                                a.surface, a.url, m.needle, m.part, m.item
                            ));
                        }
                    }
                }
            }
            if let Some(row) = rows.iter_mut().find(|r| r.0 == a.surface) {
                row.1 += 1;
                row.2 += fetched;
            } else {
                rows.push((a.surface, 1, fetched));
            }
            let covers = !carried.is_empty() || a.carries.is_empty();
            if covers {
                let seen = if hidden {
                    &mut self.hidden_checked
                } else {
                    &mut self.shown_checked
                };
                if !seen.contains(&a.surface) {
                    seen.push(a.surface);
                }
            }
        }
        assert!(
            problems.is_empty(),
            "{} problems with the table ({state}):\n{}",
            problems.len(),
            problems.join("\n")
        );
        for (surface, addresses, fetches) in rows {
            let row = SURFACES.iter().find(|s| s.id == surface).expect("a row");
            let parts: Vec<&str> = [
                (row.shows.content, "content"),
                (row.shows.summary, "summary"),
                (row.shows.author, "author"),
            ]
            .iter()
            .filter(|p| p.0)
            .map(|p| p.1)
            .collect();
            self.t.saw(
                true,
                &format!(
                    "{surface} ({}): {addresses} addresses, {fetches} answers, {state}",
                    if parts.is_empty() {
                        "shows none of an item".to_owned()
                    } else {
                        format!("can show: {}", parts.join(", "))
                    }
                ),
                "",
            );
        }
    }

    /// Hide `parts` of the item whose form is at `query` as Maya, through the form.
    fn hide(&mut self, query: &str, parts: &[&str]) {
        let page = self.maya.browser.get(&format!("{MODERATE}?{query}"));
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
        let hid = self.maya.browser.submit(&form, None);
        assert_eq!(hid.status, 200, "{query}: {}\n{}", hid.head, hid.body);
    }

    /// Every hide in force is undone, through the moderation log's own form.
    fn unhide_all(&mut self) -> usize {
        let mut undone = 0;
        loop {
            let log = self.maya.browser.get(LOG);
            assert_eq!(log.status, 200, "{}", log.body);
            let Some(form) = forms_of(&log.body)
                .into_iter()
                .find(|f| f.action == format!("{MODERATE}/unhide"))
            else {
                return undone;
            };
            let sent = self.maya.browser.submit(&form, None);
            assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
            undone += 1;
        }
    }

    /// (c)
    fn hidden_items_are_not_served(&mut self) {
        self.t.part("(c) a hidden revision, comment, claim comment and summary are served on no surface, and the unhide brings them back (RFC 006 D-8, D-12; criteria 8, 9; INV-13)");
        // What there is to hide.
        self.t.did("Noor attaches a source to a paragraph of Help; it is reviewed and accepted: that is a claim");
        let source = attach(
            &mut self.noor,
            None,
            "supports",
            &SURVEY,
            "Cite the link syntax",
        );
        self.accepted_by_two(&source);
        let help = self.anonymous.get(HELP);
        self.claim = ids_after(&help.body, "/handbook/-/claim/")
            .into_iter()
            .next()
            .expect("the claim link names the claim");
        let claim_page = format!("/handbook/-/claim/{}", self.claim);
        self.t.did("Zora writes in the claim's thread");
        let page = self.zora.browser.get(&claim_page);
        let mut form = form_to(&page, &format!("{claim_page}/discuss"));
        form.set(
            "body",
            &format!("{} where in chapter 2 is this said?", NOTE_BODY.needle),
        );
        let said = self.zora.browser.submit(&form, None);
        assert_eq!(said.status, 303, "{}\n{}", said.head, said.body);
        self.t
            .did("Noor opens a proposal that stays open, and Zora comments on it");
        self.open = propose_edit(&mut self.noor, "Help", "Name the licence's place", |text| {
            assert!(text.contains("Unless a page says otherwise"), "{text}");
            text.replacen(
                "Unless a page says otherwise",
                "Unless a page says otherwise in its header",
                1,
            )
        });
        let said = comment(
            &mut self.zora,
            &self.open.clone(),
            &format!("{} this reads better with the header", COMMENT_BODY.needle),
        );
        assert_eq!(said.status, 303, "{}\n{}", said.head, said.body);
        self.t.did(
            "Quinn, a reviewer, makes a second view of the wiki to compare the default view with",
        );
        let page = self.quinn.browser.get("/handbook/-/views/new");
        let mut form = form_to(&page, "/handbook/-/views/new");
        form.set("name", "alt")
            .check("kind", "interpretation")
            .set("from", "main");
        let made = self.quinn.browser.submit(&form, None);
        assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);

        let all = [Item::Revision, Item::Comment, Item::Note, Item::Rules];
        self.t.did(
            "before anything is hidden: every row of the table, as a reader and as the moderator",
        );
        self.check_table(&all, false, &[]);

        // The moderator's own links, on the pages that carry the items.
        let on_open = self.maya.browser.get(&proposal_page(&self.open));
        let comment_query = moderate_query(&on_open, "proposal-comment");
        let on_claim = self.maya.browser.get(&claim_page);
        let note_query = moderate_query(&on_claim, "claim-comment");
        let revision_query = format!("kind=proposal-revision&proposal={}&seq=1", self.revision);
        let rules_query = format!("kind=proposal-revision&proposal={}&seq=1", self.rules);
        let log_before = self.maya.browser.get(LOG);
        self.t.saw(
            !forms_of(&log_before.body)
                .iter()
                .any(|f| f.action == format!("{MODERATE}/unhide")),
            "the moderation log has nothing to restore",
            &log_before.body,
        );
        self.t.did("Maya hides, through the moderation form: the text, summary and author of Zora's revision; the body and author of Zora's comment; the body and author of her comment in the claim's thread; and the summary of the proposal that changed the rules");
        self.hide(
            &revision_query,
            &["part_content", "part_summary", "part_author"],
        );
        self.hide(&comment_query, &["part_content", "part_author"]);
        self.hide(&note_query, &["part_content", "part_author"]);
        self.hide(&rules_query, &["part_summary"]);
        self.t
            .did("every row of the table again, hidden, as a reader and as the moderator");
        self.check_table(&all, true, &[]);
        let log = self.anonymous.get(LOG);
        self.t.saw(
            log.status == 200
                && !log.body.contains(ZORA)
                && !log.body.contains("QUOKKA"),
            "the moderation log (public) names the items by address and category, never the words and never a name",
            &log.body,
        );

        let undone = self.unhide_all();
        self.t
            .did("Maya restores each hide from the moderation log");
        self.t.saw(
            undone >= 4,
            &format!(
                "{undone} hides were undone (a revision's text and its author are separate records)"
            ),
            "",
        );
        self.t
            .did("every row of the table once more, shown, as a reader and as the moderator");
        self.check_table(&all, false, &[]);
    }
}

// ------------------------------------------------------------------------------------ (d) an appeal

const APPEALS: &str = "/handbook/-/appeals";
const GRIEVANCE: &str = "I wrote this myself and it names nobody; please look again.";
const REASON: &str = "The record supports the appeal: nothing in it is personal.";

impl Story {
    /// The pages of the hidden revision as a reader gets them, to be compared byte for byte.
    fn reader_pages_of_the_revision(&mut self) -> Vec<(String, String)> {
        let uris = [
            "/handbook/-/history/Help".to_owned(),
            format!("/handbook/-/change/{}", self.revision_block),
            format!("/handbook/-/at/{}/Help", self.revision_block),
            format!(
                "/handbook/-/diff/{}/{}/Help",
                self.rules_block, self.revision_block
            ),
            format!("/handbook/-/proposals/{}", self.revision),
            "/handbook/-/changes".to_owned(),
        ];
        uris.into_iter()
            .map(|uri| {
                let page = self.anonymous.get(&uri);
                assert_eq!(page.status, 200, "{uri}: {}", page.body);
                (uri, page.body)
            })
            .collect()
    }

    /// (d)
    fn an_appeal_is_decided(&mut self) {
        self.t.part("(d) the author appeals a hiding and a person who did not hide it reverses it (RFC 006 D-9; criterion 9)");
        let before = self.reader_pages_of_the_revision();
        let query = format!("kind=proposal-revision&proposal={}&seq=1", self.revision);
        self.hide(&query, &["part_content", "part_summary", "part_author"]);
        self.t
            .did("Maya hides Zora's revision again: its text, summary and author");
        self.t
            .did("every row, hidden, as a reader and as the moderator");
        self.check_table(&[Item::Revision], true, &[]);

        // The author is offered *contest this* for what was hidden of what is theirs, on their own page of appeals.
        let prefix = "/handbook/-/appeals/new?kind=tombstone&id=";
        let page = self.zora.browser.get(&format!("{APPEALS}/mine"));
        let shown = page.body.replace("&#38;", "&");
        self.t.did("Zora opens her own page of appeals");
        self.t.saw(
            shown.contains(prefix) && shown.contains("Contest this"),
            "it offers *Contest this* for the hiding of what is hers",
            &shown,
        );
        let id = shown
            .split(prefix)
            .nth(1)
            .and_then(|rest| rest.split(['"', '&']).next())
            .expect("the hiding's id")
            .to_owned();
        let others = self.noor.browser.get(&format!("{APPEALS}/mine"));
        self.t.saw(
            !others.body.contains(prefix),
            "and nobody else is offered it",
            &others.body,
        );
        let form_page = self.zora.browser.get(&format!("{prefix}{id}"));
        assert_eq!(form_page.status, 200, "{}", form_page.body);
        let mut form = form_to(&form_page, &format!("{APPEALS}/new"));
        form.set("text", GRIEVANCE);
        let filed = self.zora.browser.submit(&form, None);
        self.t.did("she files an appeal, in her own words");
        assert_eq!(filed.status, 303, "{}\n{}", filed.head, filed.body);
        let log = self.anonymous.get(LOG);
        self.t.saw(
            plain(&log.body)
                .contains("This hiding was appealed. The appeal is waiting for a decision.")
                && !log.body.contains(GRIEVANCE),
            "the public moderation log says that it was appealed and nothing of what she wrote",
            &log.body,
        );
        let queue = self.ada.browser.get(APPEALS);
        self.t
            .did("Ada, an admin who did not hide it, opens the appeals page");
        self.t.saw(
            queue.status == 200 && queue.body.contains(GRIEVANCE) && queue.body.contains(ZORA),
            "she reads the appeal and who made it",
            &queue.body,
        );
        let denied = self.noor.browser.get(APPEALS);
        self.t.saw(
            denied.status >= 400 && !denied.body.contains(GRIEVANCE),
            "nobody else reads it",
            &denied.body,
        );
        let mut decide = form_to(&queue, &format!("{APPEALS}/decide"));
        decide.set("reason", REASON);
        let press = ("decision", "overturned");
        let decision = if decide
            .buttons
            .iter()
            .any(|b| b.0 == press.0 && b.1 == press.1)
        {
            self.ada.browser.submit(&decide, Some(press))
        } else {
            decide.check(press.0, press.1);
            self.ada.browser.submit(&decide, None)
        };
        self.t.did("she overturns it, with a reason");
        assert_eq!(decision.status, 303, "{}\n{}", decision.head, decision.body);
        self.t.saw(
            decision.location() == format!("{APPEALS}?done=overturned"),
            "the decision is recorded: overturned",
            &decision.location(),
        );
        let done = self.ada.browser.get(&format!("{APPEALS}?done=overturned"));
        let said = plain(&done.body);
        self.t.saw(
            said.contains("The decision was recorded: the action was reversed.")
                && said.contains(REASON),
            "the appeals page says the action was reversed, and why",
            &said,
        );
        let under = format!(
            "Decided as admin, under the rules as of {}",
            self.rules_block.get(..12).unwrap_or_default()
        );
        self.t.saw(
            said.contains(&under),
            "and the rung that decided and the block of the rules it decided under",
            &said,
        );
        let mine = self.zora.browser.get(&format!("{APPEALS}/mine"));
        let said = plain(&mine.body);
        self.t.saw(
            said.contains("the action was reversed.")
                && said.contains(REASON)
                && said.contains(&under),
            "her own page says the same, with the rung and the block",
            &said,
        );
        let log = plain(&self.anonymous.get(LOG).body);
        self.t.saw(
            log.contains("The appeal was decided: the hiding was reversed.")
                && log.contains("Restored because an appeal was decided.")
                && !log.contains(REASON),
            "the moderation log says the hiding was reversed by an appeal, and not the words",
            &log,
        );
        self.t
            .did("every row, shown, as a reader and as the moderator");
        self.check_table(&[Item::Revision], false, &[]);
        let after = self.reader_pages_of_the_revision();
        self.t.saw(
            before == after,
            &format!(
                "the {} pages of the revision are as they were, byte for byte",
                before.len()
            ),
            "a page differs after the appeal",
        );
        let help = page_text(&mut self.anonymous, HELP);
        self.t.saw(
            !help.contains("QUOKKATEXT"),
            "and the page itself is Noor's later text",
            &help,
        );
    }
}

// ------------------------------------------------------------------------------------ (e) removal and restoration

const GONE: &str = "/handbook/Scratch";
const RESTORE: &str = "/handbook/-/restore/Scratch";

fn scratch(body: &str) -> String {
    format!("---\ntitle: Scratch\n---\n\n{body}\n")
}

impl Story {
    /// (e)
    fn a_page_is_removed_and_restored(&mut self) {
        self.t.part("(e) a page is removed and restored, and what moderation hid does not come back with it (RFC 006 D-10; criterion 10)");
        self.t.did("Noor starts a page, Scratch, and then rewrites it, through the API; each is reviewed and accepted");
        let (first, _) = self.sent(
            "noor",
            "Start the scratch page",
            &format!(
                "{{\"title\":\"Scratch\",\"text\":{}}}",
                json_quote(&scratch("The first text of the scratch page."))
            ),
        );
        self.accepted_by_two(&first);
        let (last, _) = self.sent(
            "noor",
            "Say what the scratch page is for",
            &format!(
                "{{\"path\":\"pages/Scratch.md\",\"text\":{}}}",
                json_quote(&scratch(&format!(
                    "{} is what the scratch page ends up saying.",
                    LAST_TEXT.needle
                )))
            ),
        );
        self.accepted_by_two(&last);
        let text = page_text(&mut self.anonymous, GONE);
        self.t.saw(
            text.contains(LAST_TEXT.needle),
            "the page reads as its last text",
            &text,
        );

        let (gone, _) = self.sent(
            "noor",
            &format!("{} the scratch page is out of date", REMOVAL_SUMMARY.needle),
            "{\"path\":\"pages/Scratch.md\",\"text\":null}",
        );
        self.accepted_by_two(&gone);
        self.t
            .did("Noor proposes to remove the page, and it is accepted");
        let removed = self.anonymous.get(GONE);
        let said = plain(&removed.body);
        self.t.saw(
            removed.status == 410 && said.contains(REMOVAL_SUMMARY.needle),
            "the page answers 410 Gone, and says why, in the removal's own words",
            &said,
        );
        self.t.note(&format!(
            "the 410 page says: {}",
            said.chars()
                .skip(said.find("Scratch").unwrap_or(0))
                .take(300)
                .collect::<String>()
        ));
        self.removal = Some(Removal { title: "Scratch" });
        let items = [Item::Removal, Item::LastText];
        let rows = ["removed-page", "restore-offer"];
        self.t.did("the two rows of the table that speak of a removed page, as a reader and as the person who may restore it");
        self.check_table(&items, false, &rows);

        let offer = self.noor.browser.get(RESTORE);
        let stale = form_to(&offer, RESTORE);
        self.t
            .did("Maya hides the removal's summary and the text the page ended with");
        self.hide(
            &format!("kind=proposal-revision&proposal={gone}&seq=1"),
            &["part_summary"],
        );
        self.hide(
            &format!("kind=proposal-revision&proposal={last}&seq=1"),
            &["part_content"],
        );
        self.check_table(&items, true, &rows);
        let offer = self.noor.browser.get(RESTORE);
        self.t.saw(
            offer.status == 200 && !offer.body.contains(LAST_TEXT.needle),
            "the offer to restore does not copy the hidden text",
            &plain(&offer.body),
        );
        self.t.saw(
            plain(main_of(&offer)).contains("The last text of Scratch was hidden by moderation, so this form cannot restore it."),
            "it says so, and offers a new page with the title instead",
            &plain(main_of(&offer)),
        );
        let blocks = self.world.blocks().len();
        let refused = self.noor.browser.submit(&stale, None);
        self.t
            .did("Noor sends the form she opened before the hiding");
        self.t.saw(
            refused.status == 409 && !refused.body.contains(LAST_TEXT.needle),
            "it is refused (409): a draft that carries the hidden text is never made",
            &refused.body,
        );
        self.t.saw(
            self.world.blocks().len() == blocks,
            "nothing was written",
            &format!("{:?}", self.world.blocks()),
        );
        let undone = self.unhide_all();
        self.t.note(&format!("{undone} hides undone"));
        self.t.did("both are restored; the rows again, shown");
        self.check_table(&items, false, &rows);

        // The way back.
        let offer = self.noor.browser.get(RESTORE);
        self.t.did("Noor opens the offer to restore the page");
        assert_eq!(offer.status, 200, "{}", offer.body);
        let form = form_to(&offer, RESTORE);
        let made = self.noor.browser.submit(&form, None);
        self.t.did(
            "she asks for the page back: the form makes a draft of the way back, and sends nothing",
        );
        assert_eq!(made.status, 303, "{}\n{}", made.head, made.body);
        let draft = proposal_id(&made.location());
        let page = self.noor.browser.get(&proposal_page(&draft));
        let said = plain(main_of(&page));
        self.t.saw(
            said.contains("Draft")
                && said.contains(LAST_TEXT.needle)
                && said.contains("nobody has been asked to review it"),
            "the draft is hers alone, carries the page's last text, and has been shown to nobody",
            &said,
        );
        let edit = self
            .noor
            .browser
            .get(&format!("{}/edit", proposal_page(&draft)));
        assert_eq!(edit.status, 200, "{}", edit.body);
        let form = form_to(&edit, &format!("{}/edit", proposal_page(&draft)));
        let sent = self.noor.browser.submit(&form, Some(("op", "submit")));
        self.t.did("she sends it for review");
        assert_eq!(sent.status, 303, "{}\n{}", sent.head, sent.body);
        self.accepted_by_two(&draft);
        self.t.did("it is reviewed and accepted like any change");
        let back = self.anonymous.get(GONE);
        let said = plain(main_of(&back));
        self.t.saw(
            back.status == 200 && said.contains(LAST_TEXT.needle) && !said.contains("removed"),
            "the page is back, with the last text it had, for a reader",
            &said,
        );
        let history = plain(&self.anonymous.get("/handbook/-/history/Scratch").body);
        self.t.saw(
            history.contains("Undo"),
            "its history shows the way back as a change of its own",
            &history,
        );
        self.removal = None;
    }
}

// ------------------------------------------------------------------------------------ (f) a claim is retired

const RETIRED_BECAUSE: &str = "The survey was withdrawn by its author.";

impl Story {
    /// (f)
    fn a_claim_is_retired(&mut self) {
        self.t.part("(f) a claim is retired, with its reason, through the curation proposal (RFC 006 D-10; criterion 11)");
        let claim_page = format!("/handbook/-/claim/{}", self.claim);
        let help = self.anonymous.get(HELP);
        self.t.saw(
            help.body.matches("class=\"claim-link\"").count() == 1,
            "before: the paragraph carries the claim's link",
            &help.body,
        );
        let proposal = curate(
            &mut self.noor,
            &self.claim.clone(),
            None,
            |f| {
                f.set("retire", "retire")
                    .set("retire_reason", RETIRED_BECAUSE);
            },
            "Retire the claim: its source was withdrawn",
        );
        self.t.did(
            "Noor opens the curation form of the claim, chooses *Retire it* and gives a reason",
        );
        let held = self.noor.browser.get(&proposal_page(&proposal));
        self.t.saw(
            plain(&held.body).contains(&format!("Why it is retired becomes: {RETIRED_BECAUSE}")),
            "the proposal says what it does: why the claim is retired, in the reason given",
            &plain(main_of(&held)),
        );
        let still = self.anonymous.get(&claim_page);
        self.t.saw(
            !main_of(&still).contains("This claim is retired."),
            "while it is held, the claim is as it was",
            main_of(&still),
        );
        self.accepted_by_two(&proposal);
        self.t.did("it is reviewed and accepted");

        let seen = self.anonymous.get(&claim_page);
        let said = plain(main_of(&seen));
        self.t
            .did("a reader with no account opens the claim's page");
        self.t.saw(
            seen.status == 200 && said.contains("This claim is retired."),
            "it says that the claim is retired",
            &said,
        );
        self.t.saw(
            said.contains(&format!("Why it was retired: {RETIRED_BECAUSE}")),
            "and why, in the reason it was retired with",
            &said,
        );
        self.t.saw(
            said.contains("A survey of wiki link syntax") && said.contains(NOTE_BODY.needle),
            "the page keeps its source and its thread",
            &said,
        );
        let help = self.anonymous.get(HELP);
        self.t.saw(
            help.body.matches("class=\"claim-link\"").count() == 0
                && !help.body.contains("/-/claim/"),
            "the paragraph it was said in no longer offers it",
            &help.body,
        );
        let api = wire(
            self.running.port,
            "GET",
            &format!("{API}/claims/{}", self.claim),
            &[("Accept".into(), "application/json".into())],
            &[],
        );
        self.t.saw(
            api.status == 200 && api.body.contains("retired"),
            "the API gives the same claim as retired",
            &api.body,
        );
    }
}
