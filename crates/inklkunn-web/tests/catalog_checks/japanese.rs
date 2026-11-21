//! M60: the rules the Japanese catalog was signed off under (the owner's sign-off of 2026-11-19 and the architect's
//! final check; `docs/glossary.md` section 3.2). They hold every message, and each check is shown able to fail on a
//! planted text, so a later message that breaks one fails the build instead of waiting for a reader.
//!
//! * no `さん` after a person placeholder (a person is named without it everywhere: when the person is unknown the
//!   value is `だれか`, and *だれか さん* is wrong);
//! * a colon is half-width and followed by a space (or ends the message);
//! * a placeholder next to Japanese text has a half-width space on each side, except inside 「」;
//! * a counted message is one form, written `msg(…)`: Japanese has no grammatical plural;
//! * the renderings the check replaced do not come back (one rendering per meaning, the glossary's section 1).

use std::fs;

use inklkunn_web::catalog::JA_DEF;

use super::{read, root};

/// Every Japanese message, with its key, in both forms.
fn messages() -> Vec<(&'static str, &'static str)> {
    JA_DEF
        .entries
        .iter()
        .flat_map(|e| [(e.key, e.one), (e.key, e.other)])
        .collect()
}

/// Run `check` over every Japanese message: the problems, each naming its key.
fn found(check: impl Fn(&str) -> Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for (key, text) in messages() {
        out.extend(check(text).into_iter().map(|p| format!("{key}: {p}")));
    }
    out.sort();
    out.dedup();
    out
}

// ---------------------------------------------------------------- no さん after a person

/// `さん` straight after a placeholder, spaced or not.
fn san_after_a_placeholder(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices('}') {
        let rest = text.get(at + 1..).unwrap_or("").trim_start_matches(' ');
        if rest.starts_with("さん") {
            out.push(format!("a placeholder is followed by さん: {text}"));
        }
    }
    out
}

#[test]
fn no_message_puts_san_after_a_person_placeholder() {
    assert_eq!(found(san_after_a_placeholder), Vec::<String>::new());
    // Shown able to fail: with and without the space.
    assert!(!san_after_a_placeholder("{person} さんが見送りました。").is_empty());
    assert!(!san_after_a_placeholder("{author}さんに返信する").is_empty());
    assert!(san_after_a_placeholder("{person} が見送りました。").is_empty());
}

// ---------------------------------------------------------------- colons

/// A full-width colon, or a half-width one followed by anything but a space (the end of the message is fine).
fn colon_problems(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    if text.contains('：') {
        out.push(format!("a full-width colon: {text}"));
    }
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == ':' && chars.get(i + 1).is_some_and(|n| *n != ' ') {
            out.push(format!("a colon without a space after it: {text}"));
        }
    }
    out
}

#[test]
fn a_colon_is_half_width_and_followed_by_a_space() {
    assert_eq!(found(colon_problems), Vec::<String>::new());
    assert!(!colon_problems("主張の段階：{lifecycle}。").is_empty());
    assert!(!colon_problems("書き添えました:「{text}」").is_empty());
    assert!(colon_problems("書き添えました: 「{text}」").is_empty());
    assert!(colon_problems("これを引用しています:").is_empty());
}

// ---------------------------------------------------------------- a placeholder beside Japanese text

/// A character of Japanese text (kana, the long vowel mark, kanji): not Japanese punctuation, which carries its own
/// space.
fn is_japanese_letter(c: char) -> bool {
    matches!(c,
        '\u{3041}'..='\u{309f}' | '\u{30a1}'..='\u{30fa}' | '\u{30fc}'..='\u{30ff}' | '\u{3005}' | '\u{4e00}'..='\u{9fff}')
}

/// A placeholder outside 「」 that touches Japanese text on either side.
fn tight_placeholders(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let (mut depth, mut i) = (0usize, 0usize);
    while let Some(&c) = chars.get(i) {
        match c {
            '「' => depth += 1,
            '」' => depth = depth.saturating_sub(1),
            '{' => {
                let end = chars
                    .iter()
                    .enumerate()
                    .skip(i)
                    .find(|(_, ch)| **ch == '}')
                    .map_or(chars.len() - 1, |(at, _)| at);
                if depth == 0 {
                    let before = i.checked_sub(1).and_then(|b| chars.get(b));
                    let after = chars.get(end + 1);
                    if before.is_some_and(|b| is_japanese_letter(*b))
                        || after.is_some_and(|a| is_japanese_letter(*a))
                    {
                        out.push(format!(
                            "a placeholder touches Japanese text: {}",
                            chars.iter().collect::<String>()
                        ));
                    }
                }
                i = end;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

#[test]
fn a_placeholder_next_to_japanese_text_is_spaced_except_inside_brackets() {
    assert_eq!(found(tight_placeholders), Vec::<String>::new());
    // Shown able to fail: either side, and the exceptions stay exceptions.
    assert!(!tight_placeholders("主張{claim}を記録します。").is_empty());
    assert!(!tight_placeholders("{view}から分岐しました。").is_empty());
    assert!(tight_placeholders("主張 {claim} を記録します。").is_empty());
    assert!(tight_placeholders("ビュー「{view}」を表示しています。").is_empty());
    assert!(tight_placeholders("出典: {title}（{id}）。{first}、{second}").is_empty());
    assert!(!tight_placeholders("{count}件").is_empty());
}

// ---------------------------------------------------------------- counted messages

#[test]
fn a_counted_message_is_one_form_written_msg() {
    for e in JA_DEF.entries {
        if e.one.contains("{count}") || e.other.contains("{count}") {
            assert_eq!(
                e.one, e.other,
                "{}: Japanese has no grammatical plural",
                e.key
            );
        }
    }
    // The table does not spell a counted message with two equal forms.
    let source = read(&root().join("src/catalog/ja.rs"));
    let calls: Vec<&str> = source
        .lines()
        .filter(|l| l.contains("plural_msg"))
        .collect();
    assert_eq!(
        calls,
        Vec::<&str>::new(),
        "write a counted message with msg(…)"
    );
}

// ---------------------------------------------------------------- one rendering per meaning

/// A rendering the check retired: the text, what must not come before or after it for it to count, and what is said
/// instead.
struct Retired {
    term: &'static str,
    not_after: &'static [&'static str],
    not_before: &'static [&'static str],
    instead: &'static str,
}

const fn r(term: &'static str, instead: &'static str) -> Retired {
    Retired {
        term,
        not_after: &[],
        not_before: &[],
        instead,
    }
}

const RETIRED: &[Retired] = &[
    r("リビジョン", "修正版"),
    r("改訂", "修正版 (the numbered noun), 修正 (the act)"),
    r("変更を求め", "修正を求める"),
    r("レビュー担当者", "レビュアー"),
    r("認められている", "受け入れられている"),
    r("認められています", "受け入れられています"),
    r("争われ", "異論がある"),
    r("決着していない", "未解決"),
    r("コミュニティの状態", "コミュニティでの扱い"),
    r("扱いの段階", "主張の段階"),
    r("支持する", "裏付ける"),
    r("異議を唱え", "異議を示す"),
    r("出典リンク", "出典との結びつき"),
    r("利用できるかどうか", "入手できるかどうか"),
    r("題名", "タイトル"),
    r("機械", "自動化ツール"),
    r("示されていません", "未記入"),
    r("へ戻る", "に戻る"),
    r(
        "運用している人",
        "このサーバーの管理者 / このウィキを運営している人",
    ),
    r(
        "運用して",
        "このサーバーの管理者 / このウィキを運営している人",
    ),
    r("知らせてください", "お知らせください"),
    r("伝えてください", "お知らせください"),
    r("試してください", "お試しください"),
    r("最初に確認", "初めて確認"),
    r("直前の確認", "前回の確認"),
    r("鍵の変更", "鍵の更新作業"),
    r("失効時", "廃止時"),
    r("レビュー待ちのまま", "レビュー待ちで保持されたまま"),
    r("未決の", "進行中の"),
    r("見られます", "閲覧できます"),
    r("何も変更されて", "何も変更して"),
    r("改善の提案", "改善のヒント"),
    r("リンターからの提案", "改善のヒント"),
    r("ぐるぐる", "循環"),
    r("輪になって", "循環"),
    r("1 度", "1 回"),
    r("さん", "(a person is named without it)"),
    Retired {
        term: "ブラウザ",
        not_after: &[],
        not_before: &["ー"],
        instead: "ブラウザー",
    },
    Retired {
        term: "他の",
        not_after: &["その"],
        not_before: &[],
        instead: "ほかの",
    },
];

/// The retired renderings a text still holds.
fn retired_renderings(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for r in RETIRED {
        for (at, _) in text.match_indices(r.term) {
            let before = text.get(..at).unwrap_or("");
            let after = text.get(at + r.term.len()..).unwrap_or("");
            if r.not_after.iter().any(|p| before.ends_with(p))
                || r.not_before.iter().any(|p| after.starts_with(p))
            {
                continue;
            }
            out.push(format!("{} is retired, say {}: {text}", r.term, r.instead));
        }
    }
    out
}

#[test]
fn the_renderings_the_architects_check_replaced_do_not_come_back() {
    assert_eq!(found(retired_renderings), Vec::<String>::new());
    // Shown able to fail, with the exceptions that are not failures.
    for bad in [
        "直接に異議を唱えている",
        "このリビジョンには",
        "この主張は争われています。",
        "このサーバーを運用している人にお知らせください。",
        "このブラウザをサインアウトします。",
        "他の人が読めます。",
        "{when} · 改訂 {number} について",
    ] {
        assert!(!retired_renderings(bad).is_empty(), "{bad}");
    }
    for fine in [
        "このブラウザーをサインアウトします。",
        "ほかの人が読めます。",
        "その他の種類",
        "このサーバーの管理者にお知らせください。",
    ] {
        assert!(retired_renderings(fine).is_empty(), "{fine}");
    }
}

/// The retired list names every term of the glossary's section 3.2 table, so the page and the test say one thing.
#[test]
fn the_glossary_names_the_retired_renderings_of_section_3_2() {
    let glossary = fs::read_to_string(root().join("../../docs/glossary.md")).expect("glossary");
    let section = glossary
        .split("### 3.2")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .expect("the glossary has a section 3.2");
    for r in RETIRED {
        assert!(
            section.contains(r.term),
            "section 3.2 of the glossary does not name {}",
            r.term
        );
    }
}
