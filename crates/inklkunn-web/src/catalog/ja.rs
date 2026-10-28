//! The Japanese messages: **a preview** (DR-7). The draft is the dev team's, written from the English and
//! the glossary's Japanese terms; nothing here has been read by a fluent reviewer yet, and every page that
//! offers Japanese says so. The style is the polite register; headings and labels are noun phrases; Japanese
//! has no grammatical plural, so a counted message would say the same for every count. The order is the
//! English table's. The words are the glossary's (`docs/glossary.md`, section 2 and section 4); a test holds
//! the two equal.

use super::{Entry, msg};

/// The language's name in itself, for the switcher.
pub const ENDONYM: &str = "日本語";

/// Every message, in Japanese.
pub const CATALOG: &[Entry] = &[
    // layout
    msg("layout.skip", "メインコンテンツへ移動"),
    msg("layout.language", "言語"),
    msg("layout.footer", "{product} バージョン {version}"),
    // the language switcher and its page
    msg("title.language", "言語 · {product}"),
    msg("language.heading", "言語"),
    msg(
        "language.intro",
        "ページを表示する言語を選んでください。選んだ言語は、この端末の Cookie に保存されます。",
    ),
    msg("language.in_use", "現在使用中です。"),
    msg("language.preview_choice", "{name}（プレビュー版）"),
    msg(
        "language.preview_note",
        "この翻訳はプレビュー版です。日本語に堪能な方による確認がまだ終わっていないため、不自然な表現が含まれている場合があります。",
    ),
    msg("language.back", "元のページに戻る"),
    // not found
    msg("title.not_found", "見つかりません · {product}"),
    msg("not_found.heading", "見つかりません"),
    msg(
        "not_found.body",
        "このアドレスには表示できるものがありません。アドレスに入力ミスがないか確認してください。",
    ),
    // a switch that was refused
    msg(
        "title.language_refused",
        "言語は変更されていません · {product}",
    ),
    msg("language_refused.heading", "言語は変更されていません"),
    msg(
        "language_refused.body",
        "フォームで指定された言語は利用できないため、何も変更していません。言語のページで利用できる言語を選んでください。",
    ),
    msg("language_refused.link", "言語のページへ移動"),
    msg(
        "title.form_refused",
        "フォームを受け付けられませんでした · {product}",
    ),
    msg("form_refused.heading", "フォームを受け付けられませんでした"),
    msg(
        "form_refused.body",
        "このサイトのページから送信されたものだと確認できなかったため、何も変更していません。前のページに戻り、再読み込みしてからもう一度お試しください。",
    ),
    // answers that are not pages
    msg(
        "page.render_failed",
        "このページを表示できませんでした。しばらくしてから、もう一度お試しください。それでも解決しない場合は、このサーバーの管理者にお知らせください。",
    ),
    msg(
        "misdirected.body",
        "このサーバーは、そのアドレスのページを提供していません。入力したアドレスを確認するか、このサーバーの管理者にお知らせください。",
    ),
];
