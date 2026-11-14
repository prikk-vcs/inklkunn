//! The Japanese messages: **a preview** (DR-7). The draft is the dev team's, written from the English and
//! the glossary's Japanese terms; nothing here has been read by a fluent reviewer yet, and every page that
//! offers Japanese says so. The style is the polite register; headings and labels are noun phrases; Japanese
//! has no grammatical plural, so a counted message would say the same for every count. The order is the
//! English table's. The words are the glossary's (`docs/glossary.md`, section 2 and section 4); a test holds
//! the two equal.

use super::{Entry, msg, plural_msg};

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
    // a page that is not there, or not one
    msg("missing.heading", "このページはまだありません"),
    msg(
        "missing.body",
        "{wiki} に「{title}」というタイトルのページはありません。",
    ),
    msg(
        "missing.invite",
        "このページを提案できます。提案にはアカウントが必要で、権限のある人が提案を履歴に取り込むと、ページができます。",
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
    msg(
        "verification.explained_by_retirement",
        "確認に失敗した箇所は、鍵 {key} の失効時に記録されたものだけです。その鍵はこのウィキの信頼から外されたため、その鍵が署名した部分は確認できなくなっています。ほかに失敗はありません。",
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
        "このサーバーのビルドでは、提供されていません。",
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
        "この API のこのアドレスでは、そのメソッドを使えません。",
    ),
    msg(
        "problem.method_not_allowed.hint",
        "各アドレスで使えるメソッドは、このサーバーの API の説明に載っています。",
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
    msg("health.propose_page", "このページを提案する（{title}）"),
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
    msg("health.knowledge_name", "読み取れない知識ファイル"),
    msg(
        "health.knowledge_desc",
        "ウィキが読める形になっていない、主張と出典のファイル。これらからは何も読み取られず、ページには影響しません。",
    ),
    msg("health.col_file", "ファイル"),
    msg("health.col_rule", "問題の内容"),
    msg(
        "health.knowledge_action",
        "修正したファイルを提案するか、ファイルの削除を提案してください。",
    ),
    msg(
        "health.none_knowledge",
        "読み取れない知識ファイルはありません。",
    ),
    msg(
        "health.rule_path",
        "ファイルが knowledge/claims にも knowledge/sources にもないか、ファイル名が「正しい形式の id + .yaml」になっていません。",
    ),
    msg(
        "health.rule_not_text",
        "ファイルが UTF-8 のテキストではありません。",
    ),
    msg(
        "health.rule_too_long",
        "ファイルが、知識ファイルとして許される長さを超えています。",
    ),
    msg(
        "health.rule_syntax",
        "テキストが、ウィキが読む単純な YAML の範囲に収まっていません。",
    ),
    msg("health.rule_missing", "形式で必須のキーがありません。"),
    msg("health.rule_unknown", "形式にないキーがあります。"),
    msg(
        "health.rule_type",
        "値の形が、そのキーに必要な形ではありません。",
    ),
    msg(
        "health.rule_number",
        "形式に数値はありません。日付や年は文字列で書いてください。",
    ),
    msg(
        "health.rule_value",
        "値が、そのキーで許される語や形式の外にあります。",
    ),
    msg("health.rule_id", "識別子の形式が正しくありません。"),
    msg("health.rule_duplicate", "同じ項目が 2 回現れています。"),
    msg(
        "health.rule_bounds",
        "リストまたはテキストが、形式で許される長さを超えています。",
    ),
    msg(
        "health.rule_reference",
        "ウィキにない出典または主張が指定されています。",
    ),
    msg(
        "health.rule_anchor",
        "表現が、ウィキにないページまたはページ内の段落を指しています。",
    ),
    msg(
        "health.rule_in_use",
        "別のファイルから指定されているファイルが削除されています。",
    ),
    msg(
        "health.rule_owned",
        "サービスだけが書くページのキーも変更されています。",
    ),
    msg(
        "health.rule_status",
        "コミュニティの状態、確認日、入手できるかどうかの設定、主張がすでに持つ出典リンクの変更・削除、確認済みの出典の題名・著者・日付・参照先・置き換え先の変更、これらを持つ主張や出典の削除、読み取れないファイルの置き換え・削除が、キュレーション提案ではない変更で行われています。",
    ),
    msg(
        "health.rule_judged",
        "変更が設定した判断（出典リンクまたはコミュニティでの扱い）が、その主張のどの段落にも結果として存在しない文言で判断されています。",
    ),
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
    // the write API's problems (handoff 004 T7)
    msg(
        "problem.authentication_required.title",
        "サインインが必要です",
    ),
    msg(
        "problem.authentication_required.detail",
        "このリクエストには認証情報がなく、書き込みにはアカウントが必要です。",
    ),
    msg(
        "problem.authentication_required.hint",
        "Authorization ヘッダーに、Bearer に続けて API トークンを付けて送ってください。",
    ),
    msg(
        "problem.invalid_token.title",
        "トークンを受け付けられません",
    ),
    msg(
        "problem.invalid_token.detail",
        "Authorization ヘッダーのトークンは、このサーバーが受け付けるものではありません。未知、期限切れ、失効済みのいずれか、またはアカウントが無効になっている可能性があります。",
    ),
    msg(
        "problem.invalid_token.hint",
        "新しいトークンを発行して、そちらを送ってください。",
    ),
    msg("problem.forbidden.title", "許可されていません"),
    msg(
        "problem.forbidden.detail",
        "この認証情報ではこの操作はできません。このウィキでのアカウントの役割か、トークンの発行時のスコープが許していないか、セッション Cookie を伴うリクエストが別のサイトから来ています。",
    ),
    msg(
        "problem.forbidden.hint",
        "ウィキのメンテナーに役割を依頼するか、より広いスコープで発行したトークンを使ってください。",
    ),
    msg("problem.base_required.title", "基準が必要です"),
    msg(
        "problem.base_required.detail",
        "書き込みは、書いた時点の状態を示す必要がありますが、このリクエストには If-Match ヘッダーがありません。",
    ),
    msg(
        "problem.base_required.hint",
        "If-Match に提案のリビジョンの ETag を付けて送ってください。新しい提案では、ビューの先頭の状態 ID を送ってください。",
    ),
    msg("problem.stale_base.title", "基準が古くなっています"),
    msg(
        "problem.stale_base.detail",
        "この書き込みの基準は、もう現在の状態ではありません。先に別の操作がありました。何も記録されていません。",
    ),
    msg(
        "problem.stale_base.hint",
        "現在の状態を読み、変更を照らし合わせてから、If-Match に現在の値を付けて送り直してください。この問題の応答にその値が含まれています。",
    ),
    msg("problem.invalid_body.title", "本文を受け付けられません"),
    msg(
        "problem.invalid_body.detail",
        "本文が、このエンドポイントの形の JSON オブジェクトではありません。メンバー {member} が、ない、重複している、未知である、または許されない値です。",
    ),
    msg(
        "problem.invalid_body.hint",
        "このエンドポイントの説明にあるメンバーだけを送ってください。",
    ),
    msg("problem.body_too_large.title", "本文が大きすぎます"),
    msg(
        "problem.body_too_large.detail",
        "本文が、このエンドポイントの受け付ける大きさ（最大 {limit} バイト）を超えています。内容は読み込まれていません。",
    ),
    msg(
        "problem.body_too_large.hint",
        "送る量を減らしてください。変更を分けるか、テキストを短くします。",
    ),
    msg(
        "problem.unsupported_media_type.title",
        "JSON として送られていません",
    ),
    msg(
        "problem.unsupported_media_type.detail",
        "本文は JSON で、コンテンツタイプ application/json で送る必要があります。",
    ),
    msg(
        "problem.unsupported_media_type.hint",
        "Content-Type ヘッダーを application/json にしてください。",
    ),
    msg(
        "problem.unacceptable.title",
        "このままでは受け付けられません",
    ),
    msg("problem.unacceptable.detail", "{explanation}"),
    msg(
        "problem.unacceptable.hint",
        "reason が示す点を直して、もう一度送ってください。何も記録されていません。",
    ),
    msg("problem.not_in_this_state.title", "今はできません"),
    msg(
        "problem.not_in_this_state.detail",
        "提案が {state} の間は、これはできません。",
    ),
    msg(
        "problem.not_in_this_state.hint",
        "提案を読んでください。できることは提案の状態によって決まります。",
    ),
    msg("problem.already_decided.title", "すでに判断しています"),
    msg(
        "problem.already_decided.detail",
        "このリビジョンにはすでに判断を出しており、判断は変更できません。",
    ),
    msg(
        "problem.already_decided.hint",
        "次のリビジョンができたら、そのときにあらためて判断してください。",
    ),
    msg("problem.own_proposal.title", "自分の提案です"),
    msg(
        "problem.own_proposal.detail",
        "提案者は、自分の提案を承認したり、修正を求めたりすることはできません。",
    ),
    msg(
        "problem.own_proposal.hint",
        "ほかのレビュー担当者に判断を依頼してください。自分の提案にコメントすることはできます。",
    ),
    msg("problem.refused_limit.title", "拒否: 上限に達しました"),
    msg(
        "problem.refused_limit.detail",
        "上限に達しました: {limit}。allowed、current、asked の各メンバーが数を示し、reset がまた可能になる時刻を示します。",
    ),
    msg(
        "problem.refused_limit.hint",
        "待てば解消する場合は Retry-After が待つ秒数を示します。そうでない場合は、ご自身の提案を閉じるか、求める量を減らしてください。",
    ),
    msg("problem.busy.title", "ウィキは混み合っています"),
    msg(
        "problem.busy.detail",
        "このウィキでは別の操作が実行中または待機中で、この操作の前に {waiting} 件あります。何も行われていません。",
    ),
    msg(
        "problem.busy.hint",
        "Retry-After が示す時間のあとで、もう一度送ってください。",
    ),
    msg("problem.needs_recovery.title", "ウィキの復旧が必要です"),
    msg(
        "problem.needs_recovery.detail",
        "このウィキへの書き込みが完了しませんでした。復旧が確認するまで、ウィキは新しい変更を受け付けませんが、読み取りはできます。",
    ),
    msg(
        "problem.needs_recovery.hint",
        "このサーバーを運用している人に知らせてください。このリクエストでは何も行われていません。",
    ),
    msg("problem.overlap.title", "変更が重なっています"),
    msg(
        "problem.overlap.detail",
        "停止しました。提案とウィキが同じ箇所を変更しており、{overlaps} 件あります。提案は改訂が必要な状態になり、ウィキは変わっていません。",
    ),
    msg(
        "problem.overlap.hint",
        "提案に記録された重なりを読み、両方のテキストを踏まえた改訂を書いて送ってください。",
    ),
    msg(
        "problem.nothing_to_change.title",
        "変更するものがありません",
    ),
    msg(
        "problem.nothing_to_change.detail",
        "このリビジョンが求める内容は、すでにウィキにあります。",
    ),
    msg(
        "problem.nothing_to_change.hint",
        "提案を却下または取り下げるか、改訂してください。",
    ),
    msg(
        "problem.acceptance_in_progress.title",
        "受け入れがすでに進行中です",
    ),
    msg(
        "problem.acceptance_in_progress.detail",
        "このリビジョンの先の受け入れが、まだ署名を終えていません。",
    ),
    msg(
        "problem.acceptance_in_progress.hint",
        "しばらく待ってから、もう一度試してください。先の受け入れが中断されていた場合は、復旧が解決します。",
    ),
    msg("problem.acceptance_failed.title", "受け入れに失敗しました"),
    msg(
        "problem.acceptance_failed.detail",
        "変更は行われませんでした: {word}。提案は開いたままで、履歴は変わっていません。",
    ),
    msg(
        "problem.acceptance_failed.hint",
        "もう一度試すか、このサーバーを運用している人に知らせてください。",
    ),
    msg(
        "problem.signing_not_configured.title",
        "署名が設定されていません",
    ),
    msg(
        "problem.signing_not_configured.detail",
        "このサーバーには変更を履歴へ署名する設定がないため、提案を履歴に取り込めません。読み取り、提案、コメントはできます。",
    ),
    msg(
        "problem.signing_not_configured.hint",
        "このサーバーを運用している人に知らせてください。このリクエストでは何も行われていません。",
    ),
    msg(
        "problem.retirement_unfinished.title",
        "ウィキの鍵の変更が完了していません",
    ),
    msg(
        "problem.retirement_unfinished.detail",
        "このウィキの鍵の変更が完了していないため、運用している人が鍵の変更を完了するか取りやめるまで、履歴に新しく署名することはできません。変更は行われておらず、提案はそのまま保たれています。",
    ),
    msg(
        "problem.retirement_unfinished.hint",
        "このウィキを運用している人に知らせてください。完了または取りやめになったら、もう一度この提案を受け入れてください。",
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
    // proposals (handoff 004 T3): begin
    msg("wiki.nav_proposals", "提案"),
    msg("page.propose_change", "変更を提案する"),
    msg("page.propose_part", "セクションや一部分への変更を提案する"),
    msg("page.discussion", "ディスカッション"),
    msg("propose.new_page_link", "このページを提案する"),
    msg("state.draft", "下書き"),
    msg("state.held", "レビュー待ちで保持中"),
    msg("state.needs_revision", "修正が必要"),
    msg("state.accepted", "受け入れ済み"),
    msg("state.declined", "見送り済み"),
    msg("state.withdrawn", "取り下げ済み"),
    msg("state.superseded", "置き換え済み"),
    msg(
        "state_note.draft",
        "下書きです。始めた本人だけが見られます。まだ誰にもレビューを依頼していません。",
    ),
    msg(
        "state_note.held",
        "ウィキサービスはこの提案をレビュー待ちとして保持しています。権限のある人が提案を履歴に取り込むまで、ページには何も反映されません。",
    ),
    msg(
        "state_note.needs_revision",
        "この提案は、履歴に取り込む前に {author} さんによる修正が必要です。",
    ),
    msg(
        "state_note.accepted",
        "受け入れ済み: 権限のある人がこの提案を履歴に取り込みました。",
    ),
    msg(
        "state_note.declined",
        "見送り済み: 権限のある人が、この提案を履歴に取り込まないと判断しました。",
    ),
    msg(
        "state_note.withdrawn",
        "提案者が取り下げました。記録として残してあります。",
    ),
    msg(
        "state_note.superseded",
        "置き換え済み: 後から出された提案がこの提案に代わります。",
    ),
    msg(
        "proposal.superseded_by",
        "{proposal} に置き換えられました。",
    ),
    msg("proposal.another", "別の提案"),
    msg(
        "proposal.reason_declined",
        "{person} さんがこの提案を見送りました。理由: {reason}",
    ),
    msg(
        "proposal.reason_withdrawn",
        "{person} さんがこの提案を取り下げました。理由: {reason}",
    ),
    msg(
        "proposal.reason_asked",
        "{person} さんが修正を求めました。理由: {reason}",
    ),
    msg("outcome.signed", "署名して履歴に記録済み"),
    msg("outcome.overlap", "停止: 変更が重なっています"),
    msg("outcome.not_allowed", "許可されていません"),
    msg("outcome.busy", "ウィキは混み合っています"),
    msg("outcome.recovery", "ウィキの復旧が必要です"),
    msg("outcome.refused", "拒否: {limit}"),
    msg("outcome.refused_heading", "拒否されました"),
    msg("outcome_note.signed", "提案は履歴に取り込まれました。"),
    msg(
        "outcome_note.overlap",
        "同じ箇所に別の変更が入っていました。何も変更していません。提案者に修正をお願いし、両方の文面を提案のページに表示します。",
    ),
    msg(
        "outcome_note.not_allowed",
        "このウィキでのあなたの役割には、提案を受け入れる権限がありません。何も変更していません。",
    ),
    msg(
        "outcome_note.busy",
        "別の変更を履歴に記録している最中です。何も変更していません。しばらくしてからもう一度お試しください。",
    ),
    plural_msg(
        "outcome_note.busy_depth",
        "あなたの前に {count} 件の受け入れが待っています。",
        "あなたの前に {count} 件の受け入れが待っています。",
    ),
    msg(
        "outcome_note.recovery",
        "ウィキは中断後の確認を行っています。読むことはできますが、確認が終わるまで変更はできません。",
    ),
    msg(
        "outcome.unchanged_refused",
        "署名されませんでした: 履歴が受け付けませんでした",
    ),
    msg(
        "outcome_note.unchanged_refused",
        "履歴は、用意された変更をそのままでは受け付けませんでした。何も変更しておらず、提案はレビュー待ちのままです。",
    ),
    msg(
        "outcome.knowledge_invalid",
        "署名されませんでした: 主張または出典のファイルを受け付けられません",
    ),
    msg(
        "outcome_note.knowledge_invalid",
        "この提案の主張または出典のファイルが、ウィキが読める形になっていないか、この変更によって別のファイルが存在しないものを指すことになります。何も変更しておらず、提案はレビュー待ちのままです。ファイルを直してから、もう一度受け入れてください。",
    ),
    msg("outcome.too_large", "大きすぎて今は署名できません"),
    msg(
        "outcome_note.too_large",
        "この変更の署名には、1 回の署名で使える量を超えるメモリが必要です。何も変更しておらず、提案はレビュー待ちのままです。",
    ),
    msg(
        "outcome.retirement_unfinished",
        "署名されませんでした: このウィキの鍵の変更が完了していません",
    ),
    msg(
        "outcome_note.retirement_unfinished",
        "このウィキの鍵の変更が完了していないため、運用している人が鍵の変更を完了するか取りやめるまで、履歴に新しく署名することはできません。何も変更しておらず、提案はそのまま保たれています。その人に伝えてください。完了または取りやめになったら、もう一度この提案を受け入れてください。",
    ),
    msg("outcome.unavailable", "今は実行できません"),
    msg(
        "outcome_note.unavailable",
        "記録または履歴を今は利用できませんでした。何も変更していません。しばらくしてからもう一度お試しください。何度も起きる場合は、このサーバーを運用している人に伝えてください。",
    ),
    msg("outcome.in_progress", "この修正版は署名の最中です"),
    msg(
        "outcome_note.in_progress",
        "この修正版を履歴に取り込む以前の操作が、まだ終わっていません。今回は何も行っていません。しばらく待ってから、提案をもう一度開いてください。",
    ),
    msg("outcome.nothing_to_change", "変更するものがありません"),
    msg(
        "outcome_note.nothing_to_change",
        "この提案が求める内容は、すでに履歴に入っています。何も変更していません。",
    ),
    msg("outcome.store", "結果を確定できません"),
    msg(
        "outcome_note.store",
        "変更を取り込む途中で、記録を利用できなくなりました。変更が履歴に入ったかどうかは分かりません。もう一度試す前に履歴を確認し、このサーバーを運用している人に伝えてください。",
    ),
    msg("accept.heading", "このビルドでは受け入れを利用できません"),
    msg(
        "accept.not_served",
        "このサーバーのビルドは、提案を履歴に取り込みません。",
    ),
    msg(
        "accept.not_served_kept",
        "提案はレビュー待ちのままで、何も変更していません。",
    ),
    msg(
        "accept.not_served_note",
        "このサーバーでは、受け入れを利用できません。",
    ),
    msg(
        "accept.unconfigured_heading",
        "このサーバーでは受け入れが設定されていません",
    ),
    msg(
        "accept.unconfigured",
        "このサーバーには履歴へ署名する設定がないため、何も変更しておらず、提案はレビュー待ちのままです（このサーバーを運用している人に伝えてください）。",
    ),
    msg(
        "accept.unconfigured_note",
        "このサーバーでは、受け入れが設定されていません。",
    ),
    msg("overlap.heading", "変更が重なっている箇所"),
    msg(
        "overlap.intro",
        "ウィキの現在の文面とこの提案が、下の箇所をどちらも変更しています。何も統合していません。両方の文面を見ながら、提案を修正してください。",
    ),
    msg("overlap.caption", "ウィキと提案の両方で変更された箇所"),
    msg("overlap.col_where", "場所"),
    msg("overlap.col_theirs", "ウィキの現在の文面"),
    msg("overlap.col_ours", "この提案の文面"),
    msg("overlap.nothing", "（削除されています: 何もありません）"),
    msg("overlap.row", "{page}: {place}"),
    msg("overlap.place_top", "ページ先頭の情報"),
    msg("overlap.place_key", "ページ先頭の情報: {key}"),
    msg("overlap.place_part", "ページの一部分"),
    msg("overlap.place_number", "部分 {number}"),
    msg("overlap.place_named", "{id} と名付けられた部分"),
    msg("overlap.place_page", "ページ全体"),
    msg("overlap.place_path", "ページのアドレス"),
    msg("quota.open_proposals", "未決の提案が多すぎます"),
    msg("quota.revisions", "1 つの提案の修正が多すぎます"),
    msg("quota.accepted_bytes", "本日受け入れられた文章が多すぎます"),
    msg("quota.hour_person", "直近 1 時間に出した提案が多すぎます"),
    msg(
        "quota.hour_wiki",
        "直近 1 時間にこのウィキで出された提案が多すぎます",
    ),
    msg("quota.queue", "待機中の受け入れが多すぎます"),
    msg(
        "quota.comments_hour_person",
        "直近 1 時間のコメントが多すぎます",
    ),
    msg(
        "quota.counts",
        "上限は {allowed} です。現在の数は {current} で、今回の要求は {asked} です。",
    ),
    msg("quota.reset_at", "{when} にリセットされます。"),
    msg(
        "quota.reset_proposal_closes",
        "あなたの未決の提案のどれかが、受け入れ・見送り・取り下げ・置き換えのいずれかになるとリセットされます。",
    ),
    msg(
        "quota.reset_queue",
        "待機中の受け入れの順番が来るか、取りやめになるとリセットされます。",
    ),
    msg(
        "quota.reset_never_revisions",
        "待っても解消しません。この提案はこれ以上修正できません。新しい提案を出してください。",
    ),
    msg(
        "quota.reset_never_too_big",
        "待っても解消しません。この量は上限を超えています。もっと小さくしてください。",
    ),
    msg("size.mib", "{amount} MiB"),
    msg("size.kib", "{amount} KiB"),
    msg("size.bytes", "{amount} バイト"),
    msg("needs_account.heading", "提案にはアカウントが必要です"),
    msg(
        "needs_account.body",
        "サインインしていません。変更の提案、コメント、レビューにはアカウントが必要です。",
    ),
    msg(
        "needs_account.advice",
        "アカウントの作り方は、このウィキを運営している人にお尋ねください。",
    ),
    msg(
        "refused.not_allowed",
        "このウィキでのあなたの役割には、この操作が含まれていません。何も変更していません。",
    ),
    msg("refused.not_now_heading", "今はできません"),
    msg("refused.invalid_heading", "保存できませんでした"),
    msg(
        "refused.not_in_this_state",
        "この提案は「{state}」の状態なので、今はこの操作はできません。",
    ),
    msg(
        "refused.already_decided",
        "この修正にはすでに判断を記録しています。判断は変更できません。次の修正であらためて判断してください。",
    ),
    msg(
        "refused.own_proposal",
        "自分の提案を承認したり、修正を求めたりすることはできません。ほかのレビュー担当者に判断を依頼してください。コメントはできます。",
    ),
    msg(
        "refused.stale_revision",
        "この提案には新しい修正（{latest} 番）があります。読み直して、そちらについて判断してください。",
    ),
    msg(
        "refused.superseder",
        "「{state}」の提案は、別の提案に代わることはできません。",
    ),
    msg(
        "refused.changed",
        "作業している間に提案が変更されました。読み直してから、もう一度お試しください。",
    ),
    msg(
        "invalid.incomplete",
        "フォームの内容が不完全でした。ページからもう一度開いて、お試しください。",
    ),
    msg(
        "invalid.path_collides",
        "大文字と小文字の違いしかない名前のページ、または移動先のページがすでにあります。別のタイトルにしてください。",
    ),
    msg(
        "invalid.form_too_large",
        "送信された内容が、フォームで扱える大きさを超えています。",
    ),
    msg(
        "invalid.summary_missing",
        "何をなぜ変更したのかを、短く書いてください。",
    ),
    msg(
        "invalid.summary_too_long",
        "要約が {limit} 文字を超えています。",
    ),
    msg(
        "invalid.summary_not_one_line",
        "要約は、改行を含まない 1 行の文章にしてください。",
    ),
    msg("invalid.too_many_sources", "出典は {limit} 件までです。"),
    msg(
        "invalid.bad_source",
        "出典は 1 行で、空でなく、{limit} バイト以内にしてください。",
    ),
    msg(
        "invalid.nothing_proposed",
        "文章がページと同じです。先に変更してから送信してください。",
    ),
    msg(
        "invalid.too_large",
        "1 つの提案で扱える文章は、合計で {limit} までです。",
    ),
    msg(
        "invalid.page_too_large",
        "1 ページで扱える文章は {limit} までです。",
    ),
    msg(
        "invalid.reason_missing",
        "理由が必要です。やさしい言葉で、次にできることも書いてください。",
    ),
    msg("invalid.reason_too_long", "理由が {limit} を超えています。"),
    msg("invalid.comment_empty", "コメントには文章が必要です。"),
    msg(
        "invalid.comment_too_long",
        "コメントが {limit} を超えています。",
    ),
    msg("title.proposals", "提案 · {wiki} · {product}"),
    msg("proposals.heading", "提案"),
    msg(
        "proposals.intro",
        "{wiki} への変更はここで提案され、ほかの人に読まれ、権限のある人によって履歴に取り込まれます。",
    ),
    msg("proposals.empty", "まだ提案はありません。"),
    msg("proposals.new_page", "新しいページを提案する"),
    msg("proposals.drafts", "あなたの下書き"),
    msg("proposals.open", "未決の提案"),
    msg("proposals.closed", "決着済みの提案"),
    msg("proposals.row_by", "提案者: {author} · 最終更新 {when}"),
    msg("proposal.no_summary", "（要約はまだありません）"),
    msg("proposals.back", "提案の一覧へ戻る"),
    msg("proposal.back", "提案へ戻る"),
    msg("person.unknown", "ある方"),
    msg(
        "title.propose_parts",
        "{page} の変更箇所を選ぶ · {wiki} · {product}",
    ),
    msg("parts.heading", "{page} の変更箇所を選ぶ"),
    msg(
        "parts.intro",
        "ページ全体、1 つのセクション、または 1 つの部分への変更を提案できます。",
    ),
    msg("parts.whole", "ページ全体"),
    msg("parts.sections_heading", "セクション"),
    msg("parts.parts_heading", "部分"),
    msg("parts.no_sections", "このページには見出しがありません。"),
    msg("parts.section_label", "セクション: {heading}"),
    msg("parts.part_label", "部分 {number}: {excerpt}"),
    msg("parts.cut", "最初の {limit} 件のみ表示しています。"),
    msg("parts.back", "{page} へ戻る"),
    msg(
        "title.propose_page",
        "{page} への変更を提案する · {wiki} · {product}",
    ),
    msg(
        "title.propose_new",
        "新しいページを提案する · {wiki} · {product}",
    ),
    msg(
        "title.propose_revise",
        "提案を修正する · {wiki} · {product}",
    ),
    msg("propose.heading_page", "{page} への変更を提案する"),
    msg("propose.heading_new", "新しいページを提案する"),
    msg("propose.heading_revise", "この提案を修正する"),
    msg(
        "propose.intro",
        "文章を編集し、何をなぜ変更したのかを書いて、レビューに送ってください。権限のある人があなたの提案を履歴に取り込むまで、ウィキは変わりません。",
    ),
    msg("propose.scope_whole", "ページ全体を変更します。"),
    msg(
        "propose.scope_section",
        "1 つのセクションを変更します: {part}",
    ),
    msg("propose.scope_part", "1 つの部分を変更します: {part}"),
    msg("propose.choose", "別のセクションや部分を選ぶ"),
    msg("propose.label_text", "{title} の文章"),
    msg("propose.label_section", "{title} のこのセクションの文章"),
    msg("propose.label_part", "{title} のこの部分の文章"),
    msg("propose.label_title", "新しいページのタイトル"),
    msg(
        "propose.title_hint",
        "スラッシュを使うと、ページをグループに入れられます。例: Guides/Setup",
    ),
    msg("propose.new_page_name", "新しいページ"),
    msg("propose.label_summary", "何をなぜ（{limit} 文字まで）"),
    msg(
        "propose.summary_hint",
        "1 行で書いてください。例: 最初の会合の日付を訂正。",
    ),
    msg("propose.label_sources", "出典（1 行に 1 件、省略可）"),
    msg("propose.sources_hint", "変更の裏付けとなるリンクや文献。"),
    msg("propose.preview", "プレビュー"),
    msg("propose.show_changes", "変更点を表示"),
    msg("propose.save_draft", "下書きとして保存"),
    msg("propose.submit", "送信"),
    msg("propose.submit_revision", "修正を送信"),
    msg(
        "propose.saved",
        "下書きを保存しました。あなただけが見られます。",
    ),
    msg("propose.problems", "先に次の点を直してください"),
    msg("propose.continue_draft", "下書きの続きを書く"),
    msg("propose.removed_note", "この提案は {title} を削除します。"),
    msg("propose.preview_heading", "{title} のプレビュー"),
    msg(
        "propose.preview_note",
        "あなたの文章はこのように表示されます。まだ何も保存されていません。",
    ),
    msg("propose.suggestions", "改善の提案"),
    msg(
        "propose.front_unreadable",
        "ページ先頭の情報が読み取れません。各行は「名前」「コロン」「値」の形にしてください。",
    ),
    msg(
        "propose.front_owned",
        "ページ先頭の {keys} の行は、このウィキの管理者だけが変更できます。",
    ),
    msg(
        "propose.title_empty",
        "タイトルは空にできません。スラッシュで区切った各部分も空にできません。",
    ),
    msg(
        "propose.title_reserved",
        "タイトルの一部をドット 1 つや 2 つだけにすることはできません。",
    ),
    msg("propose.title_too_long", "タイトルの一部が長すぎます。"),
    msg(
        "propose.title_too_deep",
        "タイトルが長すぎるか、部分が多すぎます。",
    ),
    msg(
        "propose.title_taken",
        "{title} というタイトルのページはすでにあります。代わりにそのページへの変更を提案してください。",
    ),
    msg("changes.proposal_heading", "変更内容"),
    msg(
        "changes.proposal_none",
        "文章がいまのページと同じです。提案する変更はありません。",
    ),
    msg("changes.page_heading", "ページ: {title}"),
    msg("changes.status_added", "新しいページです。"),
    msg("changes.status_removed", "このページは削除されます。"),
    msg("changes.status_modified", "このページは変更されます。"),
    msg("changes.info_heading", "ページ先頭の情報"),
    msg("changes.part_changed", "部分 {number} が変更されます"),
    msg("changes.part_added", "部分 {number} が追加されます"),
    msg("changes.part_removed", "部分 {number} が削除されます"),
    plural_msg(
        "changes.unchanged_parts",
        "{count} 件の部分は変更されません。",
        "{count} 件の部分は変更されません。",
    ),
    msg("title.proposal", "提案: {summary} · {wiki} · {product}"),
    msg("proposal.proposed_by", "提案者: {author}、{when}。"),
    plural_msg(
        "proposal.pages",
        "変更するページ: {count} 件。",
        "変更するページ: {count} 件。",
    ),
    msg(
        "proposal.revision",
        "{total} 件中の {number} 番目の修正です。",
    ),
    msg("proposal.view", "ビュー {view} に対する提案です。"),
    msg("proposal.sources_heading", "出典"),
    msg("proposal.signed_change", "変更を見る"),
    msg("proposal.actions_heading", "できること"),
    msg("action.continue", "編集を続ける"),
    msg("action.revise", "この提案を修正する"),
    msg("action.withdraw", "この提案を取り下げる"),
    msg("action.ask", "修正を求める"),
    msg("action.decline", "この提案を見送る"),
    msg("action.accept", "受け入れる"),
    msg("thread.heading", "ディスカッション"),
    msg("thread.none", "まだコメントはありません。"),
    msg("comment.meta", "{when} · 修正 {number} について"),
    msg("comment.on_part", "{title} の部分 {number} について"),
    msg(
        "comment.orphaned",
        "孤立したコメント: このコメントの対象だった {title} の部分は、提案から無くなっています。対象だった文章:",
    ),
    msg("comment.on_this_part", "部分 {number} にコメントする"),
    msg("comment.reply", "{author} さんに返信する"),
    msg("comment_form.label", "コメントを書く"),
    msg("comment_form.button", "コメントする"),
    msg(
        "comment_form.about_part",
        "このコメントは {title} の部分 {number} についてです。",
    ),
    msg(
        "comment_form.reply_to",
        "このコメントは {author} さんへの返信です。",
    ),
    msg("decisions.heading", "判断"),
    msg("decisions.none", "まだ判断したレビュアーはいません。"),
    msg(
        "decision.approved",
        "{reviewer} さんが承認しました（修正 {number} について）。",
    ),
    msg(
        "decision.requested",
        "{reviewer} さんが変更を求めました（修正 {number} について）。",
    ),
    msg(
        "decision.older",
        "この後に新しい修正が書かれました。この判断は、受け入れの条件としてはもう数えられません。",
    ),
    msg("decide.label", "判断に添えるメモ（省略可）"),
    msg("decide.approve", "この修正を承認する"),
    msg("decide.request", "変更を求める"),
    msg("title.decline", "提案を見送る · {wiki} · {product}"),
    msg("title.withdraw", "提案を取り下げる · {wiki} · {product}"),
    msg("title.ask", "修正を求める · {wiki} · {product}"),
    msg("reason.decline_heading", "この提案を見送る"),
    msg(
        "reason.decline_intro",
        "提案者の労をねぎらい、履歴に取り込まない理由と、次にできることを書いてください。提案者に伝わり、提案を読む人全員に理由が表示されます。",
    ),
    msg("reason.decline_button", "見送る"),
    msg("reason.withdraw_heading", "この提案を取り下げる"),
    msg(
        "reason.withdraw_intro",
        "提案を取り下げる理由を書いてください。提案は記録として残り、読む人全員に理由が表示されます。",
    ),
    msg("reason.withdraw_button", "取り下げる"),
    msg("reason.ask_heading", "修正を求める"),
    msg(
        "reason.ask_intro",
        "何をなぜ直す必要があるのかを書いて、提案者が次に何をすればよいか分かるようにしてください。提案者に伝わり、新しい修正を送れます。",
    ),
    msg("reason.ask_button", "修正を求める"),
    msg("reason.label", "理由"),
    msg(
        "title.discussion",
        "{page} のディスカッション · {wiki} · {product}",
    ),
    msg("discussion.heading", "{page} のディスカッション"),
    msg(
        "discussion.intro",
        "このページについてこれまでに語られたこと。ページを変更する提案と、それぞれのディスカッションです。",
    ),
    msg(
        "discussion.empty",
        "このページを変更する提案はまだありません。",
    ),
    msg(
        "discussion.cut",
        "新しい {limit} 件の提案のみ確認しました。",
    ),
    msg("discussion.page", "{page} を読む"),
    msg("discussion.propose", "このページへの変更を提案する"),
    msg(
        "lint.bom",
        "{guideline}: テキストの先頭にバイトオーダーマークがあります。",
    ),
    msg(
        "lint.crlf",
        "{guideline}: このページの改行は CR LF です。LF をおすすめします。",
    ),
    msg(
        "lint.newline",
        "{guideline}: テキストの末尾に改行がありません。",
    ),
    msg(
        "lint.tabs",
        "{guideline}: コード以外でタブ文字が使われています。空白をおすすめします。",
    ),
    msg(
        "lint.no_info",
        "{guideline}: ページ先頭に情報がありません。タイトルを書くと、ページを持ち運びやすくなります。",
    ),
    msg(
        "lint.no_title",
        "{guideline}: ページ先頭の情報にタイトルがありません。",
    ),
    msg(
        "lint.coerced",
        "{guideline}: ページ先頭の値が、意図と異なる種類の値として読まれました。",
    ),
    msg(
        "lint.list",
        "{guideline}: ページ先頭の値のうち、文字列の一覧であるべきものが一覧になっていません。",
    ),
    msg(
        "lint.status",
        "{guideline}: ページ先頭の状態が、推奨されている値ではありません。",
    ),
    msg(
        "lint.redirect",
        "{guideline}: このページはリダイレクトですが、移動先のページが書かれていません。",
    ),
    msg(
        "lint.visibility",
        "{guideline}: ページ先頭の公開範囲が、公開・内部・制限のどれでもありません。",
    ),
    msg(
        "lint.secret",
        "{guideline}: ページ先頭に、秘密情報のように見える名前があります。ページは秘密を書く場所ではありません。",
    ),
    msg(
        "lint.duplicate_id",
        "{guideline}: ページ内の 2 つの部分が同じ識別子を持っています。",
    ),
    msg(
        "lint.unclosed",
        "{guideline}: 開いたコンテナが閉じられていません。",
    ),
    msg(
        "lint.callout",
        "{guideline}: 注意書きの種類が、よく使われるものではありません。",
    ),
    msg(
        "lint.container",
        "{guideline}: このウィキが知らない名前のコンテナがあります。",
    ),
    msg(
        "lint.heading_top",
        "{guideline}: ページが最上位の見出しを使っているか、見出しの段階を飛ばしています。",
    ),
    msg(
        "lint.heading_dup",
        "{guideline}: 同じ文面の見出しが 2 つあり、リンク先の名前が重なります。",
    ),
    msg(
        "lint.tags",
        "{guideline}: タグの書き方が一般的な形ではありません。",
    ),
    msg(
        "lint.empty_label",
        "{guideline}: ラベルが空のリンクがあります。",
    ),
    msg(
        "lint.empty_target",
        "{guideline}: 行き先のないリンクがあります。",
    ),
    msg(
        "lint.missing_part_ref",
        "{guideline}: このページ内の部分へのリンクが、存在しない部分を指しています。",
    ),
    msg(
        "lint.prefix",
        "{guideline}: リンクが、このウィキで使われない接頭辞で場所を指しています。",
    ),
    msg(
        "lint.order",
        "{guideline}: リンクが複数のページを指しうるため、いちばん近いページが選ばれます。",
    ),
    msg(
        "lint.dangling",
        "{guideline}: まだ存在しないページへのリンクがあります。",
    ),
    msg(
        "lint.ambiguous",
        "{guideline}: リンクが複数のページに当てはまり、どれも近くありません。",
    ),
    msg(
        "lint.other",
        "{guideline}: このテキストについて、リンターからの提案があります。",
    ),
    // proposals (handoff 004 T3): end
    // sign-in and accounts (the sign-in handoff U1, U2): begin
    msg("header.account_nav", "アカウント"),
    msg("header.signed_in", "{name} としてサインイン中"),
    msg("header.sign_out", "サインアウト"),
    msg("header.sign_in", "サインイン"),
    msg(
        "title.form_expired",
        "フォームの有効期限が切れました · {product}",
    ),
    msg("form_expired.heading", "フォームの有効期限が切れました"),
    msg(
        "form_expired.body",
        "このフォームをあなたのサインインと照合できなかったため、何も変更していません。フォームが古いか、開いたあとにサインインまたはサインアウトした可能性があります。前のページに戻り、再読み込みしてからもう一度お試しください。",
    ),
    msg(
        "problem.unavailable",
        "今回は処理できず、何も変更していません。しばらくしてからもう一度お試しください。何度も起きる場合は、このサーバーの管理者に知らせてください。",
    ),
    msg("title.sign_in", "サインイン · {product}"),
    msg("sign_in.heading", "サインイン"),
    msg("sign_in.login_label", "サインイン名"),
    msg("sign_in.password_label", "パスワード"),
    msg("sign_in.button", "サインインする"),
    msg(
        "sign_in.failed",
        "サインイン名とパスワードが一致しませんでした。両方を確かめて、もう一度お試しください。",
    ),
    msg(
        "sign_in.locked",
        "試行回数が多すぎます。しばらく待ってからもう一度お試しください。",
    ),
    msg(
        "sign_in.busy",
        "サーバーがただいま混み合っています。少し待ってからもう一度お試しください。",
    ),
    msg("sign_in.already", "{name} としてサインインしています。"),
    msg("sign_in.to_account", "アカウントへ移動"),
    msg(
        "sign_in.invitation_only",
        "招待が必要な場合は、このウィキを運営している人にお願いしてください。",
    ),
    msg("sign_in.join", "このウィキに参加する"),
    msg("title.sign_out", "サインアウト · {product}"),
    msg("sign_out.heading", "サインアウト"),
    msg(
        "sign_out.body",
        "このブラウザをサインアウトします。ほかのブラウザでのサインインはそのまま続きます。",
    ),
    msg("sign_out.not_signed_in", "サインインしていません。"),
    msg("title.join", "参加 · {product}"),
    msg("join.heading", "このウィキに参加する"),
    msg(
        "join.intro",
        "サインイン名とパスワードを決めて、アカウントを作ります。パスワードは、このサーバーの管理者を含め、誰にも読まれません。",
    ),
    msg("join.button", "参加する"),
    msg("join_closed.heading", "参加は受け付けていません"),
    msg(
        "join_closed.body",
        "ここでのアカウントは、招待から作られます。このウィキを運営している人に招待をお願いしてください。",
    ),
    msg("title.invite", "招待 · {product}"),
    msg("invite.heading", "招待されています"),
    msg(
        "invite.intro",
        "サインイン名とパスワードを決めて、アカウントを作ります。このリンクは 1 回だけ使えます。パスワードは、このサーバーの管理者を含め、誰にも読まれません。",
    ),
    msg("invite.button", "アカウントを作る"),
    msg("invite_unusable.heading", "この招待は使えません"),
    msg(
        "invite_unusable.body",
        "すでに使われたか、期限が切れた可能性があります。新しい招待を、このウィキを運営している人にお願いしてください。",
    ),
    msg("account_form.name_label", "あなたの名前"),
    msg(
        "account_form.name_hint",
        "あなたが書いたものの横に、ほかの人へ表示される名前です。どの文字でも使えます。",
    ),
    msg(
        "account_form.login_hint",
        "3 から 64 文字で、英字 a から z、数字、ドット、アンダースコア、ハイフンが使えます。大文字は小文字として扱われます。",
    ),
    msg("account_form.password_label", "パスワードを決める"),
    msg(
        "account_form.password_hint",
        "{min} 文字以上です。ほかの決まりはありません。長い文章は、よいパスワードになります。",
    ),
    msg(
        "account_form.login_taken",
        "そのサインイン名は使われています。別の名前を選んでください。",
    ),
    msg(
        "account_form.login_length",
        "サインイン名は 3 から 64 文字です。",
    ),
    msg(
        "account_form.login_characters",
        "サインイン名には、英字 a から z、数字、ドット、アンダースコア、ハイフンだけを使い、先頭は英字か数字にします。",
    ),
    msg("account_form.name_length", "名前は 1 から 120 文字です。"),
    msg(
        "account_form.name_control",
        "名前に改行や制御文字は使えません。",
    ),
    msg(
        "account_form.name_space",
        "名前の先頭と末尾に空白は使えません。",
    ),
    msg(
        "account_form.password_short",
        "パスワードは {min} 文字以上にしてください。",
    ),
    msg("account_form.password_long", "そのパスワードは長すぎます。"),
    msg("title.account", "あなたのアカウント · {product}"),
    msg("account.heading", "あなたのアカウント"),
    msg("account.name_heading", "あなたの名前"),
    msg("account.name_label", "名前"),
    msg(
        "account.name_hint",
        "あなたが書いたものの横に、ほかの人へ表示される名前です。",
    ),
    msg("account.name_button", "名前を保存"),
    msg("account.name_saved", "名前を保存しました。"),
    msg("account.sessions_heading", "サインインしている場所"),
    msg(
        "account.sessions_intro",
        "各行は、いまあなたのアカウントにサインインしているブラウザです。セッションを終了すると、そのブラウザはサインアウトされます。",
    ),
    msg("account.session_line", "開始 {started}、終了予定 {ends}"),
    msg("account.session_current", "いま使っているブラウザです。"),
    msg("account.session_end", "このセッションを終了"),
    msg("account.session_ended", "そのセッションを終了しました。"),
    msg("account.tokens_heading", "API トークン"),
    msg(
        "account.tokens_intro",
        "API トークンを使うと、プログラムがあなたとして書き込み API を使えます。トークンにできるのは、与えたスコープの範囲と、あなたの役割で許される範囲までです。作成したときに 1 度だけ表示されます。",
    ),
    msg("account.tokens_none", "トークンはありません。"),
    msg("account.token_unlabelled", "メモのないトークン"),
    msg("account.token_scope", "スコープ: {scope}"),
    msg("account.token_live", "{when} まで使えます"),
    msg("account.token_expired", "{when} に期限が切れました"),
    msg("account.token_revoked", "{when} に取り消しました"),
    msg("account.token_revoke", "取り消す"),
    msg(
        "account.token_revoked_note",
        "そのトークンを取り消しました。",
    ),
    msg("account.token_new_heading", "トークンを作る"),
    msg("account.token_label", "用途"),
    msg(
        "account.token_label_hint",
        "見分けるための短いメモです。80 文字までです。",
    ),
    msg("account.token_scope_label", "スコープ"),
    msg("account.scope_read", "読み取り: ページ、履歴、提案"),
    msg("account.scope_propose", "提案: コメントと変更の提案も可能"),
    msg("account.scope_review", "レビュー: 提案のレビューも可能"),
    msg("account.scope_accept", "受け入れ: 提案の受け入れも可能"),
    msg("account.scope_admin", "管理: あなたの役割で許されるすべて"),
    msg("account.token_lifetime_label", "有効期間"),
    msg("account.lifetime_day", "1 日"),
    msg("account.lifetime_week", "7 日"),
    msg("account.lifetime_month", "30 日"),
    msg("account.lifetime_quarter", "90 日"),
    msg("account.lifetime_year", "1 年"),
    msg("account.token_button", "トークンを作る"),
    msg("account.token_made_heading", "新しいトークン"),
    msg(
        "account.token_made_body",
        "いまコピーしてください。表示されるのは今回 1 度だけで、あとから再表示することはできません。",
    ),
    msg(
        "account.token_made_use",
        "プログラムは、Authorization ヘッダーに Bearer とトークンを続けて送ります。",
    ),
    msg(
        "account.token_too_many",
        "有効なトークンがすでに {max} 個あります。先に 1 つ取り消してください。",
    ),
    msg(
        "account.token_label_long",
        "メモが長すぎます。80 文字までです。",
    ),
    // sign-in and accounts (the sign-in handoff U1, U2): end
    // sign-in through a provider (the OpenID Connect handoff U3): begin
    msg("sign_in.provider", "{provider} でサインイン"),
    msg(
        "sign_in.provider_joins",
        "{provider} で初めてサインインすると、アカウントが作られます。",
    ),
    msg(
        "sign_in.provider_failed",
        "{provider} でのサインインがうまくいきませんでした。何も変更されていません。もう一度お試しください。繰り返し起きる場合は、このサーバーの管理者にお知らせください。",
    ),
    msg(
        "sign_in.provider_state",
        "このサインインは、このブラウザーで始めたものではないか、すでに使われたか、時間がかかりすぎました。このページからもう一度始めてください。",
    ),
    msg(
        "sign_in.provider_not_linked",
        "プロバイダーはあなたを確認しましたが、そのサインインに結び付いたアカウントがここにはありません。招待が必要です。このウィキを運営している人にお願いしてください。",
    ),
    msg(
        "sign_in.provider_unreachable",
        "いまプロバイダーに接続できません。何も変更されていません。しばらくしてからもう一度お試しください。",
    ),
    msg(
        "invite.provider_intro",
        "パスワードの代わりに、プロバイダーに本人確認を任せることもできます。その場合、アカウントはそのサインインに結び付き、パスワードを覚える必要はありません。",
    ),
    msg(
        "invite.provider_link",
        "パスワードの代わりに {provider} を使う",
    ),
    msg(
        "invite.provider_taken",
        "{provider} でのサインインにはすでにここのアカウントがあるため、この招待は使われていません。代わりに {provider} でサインインしてください。",
    ),
    msg("title.landing", "サインインしています · {product}"),
    msg(
        "landing.body",
        "サインインしています。このページが自動で切り替わらない場合は、{link}。",
    ),
    msg("landing.continue", "続きのページへ進んでください"),
    msg(
        "sign_in.provider_not_navigation",
        "プロバイダーでのサインインは、このサイトのサインインのページから、ご自身でアドレスを開いたときにだけ始まります。何も開始されていません。このページのボタンをお使いください。",
    ),
    msg("account.token_password_label", "パスワード"),
    msg(
        "account.token_password_hint",
        "トークンはサインアウトした後も使えるため、パスワードをもう一度確認します。",
    ),
    msg(
        "account.token_password_wrong",
        "パスワードが違うため、トークンは作られませんでした。",
    ),
    msg(
        "account.token_provider_hint",
        "このアカウントはプロバイダーでサインインし、パスワードはありません。トークンは、サインインしてから 10 分以内にだけ作れます。",
    ),
    msg(
        "account.token_sign_in_again",
        "最後のサインインから時間がたっているため、トークンは作られませんでした。サインアウトしてもう一度サインインし、10 分以内にトークンを作ってください。",
    ),
    msg("account.password_heading", "パスワード"),
    msg(
        "account.password_intro",
        "パスワードを変更すると、このアカウントの他のセッションとすべての API トークンが終了します。このブラウザーはサインインしたままです。",
    ),
    msg(
        "account.password_none",
        "このアカウントにはパスワードがありません。プロバイダーでサインインします。",
    ),
    msg("account.password_current_label", "現在のパスワード"),
    msg("account.password_new_label", "新しいパスワード"),
    msg("account.password_button", "パスワードを変更"),
    msg(
        "account.password_wrong",
        "現在のパスワードが違います。何も変更されていません。",
    ),
    msg(
        "account.password_changed",
        "パスワードを変更しました。このアカウントの他のセッションとすべての API トークンは終了しました。",
    ),
    // sign-in through a provider (the OpenID Connect handoff U3): end
    // the knowledge layer's reader (K-T6)
    msg("page.link_why", "なぜここにあるのか"),
    msg("claim.link", "出典とビュー"),
    msg("claim.link_name", "出典とビュー: {paragraph}"),
    msg(
        "claim.link_name_nth",
        "出典とビュー（主張 {n}/{total}）: {paragraph}",
    ),
    msg("claim.paragraph_start", "「{text}」で始まる段落"),
    msg("claim.paragraph_empty", "文字のない段落"),
    msg("title.claim", "出典とビュー · {wiki} · {product}"),
    msg("claim.heading", "出典とビュー"),
    msg("claim.kind", "この主張の種類: {kind}。"),
    msg("claim.kind_observation", "観察"),
    msg("claim.kind_interpretation", "解釈"),
    msg("claim.kind_hypothesis", "仮説"),
    msg("claim.kind_opinion", "意見"),
    msg("claim.kind_decision", "決定"),
    msg("claim.kind_policy", "方針"),
    msg("claim.kind_question", "問い"),
    msg("claim.kind_unknown", "示されていません"),
    msg("claim.said_heading", "この主張が述べられている場所"),
    msg(
        "claim.said_nowhere",
        "この主張が述べられている段落は、現在のどのページにもありません。",
    ),
    msg("claim.sources_heading", "出典"),
    msg(
        "claim.sources_none",
        "この主張に付けられた出典はありません。",
    ),
    msg("claim.source_line", "出典 {id}（{kind}）: {how}。"),
    msg("claim.source_primary", "一次"),
    msg("claim.source_secondary", "二次"),
    msg("claim.source_observational", "観察"),
    msg("claim.source_computational", "計算"),
    msg("claim.source_anecdotal", "逸話"),
    msg("claim.source_other", "その他"),
    msg("claim.source_unreadable", "ファイルを読み取れません"),
    msg("claim.source_line_named", "{name}（{kind}）: {how}。"),
    msg("claim.source_titled", "「{title}」"),
    msg("claim.source_untitled", "題名のない出典"),
    msg("claim.source_by", "{author} による"),
    msg("claim.source_dated", "日付 {date}"),
    msg("claim.source_join", "{first}、{second}"),
    msg("claim.source_found_at", "所在: {reference}。"),
    msg("claim.source_identifier", "識別子: {id}。"),
    msg("claim.how_supports_direct", "直接に裏付けている"),
    msg("claim.how_supports_partial", "一部を裏付けている"),
    msg("claim.how_supports_indirect", "間接的に裏付けている"),
    msg("claim.how_challenges_direct", "直接に異議を唱えている"),
    msg("claim.how_challenges_partial", "一部に異議を唱えている"),
    msg("claim.how_challenges_indirect", "間接的に異議を唱えている"),
    msg(
        "claim.judged_earlier",
        "以前の文言に基づいて判断されたものです。",
    ),
    msg("claim.status_heading", "このビューでの状態"),
    msg(
        "claim.status_accepted",
        "このビューでは、この主張は認められています。",
    ),
    msg(
        "claim.status_contested",
        "このビューでは、この主張は争われています。",
    ),
    msg(
        "claim.status_unresolved",
        "このビューでは、この主張は未解決です。",
    ),
    msg(
        "claim.status_unknown",
        "このビューでは、この主張はまだ決定されていません。",
    ),
    msg("claim.decided_by", "{proposal}によって決定されました。"),
    msg("claim.decided_link", "決定した提案"),
    msg(
        "claim.decided_unlisted",
        "このウィキの記録にない提案によって決定されました。",
    ),
    msg("claim.none_heading", "そのような主張はありません"),
    msg(
        "claim.none_body",
        "このウィキには、そのアドレスの主張がないか、現在は読み取れません。",
    ),
    msg("why.heading", "なぜここにあるのか"),
    msg("why.back", "{page} に戻る"),
    // views: creating and comparing (K-T8)
    msg("title.views", "ビュー · {wiki} · {product}"),
    msg("views.heading", "ビュー"),
    msg(
        "views.intro",
        "ビューは、ウィキ全体の名前付きのバージョンです。読者は、別のビューを選ばない限り、既定のビューを見ます。",
    ),
    msg("views.default_name", "既定のビュー"),
    msg("views.named", "ビュー「{view}」"),
    msg(
        "views.default_note",
        "ビューを指定しないときに表示されます。",
    ),
    msg("views.kind_experimental", "実験的"),
    msg("views.kind_interpretation", "解釈"),
    msg(
        "views.kind_experimental_note",
        "作業中の状態です。その変更は、分岐元のビューに取り込まれることも、破棄されることもあります。",
    ),
    msg(
        "views.kind_interpretation_note",
        "競合する理解を、既定のビューと並べて保っています。",
    ),
    msg("views.forked_from", "{view}から分岐しました。"),
    msg(
        "views.no_origin",
        "このビューの出どころは、ウィキに記録されていません。",
    ),
    msg("views.read_link", "{view}を読む"),
    msg("views.compare_with_default", "{view}を既定のビューと比べる"),
    msg(
        "views.omitted",
        "ここには最初の {shown} 件のビューを表示しています。残りの {omitted} 件は表示していません。",
    ),
    msg(
        "views.only_default",
        "このウィキには既定のビューしかありません。",
    ),
    msg("views.create_link", "ビューを作成する"),
    msg("views.compare_link", "2 つのビューを比べる"),
    msg("views.all_link", "このウィキのすべてのビュー"),
    msg("title.views_new", "ビューを作成 · {wiki} · {product}"),
    msg("views_new.heading", "ビューを作成"),
    msg(
        "views_new.intro",
        "ビューは、すでにあるビューから分岐させた、ウィキ全体の名前付きのバージョンです。読める人なら誰でも、そのビューに変更を提案できます。ビューの作成は記録されます（作成した人、時刻、分岐元）。",
    ),
    msg("views_new.name_label", "名前"),
    msg(
        "views_new.name_hint",
        "半角の小文字、数字、ドット、ハイフン、アンダースコア。先頭は小文字か数字で、64 文字までです。",
    ),
    msg("views_new.kind_legend", "このビューの目的"),
    msg("views_new.from_label", "分岐元"),
    msg("views_new.submit", "ビューを作成する"),
    msg(
        "views_new.problem_name_empty",
        "ビューの名前を入力してください。",
    ),
    msg(
        "views_new.problem_name_start",
        "名前の先頭は、半角の小文字か数字にしてください。",
    ),
    msg(
        "views_new.problem_name_chars",
        "名前に使えるのは、半角の小文字、数字、ドット、ハイフン、アンダースコアだけです（ドットの連続や末尾のドットは使えません）。",
    ),
    msg("views_new.problem_name_long", "名前は 64 文字までです。"),
    msg(
        "views_new.problem_name_retired",
        "その名前はビューに付けられません。このビューがほかより優れている、または確定していると読めてしまうためです。ビューの内容を表す名前を選んでください。",
    ),
    msg(
        "views_new.problem_kind",
        "このビューの目的を選んでください。",
    ),
    msg(
        "views_new.problem_from",
        "分岐元には、すでにあるビューを選んでください。",
    ),
    msg(
        "views_new.problem_taken",
        "その名前のビューはすでにあります。ほかの名前を選んでください。",
    ),
    msg(
        "views_new.not_allowed",
        "このウィキでのあなたの役割には、ビューを作成する権限がありません。何も変更していません。",
    ),
    msg(
        "views_new.busy",
        "ウィキに別の変更を加えている最中です。何も変更していません。しばらくしてからもう一度お試しください。",
    ),
    msg("views_new.too_many_heading", "ビューが多すぎます"),
    msg(
        "views_new.too_many",
        "このウィキにはすでに {bound} 件のビューが開いており、これが上限です。何も変更していません。",
    ),
    msg(
        "views_new.retirement_heading",
        "作成されませんでした: このウィキの鍵の変更が完了していません",
    ),
    msg(
        "views_new.retirement",
        "このウィキの鍵の変更が完了していないため、運用している人がその変更を完了するか取りやめるまで、履歴に新しいものを作れません。何も変更していません。その人に伝えてください。完了または取りやめの後に、もう一度ビューを作成してください。",
    ),
    msg("views_new.failed_heading", "ビューは作成されませんでした"),
    msg(
        "views_new.failed",
        "履歴は、新しいビューを受け付けませんでした。何も変更していません。",
    ),
    msg(
        "views_new.uncertain",
        "ビューができたかどうかは分かりません。作ったあとでウィキのビューを読めなかったか、ビューは一覧にあるものの、作成が完了したことをリポジトリが示しませんでした。ビューができているかもしれませんし、できていないかもしれません。もう一度試す前にビューの一覧を確認し、このサーバーを運用している人に伝えてください。その人が確認するまで、ウィキは変更を受け付けないことがあります。",
    ),
    msg(
        "views_new.not_kept_heading",
        "このサーバーではビューを作成できません",
    ),
    msg(
        "views_new.not_kept",
        "このサーバーには、新しいビューに必要な記録を保つ仕組みがないため、今はここでビューを作成できません。何も変更していません。",
    ),
    msg(
        "views_new.unconfigured_heading",
        "このサーバーではビューの作成が設定されていません",
    ),
    msg(
        "views_new.unconfigured",
        "このサーバーには履歴へ署名する設定がないため、ビューを作成できません（このサーバーを運用している人に伝えてください）。何も変更していません。",
    ),
    msg("title.compare", "ビューの比較 · {wiki} · {product}"),
    msg("compare.heading", "ビューの比較"),
    msg(
        "compare.intro",
        "2 つのビューの違いを一覧にします。どちらが正しいかは示しません。",
    ),
    msg("compare.a_label", "1 つ目のビュー"),
    msg("compare.b_label", "2 つ目のビュー"),
    msg("compare.go", "比べる"),
    msg("compare.pick_two", "異なる 2 つのビューを選んでください。"),
    msg("compare.pair", "{a}と{b}の比較"),
    msg("compare.same", "2 つのビューの内容は同じです。"),
    msg("compare.claims_heading", "違いのある主張"),
    msg(
        "compare.claims_none",
        "2 つのビューで違いのある主張はありません。",
    ),
    msg("compare.claim_said_in", "{page}にある主張"),
    msg("compare.claim_unplaced", "どのページにも書かれていない主張"),
    msg("compare.diff_wording", "文面が異なります。"),
    msg("compare.diff_status", "状態: {va}では{a}、{vb}では{b}。"),
    msg("compare.diff_sources", "出典が異なります。"),
    msg("compare.only_in", "{view}にだけあります。"),
    msg(
        "compare.unreadable_in",
        "{view}にあるこの主張のファイルは読めないため、そこでの状態は示せません。",
    ),
    msg("compare.status_accepted", "受け入れ済み"),
    msg("compare.status_contested", "異論あり"),
    msg("compare.status_unresolved", "未解決"),
    msg("compare.status_unknown", "未決定"),
    msg(
        "compare.claims_omitted",
        "{shown} 件の主張を表示しています。違いのある主張がほかに {omitted} 件ありますが、表示していません。",
    ),
    msg("compare.pages_heading", "違いのあるページ"),
    msg(
        "compare.pages_none",
        "2 つのビューで違いのあるページはありません。",
    ),
    msg("compare.page_differs", "ページの内容が異なります。"),
    msg("compare.page_link", "{page}の違い"),
    msg(
        "compare.pages_omitted",
        "{shown} 件のページを表示しています。違いのあるページがほかに {omitted} 件ありますが、表示していません。",
    ),
    msg("compare.back", "ビューの一覧"),
    msg("title.compare_page", "{page}: 違い · {wiki} · {product}"),
    msg("compare.page_heading", "{title}: 2 つのビューでの違い"),
    msg(
        "compare.page_range",
        "「削除」の行は{a}だけに、「追加」の行は{b}だけにあります。",
    ),
    msg("compare.page_read", "{view}で{page}を読む"),
    msg("compare.page_back", "比較へ戻る"),
    msg(
        "compare.page_absent",
        "どちらのビューにもこのページはありません。",
    ),
    msg("claim.others_heading", "ほかのビュー"),
    msg(
        "claim.others_none",
        "ほかのビューで、この主張が異なって読めるものはありません。",
    ),
    msg("claim.others_compare", "{view}と比べる"),
    msg(
        "claim.others_limit",
        "このページで調べるほかのビューは {shown} 件までです。そのほかのビューは、ビューの比較で確かめてください。",
    ),
    msg(
        "claim.others_unchecked",
        "今は読めなかったビューがあるため、ここには載せていません。",
    ),
    // views: creating and comparing (K-T8): end
    // the knowledge layer's reader (K-T6): end
    // Why is this here? (K-T7)
    msg(
        "title.why",
        "なぜここにあるのか: {page} · {wiki} · {product}",
    ),
    msg(
        "why.intro",
        "{page} の各段落がどこから来たのかを、このページの履歴の範囲で示します。ここにある各段落には、それぞれリンクできます。",
    ),
    msg("why.history_link", "{page} の履歴全体"),
    plural_msg(
        "why.bounded",
        "このページの変更のうち、最新の {count} 件だけをここで読みました。それより古い変更があり、その前のことは示されません。",
        "このページの変更のうち、最新の {count} 件だけをここで読みました。それより古い変更があり、その前のことは示されません。",
    ),
    msg(
        "why.bounded_none",
        "一度のリクエストで読めるこのウィキの最新の変更の中に、このページの変更は見つかりませんでした。それより古い変更があり、示されません。",
    ),
    msg(
        "why.spent_none",
        "一度のリクエストでこのページに使える処理量を、このページの変更を一件も読まないうちに使い切りました。変更はありますが、示されません。",
    ),
    msg(
        "why.unfollowed",
        "このページの履歴を最初までたどれませんでした。古い版を読めなかったか、このページを書いた変更が履歴に載っていません。以下は、読めた変更についてだけ言えることです。",
    ),
    plural_msg(
        "why.left_out",
        "さらに {count} 件の段落はたどっていません。1 ページでたどるのは最初の {limit} 件の段落までです。",
        "さらに {count} 件の段落はたどっていません。1 ページでたどるのは最初の {limit} 件の段落までです。",
    ),
    msg("why.paragraphs_heading", "段落"),
    msg("why.no_paragraphs", "このページには段落がありません。"),
    msg("why.paragraph_label", "段落 {n}"),
    msg(
        "why.written_unchanged",
        "{change}で書かれました。それ以降は変更されていません。",
    ),
    msg("why.last_changed", "最後に変更されたのは{change}です。"),
    msg("why.first_written", "最初に書かれたのは{change}です。"),
    msg(
        "why.first_before",
        "ここで読んだ最も古い変更より前に書かれました。",
    ),
    msg(
        "why.unchanged_read",
        "ここで読んだ変更の中では、変更されていません。",
    ),
    msg(
        "why.found_written_unchanged",
        "最も古い文言が見つかったのは{change}です。それ以降の変更は見つかりませんでした。",
    ),
    msg("why.found_last", "見つかった最新の変更は{change}です。"),
    msg(
        "why.found_first",
        "最も古い文言が見つかったのは{change}です。それより前に一致する文言は見つかりませんでした。",
    ),
    msg(
        "why.basis_anchor",
        "アンカーでたどりました。読んだすべての変更で、このアンカーを持つ段落はこの段落ただ一つです。",
    ),
    msg(
        "why.basis_text",
        "文面の一致でたどりました。この段落を全変更にわたって結びつけるものは文面しかないため、以前の文言は文を比べて見つけています。複数の段落が同時に書き換えられた場合は、別の段落をたどっているかもしれません。",
    ),
    msg("why.changes_heading", "読んだ変更"),
    msg("why.changes_none", "このページの変更は読んでいません。"),
    msg("why.change_open", "履歴で変更 {number} を見る"),
    msg("why.proposal", "提案{proposal}から作られた変更です。"),
    msg(
        "why.proposal_named",
        "この変更のメッセージは、提案{proposal}を示しています。",
    ),
    msg(
        "why.proposal_unlisted",
        "この変更のメッセージが示す提案は、あなたが読めるこのウィキの記録にありません。",
    ),
    msg(
        "why.proposal_unconfirmed",
        "この提案がこの変更として受け入れられたことは、このウィキの記録からは確認できないため、そのレビューはここに示しません。",
    ),
    msg("why.review_heading", "レビュー"),
    msg(
        "why.review_none",
        "この提案にコメントや決定をした人はいません。",
    ),
    msg("why.decision_words", "次のように書き添えました:「{text}」"),
    msg(
        "why.comment",
        "{author}さんが改訂 {number} についてコメントしました:「{text}」",
    ),
    plural_msg(
        "why.review_more",
        "レビューのうちさらに {count} 件は提案のページにあります。",
        "レビューのうちさらに {count} 件は提案のページにあります。",
    ),
    msg("why.sources_heading", "提案に添えられた出典"),
    msg("why.sources_none", "提案に出典は添えられていません。"),
    // Why is this here? (K-T7): end
    // knowledge: claims and sources in the proposal path (handoff 005 K-T5): start
    msg("parts.attach", "出典を付ける"),
    msg("parts.challenge", "これに異議を示す"),
    msg("parts.attach_for", "部分 {number} に出典を付ける"),
    msg("parts.challenge_for", "これに異議を示す: 部分 {number}"),
    msg("title.attach", "{page} に出典を付ける · {wiki} · {product}"),
    msg(
        "title.challenge",
        "{page} の一部に異議を示す · {wiki} · {product}",
    ),
    msg("attach.heading_supports", "{page} に出典を付ける"),
    msg("attach.heading_challenges", "{page} の一部に異議を示す"),
    msg(
        "attach.intro_supports",
        "出典とは、ある記述のよりどころとなるものです。本、記事、記録、人の証言などがあります。出典を付けると、この段落が主張として記録され、何をよりどころにしているかを他の人が見られるようになります。段落の文章も変更できます。変更と出典は一つの提案になります。",
    ),
    msg(
        "attach.intro_challenges",
        "異議を示す出典は、ある記述、またはその一部に反することを述べています。追加すると、この段落が主張として記録され、何がそれに反して述べられているかが示されます。段落の文章も変更できます。変更と出典は一つの提案になります。",
    ),
    msg(
        "attach.claim_exists",
        "この段落はすでに主張として記録されています。出典はその主張に追加されます。",
    ),
    msg("attach.legend", "出典"),
    msg("attach.label_kind", "出典の種類"),
    msg("attach.kind_primary", "一次資料"),
    msg("attach.kind_secondary", "一次資料について述べた資料"),
    msg("attach.kind_observational", "観察の記録"),
    msg("attach.kind_computational", "計算の結果"),
    msg("attach.kind_anecdotal", "見聞きしたことについての本人の話"),
    msg("attach.kind_other", "その他"),
    msg("attach.label_title", "タイトル"),
    msg("attach.label_author", "著者または発行者"),
    msg("attach.label_date", "日付（書かれているとおり）"),
    msg(
        "attach.hint_date",
        "出典に書かれているとおりに、年、季節、日などを書いてください。",
    ),
    msg("attach.label_ref_form", "参照の方法"),
    msg("attach.ref_url", "ウェブのアドレス"),
    msg("attach.ref_doi", "DOI"),
    msg("attach.ref_isbn", "ISBN"),
    msg("attach.ref_page", "このウィキのページ"),
    msg("attach.ref_citation", "文章で書いた引用情報"),
    msg("attach.label_ref", "参照先"),
    msg(
        "attach.hint_ref",
        "タイトルだけで足りるときは空のままで構いません。ウェブのアドレスは https または http で始め、空白を含めないでください。",
    ),
    msg(
        "attach.label_bearing",
        "出典がこの段落にどの程度直接ふれているか",
    ),
    msg("attach.bearing_direct", "段落全体にふれている"),
    msg("attach.bearing_partial", "段落の一部にふれている"),
    msg("attach.bearing_indirect", "言外の含みとしてふれている"),
    msg("attach.label_note", "メモ（任意）"),
    msg(
        "attach.hint_note",
        "出典のどこを見ればよいか、または何が書かれているか。",
    ),
    msg("attach.label_claim_kind", "この段落はどんな種類の記述か"),
    msg(
        "attach.claim_kind_hint",
        "わかる範囲で選んでください。「未記入」でも構いません。",
    ),
    msg("attach.claim_kind_observation", "観察"),
    msg("attach.claim_kind_interpretation", "解釈"),
    msg("attach.claim_kind_hypothesis", "仮説"),
    msg("attach.claim_kind_opinion", "意見"),
    msg("attach.claim_kind_decision", "決定"),
    msg("attach.claim_kind_policy", "方針"),
    msg("attach.claim_kind_question", "問い"),
    msg("attach.claim_kind_unknown", "未記入"),
    msg(
        "attach.no_paragraph",
        "編集した部分には、出典を付けられる段落がありません。",
    ),
    msg(
        "attach.page_unreadable",
        "このページの先頭の情報を読み取れないため、主張に必要な識別子をページに付けられません。",
    ),
    msg(
        "attach.several_claims",
        "この段落には複数の主張が記録されており、このフォームでは出典がどれのためのものかを判断できません。",
    ),
    msg(
        "attach.full",
        "この主張には、一つの主張が持てる数の出典がすでにあります。",
    ),
    msg(
        "attach.says_nothing",
        "出典にはタイトルか参照先が必要です。",
    ),
    msg(
        "attach.too_long",
        "出典の文章が {limit} 文字を超えています。",
    ),
    msg(
        "attach.control",
        "出典の文章に、改行など、保存できない文字が含まれています。",
    ),
    msg(
        "attach.not_web",
        "ウェブのアドレスは https または http で始め、空白を含めないでください。",
    ),
    msg(
        "attach.unwritable",
        "出典を書き込めませんでした。何も変更されていません。もう一度お試しください。",
    ),
    msg(
        "curate.not_a_day",
        "日付は 2026-11-04 のように、年、月、日の順に書いてください。",
    ),
    msg(
        "curate.unknown_source",
        "この主張は、フォームにある出典の一つを引用していません。フォームを開き直してください。",
    ),
    msg(
        "curate.no_wording",
        "この主張はいま、どの段落にも書かれていないため、出典や状態を判断する文章がありません。",
    ),
    msg(
        "curate.day_not_come",
        "出典: {title}。{day} はどの時間帯でもまだ来ていない日のため、その日に確認した人はいません。確認した日を書くか、日付を空にしてください。",
    ),
    msg(
        "propose.claim_held_one",
        "この段落は他の人が頼りにしています。出典が付いています。目印はこちらで保っておくので、文章だけを編集してください。",
    ),
    msg(
        "propose.claim_held_many",
        "ここには出典が付いていて、他の人が頼りにしている段落があります。その段落の文章をそのままにしている間は、目印はこちらで保っておきます。その段落の文章を変えるときは、その部分だけを単独で変更してください。",
    ),
    msg(
        "propose.claim_lost",
        "「{excerpt}」で始まる段落には出典が付いていますが、この変更ではその段落がなくなります。文章を変えたいときは、その部分だけを単独で変更してください。削除するときは、下の欄にチェックを入れてください。",
    ),
    msg("propose.claim_drop_legend", "削除する段落"),
    msg(
        "propose.claim_drop",
        "「{excerpt}」を削除する。この段落は主張が書かれている場所ではなくなり、レビュアーにもそのことが示されます。",
    ),
    msg("know.heading", "この提案での主張と出典"),
    msg(
        "know.claim_made",
        "主張{claim}を記録します（{kind}）。{count} か所に書かれています。コミュニティでの扱い：{standing}。",
    ),
    msg(
        "know.cite",
        "出典 {source} を引用します: {relation}（{bearing}）。この提案を反映した結果で主張が持つ文言で判断しています。",
    ),
    msg(
        "know.cite_unjudged",
        "出典 {source} を引用します: {relation}（{bearing}）。この提案を反映した結果で主張が持たない文言で判断されています。",
    ),
    msg(
        "know.source_recorded",
        "出典 {source} を記録します（{kind}）。",
    ),
    msg("know.claim_changed", "主張{claim}を変更します:"),
    msg("know.source_changed", "出典 {source} を変更します:"),
    msg("know.removed_claim", "主張{claim}を削除します。"),
    msg("know.removed_source", "出典 {source} を削除します。"),
    msg(
        "know.unreadable",
        "この提案のファイル {file} は、ウィキが読める形になっていません。{reason}",
    ),
    msg("know.change_kind", "記述の種類が {kind} になります。"),
    msg(
        "know.change_lifecycle",
        "主張の扱いの段階が「{lifecycle}」になります。",
    ),
    msg(
        "know.change_standing",
        "コミュニティでの扱いが「{from}」から「{to}」に変わります。",
    ),
    msg(
        "know.change_said",
        "もう一つの段落にも書かれることになります：{paragraph}。",
    ),
    msg(
        "know.change_unsaid",
        "段落 {paragraph} には書かれなくなります。段落が削除されるか、主張がその段落から外れます。",
    ),
    msg(
        "know.change_related",
        "主張{other}との関係が加わります: {relation}。",
    ),
    msg(
        "know.change_unrelated",
        "主張{other}との関係がなくなります: {relation}。",
    ),
    msg(
        "know.change_uncited",
        "出典 {source} を引用しなくなります。",
    ),
    msg(
        "know.change_recited",
        "引用している出典 {source} と主張の関係が変わります: {relation}（{bearing}）。",
    ),
    msg(
        "know.change_noted",
        "引用している出典 {source} のメモが変わります。",
    ),
    msg("know.source_change_kind", "種類が {kind} になります。"),
    msg(
        "know.source_change_identity",
        "書かれている出典の名前や著者などが {identity} になります。",
    ),
    msg(
        "know.source_change_reference",
        "参照先が（{form}）{reference} になります。",
    ),
    msg(
        "know.source_change_snapshot",
        "保存されている写しが変わります。",
    ),
    msg(
        "know.source_change_reviewed",
        "最後に確認した日が {day} になります。",
    ),
    msg(
        "know.source_change_unreviewed",
        "確認済みの印がなくなります。",
    ),
    msg(
        "know.source_change_availability",
        "入手できるかどうかが「{availability}」になります。",
    ),
    msg(
        "know.source_change_superseded",
        "出典 {source} に置き換えられます。",
    ),
    msg(
        "know.is_curation",
        "これはキュレーション提案です。主張と出典を変更し、ページは変更しません。",
    ),
    msg(
        "know.not_curation",
        "この提案はコミュニティでの扱い、確認日、入手できるかどうかを設定するか、主張がすでに持つ出典リンクを変更・削除するか、確認済みの出典の題名・著者・日付・参照先・置き換え先の出典を変更するか、これらを持つ主張や出典を削除するか、読み取れないファイルを置き換えるか削除していますが、これができるのはキュレーション提案だけです。このままでは受け入れられません。",
    ),
    msg(
        "know.not_judged",
        "この提案は主張に対する判断（出典リンクまたはコミュニティでの扱い）を設定または変更していますが、その判断は、提案を反映した結果にその主張のどの段落にも存在しない文言に基づいています。このままでは受け入れられません。",
    ),
    msg("know.kind_observation", "観察"),
    msg("know.kind_interpretation", "解釈"),
    msg("know.kind_hypothesis", "仮説"),
    msg("know.kind_opinion", "意見"),
    msg("know.kind_decision", "決定"),
    msg("know.kind_policy", "方針"),
    msg("know.kind_question", "問い"),
    msg("know.kind_unknown", "種類は未記入"),
    msg("know.source_primary", "一次資料"),
    msg("know.source_secondary", "一次資料について述べた資料"),
    msg("know.source_observational", "観察の記録"),
    msg("know.source_computational", "計算の結果"),
    msg("know.source_anecdotal", "本人の話"),
    msg("know.source_other", "その他の種類"),
    msg("know.supports", "支持する"),
    msg("know.challenges", "異議を示す"),
    msg("know.directly", "直接"),
    msg("know.in_part", "部分的に"),
    msg("know.by_implication", "間接的に"),
    msg("know.life_proposed", "提示された"),
    msg("know.life_adopted", "取り上げられた"),
    msg("know.life_retired", "役目を終えた"),
    msg("know.life_unknown", "未記入"),
    msg("know.standing_accepted", "受け入れられている"),
    msg("know.standing_contested", "争われている"),
    msg("know.standing_unresolved", "決着していない"),
    msg("know.standing_unknown", "未決定"),
    msg("know.available", "入手できる"),
    msg("know.unavailable", "入手できない"),
    msg("know.availability_unknown", "不明"),
    msg("know.rel_refines", "より詳しく述べている"),
    msg("know.rel_supersedes", "置き換える"),
    msg("know.rel_contradicts", "食い違っている"),
    msg("know.rel_split_from", "から分かれた"),
    msg("know.rel_merged_into", "に統合された"),
    // knowledge: claims and sources in the proposal path (handoff 005 K-T5): end
    // knowledge: the curation proposal (handoff 005 K-T5): start
    msg(
        "title.curate",
        "主張をキュレーションする · {wiki} · {product}",
    ),
    msg("curate.heading", "主張をキュレーションする"),
    msg(
        "curate.intro",
        "キュレーション提案は、主張の扱い、出典が主張にどうかかわるか、出典を最後に確認した日を変更します。ページは変更しません。権限のある人が履歴に取り込むまで何も変わりません。コミュニティでの扱いは、この方法でのみ設定されます。",
    ),
    msg("curate.wording", "現在の書かれ方: 「{words}」"),
    msg(
        "curate.wording_none",
        "この主張は現在、どの段落にも書かれていません。",
    ),
    msg("curate.label_status", "コミュニティでの扱い"),
    msg(
        "curate.status_hint",
        "この主張についてコミュニティが決めたこと。この提案が受け入れられたときに設定されます。",
    ),
    msg(
        "curate.earlier",
        "これは、この主張の以前の文面について判断されたものです。",
    ),
    msg("curate.source_legend", "出典: {title}"),
    msg("curate.source_untitled", "タイトルのない出典"),
    msg("curate.label_relation", "主張との関係"),
    msg("curate.label_bearing", "主張にどの程度直接ふれているか"),
    msg("curate.label_reviewed", "最後に確認した日（年-月-日）"),
    msg(
        "curate.reviewed_hint",
        "確認した人が設定します。誰も確認していなければ空のままにしてください。",
    ),
    msg("curate.label_availability", "出典を入手できるか"),
    msg(
        "curate.availability_hint",
        "確認した人が判断します。自動では確認されません。",
    ),
    msg(
        "curate.confirm",
        "出典と扱いを、主張の現在の文面について改めて判断する",
    ),
    msg(
        "curate.confirm_hint",
        "主張の文面が変わり、出典を新しい文面と照らして読んだときに使ってください。",
    ),
    msg("curate.submit", "レビューに送る"),
    msg(
        "curate.nothing",
        "フォームの内容は、ウィキの現在の内容と変わりません。先に何かを変更してください。",
    ),
    // knowledge: the curation proposal (handoff 005 K-T5): end
    // knowledge: the claims a change touches (handoff 005 K-T5): start
    msg("touch.heading", "この変更が触れる主張"),
    msg(
        "touch.intro",
        "これらの主張は、この変更で文面が変わる、または削除される段落に書かれています。ウィキは見えることだけを示し、文面が変わった段落が同じ意味かどうかは判断しません。",
    ),
    msg(
        "touch.reworded",
        "段落「{words}」の文面が変わります。目印が残るため、同じ主張のままです。",
    ),
    msg(
        "touch.removed",
        "段落「{words}」が削除されます。この主張はそこには書かれなくなります。",
    ),
    msg(
        "touch.links_earlier",
        "{count} 件の出典は以前の文面について判断されています。以前の文面について判断されたものとして表示されます。",
    ),
    msg(
        "touch.standing_earlier",
        "コミュニティでの扱い（{status}）は以前の文面について決められています。以前の文面について判断されたものとして表示されます。",
    ),
    msg(
        "touch.nothing_earlier",
        "以前の文面について判断されたものはありません。",
    ),
    msg(
        "touch.reconfirm_hint",
        "すべてをまとめて一つの提案にします。出典と、決められている場合はコミュニティでの扱いを、いまの文面について改めて判断します。ページは変更せず、受け入れる人が必要です。",
    ),
    msg("touch.reconfirm_button", "改めて判断する提案をする"),
    msg(
        "touch.reconfirm_later",
        "この提案が受け入れられたら、ここから一度の操作で、これらの主張を改めて判断する提案ができます。",
    ),
    msg("touch.reconfirm_summary", "改めて判断する: {summary}"),
    msg(
        "touch.nothing_to_judge",
        "改めて判断する必要はありません。これらの主張はどれも、いまの文面について判断されています。",
    ),
    msg(
        "touch.reconfirm_open",
        "これらの主張を改めて判断する提案はすでにあります: {proposal}。その提案はまだ開いています。",
    ),
    msg(
        "touch.reconfirm_done",
        "これらの主張はどれも、受け入れられた提案で、いまの文面について改めて判断されています。",
    ),
    msg(
        "touch.reconfirm_of",
        "この提案は、{proposal}による変更で以前の文面について判断されたままになった主張を、改めて判断します。",
    ),
    // knowledge: the claims a change touches (handoff 005 K-T5): end
    msg(
        "invalid.knowledge_not_shown",
        "この提案の主張または出典の変更には、このページが説明しないものがあり、確認する人が内容を見られません。送信できません。",
    ),
    msg(
        "invalid.knowledge_unjudged",
        "この提案が主張に追加または変更する出典リンクは、提案を反映した結果にその主張のどの段落にも存在しない文言で判断されています。送信できません。",
    ),
    msg(
        "invalid.knowledge_status_unjudged",
        "この提案が主張に設定するコミュニティでの扱いは、提案を反映した結果にその主張のどの段落にも存在しない文言で判断されています。送信できません。",
    ),
    msg(
        "know.change_rejudged",
        "{subject}を判断したときの文面が、次の文面に変わります：「{words}」",
    ),
    msg(
        "know.change_rejudged_unseen",
        "{subject}を判断したときの文面が、この提案のあとには主張のどの段落にもない文面に変わります。",
    ),
    msg(
        "know.change_rejudged_cleared",
        "{subject}を判断した文面は、記録されなくなります。",
    ),
    msg("know.rejudged_status", "コミュニティでの扱い"),
    msg("know.rejudged_source", "引用している出典 {source}"),
    msg(
        "know.nothing_changes",
        "この提案の主張・出典ファイルは、ウィキにすでにあるファイルと同じです。変更はありません。",
    ),
    // knowledge: re-judgements shown (handoff 005 corrections K5): end
    msg(
        "attach.direction",
        "出典の文章に、文字の向きを変える文字や、表示されない文字が含まれていて、保存できません。",
    ),
    // knowledge: direction characters refused (handoff 005 corrections K14): end
    msg(
        "know.replaced_unreadable_claim",
        "読み取れない主張ファイル（{id}）を置き換えるか削除します。その内容は分かっていません。",
    ),
    msg(
        "know.replaced_unreadable_source",
        "読み取れない出典ファイル（{id}）を置き換えるか削除します。その内容は分かっていません。",
    ),
    msg("know.made_lifecycle", "主張の扱いの段階：{lifecycle}。"),
    msg(
        "know.made_related",
        "主張{other}との関係があります: {relation}。",
    ),
    msg(
        "know.source_made_availability",
        "入手できるかどうか：{availability}。",
    ),
    msg("know.source_where", "参照先（{form}）：{reference}。"),
    msg("know.source_copy", "写しが保存されています。"),
    msg(
        "know.source_change_unsuperseded",
        "置き換えられたという印がなくなります。",
    ),
    msg(
        "know.source_change_reference_cleared",
        "参照先が書かれなくなります。",
    ),
    msg("know.source_nameless", "題名も著者も日付もない出典"),
    // knowledge: a file that cannot be read is replaced (handoff 005 corrections, second round K16): end
    msg("know.subject_claim", "{id}"),
    msg("know.subject_claim_id", "「{excerpt}」［主張 {id}］"),
    msg("know.subject_source_id", "{name}［出典 {id}］"),
    msg("know.subject_words", "「{words}」"),
    msg("know.subject_paragraph", "ページ {page} の「{words}」"),
    msg(
        "know.subject_paragraph_block",
        "ページ {page} の「{words}」（段落の印 {block}）",
    ),
    // knowledge: subjects told apart (handoff 005 corrections, third round K24): end
    msg(
        "know.removed_had_standing",
        "コミュニティでの扱いも一緒に削除されます：{standing}。",
    ),
    msg(
        "know.removed_had_link",
        "出典 {source} とのつながりも一緒に削除されます: {relation}（{bearing}）。",
    ),
    msg(
        "know.removed_had_reviewed",
        "最後に確認した日も一緒に削除されます：{day}。",
    ),
    msg(
        "know.removed_had_availability",
        "入手できるかどうかの記録も一緒に削除されます：{availability}。",
    ),
    // knowledge: what leaves with a removal (handoff 005 corrections, third round K27): end
    msg("health.sources_name", "見直す出典"),
    msg(
        "health.sources_desc",
        "ずいぶん前に確認されたきり、または一度も確認されていない出典と、確認した人が利用できないと記した出典を、それを引用する主張とともに示します。ウィキは何も確認せず、何も取得しません。人が出典を確かめ、提案でそのことを記します。",
    ),
    msg("health.col_source", "出典"),
    msg("health.col_why", "挙げられている理由"),
    msg("health.col_claims", "それを引用する主張"),
    msg(
        "health.sources_action",
        "出典を確かめたうえで、確かめた日と、利用できるかどうかを提案してください。",
    ),
    msg("health.none_sources", "見直す必要のある出典はありません。"),
    msg(
        "health.sources_unavailable",
        "確認した人が、利用できないと記しています。",
    ),
    msg(
        "health.sources_never",
        "確認したと記録した人はまだいません。",
    ),
    msg("health.sources_last", "最後の確認は {day} です。"),
    msg(
        "health.sources_overdue",
        "最後の確認は {day} で、{limit} 日を超えて前です。",
    ),
    plural_msg(
        "health.sources_claims",
        "{count} 件の主張がこれを引用しています：",
        "{count} 件の主張がこれを引用しています：",
    ),
    msg(
        "health.sources_cut",
        "先頭の {limit} 件だけを挙げています。",
    ),
    msg(
        "health.sources_no_claims",
        "これを引用する主張はありません。",
    ),
    msg("health.sources_claim", "{where}（{how}）"),
    msg(
        "health.sources_claim_nowhere",
        "いまどのページにも書かれていない主張、識別子 {id}",
    ),
    msg(
        "health.sources_setting",
        "最後の確認から {days} 日を超えた出典が挙げられます。この日数は、このサーバーの管理者が定めます。",
    ),
    msg(
        "health.sources_future",
        "確認した日として {day} と記録されていますが、その日はまだ来ていません。",
    ),
    msg(
        "health.sources_showing",
        "{total} 件のうち {from} 件目から {to} 件目までを表示しています。",
    ),
    msg("health.sources_more", "続きの出典を表示"),
    msg("health.sources_first", "最初の出典を表示"),
    msg("health.sources_no_more", "これ以上の出典はありません。"),
    msg("know.curate_link", "この主張の扱いや出典の変更を提案する"),
    msg(
        "know.curate_link_of",
        "主張 {claim} の扱いや出典の変更を提案する",
    ),
    msg("claim_thread.heading", "この主張についての議論"),
    msg(
        "claim_thread.never_changes",
        "議論が主張を変えることはありません。主張を変えるのは、人々が受け入れた提案だけです。",
    ),
    plural_msg(
        "claim_thread.total",
        "{count} 件のコメント",
        "{count} 件のコメント",
    ),
    msg(
        "claim_thread.none",
        "この主張についての議論はまだありません。",
    ),
    msg("claim_thread.meta", "コメント {number} · {when}"),
    msg(
        "claim_thread.in_reply_to",
        "コメント {number} への返信です。",
    ),
    msg("claim_thread.more", "続きのコメントを表示"),
    msg("claim_thread.first", "最初のコメントを表示"),
    msg(
        "claim_thread.unavailable",
        "この主張の議論を現在は読み取れません。",
    ),
    msg("claim_thread.form_label", "この議論にコメントを追加"),
    msg("claim_thread.form_reply", "コメント {number} への返信"),
    msg("claim_thread.back", "主張のページに戻る"),
    msg(
        "api.claim_lost",
        "「{excerpt}」で始まる、マーカー ^{block} の段落には出典が付いていますが、この変更ではその段落がなくなります。削除するときは `drops` に ^{block} を指定してください。残すときは元に戻してください。",
    ),
    msg(
        "api.drop_unused",
        "`drops` にマーカー ^{block} の段落が指定されていますが、この変更ではその段落は削除されないか、その段落に書かれた主張がありません。この変更で削除する段落だけを指定してください。",
    ),
    msg("claim_thread.in_reply_to_link", "{comment} への返信です。"),
    msg("claim_thread.comment_label", "コメント {number}"),
];
