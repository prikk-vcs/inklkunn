//! The words of an import (RFC 007 D-3; acceptance criterion 8; handoff I-T3), through the real router: an imported revision's row says
//! *Imported. Originally by {name}, {date}: {summary}* with the source's date labelled *originally*, in both languages; the notice names
//! the source as text, the day, the checksums and whose word the license is; **a source author's name is never linked to or matched with
//! an account**, and a name with markup in it is text. The repository is an in-memory double with a scripted history, so these run in both
//! test runs.
#![allow(clippy::expect_used, clippy::unwrap_used)]

#[path = "../common/mod.rs"]
mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use common::{World, op, page, shown};
use inklkunn_core::message::{
    ChecksState, ImportNotice, ImportSource, LicenseWord, Message, SourceAuthorKind,
    SourceRevision, Summary, SystemLabel,
};
use inklkunn_web::WebConfig;
use inklkunn_web::routes::site_with;
use tower::ServiceExt as _;

fn router(w: &World) -> Router {
    site_with(
        &WebConfig::new(
            "127.0.0.1:8080",
            "http://localhost:8080",
            "http://localhost:8081",
        )
        .unwrap(),
        Some(w.services.clone()),
    )
    .unwrap()
}

async fn get(w: &World, uri: &str, cookie: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header(header::HOST, "localhost:8080");
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    let res = router(w)
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let (parts, body) = res.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 22).await.unwrap();
    (parts.status, String::from_utf8_lossy(&bytes).into_owned())
}

/// The message of one replayed revision, as the import writes it.
fn replayed(author: &str, summary: Option<&str>, notice: ImportNotice) -> String {
    let mut revision = SourceRevision::new("r-17", 1_234_567_890)
        .unwrap()
        .with_author(SourceAuthorKind::Person, author);
    revision = match summary {
        Some(s) => revision.with_summary(s),
        None => revision.with_summary_hidden(),
    };
    Message::import_of_bundle(
        Summary::new("Import a revision of Home").unwrap(),
        SystemLabel::inklkunn(),
        ImportSource::new("bundle sha256:00").unwrap(),
        notice,
        Some(revision),
    )
    .to_text()
}

fn notice(license: LicenseWord, checks: ChecksState) -> ImportNotice {
    ImportNotice::new(1_764_000_000, checks, license)
        .unwrap()
        .with_source_name("Garden Notes")
        .with_source_address("https://garden.example.org/wiki")
}

fn world(message: &str) -> World {
    let w = World::new(
        "handbook",
        "The Handbook",
        &[("pages/Home.md", &page(&["title: Home"], "Home.\n"))],
    );
    w.fake.say(1, "Create the handbook");
    w.fake
        .show(1, shown(vec![op("create-file", &["pages/Home.md"])]));
    w.fake.set_files(&[(
        "pages/Home.md",
        &page(&["title: Home"], "Home, imported.\n"),
    )]);
    w.fake.say(2, message);
    w.fake
        .show(2, shown(vec![op("edit-text", &["pages/Home.md"])]));
    w
}

fn main_of(body: &str) -> &str {
    let from = body.find("<main").expect("a main");
    let to = body.find("</main>").expect("a main end");
    body.get(from..to).expect("main")
}

#[tokio::test]
async fn an_imported_revision_says_who_wrote_it_at_the_source_and_when_and_the_page_says_where_it_came_from()
 {
    let w = world(&replayed(
        "Ada Lovelace",
        Some("Fix the table"),
        notice(LicenseWord::DeclaredByAdmin, ChecksState::None),
    ));
    let (status, body) = get(&w, "/handbook/-/history/Home", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let main = main_of(&body);
    // The row: the source's author as a name at the source, the source's date labelled originally, the source's summary.
    assert!(
        main.contains("Imported. Originally by Ada Lovelace (a name at the source), 2009-02-13 23:31 UTC (originally): Fix the table"),
        "{main}"
    );
    // Not the wiki's: nothing of the row says the wiki service made it, and no person signed it.
    assert!(!main.contains("Made by"), "{main}");
    // The notice: the source's name and address as text, the day of the import, no checksums, the admin's license.
    assert!(
        main.contains(
            "Imported from Garden Notes (https://garden.example.org/wiki) on 2025-11-24 16:00 UTC."
        ),
        "{main}"
    );
    assert!(
        main.contains("The bundle had no checksums, so its files were not checked."),
        "{main}"
    );
    assert!(
        main.contains("The license was declared by the importing admin; the bundle declared none."),
        "{main}"
    );
    // Text, never a link: the address is not an anchor, and nothing of the source is.
    assert!(
        !main.contains("href=\"https://garden.example.org"),
        "{main}"
    );
    // The two dates are not one: the source's date is not the day of the import.
    assert!(
        !main.contains("2025-11-24 16:00 UTC (originally)"),
        "{main}"
    );
    // The same, in Japanese.
    let ja = Some("inklkunn_lang=ja");
    let (_, body) = get(&w, "/handbook/-/history/Home", ja).await;
    let main = main_of(&body);
    assert!(
        main.contains("インポートされた変更。元の作成者: Ada Lovelace（出典での名前）、2009-02-13 23:31 UTC（元の日時）: Fix the table"),
        "{main}"
    );
    assert!(
        main.contains("Garden Notes（https://garden.example.org/wiki） から 2025-11-24 16:00 UTC にインポートしました。"),
        "{main}"
    );
    assert!(
        main.contains("バンドルにチェックサムがなかったため、ファイルの照合は行っていません。"),
        "{main}"
    );
    assert!(
        main.contains(
            "ライセンスは、バンドルではなく、インポートを実行した管理者が宣言したものです。"
        ),
        "{main}"
    );
    // The recent changes say it too, with the day this service first saw the change in its own column.
    let (_, body) = get(&w, "/handbook/-/changes", None).await;
    let main = main_of(&body);
    assert!(
        main.contains("Imported. Originally by Ada Lovelace"),
        "{main}"
    );
    assert!(main.contains("Imported from Garden Notes"), "{main}");
    assert!(
        main.contains("The files of the bundle were checked") == false,
        "{main}"
    );
}

#[tokio::test]
async fn a_source_name_is_text_and_is_never_linked_to_an_account_even_one_of_the_same_name_or_one_with_markup()
 {
    // A local account has the same name, and the markup of a name is text.
    for name in ["Ada Lovelace", "<b>Eve</b> & \"co\""] {
        let w = common::with_accounts(world(&replayed(
            name,
            Some("x"),
            notice(LicenseWord::Manifest, ChecksState::Verified),
        )));
        if name == "Ada Lovelace" {
            common::add_person(&w, &common::person(7), name, None);
        }
        let (_, body) = get(&w, "/handbook/-/history/Home", None).await;
        let main = main_of(&body);
        // The name is in the row, escaped, and in no link: not to a person's page, not to a profile.
        let escaped: String = name
            .chars()
            .map(|c| match c {
                '&' | '<' | '>' | '"' | '\'' => format!("&#{};", c as u32),
                c => c.to_string(),
            })
            .collect();
        assert!(main.contains(&escaped), "{name}: {main}");
        assert!(!main.contains("<b>Eve"), "{main}");
        assert!(
            !main.contains(&format!("{escaped}</a>")),
            "{name}: a link: {main}"
        );
        assert!(
            !main.contains("/-/people") && !main.contains("/-/account"),
            "{name}: a link to a person: {main}"
        );
        // The checksums were verified: said, and the admin's license is not claimed.
        assert!(main.contains("The files of the bundle were checked against its checksums."));
        assert!(!main.contains("declared by the importing admin"));
    }
}

#[tokio::test]
async fn what_the_source_hid_is_said_hidden_at_the_source_and_a_plain_import_has_no_notice_of_a_bundle()
 {
    // A summary the source hid.
    let w = world(&replayed(
        "Ada Lovelace",
        None,
        notice(LicenseWord::Manifest, ChecksState::Verified),
    ));
    let (_, body) = get(&w, "/handbook/-/history/Home", None).await;
    assert!(
        main_of(&body).contains("2009-02-13 23:31 UTC (originally): hidden at the source"),
        "{}",
        main_of(&body)
    );
    let (_, body) = get(&w, "/handbook/-/history/Home", Some("inklkunn_lang=ja")).await;
    assert!(
        main_of(&body).contains("出典で非表示"),
        "{}",
        main_of(&body)
    );
    // A folder import (no notice, no source facts) is what it was: made by the wiki service, with no notice of a bundle.
    let w = world(&common::import_message("Import 3 pages"));
    let (_, body) = get(&w, "/handbook/-/history/Home", None).await;
    let main = main_of(&body);
    assert!(main.contains("Made by the wiki service"), "{main}");
    assert!(
        !main.contains("Imported from") && !main.contains("Originally by"),
        "{main}"
    );
}
