# Changelog

Releases are bare version tags; each section is the release's notes. A section's lead (up to *The entries below…*) is
the GitHub release's text, with a link to the whole section (`release.yml`). Kept by hand, per release.

## [Unreleased]

- **K-T6 (handoff 005; RFC 005 part 2 D-7): what a reader sees of the knowledge layer.** A paragraph that holds a claim
  carries one quiet link, *Sources and views*, at its end (named by the paragraph it is on, so every link's name is
  distinct; a paragraph with several claims has one link for each); the sanitized body is only cut at the paragraph's
  closing tag and printed in order, so **a page with no claims is the page it was, byte for byte** (held against the
  pages as 0.2.0 drew them), plus one *Why is this here?* link among the page's tools (not on a permalink). The claim's
  page (`/{slug}/-/claim/{id}`, `?view=` for a view) says in words what kind of statement it is, where it is said, each
  source with its relation and how directly it speaks to it (*supports, directly*; *challenges, in part*), *judged on an
  earlier wording* where the paragraph's words are no longer those it was judged on, and its status in this view
  (accepted, contested, unresolved, not decided) with the proposal that decided it; it is read from the derived knowledge
  index (`claims_on_page`), authorized as pages are, and an address that names no claim is one plain page. *Other views*
  is filled in by K-T8 (below): it says that no view differs only for views it has checked.
  The *Why is this here?* link leads to `/{slug}/-/why/{title}` (K-T7, below). The retired-words test gains *true*, *false*, *verified* (only the
  verification badge's label, held to one message and one place), *proven*, *canonical*, *adopted*, *official* and their
  Japanese terms; a stylesheet test holds that nothing drawn for a claim is coloured apart, pictured or counted and that no
  rule is named for a verdict, a relation, a bearing or a status. Every message is in both languages with its glossary
  row; every state is in the structural tests.
- **K-T7 (handoff 005; RFC 005 part 2 D-8): *Why is this here?*.** `/{slug}/-/why/{title}` (`?view=` for a view), which
  until now answered *not available in this release*, lists the paragraphs of a page, each with an anchor so that a single
  paragraph can be linked to (`#w-^b1` for a paragraph with an `^id`, `#w-3` for any other), and for each the change that
  **last changed** it and the change that **first wrote** it, linked to the changes listed below them. For each change
  the page gives the people as its message records them (the history page's own sentences, no more), the proposal it was
  made from, the proposal's review (decisions and comments, at most twenty items, each cut where long, the rest counted),
  the sources the proposer gave, and the way to the change itself. **A proposal the visitor may not read is one plain
  sentence, and a proposal that the acceptance records do not tie to the change has no review shown.** Everything is derived
  from prikk's history and the service's records, read as the visitor may read them: no new store. **The chain never claims
  more than the method has.** A paragraph with an `^id` that is the one block with it on both sides of every step is
  followed *through its anchor*, and the page says *First written in*; any other paragraph is followed with the product's
  own segmentation and the merge's own matching and is labelled **traced by matching the text**, with what that means
  (*earliest wording found in…*, not *first written in…*, because two paragraphs reworded together have nothing to match).
  **Bounded by the history page's own step**: at most the 20 changes of the page that one history page lists (and so its
  inspection, `show` and log-window bounds), one `cat` per change read and one more for the page before the oldest,
  two texts held at a time, at most 400 paragraphs followed; where the bounds end the reading the page says so in words with the
  number of changes read, and a history that could not be followed to its start says that. The reads go through the same
  storage as the history page's, so the admission gate, the deadlines and the ceilings are the same. A paragraph that
  holds a claim carries the *Sources and views* link. `ChangeInfo` gains `proposal` (the message's `Proposal:` trailer);
  the core's read service gains `page_why`. Both languages, with glossary rows (and the word *anchor*); every state is in
  the structural tests.
- **K-T5 (handoff 005; RFC 005 part 1 D-1, part 2 D-3): making claims and attaching sources.** In the propose form's part
  menu every paragraph gains *Attach a source* and *Challenge this* (a heading or a list has neither): a form in plain
  words (what kind of source, its title, author, date and reference, how directly it speaks to the paragraph, a note; the
  kind of statement only when the paragraph is not a claim yet) that makes the claim and the source **in the same proposal
  as any page edit**, giving the paragraph its `^id` and the page an `id` where they were missing, and adds the source to
  the claim when the paragraph already is one. A web address is refused unless it is `http` or `https`. The `^id` of a
  paragraph others rely on is held by the form and never shown in the text box: it says in one sentence that others rely
  on the paragraph, puts the marker back on the first paragraph of what was written in the form for that one paragraph,
  and, in a form for several paragraphs, back on a paragraph that is exactly the words it was shown with; **removing**
  such a paragraph (or rewording it among others) is an explicit act the person confirms with a box, which also takes the
  expression out of the claim, and the reviewer sees it. A draft or a revision carries the claim and source files forward.
  A proposal's page lists the claims and sources it records, in facts and without a verdict, and says when it sets what only
  curation may. The **curation proposal** is its own small form at `/{slug}/-/proposals/curate?claim=<id>` (status, relation,
  bearing, `last_reviewed`, `availability`, and *judge the sources and the status again on the wording the claim has*; no
  page text), read as the base had the claim; `community_status`, a source's `last_reviewed` and its `availability` are set
  **only** by an accepted curation proposal (a revision of knowledge files alone, no rename: a fact about its content), refused
  before the signer is asked in any other (rule `status`), named by the accepting proposal whatever the file says, and a test
  tries every other way in (the page-edit form, a direct file change, the write API, the import, a system change, which no
  longer writes `knowledge/`). The proposal page also lists **the claims a change touches** (a paragraph reworded or
  removed) with the consequence for what was judged on the earlier wording (*judged on an earlier wording*), and, once
  the proposal is accepted, **one step** that proposes judging all of them again as a single curation proposal. 162
  messages in both languages with their glossary rows; every new state through the structural rules. **Not in this
  entry:** the reviewers' identity decisions and re-confirmations recorded and counted in the participation metrics (part 1
  D-1 d) need a store record the schema does not have (a decision request).
- **K-T8 (handoff 005; RFC 005 part 2 D-9): views.** *Creating a view is a reviewers' act* (the owner's ruling):
  `create-view` needs the reviewer role and the `review` token scope by default (it was the maintainer's). The form
  (`/{slug}/-/views/new`, no script, signed in, the session's token like every write) takes a name (lower case letters,
  digits, `.`, `-`, `_`, at most 64; never changed on the way), what the view is for (*experimental* or
  *interpretation*, neither preselected) and the view to fork from; the act goes through the acceptance path's steps
  (`ProposalAcceptor::create_view`: authorize first, the wiki's write lease, the guard against an unfinished act, the
  views read from prikk now, **the act recorded before the signer is asked**, the signer's `create_view` inside the lease,
  the end settled from the repository and not from the signer's answer). The signer's ledger now reads that record back (it
  answered *none* before, so `create_view` could not be called), and an unfinished key retirement pauses it as it pauses a
  seal. The record (`ViewActRecord`: name, the view and the block it was forked from, kind, creator) is the view's
  recorded origin; `ActStore` gains `begin_view_act`, `finish_view_act`, `open_view_act`, `view_acts` and
  `keeps_view_acts`, whose defaults keep none (**the SQLite store keeps none until the schema is decided**: a server over
  it says views cannot be created here, before showing a form), and the in-memory store keeps them. A holder that dies
  making a view flags nothing (`Purpose::View`: one ref, no history to recover); an act that never ended is settled from
  prikk before the next creation; a closed view's name stays taken; a wiki holds at most 100 open views. *Views*
  (`/{slug}/-/views`, linked from the about page) lists the default view first, called **the default view**, then the
  others by name, each with its recorded kind and origin (a view the service did not make says so in one plain sentence),
  bounded and counted. *Compare views* (`/{slug}/-/views/compare?a=&b=`) lists the claims that differ (in wording, in
  status, in sources; a claim only one view has; a file that cannot be read is not "no such claim") and the pages that
  differ, from the two views' derived knowledge indexes and **one** whole-tree `diff` of the two tips, bounded (200 each,
  the rest counted); `…/compare/{title}` is RFC 003's diff of one page between the two tips, with its word marks and its
  line bound. A claim's page now ends with *Other views*: each of at most ten other views where the claim reads
  differently, with a link to compare; it says that none does only when it looked at every other view, and says so when it
  could not, and prints nothing when the wiki has one view. No view is called canonical, adopted or official, and the
  comparison treats its two sides alike. Every message is in both languages with its glossary row; every state is in the
  structural tests; the route and enumeration tests carry `create-view`.

- **K8 (handoff 005 corrections): *Why is this here?* is exact only when it is.** A step where the alignment finds no
  earlier counterpart for a paragraph (*Attach a source* gives a paragraph its `^id` in the same change that rewords the
  next one: two blocks against two are never paired) now clears *followed through its anchor*; the paragraph is
  *traced by matching the text* and its beginning is *the earliest wording found*, not a claim that it was written
  there. Only a paragraph whose `^id` carried it through every step to the page's creation is still said to be followed
  through its anchor.

- **K13 (handoff 005 corrections): *Why is this here?*, smaller.** A request has a work budget
  (`MAX_WHY_WORK`, 32 million units: a byte read, a block looked at, a cell of an alignment's table) shared by the up to
  twenty alignments of a page's versions; the reading ends at the step that would pass it (`Reach::Spent`), the page
  lists only the changes it did read and says so in the existing *Only the latest N changes … were read here* words (a
  reading that could not read even one change has its own sentence, since *no change was found* would not be true). The
  merge's alignment counts nothing and is never stopped. A `^id` that two paragraphs carry gives way to the paragraphs'
  numbers for the element ids, so no two elements of the page have one id. A change whose message names a proposal the
  acceptance records do not tie to it reads *The message of this change names the proposal …*, not *Made from the
  proposal …*. The claim links' insertion relies on the sanitizer's serializer escaping `<` in attributes; that is now a
  tested fact (`</p>` and `<p id="h-^b1">` in an image's alt and a link's title).

- **K9 (handoff 005 corrections): a source has a name on the claim page.** The derived knowledge index keeps each
  source's title, author or publisher, date (as written) and reference, each cut at 300 characters when the index is
  built (on a character boundary, with a visible ellipsis where it was cut). The claim page's *Sources* line names a
  source by them (*“The Bridge Survey”, by The Harbour Authority, dated Spring 1950 (primary): supports, directly.*, with
  *Where it is found: …* after it, as words and never a link), one line each and escaped; a source that gives no title
  is *a source without a title*, and only one that gives nothing at all is told by its identifier. Never a score. **The
  store's schema moves from 15 to 17** (16 is the view-creation and review records of K3; this build's list skips it,
  the migration test's reserved list says so): four columns on `knowledge_sources`, and the derived knowledge index of
  every tip is dropped by the migration so that it is rebuilt, whole, when next asked for; an index cleared and built again
  is the same index.
- **K3 a (handoff 005 corrections): the SQLite store keeps the record of a view's creation (store schema 16).** Two
  tables, `view_acts` (the start, written before the signer is asked) and `view_act_ends` (how it ended, written once),
  added by an additive migration from 15 (tested on a version 15 file: the wikis are kept, the migrated schema is the
  fresh schema). Both carry the write-once triggers of the system acts. **A name is held by one act that stands** (open
  or created) in a wiki: the store checks it inside the `IMMEDIATE` transaction and a `BEFORE INSERT` trigger refuses it
  from any connection; an act that failed frees it. The database checks the name's form (lower case, 1 - 64 characters
  of `a-z 0-9 . - _`, starting with a letter or a digit, no `..`, no trailing `.`: the form `views::new_name` allows,
  and a test holds the two to agree; the memory store refuses the same names), a created view carries no word and a
  failed one always does, an end names an act that is there, and an index serves `(wiki_id, name)`.
  `ActStore::keeps_view_acts` is `true` for both stores, so a server over SQLite stops answering *Creating views is not
  available on this server* (the answer a store that keeps none, a stub, still gets; **creating views on a real server
  is enabled by K3 and K4 together**: K4 makes a killed creation recoverable). `inklkunn_store::views::is_view_name` is
  the form check. The pins move to 16.

- **K3 b (handoff 005 corrections; RFC 005 part 1 D-1 d): the review record, and the overrides in `inklkunn wiki
  metrics`.** Store schema 16 holds it in four tables written once (triggers refuse an `UPDATE`, a `DELETE` and a
  replacing `INSERT`): `change_claims` (per accepted change and per claim it touched: *kept*, the paragraph reworded and
  its marker continuing, the default of H-1 point 2, or *dropped*, the paragraph and the claim's expression in it
  removed), `claim_reconfirmations` (a kept claim later *re-confirmed by* the curation acceptance that judged it again,
  naming the sealed change it re-confirms; only a kept claim of a sealed change of the same wiki, by an acceptance still
  pending), `proposal_reconfirms` (a curation proposal made by the reconfirm step names the change it re-confirms from
  its creation or never) and `decision_claims` (the claims the revision of a *request changes* decision touched, written
  in the same step as the decision). **The acceptance writes the change's record after its own record and before the
  intent**, so a record that cannot be kept ends the acceptance as failed with nothing signed, a retry writes its own,
  and an acceptance that signs always has its record; the store refuses to add to an acceptance that has ended, and
  every read answers from acceptances that sealed only. `ProposalStore` gains `add_decision_touching`,
  `touching_decisions`, `record_claim_reviews`, `record_reconfirmations`, `claim_reviews_of`, `claim_reviews`,
  `create_reconfirm_proposal`, `reconfirm_of` and `reconfirmations_of` (defaults keep none and refuse every write);
  `ProposalService` gains `decide_touching`, `create_reconfirm`, `reconfirm_of`, `reconfirmations_of` and
  `claim_reviews_of`. A claim said in two places, one reworded and one removed, is told (and recorded) as removed.
  `inklkunn wiki metrics` gains `claims`: of the proposals submitted in the period, the claims their changes **kept**,
  **dropped** and, of the kept, **reconfirmed**, the **changes_requested** (requests for changes on a proposal that
  touches a claim, once per decision), and **overrides** = dropped + changes_requested (the default rejected). They are
  counts of what people did and say nothing of who was right; they begin with schema 16 (what came before left no
  record). Tested on both stores and, for the acceptance, against a real history (a failed acceptance writes records
  that are never read; a store that cannot keep a record stops the acceptance before the signer).

- **K11 (handoff 005 corrections): the reconfirm step.** It now works from **the revision that was accepted** (the one
  whose acceptance sealed), not the last; the curation proposal it makes is created with the note of the change it
  re-confirms (`create_reconfirm`), which the acceptance reads to record, per claim, that the change was re-confirmed;
  and it is **not offered again** while a proposal made for the same change is open (the page says *Judging these claims
  again is already proposed*, linking to it, and pressing the button again makes nothing), nor for claims an accepted
  one already judged again (the page says each was judged again). Closing the proposal without accepting it offers the
  step again.
- **K6 and K7 (handoff 005 corrections; RFC 005 part 2 D-3): the gate is D-3, and no proposal that can never be
  accepted.** What only a curation proposal sets now includes **every change to a source link a claim already has**: its
  `relation`, its `bearing`, the wording it was `judged_on`, or its removal (`Curated::Claim` carries the links'
  judgements; `Curated::departure` is the one comparison). Attaching a new link stays with the page edit it belongs to, a
  link's note stays words, and a new claim brings whatever links it likes; the refusal is the rule *status* with the key
  `sources` (the rule's sentence, its catalog and glossary rows now name the links). The same rule runs where it can be
  acted on: `ProposalService::submit` and `revise` refuse a proposal the acceptance would always refuse
  (`Invalid::KnowledgeCurated`, API reason `knowledge-curated`: a curated value, a link changed or taken away, or the
  removal of a claim whose standing was decided, none of them in a curation proposal), in the sentence the proposal page's
  warning uses; a draft may still be saved. `check::curated_by` judges the files against the base without a tree and is
  the one function behind the submission and the page's warning, and the acceptance's `validate` asks the same
  `Curated::departure` and `Curated::removal_refused`. The page's warning now covers removals and links as well (it
  ignored both); `summary::sets_curated` is removed in its favour.
- **K5 (handoff 005 corrections; RFC 005 part 1 H-1): trust never moves unseen.** A re-judgement, a source link's or a
  status's `judged_on` rebound to other wording, is now a line of its own on the proposal page
  (`ClaimChange::Rejudged`), with the wording it moved to: the whole paragraph, white space made single spaces and cut at
  240 characters, escaped when drawn; when no paragraph of the claim has that wording the line says so, and when the
  claim is no longer tied to a wording it says that. A status whose wording alone changes no longer reads *accepted to
  accepted* (`Standing` is only for a change of the word). The curation form's preview prints the same lines. The page
  says so when a proposal's claim and source files are the files the wiki already has (an empty list used to print
  nothing), and the knowledge section is drawn for a note alone. **A change the page would not describe cannot be sent**:
  `ProposalService::submit` and `revise` ask `summary::undescribed` (a file whose content differs from the base's, without
  the proposal ids the service stamps itself at acceptance, and for which `summarize` gives no item) and refuse with
  `Invalid::KnowledgeNotShown` (API reason `knowledge-not-shown`). *Described* is defined in that one place, and
  `claim_changes` and `source_changes` now take both files apart field by field with no `..`, so a field added to the
  format does not compile until it is described; property-style tests hold it over every field, from many starting points,
  and over thousands of random pairs. `Item::ClaimChanged` carries where the claim is said (`said`) for the surface that
  finds the wording.
- **K10 (handoff 005 corrections; RFC 005 part 1 D-1 b): markers.** A held `^id` is put back on the paragraph that is the
  same *occurrence* of its words in what was sent, by its place among the paragraphs with equal words as the form showed
  them, and no longer on the first one: untouched `Same.` / `Same. ^c1` comes back byte for byte. When the number of
  paragraphs with those words is not what the form showed (one taken out, one added) the text cannot say which the claim
  was, so the marker is *lost*, which asks the person to confirm a removal, instead of landing on a duplicate; words shown
  once and sent once are matched as before. (`marker::reattach_all`; two held paragraphs with the same words, one of them
  removed, now ask about both.) The one-paragraph form puts the marker back after the whitespace it had (a line break
  when it stood on a line of its own, two spaces when it had two), so a text nobody touched is not a whitespace-only
  edit: `marker::reattach_part` now takes the block the form was opened on (**public API: a new first argument**).
- **K14 (handoff 005 corrections): direction characters, and un-deciding.** No text of a claim or source file may hold a
  bidirectional control or format character (U+202A – U+202E, U+2066 – U+2069, U+200E, U+200F): the reader refuses it
  (rule *bad value*, the key of the text), so the acceptance and the derived index refuse it too (a file already in a
  wiki that has one now reads as invalid and is listed on the health page); the source and curation forms refuse it
  before anything is written, with a message of its own (`attach.direction`, English and Japanese, with its glossary
  row; `compose::Mistake::Direction`). A curation that sets a status back to *not decided* is a decision like any other:
  the acceptance writes its own proposal id into the claim's `community_status.proposal`, as it does for every other
  change of a standing, and a claim that nobody decided and a curation leaves so keeps none.
- **K4 (handoff 005 corrections; review finding K-c): a view's creation is a recoverable act.** `Purpose::View` is no
  longer one of the holders whose death leaves nothing to recover, and the guard of every write counts a view creation
  that never ended as an unfinished act: **a holder that dies creating a view flags the wiki.** The reason is prikk's
  (measured on 0.55.0 with real `SIGKILL`s of `branch create`): the ref's pointer is published and then the ref log is
  appended, and killed between them the view is **listed** while `verify` fails (`doctor`: exactly
  `PRIKK-VERIFY-REF-POINTER-LEADS-LOG`) and every other publication refuses until `prikk ref complete <ref>` (RFC 165 R4)
  finishes it; the listing alone cannot tell a whole view from that one. `settle_earlier_acts` is gone: the creation
  settles nothing from `branch list`, and when the view is listed and prikk did not say its call finished, the act
  stays open, the wiki is flagged, and the caller is told *uncertain*. **Recovery treats an open view creation as a
  known act** (`Witness::view_acts`, reason `open-view-act`; the listing now includes closed views, `Storage::all_views`,
  `branch list --all`): the view is listed at the block it was forked from and `doctor` names nothing unfinished: the
  act ends *created* (`end-view-created`); the same with exactly that ref's publication named: the new step
  `complete-view-creation`, **prikk's own recommendation, `ref complete`, which is not P15's `seal`**, run by the signer
  bound to the recovery act as `complete-publication` is (`RecoveryOp::CompleteViewCreation`, `Recover::complete_view_creation`;
  it re-checks `doctor`, the recorded creation, prikk's own `--plan-only` (first publication, sequence 1, no log tip, no
  tail, onto the block the source ref points at) and the lent lease, pauses for an unfinished key retirement, and refuses
  to call it done while `doctor` still names the lead), then the same end; the view is not listed and nothing is
  unfinished: *failed* (`interrupted`; `end-view-failed`); anything else is the new stop **`view-creation-unclear`**
  (the view at another block, its block unreadable, not listed while `doctor` names its publication, a queue or a torn
  commit beside it),
  with a new section in `docs/RECOVERY.md`. The end steps read prikk again at the step and refuse a state that moved
  since the plan. New signer verb `ref complete` (cost rule: `branch create`'s, flat); `Storage::all_views` has a default
  (the open views) and the driver answers `branch list --all`. Tests: real kills of `branch create` (a loop of real
  `SIGKILL`s whose landing is counted, and the state made on cue), the three plan rows and each stop, the signer's
  binding and every refusal, the runbook's statements against prikk's and recovery's words.

- **K12 (handoff 005 corrections; review finding K-i): views.** **The name check sees every view, closed ones
  included, ignoring case:** the creation reads `Storage::all_views` (`branch list --all`), so a closed `heads/a_` made
  outside the service (prikk keeps a closed branch's name, in any case) ends *a view of that name is there*, with no act
  recorded and prikk not asked, and not *Failed (refused)*; closed views do not count toward the bound on open ones, and a
  view created and closed before the creation looked is still created. **The name is validated before it enters any
  resource or audit name** (`create_view` checks `views::new_name` first, a pure check of what was typed). **Names that
  are retired words are refused, whole and in any case:** *official*, *canonical*, *adopted*, *true*, *false*,
  *verified*, *proven* (`NameFault::Retired`, `views::RETIRED_NAMES`; `official-draft` is a name), with a catalogued
  message (`views_new.problem_name_retired`, English and Japanese, glossary row). ***Compare views* is held by its two
  tips** (the wiki and the two block ids, in the order asked; both blocks are immutable, so nothing is forgotten when a
  view moves: a moved view is another block and another key): a pair seen again is answered without the whole-tree
  `diff`, the tips in the answer are the ones read now, an answer that could not be made is not held, and at most 32
  comparisons of each kind are held (`CACHED_COMPARISONS`, oldest first). A claim page's ***Other views* uses the same
  hold** (`ReadService::claim_differences`, the claims two tips differ in, held under the same key and used by the
  comparison too) instead of reading two indexes and comparing them for every other view on every request; the indexes
  of a comparison are read for the tips it read (`knowledge_at`), so that a view that moves meanwhile cannot put another
  block's claims under a block's key. Tests: the closed name against a real prikk (the signer makes a view and the
  stand-in closes it) and a fake listing, every retired word in three forms, the order of the checks, the hold and its
  bound, its key, its pinning, and the claim page through the same list.

- **M1 (handoff 010 maintenance): RUSTSEC-2026-0459 fixed by upgrading, no ignore.** `rusqlite` 0.40 → 0.42
  (`libsqlite3-sys` 0.38 → 0.40) carries SQLite **3.54.1** in place of 3.53.2 (a crafted database and `-wal` pair with
  different page sizes could return uninitialized page-cache memory); the lock keeps its 210 entries and no source
  line changed. **The minimum compiler rises from 1.88.0 to 1.89.0** (`rusqlite` 0.42 declares it; the `msrv` job, the
  README and `Cargo.toml` follow; the pinned compiler, 1.99.1, does not change). 0.2.0's exposure was a pair written
  into the operator's data directory by its own user; 0.3.0 ships the fixed SQLite.

- **M2 (handoff 010 maintenance): the curl ceiling test no longer assumes one curl build.** The real-vector test ran
  the real `curl` under a 1 MiB ceiling and expected it not to connect; curl 8.23.0 starts under 1 MiB. It is now
  three tests that hold what handoff 002's F3 is for without the host's curl version: the vector's child reports
  exactly the configured ceiling; a recording `prlimit` shows the wrapper's command is `--data=<ceiling> -- bwrap ...
  -- <the real curl>` with the ceiling once and nowhere else; and a stand-in that holds 96 MiB is stopped by a 48 MiB
  ceiling in the real bubblewrap while a 2 MiB one runs under the same ceiling and the 96 MiB one runs with room
  enough (so the vector is not what failed). No prikk-dependent test is added or removed.
- **K18 (handoff 005 corrections, second): schema 16 before it ships.** Version 16 is edited in place (it is
  unreleased): `view_acts.created_by` now `REFERENCES accounts(id)`, as every other actor column does, and
  `view_act_ends.word` refuses `''` where it is not null (the outcome and word check stays). Both stores follow: recording
  a view act for an account that is not there is `AccountNotFound` (the memory store as the database), and a failed end
  with an empty word is refused by both. Tests make real accounts for the views they record; the raw-connection test
  proves the foreign key and the check from the database, and both stores' suite proves the refusals.

- **K19 (handoff 005 corrections, second): views and the signer.** `ViewCreationFacts` carries the recorded `from_block`
  (the block the view's source pointed at when the act was written, under the lease), and finishing an interrupted view
  creation (`complete_view_creation`) requires prikk's `ref complete --plan-only` to target **that** block, not wherever
  the source ref points later; the doc now says exactly what the code checks (it chose the record over a re-read of the
  tip: the record is what the act was bound to, and a tip read at recovery time is not). **A key rotation, kept or
  retiring, refuses while `doctor` names a view's unfinished creation** (`PRIKK-VERIFY-REF-POINTER-LEADS-LOG`, whatever
  its wording), with the words of the cleanup case and `inklkunn wiki recover <wiki>`: completing it appends a record
  the then-current maintainer key signs. The recovery plan stops at `retirement-unfinished`, with the retirement's note
  and command, for a view lead beside an unfinished retirement, as it does for a queued patch (the creations that sign
  nothing are not held up). Tests: against a real prikk, the rotation refusal (both old-key choices, then the rotation
  after recovery) and a creation recorded at an older block than its interrupted ref points at; plan unit tests.

- **K20 (handoff 005 corrections, second): *Why is this here?* and a repeated `^id` at the creation.** A paragraph whose
  `^id` more than one block of the page carries is followed by its text, not through its anchor, also when the page's
  only change is its creation (there was no step comparing two versions' ids, so the page alone was believed). A
  paragraph alone with its id is still followed exactly to the creation. Test: a page created with a repeated id and a
  unique one.

- **K21 (handoff 005 corrections, second): the metrics do not add claims to decisions.** `inklkunn wiki metrics` no longer
  reports `claims.overrides` (`dropped` plus `changes_requested`): a drop counts claims and a request for changes counts
  decisions, and a sum is neither. `kept`, `dropped`, `reconfirmed` and `changes_requested` stay, each documented with what
  it counts, and the record now carries `claims.since_schema` (16) and `claims.note`: the review record begins with
  store schema 16, and a change accepted or a decision made before it is in none of the figures.
  `ClaimFigures::overrides` is removed (`REVIEW_RECORD_NOTE` and `REVIEW_RECORD_SCHEMA` are added). Tests: the hand count
  of the review-record figures, and the command's whole line.

- **K22 (handoff 005 corrections, second): the reconfirm step.** The curation proposal the step makes now **says which change
  it re-confirms** (a line on its page linking the proposal that made the change; catalog `touch.reconfirm_of`, English and
  Japanese preview, glossary row). **At most one open proposal per change, kept by the store:** schema 16 (edited, not
  followed) gets a trigger on `proposal_reconfirms` that refuses a second while a proposal that re-confirms the change is a
  draft, held for review or needing revision, from any connection, and the memory store does the same; the refusal is
  `AlreadyRecorded` and the proposal is not made. Two presses at once make one proposal and the other press is told
  *already proposed* (with the proposal, when the person may see it: another's draft is theirs alone). **The offer and the
  record are of the tip:** the claims the accepted change touched are read against the view's tip as the wiki has them
  now (a claim judged again since is not offered; one reworded again since is), the same tip the step reads the claim
  files it writes from; the proposal page of an accepted proposal uses the same reading. A **renamed page is read at the
  base by its base path:** a re-judgement's wording is looked for in the page at the proposal's base, found through the
  base's index (`ReadService::knowledge_index_at_base`) and not the tip's. Tests: the line and its link, the store's
  rule on both stores and through a raw connection, the press that loses (a draft nobody else can see), two presses at
  once, the offer at the tip, and a page renamed after the proposal was written.

- **K23 (handoff 005 corrections, second): small corrections.** **U+061C (the Arabic letter mark)** joins the direction
  characters that no text of a knowledge file may hold (the reader and the form check one set). **The work meter's factor is
  stated:** a cell of an alignment's table is one unit of work however long the two blocks it compares are (charging by
  length would multiply every charge by a block's size and the limits set for the callers would have to be set again);
  the bytes one table compares are bounded by its size limit, and the documentation of the meter, of the request's work
  budget and of the tracer say so. **`Storage::all_views` is a required method:** it no longer falls back to the open views
  (which would let a closed view's name be taken again unnoticed); every implementation, test doubles included, says what
  it has (`inklkunn-prikk` has a test that keeps a default body out of the declaration). **A claim page's *Other views*
  compare from the tip its own index was read at** (`ReadService::claim_differences_from`, with `knowledge_at` public):
  the tip is read once, so a view that moves meanwhile cannot put one state's index beside another's differences.
  **`complete_view_creation` keeps the *seeds not removed* note** when prikk's plan or `ref complete` fails and the
  temporary directory of decrypted seeds could not be removed, as an acceptance does: the failure is the news and the
  directory is told beside it. The stale comments of `sqlite.rs` (what the migrations remove) and of the migration-list
  test (no version is reserved now) are corrected. Tests: the direction set on the files and on the form, the
  declaration of `all_views`, the pinned tip of the other views, and (against a real prikk) both failures with a seed
  directory left.
- **K17 (handoff 005 corrections, second): *re-confirmed* is recorded only for what was judged again.** The review
  record written at the acceptance of a reconfirm's curation proposal (`acceptance/claims.rs`) now marks a claim
  *re-confirmed* only when the claim file it writes carries at least one judgement (its status's wording, a source link's)
  and **every** one of them names the wording one of the claim's paragraphs has in the accepted change
  (`judged_on_now`, against the tip's index: a curation proposal writes no page). A revision that changes only a note, or
  judges the status again and leaves a link on the older wording, records nothing for the claim, so the page no longer
  says *judged again on the wording it has now* of a claim it did not judge again. Tests: the predicate (every judgement,
  one stale, none carried, no paragraph) and, against a real prikk, a note-only revision, a half-judged one and a whole
  one through the acceptance (one prikk-dependent test).

- **K16 (handoff 005 corrections, second; review finding L-b): removal is curation when it would take anything curated
  with it.** `Curated::removal_refused` is now *the file's curated content is not what a fresh file's is*: a claim file
  with a decided status, an un-decided record **or a source link**, and a source file that was checked or marked
  available or not, is not removed by a page proposal (it was only a decided status before, so a page proposal could
  remove an undecided claim that held a *challenges* link and record another on the same paragraph with only *supports*:
  the challenge was laundered out although taking a link away is curation's). **A knowledge file the base has and the
  layer cannot read** (more of them since K14) **is not removed or written over by any other proposal either**: the
  rule is the acceptance's (`Touched::unreadable_before`, filled from the tree) and the submission's
  (`curated_by`), refused as `Rule::Status` with the field `unreadable`; the page gives it a line of its own
  (`Item::UnreadableReplaced`: *Replaces or removes a claim file that cannot be read … What it held is not known.*,
  never *Removes a claim*), and the sentence a refused proposal is shown names removals and replaced unreadable files.
  Tests: each curated content in a claim and a source (status, un-decided record, wording only, either relation of a
  link, availability, review date), the laundering shape at submit, in `validate` and through a real acceptance (one
  prikk-dependent test), the unreadable file removed and written over at submit, in `validate`, in `judge` over a tree
  and on the page.

- **K15 (handoff 005 corrections, second; review finding L-a): the page says everything the core describes, and
  which.** Every line of *Claims and sources in this proposal* names its subject: **a claim by an excerpt of a paragraph
  it is said in** (80 characters, escaped, in `<bdi>`; else, as the proposal leaves the pages, the paragraph the base
  had; a claim said nowhere that can be found is named by its identifier) and **a source by its name**, the title,
  author and date the claim page names it by (K9), **in `<cite>`** (`<cite><bdi>…</bdi></cite>`: a title written to read
  like another source's whole line stays inside its own boundary). A new claim's line carries its **community status**,
  its place in its life, the wording the status was decided on, each relation (naming the other claim) and each
  citation (naming the source); a new source's line carries its name, where it is found, **whether it can be had, the
  day it was checked**, that a copy is kept and what replaces it; a changed source's **identity and reference show their
  new values** (and a source no longer marked as replaced says so, which a *now replaced by another source* line could
  not); the lines under a changed claim name the paragraph that is added or let go, the other claim of a relation, and
  the source of a citation, a re-citation, a note and a re-judgement; a removal and a file that cannot be read are
  named by what they were. **No renderer arm drops a field with `..`:** `Item` carries what the file says
  (`ClaimMade`: lifecycle, relations, the wording of the status; `SourceRecorded`: identity, reference, copy, review day,
  replacement, availability in place of `title` and `checked`; `SourceChanged`: the name by which it was known;
  `SourceChange::{Identity, Reference, Superseded}`: what they become; `Unreadable`: the file's name), `summarize` takes
  `Claim` and `Source` apart field by field, and `lines` matches every field of every item (the one field named `_`,
  an unreadable file's family, is said by its name). New catalog slots: `Msg::rich` with `catalog::bdi` and
  `catalog::cite` (markup only the catalog builds); the lines are `Rich`, not text the page escapes. Eight messages are new,
  nineteen reworded, one (`know.source_recorded_untitled`) is gone: the name of a source without a title says so.
  **`KnowledgeNotShown`'s property tests moved to the rendered layer** (`tests/proposals/rendered.rs`, the generators of
  the core's `summary.rs` made to fit a wiki): for every single-field change of a claim and of a source, from five starting
  points, through the router and the real renderer, the page differs from the page of the proposal that changes nothing and
  names the claim by its paragraph or the source by its name; the printed fields of a new claim and a new source likewise;
  and the standing, availability, review day, identity and reference are asserted by their words. (The wording a status
  or a link was judged on is not printed when no paragraph has it: two such wordings read alike, as the line says they
  are *a wording none of the claim's paragraphs has*; a new link's note is not printed.)
- **M3 (maintenance handoff, 2026-11), prikk 0.56.0 confirmed.** The suite passes on the published 0.56.0 apart from
  the curl test (M2): 2,618 passed, 1 failed. Every verb the driver and the signer run was compared across 0.55.0,
  0.56.0 and 0.57.0 on one repository (`m3-measurements-2026-11.md` has the table): none of 0.56.0's output changes
  touches what inklkunn parses. `log --ref <block id>` now exits 1 (PK-36's fix), and the driver already refused a
  block id itself; `show`, `extract` and `worktree-status` gained fields the readers pass over; `unreached objects` now
  counts patches and blobs, in the warning shape recovery already treats as a note; the signer in `branch list` (PK-26)
  did **not** ship in 0.56.0 or 0.57.0. New tests (`inklkunn-prikk/tests/prikk_shapes.rs`, stand-ins that print the
  measured shapes, so no real prikk is needed): the fields 0.56.0 and 0.57.0 added are read past, and a log prikk
  refuses is an error with prikk's words, never an empty history.

- **M3, prikk 0.57.0 measured; one break found and fixed.** The whole suite on a 0.57.0 build (prikk `main` `bfd5b3c9`,
  not a published asset) failed 13 tests: the curl test (M2), two timing tests under load, and ten that read `prikk
  unlock`. 0.57.0 lists a lock it wrote with one more detail line, `recorded start: <time>`; the strict reader refused
  the listing, so **recovery could not read its witnesses and a wiki stayed flagged** after a killed prikk. The line is
  now read past (one place, with a value; anything else is still refused). Nine of the ten were fixtures that planted a
  lock body no prikk ever wrote (`note=planted`): 0.57.0 reads such a body as cut short, so the fixtures now write
  0.56.0's body byte for byte (0.55.0 and 0.56.0 read pid and kind only), and four assertions that named prikk's words
  for an old-style lock also accept 0.57.0's `unknown: written by an older prikk`. `docs/RECOVERY.md` names both
  wordings of an empty lock's `recorded pid`. **A repository created by 0.57.0 is format 8 and 0.55.0 and 0.56.0
  refuse to open it** (measured; `m3-measurements-2026-11.md`). Tests: the 0.57.0 listing with and without the line
  and every misplaced form, an incomplete body, and the `doctor` shapes of 0.56.0 and 0.57.0 for unreached content.

- **M3, PK-39 adopted: the late-seal check (C11) reads `sealed_by` where prikk provides it.** With prikk 0.57.0 the
  check reads who sealed each block from `verify --format json`'s `block_seals` (the key ids as recorded, trusted or
  not); a report without the field is an older prikk, and `prikk verify`'s `sealed-block` lines are read as before. The
  report is the better source: the text lists nothing for a repository whose verification fails, so up to 0.56.0 the
  check could not run after a retirement (and `keys status` said "this check cannot see <key>"); with 0.57.0 it runs
  there too, finds a block a retired key sealed after its replacement, and does not claim blindness. The field is read
  strictly (a list of objects with a 64-hex `block_id` and a `sealed_by` list of key ids prikk accepts, no block twice),
  a report that did not run to the end gives no list, and a stage that ran out of memory is the ceiling. New:
  `Signer::late_seal_check` (`LateSealCheck { late, sees_untrusted_keys }`; `seals_by_superseded_keys` is unchanged).
  **The floor stays 0.54.0**: nothing here needs 0.57.0, it uses the field where it is. Tests: the parser against every
  malformed form, a stand-in `prikk` for both shapes (the text is never asked when the report has the field), and four
  real-prikk tests made to expect the right behaviour on the prikk that is on `PATH` (a retirement, the late block after
  it, the blind-key message, the unfinished retirement).

- **M3, PK-43 adopted: recovery offers to take an orphan queued patch back, and does it only on a person's word.** A
  sound patch queued with no recorded act describing it is still the `foreign-queued-patch` stop, and recovery still
  never discards on its own judgement. With prikk 0.57.0 the plan also shows prikk's own preview (`prikk uncommit <patch>
  --plan-only`: the commit's message and the paths it changes, control characters replaced and lines cut) and the
  command that confirms it: `inklkunn wiki recover <wiki> --discard-queued-patch <patch id>`, the id typed in full. Then
  recovery takes that one patch back (`prikk uncommit`, kept in prikk's recovery log and undoable with `prikk doctor
  --recovery-restore <run>`, which the step prints), puts the worktree back to the tip, verifies, reconciles and clears
  the flag. The word is audited when the act begins, recorded in the plan the act keeps, and bound to that one patch in
  the signer (`RecoveryOp::DiscardQueued`, `Recover::discard_queued`); the signer also refuses unless the queue holds
  exactly that patch on the branch prikk's own plan names. It is never taken for a patch that is a recorded act's
  change, beside several queued patches, with a foreign process, a torn log or an interrupted state, or for a queue that
  targets another branch than the worktree's. `--plan` with the word is a usage error. **It degrades honestly**: on
  0.54.0 to 0.56.0 `uncommit` is `unknown command`, the plan says so, the word is ignored and the stop is all there is
  (`DiscardOffer::NoVerb`). The floor stays 0.54.0 (the feature is absent below 0.57.0, and a 0.57.0 floor needs the
  published asset's checksum). New: `Recovery::recover_with` and `RecoverOptions` (`recover` is unchanged),
  `plan::build_with`, `Step::DiscardQueued`, `Witnesses::uncommit_plan`, a cost rule for `uncommit` measured on 0.57.0
  (5.0 x the store, floor 16 MiB). Tests: the plan's rules against every other state, the signer's guards with a real
  prikk (and the undo), prikk's text read strictly, the verb absent, and the CLI's flag. Five new prikk-dependent tests in all across M3 (the shimmed run's self-skips: 289 to 294); the stand-in tests need no prikk.

Dependency updates (the architect, 2026-11-14).

- **`ammonia` 4.2.4 → 4.2.5** (RUSTSEC-2026-0533, mutation XSS through embedded SVG and MathML) and **`rustix` 1.1.5 → 1.1.6**
  (RUSTSEC-2026-0486, unsound `xattr` functions), lock only. inklkunn's sanitizer allows no MathML, SVG or raw-text element and drops
  them with their contents, and Markdown's raw HTML is escaped before it, so **0.2.0 is not affected**; `rustix`'s `xattr` functions
  are not called. The gates match by version, so the lock moves to the fixed releases.

- **K26 (handoff 005 corrections, third): a reconfirm never leaves a draft.** The reconfirm step created its curation
  proposal and submitted it in a second call; a submit that failed (the wiki's hourly limit, a check) or a crash between
  the two left a draft only its author could see, and the store's *one open proposal per change* counted a draft as open,
  so every later press (a curator's too) was told *already proposed* with no link. Now `ProposalService::create_reconfirm`
  runs the submission's own checks first (the finished text, the files, the knowledge files, the wiki's hourly limit,
  shared with `submit` in `judge_submission`, `check_submission_rate` and `submission`) and only then writes the proposal
  **already held for review**, with its creation, its submission (revision 1) and the note of the change, in one store
  transaction (`ProposalStore::create_held_reconfirm_proposal`, on both stores; it replaces `create_reconfirm_proposal`,
  which could only make a draft, and refuses a record that is not whole). A refusal leaves nothing behind: no proposal, no
  draft, no note, nothing counted. The press that loses a race gets the winner's link when it may read it, which a held
  proposal always allows. Schema stays 17; migration V16's trigger already names the open states and is unchanged, and a
  test now pins its list to `ProposalState::is_open` for every state, from a raw connection (a draft included) and on both
  stores. The returned record is the held proposal: callers no longer submit it. No prikk-dependent test is added or removed.

## [0.2.0] — 2026-11-04

**M2 — propose and accept.** People sign in, propose changes to pages, review them, and a maintainer accepts them into the
wiki's signed history. Accounts come into being by the operator's invitation (`inklkunn account invite`), or by joining
where the server allows it (`INKLKUNN_JOIN`, default `off`); sign-in is a local password or an OpenID Connect provider. A
proposal is written through a form that needs no script — a whole page, a section, a paragraph, or a new page (every link
to a page that does not exist leads there) — and its page shows what changes, word by word. Reviewers comment, approve or
ask for changes (nobody decides on their own proposal); accepting seals the change into the history with the wiki's key,
shows it on the page, and checks the history again. A proposal whose base moved is re-authored without its author; two
edits of the same paragraph stop with both texts side by side. The same through a write API with scoped tokens
(OpenAPI 3.1). Also: undo as a proposal, an inbox and following, quotas that refuse by name, recovery from a stopped
acceptance (`inklkunn wiki recover`, `docs/RECOVERY.md`), key rotation and retirement (`docs/KEYS.md`), participation
metrics (`inklkunn wiki metrics`), and the knowledge layer's files and checks without a surface yet.

**Before you upgrade or deploy.** The server now opens the key store and holds the means to sign (0.1.0's held none): keep
the operator's secret file readable by the service's user only (SECURITY.md), and **restart the server after the key store
first appears**. It must sit **behind TLS** (the cookies are `Secure`), and **declare your reverse proxy** in
`INKLKUNN_TRUSTED_PROXIES` (its own addresses only), or every visitor counts as one address and thirty bad passwords by anyone lock
every password sign-in for fifteen minutes. The store migrates itself from 0.1.0's schema at the first start; 0.1.0's key records open as they are.
**Not in this release:** the knowledge layer's pages (M3), governance and removal of content (M4: removal from a signed
history is lossy; do not host content you may have to erase). OpenID Connect has run against a stand-in provider and a
real browser, not yet against a third-party provider; no person who uses assistive technology has tested the site yet
(the about page says so); Japanese is a labelled preview. Runtime: prikk ≥ 0.54.0, bubblewrap and prlimit on the host
(curl for OpenID Connect), or the container image.

The entries below are in the order the work landed.

- **Proposals T2 (RFC 004 D-5; handoff 004): the block-aware three-way merge.** `inklkunn_markup::blocks` segments a body
  into its top-level blocks from the parser's own structure (paragraph, heading, list, quotation, table, code block, HTML,
  container, footnote), each with its `^id` (a trailing one on a paragraph, a line of its own after a table, code block,
  list or quotation) and its whitespace-trimmed text; a container is one block, and anything the parser consumes without a
  token (a link reference definition) is a block of its own, so only whitespace lies between blocks. `inklkunn_core::merge`
  merges three sides (base, tip, proposal) of a set of pages: pages by page id (an edit lands on the renamed file; removed
  on one side and changed on the other, two new paths for one page, and two pages at one path are overlaps), the
  frontmatter key by key (unreadable frontmatter as one unit), the body block by block (matched by `^id`, then by unique
  content, then by a bounded longest-common-subsequence alignment). A change on one side is taken, the same change on both
  sides once, anything else is an **overlap carrying both texts and where**; the code never resolves one, and two sides
  that add different blocks at one place overlap too. `MergeOutcome` is `Clean { pages }` or `Overlaps { overlaps }`. The
  measurement hook says, for each overlap, whether a line-level merge of the block would have been clean
  (`LineLevel`, `MergeOutcome::line_level_clean`), made with the same engine and no dependency, and never applied: on the
  spike's 21 overlaps it is 0. The 120 pairs of spike B are a committed fixture
  (`crates/inklkunn-core/tests/fixtures/merge-spike-b-pairs.json`, regenerated without prikk by
  `inklkunn-corpus merge-fixture`, which refuses to write if its replay of the generator differs from the report) and yield
  exactly 99 clean merges and 21 overlaps, the same verdict as the spike's own model on every pair. `tools/corpus` now
  measures with the product function (a workspace path edge to `inklkunn-core`) and reports the hook's count.
- **Handoff 004 T1: store schema 9, the proposal records.** Explicit migration from 8, tested on a copy built from
  `MIGRATIONS` up to 8 (and the migrated schema is compared with a fresh one): proposals, their revisions, the log of every
  transition, comments (with their anchors: a block's `^id`, or a quote selector and an ordinal), reviewers' decisions,
  acceptances and their results, intents and their completions, the wiki's lease holder, watches and inbox events. New ids
  are UUID v7 (`CommentId`, `DecisionId`, `AcceptanceId`, `IntentId`, `InboxEventId`). **Revisions, transitions, comments,
  decisions, acceptances, acceptance results, intents and intent completions are written once, in the database itself**:
  triggers abort an `UPDATE`, a `DELETE` and an `INSERT` over a key already taken, counting every unique key of a table (a
  replacing insert on the decisions' second key, one per reviewer per revision, removes a row without firing a delete
  trigger). The memory store refuses the same writes through the trait; one suite runs on both. An acceptance and an intent
  end by a second record, so nothing is updated; a retry of an accept of the same revision finds the pending or sealed
  record and starts nothing (CON-6), and a failed one does not stand in the way. **A draft is its author's alone, in the
  store**: every read takes the viewer, and a proposal that was never submitted (a withdrawn draft included) is absent to
  anyone else; it is a working copy replaced by each save and turned into revision 1 by the submission, so saving does not
  use up the revision quota.
- **Handoff 004 T1: `inklkunn_core::proposal`.** Create (the base from `Storage::views` through the read service: the
  view's state id and the block it belongs to), save and resume a draft, submit, revise, comment, decide (one per
  reviewer per revision, on the latest), ask for a revision, decline, withdraw, supersede, and the reads. **The state
  machine** (`proposal::machine`) names every transition and every other is refused (`NotInThisState`). Every act is
  authorized first through `authorize()` (propose on each page touched, comment, review, accept); only an account acts;
  a proposal of another wiki is as absent as one that does not exist. Refusals are typed (`ProposalError`) and carry no
  path or text; the glossary words arrive with the pages (T3). **Quotas** (`QuotaConfig`, the RFC's defaults, halved for
  an account in its first week): open proposals, revisions per proposal, proposals per hour per principal and per wiki,
  accepted bytes per day, the acceptance queue; a refusal names the limit and carries the allowed value, the current
  one, what was asked and when it resets (rolling windows). **Participation metrics** (`proposal::metrics`) are computed
  from the records alone: proposals by state, medians to the first review and to a decision, acceptance and decline
  rates, distinct and first-time proposers, reviewers per proposal; drafts are in no figure and a proposer's decision on
  her own proposal is not a review. Tests: one invariant suite per record family on both stores; every sequence of up to
  four acts (three on SQLite) held to the state machine; every quota at its boundary; drafts invisible to other accounts;
  reopen persistence; the metrics against a hand count.
- **Handoff 004 T6: undo as a proposal, the inbox and following, the flags (RFC 004 D-8, D-9, D-10).** *Store schema
  10*: `proposal_rollbacks` (a proposal is a rollback from its creation, in the same step, and the block it undoes is
  written once; the migration from 9 adds one table and changes nothing), `ProposalStore::create_rollback_proposal`,
  `rollback_of` and `revision_flags` (the flags of the latest revision of each proposal asked for, in one step).
  **Propose a rollback**: `ReadService::rollback_plan` computes the inverse of a change at content level (one `show`,
  two `cat`s per page): the pages as they were before it, byte for byte, a page the change created removed, one it
  removed written again, one it moved put back. The plan is written against the *undone block* (its state and id are the
  proposal's base), so the merge reads it as "take back what this block did": later edits elsewhere on a page stay and a
  later change to the very text is an overlap for a person, and the message's `Base:` trailer names the undone change.
  A rollback of a rollback restores the change because an accepted rollback is a change like any other. What cannot be
  undone says why (the first change, a file that is not a page, text that is not text, more than 200 pages, a change
  that left the pages as they were). `ProposalService::propose_rollback` plans, opens and submits it (authorized before
  the repository is asked); `create_rollback`, `rollback_of` and `RollbackSource` are the parts. **The inbox**
  (`inklkunn_core::notify`, `inbox`): the proposal service, given a `Notifier` (`with_notifier`), tells the proposer of
  a review, a decline, a request for a revision and a supersession, the people who held a proposal (those who may
  accept, and those who reviewed or commented) of its withdrawal, the author of a comment replied to and the proposer
  of a comment on their proposal, and each account a comment mentions by sign-in name (`@name`; at most ten; the writer
  never learns whether a name exists). Telling is best effort and counted (`Notifier::failures`); an act never fails
  for want of an inbox record. **An event is ids and a kind** and is made only for someone who may read the proposal
  now; the inbox then reads what it is about *as the person who opens it*, so an event about a wiki that stopped being
  theirs to read is one sentence that names nothing. `Notifier::accepted` and `Notifier::page_changed` are what the
  acceptance calls after the seal (a watch is told of a change that is in the history and of nothing else). Channels
  are a trait (`Channel`) with the in-app inbox as its one implementation. Watching a page or the pages under a title
  (`Inbox::watch`, `unwatch`, `followed`) needs the right to read it and an account, and is bounded (500). **Flags**:
  recent changes carry the accepted revision's *minor* (real now) and *machine-assisted* flags
  (`ReadService::with_revision_flags`), filter on them (`minor`, and `automated`, which now covers what the service
  made itself and what a machine assisted; a filter the records cannot answer is refused, never answered as if nothing
  were flagged), and label machine-assisted content with the person who accepted it. *Web*: `/-/inbox` (an account
  is needed and said in words; each kind of event a sentence in the reader's language, other people's text escaped;
  mark everything read and stop following are `POST`s from this site), `POST /<slug>/-/watch/<Title>` (the generic
  not-found page for a wiki the visitor may not read, then an account, then a page the account may read), the *minor*
  choice and the labels on the changes page, `minor=` on the feed and the API (`minor`, `machine_assisted` in each
  item, drawn in the OpenAPI document), `<category term="minor|automated"/>` in the feed. Forty-one catalog
  messages in English and the Japanese preview, with their glossary rows; three existing messages say that the
  *automated* filter covers machine-assisted changes too. Tests: the inverse on exact text (multibyte, CRLF, no final
  newline), a creation, a removal, a move and a rollback of a rollback; every refusal; the events of every act and who
  is not told; a wiki restricted after the event; watches fired by the acceptance's function only (against a channel
  that remembers); the flags on both stores, in the list, the feed and the API; the migration from 9; the structure
  rules on every new page state in both languages; the route-enumeration harness over the new routes.
- **Handoff 004 T3: the propose form, the proposal's page and review (`inklkunn_web::proposals`).** Routes under
  `/<slug>/-/proposals` and `/<slug>/-/discussion/<Title>`, each registered with the action its handler authorizes
  (`propose`, `comment`, `review`, `accept`), and every one begins with the wiki's read check, so a wiki the visitor
  may not read, a proposal of another wiki and a draft that is not the visitor's own are the one not-found page. The
  **form** (a page, a section by its heading, a part by its position, or a new page; the page view and the page that
  is not there link to it) holds the current text, a summary (required, at most 120 characters), sources, and *Preview*,
  *Show changes*, *Save as draft* and *Submit*, all server round trips and none needing a script; a section or part goes
  back into the **base text the form was opened on** (so the page may have moved since); a page keeps its own line
  endings (a browser sends CR LF whatever the page had). **Sizes are judged before anything is parsed** (a page over
  1 MiB, a proposal over 2 MiB, a body no form can be); the keys the service owns (`kind`, `visibility`, `system`) are
  refused when a proposal changes them unless the principal administers the wiki; a new page's title goes through
  `title_to_path` and a refusal names the rule, never the path; the linter's findings are suggestions on *Preview* and
  *Show changes*. Everything wrong is said at once on the form, with the person's text still in it, and a refused
  submission leaves no draft behind. **The proposal's page** shows its state in the glossary's seven words, who
  proposed it, what it changes page by page and part by part (the history's line diff and word marks over
  `inklkunn_markup::blocks`), the thread with comments anchored to parts, each decision with the revision it is bound
  to ("on revision 1", and a note once a newer revision exists), and the actions the viewer's role and the state allow.
  A comment's anchor (the part's `^id`, else a quote with its surroundings and position) is **re-anchored every time
  the page is drawn** and shown as *orphaned*, with the text it was about, when nothing matches. Decline, withdraw and
  ask for a revision are thin forms with a required reason; *Accept* is a form whose answer says it is **not available
  yet** (`review::accept` is the one function the acceptance replaces; `AcceptOutcome` and `render_accept_outcome` hold
  the words for every outcome of an acceptance). A page's discussion lists the proposals that change it with their
  threads. An anonymous visitor is told in words that proposing needs an account. **Words:** a catalog entry in English
  and Japanese (preview) and a glossary row, in the same order, for every state, outcome, named limit, refusal, label
  and suggestion; the concept rows for proposal, held for review, accepted and declined are now in use, and draft,
  needs a revision, withdrawn, superseded and orphaned are added. Additive changes outside the crate: `Services`
  gains `proposals`, `accounts` and `acceptances`, `ReadService` gains `source_at` (a file at a block) and `preview`
  (source text drawn and sanitized as the page would be), and the binary builds the service. The route table's unit
  test and the route-enumeration harness now hold the proposals' writes to their actions; the structural harness
  runs 49 more states, both languages.
- **Handoff 004 T4: acceptance under the lease (RFC 004 D-4, D-6, D-14).** *Store schema 11*: the message an
  intent means to seal (`intents.message`; the facts a seal is bound to and the reconciliation compares are read from
  it, so a later rename of a person changes nothing), `wiki_flags` (the recovery flag: set once, the first reason
  stays, cleared by recovery), `proposal_overlaps` (both texts of every overlap, written once), `system_acts` and
  `system_act_ends` (the service's own changes, which have no proposal; written once), `ActStore`,
  `AcceptanceStore::pending_acceptance` and `sealed_acceptances`; the migration from 10 adds tables and one column and
  changes nothing. **The write lease** (`inklkunn_core::lease`, `inklkunn_signer::lease`): one writer per wiki across
  processes (the per-wiki `flock` of the 002 corrections, held for the whole act, so the command line and a running
  server exclude each other), a fair in-process queue of at most 64 waiting per wiki, a bounded wait after which the
  waiter is told *busy* with how many acts are ahead and an estimate from what leases of that wiki have taken, and a
  durable holder record written at the take and cleared at the release (a record of another holder found at the take
  is a holder that died). The acts inside a lease on one thread (the signer's `accept`, the export, the checkpoint
  refresh) *borrow* it (`Signer::with_leases`, `Maintenance::with_leases`, `WriteLease::scope`): `Accept` is
  unchanged. It blocks the calling thread, as every service here does; there is no async runtime in the crate.
  `WikiLeases::export_archive` and `refresh_checkpoint` take the lease. **Acceptance** (`ProposalAcceptor::accept`):
  `authorize(_, accept, proposal)` first, before the lease; the lease; the guard against an unfinished earlier act;
  the tip read again through `Storage::views`; the revision as it is when its base is the tip, else re-authored by
  the merge against the tip (only the pages whose text differs there are written, a rename the tip can still carry
  out stays a rename); on an overlap both texts are recorded on the proposal, which needs a revision, the proposer is
  told and nothing in prikk changes; else the acceptance record (*pending seal*, with the digest of the files) and
  the intent, both **before** `Accept::accept`, which reads them back through `StoreLedger` (nothing is kept in
  memory between the records and the signer). The signer's outcomes are read as typed: *sealed* (ids recorded,
  proposal accepted, caches forgotten, reconciliation run, lease released, then the proposer and the watchers told
  and the title index and verification asked for through `AfterSeal`), *nothing happened* (the record says *failed*
  with a fixed word, the intent is abandoned, the proposal stays open), *queued*, *worktree dirty* or *sealed but
  unconfirmed without ids* (the record stays pending, the intent open, the wiki is flagged *needs recovery*, writes
  are refused with the word, reads are served, and nothing is committed again); a seal that could not be confirmed
  but has its block is recorded as sealed and flags the wiki for a look. A retry finds the record and answers
  `AlreadySigned`; a retry behind the first waits and finds it. A maintainer accepts their own proposal only in a
  wiki with fewer than three; the accepted-bytes quota and the queue bound refuse by name. `^id`s are attached to
  commented blocks that had none in the same change (CM-5; paragraphs, tables and code blocks, each checked by
  reading the page again). A proposal created as a rollback is signed as `Kind: rollback`. **One path**:
  `SystemActs` (authorize as the system principal, lease, record, `Accept`, end, reconciliation) is how
  `import-folder` and any system change are made; `run_import` takes it, and `ImportLedger` is gone (the signer's
  guard reads the store). Tests: the lease on its own (queue order, bound, estimate, the other process, the holder
  record, export and refresh), the acceptance against an in-memory repository and a scripted signer (every outcome,
  the race, the retry, the quotas, the flag, ids, who is told), the sides of the merge, and eleven against a real
  repository (`tests/acceptance.rs`) including the reconciliation finding nothing after every kind of act.
- **Handoff 004 T4b: Accept on the web, and the creation of a wiki is a recorded act.** *The web*: `Services` gains
  `acceptor` (`Option<Arc<dyn Acceptor>>`; `ProposalAcceptor` implements the trait; `None` is a build that does not
  accept, and the page says so, as before) and `acts` (the recorded overlaps). The Accept form posts the revision
  it shows; after the checks that precede an acceptance (a form from this site, an account, a proposal the visitor
  may read, the `accept` action) the handler asks the acceptor on a blocking thread, and **every** `Accepted` and
  `AcceptFailure` is a page in the words of RFC 004 D-12 (the match is exhaustive): *signed into the history* (also
  for a retry that finds the record), *stopped: changes overlap*, *not allowed* (a proposal the acceptance cannot
  find looks exactly the same), *the wiki is busy* (with the depth, and `Retry-After` from the estimate, never a
  depth of zero), *the wiki needs recovery*, *refused: the named limit*, and for the rest a fixed word each: *not
  signed: the history refused it*, *too large to sign now*, *this revision is being signed*, *nothing to change*,
  *could not be done now*, *the result is not certain* (the records failed after the change may have been made).
  A missing or unreadable revision is a refusal in words and asks nobody. Nothing of prikk's text, a path or a cause
  is on a page; operators get it through `services.log`. A proposal that *needs a revision* shows the overlaps its
  acceptance recorded, both texts side by side in a table (the wiki's now, the proposal's), for the revision they
  were found for. 27 catalog messages (English, Japanese preview) with glossary rows; the structural harness
  draws the seven new outcome pages and the overlaps, both languages. The binary passes `acceptor: None`: sign-in is
  not served, and the server holds no write signer. **Creation**: `create_wiki` (and `create_wiki_observed`, and the
  start-up sweep) take an `ActStore`; once the wiki's record is stored (the act's table refers to it) the first
  change is kept as the sealed `Kind: system` act `SystemActs` writes (a new step, `CreationStep::ActRecorded`, after
  `Stored`), so a production reconciliation no longer reports the first block as an unrecorded system change. A
  creation killed between the record and the act is made whole by the sweep, which writes the act from the history
  before it removes the mark (and keeps the mark when it cannot); `CreatedWiki::first_change_recorded` says whether
  the act was kept. Tests: every outcome in both languages, the revision field, nothing run for a visitor who may
  not accept or a form from elsewhere, the log, the overlaps; the creation against a repository double (steps, the
  act, the kill after the record, a store that cannot take the act, the sweep's refusals), and against a real
  repository (the reconciliation of a created wiki, the kill test over the new step).
- **The write API (handoff 004 T7; RFC 004 D-13; API-03 … API-05).** `POST` under the versioned path
  `/{slug}/-/api/v1/`: `proposals` (open one: a draft, or sent for review at once), `proposals/{id}/draft` (save),
  `…/submission` (send a draft), `…/revisions`, `…/comments`, `…/decisions`, `…/acceptances`, `…/declines`,
  `…/withdrawals`, and `GET proposals/{id}` (the proposal and the base to write against). Each is registered in
  `routes.rs` with its `Action` (the one its description names, so the router and the document cannot say two things),
  and appears in the OpenAPI document, which now has request bodies, an `If-Match` header, the scope each needs
  (`x-inklkunn-scope`), a bearer security scheme and the problems each can answer; the drift tests hold the table and
  the document both ways, for writes as for reads. **Each write names its base** in `If-Match`: the proposal
  revision's `ETag` (`"<id>-r<n>"`, `-r0` for a draft; one strong tag, never `*` or a list), or for a new proposal
  the `state_id` of the view's tip (now in the read API's `tip`). No header is `428 base-required`, a tag that is not
  one is `invalid-parameter`, and a base that is not the current one is **`409 stale-base`** carrying the current
  value; nothing is recorded. **Credentials**: a request that presents `Authorization: Bearer …` is the token's
  account with the token's scope whatever cookies it carries, and a token that does not authenticate is `401
  invalid-token` (the header says it was presented), before any wiki is looked at; a request with only a session cookie
  must pass the pages' same-site check and send JSON, which a form cannot; none is needed for a token. The web layer
  still never makes a principal: `Identify::principal` is the seam, now documented to resolve the bearer token
  (the binary's implementation over `Authenticator::principal_for_token` comes with sign-in; `AnonymousOnly` stays
  until then). **Order of the checks** (each stops the request and none depends on a fact a caller must not learn):
  credential, wiki (absent and unreadable are the same `404`, byte for byte), an account (`401
  authentication-required`), the action for this principal (a token's scope intersected with the account's role, AZ-3:
  a `propose` token is `403 forbidden` at `accept` whatever its account is, before the body is read), the base, the
  body. **Bodies** are JSON objects, bounded before a byte is parsed (a declared length over the bound is refused
  unread; the bound is the form's texts, six bytes a byte, for a proposal and for a comment or a reason), read
  strictly (an unknown, repeated, missing or mistyped member is `400 invalid-body` naming it, at any depth), and the
  texts are carried as they are; the pages follow the proposal form's rules (a page over 1 MiB or a proposal over
  2 MiB before the parser, the keys the service owns, a title that is taken, a path inside `pages/`). **Refusals** are
  problem details with stable `type`s (eighteen new: `authentication-required`, `invalid-token`, `forbidden`,
  `base-required`, `stale-base`, `invalid-body`, `body-too-large`, `unsupported-media-type`, `unacceptable` with a
  `reason` word and the pages' own sentence, `not-in-this-state`, `already-decided`, `refused-limit` with the limit's
  name, `allowed`/`current`/`asked` and `reset`, `busy`, `needs-recovery`, `overlap`, `nothing-to-change`,
  `acceptance-in-progress`, `acceptance-failed`), each with a catalog entry and a glossary row; a quota that waiting
  lifts carries **`Retry-After`** (a rate: the seconds to its reset; a full queue; a busy or recovering wiki), one that
  waiting cannot lift does not. Problem bodies now write `<`, `>` and `&` as escapes like every answer of the API.
  `Services` gains `acceptor: Option<Arc<ProposalAcceptor>>`. Tests: the document names every write (and drift both
  ways, and a write guarded by another action is drift), every endpoint is in the route registry and the
  enumeration harness walks the writes, the stale base (new proposal, revision, decision, draft), a token with
  `propose` cannot accept, `Retry-After` on a refused rate, a private wiki's writes answer `404` as an absent one
  for four kinds of caller, multibyte JSON round trips, hostile content is data, the bounds, the strict body, the
  same-site check, and an acceptance through a real `ProposalAcceptor` over a scripted signer.

- **Proposals T5 (RFC 004 D-7; handoff 004): recovery and the kill test.** *Two reads in the driver and the signer.*
  `Storage::status` (the read driver's ninth verb: a flat cost rule, measured at 2 MiB on 3,000 pages; `status-report-v1`
  pinned; the queue's patches with their messages and operations, the torn bytes at the end of the log, the interrupted
  states; a report of another schema is refused, one of another shape or that disagrees with itself is `Damaged`; the
  default method fails closed, so a storage that cannot report is never taken for one that found nothing). `doctor` and
  the lock listing have no JSON form, and the boundary gate keeps `doctor` and `unlock` out of the read driver, so they are
  `Witnesses` in the signer (keyless, read-only, cost rules for `doctor` as `verify`'s, `unlock` flat, `cat` as the read
  driver's) with strict readers that refuse a shape they do not know; unreached-object warnings are a note, never damage
  (ledger PL-12). *The signer's recovery operations* (`Recover`: `seal` alone on the one queued patch, `doctor
  --discard-damaged-commits`, `unlock --lock <path> --yes` for the five lock kinds an acceptance leaves, and putting a
  worktree back to the tip) are bound as an acceptance is: each is given a token naming a **recorded recovery act**, read
  back through the ledger, and does nothing unless the act is the wiki's, still open, and names this very operation;
  sealing is also bound to the pending acceptance or system act whose change is queued (its message, its base, and its files
  by the digest, with the renames' order proven, because prikk lists them sorted); and none runs unless the wiki's write
  lease is lent to the calling thread. **A lock's liveness is decided by the lease record, never by prikk's process check**
  (ledger PL-8): the listing's `liveness` line is read, kept in the plan and used for nothing. *The procedure*
  (`Recovery::recover`, at start and on demand, `RecoveryMode::PlanOnly | Run`): under the lease, the witnesses are read
  (the flag, the open intents, acceptances with no end with or without an intent, system acts with no end, the record of a
  holder that died, `status`, `doctor`, the locks, the history, the worktree, and processes of somebody else's that may be
  working in the repository), a pure function makes the plan (`recovery::build`: steps in order, or a *stop* with a fixed
  word), the plan is recorded as a recovery act before any step and every step is an audit record before it is taken. A
  queued patch that is the change of the unfinished act is sealed alone and its records ended; a commit prikk calls torn
  is discarded and its act ended as failed (`interrupted`) with its intent `discarded`; stale locks are cleared first;
  a worktree that is not the baseline is put back; then `verify`, the reconciliation, and the flag is cleared. **A sound
  queued patch that matches no recorded act is a stop, not a discard**: prikk 0.55.0 has no verb that removes one
  (`--discard-damaged-commits` answers an acknowledged commit the log no longer holds soundly and refuses over anything
  else), and sealing it would put an unaccepted change in the history. *The surfaces:* `inklkunn wiki recover <wiki>
  [--plan]` (the plan alone changes nothing and writes nothing; a stop is exit 1) and a recovery of every wiki that needs
  it in the start-up work, before anything is served; a wiki that stays flagged serves reads and refuses writes with the
  word. *API and records:* store **schema 12** (`recovery_acts` and their ends, written once, with the triggers of the
  acceptance records; a version 11 database migrates with its data) and `AcceptanceStore::{pending_acceptances,
  acceptance_entry}`; `ProposalAcceptor::with_observer` and `AcceptStep` (the nine steps of D-4, observed and nothing
  more; nothing in production sets one); `words::INTERRUPTED`; `WikiLeases::holder_name`. **The kill test**
  (`inklkunn-signer/tests/acceptance_kill.rs`: the one crate besides the driver that may spawn a process, with a
  dev-dependency on core): the acceptance runs in a child process that is `SIGKILL`ed at each of D-4's steps and at
  each prikk call of the signer's write sequence, held by a stand-in `prikk`; after each of fourteen kills a fresh
  recovery leaves `verify` green, exactly the expected blocks in `log`, the acceptance record and the intent
  consistent with them, the plan recorded and ended, the reconciliation clean, and the wiki taking writes (a retry of a
  sealed acceptance seals nothing twice). Also tested against a real repository: a planted queued patch that no intent
  describes (a stop, the repository untouched), the same torn (discarded and reported), a planted lock whose recorded
  pid this namespace shows as alive (cleared all the same), a lock of another kind (a stop), recovery refused without the
  lease, a step that fails (the flag kept, the plan ended as stopped), an interrupted recovery, a doctor report of an
  unknown shape.

Handoff 004 T8 (RFC 004: proposals).

- **The participation metrics are exposed.** `inklkunn wiki metrics <wiki> [--days N | --from T --to T]` prints one JSON Lines record
  on standard output (`Metrics::json_line`: the wiki's id, the period, the proposals by state, the medians, the acceptance and
  decline shares as `{num, den}`, the proposers and the reviewers per proposal; an absent figure is `null`), after an audit event
  begins the act; `operator_metrics` is the command line's computation (no principal; the same figures as the service's).
- **Threat model v0.2.** The write path (the lease, the intent journal, the overlap rule, recovery and its recorded acts: C-1g – C-1k),
  the write API's scope ∩ role rule, the proposal quotas and the verification claim (C-3b, C-9d), RR-9 – RR-11, and the
  verification record §7.2 (what carries each item and what does not).
- **Two retired words.** *conflict* (コンフリクト) and *merge* (マージ) join the retired-words table; the one message that said
  "Nothing was merged" says "Nothing was changed".

Spike C, the measurement tool (RFC 005 part 1; no product crate changes).

- **`inklkunn-corpus claims`** (`tools/corpus`, see `CLAIMS.md`): `plan` pins the newest N revision ids of listed public-wiki
  articles (ids and parent ids only, through `curl`, one request a second, no editor identity requested), `get` fetches exactly those
  ids into a cache outside any repository and counts the ones deleted upstream, `convert` runs wiki-commons' converter as a
  subprocess, `replay` walks each history through `inklkunn_markup::blocks` and `inklkunn_core::align_blocks` and writes one event
  per edited block (metrics, never text), `pool` draws the seeded, language-balanced, per-article-equal items the two labellers see,
  and `measure` computes M-1 … M-6 (and M-2b, Cohen's kappa) from their labels. `inklkunn-markup` is a workspace edge of the tool.

Knowledge layer K-T1 – K-T4 (RFC 005 part 2, D-1 … D-6, on part 1's decisions D-1 … D-4; `inklkunn-core::knowledge`; no new crate, no new dependency).

- **The two files and their reader (K-T1).** `knowledge/claims/<id>.yaml` and `knowledge/sources/<id>.yaml` as values (`Claim`,
  `Source`), read in the frontmatter's plain YAML subset on `inklkunn_markup::frontmatter` (no anchor, no tag, no repeated key,
  flow mappings included) and written back in one canonical form (`to_yaml`; the lists in one order). The enumerations are
  closed (`bearing` is `direct | partial | indirect`), a key the format does not have is refused, a number is refused wherever
  it stands (dates and years are text), and nothing is read from a file that is not wholly well formed. `judged_on` is the SHA-256
  of an expression's block text as `inklkunn_markup::blocks` trims it, by one function (`block_digest`), tested on English and
  Japanese against digests computed outside the product.
- **Validation at acceptance (K-T2).** Before the signer is asked, a change that touches `knowledge/` (or lands on a tip that has
  knowledge files) is judged on the files as it would leave them: the schema and ids, every source and claim a file names present
  and readable in the same view, every anchor naming a page and a block the result has once, no key only the service writes
  (`kind`, `visibility`, `system`) changed beside it, a removed file not still named, and a claim left pointing at a block the change
  took out. The refusal is typed (`AcceptFailure::KnowledgeInvalid`, the word `knowledge-invalid`), names each file (escaped as the
  import escapes a name) and the rule, never what the file said; no acceptance is recorded. A wiki with no knowledge layer (the `knowledge/.keep` every new wiki has is no knowledge file) pays one
  `tree` call; otherwise the tip is extracted once. Files that were already invalid, and not touched, do not hold up other changes.
  The accept page has its own outcome (*Not signed: a claim or source file is not acceptable*), the write API its word.
- **A seventh health list, *Knowledge files that cannot be read*.** `-/health/knowledge-files`, the API's `health?list=knowledge-files`
  (rows: the file's repository path as `title`, `rule` as a code, `action` `fix-or-remove-file`), catalog entries in English and
  the Japanese preview with their glossary rows, the structural scenarios (listed and empty) and the OpenAPI shapes. A repository
  that already holds invalid files serves its pages all the same, and no claim is read from them.
- **The knowledge files in the merge (K-T3).** RFC 004's merge gains a third file family: `expressions` matched by anchor,
  `relations` by `(type, claim)`, `sources` by source id, a source link merged by its keys, the standing one value; a value changed
  on both sides is an overlap with both texts (`Place::Knowledge`), an entry one side removed and the other changed is one, and
  nothing is lost. A file one side left alone is taken as written. The corpus: 324 hand pairs (306 clean, 18 overlaps) and 216
  pairs derived from spike C's replay facts (209 clean, 7 overlaps), the counts held by a test, a lost-nothing rule and an oracle
  checked on every pair (`tests/fixtures/KNOWLEDGE-MERGE-CORPUS.md`).
- **The derived knowledge index (K-T4), store schema version 13.** Claims, sources, relations, expressions with what each block
  is now, and per link (and per standing) whether it was judged on the wording the claim has now (`current` or `earlier`), built
  in the same pass over the same `extract` as the title index (one extract serves both), stored in seven derived tables (migrated
  from 12, dropped and rebuilt with the rest of the derived store, a count that says an index is whole), bounded by the walk's
  entry bound and the file size, and asked for by `ReadService::knowledge_index`. A tip whose knowledge directory is over the
  bound still serves its pages.
Corrections from the review of 2026-10-31 (handoff 004, P4, P5, P8, P9, P11, P12; handoff 003 T1, T2; spike K1).

- **P4 — every path is checked in the proposal service.** Every file path and both ends of every rename must be a `.md`
  file inside `pages/` (`Invalid::PathOutsidePages`); a path that is not in the repository at the proposal's base is a new
  page and must be exactly what its title gives, `title_to_path(path_to_title(path))` (`PathNotCanonical`); a new path may
  not collide with a page that stays, or with another new path, by `collision_key`, and a rename may not land on a page that
  stays (`PathCollides`). The rules hold for a draft, a submission, a revision and a rollback plan alike, and the base is read
  through `BaseSource::page_paths` (one `tree`). The form and the API keep their checks as the first line; the API's reasons
  are `path-outside-pages`, `path-not-canonical`, `path-collides`, and the catalog gains `invalid.path_collides`.
- **P5 — the owned-keys rule lives in the service.** `kind`, `visibility` and `system` are checked in `ProposalService`
  (`proposal::files`) on every edit, removal, rename and rollback plan: removing or renaming a page that carries one counts as
  changing it (`ProposalError::OwnedKeys`, API reason `front-owned`), and information at the top of a page that cannot be read
  is refused (`Invalid::FrontUnreadable`). Owned keys stay administrators' alone, **except `kind: redirect`** (ruling D8): a
  page may become a redirect, or stop being one, for anyone who may propose. The form and the API use the service's rule
  (`changed_owned_keys`, `front_keys`) for their first-line check, so there is one rule.
- **P8 — the creation act and the wiki record are one transaction.** `ActStore::insert_wiki_with_creation_act` stores the
  wiki's record and its sealed creation act together, on both stores (one SQLite transaction); a kill leaves both or neither.
  `CreationStep::ActRecorded` and `CreatedWiki::first_change_recorded` retire (the creation is complete at `Stored`), and the
  sweep's repair of a missing act stays as the second line, for a wiki an earlier build stored in two steps.
- **P8 (D9) — a proposer's own approve or request for changes is refused by name.** `ProposalError::OwnProposal`, the API's
  `own-proposal` problem type (403), and the page's words (`refused.own_proposal`; `problem.own_proposal.*`); a comment on
  one's own proposal stays.
- **P8 (D3) — the operator's way out of a stop on a queued patch that matches no intent** is `docs/RECOVERY.md`: how to look
  (read-only commands), what to keep, why recovery will not seal or discard it, when prikk 0.57.0 lets the operator discard it
  (PK-43), a new wiki from an export if writes cannot wait, and the actions that must not be taken.
- **P9 — maintenance goes through the wiki leases.** The command line's checkpoint refresh and archive export take the wiki's
  lease (the holder record, the queue) through `WikiLeases::refresh_checkpoint` and `export_archive`, and wait their turn behind
  an acceptance; the start-up refresh leaves a wiki alone that still needs recovery and says so.
- **P11 — abuse bounds.** A per-principal comment quota (`comments_per_hour_per_principal`, 60, half for a new account;
  limit `comments-per-hour-per-principal`); at most 10 mention events (`MAX_MENTION_EVENTS_PER_HOUR`) one actor may send one
  recipient in an hour, the rest dropped and counted (`Notifier::suppressed`); the per-wiki hourly proposals quota counts
  proposals **sent for review** (checked at the submit, not when a draft is opened), is not bound by and does not count
  reviewers and above, and its refusal shows the limit and not the count. The write forms take the raw body (`urlencoded::Raw`)
  and parse it after the checks (the site, the wiki, the account), no longer through axum's `Form` extractor; a parity test holds
  the parser to that extractor.
- **P12 — `PRAGMA synchronous = FULL`** is set explicitly on every SQLite connection the store opens, with a test.
- **Handoff 003 T1 — one RFC 3339 function.** The web crate's `time::rfc3339` is `inklkunn_store::record::rfc3339`, and the
  binary's `keys.rs` calls it; their tests stay.
- **Handoff 003 T2 — an export refused by a file system that cannot move without replacing says so.** `renameat2` answering
  `EINVAL` or `ENOTSUP` now reads "this file system cannot place the bundle without the risk of replacing an existing one".
- **Spike K1 — `align_blocks` is tool-only.** `inklkunn_core::align_blocks` is behind the non-default `measurement-alignment`
  feature, which only `tools/corpus` enables; the boundary gate lists it as a tool-only feature and refuses it in a product
  crate's dependencies.
Serving sign-in (the sign-in and acceptance handoff U1, U2, U4; handoff 004's P6).

- **U4: a real `Identify`, and the identity authority claimed once.** `inklkunn_web::AuthIdentity` replaces `AnonymousOnly` in
  the binary. A **session cookie** resolves through `Authenticator::principal_for_session` for a read and **only** through
  `principal_for_write` (the cookie and the session's CSRF token, checked together) for a write; a **bearer token** resolves
  through `principal_for_token` (a hashed lookup; scope intersected with the account's role); and **a request that presents a
  bearer token is judged by the token alone**: a token that is unknown, expired, revoked, of a disabled account or malformed
  is *invalid token* (a write) or an anonymous visitor (a read), never a fall back to a cookie the request also carries,
  and a valid one is not widened or narrowed by one. Another scheme in `Authorization` (a proxy's Basic) is not a token of
  ours and does not stop a cookie. `Identify` gains `session` (the live session for drawing a page; none for a token's
  request), `may_be_signed_in` (a cheap no-lookup answer) and `for_write` (`WriteIdentity`: a principal, *CSRF mismatch*, or
  *invalid token*). `serve` claims the `IdentityAuthority` once (`cli.rs`) and the authenticator keeps it (C7); the
  authenticator is built over the store with the declared trusted proxies (`INKLKUNN_TRUSTED_PROXIES`, which now also decide
  which address the sign-in throttle counts) and the audit sink. Tested for: cookie only, token only, both (the token wins),
  an invalid token with a valid cookie (refused; and not "forbidden" or "authentication required"), an expired session, a
  revoked token, an expired token, a disabled account, and a Basic header beside a cookie.
- **P6: one CSRF mechanism for every write a signed-in person can make.** A layer in front of **every route** of the
  application origin (`crate::csrf`; the only place it is decided, so a new route is covered the day it is registered) asks a
  request that changes something and carries a live session for the session's synchronizer token: the form's hidden field
  `csrf`, or the header `X-CSRF-Token` for a client that is not a form. It is checked **before any handler runs, so before the
  wiki is looked up**: the refusal (403, *the form has expired*; for the API a `forbidden` problem) is byte-for-byte the same for a
  wiki that is there, one that is private and one that is not there. The token of another session, an empty one, a prefix of the
  right one and none are all refused. The same-site check stays in every handler as the second wall. The posts that need no
  session token are listed in one place and each has its reason: the language form (a cookie of the visitor's own), and the
  three pages that make a session (sign-in, joining, an invitation), which carry a double-submit token in a `__Host-` cookie.
  The body is read in the layer only for a request with a **live** session (a stranger's flood costs a lookup, not a buffer).
  Every form the pages draw (proposals, comments, decisions, accept, decline, ask, withdraw, the inbox, the check-now
  verification, following, and the account page) includes `templates/csrf.html`; a structural rule over every page state
  (`form-token`, with planted failures) finds a form that does not. The write API asks `for_write` too. A test walks the route
  registry and posts to every write route with a live cookie and no token.
- **U1: the sign-in pages.** `/-/sign-in` (a sign-in name and a password; constant-work and throttled as built; the client
  address is the peer's, or the one a *declared* trusted proxy forwards, so a forged `X-Forwarded-For` buys nothing; the
  refusal is one sentence for a wrong password, an unknown name, a disabled account and a name nobody could have, and the
  pages are the same but for the typed name and the fresh token), `/-/sign-out` (a button; a form with the session's token),
  `/-/account` (the display name; the account's live sessions, newest first, with *End this session* each, the one in use marked;
  API tokens made with a scope and an expiry of 1, 7, 30, 90 or 365 days, **shown once** on a page that is never cached, listed
  and revoked), no script anywhere. The session cookie is `__Host-inklkunn-session`, `Secure`, `HttpOnly`, `SameSite=Strict`,
  no `Domain`. The banner names who is signed in and offers *Sign out*; a signed-out visitor is offered *Sign in* only where
  sign-in is served. A token alone has no account page and cannot make another token. The authenticator gains `sessions`,
  `end_session_of` (by a handle that is a piece of the stored hash, never the cookie) and `set_display_name`; the store gains
  `SessionStore::sessions_of` and `AccountStore::set_display_name`. 93 catalog messages in English and a Japanese preview
  (the glossary's rows in the catalog's order, five concept rows: account, sign in, session, invitation, API token); the
  structural rules and the retired-words test hold every new state in both languages.
- **U2: how accounts come into being (DR-11).** Store schema **14**: `invitations` and `invitation_uses`, **written once in
  the database itself** (triggers abort an `UPDATE`, a `DELETE` and an `INSERT` over a key already taken, counting every
  unique key), the link's secret kept only as its SHA-256, a wiki and a role together or neither, an end after the beginning,
  and one account from at most one invitation. Redeeming is **one store step** that checks the invitation, adds the account,
  grants the role it named (granted by no account: the operator did) and records the use, all or none: a taken name or a short
  password spends nothing, and eight threads opening one link make one account on either store. (Version 13 is the
  knowledge tables'; `MIGRATIONS` skips it here, and the migration-chain tests allow that one gap and no longer pin the schema
  number.) `inklkunn account invite [--role <role> --wiki <wiki>] [--days <1-30>]` makes a one-time link, seven days by
  default, and shows it **once** on standard output (under `INKLKUNN_APP_ORIGIN` when it is set; never in the audit trail or
  the error stream); `/-/invite/<secret>` is where the person who opens it chooses a sign-in name, a display name and a
  password (the form checks the invitation before it hashes anything, so a stranger's bad link costs a lookup), and is the
  same page for a link that was used, one that ended and one that never was. No password is ever sent or printed. The first
  administrator is made this way (`--role admin --wiki <wiki>`). `INKLKUNN_JOIN = off | provider | anyone`, **default
  `off`**: a first sign-in through the configured provider makes an account (for an organization's own provider; a public
  provider makes this open registration), and `anyone` also lets a person make a local account with a password at `/-/join`,
  throttled per client address; a joined account has no role beyond what the wiki gives everyone and the new-account quotas.
  Accounts are never matched or linked by e-mail address. The sign-in page shows only what exists: *Join* only where joining
  is open, otherwise the one sentence *Ask the person who runs this wiki for an invitation.* The seam for a provider is the
  next task's: an invitation names no credential. `account disable | enable <login or id>`, `account grant <login or id>
  <role> <wiki>` and `account revoke-grant <login or id> <wiki>`: **every act is audited before it is done** (an act the trail
  cannot take is not begun) and again when it ends, principal `system:operator-cli`; the store must exist (a typo in the data
  directory never makes an empty one).
Corrections from the review of 2026-10-31 (handoff 004, P1 – P3, P7, P8, P10).

- **P1 — an ambiguous `commit` is queued, never undone.** After a `commit` error the signer reads `status` before it touches the
  worktree (`Signer::after_commit_error`): a patch that is queued leaves the files exactly as they are and answers
  `AcceptError::Queued { step: "commit" }` (recovery seals it alone); nothing queued and nothing torn puts the worktree back, as
  before; a torn log, or a `status` that cannot be read, undoes nothing and answers `WorktreeDirty`. Before this, a `commit`
  stopped at its deadline (or over its output bound, or by its ceiling) *after* its patch was durable had its files put back,
  and the queued patch then matched no worktree: `seal-queued` and `restore-worktree` both refused and the wiki was flagged for
  good. Tests: the signer's `a_commit_that_fails_after_its_patch_is_durable_is_queued_and_its_files_are_left_never_undone` and
  `a_commit_error_with_nothing_queued_is_undone_and_one_with_an_unreadable_state_is_not`; core's
  `an_ambiguous_commit_is_queued_never_undone_and_recovery_seals_it` (a stand-in that runs the real `commit` and then fails);
  the kill matrix's `prikk commit #0 done` rows (the real `commit` ran to completion and its caller was never answered).
- **P2 — a retired key never freezes the wiki (DR-10).** What the check reads, in prikk 0.55.0's own output (measured; ledger
  PK-34): after `trust maintainer remove`, `prikk verify --format json` has `verdict.ok` false and **one** failed condition,
  `{"id": "publication-trust"}`, with every stage `evaluated`; `prikk verify` (text) prints `publication trust issues: N` and N
  lines of exactly `publication-trust [PRIKK-TRUST-PUBLICATION-UNTRUSTED]: <block|ref-state|ref-update> <64 hex> has no trusted
  MAINTAINER signature`, one for each block, ref state and ref update the removed key signed; `doctor` prints the same N as
  errors with the code `PRIKK-TRUST-PUBLICATION-UNTRUSTED`. **"Exactly and only by publication trust" is: the report is complete
  (no partial stage); its failed conditions are that one condition and no other; every `publication-trust` line is of that one
  shape and they are as many as prikk counts; every named block is in the history up to a tip the key record kept for a
  *retired* key when the rotation replaced it (handoff 002 C11), so a block newer than that point, or a third key's, is not
  excused; a ref state that follows a block follows an excused block; a ref state that follows no block (a branch's publication)
  is excused up to one per view, and ref updates (which prikk's reports give no link to a block) up to the number of ref states.**
  A retirement with no recorded point excuses nothing. Then an acceptance whose `verify` fails that way is **confirmed**
  (`Ok(SealedChange)`, not `SealedUnconfirmed { VerifyFailed }`), recovery's plan takes `doctor`'s publication-trust errors for a
  note instead of `DoctorTrouble` and its `verify` step passes with that note (`Recover::verify_after_retirement`, new, read-only,
  default "no"); the badge still says *Verification failed*, as DR-10 ruled. Any other failed condition, an unreadable line, or
  an untrusted block outside the retired key's point still stops. Tests: `signer/retirement/tests.rs` (every row of the
  judgement, no repository); the signer's `after_a_retirement_writes_continue_and_the_repository_still_fails_its_verification`
  (three acceptances after a retirement; prikk's own verify still fails; the canary for PK-34) and
  `a_second_failure_planted_after_a_retirement_still_stops_the_wiki` (a real block sealed by a stranger's key that is then
  removed from trust); core's `a_retirement_never_freezes_the_wiki_and_recovery_passes_verify_with_a_note` and
  `a_failure_the_signer_does_not_excuse_keeps_the_flag_even_after_a_retirement`. `docs/KEYS.md` says so.
- **P3 — recovery keeps the flag on disagreement.** The reconciliation step judges its findings against the act being finished:
  a finding that names the act's record, or a block of the act's view newer than the block the act was made on, **stops the
  recovery** (`Stopped { at: reconcile }`, the flag kept, the findings named by their audit words); a finding about an older
  block is reported (audit trail, `RecoveryReport::reconciliation`, the step's detail) and does not block; a base that cannot be
  found in the part of the history read stops it too. `Finding` gains `block_id`, `record_id` and `describe`. Tests (core's
  `tests/recovery.rs`): `a_disagreement_that_touches_what_the_act_did_stops_the_recovery_with_the_flag_kept` and
  `a_disagreement_older_than_the_act_is_reported_and_does_not_block_the_recovery`.
- **P7 — the inline reconciliation is bounded.** After a seal, the acceptance and the system act reconcile only the blocks of the
  act's view since the block the intent was made on (`reconcile::reconcile_since`, `ReconcileError::BaseNotFound`): the history is
  read from the tip as far as the base, the first read small and each four times larger; a record whose block was not read is
  not a missing block here. The records are still read in full (a store query; no prikk call). The full pass stays in recovery
  and on its schedule. Tests: core's `tests/reconcile.rs` (a bound too small for the whole history refuses the full pass and
  not the bounded one; a lost record for a block read is found, one far below the base is not this pass's to find; a base the
  view does not hold is an error) and `tests/recovery.rs` (`the_inline_reconciliation_reads_the_blocks_since_the_base_and_the_full_pass_still_finds_the_rest`).
- **P8 — recovery and acceptance edges (the parts the review listed for this change).**
  - *An unclean precondition is its own error and flags the wiki.* `AcceptError::Unclean { queued, detail }` (a patch is queued, or
    the worktree has changes, when the act begins and no record explains it) is settled as *incomplete*: the wiki is flagged
    `queued` or `worktree-dirty` and recovery looks first. It was `Refused`, which flagged nothing.
  - *The foreign-process witness stops every step.* A `prikk` or `bwrap` of somebody else's in the repository is the plan's first
    stop (`foreign-process`), before any seal, restore, discard or lock is attempted. Note for operators and tests: a process of
    another user that is a `prikk` or a `bwrap` and whose working directory cannot be read counts as foreign, as it did for
    locks, and now holds every recovery.
  - *A lease holder is named by pid and start time and an instance id.* `WikiLeases::new` records `<name>@<start time>-<random>`
    (`lease::instance_name`; `/proc/self/stat` field 22, and 64 random bits from the standard library's seeded hasher), so the
    record an earlier process left under the same pid is another holder's and a holder that died, not this process's own.
    `recovery_guard` also sees pending acceptances, whether or not their intent was written.
  - *`is_lent` also requires the lease to be held.* Every in-process turn has a generation; a loan (`LeaseScope`) records the
    generation of the lease it was made under and counts only while that turn is the one held (a leaked scope no longer lets an
    act on that thread believe a lease it does not hold, after the lease or after another lease of the same wiki).
  - *The `AlreadySigned` path settles the proposal.* A retry that finds a sealed acceptance of a proposal still *held for review*
    marks it accepted (by the acceptor of the sealed acceptance) before it answers.
  Tests: signer `a_loan_is_good_only_while_the_lease_it_was_made_under_is_the_one_held`; core `lease/tests.rs`
  (`a_holder_is_named_by_its_pid_and_start_time_and_an_instance_id_…`), `recovery/plan/tests.rs`
  (`a_foreign_process_stops_the_seal_the_restore_and_the_tail_too`) and `tests/recovery.rs`
  (`an_unclean_repository_at_the_start_of_an_acceptance_flags_the_wiki_and_names_what_it_found`,
  `a_foreign_process_in_the_repository_stops_every_step_of_a_recovery_not_only_a_lock`,
  `an_acceptance_with_no_end_blocks_the_next_act_even_when_its_intent_was_never_written`,
  `a_retry_of_a_sealed_acceptance_settles_a_proposal_still_held_for_review`).
- **P10 — the kill matrix reaches inside prikk.** `tests/acceptance_kill.rs` now kills the child's whole **process group** (a shell
  already waiting on its input sends the signal, so nothing the child had begun goes on to write) at 33 points (14 before; 19 new): the acceptance's
  steps and prikk calls as before; **`done` rows** where the stand-in runs the real `commit` / `seal` to completion and then
  holds (the effect durable, the caller never answered: deterministic); **`inside` rows** where the stand-in starts the real verb,
  waits half a millisecond (`INKLKUNN_AK_INSIDE` changes it) and the parent kills the whole group at once (the state is whatever
  the verb had got to; the plan is checked against what the kill left, and every other invariant is asserted as in the other
  rows); **the system act's rows** (`SystemActs::with_observer`, `SystemStep`, and the same prikk calls); and **a kill of recovery
  itself** after its `seal-queued` step sealed (the real seal ran, none of it recorded, a recovery act begun and never ended): the
  next recovery ends the interrupted act as stopped and finishes the records. What the rows that kill inside a verb found (100
  runs of the matrix: 0 failures; the classes below are asserted, not avoided): most kills leave a queued patch or a sealed
  block, and recovery finishes them as in any other row; about one `seal` kill in ten lands before the seal has finished; and three
  states recovery has no answer for, each of which it meets by **failing closed** (nothing changed, the wiki stays flagged, the
  stop or the error named) rather than guessing:
  (1) a kill at the instant prikk creates a lock file leaves it **empty** (`prikk unlock` lists `kind:` empty and `recorded pid:
  (unparseable)`), which the strict reader of `unlock` refuses (a witness error) until an operator clears the file; the rows
  assert the refusal, clear the empty lock as an operator would, and go on;
  (2) a `seal` killed just after it published the block and drained the queue leaves only a `doctor` *warning* about stale
  reference metadata ("no repair is required") beside a stale `active` lock: the plan stops at `doctor-trouble`, because
  `Diagnostic::is_metadata_debris` rides along only with an acknowledged damage;
  (3) a `seal` killed after it published the block and before it drained the queue leaves `prikk status --format json`
  refusing with an integrity error (the queued patch replayed on a tip that already has it) while `doctor` says
  `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING` and recommends *run `prikk seal --allow-no-audit` for the affected ref*: recovery
  cannot read `status` and stops; a second `seal` is prikk's own answer. These three are decision requests in the handoff
  report; `INKLKUNN_AK_KEEP=<dir>` keeps a copy of the repository of a row that stopped or failed closed, for study.
Serving sign-in and accepting, U5: the server's acceptor (decision D1 of the review of handoff 004).

- **`inklkunn serve` opens the signer and builds the acceptor.** After the start-up work (the sweep of unfinished creations,
  recovery, the checkpoint refresh) the server opens the key store as the command line does (`INKLKUNN_KEY_DIR`, the secret from
  `INKLKUNN_SECRET_FILE`, the seeds decrypted per call onto the tmpfs; bubblewrap required as for every prikk call) and builds a
  `ProposalAcceptor` with `Signer::with_leases` over the **same** `WriteLeases` that its `WikiLeases` wrap, `StoreLedger` as the
  ledger, the notifier, the audit trail and the reconciliation, so the command line and the server exclude each other (each is told
  *busy*, naming the process, when the other holds the wiki). `Services.acceptor` is set. The banner says
  *accepting proposals: the signer is open …*. The serving process now holds the key store (threat model RR-1, RR-11, SECURITY.md).
- **After a seal, the title index is warmed and the wiki verified off the request path.** `AfterSeal` is implemented by one worker thread
  with a bounded queue (16 jobs, a repeated job for the same wiki collapses, a full queue drops the ask, because the next acceptance
  asks again): a flood of acceptances makes no thread and no memory.
- **A server started without the means to sign still serves.** With no secret file, or a key store that does not open (a wrong secret,
  a key directory open to others, no keys made yet, a seed directory off the tmpfs) the server prints **one line** at start naming the
  setting and the cause, serves every read, and refuses every write with one plain sentence (`accept.unconfigured`, with a heading and a
  note beside the button; the API's `signing-not-configured` problem, 503, distinct from `not-available`). It never asks for the secret
  on a terminal and does not retry on its own: fix the cause and restart. `inklkunn_web::NoSigner` is that acceptor; `Acceptor` gains
  `is_configured` (default `true`); `AcceptOutcome::SigningNotConfigured` is the page's outcome; `inklkunn_web::HeaderMap` is re-exported
  so that the binary can implement `Identify`.
- **Tests (real prikk, a real key store and a real socket)**: a start without the secret file serves a page and refuses a write with the
  sentence; a start with the secret accepts a proposal end to end (the history has the block, the page says *Signed into the history*,
  a retry writes nothing, the verification after the seal is recorded, no seed directory is left) and a search of the operator's lines,
  the log and every answer for the secret's value finds nothing; a wrong secret is the same unconfigured server; a wiki the command line
  holds makes the server's acceptance busy and it accepts when released; a wiki the server is signing is refused to the command line by
  name; a wiki flagged *needs recovery* refuses the accept with the signer open, and a flagged wiki is recovered before the acceptor
  exists. SECURITY.md says where the secret file lives and what the signer checks of it, the key directory and the seed directory.

Serving sign-in, U3 (handoff `serve-sign-in-and-accept-handoff-v1`) with handoff 002's follow-ups F2, F3 and F4: **OpenID Connect switched on**.

- **U3 — the provider's sign-in, from the settings.** `INKLKUNN_OIDC_ISSUER` (an `https` address, no query or fragment) and
  `INKLKUNN_OIDC_CLIENT_ID` switch it on, together or not at all (half a provider is refused naming the missing setting);
  `INKLKUNN_OIDC_CLIENT_SECRET_FILE` names the file the client secret is read from once at start (absolute; a link is followed;
  at most 4 KiB, one line, trailing line breaks dropped, never printed; none means a public client with the PKCE proof alone);
  `INKLKUNN_OIDC_SCOPES` adds scopes to the `openid` that is always asked (default: nothing more, only `iss` and `sub` are ever
  read). The start checks the secret file and probes the vector (`curl`, the CA bundle, `prlimit`, bubblewrap) **before anything
  is opened**, and says at every start that the provider is on and which redirect address to register
  (`<app origin>/-/sign-in/oidc/callback`). Routes: `GET /-/sign-in/oidc` (start; `?next=` is a local path), `GET
  /-/sign-in/oidc/callback`, `GET /-/invite/{secret}/oidc` (an invitation's start). They are **links, not forms**: browsers
  apply the page policy's `form-action 'self'` to the redirects that follow a form submission, so a form that ends at the
  provider would be refused. Without a provider all three are the generic not-found page and no page offers the button.
- **A login in flight remembers what it is for, on the server.** `Authenticator::begin_oidc(OidcPurpose, client, now)` takes
  `SignIn { next, after_joining }` or `Invitation { secret, next }` and keeps it with the state (ten minutes, one-shot): the
  invitation's secret and the landing path are in no cookie, no callback address and no provider log, and `OidcPurpose`'s
  `Debug` prints neither. `finish_oidc(state, cookies, code, client, now)` decides by the purpose: a linked identity signs in (a
  disabled account does not); **an identity nobody linked makes an account only where `INKLKUNN_JOIN` is `provider` or `anyone`**
  (counted per client address with the join throttle, no password, a placeholder name `member-xxxxxx` to change on the account
  page, no role in any wiki, the new-account quotas from that moment) and otherwise is `OidcSignInError::NotLinked`, which
  the page says in a sentence of its own (*ask for an invitation*); never matched or linked by e-mail address (the provider's
  claims other than `iss` and `sub` are not read, not even a name). An **invitation may be redeemed with the provider instead
  of a password**: the person who opens the link chooses (the invitation page offers the button only when a provider is
  configured), the account is made with the identity and the role the invitation named and the link is spent in one store
  step; a provider identity that already has an account spends nothing (`IdentityTaken`, *sign in with it instead*); a link
  used meanwhile is the one *unusable* page. `inklkunn-store`: `AccountStore::add_account_with_identity` and
  `InvitationStore::redeem_invitation_with_identity` (memory and SQLite, one transaction each; **no schema change**).
- **`inklkunn account link <login | account id> <issuer> <subject>`** is the administrative link: audited before it is done
  (`account-link`, principal `system:operator-cli`, the issuer and subject in the detail) and again when it ends; an identity
  another account holds is refused (*one person has one account*); the usage and the all-commands trail test include it.
- **C9 — the login-state cookie and the session cookie are checked in a real browser (F2).** The state cookie is `__Host-`,
  `Secure`, `HttpOnly`, **`SameSite=Lax`**, ten minutes, removed by the answer that uses it (and by every refusal); the
  session cookie stays `SameSite=Strict`. `rfcs/handoffs/002-identity-keys-and-signing/evidence/c9-browser-run.py` drives a
  headless Chromium 156 over the DevTools protocol (a WebSocket client and a request matcher in the standard library; no new
  crate) against the real router on a real port and a stand-in provider on `https://idp.example` (a different site); Chromium
  reports which cookies each request sent or had blocked. 33 checks pass (`c9-browser-run.out.txt`): the Lax cookie is carried
  back by the provider's cross-site redirect, the callback is a 200 page, the browser's own follow-up request carries the Strict
  session, a cross-site navigation into the wiki is sent without it (so a redirect would have landed the person signed out), a
  callback this browser did not start and a replayed one sign nobody in. The same script against a build with the state
  cookie `Strict` fails (`...mutated-strict-state-cookie.out.txt`, 6 checks). The run needs `cargo`, `openssl` and `chromium`; its
  server is the ignored test `browser::the_server_the_real_browser_run_drives` of the `signin` tests, which normal runs skip.
- **F3 — `prlimit` on the `curl` vector.** `prlimit --data=<N> --` goes in front of `bwrap` (and of `curl` in the tests'
  unconfined mode), as on the driver's vector; the wrapper is **one function**, `inklkunn_prikk::sandbox::with_data_ceiling`,
  now used by both (so the fetch crate still has no `Command::new` of its own for the gate to count). The default ceiling is
  256 MiB (`DEFAULT_DATA_CEILING`, `CurlFetcher::with_data_ceiling`); `probe()` checks `prlimit`; a `prlimit` that cannot run its
  command is the typed `SandboxUnavailable` (`Spawn` when unconfined and it is `curl` that cannot start). Tests: a stand-in
  `curl` reports the limit it runs under (the configured one, in the unconfined vector and inside the real bubblewrap), a child
  that allocates past a small ceiling is stopped, and the real `curl` is stopped by a 1 MiB ceiling before it connects. The
  `Dockerfile`'s runtime image gains `curl` (with `ca-certificates`, as before) and checks both are usable.
- **F4 — the landing page is drawn by the web layer from the catalog.** `inklkunn_auth::Landing` no longer writes a sentence: it
  hands over the status (always 200), the target path, the cookies and the headers; `templates/landing.html` draws the page in
  the reader's language (`title.landing`, `landing.body`, `landing.continue`; English and a Japanese preview, glossary rows).
  The page is standalone, has no script, style or sub-resource, and says `<meta name="referrer" content="no-referrer">`,
  because the site's header layer sends `Referrer-Policy: same-origin` and the callback's address holds the provider's code.
- **Catalog.** Twelve messages, appended at the tails (`sign_in.provider`, `.provider_joins`, `.provider_failed`,
  `.provider_state`, `.provider_not_linked`, `.provider_unreachable`; `invite.provider_intro`, `.provider_link`,
  `.provider_taken`; `title.landing`, `landing.body`, `landing.continue`) with glossary rows in the same order, and a concept
  row for *provider* (プロバイダー). The structural rules and the retired-words test hold every new state in both languages
  (`tests/signin/provider_states.rs`: 11 states each); the route enumeration lists the three routes with their public reasons.
- **Tests that fail without each behaviour (mutated, see the handoff report):** a callback without the state cookie (the
  comparison removed), a replayed state (the state kept after use), the Lax cookie and its single use (Strict; the removal
  dropped), linking only where the policy says (the join check made `true`), the button hidden when unconfigured (a provider
  name always present), the ceiling (`prlimit` removed, a wrong number), the store's one-step redemption, and the audit of
  `account link` (taken out).

Corrections from the review of 2026-11-01 (handoff 004, P2′, P13, P14).

- **P2′ — a retirement excuses its own recorded findings and nothing wider.** The first rule (any block at or before the retired key's
  recorded point, ref updates by count, ref states up to the number of views *now*) let a rollback of `heads/main`, or a branch
  published from an old block, after a retirement read as the retirement's own cost; the counting rules and the `views` allowance are
  gone. Now a retirement (`rotate_keys` with `OldKey::Retire`, under the wiki's write lock) runs `prikk verify` **before anything
  changes** and **refuses to retire** unless it passes (or fails only by what an earlier retirement recorded): the old key stays
  trusted, nothing else is touched, and the error is `SignerError::NeedsRecovery` (*the wiki needs recovery first; nothing was
  changed*). It removes the key from trust, runs `verify` again, and records **the exact set of blocks, ref states and ref updates
  it names** in the authenticated key record (`excused`, in format `inklkunn-key-record/3`, bound into the seal; formats 1 and 2
  still open and a retirement recorded in them **excuses nothing**). A later failing `verify` is excused **only if** its report has
  no failed condition but `publication-trust`, its text has exactly one `publication trust issues:` count line (two are refused, as
  is any line of another shape) and as many findings as it counts, **and every block, ref state and ref update it names is in the
  recorded set**. Nothing is excused by count or by position. Measured on prikk 0.55.0: the text of the verify *before* lists the
  signer of every sealed block (`sealed-block <id>: <key id>`); after the key is removed it lists none (`sealed blocks: 0`), and the
  JSON report names no block at all. So a **block** is recorded only when the verify before listed the retired key as its signer
  (a named block with another signer, or none, is left out and reported in `OldKeyOutcome::RetiredUnrecorded`, and stops the wiki).
  **What the record can prove:** that the ids were there, signed by a trusted key, when `verify` passed just before the removal, and
  that they lost their trusted signature at that moment; the signer of each block. **What it cannot:** the signer of a *ref state* or
  *ref update* (prikk 0.55 names none, so they are in the set because the removal made them fail), and that the retired key's blocks
  were honest before the retirement (that is what a compromise puts in doubt). A second retirement starts from a repository that fails
  only by the first one's record and records the findings the first had not; the union is what is excused. A retirement that would
  name more than 150,000 findings is refused before the key leaves trust (the record's bound, 16 MiB). A retirement made before
  this change, or one whose record could not be taken (`RetiredUnrecorded`), excuses nothing: every later `verify` failure stops
  the wiki, and there is no command that repairs that record. The verification page (after *Check now*) adds *The only failures are
  those the retirement of key … recorded* when that is so (`CheckOutcome::Failed::explained_by_retirement`, filled by the signer
  through `Verifier::explain_retirements_with`); the badge still says *Verification failed* (DR-10). The persisted state shows the
  sentence only after a check made now, not on the page's first load: showing it from the record would need a store field (a schema
  change), which was not made. Tests (real prikk, self-skipping): the signer's `a_branch_published_from_an_old_block_after_a_retirement_is_not_excused`,
  `a_rollback_after_a_retirement_is_not_excused`, `a_block_sealed_by_a_third_key_before_the_rotation_is_not_excused`,
  `a_block_sealed_after_a_retirement_by_the_retired_key_itself_is_not_excused`, `a_retirement_is_refused_while_verify_fails_and_nothing_changes`,
  `a_second_retirement_starts_from_what_the_first_recorded_and_the_two_add_up`, the lifecycle tests
  `a_retirement_recorded_without_findings_excuses_nothing` and `a_verdict_and_a_text_that_disagree_conclude_nothing`, plus the
  existing writes-continue and planted-second-failure tests; the pure judgement in `signer/retirement/tests.rs`; the key record's
  `a_retirements_recorded_findings_survive_a_reopen_and_are_bound_into_the_seal` and the format-2 test; core's check tests and the
  web's `a_failure_a_retirement_recorded_is_said_so_beside_a_badge_that_still_says_failed`. A catalog message and its glossary row
  (English and a Japanese preview): `verification.explained_by_retirement`.
- **P13 — recovery removes only the `doctor` findings the excuse names.** `Recover::verify_after_retirement` now answers with a
  `RetirementExcuse` (the note, the retired key ids and the ids of the findings), and the plan takes out of `doctor`'s report the
  `PRIKK-TRUST-PUBLICATION-UNTRUSTED` diagnostics whose kind and id are in it (`DoctorReport::without_excused_publication_findings`,
  `Diagnostic::publication_subject`); one that names anything else, or whose message is not of the known shape, stays and the plan stops
  on it. Tests: core's `recovery_takes_out_only_the_doctor_findings_the_excuse_names`; the reader's
  `only_the_publication_findings_an_excuse_names_are_taken_out_of_the_report`.
- **P14 — one look at `verify`, and exactly one count line.** The verdict and the findings come from one `Writes::verify_run`: prikk
  0.55.0 prints the verdict as JSON **or** the findings as text, never both in a run, so the two are asked for back to back (under the
  wiki's write lock or the recovery lease) and the answer is refused when they disagree on whether the repository verifies. The text
  parse refuses a report with two `publication trust issues:` lines (it used to take the last). A question to the signer is now one
  such pair of runs (it used to be a verdict, then a text read by a separate call, with the caller's own run before them), and none at
  all when the wiki has no retirement that recorded anything; the acceptance's and recovery's own first `verify` is still separate.
Corrections from the review of 2026-11-01 (handoff 004, P15, P16).

- **P15 — recovery finishes a publication a killed `seal` left, with prikk's own recommendation, as a recorded step.** A `seal`
  killed between publishing its block and draining the queue (about 3 % of the kill matrix's `inside prikk seal` runs) leaves
  the block in the history and the patch in the queue: `prikk status --format json` and `worktree-status` refuse with an
  integrity error, and `doctor` reports one error, `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING`, recommending `seal
  --allow-no-audit` for the ref. Recovery now answers it with the step `complete-publication`, taken **only when** `doctor`
  names that code and nothing else (any other code beside it is `doctor-trouble`), and the block published is the one
  unfinished act's own: the newest block of the act's view seals exactly one patch, whose message has the act's digest,
  made on the block the act was made on, and the history found the act's message in it; otherwise the new stop
  `published-block-is-not-the-act` (nothing changed, flag kept). The signer's new `Recover::complete_publication` (a
  `RecoveryOp::CompletePublication` in the recorded plan, the lent lease, the recorded act) re-checks `doctor`, the record,
  the base, the one patch's message, and the patch's files read **from the block** (`show` for the operations, `cat` at the tip
  for the bytes, against the accepted change's digest), refuses a queue that holds anything but that patch, runs `seal` once,
  and calls it done only when the tip is the same block and `doctor` no longer reports the cleanup (`show` joins the signer's
  cost table at the read driver's measured rule). The default method is *unable*, as `Storage::status`'s is. The plan clears a
  stale `active.lock` first, puts the worktree back after the seal if it cannot be compared (prikk refuses to report it until
  the publication is finished), and ends the act as sealed in that block. The empty lock file (`prikk unlock` lists it with
  `recorded pid: (unparseable)`) and a lone `ACTIVE-REF-METADATA-DEBRIS` warning stay **fail-closed** (a witness that cannot be
  read, and `doctor-trouble`; nothing changed, flag kept), each with its operator steps, what to check before and after, in
  `docs/RECOVERY.md`; they join the next letter to prikk. Tests: the plan table (the code exactly, the block the act's mark by
  mark: another message, two patches, another base, another view, the history not finding it, an ended act, no act; other
  codes and a second error are `doctor-trouble`; the step is in the recorded plan and read back as the signer's operation); the
  signer against a real prikk (the state made by putting back the three files of the active session that a `seal` empties:
  finished with one more `seal` and no second block, renames and a removal proven against the digest, a message, files or
  base that is not the record's refused with the cleanup still pending, every guard); recovery against a real prikk with a
  stand-in `prikk` whose `seal` publishes and then fails (`dies-after-publishing-seal`, with and without the stale lock): the
  plan, the run, the records, the audit, the next acceptance; the empty lock and the debris asserted fail-closed, and the
  runbook's step for the empty lock followed; the real kill matrix gains two rows that make the state every time (the stand-in's
  `halfway-seal`, for an acceptance and a system act), and its `inside` rows that land in it now recover (measured, 76 runs of the
  real matrix: 3 recovered and 4 ended fail-closed, as the runbook says they do; corrected from "56 runs, 2 landed" by Q8). 8 new prikk-dependent tests (and
  2 rows of the matrix).
- **P16 — the runbook for an orphan queued patch is tested against a real queued patch.** A test builds the state the kill
  matrix's `prikk commit` rows leave when the service's records of the act are gone (a database restored from a copy older
  than the repository), reads the commands out of `docs/RECOVERY.md` and runs them as written (`prikk status --format json`,
  `show`, `worktree-status`, `log`), and checks each statement: the STOP word and its meaning in the plan, the witnesses it
  names, that `prikk` 0.55 has no discard but `doctor --discard-damaged-commits`, which refuses over a sound patch, that the
  stop records the plan and that it stopped and changes nothing. It found two errors, now fixed: `prikk status` (text) does not
  print the patch id (the runbook asks for `--format json`), and `prikk show` prints the replaced and the new text of each
  operation, not whole pages (the worktree, which is the patch's result, has them). The auth crate's timing test
  `unknown_account_and_wrong_password_cost_the_same_within_noise` believes a difference between the two paths only when five
  measurements in a row show it (each with names and addresses of its own, taken one by one and stopped at the first in which
  the two cost alike), as the markup crate's scaling tests do (R10): a descheduled sample can only make a path look dearer.
  The cheaper-than-a-hash checks are not retried, and the decision has a test of its own on synthetic measurements
  (`a_difference_between_the_two_paths_is_believed_only_when_every_measurement_shows_it`). 20 runs in a row beside 8 busy loops
  pinned to the test's two CPUs hold. 1 new prikk-dependent test.
- **U6 — writes switched on (the sign-in handoff).** Two end-to-end tests in `crates/inklkunn/src/serve/writes_on/`, over the
  binary's own `prepare` and `Identify` (nothing stood in), a real prikk, a real key store, a real SQLite store and a real
  socket. *The web:* accounts are made by the operator's `account invite` and the invitation page (a maintainer, a
  contributor, a reviewer; the link works once), signed in through `/-/sign-in` with a cookie jar that obeys the `__Host-`
  rules and forms read off the pages (the hidden `csrf` field, the double-submit cookie, the same-site headers, a text area
  sent with CR LF as a browser sends it); a forged, another session's, a missing and an empty CSRF token and a post from
  another site each change nothing; an anonymous visitor and a contributor cannot accept (even by posting the form), a
  contributor cannot review and a reviewer cannot approve or ask for changes on her own proposal (D9); then propose, comment,
  approve, accept: the page shows the change, the proposal says *Signed into the history*, the history and the change page
  show the message and both people, `prikk log` has exactly one new block, a retry writes nothing, the badge comes from a check
  that ran afterwards (the verification page says *Verified* with that time), a second person's proposal written against the
  old tip is re-authored and accepted (both changes on the page), and a proposal that overlaps stops with *Stopped: changes
  overlap*, shows both texts and leaves the repository as it was; an open proposal never changes the page every reader sees;
  an ended session is no session; no password, operator secret or session value reaches a log, a note or a page. *The write
  API:* a script with `propose` / `review` / `accept` tokens does the same (`reauthored: true` on the moved base, `overlap`
  as a problem); a `propose` token cannot review or accept, a `review` token cannot accept, a contributor's `accept` token
  cannot review or accept, an invalid token is `invalid-token`, a cookie with no CSRF token cannot write. **What still said
  writes were off is gone:** a link to a page that is not there leads to the form that proposes it (in the default view; NAV-3),
  the missing-page notice no longer waits for "the next release", the build-without-an-acceptor sentences (kept, for the tests
  and for any build that serves no acceptor) say *this build* rather than *yet* (`accept.not_served*`,
  `AcceptOutcome::NotServed`), the API's `405` no longer says the API only reads, the README's status and quick start, the
  health module's note; the threat model gains §7.3 (the cells §7.1 and §7.2 left open until sign-in was served). The
  readiness report for the owner is `docs/releases/0.2.0-readiness.md`. Mutations: the CSRF layer made to pass everything fails
  the web test at the forged token; the follow-up's verify job made to return fails both at the fresh check; the acceptor
  unset fails both with the 501; `create_href` made `None` fails its unit test and the page test. 2 new prikk-dependent tests.
- **Corrections from the review of 2026-11-01 (sign-in handoff, G1 – G10, A1 – A4).**
  - **G1.** The proxy warning at start fires for a loopback bind too (the default bind, and the likeliest deployment is a
    TLS proxy on the same host), and the server says once, at run time, the first time a loopback peer sends
    `X-Forwarded-For` while no proxy is trusted: *every client counts as one address*. `SECURITY.md` gains "Deployment
    notes", which lead with `INKLKUNN_TRUSTED_PROXIES`.
  - **G2.** A provider sign-in start keeps its client key with the pending login and is refunded when the callback
    succeeds (to the address that started it); the start requires `Sec-Fetch-Mode: navigate` and `Sec-Fetch-Dest: document`
    when the browser sends them (a request without them is served as before; a refusal spends nothing); pending logins are
    bounded per address (32) as well as overall, and a full table drops the oldest login of the address with the most.
  - **G3.** Redeeming an invitation is throttled like joining (`by_join`, per client address) and tells a taken sign-in name
    before the password is hashed (`new_local_account` checks the cheap things first; the store still checks in the step that
    spends the link). `Authenticator::redeem_with_password` takes the client address.
  - **G4.** The CSRF layer decides with `Identify::for_write` (closed: a session it could not read is a refusal) and lets
    through only a request that is signed out, a valid token, or a session with its token; the decision goes into the
    request's extensions (`csrf::Decided`) and every write handler (the proposal pages, accept, the inbox, following, the
    verification check, the write API, the account page) takes its principal from there. The wiki lookup of a read takes the
    principal it already has (`wiki::open_as`), so a read resolves once instead of twice.
  - **G5.** Sign-out with a bearer token does nothing to any cookie session and does not touch its cookie.
  - **G6.** Making an API token asks for the password again; an account with no password (it signs in through a provider)
    needs a session started within ten minutes (`Authenticator::confirm_for_sensitive_act`, `FRESH_SESSION_SECS`),
    throttled and constant-work like a sign-in and counted under the account's sign-in name.
  - **G7.** `X-Forwarded-For` is read from the right and stops at the first valid hop that is not a trusted proxy; what a
    client prepended on the left (junk, a long hop, any number of hops) is never read. The header-length limit is gone with
    it; 32 hops are examined at most.
  - **G8.** Signing in, joining or redeeming an invitation while signed in ends the session the request presented (a
    provider sign-in cannot: its callback is a cross-site navigation and the `Strict` cookie is not sent).
  - **G9.** The OpenAPI document describes the cookie client's `X-CSRF-Token` (the `sessionCookie` and `csrfToken` security
    schemes, the header on every write, and the two alternatives of a write's `security`), with a drift test against the
    server's own constants. One term, *the person who runs this wiki*, replaces *server* in the needs-account message
    (catalog, Japanese preview, glossary row and a concept row).
  - **G10.** The account page changes the password (`POST /-/account/password`): the current password is asked and judged
    first, throttled and constant-work like a sign-in; every other session of the account ends and every API token is
    revoked; the CSRF token is on the form. Catalog, Japanese preview and glossary; structural states in both languages.
    An account with no password sees a sentence and no form.
  - **A1.** The process makes itself non-dumpable when a command starts (`rustix::process::set_dumpable_behavior`, no new
    crate; recorded in `docs/dependency-policy.md`), and `SECURITY.md` says `LimitCORE=0`.
  - **A2.** A start-up warning, not a refusal, when the secret file can be read or changed by group or others or is owned by
    a user who is neither the server's nor root.
  - **A3.** The docs say the secret is read **at start** (up to three times, each copy zeroized) and that a server without a
    signer refuses **every change to the history** while proposals and comments are still stored: `SECURITY.md`,
    `serve/acceptor.rs` (comments and the start-up line), threat model C-1c and RR-11, and the API's `signing-not-configured`
    detail (it said *accepts no proposals*).
  - **A4.** The follow-up worker runs each job under `catch_unwind` (the panic is a line and the next job runs) and says
    once if its queue is closed.
  Tests are in the files of the code they hold; 1 new prikk-dependent test (`a_secret_file_that_others_can_read_is_a_warning_at_start_and_the_server_still_signs`).

Spike C (RFC 005 part 1: claim identity), measurement only.

- **`inklkunn-corpus claims`** (`plan`, `get`, `convert`, `replay`, `pool`, `measure`) replays real revision histories (24 articles,
  English and Japanese, 2,160 pinned revisions) through the product's own block segmentation and the merge's own alignment,
  with two labellers under a published rule sheet; the results and the report are in `rfcs/handoffs/005-knowledge-layer/spikes/`.
  No article text and no editor identity is in the repository; the text lives in a cache outside it.
- **`inklkunn_core::align_blocks`**: the merge's own block alignment between a base page and a side, exposed for measurement tools
  (no behaviour changed).

Follow-up K2 (handoff 005, review of 2026-11-01).

- **One list of owned page keys.** `knowledge/check.rs` re-exports the proposal service's `OWNED_KEYS` as `OWNED_PAGE_KEYS` instead of keeping a copy
  (`kind`, `visibility`, `system`): the two rules cannot drift apart.

Corrections from the review of 2026-11-03 (handoff 004, Q1 – Q8).

- **Q1 — a retirement never strands a wiki.** `rotate --retire-old` writes a `retiring` state into the authenticated key record
  **before** the old maintainer key leaves the trust store: the blocks the `verify` just before listed as the key's, how many
  publication-trust records prikk checked, and the newest block of every view. The record format is `inklkunn-key-record/4`
  (`/1` – `/3` still open and mean nothing is retiring; the AAD binds the state, a `/3` record cannot carry one). If anything
  after the removal fails to complete the record (the `verify` after cannot run, the two looks disagree, the count line is
  missing or differs, the findings exceed the record's limit, the key store cannot be written) the state stays;
  `OldKeyOutcome::RetiredUnrecorded` makes `inklkunn wiki keys rotate` **exit 1 naming the command**, and
  **`inklkunn wiki keys finish-retirement <wiki>`** (new; claims the `KeyAuthority`, audited before and after as
  `keys-finish-retirement`) completes the after-`verify` and the record **from the kept state**, under the write lock, and only
  while every view's tip is the block the state kept; it refuses, changing nothing, when no retirement is unfinished, when the
  key is still trusted (a removal that failed keeps the state too: remove it by hand, then finish), when the `verify` is not a
  clean publication-trust failure with the kept count, and when anything was written since. `rotate` is refused while a
  retirement is unfinished; `status` prints `UNFINISHED` with the command. While it is unfinished nothing is excused: an
  acceptance answers *sealed, not confirmed* with the command in its detail, and **recovery stops with the new word
  `retirement-unfinished`**, names the command and runs nothing (it needs the key authority). `OldKeyOutcome` gains
  `RetiredPartly` (the record is as complete as it can be, but some block lacks a trusted signature without being the retired
  key's: also exit 1, with no command to run); the module docs no longer say "a later rotation repairs it" (none could).
  Tests (the ways in each end in a state the command finishes: after-`verify` unavailable, the runs disagreeing, the count line
  missing after, the count differing, the key directory read-only, the limit exceeded; a write after the failure makes it
  refuse; rotation refused meanwhile; a removal that failed; the CLI end to end with its audit trail; the plan rows; recovery
  against a real repository and the accept path), and the key store's (round trip, binding of every field of the state, shape
  checks, `/3` opens, `update_meta` keeps the seeds and cannot change an id).
- **Q2 — the publication a killed seal left is finished only if the queue is exactly the published patch.** Measured on prikk
  0.55.0 in the cleanup-pending state: `status --format json`, `worktree-status` and the other JSON reads refuse (an integrity
  error), `doctor` and `verify` print no queue, and **the plain-text `prikk status` reads** and prints `active WAL records: N`,
  `trailing partial WAL bytes: N` and `queued patches: N targeting REF`. `complete_publication` now reads that text (each line
  exactly once, in the known shape) and refuses unless it shows one patch on the view, in a log of one record with no torn byte;
  an unreadable text is a failure to read, not a mismatch. **It is a real check of the queue's size, not of its content**: the
  text names no patch id, so that the one queued patch is the published one rests on `doctor` naming a publication whose active
  state matches and on the block's own patch (read from the history); the question for prikk (a read that lists the queued
  patch ids in this state) is recorded in `docs/UPSTREAM.md` for the next letter. 1 prikk-dependent test (nine refusals of
  queues and texts, then the finish).
- **Q3.** The text of a P14 `verify` run is refused when it holds a finding line (`<condition> [PRIKK-…]: …`) of any condition
  but publication trust and the verdict is a failure. Test: a retirement's failure with an injected `ref-publication` finding
  is no longer excused (refused with the condition named), and the parser's rows.
- **Q4.** A retirement records only if prikk's `checked publication trust records` is the same before and after the removal
  (the count is kept in the `retiring` state; a missing or different count leaves the state for `finish-retirement`, which
  applies the same rule); a retirement is not even begun from a `verify` that prints no count.
- **Q5.** `explained_by_retirement` is **stored with the verdict**: store schema **15** (`wikis.verification_explained_by`, a JSON
  array of key ids, `NULL` for none, with a `CHECK` that only a failed verdict carries one; migration from 14 tested on a
  database with a failed and a verified wiki, once, and equal to the fresh schema). `WikiStore::set_verification_explained`
  writes verdict and explanation in one statement (refused for a verdict that did not fail, for what is not a key id, for more
  than 64); `set_verification` clears it, an attempt that could not run leaves it with the verdict (R1). The verification page
  says *the only failures are those the retirement of key … recorded* whenever it is so, on the first load as well, beside a
  badge that still says *Verification failed* (DR-10); *Check now* shows it once. A verdict recorded before schema 15 carries no
  explanation until the next check.
- **Q6.** `complete_publication` filters `doctor` as the plan does (P13: it asks the signer's own question and takes out
  exactly the excused findings). Measured: while a cleanup is pending `verify` fails by **two** conditions
  (`blocking-ref-publication` and `publication-trust`), so no retirement's record excuses the retired key's findings there; the
  plan therefore never offered the step on such a wiki, and now **stops with the word `cleanup-beside-trust-findings`** instead
  of the generic `doctor-trouble`. The question for prikk joins `docs/UPSTREAM.md`; `docs/RECOVERY.md` has both new words.
- **Q7.** Duplicate `sealed-block` lines are refused (the retirement is not made from such a look); the `verify` before a
  retirement also asks `is_partial()` when the verdict is ok; **the signer enforces the base for every acceptance**: a record
  whose `pinned_tip` is `None` is refused by `accept` (nothing written), by `seal_queued` and by `complete_publication`
  (the field keeps its type; its documentation says every acceptance names one, as the service's records always did). The
  concurrent-acceptance test now asserts that the second of two changes planned on one tip is refused when its turn comes.
- **Q8.** The real-kill evidence is stated as measured: 76 runs of the real matrix, 3 recovered and 4 ended fail-closed (the P15
  entry above is corrected).

Q1 – Q8 add **17 prikk-dependent tests** (lifecycle 8, the accept tests 3, the recovery tests of the signer 3, of the core 2, the
command line 1): `EXPECTED_SKIPS` 183 → 200. Every fix was mutated (reverted, the test seen to fail, restored).

Corrections from the review of 2026-11-03 (sign-in handoff, H1 – H6, E1, E2).

- **H1 — `SECURITY.md` says only what is true.** `INKLKUNN_TRUSTED_PROXIES` lists **only the proxies' own addresses, never a
  network clients are in** (a client inside it would choose its own address); the `10.0.0.0/8` example is gone from
  `SECURITY.md`, the `config.rs` table and the start-up error for a typo. Signing in while signed in ends the presented
  session **except through the provider** (the `Strict` cookie is not sent on its callback: end it from the account page). A
  provider start cannot be spent by another site's images, scripts or frames (a top-level navigation can). The owner of an
  account whose sign-in is locked from a stolen session ends that session at the account page, which needs no password; the
  account page and the sign-in page share one bucket per sign-in name. Pending provider logins are per IPv4 address or IPv6
  /64, so a /48 can still push out other people's logins (a retried sign-in).
- **H2 — a declared proxy whose header cannot be read is said once.** `Authenticator::first_unreadable_forwarded_for`
  (beside `first_ignored_forwarded_for`, an atomic swap) and one run-time line naming `INKLKUNN_TRUSTED_PROXIES`: a proxy that
  appends ports, writes `unknown` or sends none puts every client on the proxy's address (`ClientAddress::fell_back`, which
  nothing read before).
- **H3 — no decision, no principal.** `csrf::Decided` with nothing attached answers `Anonymous` and its `write_identity` is
  the refusal (*the form expired* for a page, `forbidden` for the API); it no longer asks the credential again, so
  `principal()` and `write_identity()` take no arguments (and `Ctx::open_deciding` and the watch lookup lose their headers).
  The registry test now walks every `POST`, `PUT`, `PATCH` and `DELETE` of the application table (the content origin has no
  write), proves each refuses a cookie write with no token and answers a wiki that is not there identically, and fails for
  any write that is not in its list of exemptions.
- **H4 — a redemption that succeeds gives its join budget back** (a refused one stays counted).
- **H5 — a password change rotates the session.** `Authenticator::change_own_password` loses its `session_secret`
  argument and returns the new `NewSession`: every session of the account ends (the one that made the change too), a new one
  starts, every token is revoked, and the account page answers with the new `__Host-` cookie (`no-store`) so the following
  page carries the new CSRF token. The credential, the sessions and the tokens change in **one store step**:
  `CredentialStore::change_credential` (new trait, in `MemoryStore` and `SqliteStore`; `Authenticator::new` needs it), tested
  for all-or-nothing on both stores (a refusal met after the writes leaves the old password, sessions and tokens) and through
  the authenticator over a store whose step fails. `Sessions::end_all_but` is gone. The G10 tests are rewritten.
- **H6 — words.** The OpenAPI text says the token is in the hidden field `csrf` of the forms of the pages served (not that a
  page shows it); the stale comment in `api/write.rs` and the one above `mod hardening` in `main.rs`; the unused
  `account:<id>` throttle key is removed (the password asked at the account page is counted under the account's sign-in name,
  as at the sign-in page; an account with no password has none to ask), and `SECURITY.md` says the one bucket.
- **E1 — the decide form is not shown to the proposer.** A proposer who is a reviewer sees the existing sentence
  `refused.own_proposal` where the form would be; the POST is still refused by the service (the web test pins both).
- **E2 — *Propose this page* on the health page's wanted rows**, beside *who links here*, to the new-page form of the default
  view (the view these lists are of): `health.propose_page` (catalog, Japanese preview, glossary row after `health.see_links`);
  the row cell holds a list of links; the module note of `health.rs` says so.

Every fix was mutated (reverted, the test seen to fail, restored).
Corrections from the review of 2026-11-03, second (handoff 004, W1 – W8).

- **W1 — writes pause while a retirement is unfinished.** While the key record's `retiring` state is set the signer refuses every
  new seal before it changes anything: `accept`, recovery's `seal_queued` and the creation of a view (a new ref in the history whose
  views the state recorded), in one sentence that names `inklkunn wiki keys finish-retirement <wiki>`. This replaces Q1's *sealed,
  not confirmed meanwhile*. The refusal is typed (`AcceptError::RetirementUnfinished`, `RecoverError::RetirementUnfinished`) and
  mapped to a fixed word of its own, `retirement-unfinished` (`acceptance::words`): the acceptance record ends *failed* with that
  word, the intent is abandoned, the proposal stays held for review, the wiki is not flagged, and the same proposal is accepted
  once the retirement is finished or given up. The web answers it as its own outcome (`Unchanged::RetirementUnfinished`, 503: *a
  change to this wiki's keys is not finished; the person who runs this wiki must finish it; the proposal is kept*; catalog English
  and Japanese preview, glossary rows in the same order); the API as the problem **`retirement-unfinished`** (36 problem codes;
  listed on the accept endpoint, so the OpenAPI document and its drift test follow). The recovery plan stops with
  `retirement-unfinished` for a queued patch too (before, only for `doctor`'s findings) and names both commands. `docs/KEYS.md` and
  `docs/RECOVERY.md` say so. Of the three points of `rotate --retire-old` a kill can come at, the first (before the key record is
  replaced) leaves nothing unfinished (the new key is adopted and harmless): there is nothing to pause or finish, and a test says so.
- **W2 — `finish-retirement` finishes from whatever step remains.** With the old key *still trusted* (a kill before the removal, or
  a removal that failed) it takes a fresh `verify` under the write lock (it must pass, as `rotate` requires), writes the `retiring`
  state again from it, **removes the key itself**, then takes the `verify` after and records. No message or document tells an
  operator to run `prikk trust maintainer remove` any more (the rotation's failure text, the CLI, `docs/KEYS.md` and the tests were
  fixed). With the key already removed it works from the kept state as before.
- **W3 — `inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>`** (audited before and after as `keys-abandon-retirement`,
  under the write lock): with the key still trusted it clears the state (the rotation stands, as a rotation that kept the old key);
  with the key already removed it adopts it again in the repository's trust under its own id with the public key the state kept
  (`prikk trust maintainer add --key-id ID --public-key HEX`: the public key is all that adding needs, and it is read from
  `trust maintainer list --format json` when the retirement begins, so no seed is touched), then clears the state. It prints *the old
  maintainer key … is trusted again; nothing is retired*. Every refusal of `finish-retirement` that no later run can cure (the
  repository changed outside inklkunn, the count differs, `verify` after the removal is not a clean publication-trust failure, the
  trusted keys are not those the retirement began with, a block was left out, the record limit) says so and names this command; the
  ones a later run can cure (`verify` cannot run, the key record cannot be written) say to run `finish-retirement` again.
- **The record format changed: `inklkunn-key-record/5`.** A `retiring` state now also holds the retired key's **public** key and
  the sorted ids of the maintainer keys the repository trusted when it was written (both inside the AAD, in that order, after the
  existing fields). `/1` – `/3` open and mean nothing is retiring; a `/4` record opens too, and a `/4` state (which has neither) opens
  with both empty: it can be finished (the list is not compared) and cannot be abandoned after the key was removed (the refusal says
  so and names `finish-retirement`); a `/5` state must hold both (the retired key among the ids, sorted), `/4` must hold neither.
  Every record written is `/5`. No store schema change (15 stays).
- **W4.** A finish that would leave blocks out (`left_out > 0`) records **nothing** and keeps the state; its refusal names
  `abandon-retirement`. `OldKeyOutcome::RetiredPartly` and `RetirementFinished::note` are gone (nothing else produced them).
- **W5.** Finishing, with the key already removed, requires the trust list now to be exactly the kept list without the retired key;
  a key added or removed outside inklkunn in the window makes it refuse (naming abandon).
- **W6 — the stored explanation never outlives its truth.** `rotate --retire-old`, `finish-retirement` and `abandon-retirement` each
  end by recording a fresh verdict through the check the server uses: the binary builds the same `Verifier` over the same store and
  prikk, with the signer as its retirement reader (`Verifier::check_unthrottled`, new: `check` without the recency rule, because the
  operator at the host is not a visitor and a verdict a minute old is exactly what a retirement makes untrue). If the check cannot run
  or its verdict cannot be recorded, the stored explanation is cleared with `set_verification` (the verdict itself stays) and the
  command says so. Test: K1 retired and recorded (verdict *Failed, explained by K1*), then K2's retirement left unfinished: the
  explanation is gone; finishing or giving K2 up records a fresh one.
- **W7 — a killed seal on a retired wiki is finished, narrowly.** `complete-publication` is offered beside a recorded retirement
  only when `verify` fails by exactly `blocking-ref-publication` and `publication-trust` (run to the end), the text names the pending
  cleanup once (`ref publication issues: 1`), every publication-trust finding is in a recorded set (`judge`'s rule, unchanged), and
  `doctor`, with those findings taken out, shows only the cleanup. After the step the plain rule must hold (the `Verify` step is
  unchanged: publication trust alone, all of it recorded) or recovery stops and the flag stays. The verdict's rule and the
  acceptance's rule do not change. `Recover::verify_beside_cleanup` is the new question (default: no); `cleanup-beside-trust-findings`
  stays for anything else. Tested against real prikk (the state a kill leaves, measured) and with stand-ins for each way the answer
  could be wider.
- **W8.** `ExpectedMessage.pinned_tip` is a `BlockId`; the refusals of `None` and the two tests that made one go with the `Option`.

W1 – W8 add **21 prikk-dependent tests** and remove 3 (the two tests of a missing base, and Q1's *sealed, not confirmed* test, replaced by
the W1 tests): `EXPECTED_SKIPS` 203 → 221 (measured with prikk shimmed off). 2,144 tests pass in both runs. Every fix was mutated
(reverted, the test seen to fail, restored): 23 mutations, three of which first survived and got a sharper test (a stale `retiring`
state left in the key record by a finish that stops at the removal; the trust list inside the AAD edited without changing its count).

Corrections from the review of 2026-11-03, third (handoff 004, X1 – X5).

- **X1 — any key that left the service can be retired, by its id.** `inklkunn wiki keys retire <wiki> --key <maintainer key id>
  --confirm <slug>` (audited before and after as `keys-retire`, then a fresh verdict as for the other retirement commands) retires a
  **superseded** maintainer key that is still trusted in the repository: the same before-`verify` (which must pass), `retiring`
  state (that key's id and public key), removal, after-`verify` and record as `rotate --retire-old`, with the record made on **that
  key's** superseded entry found by its id (`conclude_retirement` no longer takes `last_mut()`). It refuses, changing nothing, the
  wiki's current key (*rotate first*), an id the key record does not know (it lists the ones still to be retired), a key already
  retired, a key the repository does not trust, and any key while another retirement is unfinished. **One code path:**
  `Signer::retire_superseded` is the begin, the removal and the end for both commands; `rotate --retire-old` is the rotation
  (taken after the `verify` that must pass, which still refuses the whole rotation when it does not pass) followed by that
  retirement of the key it just replaced. The key record's check (`Meta::check`) now accepts a `retiring` state about any superseded
  key that is not retired and holds no findings, found by id (it was the newest only). `OldKeyOutcome` gains
  `RetireNotBegun(String)` (the rotation happened, the old key is trusted, nothing is paused, the state could not be begun; the
  text names `retire`); `Signer::retire_key` and `retire_command` are new public API. Abandon's sentence, every refusal that names
  abandon (`abandon_text`), `status`'s `UNFINISHED` line, recovery's `retirement-unfinished` stop, `docs/KEYS.md` and
  `docs/RECOVERY.md` say what abandoning costs (*the old key … is trusted again; nothing is retired; to retire it later run
  `inklkunn wiki keys retire <wiki> --key <id>`*). The compromise playbook says to retire (a kept key too), and the false sentence
  that a retirement can be tried again with `rotate --retire-old` after an abandon is gone: it retires the *current* key.
- **X2 — abandon never re-trusts a key whose retirement finished.** `abandon_retirement` takes the audit trail (`&dyn AuditLog`) and,
  before it adopts a removed key again, reads the whole trail (a page of 500 at a time) for an *allowed* event of `keys-rotate`,
  `keys-retire` or `keys-finish-retirement` for this wiki whose detail **begins** `retirement recorded: key <id>` (the fixed form,
  written first because an event keeps 200 characters and an id up to 128; `trail::recorded_detail` writes it, `recorded_key` reads
  it strictly: a key id as prikk accepts one, then the end or `; `). The commands write it only when the record was made. If the
  trail says so, abandon refuses and changes nothing: *the key record looks older than the audit trail; restore the key directory
  from a later backup*; a trail that cannot be read is no answer (the key is not adopted, nothing is changed, run it again). Events
  written before this carry no such form and count as *not known to be finished*. `docs/KEYS.md`'s restore paragraph says the same.
- **X3 — words.** Finish's refusal after a failed fresh look no longer says *retire the key afterwards* (the shared look's
  texts are neutral; `rotate` and `retire` add their own *run `inklkunn wiki recover <wiki>`, then …*, which also replaces the old
  `inklkunn recover`, a command that does not exist); abandon's audit text and sentence are true when the key was added again
  outside inklkunn (*the key was trusted in the repository when this ran*; *is trusted* where it was not put back, *is trusted
  again* where it was); every error on abandon's trust path says what is true and names the command: the trust list unreadable
  (*nothing was changed*), the add failing (*the key may or may not have been added*, run abandon again, which looks first), the
  add that did not show in the list, the list unreadable after the add; finish's first read of the list says the same.
- **X4 — fail closed.** `seal_pause` returns `Result<Option<String>, String>`: a key record that cannot be read (a damaged one, a
  missing one) refuses the seal before the ledger is asked or a seed is read (`AcceptError::Unavailable`, `RecoverError::Unavailable`;
  `create_view` on a wiki with no key record is now this refusal instead of *not pending*). Abandon's *still trusted* check compares the
  public key as well as the id; another public key under the id is a refusal that names it and changes nothing.
- **X5 — the never-released `/4` record format is refused**: *format /4 was never released, so this record did not come from a released
  inklkunn and is not read; restore the key directory from a backup made by a released build or by this one*. `/1` – `/3` and `/5`
  open as before. A `/5` state must hold the public key and the sorted trusted ids (the empty-list path of W5, the *older build*
  refusal of abandon and `KeyStore::rewrite_as_format_four` are gone).

X1 – X5 add **18 prikk-dependent tests** net (19 added, one removed: the `/4` state test): `EXPECTED_SKIPS` 221 → 239 (measured with
prikk shimmed off). 2,172 tests pass in both runs. Every fix was mutated (reverted, the test seen to fail, restored): 28 mutations, one of which first survived
(the cost sentence of abandon's refusals) and got a sharper assertion.

Corrections from the review of 2026-11-03, fourth (handoff 004, Y1 – Y6).

- **Y1 — a retirement never excuses a late seal.** A retirement records only the blocks the key sealed **up to the tips the key
  record kept when that key was superseded**. `signer/superseded.rs` has one function for that set (`known_when_replaced`), used by
  the late-seal check of `status` and by the end of every retirement (`conclude_retirement` →
  `findings_after_retirement` → `record_of`), so a block the check calls a late seal is a block no retirement records. A block the
  key sealed afterwards is **not recorded**, is **not** W4's *left out* (W4 still refuses for any other block that lost its
  signature), and is **not** a reason to keep a leaked key trusted: the key is still removed and the record holds its set less the
  late blocks. The key record keeps nothing new. The verification page and recovery therefore do not describe a failure on a late
  block as *only a retired key* (no recorded set holds it, so `judge` does not explain the failure). It is said where the operator
  reads it: `OldKeyOutcome::Retired` became `Retired { late_seals: Vec<String> }` and `RetirementFinished` gained `late_seals`; the
  CLI of `retire`, `rotate --retire-old` and `finish-retirement` prints `WARNING: block <id> was sealed with maintainer key <key>`
  for each and `LATE_SEALS_SENTENCE` (*these blocks were sealed with this key after it was replaced, so this service did not make
  them; verification keeps failing on them, and nothing excuses them*), notes the count in the audit detail, and **exits 1** as
  `status` does for the same blocks (the retirement itself is done, recorded and audited as finished). prikk names no signer for a
  ref state or a ref update, so the two the thief's seal made are in the record beside the service's own; the late block is what keeps
  the failure unexplained. `Signer::view_histories` is the one reading of every view's history. `docs/KEYS.md`'s compromise runbook
  says what a late seal means (a block in the history the service did not make: export, read, decide; nothing rewrites history).
- **Y2 — `status` says what the repository trusts.** A superseded key whose record is not marked retired is *not retired*; *still
  trusted* / *out of the trust store* come from the repository's trust list (`Signer::trusted_maintainer_ids`, one read); if the
  list cannot be read, `status` says it cannot tell. An unfinished retirement that removed the key no longer reads *still trusted*.
- **Y3.** `status` names `inklkunn wiki keys retire <wiki> --key <id> --confirm <the wiki's slug> --accept-unverifiable-history`
  beside every superseded key that is still trusted and not retired (for the key whose retirement is unfinished it names nothing:
  `UNFINISHED` names what ends that; for another key it adds *once the unfinished retirement above is finished or given up*).
- **Y4.** `OldKeyOutcome::RetireNotBegun`'s text drops the inner *nothing was changed* (`NOTHING_CHANGED` closes the refusals made
  before the state is written, and `not_begun_text` takes it off after a rotation): the keys were rotated, the old key is still
  trusted, and the command that retires it is named.
- **Y5 — `docs/KEYS.md` says only what is true.** A failed retirement after a rotation is a table of what each outcome names
  (`retire` for *not begun*; `finish-retirement` / `abandon-retirement` for an unfinished one, where the key may already be out of
  the trust store). The restore paragraph: back up the key directory after every key command; finishing after a restore works only
  if nothing was written and no key changed since; after restoring the key directory and the store together, run `status` and retire
  again whatever should be retired. One sentence on X2's limits (a retirement whose ending event was never written, or lost when the
  audit store and key directory are restored together, is invisible to abandon's check). `docs/RECOVERY.md` names the late seal.
- **Y6 — `retire` takes `--accept-unverifiable-history`.** One flag for one cost: `retire` is refused without it, changing nothing
  and before the audit trail is touched, with the sentence `rotate --retire-old` says (`refusal_without_acceptance`). The usage
  line, `docs/KEYS.md`'s command list, `retire_command` (and so every text that names `retire`: abandon's sentences, `status`,
  `RetireNotBegun`, recovery's `retirement-unfinished` stop) show the flag.

Public API: `OldKeyOutcome::Retired` is now `Retired { late_seals }`, `RetirementFinished::late_seals`, `LATE_SEALS_SENTENCE`,
`Signer::trusted_maintainer_ids` (new); `retire_command` carries the flag. Store schema unchanged (15).

Y1 – Y6 add **12 prikk-dependent tests** (`EXPECTED_SKIPS` 239 → 251, measured with prikk shimmed off): five in
`signer/lifecycle/tests.rs` (a late seal named and not recorded; the same with none; finish with the key removed and with it still
trusted; abandon then retire again; a rotation that retires the replaced key beside a kept key's late seal) and seven in
`keys/tests.rs` (`retire` and `finish-retirement` with a late seal; `retire` without and with the flag; `status` for a kept key, for
an unfinished retirement, for an unreadable trust list, and for the unfinished key beside another). Two tests need no prikk
(`record_of` with a late seal; `RetireNotBegun`'s text). Every fix was mutated (reverted, the test seen to fail, restored): 31
mutations, none survived; one first did not apply (a formatting split) and was redone as two.

Corrections from the review of 2026-11-04 (handoff 004, Z1 – Z6).

**Z1 amends Y1: there is no separate late-seal path any more.** The fifth round on retirement had added a path each time; this one
removes one.

- **Z1 — a retirement records only what it can trace, or nothing.** After the removal every finding of the fresh `verify` must be
  traceable to the key's own history as the service made it: a block in `known_when_replaced` (the one function the late-seal check
  of `status` uses too); a ref state in the same function's other half — the `ref_state_id` of each view's log entries from the
  **oldest** entry whose block is the recorded tip back to the start (`Writes::view_blocks` keeps the ref state it used to drop, so
  one function yields blocks and ref states); ref updates as they were. A `tag` finding is now parsed (`PublicationTrust::tags`)
  instead of being a line of an unknown shape, and is untraceable, as is a late block, a ref state nothing in the history explains
  (a `branch close`, a `branch create` or any other act with the stolen key that seals no block), a block another key lost
  (W4's case, now one rule) and a recorded tip the repository no longer shows (said in its own words). If **anything** is
  untraceable the retirement records nothing and stays unfinished: the key is out of the trust store, writes stay paused (W1), the
  key record gains nothing. `retire` / `rotate --retire-old` (as `RetiredUnrecorded`), `finish-retirement` (a refusal) and `status`'s
  `UNFINISHED` line (`Signer::untraceable_in_unfinished_retirement`, the same trace made read-only) name every finding (the first 20
  by name, then a count) and say `UNTRACEABLE_SENTENCE` once: *the repository holds acts with this key that cannot be traced to what
  it sealed for this service; nothing excuses them, and this service seals nothing more until the history is restored from before
  them (`docs/KEYS.md`, compromise) or the retirement is given up with `abandon-retirement`, which trusts the key again*. Removed:
  `OldKeyOutcome::Retired { late_seals }` (a plain `Retired` again), `RetirementFinished::late_seals`, `LATE_SEALS_SENTENCE`, the
  exit-1 late path of the command line and the audit detail's late-seal count. A compromised key whose acts the service cannot
  trace is therefore never half-retired: it is out of trust, the wiki is paused, and the operator is told the two ways out.
- **Z2 — no loop.** Recovery stops with `retirement-unfinished` (never `doctor-trouble`) with the same sentence in its note; no
  text sends the operator to `inklkunn wiki recover` for it. A `verify` that fails by publication trust alone on findings no
  retirement recorded is a `Refused` that says recovery does not cure it (it used to be `NeedsRecovery`, whose wrapper said
  *run `inklkunn wiki recover`, then retire*). `docs/RECOVERY.md` says so in a paragraph of its own.
- **Z3 — `docs/KEYS.md`.** An act made with a key after it was replaced needs write access to the wiki's repository outside this
  service: a host compromise. Restore the repository, the key directory and the store **together** from a backup taken before it
  (every later write is lost) and retire again, or abandon and accept the key as trusted. Step 5 no longer says the acceptance
  records do not exist.
- **Z-b.** `status` no longer says *no block was sealed by a superseded key after its rotation* when a superseded key is out of the
  trust store (prikk lists the signer of a block only for trusted keys): it says the check cannot see that key.
- **Z4.** `status` says *the repository looks older than the key record: restore the key directory from the same backup, then
  retire it again* when the record calls a key retired and the repository trusts it again.
- **Z5.** A rotation, `Keep` or `Retire`, refuses while a publication is pending (the state recovery's `complete-publication`
  finishes), naming `inklkunn wiki recover <wiki>`.
- **Z6.** This entry and the readiness report say what Z1 makes of a compromised key.

Public API: `OldKeyOutcome::Retired` is a unit variant again; `RetirementFinished` loses `late_seals`; `LATE_SEALS_SENTENCE` is
`UNTRACEABLE_SENTENCE`; `untraceable_text` and `Signer::untraceable_in_unfinished_retirement` are new. Store schema unchanged (15).

Z1 – Z6 add **8 prikk-dependent tests** (`EXPECTED_SKIPS` 251 → 259, measured with prikk shimmed off; 11 added and 5 Y1 tests removed in
`signer/lifecycle/tests.rs`: a late block; a `branch close`; a `branch create`; a tag; all three beside the service's own acts; a lost
tip; the same retirement with none of them; a view the service made; finish with the key still trusted; a failure no retirement
recorded does not send anyone to recover; a rotation that retires beside a kept key's acts. One in `tests/recover.rs`, a rotation
while a publication is pending. In `keys/tests.rs` the two Y1 tests became one plus `status` for a key it cannot see (Z-b) and for
a repository older than the record (Z4)). The tests that need no prikk: `record_of` with everything untraceable, the tag, the
long list, `known_when_replaced` counting from the oldest entry, the plan's note and stop. Every fix was mutated (reverted, the test
seen to fail, restored): 24 mutations; three survived at first (the finish and retirement texts, and `status`'s sentence when the check
runs) and their tests were strengthened until they were killed; two were reapplied after a formatting split.

Follow-ups from the review of 2026-11-04, second (handoff 004, Z7, Z8).

- **Z7.** `inklkunn wiki keys status` exits non-zero whenever a retirement is unfinished (writes are paused), whatever the late-seal
  check could see. The check cannot see a key that is out of trust, so after `retire` ended unfinished (a close, a tag, a late block)
  every `status` used to exit 0. The `UNFINISHED` line is still printed; the error says to look at it. A wiki with no unfinished
  retirement keeps its exits.
- **Z8.** A recorded tip that is not in its view's log is named **first** among the untraceable findings, so the cap of twenty names
  never hides it; while a tip is lost, the blocks the key sealed that are not known are worded *not in that view's history up to the
  newest block recorded when the key was replaced*, not *sealed with the key after it was replaced* (which is not known to be true
  then). The sentence shown to people whose proposal is held while a retirement is unfinished (the API's detail and hint, and the
  outcome's note, English and the Japanese preview) says writes resume *when the person who runs this wiki finishes or gives up a
  change to its keys*; it no longer promises that finishing does.

Z7 and Z8 add **1 prikk-dependent test** (`EXPECTED_SKIPS` 259 → 260): `status` after a `branch close` and after a tag by the
superseded key. The tests that need no prikk: a lost tip named first and the neutral wording, the lost tip inside the cap, the
pause sentence. The three `keys/tests.rs` tests that pinned exit 0 for an unfinished retirement now pin 1. Every fix was mutated
(reverted, the tests seen to fail, restored). Store schema unchanged (15).

## [0.1.0] — 2026-10-31

**M1 — read a wiki.** The first release: a standalone server that serves a prikk repository as a wiki. Pages in the
Portable Wiki Markdown profile with free links, backlinks, dangling links, redirects and aliases; history, differences
and permalinks read from prikk; an honest verification badge on every page (*Verified*, *Verification failed*, or
*Could not be checked now*, never a guess); serve-stale when prikk cannot be asked; search, recent changes and Atom
feeds; the raw source of each page on a separate content origin; export as a Portable Wiki Bundle and as a prikk
archive; `inklkunn wiki create`, `import-folder` and the key lifecycle on the command line; a read API with an OpenAPI
3.1 document; English, and Japanese as a labelled preview. **Not in this release:** signing in, proposing and
accepting changes (M2, 0.2.0). Every visitor is anonymous, and only public wikis are readable. Runtime: prikk ≥ 0.54.0,
bubblewrap and prlimit on the host, or the container image.

The entries below are in the order the work landed, M0's foundations first.

M0 — foundations (RFC 001, RFC 010).

- **The workspace and its gates.** Eight crates in three layers (`inklkunn-prikk`, `inklkunn-signer`,
  `inklkunn-store`; `inklkunn-markup`, `inklkunn-core`, `inklkunn-auth`; `inklkunn-web`, `inklkunn`) and four
  gates as tests: layering, key reachability (only `inklkunn-core` may depend on the signer; its key store
  stays private), prikk spawn (only the driver and the signer spawn prikk; the driver names no write verb),
  markup purity. Pinned build chain: one compiler for CI and release, MSRV 1.88.0 built separately, actions
  by commit SHA, images by digest, the prikk floor 0.54.0 in one place.
- **`inklkunn-markup`.** Frontmatter in the plain YAML subset (anchors and tags refused, YAML's date and
  boolean coercions kept visible), the Portable Wiki Markdown Layer 2 scanner, MKUP-5 link resolution, HTML
  rendering with the MKUP-12 conventions, advisory lint with guideline ids, and `title_to_path` under a
  property test. wiki-commons' conformance corpus (24 scan cases, 10 resolution cases) passes.
- **`inklkunn-prikk`.** The `Storage` trait and `PrikkCli`: eight read verbs with pinned `schema_version`s
  and typed outcomes, bubblewrap with the repository read-only, a server-kept deadline, a `prlimit` ceiling
  from a per-verb cost table, and a light/heavy admission gate.
- **`inklkunn-store`.** The wiki record, validated slugs, opaque time-ordered ids, the `<root>/<shard>/<id>/`
  layout, in-memory and SQLite stores under one invariant suite.
- **`inklkunn-core` and `inklkunn-signer`.** The `authorize()` seam (default deny), the `Accept` trait, the
  `Assurance` values, the egress guard, the bundle layout, and *create wiki* end to end through a bootstrap
  signer (plain-text key directory; RFC 002 replaces it).
- **The two spikes** (RFC 001 D-10), measured on prikk 0.54.1 and reported under `rfcs/handoffs/001-foundations/`:
  the per-verb cost table is inklkunn's own (`diff` 4.6 × the store size is the expensive read; `tree` and `cat`
  1.0 ×; `verify` 2.0 ×; `branch` and `log` flat), and the content-merge spike shows that prikk's merge is
  confluent for 53 of 120 concurrent-edit pairs while a block-aware three-way merge is clean for 99 — the
  justification for inklkunn's own merge in RFC 004. `tools/corpus` generates both corpora from a seed;
  `tools/spikes/` holds the harnesses. prikk refuses repository paths over 1024 bytes, which bounds a wiki's
  hierarchy depth (about 500 one-character levels).
- **The M0 review's corrections** (2026-10-27; review requests v2). The driver reads a ceiling hit through
  bubblewrap (exit 128 + n, or bwrap's "Out of memory") as a ceiling, charges the admission gate the real 1.5 ×
  ceiling, checks every argument's shape before prikk runs, binds exactly the caller's scratch directory for
  `materialize` and resolves every mount against symlinks, bounds a call's output, and refuses bubblewrap's own
  failures as prerequisites rather than prikk's refusals. The store validates ids, slugs and display names on
  every path including deserialization, refuses a newer schema before touching the file, and runs each change
  in a transaction. *Create wiki* stores its record last and removes a failed attempt's directories. The signer
  exposes only named operations; the bootstrap signer is behind the `bootstrap` feature. The egress guard
  refuses control characters, port 0, Teredo, ORCHID and more, and resolves through a seam. The markup crate
  bounds frontmatter nesting, takes code and heading ranges from CommonMark's parser, replaces constructs at
  their scanned spans, writes the scanner's anchors as ids, never emits a `javascript:` or `data:` href, and
  is linear on every pathological input in a bounded test. `title_to_path` percent-encodes non-ASCII titles
  (prikk refuses non-ASCII paths) exactly reversibly. The gates read manifests in every spelling, scan tests
  and nested files, require the lints and `forbid(unsafe_code)` in every crate, and keep test features out of
  product crates; `pin-check` understands job-level guards and pins `cargo install`; `floor-check` records
  the prikk assets' checksums; CI counts the shimmed self-skips; the release gates run both test passes, join
  the images under one tag and attest them.
- **Conventions and ledgers** (RFC 001 D-11; RFC 010): README, this changelog, `SECURITY.md` stating RR-1, the
  dependency policy with a generated lock inventory that a test keeps equal to `Cargo.lock`, the upstream
  dependency ledger (PK/PL/WC rows), `docs/UPSTREAM.md`, `docs/RELEASING.md`, the mdBook.

RFC 002 and RFC 003 — the first handoffs after M0, in progress. Still nothing serves pages.

- **The patch message** (RFC 002 D-5; handoff 002 T5). One grammar, in `inklkunn-signer::message` and
  re-exported as `inklkunn-core::message`, writes and reads what every signed change carries: a one-line
  summary of at most 120 characters, then `Proposal`, `Proposed-By`, `Accepted-By`, `Base`, `Kind` and
  `Imported-From` once each in that order, for the four kinds (proposal, rollback, import, system). The parser
  is strict and total: only canonical text parses, and an error never repeats the text it refused. A real
  prikk returns a sealed change's message byte for byte, which a test checks for every kind. The text M0
  wrote for a created wiki still parses.
  *Output change:* `DisplayName` now refuses U+2028 and U+2029 as well as controls, as its error text always
  said: a reader that splits lines on them would otherwise see a forged trailer in a signed message.
- **`authorize()` and the roles** (RFC 002 D-2; handoff 002 T2). The decision is a pure rule (`decide`) over a
  snapshot that an `Authorizer` loads itself, so a handler can no longer say that a wiki is public: a
  resource names its wiki and nothing about it. The ladder is observer, questioner, contributor, reviewer,
  maintainer, admin; a token's scope (`read ⊂ propose ⊂ review ⊂ accept ⊂ admin`) is intersected with the
  account's role. A principal with no grant may read what the wiki's visibility lets it read, and nothing else;
  a wiki that is not there and a restricted one it has no role in give the same answer, the same audit event
  and the same number of lookups. The decision fails closed, records consequential decisions (accept,
  moderate, admin, export, import) allowed or not, and refuses an allowed one it could not record. The
  system principal cannot be built outside `inklkunn-core`. `inklkunn-store` gains the role grants and the
  audit sink trait, and its SQLite file now migrates forward from M0's schema (schema version 2).
  Two gates join the others: routes are registered in the web crate's `routes.rs` only (so the route table is
  complete), and no web source names a system principal or builds an account one.
  *Output change:* `inklkunn_core::authorize` (the free function that denied everything) is replaced by
  `Authorizer::authorize`; `Resource::Wiki` no longer carries a visibility; `Principal::Account` carries the
  credential's scope.
- **The key store** (RFC 002 D-3; handoff 002 T3). Each wiki's author and maintainer seeds are sealed in one
  record, `<wiki id>.key.json`, under a master key that Argon2id derives once at start from the operator's
  secret (a file; 16 to 1024 bytes without its trailing line break) and a store header that detects a wrong
  secret when the store is opened. Records use XChaCha20-Poly1305 with a fresh 192-bit nonce each, and bind
  the wiki, key ids, generation and times as associated data, so none can be edited or moved to another wiki.
  A seed is decrypted only into a new 0700 directory on a verified tmpfs with 0600 files, for one write call,
  overwritten and removed afterwards (and on a panic); a removal that fails is reported, never swallowed.
  Randomness comes from `/dev/urandom` with no fallback. Nothing in the module is public, and a new gate fails
  if any public item of the signer crate mentions a seed, a secret or a key directory. The module is built
  and tested here and is wired to `Accept` by the next task. Dependencies: `argon2`, `chacha20poly1305`,
  `zeroize` (+22 lock entries, measured; see the dependency policy).
- **`Accept` and `create_view`** (RFC 002 D-4; handoff 002 T4). `Accept` is now defined in `inklkunn-signer`, the
  crate that implements and enforces it, and re-exported by `inklkunn-core`; `Signer` implements it. Before a byte
  is written the signer reads the acceptance record the token names (through an `AcceptanceLedger`) and
  refuses unless it is pending seal for this wiki and view and the message says exactly what the record says
  (proposal, proposer, acceptor, base, kind). Then: nothing unsealed and a clean worktree, the view's worktree,
  the renames, writes and removals (paths validated whole beforehand: ASCII, prikk's rules, the bundle's
  roots), `commit`, `seal`, read the block back and check it is this change, `verify`. A failure before
  `commit` puts the worktree back and says so; a failure after is `Incomplete` for recovery. A view is created
  only for a recorded act. The first change of a new wiki goes through `RepositoryBootstrap`, which now
  refuses anything but a `system` message to a repository with no history. A start-up sweep removes the
  seed directories a killed server left behind.
  *Output change:* `inklkunn_core::accept` re-exports the signer's types; `PreparedChange.message` is the text
  of a `Message`; `SignerConfig` is built with `SignerConfig::new`.
- **The audit trail and the reconciliation** (RFC 002 D-7, D-9; handoff 002 T6). The audit log is a SQLite
  table that the database itself refuses to update or delete from (two triggers abort with *the audit log is
  append-only*, so a stray `UPDATE` or `DELETE` fails even from a hand-opened connection); the store's schema
  is now version 3 and migrates forward from M0's schema. Every sink cleans each field again on the way in
  (control characters and U+2028/U+2029 become `?`, at most 200 characters), so an event built by hand cannot
  forge a second line. `AuditLog` reads the trail in pages after a position, and `export_jsonl` writes it, oldest
  first, one object per line with fixed keys. The reconciliation (`inklkunn_core::reconcile`) reads every
  view's whole history, a page of 500 blocks at a time, and compares it with the acceptance records and the
  system-act records: a block whose message cannot be read, a seal no record explains, a system change no act
  explains, a record whose block is not in the history, and a record that disagrees with the message it names
  (kind, proposal, proposer, acceptor, base, source) or puts the change in a view that does not hold it. It
  only reads: it repairs nothing, and `record_findings` writes ids and
  a fixed word to the audit trail (outcome *reported*), never a message, a name or a path. Tested against a
  fake storage for every finding and against a real prikk history, where it also leaves the history and the
  current view as they were.
- **Principals and credentials** (RFC 002 D-1; handoff 002 T1; the owner's ruling DR-9). `inklkunn-auth` turns a
  presented credential into a principal. *Local passwords* are Argon2id (19 MiB, two passes) in the PHC format,
  NFKC-normalised, with a cost ceiling on what a stored hash may ask for; a new password has ten characters
  and no composition rule. *Sign-in is constant-work*: past the throttles every attempt does exactly one
  verification (against a dummy hash made at start, and of the same cost, when there is nothing usable to
  check, a stored string nobody can verify included), after taking a permit from a bounded gate, and unknown
  name, wrong password, disabled account and correct-password-for-a-disabled-account are one answer. *Throttles
  are counted before they are judged*: an attempt is checked against the lock and counted in one step, so a
  burst of simultaneous guesses is held to the limit exactly as a sequence is (sixty at once at one name: five
  judged, fifty-five turned away); an attempt that was right, or that the service was too busy to judge, gives
  its count back. They count failures per sign-in name as typed (so a made-up name locks exactly like a real
  one) and per client address (an IPv4 address or an IPv6 /64), five and thirty failures lock for fifteen
  minutes, a correct password is refused while locked, a lock is written to the audit trail (no name, no
  address), and the address comes from `X-Forwarded-For` only when the peer is a declared trusted proxy (a
  trusted proxy whose header cannot be read says so). *Sessions* are opaque random ids kept only as a hash, in
  `__Host-` cookies that are `Secure`, `HttpOnly` and `SameSite=Strict`, each with its own CSRF token (a
  double-submit cookie for the sign-in form), a new one at every sign-in; a request that changes anything goes
  through `principal_for_write`, which cannot be answered without the token. *Changing a password* proves the
  old one (throttled like a sign-in) and, like an administrator's reset or a disabling, ends every session
  **and revokes every API token**; enabling an account again does not bring them back. *API tokens* are `ink_`
  plus 256 random bits, kept as a hash, with a scope, a lifetime of at most a year and a day, at most a hundred
  live per account, and a revocation; a token's principal carries its scope, which `authorize()` intersects with
  the account's role (a test runs every scope, role, visibility and action). *OpenID Connect* is the
  authorization code flow with PKCE, owned by the authenticator: starting one spends the client address's
  budget; the state is one-shot, server-side and must also be in the browser's state cookie, which the
  authenticator reads from the `Cookie` header itself (login CSRF); ID tokens are verified by `kid` against
  the provider's keys with `jsonwebtoken` on the `aws_lc_rs` backend (RS256, RS384, RS512, ES256, ES384 and
  EdDSA only; the key must be published for signatures; issuer, audience, `azp`, nonce and times checked,
  `iat` required and at most fifteen minutes old; every refusal carries the reason for the log); the provider's
  keys are refetched for an unknown `kid` at most once a minute, and a provider that is down is not asked again
  for five seconds. An identity nobody linked signs nobody in, and an e-mail address is only a hint. The
  network is reached through one seam, `HttpsFetcher`, and the token verifier is a seam with a test double.
  The store gains accounts, linked identities, sessions and tokens (schema version 4, with `CHECK` and
  foreign-key rules in the database, and a password hash that is not an Argon2id string refused). Dependencies:
  `jsonwebtoken` (`aws_lc_rs`) and `sha2` (+27 lock entries, measured, `rsa` and `crypto-bigint` absent, no
  advisory ignored; see the dependency policy). *Changed after an independent reading of this task:* the
  throttle ordering above, the cost of an unverifiable stored hash, the credential revocations, the typed
  state-cookie path, the audit of locks, the reasons, the redacted `Debug` output of the OpenID Connect types,
  the token cap, the configuration checks, and `Sessions`, `Tokens`, the throttles and the gate becoming
  private to the crate.
- **Key lifecycle** (RFC 002 D-8; handoff 002 T7). `inklkunn wiki keys status|create|rotate <wiki>` shows a
  wiki's key ids, generation and dates (never a secret), makes keys for an initialised repository that has
  none, and rotates them: new author and maintainer keys under **new ids** (prikk binds an id to one key for
  good), the new maintainer key adopted first, the sealed record replaced last, so a failure leaves the wiki as
  it was. Rotation needs the wiki's slug typed (`--confirm`) and is audited. **The old maintainer key stays
  trusted by default** and its history keeps verifying; `--retire-old --accept-unverifiable-history` removes it
  from the repository's trust, after which it can seal nothing and **prikk 0.54.1 cannot verify what it sealed**
  (ledger PK-34, a test that fails the day prikk changes this; RFC 002 D-8's "the repository still verifies"
  cannot hold with retirement until then). The binary gains its configuration (`INKLKUNN_DATA_DIR`,
  `INKLKUNN_SECRET_FILE`, `INKLKUNN_KEY_DIR`, `INKLKUNN_SEED_DIR`, `INKLKUNN_PRIKK`, and the `--dev-unconfined`
  flag, which prints a banner and is not settable from the environment). `docs/KEYS.md` is the runbook,
  including the compromise procedure and what a lost key store means.

- **Changes after an independent reading of T2 – T7** (the second of the two readings of handoff 002).
  *The reconciliation read only the newest page of a long history*: it paged by block id, which real prikk
  answers with an empty list and exit 0 (ledger PK-36), so past 500 blocks it saw a prefix and called the rest
  missing or clean. It now reads each view whole (one `log --limit`, repeated four times larger while it comes
  back full, an error at a stated bound), the driver refuses a block point for `history`, and a standing finding
  is reported once, not at every run. *Acceptance outcomes say what state the repository is in*:
  `Queued { step }` (one patch queued, nothing sealed), `SealedUnconfirmed { sealed, why }` (sealed; do not
  retry), `WorktreeDirty` (a failure that could not be undone), beside `Refused`, `Unavailable` and
  `SeedsNotRemoved`; after a retirement every acceptance is `SealedUnconfirmed { VerifyFailed }`, which a test
  pins. *Two acceptances on one wiki in one process no longer interleave* (a lock per wiki; they once sealed one
  change's message over the other's file). A rotation whose retirement fails is a `Rotation` that says the old
  key is still trusted, the command line audits an act before and after it and refuses to begin one it cannot
  record, and the wiki record follows the key store. A failed wiki creation now removes its sealed key record.
  The audit table also refuses `INSERT OR REPLACE` over an existing position. Seed paths are scrubbed from
  prikk's words before they become errors; the seed directory must be owned by the service's user and not
  writable by others, and a live call touches it so a long acceptance is not taken for a dead one. The gates
  catch `Principal as P`, glob and group imports, function-call spellings of route registration and
  `routes.rs` at any depth, and a surface naming the signer; their matcher is a lexer now, so `impl X` or
  `dyn X` cannot hide `X`. `Decision` is `#[must_use]`. `inklkunn audit export` writes the trail as JSON
  Lines. The write ceiling of the signer's prikk calls is **still the fixed 1 GiB** the handoff asked to
  measure and replace; that measurement was not done (see the review request).

Corrections to handoff 002 (the architect's review of 2026-10-29, `identity-keys-and-signing-corrections-handoff-v1`).

- **A key store is never made by accident, and its creation is one critical section** (C4, IS-17).
  `Signer::open` refuses a key directory with no store and names the path; `Signer::open_creating_store` (what
  `inklkunn wiki keys create` uses) makes one. Creation takes an exclusive `flock` across the emptiness check and
  the header's link, so the loser of a race opens the winner's store instead of being told its directory "is not
  a key store", and a crash's leftover temporary file no longer blocks creation for good. `rustix` (`fs`) joins
  the signer for `flock(2)` (no new lock entry); `std::fs::File::lock` needs Rust 1.89 and the minimum is 1.88.
- **A memory ceiling is never a refusal or an unconfirmed seal** (C1, threat model C-9a). Every call the signer
  makes runs under a ceiling from a costed rule per verb, sized from the store and the bytes of the change
  (measured on prikk 0.55.0, ledger PK-38), replacing a fixed 1 GiB that `verify` of one 300 MiB attachment
  exceeded; a 300 MiB attachment is now accepted, and so is the change after it. A death by the ceiling uses the
  driver's own classifier and is `CeilingExceeded` (nothing committed, worktree as it was), `Queued { ceiling }`
  (a patch is queued) or `SealedUnconfirmed { CeilingExceeded }` (sealed, not confirmed; never "verify failed"); a
  `verify` stage that failed with the whole message `i/o error: out of memory` is the ceiling too.
- **Each write verb sees only the seed it signs with** (C2). `commit` the author seed; `seal` and `branch create`
  the maintainer seed; `init`, `trust`, `mv`, `branch switch`, `verify` and `log` none (a stand-in lists what each
  can see).
- **One writer per wiki across processes** (C3). Every act that writes a wiki's repository or key record takes an
  exclusive `flock` on `<root>/<shard>/<id>.lock` (never inside `.prikk/`); another process, such as the command
  line beside a running server, is refused at once with `Busy`, naming the process that holds it. The kernel drops
  the lock with a killed holder.
- **The signer checks the files, not only the message** (C5). A SHA-256 over the prepared change (files sorted,
  removals, renames in order) joins `ExpectedMessage`; a change altered after the record is `ChangeMismatch`
  before one prikk call, seed or file. `sha2` joins the signer (no new lock entry).
- **Seed directories are swept by liveness, not age** (C6). A seed directory is born locked and held for its
  call; a start removes every directory it can lock, at once, and none a live call holds; removal is by handle
  (`O_NOFOLLOW`), never following a link. The heartbeat is gone.
- **Principals and key operations need a witness** (C7). An account principal is built only with the process's one
  `IdentityAuthority` (held by the authenticator) and has private fields; `create_keys`, `rotate_keys` and
  `create_repository` (so `create_wiki`) take the process's one `KeyAuthority`. Compile-fail doctests show the
  refusals. *API changes:* `Principal::Account` holds an `AccountPrincipal`; `Authenticator::new` takes the
  authority; `create_wiki` takes the key authority; `RepositoryBootstrap::create_repository` takes it.
- **Outbound HTTPS is a confined `curl`** (C8, IQ-3). New lower-layer crate `inklkunn-fetch` runs `curl` in its
  own bubblewrap vector (network shared; no repository, key or seed; `/usr` and the CA bundle read-only; fresh
  `/tmp`; cleared environment) at exactly the addresses the egress guard pinned: https only, no redirects,
  bounded time and bytes, no proxy, no config file. The `Authorization` header and the form body travel on
  curl's standard input and appear in no argument, error, `Debug` output or log. No TLS crate enters the lock
  (209 → 210: the new member). The boundary gate makes it the third crate allowed to spawn (`bwrap` or `curl`
  only), depended on by the binary alone, and the only one that may name `curl`. The binary carries the
  `HttpsFetcher` adapter (guard + fetcher), unused until OpenID Connect is switched on in M2.
- **The OIDC round trip survives the browser** (C9). The callback is answered with a `Landing`, a 200 page that
  moves the browser on with a same-site request (a `SameSite=Strict` session cookie is withheld after a redirect
  from a cross-site-initiated navigation); the login-state cookie is `Lax`, `__Host-`, `HttpOnly`, `Secure`, ten
  minutes, and removed by the callback. Checked once in headless Chromium 156. *API change:*
  `Authenticator::finish_oidc` takes the path to land on and returns `OidcSignedIn { signed_in, landing }`.
- **Smaller items from the readings** (C10). Store schema **6**: `role_grants` has foreign keys to `accounts`
  (the migration rebuilds the table and refuses by name, rolling back, when a grant names an account that does
  not exist); `inklkunn serve` warns on a non-loopback bind with no declared trusted proxy (no setting exists
  yet); an account holds at most 100 live sessions (a sign-in beyond that ends the oldest, never refused).
- **A rotation records where the old key stopped, and a seal after that point is reported** (C11, DR-10). The
  key record (now format 2; format 1 still opens) keeps, for every maintainer key a rotation replaced, the
  newest block of every view at that moment and whether the key was retired. `inklkunn wiki keys status` (and
  every rotation, into the audit trail) reports every block such a key sealed that is not in the history up to
  those points: a leaked seed used against the repository while the service's records are intact. It changes
  nothing, `status` exits non-zero on a finding, and it cannot run on a repository that does not verify (after
  `--retire-old`; ledger PK-39: prikk prints who sealed what only in `verify`'s text).

RFC 003 — the read path, in progress (handoff 003 T1 – T3). Still no wiki page is served; the pages that exist
are the language switcher and the notices.

- **The web foundations** (RFC 003 D-5; handoff 003 T1). `inklkunn-web` holds what every later page inherits.
  *Two origins, one listener:* a request goes to the router of the origin its `Host` names, and a host that
  names neither is misdirected (421), so the content origin never answers an application route and the
  application never serves wiki-authored bytes. *Headers on every response* (200, 304, 404, 405, 421, errors),
  set in one function that a handler cannot override for the security headers: a Content-Security-Policy with
  `default-src 'none'` and **no script source at all** (one stylesheet from the application origin), a
  sandboxed policy on the content origin, `nosniff`, `Referrer-Policy: same-origin`, `Permissions-Policy`,
  frame denial, cross-origin policies, HSTS on https, and `Cache-Control: private, no-cache` with `Vary` unless
  a handler says otherwise. *One route registry:* routes are registered in `routes.rs` only (a gate), each with
  an action or a stated public reason, and a test fails any route that could shadow a page title (nothing but a
  wiki's slug and the reserved prefix `-/` may precede one). *Validated at start, every refusal named:* the
  bind address and the two origins are checked together (http only for loopback; the origins differ; on a
  reachable address the content origin has its own host), `Sandbox::Unconfined` needs the `--dev-unconfined`
  flag and prints a banner, prikk must be found at the floor, the store must open. `inklkunn serve [--config
  FILE]` reads `NAME=value` lines (a typo is refused with its line number) under the environment;
  `--version` prints the floor and the prikk found. The only route is `/-/health`. Dependencies: `axum`,
  `tokio` (1.54.1, past RUSTSEC-2026-0386) and `tower-service` (+26 lock entries, measured).

- **The words, the layout and the stylesheet** (RFC 003 D-6; handoff 003 T2; no new dependency). Every sentence
  a page says is a message in a catalog (`inklkunn-web::catalog`): stable keys, whole sentences with named
  placeholders, plural forms, and typed slots for a link or for a name in another language, so a message cannot
  carry markup. English is the source; Japanese is compiled in as a **preview**: the language switcher offers it
  with the word "preview" beside it, and it is chosen only by the reader (one `HttpOnly`, `SameSite=Lax` cookie
  holding a language code), never from a browser's `Accept-Language`, until the reviewers named under DR-7
  sign it off. The switcher at `/-/lang` is a form per language that needs no script, answers a form from
  another site with a refusal page, and returns only to a local address. `docs/glossary.md` fixes the words in
  both languages (a test holds it equal to both catalogs) and retires the version-control vocabulary (`seal`,
  `commit`, `rebase`, `patch`, `block`, `ref`) outside explanations a reader opens on demand. One base layout (a
  skip link first, a banner, one `main`, a footer, `lang` and `dir` on `<html>`) and one stylesheet with light
  and dark colour tokens, served from the application origin under a name made of its own bytes with a
  year-long `immutable` cache lifetime. The tests every later page inherits, each with its planted violation:
  the catalog audit (missing, orphan, duplicate, mismatched placeholders, plural forms, markup, braces),
  literal text in a template or a source file outside an allowlist whose dead lines fail, `|safe`, the retired
  words, nineteen structural rules over every page state in both languages parsed with `html5ever`, and the
  stylesheet checks (contrast of every pair in both schemes computed from the tokens, colours only as tokens,
  logical properties only, focus, reduced motion, 24 px targets, forced colours, nothing loaded). A one-off
  headless-Chromium run, which is not part of the build, found no sideways scroll at 320 px, with or without
  the text-spacing override, in either language; no person using assistive technology has tested any of it,
  and no fluent reader has read the Japanese. *Output change:* an unknown address on the application origin is
  now a 404 page in the visitor's language, not the text `Not found.`; the content origin's 404 and the 421 are
  one catalog sentence of plain text; `/-/health` and the 404 handler moved out of `routes.rs`.

- **The read path** (RFC 003 D-2; handoff 003 T3; no new dependency). `inklkunn-core::read` serves a page from a
  held tip: the title index (titles, aliases, redirects, page ids) is built from **one `extract` of the tip** into
  the wiki's scratch directory, once per tip however many readers meet it, and rendered pages are cached by the
  tip's state id, the path, the catalog language and the link base. A cached view makes no prikk call and a miss
  makes one once the index is there (a counting harness holds both). Everything a page carries reaches a template
  only as `SafeHtml`, which only the `ammonia` sanitizer can construct (a policy built from the markup's own
  vocabulary plus the profile's raw-HTML subset, empty today; a cached row is sanitized again on the way out); the
  cache has no field for a verification result. `extract`'s refusal for want of prikk's checkpoint record is the
  ordinary "could not be read now" and flags the wiki for a refresh; a request never passes
  `--allow-full-replay` (a test scans the driver's sources). The store gains **schema 5** (derived pages, the title
  index, and the wiki's checkpoint refresh; a database at 4 migrates forward with its data) and the driver an
  optional size hint that stands in for the store walk (flat verbs no longer walk the store). `inklkunn-markup`
  writes a block identifier as the `id` of its block (MKUP-12) and renders `![[Page]]` to depth 2, refusing a cycle
  and naming it (and a budget of 32 pages and 4 MiB per page, which the handoff does not state). Measured on
  spike A's corpora with prikk 0.54.1, a cold page on 10,000 pages takes about 0.55 s at 76 MiB and **about
  0.98 s at 212 MiB** (0.38 s and 0.64 s of it the index build), which is at the 1 s PERF-1 draws; a cached view
  takes 0.2 ms. *Not done and stated:* an author's own HTML that spells an element the renderer writes (a plain
  `<a href>`) passes the sanitizer, without `rel="nofollow"`; a stale page is not served when prikk is busy
  (OP-03). *Output change:* a block identifier is the `id` of its paragraph or block (`<p id="^p1">`), no longer a
  separate empty `<span>`; `![[Page]]` of a page shows that page (it was a link). *API changes:*
  `StorageError::Refused` carries the refusal's `reason`; `WikiStore` has two more methods
  (`request_checkpoint_refresh`, `record_checkpoint_refresh`); `RenderLinks` has three more for transclusion.

Follow-on to RFC 001 (issue IS-7, `verify-verdict-honesty-handoff-v1`).

- **A memory limit reached while checking a repository is reported as *could not be checked now*, never as a failed
  verification.** Under a memory ceiling prikk's `verify` may print a *complete* failing report whose `objects` stage
  failed with `i/o error: out of memory` (ledger PK-22). The driver now returns a failing report as a verdict only
  when every stage ran to an outcome: `evaluated`, `failed` with a finding, or `not_evaluated` behind a stage of the
  same report that failed with a finding. A stage that ran out of memory, a `halted` stage, a status the driver does
  not know, or a `not_evaluated` stage with no such blocker makes the report partial, and a partial report is
  `CeilingExceeded`. The out-of-memory message is matched only whole, after the prefix `i/o error:`, never as a
  substring. *API change:* `Stage` carries `message` and `blocked_by` and is read through `Stage::outcome()`;
  `Verification::is_partial()` and `stage_summary()` are new.
- **The completeness rule holds whatever the exit code, and an empty stage list is partial** (review V1, V2 of the
  driver follow-on). A failing report on exit 0, which prikk's contract says never comes, is a verdict only if it is
  complete; a report with no stages is not one prikk prints and is read as the memory limit.

Handoff 003, the notes before T4 (the architect's review of 2026-10-29).

- **Raw HTML an author wrote is shown as text, never as markup** (IS-14). While the profile declares no raw-HTML
  subset, an element a page's author spells out — a link that looks like an internal wiki link, a callout, a heading
  id, a block marker — is rendered as visible, escaped text, and a hidden comment stays hidden. The markup crate
  records the byte ranges of the Markdown source that *it* wrote and treats an HTML event that is not wholly inside one
  as the author's, so no spelling of the renderer's own elements can pass as it. The sanitizer stays as the second line.
  *API:* `AuthorHtml` and `render_page_with_html`; `render_page` and `render_html` escape.
- **Reads are pinned to the tip's block.** A view's tip is its state id and the block that state belongs to (one `log`
  per new state); `cat` and `extract` read at the block, so a write between two calls of a request cannot cache newer
  content under an older key. The first miss at a new tip is four prikk calls (the list, the log, the extract, the
  `cat`), a hit none.

Handoff 003 T4 (the page view; RFC 003 D-1, D-5).

- **A wiki's pages are served.** `/<wiki>/<Title>` (and `/<wiki>/~<view>/<Title>`), with the wiki's front door
  (`/<wiki>`: the page *Home*, else the about page), `/<wiki>/-/about` (license, default view, the accessibility
  statement that says no person using assistive technology has tested the site yet), `/<wiki>/-/verification` and
  `/-/wikis`. A title that resolves through an alias or a redirect says *redirected from*; a near title says what
  was asked; a missing page is a 404 that says so, with *Propose a change* shown disabled and a note that proposals
  arrive with the next release (no dead link); an ambiguous title lists its candidates; a redirect that leads nowhere,
  a page that cannot be shown, an unknown view and a title that cannot name a page each have a page in the reader's
  language. A page's own headings move down one level (its title is the page's one `h1`).
- **Existence is neutral.** A wiki that is not there and a wiki the visitor may not read give the same answer, byte
  for byte, for every address under it, including the verification form.
- **The badge is the record's.** Every page shows *Verification: Verified | Verification failed | Could not be checked
  now | Not checked yet*, read from the wiki record for that request, never from a cache; a page served stale says
  *Could not be checked now* whatever the record says. The verification page runs `verify` on request (a form, same-site
  only, never repeated within a minute), records the outcome, names the stages that did not come out well, and keeps a
  check that could not run (busy, over its limit, not started) apart from a verdict.
- **Serve-stale** (OP-03): when prikk cannot be asked, a page whose copy this server holds for the last tip it saw is
  served labelled as possibly out of date; a page it does not hold is *could not be read now* (503, `Retry-After`),
  never rendered from nothing.
- **405 and undecodable addresses are pages** in the reader's language with the shared headers.
- **Settings** (all optional, see `inklkunn` config): `INKLKUNN_TIP_TTL_SECS` (5), `INKLKUNN_INDEX_MAX_MIB` (4096; the
  index build extracts the wiki into the data directory for a moment, so it needs that much free disk),
  `INKLKUNN_INDEX_MAX_FILES` (200000), `INKLKUNN_PAGE_MAX_KIB` (1024), and `INKLKUNN_TRUSTED_PROXIES`, which ends the
  start-up warning for a reachable bind. *Not yet:* sign-in is not served, so every visitor is anonymous and only public
  wikis can be read.

Handoff 003 T9, the measurements (IS-12; corrections follow-up F1).

- **The read driver's cost table, measured on the single-large-attachment shape through the driver's own vector**
  (`prlimit` + bubblewrap, the repository mounted read-only) at 24, 64 and 200 MiB on prikk 0.55.0: `tree`, `cat`,
  `extract`, `show` and `diff` need **2.0 × S**, `verify` **3.0 × S**, at every size. The table (`tree`/`cat` 2.1 ×,
  `verify` 4.1 ×, ceilings 1.5 ×) holds with margin; **no multiple changes**. (The 5.0 × S for `verify` that PK-20 and
  PK-38 record is the *writable* `verify` of the signer's write path; the read vector is read-only.)
- **`tools/spikes/spike-a-write.py`** measures the write verbs the signer's table (`cost.rs`, PK-38) is derived from;
  `crates/inklkunn-prikk/examples/measure_vector.rs` the read verbs through the vector; `tools/spikes/make-attachment-shape.sh`
  builds the attachment shapes. Their outputs are under `rfcs/handoffs/001-foundations/spikes/`, and `docs/UPSTREAM.md`
  step 5 names them.

Handoff 003 T8 (the read API; RFC 003 D-13; no new crate).

- **The read API, GET half** (external design API-01, API-02, API-04). Under `/<wiki>/-/api/v1/` (the reserved
  `-/`, so no title can collide; the version is in the path): `pages` (a list, in cursor pages of at most 100, with
  the `namespace` and `kind` filters and a `view`), `pages/<title>` (metadata, table of contents and the sanitized
  HTML fragment; an alias or a redirect leads to its page and says so; served stale and labelled when prikk cannot be
  asked), `raw/<title>` (where the source is: the storage path; the source itself stays on the content origin),
  `fragment/<title>` (the HTML alone). **Not in this build, and answering the problem `not-available` (501) with the
  missing read named:** `history/<title>`, `revisions/<id>`, `diff` (which refuses a request without `path` first: there
  is no diff of a whole tree), `backlinks/<title>`, `health`, `search`, `changes`, and the list's `tag` and
  `changed_since` filters. Their reads are methods of `ReadService` in `crates/inklkunn-core/src/read/api.rs`, which
  the tasks that own them fill in.
- **Problem details (RFC 9457)** with a registry of 14 stable codes. The `type` is `urn:inklkunn:problem:<code>`, so
  it never depends on the host and is never a link that might not resolve; `title`, `detail` and `hint` are catalog
  messages in the reader's language (the language cookie, else `Accept-Language`, else English; Japanese stays a
  preview chosen only by the reader), and the `type`, `status` and extension members do not change with it. Unknown or
  repeated query parameters are refused by name, a page size above 100 is refused and never trimmed.
- **`ETag` and `304`** on every success (a hash of the body; checked only after the caller was allowed to read), a
  `Link: </-/api/v1/openapi.json>; rel="service-desc"` header on every API answer, and **existence-neutral 404s**: a
  wiki that is absent and a wiki the caller may not read give the same problem, byte for byte, whatever the path,
  query or `If-None-Match`; the right to read is checked before the query is.
- **The OpenAPI 3.1 document** at `/-/api/v1/openapi.json`, generated from the router's route table and the
  per-endpoint descriptions in `crates/inklkunn-web/src/api/describe.rs` (the handlers check the query with the same
  parameter descriptions the document is built from). A route without a description, or a description without a route,
  fails the build of the document; tests hold it both ways and check the document's structure.
- **Dependency:** `serde_json` becomes a direct dependency of `inklkunn-web` (already locked; the lock gains one edge,
  no package). axum's `json` feature stays off.

Handoff 003 T8, completion (the read API's seven stubs become real reads; no new crate, no new external dependency).

- **`history/<title>`, `revisions/<id>`, `diff`, `backlinks/<title>`, `health`, `search` and `changes` are real.** Each
  calls the read the corresponding web page calls (`page_history`, `change` and `page_at`, `page_diff`, the link graph,
  `stale_pages_in`, `search`, `changes`) and shapes the result as JSON, described in the OpenAPI document (a schema
  for each answer, an `enum` for a parameter that takes words) and held to it by a test in both directions (every answer
  validates against its schema; every declared member is seen). The history is stepped by cursor (a step is at most 20
  rows and may be empty with a cursor: the same step the page takes); a revision is a change by its `id` and, with
  `?title=`, carries that page as it stood after it; **`diff`'s `path` is the page's title** (the web diff's address; it
  was the storage path) and the points may be left out (`to` is the tip, `from` the change before it); `backlinks`, `health`
  (`list` is required: `wanted`, `orphans`, `dead-ends`, `redirects`, `wants-help`, `stale`) and `changes` are in cursor pages of at
  most 100; `search` returns at most 50 hits whose snippet is an **array of `{text, marked}` segments, never HTML**.
  Health rows carry what would resolve them as a **code** (`link-to-it`, `retarget-or-remove`, …) from one list shared
  with the pages (`Resolve`), so the words and the codes cannot disagree. `changes` takes `view`, `namespace`,
  `automated` (`show`, `hide`, `only`), `limit` and `cursor`, checks every change against the principal exactly as the
  changes page does, and says how many it left out (`left_out`).
- **People are what the message says.** Every row that has people carries `provenance` (`kind`, `proposed_by`,
  `accepted_by`; `not-recorded` for a message not in the service's form, which names nobody) and `people`, the sentences
  the pages say, produced by the pages' own functions; no member says a person signed.
- **Cursors.** A cursor of one list is refused by every other (`invalid-cursor`): paths for pages, the change number for a
  history, an offset stamped with the tip and the list for backlinks and health (a cursor does not outlive its tip), and a
  hash of the last change for the changes (a cursor never carries a block id).
- **One new problem code, `change-not-found` (404)**, for an id that is not a change of the view (and for the diff
  of the first change, which has nothing before it); the registry has 15 codes. `not-available` stays, for the list's
  `tag` and `changed_since` filters. Three catalog messages (English and Japanese, glossary rows).
- **JSON answers write `<`, `>` and `&` as `\u003c`, `\u003e` and `\u0026`** (the same document to any JSON reader), so no
  markup of an author's is markup on the wire in any endpoint.

Handoff 003 T5 (history, change, difference and permalinks; RFC 003 D-4).

- **A page has a past you can read.** `/<wiki>/-/history/<Title>` lists the changes that touched the page, newest
  first, each with the change's number, its summary, what it did to the page (created, edited, removed, moved from or
  to another title — a rename is an event, and the history goes on under the old title), who proposed and who accepted
  it as the change's message records them (CT-03; a message in another form names nobody and is said to), and flags
  (an undo, an import, made by the wiki service, the first change). `/<wiki>/-/change/<block>` says *Signed into the
  history* and, for a change that came from a proposal, *signed by the wiki service on behalf of …, accepted by …* —
  never that a person signed — and lists what the change did (pages by title; attachments and other files unnamed).
  `/<wiki>/-/diff/<from>/<to>/<Title>` shows what changed on the page between two changes with `ins` and `del` that
  carry a text alternative, a text summary, and the changed words marked by inklkunn (labelled as inklkunn's; the
  lines are prikk's, and writing the marked model back gives prikk's hunk byte for byte).
  `/<wiki>/-/at/<block>/<Title>` shows the page as it was after a change, with a banner that links to the current page
  and no proposal button. A view other than the default is named by `?view=`. There is no time on any of these pages:
  prikk's log carries none. Every address opens with the wiki's read check, so a private wiki and an absent one give
  the same answer.
- **Bounded reads.** prikk's `log` lists a view's changes and nothing about paths, so a page's history is the view's
  log, read once per tip and held (at most 10,000 changes), with each change's `show` kept by block id for good and
  filtered to the page's path; a request makes at most 24 `show`s it did not already hold and inspects at most 400
  changes, and when it stops early it says where to go on. A difference is exactly one `diff`, always `--path <the
  page>`, between two changes of the view's history; a whole-tree diff is never asked for. A permalink reads the page's text at the block (one `cat`) and decides
  which links lead to pages from the titles of the view's tip, because an index of every block a reader names would
  cost an extract each; its render is held in memory only. A change too large for prikk's report to fit the server's
  output bound is listed as such, never skipped. *API:* `ReadService::{page_history, change, page_diff, page_at}` and
  the types of `read::history`/`read::linediff`; `forget_wiki` also forgets the log.

Handoff 003 T7 (RFC 003 D-11, D-12; hold notes IS-8, IS-9, IS-13).

- **Handoff 003 T7: `inklkunn wiki create`, `import-folder` and `export` in the binary.** `wiki create <slug> --name
  <name>` is *create wiki* with the real signer and key store (`--license`, `--lang`, `--visibility` optional); it
  makes the deployment's store and key store when it is the first act. `wiki import-folder <wiki> <dir>` commits a
  folder (`pages/**/*.md`, `attachments/**`) as **one accepted change under the system principal** (`Kind: import`,
  `Imported-From: folder sha256:<digest of the files>`) through `Accept`: the plan is the dry run (`--dry-run` prints
  the same report and writes nothing), it walks the folder without following a link, and **any refusal refuses the
  whole import** with every reason named and no content repeated: a link, a device or a pipe, a name that is not UTF-8
  or not ASCII, a path over prikk's 1024- and 255-byte caps, a name a repository refuses (the signer's own rule), a
  page that is not UTF-8 text or whose frontmatter the parser refuses, a duplicate page id, two paths one
  case-insensitive file system cannot tell apart, a path the wiki already has (`--replace-existing` replaces it).
  `wiki export <wiki> [--bundle <dir>] [--archive <file>]` writes a Portable Wiki Bundle at fidelity level 3 (the
  files of one tip as stored, extracted by block id; `history/<page id>.jsonl` from `history()` and `change()` with
  the message's people as authors; `users.yaml`; `wiki-bundle.yaml` with the fidelity, the real counts, the block it
  is as of, the interwiki map and the SHA-256 of every other file) and/or prikk's own archive. A bundle is built
  beside its destination and renamed into place only when complete; the destination is never overwritten. Every one
  of these acts is audited (`system:operator-cli`; begun and ended). *Not built:* offering the exports from `-/admin`
  as downloads from the content origin, which needs sign-in pages that do not exist yet; `users.yaml`'s
  pseudonymization option.
- **R6 (handoff 003 corrections): the bundle manifest follows wiki-commons.** `checksums` is now the *name* of a
  `SHA256SUMS` file (`checksums: SHA256SUMS`), written beside the manifest, with one `"<sha256>  <path>"` line for
  every other file of the bundle, in path order (`sha256sum -c` reads it); the object that held the sums is gone. A
  path with a line break is refused rather than written into the file. `inklkunn_core::bundle::ExportFacts` loses
  its `checksums` field; `bundle::CHECKSUMS` and `bundle::sha256sums` are new. Every manifest the code writes (the
  creation manifest, an export's, and a real wiki's export) is validated in tests against wiki-commons'
  `schemas/bundle-manifest.schema.json` (a copy under `crates/inklkunn-core/tests/fixtures/`, wiki-commons 0.4.0, its
  commit recorded beside it), by a small checker of the tests' own that fails on any schema keyword it does not
  apply (no JSON Schema crate is in the lock). `import-folder` lists `SHA256SUMS` among the entries it does not read.
- **R8 (handoff 003 corrections): the export sweeps, never replaces, and counts.** An export begins by removing what
  a killed one left: `export-<pid>-<nanos>` in the wiki's scratch directory and `.<name>.partial-<pid>-<nanos>`
  beside the destination, only when the process that made it has ended. The finished bundle is moved in over an empty
  directory the export has just made itself with `mkdir`, so a name taken meanwhile (even by an empty directory) is
  refused and left alone; `std` has no `RENAME_NOREPLACE`, so a window remains in which another process removes that
  empty directory and makes its own. Pages whose history has no file because the page id cannot name one, or two pages
  share it, are counted: `export.histories_not_written` in the manifest, `Exported::histories_not_written`, and a
  clause in `inklkunn wiki export`'s report. A shared id's one mixed history file is no longer written.
- **The signer's keyless writable mode** (IS-8, IS-9). `inklkunn_signer::Maintenance`: the repository writable, **no
  key store, no secret, no key directory mounted and no seed variable**, for exactly two operations that write inside
  `.prikk/` and sign nothing. `archive export` (its own cost rule, a flat 16 MiB measured at 0.4 to 3.5 MiB on stores
  of 3 to 200 MiB, and the pinned report schema `prepo-export-report-v1`) copies the archive to the destination with
  mode 0600 after checking its size against prikk's report. The **checkpoint refresh** is a `verify` with the
  repository writable (the existing `verify` rule; 2.0 × S, 5.0 × S for one attachment, re-measured) and re-records
  prikk's checkpoint record that `extract` anchors on. Both take the wiki's write lock, so an export never runs beside
  an acceptance, a key act or another export, in this process or another (a second is told the wiki is busy). The
  binary runs the refresh at every start for each wiki whose last refresh ran under another prikk version or whose
  refresh a reader asked for, and records it in the wiki record (`record_checkpoint_refresh`) only when `verify`
  ran to a complete passing verdict. New sandbox constructor `Mounts::write_keyless_with_output`, named in the
  signer-only list of the key-reachability gate. *No new dependency crate:* `sha2` and `serde_json` are added as
  direct dependencies of `inklkunn-core` (lock +0 crates).
- **R2, R3 (handoff 003 corrections): the archive copy no longer follows a link or removes a stranger's file.** The
  archive's temporary file is opened with `O_NOFOLLOW` (and `O_NONBLOCK`), checked with `fstat` to be a regular file
  on the scratch directory's device, and measured and copied from that one handle; a link there is refused as damage
  (a compromised prikk could otherwise have inklkunn copy any file it can read into the operator's archive). A failed
  copy removes the destination only when this call created it. No new dependency (`rustix` was already the signer's).
- **Wiki creation is kill-safe** (IS-13). A creation is bracketed by a **creation mark** (`<id>.creating` beside the
  repository, made durable before the first directory, locked while the creation runs, removed after the record is
  stored), and `sweep_unfinished_creations`, run by every `wiki create` and every `serve` start, removes whole what a
  creation whose process is gone left: the sealed key record, the repository, the scratch directory, the lock file
  and the mark. A creation that has stored its record keeps the wiki and only loses its mark; a creation that is still
  running (another process) is left alone; one the sweep cannot remove whole keeps its mark and is retried at the next
  start. Proved by a `SIGKILL` inside each prikk call of the signer's part (`init`, three `key` calls, `trust`, `log`,
  `commit`, `seal`, `log`; `crates/inklkunn-signer/tests/creation_kill.rs`) and by a stop after each of the seven
  steps with the real signer and key store (`crates/inklkunn-core/tests/creation_kill.rs`). *API:* `create_wiki`
  gains `create_wiki_observed` (the same, naming each step), `CreateWikiError::Marked`, `sweep_unfinished_creations`;
  a failed creation's cleanup now removes the key record before the lock file, which stops the lock file being
  re-made after it was removed. `RootLayout` gains `creation_mark` and `creation_marks`.
- **F5** (handoff 002 corrections). Opening a key directory that holds files but no store header says so ("the
  directory holds files but no header; it is not a key store (or its header was removed)"), not "there is no key
  store … check the key directory setting"; an empty directory (the creation lock and a creator's temporary file
  aside) is still "no key store".
Handoff 003 T6 (recent changes, feeds, search, health, discovery; RFC 003 D-7 … D-10). No new external crate
(`unicode-normalization`, already locked for the markup crate, is now also a dependency of `inklkunn-core`).

- **Recent changes** at `/<wiki>/-/changes`, over the changes of all the wiki's open views (a change the views share is
  listed once): the summary, the people from the patch message (proposed by, accepted by; *made by the wiki service* for
  an import; a change sealed outside the service is listed by its first line with *the people are not known*), the pages
  it touched as links, the view, and *First seen here*. Filters by namespace (a title prefix), view, the wiki service's own
  changes (show, hide, only) and, for a feed, one page; a filtered list says so, names the filters in use and links to the
  unfiltered one. Every change is checked with `authorize(Read)` against its view and each page it touched; changes the
  reader may not read are left out and the page says so. **prikk's log carries no time**, so the time shown (and every
  Atom `updated`) is when this server first saw the change, kept in the service store (schema 7, `block_seen`; the cache's
  `clear` leaves it alone) and named so on the page. **No change is marked minor**: the patch message has no such trailer,
  so the `minor` filter exists in the query and passes everything, and the page offers no control for it.
- **Atom feeds** for the wiki, a namespace and a page (`-/changes.atom`, `?namespace=`, `?page=`): well-formed, every
  text escaped and characters XML cannot carry dropped, ids `urn:inklkunn:<wiki id>:<block id>` (stable across a rename of
  the address), absolute links, `rel=self`, newest first. The tests read them with a strict XML reader and check
  Atom's structure (RFC 4287 §4.1, §4.2), each rule with a planted violation. A wiki the reader may not read answers its feed
  exactly as a wiki that is not there.
- **Search** at `-/search?q=` over titles and aliases: **SQLite FTS5 with the trigram tokenizer** (the bundled SQLite has
  both; a test says so), one index per (wiki, tip) built from the title index in memory and replaced with the tip (store
  schema 7: `search_docs`, `search_fts`, `search_tips`). Texts are folded before they are stored and before they are
  asked (`fold`: NFKC, diacritics removed, kana voicing marks kept, lower case), so width variants, accents and case are one
  letter, and a Japanese title is found by any part of it (a query of fewer than three characters is a scan of the tip's
  texts, the same answer). Results show the title or alias that matched with the matching part in `<strong>`; the text is
  escaped and passes the page sanitizer, so an alias that is a script element is shown as its letters. When no page has the
  title searched for, the pages with similar titles come first, then *Create “…”* as a button that cannot be pressed (proposals
  arrive with RFC 004; no dead link). `-/opensearch.xml` describes the search.
- **Health lists** at `-/health` and `-/health/{wanted,orphans,dead-ends,redirects,wants-help,stale}`, from a link graph of
  the tip built on demand from one extract (every page read, parsed and its links resolved by the renderer's rules; held
  in memory per tip, never stored): wanted pages ranked by the pages that link to them, orphans, dead ends, redirects that
  lead nowhere, loop, run long or lead to a redirect, pages with status `wip`/`draft` or marked `stub`, and the pages unchanged
  longest (judged within the newest 100 changes, and said so). Each row says what would resolve it. **Backlinks** at
  `-/links/<Title>` list the pages that link to a page (or to a wanted title), each with the line of its source that makes the
  link, shown as text.
- **The stored file** at `/<wiki>/-/raw/<Title>`, byte for byte, **on the content origin only**: `text/markdown;
  charset=UTF-8`, `ETag` (the tip's block and the path), `Last-Modified` (when this server first saw the tip's block),
  conditional requests answered 304, `nosniff`, the content origin's sandbox policy, `noindex`, a `Link` to the page. The
  application origin refuses the address with the generic not-found page; a wiki that is not there, one the reader may not
  read and a title that leads nowhere are one plain-text answer. The content origin sees no session cookie on a deployment
  with its own host, so it serves the wikis anyone may read (a way to carry a signed-in reader across is an open question).
- **Discovery.** Every wiki page's head carries `canonical`, `license`, `search`, `alternate` (the wiki's feed; on a page also its
  Markdown source on the content origin and its feed) and `describedby`; a page adds `version-history` and `bookmark` (these
  two point at routes of handoff 003 T5). **schema.org JSON-LD is a document of its own** at `-/describe/<Title>`
  (`application/ld+json`, linked with `rel="describedby"`): the page policy is `default-src 'none'` with no `script-src`,
  and JSON-LD inside a page needs a `<script>` element, so the policy is not weakened for it. The site description document
  (`-/site.json`), `-/sitemap.xml` (public wikis only) and `/robots.txt` (the sitemaps of the public wikis, naming no other).
  A wiki that is not public says `noindex` in the page and in `X-Robots-Tag`.
- **Messages:** 126 catalog messages in English and Japanese (the preview), glossary rows for each, five concept rows
  (*orphan page*, *dead-end page*, *wiki health*, *first seen here*, and *wanted page*, *alias* and *the wiki service* are now in use).
  `-/changes`, `-/search`, the six lists, the index and the backlinks join the structural states in both languages.
- **Measured** (`cargo run --release -p inklkunn-core --example measure_search` and `measure_index`, 10,000 pages, prikk 0.55.0): building a
  tip's search index 20 ms (a rebuild 35 – 100 ms), an index of 8 MiB; a query through the index 0.3 ms at p95 (6 ms with twenty
  wikis' indexes in the same table), a one- or two-character query 3 ms (4 ms); the link graph's first use 1.9 s (one extract,
  every page read), then free; the first recent-changes page 1.7 s for 21 rows (one `show` of about 80 ms each, remembered
  afterwards), then 30 ms; the raw source of a page 0.13 s.

RFC 003 — corrections from the review of 2026-10-30 (R1 – R10, before 0.1.0).

- **A check that could not run never replaces a verdict** (R1; INT-6). The wiki record keeps the last verdict (*Verified* or
  *Failed*, with its time) apart from the last attempt that could not run (its time and why: busy, a limit, unavailable); a
  later verdict clears the attempt, an attempt never touches the verdict. The badge and the verification page read the
  verdict; *Could not be checked now* is said when there is no verdict yet, when the page itself is served stale, and, as a
  sentence of its own, for the attempt. **Store schema 8** adds `attempt_at` / `attempt_why`; a row that held `could-not-check`
  becomes `unknown` with the attempt kept. A check is claimed before it runs: one per wiki at a time (a concurrent request is
  told *a check is running*), at most `CONCURRENT_CHECKS` (2) at once overall, and the 60-second guard is read inside the
  claim from the record as it stands (a recorded time in the future counts as recent; an attempt the gate turned away does
  not). `VerificationState::CouldNotCheck` is gone; `WikiStore::record_attempt` and `WikiRecord::last_attempt` are new.
- **R4 — `import-folder` does not trust the folder between the plan and the run.** The plan records the device and
  inode of every directory it walked and every file it found; the run checks that each directory above a file is still
  that directory (not a link, the same inode), opens the file with `O_NOFOLLOW | O_NONBLOCK`, compares the open
  handle's identity with the plan's, and refuses a file with more than one hard link (at the plan too). A directory
  swapped for a link, a file replaced by another with the same bytes and a pipe swapped in are each refused, and the open
  never waits on a pipe. Not `openat` from a directory handle: that needs `rustix` in `inklkunn-core` (decision request
  in the review request); the open flags are spelled per architecture (x86-64, aarch64) and a test proves them. The
  signer gains `make_fifo_for_tests` (behind `test-knobs`): the tests may not spawn `mkfifo`.
- **R5 — no refusal reason carries a byte of the folder's content.** A repeated frontmatter key is reported by line and
  rule (`line 2: a key is repeated (keys are unique)`), never by the key; every reason passes through the escaping
  names get.
- **R7 — `import-folder` against the wiki.** The wiki's files are read at one tip (`wiki_state`, one extraction): a name
  that collides in case with the wiki's, a page id the wiki keeps, a page path that is not the canonical one of its
  title, an attachment over the cap of one acceptance and an overflowing sum are refused at the plan. The plan carries
  that tip and the signer refuses the change if the view has moved (`pinned_tip` of the acceptance record; the message has no
  `Base` for an import) and writes nothing. Re-importing a folder the wiki already has, byte for byte (with
  `--replace-existing`), is *nothing changed* (exit 0, no act begun), and a mixed folder writes only what differs.
- **Correction R9: the start-up sweep.** Each unfinished creation the sweep removes (or cannot remove) is one audit event
  (`creation-sweep`, `wiki:<id>`; *allowed* for a removal, *failed* for one left behind), from `serve` and from `wiki create`
  alike. The sweep removes nothing when the store was made by this start (`wiki create` or `serve` found no `store.sqlite`), and
  nothing from a repository that already holds a sealed block (asked through the driver's `branch list` and `log`): both are
  reported as left behind, with the mark kept, for the operator. `sweep_unfinished_creations` takes the `Storage` and a
  `store_is_new` flag.
- **Correction R10: hardening and small points.** An image source is refused when it holds a backslash or any control
  character (tab, CR, LF among them) anywhere, so `/\host` and `/<tab>/host` no longer pass as paths. The content origin's
  `Cross-Origin-Resource-Policy` is `same-site` (nothing of ours embeds it: the page policy's `img-src` is `'self'` and the
  raw source is a link). The discovery documents (`site.json`, JSON-LD) write `<`, `>` and `&` as JSON escapes through the API's
  helper. The page body's heading and block ids are prefixed `h-` (`RenderOptions::id_prefix`), with the contents list, the API's
  `toc` and every link to one, so a heading "Main" cannot take the template's `main`; the derived cache is keyed by the prefix,
  so rows drawn without it are drawn again. Markdown on the line right after a closing `:::` renders as Markdown. A permalink's
  banner says that its links lead to the pages as they are now (`at.links`). The markup crate's scaling tests believe a ratio
  over the bound only when five measurements in a row are over it.

Follow-ups from the review of 2026-10-31 (handoff 003, S2 – S4).

- **S2 — the export's history count has a unit test of its shared-id branch.** The rule (a history file is written only for a
  safe id that exactly one page of the tip carries) is now one function, `history_file_allowed`, tested for a shared id, an id no
  page carries, and unsafe ids; a repository written by prikk directly can still hold two pages with one id.
- **S3 — the manifest uses wiki-commons' key for the export time.** `exported_at` is an RFC 3339 date-time (`inklkunn_store::record::rfc3339`);
  inklkunn's own export facts (`as_of_block`, `history_changes_without_a_page`, `histories_not_written`) are under the manifest's
  `inklkunn:` key, and the top-level `export:` is gone. The schema check in the tests now checks `date-time`.
- **S4 — the start-up sweep audits before it removes.** `sweep_unfinished_creations` takes a `SweepAudit`: `before` is asked ahead of each
  removal and a refusal (the trail cannot take the event) leaves the creation in place with its mark; `after` records the outcome
  (*allowed* when removed, *failed* when left behind). `NoSweepAudit` is for tests of the sweep itself.
- **S1 (follow-up of the review of handoff 003's corrections): `rustix` in core; the import walk anchored at one
  handle; the export's move with `RENAME_NOREPLACE`.** `rustix` 1 (`fs`, `std`, no default features, as the signer declares
  it) is a direct dependency of `inklkunn-core` (no crate added to the lock; a row in `docs/dependency-policy.md`, the
  inventory regenerated). `import-folder` opens the folder the operator named once (a link there is followed, as before, and its
  identity recorded), then opens every directory with `openat` relative to its parent's handle (`O_DIRECTORY | O_NOFOLLOW`),
  lists it through its own handle, and opens every file the same way (`O_NOFOLLOW | O_NONBLOCK`) and examines it with `fstat`
  on the handle; at the run each planned file is reached again from a fresh handle on the folder, component by component, each
  directory checked against the identity the plan recorded, the file against its identity and link count. A directory swapped
  for a link or another directory while the walk is in it can no longer redirect the walk. The inode pins stay as the second
  line; the reasons are the same, except that a file swapped for a link now says it is a symbolic link. The hand-written open
  flags and the `compile_error!` for two architectures are gone. The export puts the finished bundle in place with one
  `renameat2(RENAME_NOREPLACE)` instead of a `mkdir` and a `rename`: an existing destination, an empty directory included,
  is refused by the kernel in the same step, so the race with a process that makes the destination meanwhile is closed, not
  narrowed.
