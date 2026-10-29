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
    // wiki shell
    msg("wiki.nav", "このウィキ"),
    msg("wiki.nav_about", "このウィキについて"),
    msg("wiki.nav_verification", "検証"),
    msg("wiki.nav_wikis", "ウィキの一覧"),
    msg("wiki.view_in_use", "ビュー「{view}」を表示しています。"),
    msg("title.page", "{page} · {wiki} · {product}"),
    // a page
    msg(
        "page.redirected_from",
        "「{title}」からリダイレクトされました。",
    ),
    msg(
        "page.alias_from",
        "「{title}」はこのページの別のタイトルです。",
    ),
    msg(
        "page.matched_loosely",
        "「{title}」を探しています。タイトルが最も近いページを表示しています。",
    ),
    msg(
        "page.stale",
        "このページは古い可能性があります。今は最新版を取得できなかったため、サーバーが持っている最後の版を表示しています。",
    ),
    msg("page.contents", "このページの内容"),
    msg("page.license", "ライセンス: {license}"),
    msg("page.propose_disabled", "変更を提案する"),
    msg(
        "page.propose_note",
        "提案の機能は、次のリリースで提供されます。",
    ),
    // a page that is not there, or not one
    msg("missing.heading", "このページはまだありません"),
    msg(
        "missing.body",
        "{wiki} に「{title}」というタイトルのページはありません。",
    ),
    msg(
        "missing.invite",
        "提案の機能が次のリリースで提供されたら、このページを提案できます。",
    ),
    msg("ambiguous.heading", "該当するページが複数あります"),
    msg(
        "ambiguous.body",
        "「{title}」に当たるページが複数あります。ひとつ選んでください。",
    ),
    msg(
        "redirect.heading",
        "このリダイレクトの先にはページがありません",
    ),
    msg(
        "redirect.dangling",
        "ページ「{title}」は「{target}」へリダイレクトしますが、そのタイトルのページはありません。",
    ),
    msg(
        "redirect.loop",
        "ページ「{title}」のリダイレクトは、ぐるぐると回り続けます。",
    ),
    msg(
        "redirect.too_long",
        "ページ「{title}」のリダイレクトは、経由するページが多すぎます。",
    ),
    msg("unreadable.heading", "このページは表示できません"),
    msg(
        "unreadable.not_text",
        "このページのファイルは、このサイトが読み取れるテキストではありません。",
    ),
    msg(
        "unreadable.info",
        "このページの先頭にある情報の部分を読み取れません。",
    ),
    msg(
        "unreadable.info_long",
        "このページの先頭にある情報の部分が長すぎます。",
    ),
    msg(
        "unreadable.advice",
        "このウィキを書いている人が直すことができます。",
    ),
    msg("unavailable.heading", "今はこのページを読み込めません"),
    msg(
        "unavailable.body",
        "しばらくしてから、もう一度お試しください。それでも解決しない場合は、このサーバーの管理者にお知らせください。",
    ),
    msg("no_view.heading", "そのビューはありません"),
    msg(
        "no_view.body",
        "このウィキに「{view}」というビューはありません。",
    ),
    msg(
        "invalid_title.heading",
        "そのタイトルはページの名前にできません",
    ),
    msg(
        "invalid_title.body",
        "タイトルを空にすることはできません。スラッシュで区切った部分を空にすることも、ウィキが保存できる長さを超えることもできません。",
    ),
    // verification
    msg("assurance.verified", "検証済み"),
    msg("assurance.verification_failed", "検証に失敗"),
    msg("assurance.could_not_be_checked_now", "現在は確認できません"),
    msg("assurance.not_checked_yet", "まだ確認していません"),
    msg("badge.sentence", "検証: {state}"),
    msg("title.verification", "検証 · {wiki} · {product}"),
    msg("verification.heading", "検証"),
    msg(
        "explain.verification",
        "検証では、このウィキの履歴を保管しているプログラム prikk に、履歴のすべてと署名を確認してもらいます。「検証済み」は、直前の確認で問題が見つからなかったという意味です。ページの内容が正しいという意味ではありません。",
    ),
    msg("verification.explain_summary", "これは何を意味しますか"),
    msg(
        "verification.state_verified",
        "直前の確認では、問題は見つかりませんでした。",
    ),
    msg(
        "verification.state_failed",
        "直前の確認で、このウィキの履歴に問題が見つかりました。このサーバーの管理者が調べることができます。",
    ),
    msg(
        "verification.state_could_not",
        "直前の確認は最後まで実行できませんでした。履歴に問題があるかどうかについては、何も分かっていません。",
    ),
    msg(
        "verification.state_unknown",
        "このウィキはまだ確認されていません。",
    ),
    msg("verification.checked_at", "最終確認: {when}。"),
    msg("verification.check_now", "今すぐ確認"),
    msg(
        "verification.recent",
        "つい先ほど確認を実行したため、新しい確認は開始しませんでした。少し待ってからもう一度お試しください。",
    ),
    msg("verification.found_heading", "確認の結果"),
    msg(
        "verification.found_intro",
        "確認のうち、次の部分で問題がありました:",
    ),
    msg(
        "verification.now_verified",
        "いま実行した確認では、問題は見つかりませんでした。",
    ),
    msg(
        "verification.not_saved",
        "結果を保存できなかったため、ページに表示されるラベルは変わっていません。",
    ),
    msg(
        "verification.busy",
        "サーバーが他の作業で混み合っているため、確認を実行できませんでした。しばらくしてから、もう一度お試しください。",
    ),
    msg(
        "verification.limit",
        "この確認には、このサーバーに設定されている以上の時間またはメモリが必要です。このサーバーの管理者にお知らせください。",
    ),
    msg(
        "verification.unavailable",
        "確認を開始できませんでした。このサーバーの管理者にお知らせください。",
    ),
    // about, and the list of wikis
    msg("title.about", "{wiki} について · {product}"),
    msg("about.heading", "{wiki} について"),
    msg(
        "about.license",
        "ページに別の記載がない限り、このウィキのページはライセンス {license} で共有されています。",
    ),
    msg(
        "about.view",
        "ビューを指定しない場合に表示されるのは「{view}」です。",
    ),
    msg(
        "about.accessibility_heading",
        "アクセシビリティに関する声明",
    ),
    msg(
        "about.accessibility_tools",
        "このサイトのすべての種類のページについて、自動テストが次の点を確認しています: メインの見出しが一つであること、ランドマーク、本文へ移動するリンク、ラベル、代替テキスト、言語と文字の方向、明るい配色と暗い配色でのコントラスト、キーボードフォーカスの表示、十分な大きさの操作対象。",
    ),
    msg(
        "about.accessibility_people",
        "支援技術を使っている方による、このサイトの確認はまだ行われていません。うまく使えないところがあれば、このサーバーの管理者にお知らせください。",
    ),
    msg("title.wikis", "ウィキ · {product}"),
    msg("wikis.heading", "ウィキ"),
    msg("wikis.intro", "ここで読めるウィキの一覧です。"),
    msg("wikis.none", "ここで読めるウィキはまだありません。"),
    // transclusion (a page that includes another)
    msg("transclusion.from", "{title} から"),
    msg(
        "transclusion.cycle",
        "含めません。自分自身を含むことになります: {chain}",
    ),
    msg(
        "transclusion.too_deep",
        "含めません。{limit} 段より深くなります: {title}",
    ),
    msg(
        "transclusion.over_budget",
        "含めません。1 ページに含められる量には上限があります: {title}",
    ),
    // a request this address does not take
    msg("title.method_not_allowed", "ここではできません · {product}"),
    msg(
        "method_not_allowed.heading",
        "このアドレスではその操作はできません",
    ),
    msg(
        "method_not_allowed.body",
        "このアドレスは開くことはできますが、その使い方はできません。前のページに戻って、ページ内のリンクをお使いください。",
    ),
    // the read API's problem details (handoff 003 T8)
    msg("problem.not_found.title", "見つかりません"),
    msg(
        "problem.not_found.detail",
        "このアドレスには表示できるものがありません。",
    ),
    msg(
        "problem.not_found.hint",
        "アドレスに入力ミスがないか確認してください。",
    ),
    msg("problem.page_not_found.title", "ページが見つかりません"),
    msg(
        "problem.page_not_found.detail",
        "このウィキには、このタイトルのページがありません。",
    ),
    msg(
        "problem.page_not_found.hint",
        "タイトルを確認するか、ウィキのページの一覧から探してください。",
    ),
    msg("problem.ambiguous_title.title", "複数のページが該当します"),
    msg(
        "problem.ambiguous_title.detail",
        "このタイトルに該当するページが複数あり、どれが最も近いか決められません。",
    ),
    msg(
        "problem.ambiguous_title.hint",
        "candidates に挙がっているページのうち、ひとつを正確なタイトルで指定してください。",
    ),
    msg(
        "problem.redirect_broken.title",
        "リダイレクトをたどれません",
    ),
    msg(
        "problem.redirect_broken.detail",
        "このタイトルは、ページにつながっていないリダイレクトです。",
    ),
    msg(
        "problem.redirect_broken.hint",
        "ウィキを管理する方にリダイレクトの修正を依頼するか、行き先のはずのページを直接指定してください。",
    ),
    msg("problem.page_unreadable.title", "ページを表示できません"),
    msg(
        "problem.page_unreadable.detail",
        "ページはありますが、保存されている内容をページとして読み取れません。",
    ),
    msg(
        "problem.page_unreadable.hint",
        "ページを書いている方に修正を依頼してください。",
    ),
    msg(
        "problem.invalid_title.title",
        "タイトルを受け付けられません",
    ),
    msg(
        "problem.invalid_title.detail",
        "このタイトルでは、ページを指定できません。",
    ),
    msg(
        "problem.invalid_title.hint",
        "空でなく、長すぎず、スラッシュの間に空の部分がないタイトルを使ってください。",
    ),
    msg("problem.no_such_view.title", "ビューが見つかりません"),
    msg(
        "problem.no_such_view.detail",
        "このウィキには、{view} という名前の開いているビューがありません。",
    ),
    msg(
        "problem.no_such_view.hint",
        "ビューを指定しなければ、ウィキの既定のビューが使われます。",
    ),
    msg(
        "problem.invalid_parameter.title",
        "パラメーターを受け付けられません",
    ),
    msg(
        "problem.invalid_parameter.detail",
        "パラメーター {parameter} は、指定された形では受け付けられません。",
    ),
    msg(
        "problem.invalid_parameter.hint",
        "{parameter} に指定できるものは、API の説明で確認してください。",
    ),
    msg("problem.diff_path_required.title", "差分にはパスが必要です"),
    msg(
        "problem.diff_path_required.detail",
        "差分は常にひとつのページについて求めるため、パラメーター path は必須です。",
    ),
    msg(
        "problem.diff_path_required.hint",
        "raw のエンドポイントが返す、ページの保存先のパスを指定してください。",
    ),
    msg(
        "problem.invalid_cursor.title",
        "カーソルを受け付けられません",
    ),
    msg(
        "problem.invalid_cursor.detail",
        "このサーバーが発行したものではないカーソルです。",
    ),
    msg(
        "problem.invalid_cursor.hint",
        "カーソルを付けずに、最初のページからやり直してください。",
    ),
    msg(
        "problem.not_available.title",
        "このビルドでは利用できません",
    ),
    msg(
        "problem.not_available.detail",
        "このサーバーのビルドでは、まだ提供されていません。",
    ),
    msg(
        "problem.not_available.hint",
        "このサーバーが提供するものは、API の説明で確認してください。",
    ),
    msg("problem.unavailable.title", "現在は読み取れません"),
    msg(
        "problem.unavailable.detail",
        "サーバーが今、ウィキを読み取れませんでした。",
    ),
    msg(
        "problem.unavailable.hint",
        "しばらくしてから、もう一度お試しください。",
    ),
    msg("problem.failed.title", "リクエストに失敗しました"),
    msg(
        "problem.failed.detail",
        "サーバー側で問題が起きたため、リクエストを処理できませんでした。",
    ),
    msg(
        "problem.failed.hint",
        "しばらくしてからもう一度お試しになり、続く場合はこのサーバーの管理者にお知らせください。",
    ),
    msg(
        "problem.method_not_allowed.title",
        "このメソッドは使えません",
    ),
    msg(
        "problem.method_not_allowed.detail",
        "この版の API は読み取り専用のため、このアドレスではそのメソッドを使えません。",
    ),
    msg(
        "problem.method_not_allowed.hint",
        "GET または HEAD で求めてください。",
    ),
];
