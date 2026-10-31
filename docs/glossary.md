# inklkunn — words the interface uses

**Status.** Drafted by the dev team with handoff 003 T2 (RFC 003 D-6), for the architect's reading and the
owner's. The English words of section 2 are **in force** in the catalog
(`crates/inklkunn-web/src/catalog/en.rs`); the Japanese words are a **preview**: a draft that no fluent
reviewer has read yet (DR-7: the owner names the reviewers). The tests
`crates/inklkunn-web/tests/glossary.rs` and `tests/retired_words.rs` read this page: **change the words here and
in the catalog together**, and a message the catalog has that this page does not list fails the build.

**Rules (INC-3; planeter RFC 012 D-4, D-6).** Say what happened and what to do next. Prefer the reader's word
to the implementer's. No jargon without a one-line explanation the reader opens on demand (a native
`<details>`, never a tooltip). Short sentences, active voice, no idioms or metaphors that do not translate. The
same word for the same thing everywhere. Calm and respectful tone; never blame the reader. No gendered words.
A message is a whole sentence with named placeholders; two sentences are two messages. The product name
`inklkunn` and the name `prikk` are names, not jargon.

**Where the words come from.** *Reused from planeter* means the sibling project's owner-approved glossary
already fixed the word for the same concept, and inklkunn uses it so that a person who knows one knows the
other. *Named by the handoff* means handoff 003 fixes it. *Drafted* means the dev team proposes it and the
architect decides. Every Japanese word is a draft until the reviewers sign off; **hard term** marks the ones
where a fluent reader's judgement matters most (a trust label, or a word with two usual translations).

**Case.** Standalone labels, headings, titles and links are in sentence case ("Skip to the main content").
Page titles follow one pattern: *the page · the product* ("Not found · inklkunn"); a page about a wiki will
read *the page · the wiki · the product*.

## 1. Concepts

*In use* says whether a message of the catalog uses the word now; the test holds the *yes* rows to it, in both
languages. The rows marked *not yet* are fixed in advance so that the pages that need them (handoff 003 T4 …
T6, RFC 004) take the word from here and do not choose their own.

| Concept | English | Japanese | In use | Why | Status |
|---|---|---|---|---|---|
| The whole web address that serves wikis | site | サイト | yes | "This site" says where a form must come from without naming a server or a product | Drafted |
| A named collection of pages with its own history (storage: one prikk repository) | wiki | ウィキ | yes | The usual word, and the name of the thing the reader came for | Drafted · ja: hard term |
| One document of a wiki, found by its title | page | ページ | yes | The reader's word; never "file" or "article" | Named by the handoff |
| The name of a page, which is also its address | title | タイトル | yes | A page's title is its address (RFC 003 D-1); the storage path is never the address | Drafted |
| What a reader types in the browser to reach something | address | アドレス | yes | The plain word for what a not-found page asks the reader to check | Drafted |
| A named version of the whole wiki that readers can switch between (storage: a branch) | view | ビュー | yes | A wiki reader does not know "branch"; the default view is the one shown when none is named | Named by the handoff · ja: hard term |
| Everything that happened to a page, newest first | history | 履歴 | yes | The usual word | Reused from planeter |
| One recorded step in a wiki's history: work that was accepted and signed in (storage: a block) | change | 変更 | yes | One word for one thing; never "commit", "block" or "patch" | Reused from planeter · ja: hard term |
| Work someone puts forward for review before it becomes part of a page | proposal | 提案 | yes | "Change" is taken by the recorded step; one word for two things is the confusion this page exists to remove | Reused from planeter · ja: hard term |
| A proposal the wiki service has received and keeps: it is not part of the history yet | held for review | レビュー待ちで保持中 | yes | Says what is true and no more: the service holds the work, nothing is approved for merging. As a label: *Held for review* (RFC 002 D-6) | Named by the handoff · ja: hard term |
| A proposal that an authorized person took into the history | accepted | 受け入れ済み | yes | RFC 002 D-6, WEB-09; "approved" is a reviewer's opinion about one revision and means something else. As a label: *Accepted* | Drafted · ja: hard term |
| A proposal that an authorized person did not take into the history | declined | 見送り済み | yes | RFC 002 D-6, WEB-09; calm, assumes good faith, and the message beside it names the next step. As a label: *Declined* | Drafted · ja: hard term |
| A proposal that has not been sent for review; only its author sees it | draft | 下書き | yes | The usual word, and a respectable state (CM-9): it says that nothing has been asked of anyone. As a label: *Draft* | Drafted · ja: hard term |
| A proposal that was sent back to its proposer for a new revision | needs a revision | 修正が必要 | yes | Says what is true and what the next step is; it does not say the proposal was refused. As a label: *Needs a revision* | Drafted · ja: hard term |
| A proposal that its proposer took back | withdrawn | 取り下げ済み | yes | The proposer's own act, with a reason; calm, and the proposal is kept for the record. As a label: *Withdrawn* | Drafted · ja: hard term |
| A proposal that a later proposal replaced | superseded | 置き換え済み | yes | Says that something took its place, and the page names it. As a label: *Superseded* | Drafted · ja: hard term |
| A comment whose part is no longer in the proposal | orphaned | 孤立した | yes | The comment is kept with the text it was about; the word says only that nothing matches any more (COLL-5) | Drafted · ja: hard term |
| A page that other pages link to and that does not exist yet | wanted page | 未作成のページ | yes | Says what the reader can do about it: write it | Named by the handoff · ja: hard term |
| The label that prikk ran all of its checks on the wiki and found no problems | Verified | 検証済み | yes | A trust label: it never means more than the check found, and never appears without it | Reused from planeter · ja: hard term, trust label |
| The label that prikk checked and found problems | Verification failed | 検証に失敗 | yes | "Unverified" would read as "not checked" | Reused from planeter · ja: hard term, trust label |
| The label for a check that could not run | Could not be checked now | 現在は確認できません | yes | Separates "failed" from "could not check"; it claims nothing either way | Reused from planeter · ja: hard term, trust label |
| The label on a change that was signed into the history | Signed into the history | 署名して履歴に記録済み | yes | Plain for "sealed"; on a change page it must not read as "Verified" | Reused from planeter · ja: hard term, trust label |
| inklkunn itself, as the author of record of every change | the wiki service | ウィキサービス | yes | RFC 002 D-6, WEB-09: "signed by the wiki service on behalf of {proposer}, accepted by {acceptor}" says who signed and for whom, and never implies that a person signed cryptographically | Drafted · ja: hard term |
| A page title that sends the reader to another page | redirect | リダイレクト | yes | The usual word; the page says "redirected from {title}" | Drafted |
| Another title a page also answers to | alias | 別名 | yes | Plain | Drafted |
| An address that always shows a page as it was at one point in its history | permalink | 固定リンク | not yet | The usual word | Drafted |
| The pages that link to this page | backlinks | リンク元 | not yet | The usual word on wikis | Drafted |
| The page exactly as it is stored, as text | raw source | ソース | not yet | The reader's word; it is served from the separate content address | Drafted |
| Where a page's file is kept in the wiki's repository | storage path | 保存先のパス | not yet | Shown only beside the raw source and in the export, and labelled so; a title is the address (RFC 003 D-1) | Drafted |
| A translation no fluent reader has checked yet | preview | プレビュー版 | yes | Said wherever the language is offered, so nobody takes a draft for a finished translation (DR-7) | Drafted |
| The small file a browser keeps for a site | cookie | Cookie | yes | The usual word in both languages; the language choice is the only thing kept for a visitor who is not signed in | Drafted |
| Whoever looks after this installation | the person who runs this server | このサーバーの管理者 | yes | Operator vocabulary stays out of the pages | Reused from planeter |
| A form a page offers to be filled in and sent | form | フォーム | yes | The plain word | Drafted |
| What the language choice is made on | language page | 言語のページ | yes | Names the page the refusal sends the reader to | Drafted |
| A page that no other page links to | orphan page | 孤立したページ | yes | The usual wiki word; the health list says what would resolve it | Drafted |
| A page that links to no other page | dead-end page | 行き止まりのページ | yes | The usual wiki word; the health list says what would resolve it | Drafted |
| The lists of what can be improved in a wiki | wiki health | ウィキの健全性 | yes | Names the lists as opportunities, never as faults (NAV-15) | Drafted |
| When this server first saw a change | first seen here | 初めて確認した日時 | yes | The history carries no time, so the only time shown is the server's own observation, named so it is never read as the time of the change | Drafted |

## 2. Messages

Each row is a catalog key. *English* and *Japanese* are the catalog texts, character for character, placeholders
`{like_this}` included (a counted message would be written `"one" / "other"`). Every catalog message has a row
and every row names a catalog message. *Where it appears* is for the translators and reviewers. A message has no
`|` in it, so the table can be read by the test.

| Key | Where it appears | English | Japanese |
|---|---|---|---|
| `layout.skip` | the first link of every page, shown when it has focus | "Skip to the main content" | "メインコンテンツへ移動" |
| `layout.language` | the name of the language buttons in the footer | "Language" | "言語" |
| `layout.footer` | the footer line of every page | "{product} version {version}" | "{product} バージョン {version}" |
| `title.language` | the title of the language page | "Language · {product}" | "言語 · {product}" |
| `language.heading` | the heading of the language page | "Language" | "言語" |
| `language.intro` | the sentence under the heading of the language page | "Choose the language in which pages are shown. Your choice is kept in a cookie on this device." | "ページを表示する言語を選んでください。選んだ言語は、この端末の Cookie に保存されます。" |
| `language.in_use` | beside the language now in use, on the language page | "In use now." | "現在使用中です。" |
| `language.preview_choice` | the button of a language that is still a preview; `{name}` is the language's name in itself | "{name} (preview)" | "{name}（プレビュー版）" |
| `language.preview_note` | beside a preview language, on the language page | "This translation is a preview. Fluent readers have not checked it yet, so some wording may be wrong." | "この翻訳はプレビュー版です。日本語に堪能な方による確認がまだ終わっていないため、不自然な表現が含まれている場合があります。" |
| `language.back` | the link back to the page the reader came from | "Go back to the page you were on" | "元のページに戻る" |
| `title.not_found` | the title of the not-found page | "Not found · {product}" | "見つかりません · {product}" |
| `not_found.heading` | the heading of the not-found page | "Not found" | "見つかりません" |
| `not_found.body` | the sentence of the not-found page; also the whole answer from the content address | "There is nothing to show at this address. Check the address for typing mistakes." | "このアドレスには表示できるものがありません。アドレスに入力ミスがないか確認してください。" |
| `title.language_refused` | the title of the page that says a language was not changed | "Language not changed · {product}" | "言語は変更されていません · {product}" |
| `language_refused.heading` | its heading | "The language was not changed" | "言語は変更されていません" |
| `language_refused.body` | its sentence | "The form did not name a language that is available, so nothing was changed. Choose a language on the language page." | "フォームで指定された言語は利用できないため、何も変更していません。言語のページで利用できる言語を選んでください。" |
| `language_refused.link` | its link, to the language page | "Go to the language page" | "言語のページへ移動" |
| `title.form_refused` | the title of the page that says a form from another site was refused | "Form not accepted · {product}" | "フォームを受け付けられませんでした · {product}" |
| `form_refused.heading` | its heading | "The form was not accepted" | "フォームを受け付けられませんでした" |
| `form_refused.body` | its sentence | "The form could not be confirmed as coming from a page of this site, so nothing was changed. Go back, reload the page and try again." | "このサイトのページから送信されたものだと確認できなかったため、何も変更していません。前のページに戻り、再読み込みしてからもう一度お試しください。" |
| `page.render_failed` | the plain answer when a page could not be drawn | "This page could not be shown. Try again in a little while. If it keeps happening, tell the person who runs this server." | "このページを表示できませんでした。しばらくしてから、もう一度お試しください。それでも解決しない場合は、このサーバーの管理者にお知らせください。" |
| `misdirected.body` | the plain answer to a request for an address this server does not serve | "This server does not serve pages at that address. Check the address you typed, or tell the person who runs this server." | "このサーバーは、そのアドレスのページを提供していません。入力したアドレスを確認するか、このサーバーの管理者にお知らせください。" |
| `wiki.nav` | the name of the navigation bar of a wiki's pages (a screen reader announces it) | "This wiki" | "このウィキ" |
| `wiki.nav_about` | a link in the navigation bar of a wiki's pages | "About this wiki" | "このウィキについて" |
| `wiki.nav_verification` | a link in the navigation bar of a wiki's pages | "Verification" | "検証" |
| `wiki.nav_wikis` | a link in the navigation bar of a wiki's pages, to the list of wikis | "All wikis" | "ウィキの一覧" |
| `wiki.view_in_use` | under the heading of a page read in a view other than the wiki's default; `{view}` is the view's name | "You are reading the view {view}." | "ビュー「{view}」を表示しています。" |
| `title.page` | the title of a page; `{page}` is the page's title, `{wiki}` the wiki's name | "{page} · {wiki} · {product}" | "{page} · {wiki} · {product}" |
| `page.redirected_from` | under the heading of a page the reader reached through a redirect; `{title}` is the title the reader asked for | "Redirected from {title}." | "「{title}」からリダイレクトされました。" |
| `page.alias_from` | under the heading of a page the reader reached by another title of it; `{title}` is that title | "{title} is another title of this page." | "「{title}」はこのページの別のタイトルです。" |
| `page.matched_loosely` | under the heading of a page whose title only nearly matches what the reader asked for; `{title}` is what was asked | "You asked for {title}. This is the page with the closest title." | "「{title}」を探しています。タイトルが最も近いページを表示しています。" |
| `page.stale` | a notice above a page that is the last version the server has, shown when the newest could not be read | "This page may be out of date. The server could not ask for the newest version just now, so this is the last version it has." | "このページは古い可能性があります。今は最新版を取得できなかったため、サーバーが持っている最後の版を表示しています。" |
| `page.contents` | the name of the list of headings of a page | "On this page" | "このページの内容" |
| `page.license` | under a page; `{license}` is the license of the page, or of the wiki when the page names none | "License: {license}" | "ライセンス: {license}" |
| `missing.heading` | the heading of the page shown for a title no page has | "This page does not exist yet" | "このページはまだありません" |
| `missing.body` | its sentence; `{title}` is the title asked for, `{wiki}` the wiki's name | "No page in {wiki} has the title {title}." | "{wiki} に「{title}」というタイトルのページはありません。" |
| `missing.invite` | its second sentence, an invitation | "You can propose it when proposals arrive with the next release." | "提案の機能が次のリリースで提供されたら、このページを提案できます。" |
| `ambiguous.heading` | the heading of the page that lists the pages a title could mean | "More than one page fits" | "該当するページが複数あります" |
| `ambiguous.body` | its sentence; `{title}` is the title asked for | "Several pages could be the one called {title}. Choose one." | "「{title}」に当たるページが複数あります。ひとつ選んでください。" |
| `redirect.heading` | the heading of the page shown for a redirect that leads nowhere | "This redirect does not lead to a page" | "このリダイレクトの先にはページがありません" |
| `redirect.dangling` | its sentence for a redirect to a title no page has; `{title}` is the redirecting page, `{target}` where it points | "The page {title} sends readers to {target}, and no page has that title." | "ページ「{title}」は「{target}」へリダイレクトしますが、そのタイトルのページはありません。" |
| `redirect.loop` | its sentence for redirects that go round in a circle; `{title}` is the page the chain began at | "The page {title} sends readers round in a circle." | "ページ「{title}」のリダイレクトは、ぐるぐると回り続けます。" |
| `redirect.too_long` | its sentence for a chain of redirects that is too long; `{title}` is the page the chain began at | "The page {title} sends readers through too many other pages." | "ページ「{title}」のリダイレクトは、経由するページが多すぎます。" |
| `unreadable.heading` | the heading of the page shown for a page that exists and cannot be shown | "This page cannot be shown" | "このページは表示できません" |
| `unreadable.not_text` | its sentence when the page's file is not text | "The file of this page is not text that this site can read." | "このページのファイルは、このサイトが読み取れるテキストではありません。" |
| `unreadable.info` | its sentence when the information at the top of the page cannot be read | "The information at the top of this page cannot be read." | "このページの先頭にある情報の部分を読み取れません。" |
| `unreadable.info_long` | its sentence when that block is too long | "The information at the top of this page is too long." | "このページの先頭にある情報の部分が長すぎます。" |
| `unreadable.advice` | its last sentence | "The people who write this wiki can fix it." | "このウィキを書いている人が直すことができます。" |
| `unavailable.heading` | the heading of the page shown when the page could not be read just now | "This page could not be read now" | "今はこのページを読み込めません" |
| `unavailable.body` | its sentence | "Try again in a little while. If it keeps happening, tell the person who runs this server." | "しばらくしてから、もう一度お試しください。それでも解決しない場合は、このサーバーの管理者にお知らせください。" |
| `no_view.heading` | the heading of the page shown for a view the wiki does not have | "There is no such view" | "そのビューはありません" |
| `no_view.body` | its sentence; `{view}` is the name asked for | "This wiki has no view called {view}." | "このウィキに「{view}」というビューはありません。" |
| `invalid_title.heading` | the heading of the page shown for a title that cannot name a page | "That title cannot name a page" | "そのタイトルはページの名前にできません" |
| `invalid_title.body` | its sentence | "A title cannot be empty, cannot have an empty part between slashes, and cannot be longer than the wiki can store." | "タイトルを空にすることはできません。スラッシュで区切った部分を空にすることも、ウィキが保存できる長さを超えることもできません。" |
| `assurance.verified` | the label of a wiki whose last check found nothing wrong | "Verified" | "検証済み" |
| `assurance.verification_failed` | the label of a wiki whose last check found a problem | "Verification failed" | "検証に失敗" |
| `assurance.could_not_be_checked_now` | the label of a wiki whose last check could not be completed, and of a page the server could not confirm | "Could not be checked now" | "現在は確認できません" |
| `assurance.not_checked_yet` | the label of a wiki that has never been checked | "Not checked yet" | "まだ確認していません" |
| `badge.sentence` | under every page of a wiki; `{state}` is one of the four labels above, linked to the verification page | "Verification: {state}" | "検証: {state}" |
| `title.verification` | the title of the verification page | "Verification · {wiki} · {product}" | "検証 · {wiki} · {product}" |
| `verification.heading` | the heading of the verification page | "Verification" | "検証" |
| `explain.verification` | an explanation the reader opens on the verification page | "Verification asks prikk, the program that keeps this wiki's history, to check every part of that history and its signatures. “Verified” means the last check found nothing wrong. It does not mean the pages are correct." | "検証では、このウィキの履歴を保管しているプログラム prikk に、履歴のすべてと署名を確認してもらいます。「検証済み」は、直前の確認で問題が見つからなかったという意味です。ページの内容が正しいという意味ではありません。" |
| `verification.explain_summary` | the label of the explanation above, closed | "What does this mean?" | "これは何を意味しますか" |
| `verification.state_verified` | a sentence on the verification page | "The last check found nothing wrong." | "直前の確認では、問題は見つかりませんでした。" |
| `verification.state_failed` | a sentence on the verification page | "The last check found a problem with this wiki's history. The person who runs this server can look into it." | "直前の確認で、このウィキの履歴に問題が見つかりました。このサーバーの管理者が調べることができます。" |
| `verification.state_could_not` | a sentence on the verification page | "The last check could not be completed. That says nothing about whether the history is sound." | "直前の確認は最後まで実行できませんでした。履歴に問題があるかどうかについては、何も分かっていません。" |
| `verification.state_unknown` | a sentence on the verification page | "This wiki has not been checked yet." | "このウィキはまだ確認されていません。" |
| `verification.checked_at` | a sentence on the verification page; `{when}` is a date and time in UTC | "Last checked: {when}." | "最終確認: {when}。" |
| `verification.check_now` | the button that starts a check | "Check now" | "今すぐ確認" |
| `verification.recent` | shown instead of a new check when one ran a moment ago | "A check ran a moment ago, so no new one was started. Wait a little before asking again." | "つい先ほど確認を実行したため、新しい確認は開始しませんでした。少し待ってからもう一度お試しください。" |
| `verification.found_heading` | the heading of what a check found | "What the check found" | "確認の結果" |
| `verification.found_intro` | above the list of parts of the check that did not come out well | "These parts of the check did not come out well:" | "確認のうち、次の部分で問題がありました:" |
| `verification.now_verified` | the result of a check just run | "The check you asked for found nothing wrong." | "いま実行した確認では、問題は見つかりませんでした。" |
| `verification.not_saved` | after a check whose result could not be saved | "The result could not be saved, so the label on the pages has not changed." | "結果を保存できなかったため、ページに表示されるラベルは変わっていません。" |
| `verification.busy` | the result of a check that could not start because the server was busy | "The server is busy with other work, so the check did not run. Try again in a little while." | "サーバーが他の作業で混み合っているため、確認を実行できませんでした。しばらくしてから、もう一度お試しください。" |
| `verification.limit` | the result of a check that was stopped for want of time or memory | "The check needs more time or memory than this server is set up to give it. Tell the person who runs this server." | "この確認には、このサーバーに設定されている以上の時間またはメモリが必要です。このサーバーの管理者にお知らせください。" |
| `verification.unavailable` | the result of a check that could not be started | "The check could not be started. Tell the person who runs this server." | "確認を開始できませんでした。このサーバーの管理者にお知らせください。" |
| `verification.attempt_at` | on the verification page, when a check made after the last verdict could not be completed; `{when}` is a date and time in UTC | "A later check could not be completed ({when}). That says nothing about whether the history is sound, and the result above still stands." | "その後の確認は最後まで実行できませんでした（{when}）。履歴に問題があるかどうかについては何も分かっておらず、上の結果は変わりません。" |
| `verification.running` | shown instead of a new check when a check of this wiki is already running | "A check of this wiki is already running. Wait for it to finish, then look at this page again." | "このウィキの確認はすでに実行中です。終わるまでお待ちになってから、このページをもう一度ご覧ください。" |
| `title.about` | the title of a wiki's about page | "About {wiki} · {product}" | "{wiki} について · {product}" |
| `about.heading` | the heading of a wiki's about page | "About {wiki}" | "{wiki} について" |
| `about.license` | a sentence on the about page | "Unless a page says otherwise, the pages of this wiki are shared under the license {license}." | "ページに別の記載がない限り、このウィキのページはライセンス {license} で共有されています。" |
| `about.view` | a sentence on the about page; `{view}` is the wiki's default view | "The view shown when none is named is {view}." | "ビューを指定しない場合に表示されるのは「{view}」です。" |
| `about.accessibility_heading` | the heading of the accessibility statement, on the about page | "Accessibility statement" | "アクセシビリティに関する声明" |
| `about.accessibility_tools` | the first sentence of the statement | "Automated tests check every kind of page on this site for one main heading, landmarks, a link that skips to the content, labels, text alternatives, language and direction, contrast in the light and dark schemes, visible keyboard focus and large enough targets." | "このサイトのすべての種類のページについて、自動テストが次の点を確認しています: メインの見出しが一つであること、ランドマーク、本文へ移動するリンク、ラベル、代替テキスト、言語と文字の方向、明るい配色と暗い配色でのコントラスト、キーボードフォーカスの表示、十分な大きさの操作対象。" |
| `about.accessibility_people` | the second sentence of the statement | "No person who uses assistive technology has tested this site yet. If something does not work for you, tell the person who runs this server." | "支援技術を使っている方による、このサイトの確認はまだ行われていません。うまく使えないところがあれば、このサーバーの管理者にお知らせください。" |
| `title.wikis` | the title of the list of wikis | "Wikis · {product}" | "ウィキ · {product}" |
| `wikis.heading` | the heading of the list of wikis | "Wikis" | "ウィキ" |
| `wikis.intro` | under that heading | "These are the wikis you can read here." | "ここで読めるウィキの一覧です。" |
| `wikis.none` | shown instead of the list when there is none | "There are no wikis you can read here yet." | "ここで読めるウィキはまだありません。" |
| `transclusion.from` | above the text of a page included in another; `{title}` is the included page's title | "From {title}" | "{title} から" |
| `transclusion.cycle` | in place of an included page that would include itself; `{chain}` is the pages in the circle | "Not included, it would include itself: {chain}" | "含めません。自分自身を含むことになります: {chain}" |
| `transclusion.too_deep` | in place of an included page that is too deep; `{title}` is its title, `{limit}` the depth allowed | "Not included, deeper than {limit}: {title}" | "含めません。{limit} 段より深くなります: {title}" |
| `transclusion.over_budget` | in place of an included page when a page has included as much as it may; `{title}` is its title | "Not included, a page includes only so much: {title}" | "含めません。1 ページに含められる量には上限があります: {title}" |
| `title.method_not_allowed` | the title of the page shown when the address does not take that kind of request | "Not possible here · {product}" | "ここではできません · {product}" |
| `method_not_allowed.heading` | its heading | "That is not possible at this address" | "このアドレスではその操作はできません" |
| `method_not_allowed.body` | its sentence | "This address can be opened, but not used in that way. Go back and use the links on the page." | "このアドレスは開くことはできますが、その使い方はできません。前のページに戻って、ページ内のリンクをお使いください。" |
| `problem.not_found.title` | the API's problem detail `not-found`: the title | "Not found" | "見つかりません" |
| `problem.not_found.detail` | the API's problem detail `not-found`: the detail | "There is nothing to show at this address." | "このアドレスには表示できるものがありません。" |
| `problem.not_found.hint` | the API's problem detail `not-found`: the hint | "Check the address for typing mistakes." | "アドレスに入力ミスがないか確認してください。" |
| `problem.page_not_found.title` | the API's problem detail `page-not-found`: the title | "Page not found" | "ページが見つかりません" |
| `problem.page_not_found.detail` | the API's problem detail `page-not-found`: the detail | "The wiki has no page with this title." | "このウィキには、このタイトルのページがありません。" |
| `problem.page_not_found.hint` | the API's problem detail `page-not-found`: the hint | "Check the title, or list the pages of the wiki to find it." | "タイトルを確認するか、ウィキのページの一覧から探してください。" |
| `problem.ambiguous_title.title` | the API's problem detail `ambiguous-title`: the title | "Several pages match" | "複数のページが該当します" |
| `problem.ambiguous_title.detail` | the API's problem detail `ambiguous-title`: the detail | "More than one page matches this title, and none is nearer." | "このタイトルに該当するページが複数あり、どれが最も近いか決められません。" |
| `problem.ambiguous_title.hint` | the API's problem detail `ambiguous-title`: the hint | "Ask for one of the pages listed under candidates, by its exact title." | "candidates に挙がっているページのうち、ひとつを正確なタイトルで指定してください。" |
| `problem.redirect_broken.title` | the API's problem detail `redirect-broken`: the title | "The redirect cannot be followed" | "リダイレクトをたどれません" |
| `problem.redirect_broken.detail` | the API's problem detail `redirect-broken`: the detail | "This title is a redirect that does not lead to a page." | "このタイトルは、ページにつながっていないリダイレクトです。" |
| `problem.redirect_broken.hint` | the API's problem detail `redirect-broken`: the hint | "Ask the people who look after the wiki to fix the redirect, or ask for the page it should lead to." | "ウィキを管理する方にリダイレクトの修正を依頼するか、行き先のはずのページを直接指定してください。" |
| `problem.page_unreadable.title` | the API's problem detail `page-unreadable`: the title | "The page cannot be shown" | "ページを表示できません" |
| `problem.page_unreadable.detail` | the API's problem detail `page-unreadable`: the detail | "The page exists, but its stored text cannot be read as a page." | "ページはありますが、保存されている内容をページとして読み取れません。" |
| `problem.page_unreadable.hint` | the API's problem detail `page-unreadable`: the hint | "Ask the people who write the page to correct it." | "ページを書いている方に修正を依頼してください。" |
| `problem.invalid_title.title` | the API's problem detail `invalid-title`: the title | "Title not accepted" | "タイトルを受け付けられません" |
| `problem.invalid_title.detail` | the API's problem detail `invalid-title`: the detail | "This title cannot name a page." | "このタイトルでは、ページを指定できません。" |
| `problem.invalid_title.hint` | the API's problem detail `invalid-title`: the hint | "Use a title that is not empty, not too long, and has no empty part between its slashes." | "空でなく、長すぎず、スラッシュの間に空の部分がないタイトルを使ってください。" |
| `problem.no_such_view.title` | the API's problem detail `no-such-view`: the title | "View not found" | "ビューが見つかりません" |
| `problem.no_such_view.detail` | the API's problem detail `no-such-view`: the detail | "The wiki has no open view named {view}." | "このウィキには、{view} という名前の開いているビューがありません。" |
| `problem.no_such_view.hint` | the API's problem detail `no-such-view`: the hint | "Leave the view out to use the default view of the wiki." | "ビューを指定しなければ、ウィキの既定のビューが使われます。" |
| `problem.invalid_parameter.title` | the API's problem detail `invalid-parameter`: the title | "Parameter not accepted" | "パラメーターを受け付けられません" |
| `problem.invalid_parameter.detail` | the API's problem detail `invalid-parameter`: the detail | "The parameter {parameter} is not accepted as it was given." | "パラメーター {parameter} は、指定された形では受け付けられません。" |
| `problem.invalid_parameter.hint` | the API's problem detail `invalid-parameter`: the hint | "Check the API description for what {parameter} may be." | "{parameter} に指定できるものは、API の説明で確認してください。" |
| `problem.diff_path_required.title` | the API's problem detail `diff-path-required`: the title | "A diff needs a path" | "差分にはパスが必要です" |
| `problem.diff_path_required.detail` | the API's problem detail `diff-path-required`: the detail | "A diff is always for one page, so the parameter path is required." | "差分は常にひとつのページについて求めるため、パラメーター path は必須です。" |
| `problem.diff_path_required.hint` | the API's problem detail `diff-path-required`: the hint | "Give the storage path of the page, as the raw endpoint reports it." | "raw のエンドポイントが返す、ページの保存先のパスを指定してください。" |
| `problem.invalid_cursor.title` | the API's problem detail `invalid-cursor`: the title | "Cursor not accepted" | "カーソルを受け付けられません" |
| `problem.invalid_cursor.detail` | the API's problem detail `invalid-cursor`: the detail | "The cursor is not one this server gave out." | "このサーバーが発行したものではないカーソルです。" |
| `problem.invalid_cursor.hint` | the API's problem detail `invalid-cursor`: the hint | "Start again from the first page, without a cursor." | "カーソルを付けずに、最初のページからやり直してください。" |
| `problem.not_available.title` | the API's problem detail `not-available`: the title | "Not available in this build" | "このビルドでは利用できません" |
| `problem.not_available.detail` | the API's problem detail `not-available`: the detail | "This build of the server does not offer this yet." | "このサーバーのビルドでは、まだ提供されていません。" |
| `problem.not_available.hint` | the API's problem detail `not-available`: the hint | "Check the API description of this server for what it offers." | "このサーバーが提供するものは、API の説明で確認してください。" |
| `problem.unavailable.title` | the API's problem detail `unavailable`: the title | "Cannot be read now" | "現在は読み取れません" |
| `problem.unavailable.detail` | the API's problem detail `unavailable`: the detail | "The server could not read the wiki just now." | "サーバーが今、ウィキを読み取れませんでした。" |
| `problem.unavailable.hint` | the API's problem detail `unavailable`: the hint | "Try again in a little while." | "しばらくしてから、もう一度お試しください。" |
| `problem.failed.title` | the API's problem detail `failed`: the title | "The request failed" | "リクエストに失敗しました" |
| `problem.failed.detail` | the API's problem detail `failed`: the detail | "Something went wrong on the server, so the request was not served." | "サーバー側で問題が起きたため、リクエストを処理できませんでした。" |
| `problem.failed.hint` | the API's problem detail `failed`: the hint | "Try again in a little while, and tell the person who runs this server if it keeps happening." | "しばらくしてからもう一度お試しになり、続く場合はこのサーバーの管理者にお知らせください。" |
| `problem.method_not_allowed.title` | the API's problem detail `method-not-allowed`: the title | "Method not allowed" | "このメソッドは使えません" |
| `problem.method_not_allowed.detail` | the API's problem detail `method-not-allowed`: the detail | "This release of the API only reads, so this address does not take that method." | "この版の API は読み取り専用のため、このアドレスではそのメソッドを使えません。" |
| `problem.method_not_allowed.hint` | the API's problem detail `method-not-allowed`: the hint | "Ask with GET or HEAD instead." | "GET または HEAD で求めてください。" |
| `page.link_history` | under a page, the link to its history | "History of this page" | "このページの履歴" |
| `title.history` | the title of a page's history; `{page}` is the page's title, `{wiki}` the wiki's name | "History of {page} · {wiki} · {product}" | "{page} の履歴 · {wiki} · {product}" |
| `history.heading` | the heading of a page's history; `{title}` is the page's title | "History of {title}" | "{title} の履歴" |
| `history.intro` | the sentence under the heading, above the list of changes | "These changes touched this page. The newest is first." | "このページに関わった変更です。新しい順に並んでいます。" |
| `history.current` | a link from the history, the difference and the permalink to the page itself; `{title}` is its title | "The current version of {title}" | "{title} の現在の版" |
| `history.none` | in place of the list when no change touched the page | "No change to this page was found." | "このページに関わる変更は見つかりませんでした。" |
| `history.none_so_far` | in place of the list when none of the changes read so far touched the page and older ones remain | "No change to this page was found among those read so far. Older changes may have touched it." | "ここまでに読み込んだ変更の中に、このページに関わるものは見つかりませんでした。さらに古い変更に含まれている可能性があります。" |
| `history.older` | the link under the list that goes on to older changes | "Look at older changes" | "さらに古い変更を見る" |
| `history.beyond` | under the list when the wiki has more changes than are read at once | "This wiki has more changes than are read at once. The oldest are not listed here." | "このウィキには一度に読み込める数より多くの変更があります。いちばん古い変更はここには表示されません。" |
| `history.change_label` | the name of one change, a link to its page; `{number}` is its place in the history, counting from the first | "Change {number}" | "変更 {number}" |
| `history.link_diff` | a link from a change in a page's history to what that change did to the page; `{number}` as above | "What change {number} did to this page" | "変更 {number} でこのページがどう変わったか" |
| `history.link_version` | a link from a change in a page's history to the page as it was after that change (a permalink); `{number}` as above | "The page after change {number}" | "変更 {number} の後のページ" |
| `people.proposed_by` | beside a change: who put the change forward; `{name}` is a person's name, an anonymous contributor or the wiki service | "Proposed by {name}" | "提案者: {name}" |
| `people.accepted_by` | beside a change: who took it into the history; `{name}` as above | "Accepted by {name}" | "受け入れた人: {name}" |
| `people.made_by` | beside a change the wiki service made itself (a start, an import); `{name}` is the wiki service | "Made by {name}" | "{name} による変更" |
| `people.service` | a name in the three messages above and in the signed line of a change: the wiki service itself | "the wiki service" | "ウィキサービス" |
| `people.anonymous` | a name in the messages above: someone who proposed without an account; `{token}` is the temporary name the wiki gave them | "an anonymous contributor ({token})" | "匿名の投稿者（{token}）" |
| `people.unrecorded` | beside a change whose record does not name the people | "The record of this change does not say who made it." | "この変更の記録には、誰が行ったかが書かれていません。" |
| `flag.undo` | a mark on a change that undoes earlier changes | "Undoes earlier changes" | "以前の変更を取り消します" |
| `flag.import` | a mark on a change that brought in content from another source | "Import" | "取り込み" |
| `flag.service` | a mark on a change the wiki service made itself | "Made by the wiki service itself" | "ウィキサービス自身が行った変更" |
| `flag.first` | a mark on the first change of a wiki | "The first change of this wiki" | "このウィキの最初の変更" |
| `event.created` | what a change did to the page whose history is shown | "The page was created" | "ページが作成されました" |
| `event.edited` | as above | "The page was edited" | "ページが編集されました" |
| `event.deleted` | as above | "The page was removed" | "ページが削除されました" |
| `event.moved_from` | as above; `{title}` is the title the page had before | "The page was moved here from {title}" | "ページは {title} から移動されました" |
| `event.moved_to` | as above; `{title}` is the title the page has now | "The page was moved to {title}" | "ページは {title} へ移動されました" |
| `event.not_inspected` | in place of the above when the change is too large to list what it did | "This change is too large to list what it did, so it may have touched this page." | "この変更は大きすぎて内容を一覧できないため、このページに関わっている可能性があります。" |
| `title.change` | the title of a change's page; `{number}` is its place in the history | "Change {number} · {wiki} · {product}" | "変更 {number} · {wiki} · {product}" |
| `change.heading` | the heading of a change's page | "Change {number}" | "変更 {number}" |
| `change.signed` | the label on a change's page: it was signed into the history. It says nothing about who signed or whether the history was checked | "Signed into the history" | "署名して履歴に記録済み" |
| `change.signed_by` | under the label above, for a change that came from a proposal | "Signed by the wiki service on behalf of {proposer}, accepted by {acceptor}." | "ウィキサービスが {proposer} に代わって署名し、{acceptor} が受け入れました。" |
| `change.signed_service` | under the label above, for a change the wiki service made itself | "Signed by the wiki service." | "ウィキサービスが署名しました。" |
| `change.did_heading` | the heading over what a change did | "What this change did" | "この変更の内容" |
| `change.did_none` | in place of the list when the change has nothing to list | "This change has nothing to list." | "この変更に一覧できる内容はありません。" |
| `change.did_added` | one thing a change did; `{title}` is a link to the page | "Added the page {title}" | "ページ {title} を追加しました" |
| `change.did_edited` | as above | "Edited the page {title}" | "ページ {title} を編集しました" |
| `change.did_removed` | as above | "Removed the page {title}" | "ページ {title} を削除しました" |
| `change.did_moved` | as above; `{from}` and `{to}` are the old and the new title | "Moved the page {from} to {to}" | "ページ {from} を {to} へ移動しました" |
| `change.did_file_added` | as above, for something that is not a page (an attachment, a manifest) | "Added an attachment or another file that is not a page" | "ページではない添付ファイルなどを追加しました" |
| `change.did_file_edited` | as above | "Edited an attachment or another file that is not a page" | "ページではない添付ファイルなどを編集しました" |
| `change.did_file_removed` | as above | "Removed an attachment or another file that is not a page" | "ページではない添付ファイルなどを削除しました" |
| `change.did_file_moved` | as above, when a move involves something that is not a page | "Moved an attachment or another file that is not a page" | "ページではない添付ファイルなどを移動しました" |
| `change.did_more` | under the list when the change did more than is listed; `{count}` is how many more | "{count} more item is not listed." / "{count} more items are not listed." | "ほかに {count} 件は一覧に含まれていません。" |
| `change.too_large` | in place of the list when the change is too large to describe | "This change is too large to list what it did." | "この変更は大きすぎて、内容を一覧できません。" |
| `change.link_diff` | beside an edited page in a change: a link to what the change did to that page; `{title}` is its title | "What this change did to {title}" | "この変更で {title} がどう変わったか" |
| `change_none.heading` | the heading of the page for a change, a difference or a permalink that this wiki does not have | "There is no such change" | "そのような変更はありません" |
| `change_none.body` | its sentence | "This wiki has no change at this address. Check the address, or open the history of a page and follow a link from there." | "このアドレスに対応する変更は、このウィキにはありません。アドレスを確認するか、ページの履歴を開いてリンクをたどってください。" |
| `title.diff` | the title of a page's difference; `{page}` is the page's title | "Changes to {page} · {wiki} · {product}" | "{page} の変更点 · {wiki} · {product}" |
| `diff.heading` | the heading of a page's difference | "Changes to {title}" | "{title} の変更点" |
| `diff.from_to` | under the heading: what is compared; `{from}` and `{to}` are links to the two changes | "From {from} to {to}." | "{from} から {to} まで。" |
| `diff.summary_heading` | the heading of the text summary of a difference | "Summary" | "概要" |
| `diff.summary_added` | a line of the summary: how many lines the difference adds | "{count} line added" / "{count} lines added" | "{count} 行追加" |
| `diff.summary_removed` | a line of the summary: how many lines the difference removes | "{count} line removed" / "{count} lines removed" | "{count} 行削除" |
| `diff.summary_none` | in place of the summary when no line differs | "No line differs." | "異なる行はありません。" |
| `diff.status_added` | under the summary: the page is new at the second point | "The page did not exist at the first point and exists at the second." | "このページは、比較の前の時点には存在せず、後の時点に存在します。" |
| `diff.status_removed` | under the summary: the page is gone at the second point | "The page existed at the first point and does not exist at the second." | "このページは、比較の前の時点に存在し、後の時点には存在しません。" |
| `diff.status_unchanged` | under the summary: nothing differs | "The page is the same at both points." | "このページは、どちらの時点でも同じです。" |
| `diff.no_lines` | under the summary: the lines cannot be shown | "The lines of this page cannot be shown. It may not be text, or it could not be compared." | "このページの行は表示できません。テキストではないか、比較できなかった可能性があります。" |
| `diff.truncated` | under the lines when the difference is longer than is shown; `{limit}` is how many lines are shown | "Only the first {limit} lines are shown." | "最初の {limit} 行だけを表示しています。" |
| `diff.refinement` | above the lines: where the marked words come from | "Marked words are found by inklkunn, which compares the changed lines word by word. The lines themselves come from prikk." | "印の付いた語は、inklkunn が変更された行を語ごとに比べて見つけたものです。行そのものは prikk が出力したものです。" |
| `diff.hunk_near` | the heading of one group of changed lines; `{line}` is the line of the page where the group begins | "Changes near line {line}" | "{line} 行目付近の変更" |
| `diff.hunk_plain` | as above, when the line is not known | "Changes" | "変更" |
| `diff.hunk_raw` | above a group of lines that could not be read as changes and is shown as it is | "These lines could not be read as a list of changes. They are shown as they are." | "この部分は変更の一覧として読み取れませんでした。そのまま表示しています。" |
| `diff.added_label` | read aloud before an added line (not shown on screen) | "Added:" | "追加:" |
| `diff.removed_label` | read aloud before a removed line (not shown on screen) | "Removed:" | "削除:" |
| `diff.no_newline` | in place of a line that says the text ends without a line break | "The text ends here without a line break." | "ここでテキストは改行なしで終わっています。" |
| `title.at` | the title of a page as it was after a change (a permalink); `{page}` is its title, `{number}` the change | "{page} after change {number} · {wiki} · {product}" | "変更 {number} の後の {page} · {wiki} · {product}" |
| `at.banner` | the banner above a page shown as it was after a change; `{change}` is a link to that change | "This is the page as it was after {change}." | "これは {change} の後のページです。" |
| `at.current` | under the banner above; `{current}` is a link to the page as it is now | "Go to {current}." | "{current} へ移動できます。" |
| `at.current_label` | the words of the link in the message above | "the current version of this page" | "このページの現在の版" |
| `at.links` | the last line of the banner above; says that the links in the page lead to the pages as they are now | "The links in this page lead to the pages as they are now, not as they were then." | "このページのリンクは、そのときのページではなく、現在のページに移動します。" |
| `at.missing_heading` | the heading of the page for a permalink to a point before the page existed, or after it was removed | "The page did not exist then" | "そのときこのページはありませんでした" |
| `at.missing_body` | its sentence; `{title}` is the title asked for, `{change}` the name of the change, which a link under it opens | "After {change}, this wiki had no page titled {title}." | "{change} の後、このウィキに {title} というタイトルのページはありませんでした。" |
| `wiki.nav_changes` | a link in the navigation bar of a wiki's pages, to the recent changes | "Recent changes" | "最近の変更" |
| `wiki.nav_search` | a link in the navigation bar of a wiki's pages, to the search | "Search" | "検索" |
| `wiki.nav_health` | a link in the navigation bar of a wiki's pages, to the lists of what can be improved | "Wiki health" | "ウィキの健全性" |
| `title.changes` | the title of the recent changes page | "Recent changes · {wiki} · {product}" | "最近の変更 · {wiki} · {product}" |
| `changes.heading` | the heading of the recent changes page | "Recent changes" | "最近の変更" |
| `changes.intro` | under the heading of the recent changes page; `{wiki}` is the wiki's name | "The changes recorded in the history of {wiki}, newest first." | "{wiki} の履歴に記録された変更を、新しい順に表示しています。" |
| `changes.seen_note` | under the recent changes: why the time shown is not the time of the change | "The history does not record when a change was made, so each change shows when this server first saw it." | "履歴には変更を行った日時が記録されないため、各変更にはこのサーバーが最初に確認した日時を表示しています。" |
| `changes.filter_legend` | the name of the group of filters on the recent changes page | "Choose which changes to show" | "表示する変更を選ぶ" |
| `changes.filter_namespace` | the label of the field that limits the changes to the pages under a title | "Only pages under this title" | "このタイトルの下にあるページのみ" |
| `changes.filter_namespace_hint` | the hint of that field | "For example Guides shows changes to Guides/Setup. Leave it empty for all pages." | "たとえば Guides と入力すると、Guides/Setup への変更が表示されます。空欄にするとすべてのページが対象になります。" |
| `changes.filter_view` | the label of the view choice, and the heading of the column that names a change's view | "View" | "ビュー" |
| `changes.all_views` | the choice that shows the changes of every view | "All views" | "すべてのビュー" |
| `changes.filter_automated` | the label of the choice about changes the wiki service made itself (imports, creating the wiki) | "Changes made by the wiki service itself or with the help of a machine" | "ウィキサービス自身または機械の補助による変更" |
| `changes.opt_show` | a choice: show those changes together with the others | "Show them" | "表示する" |
| `changes.opt_hide` | a choice: leave those changes out | "Hide them" | "表示しない" |
| `changes.opt_only` | a choice: show only those changes | "Show only them" | "それらのみ表示する" |
| `changes.filter_apply` | the button of the filters | "Show these changes" | "この条件で表示" |
| `changes.active_heading` | the heading of the list of filters in use, shown when the list is filtered | "The list is filtered" | "一覧は絞り込まれています" |
| `changes.active_namespace` | an item of the filters in use; `{namespace}` is the title typed | "Only pages under {namespace}" | "{namespace} の下にあるページのみ" |
| `changes.active_page` | an item of the filters in use on a page's own list of changes; `{page}` is its title | "Only changes to the page {page}" | "ページ {page} への変更のみ" |
| `changes.active_view` | an item of the filters in use; `{view}` is the view's name | "Only the view {view}" | "ビュー {view} のみ" |
| `changes.active_hide` | an item of the filters in use | "Changes made by the wiki service itself or with the help of a machine are hidden" | "ウィキサービス自身または機械の補助による変更は表示していません" |
| `changes.active_only` | an item of the filters in use | "Only changes made by the wiki service itself or with the help of a machine" | "ウィキサービス自身または機械の補助による変更のみ" |
| `changes.clear` | a link that removes every filter | "Show all changes" | "すべての変更を表示" |
| `changes.caption` | the caption of the table of changes | "Changes, newest first" | "変更の一覧（新しい順）" |
| `changes.col_change` | the heading of the column with each change's summary | "Change" | "変更" |
| `changes.col_people` | the heading of the column that says who made each change | "People" | "関わった人" |
| `changes.col_pages` | the heading of the column that lists the pages each change touched | "Pages" | "ページ" |
| `changes.col_seen` | the heading of the column with the time this server first saw each change | "First seen here" | "初めて確認した日時" |
| `changes.proposed_by` | in a change's row; `{person}` is who proposed it | "Proposed by {person}" | "提案者: {person}" |
| `changes.accepted_by` | in a change's row; `{person}` is who accepted it | "Accepted by {person}" | "受け入れた人: {person}" |
| `changes.made_by` | in the row of a change nobody proposed; `{person}` is who made it | "Made by {person}" | "変更した人: {person}" |
| `changes.person_anonymous` | a person in a change's row; `{token}` is the temporary name an anonymous visitor was given | "an anonymous visitor ({token})" | "匿名の訪問者（{token}）" |
| `changes.person_service` | a person in a change's row; `{label}` is the name the service gave itself | "{label}, the wiki service" | "ウィキサービス（{label}）" |
| `changes.foreign` | in the row of a change that was not recorded through this service | "Recorded outside this service, so the people are not known." | "このサービスの外で記録されたため、関わった人は不明です。" |
| `changes.rollback` | in the row of a change that undoes earlier ones | "Undoes earlier changes" | "以前の変更を取り消す変更" |
| `changes.no_pages` | in the pages column of a change that touched no page | "No pages" | "ページなし" |
| `changes.pages_unavailable` | in the pages column when the pages of a change could not be read | "The pages of this change could not be read just now." | "この変更のページを今は取得できませんでした。" |
| `changes.page_gone` | a page a change touched that is no longer there; `{title}` is its title | "{title} (no longer there)" | "{title}（現在は存在しません）" |
| `changes.none` | in place of the table when there is no change to show | "There are no changes to show." | "表示できる変更はありません。" |
| `changes.more` | under the table when older changes exist than the ones looked through | "Older changes exist than these. Only the newest changes were looked through." | "ここに表示されたより古い変更があります。確認したのは新しい変更のみです。" |
| `changes.pages_missing` | under the table when some changes could not be matched to their pages | "Some changes could not be matched to their pages just now, so this list may miss changes." | "一部の変更はページとの対応を今は確認できなかったため、この一覧には載っていない変更がある可能性があります。" |
| `changes.hidden` | under the table when changes to pages the reader may not read were left out | "Changes to pages you may not read are not shown." | "閲覧できないページへの変更は表示していません。" |
| `changes.feed` | a link to the Atom feed of the changes shown | "Follow these changes in a feed reader (Atom)" | "これらの変更をフィードリーダーで購読する（Atom）" |
| `feed.title_wiki` | the title of the feed of a wiki's changes; `{wiki}` is its name | "Recent changes · {wiki}" | "最近の変更 · {wiki}" |
| `feed.title_namespace` | the title of the feed of the changes under a title | "Recent changes under {namespace} · {wiki}" | "{namespace} の下の最近の変更 · {wiki}" |
| `feed.title_page` | the title of the feed of one page's changes | "Changes to {page} · {wiki}" | "{page} への変更 · {wiki}" |
| `feed.pages` | the text of a feed entry; `{pages}` is the titles of the pages the change touched | "Pages: {pages}" | "ページ: {pages}" |
| `title.search` | the title of the search page | "Search · {wiki} · {product}" | "検索 · {wiki} · {product}" |
| `search.heading` | the heading of the search page | "Search" | "検索" |
| `search.label` | the label of the search field | "Search the titles and aliases of pages" | "ページのタイトルと別名を検索" |
| `search.button` | the button of the search form | "Search" | "検索する" |
| `search.scope` | under the search field: what the search looks through | "Search finds pages by their titles and aliases. Searching the text of pages arrives in a later release." | "検索の対象はページのタイトルと別名です。ページ本文の検索は、今後のリリースで提供されます。" |
| `search.empty` | on the search page before anything was searched for | "Type a title, or part of one, to search for." | "検索するタイトル、またはその一部を入力してください。" |
| `search.results_heading` | the heading of the results; `{query}` is what was searched for | "Pages found for {query}" | "「{query}」で見つかったページ" |
| `search.matched_title` | before the part of a result that matched, when it is the page's title | "Title:" | "タイトル:" |
| `search.matched_alias` | before the part of a result that matched, when it is one of the page's aliases | "Alias:" | "別名:" |
| `search.none` | when nothing was found; `{query}` is what was searched for | "No page has {query} in its title or aliases." | "タイトルにも別名にも「{query}」を含むページはありません。" |
| `search.cut` | under the results when only the best matches are listed | "Only the best matches are listed. Type more of the title to narrow the search." | "一致度の高いものだけを表示しています。タイトルをもう少し入力すると絞り込めます。" |
| `search.exact` | when the search words name a page that exists | "A page with this title exists already." | "このタイトルのページはすでにあります。" |
| `search.similar_heading` | the heading of the pages whose titles resemble what was searched for, shown before the offer to create it | "Pages with similar titles" | "タイトルが似ているページ" |
| `search.create_heading` | the heading of the offer to create the page searched for | "Create this page" | "このページを作成する" |
| `search.create` | the label of a button that cannot be pressed yet; `{title}` is the title searched for | "Create “{title}”" | "「{title}」を作成する" |
| `opensearch.description` | the description of the search in a browser's list of search engines; `{wiki}` is the wiki's name | "Search the titles and aliases of {wiki}" | "{wiki} のページをタイトルと別名で検索" |
| `title.health` | the title of the page that lists what can be improved | "Wiki health · {wiki} · {product}" | "ウィキの健全性 · {wiki} · {product}" |
| `health.heading` | the heading of that page; `{wiki}` is the wiki's name | "Health of {wiki}" | "{wiki} の健全性" |
| `health.intro` | under that heading | "These lists show where the wiki can be improved. Each row says what would resolve it." | "これらの一覧は、ウィキを改善できる箇所を示します。各行には、解決するための対応が書かれています。" |
| `health.wanted_name` | the name of the list of titles that are linked and have no page | "Wanted pages" | "未作成のページ" |
| `health.wanted_desc` | what that list shows | "A wanted page is a title that pages link to and that no page has yet." | "ページからリンクされているのに、まだ存在しないタイトル。" |
| `health.orphans_name` | the name of the list of pages nothing links to | "Orphan pages" | "孤立したページ" |
| `health.orphans_desc` | what that list shows | "An orphan page is a page that no other page links to." | "ほかのどのページからもリンクされていないページ。" |
| `health.dead_ends_name` | the name of the list of pages that link to nothing | "Dead-end pages" | "行き止まりのページ" |
| `health.dead_ends_desc` | what that list shows | "A dead-end page is a page that links to no other page." | "ほかのどのページにもリンクしていないページ。" |
| `health.redirects_name` | the name of the list of redirects with a problem | "Redirect problems" | "リダイレクトの問題" |
| `health.redirects_desc` | what that list shows | "Redirects that lead nowhere, go round in a circle, run too long, or lead to another redirect." | "行き先がない、循環している、長すぎる、または別のリダイレクトに続くリダイレクト。" |
| `health.help_name` | the name of the list of pages marked as needing work | "Pages that want help" | "手助けが必要なページ" |
| `health.help_desc` | what that list shows | "Pages marked as work in progress, draft or stub." | "作業中、下書き、またはスタブと示されているページ。" |
| `health.stale_name` | the name of the list of pages that changed longest ago | "Pages unchanged longest" | "長く変更されていないページ" |
| `health.stale_desc` | what that list shows | "Pages whose latest change is the oldest among the recent changes." | "最近の変更の中で、最後に変更された時期がいちばん古いページ。" |
| `health.back` | a link from one list back to the page that names them all | "All health lists" | "すべての一覧" |
| `health.col_page` | the heading of the column with a page's title | "Page" | "ページ" |
| `health.col_wanted` | the heading of the column with a wanted title | "Wanted title" | "未作成のタイトル" |
| `health.col_linked` | the heading of the column that counts the pages linking to a title | "Pages linking to it" | "リンクしているページ数" |
| `health.col_action` | the heading of the column that says what would resolve each row | "What would resolve it" | "解決するための対応" |
| `health.col_target` | the heading of the column with a redirect's target | "Points to" | "リダイレクト先" |
| `health.col_problem` | the heading of the column that names a redirect's problem | "Problem" | "問題" |
| `health.col_mark` | the heading of the column that says how a page is marked | "Marked as" | "表示されている状態" |
| `health.col_changed` | the heading of the column that says when a page last changed | "Last change" | "最後の変更" |
| `health.see_links` | a link to the pages that link to a wanted title; `{title}` is that title | "Pages that link to {title}" | "{title} へのリンク元のページ" |
| `health.wanted_action` | the action beside a wanted title | "Add a page with this title, or change the links that point to it." | "このタイトルのページを追加するか、リンクを修正します。" |
| `health.orphans_action` | the action beside an orphan page | "Link to this page from a related page, so that readers can find it." | "関連するページからこのページへリンクして、読者が見つけられるようにします。" |
| `health.dead_ends_action` | the action beside a dead-end page | "Add links from this page to related pages." | "このページから関連するページへのリンクを追加します。" |
| `health.problem_broken` | a redirect's problem | "The target is not a page." | "リダイレクト先がページではありません。" |
| `health.problem_loop` | a redirect's problem | "The redirects lead round in a circle." | "リダイレクトが循環しています。" |
| `health.problem_long` | a redirect's problem | "The chain of redirects is too long." | "リダイレクトの連鎖が長すぎます。" |
| `health.problem_double` | a redirect's problem | "The target is itself a redirect." | "リダイレクト先がさらに別のリダイレクトです。" |
| `health.action_broken` | the action beside a redirect whose target is no page | "Point it at a page that exists, or remove the redirect." | "存在するページを指すように直すか、リダイレクトを削除します。" |
| `health.action_loop` | the action beside redirects that go round in a circle | "Make one of the redirects point at a real page." | "どれか一つのリダイレクトを、実在するページに向けます。" |
| `health.action_long` | the action beside a chain of redirects that is too long | "Point the first redirect straight at the final page." | "最初のリダイレクトを最終的なページに直接向けます。" |
| `health.action_double` | the action beside a redirect to a redirect | "Point this redirect straight at the final page." | "このリダイレクトを最終的なページに直接向けます。" |
| `health.mark_wip` | how a page is marked | "Work in progress" | "作業中" |
| `health.mark_draft` | how a page is marked | "Draft" | "下書き" |
| `health.mark_stub` | how a page is marked | "Stub" | "スタブ" |
| `health.help_action` | the action beside a page that wants help | "Finish or improve the page, then remove the mark." | "ページを仕上げるか改善してから、その表示を外します。" |
| `health.stale_never` | in the last change column of a page the recent changes did not touch | "Not changed in the recent changes looked through" | "確認した最近の変更の中では変更されていません" |
| `health.stale_seen` | in the last change column; `{when}` is when this server first saw the change | "In a change first seen {when}" | "{when} に初めて確認された変更" |
| `health.stale_action` | the action beside a page unchanged for long | "Read the page, and update it if it is out of date." | "ページを読み、古くなっていれば更新します。" |
| `health.stale_window` | under the list of pages unchanged longest; `{limit}` is how many of the newest changes were looked through | "The history does not say when a page last changed, so this list looks only through the newest {limit} changes." | "履歴にはページが最後に変更された時期が記録されないため、この一覧では新しい {limit} 件の変更だけを確認しています。" |
| `health.none_wanted` | in place of the list of wanted pages when there are none | "No page is wanted: every link leads to a page." | "未作成のページはありません。すべてのリンクがページにつながっています。" |
| `health.none_orphans` | in place of the list of orphan pages when there are none | "Every page has a page that links to it." | "すべてのページが、ほかのページからリンクされています。" |
| `health.none_dead_ends` | in place of the list of dead-end pages when there are none | "Every page links to another page." | "すべてのページが、ほかのページにリンクしています。" |
| `health.none_redirects` | in place of the list of redirect problems when there are none | "Every redirect leads to a page." | "すべてのリダイレクトがページにつながっています。" |
| `health.none_help` | in place of the list of pages that want help when there are none | "No page is marked as needing help." | "手助けが必要と示されているページはありません。" |
| `health.none_stale` | in place of the list of pages unchanged longest when the wiki has no page | "There are no pages to list." | "表示できるページはありません。" |
| `title.links` | the title of the page that lists the pages linking to a page; `{page}` is its title | "Links to {page} · {wiki} · {product}" | "{page} へのリンク · {wiki} · {product}" |
| `links.heading` | its heading | "Pages that link to {page}" | "{page} へのリンク元のページ" |
| `links.missing` | under that heading when no page has the title | "No page has this title yet. These pages link to it." | "このタイトルのページはまだありません。次のページがリンクしています。" |
| `links.go_to` | a link to the page whose backlinks are listed | "Go to {page}" | "{page} を開く" |
| `links.none` | when no page links to the page | "No page links here." | "このページへリンクしているページはありません。" |
| `links.kind_include` | beside a page that includes this one | "Includes this page" | "このページを埋め込んでいます" |
| `links.kind_redirect` | beside a redirect to this page | "Redirects here" | "ここへリダイレクトしています" |
| `links.cut` | under the list when it is cut; `{limit}` is how many are listed | "Only the first {limit} are listed." | "最初の {limit} 件のみ表示しています。" |
| `head.feed_wiki` | the name of the feed of a wiki's changes in a browser's or reader's list; `{wiki}` is the wiki's name | "Recent changes to {wiki}" | "{wiki} の最近の変更" |
| `head.feed_page` | the name of the feed of one page's changes; `{page}` is its title | "Changes to {page}" | "{page} への変更" |
| `head.markdown` | the name of a page's Markdown source in a tool's list; `{page}` is its title | "{page} as Markdown" | "{page}（Markdown）" |
| `head.search` | the name of the search in a browser's list of search engines; `{wiki}` is the wiki's name | "Search {wiki}" | "{wiki} を検索" |
| `problem.change_not_found.title` | the API's problem detail `change-not-found`: the title | "Change not found" | "変更が見つかりません" |
| `problem.change_not_found.detail` | the API's problem detail `change-not-found`: the detail | "This wiki has no change with this id in this view." | "このウィキのこのビューには、この ID の変更がありません。" |
| `problem.change_not_found.hint` | the API's problem detail `change-not-found`: the hint | "Take the id of a change from the history of a page or from the recent changes." | "変更の ID は、ページの履歴または最近の変更から取ってください。" |
| `problem.authentication_required.title` | the API's problem detail `authentication-required`: the title | "Sign-in needed" | "サインインが必要です" |
| `problem.authentication_required.detail` | the API's problem detail `authentication-required`: the detail | "This request carries no credential, and writing needs an account." | "このリクエストには認証情報がなく、書き込みにはアカウントが必要です。" |
| `problem.authentication_required.hint` | the API's problem detail `authentication-required`: the hint | "Send an API token in an Authorization header, as Bearer followed by the token." | "Authorization ヘッダーに、Bearer に続けて API トークンを付けて送ってください。" |
| `problem.invalid_token.title` | the API's problem detail `invalid-token`: the title | "Token not accepted" | "トークンを受け付けられません" |
| `problem.invalid_token.detail` | the API's problem detail `invalid-token`: the detail | "The token in the Authorization header is not one this server accepts: it may be unknown, expired or revoked, or its account may be disabled." | "Authorization ヘッダーのトークンは、このサーバーが受け付けるものではありません。未知、期限切れ、失効済みのいずれか、またはアカウントが無効になっている可能性があります。" |
| `problem.invalid_token.hint` | the API's problem detail `invalid-token`: the hint | "Issue a new token and send that one." | "新しいトークンを発行して、そちらを送ってください。" |
| `problem.forbidden.title` | the API's problem detail `forbidden`: the title | "Not allowed" | "許可されていません" |
| `problem.forbidden.detail` | the API's problem detail `forbidden`: the detail | "This credential may not do this: the account's role in this wiki or the scope the token was issued with does not allow it, or a request made with a session cookie came from another site." | "この認証情報ではこの操作はできません。このウィキでのアカウントの役割か、トークンの発行時のスコープが許していないか、セッション Cookie を伴うリクエストが別のサイトから来ています。" |
| `problem.forbidden.hint` | the API's problem detail `forbidden`: the hint | "Ask a maintainer of the wiki for the role, or use a token issued with a wider scope." | "ウィキのメンテナーに役割を依頼するか、より広いスコープで発行したトークンを使ってください。" |
| `problem.base_required.title` | the API's problem detail `base-required`: the title | "Base needed" | "基準が必要です" |
| `problem.base_required.detail` | the API's problem detail `base-required`: the detail | "A write names the state it was written against, and this request has no If-Match header." | "書き込みは、書いた時点の状態を示す必要がありますが、このリクエストには If-Match ヘッダーがありません。" |
| `problem.base_required.hint` | the API's problem detail `base-required`: the hint | "Send the ETag of the proposal's revision as If-Match; for a new proposal, send the state id of the view's tip." | "If-Match に提案のリビジョンの ETag を付けて送ってください。新しい提案では、ビューの先頭の状態 ID を送ってください。" |
| `problem.stale_base.title` | the API's problem detail `stale-base`: the title | "The base is out of date" | "基準が古くなっています" |
| `problem.stale_base.detail` | the API's problem detail `stale-base`: the detail | "What this write was made against is no longer the current state: someone else got there first. Nothing was recorded." | "この書き込みの基準は、もう現在の状態ではありません。先に別の操作がありました。何も記録されていません。" |
| `problem.stale_base.hint` | the API's problem detail `stale-base`: the hint | "Read the current state, check your change against it, and send again with the current value in If-Match. This problem carries that value." | "現在の状態を読み、変更を照らし合わせてから、If-Match に現在の値を付けて送り直してください。この問題の応答にその値が含まれています。" |
| `problem.invalid_body.title` | the API's problem detail `invalid-body`: the title | "Body not accepted" | "本文を受け付けられません" |
| `problem.invalid_body.detail` | the API's problem detail `invalid-body`: the detail | "The body is not a JSON object of the shape this endpoint takes: the member {member} is missing, repeated, unknown or has a value it may not have." | "本文が、このエンドポイントの形の JSON オブジェクトではありません。メンバー {member} が、ない、重複している、未知である、または許されない値です。" |
| `problem.invalid_body.hint` | the API's problem detail `invalid-body`: the hint | "Send the members the description of this endpoint lists, and no others." | "このエンドポイントの説明にあるメンバーだけを送ってください。" |
| `problem.body_too_large.title` | the API's problem detail `body-too-large`: the title | "Body too large" | "本文が大きすぎます" |
| `problem.body_too_large.detail` | the API's problem detail `body-too-large`: the detail | "The body is larger than this endpoint takes: at most {limit} bytes. Nothing was read of it." | "本文が、このエンドポイントの受け付ける大きさ（最大 {limit} バイト）を超えています。内容は読み込まれていません。" |
| `problem.body_too_large.hint` | the API's problem detail `body-too-large`: the hint | "Send less: split the change, or shorten the text." | "送る量を減らしてください。変更を分けるか、テキストを短くします。" |
| `problem.unsupported_media_type.title` | the API's problem detail `unsupported-media-type`: the title | "Not sent as JSON" | "JSON として送られていません" |
| `problem.unsupported_media_type.detail` | the API's problem detail `unsupported-media-type`: the detail | "The body must be JSON, sent with the content type application/json." | "本文は JSON で、コンテンツタイプ application/json で送る必要があります。" |
| `problem.unsupported_media_type.hint` | the API's problem detail `unsupported-media-type`: the hint | "Set the Content-Type header to application/json." | "Content-Type ヘッダーを application/json にしてください。" |
| `problem.unacceptable.title` | the API's problem detail `unacceptable`: the title | "Not acceptable as sent" | "このままでは受け付けられません" |
| `problem.unacceptable.detail` | the API's problem detail `unacceptable`: the detail | "{explanation}" | "{explanation}" |
| `problem.unacceptable.hint` | the API's problem detail `unacceptable`: the hint | "Correct what the reason names and send again. Nothing was recorded." | "reason が示す点を直して、もう一度送ってください。何も記録されていません。" |
| `problem.not_in_this_state.title` | the API's problem detail `not-in-this-state`: the title | "Not possible now" | "今はできません" |
| `problem.not_in_this_state.detail` | the API's problem detail `not-in-this-state`: the detail | "This cannot be done while the proposal is {state}." | "提案が {state} の間は、これはできません。" |
| `problem.not_in_this_state.hint` | the API's problem detail `not-in-this-state`: the hint | "Read the proposal: what is possible depends on its state." | "提案を読んでください。できることは提案の状態によって決まります。" |
| `problem.already_decided.title` | the API's problem detail `already-decided`: the title | "Already decided" | "すでに判断しています" |
| `problem.already_decided.detail` | the API's problem detail `already-decided`: the detail | "You have decided on this revision already, and a decision is not changed." | "このリビジョンにはすでに判断を出しており、判断は変更できません。" |
| `problem.already_decided.hint` | the API's problem detail `already-decided`: the hint | "Decide again on the next revision, when there is one." | "次のリビジョンができたら、そのときにあらためて判断してください。" |
| `problem.refused_limit.title` | the API's problem detail `refused-limit`: the title | "Refused: a limit" | "拒否: 上限に達しました" |
| `problem.refused_limit.detail` | the API's problem detail `refused-limit`: the detail | "A limit was reached: {limit}. The members allowed, current and asked give the numbers, and reset says when it will be possible again." | "上限に達しました: {limit}。allowed、current、asked の各メンバーが数を示し、reset がまた可能になる時刻を示します。" |
| `problem.refused_limit.hint` | the API's problem detail `refused-limit`: the hint | "Where waiting helps, Retry-After says how long. Otherwise close a proposal of yours, or ask for less." | "待てば解消する場合は Retry-After が待つ秒数を示します。そうでない場合は、ご自身の提案を閉じるか、求める量を減らしてください。" |
| `problem.busy.title` | the API's problem detail `busy`: the title | "The wiki is busy" | "ウィキは混み合っています" |
| `problem.busy.detail` | the API's problem detail `busy`: the detail | "Another act on this wiki is running or waiting. Acts ahead of this one: {waiting}. Nothing was done." | "このウィキでは別の操作が実行中または待機中で、この操作の前に {waiting} 件あります。何も行われていません。" |
| `problem.busy.hint` | the API's problem detail `busy`: the hint | "Send it again after the time Retry-After gives." | "Retry-After が示す時間のあとで、もう一度送ってください。" |
| `problem.needs_recovery.title` | the API's problem detail `needs-recovery`: the title | "The wiki needs recovery" | "ウィキの復旧が必要です" |
| `problem.needs_recovery.detail` | the API's problem detail `needs-recovery`: the detail | "A write to this wiki did not finish. The wiki accepts no more changes until recovery has looked at it, and reading still works." | "このウィキへの書き込みが完了しませんでした。復旧が確認するまで、ウィキは新しい変更を受け付けませんが、読み取りはできます。" |
| `problem.needs_recovery.hint` | the API's problem detail `needs-recovery`: the hint | "Tell the person who runs this server. This request did nothing." | "このサーバーを運用している人に知らせてください。このリクエストでは何も行われていません。" |
| `problem.overlap.title` | the API's problem detail `overlap`: the title | "Changes overlap" | "変更が重なっています" |
| `problem.overlap.detail` | the API's problem detail `overlap`: the detail | "Stopped: the proposal and the wiki changed the same places, {overlaps} of them. The proposal now needs a revision, and the wiki is as it was." | "停止しました。提案とウィキが同じ箇所を変更しており、{overlaps} 件あります。提案は改訂が必要な状態になり、ウィキは変わっていません。" |
| `problem.overlap.hint` | the API's problem detail `overlap`: the hint | "Read the overlap on the proposal, write a revision that takes both texts into account, and send it." | "提案に記録された重なりを読み、両方のテキストを踏まえた改訂を書いて送ってください。" |
| `problem.nothing_to_change.title` | the API's problem detail `nothing-to-change`: the title | "Nothing to change" | "変更するものがありません" |
| `problem.nothing_to_change.detail` | the API's problem detail `nothing-to-change`: the detail | "The wiki already has everything this revision asks for." | "このリビジョンが求める内容は、すでにウィキにあります。" |
| `problem.nothing_to_change.hint` | the API's problem detail `nothing-to-change`: the hint | "Decline or withdraw the proposal, or revise it." | "提案を却下または取り下げるか、改訂してください。" |
| `problem.acceptance_in_progress.title` | the API's problem detail `acceptance-in-progress`: the title | "Acceptance already underway" | "受け入れがすでに進行中です" |
| `problem.acceptance_in_progress.detail` | the API's problem detail `acceptance-in-progress`: the detail | "An earlier acceptance of this revision has not finished." | "このリビジョンの先の受け入れが、まだ署名を終えていません。" |
| `problem.acceptance_in_progress.hint` | the API's problem detail `acceptance-in-progress`: the hint | "Wait and ask again. Recovery settles it if the earlier one was interrupted." | "しばらく待ってから、もう一度試してください。先の受け入れが中断されていた場合は、復旧が解決します。" |
| `problem.acceptance_failed.title` | the API's problem detail `acceptance-failed`: the title | "Acceptance failed" | "受け入れに失敗しました" |
| `problem.acceptance_failed.detail` | the API's problem detail `acceptance-failed`: the detail | "The change was not made: {word}. The proposal stays open and the history is as it was." | "変更は行われませんでした: {word}。提案は開いたままで、履歴は変わっていません。" |
| `problem.acceptance_failed.hint` | the API's problem detail `acceptance-failed`: the hint | "Try again, or tell the person who runs this server." | "もう一度試すか、このサーバーを運用している人に知らせてください。" |
| `title.inbox` | the title of the inbox | "Inbox · {product}" | "受信箱 · {product}" |
| `inbox.heading` | the heading of the inbox | "Inbox" | "受信箱" |
| `inbox.intro` | under the heading of the inbox | "What has happened to your proposals and to the pages you follow, newest first." | "あなたの提案と、フォローしているページに起きたことを、新しい順に表示します。" |
| `inbox.none` | shown instead of the list when nothing has happened that involves the reader | "Nothing has happened yet that involves you." | "あなたに関わる出来事はまだありません。" |
| `inbox.new` | a mark before an event that has not been read | "New" | "新着" |
| `inbox.mark_all_read` | the button under the list of events | "Mark everything as read" | "すべて既読にする" |
| `inbox.someone` | in an event, in place of the name of a person who is not known | "Someone" | "だれか" |
| `inbox.event_reviewed` | an event; `{person}` is who reviewed, `{proposal}` the proposal (a link), `{wiki}` the wiki | "{person} reviewed your proposal {proposal} in {wiki}." | "{person} さんが {wiki} のあなたの提案 {proposal} をレビューしました。" |
| `inbox.event_accepted` | an event; `{proposal}` is the proposal (a link), `{wiki}` the wiki | "Your proposal {proposal} in {wiki} was accepted and signed into the history." | "{wiki} のあなたの提案 {proposal} は受け入れられ、署名して履歴に記録されました。" |
| `inbox.event_declined` | an event; `{person}` is who declined | "{person} declined your proposal {proposal} in {wiki}." | "{person} さんが {wiki} のあなたの提案 {proposal} を見送りました。" |
| `inbox.event_needs_revision` | an event; `{person}` is who asked | "{person} asked for a revision of your proposal {proposal} in {wiki}." | "{person} さんが {wiki} のあなたの提案 {proposal} の修正を依頼しました。" |
| `inbox.event_withdrawn` | an event, for the people who held the proposal; `{person}` is who took it back | "{person} withdrew the proposal {proposal} in {wiki}." | "{person} さんが {wiki} の提案 {proposal} を取り下げました。" |
| `inbox.event_superseded` | an event; `{person}` is who put a later proposal in its place | "{person} replaced your proposal {proposal} in {wiki} with a later one." | "{person} さんが {wiki} のあなたの提案 {proposal} を、あとの提案に置き換えました。" |
| `inbox.event_replied` | an event; `{person}` is who commented | "{person} commented in the discussion of {proposal} in {wiki}." | "{person} さんが {wiki} の {proposal} の議論にコメントしました。" |
| `inbox.event_mentioned` | an event; `{person}` is who wrote the comment | "{person} mentioned you in the discussion of {proposal} in {wiki}." | "{person} さんが {wiki} の {proposal} の議論であなたに言及しました。" |
| `inbox.event_watched` | an event; `{proposal}` is the proposal the change came from (a link) | "A page you follow in {wiki} changed. The change came from the proposal {proposal}." | "{wiki} でフォローしているページが変更されました。変更のもとになった提案は {proposal} です。" |
| `inbox.event_unreadable` | an event about something the reader may not read any more | "Something happened that involves you, in a wiki you can no longer read." | "あなたに関わる出来事がありましたが、そのウィキはもう読むことができません。" |
| `inbox.account_heading` | the heading of the page that says an account is needed (the inbox, and following a page) | "An account is needed" | "アカウントが必要です" |
| `inbox.account_body` | its sentence, on the inbox | "Your inbox belongs to an account. Sign in to see what has happened to your proposals and to the pages you follow." | "受信箱はアカウントごとのものです。サインインすると、あなたの提案やフォローしているページに起きたことを確認できます。" |
| `inbox.following_heading` | the heading of the list of what the reader follows, on the inbox | "Pages you follow" | "フォローしているページ" |
| `inbox.following_none` | shown instead of that list when it is empty | "You do not follow any page yet." | "フォローしているページはまだありません。" |
| `inbox.following_page` | an item of that list; `{page}` is the page's title (a link), `{wiki}` the wiki | "{page} in {wiki}" | "{wiki} の {page}" |
| `inbox.following_namespace` | an item of that list; `{page}` is the title the pages are under (a link), `{wiki}` the wiki | "The pages under {page} in {wiki}" | "{wiki} の {page} の下にあるページ" |
| `inbox.unfollow` | the label of the button beside an item of that list; `{what}` is the item in words | "Stop following {what}" | "{what} のフォローをやめる" |
| `watch.heading_followed` | the heading of the answer to following a page | "Following" | "フォロー中" |
| `watch.heading_stopped` | the heading of the answer to stopping | "No longer following" | "フォローをやめました" |
| `watch.heading_full` | the heading of the answer when the reader follows as many as they may | "You follow as many as you can" | "フォローできる数の上限です" |
| `watch.page_followed` | the answer to following a page; `{title}` is its title | "You follow the page {title}. When a change to it is signed into the history, you will be told in your inbox." | "ページ「{title}」をフォローしました。このページへの変更が署名して履歴に記録されると、受信箱でお知らせします。" |
| `watch.namespace_followed` | the answer to following the pages under a title; `{title}` is the title | "You follow the pages under {title}. When a change to one of them is signed into the history, you will be told in your inbox." | "「{title}」の下にあるページをフォローしました。それらへの変更が署名して履歴に記録されると、受信箱でお知らせします。" |
| `watch.page_stopped` | the answer to stopping following a page; `{title}` is its title | "You no longer follow the page {title}." | "ページ「{title}」のフォローをやめました。" |
| `watch.namespace_stopped` | the answer to stopping following the pages under a title | "You no longer follow the pages under {title}." | "「{title}」の下にあるページのフォローをやめました。" |
| `watch.account_body` | the sentence under the heading that says an account is needed, when the reader tried to follow a page | "Following a page belongs to an account. Sign in, then follow the page again." | "ページのフォローはアカウントごとの機能です。サインインしてから、もう一度フォローしてください。" |
| `watch.full_body` | its sentence, when the reader follows as many pages as they may | "You follow as many pages as you can. Stop following one of them, then try again." | "フォローできるページの数の上限に達しています。どれかのフォローをやめてから、もう一度お試しください。" |
| `watch.back` | a link back to the page that was followed; `{title}` is its title | "Back to {title}" | "{title} に戻る" |
| `watch.to_inbox` | a link to the inbox | "Your inbox" | "受信箱を開く" |
| `changes.filter_minor` | the label of the choice that shows, hides or isolates the minor changes, on the recent changes | "Minor changes" | "軽微な変更" |
| `changes.active_minor_hide` | in the list of filters in use, on the recent changes | "Minor changes are hidden" | "軽微な変更は表示していません" |
| `changes.active_minor_only` | in the list of filters in use, on the recent changes | "Only minor changes" | "軽微な変更のみ" |
| `changes.minor_label` | a mark on a change that was proposed as minor | "Minor change" | "軽微な変更" |
| `changes.machine_assisted` | a mark on a change that was proposed as machine-assisted; `{person}` is who accepted it and answers for it | "Machine-assisted content, accepted by {person}" | "機械の補助によるコンテンツ（受け入れた人: {person}）" |
| `changes.machine_assisted_only` | the same mark, when nobody who accepted it is known | "Machine-assisted content" | "機械の補助によるコンテンツ" |
| `wiki.nav_proposals` | in the wiki's navigation: the link to its proposals | "Proposals" | "提案" |
| `page.propose_change` | under a page: the link to the form that proposes a change to the page | "Propose a change" | "変更を提案する" |
| `page.propose_part` | under a page: the link to the choice of a section or part to propose a change to | "Propose a change to a section or a part" | "セクションや一部分への変更を提案する" |
| `page.discussion` | under a page: the link to the page's discussion | "Discussion" | "ディスカッション" |
| `propose.new_page_link` | on the page for a title that has no page: the link to the form that proposes the page | "Propose this page" | "このページを提案する" |
| `state.draft` | the label of a proposal that has not been sent for review; only its author sees it | "Draft" | "下書き" |
| `state.held` | the label of a proposal sent for review and not yet decided | "Held for review" | "レビュー待ちで保持中" |
| `state.needs_revision` | the label of a proposal sent back to its proposer | "Needs a revision" | "修正が必要" |
| `state.accepted` | the label of a proposal that was taken into the history | "Accepted" | "受け入れ済み" |
| `state.declined` | the label of a proposal that was not taken into the history | "Declined" | "見送り済み" |
| `state.withdrawn` | the label of a proposal its proposer took back | "Withdrawn" | "取り下げ済み" |
| `state.superseded` | the label of a proposal that a later proposal replaced | "Superseded" | "置き換え済み" |
| `state_note.draft` | under the label of a draft | "A draft. Only the person who started it can see it, and nobody has been asked to review it." | "下書きです。始めた本人だけが見られます。まだ誰にもレビューを依頼していません。" |
| `state_note.held` | under the label of a proposal held for review | "The wiki service holds this proposal for review. Nothing becomes part of a page until someone who is allowed to takes the proposal into the history." | "ウィキサービスはこの提案をレビュー待ちとして保持しています。権限のある人が提案を履歴に取り込むまで、ページには何も反映されません。" |
| `state_note.needs_revision` | under the label of a proposal that needs a revision; `{author}` is its proposer | "This proposal needs a revision from {author} before it can be taken into the history." | "この提案は、履歴に取り込む前に {author} さんによる修正が必要です。" |
| `state_note.accepted` | under the label of an accepted proposal | "Accepted: someone who is allowed to took this proposal into the history." | "受け入れ済み: 権限のある人がこの提案を履歴に取り込みました。" |
| `state_note.declined` | under the label of a declined proposal | "Declined: someone who is allowed to decided not to take this proposal into the history." | "見送り済み: 権限のある人が、この提案を履歴に取り込まないと判断しました。" |
| `state_note.withdrawn` | under the label of a withdrawn proposal | "Withdrawn by its proposer. It is kept here for the record." | "提案者が取り下げました。記録として残してあります。" |
| `state_note.superseded` | under the label of a superseded proposal | "Superseded: a later proposal takes its place." | "置き換え済み: 後から出された提案がこの提案に代わります。" |
| `proposal.superseded_by` | on a superseded proposal; `{proposal}` links to the one that replaced it | "It was replaced by {proposal}." | "{proposal} に置き換えられました。" |
| `proposal.another` | the name of a proposal the reader may not see, in a link to it | "another proposal" | "別の提案" |
| `proposal.reason_declined` | on a declined proposal: who declined it and why | "{person} declined this proposal. Reason: {reason}" | "{person} さんがこの提案を見送りました。理由: {reason}" |
| `proposal.reason_withdrawn` | on a withdrawn proposal: who took it back and why | "{person} withdrew this proposal. Reason: {reason}" | "{person} さんがこの提案を取り下げました。理由: {reason}" |
| `proposal.reason_asked` | on a proposal sent back: who asked for a revision and why | "{person} asked for a revision. Reason: {reason}" | "{person} さんが修正を求めました。理由: {reason}" |
| `outcome.signed` | the outcome of an acceptance that is in the history | "Signed into the history" | "署名して履歴に記録済み" |
| `outcome.overlap` | the outcome of an acceptance that stopped because the same text was changed twice | "Stopped: changes overlap" | "停止: 変更が重なっています" |
| `outcome.not_allowed` | the outcome of an act the visitor's role does not include; also the heading of that refusal | "Not allowed" | "許可されていません" |
| `outcome.busy` | the outcome of an acceptance that found the wiki busy | "The wiki is busy" | "ウィキは混み合っています" |
| `outcome.recovery` | the outcome of an acceptance on a wiki that is being checked after an interruption | "The wiki needs recovery" | "ウィキの復旧が必要です" |
| `outcome.refused` | the outcome of an act a limit refused; `{limit}` names the limit | "Refused: {limit}" | "拒否: {limit}" |
| `outcome.refused_heading` | the heading of the page that says a limit refused an act | "Refused" | "拒否されました" |
| `outcome_note.signed` | under the outcome signed into the history | "The proposal is part of the history now." | "提案は履歴に取り込まれました。" |
| `outcome_note.overlap` | under the outcome stopped: changes overlap | "Another change touched the same text. Nothing was changed. The proposer is asked for a revision, and both texts are shown on the proposal." | "同じ箇所に別の変更が入っていました。何も変更していません。提案者に修正をお願いし、両方の文面を提案のページに表示します。" |
| `outcome_note.not_allowed` | under the outcome not allowed, for an acceptance | "Your role in this wiki does not include accepting proposals. Nothing was changed." | "このウィキでのあなたの役割には、提案を受け入れる権限がありません。何も変更していません。" |
| `outcome_note.busy` | under the outcome the wiki is busy | "Another change is being signed into the history. Nothing was changed; try again in a moment." | "別の変更を履歴に記録している最中です。何も変更していません。しばらくしてからもう一度お試しください。" |
| `outcome_note.busy_depth` | under that: how many acceptances wait before this one; `{count}` is the number | "One acceptance is waiting ahead of yours." / "{count} acceptances are waiting ahead of yours." | "あなたの前に {count} 件の受け入れが待っています。" |
| `outcome_note.recovery` | under the outcome the wiki needs recovery | "The wiki is being checked after an interruption. It can be read, but nothing can be changed until the check is done." | "ウィキは中断後の確認を行っています。読むことはできますが、確認が終わるまで変更はできません。" |
| `outcome.unchanged_refused` | the outcome of an acceptance the history refused as it was prepared; nothing was changed | "Not signed: the history refused it" | "署名されませんでした: 履歴が受け付けませんでした" |
| `outcome_note.unchanged_refused` | under the outcome not signed: the history refused it | "The history did not accept this change as it was prepared. Nothing was changed, and the proposal is still held for review." | "履歴は、用意された変更をそのままでは受け付けませんでした。何も変更しておらず、提案はレビュー待ちのままです。" |
| `outcome.too_large` | the outcome of an acceptance that needed more memory than one signing may use | "Too large to sign now" | "大きすぎて今は署名できません" |
| `outcome_note.too_large` | under the outcome too large to sign now | "This change needs more memory than one signing may use. Nothing was changed, and the proposal is still held for review." | "この変更の署名には、1 回の署名で使える量を超えるメモリが必要です。何も変更しておらず、提案はレビュー待ちのままです。" |
| `outcome.unavailable` | the outcome of an acceptance that could not be done now, before anything was changed | "Could not be done now" | "今は実行できません" |
| `outcome_note.unavailable` | under the outcome could not be done now | "The records or the history could not be used just now. Nothing was changed. Try again in a little while; if it keeps happening, tell the person who runs this server." | "記録または履歴を今は利用できませんでした。何も変更していません。しばらくしてからもう一度お試しください。何度も起きる場合は、このサーバーを運用している人に伝えてください。" |
| `outcome.in_progress` | the outcome of an acceptance of a revision whose earlier acceptance has not finished | "This revision is being signed" | "この修正版は署名の最中です" |
| `outcome_note.in_progress` | under the outcome this revision is being signed | "An earlier attempt to take this revision into the history has not finished. Nothing was done now. Wait a little and read the proposal again." | "この修正版を履歴に取り込む以前の操作が、まだ終わっていません。今回は何も行っていません。しばらく待ってから、提案をもう一度開いてください。" |
| `outcome.nothing_to_change` | the outcome of an acceptance whose changes the history already has | "Nothing to change" | "変更するものがありません" |
| `outcome_note.nothing_to_change` | under the outcome nothing to change | "The history already has everything this proposal asks for. Nothing was changed." | "この提案が求める内容は、すでに履歴に入っています。何も変更していません。" |
| `outcome.store` | the outcome of an acceptance when the records failed and it is not known whether the change is in the history | "The result is not certain" | "結果を確定できません" |
| `outcome_note.store` | under the outcome the result is not certain | "The records could not be used while the change was being taken in. It may or may not be in the history now: read the history before you try again, and tell the person who runs this server." | "変更を取り込む途中で、記録を利用できなくなりました。変更が履歴に入ったかどうかは分かりません。もう一度試す前に履歴を確認し、このサーバーを運用している人に伝えてください。" |
| `accept.heading` | the heading of the page that answers the Accept button while accepting is not served | "Accepting is not available yet" | "受け入れはまだ利用できません" |
| `accept.not_yet` | on that page | "This server does not take proposals into the history yet." | "このサーバーは、まだ提案を履歴に取り込めません。" |
| `accept.not_yet_kept` | on that page | "The proposal stays held for review and nothing was changed." | "提案はレビュー待ちのままで、何も変更していません。" |
| `accept.not_yet_note` | beside the Accept button | "Accepting is not available yet on this server." | "このサーバーでは、受け入れはまだ利用できません。" |
| `overlap.heading` | the heading of the section of a proposal that needs a revision because it overlaps the wiki's current text | "Where the changes overlap" | "変更が重なっている箇所" |
| `overlap.intro` | under that heading | "The wiki's current text and this proposal both changed the places below. Nothing was merged. Revise the proposal with both texts in front of you." | "ウィキの現在の文面とこの提案が、下の箇所をどちらも変更しています。何も統合していません。両方の文面を見ながら、提案を修正してください。" |
| `overlap.caption` | the caption of the table of overlapping places | "Places changed both in the wiki and in this proposal" | "ウィキと提案の両方で変更された箇所" |
| `overlap.col_where` | a column heading of that table: the page and the place | "Where" | "場所" |
| `overlap.col_theirs` | a column heading of that table: the wiki's text now | "In the wiki now" | "ウィキの現在の文面" |
| `overlap.col_ours` | a column heading of that table: the proposal's text | "In this proposal" | "この提案の文面" |
| `overlap.nothing` | in a cell of that table, when that side removed the text | "(removed: nothing is there)" | "（削除されています: 何もありません）" |
| `overlap.row` | the name of a row of that table; `{page}` is the page's title, `{place}` the place in it | "{page}: {place}" | "{page}: {place}" |
| `overlap.place_top` | a place in that table: the information at the top of a page, as a whole | "Information at the top of the page" | "ページ先頭の情報" |
| `overlap.place_key` | a place in that table: one item of the information at the top of a page; `{key}` is its name | "Information at the top of the page: {key}" | "ページ先頭の情報: {key}" |
| `overlap.place_part` | a place in that table: a part of the page that has no number | "A part of the page" | "ページの一部分" |
| `overlap.place_number` | a place in that table: a numbered part of the page | "Part {number}" | "部分 {number}" |
| `overlap.place_named` | a place in that table: a part of the page that has a name; `{id}` is the name | "The part named {id}" | "{id} と名付けられた部分" |
| `overlap.place_page` | a place in that table: the whole page | "The whole page" | "ページ全体" |
| `overlap.place_path` | a place in that table: the page's address | "The page's address" | "ページのアドレス" |
| `quota.open_proposals` | the name of the limit on open proposals, as a clause | "too many open proposals" | "未決の提案が多すぎます" |
| `quota.revisions` | the name of the limit on revisions of one proposal | "too many revisions of one proposal" | "1 つの提案の修正が多すぎます" |
| `quota.accepted_bytes` | the name of the limit on text taken into the history in a day | "too much accepted text today" | "本日受け入れられた文章が多すぎます" |
| `quota.hour_person` | the name of the limit on proposals a person opens in an hour | "too many proposals opened in the last hour" | "直近 1 時間に出した提案が多すぎます" |
| `quota.hour_wiki` | the name of the limit on proposals opened in the wiki in an hour | "too many proposals opened in this wiki in the last hour" | "直近 1 時間にこのウィキで出された提案が多すぎます" |
| `quota.queue` | the name of the limit on acceptances waiting | "too many acceptances waiting" | "待機中の受け入れが多すぎます" |
| `quota.counts` | under a refusal by a limit: the limit, the count now and what was asked | "The limit is {allowed}. The count now is {current}, and this asked for {asked}." | "上限は {allowed} です。現在の数は {current} で、今回の要求は {asked} です。" |
| `quota.reset_at` | under a refusal by a limit: when it ends; `{when}` is a time in UTC | "It resets at {when}." | "{when} にリセットされます。" |
| `quota.reset_proposal_closes` | under a refusal by the limit on open proposals | "It resets when one of your open proposals is accepted, declined, withdrawn or superseded." | "あなたの未決の提案のどれかが、受け入れ・見送り・取り下げ・置き換えのいずれかになるとリセットされます。" |
| `quota.reset_queue` | under a refusal by the limit on acceptances waiting | "It resets when a waiting acceptance gets its turn or gives up." | "待機中の受け入れの順番が来るか、取りやめになるとリセットされます。" |
| `quota.reset_never_revisions` | under a refusal by the limit on revisions | "Waiting does not help: this proposal has all the revisions it can have. Open a new proposal instead." | "待っても解消しません。この提案はこれ以上修正できません。新しい提案を出してください。" |
| `quota.reset_never_too_big` | under a refusal by a limit that one request can exceed alone | "Waiting does not help: this is more than the limit allows at any time. Make it smaller." | "待っても解消しません。この量は上限を超えています。もっと小さくしてください。" |
| `size.mib` | a size in mebibytes; `{amount}` is the number | "{amount} MiB" | "{amount} MiB" |
| `size.kib` | a size in kibibytes | "{amount} KiB" | "{amount} KiB" |
| `size.bytes` | a size in bytes | "{amount} bytes" | "{amount} バイト" |
| `needs_account.heading` | the heading of the page that tells a visitor who is not signed in that proposing needs an account | "Proposing needs an account" | "提案にはアカウントが必要です" |
| `needs_account.body` | on that page | "You are not signed in, and proposing a change, commenting or reviewing needs an account." | "サインインしていません。変更の提案、コメント、レビューにはアカウントが必要です。" |
| `needs_account.advice` | on that page | "Ask the person who runs this server how to get an account." | "アカウントの作り方は、このサーバーの管理者にお尋ねください。" |
| `refused.not_allowed` | under the heading not allowed, for an act on a proposal | "Your role in this wiki does not include this. Nothing was changed." | "このウィキでのあなたの役割には、この操作が含まれていません。何も変更していません。" |
| `refused.not_now_heading` | the heading of the page that says an act is not possible now | "Not possible now" | "今はできません" |
| `refused.invalid_heading` | the heading of the page that says what was sent could not be used | "This could not be saved" | "保存できませんでした" |
| `refused.not_in_this_state` | under that heading; `{state}` is the proposal's state | "This proposal is {state}, so this cannot be done now." | "この提案は「{state}」の状態なので、今はこの操作はできません。" |
| `refused.already_decided` | when a reviewer decides twice on one revision | "You have already decided on this revision. A decision is not changed; decide again on the next revision." | "この修正にはすでに判断を記録しています。判断は変更できません。次の修正であらためて判断してください。" |
| `refused.stale_revision` | when a decision is on a revision that is not the latest; `{latest}` is the latest | "This proposal has a newer revision, number {latest}. Read it again and decide on that one." | "この提案には新しい修正（{latest} 番）があります。読み直して、そちらについて判断してください。" |
| `refused.superseder` | when a proposal that cannot replace another is named; `{state}` is its state | "A proposal that is {state} cannot replace another one." | "「{state}」の提案は、別の提案に代わることはできません。" |
| `refused.changed` | when the proposal changed while a form was being filled in | "The proposal changed while you were working on it. Read it again, then try again." | "作業している間に提案が変更されました。読み直してから、もう一度お試しください。" |
| `invalid.incomplete` | when a form is not the one a page offers | "The form was not complete. Open it again from the page and try again." | "フォームの内容が不完全でした。ページからもう一度開いて、お試しください。" |
| `invalid.form_too_large` | when a form is larger than any form can be | "What you sent is larger than a form can carry." | "送信された内容が、フォームで扱える大きさを超えています。" |
| `invalid.summary_missing` | when a proposal is sent without its summary | "Say in a few words what you changed and why." | "何をなぜ変更したのかを、短く書いてください。" |
| `invalid.summary_too_long` | when the summary is too long; `{limit}` is the most characters | "The summary is longer than {limit} characters." | "要約が {limit} 文字を超えています。" |
| `invalid.summary_not_one_line` | when the summary has a line break | "The summary must be one line of plain text." | "要約は、改行を含まない 1 行の文章にしてください。" |
| `invalid.too_many_sources` | when there are too many sources; `{limit}` is the most | "At most {limit} sources can be named." | "出典は {limit} 件までです。" |
| `invalid.bad_source` | when a source is empty, too long or more than one line; `{limit}` is the most bytes | "Each source must be one line, not empty and not longer than {limit} bytes." | "出典は 1 行で、空でなく、{limit} バイト以内にしてください。" |
| `invalid.nothing_proposed` | when a proposal changes nothing | "The text is the same as the page: change it first, then send it." | "文章がページと同じです。先に変更してから送信してください。" |
| `invalid.too_large` | when a proposal is over its size; `{limit}` is the size | "A proposal can hold at most {limit} of text in all." | "1 つの提案で扱える文章は、合計で {limit} までです。" |
| `invalid.page_too_large` | when a page of a proposal is over its size; `{limit}` is the size | "A page can hold at most {limit} of text." | "1 ページで扱える文章は {limit} までです。" |
| `invalid.reason_missing` | when a reason is required and empty | "A reason is needed. Say it kindly and say what could happen next." | "理由が必要です。やさしい言葉で、次にできることも書いてください。" |
| `invalid.reason_too_long` | when a reason is too long; `{limit}` is the size | "The reason is longer than {limit}." | "理由が {limit} を超えています。" |
| `invalid.comment_empty` | when a comment says nothing | "A comment needs some words." | "コメントには文章が必要です。" |
| `invalid.comment_too_long` | when a comment is too long; `{limit}` is the size | "The comment is longer than {limit}." | "コメントが {limit} を超えています。" |
| `title.proposals` | the title of the list of proposals | "Proposals · {wiki} · {product}" | "提案 · {wiki} · {product}" |
| `proposals.heading` | its heading | "Proposals" | "提案" |
| `proposals.intro` | under it; `{wiki}` is the wiki's name | "Changes to {wiki} are proposed here, read by others, and taken into the history by someone who is allowed to." | "{wiki} への変更はここで提案され、ほかの人に読まれ、権限のある人によって履歴に取り込まれます。" |
| `proposals.empty` | when the list is empty | "Nobody has proposed anything yet." | "まだ提案はありません。" |
| `proposals.new_page` | the link to the form that proposes a new page | "Propose a new page" | "新しいページを提案する" |
| `proposals.drafts` | the heading of the reader's own drafts | "Your drafts" | "あなたの下書き" |
| `proposals.open` | the heading of the proposals that are not decided | "Open proposals" | "未決の提案" |
| `proposals.closed` | the heading of the proposals that are decided | "Decided proposals" | "決着済みの提案" |
| `proposals.row_by` | under a proposal in the list; `{when}` is when it last changed | "Proposed by {author} · last changed {when}" | "提案者: {author} · 最終更新 {when}" |
| `proposal.no_summary` | in place of the summary of a draft that has none yet | "(no summary yet)" | "（要約はまだありません）" |
| `proposals.back` | a link back to the list of proposals | "Back to the proposals" | "提案の一覧へ戻る" |
| `proposal.back` | a link back to the proposal | "Back to the proposal" | "提案へ戻る" |
| `person.unknown` | in place of the name of a person who is not known | "someone" | "ある方" |
| `title.propose_parts` | the title of the page that offers a page's sections and parts; `{page}` is its title | "Choose what to change in {page} · {wiki} · {product}" | "{page} の変更箇所を選ぶ · {wiki} · {product}" |
| `parts.heading` | its heading | "Choose what to change in {page}" | "{page} の変更箇所を選ぶ" |
| `parts.intro` | under it | "You can propose a change to the whole page, to one section, or to one part." | "ページ全体、1 つのセクション、または 1 つの部分への変更を提案できます。" |
| `parts.whole` | the link to the form for the whole page | "The whole page" | "ページ全体" |
| `parts.sections_heading` | the heading of the sections | "Sections" | "セクション" |
| `parts.parts_heading` | the heading of the parts | "Parts" | "部分" |
| `parts.no_sections` | when the page has no headings | "This page has no headings." | "このページには見出しがありません。" |
| `parts.section_label` | the link to the form for one section; `{heading}` is its heading | "Section: {heading}" | "セクション: {heading}" |
| `parts.part_label` | the link to the form for one part; `{excerpt}` is how it begins | "Part {number}: {excerpt}" | "部分 {number}: {excerpt}" |
| `parts.cut` | under a list of parts that is cut; `{limit}` is how many are listed | "Only the first {limit} are listed." | "最初の {limit} 件のみ表示しています。" |
| `parts.back` | a link back to the page; `{page}` is its title | "Back to {page}" | "{page} へ戻る" |
| `title.propose_page` | the title of the form that proposes a change to a page | "Propose a change to {page} · {wiki} · {product}" | "{page} への変更を提案する · {wiki} · {product}" |
| `title.propose_new` | the title of the form that proposes a new page | "Propose a new page · {wiki} · {product}" | "新しいページを提案する · {wiki} · {product}" |
| `title.propose_revise` | the title of the form that revises a proposal | "Revise a proposal · {wiki} · {product}" | "提案を修正する · {wiki} · {product}" |
| `propose.heading_page` | the heading of the form that proposes a change to a page | "Propose a change to {page}" | "{page} への変更を提案する" |
| `propose.heading_new` | the heading of the form that proposes a new page | "Propose a new page" | "新しいページを提案する" |
| `propose.heading_revise` | the heading of the form that revises a proposal | "Revise this proposal" | "この提案を修正する" |
| `propose.intro` | under the heading of the form | "Edit the text, say what you changed and why, and send it for review. Nothing changes in the wiki until someone who is allowed to takes your proposal into the history." | "文章を編集し、何をなぜ変更したのかを書いて、レビューに送ってください。権限のある人があなたの提案を履歴に取り込むまで、ウィキは変わりません。" |
| `propose.scope_whole` | on the form: what is being changed | "You are changing the whole page." | "ページ全体を変更します。" |
| `propose.scope_section` | on the form; `{part}` is how the section begins | "You are changing one section: {part}" | "1 つのセクションを変更します: {part}" |
| `propose.scope_part` | on the form; `{part}` is how the part begins | "You are changing one part: {part}" | "1 つの部分を変更します: {part}" |
| `propose.choose` | on the form: the link to the choice of a section or part | "Choose a different section or part" | "別のセクションや部分を選ぶ" |
| `propose.label_text` | the label of the text of a page; `{title}` is its title | "Text of {title}" | "{title} の文章" |
| `propose.label_section` | the label of the text of a section | "Text of this section of {title}" | "{title} のこのセクションの文章" |
| `propose.label_part` | the label of the text of a part | "Text of this part of {title}" | "{title} のこの部分の文章" |
| `propose.label_title` | the label of the title of a new page | "Title of the new page" | "新しいページのタイトル" |
| `propose.title_hint` | under it | "A slash makes a page part of a group, for example Guides/Setup." | "スラッシュを使うと、ページをグループに入れられます。例: Guides/Setup" |
| `propose.new_page_name` | in place of the title of a new page that has none yet | "the new page" | "新しいページ" |
| `propose.label_summary` | the label of the summary; `{limit}` is the most characters | "What and why (up to {limit} characters)" | "何をなぜ（{limit} 文字まで）" |
| `propose.summary_hint` | under it | "One line, for example: Correct the date of the first meeting." | "1 行で書いてください。例: 最初の会合の日付を訂正。" |
| `propose.label_sources` | the label of the sources | "Sources (one per line, optional)" | "出典（1 行に 1 件、省略可）" |
| `propose.sources_hint` | under it | "Links or references that support the change." | "変更の裏付けとなるリンクや文献。" |
| `propose.preview` | the button that shows how the text will look | "Preview" | "プレビュー" |
| `propose.show_changes` | the button that shows what the text changes | "Show changes" | "変更点を表示" |
| `propose.save_draft` | the button that keeps the work as a draft | "Save as draft" | "下書きとして保存" |
| `propose.submit` | the button that sends the proposal for review | "Submit" | "送信" |
| `propose.submit_revision` | the button that sends a revision of a proposal for review | "Submit the revision" | "修正を送信" |
| `propose.saved` | after a draft is saved | "Your draft is saved. Only you can see it." | "下書きを保存しました。あなただけが見られます。" |
| `propose.problems` | the heading of what is wrong with a form | "Please fix this first" | "先に次の点を直してください" |
| `propose.continue_draft` | a link to the draft the proposal was kept as | "Continue with your draft" | "下書きの続きを書く" |
| `propose.removed_note` | in place of the text of a page the proposal removes; `{title}` is its title | "This proposal removes {title}." | "この提案は {title} を削除します。" |
| `propose.preview_heading` | the heading of a preview; `{title}` is the page's title | "Preview of {title}" | "{title} のプレビュー" |
| `propose.preview_note` | under it | "This is how your text will look. Nothing is saved yet." | "あなたの文章はこのように表示されます。まだ何も保存されていません。" |
| `propose.suggestions` | the heading of what could be better in a text | "Suggestions" | "改善の提案" |
| `propose.front_unreadable` | when the information at the top of a page cannot be read | "The information at the top of the page cannot be read. Each line needs a name, a colon and a value." | "ページ先頭の情報が読み取れません。各行は「名前」「コロン」「値」の形にしてください。" |
| `propose.front_owned` | when the information at the top of a page changes names only the service writes; `{keys}` names them | "The lines {keys} at the top of the page can only be changed by an administrator of this wiki." | "ページ先頭の {keys} の行は、このウィキの管理者だけが変更できます。" |
| `propose.title_empty` | when a new page has no title, or a part of it is empty | "A title cannot be empty, and no part of it between slashes can be empty." | "タイトルは空にできません。スラッシュで区切った各部分も空にできません。" |
| `propose.title_reserved` | when a part of a title is a dot or two dots | "A part of a title cannot be a single dot or two dots." | "タイトルの一部をドット 1 つや 2 つだけにすることはできません。" |
| `propose.title_too_long` | when a part of a title is too long | "A part of the title is too long." | "タイトルの一部が長すぎます。" |
| `propose.title_too_deep` | when the whole title is too long or has too many parts | "The title is too long, or has too many parts." | "タイトルが長すぎるか、部分が多すぎます。" |
| `propose.title_taken` | when a new page has the title of a page that exists; `{title}` is it | "A page titled {title} exists already. Propose a change to it instead." | "{title} というタイトルのページはすでにあります。代わりにそのページへの変更を提案してください。" |
| `changes.proposal_heading` | the heading of what a proposal changes | "What it changes" | "変更内容" |
| `changes.proposal_none` | when every page is the page as it is | "The text is the same as the page now: there is nothing to propose." | "文章がいまのページと同じです。提案する変更はありません。" |
| `changes.page_heading` | the heading of one page among the changes; `{title}` is its title | "Page: {title}" | "ページ: {title}" |
| `changes.status_added` | under it: the proposal brings the page | "A new page." | "新しいページです。" |
| `changes.status_removed` | under it: the proposal removes the page | "The page is removed." | "このページは削除されます。" |
| `changes.status_modified` | under it: the proposal changes the page | "The page is changed." | "このページは変更されます。" |
| `changes.info_heading` | the heading of a change to the information at the top of a page | "Information at the top of the page" | "ページ先頭の情報" |
| `changes.part_changed` | the heading of a part that differs; `{number}` is its position | "Part {number} is changed" | "部分 {number} が変更されます" |
| `changes.part_added` | the heading of a part that is new | "Part {number} is added" | "部分 {number} が追加されます" |
| `changes.part_removed` | the heading of a part that is removed | "Part {number} is removed" | "部分 {number} が削除されます" |
| `changes.unchanged_parts` | under a page: how many parts are the same on both sides | "{count} part is the same on both sides." / "{count} parts are the same on both sides." | "{count} 件の部分は変更されません。" |
| `title.proposal` | the title of a proposal's page; `{summary}` is its summary | "Proposal: {summary} · {wiki} · {product}" | "提案: {summary} · {wiki} · {product}" |
| `proposal.proposed_by` | a fact about a proposal; `{when}` is when it was sent | "Proposed by {author}, {when}." | "提案者: {author}、{when}。" |
| `proposal.pages` | a fact: how many pages it changes | "It changes {count} page." / "It changes {count} pages." | "変更するページ: {count} 件。" |
| `proposal.revision` | a fact: which revision the page shows | "This is revision {number} of {total}." | "{total} 件中の {number} 番目の修正です。" |
| `proposal.view` | a fact: the view the proposal is for | "It is for the view {view}." | "ビュー {view} に対する提案です。" |
| `proposal.sources_heading` | the heading of the sources | "Sources" | "出典" |
| `proposal.signed_change` | the link from an accepted proposal to its change in the history | "See the change" | "変更を見る" |
| `proposal.actions_heading` | the heading of what the reader can do | "What you can do" | "できること" |
| `action.continue` | the link that continues a draft | "Continue editing" | "編集を続ける" |
| `action.revise` | the link that revises a proposal | "Revise this proposal" | "この提案を修正する" |
| `action.withdraw` | the link to the form that takes a proposal back | "Withdraw this proposal" | "この提案を取り下げる" |
| `action.ask` | the link to the form that asks for a revision | "Ask for a revision" | "修正を求める" |
| `action.decline` | the link to the form that declines a proposal | "Decline this proposal" | "この提案を見送る" |
| `action.accept` | the button that accepts a proposal | "Accept" | "受け入れる" |
| `thread.heading` | the heading of the discussion on a proposal | "Discussion" | "ディスカッション" |
| `thread.none` | when nobody has commented | "Nobody has commented yet." | "まだコメントはありません。" |
| `comment.meta` | under a comment's author: when, and the revision it was made on | "{when} · on revision {number}" | "{when} · 修正 {number} について" |
| `comment.on_part` | on a comment about a part that is not among the changes shown | "About part {number} of {title}" | "{title} の部分 {number} について" |
| `comment.orphaned` | on a comment whose part is no longer in the proposal; it was about the text quoted | "Orphaned: the part of {title} this comment was about is no longer in the proposal. It was about this text:" | "孤立したコメント: このコメントの対象だった {title} の部分は、提案から無くなっています。対象だった文章:" |
| `comment.on_this_part` | the link that comments on a part; `{number}` is its position | "Comment on part {number}" | "部分 {number} にコメントする" |
| `comment.reply` | the link that replies to a comment | "Reply to {author}" | "{author} さんに返信する" |
| `comment_form.label` | the label of the comment text | "Add a comment" | "コメントを書く" |
| `comment_form.button` | the button that sends a comment | "Comment" | "コメントする" |
| `comment_form.about_part` | on the comment form: what the comment is about | "Your comment is about part {number} of {title}." | "このコメントは {title} の部分 {number} についてです。" |
| `comment_form.reply_to` | on the comment form: whom it replies to | "Your comment is a reply to {author}." | "このコメントは {author} さんへの返信です。" |
| `decisions.heading` | the heading of the reviewers' decisions | "Decisions" | "判断" |
| `decisions.none` | when no reviewer has decided | "No reviewer has decided yet." | "まだ判断したレビュアーはいません。" |
| `decision.approved` | a decision: approved; `{number}` is the revision it is on | "{reviewer} approved, on revision {number}." | "{reviewer} さんが承認しました（修正 {number} について）。" |
| `decision.requested` | a decision: changes requested | "{reviewer} asked for changes, on revision {number}." | "{reviewer} さんが変更を求めました（修正 {number} について）。" |
| `decision.older` | under a decision on an earlier revision | "A newer revision was written after this. The decision no longer counts toward accepting it." | "この後に新しい修正が書かれました。この判断は、受け入れの条件としてはもう数えられません。" |
| `decide.label` | the label of the note with a decision | "A note with your decision (optional)" | "判断に添えるメモ（省略可）" |
| `decide.approve` | the button that approves the revision | "Approve this revision" | "この修正を承認する" |
| `decide.request` | the button that asks for changes | "Request changes" | "変更を求める" |
| `title.decline` | the title of the form that declines a proposal | "Decline a proposal · {wiki} · {product}" | "提案を見送る · {wiki} · {product}" |
| `title.withdraw` | the title of the form that takes a proposal back | "Withdraw a proposal · {wiki} · {product}" | "提案を取り下げる · {wiki} · {product}" |
| `title.ask` | the title of the form that asks for a revision | "Ask for a revision · {wiki} · {product}" | "修正を求める · {wiki} · {product}" |
| `reason.decline_heading` | the heading of the form that declines a proposal | "Decline this proposal" | "この提案を見送る" |
| `reason.decline_intro` | under it | "Thank the proposer for the effort, say why the proposal is not taken into the history, and say what could be done next. The proposer is told, and everyone who reads the proposal sees your reason." | "提案者の労をねぎらい、履歴に取り込まない理由と、次にできることを書いてください。提案者に伝わり、提案を読む人全員に理由が表示されます。" |
| `reason.decline_button` | the button that declines | "Decline" | "見送る" |
| `reason.withdraw_heading` | the heading of the form that takes a proposal back | "Withdraw this proposal" | "この提案を取り下げる" |
| `reason.withdraw_intro` | under it | "Say why you are taking the proposal back. It stays here for the record, and everyone who reads it sees your reason." | "提案を取り下げる理由を書いてください。提案は記録として残り、読む人全員に理由が表示されます。" |
| `reason.withdraw_button` | the button that withdraws | "Withdraw" | "取り下げる" |
| `reason.ask_heading` | the heading of the form that asks for a revision | "Ask for a revision" | "修正を求める" |
| `reason.ask_intro` | under it | "Say what needs to change, and why, so that the proposer knows what to do. The proposer is told and can send a new revision." | "何をなぜ直す必要があるのかを書いて、提案者が次に何をすればよいか分かるようにしてください。提案者に伝わり、新しい修正を送れます。" |
| `reason.ask_button` | the button that asks for a revision | "Ask for a revision" | "修正を求める" |
| `reason.label` | the label of the reason | "Reason" | "理由" |
| `title.discussion` | the title of the discussion of a page | "Discussion of {page} · {wiki} · {product}" | "{page} のディスカッション · {wiki} · {product}" |
| `discussion.heading` | its heading | "Discussion of {page}" | "{page} のディスカッション" |
| `discussion.intro` | under it | "What has been said about this page so far: the proposals that change it, each with its discussion." | "このページについてこれまでに語られたこと。ページを変更する提案と、それぞれのディスカッションです。" |
| `discussion.empty` | when no proposal touches the page | "No proposal changes this page yet." | "このページを変更する提案はまだありません。" |
| `discussion.cut` | when the proposals looked through are cut; `{limit}` is how many | "Only the newest {limit} proposals were looked through." | "新しい {limit} 件の提案のみ確認しました。" |
| `discussion.page` | the link to the page; `{page}` is its title | "Read {page}" | "{page} を読む" |
| `discussion.propose` | the link to the form that proposes a change to the page | "Propose a change to this page" | "このページへの変更を提案する" |
| `lint.bom` | a suggestion for a text, from the linter's finding `MKUP-19-bom`; `{guideline}` is the guideline it comes from | "{guideline}: The text starts with a byte-order mark." | "{guideline}: テキストの先頭にバイトオーダーマークがあります。" |
| `lint.crlf` | a suggestion for a text, from the linter's finding `MKUP-19-crlf`; `{guideline}` is the guideline it comes from | "{guideline}: The page uses CR LF line endings; LF is recommended." | "{guideline}: このページの改行は CR LF です。LF をおすすめします。" |
| `lint.newline` | a suggestion for a text, from the linter's finding `MKUP-19-newline`; `{guideline}` is the guideline it comes from | "{guideline}: The text does not end with a line break." | "{guideline}: テキストの末尾に改行がありません。" |
| `lint.tabs` | a suggestion for a text, from the linter's finding `MKUP-19-tabs`; `{guideline}` is the guideline it comes from | "{guideline}: A tab character is used outside code; spaces are recommended." | "{guideline}: コード以外でタブ文字が使われています。空白をおすすめします。" |
| `lint.no_info` | a suggestion for a text, from the linter's finding `META-1-missing`; `{guideline}` is the guideline it comes from | "{guideline}: The page has no information at the top. A title there makes the page portable." | "{guideline}: ページ先頭に情報がありません。タイトルを書くと、ページを持ち運びやすくなります。" |
| `lint.no_title` | a suggestion for a text, from the linter's finding `META-2-title`; `{guideline}` is the guideline it comes from | "{guideline}: The information at the top of the page has no title." | "{guideline}: ページ先頭の情報にタイトルがありません。" |
| `lint.coerced` | a suggestion for a text, from the linter's finding `META-3-coerced`; `{guideline}` is the guideline it comes from | "{guideline}: A value at the top of the page was read as another kind of value than you may mean." | "{guideline}: ページ先頭の値が、意図と異なる種類の値として読まれました。" |
| `lint.list` | a suggestion for a text, from the linter's finding `META-4-list`; `{guideline}` is the guideline it comes from | "{guideline}: A value at the top of the page that should be a list of texts is not one." | "{guideline}: ページ先頭の値のうち、文字列の一覧であるべきものが一覧になっていません。" |
| `lint.status` | a suggestion for a text, from the linter's finding `META-6-status`; `{guideline}` is the guideline it comes from | "{guideline}: The status at the top of the page is not one of the recommended ones." | "{guideline}: ページ先頭の状態が、推奨されている値ではありません。" |
| `lint.redirect` | a suggestion for a text, from the linter's finding `META-7-redirect`; `{guideline}` is the guideline it comes from | "{guideline}: The page is a redirect but names no page to redirect to." | "{guideline}: このページはリダイレクトですが、移動先のページが書かれていません。" |
| `lint.visibility` | a suggestion for a text, from the linter's finding `META-13-visibility`; `{guideline}` is the guideline it comes from | "{guideline}: The visibility at the top of the page is not public, internal or restricted." | "{guideline}: ページ先頭の公開範囲が、公開・内部・制限のどれでもありません。" |
| `lint.secret` | a suggestion for a text, from the linter's finding `META-14-secret`; `{guideline}` is the guideline it comes from | "{guideline}: A name at the top of the page looks like a secret. A page is not the place for secrets." | "{guideline}: ページ先頭に、秘密情報のように見える名前があります。ページは秘密を書く場所ではありません。" |
| `lint.duplicate_id` | a suggestion for a text, from the linter's finding `MKUP-9-duplicate`; `{guideline}` is the guideline it comes from | "{guideline}: Two parts of the page have the same identifier." | "{guideline}: ページ内の 2 つの部分が同じ識別子を持っています。" |
| `lint.unclosed` | a suggestion for a text, from the linter's finding `EXT-4-unclosed`; `{guideline}` is the guideline it comes from | "{guideline}: A container is opened and never closed." | "{guideline}: 開いたコンテナが閉じられていません。" |
| `lint.callout` | a suggestion for a text, from the linter's finding `MKUP-15-callout`; `{guideline}` is the guideline it comes from | "{guideline}: A callout is not one of the usual kinds." | "{guideline}: 注意書きの種類が、よく使われるものではありません。" |
| `lint.container` | a suggestion for a text, from the linter's finding `MKUP-15-container`; `{guideline}` is the guideline it comes from | "{guideline}: A container has a name this wiki does not know." | "{guideline}: このウィキが知らない名前のコンテナがあります。" |
| `lint.heading_top` | a suggestion for a text, from the linter's finding `MKUP-8-h1`; `{guideline}` is the guideline it comes from | "{guideline}: The page uses a top-level heading, or skips a heading level." | "{guideline}: ページが最上位の見出しを使っているか、見出しの段階を飛ばしています。" |
| `lint.heading_dup` | a suggestion for a text, from the linter's finding `MKUP-8-duplicate-heading`; `{guideline}` is the guideline it comes from | "{guideline}: Two headings have the same text, so their links share a name." | "{guideline}: 同じ文面の見出しが 2 つあり、リンク先の名前が重なります。" |
| `lint.tags` | a suggestion for a text, from the linter's finding `MKUP-14-tags`; `{guideline}` is the guideline it comes from | "{guideline}: A tag is not written the usual way." | "{guideline}: タグの書き方が一般的な形ではありません。" |
| `lint.empty_label` | a suggestion for a text, from the linter's finding `MKUP-4-empty-label`; `{guideline}` is the guideline it comes from | "{guideline}: A link has an empty label." | "{guideline}: ラベルが空のリンクがあります。" |
| `lint.empty_target` | a suggestion for a text, from the linter's finding `MKUP-4-empty`; `{guideline}` is the guideline it comes from | "{guideline}: A link has no target." | "{guideline}: 行き先のないリンクがあります。" |
| `lint.missing_part_ref` | a suggestion for a text, from the linter's finding `MKUP-9-missing-block`; `{guideline}` is the guideline it comes from | "{guideline}: A link to a part of this page points to a part that does not exist." | "{guideline}: このページ内の部分へのリンクが、存在しない部分を指しています。" |
| `lint.prefix` | a suggestion for a text, from the linter's finding `MKUP-13-prefix`; `{guideline}` is the guideline it comes from | "{guideline}: A link names a place with a prefix the wiki does not use." | "{guideline}: リンクが、このウィキで使われない接頭辞で場所を指しています。" |
| `lint.order` | a suggestion for a text, from the linter's finding `MKUP-6-order`; `{guideline}` is the guideline it comes from | "{guideline}: A link could mean more than one page; the nearest one is taken." | "{guideline}: リンクが複数のページを指しうるため、いちばん近いページが選ばれます。" |
| `lint.dangling` | a suggestion for a text, from the linter's finding `NAV-3-dangling`; `{guideline}` is the guideline it comes from | "{guideline}: A link leads to a page that does not exist yet." | "{guideline}: まだ存在しないページへのリンクがあります。" |
| `lint.ambiguous` | a suggestion for a text, from the linter's finding `MKUP-5-ambiguous`; `{guideline}` is the guideline it comes from | "{guideline}: A link matches several pages and none is nearer." | "{guideline}: リンクが複数のページに当てはまり、どれも近くありません。" |
| `lint.other` | a suggestion for a text, from the linter's finding `any other code`; `{guideline}` is the guideline it comes from | "{guideline}: The linter has a suggestion for this text." | "{guideline}: このテキストについて、リンターからの提案があります。" |

## 3. Words the interface does not say

### 3.1 Retired words

The version-control vocabulary never appears in a message, in either language, **except inside an
explanation the reader opens on demand** (a message whose key starts `explain.`, shown only inside a native
`<details>`). English words are matched whole, whatever their case, and not inside a `{placeholder}`; a
Japanese term is matched anywhere in the text. A legitimate exception goes in
`crates/inklkunn-web/tests/retired_words.allow`, with its reason.

| Retired in English | Retired in Japanese | Say instead | Why |
|---|---|---|---|
| seal, seals, sealed, unsealed, sealing | シール, 封印 | signed into the history | prikk's verb for a maintainer signing work into the history; a reader knows "signed" |
| commit, commits, committed, committing | コミット | change (a recorded step); save (an action in a form) | A version-control verb; a wiki reader sees changes |
| rebase, rebases, rebased | リベース | re-authored against the current page | What the wiki does when the page moved under a proposal (WEB-04); the reader is told the result |
| patch, patches | パッチ | change (the recorded step); edit (a part of a change) | prikk's smallest edit; one word, "change", for what the reader sees |
| block, blocks | ブロック | change | prikk's recorded unit of history; the glossary's word is "change" |
| ref, refs | リファレンス | view | A version-control name for a branch or a tag; the wiki's word is "view" |

## 4. Japanese: what the reviewers are asked

The Japanese is a **draft** by the dev team: it is compiled in and checked by the tests (every key, the same
placeholders as English, no markup, no retired term, the structural checks on every page), and it is offered
labelled as a preview. **A preview is chosen only by the reader**, on the language page or by the switcher in
any page's footer; a browser's `Accept-Language` never selects it, so nobody is given an unchecked translation
without having asked for it. It becomes finished when the reviewers the owner names (DR-7) have gone through
this page and the catalog, and the dev team has integrated their corrections in one commit with the sign-off
recorded in `JA_DEF.review` (`catalog.rs`), this page's status line and the accessibility statement; from then
on `Accept-Language: ja` selects it like any other language.

What to look at first:

- **The hard terms** of section 1 (ja column marked *hard term*): 変更 for *change* and 提案 for *proposal* (two
  different things; the draft keeps them apart), レビュー待ちで保持中 for *held for review*, 受け入れ済み and
  見送り済み for *accepted* and *declined* (見送り is gentler than 却下, which the draft avoids on purpose),
  未作成のページ for *wanted page*, ビュー for *view*, and the four trust labels, which carry meaning and must not
  read as a lasting guarantee (検証済み) or as a verification that did not happen (署名して履歴に記録済み).
- **The signed line of a change** (`change.signed_by`, handoff 003 T5): ウィキサービスが {proposer} に代わって署名し、
  {acceptor} が受け入れました。 It must say that the *service* signed *for* the proposer and that a person
  *accepted*; it must never read as if a person signed. Also the people lines of a history (提案者, 受け入れた人),
  and the word 変更 for a change in a sentence that also names 提案.
- **Tone.** The polite register throughout; headings and labels are noun phrases; a refusal says what happened,
  what was not done, and what to do next.
- **Cookie** is written in Latin letters, as many Japanese sites do; クッキー is the alternative.
- **The preview sentence** (`language.preview_note`) says "a fluent reader has not checked it": 日本語に堪能な方
  follows the wording of DR-7; the reviewers may prefer a plainer phrase.

Japanese has no grammatical plural, so a counted message has one form (`"{count} 件"`); numbers group with
commas (`1,200`).

## 5. How this page is used

- **Change a message's words:** change its row in section 2 and the catalog in the same commit.
  `tests/glossary.rs` fails when a row and the catalog disagree, when a catalog message has no row, and when a
  row names a message that does not exist.
- **Add a message:** add a variant to `keys!` in `catalog.rs` (`PageNewThing => "page.new_thing"`; the
  identifier is dotted and stable), an entry to `catalog/en.rs` **and** `catalog/ja.rs` in the same order,
  `msg(…)` for a plain one or `plural_msg(…)` for a counted one, a row in section 2, and use it with
  `page.get(Key::PageNewThing)` in a template or `t.msg(Key::…).with("name", value)` for a sentence with
  placeholders. A sentence that holds a link or a name in another language takes a typed slot
  (`.link(…)`, `.in_lang(…)`): the catalog never holds markup, and `|safe` is never used on catalog output.
  `cargo test -p inklkunn-web --test catalog_checks` says what is missing, unused or malformed.
- **Add a word:** a row in section 1 first, with its reason; the catalog follows.
- **Retire a word:** a row in section 3.1; the test then finds every message that still uses it.
- **Add a language:** a table `xx.rs` with every key in the English order, a `LangDef` with
  `review: Review::Preview`, a line in `languages()`, a column in sections 1 and 2, and the reviewers' pass
  before `Review::Reviewed`.
