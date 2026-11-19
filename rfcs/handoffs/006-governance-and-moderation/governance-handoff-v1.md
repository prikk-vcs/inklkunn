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
