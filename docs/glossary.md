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
| A named collection of pages with its own history (storage: one prikk repository) | wiki | ウィキ | not yet | The usual word, and the name of the thing the reader came for | Drafted · ja: hard term |
| One document of a wiki, found by its title | page | ページ | yes | The reader's word; never "file" or "article" | Named by the handoff |
| The name of a page, which is also its address | title | タイトル | not yet | A page's title is its address (RFC 003 D-1); the storage path is never the address | Drafted |
| What a reader types in the browser to reach something | address | アドレス | yes | The plain word for what a not-found page asks the reader to check | Drafted |
| A named version of the whole wiki that readers can switch between (storage: a branch) | view | ビュー | not yet | A wiki reader does not know "branch"; the default view is the one shown when none is named | Named by the handoff · ja: hard term |
| Everything that happened to a page, newest first | history | 履歴 | not yet | The usual word | Reused from planeter |
| One recorded step in a wiki's history: work that was accepted and signed in (storage: a block) | change | 変更 | not yet | One word for one thing; never "commit", "block" or "patch" | Reused from planeter · ja: hard term |
| Work someone puts forward for review before it becomes part of a page | proposal | 提案 | not yet | "Change" is taken by the recorded step; one word for two things is the confusion this page exists to remove | Reused from planeter · ja: hard term |
| A proposal the wiki service has received and keeps: it is not part of the history yet | held for review | レビュー待ちで保持中 | not yet | Says what is true and no more: the service holds the work, nothing is approved for merging. As a label: *Held for review* (RFC 002 D-6) | Named by the handoff · ja: hard term |
| A proposal that an authorized person took into the history | accepted | 受け入れ済み | not yet | RFC 002 D-6, WEB-09; "approved" is a reviewer's opinion about one revision and means something else. As a label: *Accepted* | Drafted · ja: hard term |
| A proposal that an authorized person did not take into the history | declined | 見送り済み | not yet | RFC 002 D-6, WEB-09; calm, assumes good faith, and the message beside it names the next step. As a label: *Declined* | Drafted · ja: hard term |
| A page that other pages link to and that does not exist yet | wanted page | 未作成のページ | not yet | Says what the reader can do about it: write it | Named by the handoff · ja: hard term |
| The label that prikk ran all of its checks on the wiki and found no problems | Verified | 検証済み | not yet | A trust label: it never means more than the check found, and never appears without it | Reused from planeter · ja: hard term, trust label |
| The label that prikk checked and found problems | Verification failed | 検証に失敗 | not yet | "Unverified" would read as "not checked" | Reused from planeter · ja: hard term, trust label |
| The label for a check that could not run | Could not be checked now | 現在は確認できません | not yet | Separates "failed" from "could not check"; it claims nothing either way | Reused from planeter · ja: hard term, trust label |
| The label on a change that was signed into the history | Signed into the history | 署名して履歴に記録済み | not yet | Plain for "sealed"; on a change page it must not read as "Verified" | Reused from planeter · ja: hard term, trust label |
| inklkunn itself, as the author of record of every change | the wiki service | ウィキサービス | not yet | RFC 002 D-6, WEB-09: "signed by the wiki service on behalf of {proposer}, accepted by {acceptor}" says who signed and for whom, and never implies that a person signed cryptographically | Drafted · ja: hard term |
| A page title that sends the reader to another page | redirect | リダイレクト | not yet | The usual word; the page says "redirected from {title}" | Drafted |
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
