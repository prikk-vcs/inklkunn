# inklkunn — Requirements Specification

| | |
|---|---|
| Document | inklkunn Requirements: what the wiki engine must do, must never do, and what remains to decide. The contract every design must satisfy. |
| Version | v0.1 — **approved by the owner 2026-10-27 ("Approved.") and baselined**; the open questions of §16 carry the architect's recommendations as the working answers unless the owner rules otherwise (workflow §5, Phase 1) |
| Date | 2026-10-25 |
| Basis | The requirements analysis (`.git-exclude/specs/requirements-analysis/`): the main transcript (the five principles, the sixteen architectural constraints, the twenty-item issues register, the phased integration with a forge) and the sub transcript (why people give up on wikis; embeddability; a live layer over a durable layer). The roadmap (`ROADMAP.md`) and the owner's rulings DR-1 … DR-8 of 2026-10-24. wiki-commons 0.4 (the pattern language and Part III, cited by identifier). prikk 0.54.1 as its reference pages document it. planeter 0.4.0's design set as the embedding target and the house precedent. The project rules (`.git-exclude/rules/`). |
| Not | a design, an API, a schema, or code. Where a decision belongs to the owner, it is named in §16 and left there. |
| Certainty | A statement is a requirement unless marked *fact* (verified against a source), *assumption*, or *recommendation*. Owner rulings are cited as DR-n. Register items are the main transcript's twenty-item issues register; constraints C-n are §14. |
| ID scheme | `PU-` purpose · `NG-` non-goal · `US-` users and use cases · `CM-` content model · `RD-` reading · `PR-` proposing, review and acceptance · `KN-` knowledge layer · `GV-` governance and moderation · `ID-` identity, keys and signing · `IX-` interchange and API · `MD-` the planeter module · `SEC-` security · `PRV-` privacy · `INC-` inclusion (accessibility, language, words) · `PERF-` performance and scale · `DUR-` durability and recovery · `OPS-` operations · `INT-` prikk integration · `BN-` boundaries · `C-` architectural constraint · `UD-` upstream dependency · `OQ-` open question · `AC-` acceptance criteria |

**The thesis, in one sentence:** knowledge is a set of revisable claims, and a community is a system for
revising them together; the version-control system beneath is the substrate of that social system and
never its user interface.

---

## 0. What a reader must hold before the requirements make sense

**(a) prikk gives per-repository primitives, not a wiki** (*facts*, prikk 0.54.1 reference pages):

| prikk provides (inklkunn consumes) | prikk does not provide (inklkunn builds) |
|---|---|
| A signed, sealed, offline-verifiable history of **file-level** operations: create, delete, text edit by content-anchored span, binary replace, mode change, declared rename | Pages, links, rendering, search, discussion, or any notion of a wiki |
| Branches and tags; a merge that adopts patches verbatim when proven confluent; a machine-readable read surface (`log`, `show`, `tree`, `cat`, `diff`, `extract`, `verify`, `branch`, `tag`, `status`, all `--format json`) | **Content-level merge.** A conflicting pair is refused with a witness, never resolved; same-file text transforms are deferred in prikk; automatic conflict resolution is refused by prikk's design |
| Author and maintainer Ed25519 signatures; repository-local maintainer trust; a signed commit message | Accounts, sessions, authorization, or identity for people who hold no keys |
| Artifact exchange (`bundle`, `sync`, `archive`) | Network transport, multi-repository hosting, a web surface |
| One worktree and one active queue per repository; fail-fast local locks | Multi-writer concurrency: a wiki's many proposers are serialized above prikk |
| Append-only history: no object deletion, no ref deletion (a branch closes, it is never removed) | Erasure: a wiki's duty to remove content needs a procedure above prikk |

**(b) The owner ruled the shape on 2026-10-24** (`ROADMAP.md` §8): wiki-first, with claims layered in
(**DR-1**); an independent service that planeter embeds at the identity, authorization and presentation
level, so planeter stays keyless (**DR-2**); prikk driven as a sandboxed subprocess over its CLI, behind a
storage trait, never linked (**DR-3**); the wiki service signs as itself, holding one author key and one
maintainer key per wiki, with the human recorded in the signed message and honest labels (**DR-4**); a
wiki repository *is* a Portable Wiki Bundle (**DR-5**); real-time co-editing deferred (**DR-6**); English
and Japanese (**DR-7**); CC BY-SA 4.0 as the default content license for public wikis (**DR-8**).

**(c) wiki-commons is the vocabulary.** Patterns (`NAV-`, `AUTH-`, `HIST-`, `COLL-`), the Portable Wiki
Markdown profile (`MKUP-`), the metadata vocabulary (`META-`), the bundle (`XFER-`), the API capabilities
(`API-`), extensibility (`EXT-`), accessibility and language (`A11Y-`, `I18N-`, `WEB-`), security and
privacy (`SEC-`, `PRIV-`, `TRUST-`), licensing (`LIC-`) and the conformance profiles are adopted, not
reinvented; inklkunn's identifiers below cite them.

**(d) planeter is the embedding target and the house precedent.** Its single default-deny `authorize()`,
its honesty vocabulary (what prikk verified against what the service accepted), its message catalog with an
owner-approved glossary, its structural accessibility tests, its dependency policy and its re-baseline
procedure are reused as patterns. planeter's own open question OQ-4, whether wikis belong in its v1, is
unruled; inklkunn is the candidate answer.

**(e) The register's P0 items are not all settled by this document.** Claim identity (item 1), the
epistemic model (2), state classes (4), patch granularity (12) and the governance bootstrap (13) are stated
here as requirements with their shape fixed and their internals left to investigation RFCs that precede
the milestone that needs them. That is deliberate: the register itself warns against freezing those models
before prototyping.

---

## 1. Purpose (PU-…)

- **PU-1 — Participation starts with a proposal.** Every change to a wiki's content is a proposal that is
  read, discussed and accepted. A newcomer's first act can be a question or a suggestion, never an edit
  that overwrites, and the system never asks a reader to understand the machinery beneath it
  (main transcript §3, §8; principle 1).
- **PU-2 — Make provenance, disagreement and governance visible.** Every sentence and every claim can be
  traced to who proposed it, what supports it, what challenges it, and who adopted it under which rule.
  Competing views coexist and are shown as such. The rules for changing knowledge are themselves visible
  and revisable (principles 2–5). **Provenance is not truth**, and the system never claims it is.
- **PU-3 — Build above prikk, over its CLI, pushing no weight down** (DR-3; prikk RFC 145). prikk stays
  the sole source of truth for history and verification; a wiki repository stays an ordinary prikk
  repository that anyone can verify offline with prikk alone.
- **PU-4 — Be an independent service that planeter embeds** (DR-2) without weakening planeter's
  guarantees, and that works on its own.
- **PU-5 — Be inclusive by construction.** Accessible (WCAG 2.2 AA, ATAG 2.0), multilingual, written in
  plain words, usable without JavaScript, respectful by default; a low barrier to legitimate participation
  *and* safeguards against abuse (register item 19; C-15).
- **PU-6 — Clean, safe, secure and robust before rich; less is more.** The default view shows little and
  depth is one step away. Nothing ships half-safe (the owner's standing design philosophy).
- **PU-7 — Portable: knowledge outlives the software** (wiki-commons PR-16). The repository is a Portable
  Wiki Bundle and a prikk repository; export is a copy and import is a commit; conformance claims are
  honest and versioned.

## 2. Non-goals (NG-…) — stated as firmly as the goals

- **NG-1 — Not a code forge and not a prikk host.** planeter is. inklkunn hosts wikis, not software
  repositories; it does not clone, push, run CI or publish packages.
- **NG-2 — Not a change to prikk.** No new prikk command, format or protocol. A needed capability is a
  named dependency (§15) and a letter through the owner, never a prikk patch inklkunn writes.
- **NG-3 — Not a truth oracle.** No field, score or label asserts truth; there is no universal numeric
  truth or reputation value; "adopted in the default view" is a community decision with a visible history,
  never a claim of fact (register items 2, 9, 10; C-12).
- **NG-4 — Not a real-time editor and not a block database in v1.** Real-time co-editing is deferred
  (DR-6). Pages are documents with block identifiers, not a tree of typed blocks with live queries
  (register item 11; DR-1).
- **NG-5 — Not the frontier by default.** No federation, no portable identity, no AI with authority in v1.
  AI assistance, if ever added, is advisory, labelled, and attributed to the human who accepted it
  (register items 16, 17; C-13; wiki-commons AUTH-14).
- **NG-6 — Not a second source of truth for history, and never a rewriter of it.** Service records hold
  review state and attribution; history is prikk's. Removal is suppression on the view plus a documented
  repository-level procedure, never a silent rewrite (register item 14; C-14).
- **NG-7 — Not a lock-in.** The bundle export, the prikk archive export, independent verification by any
  prikk, and the export of the service's own records are requirements.
- **NG-8 — Not a content-management system and not a macro engine.** No content-provided code ever
  executes; declared extensions degrade to readable text (wiki-commons EXT-3, SEC-6).
- **NG-9 — Not a multi-tenant commercial platform.** One operator runs one inklkunn with one or more
  wikis; billing and tenancy beyond per-wiki authorization are out of scope.

## 3. Users and use cases (US-…)

### 3.1 Actors

| ID | Actor | Does |
|---|---|---|
| US-A1 | **Reader** (anonymous) | reads pages, history and discussion; follows links; subscribes to feeds |
| US-A2 | **Member** (signed in) | asks questions, comments, watches pages |
| US-A3 | **Proposer** | proposes changes; revises and withdraws own proposals |
| US-A4 | **Reviewer** | reads proposals, comments inline, approves or requests changes |
| US-A5 | **Maintainer** | accepts or declines proposals, moderates, creates views, is a steward of pages |
| US-A6 | **Wiki admin** | creates and configures a wiki, its policy bootstrap, roles, imports and exports |
| US-A7 | **Operator** | deploys, configures, backs up and upgrades inklkunn — outside the application |
| US-A8 | **Integrator or bot** | reads and writes through the API with scoped tokens; consumes feeds |
| US-A9 | **planeter** | the forge that embeds the wiki: shared identity, authorization alignment, presentation |

### 3.2 The participation ladder

The main transcript's ladder, **Visitor → Observer → Questioner → Contributor → Reviewer → Maintainer**, is
the product's model of growth: observation is a legitimate first step, and each rung adds one capability
(read, follow, ask, propose, review, accept). The ladder is per wiki and is **policy data** (GV-2), never
an opaque score.

### 3.3 Core use cases

- **US-1** Read a page; follow links including links to pages that do not exist yet; see what links here;
  see when the page last changed and by whom; see whether the repository verifies.
- **US-2** Ask a question about a page in its discussion without editing anything.
- **US-3** Propose a change to a page or to one section of it: a fix, new text, a source added, a wording
  change, with a reason; preview it; see what it changes; submit it; receive a URL for the proposal.
- **US-4** Follow a proposal: see what it changes, read the discussion, see who reviewed it and the
  decision; if declined, be told respectfully, with the reason and a next step.
- **US-5** Review a proposal: read the change page by page and block by block, comment inline, approve or
  request changes.
- **US-6** Accept a proposal in one action: the page updates, the change is signed into the history, the
  proposer and the acceptor are credited.
- **US-7** Undo an accepted change: propose a rollback, which goes through the same loop.
- **US-8** Ask "why is this here?": trace a sentence to its proposal, its author, its sources, the
  discussion and the decision (M3).
- **US-9** See disagreement: when a claim is contested, see the alternatives and their evidence; compare
  views (M3).
- **US-10** Fork a view: create an alternative view of a page or a set of pages, develop it, and propose its
  changes back (M3).
- **US-11** Govern: read the wiki's policy; propose a change to it; see who may do what and why (M4).
- **US-12** Moderate: hide harmful content with a reason, visibly; appeal a decision (M4).
- **US-13** Leave or arrive: export the whole wiki as a bundle or as a prikk archive; import a wiki from
  MediaWiki, BookStack or another inklkunn (export M1; import M5).
- **US-14** Integrate: read and write through an API with scoped tokens; follow changes by feed (M5).
- **US-15** Embed: a planeter user reaches a project's wiki already signed in, with the same vocabulary and
  the same trust labels (the module).

### 3.4 Why people give up on wikis, and what answers each (sub transcript)

| The voice | Requirements that answer it |
|---|---|
| "My edit was reverted a few minutes later, without a word" | PR-7 (a decline carries a reason and a respectful message), PR-8 (undo is itself a proposal, never a silent revert), PR-11 (the person is told) |
| "Unwritten local rules; I was scolded for a procedural mistake" | GV-1 (policy is readable data), INC-6 (help and conventions are pages linked from the form), PR-9 (suggestions never block) |
| "I might spoil a good article" | CM-9 (draft, work-in-progress and stub are respectable states), INC-6 (a sandbox), PR-10 (drafts), PR-1 (nothing overwrites: a proposal is read first) |
| "I am afraid of the markup; the layout broke" | PR-3 (plain text with a preview and a diff, no script needed), RD-1 (tolerant rendering), the editor as a later enhancement (R6) |
| "Formatting conventions exhaust me" | CM-8 (templates are optional starters), PR-9 (lint is a suggestion) |
| "I do not know where it belongs" | CM-3 (flat names, optional hierarchy), RD-9 (search or create), RD-6 (health views) |
| "Nobody reads what I write" | RD-7 (recent changes and feeds), PR-11 (notifications with restraint), GV-9 (the wiki measures feedback) |
| "I am stuck maintaining the page I wrote" | CM-10 (stewardship, not ownership), RD-11 (freshness and update requests) |
| "Free riders; no recognition" | RD-3 and PR-12 (attribution per change to proposer and acceptor), GV-9 (contribution visibility), PR-16 (thanks) |

## 4. Content model (CM-…)

- **CM-1 — One wiki is one prikk repository whose worktree is a Portable Wiki Bundle** (DR-5; wiki-commons
  chapter 10): `wiki-bundle.yaml` as the manifest, `pages/` with one Markdown file per page and directories
  as hierarchy, `attachments/` with sidecar metadata, and reserved directories for the knowledge and
  governance objects of CM-7. The repository is readable by any prikk at or above the floor and by any
  bundle importer, with no inklkunn running.
- **CM-2 — A page is a Markdown file in the Portable Wiki Markdown profile, Layer 2** (MKUP-1 … MKUP-16),
  with YAML frontmatter (META-1): `title` required; `id` a UUID (version 7) assigned at creation and stable
  under rename (NAV-9, META-2); `aliases`; `tags`; `kind` ∈ {page, category, template, help, system,
  redirect, discussion} (META-7); `status` ∈ {draft, wip, stable, deprecated, archived} (META-6); `lang`
  (BCP 47, META-10); `license` (SPDX, overriding the wiki default, META-9); `maintainers` (stewards,
  COLL-11); `description`; `visibility` as an advisory signal (META-13). Unknown keys are preserved
  (META-11). Frontmatter is content: versioned, diffed and attributed like the body (META-14).
- **CM-3 — Names and links.** A page is addressable by title and by id. Free links take the profile's
  forms, `[[Title]]`, `[[Title|label]]`, `[[Title#Heading]]`, `[[Title#^block]]`, `[[ns/Title]]`,
  `[[prefix:Title]]` (MKUP-4). Names are flat with optional `/` hierarchy (NAV-6, MKUP-7). Resolution
  follows a documented, tolerant order: exact title, alias, case-insensitive, normalized, hierarchy
  fallback (MKUP-5). Titles are Unicode, NFC-normalized, and readable in every script in URLs
  (I18N-1, I18N-2).
- **CM-4 — Dangling links are saved, rendered distinctly, and lead to creation** (NAV-3), never by color
  alone. Backlinks are computed, never maintained by hand (NAV-4). Redirects and aliases keep old titles
  working across renames (NAV-8, HIST-6).
- **CM-5 — Block identifiers.** A paragraph, list item, quotation, table or code block may carry a trailing
  `^id` (MKUP-9); ids are unique within a page and stable over time; claims, inline comments and provenance
  anchor to `[[Page#^id]]` (NAV-10). The service assigns an id to a block that a claim or comment attaches
  to, as part of the accepted change, and never silently changes an existing id.
- **CM-6 — Attachments** live under `attachments/` with sidecar metadata (filename, media type, SHA-256,
  size, alt text, license, attribution, uploaded by and when; XFER-4), are referenced relatively,
  deduplicated by hash, bounded in size (PERF-4), and never hotlinked from remote hosts by default
  (AUTH-7, PRIV-4).
- **CM-7 — State classes** (register item 4). **VCS-authoritative** (in the repository, signed, sealed,
  exported): pages, attachments, the manifest, knowledge objects (claims, evidence, decisions, under
  `knowledge/`) and governance objects (policies, roles, under `governance/`). **Service-authoritative,
  append-only** (in the service's store, exported as the bundle's discussions and as records): proposals'
  drafts and review state, comments and decisions on proposals, moderation actions, the audit log, the
  inclusiveness metrics. **Derived or ephemeral** (never authoritative): rendered HTML, backlink and search
  indexes, sessions, notification queues. Every derived view is reconstructible from the two authoritative
  classes (C-11), and a test proves it by rebuilding with the derived store cleared.
- **CM-8 — Templates, starters and help are pages** (`kind: template | help | system`), editable and
  versioned like any other (AUTH-6, COLL-12). A template is optional starter content, never a required form.
- **CM-9 — Unfinished is a valid state.** `status: draft` (visible to its author) and `wip` or a stub
  (visible to all, marked as unfinished) are respectable; readers can find pages that want help (AUTH-4).
- **CM-10 — Shared artifact, not possession.** No page is locked to its creator; `maintainers` names
  stewards who are notified and asked to review, never owners who control (COLL-11).
- **CM-11 — Views are branches.** A view is a named branch of the wiki repository. Every wiki has a
  **default view** (its main branch), which readers see unless they choose another; an alternative view
  records where it came from (`forked_from`: view, page, revision; COLL-13). The word *canonical* is not
  used anywhere a reader can see it (register item 10).

## 5. Reading (RD-…)

- **RD-1 — Rendering.** The profile renders to semantic HTML (MKUP-12): landmarks, one `h1`, heading ids,
  the `wikilink` and `wikilink-missing` classes with `data-wiki-target`, `lang` attributes, `<time>` for
  dates, `rel="nofollow ugc"` on external links; readable without scripts (WEB-1); sanitized (SEC-2).
  Callouts, footnotes, tables, task lists, math and diagram code blocks degrade per the profile's ladder;
  an unknown directive renders its fallback (EXT-3); transclusion `![[Page]]` is depth-limited, cycle-safe
  and shows its provenance (NAV-16).
- **RD-2 — Every page shows** its title and place (NAV-11); the view it belongs to when not the default;
  created and last changed and by whom (HIST-11); its license (LIC-1); its status when not stable; the
  repository's verification state (RD-4); one-step affordances to history, discussion, "propose a change"
  and the raw source; link relations in `<head>` (`alternate` text/markdown, `canonical`, `bookmark`,
  `edit`, `version-history`, `license`, `replies`, `describedby`) and schema.org JSON-LD (META-12;
  wiki-commons chapter 11 §3).
- **RD-3 — History.** Per page, the list of changes from prikk with time, the human author (ID-4), the
  acceptor, the summary, and the minor and automated flags; a diff between any two revisions with
  word-level highlighting, accessible without color (`ins`/`del`, a text summary, keyboard navigation;
  HIST-2, A11Y-5); permalinks to a revision and to a diff (HIST-5); rename and metadata events shown as
  events (HIST-6).
- **RD-4 — Verification honesty.** Pages carry the repository's verification state in planeter's words:
  *Verified* (prikk ran its checks just now and found no problems), *Verification failed*, *Could not be
  checked now*. The state is re-derived for display and never served stale as a security display. Content
  held for review (a proposal) is never shown as part of the record (planeter RFC 003 D-5, RFC 005 D-10).
- **RD-5 — Backlinks** with their linking context on every page; unlinked mentions are optional (NAV-4).
- **RD-6 — Health views:** wanted pages, orphans, dead ends, broken redirects, pages wanting help, stale
  pages — framed as opportunities, each with the action that resolves it (NAV-15).
- **RD-7 — Recent changes** for the wiki, filterable by namespace, view, minor, automated and kind, with
  Atom feeds for the wiki, a namespace and a page (COLL-1, API-6); respects the viewer's access and says
  when results are filtered.
- **RD-8 — Watch** a page or a namespace into a personal watchlist and optional notifications (COLL-2;
  PR-11). From M2.
- **RD-9 — Search:** titles and aliases from M1, full text from M2, with tokenization that handles
  languages without word delimiters and folds diacritics and width variants (I18N-4); "search or create"
  with similar-title suggestions before creating (NAV-5); an OpenSearch description (API-7).
- **RD-10 — Raw source URL per page** (API-2): the stored file byte for byte, typed
  `text/markdown; charset=UTF-8` with the profile named, with `ETag`, `Last-Modified` and `Link` relations.
- **RD-11 — Freshness.** Created, updated and optional verified-on markers; staleness signals and update
  requests in the health views (HIST-11).
- **RD-12 — Alternative views.** When a page differs across views, the page says so and offers a
  comparison (M3; KN-5).
- **RD-13 — "Why is this here?"** From any block, the trace proposal → author → sources → discussion →
  decision (M3; KN-6).
- **RD-14 — Beginner mode is the default.** The default page shows little; history, provenance,
  verification detail and governance are one step away (PU-6; the main transcript's "beginner mode").

## 6. Proposing, review and acceptance (PR-…)

- **PR-1 — Every change to VCS-authoritative content is a proposal** (PU-1). There is no direct-edit path
  for anyone. A wiki's policy may let a role accept its own proposals (GV-1), which still leaves a proposal
  record.
- **PR-2 — A proposal is a first-class object** with a URL and a state ∈ {open, accepted, declined,
  withdrawn, superseded}. It carries the target view; one or more target pages, or a new page; the
  proposed content; a summary of what and why (AUTH-10); optional sources; the base revision it was made
  against; its author; and its own revisions, since a proposer may revise an open proposal. Approvals are
  bound to the proposal revision that was reviewed and never carry over silently (planeter RFC 005 D-5).
- **PR-3 — Proposing needs no markup knowledge and no script.** From a page or a section a reader proposes
  through a plain-text form with a server-rendered preview and a "show changes" diff before submitting
  (AUTH-1, AUTH-3, AUTH-5, WEB-1); input is accepted tolerantly (AUTH-13). Section- and block-scoped
  proposals merge independently when they do not overlap (HIST-9). A richer editor is a later progressive
  enhancement with its own accessibility and input-method review (roadmap R6; A11Y-2, I18N-7).
- **PR-4 — Proposing never requires a key.** The service signs on acceptance (DR-4; ID-3 … ID-5).
- **PR-5 — Review:** a discussion thread on the proposal; inline comments anchored to blocks with robust
  selectors (COLL-5; the W3C Web Annotation selector model); per-reviewer decisions *approve* or *request
  changes*, bound to the proposal revision; required approvals per policy (GV-1).
- **PR-6 — Acceptance is one action by an authorized role.** The service re-authors the proposal against
  the current tip of the target view if the base moved (a content-level three-way merge of each page,
  INT-7); on overlap it shows both texts and stops, discarding neither (HIST-9), and the proposer or the
  acceptor resolves by revising; then it commits and seals under the wiki's keys (INT-4), updates the page,
  and records the resulting change (block and patch ids) on the proposal. Acceptance is atomic from the
  person's view: either the change is in the history and the proposal is accepted, or nothing changed and
  the reason is named (DUR-2).
- **PR-7 — Decline and withdraw carry a reason**, visible on the proposal, worded by a template that assumes
  good faith and names the next step (COLL-7, HIST-3); the proposer is told (PR-11).
- **PR-8 — Undo is a proposal.** A rollback proposal re-applies the inverse at content level and goes
  through the same loop; history is never rewritten; the result names the change it undoes (HIST-3).
  prikk's own rollback vocabulary stays internal.
- **PR-9 — Gentle guidance.** Lint findings (a dangling link, missing alt text, a missing source, a skipped
  heading level) appear in the preview and in health views as suggestions, never as blockers (AUTH-13;
  A11Y-2 Part B).
- **PR-10 — Never lose a draft.** A proposal in progress is saved server-side as a private draft, survives
  session expiry and re-authentication, and is findable ("you have an unsent proposal") (AUTH-8, SEC-3).
- **PR-11 — Notifications with restraint.** A person is told of events that involve them (their proposal
  reviewed, accepted or declined; a reply; a mention; a watched page changed); everything else is a feed.
  Channels and digests are the person's choice. A notification never carries content the recipient could
  not read (COLL-9, PRV-5).
- **PR-12 — Attribution and authority are visible.** An accepted change shows the proposer as its author
  and the acceptor as the deciding maintainer, with the role and policy version that authorized the
  acceptance (HIST-4; main transcript, "make authority visible"); both are in the signed message (ID-4)
  and in the service's records.
- **PR-13 — Flags.** Minor and automated (bot) proposals are flagged and filterable; machine-assisted
  content is labelled and attributed to the human who accepted it (HIST-8, AUTH-14).
- **PR-14 — Quotas and rate limits** per principal and per wiki (open proposals, accepted bytes, revisions
  per proposal); a refusal names the limit and the way out (register item 7).
- **PR-15 — A proposal may touch several pages**, including a rename together with edits, and is accepted
  as one change (INT-4).
- **PR-16 — A thanks action** on an accepted proposal (COLL-7). Optional; from M2.
- **PR-17 — Per-page discussion** ("talk") is separate from proposal review, durable, attributable,
  resolvable and exportable (COLL-3, COLL-4, XFER-10); discussion kinds are distinguished (KN-7).

## 7. Knowledge layer (KN-…) — M3; RFC 005

- **KN-1 — A claim's identity is distinct from its expression.** A claim has one or more expressions
  (wordings) anchored to page blocks across views, and relations to other claims: refines, contradicts,
  supersedes. An edit to wording does not create a new claim unless the meaning changes. **The rule that
  decides when identity changes is RFC 005 part 1's, validated by prototype before part 2 is designed**
  (register item 1; C-5).
- **KN-2 — Epistemic status has several dimensions and never one score** (register item 2; NG-3):
  lifecycle (proposed, adopted, retired), community status (accepted, contested, unresolved), evidence
  state (supported, mixed, challenged), scope (the default view, a named view). *Unresolved* and *unknown*
  are legitimate states, not errors.
- **KN-3 — The kind of a statement** is recorded (register item 20): observation, interpretation,
  hypothesis, opinion, decision, policy, question — enough to present and govern each appropriately, not a
  universal ontology.
- **KN-4 — Evidence** carries a source identity, a reference and where possible an archived snapshot, an
  interpretation or context, its relation to the claim (supports or challenges, with a stated strength), and
  provenance. Evidence can be challenged, shared across claims and superseded (register item 3).
- **KN-5 — Disagreement coexists** (principle 3). A contested claim shows its alternatives and their
  evidence; the default view is a community decision with a visible history, never a truth mark; evidence
  strength is shown and equal weighting is not implied (main transcript, message 4 §6).
- **KN-6 — Provenance.** For any block, the chain proposal → author → sources → discussion → decision →
  change is derivable from prikk history and the service's records (RD-13).
- **KN-7 — Discussion kinds** (register item 5): epistemic (about a claim), proposal review, governance,
  moderation, social — each with its own visibility, retention and export rule.
- **KN-8 — Freshness.** When evidence was last reviewed is recorded, and stale evidence is surfaced
  (HIST-11; the main transcript's "knowledge freshness").
- **KN-9 — Fork kinds are named** (register item 6): a technical fork (an experimental view), an
  interpretive fork (a competing understanding) and a governance fork (different rules) are presented
  differently. Cross-wiki forks are out of v1 (NG-5; wiki-commons FED-3).

## 8. Governance and moderation (GV-…) — M4; RFC 006

- **GV-1 — Policy is data.** Each wiki has a policy, versioned under `governance/`, stating who may read,
  comment, propose, review, accept, create views, moderate and administer; how many approvals an
  acceptance needs; whether anonymous proposing is allowed; how disputes are handled. A policy change is a
  proposal under a **constitutional layer** (register item 13): the operator's platform constitution (set
  by configuration) above the wiki's constitution (changeable only by its admin role) above the wiki's
  policy (changeable by the policy's own rule) above object-level settings. The history explains why the
  rules are what they are.
- **GV-2 — The participation ladder is roles**: visitor, observer, questioner (comment), contributor
  (propose), reviewer, maintainer (accept, moderate), admin. Promotion criteria are explicit policy data
  (time, accepted proposals, a maintainer's decision). **No opaque aggregate reputation value is ever a
  prerequisite** (register item 9; C-12).
- **GV-3 — Authority is visible** (PR-12): every decision names the role and the policy version that
  authorized it.
- **GV-4 — Soft security first** (COLL-6). Revert, visibility and discussion before locks. Protection
  (review required, acceptance restricted) is per namespace or page, progressive, time-limited by default,
  and shown on the page with its reason (COLL-10).
- **GV-5 — Moderation hides, it does not erase.** A proposal's, comment's or revision's content, summary
  or author can be hidden behind a tombstone that keeps the item's existence and position and states the
  reason category and the acting role; the action is logged, reversible and appealable (HIST-12, TRUST-3).
- **GV-6 — Deletion.** Pages are soft-deleted and recoverable (HIST-7); links to them become dangling
  links, never dead ends. Permanent removal for legal or privacy reasons is a documented operator procedure
  that states its costs (prikk cannot remove an object: the repository is re-created without the content,
  and the ids and permalinks of the affected history change) and it exists before any public deployment
  (register item 14).
- **GV-7 — Adversarial resistance** (register item 7): rate limits and quotas (PR-14); progressive friction
  for new accounts set by policy; proposal-spam and sock-puppet signals surfaced to maintainers from the
  audit log, never automatic bans. The wiki must remain usable and governable under adversarial
  participation without making legitimate low-friction contribution impossible.
- **GV-8 — Appeals.** A findable path exists to contest a decline, a moderation action or a role decision
  (TRUST-3).
- **GV-9 — Inclusiveness is measured** (register item 18), from the service's records, for maintainers:
  time to first proposal, proposal abandonment rate, the share of proposals that received substantive
  feedback, first-proposal acceptance rate, review turnaround, contributor return rate, and the
  concentration of accepted changes among the most active contributors. A product-health view, never a
  per-person score shown publicly unless the wiki's policy says so.
- **GV-10 — Licensing.** A default content license per wiki (DR-8: CC BY-SA 4.0 for public wikis) is
  declared on every page, in the HTML head, the site description and the bundle manifest (LIC-1); shown at
  the moment of proposing (LIC-2); overridable per page and per attachment with SPDX identifiers (LIC-3);
  attribution is satisfied by the history and the contributors view (LIC-4); a relicensing is an event in
  history (LIC-6).

## 9. Identity, keys and signing (ID-…)

- **ID-1 — Accounts.** Local passwords (Argon2id) and OpenID Connect sign-in; sessions with CSRF
  protection; scoped, expiring, revocable tokens for the API; a display name is one free-text field in any
  script, never split or truncated (planeter RFC 012 D-6); email is private.
- **ID-2 — Pseudonymous participation is possible.** Reading is anonymous. Anonymous proposing and
  commenting are wiki policy options, off by default (OQ-1), realized with temporary pseudonyms and never
  with published addresses (PRIV-3).
- **ID-3 — Signing** (DR-4). Each wiki has its own author key and maintainer key, generated with prikk's
  key tooling, held by the service encrypted at rest under an operator-provided secret, never shared across
  wikis. The maintainer key is adopted in the repository's trust store; the service is the repository's
  sole writer (INT-2). Key rotation (a new key id adopted, the old one removed from trust) and compromise
  handling are documented procedures (SEC-4). *Fact:* prikk has no author-key revocation, so an author key
  rotates by a new key id (UD-5).
- **ID-4 — Attribution in the record.** Every patch message carries, in a stable machine-readable form,
  the proposal id, the proposer's account id and display name at the time, and the acceptor's account id.
  *Fact:* prikk stores the commit message as a signed, identity-bearing field (prikk RFC 123). The service's
  records hold the same, so attribution can be cross-checked and survives either store.
- **ID-5 — Honesty labels.** Content signed by the service is labelled as such ("signed by the wiki
  service on behalf of …"); no label implies that a person signed cryptographically.
- **ID-6 — Later opt-ins, reserved and not v1:** per-account author keys held by the service;
  bring-your-own key with client-side signing for maintainers (DR-4 options b and c).
- **ID-7 — Authorization.** One pure, default-deny `authorize(principal, action, resource)` is consulted
  on every path: web, API, feeds, search, export. Actions: read, comment, propose, review, accept,
  create-view, moderate, admin, export, import. Resources: wiki, namespace, page, view, proposal,
  discussion. A private resource denies identically whether or not it exists (planeter RFC 002).
- **ID-8 — Audit log.** Security- and governance-relevant events (sign-in, role changes, acceptances,
  moderation, key operations, exports and imports) are recorded append-only, exportable off-box, as events
  and never as content (COLL-8; wiki-commons SEC-5).
- **ID-9 — Trust vocabulary.** The wiki reuses planeter's words where the concept is the same (*Verified*,
  *Verification failed*, *Could not be checked now*, *Signed into the history*) and adds its own for the
  proposal loop (*held for review*, *accepted*, *declined*); the glossary fixes them (INC-3).

## 10. Interchange and API (IX-…)

- **IX-1 — Export a wiki as a Portable Wiki Bundle** (XFER-1 … XFER-15): pages and attachments as stored;
  history as JSON Lines derived from prikk (XFER-7) with revisions, authors, summaries, renames and
  tombstones; discussions from the service's records (XFER-10); a contributor directory with a
  pseudonymization option (XFER-8); a manifest declaring fidelity, the interwiki map and checksums
  (XFER-1, XFER-2); a dry run; incremental scope, a page or a namespace (XFER-14).
- **IX-2 — Export the repository itself** as a prikk archive and as a bundle (`prikk archive export`,
  `prikk bundle export`) for an admin (NG-7); anyone holding the repository can `prikk verify` it (PU-3).
- **IX-3 — Import a Portable Wiki Bundle** (fidelity levels 0 to 3, discussions at level 4 when present) as
  an admin action that becomes commits in the wiki's repository, with attribution carried as history
  records allow (LIC-4), an import report (XFER-13), a license compatibility check (LIC-5) and
  untrusted-input handling (SEC-5). MediaWiki and BookStack arrive through wiki-commons' tooling (M5).
- **IX-4 — API.** REST over HTTPS, described in OpenAPI (API-1). Reads: pages (enumerate, get, raw source,
  HTML fragment), history, revisions, diffs, backlinks, health views, search, recent changes, proposals,
  discussions. Writes, per authorization: propose, revise, comment, review, accept, with optimistic
  concurrency by `ETag` or base revision (API-11). Errors as problem details (RFC 9457) with stable codes;
  `Link` pagination; rate-limit headers; scopes enforced per endpoint (API-13, API-14); planeter's API
  conventions where the two overlap.
- **IX-5 — Feeds.** Atom for the recent changes of the wiki, a namespace and a page, and for proposals
  (API-6).
- **IX-6 — Discovery.** A site description document (API-1); link relations in every page head
  (chapter 11 §3); an OpenSearch description (API-7); a sitemap and `robots.txt` for public wikis; the
  interwiki map published (API-9).
- **IX-7 — Webhooks** (later): HMAC-signed, delivered through the egress guard, SSRF-safe (API-15).
- **IX-8 — Conformance.** A `wiki-commons.yaml` self-assessment is published with each release, with
  versioned claims (`@0.4`): `core-reading` and `portable-content` from M1; `living-history`,
  `open-collaboration`, `accessible-authoring` and `many-languages` as M1 and M2 deliver them;
  `trustworthy-operation` before 1.0 (OQ-4 for the final set).
- **IX-9 — Interwiki links** `[[prefix:Title]]` resolve through a per-wiki map; links between wikis of one
  inklkunn resolve natively (NAV-13).

## 11. The planeter module (MD-…) — joint RFC 008; theme T8

- **MD-1 — Process boundary.** inklkunn runs as its own service with its own repositories, keys and threat
  model. planeter never holds inklkunn's keys and never writes to a wiki repository; the default planeter
  build stays keyless (planeter INV-2).
- **MD-2 — Shared identity.** A person signed in to planeter is recognized by inklkunn without a second
  sign-in. The mechanism (a common OpenID Connect provider, planeter-issued tokens, or a trusted-proxy
  contract) is RFC 008's to decide (OQ-2).
- **MD-3 — Authorization alignment.** A wiki attached to a planeter repository or organization inherits
  its visibility and maps planeter's roles onto wiki roles by an explicit, documented table; a standalone
  wiki uses its own policy. `authorize()` stays single and default-deny on both sides.
- **MD-4 — One product to the person.** planeter's navigation gains the wiki; the wiki keeps planeter's
  conventions (language choice carried over, the glossary's words, the same trust labels, the same
  accessibility baseline); a person never meets two vocabularies for one concept (MD-7).
- **MD-5 — Independent verification.** The wiki's repository can be registered in planeter as a read-only
  hosted repository, so *Verified* can be checked by a path the wiki does not control.
- **MD-6 — Cross-references** (the main transcript's phase 2, later): issues, changes and wiki pages link
  each other by URL first and by id later; code-to-knowledge provenance (phase 3) is out of v1.
- **MD-7 — Vocabulary alignment.** *Proposal* (a change under review) and *change* (a sealed block) mean
  the same in both products; inklkunn's glossary extends planeter's with page, view, claim, evidence,
  decision and policy.

## 12. Non-functional requirements

### 12.1 Security (SEC-…)

- **SEC-1 — A threat model is a first-class deliverable**: v0.1 with the design set; updated by any release
  touching identity, signing, the prikk boundary, rendering or inbound content; re-verified by the others
  (project rules).
- **SEC-2 — Rendering untrusted content.** An allowlist sanitizer on everything rendered; a strict
  Content Security Policy with no inline script; `nosniff`, frame protections and a `Referrer-Policy`;
  raw source, attachments and any user file served from an isolated origin (wiki-commons SEC-1, SEC-2;
  planeter STD-6).
- **SEC-3 — State changes need intent.** CSRF protection on every cookie-authenticated write;
  re-authentication for sensitive actions; a session expiry never discards a draft (wiki-commons SEC-3).
- **SEC-4 — Keys.** Generated by prikk's tooling; stored encrypted at rest under an operator secret; never
  in content, frontmatter, bundles, logs or error texts (wiki-commons SEC-8); rotation and compromise
  procedures documented; the signing component isolated from the web tier by a process or module boundary
  the design enforces.
- **SEC-5 — Untrusted input.** Imported bundles, pasted content, uploaded attachments and API bodies are
  untrusted: path traversal, archive bombs, oversize and type mismatch are refused by name (wiki-commons
  SEC-7); prikk runs sandboxed (INT-1); no content-provided code executes (NG-8).
- **SEC-6 — One egress guard** for every outbound fetch (OpenID Connect discovery, interwiki checks, link
  previews, webhooks): no loopback, private or link-local target unless allow-listed (planeter C-8).
- **SEC-7 — Bounded operations.** Every prikk call runs under a time and memory budget (the planeter
  RFC 011 pattern); page, proposal and attachment sizes are bounded (PERF-4); refusals are honest
  (*too large*, *busy*), never silence or an out-of-memory kill of the server.
- **SEC-8 — Supply chain.** Pinned, locked, audited dependencies; `cargo deny` and `cargo audit` at every
  project event; `forbid(unsafe_code)` in every crate; a license allow-list; every new dependency measured
  and reviewed before adoption (planeter's dependency policy, adopted).
- **SEC-9 — Secrets committed into pages** are carried faithfully by prikk and cannot be un-leaked; the
  risk is stated to operators; detection is optional, later (planeter T-14).
- **SEC-10 — A security policy and advisories** (`SECURITY.md`, `security.txt`; wiki-commons SEC-10).

### 12.2 Privacy (PRV-…)

- **PRV-1** No third-party trackers, fonts or CDNs; assets self-hosted; what is logged and for how long is
  stated (PRIV-1, PRIV-2).
- **PRV-2** Contributor identity: pseudonyms allowed; addresses never published; emails private and never
  exported; exports pseudonymizable with the manifest saying so (PRIV-3, XFER-8).
- **PRV-3** A person can export their own profile and contributions; account deletion replaces the name
  with a stable pseudonym and keeps history, as documented (PRIV-5).
- **PRV-4** Private wikis: search, feeds, backlinks, sitemaps and link previews respect visibility; pages
  carry `noindex`; errors are existence-neutral (PRIV-7).
- **PRV-5** Notifications do not leak content (PRIV-6).
- **PRV-6** Nothing is fetched on a reader's behalf without consent: no remote image hotlinking by
  default; embeds are click-to-load (PRIV-4).

### 12.3 Inclusion: accessibility, language, words (INC-…)

- **INC-1 — WCAG 2.2 level AA** for reading, proposing, reviewing and administering; **ATAG 2.0** Parts A
  and B for authoring; structural tests that fail the build: one `h1`, landmarks, a skip link, labels, alt
  text, `lang` and `dir`, contrast in both color schemes, visible focus, 24 px targets, reflow at 320 px,
  reduced motion, no information by color alone, accessible diffs; an accessibility statement per release
  naming what was and was not checked by people (planeter RFC 012 D-1, D-2, D-5; A11Y-1 … A11Y-11).
- **INC-2 — Language.** A message catalog with stable keys, whole sentences, named placeholders and plural
  rules; English and Japanese (DR-7); negotiation by `Accept-Language` with a switcher that needs no
  script and keeps its choice in a cookie; `<html lang>` and `dir`; logical CSS properties from the start;
  per-page `lang` and per-fragment marking; Japanese ships only after a fluent reviewer's pass (RFC 012
  D-3; I18N-3, I18N-5).
- **INC-3 — Plain words.** A glossary fixes every user-facing term, reusing planeter's where the concepts
  overlap. The VCS vocabulary (commit, seal, rebase, patch, block, ref) never appears in user-facing text
  except inside the explanations a reader opens on demand. Every error and empty state names a next step;
  no gendered words; calm tone; inclusive technical vocabulary (RFC 012 D-4, D-6; main transcript §8).
- **INC-4 — No script required** to read, propose, review or administer; enhancements are progressive and
  obey the input-method rule (I18N-7).
- **INC-5 — Performance as inclusion.** Small pages, one stylesheet, no web fonts, no third-party requests
  (WEB-4).
- **INC-6 — A newcomer path.** A sandbox page or wiki (AUTH-9); help and conventions as pages linked from
  the proposal form (COLL-12); a welcome on a first proposal and respectful templates for declines
  (COLL-7); the ladder and its criteria visible (GV-2).
- **INC-7 — Unicode everywhere.** Titles, tags, link targets and names in any script; NFC normalization;
  locale-aware sorting; explicit time zones (I18N-1, I18N-4, I18N-9).
- **INC-8 — Print and text-only readers work**; light and dark schemes with sufficient contrast (WEB-2).

### 12.4 Performance and scale (PERF-…) — provisional targets, to be calibrated by measurement in M1 and M2

- **PERF-1 — Reads do not re-run prikk per request.** A derived cache keyed by prikk's content-addressed
  ids, invalidated when a view's tip moves, never authoritative and never used for a security display
  (CM-7, RD-4). Provisional target: a cached page view under 100 ms of server time at the 95th percentile;
  a cold page under 1 s on a wiki of 10,000 pages.
- **PERF-2 — Acceptance end to end** (merge, commit, seal, cache update) under 5 s at the 95th percentile
  for a typical page on a wiki with 10,000 changes. Proposals are serialized per wiki (INT-2) and the queue
  is visible: *busy* with a wait estimate rather than silence.
- **PERF-3 — Scale targets for 1.0:** 10,000 pages and 100,000 changes per wiki; 100 concurrent readers per
  node; tens of wikis per instance; measured on a reference corpus kept in the repository (prikk's RFC 139
  corpus is the precedent).
- **PERF-4 — Bounds.** Page source 1 MiB, attachment 25 MiB and proposal 2 MiB by default, configurable by
  the operator within prikk's object bound; every prikk call under a memory budget with a per-call ceiling
  (SEC-7).

### 12.5 Durability and recovery (DUR-…)

- **DUR-1 — Authoritative state is one set**: the wiki repositories, the service store and the key store.
  It is backed up as one coherent, time-aligned, encrypted set with a rehearsed restore; `prikk verify`
  runs after every restore (wiki-commons SEC-9).
- **DUR-2 — An interrupted acceptance leaves no half-state.** An intent record is written before the
  write; recovery runs after a crash; the wiki refuses writes and still serves reads until recovered; every
  step is recorded (the planeter RFC 004 recovery pattern, adapted).
- **DUR-3 — prikk format.** The serving binary stays at or above the format its repositories carry;
  inklkunn never upgrades a repository's format without an intended operator act; re-baseline at each
  prikk release (UD-1).
- **DUR-4 — Export is part of disaster recovery.** The bundle export and the prikk archive are tested
  restores, not only features.

### 12.6 Operations (OPS-…)

- **OPS-1 — One server binary and a container image**; configuration by documented environment variables
  that `--help` lists; SQLite by default behind store traits with a PostgreSQL path; Linux (the sandbox,
  OQ-3); prerequisites named: prikk at the floor, bubblewrap, `prlimit`, and `curl` where the planeter
  pattern for confined outbound HTTPS is reused.
- **OPS-2 — Behind a TLS-terminating reverse proxy** with declared trusted proxies; a non-loopback bind is
  refused without them (planeter TB-8).
- **OPS-3 — Observability.** Structured logs that carry no content; a health endpoint; counters for
  admissions, refusals, acceptances and queue depth; metrics export later.
- **OPS-4 — Releases.** Signed bare `X.Y.Z` tags, attested artifacts, a CHANGELOG with "Upgrading" and
  "Output changes" sections, crates published as an install path; the architect proposes a cut, the owner
  authorizes (ROADMAP §5).
- **OPS-5 — Documentation.** A concise README in the project rules' structure; `docs/src` as an mdBook
  organized for new users, intermediate users (API, functional specification) and maintainers (the design
  set); the wiki documents itself where it can, its help pages being a wiki (COLL-12).
- **OPS-6 — The operator can see** which prikk version runs, which format each repository carries, and
  each wiki's verification state.

## 13. prikk integration (INT-…) and boundaries (BN-…)

- **INT-1 — The prikk CLI is driven as a sandboxed subprocess with `--format json`**; no prikk crate is
  linked (DR-3; prikk RFC 145). The driver sits behind a storage trait so that another backend, or a
  future prikk library if prikk ever rules one stable (RFC 170, not assumed), replaces it without reworking
  callers. The prikk version is pinned with a floor and checked at start; an unknown `schema_version` is a
  refusal, never a guess.
- **INT-2 — One prikk repository per wiki; inklkunn is its sole writer** under the wiki's own author and
  maintainer keys. Writes are serialized per wiki by a lease above prikk's fail-fast locks; a lock conflict
  or a stale lock is surfaced to the operator and never cleared automatically (prikk's own rule).
- **INT-3 — Reads are re-derived from prikk** (`log`, `show`, `tree`, `cat`, `diff`, `extract`, `branch`,
  `tag`, `verify`) and cached by content-addressed id; prikk is the source of truth for history and
  verification.
- **INT-4 — The write path of an acceptance:** bring the wiki's worktree to the target view's tip
  (`branch switch` when the view differs); apply the merged change to the files; declare renames with
  `prikk mv`; `prikk commit -m` with the ID-4 message under the author key; `prikk seal --allow-no-audit`
  under the maintainer key; confirm publication; update the cache. Each accepted proposal is traceable to
  exactly one patch, or to a declared set, and to the block that sealed it.
- **INT-5 — Operation mapping.** A new page is a created file; an edit is a text edit whose span prikk
  authors; a rename is a declared move; a removal is a deleted node; an attachment is a created file or a
  binary replacement; symbolic links never exist (prikk refuses them).
- **INT-6 — Verification.** `prikk verify` runs after every seal and on a schedule; a wiki whose last
  verification failed refuses acceptances and says why; results are shown per RD-4.
- **INT-7 — Content-level merge is inklkunn's.** A proposal whose base moved is re-authored by a three-way
  merge of each page's text against the current tip; overlapping changes are presented and never
  auto-resolved, consistent with prikk's own refusal; the proposal's base and the merge result are recorded
  (roadmap R2).
- **INT-8 — Views are branches.** A view is created with `branch create heads/<view> --from <ref>`; its
  changes are proposals accepted on that branch; bringing a view's changes to the default view is a prikk
  `merge` when the two are proven confluent and a content-level proposal when they are not. RFC 005 fixes
  the rule.
- **INT-9 — Gaps are named, not faked.** What prikk does not expose (blame or annotate, a patch's size) is
  shown as pending a prikk increment and asked by letter (UD-10).
- **INT-10 — The repository stays an ordinary prikk repository.** Nothing inklkunn-specific lives inside
  `.prikk/`; everything inklkunn needs is in the worktree (the bundle) or in its own store.

Boundaries:

- **BN-1 — inklkunn owns** pages and the wiki semantics, proposals and review, the knowledge and
  governance layers, identity above prikk, the web and the API, and the honest mapping of its decisions onto
  prikk's signatures.
- **BN-2 — prikk owns the repository**: history, signatures, verification, patch algebra and artifact
  formats. inklkunn drives these; it does not redefine them (NG-2).
- **BN-3 — planeter owns the forge**: software repositories, forge identity, and the read-only hosting of a
  wiki's repository when it is registered there (MD-5).
- **BN-4 — wiki-commons owns the vocabulary and the interchange conventions.** inklkunn adopts them and
  reports what it learns back (an engine profile, corpus cases) instead of diverging silently.
- **BN-5 — inklkunn never pushes weight into prikk or planeter.** A capability that cannot be built
  without changing either is refused or deferred, and the reason is recorded.

## 14. Architectural constraints (C-…) — binding unless revised by an approved decision

Carried verbatim in substance from the requirements analysis (the Role & System Frame, §4):

1. **C-1** The VCS is the substrate, not the user interface or the social model; ordinary users never need
   to understand commits, branches, merges or other VCS mechanics.
2. **C-2** Knowledge is modeled as revisable claims; claims, evidence, proposals, discussions, decisions,
   policies, provenance and views remain explicit domain concepts.
3. **C-3** Proposals and patches are distinct: a proposal is a human and social operation; a VCS patch is
   the durable state transition that implements an adopted change.
4. **C-4** Rendered pages and other views are projections, not the authoritative knowledge state.
5. **C-5** Claim identity is stable and independent of page location or presentation.
6. **C-6** Disagreement is representable without premature destructive resolution.
7. **C-7** Governance and consequential authority are explicit, inspectable, and versionable where
   appropriate.
8. **C-8** Authorization is enforced below the UI layer.
9. **C-9** Provenance and attribution are first-class integrity requirements.
10. **C-10** The core remains domain-neutral and does not make wiki pages or hierarchical documents its
    fundamental storage abstraction. *Reading under DR-1:* pages are the stored artifact of the first
    application, held in the bundle layout; the domain model names claims, evidence, views and decisions as
    objects of their own, so a later application is a new projection, not a rewrite.
11. **C-11** Derived indexes, caches and projections are reconstructible from authoritative state.
12. **C-12** The architecture does not depend on a universal numeric truth or reputation score.
13. **C-13** AI may assist knowledge formation but never silently becomes an authority over truth,
    governance or durable knowledge state.
14. **C-14** Privacy, moderation, deletion, legal constraints and historical integrity are potentially
    conflicting requirements; immutable history is not treated as absolute.
15. **C-15** Inclusiveness means low barriers to legitimate participation while retaining effective
    safeguards against abuse and authority capture.
16. **C-16** Implementation technology and deployment decomposition are not frozen prematurely where
    architectural evidence is still required.

## 15. Upstream dependencies (UD-…) — named so nothing assumes them

| ID | Dependency | State (facts, 2026-10-25) | What it binds |
|---|---|---|---|
| **UD-1** | prikk ≥ 0.54: `extract` (files at a point, with the claim's parent), `diff`, `tree`, `cat`, `log` and `show` JSON, the `show <block-id>` fix | **satisfied**; the M1 floor; re-baselined at each prikk release | RD-3, INT-3 |
| **UD-2** | prikk as an in-process library | RFC 170 is an unscheduled investigation; **not assumed**; the storage trait keeps the door open | INT-1 |
| **UD-3** | prikk's one worktree and one active queue per repository; a commit for a different ref is refused while the queue holds another ref's patches | constraint: writes serialized per wiki; seal follows each commit; views by `branch switch` | PR-6, INT-4, INT-8 |
| **UD-4** | prikk's same-file text transforms (deferred) and conflict refusal | content-level merge is inklkunn's | INT-7 |
| **UD-5** | prikk author-key revocation and rotation | absent; rotation is by a new key id and is documented | ID-3, SEC-4 |
| **UD-6** | prikk object and ref deletion | absent by prikk's design; removal is a procedure above prikk | GV-6 |
| **UD-7** | a planeter module mechanism and shared identity | absent today; the joint RFC 008; standalone until then | MD-1 … MD-5 |
| **UD-8** | planeter's ruling on OQ-4 | unruled | theme T8 |
| **UD-9** | wiki-commons identifier stability before its 1.0 | working draft; claims are versioned | IX-8 |
| **UD-10** | prikk blame or annotate | absent, unscheduled | INT-9 |

## 16. Open questions for the owner (OQ-…)

Each names what it changes downstream and the architect's recommendation; none blocks Phase 2.

- **OQ-1 — Anonymous proposing and commenting.** *Recommendation:* off by default, enabled per wiki by
  policy with temporary pseudonyms. Downstream: ID-2, GV-1, GV-7.
- **OQ-2 — Shared identity with planeter: which side is the identity provider?** *Recommendation:* a
  common OpenID Connect provider first (both products already speak it as clients); planeter-issued tokens
  as RFC 008's design question. Downstream: MD-2.
- **OQ-3 — Linux-only for the server.** *Recommendation:* yes, as planeter, because the sandbox is
  bubblewrap. Downstream: OPS-1.
- **OQ-4 — Which wiki-commons conformance profiles to claim at 1.0.** *Recommendation:* `core-reading`,
  `portable-content`, `living-history`, `open-collaboration`, `accessible-authoring`, `many-languages`,
  `trustworthy-operation`, `block-bridge`; `graceful-extension` and `connected-wiki` as achieved.
  Downstream: IX-8.
- **OQ-5 — A wiki across several repositories, one per namespace, as the scale lever.** *Recommendation:*
  one repository per wiki in v1; the content model keeps the lever open (namespaces are directories).
  Downstream: CM-1, PERF-3.
- **OQ-6 — Structured data and queries in v1.** *Recommendation:* frontmatter `properties` preserved and
  shown (META-8, META-11); no query language in v1 (wiki-commons open question Q3). Downstream: CM-2, RD-1.

## 17. Acceptance criteria by milestone (AC-…)

- **AC-M0 — Foundations.** The design set exists (this specification approved; external design; threat
  model v0.1; internal design). RFCs 001, 002 and 010 are accepted with handoffs. The workspace builds with
  its gates (fmt, clippy with warnings denied, tests, deny, audit, the layering gate, `forbid(unsafe_code)`).
  The prikk driver spike runs the read verbs against a real repository at the floor and refuses an older
  version. The content-merge spike measures how often prikk's span anchors survive a moved base and records
  the result (roadmap R2).
- **AC-M1 — Read a wiki.** A repository laid out as a bundle renders: pages in the profile (links, dangling
  links, backlinks, redirects, transclusion, callouts, footnotes, tables); history with accessible diffs and
  permalinks; verification honesty; the raw source URL; recent changes with feeds; title and alias search;
  the health views; export as a bundle at fidelity level 3 and as a prikk archive. English and Japanese
  catalogs exist (Japanese in preview until reviewed). The structural accessibility tests pass on every
  page state. Nothing requires a script. `prikk verify` of the repository passes independently of
  inklkunn. The threat model v0.1 is re-verified.
- **AC-M2 — Propose and accept.** Sign-in works. A reader proposes on a page or a section without a script,
  with a preview and a diff, and receives a proposal page. Reviewers comment inline and decide, bound to the
  proposal revision. An acceptance updates the page, seals the change, attributes proposer and acceptor in
  the history and labels the signature honestly. A proposal whose base moved is re-authored; an overlap
  stops with both texts shown. Decline and withdraw carry reasons and notify. Drafts survive session
  expiry. Quotas refuse by name. A rollback is a proposal. A server killed during an acceptance leaves no
  half-state and recovers (a kill test). The inclusiveness metrics are computed. The threat model v0.2 covers
  the write path and the keys.
- **AC-M3 — Claims and views.** Claims with identity and expressions anchored to blocks; evidence with
  relations; the status dimensions; alternative views created, compared and proposed back; "why is this
  here?" for any block; disagreement shown without forced resolution; the export carries the knowledge
  objects.
- **AC-M4 — Governance.** Policy as data under the constitutional layers; roles and the ladder; a policy
  change through a proposal; protections visible and time-limited; moderation tombstones with an appeal
  path; soft deletion; the removal procedure documented and rehearsed.
- **AC-M5 — Interchange and API.** Bundle import with a report and a license check, including MediaWiki and
  BookStack through wiki-commons' tooling; an OpenAPI-described read and write API with scopes, ETags and
  problem details; the site description, OpenSearch and sitemap; the conformance self-assessment
  published; the threat model updated for inbound import and API writes.
- **AC-MD — The planeter module.** A planeter user reaches a project's wiki signed in; roles are mapped by
  the documented table; the vocabulary and trust labels are shared; the wiki's repository registered
  read-only in planeter verifies there; planeter's default build is still keyless (its own symbol-absence
  check passes).

## 18. Traceability

| Requirement area | Where | Source |
|---|---|---|
| Purpose and non-goals | §1 PU-1 … PU-7; §2 NG-1 … NG-9 | principles 1–5; DR-1 … DR-8; wiki-commons PR-16 |
| Users, the ladder, the frustrations | §3 | main transcript message 4 §5; sub transcript (voices) |
| Content model | §4 CM-1 … CM-11 | DR-5; wiki-commons chapters 08–10; register items 4, 10 |
| Reading | §5 RD-1 … RD-14 | wiki-commons NAV, HIST, COLL; planeter RFC 003 |
| Proposing, review, acceptance | §6 PR-1 … PR-17 | principle 1; main transcript §3, §8; wiki-commons AUTH, HIST-3, HIST-9, COLL-7, COLL-9; planeter RFC 005 |
| Knowledge layer | §7 KN-1 … KN-9 | main transcript messages 2–8; register items 1, 2, 3, 5, 6, 20 |
| Governance and moderation | §8 GV-1 … GV-10 | main transcript message 4 §2–4; register items 7, 9, 13, 14, 18, 19; wiki-commons COLL-6, COLL-10, HIST-7, HIST-12, LIC |
| Identity, keys, signing | §9 ID-1 … ID-9 | DR-4; register item 8; prikk trust model; planeter RFC 002 |
| Interchange and API | §10 IX-1 … IX-9 | wiki-commons chapters 10, 11, 16 |
| planeter module | §11 MD-1 … MD-7 | DR-2; main transcript message 10 (phases 1–4); planeter INV-2, OQ-4 |
| Security, privacy, inclusion, performance, durability, operations | §12 | wiki-commons chapters 13–15; planeter threat model, RFC 011, RFC 012; project rules |
| prikk integration and boundaries | §13 INT-1 … INT-10; BN-1 … BN-5 | DR-3; prikk RFC 145, reference pages |
| Architectural constraints | §14 C-1 … C-16 | main transcript message 20 §4 |
| Upstream dependencies | §15 UD-1 … UD-10 | prikk and planeter current state |
| Owner questions | §16 OQ-1 … OQ-6 | this document |
| Acceptance criteria | §17 | ROADMAP §3 |

*End of inklkunn Requirements v0.1. This document is the contract a design must satisfy; it contains no
architecture, API or schema. What is buildable now, with no prikk change and no further owner ruling: the
whole of M1 and M2 over prikk 0.54 and the ruled key model. What waits: the knowledge layer's internals on
RFC 005's investigation, the governance layer on RFC 006, and the planeter module on the joint RFC 008 and
planeter's OQ-4. The external design (`inklkunn-02`) designs up to those points and stops at each; the
threat model (`inklkunn-03`) substantiates DR-4 and covers the write path before M2 is built.*
