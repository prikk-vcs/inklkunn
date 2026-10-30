//! What is refused, in which order, and what is kept whole: sizes before parsing, the keys the service owns,
//! titles, line endings and multibyte text, hostile content in a preview and in a difference, and a form from
//! another site.

use axum::http::StatusCode;
use inklkunn_core::Principal;
use inklkunn_store::{FileChange, ProposalFilter};

use super::{HOME, cast, home_fields, location, read, send};
use crate::common::{page, post_form, send_body};

/// The text of the first file of the newest proposal.
fn stored(c: &super::Cast, who: &Principal) -> Vec<FileChange> {
    let all =
        c.w.services
            .proposals
            .list(who, &c.w.wiki, &ProposalFilter::default())
            .unwrap();
    let p = all.last().unwrap();
    match c.w.services.proposals.draft(who, &c.w.wiki, &p.id) {
        Ok(content) => content.files,
        Err(_) => {
            c.w.services
                .proposals
                .revisions(who, &c.w.wiki, &p.id)
                .unwrap()
                .pop()
                .unwrap()
                .content
                .files
        }
    }
}

fn principal(c: &super::Cast, id: &inklkunn_store::AccountId) -> Principal {
    let _ = c;
    Principal::session(&inklkunn_core::IdentityAuthority::for_tests(), id.clone())
}

#[tokio::test]
async fn a_summary_is_required_and_at_most_a_hundred_and_twenty_characters_and_the_text_stays_in_the_form()
 {
    let c = cast();
    let text = HOME.replace("Second paragraph.", "Second, <b>kept</b> & 日本語.");
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&text, "  ", "submit"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    let body = a.body();
    assert!(
        body.contains("Say in a few words what you changed and why."),
        "{body}"
    );
    // What was typed is still there, escaped.
    assert!(
        body.contains("Second, &#60;b&#62;kept&#60;/b&#62; &#38; 日本語."),
        "{body}"
    );
    let long = "あ".repeat(121);
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&text, &long, "submit"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body()
            .contains("The summary is longer than 120 characters.")
    );
    // A hundred and twenty characters is taken (a character is not a byte).
    let just = "あ".repeat(120);
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&text, &just, "submit"),
    )
    .await;
    assert!(a.status.is_redirection(), "{}", a.body());
    // Nothing was made by the refusals.
    let all =
        c.w.services
            .proposals
            .list(
                &principal(&c, &c.contributor),
                &c.w.wiki,
                &ProposalFilter::default(),
            )
            .unwrap();
    assert_eq!(all.len(), 1);
    // A text that changes nothing is not a proposal.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(HOME, "Nothing", "submit"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body()
            .contains("The text is the same as the page: change it first, then send it.")
    );
    let all =
        c.w.services
            .proposals
            .list(
                &principal(&c, &c.contributor),
                &c.w.wiki,
                &ProposalFilter::default(),
            )
            .unwrap();
    assert_eq!(all.len(), 1, "a refused submission leaves no draft behind");
}

#[tokio::test]
async fn multibyte_text_and_line_breaks_round_trip_through_the_form() {
    let c = cast();
    let me = principal(&c, &c.contributor);
    // A browser sends every line break as CR LF. The page has LF, so the proposal has LF: the text the person
    // did not touch is the page, byte for byte.
    let edited = "---\ntitle: Home\n---\n\nFirst paragraph.\n\nSecond: 日本語、𠮷野家、🙂 e\u{301}.\n\nThird paragraph.\n";
    let sent = edited.replace('\n', "\r\n");
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&sent, "Multibyte", "draft"),
    )
    .await;
    let edit = location(&a);
    assert_eq!(
        stored(&c, &me).first().unwrap().text.as_deref(),
        Some(edited)
    );
    // Shown again, the text is what was stored, and sent again unchanged it is the same proposal text.
    let body = read(&c, &edit, Some(&c.contributor)).await.body();
    assert!(
        body.contains("Second: 日本語、𠮷野家、🙂 e\u{301}."),
        "{body}"
    );
    assert!(!body.contains('\r'));
    let a = send(
        &c,
        &edit,
        &c.contributor,
        &home_fields(&sent, "Multibyte", "draft"),
    )
    .await;
    assert!(a.status.is_redirection());
    assert_eq!(
        stored(&c, &me).first().unwrap().text.as_deref(),
        Some(edited)
    );

    // A page whose line breaks are all CR LF keeps them.
    let crlf_home = HOME.replace('\n', "\r\n");
    c.w.fake.set("pages/Home.md", &crlf_home);
    let mut fields = home_fields(
        &crlf_home.replace("Second paragraph.", "Second, edited."),
        "Keep the breaks",
        "draft",
    );
    fields.retain(|(k, _)| !k.starts_with("base_"));
    fields.push(("base_state", crate::common::state_id(2)));
    fields.push(("base_block", crate::common::block_id(2)));
    let a = send(&c, "/handbook/-/proposals/new", &c.contributor, &fields).await;
    assert!(a.status.is_redirection(), "{}", a.body());
    assert_eq!(
        stored(&c, &me).first().unwrap().text.as_deref(),
        Some(
            crlf_home
                .replace("Second paragraph.", "Second, edited.")
                .as_str()
        )
    );
}

#[tokio::test]
async fn a_size_over_the_bound_is_refused_before_anything_is_parsed() {
    let c = cast();
    // A page over a mebibyte whose information at the top is unreadable and claims a key the service owns:
    // were it parsed, the answer would be about that. The answer is about the size.
    let hostile = format!(
        "---\ntitle: &anchor Home\nkind: system\n---\n\n{}",
        "x".repeat(1024 * 1024)
    );
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&hostile, "Too big", "submit"),
    )
    .await;
    assert_eq!(a.status, StatusCode::PAYLOAD_TOO_LARGE);
    let body = a.body();
    assert!(
        body.contains("A page can hold at most 1 MiB of text."),
        "{body}"
    );
    assert!(
        !body.contains("cannot be read")
            && !body.contains("can only be changed by an administrator")
    );
    // Three pages, each within its bound, are over the proposal's.
    let page_text = format!("---\ntitle: P\n---\n\n{}", "y".repeat(1024 * 1024 - 100));
    let mut fields = home_fields("x", "Three pages", "submit");
    fields.retain(|(k, _)| *k != "pages");
    fields.push(("pages", "3".to_owned()));
    fields.retain(|(k, _)| *k != "p0_text");
    fields.push(("p0_text", page_text.clone()));
    for i in 1..3 {
        fields.push((
            Box::leak(format!("p{i}_path").into_boxed_str()),
            format!("pages/P{i}.md"),
        ));
        fields.push((
            Box::leak(format!("p{i}_text").into_boxed_str()),
            page_text.clone(),
        ));
    }
    let a = send(&c, "/handbook/-/proposals/new", &c.contributor, &fields).await;
    assert_eq!(a.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        a.body()
            .contains("A proposal can hold at most 2 MiB of text in all.")
    );
    // A body larger than any form can be is refused by the bound on the body.
    let big = format!("op=submit&p0_text={}", "%E3%81%82".repeat(1024 * 1024));
    let a = send_body(
        &c.w,
        "POST",
        "/handbook/-/proposals/new",
        Some(&c.contributor),
        &[("sec-fetch-site", "same-origin")],
        big,
    )
    .await;
    assert_eq!(a.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        a.body()
            .contains("What you sent is larger than a form can carry.")
    );
}

#[tokio::test]
async fn the_keys_the_service_owns_are_refused_unless_the_role_allows() {
    let c = cast();
    let owned = HOME.replace(
        "title: Home",
        "title: Home\nvisibility: public\nkind: redirect",
    );
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&owned, "Sneak", "submit"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(a.body().contains("The lines kind, visibility at the top of the page can only be changed by an administrator of this wiki."), "{}", a.body());
    // Also when only previewing, so it is known before it is sent.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&owned, "Sneak", "preview"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    // An administrator may.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.admin,
        &home_fields(&owned, "Make it a redirect", "submit"),
    )
    .await;
    assert!(a.status.is_redirection(), "{}", a.body());
    // Information that cannot be read is refused for everyone, in words.
    let broken = HOME.replace("title: Home", "title: &anchor Home");
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.admin,
        &home_fields(&broken, "Broken", "submit"),
    )
    .await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        a.body()
            .contains("The information at the top of the page cannot be read.")
    );
}

fn new_page_fields(title: &str, text: &str, op: &str) -> Vec<(&'static str, String)> {
    vec![
        ("op", op.to_owned()),
        ("pages", "1".to_owned()),
        ("part", "whole".to_owned()),
        ("base_state", crate::common::state_id(1)),
        ("base_block", crate::common::block_id(1)),
        ("p0_new", "1".to_owned()),
        ("p0_title", title.to_owned()),
        ("p0_text", text.to_owned()),
        ("summary", "A new page".to_owned()),
    ]
}

#[tokio::test]
async fn a_new_pages_title_goes_through_the_one_function_and_a_refusal_names_the_rule_never_the_path()
 {
    let c = cast();
    let me = principal(&c, &c.contributor);
    for (title, sentence) in [
        (
            "",
            "A title cannot be empty, and no part of it between slashes can be empty.",
        ),
        (
            "a//b",
            "A title cannot be empty, and no part of it between slashes can be empty.",
        ),
        (
            "Guides/..",
            "A part of a title cannot be a single dot or two dots.",
        ),
        (&"x".repeat(300), "A part of the title is too long."),
        (
            &"a/".repeat(600),
            "The title is too long, or has too many parts.",
        ),
    ] {
        let a = send(
            &c,
            "/handbook/-/proposals/new",
            &c.contributor,
            &new_page_fields(title, "Text.\n", "submit"),
        )
        .await;
        assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY, "{title:?}");
        let body = a.body();
        assert!(body.contains(sentence), "{title:?}: {body}");
        assert!(
            !body.contains("pages/") && !body.contains(".md"),
            "the path is never shown: {body}"
        );
    }
    // A title that is a page already is told so.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &new_page_fields("Home", "Text.\n", "submit"),
    )
    .await;
    assert!(
        a.body()
            .contains("A page titled Home exists already. Propose a change to it instead.")
    );
    // A good title makes a proposal for a page at the path the function gives.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &new_page_fields("Guides/Set up 日本", "Text.\n", "submit"),
    )
    .await;
    assert!(a.status.is_redirection(), "{}", a.body());
    let file = stored(&c, &me);
    assert_eq!(
        file.first().unwrap().path,
        inklkunn_markup::path::title_to_path("Guides/Set up 日本").unwrap()
    );
    let page = read(&c, &location(&a), None).await.body();
    assert!(
        page.contains("A new page.") && page.contains("Page: Guides/Set up 日本"),
        "{page}"
    );
    assert!(!page.contains("pages/Guides"));
}

#[tokio::test]
async fn a_section_goes_back_into_the_page_it_came_from_even_after_the_page_moved() {
    let c = cast();
    let me = principal(&c, &c.contributor);
    let guide = page(
        &["title: Guide"],
        "# Setup\n\nInstall it.\n\n## Details\n\nMore.\n\n# Use\n\nUse it.\n",
    );
    // The form was opened on version 1; meanwhile someone changed the page elsewhere.
    c.w.fake
        .set("pages/Guide.md", &guide.replace("Use it.", "Use it well."));
    let fields = vec![
        ("op", "draft".to_owned()),
        ("pages", "1".to_owned()),
        ("part", "section".to_owned()),
        ("ordinal", "1".to_owned()),
        ("base_state", crate::common::state_id(1)),
        ("base_block", crate::common::block_id(1)),
        ("p0_path", "pages/Guide.md".to_owned()),
        ("p0_title", "Guide".to_owned()),
        ("p0_text", "## Details\r\n\r\nMore, and more.".to_owned()),
        ("summary", "Details".to_owned()),
    ];
    let a = send(&c, "/handbook/-/proposals/new", &c.contributor, &fields).await;
    assert!(a.status.is_redirection(), "{}", a.body());
    // The edited section is in the text the form was opened on, not in today's page.
    assert_eq!(
        stored(&c, &me).first().unwrap().text.as_deref(),
        Some(guide.replace("More.", "More, and more.").as_str())
    );
    // A position that is not in the base is refused as a form that is not the one the page offers.
    let mut bad = fields.clone();
    bad.retain(|(k, _)| *k != "ordinal");
    bad.push(("ordinal", "40".to_owned()));
    let a = send(&c, "/handbook/-/proposals/new", &c.contributor, &bad).await;
    assert_eq!(a.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(a.body().contains("The form was not complete."));
}

#[tokio::test]
async fn preview_draws_the_text_through_the_sanitizer_and_shows_the_suggestions() {
    let c = cast();
    let hostile = format!(
        "{HOME}\n<script>alert(1)</script>\n\n[click](javascript:alert(2))\n\n<img src=x onerror=alert(3)>\n\n<iframe src=\"//evil.example/\"></iframe>\n"
    );
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&hostile, "Hostile", "preview"),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(
        body.contains("Preview of Home")
            && body.contains("This is how your text will look. Nothing is saved yet.")
    );
    for raw in [
        "<script",
        "<iframe",
        "onerror=alert(3)>",
        "href=\"javascript:",
        "<img src=x",
    ] {
        assert!(!body.contains(raw), "{raw} reached the page: {body}");
    }
    // The text the person typed is in the text area, escaped, and the rendered copy shows it as letters.
    assert!(body.contains("&#60;script&#62;alert(1)&#60;/script&#62;"));
    // Nothing was kept.
    let all =
        c.w.services
            .proposals
            .list(
                &principal(&c, &c.contributor),
                &c.w.wiki,
                &ProposalFilter::default(),
            )
            .unwrap();
    assert!(all.is_empty());
    // Suggestions are shown, never blockers: a page with no information at the top still previews.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields("No title here\n\n# Top\n", "Plain", "preview"),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(body.contains("Suggestions"), "{body}");
    assert!(
        body.contains("The page has no information at the top."),
        "{body}"
    );
}

#[tokio::test]
async fn show_changes_draws_hostile_lines_as_letters() {
    let c = cast();
    let hostile = HOME.replace(
        "Second paragraph.",
        "Second <img src=x onerror=alert(1)> &amp; <script>alert(2)</script>",
    );
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&hostile, "Hostile", "changes"),
    )
    .await;
    assert_eq!(a.status, StatusCode::OK);
    let body = a.body();
    assert!(body.contains("Part 2 is changed"), "{body}");
    assert!(
        body.contains("&#60;img src=x onerror=alert(1)&#62;"),
        "{body}"
    );
    for raw in ["<img src=x", "<script>alert", "onerror=alert(1)>"] {
        assert!(!body.contains(raw), "{raw} reached the page");
    }
    // The same text in a proposal's page.
    let a = send(
        &c,
        "/handbook/-/proposals/new",
        &c.contributor,
        &home_fields(&hostile, "Hostile", "submit"),
    )
    .await;
    let page = read(&c, &location(&a), None).await.body();
    assert!(
        page.contains("&#60;img src=x onerror=alert(1)&#62;"),
        "{page}"
    );
    assert!(!page.contains("<script>alert") && !page.contains("<img src=x"));
    // And in a summary, a source and a comment.
    let a = send(&c, "/handbook/-/proposals/new", &c.contributor, &{
        let mut f = home_fields(&hostile, "<b>bold</b> summary", "submit");
        f.retain(|(k, _)| *k != "sources");
        f.push(("sources", "<script>x</script>".to_owned()));
        f
    })
    .await;
    let to = location(&a);
    post_form(
        &c.w,
        &format!("{to}/comment"),
        Some(&c.reviewer),
        &[("body", "<script>alert(3)</script>")],
    )
    .await;
    let page = read(&c, &to, None).await.body();
    assert!(
        !page.contains("<b>bold</b>") && !page.contains("<script>"),
        "{page}"
    );
    assert!(page.contains("&#60;script&#62;alert(3)&#60;/script&#62;"));
}

#[tokio::test]
async fn a_form_from_another_site_is_refused_before_anything_is_looked_up() {
    let c = cast();
    let body = "op=submit".to_owned();
    for uri in [
        "/handbook/-/proposals/new",
        "/handbook/-/proposals/019a0000-0000-7000-8000-00000000dddd/comment",
        "/nothing/-/proposals/new",
    ] {
        let a = send_body(
            &c.w,
            "POST",
            uri,
            Some(&c.contributor),
            &[("sec-fetch-site", "cross-site")],
            body.clone(),
        )
        .await;
        assert_eq!(a.status, StatusCode::FORBIDDEN, "{uri}");
        assert!(
            a.body()
                .contains("The form could not be confirmed as coming from a page of this site"),
            "{}",
            a.body()
        );
    }
}

#[tokio::test]
async fn the_sanitizers_hostile_corpus_holds_on_a_preview_and_on_a_difference() {
    let c = cast();
    let vectors = [
        "<svg onload=alert(1)>",
        "<a href=\"data:text/html,x\">d</a>",
        "<style>*{display:none}</style>",
        "<object data=x></object>",
        "<math><mi>x</mi></math>",
        "<form action=x><input type=text name=q></form>",
        "[d](data:text/html;base64,AAAA)",
        "![i](http://evil.example/a.png)",
        "<iframe srcdoc=\"<script>1</script>\"></iframe>",
        "<meta http-equiv=refresh content=0>",
    ];
    for v in vectors {
        let text = format!("{HOME}\n{v}\n");
        for op in ["preview", "changes"] {
            let a = send(
                &c,
                "/handbook/-/proposals/new",
                &c.contributor,
                &home_fields(&text, "Hostile", op),
            )
            .await;
            assert_eq!(a.status, StatusCode::OK, "{v}");
            let body = a.body();
            for raw in [
                "<svg",
                "<style",
                "<object",
                "<math",
                "<iframe",
                "<meta http",
                "<form action=x",
                "<a href=\"data:",
                "<img src=\"http://evil",
                "<input type=text",
                "<script",
            ] {
                assert!(!body.contains(raw), "{op}: {raw} from {v} reached the page");
            }
        }
    }
}
