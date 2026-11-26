#![allow(clippy::expect_used, clippy::unwrap_used)]
//! Part (E): **the new surfaces** (RFC 007 D-9; acceptance criteria 5 and 10): `listProposals` and `listProposalComments` walked by their
//! `Link rel="next"` over more than one page, the proposals feed, all three for an anonymous reader, a contributor and a moderator; none
//! of them shows a draft or what a moderator hid; and an anonymous reader of a restricted wiki gets the generic not-found on each.

use std::collections::BTreeSet;

use super::roundtrip::member;
use super::*;

const MODERATE: &str = "/handbook/-/moderate";
const HIDDEN_COMMENT: &str = "QUOKKACOMMENT";
const DRAFT_WORD: &str = "DRAFTWORD";

/// Who asks the API: nobody (an anonymous reader), or a person with a bearer token.
#[derive(Clone, Copy)]
enum Asker<'a> {
    Anonymous,
    Token(&'a str),
}

/// One walked list: every page's items, the number of pages, and the raw bodies.
struct Walked {
    items: Vec<serde_json::Value>,
    pages: usize,
    bodies: String,
}

impl Proof {
    /// A person who joined earlier, signed in again with their own browser.
    fn signed_in_again(&self, login: &'static str, name: &'static str) -> Person {
        let password = password_for(login);
        let mut browser = self.running.browser();
        sign_in(&mut browser, login, &password, "/");
        Person {
            login,
            name,
            password,
            browser,
        }
    }

    fn ask(&self, who: Asker<'_>, path: &str) -> Reply {
        api(
            &self.running,
            "GET",
            path,
            match who {
                Asker::Anonymous => None,
                Asker::Token(t) => Some(t),
            },
            None,
            None,
        )
    }

    /// A list walked by the `Link` header's `rel="next"` alone: each page must say the next one in the header and in the body
    /// (`next_cursor`), the target must be the request with the cursor the body gave, and the last page has neither.
    fn walk(&self, who: Asker<'_>, first: &str) -> Walked {
        let mut path = first.to_owned();
        let mut walked = Walked {
            items: Vec::new(),
            pages: 0,
            bodies: String::new(),
        };
        loop {
            let page = self.ask(who, &path);
            assert_eq!(page.status, 200, "{path}: {}", page.body);
            walked.pages += 1;
            assert!(walked.pages < 40, "a walk that does not end: {path}");
            walked.bodies.push_str(&page.body);
            let body: serde_json::Value = serde_json::from_str(&page.body).unwrap();
            walked.items.extend(
                body.get("items")
                    .and_then(|i| i.as_array())
                    .cloned()
                    .unwrap_or_default(),
            );
            let links = page.headers("link");
            assert!(
                links
                    .first()
                    .is_some_and(|l| l.contains("rel=\"service-desc\"")),
                "{path}: the description stays the first Link: {links:?}"
            );
            let Some(cursor) = body.get("next_cursor").and_then(|c| c.as_str()) else {
                assert_eq!(links.len(), 1, "{path}: the last page has no next: {links:?}");
                return walked;
            };
            assert_eq!(links.len(), 2, "{path}: {links:?}");
            let (base, query) = path.split_once('?').unwrap_or((path.as_str(), ""));
            let mut kept: Vec<&str> = query
                .split('&')
                .filter(|p| !p.is_empty() && !p.starts_with("cursor="))
                .collect();
            let given = format!("cursor={cursor}");
            kept.push(&given);
            let expected = format!("{base}?{}", kept.join("&"));
            let next = links.get(1).cloned().unwrap_or_default();
            assert_eq!(next, format!("<{expected}>; rel=\"next\""), "{path}");
            path = expected;
        }
    }

    /// The moderation form's address for the item of `kind` whose link follows `after` on `page`.
    fn moderate_query_after(page: &Reply, kind: &str, after: &str) -> String {
        let body = page.body.replace("&#38;", "&").replace("&amp;", "&");
        let from = body
            .find(after)
            .unwrap_or_else(|| panic!("no `{after}` on the page:\n{}", page.body));
        let want = format!("{MODERATE}?kind={kind}");
        let at = body
            .get(from..)
            .and_then(|rest| rest.find(&want))
            .map(|i| from + i)
            .unwrap_or_else(|| panic!("no link to hide a `{kind}` after `{after}`:\n{}", page.body));
        body.get(at + MODERATE.len() + 1..)
            .unwrap_or_default()
            .split('"')
            .next()
            .unwrap_or_default()
            .to_owned()
    }

    pub(super) fn the_new_surfaces(&mut self) {
        self.t.part("(E) the new surfaces: `listProposals`, `listProposalComments` and the proposals feed, walked by `Link`, for three readers, hiding what a moderator hid (RFC 007 D-9; criteria 5, 10)");
        let mut noor = self.signed_in_again("noor", "Noor Contributor");
        let mut maya = self.signed_in_again("maya", "Maya Maintainer");
        let mut zora = self.signed_in_again("zora", "Zora Quokka");
        let noor_token = self.token("noor", Scope::Read);
        let maya_token = self.token("maya", Scope::Read);

        // What there is to list.
        let one = propose_edit(&mut noor, "Help", "Open proposal one", |t| {
            t.replacen("It never claims more than it checked.", "It never claims more than it checked, one.", 1)
        });
        let two = propose_edit(&mut noor, "Help", "Open proposal two", |t| {
            t.replacen("It never claims more than it checked.", "It never claims more than it checked, two.", 1)
        });
        let draft_body = format!(
            "{{\"summary\":\"{DRAFT_WORD} a draft only Noor sees\",\"submit\":false,\"files\":[{{\"title\":\"Seeds\",\"text\":{}}}]}}",
            json_quote("---\ntitle: Seeds\n---\n\nA draft.\n")
        );
        let drafted = api(
            &self.running,
            "POST",
            &format!("{API}/proposals"),
            Some(&self.token("noor", Scope::Propose)),
            Some(&self.tip()),
            Some(&draft_body),
        );
        assert_eq!(drafted.status, 201, "{}\n{}", drafted.head, drafted.body);
        let draft = json_field(&drafted.body, "id").expect("a draft id");
        for (who, text) in [
            (0, "first"),
            (1, "second"),
            (2, &format!("{HIDDEN_COMMENT} third")),
            (0, "fourth"),
            (1, "fifth"),
        ] {
            let sent = match who {
                0 => comment(&mut maya, &one, text),
                1 => comment(&mut noor, &one, text),
                _ => comment(&mut zora, &one, text),
            };
            assert_eq!(sent.status, 303, "{}", sent.body);
        }
        self.t.did("Noor opens two proposals that stay open and a third as a draft; Maya, Noor and Zora write five comments on the first (Zora's carries a word of its own)");

        // What moderation hid before this part: Zora's revision of Notes (part C). Now her comment.
        let on_page = maya.browser.get(&proposal_path("handbook", &one));
        let query = Self::moderate_query_after(&on_page, "proposal-comment", HIDDEN_COMMENT);
        self.hide(&mut maya, &query, &["part_content", "part_author"]);
        self.t.did("Maya hides the body and the author of Zora's comment, through the moderation form (the summary of Zora's revision was hidden in (C))");

        // listProposals, for three readers.
        let viewers: [(&str, Asker<'_>); 3] = [
            ("an anonymous reader", Asker::Anonymous),
            ("Noor, a contributor (her token)", Asker::Token(&noor_token)),
            ("Maya, a moderator (her token)", Asker::Token(&maya_token)),
        ];
        let mut id_sets: Vec<BTreeSet<String>> = Vec::new();
        for (name, who) in viewers {
            let walked = self.walk(who, &format!("{API}/proposals?limit=3"));
            self.t.did(&format!("{name} walks `listProposals?limit=3` by the `Link` header's `rel=\"next\"`"));
            let ids: BTreeSet<String> = walked
                .items
                .iter()
                .filter_map(|i| i.get("id").and_then(|v| v.as_str()).map(str::to_owned))
                .collect();
            self.t.saw(
                walked.pages == 3 && walked.items.len() == 9 && ids.len() == 9,
                "nine proposals over three pages, each once: the seven accepted ones and the two open ones",
                &format!("{} pages, {} items", walked.pages, walked.items.len()),
            );
            self.t.saw(
                !ids.contains(&draft) && !walked.bodies.contains(DRAFT_WORD),
                "the draft is not listed, nor a word of it, not even for Noor, whose draft it is",
                &walked.bodies,
            );
            let hidden_item = walked
                .items
                .iter()
                .find(|i| i.get("hidden").is_some())
                .cloned()
                .unwrap_or_default();
            self.t.saw(
                member(&hidden_item, "/summary") == ""
                    && member(&hidden_item, "/hidden") == &serde_json::json!(["summary", "author"])
                    && member(&hidden_item, "/tombstone/rung") == "maintainer",
                "the hidden revision's proposal has an empty summary and says what is hidden, why and by which role",
                &hidden_item.to_string(),
            );
            for needle in [HIDDEN_SUMMARY_OF_C, ZORA_NAME, HIDDEN_COMMENT] {
                self.t.saw(
                    !walked.bodies.contains(needle),
                    &format!("the list carries no `{needle}`"),
                    &walked.bodies,
                );
            }
            id_sets.push(ids);
        }
        let _ = two;
        self.t.saw(
            id_sets.windows(2).all(|w| w[0] == w[1]),
            "the three readers walk the same nine proposals: a list shows what the same person's pages show, and the pages show the same to each (the list names no author, so there is nothing for a moderator to see more of)",
            &format!("{id_sets:?}"),
        );

        // The list against the page of the proposals, for the same readers.
        for (name, browser) in [
            ("an anonymous reader", &mut self.anonymous),
            ("Noor", &mut noor.browser),
            ("Maya", &mut maya.browser),
        ] {
            let page = browser.get("/handbook/-/proposals");
            let on_page: BTreeSet<String> = page
                .body
                .split("href=\"/handbook/-/proposals/")
                .skip(1)
                .filter_map(|r| r.split('"').next())
                .filter(|id| !id.contains('/') && !id.contains('?') && *id != "new")
                .map(str::to_owned)
                .collect();
            self.t.did(&format!("{name} opens the proposals page"));
            let mut expected = id_sets.first().cloned().unwrap_or_default();
            if name == "Noor" {
                // The page keeps a person's own draft for her, in a section of its own; the list never holds one (documented: the
                // list is the same for everyone who may read the wiki).
                expected.insert(draft.clone());
            }
            self.t.saw(
                on_page == expected,
                if name == "Noor" {
                    "the page links the proposals the API lists and, for her alone, her own draft (which the list never holds)"
                } else {
                    "the page links exactly the proposals the API lists, and not the draft"
                },
                &format!("{on_page:?}\nvs\n{expected:?}"),
            );
            self.t.saw(
                plain(main_of(&page)).contains("(summary hidden) Accepted · Proposed by (hidden)"),
                "and says the hidden summary and author as hidden, where the list gave `summary: \"\"` with its `tombstone`",
                &plain(main_of(&page)),
            );
        }

        // listProposalComments.
        for (name, who) in viewers {
            let walked = self.walk(who, &format!("{API}/proposals/{one}/comments?limit=2"));
            self.t.did(&format!("{name} walks `listProposalComments?limit=2` of the first open proposal"));
            let bodies: Vec<String> = walked
                .items
                .iter()
                .map(|i| i.get("body").and_then(|b| b.as_str()).unwrap_or("?").to_owned())
                .collect();
            self.t.saw(
                walked.pages == 3 && bodies == ["first", "second", "", "fourth", "fifth"],
                "five comments over three pages, in the order they were made; the third, Zora's, has an empty body",
                &format!("{} pages {bodies:?}", walked.pages),
            );
            let hidden = walked.items.get(2).cloned().unwrap_or_default();
            self.t.saw(
                member(&hidden, "/hidden") == &serde_json::json!(["content", "author"])
                    && member(&hidden, "/tombstone/category").is_string()
                    && member(&hidden, "/tombstone/rung") == "maintainer",
                "and it is its tombstone: which parts, why (the category) and by which role, as the page draws it",
                &hidden.to_string(),
            );
            self.t.saw(
                [HIDDEN_COMMENT, ZORA_NAME].iter().all(|n| !walked.bodies.contains(n))
                    && walked.items.iter().all(|i| i.get("author").is_none()),
                "no page of the walk carries Zora's words or her name, and no comment of the list names an author",
                &walked.bodies,
            );
        }
        let page = self.anonymous.get(&proposal_path("handbook", &one));
        self.t.saw(
            plain(main_of(&page)).contains("This comment is hidden.")
                && !page.body.contains(HIDDEN_COMMENT)
                && page.body.matches("This comment is hidden.").count() == 1,
            "the proposal's own page says it too: one comment is hidden, and its words are not on it",
            &plain(main_of(&page)),
        );

        // The feed.
        for (name, reply) in [
            ("an anonymous reader", self.anonymous.get("/handbook/-/proposals.atom")),
            ("Noor", noor.browser.get("/handbook/-/proposals.atom")),
            ("Maya", maya.browser.get("/handbook/-/proposals.atom")),
        ] {
            self.t.did(&format!("{name} reads the proposals feed"));
            self.t.saw(
                reply.status == 200
                    && reply.header("content-type").is_some_and(|c| c.starts_with("application/atom+xml"))
                    && reply.body.starts_with("<?xml")
                    && reply.body.trim_end().ends_with("</feed>"),
                "it is an Atom feed",
                &reply.head,
            );
            self.t.saw(
                reply.body.matches("<entry>").count() == 16
                    && reply.body.contains("<title>Proposed: Open proposal one</title>")
                    && reply.body.contains("<title>Accepted: Remove the scratch page</title>"),
                "sixteen entries: nine opened and seven decided (the two open proposals are opened only)",
                &reply.body,
            );
            self.t.saw(
                reply.body.contains("<title>Proposed: (summary hidden)</title>")
                    && reply.body.contains("<author><name>(hidden)</name></author>"),
                "the hidden revision's entries say *(summary hidden)* and name no author",
                &reply.body,
            );
            self.t.saw(
                [HIDDEN_SUMMARY_OF_C, ZORA_NAME, HIDDEN_COMMENT, DRAFT_WORD, HIDDEN_TEXT_OF_C]
                    .iter()
                    .all(|n| !reply.body.contains(n)),
                "and the feed carries nothing that was hidden and nothing of the draft",
                &reply.body,
            );
        }
        let anonymous_draft = self.ask(Asker::Anonymous, &format!("{API}/proposals/{draft}"));
        self.t.saw(
            anonymous_draft.status == 404,
            "the draft is not found for a reader who is not its author",
            &anonymous_draft.body,
        );

        // A restricted wiki is absent to an anonymous reader, on each of the three.
        let (status, out, err) = self.cli(&["wiki", "create", "vault", "--name", "Vault", "--visibility", "restricted"]);
        self.t.did("the operator makes a restricted wiki `vault`, with the server running; an anonymous reader asks for the three surfaces of it, and of a wiki that does not exist");
        assert_eq!(status, 0, "{out}{err}");
        for (what, tail) in [
            ("listProposals", "/-/api/v1/proposals"),
            ("listProposalComments", "/-/api/v1/proposals/01a2183b-9453-7072-a902-6bc581b179c5/comments"),
            ("the proposals feed", "/-/proposals.atom"),
        ] {
            let (hidden, absent) = (
                self.anonymous.get(&format!("/vault{tail}")),
                self.anonymous.get(&format!("/nowhere{tail}")),
            );
            self.t.saw(
                hidden.status == 404
                    && hidden.status == absent.status
                    && hidden.body.replace("/vault/", "/nowhere/") == absent.body
                    && hidden.header("content-type") == absent.header("content-type"),
                &format!("{what}: a wiki the reader may not read answers the generic not-found, byte for byte as a wiki that does not exist does (but for the address the sign-in link of the page repeats)"),
                &format!("{}\n{}\n---\n{}\n{}", hidden.head, hidden.body, absent.head, absent.body),
            );
        }
    }
}

const HIDDEN_SUMMARY_OF_C: &str = "QUOKKASUMMARY";
const HIDDEN_TEXT_OF_C: &str = "QUOKKATEXT";
const ZORA_NAME: &str = "Zora Quokka";
