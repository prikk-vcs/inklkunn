//! A removed page and the offer to restore it meet the tombstones (RFC 006 D-8, D-10; handoff G-T4c; INV-13; threat model C-15d):
//! the page that says a page was removed (HTML, the raw source on the content origin, the API's `page-gone`) shows the
//! **tombstone** in place of a hidden summary and never the words, and the restore form never copies **hidden text** into a
//! draft: it refuses a rollback and the page's last text when the content of the revision that wrote the text is hidden.
//! The records are the in-memory store's and the repository is a double (scripted history), so these run in both test runs;
//! the real prikk is in `inklkunn-core/tests/soft_deletion.rs`. Every text that must never appear carries the word `ZEBRA`
//! (or the name of the proposer, where the name is what is hidden).

use axum::http::StatusCode;
use inklkunn_store::{
    AccountId, FileChange, GovernanceStore, ProposalFilter, ProposalId, ProposalStore, Role,
    TombstoneAct,
};

use super::{HIDING, nothing_hidden_in};
use crate::common::{
    Answer, CONTENT, World, accept_by_hand, add_person, block_id, get_as, op, person, post_form,
    request, service_text, shown, with_accounts,
};

const HOME: &str = "pages/Home.md";
const GUIDE: &str = "pages/Guide.md";
const NOTES: &str = "pages/Notes.md";
const DRAFT: &str = "pages/Draft.md";

fn text(title: &str, body: &str) -> String {
    format!("---\ntitle: {title}\n---\n\n{body}\n")
}

/// The summary of the removing proposal: what must not be shown when it is hidden.
pub(crate) const REMOVAL_SUMMARY: &str = "ZEBRASUMMARY the guide is out of date";
/// The proposer of the removal, by the name the message records: what must not be shown when the author is hidden.
pub(crate) const PROPOSER: &str = "Zed Wolfhound";
/// The page's last text: what must never come back when it is hidden.
pub(crate) const LAST_TEXT: &str = "ZEBRATEXT the guide says this last.";
/// The last text of the second page of a change that removes two.
pub(crate) const NOTES_LAST: &str = "ZEBRANOTES the notes say this last.";

/// How the history ends.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// 1 creation (`Home`, `Guide`), 2 an edit of `Guide` (its last text), 3 an edit of `Home`, 4 the removal of `Guide`: a
    /// rollback of 4 merges cleanly. The writer of the text (2) is not the removal's parent (3).
    Clean,
    /// As `Clean`, but 4 also edits `Home` and 5 edits it again: a rollback of 4 would overlap, so the way back is the last
    /// text. Change 3, the parent of 4, wrote `Home` only, and `Home` is not what it can hide.
    Blocked,
    /// 1 creation (`Home`, `Guide`, `Notes`), 2 an edit of `Guide`, 3 an edit of `Notes`, 4 the removal of both: one rollback
    /// that writes two pages.
    Two,
    /// 1 creation (`Home`, `Draft`), 2 an edit of `Draft` (its last text), 3 a move of `Draft` to `Guide`, 4 the removal of
    /// `Guide`: the change before the removal wrote no text, the one before it did.
    Moved,
}

pub(crate) struct Gone {
    pub(crate) w: World,
    pub(crate) maintainer: AccountId,
    /// The proposer of the removal, a contributor: whoever may propose.
    pub(crate) author: AccountId,
    pub(crate) reader: AccountId,
    pub(crate) proposal: ProposalId,
}

fn message(id: &ProposalId, proposer: &AccountId, acceptor: &AccountId) -> String {
    use inklkunn_core::message::{Actor, BlockId, Message, Summary};
    use inklkunn_store::DisplayName;
    let actor = |id: &AccountId, name: &str| Actor::Account {
        id: id.clone(),
        name: DisplayName::new(name).expect("name"),
    };
    Message::proposal(
        Summary::new(REMOVAL_SUMMARY).expect("summary"),
        id.clone(),
        actor(proposer, PROPOSER),
        actor(acceptor, "Mika Ito"),
        BlockId::new(&block_id(1)).expect("block"),
    )
    .expect("a message")
    .to_text()
}

/// The wiki of the shape, its people (a maintainer who moderates, a contributor who proposed the removal, an observer) and the
/// removing proposal, which this wiki keeps.
pub(crate) fn gone(shape: Shape) -> Gone {
    let first = if shape == Shape::Moved { DRAFT } else { GUIDE };
    let mut files = vec![
        (HOME, text("Home", "Welcome.")),
        (first, text("Guide", "The guide.")),
    ];
    if shape == Shape::Two {
        files.push((NOTES, text("Notes", "The notes.")));
    }
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let w = with_accounts(World::new("handbook", "The Handbook", &borrowed));
    let (maintainer, author, reader) = (person(4), person(1), person(6));
    add_person(&w, &maintainer, "Mika Ito", Some(Role::Maintainer));
    add_person(&w, &author, PROPOSER, Some(Role::Contributor));
    add_person(&w, &reader, "Olu Reader", Some(Role::Observer));
    let mut created = vec![op("create-file", &[HOME]), op("create-file", &[first])];
    if shape == Shape::Two {
        created.push(op("create-file", &[NOTES]));
    }
    w.fake.say(1, "Create the handbook");
    w.fake.show(1, shown(created));
    // 2: the last text of the guide.
    w.fake.set(first, &text("Guide", LAST_TEXT));
    w.fake.say(2, &service_text("Improve the guide"));
    w.fake.show(2, shown(vec![op("edit-text", &[first])]));
    // 3: another page changes (the guide's writer is not this change); in `Two`, the notes' last text; in `Moved`, the page
    // is moved to its title.
    if shape == Shape::Moved {
        w.fake.rename(DRAFT, GUIDE);
        w.fake.say(3, &service_text("Move the draft"));
        w.fake
            .show(3, shown(vec![op("rename-path", &[DRAFT, GUIDE])]));
    } else if shape == Shape::Two {
        w.fake.set(NOTES, &text("Notes", NOTES_LAST));
        w.fake.say(3, &service_text("Improve the notes"));
        w.fake.show(3, shown(vec![op("edit-text", &[NOTES])]));
    } else {
        w.fake.set(HOME, &text("Home", "Welcome again."));
        w.fake.say(3, &service_text("Tidy the home page"));
        w.fake.show(3, shown(vec![op("edit-text", &[HOME])]));
    }
    // 4: the removal, by an accepted proposal.
    let record = w.hold_unjudged(
        &author,
        REMOVAL_SUMMARY,
        vec![FileChange {
            path: GUIDE.to_owned(),
            text: None,
        }],
    );
    accept_by_hand(&w, &record.id, &maintainer, &author);
    w.fake.say(4, &message(&record.id, &author, &maintainer));
    match shape {
        Shape::Clean | Shape::Moved => {
            w.fake.remove(GUIDE);
            w.fake.show(4, shown(vec![op("delete-node", &[GUIDE])]));
        }
        Shape::Blocked => {
            w.fake
                .apply(&[(HOME, &text("Home", "Welcome, tidied."))], &[GUIDE]);
            w.fake.show(
                4,
                shown(vec![op("delete-node", &[GUIDE]), op("edit-text", &[HOME])]),
            );
            w.fake.set(HOME, &text("Home", "Welcome, edited later."));
            w.fake.say(5, &service_text("Edit the welcome"));
            w.fake.show(5, shown(vec![op("edit-text", &[HOME])]));
        }
        Shape::Two => {
            w.fake.apply(&[], &[GUIDE, NOTES]);
            w.fake.show(
                4,
                shown(vec![
                    op("delete-node", &[GUIDE]),
                    op("delete-node", &[NOTES]),
                ]),
            );
        }
    }
    Gone {
        w,
        maintainer,
        author,
        reader,
        proposal: record.id,
    }
}

impl Gone {
    /// Hide `parts` of change `n` as the maintainer; the act must have been done.
    pub(crate) async fn hide(&self, n: u64, parts: &[&str]) {
        let block = block_id(n);
        let mut fields: Vec<(&str, &str)> = vec![("kind", "page-revision"), ("block", &block)];
        for p in parts {
            fields.push((p, "1"));
        }
        fields.push(("category", "personal-data"));
        fields.push(("note", ""));
        let a = post_form(
            &self.w,
            "/handbook/-/moderate",
            Some(&self.maintainer),
            &fields,
        )
        .await;
        assert_eq!(a.status, StatusCode::OK, "hiding change {n}: {}", a.body());
    }

    /// Restore every hide in force, as the log's own buttons do.
    pub(crate) async fn unhide_all(&self) {
        let log = self
            .w
            .store
            .tombstone_log(&self.w.wiki.id, None, 50)
            .expect("the log");
        let mut done = 0;
        for hide in log.iter().filter(|e| e.act == TombstoneAct::Hide) {
            let a = post_form(
                &self.w,
                "/handbook/-/moderate/unhide",
                Some(&self.maintainer),
                &[("hide", hide.id.as_str())],
            )
            .await;
            assert_eq!(a.status, StatusCode::SEE_OTHER, "{}", a.body());
            done += 1;
        }
        assert!(done > 0, "there was nothing to restore");
    }

    /// Every viewer: signed out, an observer, the person who proposed, the moderator.
    pub(crate) fn viewers(&self) -> Vec<Option<AccountId>> {
        vec![
            None,
            Some(self.reader.clone()),
            Some(self.author.clone()),
            Some(self.maintainer.clone()),
        ]
    }

    pub(crate) async fn get(&self, uri: &str, who: Option<&AccountId>) -> Answer {
        get_as(&self.w, uri, who).await
    }

    pub(crate) async fn post(&self, mode: &str, who: &AccountId) -> Answer {
        post_form(
            &self.w,
            "/handbook/-/restore/Guide",
            Some(who),
            &[("mode", mode)],
        )
        .await
    }

    /// How many proposals the wiki holds, as `who` sees them (a draft is its author's alone).
    pub(crate) fn proposals(&self, who: &AccountId) -> usize {
        self.w
            .store
            .proposals(&self.w.wiki.id, &ProposalFilter::default(), Some(who))
            .expect("the proposals")
            .len()
    }
}

/// No byte of the answer, in the body and in every header, holds any of `words`.
fn free_of(a: &Answer, which: &str, words: &[&str]) {
    let all = format!("{:?}\n{}", a.headers, a.body());
    for word in words {
        assert!(
            !all.contains(word),
            "{which}: `{word}` is in the answer: {all}"
        );
    }
}

/// The addresses of a removed page's answers: the page, the three members of the API, and the raw source of the content
/// origin.
async fn answers(g: &Gone, who: Option<&AccountId>) -> Vec<(String, Answer)> {
    let mut out = Vec::new();
    for uri in [
        "/handbook/Guide",
        "/handbook/-/api/v1/pages/Guide",
        "/handbook/-/api/v1/raw/Guide",
        "/handbook/-/api/v1/fragment/Guide",
    ] {
        out.push((uri.to_owned(), g.get(uri, who).await));
    }
    let extra: Vec<(&str, &str)> = who
        .map(|a| vec![("x-test-account", a.as_str())])
        .unwrap_or_default();
    out.push((
        "(content origin) /handbook/-/raw/Guide".to_owned(),
        request(&g.w, CONTENT, "GET", "/handbook/-/raw/Guide", &extra).await,
    ));
    out
}

/// **The surface `removed-page`**: a hidden summary of the removing change is the tombstone in the 410 page, in the API's
/// `page-gone` and in no byte of the raw 410; and so is a hidden author, which no answer names.
pub(crate) async fn surface_removed_page() {
    // Before: the page says why, in the words.
    let g = gone(Shape::Clean);
    let html = g.get("/handbook/Guide", None).await;
    assert_eq!(html.status, StatusCode::GONE, "{}", html.body());
    assert!(html.body().contains(REMOVAL_SUMMARY), "{}", html.body());
    // The proposal is a link: its page is a surface of its own.
    assert!(
        html.body()
            .contains(&format!("/handbook/-/proposals/{}", g.proposal)),
        "{}",
        html.body()
    );
    let api = g.get("/handbook/-/api/v1/pages/Guide", None).await;
    assert!(api.body().contains(REMOVAL_SUMMARY), "{}", api.body());

    g.hide(4, &["part_summary"]).await;
    for who in g.viewers() {
        for (uri, a) in answers(&g, who.as_ref()).await {
            assert_eq!(a.status, StatusCode::GONE, "{uri} as {who:?}: {}", a.body());
            nothing_hidden_in(&a, &uri);
            if uri == "/handbook/Guide" {
                // The tombstone, in the place of the words.
                assert!(a.body().contains(HIDING), "{uri} as {who:?}: {}", a.body());
                assert!(!a.body().contains("Reason given"), "{}", a.body());
                assert!(!a.body().contains("The change gives no reason"));
            }
            if uri == "/handbook/-/api/v1/pages/Guide" {
                let json: serde_json::Value = serde_json::from_slice(&a.bytes).expect("json");
                assert_eq!(json.get("reason"), Some(&serde_json::Value::Null), "{json}");
                assert_eq!(
                    json.get("hidden"),
                    Some(&serde_json::json!(["summary"])),
                    "{json}"
                );
                assert!(
                    json.get("tombstones")
                        .is_some_and(serde_json::Value::is_array),
                    "{json}"
                );
            }
        }
    }

    // The author: no answer of a removed page names who proposed or accepted it, hidden or not, and the hiding changes none.
    let g = gone(Shape::Clean);
    g.hide(4, &["part_author"]).await;
    for who in g.viewers() {
        for (uri, a) in answers(&g, who.as_ref()).await {
            assert_eq!(a.status, StatusCode::GONE, "{uri} as {who:?}: {}", a.body());
            free_of(&a, &uri, &[PROPOSER, "Wolfhound"]);
        }
    }
    // Everything hidden at once is the tombstone and nothing else.
    let g = gone(Shape::Clean);
    g.hide(4, &["part_summary", "part_author"]).await;
    for who in g.viewers() {
        for (uri, a) in answers(&g, who.as_ref()).await {
            free_of(&a, &uri, &["ZEBRA", PROPOSER]);
        }
    }
    // An unhide gives the words back, as before.
    let g = gone(Shape::Clean);
    g.hide(4, &["part_summary"]).await;
    g.unhide_all().await;
    let back = g.get("/handbook/Guide", None).await;
    assert!(back.body().contains(REMOVAL_SUMMARY), "{}", back.body());
    assert!(!back.body().contains(HIDING), "{}", back.body());
}

/// The form of the restoring proposal, as it is offered: the mode it carries.
fn offered(a: &Answer) -> Option<&'static str> {
    let body = a.body();
    ["rollback", "last-text"]
        .into_iter()
        .find(|mode| body.contains(&format!("value=\"{mode}\"")))
}

const SENTENCE: &str = "was hidden by moderation, so this form cannot restore it";

/// **The surface `restore-offer`**: when the content of the revision that wrote the text that would come back is hidden, the
/// form offers neither way, says so, and a request for either (or for a mode that was never offered) makes no draft and says
/// no byte of the hidden text.
pub(crate) async fn surface_restore_offer() {
    for (shape, mode) in [(Shape::Clean, "rollback"), (Shape::Blocked, "last-text")] {
        let g = gone(shape);
        let (author, maintainer) = (g.author.clone(), g.maintainer.clone());
        // Before: the way back is offered, whatever is hidden of the removal.
        let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
        assert_eq!(form.status, StatusCode::OK, "{}", form.body());
        assert_eq!(offered(&form), Some(mode), "{}", form.body());

        // The content of change 2, which wrote the text: hidden.
        g.hide(2, &["part_content"]).await;
        let before = (g.proposals(&author), g.proposals(&maintainer));
        for who in [&author, &maintainer] {
            let form = g.get("/handbook/-/restore/Guide", Some(who)).await;
            assert_eq!(form.status, StatusCode::OK, "{}", form.body());
            assert_eq!(offered(&form), None, "{}", form.body());
            assert!(!form.body().contains("name=\"mode\""), "{}", form.body());
            assert!(form.body().contains(SENTENCE), "{}", form.body());
            assert!(
                form.body().contains("The last text of Guide"),
                "{}",
                form.body()
            );
            // The way on: the new page, with the title, and nothing of the text.
            assert!(
                form.body().contains("/handbook/-/proposals/new?"),
                "{}",
                form.body()
            );
            no_text(&form, "the restore form");
            // Both ways and a mode never offered: refused, no draft, no hidden text.
            for asked in ["rollback", "last-text", "", "forged"] {
                let a = g.post(asked, who).await;
                assert_eq!(a.status, StatusCode::CONFLICT, "{asked}: {}", a.body());
                assert!(a.body().contains(SENTENCE), "{asked}: {}", a.body());
                no_text(&a, "the refusal");
                assert_eq!(a.header("location"), None);
            }
        }
        assert_eq!(
            before,
            (g.proposals(&author), g.proposals(&maintainer)),
            "a draft was made"
        );
        // The page of the removal works none of it out (G-T6 correction S-1): it links to the form, which says it, and it holds
        // no byte of the text.
        let gone_page = g.get("/handbook/Guide", Some(&author)).await;
        assert_eq!(gone_page.status, StatusCode::GONE);
        assert!(!gone_page.body().contains(SENTENCE), "{}", gone_page.body());
        assert!(
            gone_page.body().contains("/handbook/-/restore/Guide"),
            "{}",
            gone_page.body()
        );
        no_text(&gone_page, "the page of the removal");
        // The API says by the rung alone: offered to whoever may propose, none to a stranger.
        let api = g.get("/handbook/-/api/v1/pages/Guide", Some(&author)).await;
        let json: serde_json::Value = serde_json::from_slice(&api.bytes).expect("json");
        assert_eq!(
            json.get("restore"),
            Some(&serde_json::json!("offered")),
            "{json}"
        );
        no_text(&api, "the API's page-gone");
        let api = g.get("/handbook/-/api/v1/pages/Guide", None).await;
        let json: serde_json::Value = serde_json::from_slice(&api.bytes).expect("json");
        assert_eq!(
            json.get("restore"),
            Some(&serde_json::json!("none")),
            "{json}"
        );
        // An observer is still refused as before: the offer is the proposers'.
        let seen = g.get("/handbook/-/restore/Guide", Some(&g.reader)).await;
        assert_eq!(seen.status, StatusCode::FORBIDDEN, "{}", seen.body());

        // Un-hide: the offer returns, and it restores exactly the text.
        g.unhide_all().await;
        let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
        assert_eq!(offered(&form), Some(mode), "{}", form.body());
        let made = g.post(mode, &author).await;
        assert_eq!(made.status, StatusCode::SEE_OTHER, "{}", made.body());
        let at = made.header("location").expect("a draft").to_owned();
        let draft = g.get(&at, Some(&author)).await;
        assert_eq!(draft.status, StatusCode::OK, "{}", draft.body());
        assert!(draft.body().contains("ZEBRATEXT"), "{}", draft.body());
        assert_eq!(g.proposals(&author), before.0 + 1);
    }
}

/// A hidden summary or author of the revision that wrote the text leaves the text free to come back; so does the hiding of a
/// change that is the removal's parent but wrote another page.
#[tokio::test]
async fn summary_and_author_do_not_refuse_and_a_parent_that_wrote_another_page_does_not() {
    let g = gone(Shape::Blocked);
    let author = g.author.clone();
    g.hide(2, &["part_summary", "part_author"]).await;
    // Change 3, the parent of the removal, wrote `Home` only: its content is not the guide's text.
    g.hide(3, &["part_content"]).await;
    let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
    assert_eq!(offered(&form), Some("last-text"), "{}", form.body());
    let made = g.post("last-text", &author).await;
    assert_eq!(made.status, StatusCode::SEE_OTHER, "{}", made.body());
}

/// The draft of a rollback or of the last text copies nothing of a hidden summary of the removal: not its words in the draft's
/// summary and not in the page of the draft.
#[tokio::test]
async fn the_restoring_draft_copies_no_hidden_summary() {
    for (shape, mode, summary) in [
        (Shape::Clean, "rollback", "Undo change 4"),
        (Shape::Blocked, "last-text", "Restore: Guide"),
    ] {
        let g = gone(shape);
        let author = g.author.clone();
        g.hide(4, &["part_summary", "part_author"]).await;
        let made = g.post(mode, &author).await;
        assert_eq!(made.status, StatusCode::SEE_OTHER, "{}", made.body());
        let at = made.header("location").expect("a draft").to_owned();
        let draft = g.get(&at, Some(&author)).await;
        assert_eq!(draft.status, StatusCode::OK, "{}", draft.body());
        assert!(draft.body().contains(summary), "{}", draft.body());
        free_of(&draft, "the draft", &["ZEBRASUMMARY"]);
        free_of(&made, "the redirect", &["ZEBRASUMMARY"]);
    }
}

/// A page that was moved before it was removed: the text was written before the move, and the move wrote none of its own.
#[tokio::test]
async fn the_revision_that_wrote_the_text_is_found_behind_a_move() {
    let g = gone(Shape::Moved);
    let author = g.author.clone();
    let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
    assert_eq!(offered(&form), Some("rollback"), "{}", form.body());
    // Change 3 only moved the page, so it wrote no text: the text is change 2's.
    g.hide(2, &["part_content"]).await;
    let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
    assert_eq!(offered(&form), None, "{}", form.body());
    assert!(
        form.body().contains("The last text of Guide"),
        "{}",
        form.body()
    );
    no_text(&form, "the restore form");
    let a = g.post("rollback", &author).await;
    assert_eq!(a.status, StatusCode::CONFLICT, "{}", a.body());
    no_text(&a, "the refusal");
}

/// A rollback that writes two pages is refused as a whole when the text of either is hidden, and says which page by its title
/// alone.
#[tokio::test]
async fn a_rollback_of_two_pages_is_refused_whole_naming_the_page() {
    let g = gone(Shape::Two);
    let author = g.author.clone();
    // Before: one rollback of both.
    let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
    assert_eq!(offered(&form), Some("rollback"), "{}", form.body());
    // The notes' last text (change 3) is hidden: the guide's own text is not, and the rollback still cannot be made.
    g.hide(3, &["part_content"]).await;
    let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
    assert_eq!(offered(&form), None, "{}", form.body());
    assert!(
        form.body().contains("The last text of Notes was hidden"),
        "{}",
        form.body()
    );
    no_text(&form, "the restore form");
    let count = g.proposals(&author);
    let a = g.post("rollback", &author).await;
    assert_eq!(a.status, StatusCode::CONFLICT, "{}", a.body());
    assert!(a.body().contains("The last text of Notes was hidden"));
    no_text(&a, "the refusal");
    assert_eq!(g.proposals(&author), count);
    // Both hidden: both titles, in the order of their places.
    g.hide(2, &["part_content"]).await;
    let form = g.get("/handbook/-/restore/Guide", Some(&author)).await;
    assert!(
        form.body()
            .contains("The last text of Guide, Notes was hidden"),
        "{}",
        form.body()
    );
    // The page of the removal works none of it out (G-T6 correction S-1): the link, and the form says why there is no offer.
    let page = g.get("/handbook/Guide", Some(&author)).await;
    assert!(!page.body().contains(SENTENCE), "{}", page.body());
    assert!(
        page.body().contains("/handbook/-/restore/Guide"),
        "{}",
        page.body()
    );
}

/// No byte of the answer holds the hidden text of a page.
fn no_text(a: &Answer, which: &str) {
    free_of(a, which, &["ZEBRATEXT", "ZEBRANOTES"]);
}
