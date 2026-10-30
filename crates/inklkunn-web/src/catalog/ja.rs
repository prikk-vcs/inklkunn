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
    msg(
        "verification.attempt_at",
        "その後の確認は最後まで実行できませんでした（{when}）。履歴に問題があるかどうかについては何も分かっておらず、上の結果は変わりません。",
    ),
    msg(
        "verification.running",
        "このウィキの確認はすでに実行中です。終わるまでお待ちになってから、このページをもう一度ご覧ください。",
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
    // history, change, diff and permalinks (handoff 003 T5)
    msg("page.link_history", "このページの履歴"),
    msg("title.history", "{page} の履歴 · {wiki} · {product}"),
    msg("history.heading", "{title} の履歴"),
    msg(
        "history.intro",
        "このページに関わった変更です。新しい順に並んでいます。",
    ),
    msg("history.current", "{title} の現在の版"),
    msg(
        "history.none",
        "このページに関わる変更は見つかりませんでした。",
    ),
    msg(
        "history.none_so_far",
        "ここまでに読み込んだ変更の中に、このページに関わるものは見つかりませんでした。さらに古い変更に含まれている可能性があります。",
    ),
    msg("history.older", "さらに古い変更を見る"),
    msg(
        "history.beyond",
        "このウィキには一度に読み込める数より多くの変更があります。いちばん古い変更はここには表示されません。",
    ),
    msg("history.change_label", "変更 {number}"),
    msg(
        "history.link_diff",
        "変更 {number} でこのページがどう変わったか",
    ),
    msg("history.link_version", "変更 {number} の後のページ"),
    msg("people.proposed_by", "提案者: {name}"),
    msg("people.accepted_by", "受け入れた人: {name}"),
    msg("people.made_by", "{name} による変更"),
    msg("people.service", "ウィキサービス"),
    msg("people.anonymous", "匿名の投稿者（{token}）"),
    msg(
        "people.unrecorded",
        "この変更の記録には、誰が行ったかが書かれていません。",
    ),
    msg("flag.undo", "以前の変更を取り消します"),
    msg("flag.import", "取り込み"),
    msg("flag.service", "ウィキサービス自身が行った変更"),
    msg("flag.first", "このウィキの最初の変更"),
    msg("event.created", "ページが作成されました"),
    msg("event.edited", "ページが編集されました"),
    msg("event.deleted", "ページが削除されました"),
    msg("event.moved_from", "ページは {title} から移動されました"),
    msg("event.moved_to", "ページは {title} へ移動されました"),
    msg(
        "event.not_inspected",
        "この変更は大きすぎて内容を一覧できないため、このページに関わっている可能性があります。",
    ),
    msg("title.change", "変更 {number} · {wiki} · {product}"),
    msg("change.heading", "変更 {number}"),
    msg("change.signed", "署名して履歴に記録済み"),
    msg(
        "change.signed_by",
        "ウィキサービスが {proposer} に代わって署名し、{acceptor} が受け入れました。",
    ),
    msg("change.signed_service", "ウィキサービスが署名しました。"),
    msg("change.did_heading", "この変更の内容"),
    msg("change.did_none", "この変更に一覧できる内容はありません。"),
    msg("change.did_added", "ページ {title} を追加しました"),
    msg("change.did_edited", "ページ {title} を編集しました"),
    msg("change.did_removed", "ページ {title} を削除しました"),
    msg("change.did_moved", "ページ {from} を {to} へ移動しました"),
    msg(
        "change.did_file_added",
        "ページではない添付ファイルなどを追加しました",
    ),
    msg(
        "change.did_file_edited",
        "ページではない添付ファイルなどを編集しました",
    ),
    msg(
        "change.did_file_removed",
        "ページではない添付ファイルなどを削除しました",
    ),
    msg(
        "change.did_file_moved",
        "ページではない添付ファイルなどを移動しました",
    ),
    msg(
        "change.did_more",
        "ほかに {count} 件は一覧に含まれていません。",
    ),
    msg(
        "change.too_large",
        "この変更は大きすぎて、内容を一覧できません。",
    ),
    msg("change.link_diff", "この変更で {title} がどう変わったか"),
    msg("change_none.heading", "そのような変更はありません"),
    msg(
        "change_none.body",
        "このアドレスに対応する変更は、このウィキにはありません。アドレスを確認するか、ページの履歴を開いてリンクをたどってください。",
    ),
    msg("title.diff", "{page} の変更点 · {wiki} · {product}"),
    msg("diff.heading", "{title} の変更点"),
    msg("diff.from_to", "{from} から {to} まで。"),
    msg("diff.summary_heading", "概要"),
    msg("diff.summary_added", "{count} 行追加"),
    msg("diff.summary_removed", "{count} 行削除"),
    msg("diff.summary_none", "異なる行はありません。"),
    msg(
        "diff.status_added",
        "このページは、比較の前の時点には存在せず、後の時点に存在します。",
    ),
    msg(
        "diff.status_removed",
        "このページは、比較の前の時点に存在し、後の時点には存在しません。",
    ),
    msg(
        "diff.status_unchanged",
        "このページは、どちらの時点でも同じです。",
    ),
    msg(
        "diff.no_lines",
        "このページの行は表示できません。テキストではないか、比較できなかった可能性があります。",
    ),
    msg("diff.truncated", "最初の {limit} 行だけを表示しています。"),
    msg(
        "diff.refinement",
        "印の付いた語は、inklkunn が変更された行を語ごとに比べて見つけたものです。行そのものは prikk が出力したものです。",
    ),
    msg("diff.hunk_near", "{line} 行目付近の変更"),
    msg("diff.hunk_plain", "変更"),
    msg(
        "diff.hunk_raw",
        "この部分は変更の一覧として読み取れませんでした。そのまま表示しています。",
    ),
    msg("diff.added_label", "追加:"),
    msg("diff.removed_label", "削除:"),
    msg(
        "diff.no_newline",
        "ここでテキストは改行なしで終わっています。",
    ),
    msg(
        "title.at",
        "変更 {number} の後の {page} · {wiki} · {product}",
    ),
    msg("at.banner", "これは {change} の後のページです。"),
    msg("at.current", "{current} へ移動できます。"),
    msg("at.current_label", "このページの現在の版"),
    msg(
        "at.links",
        "このページのリンクは、そのときのページではなく、現在のページに移動します。",
    ),
    msg("at.missing_heading", "そのときこのページはありませんでした"),
    msg(
        "at.missing_body",
        "{change} の後、このウィキに {title} というタイトルのページはありませんでした。",
    ),
    // handoff 003 T6: changes, feeds, search, health, backlinks, the head of a page
    msg("wiki.nav_changes", "最近の変更"),
    msg("wiki.nav_search", "検索"),
    msg("wiki.nav_health", "ウィキの健全性"),
    msg("title.changes", "最近の変更 · {wiki} · {product}"),
    msg("changes.heading", "最近の変更"),
    msg(
        "changes.intro",
        "{wiki} の履歴に記録された変更を、新しい順に表示しています。",
    ),
    msg(
        "changes.seen_note",
        "履歴には変更を行った日時が記録されないため、各変更にはこのサーバーが最初に確認した日時を表示しています。",
    ),
    msg("changes.filter_legend", "表示する変更を選ぶ"),
    msg(
        "changes.filter_namespace",
        "このタイトルの下にあるページのみ",
    ),
    msg(
        "changes.filter_namespace_hint",
        "たとえば Guides と入力すると、Guides/Setup への変更が表示されます。空欄にするとすべてのページが対象になります。",
    ),
    msg("changes.filter_view", "ビュー"),
    msg("changes.all_views", "すべてのビュー"),
    msg(
        "changes.filter_automated",
        "ウィキサービス自身または機械の補助による変更",
    ),
    msg("changes.opt_show", "表示する"),
    msg("changes.opt_hide", "表示しない"),
    msg("changes.opt_only", "それらのみ表示する"),
    msg("changes.filter_apply", "この条件で表示"),
    msg("changes.active_heading", "一覧は絞り込まれています"),
    msg(
        "changes.active_namespace",
        "{namespace} の下にあるページのみ",
    ),
    msg("changes.active_page", "ページ {page} への変更のみ"),
    msg("changes.active_view", "ビュー {view} のみ"),
    msg(
        "changes.active_hide",
        "ウィキサービス自身または機械の補助による変更は表示していません",
    ),
    msg(
        "changes.active_only",
        "ウィキサービス自身または機械の補助による変更のみ",
    ),
    msg("changes.clear", "すべての変更を表示"),
    msg("changes.caption", "変更の一覧（新しい順）"),
    msg("changes.col_change", "変更"),
    msg("changes.col_people", "関わった人"),
    msg("changes.col_pages", "ページ"),
    msg("changes.col_seen", "初めて確認した日時"),
    msg("changes.proposed_by", "提案者: {person}"),
    msg("changes.accepted_by", "受け入れた人: {person}"),
    msg("changes.made_by", "変更した人: {person}"),
    msg("changes.person_anonymous", "匿名の訪問者（{token}）"),
    msg("changes.person_service", "ウィキサービス（{label}）"),
    msg(
        "changes.foreign",
        "このサービスの外で記録されたため、関わった人は不明です。",
    ),
    msg("changes.rollback", "以前の変更を取り消す変更"),
    msg("changes.no_pages", "ページなし"),
    msg(
        "changes.pages_unavailable",
        "この変更のページを今は取得できませんでした。",
    ),
    msg("changes.page_gone", "{title}（現在は存在しません）"),
    msg("changes.none", "表示できる変更はありません。"),
    msg(
        "changes.more",
        "ここに表示されたより古い変更があります。確認したのは新しい変更のみです。",
    ),
    msg(
        "changes.pages_missing",
        "一部の変更はページとの対応を今は確認できなかったため、この一覧には載っていない変更がある可能性があります。",
    ),
    msg(
        "changes.hidden",
        "閲覧できないページへの変更は表示していません。",
    ),
    msg(
        "changes.feed",
        "これらの変更をフィードリーダーで購読する（Atom）",
    ),
    msg("feed.title_wiki", "最近の変更 · {wiki}"),
    msg(
        "feed.title_namespace",
        "{namespace} の下の最近の変更 · {wiki}",
    ),
    msg("feed.title_page", "{page} への変更 · {wiki}"),
    msg("feed.pages", "ページ: {pages}"),
    msg("title.search", "検索 · {wiki} · {product}"),
    msg("search.heading", "検索"),
    msg("search.label", "ページのタイトルと別名を検索"),
    msg("search.button", "検索する"),
    msg(
        "search.scope",
        "検索の対象はページのタイトルと別名です。ページ本文の検索は、今後のリリースで提供されます。",
    ),
    msg(
        "search.empty",
        "検索するタイトル、またはその一部を入力してください。",
    ),
    msg("search.results_heading", "「{query}」で見つかったページ"),
    msg("search.matched_title", "タイトル:"),
    msg("search.matched_alias", "別名:"),
    msg(
        "search.none",
        "タイトルにも別名にも「{query}」を含むページはありません。",
    ),
    msg(
        "search.cut",
        "一致度の高いものだけを表示しています。タイトルをもう少し入力すると絞り込めます。",
    ),
    msg("search.exact", "このタイトルのページはすでにあります。"),
    msg("search.similar_heading", "タイトルが似ているページ"),
    msg("search.create_heading", "このページを作成する"),
    msg("search.create", "「{title}」を作成する"),
    msg(
        "opensearch.description",
        "{wiki} のページをタイトルと別名で検索",
    ),
    msg("title.health", "ウィキの健全性 · {wiki} · {product}"),
    msg("health.heading", "{wiki} の健全性"),
    msg(
        "health.intro",
        "これらの一覧は、ウィキを改善できる箇所を示します。各行には、解決するための対応が書かれています。",
    ),
    msg("health.wanted_name", "未作成のページ"),
    msg(
        "health.wanted_desc",
        "ページからリンクされているのに、まだ存在しないタイトル。",
    ),
    msg("health.orphans_name", "孤立したページ"),
    msg(
        "health.orphans_desc",
        "ほかのどのページからもリンクされていないページ。",
    ),
    msg("health.dead_ends_name", "行き止まりのページ"),
    msg(
        "health.dead_ends_desc",
        "ほかのどのページにもリンクしていないページ。",
    ),
    msg("health.redirects_name", "リダイレクトの問題"),
    msg(
        "health.redirects_desc",
        "行き先がない、循環している、長すぎる、または別のリダイレクトに続くリダイレクト。",
    ),
    msg("health.help_name", "手助けが必要なページ"),
    msg(
        "health.help_desc",
        "作業中、下書き、またはスタブと示されているページ。",
    ),
    msg("health.stale_name", "長く変更されていないページ"),
    msg(
        "health.stale_desc",
        "最近の変更の中で、最後に変更された時期がいちばん古いページ。",
    ),
    msg("health.back", "すべての一覧"),
    msg("health.col_page", "ページ"),
    msg("health.col_wanted", "未作成のタイトル"),
    msg("health.col_linked", "リンクしているページ数"),
    msg("health.col_action", "解決するための対応"),
    msg("health.col_target", "リダイレクト先"),
    msg("health.col_problem", "問題"),
    msg("health.col_mark", "表示されている状態"),
    msg("health.col_changed", "最後の変更"),
    msg("health.see_links", "{title} へのリンク元のページ"),
    msg(
        "health.wanted_action",
        "このタイトルのページを追加するか、リンクを修正します。",
    ),
    msg(
        "health.orphans_action",
        "関連するページからこのページへリンクして、読者が見つけられるようにします。",
    ),
    msg(
        "health.dead_ends_action",
        "このページから関連するページへのリンクを追加します。",
    ),
    msg(
        "health.problem_broken",
        "リダイレクト先がページではありません。",
    ),
    msg("health.problem_loop", "リダイレクトが循環しています。"),
    msg("health.problem_long", "リダイレクトの連鎖が長すぎます。"),
    msg(
        "health.problem_double",
        "リダイレクト先がさらに別のリダイレクトです。",
    ),
    msg(
        "health.action_broken",
        "存在するページを指すように直すか、リダイレクトを削除します。",
    ),
    msg(
        "health.action_loop",
        "どれか一つのリダイレクトを、実在するページに向けます。",
    ),
    msg(
        "health.action_long",
        "最初のリダイレクトを最終的なページに直接向けます。",
    ),
    msg(
        "health.action_double",
        "このリダイレクトを最終的なページに直接向けます。",
    ),
    msg("health.mark_wip", "作業中"),
    msg("health.mark_draft", "下書き"),
    msg("health.mark_stub", "スタブ"),
    msg(
        "health.help_action",
        "ページを仕上げるか改善してから、その表示を外します。",
    ),
    msg(
        "health.stale_never",
        "確認した最近の変更の中では変更されていません",
    ),
    msg("health.stale_seen", "{when} に初めて確認された変更"),
    msg(
        "health.stale_action",
        "ページを読み、古くなっていれば更新します。",
    ),
    msg(
        "health.stale_window",
        "履歴にはページが最後に変更された時期が記録されないため、この一覧では新しい {limit} 件の変更だけを確認しています。",
    ),
    msg(
        "health.none_wanted",
        "未作成のページはありません。すべてのリンクがページにつながっています。",
    ),
    msg(
        "health.none_orphans",
        "すべてのページが、ほかのページからリンクされています。",
    ),
    msg(
        "health.none_dead_ends",
        "すべてのページが、ほかのページにリンクしています。",
    ),
    msg(
        "health.none_redirects",
        "すべてのリダイレクトがページにつながっています。",
    ),
    msg(
        "health.none_help",
        "手助けが必要と示されているページはありません。",
    ),
    msg("health.none_stale", "表示できるページはありません。"),
    msg("title.links", "{page} へのリンク · {wiki} · {product}"),
    msg("links.heading", "{page} へのリンク元のページ"),
    msg(
        "links.missing",
        "このタイトルのページはまだありません。次のページがリンクしています。",
    ),
    msg("links.go_to", "{page} を開く"),
    msg(
        "links.none",
        "このページへリンクしているページはありません。",
    ),
    msg("links.kind_include", "このページを埋め込んでいます"),
    msg("links.kind_redirect", "ここへリダイレクトしています"),
    msg("links.cut", "最初の {limit} 件のみ表示しています。"),
    msg("head.feed_wiki", "{wiki} の最近の変更"),
    msg("head.feed_page", "{page} への変更"),
    msg("head.markdown", "{page}（Markdown）"),
    msg("head.search", "{wiki} を検索"),
    // the API's problem for a change that is not in the history (handoff 003 T8, completion)
    msg("problem.change_not_found.title", "変更が見つかりません"),
    msg(
        "problem.change_not_found.detail",
        "このウィキのこのビューには、この ID の変更がありません。",
    ),
    msg(
        "problem.change_not_found.hint",
        "変更の ID は、ページの履歴または最近の変更から取ってください。",
    ),
    // the inbox, following a page, and the flags of changes (handoff 004 T6)
    msg("title.inbox", "受信箱 · {product}"),
    msg("inbox.heading", "受信箱"),
    msg(
        "inbox.intro",
        "あなたの提案と、フォローしているページに起きたことを、新しい順に表示します。",
    ),
    msg("inbox.none", "あなたに関わる出来事はまだありません。"),
    msg("inbox.new", "新着"),
    msg("inbox.mark_all_read", "すべて既読にする"),
    msg("inbox.someone", "だれか"),
    msg(
        "inbox.event_reviewed",
        "{person} さんが {wiki} のあなたの提案 {proposal} をレビューしました。",
    ),
    msg(
        "inbox.event_accepted",
        "{wiki} のあなたの提案 {proposal} は受け入れられ、署名して履歴に記録されました。",
    ),
    msg(
        "inbox.event_declined",
        "{person} さんが {wiki} のあなたの提案 {proposal} を見送りました。",
    ),
    msg(
        "inbox.event_needs_revision",
        "{person} さんが {wiki} のあなたの提案 {proposal} の修正を依頼しました。",
    ),
    msg(
        "inbox.event_withdrawn",
        "{person} さんが {wiki} の提案 {proposal} を取り下げました。",
    ),
    msg(
        "inbox.event_superseded",
        "{person} さんが {wiki} のあなたの提案 {proposal} を、あとの提案に置き換えました。",
    ),
    msg(
        "inbox.event_replied",
        "{person} さんが {wiki} の {proposal} の議論にコメントしました。",
    ),
    msg(
        "inbox.event_mentioned",
        "{person} さんが {wiki} の {proposal} の議論であなたに言及しました。",
    ),
    msg(
        "inbox.event_watched",
        "{wiki} でフォローしているページが変更されました。変更のもとになった提案は {proposal} です。",
    ),
    msg(
        "inbox.event_unreadable",
        "あなたに関わる出来事がありましたが、そのウィキはもう読むことができません。",
    ),
    msg("inbox.account_heading", "アカウントが必要です"),
    msg(
        "inbox.account_body",
        "受信箱はアカウントごとのものです。サインインすると、あなたの提案やフォローしているページに起きたことを確認できます。",
    ),
    msg("inbox.following_heading", "フォローしているページ"),
    msg(
        "inbox.following_none",
        "フォローしているページはまだありません。",
    ),
    msg("inbox.following_page", "{wiki} の {page}"),
    msg(
        "inbox.following_namespace",
        "{wiki} の {page} の下にあるページ",
    ),
    msg("inbox.unfollow", "{what} のフォローをやめる"),
    msg("watch.heading_followed", "フォロー中"),
    msg("watch.heading_stopped", "フォローをやめました"),
    msg("watch.heading_full", "フォローできる数の上限です"),
    msg(
        "watch.page_followed",
        "ページ「{title}」をフォローしました。このページへの変更が署名して履歴に記録されると、受信箱でお知らせします。",
    ),
    msg(
        "watch.namespace_followed",
        "「{title}」の下にあるページをフォローしました。それらへの変更が署名して履歴に記録されると、受信箱でお知らせします。",
    ),
    msg(
        "watch.page_stopped",
        "ページ「{title}」のフォローをやめました。",
    ),
    msg(
        "watch.namespace_stopped",
        "「{title}」の下にあるページのフォローをやめました。",
    ),
    msg(
        "watch.account_body",
        "ページのフォローはアカウントごとの機能です。サインインしてから、もう一度フォローしてください。",
    ),
    msg(
        "watch.full_body",
        "フォローできるページの数の上限に達しています。どれかのフォローをやめてから、もう一度お試しください。",
    ),
    msg("watch.back", "{title} に戻る"),
    msg("watch.to_inbox", "受信箱を開く"),
    msg("changes.filter_minor", "軽微な変更"),
    msg("changes.active_minor_hide", "軽微な変更は表示していません"),
    msg("changes.active_minor_only", "軽微な変更のみ"),
    msg("changes.minor_label", "軽微な変更"),
    msg(
        "changes.machine_assisted",
        "機械の補助によるコンテンツ（受け入れた人: {person}）",
    ),
    msg(
        "changes.machine_assisted_only",
        "機械の補助によるコンテンツ",
    ),
];
