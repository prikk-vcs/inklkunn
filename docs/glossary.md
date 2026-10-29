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
| Work someone puts forward for review before it becomes part of a page | proposal | 提案 | not yet | "Change" is taken by the recorded step; one word for two things is the confusion this page exists to remove | Reused from planeter · ja: hard term |
| A proposal the wiki service has received and keeps: it is not part of the history yet | held for review | レビュー待ちで保持中 | not yet | Says what is true and no more: the service holds the work, nothing is approved for merging. As a label: *Held for review* (RFC 002 D-6) | Named by the handoff · ja: hard term |
| A proposal that an authorized person took into the history | accepted | 受け入れ済み | not yet | RFC 002 D-6, WEB-09; "approved" is a reviewer's opinion about one revision and means something else. As a label: *Accepted* | Drafted · ja: hard term |
| A proposal that an authorized person did not take into the history | declined | 見送り済み | not yet | RFC 002 D-6, WEB-09; calm, assumes good faith, and the message beside it names the next step. As a label: *Declined* | Drafted · ja: hard term |
| A page that other pages link to and that does not exist yet | wanted page | 未作成のページ | not yet | Says what the reader can do about it: write it | Named by the handoff · ja: hard term |
| The label that prikk ran all of its checks on the wiki and found no problems | Verified | 検証済み | yes | A trust label: it never means more than the check found, and never appears without it | Reused from planeter · ja: hard term, trust label |
| The label that prikk checked and found problems | Verification failed | 検証に失敗 | yes | "Unverified" would read as "not checked" | Reused from planeter · ja: hard term, trust label |
| The label for a check that could not run | Could not be checked now | 現在は確認できません | yes | Separates "failed" from "could not check"; it claims nothing either way | Reused from planeter · ja: hard term, trust label |
| The label on a change that was signed into the history | Signed into the history | 署名して履歴に記録済み | yes | Plain for "sealed"; on a change page it must not read as "Verified" | Reused from planeter · ja: hard term, trust label |
| inklkunn itself, as the author of record of every change | the wiki service | ウィキサービス | yes | RFC 002 D-6, WEB-09: "signed by the wiki service on behalf of {proposer}, accepted by {acceptor}" says who signed and for whom, and never implies that a person signed cryptographically | Drafted · ja: hard term |
| A page title that sends the reader to another page | redirect | リダイレクト | yes | The usual word; the page says "redirected from {title}" | Drafted |
| Another title a page also answers to | alias | 別名 | not yet | Plain | Drafted |
| An address that always shows a page as it was at one point in its history | permalink | 固定リンク | not yet | The usual word | Drafted |
| The pages that link to this page | backlinks | リンク元 | not yet | The usual word on wikis | Drafted |
| The page exactly as it is stored, as text | raw source | ソース | not yet | The reader's word; it is served from the separate content address | Drafted |
| Where a page's file is kept in the wiki's repository | storage path | 保存先のパス | not yet | Shown only beside the raw source and in the export, and labelled so; a title is the address (RFC 003 D-1) | Drafted |
| A translation no fluent reader has checked yet | preview | プレビュー版 | yes | Said wherever the language is offered, so nobody takes a draft for a finished translation (DR-7) | Drafted |
| The small file a browser keeps for a site | cookie | Cookie | yes | The usual word in both languages; the language choice is the only thing kept for a visitor who is not signed in | Drafted |
| Whoever looks after this installation | the person who runs this server | このサーバーの管理者 | yes | Operator vocabulary stays out of the pages | Reused from planeter |
| A form a page offers to be filled in and sent | form | フォーム | yes | The plain word | Drafted |
| What the language choice is made on | language page | 言語のページ | yes | Names the page the refusal sends the reader to | Drafted |

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
| `page.propose_disabled` | the label of a button that cannot be pressed yet, under a page | "Propose a change" | "変更を提案する" |
| `page.propose_note` | beside the button above | "Proposals arrive with the next release." | "提案の機能は、次のリリースで提供されます。" |
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
| `at.missing_heading` | the heading of the page for a permalink to a point before the page existed, or after it was removed | "The page did not exist then" | "そのときこのページはありませんでした" |
| `at.missing_body` | its sentence; `{title}` is the title asked for, `{change}` the name of the change, which a link under it opens | "After {change}, this wiki had no page titled {title}." | "{change} の後、このウィキに {title} というタイトルのページはありませんでした。" |

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
