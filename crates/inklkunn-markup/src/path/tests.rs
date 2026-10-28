#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;

#[test]
fn plain_and_hierarchical_titles() {
    assert_eq!(title_to_path("Home").unwrap(), "pages/Home.md");
    assert_eq!(
        title_to_path("Guides/Getting Started").unwrap(),
        "pages/Guides/Getting Started.md"
    );
    assert_eq!(title_to_path("/Target").unwrap(), "pages/Target.md");
    assert_eq!(
        title_to_path("a-b_c.d (x), 'y' +=@~!").unwrap(),
        "pages/a-b_c.d (x), 'y' +=@~!.md"
    );
}

#[test]
fn non_ascii_and_forbidden_characters_are_percent_encoded_reversibly() {
    // prikk 0.54 refuses non-ASCII repository paths; the title survives in the encoding.
    let p = title_to_path(" 編集の競合 ").unwrap();
    assert_eq!(p, "pages/%E7%B7%A8%E9%9B%86%E3%81%AE%E7%AB%B6%E5%90%88.md");
    assert!(p.is_ascii());
    assert_eq!(path_to_title(&p).as_deref(), Some("編集の競合"));
    assert_eq!(title_to_path("a:b*c?").unwrap(), "pages/a%3Ab%2Ac%3F.md");
    assert_eq!(title_to_path("100%").unwrap(), "pages/100%25.md");
    assert_eq!(path_to_title("pages/100%25.md").as_deref(), Some("100%"));
    assert_eq!(
        title_to_path("a\\b|c\"d<e>f").unwrap(),
        "pages/a%5Cb%7Cc%22d%3Ce%3Ef.md"
    );
    assert_eq!(title_to_path("tab\there").unwrap(), "pages/tab%09here.md");
    // Format characters are encoded too, so a spoofing name is visible in the path.
    assert_eq!(title_to_path("a\u{200b}b").unwrap(), "pages/a%E2%80%8Bb.md");
    // NFD input is normalized before encoding, so the two spellings share one path.
    assert_eq!(title_to_path("Cafe\u{301}"), title_to_path("Café"));
}

#[test]
fn hidden_trailing_dot_and_device_names_are_encoded_not_substituted() {
    assert_eq!(title_to_path(".hidden").unwrap(), "pages/%2Ehidden.md");
    assert_eq!(
        path_to_title("pages/%2Ehidden.md").as_deref(),
        Some(".hidden")
    );
    assert_eq!(title_to_path("ends.").unwrap(), "pages/ends%2E.md");
    assert_eq!(title_to_path("CON").unwrap(), "pages/%43ON.md");
    assert_eq!(title_to_path("nul.txt").unwrap(), "pages/%6Eul.txt.md");
    assert_eq!(title_to_path("lpt9/x").unwrap(), "pages/%6Cpt9/x.md");
    assert_eq!(
        title_to_path("Console").unwrap(),
        "pages/Console.md",
        "only the exact stem"
    );
    assert_eq!(title_to_path(".prikk").unwrap(), "pages/%2Eprikk.md");
    assert_eq!(
        path_to_title("pages/%2Eprikk.md").as_deref(),
        Some(".prikk")
    );
}

#[test]
fn reserved_empty_and_too_long() {
    assert_eq!(
        title_to_path("a/../b"),
        Err(PathError::Reserved("..".into()))
    );
    assert_eq!(title_to_path("a/./b"), Err(PathError::Reserved(".".into())));
    assert_eq!(title_to_path("a//b"), Err(PathError::Empty));
    assert_eq!(title_to_path(""), Err(PathError::Empty));
    assert_eq!(title_to_path(" / "), Err(PathError::Empty));
    assert!(matches!(
        title_to_path(&"x".repeat(256)),
        Err(PathError::TooLong(_))
    ));
    // 90 Japanese characters are 270 bytes encoded nine-fold: too long for one component.
    assert!(matches!(
        title_to_path(&"編".repeat(90)),
        Err(PathError::TooLong(_))
    ));
    // Many short components: the whole path exceeds prikk's 1024 bytes.
    let deep: Vec<&str> = std::iter::repeat_n("ab", 400).collect();
    assert!(matches!(
        title_to_path(&deep.join("/")),
        Err(PathError::PathTooLong(_))
    ));
    assert!(title_to_path(&"x".repeat(255)).is_ok());
}

#[test]
fn both_prikk_caps_hold_to_the_byte_on_the_encoded_path() {
    // The path cap (1024 bytes, `pages/` and `.md` included): three full components, then one that lands the
    // whole path on the cap, and one byte over it.
    let full = "a".repeat(255);
    let on_the_cap = format!("{full}/{full}/{full}/{}", "a".repeat(247));
    let path = title_to_path(&on_the_cap).expect("exactly 1024 bytes is allowed");
    assert_eq!(path.len(), MAX_PATH_BYTES);
    let over = format!("{full}/{full}/{full}/{}", "a".repeat(248));
    assert_eq!(
        title_to_path(&over),
        Err(PathError::PathTooLong(MAX_PATH_BYTES + 1))
    );

    // The component cap (255 bytes) counts what is *written*, not the characters of the title: a character
    // outside ASCII is nine bytes in three escapes, a `%` is three.
    assert!(title_to_path(&"編".repeat(28)).is_ok(), "252 bytes");
    assert!(
        matches!(title_to_path(&"編".repeat(29)), Err(PathError::TooLong(_))),
        "261 bytes"
    );
    assert!(title_to_path(&"%".repeat(85)).is_ok(), "255 bytes");
    assert!(
        matches!(title_to_path(&"%".repeat(86)), Err(PathError::TooLong(_))),
        "258 bytes"
    );
    // A title short as text and long as a path is refused on the path.
    let deep_cjk: Vec<String> = (0..20).map(|_| "編".repeat(14)).collect();
    assert!(
        matches!(
            title_to_path(&deep_cjk.join("/")),
            Err(PathError::PathTooLong(n)) if n > MAX_PATH_BYTES
        ),
        "20 components of 14 characters are 280 characters and 2,500 bytes of path"
    );

    // Nothing is half-made: a refusal names which cap, and the title that is accepted decodes back whole.
    for title in [on_the_cap.as_str(), &"編".repeat(28)] {
        let p = title_to_path(title).expect("accepted");
        assert_eq!(path_to_title(&p).as_deref(), Some(title));
    }
}

#[test]
fn inverse_suffix_and_collision_key() {
    assert_eq!(
        path_to_title("pages/Guides/Getting Started.md").as_deref(),
        Some("Guides/Getting Started")
    );
    assert_eq!(path_to_title("attachments/a.png"), None);
    assert_eq!(path_to_title("pages/bad%2.md"), None, "a broken escape");
    assert_eq!(path_to_title("pages/%FF.md"), None, "not UTF-8");
    assert_eq!(path_to_title("pages//x.md"), None);
    assert_eq!(with_suffix("pages/a/b.md", 2), "pages/a/b-2.md");
    assert_eq!(collision_key("pages/A/b.md"), collision_key("pages/a/B.md"));
    assert!(is_inside_pages("pages/a/b.md"));
    assert!(!is_inside_pages("pages/../x.md"));
    assert!(!is_inside_pages("knowledge/c.md"));
    assert!(
        !is_inside_pages("pages/編.md"),
        "a non-ASCII path cannot be ours"
    );
}
