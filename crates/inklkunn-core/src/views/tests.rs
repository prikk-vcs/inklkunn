//! A new view's name, and the list of a wiki's views with the origin the service recorded (RFC 005 part 2 D-9;
//! handoff 005 K-T8).
#![allow(clippy::expect_used, clippy::unwrap_used)]

use inklkunn_store::{
    AccountId, AccountStore, ActStore, ViewActEnd, ViewActId, ViewActOutcome, ViewActRecord,
    ViewKind,
};

use super::*;
use crate::read::ReadConfig;
use crate::read::testing::World;

#[test]
fn a_new_name_is_lower_case_letters_digits_and_a_few_marks_and_is_never_changed() {
    for good in [
        "draft",
        "a",
        "9lives",
        "reading-2",
        "v1.2",
        "a_b",
        &"x".repeat(MAX_NAME_CHARS),
    ] {
        assert_eq!(new_name(good), Ok(good), "{good}");
    }
    assert_eq!(new_name(""), Err(NameFault::Empty));
    assert_eq!(
        new_name(&"x".repeat(MAX_NAME_CHARS + 1)),
        Err(NameFault::TooLong)
    );
    for start in ["-draft", ".draft", "_draft", "Draft", "é"] {
        assert_eq!(new_name(start), Err(NameFault::Start), "{start}");
    }
    for bad in [
        "Draft",
        "dra ft",
        " draft",
        "draft ",
        "dra/ft",
        "dra\\ft",
        "dra:ft",
        "draft\n",
        "d..f",
        "draft.",
        "dräft",
        "ドラフト",
        "a?b",
        "a#b",
        "a%20b",
        "a~b",
    ] {
        assert!(new_name(bad).is_err(), "{bad:?} is not a name");
    }
    // The length is counted in characters, not bytes: a long name of multi-byte characters is refused for its
    // characters, not mistaken for a short one.
    assert!(new_name(&"é".repeat(MAX_NAME_CHARS)).is_err());
}

#[test]
fn a_name_that_is_a_retired_word_whole_is_refused_in_any_case_and_a_name_that_holds_one_is_not() {
    // K12: no view calls itself official, canonical, adopted, true, false, verified or proven.
    assert_eq!(
        RETIRED_NAMES,
        [
            "official",
            "canonical",
            "adopted",
            "true",
            "false",
            "verified",
            "proven"
        ]
    );
    for word in RETIRED_NAMES {
        assert_eq!(new_name(word), Err(NameFault::Retired), "{word}");
        // In any case: the reason is the word, not the capital that would also have been refused.
        let upper = word.to_uppercase();
        assert_eq!(new_name(&upper), Err(NameFault::Retired), "{upper}");
        let mut chars = word.chars();
        let titled: String = chars
            .next()
            .map(|c| c.to_ascii_uppercase())
            .into_iter()
            .chain(chars)
            .collect();
        assert_eq!(new_name(&titled), Err(NameFault::Retired), "{titled}");
        // Whole only: a name that holds the word, before or after, is a name.
        for ok in [
            format!("{word}-draft"),
            format!("my-{word}"),
            format!("{word}2"),
            format!("{word}.x"),
        ] {
            assert_eq!(new_name(&ok), Ok(ok.as_str()), "{ok}");
        }
    }
    // The ordinary names stay names.
    assert_eq!(new_name("trueish"), Ok("trueish"));
    assert_eq!(new_name("untrue"), Ok("untrue"));
}

#[test]
fn the_short_name_drops_the_repositorys_prefix_and_nothing_else() {
    assert_eq!(short_name("heads/draft"), "draft");
    assert_eq!(short_name("draft"), "draft");
    assert_eq!(short_name("heads/a/b"), "a/b");
}

/// A real account: the store refuses a creator that is not one (handoff 005 K18).
fn creator(w: &World) -> AccountId {
    let id = AccountId::new();
    w.store
        .add_account(&inklkunn_store::Account {
            id: id.clone(),
            display_name: inklkunn_store::DisplayName::new("Rae Reviewer").unwrap(),
            local: None,
            created_at: 1,
            disabled: false,
        })
        .unwrap();
    id
}

fn act(w: &World, name: &str, kind: ViewKind, from: &str) -> ViewActRecord {
    ViewActRecord {
        id: ViewActId::new(),
        wiki: w.wiki.id.clone(),
        name: name.to_owned(),
        forked_from: from.to_owned(),
        from_block: "e".repeat(64),
        kind,
        created_by: creator(w),
        created_at: 1,
    }
}

fn world() -> World {
    World::with(
        &[("pages/Home.md", "---\ntitle: Home\n---\n\nHello.\n")],
        ReadConfig::default(),
    )
}

#[test]
fn the_default_view_is_listed_first_and_a_view_nobody_recorded_has_no_origin() {
    let w = world();
    w.counting.inner().add_view("heads/zeta");
    w.counting.inner().add_view("heads/alpha");
    let list = list(&w.service, &*w.store, &w.wiki).unwrap();
    let names: Vec<(&str, bool)> = list
        .rows
        .iter()
        .map(|r| (r.name.as_str(), r.is_default))
        .collect();
    assert_eq!(names, [("main", true), ("alpha", false), ("zeta", false)]);
    assert!(list.rows.iter().all(|r| r.origin.is_none()));
    assert_eq!(list.omitted, 0);
}

#[test]
fn a_recorded_view_shows_its_kind_and_where_it_was_forked_from() {
    let w = world();
    w.counting.inner().add_view("heads/reading");
    w.counting.inner().add_view("heads/scratch");
    let a = act(&w, "reading", ViewKind::Interpretation, "main");
    w.store.begin_view_act(&a).unwrap();
    w.store
        .finish_view_act(&ViewActEnd {
            act: a.id.clone(),
            outcome: ViewActOutcome::Created,
            at: 2,
        })
        .unwrap();
    // An act still open stands too: the view is there, and the record of its end may have been lost.
    let open = act(&w, "scratch", ViewKind::Experimental, "reading");
    w.store.begin_view_act(&open).unwrap();
    let list = list(&w.service, &*w.store, &w.wiki).unwrap();
    assert_eq!(
        list.view("reading").unwrap().origin,
        Some(ViewOrigin {
            kind: ViewKind::Interpretation,
            forked_from: "main".to_owned(),
            from_block: "e".repeat(64),
        })
    );
    assert_eq!(
        list.view("scratch")
            .unwrap()
            .origin
            .as_ref()
            .map(|o| (o.kind, o.forked_from.as_str())),
        Some((ViewKind::Experimental, "reading"))
    );
    assert_eq!(list.view("main").unwrap().origin, None);
}

#[test]
fn an_act_that_failed_is_no_origin_for_a_view_of_that_name_made_some_other_way() {
    let w = world();
    w.counting.inner().add_view("heads/reading");
    let a = act(&w, "reading", ViewKind::Interpretation, "main");
    w.store.begin_view_act(&a).unwrap();
    w.store
        .finish_view_act(&ViewActEnd {
            act: a.id,
            outcome: ViewActOutcome::Failed {
                word: "refused".to_owned(),
            },
            at: 2,
        })
        .unwrap();
    let list = list(&w.service, &*w.store, &w.wiki).unwrap();
    assert_eq!(list.view("reading").unwrap().origin, None);
}

#[test]
fn a_record_of_a_view_that_is_not_there_is_not_listed() {
    let w = world();
    w.store
        .begin_view_act(&act(&w, "gone", ViewKind::Experimental, "main"))
        .unwrap();
    let list = list(&w.service, &*w.store, &w.wiki).unwrap();
    assert_eq!(list.rows.len(), 1);
    assert!(list.view("gone").is_none());
}

#[test]
fn the_list_is_bounded_and_says_how_many_it_left_out() {
    let w = world();
    for n in 0..(MAX_VIEWS + 5) {
        w.counting.inner().add_view(&format!("heads/v{n:03}"));
    }
    let list = list(&w.service, &*w.store, &w.wiki).unwrap();
    assert_eq!(list.rows.len(), MAX_VIEWS);
    assert_eq!(list.omitted, 6, "105 + the default view = 106 open views");
    assert_eq!(list.rows.first().map(|r| r.name.as_str()), Some("main"));
}
