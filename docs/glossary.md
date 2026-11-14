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
| Whoever a visitor asks for an account or an invitation | the person who runs this wiki | このウィキを運営している人 | yes | One term wherever the reader is told to ask for an account (corrections G9); "the person who runs this server" stays for faults of the installation | Drafted |
| A form a page offers to be filled in and sent | form | フォーム | yes | The plain word | Drafted |
| What the language choice is made on | language page | 言語のページ | yes | Names the page the refusal sends the reader to | Drafted |
| A page that no other page links to | orphan page | 孤立したページ | yes | The usual wiki word; the health list says what would resolve it | Drafted |
| A page that links to no other page | dead-end page | 行き止まりのページ | yes | The usual wiki word; the health list says what would resolve it | Drafted |
| The lists of what can be improved in a wiki | wiki health | ウィキの健全性 | yes | Names the lists as opportunities, never as faults (NAV-15) | Drafted |
| When this server first saw a change | first seen here | 初めて確認した日時 | yes | The history carries no time, so the only time shown is the server's own observation, named so it is never read as the time of the change | Drafted |
| What a person has so that what they do is theirs | account | アカウント | yes | One person, one account: it is never matched or linked by e-mail address (the sign-in handoff U2) | Drafted |
| Proving who you are to the server, and being let in | sign in | サインイン | yes | The usual word; never "log in" in one place and "sign in" in another | Drafted · ja: loanword |
| A browser that is signed in to an account | session | セッション | yes | The account page lists them so a person can end one; no other word for the same thing | Drafted · ja: loanword |
| A link made for one person to make their account | invitation | 招待 | yes | One-time and ending; the server's operator makes it (DR-11) | Drafted |
| A secret a program uses to act as an account, with a scope | API token | API トークン | yes | Shown once; it can do no more than its scope and the account's roles allow (RFC 002 D-1) | Drafted · ja: loanword |
| The service that vouches for who a person is, when the operator has set one up (OpenID Connect) | provider | プロバイダー | yes | Named by its host (`Sign in with accounts.example.org`) and otherwise just "the provider": a person who has one knows it; never "identity provider" or "SSO", which only an administrator says (the OpenID Connect handoff U3) | Drafted · ja: loanword |
| A statement a paragraph makes that someone has attached sources to or disagreed with | claim | 主張 | yes | RFC 005: only a paragraph someone made a claim has one; the page never says whether a claim is true or false, and a claim has no score | Drafted |
| Where a claim came from, as a person recorded how it bears on the claim | source | 出典 | yes | A source supports or challenges a claim and says how directly; the page never ranks sources | Drafted |
| A paragraph's own mark (`^id`) that names it through every rewording | anchor | アンカー | yes | RFC 005: a chain of changes followed through an anchor is exact; a paragraph without one is traced by matching its text, and the page says so | Drafted · ja: loanword |
| A proposal that changes claims and sources and no page: where a community status, a review date and an availability are set | curation proposal | キュレーション提案 | yes | The only way a community status changes (RFC 005 part 2 D-3): never by a count, a tally or a timer | Drafted · ja: loanword |
| What people say about one claim, apart from what is decided about it | discussion of this claim | この主張についての議論 | yes | RFC 005 part 2 D-11: an epistemic discussion with a label of its own, kept apart from the review of a proposal; it never changes a claim (a decision does, through a proposal people accept), and the page says so beside it | Drafted |

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
| `missing.invite` | its second sentence, an invitation | "You can propose it. A proposal needs an account, and the page exists once someone who is allowed to takes the proposal into the history." | "このページを提案できます。提案にはアカウントが必要で、権限のある人が提案を履歴に取り込むと、ページができます。" |
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
| `verification.explained_by_retirement` | on the verification page, under the result of a check that failed, when every failure is one a key retirement recorded (handoff 004, P2′); `{key}` is the retired maintainer key's id; the label stays "Verification failed" | "The only failures are those the retirement of key {key} recorded. That key was removed from this wiki's trust, so what it signed can no longer be checked; nothing else failed." | "確認に失敗した箇所は、鍵 {key} の失効時に記録されたものだけです。その鍵はこのウィキの信頼から外されたため、その鍵が署名した部分は確認できなくなっています。ほかに失敗はありません。" |
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
| `problem.not_available.detail` | the API's problem detail `not-available`: the detail | "This build of the server does not offer this." | "このサーバーのビルドでは、提供されていません。" |
| `problem.not_available.hint` | the API's problem detail `not-available`: the hint | "Check the API description of this server for what it offers." | "このサーバーが提供するものは、API の説明で確認してください。" |
| `problem.unavailable.title` | the API's problem detail `unavailable`: the title | "Cannot be read now" | "現在は読み取れません" |
| `problem.unavailable.detail` | the API's problem detail `unavailable`: the detail | "The server could not read the wiki just now." | "サーバーが今、ウィキを読み取れませんでした。" |
| `problem.unavailable.hint` | the API's problem detail `unavailable`: the hint | "Try again in a little while." | "しばらくしてから、もう一度お試しください。" |
| `problem.failed.title` | the API's problem detail `failed`: the title | "The request failed" | "リクエストに失敗しました" |
| `problem.failed.detail` | the API's problem detail `failed`: the detail | "Something went wrong on the server, so the request was not served." | "サーバー側で問題が起きたため、リクエストを処理できませんでした。" |
| `problem.failed.hint` | the API's problem detail `failed`: the hint | "Try again in a little while, and tell the person who runs this server if it keeps happening." | "しばらくしてからもう一度お試しになり、続く場合はこのサーバーの管理者にお知らせください。" |
| `problem.method_not_allowed.title` | the API's problem detail `method-not-allowed`: the title | "Method not allowed" | "このメソッドは使えません" |
| `problem.method_not_allowed.detail` | the API's problem detail `method-not-allowed`: the detail | "This address of the API does not take that method." | "この API のこのアドレスでは、そのメソッドを使えません。" |
| `problem.method_not_allowed.hint` | the API's problem detail `method-not-allowed`: the hint | "The API description of this server lists the methods each address takes." | "各アドレスで使えるメソッドは、このサーバーの API の説明に載っています。" |
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
| `health.propose_page` | beside it, in the same row of the wanted pages: the link to the form that proposes the page; `{title}` is that title | "Propose this page ({title})" | "このページを提案する（{title}）" |
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
| `health.knowledge_name` | the name of the list of claim and source files that cannot be read | "Knowledge files that cannot be read" | "読み取れない知識ファイル" |
| `health.knowledge_desc` | what that list shows | "Claim and source files that are not in the form the wiki reads. Nothing is read from them, and the pages are not affected." | "ウィキが読める形になっていない、主張と出典のファイル。これらからは何も読み取られず、ページには影響しません。" |
| `health.col_file` | the heading of the column with a file's place in the repository | "File" | "ファイル" |
| `health.col_rule` | the heading of the column that says what is wrong with a file | "What is wrong" | "問題の内容" |
| `health.knowledge_action` | in each row of that list: what would resolve it | "Propose a corrected file, or propose to remove it." | "修正したファイルを提案するか、ファイルの削除を提案してください。" |
| `health.none_knowledge` | in place of that list when there is no file to list | "Every knowledge file can be read." | "読み取れない知識ファイルはありません。" |
| `health.rule_path` | a reason in that list: the file is in the wrong place or has a badly formed id | "The file is not in knowledge/claims or knowledge/sources, or its name is not a well formed id followed by .yaml." | "ファイルが knowledge/claims にも knowledge/sources にもないか、ファイル名が「正しい形式の id + .yaml」になっていません。" |
| `health.rule_not_text` | a reason in that list | "The file is not UTF-8 text." | "ファイルが UTF-8 のテキストではありません。" |
| `health.rule_too_long` | a reason in that list | "The file is longer than a knowledge file may be." | "ファイルが、知識ファイルとして許される長さを超えています。" |
| `health.rule_syntax` | a reason in that list | "The text is not in the plain YAML subset the wiki reads." | "テキストが、ウィキが読む単純な YAML の範囲に収まっていません。" |
| `health.rule_missing` | a reason in that list | "A key the format requires is missing." | "形式で必須のキーがありません。" |
| `health.rule_unknown` | a reason in that list | "The file has a key the format does not have." | "形式にないキーがあります。" |
| `health.rule_type` | a reason in that list | "A value is not of the shape its key needs." | "値の形が、そのキーに必要な形ではありません。" |
| `health.rule_number` | a reason in that list | "A number appears where the format has none. Write dates and years as text." | "形式に数値はありません。日付や年は文字列で書いてください。" |
| `health.rule_value` | a reason in that list | "A value is outside the words, or the form, its key allows." | "値が、そのキーで許される語や形式の外にあります。" |
| `health.rule_id` | a reason in that list | "An identifier is not well formed." | "識別子の形式が正しくありません。" |
| `health.rule_duplicate` | a reason in that list | "The same entry appears twice." | "同じ項目が 2 回現れています。" |
| `health.rule_bounds` | a reason in that list | "A list or a text is longer than the format allows." | "リストまたはテキストが、形式で許される長さを超えています。" |
| `health.rule_reference` | a reason in that list | "The file names a source or a claim the wiki does not have." | "ウィキにない出典または主張が指定されています。" |
| `health.rule_anchor` | a reason in that list | "An expression names a page, or a paragraph of a page, that the wiki does not have." | "表現が、ウィキにないページまたはページ内の段落を指しています。" |
| `health.rule_in_use` | a reason in that list | "The file is removed while another file still names it." | "別のファイルから指定されているファイルが削除されています。" |
| `health.rule_owned` | a reason in that list | "The change also changes a page key that only the service writes." | "サービスだけが書くページのキーも変更されています。" |
| `health.rule_status` | a reason in that list | "The change sets a community status, a review date or an availability, changes or removes a source link a claim already has, changes a source that was checked in its title, author, date, reference or the source that replaces it, removes a claim or a source that holds any of these, or replaces or removes a file that cannot be read, outside a curation proposal." | "コミュニティの状態、確認日、入手できるかどうかの設定、主張がすでに持つ出典リンクの変更・削除、確認済みの出典の題名・著者・日付・参照先・置き換え先の変更、これらを持つ主張や出典の削除、読み取れないファイルの置き換え・削除が、キュレーション提案ではない変更で行われています。" |
| `health.rule_judged` | a reason in that list | "A judgement the change sets (a source link's or the community status's) is judged on a wording that no paragraph of its claim has in the result." | "変更が設定した判断（出典リンクまたはコミュニティでの扱い）が、その主張のどの段落にも結果として存在しない文言で判断されています。" |
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
| `problem.own_proposal.title` | the API's problem detail `own-proposal`: the title | "Your own proposal" | "自分の提案です" |
| `problem.own_proposal.detail` | the API's problem detail `own-proposal`: the detail | "A proposer cannot approve or ask for changes on their own proposal." | "提案者は、自分の提案を承認したり、修正を求めたりすることはできません。" |
| `problem.own_proposal.hint` | the API's problem detail `own-proposal`: the hint | "Ask a reviewer to decide on it. You can still comment on your own proposal." | "ほかのレビュー担当者に判断を依頼してください。自分の提案にコメントすることはできます。" |
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
| `problem.signing_not_configured.title` | the API's problem detail `signing-not-configured`: the title | "Signing is not set up" | "署名が設定されていません" |
| `problem.signing_not_configured.detail` | the API's problem detail `signing-not-configured`: the detail | "This server is not set up to sign changes into the history, so it takes no proposal into the history; reading, proposing and commenting still work." | "このサーバーには変更を履歴へ署名する設定がないため、提案を履歴に取り込めません。読み取り、提案、コメントはできます。" |
| `problem.signing_not_configured.hint` | the API's problem detail `signing-not-configured`: the hint | "Tell the person who runs this server. This request did nothing." | "このサーバーを運用している人に知らせてください。このリクエストでは何も行われていません。" |
| `problem.retirement_unfinished.title` | the API's problem detail `retirement-unfinished`: the title | "A change to the wiki's keys is not finished" | "ウィキの鍵の変更が完了していません" |
| `problem.retirement_unfinished.detail` | the API's problem detail `retirement-unfinished`: the detail | "A change to this wiki's keys is not finished, so nothing new can be signed into its history until the person who runs this wiki finishes or gives up a change to its keys. The change was not made, and the proposal is kept." | "このウィキの鍵の変更が完了していないため、運用している人が鍵の変更を完了するか取りやめるまで、履歴に新しく署名することはできません。変更は行われておらず、提案はそのまま保たれています。" |
| `problem.retirement_unfinished.hint` | the API's problem detail `retirement-unfinished`: the hint | "Tell the person who runs this wiki. When they have finished or given up the change, accept the proposal again." | "このウィキを運用している人に知らせてください。完了または取りやめになったら、もう一度この提案を受け入れてください。" |
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
| `outcome.knowledge_invalid` | the outcome of an acceptance refused before signing because a claim or source file in the proposal is not acceptable; nothing was changed | "Not signed: a claim or source file is not acceptable" | "署名されませんでした: 主張または出典のファイルを受け付けられません" |
| `outcome_note.knowledge_invalid` | under that outcome | "A claim or source file in this proposal is not in the form the wiki reads, or the change would leave another file pointing at something that is not there. Nothing was changed, and the proposal is still held for review. Correct the file and accept again." | "この提案の主張または出典のファイルが、ウィキが読める形になっていないか、この変更によって別のファイルが存在しないものを指すことになります。何も変更しておらず、提案はレビュー待ちのままです。ファイルを直してから、もう一度受け入れてください。" |
| `outcome.too_large` | the outcome of an acceptance that needed more memory than one signing may use | "Too large to sign now" | "大きすぎて今は署名できません" |
| `outcome_note.too_large` | under the outcome too large to sign now | "This change needs more memory than one signing may use. Nothing was changed, and the proposal is still held for review." | "この変更の署名には、1 回の署名で使える量を超えるメモリが必要です。何も変更しておらず、提案はレビュー待ちのままです。" |
| `outcome.retirement_unfinished` | the outcome of an acceptance the signer refused because a change to the wiki's keys (a key retirement) was begun and is not finished; nothing was changed | "Not signed: a change to this wiki's keys is not finished" | "署名されませんでした: このウィキの鍵の変更が完了していません" |
| `outcome_note.retirement_unfinished` | under that outcome; the person who runs this wiki finishes or gives up the change | "A change to this wiki's keys is not finished, so nothing new can be signed into its history until the person who runs this wiki finishes or gives up a change to its keys. Nothing was changed, and the proposal is kept. Tell that person; once it is finished or given up, accept the proposal again." | "このウィキの鍵の変更が完了していないため、運用している人が鍵の変更を完了するか取りやめるまで、履歴に新しく署名することはできません。何も変更しておらず、提案はそのまま保たれています。その人に伝えてください。完了または取りやめになったら、もう一度この提案を受け入れてください。" |
| `outcome.unavailable` | the outcome of an acceptance that could not be done now, before anything was changed | "Could not be done now" | "今は実行できません" |
| `outcome_note.unavailable` | under the outcome could not be done now | "The records or the history could not be used just now. Nothing was changed. Try again in a little while; if it keeps happening, tell the person who runs this server." | "記録または履歴を今は利用できませんでした。何も変更していません。しばらくしてからもう一度お試しください。何度も起きる場合は、このサーバーを運用している人に伝えてください。" |
| `outcome.in_progress` | the outcome of an acceptance of a revision whose earlier acceptance has not finished | "This revision is being signed" | "この修正版は署名の最中です" |
| `outcome_note.in_progress` | under the outcome this revision is being signed | "An earlier attempt to take this revision into the history has not finished. Nothing was done now. Wait a little and read the proposal again." | "この修正版を履歴に取り込む以前の操作が、まだ終わっていません。今回は何も行っていません。しばらく待ってから、提案をもう一度開いてください。" |
| `outcome.nothing_to_change` | the outcome of an acceptance whose changes the history already has | "Nothing to change" | "変更するものがありません" |
| `outcome_note.nothing_to_change` | under the outcome nothing to change | "The history already has everything this proposal asks for. Nothing was changed." | "この提案が求める内容は、すでに履歴に入っています。何も変更していません。" |
| `outcome.store` | the outcome of an acceptance when the records failed and it is not known whether the change is in the history | "The result is not certain" | "結果を確定できません" |
| `outcome_note.store` | under the outcome the result is not certain | "The records could not be used while the change was being taken in. It may or may not be in the history now: read the history before you try again, and tell the person who runs this server." | "変更を取り込む途中で、記録を利用できなくなりました。変更が履歴に入ったかどうかは分かりません。もう一度試す前に履歴を確認し、このサーバーを運用している人に伝えてください。" |
| `accept.heading` | the heading of the page that answers the Accept button in a build that does not serve accepting (the shipped server always does; a build without an acceptor, as the tests make one, says this) | "Accepting is not available in this build" | "このビルドでは受け入れを利用できません" |
| `accept.not_served` | on that page | "This build of the server does not take proposals into the history." | "このサーバーのビルドは、提案を履歴に取り込みません。" |
| `accept.not_served_kept` | on that page | "The proposal stays held for review and nothing was changed." | "提案はレビュー待ちのままで、何も変更していません。" |
| `accept.not_served_note` | beside the Accept button, in such a build | "Accepting is not available on this server." | "このサーバーでは、受け入れを利用できません。" |
| `accept.unconfigured_heading` | the heading of the page that answers the Accept button when the server was started without the means to sign | "Accepting is not set up on this server" | "このサーバーでは受け入れが設定されていません" |
| `accept.unconfigured` | on that page: the one sentence | "This server is not set up to sign changes into the history, so nothing was changed and the proposal stays held for review; tell the person who runs this server." | "このサーバーには履歴へ署名する設定がないため、何も変更しておらず、提案はレビュー待ちのままです（このサーバーを運用している人に伝えてください）。" |
| `accept.unconfigured_note` | beside the Accept button, on such a server | "Accepting is not set up on this server." | "このサーバーでは、受け入れが設定されていません。" |
| `overlap.heading` | the heading of the section of a proposal that needs a revision because it overlaps the wiki's current text | "Where the changes overlap" | "変更が重なっている箇所" |
| `overlap.intro` | under that heading | "The wiki's current text and this proposal both changed the places below. Nothing was changed. Revise the proposal with both texts in front of you." | "ウィキの現在の文面とこの提案が、下の箇所をどちらも変更しています。何も統合していません。両方の文面を見ながら、提案を修正してください。" |
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
| `quota.comments_hour_person` | the name of the limit on comments a person makes in an hour | "too many comments in the last hour" | "直近 1 時間のコメントが多すぎます" |
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
| `needs_account.advice` | on that page | "Ask the person who runs this wiki how to get an account." | "アカウントの作り方は、このウィキを運営している人にお尋ねください。" |
| `refused.not_allowed` | under the heading not allowed, for an act on a proposal | "Your role in this wiki does not include this. Nothing was changed." | "このウィキでのあなたの役割には、この操作が含まれていません。何も変更していません。" |
| `refused.not_now_heading` | the heading of the page that says an act is not possible now | "Not possible now" | "今はできません" |
| `refused.invalid_heading` | the heading of the page that says what was sent could not be used | "This could not be saved" | "保存できませんでした" |
| `refused.not_in_this_state` | under that heading; `{state}` is the proposal's state | "This proposal is {state}, so this cannot be done now." | "この提案は「{state}」の状態なので、今はこの操作はできません。" |
| `refused.already_decided` | when a reviewer decides twice on one revision | "You have already decided on this revision. A decision is not changed; decide again on the next revision." | "この修正にはすでに判断を記録しています。判断は変更できません。次の修正であらためて判断してください。" |
| `refused.own_proposal` | when a proposer tries to approve or ask for changes on their own proposal | "You cannot approve or ask for changes on your own proposal. Ask a reviewer to decide on it; you can still comment." | "自分の提案を承認したり、修正を求めたりすることはできません。ほかのレビュー担当者に判断を依頼してください。コメントはできます。" |
| `refused.stale_revision` | when a decision is on a revision that is not the latest; `{latest}` is the latest | "This proposal has a newer revision, number {latest}. Read it again and decide on that one." | "この提案には新しい修正（{latest} 番）があります。読み直して、そちらについて判断してください。" |
| `refused.superseder` | when a proposal that cannot replace another is named; `{state}` is its state | "A proposal that is {state} cannot replace another one." | "「{state}」の提案は、別の提案に代わることはできません。" |
| `refused.changed` | when the proposal changed while a form was being filled in | "The proposal changed while you were working on it. Read it again, then try again." | "作業している間に提案が変更されました。読み直してから、もう一度お試しください。" |
| `invalid.incomplete` | when a form is not the one a page offers | "The form was not complete. Open it again from the page and try again." | "フォームの内容が不完全でした。ページからもう一度開いて、お試しください。" |
| `invalid.path_collides` | when a new page's path or the target of a move collides with a page that stays, or with another new path of the same proposal | "A page whose name differs from this one only in capital letters, or the page you are moving onto, exists already. Choose another title." | "大文字と小文字の違いしかない名前のページ、または移動先のページがすでにあります。別のタイトルにしてください。" |
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
| `invalid.comment_nul` | when a comment on a claim holds the character U+0000 (a bundle could not carry it as it was written) | "A comment cannot hold the character U+0000. Take it out and send the comment again." | "コメントには文字 U+0000 を含められません。取り除いて、もう一度送ってください。" |
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
| `header.account_nav` | the label of the account strip in the banner (for a screen reader) | "Account" | "アカウント" |
| `header.signed_in` | the banner, a link to the account page; `{name}` is the signed-in person's name | "Signed in as {name}" | "{name} としてサインイン中" |
| `header.sign_out` | the banner and the sign-out page, the button that signs the browser out | "Sign out" | "サインアウト" |
| `header.sign_in` | the banner of a signed-out visitor, a link to the sign-in page; shown only where sign-in is served | "Sign in" | "サインイン" |
| `title.form_expired` | the title of the page that says a form did not match the sign-in it was sent from | "Form expired · {product}" | "フォームの有効期限が切れました · {product}" |
| `form_expired.heading` | its heading | "The form has expired" | "フォームの有効期限が切れました" |
| `form_expired.body` | its sentence | "This form could not be matched to your sign-in, so nothing was changed. It may be old, or you may have signed in or out since you opened it. Go back, reload the page and try again." | "このフォームをあなたのサインインと照合できなかったため、何も変更していません。フォームが古いか、開いたあとにサインインまたはサインアウトした可能性があります。前のページに戻り、再読み込みしてからもう一度お試しください。" |
| `problem.unavailable` | under a form of the sign-in pages when the server could not do what was asked; nothing was changed | "That did not work this time, and nothing was changed. Try again in a while; if it keeps happening, tell the person who runs this server." | "今回は処理できず、何も変更していません。しばらくしてからもう一度お試しください。何度も起きる場合は、このサーバーの管理者に知らせてください。" |
| `title.sign_in` | the title of the sign-in page | "Sign in · {product}" | "サインイン · {product}" |
| `sign_in.heading` | the heading of the sign-in page | "Sign in" | "サインイン" |
| `sign_in.login_label` | the label of the sign-in name field, on the sign-in page and the pages that make an account | "Sign-in name" | "サインイン名" |
| `sign_in.password_label` | the label of the password field of the sign-in page | "Password" | "パスワード" |
| `sign_in.button` | the button of the sign-in page | "Sign in" | "サインインする" |
| `sign_in.failed` | on the sign-in page after a sign-in that did not work; it never says whether the name exists | "That sign-in name and password did not match. Check both and try again." | "サインイン名とパスワードが一致しませんでした。両方を確かめて、もう一度お試しください。" |
| `sign_in.locked` | on the sign-in page when there have been too many attempts from this name or this address | "There have been too many attempts. Wait a while before trying again." | "試行回数が多すぎます。しばらく待ってからもう一度お試しください。" |
| `sign_in.busy` | on the sign-in page when the server is checking as many passwords as it allows | "The server is busy right now. Try again in a moment." | "サーバーがただいま混み合っています。少し待ってからもう一度お試しください。" |
| `sign_in.already` | on the sign-in page of someone who is signed in; `{name}` is their name | "You are signed in as {name}." | "{name} としてサインインしています。" |
| `sign_in.to_account` | a link on that page to the account page | "Go to your account" | "アカウントへ移動" |
| `sign_in.invitation_only` | under the sign-in form when accounts come only from invitations | "Ask the person who runs this wiki for an invitation." | "招待が必要な場合は、このウィキを運営している人にお願いしてください。" |
| `sign_in.join` | under the sign-in form, a link to the page that makes an account; shown only where anyone may join | "Join this wiki" | "このウィキに参加する" |
| `title.sign_out` | the title of the sign-out page | "Sign out · {product}" | "サインアウト · {product}" |
| `sign_out.heading` | the heading of the sign-out page | "Sign out" | "サインアウト" |
| `sign_out.body` | its sentence, above the button | "This signs this browser out. Other browsers that are signed in to your account stay signed in." | "このブラウザをサインアウトします。ほかのブラウザでのサインインはそのまま続きます。" |
| `sign_out.not_signed_in` | on the sign-out page of a visitor who is not signed in | "You are not signed in." | "サインインしていません。" |
| `title.join` | the title of the page where anyone may make an account | "Join · {product}" | "参加 · {product}" |
| `join.heading` | its heading | "Join this wiki" | "このウィキに参加する" |
| `join.intro` | under that heading | "Choose a sign-in name and a password to make your account. Nobody, including the person who runs this server, can read your password." | "サインイン名とパスワードを決めて、アカウントを作ります。パスワードは、このサーバーの管理者を含め、誰にも読まれません。" |
| `join.button` | the button of that page | "Join" | "参加する" |
| `join_closed.heading` | the heading of the page shown where joining is not open | "Joining is not open" | "参加は受け付けていません" |
| `join_closed.body` | its sentence | "Accounts here are made from invitations. Ask the person who runs this wiki for an invitation." | "ここでのアカウントは、招待から作られます。このウィキを運営している人に招待をお願いしてください。" |
| `title.invite` | the title of the page an invitation link opens | "Invitation · {product}" | "招待 · {product}" |
| `invite.heading` | its heading | "You are invited" | "招待されています" |
| `invite.intro` | under that heading | "Choose a sign-in name and a password to make your account. This link works once. Nobody, including the person who runs this server, can read your password." | "サインイン名とパスワードを決めて、アカウントを作ります。このリンクは 1 回だけ使えます。パスワードは、このサーバーの管理者を含め、誰にも読まれません。" |
| `invite.button` | the button of that page | "Make my account" | "アカウントを作る" |
| `invite_unusable.heading` | the heading of the page an invitation link opens when it cannot be used | "This invitation cannot be used" | "この招待は使えません" |
| `invite_unusable.body` | its sentence; the same for a link that was used, one that ended and one that never existed | "It may have been used already, or it may have ended. Ask the person who runs this wiki for a new one." | "すでに使われたか、期限が切れた可能性があります。新しい招待を、このウィキを運営している人にお願いしてください。" |
| `account_form.name_label` | the label of the name field of the pages that make an account | "Your name" | "あなたの名前" |
| `account_form.name_hint` | under that field | "The name other people see next to what you write. Any script is fine." | "あなたが書いたものの横に、ほかの人へ表示される名前です。どの文字でも使えます。" |
| `account_form.login_hint` | under the sign-in name field of the pages that make an account | "3 to 64 characters: letters a to z, digits, dots, underscores and hyphens. Capital letters count as small ones." | "3 から 64 文字で、英字 a から z、数字、ドット、アンダースコア、ハイフンが使えます。大文字は小文字として扱われます。" |
| `account_form.password_label` | the label of the password field of the pages that make an account | "Choose a password" | "パスワードを決める" |
| `account_form.password_hint` | under that field; `{min}` is the fewest characters | "At least {min} characters. There are no other rules; a long sentence makes a good password." | "{min} 文字以上です。ほかの決まりはありません。長い文章は、よいパスワードになります。" |
| `account_form.login_taken` | a problem: the sign-in name belongs to another account | "That sign-in name is taken. Choose another one." | "そのサインイン名は使われています。別の名前を選んでください。" |
| `account_form.login_length` | a problem: the sign-in name is too short or too long | "A sign-in name has 3 to 64 characters." | "サインイン名は 3 から 64 文字です。" |
| `account_form.login_characters` | a problem: the sign-in name has a character it may not | "A sign-in name uses letters a to z, digits, dots, underscores and hyphens, and starts with a letter or a digit." | "サインイン名には、英字 a から z、数字、ドット、アンダースコア、ハイフンだけを使い、先頭は英字か数字にします。" |
| `account_form.name_length` | a problem: the name is empty or too long | "A name has 1 to 120 characters." | "名前は 1 から 120 文字です。" |
| `account_form.name_control` | a problem: the name has a line break or a control character | "A name cannot contain line breaks or control characters." | "名前に改行や制御文字は使えません。" |
| `account_form.name_space` | a problem: the name starts or ends with a space | "A name cannot start or end with a space." | "名前の先頭と末尾に空白は使えません。" |
| `account_form.password_short` | a problem: the password is too short; `{min}` is the fewest characters | "A password needs at least {min} characters." | "パスワードは {min} 文字以上にしてください。" |
| `account_form.password_long` | a problem: the password is too long | "That password is too long." | "そのパスワードは長すぎます。" |
| `title.account` | the title of the account page | "Your account · {product}" | "あなたのアカウント · {product}" |
| `account.heading` | the heading of the account page | "Your account" | "あなたのアカウント" |
| `account.name_heading` | the heading of its first section | "Your name" | "あなたの名前" |
| `account.name_label` | the label of the field in that section | "Name" | "名前" |
| `account.name_hint` | under that field | "The name other people see next to what you write." | "あなたが書いたものの横に、ほかの人へ表示される名前です。" |
| `account.name_button` | the button of that section | "Save name" | "名前を保存" |
| `account.name_saved` | a note at the top of the page after the name was saved | "Your name was saved." | "名前を保存しました。" |
| `account.sessions_heading` | the heading of the section that lists where the account is signed in | "Where you are signed in" | "サインインしている場所" |
| `account.sessions_intro` | under that heading | "Each line is a browser that is signed in to your account now. Ending a session signs that browser out." | "各行は、いまあなたのアカウントにサインインしているブラウザです。セッションを終了すると、そのブラウザはサインアウトされます。" |
| `account.session_line` | one line of that list; `{started}` and `{ends}` are moments | "Started {started}; ends {ends}" | "開始 {started}、終了予定 {ends}" |
| `account.session_current` | beside the line of the browser being used | "This is the browser you are using." | "いま使っているブラウザです。" |
| `account.session_end` | the button on each line of that list | "End this session" | "このセッションを終了" |
| `account.session_ended` | a note at the top of the page after a session was ended | "That session was ended." | "そのセッションを終了しました。" |
| `account.tokens_heading` | the heading of the section on API tokens | "API tokens" | "API トークン" |
| `account.tokens_intro` | under that heading | "An API token lets a program act as you through the write API. It can do no more than the scope you give it, and no more than your roles allow. It is shown once, when you make it." | "API トークンを使うと、プログラムがあなたとして書き込み API を使えます。トークンにできるのは、与えたスコープの範囲と、あなたの役割で許される範囲までです。作成したときに 1 度だけ表示されます。" |
| `account.tokens_none` | instead of the list when the account has no tokens | "You have no tokens." | "トークンはありません。" |
| `account.token_unlabelled` | the name of a token that was given no note | "A token with no note" | "メモのないトークン" |
| `account.token_scope` | one line of the list of tokens; `{scope}` is one of the five scopes | "Scope: {scope}" | "スコープ: {scope}" |
| `account.token_live` | one line of the list of tokens; `{when}` is a moment | "Works until {when}" | "{when} まで使えます" |
| `account.token_expired` | the same for a token that has ended | "Ended {when}" | "{when} に期限が切れました" |
| `account.token_revoked` | the same for a token its owner revoked | "Revoked {when}" | "{when} に取り消しました" |
| `account.token_revoke` | the button on each live token | "Revoke" | "取り消す" |
| `account.token_revoked_note` | a note at the top of the page after a token was revoked | "That token was revoked." | "そのトークンを取り消しました。" |
| `account.token_new_heading` | the heading of the form that makes a token | "Make a token" | "トークンを作る" |
| `account.token_label` | the label of the note field of that form | "What is it for?" | "用途" |
| `account.token_label_hint` | under that field | "A short note to recognise it by, up to 80 characters." | "見分けるための短いメモです。80 文字までです。" |
| `account.token_scope_label` | the label of the scope field of that form | "Scope" | "スコープ" |
| `account.scope_read` | an option of that field and the name of the scope on the list | "Read: pages, history and proposals" | "読み取り: ページ、履歴、提案" |
| `account.scope_propose` | an option of that field | "Propose: also comment and propose changes" | "提案: コメントと変更の提案も可能" |
| `account.scope_review` | an option of that field | "Review: also review proposals" | "レビュー: 提案のレビューも可能" |
| `account.scope_accept` | an option of that field | "Accept: also accept proposals" | "受け入れ: 提案の受け入れも可能" |
| `account.scope_admin` | an option of that field | "Admin: everything your roles allow" | "管理: あなたの役割で許されるすべて" |
| `account.token_lifetime_label` | the label of the lifetime field of that form | "Works for" | "有効期間" |
| `account.lifetime_day` | an option of that field | "1 day" | "1 日" |
| `account.lifetime_week` | an option of that field | "7 days" | "7 日" |
| `account.lifetime_month` | an option of that field | "30 days" | "30 日" |
| `account.lifetime_quarter` | an option of that field | "90 days" | "90 日" |
| `account.lifetime_year` | an option of that field | "1 year" | "1 年" |
| `account.token_button` | the button of that form | "Make token" | "トークンを作る" |
| `account.token_made_heading` | the heading of the box that shows a token once, at the top of the page | "Your new token" | "新しいトークン" |
| `account.token_made_body` | in that box | "Copy it now. It is shown only this once and cannot be shown again." | "いまコピーしてください。表示されるのは今回 1 度だけで、あとから再表示することはできません。" |
| `account.token_made_use` | in that box | "A program sends it in the Authorization header, as Bearer followed by the token." | "プログラムは、Authorization ヘッダーに Bearer とトークンを続けて送ります。" |
| `account.token_too_many` | a problem: the account holds as many live tokens as it may; `{max}` is the most | "You already have {max} live tokens. Revoke one first." | "有効なトークンがすでに {max} 個あります。先に 1 つ取り消してください。" |
| `account.token_label_long` | a problem: the note is too long | "The note is too long: at most 80 characters." | "メモが長すぎます。80 文字までです。" |
| `sign_in.provider` | the link on the sign-in page that signs in through the provider; `{provider}` is the provider's host name | "Sign in with {provider}" | "{provider} でサインイン" |
| `sign_in.provider_joins` | under that link, where the server lets a first sign-in through the provider make an account | "The first time you sign in with {provider}, an account is made for you." | "{provider} で初めてサインインすると、アカウントが作られます。" |
| `sign_in.provider_failed` | a problem on the sign-in page after a sign-in through the provider did not work (the reason is for the operator's log) | "The sign-in with {provider} did not work, and nothing was changed. Try again; if it keeps happening, tell the person who runs this server." | "{provider} でのサインインがうまくいきませんでした。何も変更されていません。もう一度お試しください。繰り返し起きる場合は、このサーバーの管理者にお知らせください。" |
| `sign_in.provider_state` | a problem: the provider sent the browser back with a sign-in that this browser did not start, that was used already, or that took too long | "This sign-in did not start in this browser, or it has been used already, or it took too long. Start it again from this page." | "このサインインは、このブラウザーで始めたものではないか、すでに使われたか、時間がかかりすぎました。このページからもう一度始めてください。" |
| `sign_in.provider_not_linked` | a problem: the provider vouched for the person, no account is linked to that sign-in and the server does not let a first sign-in make one | "The provider knows you, but no account here is linked to your sign-in there. Ask the person who runs this wiki for an invitation." | "プロバイダーはあなたを確認しましたが、そのサインインに結び付いたアカウントがここにはありません。招待が必要です。このウィキを運営している人にお願いしてください。" |
| `sign_in.provider_unreachable` | a problem: the server could not reach the provider | "The provider could not be reached right now, and nothing was changed. Try again in a while." | "いまプロバイダーに接続できません。何も変更されていません。しばらくしてからもう一度お試しください。" |
| `invite.provider_intro` | on an invitation page, above the link that uses the provider instead of a password | "Or, instead of a password, let your provider vouch for you. Your account is then tied to that sign-in, and there is no password to remember." | "パスワードの代わりに、プロバイダーに本人確認を任せることもできます。その場合、アカウントはそのサインインに結び付き、パスワードを覚える必要はありません。" |
| `invite.provider_link` | the link on an invitation page that makes the account through the provider | "Use {provider} instead of a password" | "パスワードの代わりに {provider} を使う" |
| `invite.provider_taken` | a problem on the sign-in page: the person's sign-in through the provider has an account already, so the invitation was not used | "Your sign-in with {provider} already has an account here, so this invitation was not used. Sign in with {provider} instead." | "{provider} でのサインインにはすでにここのアカウントがあるため、この招待は使われていません。代わりに {provider} でサインインしてください。" |
| `title.landing` | the title of the page that moves the browser on after a sign-in through the provider | "Signing in · {product}" | "サインインしています · {product}" |
| `landing.body` | on that page; `{link}` is the next message | "Signing you in. If this page does not move on by itself, {link}." | "サインインしています。このページが自動で切り替わらない場合は、{link}。" |
| `landing.continue` | the link in that sentence, to the page the person asked for | "continue to the page you asked for" | "続きのページへ進んでください" |
| `sign_in.provider_not_navigation` | a problem: the address that starts a sign-in with the provider was reached by something other than the person opening it themselves (an image or a script on another page) | "A sign-in with a provider starts only when you open its address yourself, from this site's sign-in page. Nothing was started. Use the button on this page." | "プロバイダーでのサインインは、このサイトのサインインのページから、ご自身でアドレスを開いたときにだけ始まります。何も開始されていません。このページのボタンをお使いください。" |
| `account.token_password_label` | the label of the password field of the form that makes a token | "Your password" | "パスワード" |
| `account.token_password_hint` | under that field: why the password is asked again | "Asked again because a token keeps working after you sign out." | "トークンはサインアウトした後も使えるため、パスワードをもう一度確認します。" |
| `account.token_password_wrong` | a problem: the password given to make a token was not right | "The password was not right, so no token was made." | "パスワードが違うため、トークンは作られませんでした。" |
| `account.token_provider_hint` | in the form that makes a token, for an account that signs in through a provider and so has no password | "This account signs in through a provider and has no password. A token can be made only within ten minutes of signing in." | "このアカウントはプロバイダーでサインインし、パスワードはありません。トークンは、サインインしてから 10 分以内にだけ作れます。" |
| `account.token_sign_in_again` | a problem: the account has no password and its sign-in is older than ten minutes | "Your last sign-in was too long ago, so no token was made. Sign out, sign in again, and make the token within ten minutes." | "最後のサインインから時間がたっているため、トークンは作られませんでした。サインアウトしてもう一度サインインし、10 分以内にトークンを作ってください。" |
| `account.password_heading` | the heading of the section of the account page that changes the password | "Password" | "パスワード" |
| `account.password_intro` | under that heading, for an account that has a password | "Changing your password ends every other session of this account and every API token it has. This browser stays signed in." | "パスワードを変更すると、このアカウントの他のセッションとすべての API トークンが終了します。このブラウザーはサインインしたままです。" |
| `account.password_none` | under that heading, for an account that signs in through a provider and has no password | "This account has no password: it signs in through a provider." | "このアカウントにはパスワードがありません。プロバイダーでサインインします。" |
| `account.password_current_label` | the label of the field for the password now in use | "Current password" | "現在のパスワード" |
| `account.password_new_label` | the label of the field for the new password | "New password" | "新しいパスワード" |
| `account.password_button` | the button that changes the password | "Change password" | "パスワードを変更" |
| `account.password_wrong` | a problem: the current password given was not right | "The current password was not right. Nothing was changed." | "現在のパスワードが違います。何も変更されていません。" |
| `account.password_changed` | a note at the top of the account page after the password was changed | "Your password was changed. Every other session and every API token of this account has ended." | "パスワードを変更しました。このアカウントの他のセッションとすべての API トークンは終了しました。" |
| `page.link_why` | a page's tools: the link to the page that shows where each paragraph came from | "Why is this here?" | "なぜここにあるのか" |
| `claim.link` | at the end of a paragraph that holds a claim: the link to that claim's page | "Sources and views" | "出典とビュー" |
| `claim.link_name` | the accessible name of that link; `{paragraph}` says which paragraph | "Sources and views: {paragraph}" | "出典とビュー: {paragraph}" |
| `claim.link_name_nth` | the accessible name of that link where a paragraph holds several claims; `{n}` of `{total}` | "Sources and views, claim {n} of {total}: {paragraph}" | "出典とビュー（主張 {n}/{total}）: {paragraph}" |
| `claim.paragraph_start` | in that name: the paragraph, by its first words (`{text}`) | "paragraph beginning “{text}”" | "「{text}」で始まる段落" |
| `claim.paragraph_empty` | in that name: a paragraph with no words to quote | "paragraph without text" | "文字のない段落" |
| `title.claim` | the title of a claim's page; `{wiki}` is the wiki's name | "Sources and views · {wiki} · {product}" | "出典とビュー · {wiki} · {product}" |
| `claim.heading` | the heading of a claim's page | "Sources and views" | "出典とビュー" |
| `claim.kind` | the first line of a claim's page; `{kind}` is one of the kinds below | "What kind of statement it is: {kind}." | "この主張の種類: {kind}。" |
| `claim.kind_observation` | a kind of statement | "an observation" | "観察" |
| `claim.kind_interpretation` | a kind of statement | "an interpretation" | "解釈" |
| `claim.kind_hypothesis` | a kind of statement | "a hypothesis" | "仮説" |
| `claim.kind_opinion` | a kind of statement | "an opinion" | "意見" |
| `claim.kind_decision` | a kind of statement | "a decision" | "決定" |
| `claim.kind_policy` | a kind of statement | "a policy" | "方針" |
| `claim.kind_question` | a kind of statement | "a question" | "問い" |
| `claim.kind_unknown` | a kind of statement: none was said | "not said" | "示されていません" |
| `claim.said_heading` | the heading of the list of pages that say the claim | "Where it is said" | "この主張が述べられている場所" |
| `claim.said_nowhere` | in place of that list when no page has the paragraph now | "No page has the paragraph this claim is said in, as the pages are now." | "この主張が述べられている段落は、現在のどのページにもありません。" |
| `claim.sources_heading` | the heading of a claim's sources | "Sources" | "出典" |
| `claim.sources_none` | under it when the claim has none | "No source is attached to this claim." | "この主張に付けられた出典はありません。" |
| `claim.source_line` | one source of a claim; `{id}` is its identifier, `{kind}` one of the kinds below, `{how}` one of the phrases below | "Source {id} ({kind}): {how}." | "出典 {id}（{kind}）: {how}。" |
| `claim.source_primary` | a kind of source | "primary" | "一次" |
| `claim.source_secondary` | a kind of source | "secondary" | "二次" |
| `claim.source_observational` | a kind of source | "observational" | "観察" |
| `claim.source_computational` | a kind of source | "computational" | "計算" |
| `claim.source_anecdotal` | a kind of source | "anecdotal" | "逸話" |
| `claim.source_other` | a kind of source | "another kind" | "その他" |
| `claim.source_unreadable` | in place of the kind of a source whose file the wiki cannot read | "its file cannot be read" | "ファイルを読み取れません" |
| `claim.source_line_named` | one source of a claim whose file the wiki can read; `{name}` is made of the messages below, `{kind}` is one of the kinds above, `{how}` one of the phrases below | "{name} ({kind}): {how}." | "{name}（{kind}）: {how}。" |
| `claim.source_titled` | the first part of a source's name: its title, as written | "“{title}”" | "「{title}」" |
| `claim.source_untitled` | in place of it when the source gives no title | "A source without a title" | "題名のない出典" |
| `claim.source_by` | a part of a source's name: its author or publisher, as written | "by {author}" | "{author} による" |
| `claim.source_dated` | a part of a source's name: its date, as written (a year, a season, a day) | "dated {date}" | "日付 {date}" |
| `claim.source_join` | joins two parts of a source's name | "{first}, {second}" | "{first}、{second}" |
| `claim.source_found_at` | after a source's line, when it gives a reference: the reference as written, never made a link | "Where it is found: {reference}." | "所在: {reference}。" |
| `claim.source_identifier` | after the line of a source that gives no title, author, date or reference: its identifier, the one way left to tell it from another | "Its identifier is {id}." | "識別子: {id}。" |
| `claim.how_supports_direct` | how a source bears on a claim: it supports it, and speaks to it directly | "supports, directly" | "直接に裏付けている" |
| `claim.how_supports_partial` | how a source bears on a claim | "supports, in part" | "一部を裏付けている" |
| `claim.how_supports_indirect` | how a source bears on a claim | "supports, indirectly" | "間接的に裏付けている" |
| `claim.how_challenges_direct` | how a source bears on a claim: it challenges it, and speaks to it directly | "challenges, directly" | "直接に異議を唱えている" |
| `claim.how_challenges_partial` | how a source bears on a claim | "challenges, in part" | "一部に異議を唱えている" |
| `claim.how_challenges_indirect` | how a source bears on a claim | "challenges, indirectly" | "間接的に異議を唱えている" |
| `claim.judged_earlier` | beside a source or a status: it was judged on a wording the paragraph no longer has, until a reviewer confirms it | "Judged on an earlier wording." | "以前の文言に基づいて判断されたものです。" |
| `claim.status_heading` | the heading of a claim's status | "Status in this view" | "このビューでの状態" |
| `claim.status_accepted` | a claim's status | "This claim is accepted in this view." | "このビューでは、この主張は認められています。" |
| `claim.status_contested` | a claim's status | "This claim is contested in this view." | "このビューでは、この主張は争われています。" |
| `claim.status_unresolved` | a claim's status | "This claim is unresolved in this view." | "このビューでは、この主張は未解決です。" |
| `claim.status_unknown` | a claim's status: nobody has decided | "This claim has not been decided in this view." | "このビューでは、この主張はまだ決定されていません。" |
| `claim.decided_by` | under a status that was decided; `{proposal}` is the link to the proposal that decided it | "Decided by {proposal}." | "{proposal}によって決定されました。" |
| `claim.decided_link` | the link to that proposal | "the proposal that decided it" | "決定した提案" |
| `claim.decided_unlisted` | under a status that was decided, when the proposal is not among this wiki's records | "Decided by a proposal that this wiki's records do not hold." | "このウィキの記録にない提案によって決定されました。" |
| `claim.none_heading` | the heading when an address names no claim of the wiki | "No such claim" | "そのような主張はありません" |
| `claim.none_body` | under it | "This wiki has no claim at that address, or it cannot be read now." | "このウィキには、そのアドレスの主張がないか、現在は読み取れません。" |
| `why.heading` | the heading of that page | "Why is this here?" | "なぜここにあるのか" |
| `why.back` | on that page: the link back to the page; `{page}` is its title | "Back to {page}" | "{page} に戻る" |
| `title.views` | the title of the page that lists a wiki's views; `{wiki}` is the wiki's name | "Views · {wiki} · {product}" | "ビュー · {wiki} · {product}" |
| `views.heading` | the heading of the page that lists a wiki's views | "Views" | "ビュー" |
| `views.intro` | under that heading | "A view is a named version of the whole wiki. Readers see the default view unless they choose another one." | "ビューは、ウィキ全体の名前付きのバージョンです。読者は、別のビューを選ばない限り、既定のビューを見ます。" |
| `views.default_name` | what the wiki's default view is called wherever it is named; it has no other name | "the default view" | "既定のビュー" |
| `views.named` | a view by its name inside a sentence; `{view}` is the name | "the view “{view}”" | "ビュー「{view}」" |
| `views.default_note` | under the default view in the list | "Shown when no view is named." | "ビューを指定しないときに表示されます。" |
| `views.kind_experimental` | the kind of an alternative view: a working state | "Experimental" | "実験的" |
| `views.kind_interpretation` | the kind of an alternative view: a competing understanding | "Interpretation" | "解釈" |
| `views.kind_experimental_note` | under the kind experimental | "A working state. Its changes may be taken back into the view it was forked from, or dropped." | "作業中の状態です。その変更は、分岐元のビューに取り込まれることも、破棄されることもあります。" |
| `views.kind_interpretation_note` | under the kind interpretation | "A competing understanding, kept beside the default view." | "競合する理解を、既定のビューと並べて保っています。" |
| `views.forked_from` | in the list: where an alternative view was forked from; `{view}` is a view named in words | "Forked from {view}." | "{view}から分岐しました。" |
| `views.no_origin` | in the list: a view the wiki has no record of making | "The wiki did not record where this view came from." | "このビューの出どころは、ウィキに記録されていません。" |
| `views.read_link` | in the list: the link that opens a view; `{view}` is the view named in words | "Read {view}" | "{view}を読む" |
| `views.compare_with_default` | in the list: the link that compares a view with the default view; `{view}` is the view named in words | "Compare {view} with the default view" | "{view}を既定のビューと比べる" |
| `views.omitted` | under the list when the wiki has more views than are listed; `{shown}` and `{omitted}` are numbers | "The first {shown} views are listed here; the other {omitted} are not shown." | "ここには最初の {shown} 件のビューを表示しています。残りの {omitted} 件は表示していません。" |
| `views.only_default` | under the list of a wiki that has only its default view | "This wiki has only its default view." | "このウィキには既定のビューしかありません。" |
| `views.create_link` | beside the list, for a person who may create views: the link to the form | "Create a view" | "ビューを作成する" |
| `views.compare_link` | beside the list: the link to the page that compares two views | "Compare two views" | "2 つのビューを比べる" |
| `views.all_link` | on the about page: the link to the list of the wiki's views | "All views of this wiki" | "このウィキのすべてのビュー" |
| `title.views_new` | the title of the form that creates a view; `{wiki}` is the wiki's name | "Create a view · {wiki} · {product}" | "ビューを作成 · {wiki} · {product}" |
| `views_new.heading` | the heading of the form that creates a view | "Create a view" | "ビューを作成" |
| `views_new.intro` | under that heading | "A view is a named version of the whole wiki, forked from a view that is already there. Anyone who may read it can propose a change in it. Creating a view is recorded: who made it, when, and what it was forked from." | "ビューは、すでにあるビューから分岐させた、ウィキ全体の名前付きのバージョンです。読める人なら誰でも、そのビューに変更を提案できます。ビューの作成は記録されます（作成した人、時刻、分岐元）。" |
| `views_new.name_label` | the label of the field for the new view's name | "Name" | "名前" |
| `views_new.name_hint` | under that field | "Lower-case letters, digits, dots, hyphens and underscores, starting with a letter or a digit; at most 64 characters." | "半角の小文字、数字、ドット、ハイフン、アンダースコア。先頭は小文字か数字で、64 文字までです。" |
| `views_new.kind_legend` | the legend of the choice of what the new view is for | "What the view is for" | "このビューの目的" |
| `views_new.from_label` | the label of the choice of the view to fork from | "Fork it from" | "分岐元" |
| `views_new.submit` | the button that creates the view | "Create the view" | "ビューを作成する" |
| `views_new.problem_name_empty` | a problem: no name was given | "Give the view a name." | "ビューの名前を入力してください。" |
| `views_new.problem_name_start` | a problem: the name does not start with a lower-case letter or a digit | "A name starts with a lower-case letter or a digit." | "名前の先頭は、半角の小文字か数字にしてください。" |
| `views_new.problem_name_chars` | a problem: the name has a character a name may not have | "A name has only lower-case letters, digits, dots, hyphens and underscores, and neither two dots in a row nor a dot at the end." | "名前に使えるのは、半角の小文字、数字、ドット、ハイフン、アンダースコアだけです（ドットの連続や末尾のドットは使えません）。" |
| `views_new.problem_name_long` | a problem: the name is longer than 64 characters | "A name has at most 64 characters." | "名前は 64 文字までです。" |
| `views_new.problem_name_retired` | a problem: the name is one that would call a view better or more settled than another | "That name cannot be given to a view: it would say that this view is better or more settled than the others. Choose a name for what the view is about." | "その名前はビューに付けられません。このビューがほかより優れている、または確定していると読めてしまうためです。ビューの内容を表す名前を選んでください。" |
| `views_new.problem_kind` | a problem: no kind was chosen | "Choose what the view is for." | "このビューの目的を選んでください。" |
| `views_new.problem_from` | a problem: the view to fork from is not one of the wiki's views | "Choose a view that is there to fork it from." | "分岐元には、すでにあるビューを選んでください。" |
| `views_new.problem_taken` | a problem: a view of that name is already there | "A view with that name is already there. Choose another name." | "その名前のビューはすでにあります。ほかの名前を選んでください。" |
| `views_new.not_allowed` | under the outcome not allowed, for creating a view | "Your role in this wiki does not include creating views. Nothing was changed." | "このウィキでのあなたの役割には、ビューを作成する権限がありません。何も変更していません。" |
| `views_new.busy` | under the outcome the wiki is busy, for creating a view | "Another change is being made to the wiki. Nothing was changed; try again in a moment." | "ウィキに別の変更を加えている最中です。何も変更していません。しばらくしてからもう一度お試しください。" |
| `views_new.too_many_heading` | the outcome of creating a view in a wiki that has as many as it may keep | "Too many views" | "ビューが多すぎます" |
| `views_new.too_many` | under that outcome; `{bound}` is the number | "This wiki already has {bound} open views, the most it keeps. Nothing was changed." | "このウィキにはすでに {bound} 件のビューが開いており、これが上限です。何も変更していません。" |
| `views_new.retirement_heading` | the outcome of creating a view while a change to the wiki's keys is not finished | "Not created: a change to this wiki's keys is not finished" | "作成されませんでした: このウィキの鍵の変更が完了していません" |
| `views_new.retirement` | under that outcome | "A change to this wiki's keys is not finished, so nothing new can be made in its history until the person who runs this wiki finishes or gives up that change. Nothing was changed. Tell that person; once it is finished or given up, create the view again." | "このウィキの鍵の変更が完了していないため、運用している人がその変更を完了するか取りやめるまで、履歴に新しいものを作れません。何も変更していません。その人に伝えてください。完了または取りやめの後に、もう一度ビューを作成してください。" |
| `views_new.failed_heading` | the outcome of creating a view that the history refused | "The view was not created" | "ビューは作成されませんでした" |
| `views_new.failed` | under that outcome | "The history did not accept the new view. Nothing was changed." | "履歴は、新しいビューを受け付けませんでした。何も変更していません。" |
| `views_new.uncertain` | under the outcome the result is not certain, for creating a view | "It is not known whether the view was made: the wiki's views could not be read afterwards, or the view is listed and the repository did not say that making it was finished. It may or may not be there. Look at the list of views before you try again, and tell the person who runs this server: the wiki may take no change until that person has looked at it." | "ビューができたかどうかは分かりません。作ったあとでウィキのビューを読めなかったか、ビューは一覧にあるものの、作成が完了したことをリポジトリが示しませんでした。ビューができているかもしれませんし、できていないかもしれません。もう一度試す前にビューの一覧を確認し、このサーバーを運用している人に伝えてください。その人が確認するまで、ウィキは変更を受け付けないことがあります。" |
| `views_new.not_kept_heading` | the outcome of opening the form on a server that cannot create views | "Creating views is not available on this server" | "このサーバーではビューを作成できません" |
| `views_new.not_kept` | under that outcome | "This server does not keep the record a new view needs, so no view can be created here yet. Nothing was changed." | "このサーバーには、新しいビューに必要な記録を保つ仕組みがないため、今はここでビューを作成できません。何も変更していません。" |
| `views_new.unconfigured_heading` | the outcome of creating a view on a server started without the means to sign | "Creating views is not set up on this server" | "このサーバーではビューの作成が設定されていません" |
| `views_new.unconfigured` | under that outcome | "This server is not set up to sign changes into the history, so no view can be created; tell the person who runs this server. Nothing was changed." | "このサーバーには履歴へ署名する設定がないため、ビューを作成できません（このサーバーを運用している人に伝えてください）。何も変更していません。" |
| `title.compare` | the title of the page that compares two views; `{wiki}` is the wiki's name | "Compare views · {wiki} · {product}" | "ビューの比較 · {wiki} · {product}" |
| `compare.heading` | the heading of the page that compares two views | "Compare views" | "ビューの比較" |
| `compare.intro` | under that heading | "This lists where two views differ. It does not say which one is right." | "2 つのビューの違いを一覧にします。どちらが正しいかは示しません。" |
| `compare.a_label` | the label of the choice of the first view | "First view" | "1 つ目のビュー" |
| `compare.b_label` | the label of the choice of the second view | "Second view" | "2 つ目のビュー" |
| `compare.go` | the button that compares the two views chosen | "Compare" | "比べる" |
| `compare.pick_two` | a note when the two views chosen are one and the same | "Choose two different views to compare." | "異なる 2 つのビューを選んでください。" |
| `compare.pair` | the heading of a comparison; `{a}` and `{b}` are the two views named in words | "Comparing {a} and {b}" | "{a}と{b}の比較" |
| `compare.same` | under that heading when nothing differs | "The two views hold the same content." | "2 つのビューの内容は同じです。" |
| `compare.claims_heading` | the heading of the claims that differ | "Claims that differ" | "違いのある主張" |
| `compare.claims_none` | under it when no claim differs | "No claim differs between the two views." | "2 つのビューで違いのある主張はありません。" |
| `compare.claim_said_in` | a claim in the list, by the page it is said in; `{page}` is the page's title | "A claim said in {page}" | "{page}にある主張" |
| `compare.claim_unplaced` | a claim in the list that no page says now | "A claim that no page says" | "どのページにも書かれていない主張" |
| `compare.diff_wording` | what differs about a claim: the paragraphs it is said in read differently | "It is worded differently." | "文面が異なります。" |
| `compare.diff_status` | what differs about a claim: its status; `{a}` and `{b}` are statuses, `{va}` and `{vb}` the views | "Its status: {a} in {va}, {b} in {vb}." | "状態: {va}では{a}、{vb}では{b}。" |
| `compare.diff_sources` | what differs about a claim: the sources that bear on it, or how, or what they are | "Its sources differ." | "出典が異なります。" |
| `compare.only_in` | what differs about a claim or a page: only one view has it; `{view}` is that view named in words | "Only in {view}." | "{view}にだけあります。" |
| `compare.unreadable_in` | what differs about a claim: a view has a file for it that cannot be read; `{view}` is that view named in words | "Its file in {view} cannot be read, so nothing is said of it there." | "{view}にあるこの主張のファイルは読めないため、そこでの状態は示せません。" |
| `compare.status_accepted` | a claim's status in a comparison | "accepted" | "受け入れ済み" |
| `compare.status_contested` | a claim's status in a comparison | "contested" | "異論あり" |
| `compare.status_unresolved` | a claim's status in a comparison | "unresolved" | "未解決" |
| `compare.status_unknown` | a claim's status in a comparison: nobody decided | "not decided" | "未決定" |
| `compare.claims_omitted` | under the list of claims when more differ than are listed; `{shown}` and `{omitted}` are numbers | "{shown} claims are listed; {omitted} more also differ and are not shown." | "{shown} 件の主張を表示しています。違いのある主張がほかに {omitted} 件ありますが、表示していません。" |
| `compare.pages_heading` | the heading of the pages that differ | "Pages that differ" | "違いのあるページ" |
| `compare.pages_none` | under it when no page differs | "No page differs between the two views." | "2 つのビューで違いのあるページはありません。" |
| `compare.page_differs` | a page in the list that both views have and read differently | "The page differs." | "ページの内容が異なります。" |
| `compare.page_link` | a page in the list: the link to how the page differs; `{page}` is its title | "How {page} differs" | "{page}の違い" |
| `compare.pages_omitted` | under the list of pages when more differ than are listed; `{shown}` and `{omitted}` are numbers | "{shown} pages are listed; {omitted} more also differ and are not shown." | "{shown} 件のページを表示しています。違いのあるページがほかに {omitted} 件ありますが、表示していません。" |
| `compare.back` | the link from a comparison to the list of views | "All views" | "ビューの一覧" |
| `title.compare_page` | the title of the page that shows how one page differs between two views; `{page}` is its title | "{page}: how it differs · {wiki} · {product}" | "{page}: 違い · {wiki} · {product}" |
| `compare.page_heading` | the heading of that page; `{title}` is the page's title | "{title}: how it differs between two views" | "{title}: 2 つのビューでの違い" |
| `compare.page_range` | under that heading; `{a}` and `{b}` are the two views named in words | "Lines marked removed are in {a} only; lines marked added are in {b} only." | "「削除」の行は{a}だけに、「追加」の行は{b}だけにあります。" |
| `compare.page_read` | on that page: the link to read the page in one view; `{page}` is its title, `{view}` the view named in words | "Read {page} in {view}" | "{view}で{page}を読む" |
| `compare.page_back` | on that page: the link back to the comparison | "Back to the comparison" | "比較へ戻る" |
| `compare.page_absent` | on that page: the page is in neither view | "Neither view has this page." | "どちらのビューにもこのページはありません。" |
| `claim.others_heading` | the heading of the section of a claim's page that names the views where it reads differently | "Other views" | "ほかのビュー" |
| `claim.others_none` | under that heading when no other view reads the claim differently | "No other view reads this claim differently." | "ほかのビューで、この主張が異なって読めるものはありません。" |
| `claim.others_compare` | in that section: the link that compares this view with another; `{view}` is the other view named in words | "Compare with {view}" | "{view}と比べる" |
| `claim.others_limit` | in that section when the wiki has more views than are checked here; `{shown}` is a number | "Only {shown} other views are checked on this page; compare views to see the others." | "このページで調べるほかのビューは {shown} 件までです。そのほかのビューは、ビューの比較で確かめてください。" |
| `claim.others_unchecked` | in that section when some views could not be read just now | "Some views could not be read just now, so they are not listed here." | "今は読めなかったビューがあるため、ここには載せていません。" |
| `title.why` | the title of that page; `{page}` is the page's title, `{wiki}` the wiki's name | "Why is this here? {page} · {wiki} · {product}" | "なぜここにあるのか: {page} · {wiki} · {product}" |
| `why.intro` | under the heading of that page; `{page}` is the page's title | "Where each paragraph of {page} came from, as far as the page's history says. Every paragraph here can be linked to." | "{page} の各段落がどこから来たのかを、このページの履歴の範囲で示します。ここにある各段落には、それぞれリンクできます。" |
| `why.history_link` | on that page: the link to the page's history; `{page}` is its title | "The whole history of {page}" | "{page} の履歴全体" |
| `why.bounded` | on that page: the changes read stopped at the bounds the history page has, with the number read | "Only the latest {count} change to this page was read here. There are older changes, and what came before is not shown." / "Only the latest {count} changes to this page were read here. There are older changes, and what came before them is not shown." | "このページの変更のうち、最新の {count} 件だけをここで読みました。それより古い変更があり、その前のことは示されません。" |
| `why.bounded_none` | on that page: the bounds ended the reading before any change to the page was met | "No change to this page was found among the latest changes of this wiki that one request can read. There are older changes, and they are not shown." | "一度のリクエストで読めるこのウィキの最新の変更の中に、このページの変更は見つかりませんでした。それより古い変更があり、示されません。" |
| `why.spent_none` | on that page: the work one request may spend ended the reading before any change to the page was read (a read that stopped later is `why.bounded`, with the number of changes it did read) | "The work one request may spend on this page was used up before any change to it could be read. There are changes, and they are not shown." | "一度のリクエストでこのページに使える処理量を、このページの変更を一件も読まないうちに使い切りました。変更はありますが、示されません。" |
| `why.unfollowed` | on that page: the reading ended for a reason other than the bounds | "The history of this page could not be followed to its start: an earlier version could not be read, or the history lists no change that wrote the page. What is said below holds only for the changes that were read." | "このページの履歴を最初までたどれませんでした。古い版を読めなかったか、このページを書いた変更が履歴に載っていません。以下は、読めた変更についてだけ言えることです。" |
| `why.left_out` | on that page, after the list: the paragraphs beyond the most one page follows | "{count} more paragraph is not followed: only the first {limit} paragraphs of a page are." / "{count} more paragraphs are not followed: only the first {limit} paragraphs of a page are." | "さらに {count} 件の段落はたどっていません。1 ページでたどるのは最初の {limit} 件の段落までです。" |
| `why.paragraphs_heading` | the heading of the list of the page's paragraphs | "Paragraphs" | "段落" |
| `why.no_paragraphs` | in place of that list | "This page has no paragraphs." | "このページには段落がありません。" |
| `why.paragraph_label` | the link to one paragraph of the list, which is also its anchor; `{n}` is its place in the page | "Paragraph {n}" | "段落 {n}" |
| `why.written_unchanged` | under a paragraph followed through its anchor; `{change}` is the link to the change | "Written in {change}. Not changed since." | "{change}で書かれました。それ以降は変更されていません。" |
| `why.last_changed` | under a paragraph followed through its anchor | "Last changed in {change}." | "最後に変更されたのは{change}です。" |
| `why.first_written` | under a paragraph followed through its anchor | "First written in {change}." | "最初に書かれたのは{change}です。" |
| `why.first_before` | under a paragraph whose first writing was not reached | "First written before the oldest change read here." | "ここで読んだ最も古い変更より前に書かれました。" |
| `why.unchanged_read` | under a paragraph that none of the changes read changed | "Not changed in the changes read here." | "ここで読んだ変更の中では、変更されていません。" |
| `why.found_written_unchanged` | under a paragraph traced by matching the text | "Earliest wording found in {change}. No change to it was found since." | "最も古い文言が見つかったのは{change}です。それ以降の変更は見つかりませんでした。" |
| `why.found_last` | under a paragraph traced by matching the text | "Latest change found in {change}." | "見つかった最新の変更は{change}です。" |
| `why.found_first` | under a paragraph traced by matching the text | "Earliest wording found in {change}. No earlier wording matches it." | "最も古い文言が見つかったのは{change}です。それより前に一致する文言は見つかりませんでした。" |
| `why.basis_anchor` | under a paragraph: how it was followed | "Followed through its anchor: in every change read, the one paragraph with this anchor is this paragraph." | "アンカーでたどりました。読んだすべての変更で、このアンカーを持つ段落はこの段落ただ一つです。" |
| `why.basis_text` | under a paragraph: how it was followed, where no anchor carries it | "Traced by matching the text. Nothing carries this paragraph through every change but its words, so each earlier wording was found by comparing text. If several paragraphs were reworded at once, it may be a different paragraph." | "文面の一致でたどりました。この段落を全変更にわたって結びつけるものは文面しかないため、以前の文言は文を比べて見つけています。複数の段落が同時に書き換えられた場合は、別の段落をたどっているかもしれません。" |
| `why.changes_heading` | the heading of the list of the changes the paragraphs name | "Changes read" | "読んだ変更" |
| `why.changes_none` | under it when there is none | "No change to this page was read." | "このページの変更は読んでいません。" |
| `why.change_open` | a change in that list: the link to the change's own page | "See change {number} in the history" | "履歴で変更 {number} を見る" |
| `why.proposal` | a change in that list: the proposal its message names, linked by its summary | "Made from the proposal {proposal}." | "提案{proposal}から作られた変更です。" |
| `why.proposal_named` | a change in that list whose message names a proposal that the acceptance records do not tie to it: the proposal, linked by its summary, as the message names it and no more | "The message of this change names the proposal {proposal}." | "この変更のメッセージは、提案{proposal}を示しています。" |
| `why.proposal_unlisted` | a change in that list whose proposal cannot be shown | "The message of this change names a proposal that is not among the records of this wiki that you may read." | "この変更のメッセージが示す提案は、あなたが読めるこのウィキの記録にありません。" |
| `why.proposal_unconfirmed` | a change in that list whose message names a proposal that the acceptance records do not tie to it | "This wiki's records do not show that this proposal was accepted as this change, so its review is not shown here." | "この提案がこの変更として受け入れられたことは、このウィキの記録からは確認できないため、そのレビューはここに示しません。" |
| `why.review_heading` | a change in that list: the heading of what was said about the proposal | "Review" | "レビュー" |
| `why.review_none` | under it when there is nothing | "Nobody commented or decided on this proposal." | "この提案にコメントや決定をした人はいません。" |
| `why.decision_words` | after a decision, when the reviewer wrote words with it | "They wrote: “{text}”" | "次のように書き添えました:「{text}」" |
| `why.comment` | a comment in the review | "{author} commented, on revision {number}: “{text}”" | "{author}さんが改訂 {number} についてコメントしました:「{text}」" |
| `why.review_more` | after the review, when it is longer than one page of this kind shows | "{count} more item of the review is on the proposal's page." / "{count} more items of the review are on the proposal's page." | "レビューのうちさらに {count} 件は提案のページにあります。" |
| `why.sources_heading` | a change in that list: the heading of the sources the proposer gave | "Sources given with the proposal" | "提案に添えられた出典" |
| `why.sources_none` | under it when there is none | "No source was given with the proposal." | "提案に出典は添えられていません。" |
| `parts.attach` | the link after a paragraph in the list of parts: the form that attaches a source to it | "Attach a source" | "出典を付ける" |
| `parts.challenge` | the link after a paragraph in the list of parts: the form that adds a source that speaks against it | "Challenge this" | "これに異議を示す" |
| `parts.attach_for` | the name a screen reader says for that link; `{number}` is the number of the part | "Attach a source to part {number}" | "部分 {number} に出典を付ける" |
| `parts.challenge_for` | the name a screen reader says for that link; `{number}` is the number of the part | "Challenge this: part {number}" | "これに異議を示す: 部分 {number}" |
| `title.attach` | the title of the form that attaches a source to a paragraph | "Attach a source to {page} · {wiki} · {product}" | "{page} に出典を付ける · {wiki} · {product}" |
| `title.challenge` | the title of the form that adds a source that speaks against a paragraph | "Challenge a part of {page} · {wiki} · {product}" | "{page} の一部に異議を示す · {wiki} · {product}" |
| `attach.heading_supports` | the heading of the form that attaches a source | "Attach a source to {page}" | "{page} に出典を付ける" |
| `attach.heading_challenges` | the heading of the form that adds a source that speaks against a paragraph | "Challenge a part of {page}" | "{page} の一部に異議を示す" |
| `attach.intro_supports` | under the heading of the form that attaches a source | "A source is what a statement rests on: a book, an article, a record, a person's account. Attaching one records this paragraph as a claim, so that others can see what it rests on. You can also change the words of the paragraph; the change and the source go in one proposal." | "出典とは、ある記述のよりどころとなるものです。本、記事、記録、人の証言などがあります。出典を付けると、この段落が主張として記録され、何をよりどころにしているかを他の人が見られるようになります。段落の文章も変更できます。変更と出典は一つの提案になります。" |
| `attach.intro_challenges` | under the heading of the form that adds a source that speaks against a paragraph | "A challenging source speaks against a statement, or against part of it. Adding one records this paragraph as a claim and shows what is said against it. You can also change the words of the paragraph; the change and the source go in one proposal." | "異議を示す出典は、ある記述、またはその一部に反することを述べています。追加すると、この段落が主張として記録され、何がそれに反して述べられているかが示されます。段落の文章も変更できます。変更と出典は一つの提案になります。" |
| `attach.claim_exists` | in that form, when the paragraph is a claim already | "This paragraph is a claim already. The source is added to it." | "この段落はすでに主張として記録されています。出典はその主張に追加されます。" |
| `attach.legend` | the legend of the group of fields that describe the source | "The source" | "出典" |
| `attach.label_kind` | the label of the choice of what kind of source it is | "What kind of source it is" | "出典の種類" |
| `attach.kind_primary` | a kind of source in that choice | "A first-hand source" | "一次資料" |
| `attach.kind_secondary` | a kind of source in that choice | "A source about first-hand sources" | "一次資料について述べた資料" |
| `attach.kind_observational` | a kind of source in that choice | "A record of an observation" | "観察の記録" |
| `attach.kind_computational` | a kind of source in that choice | "The result of a computation" | "計算の結果" |
| `attach.kind_anecdotal` | a kind of source in that choice | "A person's account of what they saw or heard" | "見聞きしたことについての本人の話" |
| `attach.kind_other` | a kind of source in that choice | "Another kind" | "その他" |
| `attach.label_title` | the label of the source's title | "Title" | "タイトル" |
| `attach.label_author` | the label of the source's author or publisher | "Author or publisher" | "著者または発行者" |
| `attach.label_date` | the label of the source's date | "Date, as written" | "日付（書かれているとおり）" |
| `attach.hint_date` | under that field | "A year, a season or a day, as the source gives it." | "出典に書かれているとおりに、年、季節、日などを書いてください。" |
| `attach.label_ref_form` | the label of the choice of how the source is referred to | "How you refer to it" | "参照の方法" |
| `attach.ref_url` | a way to refer to a source in that choice | "A web address" | "ウェブのアドレス" |
| `attach.ref_doi` | a way to refer to a source in that choice | "A DOI" | "DOI" |
| `attach.ref_isbn` | a way to refer to a source in that choice | "An ISBN" | "ISBN" |
| `attach.ref_page` | a way to refer to a source in that choice | "A page in this wiki" | "このウィキのページ" |
| `attach.ref_citation` | a way to refer to a source in that choice | "A citation in words" | "文章で書いた引用情報" |
| `attach.label_ref` | the label of the reference itself | "The reference" | "参照先" |
| `attach.hint_ref` | under that field | "Leave it empty if the title says enough. A web address starts with https or http and has no spaces in it." | "タイトルだけで足りるときは空のままで構いません。ウェブのアドレスは https または http で始め、空白を含めないでください。" |
| `attach.label_bearing` | the label of the choice of how directly the source speaks to the paragraph | "How directly the source speaks to this paragraph" | "出典がこの段落にどの程度直接ふれているか" |
| `attach.bearing_direct` | a choice in that list | "It speaks to the whole of it" | "段落全体にふれている" |
| `attach.bearing_partial` | a choice in that list | "It speaks to part of it" | "段落の一部にふれている" |
| `attach.bearing_indirect` | a choice in that list | "It speaks to it only by implication" | "言外の含みとしてふれている" |
| `attach.label_note` | the label of the note on the source | "A note (optional)" | "メモ（任意）" |
| `attach.hint_note` | under that field | "Where to look in the source, or what it says." | "出典のどこを見ればよいか、または何が書かれているか。" |
| `attach.label_claim_kind` | the label of the choice of what kind of statement the paragraph is | "What kind of statement this paragraph is" | "この段落はどんな種類の記述か" |
| `attach.claim_kind_hint` | under that choice | "Say what you can. “Not said” is fine." | "わかる範囲で選んでください。「未記入」でも構いません。" |
| `attach.claim_kind_observation` | a kind of statement in that choice | "An observation" | "観察" |
| `attach.claim_kind_interpretation` | a kind of statement in that choice | "An interpretation" | "解釈" |
| `attach.claim_kind_hypothesis` | a kind of statement in that choice | "A hypothesis" | "仮説" |
| `attach.claim_kind_opinion` | a kind of statement in that choice | "An opinion" | "意見" |
| `attach.claim_kind_decision` | a kind of statement in that choice | "A decision" | "決定" |
| `attach.claim_kind_policy` | a kind of statement in that choice | "A policy" | "方針" |
| `attach.claim_kind_question` | a kind of statement in that choice | "A question" | "問い" |
| `attach.claim_kind_unknown` | a kind of statement in that choice | "Not said" | "未記入" |
| `attach.no_paragraph` | a problem: the part that was edited has no paragraph to attach a source to | "The part you edited has no paragraph a source can be attached to." | "編集した部分には、出典を付けられる段落がありません。" |
| `attach.page_unreadable` | a problem: the information at the top of the page cannot be read | "The information at the top of this page cannot be read, so the page cannot be given the identifier a claim needs." | "このページの先頭の情報を読み取れないため、主張に必要な識別子をページに付けられません。" |
| `attach.several_claims` | a problem: more than one claim is recorded for the paragraph | "More than one claim is recorded for this paragraph, and this form cannot tell which the source is for." | "この段落には複数の主張が記録されており、このフォームでは出典がどれのためのものかを判断できません。" |
| `attach.full` | a problem: the claim has as many sources as a claim may have | "This claim already has as many sources as one claim may have." | "この主張には、一つの主張が持てる数の出典がすでにあります。" |
| `attach.says_nothing` | a problem: the source has neither a title nor a reference | "A source needs a title or a reference." | "出典にはタイトルか参照先が必要です。" |
| `attach.too_long` | a problem: a text of the source is too long; `{limit}` is the most characters | "A text in the source is longer than {limit} characters." | "出典の文章が {limit} 文字を超えています。" |
| `attach.control` | a problem: a text of the source holds a character that cannot be kept | "A text in the source holds a character that cannot be kept, such as a line break." | "出典の文章に、改行など、保存できない文字が含まれています。" |
| `attach.not_web` | a problem: a web address is not one | "A web address has to start with https or http and have no spaces in it." | "ウェブのアドレスは https または http で始め、空白を含めないでください。" |
| `attach.unwritable` | a problem: what was made could not be read back | "The source could not be written down. Nothing was changed; try again." | "出典を書き込めませんでした。何も変更されていません。もう一度お試しください。" |
| `curate.not_a_day` | a problem: a date is not written as a day | "A date has to be written year, month, day, like 2026-11-04." | "日付は 2026-11-04 のように、年、月、日の順に書いてください。" |
| `curate.unknown_source` | a problem: the claim does not cite a source that the form names | "The claim does not cite one of the sources in the form. Open the form again." | "この主張は、フォームにある出典の一つを引用していません。フォームを開き直してください。" |
| `curate.no_wording` | a problem: the claim is said nowhere now | "The claim is not said in any paragraph now, so there is no wording to judge a source or a status on." | "この主張はいま、どの段落にも書かれていないため、出典や状態を判断する文章がありません。" |
| `curate.day_not_come` | a problem: a date given as the day a source was looked at has not come in any timezone (after today's date in UTC+14); `{title}` is the source's title (or the words for a source without one), `{day}` the day as written | "Source: {title}. The day {day} has not come yet in any timezone, so no one can have looked on it. Write the day it was looked at, or leave the date empty." | "出典: {title}。{day} はどの時間帯でもまだ来ていない日のため、その日に確認した人はいません。確認した日を書くか、日付を空にしてください。" |
| `propose.claim_held_one` | above the text of a paragraph that has sources attached | "Others rely on this paragraph: sources are attached to it. Its marker is kept for you, so just edit the words." | "この段落は他の人が頼りにしています。出典が付いています。目印はこちらで保っておくので、文章だけを編集してください。" |
| `propose.claim_held_many` | above a text that holds paragraphs that have sources attached | "Some paragraphs here have sources attached, and others rely on them. Their markers are kept for you as long as you leave their words as they are. To reword one of them, change that part on its own." | "ここには出典が付いていて、他の人が頼りにしている段落があります。その段落の文章をそのままにしている間は、目印はこちらで保っておきます。その段落の文章を変えるときは、その部分だけを単独で変更してください。" |
| `propose.claim_lost` | a problem: the text leaves out a paragraph that has sources attached; `{excerpt}` is how it begins | "The paragraph that begins “{excerpt}” has sources attached, and this change leaves it out. To reword it, change that part on its own. To remove it, tick the box below." | "「{excerpt}」で始まる段落には出典が付いていますが、この変更ではその段落がなくなります。文章を変えたいときは、その部分だけを単独で変更してください。削除するときは、下の欄にチェックを入れてください。" |
| `propose.claim_drop_legend` | the legend of the box that confirms the removal of a paragraph that has sources attached | "Paragraphs you are removing" | "削除する段落" |
| `propose.claim_drop` | the box that confirms that removal; `{excerpt}` is how the paragraph begins | "Remove “{excerpt}”. It stops being where its claim is said, and the reviewer will see that." | "「{excerpt}」を削除する。この段落は主張が書かれている場所ではなくなり、レビュアーにもそのことが示されます。" |
| `know.heading` | the heading of the list of what a proposal does to claims and sources | "Claims and sources in this proposal" | "この提案での主張と出典" |
| `know.claim_made` | a line of that list: a claim is recorded; `{claim}` is the claim, named by an excerpt of a paragraph it is said in (or its identifier when none is found), (see `know.subject_claim_id`), `{kind}` what kind of statement, `{count}` in how many places it is said, `{standing}` its community status | "Records the claim {claim} ({kind}), said in {count} place. Its community status: {standing}." / "Records the claim {claim} ({kind}), said in {count} places. Its community status: {standing}." | "主張{claim}を記録します（{kind}）。{count} か所に書かれています。コミュニティでの扱い：{standing}。" |
| `know.cite` | under that line, and for a claim's sources; `{source}` is the source's name, set apart | "Cites the source {source}: it {relation} the claim {bearing}, judged on the wording it has in this proposal's result." | "出典 {source} を引用します: {relation}（{bearing}）。この提案を反映した結果で主張が持つ文言で判断しています。" |
| `know.cite_unjudged` | the same line, when the link is judged on a wording the claim does not have in the proposal's result (a draft: it cannot be sent as it is); `{source}` is the source's name, set apart | "Cites the source {source}: it {relation} the claim {bearing}, judged on a wording the claim does not have in this proposal's result." | "出典 {source} を引用します: {relation}（{bearing}）。この提案を反映した結果で主張が持たない文言で判断されています。" |
| `know.source_recorded` | a line of that list: a source is recorded; `{source}` is its name (title, author, date), set apart, `{kind}` what kind | "Records the source {source} ({kind})." | "出典 {source} を記録します（{kind}）。" |
| `know.claim_changed` | a line of that list: a claim that exists is changed, the changes follow; `{claim}` is the claim, named as in `know.claim_made` | "Changes the claim {claim}:" | "主張{claim}を変更します:" |
| `know.source_changed` | a line of that list: a source that exists is changed, the changes follow; `{source}` is its name as the wiki has it, set apart | "Changes the source {source}:" | "出典 {source} を変更します:" |
| `know.removed_claim` | a line of that list; `{claim}` is the claim, named as in `know.claim_made` | "Removes the claim {claim}." | "主張{claim}を削除します。" |
| `know.removed_source` | a line of that list; `{source}` is its name, set apart | "Removes the source {source}." | "出典 {source} を削除します。" |
| `know.unreadable` | a line of that list: a file is not in the form the wiki reads; `{file}` is its name, `{reason}` says which rule | "The file {file} in this proposal is not in the form the wiki reads. {reason}" | "この提案のファイル {file} は、ウィキが読める形になっていません。{reason}" |
| `know.change_kind` | under a changed claim | "The kind of statement becomes {kind}." | "記述の種類が {kind} になります。" |
| `know.change_lifecycle` | under a changed claim | "Where it stands in its life becomes: {lifecycle}." | "主張の扱いの段階が「{lifecycle}」になります。" |
| `know.change_standing` | under a changed claim: its community status changes | "Its community status changes from {from} to {to}." | "コミュニティでの扱いが「{from}」から「{to}」に変わります。" |
| `know.change_said` | under a changed claim; `{paragraph}` is the paragraph, named by its page and an excerpt of its words (see `know.subject_paragraph`) | "It is said in one more paragraph: {paragraph}." | "もう一つの段落にも書かれることになります：{paragraph}。" |
| `know.change_unsaid` | under a changed claim: a paragraph stops being where it is said; `{paragraph}` is the paragraph, named by its page and an excerpt of its words (see `know.subject_paragraph`) | "It is no longer said in the paragraph {paragraph}: the paragraph is removed, or the claim lets it go." | "段落 {paragraph} には書かれなくなります。段落が削除されるか、主張がその段落から外れます。" |
| `know.change_related` | under a changed claim; `{other}` is the other claim, named as in `know.claim_made`, `{relation}` says how it relates to it | "A relation to the claim {other} is added: {relation}." | "主張{other}との関係が加わります: {relation}。" |
| `know.change_unrelated` | under a changed claim | "A relation to the claim {other} is removed: {relation}." | "主張{other}との関係がなくなります: {relation}。" |
| `know.change_uncited` | under a changed claim; `{source}` is the source's name, set apart | "It no longer cites the source {source}." | "出典 {source} を引用しなくなります。" |
| `know.change_recited` | under a changed claim: how a source it cites speaks to it changes | "The source {source}, which it cites, now {relation} it {bearing}." | "引用している出典 {source} と主張の関係が変わります: {relation}（{bearing}）。" |
| `know.change_noted` | under a changed claim | "The note on the source {source}, which it cites, changes." | "引用している出典 {source} のメモが変わります。" |
| `know.source_change_kind` | under a changed source | "Its kind becomes {kind}." | "種類が {kind} になります。" |
| `know.source_change_identity` | under a changed source; `{identity}` is its name as it becomes (title, author, date), set apart | "Who or what it is, as written, becomes {identity}." | "書かれている出典の名前や著者などが {identity} になります。" |
| `know.source_change_reference` | under a changed source; `{form}` is how the reference is written, `{reference}` the reference | "Where it is found becomes ({form}): {reference}." | "参照先が（{form}）{reference} になります。" |
| `know.source_change_snapshot` | under a changed source | "The copy kept of it changes." | "保存されている写しが変わります。" |
| `know.source_change_reviewed` | under a changed source; `{day}` is the day | "Last checked on {day}." | "最後に確認した日が {day} になります。" |
| `know.source_change_unreviewed` | under a changed source | "It is no longer marked as checked." | "確認済みの印がなくなります。" |
| `know.source_change_availability` | under a changed source | "Whether it can be had becomes: {availability}." | "入手できるかどうかが「{availability}」になります。" |
| `know.source_change_superseded` | under a changed source, and under a recorded one that says it is replaced; `{source}` is the source's name, set apart | "It is now replaced by the source {source}." | "出典 {source} に置き換えられます。" |
| `know.is_curation` | on a proposal that changes claims and sources and no page | "This is a curation proposal: it changes claims and sources and no page." | "これはキュレーション提案です。主張と出典を変更し、ページは変更しません。" |
| `know.not_curation` | on a proposal that sets what only a curation proposal may set | "This proposal sets a community status, a review date or an availability, changes or removes a source link a claim already has, changes a source that was checked in its title, author, date, reference or the source that replaces it, removes a claim or a source that holds any of these, or replaces or removes a file that cannot be read, which only a curation proposal may do. It cannot be accepted as it is." | "この提案はコミュニティでの扱い、確認日、入手できるかどうかを設定するか、主張がすでに持つ出典リンクを変更・削除するか、確認済みの出典の題名・著者・日付・参照先・置き換え先の出典を変更するか、これらを持つ主張や出典を削除するか、読み取れないファイルを置き換えるか削除していますが、これができるのはキュレーション提案だけです。このままでは受け入れられません。" |
| `know.not_judged` | on a proposal that sets or changes a judgement (a source link's or the community status) judged on a wording the claim does not have in the result | "This proposal sets or changes a judgement on a claim (a source link's or the community status), judged on a wording that no paragraph of that claim has once the proposal is made. It cannot be accepted as it is." | "この提案は主張に対する判断（出典リンクまたはコミュニティでの扱い）を設定または変更していますが、その判断は、提案を反映した結果にその主張のどの段落にも存在しない文言に基づいています。このままでは受け入れられません。" |
| `know.kind_observation` | a kind of statement, inside a sentence | "an observation" | "観察" |
| `know.kind_interpretation` | a kind of statement, inside a sentence | "an interpretation" | "解釈" |
| `know.kind_hypothesis` | a kind of statement, inside a sentence | "a hypothesis" | "仮説" |
| `know.kind_opinion` | a kind of statement, inside a sentence | "an opinion" | "意見" |
| `know.kind_decision` | a kind of statement, inside a sentence | "a decision" | "決定" |
| `know.kind_policy` | a kind of statement, inside a sentence | "a policy" | "方針" |
| `know.kind_question` | a kind of statement, inside a sentence | "a question" | "問い" |
| `know.kind_unknown` | a kind of statement, inside a sentence | "kind not said" | "種類は未記入" |
| `know.source_primary` | a kind of source, inside a sentence | "first-hand" | "一次資料" |
| `know.source_secondary` | a kind of source, inside a sentence | "about first-hand sources" | "一次資料について述べた資料" |
| `know.source_observational` | a kind of source, inside a sentence | "a record of an observation" | "観察の記録" |
| `know.source_computational` | a kind of source, inside a sentence | "the result of a computation" | "計算の結果" |
| `know.source_anecdotal` | a kind of source, inside a sentence | "a person's account" | "本人の話" |
| `know.source_other` | a kind of source, inside a sentence | "of another kind" | "その他の種類" |
| `know.supports` | how a source relates to a claim, inside a sentence | "supports" | "支持する" |
| `know.challenges` | how a source relates to a claim, inside a sentence | "challenges" | "異議を示す" |
| `know.directly` | how directly a source speaks to a claim, inside a sentence | "directly" | "直接" |
| `know.in_part` | how directly a source speaks to a claim, inside a sentence | "in part" | "部分的に" |
| `know.by_implication` | how directly a source speaks to a claim, inside a sentence | "by implication" | "間接的に" |
| `know.life_proposed` | where a claim stands in its life, inside a sentence | "put forward" | "提示された" |
| `know.life_adopted` | where a claim stands in its life, inside a sentence | "taken up" | "取り上げられた" |
| `know.life_retired` | where a claim stands in its life, inside a sentence | "retired" | "役目を終えた" |
| `know.life_unknown` | where a claim stands in its life, inside a sentence | "not said" | "未記入" |
| `know.standing_accepted` | a claim's community status, inside a sentence | "accepted" | "受け入れられている" |
| `know.standing_contested` | a claim's community status, inside a sentence | "contested" | "争われている" |
| `know.standing_unresolved` | a claim's community status, inside a sentence | "unresolved" | "決着していない" |
| `know.standing_unknown` | a claim's community status, inside a sentence | "not decided" | "未決定" |
| `know.available` | whether a source can be had, inside a sentence | "it can be had" | "入手できる" |
| `know.unavailable` | whether a source can be had, inside a sentence | "it cannot be had" | "入手できない" |
| `know.availability_unknown` | whether a source can be had, inside a sentence | "not known" | "不明" |
| `know.rel_refines` | how a claim relates to another, inside a sentence | "refines" | "より詳しく述べている" |
| `know.rel_supersedes` | how a claim relates to another, inside a sentence | "replaces" | "置き換える" |
| `know.rel_contradicts` | how a claim relates to another, inside a sentence | "is at odds with" | "食い違っている" |
| `know.rel_split_from` | how a claim relates to another, inside a sentence | "came out of" | "から分かれた" |
| `know.rel_merged_into` | how a claim relates to another, inside a sentence | "was joined into" | "に統合された" |
| `title.curate` | the title of the form that curates a claim | "Curate a claim · {wiki} · {product}" | "主張をキュレーションする · {wiki} · {product}" |
| `curate.heading` | the heading of that form | "Curate a claim" | "主張をキュレーションする" |
| `curate.intro` | under the heading of that form | "A curation proposal changes how a claim stands, how its sources bear on it, and when its sources were last checked. It changes no page. Nothing changes until someone who is allowed to takes it into the history; a community status is set this way and no other." | "キュレーション提案は、主張の扱い、出典が主張にどうかかわるか、出典を最後に確認した日を変更します。ページは変更しません。権限のある人が履歴に取り込むまで何も変わりません。コミュニティでの扱いは、この方法でのみ設定されます。" |
| `curate.wording` | under that: how the claim is said now; `{words}` is the paragraph's words | "It is said as: “{words}”" | "現在の書かれ方: 「{words}」" |
| `curate.wording_none` | under that, when the claim is said in no paragraph now | "It is not said in any paragraph now." | "この主張は現在、どの段落にも書かれていません。" |
| `curate.label_status` | the label of the choice of the community status | "Community status" | "コミュニティでの扱い" |
| `curate.status_hint` | under that choice | "What the community has decided about this claim. It is set by this proposal, once someone accepts it." | "この主張についてコミュニティが決めたこと。この提案が受け入れられたときに設定されます。" |
| `curate.earlier` | beside a status or a source that was judged on a wording the claim no longer has | "This was judged on an earlier wording of the claim." | "これは、この主張の以前の文面について判断されたものです。" |
| `curate.source_legend` | the legend of the group of fields about one source; `{title}` is its title | "Source: {title}" | "出典: {title}" |
| `curate.source_untitled` | in place of the title of a source that has none | "a source without a title" | "タイトルのない出典" |
| `curate.label_relation` | the label of the choice of how a source relates to the claim | "How it relates to the claim" | "主張との関係" |
| `curate.label_bearing` | the label of the choice of how directly a source speaks to the claim | "How directly it speaks to the claim" | "主張にどの程度直接ふれているか" |
| `curate.label_reviewed` | the label of the day a person last checked the source | "Last checked on (year-month-day)" | "最後に確認した日（年-月-日）" |
| `curate.reviewed_hint` | under that field | "Set by a person who checked it. Leave it empty if nobody has." | "確認した人が設定します。誰も確認していなければ空のままにしてください。" |
| `curate.label_availability` | the label of the choice of whether the source can be had | "Whether the source can be had" | "出典を入手できるか" |
| `curate.availability_hint` | under that choice | "As the person who checked says. Nothing checks it for you." | "確認した人が判断します。自動では確認されません。" |
| `curate.confirm` | the box that judges the sources and the status again on the wording the claim has now | "Judge the sources and the status again on the wording the claim has now" | "出典と扱いを、主張の現在の文面について改めて判断する" |
| `curate.confirm_hint` | under that box | "Use this when the claim was reworded and you have read its sources against the new wording." | "主張の文面が変わり、出典を新しい文面と照らして読んだときに使ってください。" |
| `curate.submit` | the button that sends the curation proposal for review | "Send for review" | "レビューに送る" |
| `curate.nothing` | a problem: the form differs from what the wiki has in nothing | "Nothing in the form differs from what the wiki has now. Change something first." | "フォームの内容は、ウィキの現在の内容と変わりません。先に何かを変更してください。" |
| `touch.heading` | the heading of the list of claims a proposal touches | "Claims this change touches" | "この変更が触れる主張" |
| `touch.intro` | under that heading | "These claims are said in paragraphs this change rewords or removes. The wiki shows what it can see; it does not say whether a reworded paragraph still means the same." | "これらの主張は、この変更で文面が変わる、または削除される段落に書かれています。ウィキは見えることだけを示し、文面が変わった段落が同じ意味かどうかは判断しません。" |
| `touch.reworded` | a line of that list; `{words}` is how the paragraph reads after the change | "The paragraph “{words}” is reworded. It stays the same claim, because its marker stays." | "段落「{words}」の文面が変わります。目印が残るため、同じ主張のままです。" |
| `touch.removed` | a line of that list; `{words}` is how the paragraph read before | "The paragraph “{words}” is removed. The claim is no longer said there." | "段落「{words}」が削除されます。この主張はそこには書かれなくなります。" |
| `touch.links_earlier` | under such a line: sources that were judged on the wording the paragraph had; `{count}` is how many | "{count} source was judged on the earlier wording. It will be shown as judged on an earlier wording." / "{count} sources were judged on the earlier wording. They will be shown as judged on an earlier wording." | "{count} 件の出典は以前の文面について判断されています。以前の文面について判断されたものとして表示されます。" |
| `touch.standing_earlier` | under such a line: the community status was decided on the earlier wording; `{status}` is the status | "Its community status ({status}) was decided on the earlier wording. It will be shown as judged on an earlier wording." | "コミュニティでの扱い（{status}）は以前の文面について決められています。以前の文面について判断されたものとして表示されます。" |
| `touch.nothing_earlier` | under such a line: nothing was judged on the earlier wording | "Nothing was judged on the earlier wording." | "以前の文面について判断されたものはありません。" |
| `touch.reconfirm_hint` | above the button that proposes judging those claims again, on an accepted proposal | "One proposal for all of them: it judges their sources, and their status where one was decided, on the wording they have now. It changes no page, and someone has to accept it." | "すべてをまとめて一つの提案にします。出典と、決められている場合はコミュニティでの扱いを、いまの文面について改めて判断します。ページは変更せず、受け入れる人が必要です。" |
| `touch.reconfirm_button` | the button that proposes judging those claims again | "Propose judging them again" | "改めて判断する提案をする" |
| `touch.reconfirm_later` | on a proposal that is not accepted yet, under that list | "Once this proposal is accepted, a person can propose judging these claims again here, in one step." | "この提案が受け入れられたら、ここから一度の操作で、これらの主張を改めて判断する提案ができます。" |
| `touch.reconfirm_summary` | the summary of the proposal that judges the claims again; `{summary}` is the summary of the proposal that reworded them | "Judge the claims again after: {summary}" | "改めて判断する: {summary}" |
| `touch.nothing_to_judge` | a note: every claim of the list is judged on the wording it has now | "Nothing needs judging again: each of these claims is judged on the wording it has now." | "改めて判断する必要はありません。これらの主張はどれも、いまの文面について判断されています。" |
| `touch.reconfirm_open` | a note in place of the reconfirm step: a proposal made to judge the claims again is still open (the placeholder links to it) | "Judging these claims again is already proposed: {proposal}. That proposal is still open." | "これらの主張を改めて判断する提案はすでにあります: {proposal}。その提案はまだ開いています。" |
| `touch.reconfirm_done` | a note in place of the reconfirm step: an accepted proposal judged every claim of the list again | "Each of these claims was judged again on the wording it has now, by a proposal that was accepted." | "これらの主張はどれも、受け入れられた提案で、いまの文面について改めて判断されています。" |
| `touch.reconfirm_of` | a note on a curation proposal made by the reconfirm step: the change it re-confirms; `{proposal}` links the proposal that made that change | "This proposal judges again the claims that the change made by {proposal} left judged on an earlier wording." | "この提案は、{proposal}による変更で以前の文面について判断されたままになった主張を、改めて判断します。" |
| `invalid.knowledge_not_shown` | when a proposal's claim or source file changes something its page would not describe line by line | "A change to a claim or a source in this proposal is one this page would not describe, so nobody reviewing it could see what it does. It is not sent." | "この提案の主張または出典の変更には、このページが説明しないものがあり、確認する人が内容を見られません。送信できません。" |
| `invalid.knowledge_unjudged` | when a proposal adds or changes a source link judged on a wording the claim does not have once the proposal is made | "A source link this proposal adds to a claim or changes is judged on a wording that no paragraph of that claim has once the proposal is made. It is not sent." | "この提案が主張に追加または変更する出典リンクは、提案を反映した結果にその主張のどの段落にも存在しない文言で判断されています。送信できません。" |
| `invalid.knowledge_status_unjudged` | when a proposal sets or changes a claim's community status judged on a wording the claim does not have once the proposal is made | "The community status this proposal sets on a claim is judged on a wording that no paragraph of that claim has once the proposal is made. It is not sent." | "この提案が主張に設定するコミュニティでの扱いは、提案を反映した結果にその主張のどの段落にも存在しない文言で判断されています。送信できません。" |
| `know.change_rejudged` | a line of a proposal: what a status or a source was judged on is now this wording; `{subject}` is the next two rows, `{words}` the wording | "What {subject} was judged on becomes this wording: “{words}”" | "{subject}を判断したときの文面が、次の文面に変わります：「{words}」" |
| `know.change_rejudged_unseen` | the same, when no paragraph of the claim has that wording once the proposal is made | "What {subject} was judged on becomes a wording that none of the claim's paragraphs has once this proposal is made." | "{subject}を判断したときの文面が、この提案のあとには主張のどの段落にもない文面に変わります。" |
| `know.change_rejudged_cleared` | the same, when it is no longer tied to a wording | "It is no longer recorded which wording {subject} was judged on." | "{subject}を判断した文面は、記録されなくなります。" |
| `know.rejudged_status` | the subject of those lines: the claim's status | "its community status" | "コミュニティでの扱い" |
| `know.rejudged_source` | the subject of those lines: a source the claim cites; `{source}` is its name, set apart | "the source {source} that it cites" | "引用している出典 {source}" |
| `know.nothing_changes` | a note on a proposal page whose claim and source files are the files the wiki already has | "The claim and source files in this proposal are the files the wiki already has: they change nothing." | "この提案の主張・出典ファイルは、ウィキにすでにあるファイルと同じです。変更はありません。" |
| `attach.direction` | a problem: a text of the source holds a character that changes the direction of text or is not shown | "A text in the source holds a character that changes the direction of text or is not shown, which cannot be kept." | "出典の文章に、文字の向きを変える文字や、表示されない文字が含まれていて、保存できません。" |
| `know.replaced_unreadable_claim` | a line of a proposal: a claim file the wiki has and cannot read is replaced or removed; `{id}` is its identifier | "Replaces or removes a claim file that cannot be read ({id}). What it held is not known." | "読み取れない主張ファイル（{id}）を置き換えるか削除します。その内容は分かっていません。" |
| `know.replaced_unreadable_source` | the same, for a source file | "Replaces or removes a source file that cannot be read ({id}). What it held is not known." | "読み取れない出典ファイル（{id}）を置き換えるか削除します。その内容は分かっていません。" |
| `know.made_lifecycle` | under a recorded claim | "Where it stands in its life: {lifecycle}." | "主張の扱いの段階：{lifecycle}。" |
| `know.made_related` | under a recorded claim; `{other}` is the other claim, named as in `know.claim_made` | "It has a relation to the claim {other}: {relation}." | "主張{other}との関係があります: {relation}。" |
| `know.source_made_availability` | under a recorded source | "Whether it can be had: {availability}." | "入手できるかどうか：{availability}。" |
| `know.source_where` | under a recorded source; `{form}` is how the reference is written, `{reference}` the reference | "Where it is found ({form}): {reference}." | "参照先（{form}）：{reference}。" |
| `know.source_copy` | under a recorded source | "A copy of it is kept." | "写しが保存されています。" |
| `know.source_change_unsuperseded` | under a changed source | "It is no longer marked as replaced." | "置き換えられたという印がなくなります。" |
| `know.source_change_reference_cleared` | under a changed source | "It no longer says where it is found." | "参照先が書かれなくなります。" |
| `know.source_nameless` | the name of a source that has no title, author or date in the wiki or the proposal, in a line of that list (its identifier follows it, see `know.subject_source_id`) | "A source without a title, author or date" | "題名も著者も日付もない出典" |
| `know.subject_claim` | the name of a claim in a line of that list when no paragraph it is said in can be found; `{id}` is its whole identifier, in `<code>`, linked to the claim's page when the wiki has the claim | "{id}" | "{id}" |
| `know.subject_claim_id` | the name of a claim in a line of that list: `{excerpt}` is the beginning of a paragraph it is said in, `{id}` its **whole** identifier, in `<code>`, linked to the claim's page when the wiki has the claim (identifiers are written by whoever writes the file, so a shortened form could be copied; two claims can be said in one paragraph, and two paragraphs can begin alike, so the words alone do not tell which claim a line is about) | "“{excerpt}” [claim {id}]" | "「{excerpt}」［主張 {id}］" |
| `know.subject_source_id` | the name of a source in a line of that list: `{name}` is its name (title, author, date), set apart, `{id}` its **whole** identifier, in `<code>`, linked to the source's line on the page of the claim the line is about when there is one (two sources can have the same name, and a shortened identifier could be copied) | "{name} [source {id}]" | "{name}［出典 {id}］" |
| `know.subject_words` | the name of a paragraph in a line of that list when its page cannot be named, or the paragraph cannot be found (then `{words}` is its marker, `^` and the block's id); `{words}` is otherwise an excerpt of its words, set apart | "“{words}”" | "「{words}」" |
| `know.subject_paragraph` | the name of a paragraph in a line of that list: `{words}` is an excerpt of its words (the first 80 characters), set apart, `{page}` the title of the page it is on, as the proposal leaves it, set apart | "“{words}” on the page {page}" | "ページ {page} の「{words}」" |
| `know.subject_paragraph_block` | the same, when another paragraph of that page begins with the same words (a claim that moves between them would read as no change); `{block}` is the marker of the block, in `<code>` | "“{words}” (the paragraph marked {block}) on the page {page}" | "ページ {page} の「{words}」（段落の印 {block}）" |
| `know.removed_had_standing` | under a removed claim: what is lost with it; `{standing}` is the community status it had | "Its community status goes with it: {standing}." | "コミュニティでの扱いも一緒に削除されます：{standing}。" |
| `know.removed_had_link` | under a removed claim, one line for each source it cited; `{source}` is the source's name, set apart, `{relation}` and `{bearing}` how it spoke to the claim | "Its link to the source {source} goes with it: it {relation} the claim {bearing}." | "出典 {source} とのつながりも一緒に削除されます: {relation}（{bearing}）。" |
| `know.removed_had_reviewed` | under a removed source that was marked as checked; `{day}` is the day | "The day it was last checked goes with it: {day}." | "最後に確認した日も一緒に削除されます：{day}。" |
| `know.removed_had_availability` | under a removed source; `{availability}` is what was recorded of whether it can be had | "What is recorded of whether it can be had goes with it: {availability}." | "入手できるかどうかの記録も一緒に削除されます：{availability}。" |
| `health.sources_name` | the name of the list of sources a person should look at again | "Sources to review" | "見直す出典" |
| `health.sources_desc` | what that list shows | "Sources that were last reviewed long ago or never, and sources a person marked as unavailable, each with the claims that cite it. The wiki checks nothing and fetches nothing: a person looks at the source and says so in a proposal." | "ずいぶん前に確認されたきり、または一度も確認されていない出典と、確認した人が利用できないと記した出典を、それを引用する主張とともに示します。ウィキは何も確認せず、何も取得しません。人が出典を確かめ、提案でそのことを記します。" |
| `health.col_source` | the heading of the column with a source's name | "Source" | "出典" |
| `health.col_why` | the heading of the column that says why a source is listed | "Why it is listed" | "挙げられている理由" |
| `health.col_claims` | the heading of the column with the claims that cite a source | "Claims that cite it" | "それを引用する主張" |
| `health.sources_action` | in each row of that list: what would resolve it | "Look at the source, then propose the day you looked and whether it can be had." | "出典を確かめたうえで、確かめた日と、利用できるかどうかを提案してください。" |
| `health.none_sources` | in place of that list when there is no source to list | "No source needs a look." | "見直す必要のある出典はありません。" |
| `health.sources_unavailable` | a reason in that list: a person who checked marked the source as unavailable | "A person who checked marked it as unavailable." | "確認した人が、利用できないと記しています。" |
| `health.sources_never` | a reason in that list: no one has recorded looking at the source | "No one has recorded looking at it." | "確認したと記録した人はまだいません。" |
| `health.sources_last` | under a source in that list that is listed for another reason; `{day}` is the day it was last reviewed, as written | "Last reviewed on {day}." | "最後の確認は {day} です。" |
| `health.sources_overdue` | a reason in that list; `{day}` is the day it was last reviewed, as written, `{limit}` the number of days after which a review is old | "Last reviewed on {day}, more than {limit} days ago." | "最後の確認は {day} で、{limit} 日を超えて前です。" |
| `health.sources_claims` | in the cell of the claims that cite a source; `{count}` is how many | "{count} claim cites it:" / "{count} claims cite it:" | "{count} 件の主張がこれを引用しています：" |
| `health.sources_cut` | after that sentence when more claims cite the source than are named; `{limit}` is how many are named | "The first {limit} are named." | "先頭の {limit} 件だけを挙げています。" |
| `health.sources_no_claims` | in the cell of the claims when no claim cites the source | "No claim cites it." | "これを引用する主張はありません。" |
| `health.sources_claim` | the name of a claim that cites a source; `{where}` is the page it is said on, `{how}` how the source bears on it | "{where} ({how})" | "{where}（{how}）" |
| `health.sources_claim_id` | the name of a claim that cites a source, where a page says it: the page, then the claim's whole identifier in `<code>`; `{where}` is the page it is said on, `{id}` the identifier | "{where} [claim {id}]" | "{where}［主張 {id}］" |
| `health.sources_claim_nowhere` | the name of a claim that cites a source and that no page says now; `{id}` is its identifier | "A claim said on no page now, identifier {id}" | "いまどのページにも書かれていない主張、識別子 {id}" |
| `health.sources_setting` | under that list; `{days}` is the number of days after which a review is old | "A source is listed when it was last reviewed more than {days} days ago. The person who runs this server sets the number." | "最後の確認から {days} 日を超えた出典が挙げられます。この日数は、このサーバーの管理者が定めます。" |
| `health.sources_future` | a reason in that list: the source's file says a person reviewed it on a day that has not come (a mistyped year, for one); `{day}` is that day, as written | "Recorded as reviewed on {day}, a day that has not come." | "確認した日として {day} と記録されていますが、その日はまだ来ていません。" |
| `health.sources_showing` | under that list when it has more rows than a page; `{from}` and `{to}` are the places of the first and the last row shown, `{total}` how many rows the list has | "Showing sources {from} to {to} of {total}." | "{total} 件のうち {from} 件目から {to} 件目までを表示しています。" |
| `health.sources_more` | the link to the next rows of that list | "Show the next sources" | "続きの出典を表示" |
| `health.sources_first` | the link back to the first rows of that list, on a later page of it | "Show the first sources" | "最初の出典を表示" |
| `health.sources_no_more` | the whole of that list's body when the page asked for begins after its last row (the list is shorter than when the link was made) | "No more sources." | "これ以上の出典はありません。" |
| `know.curate_link` | the link to the curation form, beside a claim (on its page, in a row of that list, under a refusal that says the change needs a curation proposal), for those who may propose | "Propose a change to its status or sources" | "この主張の扱いや出典の変更を提案する" |
| `know.curate_link_of` | the same link where a page that is not the claim's offers it (a proposal's page, a refusal), one claim or several; `{claim}` is the claim's whole identifier, in `<code>` | "Propose a change to the status or sources of the claim {claim}" | "主張 {claim} の扱いや出典の変更を提案する" |
| `know.curate_more` | under those links when a change touches more claims than are offered a form; `{count}` is how many more | "{count} more claim is touched: its own page leads to its curation form." / "{count} more claims are touched: each claim's own page leads to its curation form." | "ほかに {count} 件の主張にも関わります。それぞれの主張のページから、扱いや出典の変更を提案できます。" |
| `claim_thread.heading` | the heading of a claim's discussion on the claim's page | "Discussion of this claim" | "この主張についての議論" |
| `claim_thread.never_changes` | under that heading, in one sentence: what a discussion does not do | "A discussion never changes a claim: only a proposal that people accept does." | "議論が主張を変えることはありません。主張を変えるのは、人々が受け入れた提案だけです。" |
| `claim_thread.total` | under that sentence when the discussion has comments; `{count}` is how many in all | "{count} comment" / "{count} comments" | "{count} 件のコメント" |
| `claim_thread.none` | in place of the comments when nobody has written in the discussion | "Nobody has discussed this claim yet." | "この主張についての議論はまだありません。" |
| `claim_thread.meta` | the line of a comment in a claim's discussion; `{number}` is its place in the discussion, `{when}` the time | "Comment {number} · {when}" | "コメント {number} · {when}" |
| `claim_thread.in_reply_to` | under a comment that answers another; `{number}` is the place of the comment it answers | "In reply to comment {number}." | "コメント {number} への返信です。" |
| `claim_thread.more` | the link to the next comments, under a long discussion | "Show the next comments" | "続きのコメントを表示" |
| `claim_thread.first` | the link back to the first comments, on a later page of a long discussion | "Show the first comments" | "最初のコメントを表示" |
| `claim_thread.unavailable` | in place of the discussion when its records cannot be read just now | "The discussion of this claim could not be read now." | "この主張の議論を現在は読み取れません。" |
| `claim_thread.form_label` | the label of the field that adds a comment to a claim's discussion | "Add a comment to this discussion" | "この議論にコメントを追加" |
| `claim_thread.form_reply` | above that field when the comment answers another; `{number}` is the place of the comment it answers | "Replying to comment {number}" | "コメント {number} への返信" |
| `claim_thread.back` | the way back to the claim's page from a refused comment | "Back to the claim" | "主張のページに戻る" |
| `api.claim_lost` | a problem of the write API: the text leaves out a paragraph that claims are said in and `drops` does not name it with its page; `{path}` is the page's path, `{block}` the paragraph's marker, `{excerpt}` how it begins | "The paragraph marked ^{block} on {path}, which begins “{excerpt}”, is where claims are said, and this change leaves it out. To remove it, name it in `drops` with the `path` {path} and the `marker` {block}. To keep it, put it back." | "{path} の、「{excerpt}」で始まるマーカー ^{block} の段落は主張が書かれている場所ですが、この変更ではその段落がなくなります。削除するときは `drops` に `path` として {path}、`marker` として {block} を指定してください。残すときは元に戻してください。" |
| `api.drop_unused` | a problem of the write API: `drops` names a paragraph of a page that the change does not leave out of it, or that no claim is said in; `{path}` is the page's path, `{block}` the paragraph's marker | "`drops` names the paragraph marked ^{block} on {path}, but this change does not leave it out of that page, or no claim is said in it. Name only the paragraphs this change removes." | "`drops` に {path} のマーカー ^{block} の段落が指定されていますが、この変更ではその段落は {path} から削除されないか、その段落に書かれた主張がありません。この変更で削除する段落だけを指定してください。" |
| `claim_thread.in_reply_to_link` | under a comment that answers another, when the comment it answers can be linked to; `{comment}` is a link to it, worded by `claim_thread.comment_label` | "In reply to {comment}." | "{comment} への返信です。" |
| `claim_thread.comment_label` | the words of that link; `{number}` is the place of the comment in its thread | "comment {number}" | "コメント {number}" |
| `curate.label_remove` | the label of the box in the curation form that lets a link go | "The claim no longer cites this source" | "この主張はこの出典を引用しなくなる" |
| `curate.remove_hint` | under that box | "Ticking this removes the link between the claim and the source. The source's own record stays, and other claims keep their links to it." | "この欄にチェックを入れると、主張と出典のリンクが外れます。出典自体の記録は残り、他の主張のリンクもそのままです。" |
| `curate.work_hint` | above the fields of a checked source in the curation form | "This source was checked, so who and what it is, where it is found and which source replaces it are changed here, and not in a page edit." | "この出典は確認済みなので、何であるか、どこで見つかるか、どの出典に置き換えられるかは、ページの編集ではなくここで変更します。" |
| `curate.label_superseded` | the label of the field for the source that replaces this one | "The source that replaces it" | "これを置き換える出典" |
| `curate.superseded_hint` | under that field | "The identifier of another source in this wiki. Leave it empty if no source replaces it." | "このウィキにある別の出典の識別子です。置き換える出典がなければ空のままにしてください。" |
| `curate.removed_and_changed` | a problem: a link is both changed and removed in the curation form | "A link is either changed or removed, not both. Untick the box, or leave the link as it was." | "リンクは変更するか外すかのどちらかです。両方はできません。チェックを外すか、リンクを元のままにしてください。" |
| `curate.not_checked` | a problem: who and what a source is was changed, but the source was not checked | "This source has not been checked, so who and what it is, where it is found and which source replaces it are changed in a page edit. Record when it was checked, or whether it can be had, and they can be changed here." | "この出典はまだ確認されていないため、何であるか、どこで見つかるか、どの出典に置き換えられるかはページの編集で変更します。確認した日か入手できるかどうかを記録すると、ここで変更できます。" |
| `curate.replaced_by_itself` | a problem: a source is named as replacing itself | "A source cannot be replaced by itself." | "出典を、その出典自身に置き換えることはできません。" |
| `curate.unknown_successor` | a problem: the source that replaces another is not a source of the wiki, or is not written as an identifier | "The source that replaces this one has to be a source in this wiki, written as its identifier." | "置き換える出典は、このウィキにある出典で、識別子の形で書く必要があります。" |
| `know.not_by_form` | under the sentence that says a change needs a curation proposal, when the change also removes a claim or a source file, or replaces a file that cannot be read; it names no reviewer and no form, because none can do it in this version | "Removing a claim or a source file, or replacing a file that cannot be read, is not offered in this version." | "主張や出典のファイルの削除と、読み取れないファイルの置き換えは、このバージョンでは提供されていません。" |

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
| conflict, conflicts, conflicting | コンフリクト | the changes overlap | A version-control name; the word of an acceptance that stops is "changes overlap" (RFC 004 D-12), and a person, not the service, decides what to keep |
| merge, merges, merged, merging | マージ | re-authored against the current page; combined | A version-control verb; the reader is told the result (re-authored, or the places that overlap), not the operation |
| true, false | 真実, 虚偽 | what the sources do (supports, challenges) and the status people decided | The wiki records how a source bears on a claim and what people decided; it never says that a claim is true or false (RFC 005 part 2 D-7) |
| proven, canonical, official | 証明済み, 正典, 公式 | the default view; supports, challenges | Nothing is proven, and no view or claim is called canonical or official (RFC 005 part 2 D-7, D-9) |
| adopted | 採用済み | the default view | A view is not adopted by anyone: the shipped word for the view shown when none is named is the default view (RFC 005 part 2 D-9) |
| verified | 検証済み | signed into the history | prikk's word for the history alone: only the verification badge and the verification page say it, so a reader cannot take a claim for something that was verified (RFC 005 part 2 D-7) |

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
