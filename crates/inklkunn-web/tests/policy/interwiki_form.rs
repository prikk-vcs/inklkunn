//! The policy form edits the interwiki map (RFC 007 D-8; W-6): a row for each prefix, a prefix and the address it leads to, added and
//! removed without a script; each rule of the map refused by its own sentence **beside the row it concerns**, by the validator
//! acceptance uses and with the wiki's namespaces; a change sent as an ordinary proposal that writes the canonical file (and
//! removes it when no row is left), alone or with the files of the rules, never raw YAML. The records are the in-memory store's and
//! the repository is a double; the proof through acceptance on a real prikk is `serve::writes_on::tests::policy`.

use std::collections::BTreeSet;

use inklkunn_core::governance::{
    GovernanceFile, Interwiki, ReadFault, Refusal, Rules, validate_interwiki,
};
use inklkunn_store::Visibility;

use super::form::{proposals_made, value};
use super::interwiki::{MAP, MAP_FILE, env_with_map};
use super::*;

const POLICY_FILE: &str = "governance/policy.yaml";
const WIKI: &str = "https://en.wikipedia.org/wiki/{title}";
const OK: &str = "https://example.org/docs?page={title}";

/// The canonical text of a map made of `rows`.
fn canonical(rows: &[(&str, &str)]) -> String {
    let text: String = rows
        .iter()
        .map(|(p, t)| format!("\"{p}\": \"{t}\"\n"))
        .collect();
    validate_interwiki(&text, &BTreeSet::new())
        .expect("a map")
        .to_yaml()
}

/// A public wiki with a page under `Help/` (so that `help` is a namespace of it), and `map` when there is one.
fn env_with_help(map: Option<&str>) -> Env {
    let help = common::page(&["title: Help/Contents"], "Help.\n");
    let home = common::home_with("The bridge opened in 1950.");
    let mut files: Vec<(&str, &str)> =
        vec![("pages/Home.md", &home), ("pages/Help/Contents.md", &help)];
    if let Some(map) = map {
        files.push((MAP_FILE, map));
    }
    cast(common::with_accounts(common::World::with_visibility(
        "handbook",
        "The Handbook",
        Visibility::Public,
        &files,
    )))
}

/// `fields` with the map rows set to `rows` (and the count the form says it sent).
fn with_map(fields: &[(String, String)], rows: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut out = with(fields, "map_rows", &rows.len().to_string());
    for (n, (prefix, template)) in rows.iter().enumerate() {
        out = with(&out, &format!("map-{}-prefix", n + 1), prefix);
        out = with(&out, &format!("map-{}-template", n + 1), template);
    }
    out
}

/// The form as `who` is shown it, with the map rows set to `rows`, sent.
async fn send_map(e: &Env, who: &AccountId, rows: &[(&str, &str)]) -> Answer {
    let fields = with_map(&opened(e, who, "send").await, rows);
    post_fields(&e.w, Some(who), &fields).await
}

/// The sentence said beside `control`, or a panic when none is.
fn said_beside(a: &Answer, control: &str) -> String {
    let main = main_of(a);
    let at = main
        .find(&format!("id=\"c-{control}-problem\""))
        .unwrap_or_else(|| panic!("no problem beside {control}: {main}"));
    let said = main.get(at..).unwrap_or("");
    said.get(..said.find("</p>").unwrap_or(said.len()))
        .unwrap_or("")
        .to_owned()
}

/// A refusal of the map is said by `sentence` beside `control` (and by nothing else), and nothing is made.
async fn refused_beside(e: &Env, rows: &[(&str, &str)], control: &str, sentence: &str) {
    let made = proposals_made(e);
    let a = send_map(e, &e.maintainer, rows).await;
    assert_eq!(
        a.status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "{control}: {}",
        a.body()
    );
    let said = said_beside(&a, control);
    assert!(
        said.contains(sentence),
        "{control}: `{sentence}` not in `{said}`"
    );
    let problems = main_of(&a).matches("class=\"field-problem\"").count();
    assert_eq!(problems, 1, "{control}: one problem expected: {}", a.body());
    assert_eq!(proposals_made(e), made, "{control}");
    assert_plain_words("a refusal of the map's form", &a.body());
}

#[tokio::test]
async fn the_form_is_filled_with_a_row_for_each_prefix_in_force_and_works_without_a_script() {
    let e = env_with_map(MAP);
    let a = as_(&e, &e.contributor, EDIT).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let main = main_of(&a);
    for want in [
        "Interwiki map</legend>",
        "Interwiki prefix 1</legend>",
        "Interwiki prefix 2</legend>",
        "Add an interwiki prefix</button>",
        "Remove this prefix</button>",
        "Prefix</label>",
        "Address, with {title} where the page title goes</label>",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    assert!(!main.contains("<script"), "{main}");
    let fields = form_fields(&main);
    for (name, want) in [
        ("map_rows", "2"),
        ("map-1-prefix", "docs"),
        ("map-1-template", OK),
        ("map-2-prefix", "wikipedia"),
        ("map-2-template", WIKI),
    ] {
        assert_eq!(value(&fields, name), want, "{name}: {main}");
    }
    assert_plain_words("the policy form with a map", &a.body());
    // In Japanese too, with the words the policy page uses.
    let ja = as_ja(&e, Some(&e.contributor), EDIT).await;
    for want in [
        "Wiki 間リンクの対応表</legend>",
        "Wiki 間リンクの接頭辞 1</legend>",
        "Wiki 間リンクの接頭辞を追加</button>",
        "接頭辞</label>",
    ] {
        assert!(main_of(&ja).contains(want), "{want}: {}", ja.body());
    }
    // A wiki with no map has no row, and the button adds one.
    let none = env();
    let fields = opened(&none, &none.maintainer, "map-add").await;
    assert_eq!(value(&fields, "map_rows"), "0");
    let a = post_fields(&none.w, Some(&none.maintainer), &fields).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    let fields = form_fields(&main_of(&a));
    assert_eq!(value(&fields, "map_rows"), "1");
    assert_eq!(value(&fields, "map-1-prefix"), "");
}

#[tokio::test]
async fn rows_are_added_and_removed_keeping_what_was_typed_and_a_blank_row_is_no_prefix() {
    let e = env();
    let typed = with_map(&opened(&e, &e.maintainer, "map-add").await, &[("gh", OK)]);
    // Adding keeps what was typed and gives an empty row.
    let a = post_fields(&e.w, Some(&e.maintainer), &typed).await;
    let after = form_fields(&main_of(&a));
    assert_eq!(value(&after, "map_rows"), "2");
    assert_eq!(value(&after, "map-1-prefix"), "gh");
    assert_eq!(value(&after, "map-2-prefix"), "");
    // The blank row is no prefix: sent as it is, the map is `gh` alone.
    let id =
        proposal_of(&post_fields(&e.w, Some(&e.maintainer), &with(&after, "act", "send")).await);
    assert_eq!(
        files_of(&e.w, &id),
        vec![(MAP_FILE.to_owned(), Some(canonical(&[("gh", OK)])))]
    );
    // Removing a row keeps the others.
    let two = with_map(
        &opened(&e, &e.maintainer, "map-remove-1").await,
        &[("a", OK), ("b", OK)],
    );
    let a = post_fields(&e.w, Some(&e.maintainer), &two).await;
    let after = form_fields(&main_of(&a));
    assert_eq!(value(&after, "map_rows"), "1");
    assert_eq!(value(&after, "map-1-prefix"), "b");
    // A number that is no row removes nothing.
    let a = post_fields(
        &e.w,
        Some(&e.maintainer),
        &with(&two, "act", "map-remove-9"),
    )
    .await;
    assert_eq!(value(&form_fields(&main_of(&a)), "map_rows"), "2");
    // Neither button makes a proposal.
    assert_eq!(proposals_made(&e), 1);
}

#[tokio::test]
async fn an_unchanged_form_creates_nothing_with_a_map_and_without_one() {
    for e in [env(), env_with_map(MAP)] {
        let fields = opened(&e, &e.contributor, "send").await;
        let a = post_fields(&e.w, Some(&e.contributor), &fields).await;
        assert_eq!(a.status, StatusCode::OK, "{}", a.body());
        assert!(
            main_of(&a).contains("Nothing was changed, so no proposal was made."),
            "{}",
            a.body()
        );
        assert_eq!(proposals_made(&e), 0);
    }
    // Blank rows beside the map change nothing, and neither does the order the rows are typed in.
    let e = env_with_map(MAP);
    let a = send_map(
        &e,
        &e.maintainer,
        &[("wikipedia", WIKI), ("docs", OK), ("", ""), ("  ", "  ")],
    )
    .await;
    assert!(main_of(&a).contains("Nothing was changed"), "{}", a.body());
    assert_eq!(proposals_made(&e), 0);
}

#[tokio::test]
async fn a_prefix_added_changed_and_removed_is_a_proposal_that_writes_the_canonical_file_alone() {
    // Added to a wiki with no map.
    let e = env();
    let a = send_map(&e, &e.contributor, &[("wikipedia", WIKI)]).await;
    let id = proposal_of(&a);
    assert_eq!(
        files_of(&e.w, &id),
        vec![(MAP_FILE.to_owned(), Some(canonical(&[("wikipedia", WIKI)])))]
    );
    // The proposal says it as a sentence, never as the file.
    let shown = as_(&e, &e.maintainer, &format!("/handbook/-/proposals/{id}")).await;
    let main = main_of(&shown);
    assert!(
        main.contains(&format!("Interwiki prefix added: wikipedia → {WIKI}")),
        "{main}"
    );
    assert!(!main.contains("wikipedia: \""), "{main}");
    // Changed, removed and added at once, in a wiki that has a map: the file is the whole new map, in prefix order.
    let e = env_with_map(MAP);
    let a = send_map(
        &e,
        &e.maintainer,
        &[
            ("zed", "https://zed.example/{title}"),
            ("wikipedia", "https://de.wikipedia.org/wiki/{title}"),
        ],
    )
    .await;
    let id = proposal_of(&a);
    assert_eq!(
        files_of(&e.w, &id),
        vec![(
            MAP_FILE.to_owned(),
            Some(canonical(&[
                ("wikipedia", "https://de.wikipedia.org/wiki/{title}"),
                ("zed", "https://zed.example/{title}"),
            ]))
        )]
    );
    let main = main_of(&as_(&e, &e.maintainer, &format!("/handbook/-/proposals/{id}")).await);
    for want in [
        "Interwiki prefix removed: docs (it led to https://example.org/docs?page={title})",
        "Interwiki prefix added: zed → https://zed.example/{title}",
        "Interwiki prefix changed: wikipedia led to https://en.wikipedia.org/wiki/{title} and now leads to https://de.wikipedia.org/wiki/{title}",
    ] {
        assert!(main.contains(want), "{want}: {main}");
    }
    // The prefix and the address are trimmed.
    let e = env();
    let a = send_map(&e, &e.maintainer, &[("  gh  ", &format!("  {OK}  "))]).await;
    assert_eq!(
        files_of(&e.w, &proposal_of(&a)),
        vec![(MAP_FILE.to_owned(), Some(canonical(&[("gh", OK)])))]
    );
}

#[tokio::test]
async fn no_row_left_proposes_removing_the_file_and_a_row_removed_by_its_button_is_gone() {
    let e = env_with_map(MAP);
    let a = send_map(&e, &e.maintainer, &[]).await;
    let id = proposal_of(&a);
    assert_eq!(files_of(&e.w, &id), vec![(MAP_FILE.to_owned(), None)]);
    let main = main_of(&as_(&e, &e.maintainer, &format!("/handbook/-/proposals/{id}")).await);
    assert!(
        main.contains(
            "Interwiki prefix removed: wikipedia (it led to https://en.wikipedia.org/wiki/{title})"
        ),
        "{main}"
    );
    // The button takes the row out of the form, and what is sent then is the map without it.
    let e = env_with_map(MAP);
    let a = post_fields(
        &e.w,
        Some(&e.maintainer),
        &opened(&e, &e.maintainer, "map-remove-1").await,
    )
    .await;
    let after = form_fields(&main_of(&a));
    assert_eq!(value(&after, "map_rows"), "1");
    assert_eq!(value(&after, "map-1-prefix"), "wikipedia");
    let id =
        proposal_of(&post_fields(&e.w, Some(&e.maintainer), &with(&after, "act", "send")).await);
    assert_eq!(
        files_of(&e.w, &id),
        vec![(MAP_FILE.to_owned(), Some(canonical(&[("wikipedia", WIKI)])))]
    );
}

#[tokio::test]
async fn a_change_of_the_map_and_of_the_rules_is_one_proposal_and_each_alone_leaves_the_other_file_out()
 {
    // Both: one proposal with both files (a change to the rules touches nothing else, and the map is one of them).
    let e = env();
    let fields = with(
        &with_map(&opened(&e, &e.maintainer, "send").await, &[("gh", OK)]),
        "approvals",
        "1",
    );
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    let files = files_of(&e.w, &id);
    let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(paths, [POLICY_FILE, MAP_FILE]);
    // The rules alone, in a wiki that has a map: the map is not written.
    let e = env_with_map(MAP);
    let fields = with(&opened(&e, &e.maintainer, "send").await, "approvals", "1");
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    let paths: Vec<String> = files_of(&e.w, &id).into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, [POLICY_FILE]);
}

#[tokio::test]
async fn a_prefix_with_a_character_no_prefix_has_is_refused_beside_its_prefix() {
    refused_beside(
        &env(),
        &[("wiki pedia", OK)],
        "map-1-prefix",
        "A prefix is made of letters (a to z), digits and hyphens only.",
    )
    .await;
    // Nothing a person types is syntax: a prefix that tries to write a second key is one prefix with bad characters.
    refused_beside(
        &env(),
        &[("a\": \"https://x.example/{title}\"\nb", OK)],
        "map-1-prefix",
        "A prefix is made of letters (a to z)",
    )
    .await;
}

#[tokio::test]
async fn a_prefix_that_is_too_long_is_refused_beside_its_prefix() {
    refused_beside(
        &env(),
        &[(&"a".repeat(33), OK)],
        "map-1-prefix",
        "A prefix has at most 32 characters.",
    )
    .await;
}

#[tokio::test]
async fn a_prefix_that_is_how_titles_of_the_wiki_begin_is_refused_whatever_its_case() {
    for prefix in ["help", "Help", "HELP"] {
        refused_beside(
            &env_with_help(None),
            &[(prefix, OK)],
            "map-1-prefix",
            "how the titles of some pages of this wiki begin",
        )
        .await;
    }
    // The second row is the one named when it is the second that clashes.
    refused_beside(
        &env_with_help(None),
        &[("gh", OK), ("help", OK)],
        "map-2-prefix",
        "how the titles of some pages of this wiki begin",
    )
    .await;
}

#[tokio::test]
async fn a_prefix_written_in_two_rows_is_refused_beside_the_second() {
    refused_beside(
        &env(),
        &[("gh", OK), ("gh", "https://other.example/{title}")],
        "map-2-prefix",
        "Another row has this prefix. A prefix leads to one address.",
    )
    .await;
    // A blank row between them does not change which row is the second the person sees.
    refused_beside(
        &env(),
        &[("gh", OK), ("", ""), ("gh", OK)],
        "map-3-prefix",
        "Another row has this prefix.",
    )
    .await;
}

#[tokio::test]
async fn an_address_that_is_too_long_is_refused_beside_its_address() {
    let long = format!("https://example.org/{}/{{title}}", "a".repeat(500));
    refused_beside(
        &env(),
        &[("gh", &long)],
        "map-1-template",
        "An address has at most 512 characters.",
    )
    .await;
}

#[tokio::test]
async fn an_address_with_a_character_it_may_not_have_is_refused_beside_its_address() {
    for template in [
        "https://example.org/a b/{title}",
        "https://example.org/é/{title}",
        "https://example.org/<x>/{title}",
    ] {
        refused_beside(
            &env(),
            &[("gh", template)],
            "map-1-template",
            "has a character that is not allowed in it",
        )
        .await;
    }
}

#[tokio::test]
async fn an_address_that_is_not_https_is_refused_beside_its_address() {
    for template in [
        "http://example.org/{title}",
        "ftp://example.org/{title}",
        "example.org/{title}",
    ] {
        refused_beside(
            &env(),
            &[("gh", template)],
            "map-1-template",
            "is not an https address",
        )
        .await;
    }
}

#[tokio::test]
async fn an_address_with_no_host_or_a_bad_one_is_refused_beside_its_address() {
    for template in [
        "https:///{title}",
        "https://exa_mple.org/{title}",
        "https://-bad.example/{title}",
        "https://example.org:port/{title}",
    ] {
        refused_beside(
            &env(),
            &[("gh", template)],
            "map-1-template",
            "no host name, or one that is not a host name",
        )
        .await;
    }
}

#[tokio::test]
async fn an_address_that_names_a_user_is_refused_beside_its_address() {
    refused_beside(
        &env(),
        &[("gh", "https://user@example.org/{title}")],
        "map-1-template",
        "names a user (user@host)",
    )
    .await;
}

#[tokio::test]
async fn an_address_that_does_not_hold_the_title_exactly_once_is_refused_beside_its_address() {
    for template in [
        "https://example.org/page",
        "https://example.org/{title}/{title}",
        "https://{title}.example.org/",
        "https://example.org/{other}/{title}",
        "https://example.org/x#{title}",
    ] {
        refused_beside(
            &env(),
            &[("gh", template)],
            "map-1-template",
            "must hold the page title exactly once",
        )
        .await;
    }
}

#[tokio::test]
async fn a_row_with_only_a_prefix_or_only_an_address_is_refused_as_empty_beside_the_one_that_is() {
    refused_beside(
        &env(),
        &[("gh", "")],
        "map-1-template",
        "This cannot be empty.",
    )
    .await;
    refused_beside(&env(), &[("", OK)], "map-1-prefix", "This cannot be empty.").await;
}

#[tokio::test]
async fn a_map_with_too_many_prefixes_is_refused_at_the_top_and_nothing_is_made() {
    let e = env();
    let prefixes: Vec<String> = (0..257).map(|n| format!("p{n}")).collect();
    let rows: Vec<(&str, &str)> = prefixes.iter().map(|p| (p.as_str(), OK)).collect();
    let a = send_map(&e, &e.maintainer, &rows).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    let main = main_of(&a);
    let at = main.find("id=\"problems\"").expect("the problems");
    assert!(
        main.get(at..)
            .is_some_and(|t| t.contains("An interwiki map has at most 256 prefixes.")),
        "{main}"
    );
    assert_eq!(proposals_made(&e), 0);
    // 256 are fine.
    let a = send_map(&e, &e.maintainer, rows.get(..256).expect("rows")).await;
    proposal_of(&a);
}

#[tokio::test]
async fn every_row_that_breaks_a_rule_is_said_at_once_each_beside_its_own_row() {
    let e = env();
    let a = send_map(
        &e,
        &e.maintainer,
        &[
            ("bad one", OK),
            ("ok", OK),
            ("gh", "http://example.org/{title}"),
            ("ok", "https://example.org/x"),
        ],
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(said_beside(&a, "map-1-prefix").contains("letters (a to z)"));
    assert!(said_beside(&a, "map-3-template").contains("not an https address"));
    assert!(said_beside(&a, "map-4-template").contains("exactly once"));
    // The rows that are sound say nothing.
    let main = main_of(&a);
    assert!(!main.contains("id=\"c-map-2-prefix-problem\""), "{main}");
    assert_eq!(proposals_made(&e), 0);
}

#[tokio::test]
async fn the_sentences_are_in_japanese_too_and_what_was_typed_is_shown_again_escaped() {
    let e = env_with_help(None);
    let fields = with_map(
        &opened(&e, &e.maintainer, "send").await,
        &[
            ("<b>x</b>", OK),
            ("help", OK),
            ("gh", "http://example.org/{title}"),
        ],
    );
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
    let a = common::send_body(
        &e.w,
        "POST",
        EDIT,
        Some(&e.maintainer),
        &[
            ("sec-fetch-site", "same-origin"),
            ("cookie", "inklkunn_lang=ja"),
            ("x-csrf-token", common::TEST_CSRF),
        ],
        body,
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(
        said_beside(&a, "map-1-prefix")
            .contains("接頭辞に使えるのは、英字（a から z）、数字、ハイフンだけです。"),
        "{}",
        a.body()
    );
    assert!(
        said_beside(&a, "map-2-prefix").contains("2 通りの意味に取れてしまいます。"),
        "{}",
        a.body()
    );
    assert!(
        said_beside(&a, "map-3-template").contains("https で始まるアドレスになっていません。"),
        "{}",
        a.body()
    );
    // What was typed is escaped when shown again; no tag of the person's own reaches the page.
    let body = a.body();
    assert!(!body.contains("<b>x</b>"), "{body}");
    assert!(body.contains("&#60;b&#62;x&#60;/b&#62;"), "{body}");
    assert_eq!(proposals_made(&e), 0);
}

#[tokio::test]
async fn an_unchanged_map_is_not_judged_against_the_namespaces_a_page_made_later_took_but_a_changed_one_is()
 {
    // `gh` was accepted, and a page `GH/Notes` was made after: the map in force clashes with the wiki's namespaces now.
    let notes = common::page(&["title: GH/Notes"], "Notes.\n");
    let home = common::home_with("The bridge opened in 1950.");
    let map = format!("gh: \"{OK}\"\n");
    let e = cast(common::with_accounts(common::World::with_visibility(
        "handbook",
        "The Handbook",
        Visibility::Public,
        &[
            ("pages/Home.md", &home),
            ("pages/GH/Notes.md", &notes),
            (MAP_FILE, &map),
        ],
    )));
    // A change to the rules alone is not refused for it (the file is not written, and acceptance does not judge it).
    let fields = with(&opened(&e, &e.maintainer, "send").await, "approvals", "1");
    let id = proposal_of(&post_fields(&e.w, Some(&e.maintainer), &fields).await);
    let paths: Vec<String> = files_of(&e.w, &id).into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, [POLICY_FILE]);
    // A change to the map is: acceptance would refuse the file as a whole, so the form says it beside the row.
    refused_beside(
        &e,
        &[("gh", OK), ("other", OK)],
        "map-1-prefix",
        "how the titles of some pages of this wiki begin",
    )
    .await;
}

#[tokio::test]
async fn who_may_send_the_form_is_unchanged_for_the_map() {
    let e = env();
    let fields = with_map(&opened(&e, &e.maintainer, "send").await, &[("gh", OK)]);
    for who in [&e.observer, &e.newcomer] {
        let posted = post_fields(&e.w, Some(who), &fields).await;
        assert_eq!(
            posted.status,
            StatusCode::FORBIDDEN,
            "{who}: {}",
            posted.body()
        );
    }
    let visitor = post_fields(&e.w, None, &fields).await;
    assert!(visitor.status.is_client_error(), "{}", visitor.body());
    assert_eq!(proposals_made(&e), 0);
}

#[tokio::test]
async fn the_repair_of_a_map_that_cannot_be_read_writes_the_rows_typed_or_removes_the_file() {
    let fault = ReadFault::from_stored(GovernanceFile::Interwiki, Some(1), Refusal::TemplateHost);
    let broken = || {
        env_rules(Rules::faulted(
            Some(common::POLICY_BLOCK.to_owned()),
            fault.clone(),
        ))
    };
    // The form starts from no row and names the fault.
    let e = broken();
    let a = as_(&e, &e.admin, EDIT).await;
    assert_eq!(a.status, StatusCode::OK, "{}", a.body());
    assert!(main_of(&a).contains("governance/interwiki.yaml, line 1"));
    assert_eq!(
        value(&form_fields(&main_of(&a)), "map_rows"),
        "0",
        "{}",
        a.body()
    );
    // A map typed replaces the file that could not be read.
    let id = proposal_of(&send_map(&e, &e.admin, &[("gh", OK)]).await);
    assert_eq!(
        files_of(&e.w, &id),
        vec![(MAP_FILE.to_owned(), Some(canonical(&[("gh", OK)])))]
    );
    // And a refusal is said beside its row there too.
    let e = broken();
    let a = send_map(&e, &e.admin, &[("gh", "http://example.org/{title}")]).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{}", a.body());
    assert!(said_beside(&a, "map-1-template").contains("not an https address"));
    assert_eq!(proposals_made(&e), 0);
}

#[tokio::test]
async fn while_the_policy_cannot_be_read_the_map_in_force_stays_and_is_not_written() {
    let fault = ReadFault::from_stored(GovernanceFile::Policy, Some(3), Refusal::WrongShape);
    let home = common::home_with("The bridge opened in 1950.");
    let w = common::World::with_visibility(
        "handbook",
        "The Handbook",
        Visibility::Public,
        &[("pages/Home.md", &home), (MAP_FILE, MAP)],
    );
    let e = cast(common::under_rules(
        &w,
        Rules::faulted(Some(common::POLICY_BLOCK.to_owned()), fault),
    ));
    let fields = opened(&e, &e.admin, "send").await;
    assert_eq!(value(&fields, "map_rows"), "2");
    let id = proposal_of(&post_fields(&e.w, Some(&e.admin), &fields).await);
    let paths: Vec<String> = files_of(&e.w, &id).into_iter().map(|(p, _)| p).collect();
    assert_eq!(paths, [POLICY_FILE]);
}

#[test]
fn the_form_and_the_validator_agree_on_what_a_canonical_map_is() {
    let map: Interwiki =
        validate_interwiki(&format!("b: \"{OK}\"\na: \"{WIKI}\"\n"), &BTreeSet::new())
            .expect("a map");
    assert_eq!(
        canonical(&[("b", OK), ("a", WIKI)]),
        map.to_yaml(),
        "the order of the rows is not the file's"
    );
}
