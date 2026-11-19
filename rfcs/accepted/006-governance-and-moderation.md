# RFC 006 — Governance and moderation: rules a wiki can read and change, and moderation that hides without erasing

**Status.** **Accepted by the owner 2026-11-19** (*"All your recommendations are accepted."*: Q-1 – Q-5 ruled as recommended). The owner asked the architect to check each recommendation against the owner's philosophy. That check made three refinements before acceptance: grants are not exported (D-6), a tip's author and summary can be hidden but its content cannot (D-8), and a hidden author is omitted entry by entry (D-13). It also added two sentences: what a restore from the repository alone gives back (D-6), and one display for both kinds of protection (D-7). Handoff: `../handoffs/006-governance-and-moderation/governance-handoff-v1.md`. Proposed by the architect on 2026-11-19.
This is 0.4.0's R-1 (`docs/releases/0.4.0-plan.md`, authorized 2026-11-19 with P-1 – P-3). **Tracks.** ROADMAP M4
(0.4.0), theme T6, risks R8 and R9. Requirements `GV-1 … GV-6`, `GV-8`, `CM-7`, `PR-12`, `KN-2`, `C-12`; AC-M4 as the plan
amends it (P-1); external design `WEB-12`, `GATED-2`; internal design `AZ-2`, `AZ-3`, NR-6; threat model `T-3`,
`T-14`, `T-15`, `INV-4`; wiki-commons `COLL-6`, `COLL-10`, `HIST-7`, `HIST-12`, `XFER-7`. RFC 002 D-2 (*RFC 006 replaces
the fixed set with policy as data without changing this contract*), RFC 004 D-3 and D-10, and RFC 005 part 2 D-9 and
D-10, each of which left a named piece to this RFC. **Touches.**
- `inklkunn-core`: the policy reader and validator, `authorize()`'s snapshot, acceptance, moderation, appeals;
- `inklkunn-store`: moderation actions, protections, restrictions, appeals (schema 18 → 19);
- `inklkunn-web`: the surfaces;
- the read API and OpenAPI.

**Not here** (the plan, section 2):
- the permanent removal procedure (the operations RFC, P-1);
- GV-7 beyond today's limits, GV-9, and GV-10 beyond what ships;
- governance forks and page discussion;
- anonymous proposing (GATED-4, which stays off);
- IS-18.

## Summary

Today a wiki's rules are inklkunn's code. Six roles, granted by an admin, each allow a fixed list of actions, and the
acceptance rule is RFC 004's bootstrap. This RFC moves the rules into the wiki's own repository, as two small files
under `governance/`, read and changed like everything else there: by proposal, reviewed, accepted and signed. **The
default, a wiki with no such files, is exactly today's behaviour,** so nothing changes for a wiki until its people
propose a change.

The rules sit in four layers:
- **the platform constitution:** what the operator allows any wiki;
- **the wiki's constitution:** who may change the rules;
- **the wiki's policy:** who may do what;
- **object-level settings:** a page's protection.

**No policy can lock a wiki:** its admins can always change `governance/`.

Moderation gets what the requirements ask for and nothing more:
- **A tombstone** hides a comment, a thread entry, or a revision's content, summary or author. It keeps the item's
  place, says why and by which role, is logged, and can be reversed.
- **Time-limited protections and restrictions**, each with a reason shown to the people it affects.
- **An appeal** against any decline, moderation action or role decision.
- **Soft deletion:** a removed page, a retired claim or a retired source is shown as such, with a way back.

Hiding is not removal. prikk keeps every object, and the RFC says where the hidden content still exists.

## Decisions

- **D-1 — Two files, one schema each.** Plain YAML subset, with the frontmatter rules, like `knowledge/` (RFC 005
  part 2 D-1). Both are read from **the default view's tip**.
  - **`governance/constitution.yaml`** says who may change `governance/`:
    - a policy change needs a rung (default `maintainer`) and a number of approvals (default 0);
    - a constitution change needs the `admin` rung and a number of approvals (default 0).
  - **`governance/policy.yaml`** holds the rules (D-2).
  - **Both are optional.** A missing file is its default, and the defaults are today's behaviour.
  - **A file of the wrong shape can never be accepted** (D-4). One that arrives by import, or is written by hand
    outside inklkunn, makes the wiki **refuse every write** with a message that names the file and the line, while it
    still reads by its visibility. That is fail-closed, and it shows the reason (C-3d).
  - **Governance files are refused in any view but the default one**, because a governance fork is not in scope. A
    proposal in an alternative view that touches `governance/` is refused, with that sentence.
- **D-2 — What the policy says (schema version 1).** Every key is optional, and each default is today's behaviour.

  | Key | Values | Default |
  |---|---|---|
  | `entry` | the rung every signed-in account holds in this wiki without a grant: `none`, `observer`, `questioner` or `contributor`, up to the platform's limit (D-3) | `none` |
  | `rungs.comment` / `propose` / `review` / `create_view` / `accept` / `moderate` | the lowest rung for each action, under the ordering rule below | `questioner` / `contributor` / `reviewer` / `reviewer` / `maintainer` / `maintainer` |
  | `rungs.decide_appeal` | `maintainer` or `admin` | `admin` |
  | `rungs.grant_up_to` | the highest rung a maintainer may grant: `none` (admins only) up to `reviewer` | `none` |
  | `acceptance.approvals` | 0 – 5 approvals from people other than the proposer | `0` |
  | `acceptance.self` | `never`, or `small-wiki` (a maintainer may accept their own proposal while the wiki has fewer than three maintainers) | `small-wiki` |
  | `knowledge.review_after` | days (RFC 005 part 2 D-10, now per wiki) | the operator's value |
  | `promotion.<rung>` | a short text: what this wiki looks for before granting the rung | none |
  | `protect` | standing protections: a list of `{path or namespace, require: review | rung, reason}` | none |

  - **The ordering rule:** `propose` ≤ `review` ≤ `accept`, and `moderate` ≥ `reviewer`.
  - **The admin actions stay `admin`, and no policy changes them:** roles, keys, export, import and the constitution.
  - **Promotion texts are guidance, shown on the people page beside each rung.** Nothing promotes automatically, no
    count is turned into a rung, and no score exists (C-12, GV-2).
- **D-3 — The platform constitution is two settings of the operator's,** and a few rules no setting changes.
  - **The two settings:**
    - `INKLKUNN_GOVERNANCE_ENTRY_MAX` is the highest `entry` a wiki may set (default `contributor`);
    - `INKLKUNN_GOVERNANCE_LIMIT_DAYS` is the longest temporary protection or restriction (default 90).
  - **The rules no setting changes:**
    - a wiki's visibility stays the operator's;
    - the admin actions stay `admin`;
    - the hatch of D-4 exists;
    - accounts come only from the operator's invitations (DR-11).
  - **A policy that exceeds a platform limit is refused at acceptance,** naming the limit. If the operator later
    lowers a limit, a wiki above it is read as if it were at the limit, and its policy page says so.
- **D-4 — A governance change is a proposal, validated when it is accepted, and the wiki cannot lock itself.**
  - **The proposal.** Changing `governance/` is an ordinary proposal through a form, the *policy form*. It shows each
    key as a sentence (*who may accept: maintainers*), never raw YAML. The proposal page states the change in the same
    sentences: before and after.
  - **Acceptance** needs the rung and the approvals the constitution names, counted as RFC 004 counts them.
  - **What is refused at acceptance, by name:**
    - a wrong shape;
    - a key or value the schema does not know;
    - the ordering rule broken;
    - a platform limit exceeded.
  - **The hatch.** An admin of the wiki may always accept a change to `governance/`, with no approvals, whatever the
    policy and the constitution say. The acceptance record names the hatch. So no policy can leave a wiki unable to
    change its rules (INV-14).
  - **A warning, not a refusal.** A policy whose `acceptance.approvals` exceeds the number of people who could give
    them today (they hold `review` or above and are not the proposer) is accepted. The policy page and each proposal's
    page then say that *no proposal can be accepted until more people hold the rung*. Grants change, so refusing at
    acceptance would be wrong a day later.
- **D-5 — `authorize()` reads the policy; the contract does not change** (RFC 002 D-2).
  - **The snapshot.** `authorize()`'s snapshot gains the policy in force, keyed by the block that set it. It is derived
    and rebuilt from the tip, like the knowledge index (CM-7: derived, never authoritative), and it is reached through
    the same loader, so a handler still cannot assert anything about a wiki.
  - **The rung an account holds** is the higher of its grant and the wiki's `entry`, capped by any restriction (D-7).
  - **The rest stays:** default-deny, existence-neutrality, scopes intersected with rungs (AZ-3), and the audit of
    consequential decisions.
  - **Each audited decision now names the rung and the policy block** (GV-3). The proposal page's *accepted by*,
    *reviewed by* and every moderation record show both: *accepted by Name (maintainer), under the rules as of
    `<short block id>`*, with that block's own link.
- **D-6 — Grants stay in the service's store.**
  - Who holds which rung is in `role_grants`, as today: audited, never in the repository, and **not in the export**.
    Carrying roles between wikis is an interchange question, RFC 007's. The bundle names people only as authors, as it
    does today. CM-7 lists *roles* under `governance/`, and this RFC reads that as **the rungs and their rules**, not
    the names that hold them (question Q-1).
  - **A grant or a revocation is a recorded decision.** It names the person who acted, their rung and the policy
    block, and has an optional reason. Where the policy has a promotion text for the rung, the form shows it.
  - **Who grants:** admins, any rung; maintainers, up to `rungs.grant_up_to`.
  - **Grants are backed up with the store, and the repository alone does not restore them.** A wiki rebuilt from its
    repository keeps its rules but not who holds the rungs. The operator then grants again with `inklkunn account
    grant`, as at a wiki's creation, and the operations RFC's rehearsed restore treats the store and the repository as
    one set (DUR-1).
  - **The people page** lists the rungs, the promotion texts, and who holds each rung, to anyone who may read the
    wiki's members (members of an `internal` or `restricted` wiki; everyone, for a `public` one).
- **D-7 — Protections and restrictions: temporary ones are moderation, standing ones are policy** (GV-4, COLL-6;
  question Q-3).
  - **A temporary protection** of a page or a namespace is a direct action of the `moderate` rung. It requires review
    for every proposal there, or restricts acceptance there to a rung. It has a reason (shown on the page, in the
    proposal form, and on every refusal it causes) and an expiry of at most the platform's limit. It is recorded in the
    store, append-only. It ends by itself; renewing it is a new action, and lifting it early is an action too.
  - **A standing protection** is the policy's `protect` list, changed only by proposal (D-4).
  - **Both are evaluated inside `authorize(_, propose | accept, page)`** (AZ-3), never in a template.
  - **One display for both.** A page lists each protection once, with its kind, *by this wiki's rules* or
    *temporarily, until <date>*, its reason, and what it requires. Where both apply, the stricter decides and both are
    listed.
  - **A restriction** caps one account's rung in one wiki at `observer` (it may read, but not comment, propose or
    review). It is set by the `moderate` rung with a reason and an expiry within the platform's limit.
    - The person sees the reason and the end date on every action it refuses, and on their own account page.
    - A maintainer cannot restrict a maintainer or an admin; an admin can restrict a maintainer.
    - A restriction is appealable (D-9), and is recorded like a protection.
- **D-8 — Moderation hides behind a tombstone; it never erases** (GV-5, HIST-12, CM-7).
  - **What can be hidden:**
    - a proposal comment, or a claim-thread comment: its body, or its author;
    - a revision of a proposal, accepted or not: its content, its summary, or its author;
    - a revision of a page: its content, its summary (the proposal's message, as shown), or its author.

    For an accepted proposal, the proposal's revision and the page's revision are one content, so one act hides both
    and every place it is shown. **Hiding the latest revision of an open proposal also declines it,** in the same
    recorded act and with the same reason, because a proposal nobody can read cannot be reviewed.
  - **The content of a view's tip cannot be hidden;** its summary and its author can. For content, the way is to revert or remove it first, by proposal (soft security
    first). The moderation form says so, and offers the rollback.
  - **The tombstone** keeps the item's existence, position and identifier. It states a **reason category** (`personal
    data`, `harassment`, `unlawful`, `copyright`, `spam`, `other`), the acting rung and the date, never a name. The
    reason category is shown to everyone who can see the item; a free-text note is shown to the `moderate` rung only.
  - **Every surface that shows the hidden part shows the tombstone instead** (INV-13). For a revision, that is:
    - the history row;
    - the change view, and any diff from or to it (that side says *hidden*);
    - the page as of that revision, its permalink and its raw source;
    - *Why is this here?* (that wording is not quoted);
    - *Compare views*;
    - the API members for each of these.

    Search and backlinks read only the tip's content, which cannot be hidden. A hidden author or summary of the tip is
    replaced wherever the page names its last change.
  - **Seeing hidden content:** the `moderate` rung may open it with an explicit *show hidden content*, which is
    audited. Nobody else sees it through inklkunn (question Q-5).
  - **Logged and reversible.** Each hide and unhide is an append-only moderation record. The wiki's **moderation log**
    lists the item, the category, the rung and the date, never the hidden content, to whoever may read the wiki. An
    unhide restores the item exactly.
  - **What still holds the content** is said plainly on the moderation form and in SECURITY.md: the repository (prikk
    keeps every object), the operator's backups and `archive export`, and any copy made before the hide. Removal from
    those is P-1's procedure (RR-15).
- **D-9 — Appeals** (GV-8).
  - **Where.** Every decline, tombstone, restriction, temporary protection and grant decision about a person shows
    *contest this* to the person it affects: the proposer, the author, the restricted account, the person demoted.
  - **What an appeal is:**
    - one bounded text (the comment limits);
    - recorded, and visible to the appellant and the `decide_appeal` rung;
    - listed for that rung on the wiki's *appeals* page;
    - decided once, *upheld* or *overturned*, with a reason, by someone who did not take the action appealed.
  - **An overturn** reverses the action in the same step, as its own recorded action.
  - **Limits:** one open appeal per action per person, and the quotas of RFC 004 D-11, so that appeals cannot flood
    the rung that decides them.
  - **Not public.** An appeal names a person and a grievance. The moderation log says only that the action was
    appealed and the outcome.
- **D-10 — Soft deletion, and retiring knowledge** (GV-6 soft part, HIST-7; RFC 005 part 2's deferral).
  - **A removed page** (removed by an accepted proposal, as today) answers with a page that says so: when, by which
    proposal, and its reason. It offers *propose restoring this page*: a rollback of that change (RFC 004 D-8), or a
    proposal with the last text if later changes block the rollback. The status is `410 Gone` with that page, never a
    bare 404. Links to it are dangling links (they already are). A restricted wiki still answers as an absent wiki does.
  - **Retiring a claim** is a curation proposal that sets `lifecycle: retired` with a reason. The field exists (RFC 005
    part 2 D-1); acceptance stops refusing it.
    - A retired claim leaves the paragraph's *Sources and views* link.
    - Its page stays at its address with *retired* and the reason.
    - It stays in the export.
  - **Retiring a source** adds one optional key, `lifecycle: active | retired` (default `active`). The source file
    stays a valid version 1 file without it. A retired source keeps its page, is marked on every claim that still
    cites it, and leaves *Sources to review*.
  - **Replacing a knowledge file that cannot be read** (the `knowledge_invalid` state of 0.3.0) is a curation proposal
    by the `moderate` rung that writes a valid file in its place, or retires it.
  - **Every surface of 0.3.0 that said *not offered yet* about these three** now offers them.
- **D-11 — The policy page is where governance is visible** (GV-3, WEB-12). One page per wiki shows:
  - the rules in sentences;
  - each rule's history, with the proposals that made it (*why the rules are what they are*);
  - the platform limits in force;
  - the protections in force, with reasons and ends;
  - links to the people page, the moderation log and the appeals page.

  It is in both languages, and the glossary's words apply. The policy and constitution are never called *official*,
  only *the rules of this wiki*.
- **D-12 — The API reads all of this; moderation is done in the web.**
  - **The read API** gains:
    - the policy in force, as data with its block;
    - protections;
    - the moderation log;
    - tombstones in every member that can carry hidden content;
    - an appellant's own appeals.
  - **Policy changes** go through the existing propose API, as governance files.
  - **Moderation, restrictions, protections, grants and appeal decisions are web-only in 0.4.0, and OpenAPI says
    so.** They are deliberate human acts, and a scripted moderator is the automated sanction GV-7 rules out.
- **D-13 — The export carries tombstones, not hidden content** (XFER-7, HIST-12's interchange; question Q-2).
  - `history/<page>.jsonl` marks a hidden revision `suppressed: [content | summary | author]` with its `reason`, and
    omits what is hidden.
  - A hidden thread comment's `body` is omitted, with the same marker. An entry whose author is hidden names no author,
    and `users.yaml` lists a person only for what is not hidden.
  - The manifest declares this. The tip's files are unaffected, because a tip's content cannot be hidden.
- **D-14 — The upgrade is a migration and no write.**
  - The store goes 18 → 19 (moderation records, protections, restrictions, appeals), in one transaction at the first
    start, forward only, as at 0.3.0.
  - **No repository is written on upgrade:** a wiki without `governance/` runs on the defaults, which are today's rules.
  - The operator's `INKLKUNN_KNOWLEDGE_REVIEW_AFTER_DAYS` stays the default for wikis whose policy does not set
    `knowledge.review_after`.

## What "done" means (acceptance criteria)

Each one is shown by a named test, on a real prikk where it touches the repository.

1. **The defaults.** A wiki without `governance/` decides every action of RFC 002 D-2's table exactly as 0.3.0 does.
   The test is the 0.3.0 authorization table, run against the default policy.
2. **A policy change by proposal** changes who may accept, from the next decision on. The decision record names the new
   block.
3. **The refusals of D-4,** each by name:
   - a wrong shape;
   - an unknown key;
   - the ordering rule broken;
   - a platform limit exceeded;
   - a governance file in an alternative view.
4. **The hatch.** A policy that raises `approvals` beyond who could give them is accepted with the warning on both
   pages. An admin then accepts a change back with the hatch, and the record names it.
5. **A file of the wrong shape** written outside inklkunn makes the wiki refuse writes, naming the file and line,
   while it still reads.
6. **`entry`, grants up to `grant_up_to`, and a restriction** each change exactly the actions they should. A restricted
   person sees the reason and the end on a refused action.
7. **Protections.**
   - A temporary protection is shown with its reason, refuses what it should, and ends by itself at its expiry (with a
     controlled clock).
   - A standing protection does the same through the policy.
8. **Tombstones.**
   - A hidden comment, a hidden thread comment, a hidden proposal revision and a hidden page revision are absent from
     every surface D-8 lists, in HTML and in the API. The test names each surface.
   - Hiding an open proposal's latest revision declines it.
   - An unhide restores each exactly.
   - *Show hidden content* is audited.
   - Hiding a tip's content is refused, with the rollback offered; hiding its author or summary is not.
9. **Appeals.** An appeal is filed, listed and decided, and an overturn reverses the action. A second open appeal on the
   same action is refused, and so is a decision by the person who acted.
10. **Soft deletion.** A removed page answers `410` with its page, and is restored by the offered proposal. A restricted
    wiki's removed page answers as an absent wiki's would.
11. **Retiring.** A claim and a source are retired by curation, and their surfaces say so. An unreadable knowledge file
    is replaced.
12. **Authority is visible.** Every decision page shows the rung and the policy block.
13. **The export** carries the tombstones of D-13 and no hidden content, and validates against wiki-commons' schema.
14. **The upgrade.** A data directory of the 0.3.0 release binary migrates (18 → 19), and its wikis behave as before.
15. **The words.** Every new message is in both catalogs, passes the glossary tests, and calls nothing *official*.

## How this was checked against the owner's two lenses

- **Finally clean, safe and secure, robust and sophisticated.**
  - **The rules live where the content lives,** signed and versioned, and a policy that cannot be read fails closed.
    The authorization contract does not change, so the 0.3.0 table becomes acceptance criterion 1.
  - **The hatch** removes the one way such systems die, a wiki locked by its own rules, without a special path in
    `authorize()`.
  - **Hiding is one rule over one list of surfaces** (INV-13), tested surface by surface. Where hiding cannot reach, the
    design says so instead of implying erasure (RR-15).
  - **Nothing here needs prikk to change.**
- **APIs and UI that cannot confuse.**
  - **The policy form and the policy page speak in sentences**, never raw YAML.
  - **Every refusal names its rule:** the protection, the restriction or the limit, with its reason and end.
  - **Every decision names the rung and the rule version.**
  - **A tombstone says why and by which rung,** never just *removed*.
  - **A removed page explains itself** and offers the way back.
  - **Automation is left out on purpose:** moderation stays web-only (D-12), and no number becomes a rung (D-2).

## Alternatives considered

- **Grants in the repository** (a literal reading of CM-7). This would give one signed record of who may do what. But
  it publishes, for good, the names of everyone who ever held a rung. The history can never forget a name (T-15), and
  an account's deletion (PRV-3) could not reach it. Not recommended (Q-1).
- **Every protection by proposal.** This would put every rule in one place. But vandalism is answered in minutes, not
  after a review. Standing protections are policy; temporary ones are moderation (Q-3).
- **A free-form policy language** (expressions over attributes). It can say anything, and so can be misread and
  mis-evaluated. A fixed schema with sentences for each key is what a community can read. A later version of the
  schema can add keys.
- **Refusing a policy whose approvals cannot be met.** Refused instead: grants change after acceptance, so the check
  would be wrong a day later. The warning and the hatch cover it (D-4).
- **Hiding the tip's content.** Refused: a page whose current text is hidden is not a page. Reverting first is soft security, and
  it keeps one rule for every view.

## Threat model v0.4 additions (folded into `docs/src/inklkunn-03-threat-model.md` on acceptance)

- **T-3 (authorization):**
  - **C-3d:** a governance file that cannot be read makes the wiki refuse writes, failing closed with the reason;
  - **C-3e:** governance files are read only from the default view's tip, and refused in any other;
  - **C-3f:** the hatch of D-4;
  - **C-3g:** protections and restrictions are outcomes of `authorize()` (AZ-3), with each refusal naming its cause.
- **T-14 (abuse of the social system):**
  - **C-14f:** moderation is a recorded, attributed, reversible act; its log is public to the wiki's readers, and it
    is never scripted through the API (D-12);
  - **C-14g:** appeals are bounded (one open per action per person, plus quotas) and decided by someone other than the
    actor;
  - **C-14h:** a maintainer cannot restrict a maintainer or an admin.
- **T-15 (removal and the record):**
  - **C-15d:** INV-13 over D-8's list of surfaces;
  - **C-15e:** the export carries tombstones, not hidden content (D-13);
  - **C-15f:** seeing hidden content is the `moderate` rung's, and audited.
- **INV-13:** no inklkunn surface serves hidden content to a principal without the `moderate` rung.
- **INV-14:** an admin of a wiki can always change its `governance/`.
- **RR-15:** hiding is not removal. The content remains in the repository, in the operator's backups and
  `archive export`, and in any copy or clone made before the hide. Anyone given the repository itself can read it,
  including a future read-only host (MD-5). Removal from all of those is P-1's procedure, due before any public
  deployment.
- **§7** gains the tests of acceptance criteria 3 – 10 and 13, by name, at readiness.

## Questions for the owner (ruled 2026-11-19: all as recommended)

- **Q-1 — Where do grants live? In the service's store** (D-6), with CM-7's *roles* read as the rungs and their
  rules. The history can never forget a name.
- **Q-2 — Does a hidden item leave in the export? No.** The bundle carries the tombstone without the content (D-13), as
  wiki-commons' interchange says. The operator's backup still holds everything (RR-15).
- **Q-3 — Is a protection a direct action or a proposal? Both:** temporary protections are direct, logged moderation
  actions, and standing protections are policy (D-7).
- **Q-4 — What can the platform constitution forbid a wiki?** Two settings: the highest `entry` (default
  `contributor`), and the longest temporary protection or restriction (default 90 days). The rest are fixed rules, not
  settings (D-3).
- **Q-5 — Who may see hidden content through inklkunn? The `moderate` rung, explicitly and audited** (D-8). The
  alternative, admins only, puts every look at hidden content on the busiest rung.

## Sequencing and handoff

On acceptance, the architect folds the threat model additions into `docs/src/inklkunn-03-threat-model.md` (v0.4) and
writes `rfcs/handoffs/006-governance-and-moderation/governance-handoff-v1.md`. The order of the tasks is:
1. the files, the schema, the validator and the defaults, with acceptance criterion 1 first (D-1 – D-5);
2. grants, `entry` and restrictions (D-6, D-7);
3. protections (D-7);
4. tombstones over D-8's list (D-8, D-13);
5. appeals (D-9);
6. soft deletion and retiring (D-10);
7. the policy page and the API (D-11, D-12).

Tasks 4 – 6 may run in parallel once task 1 is approved. Readiness follows the plan (R-5).
