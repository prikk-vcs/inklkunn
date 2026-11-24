# Governance and moderation — Developer Handoff v1 (RFC 006; M4, 0.4.0)

| | |
|---|---|
| Document | The rules of a wiki as data, the ladder as policy, protections and restrictions, tombstones, appeals, soft deletion and retiring knowledge (RFC 006 D-1 … D-14). |
| Status | RFC 006 accepted by the owner 2026-11-19 (Q-1 – Q-5 as recommended). **G-T1 is assignable now,** after R-0 (maintenance M56 – M59) is handed back. **G-T2 onward follow G-T1's approval,** and G-T4 – G-T6 may then run in parallel. The 0.4.0 plan's dates apply: built and reviewed by 2026-12-04 |
| Basis | `rfcs/accepted/006-governance-and-moderation.md` (D-1 … D-14, the acceptance criteria 1 – 15, *How this was checked*); threat model v0.4 (C-3d – C-3g, C-14f – C-14h, C-15d – C-15f, INV-13, INV-14, RR-15); requirements `GV-1 … GV-6`, `GV-8`, `CM-7`, `PR-12`; wiki-commons `COLL-6`, `HIST-7`, `HIST-12`, `XFER-7`; `docs/releases/0.4.0-plan.md` |
| Audience | The dev team. The two lenses of handoff 003 apply unchanged, and RFC 006's *How this was checked* is the reviewer's checklist. |
| Handback | One review request per group: G-T1; G-T2 – G-T3; G-T4 – G-T6; G-T7. Each comes with the usual gates (the whole workspace on prikk 0.55.0 first, shimmed off, on 0.57.0 and on 0.58.0), a mutation table for each refusal and invariant, and every new page state in both languages |

**The rule above every task:** RFC 006 acceptance criterion 1. **A wiki with no `governance/` decides every action
exactly as 0.3.0 does.** Write that test first, from 0.3.0's authorization table, and keep it green through every task.

## G-T1 — The files, the validator, the defaults, and `authorize()` reading them (D-1 – D-5)

- **The two files:**
  - `governance/constitution.yaml` and `governance/policy.yaml`, exactly as D-1 and D-2 define them, read with the
    frontmatter's plain YAML subset rules (as `knowledge/`);
  - enumerations closed; unknown keys refused;
  - types in `inklkunn-core::governance`, with no surface.
- **The defaults** are constants in one place, each documented against the 0.3.0 behaviour it reproduces:
  `SELF_ACCEPTANCE_BELOW`, `Action::minimum_role`, and the operator's review interval.
- **Validation at acceptance** (D-4), before the signer is asked. It refuses by name:
  - a wrong shape or an unknown key;
  - the ordering rule broken;
  - a platform limit exceeded (D-3: `INKLKUNN_GOVERNANCE_ENTRY_MAX`, `INKLKUNN_GOVERNANCE_LIMIT_DAYS`, documented in
    `config.rs`'s table);
  - `governance/` in a proposal outside the default view.

  The constitution's rung and approvals govern a governance change. **The hatch** (an admin, no approvals, named in
  the record) is the one exception, and it is in the acceptance code, not in `authorize()`.
- **A file that cannot be read** in the repository (by import, or written by hand) makes every write refuse, naming
  the file and the line. Reads continue by visibility (C-3d).
- **The snapshot.** `authorize()`'s snapshot gains the policy in force and its block. It is derived per tip, rebuilt
  with the derived store cleared (a test), and loaded by the same loader, so a handler cannot pass one in.
  - An account's rung is the higher of its grant and `entry`, capped by a restriction. Restrictions arrive in G-T2;
    the cap is the identity until then.
  - The audit record of each consequential decision names the rung and the policy block.
- **The approvals warning** of D-4, computed on read. It goes on the proposal page in this task, and on the policy
  page in G-T7.
- **Store 18 → 19** is written in this task, for every table G-T2 – G-T5 need, in one migration with the upgrade test
  of acceptance criterion 14 against the 0.3.0 release binary (pin its asset by checksum, as 0.2.0's).
- **Tests:**
  - criteria 1 – 5;
  - mutations: the hatch removed; the ordering rule loosened; a platform limit ignored; the file read from a
    non-default view.

## G-T2 — Grants, `entry` and restrictions (D-6, D-7)

- **A grant or revocation** is a recorded decision: who acted, their rung, the policy block, and an optional reason.
  The promotion text, where the policy has one, is shown on the form. Who grants:
  - admins: any rung;
  - maintainers: up to `rungs.grant_up_to`.
- **Grants stay out of the repository and out of the export** (Q-1). `inklkunn account grant` stays the operator's
  way in.
- **The people page** shows the rungs, the promotion texts and the holders, to whoever may read the wiki's members.
- **A restriction** caps one account at `observer` in one wiki, with a reason and an expiry within the platform's limit:
  - it is set by the `moderate` rung;
  - a maintainer cannot restrict a maintainer or an admin;
  - it ends by itself (a controlled clock in the tests);
  - every action it refuses tells the person its reason and end, and so does their own account page.
- **Tests:** criterion 6; C-14h; a restriction that outlives its expiry (it must not).

## G-T3 — Protections (D-7)

- **A temporary protection** is a direct action of the `moderate` rung on a page or a namespace:
  - it requires review, or restricts acceptance to a rung;
  - it has a reason and an expiry within the limit;
  - it is append-only in the store, and renewing or lifting it is a new action.
- **A standing protection** is the policy's `protect` list.
- **Both are evaluated in `authorize(_, propose | accept, page)`** (C-3g). The page, the proposal form and every
  refusal show them in one display: the kind, the reason, what it requires, and the end. Where both apply, the
  stricter decides.
- **Tests:** criterion 7; a mutation that moves the check into a template; a protection whose namespace matching is
  off by one segment (it must not match `Help/x` for `Hel`).

## G-T4 — Tombstones (D-8, D-13)

- **Write the list of surfaces first,** as one table in the code that every surface's test iterates: the history row,
  the change view, a diff from or to the revision, the page as of the revision, its permalink and raw source, *Why is
  this here?*, *Compare views*, the API member for each, the proposal page, the comments and the claim thread. **A new
  surface that shows revision content and is not in the table fails a test** (INV-13), the way the route table works.
- **What can be hidden:**
  - comments and thread comments: the body or the author;
  - proposal revisions (accepted or not) and page revisions: the content, the summary or the author.

  One act covers a content wherever it appears. **Hiding an open proposal's latest revision declines it, in the same
  record.**
- **A tip's content is refused,** with the rollback offered. Its author and summary are allowed, and are replaced
  wherever the page names its last change.
- **The tombstone** shows the category, the rung and the date, never a name; the note is for the `moderate` rung only.
- **The moderation log.** Unhide restores exactly. *Show hidden content* is audited.
- **SECURITY.md and the moderation form** say what still holds hidden content (RR-15).
- **The export** (D-13): `suppressed: [...]` with `reason`, the hidden parts omitted, and `users.yaml` listing a person
  only for what is not hidden. It is declared in the manifest, and the bundle validates against wiki-commons' schema.
- **Tests:** criteria 8 and 13, surface by surface in HTML and in the API, and a mutation per surface.

## G-T5 — Appeals (D-9)

- *Contest this* appears on every decline, tombstone, restriction, temporary protection and grant decision, to the
  person each affects.
- An appeal is one bounded text, recorded, and visible to the appellant and the `decide_appeal` rung.
- The *appeals* page is for that rung.
- **The decision:** upheld or overturned, with a reason, by someone other than the actor. An overturn reverses the
  action as its own record.
- **Limits:** one open appeal per action per person, plus RFC 004 D-11's quotas.
- The moderation log shows only that an appeal was made, and its outcome.
- **Tests:** criterion 9; C-14g; a decision by the actor refused; an appeal by someone the action did not affect refused.

## G-T6 — Soft deletion and retiring knowledge (D-10)

- **A removed page** answers `410 Gone` with its page: when, which proposal, the reason, and *propose restoring this
  page*. That is a rollback of the change, or a proposal with the last text when later changes block the rollback.
  A restricted wiki answers as an absent wiki does, byte for byte.
- **Retiring:**
  - a claim's `lifecycle: retired` with a reason, by curation;
  - a source's new optional `lifecycle: active | retired`, with version 1 files still valid without it.
- **Replacing an unreadable knowledge file** is curation by the `moderate` rung.
- **Every 0.3.0 sentence saying these are *not offered yet*** is replaced by the offer itself (find them by the
  catalog key, and list them in the request).
- **Tests:** criteria 10 and 11, and a retired source still shown, marked, on a claim that cites it.

## G-T7 — The policy page, the policy form, and the API (D-4, D-11, D-12)

- **The policy form.** Each key is a sentence and a control, never raw YAML, and the proposal page shows the change as
  before and after sentences.
- **The policy page** shows:
  - the rules in sentences;
  - each rule's history, with its proposals;
  - the platform limits in force, and the note when a limit lowers a wiki's own value;
  - the protections in force;
  - links to the people page, the moderation log and the appeals.
- **The read API:**
  - the policy as data with its block;
  - protections;
  - the moderation log;
  - tombstones in every member that can carry hidden content;
  - the appellant's own appeals.
- **OpenAPI** says that moderation, restrictions, protections, grants and appeal decisions are web-only in 0.4.0.
- **Words:** both catalogs and the glossary tests (criterion 15). Nothing is called *official*; the rules are *the rules
  of this wiki*.
- **Tests:** criteria 2 (through the form), 12 and 15.

## Not in this handoff

The plan's section 2 *Not in 0.4.0*:
- the permanent removal procedure (P-1);
- GV-7 beyond today, GV-9, and the rest of GV-10;
- governance forks and page discussion;
- anonymous proposing;
- IS-18.

A surface that meets one of these says plainly that it is not offered, and names no form that cannot act. Readiness
(R-5: the end-to-end proof on a real prikk, the CHANGELOG lead, the report) gets its own section here when G-T7 is
approved.

## G-T1 reviewed: corrections C-1 – C-7 (2026-11-23; binding, before G-T2)

G-T1 is **Conditionally Approved** (`.git-exclude/reviewed/006-governance-g-t1-review-result.md`) and pushed. The
details and tests are in the result. In short:
- **C-1 (Medium, first): a change to the rules is its own proposal.** A proposal touching `governance/` touches nothing
  else (`governance-mixed`, refused at proposing, at each revision and under the lease). The hatch also checks it
  itself.
- **C-2:** under the lease, the rules are those of the tip the change is signed on, never cached.
- **C-3:** while the rules cannot be read, decline, ask-for-revision and supersede also refuse, unless the proposal
  touches only the governance files.
- **C-4:** a disabled account's approval does not count.
- **C-5:** a non-file at a governance path is a read fault. The frozen test pins its enum orders as literals.
- **C-6:** the policy block does not drift. The walk stops at the previous row's tip and inherits its block; the window
  is only a cold build's bound.
- **C-7:** OpenAPI names the new refusals.

**Clarifications of this handoff, from the review:**
- The repair path (an admin's change to the governance files only, while they cannot be read) is INV-14's reading of
  *every write refuses*.
- The derived row is keyed by the view's state.
- A namespace protection does not cover the page of the same name, as everywhere else, and G-T7's sentences say so.
- `entry` has no effect in a restricted wiki.

Hand back as `006-governance-g-t1-corrections-review-request.md`. **G-T2 – G-T3 start when it is approved.**

## G-T1's corrections reviewed; D-1; G-T2 – G-T3 assignable (2026-11-23)

C-1 – C-7 are **Conditionally Approved** (`.git-exclude/reviewed/006-governance-g-t1-corrections-review-result.md`) and
pushed. **G-T2 – G-T3 are assignable now.**

**D-1 (binding, beside them).** Under `governance/`, a change writes only the two files but may delete any path. So an
admin's repair can clear something that stands at or below a governance path (C-5's `not-a-file`) and write the file.
- Tests: a real-prikk repair from a directory at `governance/policy.yaml`.
- Mutations: an unknown write still refused; a deletion beside a page still `governance-mixed`.
- The CHANGELOG says the repair.

Hand back as `006-governance-g-t1-d1-review-request.md`, alone or with G-T2 – G-T3's request
(`006-governance-g-t2-g-t3-review-request.md`).

## D-1, G-T2 and G-T3 reviewed: corrections F-1 – F-7 and E-1; G-T4 – G-T6 assignable (2026-11-23)

All three are **Conditionally Approved** (`.git-exclude/reviewed/006-governance-g-t1-d1-review-result.md`,
`.git-exclude/reviewed/006-governance-g-t2-g-t3-review-result.md`) and pushed. The details and tests are in the results.
In short:
- **F-1 (Medium, first).** Under the lease, `judge` refuses unless the acceptor holds `rungs.accept` (the hatch
  excepted). The refusal is `restricted` when a restriction is the cause, otherwise `not-allowed`, and it comes before
  the protection check.
- **F-2.** The in-force checks for a restriction and a protection, and the last-admin guard, are one transaction with
  their write, and *lift* ends every record in force.
- **F-3.** The unreachable protection branch in `authorize()` is removed. Protections are enforced by `judge` at
  acceptance, and only acceptance is limited.
- **F-4.** A record that fails after its audit event gets a `failed` follow-up event.
- **F-5.** `is_valid_reason` checks every reason; the typed target goes in the `moderate` audit detail; the store's
  rustdoc says *title*.
- **F-6.** A test for a restricted reviewer's approval.
- **F-7.** A maintainer's grant depends on `maintainer` and `grant_up_to` alone, never on `rungs.moderate`.
- **E-1.** For `not-a-file`, the page and the problem list what stands below the path (up to 50), with the repair's
  sentence.

Hand back as `006-governance-g-t2-g-t3-corrections-review-request.md` (F-1 may come first). **G-T4 – G-T6 are
assignable now, beside the round** (one request for the group, `006-governance-g-t4-g-t6-review-request.md`). F-1 changes
`judge`: merge it before G-T5's overturns touch acceptance.

## G-T4 – G-T6 reviewed; the corrections approved; G-T7 assignable (2026-11-24)

- **F-1 – F-7 and E-1: Approved** (`.git-exclude/reviewed/006-governance-g-t2-g-t3-corrections-review-result.md`).
- **G-T5 and G-T6: Conditionally Approved. G-T4: Changes Requested**
  (`.git-exclude/reviewed/006-governance-g-t4-g-t6-review-result.md`).
- **Nothing is pushed** until G-T4's round is approved, because the commits are interleaved.

The corrections, in order of weight. The details and tests are in the result.
- **H-1 (High).** One reader in the core for page and knowledge-file text at a block. It resolves the revision that
  wrote each path and checks that revision's content mark, and every read goes through it: `at`, both diff sides, every
  proposal base, revise, `rollback_plan`, Why's attached sources. The storage read is private to it, with a test that
  fails if another module calls it. INV-13 then holds for reads as well as routes.
- **H-2 (High).** Every surface of a page change, and the export, read the hides of every revision of its proposal.
  Acceptance refuses while the latest revision's summary is hidden (`summary-hidden`).
- **H-3 – H-7.**
  - H-3: a hidden author is masked in the withdraw sentence and on the author's comments.
  - H-4: Why's attached sources go through H-1.
  - H-5: the tip check compares every path, and the form states surviving lines with their count.
  - H-6: a decline by a hide names no moderator.
  - H-7: new rollbacks write *Undo: change {number}*, and old *Undo:* summaries that quote a hidden summary are masked.
- **N-1.** The renew and lift races say `state-changed`.
- **A-1 – A-6.**
  - A-1: an overturn and its decision in one transaction.
  - A-2: an act id in V19, replacing the 2 s heuristic.
  - A-3: only a grant's latest decision can be overturned.
  - A-4: `decide_appeal` ≥ `moderate` in the ordering rule.
  - A-5: every actor of the chain is barred from deciding.
  - A-6: the open queue is read separately, plus `INKLKUNN_APPEAL_OPEN_PER_PERSON`.
- **S-1 – S-3.**
  - S-1: the 410 reads the removal only, and the way back is worked out on `/-/restore` only.
  - S-2: *unreadable* is judged from the bytes (not UTF-8, over the bound, not parsing), and `moderate` is asked again
    at acceptance.
  - S-3: the restore summary from the catalog, and a separate not-offered sentence.
- **Small items:**
  - `export_bundle` is test-only;
  - glossary rows for *appeal* and *contest this*.

Hand back as `006-governance-g-t4-g-t6-corrections-review-request.md` (H-1 and H-2 may come first).

**G-T7 is assignable now, beside the round** (`006-governance-g-t7-review-request.md`). It should read page text only
through H-1's reader once that lands. Readiness (R-5) gets its section here when G-T7 is approved.

## The corrections and G-T7 reviewed: the J round (2026-11-24; binding, before readiness)

- **Approved:** H-2 – H-7, N-1, A-1 – A-6, S-1 – S-3
  (`.git-exclude/reviewed/006-governance-g-t4-g-t6-corrections-review-result.md`).
- **G-T7: Conditionally Approved** (`.git-exclude/reviewed/006-governance-g-t7-review-result.md`).
- **H-1: Changes Requested again.**
- **Nothing is pushed** until the J round is approved.

The details and tests are in the results. In short:
- **J-1, the rule.** The text of P at X is hidden exactly when the change that last wrote P at or before X (following
  pure renames back) has its content hidden. A case that cannot be settled hides, and no bound answers *not hidden*.
  The hash comparison, the 20-file cap and the 1,000-id cap go.
- **J-2, the cost.** A lookup that does not grow with the history or the hidden changes. The recommended mechanism is a
  derived index of each block's writes and renames, built as tips advance and rebuilt after `clear_derived`. Counting
  tests run with 0, 10 and 1,000 hidden changes, and with 3 and 3,000 revisions.
- **J-3, no way around the reader.**
  - `Tip` is made only by `tip()`.
  - One point type for any other block, through which text, transclusions, extract and materialize, the knowledge and
    title indexes at a base, and diff hunks all pass the reader.
  - The scan covers `read(`, `materialize`, `with_tree`, `with_block_tree`, `diff(` and `Tip {`.
- **J-4.** The moderation form lists the later revisions that still hold most of the added lines.
- **J-5.** One source of truth for hidden content.
- **K-1 – K-3.**
  - K-1: the export masks old *Undo:* quotes.
  - K-2: `BeyondHistory` dropped.
  - K-3: `base-hidden` on revise, and `prepare()` applying an unreadable file's replacement when the bytes are unchanged.
- **G-1 – G-3.**
  - G-1: the rung and the block on appeal decisions (V19) and on the change view's *accepted by*.
  - G-2: the policy page fails closed, and one hidden governance change marks only itself.
  - G-3: G-T7's reads move to J-3's point type.

Hand back as `006-governance-h1-review-request.md`, with G-1 – G-3. **Readiness (R-5)** is assigned when it is
approved: the end-to-end proof on a real prikk, the upgrade from 0.3.0, the CHANGELOG lead, the report, the longer
kill-matrix soak with every failing log kept, and the list of Japanese messages changed after
`.git-exclude/ja-review/0.4.0-messages-since-sign-off.md`.

## The J round reviewed; the L round; readiness R-5 assigned (2026-11-24)

**The J round: Conditionally Approved** (`.git-exclude/reviewed/006-governance-h1-review-result.md`). No path serves a
hidden change's text at a block that is not a tip. **G-T4 is Conditionally Approved**, and the whole tree is pushed.

**The L round (binding; beside R-5).** The details and tests are in the result.
- **L-1 (Medium, first):** the fallback across views happens only when the block is not in the default view's log at all,
  persists a miss, caches the views list, and is capped per request.
- **L-2:** the tip check refuses a hide for a change with an unresolved operation.
- **L-3:** only *not found* means no file.
- **L-4:** the gate is per call site; `Operation.content` is removed or scanned; `rules_at` is `pub(crate)` on a typed
  point.
- **L-5:** anchors at a base are filtered.
- **L-6:** J-4 counts *maybe* and *opaque* blocks and follows renames forward.
- **L-7 – L-9:**
  - L-7: the preview asks `moderate` in the core.
  - L-8: two defaults fail closed.
  - L-9: a generation on the writes index.
- **L-10:** `titles_without_hidden` is cached, and the hide's tip check reads the tip fresh.
- **L-11:** unattributed text answers *cannot be checked yet* (503, `Retry-After`) with no tombstone.
- **L-12:** the appeal pages link the block.

Hand back as `006-governance-l-review-request.md`.

### R-5 — Readiness (the 0.4.0 plan, §4 and §5; one request, `0.4.0-readiness-review-request.md`)

**From R-5 on, the store schema (V19) is frozen.** A defect found from here is fixed without amending it, or the request
says why it must be.
1. **The end-to-end proof on a real prikk** (exit criterion 4), in the shape of 0.3.0's R-2: one scripted run, kept as a
   test, and its transcript in the request. It shows, in order:
   - an accepted policy change changes who may accept;
   - a protection is shown and expires;
   - a hidden comment and a hidden revision are not served, on every surface of the table, and the hiding is reversed;
   - an appeal is decided;
   - a page is removed and restored;
   - a claim is retired.
2. **The upgrade from 0.3.0** (exit criterion 6). The test exists. In the request, show it run against the release
   binary, and show a policy change taking effect afterwards.
3. **The readiness report.** It shows every RFC 006 acceptance criterion (1 – 15) by named test, and every item the
   threat model's §7 gains for v0.4 (C-3d – C-3g, C-14f – C-14h, C-15d – C-15f, INV-13, INV-14) by named test, with the
   gaps said.
4. **The CHANGELOG lead** (exit criterion 7): in a reader's words, what 0.4.0 adds and what it does not do. Name the
   section 2 *Not in 0.4.0* items a reader would look for: permanent removal, forks, page discussion.
5. **The longer kill-matrix soak.** A few hundred runs of `acceptance_kill` on 0.58.0, every failing log kept, and every
   failure explained. The first soak's unexplained rc 101 is either reproduced and explained or shown absent.
6. **The writes index measured** on `tools/corpus`'s long history: a first anonymous request's cost on an unwarmed
   index, the number of requests to settle, and a warmed request's cost.
7. **The Japanese list's update:** the messages added, changed or removed since
   `.git-exclude/ja-review/0.4.0-messages-since-sign-off.md` (2026-11-24, at `9b8ace1`), in the same format, as
   `.git-exclude/ja-review/0.4.0-messages-since-2026-11-24.md`.
8. **planeter 0.6.0 – 0.8.0** (RFC 010; last re-baselined at 0.5.0, PL-14): read their release notes for anything inklkunn
   relies on (the ledger's PL rows). Fold the result into the ledger, and list any letter item for the owner.
9. **The GitHub release title is the bare version** (the owner, 2026-11-24, *"Yes"*). In `.github/workflows/release.yml`,
   `gh release create … --title "${GITHUB_REF_NAME}"`, not `"inklkunn ${GITHUB_REF_NAME}"`. This matches prikk and
   planeter. The signed tag's message stays `inklkunn X.Y.Z` (`docs/RELEASING.md`). The architect has already retitled
   0.1.0 – 0.3.0 on GitHub. The rehearsal at R-8 shows the new title.
10. **The owner's review of the Japanese** (`.git-exclude/ja-review/0.4.0-messages-since-sign-off.md`, 531 new and 2
    changed): **accepted on 2026-11-24**, with these rulings.
    - The four term questions as recommended: 閲覧者, 申し立て, 非表示. 管理者 stays as written.
    - **(a) *wiki* is written `Wiki` in Latin letters in Japanese**, not ウィキ: in every catalog message (204 places, the
      0.3.0 messages included), the glossary and the Japanese parts of the docs.
      - The spelling follows the catalog's existing spacing rule for Latin words (as `Cookie`): a half-width space on each
        side (この Wiki のページ), none beside Japanese punctuation or brackets (「Wiki」, Wiki、).
      - ウィキ joins the retired renderings in `tests/catalog_checks/japanese.rs`.
      - English stays *wiki*.
    - **(b) モデレーション / モデレーター stay**, and say what they mean where a reader first meets them.
      - `policy.label_moderate` becomes *Who may moderate (hide content, restrict an account for a time, protect a
        page)*, and the Japanese モデレーションできる人（内容の非表示、アカウントの一時的な制限、ページの保護）.
      - The moderation form's and the moderation log's introductions carry the same clause.
      - Glossary §1's row for *moderate* says it.
    - The list of messages changed after 2026-11-24 (item 7) goes to the owner at R-5, as planned.

**Item 10: Approved** (2026-11-24, `.git-exclude/reviewed/006-governance-r5-item10-review-result.md`). The L round and
items 1 – 9 stand assigned as written above; no further word is needed to start them. In item 7's list, the 204 *Wiki*
messages are one line (*spelling and spacing only, per the owner's ruling*).

