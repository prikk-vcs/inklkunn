# inklkunn — Roadmap

**Status.** **PROPOSED by the architect, 2026-10-24**, as the Phase 0 planning package (workflow §5,
Phase 0: roadmap and milestone definition). **Nothing below is agreed until the owner authorizes it.**
The decision requests in §8 are the owner's to rule; the architect's recommendation is stated under
each so that disagreement has something to push against. No RFC, design or code starts from this file
before that authorization.

How inklkunn gets from two requirements-analysis transcripts to a wiki engine people can use. This is a
**direction with milestones**, not a dated schedule: work ships when it is correct, tested, secure and
honest. The governing material is under `.git-exclude/` (the requirements analysis in `specs/`, the
project rules in `rules/`, the workflow and operating instructions in `roles/`) and in the three sibling
projects: [prikk](https://github.com/prikk-vcs/prikk), [planeter](https://github.com/prikk-vcs/planeter)
and [wiki-commons](https://github.com/kos-commons/wiki-commons).

## 1. The wide perspective (read this first)

**inklkunn is a wiki engine whose storage is a patch-theoretic version-control system, built so that
participation starts with a proposal.** The name is Norwegian, "inclusive" + "knowledge". The
requirements analysis states the thesis in five principles: every change is a proposal; every claim has
provenance; disagreement can coexist; forks are legitimate; governance is explicit. And one design rule
above all: **the VCS is the substrate of the social system, never its user interface.** People see
*suggest, explain, add a source, accept, compare views*; they never see *commit, seal, rebase*.

Three sibling projects fix the ground it stands on (verified facts, 2026-10-24):

- **prikk** (0.54.1) is the substrate: signed, block-sealed, patch-based history, offline, no network.
  The ecosystem consumes it over the CLI's machine-readable surface and never over its unstable crates
  (prikk RFC 145, ruled 2026-09-12). Library embedding is an unscheduled investigation (prikk RFC 170).
- **planeter** (0.4.0) is the forge inklkunn is expected to join as a module: layered and gated, keyless
  by construction, honest about what prikk verified, accessible and multilingual by test. Its open
  question OQ-4, whether wikis belong in its v1, has not been ruled; inklkunn is the candidate answer.
- **wiki-commons** (0.4) is the vocabulary: the pattern language (NAV, AUTH, HIST, COLL), the Portable
  Wiki Markdown profile, the Portable Wiki Bundle, the conformance profiles. inklkunn adopts it rather
  than inventing a wiki model, and can claim its profiles honestly.

**One structural fact shapes the whole plan.** prikk gives a single-writer, file-level, signed
repository. A wiki adds many readers, many proposers, content-level merge, identity for people who hold
no signing keys, and a web surface. All of that is inklkunn's to build *above* prikk, exactly as
planeter builds hosting above it, and none of it may push weight into prikk.

**The arc, at a glance.** Each milestone is usable on its own; 1.0 is an explicit owner gate:

```
 M0 foundations → M1 read a wiki → M2 propose and accept → M3 claims and views → M4 governance → M5 interchange and API ─► 1.0
                                      └── planeter module (joint RFC, from M2 on) ────────────────────────────────────────┘
 Later (deferred): real-time co-editing · block editor · federation · AI assistance (advisory only)
                   · the transcript's phases 2–4 (link issues and decisions, code ↔ knowledge provenance)
```

## 2. Guiding rules (constant across the roadmap)

- **Design before implementation** (project rules): requirements → external design → threat model →
  internal design → **RFC + handoff** → implementation → tests. Never inverted.
- **Build above prikk, over its CLI surface.** Never link its crates; never push weight into prikk
  (prikk RFC 145; planeter PU-2). A capability prikk lacks is a named dependency and a letter through
  the owner, never a patch inklkunn writes.
- **The VCS is the substrate, not the UI** (requirements analysis §8). Pages and other views are
  projections; claims, evidence, proposals, decisions and policies stay explicit domain concepts
  (the sixteen architectural constraints in the main transcript).
- **Honesty is a type.** What prikk verified, what the wiki service accepted and what a person approved
  are distinct and shown as such, in planeter's Assurance discipline; no page says more than is known.
- **Inclusive by construction.** A message catalog, plain words, WCAG 2.2 AA checks that fail the build,
  and no JavaScript required to read or to propose, from the first page (wiki-commons WEB-1 and AUTH-12;
  planeter RFC 012). Low barrier to legitimate participation *and* safeguards against abuse.
- **Immutable history and the duty to remove are both real.** Suppression without erasure on the view,
  never silent rewriting (requirements register item 14; wiki-commons HIST-12).
- **Less is more.** The default view shows little; depth is one step away.
- **Security is release-gating.** A threat model is a first-class deliverable, updated by any release
  that touches identity, signing, the prikk boundary, rendering or inbound content.

## 3. Milestones and versions

| Milestone | Version | Contents | What it proves | Waits on |
|---|---|---|---|---|
| **M0** | 0.0.0 (untagged) | **Foundations:** the scaffold (this roadmap, `rfcs/`, `docs/src` for mdBook, a charter); the design set v0.1 (requirements specification, external design, threat model, internal design); RFCs 001, 002 and 010 accepted with handoffs; the Cargo workspace with its layering gate; the prikk driver and content-merge spikes (§9, R2) | the ground is right before anything is built | the owner's rulings DR-1 … DR-5 |
| **M1** | **0.1.0** | **Read a wiki:** pages rendered from a prikk repository in the Portable Wiki Markdown profile; free links, backlinks, dangling links, redirects and aliases; history, diff, attribution and permalinks from prikk; verification honesty on every page; a raw source URL per page; export as a Portable Wiki Bundle; English and Japanese; WCAG 2.2 AA structural tests; a standalone binary | a real, browsable, honest, accessible wiki over prikk | prikk ≥ 0.54 (`extract`, `diff`, `tree`, `cat`, the `show` fix) |
| **M2** | **0.2.0** | **Propose and accept:** sign in (local password, OpenID Connect); propose a change to a page through a Markdown form that needs no script; a proposal page that shows what changes; review (comment, approve, request changes); accept, which seals the change and updates the page; re-authoring of a proposal whose base moved; participation metrics instrumented (register item 18) | the thesis's core loop: every change is a proposal | DR-4 ruled; threat model v0.2 |
| **M3** | **0.3.0** | **Claims and views:** claim and evidence objects anchored to page blocks; "why is this here?" provenance for a sentence; show disagreement (alternative views as branches with a comparison); knowledge freshness | the knowledge layer above pages | the claim-identity investigation (RFC 005 part 1) |
| **M4** | **0.4.0** | **Governance:** policy as versioned data (who may propose, review, accept); the participation ladder; moderation with soft security first; suppression without erasure; the deletion procedure | governance explicit and revisable | — |
| **M5** | **0.5.0** | **Interchange and API:** bundle import (MediaWiki and BookStack through wiki-commons' tooling), an OpenAPI-described read and write API, Atom feeds, the site description document, a conformance self-assessment | legible to tools and to other wikis | — |
| **planeter module** | within 0.x | sign-in and authorization shared with planeter, the wiki embedded in planeter's UI, planeter hosting the same repositories read-only so "verified by prikk" is checkable independently | the forge gains a wiki without gaining a key | planeter's ruling on OQ-4; joint RFC 008 |
| **1.0.0** | 1.0 | the usable, hardened, backup-tested wiki with the knowledge and governance layers, a settled content model and conformance claims | | **owner confirmation**, never automatic |

> **v0 → v1 is an owner gate.** All development proceeds in 0.x. Promotion to 1.0 requires the owner's
> explicit confirmation; green gates and feature completeness are necessary, not sufficient. Minors are
> not pre-assigned to milestones: a milestone takes the next free minor when it ships.

## 4. Development themes

| Theme | Serves | What lands | Governing record |
|---|---|---|---|
| **T1 storage boundary and content model** | M0, M1 | one crate drives prikk as a sandboxed, version-pinned subprocess behind a trait (planeter's `planeter-prikk` is the precedent); a wiki repository *is* a Portable Wiki Bundle: `pages/**/*.md` with frontmatter, `attachments/` with sidecars, block identifiers `^id` | RFC 001 |
| **T2 identity, keys and signing** | M0 → M2 | who signs a change and where keys live (DR-4); attribution and trust labels; sign-in | RFC 002 |
| **T3 read path and rendering** | M1 | the Markdown profile, link resolution, history and diff from prikk, verification honesty, raw source, the message catalog and the accessibility tests | RFC 003 |
| **T4 proposals** | M2 | propose, review, accept, seal; the stale-base re-authoring; conflicts as refusals with a way out, never a silent choice | RFC 004 |
| **T5 knowledge layer** | M3 | claim identity and expression, evidence, views, provenance, disagreement as branches | RFC 005 |
| **T6 governance and moderation** | M4 | policy as data, roles and the ladder, moderation, suppression and deletion | RFC 006 |
| **T7 interchange and API** | M5 | bundle import and export, OpenAPI, feeds, site description, conformance | RFC 007 |
| **T8 planeter integration** | M2 onward | the module boundary, shared identity and authorization, UI embedding, read-only hosting | RFC 008 (joint with planeter) |
| **T9 real-time co-editing** | later | a live layer whose durable checkpoints are prikk patches (HIST-10) | RFC 009, deferred |
| **T10 upstream tracking and supply chain** | all | re-baseline at each prikk and each planeter release; dependency policy and gates at every project event | RFC 010 |
| **T11 hardening and operations** | before 1.0 | backup as one coherent set with a rehearsed restore, observability, the deployment guide | an operations RFC before 1.0 |

## 5. Release cycles

- **A release is a signed tag plus built artifacts**: the standalone server binary for Linux and a
  container image, published with build attestations; the crates are also published to crates.io as an
  install path, never as the release itself. Tags are bare `X.Y.Z` (no `v`), the Rust convention and the
  project rule.
- **Nothing is tagged until M1.** The workspace stays `0.0.0` through foundations.
- **Milestone-driven minors in 0.x.** A minor ships only when its gates are green: `cargo fmt`,
  `clippy -D warnings`, tests, `cargo deny`, `cargo audit`, the layering gate, the catalog and
  accessibility tests, and the threat model updated or re-verified. A release that changes no runtime
  source (docs, metadata, installer) is a patch.
- **Re-baseline against each upstream release.** inklkunn has two fast-moving upstreams, prikk and
  planeter. For each release of either: read the notes, run the version-pinned suites against the new
  binary, decide the floor, record the outcome (planeter's `UPSTREAM.md` procedure, adopted).
- **Security releases are out of band.**
- **The architect proposes each cut with a readiness report; the owner authorizes; the architect
  executes** (tag, workflow, publish), as the ecosystem does.

## 6. Dependencies (on prikk, on planeter, on wiki-commons, on the owner) — named so nothing assumes them

- **prikk ≥ 0.54.x** carries what M1 needs: `extract` (files at a point, with `claim_parent_block_id`),
  `diff` (`diff-report-v1`), `tree`, `cat`, `log`/`show` JSON, and the `show <block-id>` fix. The pin
  starts there and moves only when a release carries a verb inklkunn needs or a security fix.
- **prikk RFC 170** (embedding as a library) is an unscheduled investigation and **is not assumed**;
  the storage trait keeps the door open (DR-3). **prikk RFC 112** (seal, branch and tag live in the CLI
  crate) is accepted and unscheduled: a library consumer could not seal today even if linking were
  sanctioned.
- **prikk's write model** binds the proposal flow: every commit is author-signed, every seal
  maintainer-signed; `sync build` needs a maintainer key (planeter ledger PK-27); conflicts are detected
  and refused, never resolved by automation; same-file text transforms are deferred. Content-level merge
  is therefore inklkunn's (R2).
- **prikk RFC 154** (adopting a trusted fast-forward) is unscheduled. It matters only for the
  planeter-hosted write path; inklkunn's own service path does not wait for it.
- **planeter** has no module or plugin mechanism today; embedding is a joint RFC on both sides. Its
  message-catalog design (RFC 012), glossary, `authorize()` contract and threat model are reused as
  patterns. Its ruling on OQ-4 gates T8.
- **wiki-commons 0.4** identifiers may change before its 1.0; conformance claims are versioned
  (`portable-content@0.4`) and re-assessed per release.
- **The owner**: the decision requests of §8, every tag and publish, letters to the prikk and planeter
  teams, and the 1.0 promotion.

## 7. The RFC plan (priority and order)

Detailed design lives in [`rfcs/`](rfcs/) under the 5-folder lifecycle (RFC 000); this is their
**priority and order**, each mapped to the milestone it serves. The architect writes each RFC and its
handoff; the dev team implements against it; the owner authorizes.

| # | RFC (planned) | Serves | Priority |
|---|---|---|---|
| **001** | Foundations: the workspace and its layering gate, the storage boundary (prikk CLI driver behind a trait, sandboxed, version-pinned), the repository layout as a Portable Wiki Bundle, the page model | M0 → M1 | **first** |
| **002** | Identity, keys and signing: who signs what, key custody, attribution, trust labels, sign-in (DR-4) | M0 → M2 | high |
| **003** | Read path and rendering: the Markdown profile, link resolution, history and diff, verification honesty, raw source, catalog and accessibility | M1 | high |
| **004** | Proposals: propose, review, accept, seal; stale-base re-authoring; conflicts as refusals with a way out | M2 | high |
| **005** | Knowledge layer, in two parts: (1) the claim-identity and patch-granularity investigation (register items 1, 2, 11, 12), then (2) claims, evidence, views, provenance, disagreement as branches | M3 | medium |
| **006** | Governance and moderation: policy as versioned data, the participation ladder, soft security, suppression without erasure, deletion | M4 | medium |
| **007** | Interchange and API: bundle import and export, OpenAPI, feeds, site description, conformance self-assessment | M5 | medium |
| **008** | *(joint with planeter)* The planeter module: boundary, shared identity and authorization, UI embedding, read-only hosting of wiki repositories | T8 | parallel, gated on planeter's OQ-4 ruling |
| **009** | Real-time co-editing: a live layer with durable prikk checkpoints | later | deferred |
| **010** | Upstream tracking and supply-chain maintenance: re-baseline procedure, floors, gates at project events, pinned build chain | all | with M0 |

## 8. Decisions requested from the owner

Each request states the decision, the options with their trade-offs, the architect's recommendation and
what deferring costs (workflow §9.3). **The roadmap above is drawn on the recommended options**; a
different ruling changes the milestones as noted.

### DR-1 — Scope and first application: wiki-first, or claims-first?

- **(a) Wiki-first.** Pages are the stored artifact (Markdown files in the Portable Wiki Markdown
  profile with frontmatter). Claims, evidence and views arrive in M3 as explicit objects anchored to
  page blocks. The CKI vocabulary (claim, evidence, proposal, decision, policy, view) names the domain
  model from M0 so nothing has to be renamed, but M1 and M2 ship without it being load-bearing.
- **(b) Claims-first.** Claims and evidence are the primary store; pages are generated projections.
  The most faithful reading of "knowledge is a set of revisable claims", and the one the requirements
  register itself warns against freezing before prototyping (items 1, 2, 11, 12 are P0 *investigations*).
- **(c) Both from day one.** Doubles M1's scope.
- **Recommendation: (a)**, with (b)'s questions worked as RFC 005 part 1 before M3 is designed. A wiki
  that people can read and propose to in two milestones validates the social thesis sooner than an
  elegant model that is never exercised, and nothing in (a) forecloses (b): files are patches, and a
  claim object is a file too.
- **If deferred:** M0 cannot fix the repository layout; nothing downstream starts.

### DR-2 — Deployment shape: an independent service that planeter embeds, or a crate inside planeter?

- **(a) Own service, embedded presentation.** inklkunn is its own binary with its own repositories, keys
  and threat model, built from library-first crates. planeter integrates it as a module at the identity
  (single sign-on, tokens), authorization and UI level, and may host the same repositories read-only
  through its own path, so "verified by prikk" is checkable by a route the wiki does not control.
  planeter stays keyless; both products ship on their own cadence; standalone use works.
- **(b) A crate in planeter's workspace.** Subject to planeter's gates from day one. The signing path
  (DR-4) would have to live behind a non-default feature like planeter's reserved `forge-seal`, so a
  default planeter build could not run a writable wiki.
- **(c) Standalone only.** Simplest; misses the stated goal of joining the forge.
- **Recommendation: (a).** It keeps planeter's headline property intact ("a compromised forge can deny
  history but never forge it") while giving the wiki the low-friction write path it needs.
- **If deferred:** M0's workspace layout and the crate boundaries cannot be fixed.

### DR-3 — prikk integration: subprocess over the CLI, or link the crates?

- **(a) Subprocess over the CLI's JSON surface**, behind a storage trait, reusing the design of
  planeter's driver (bubblewrap sandbox, version pin, typed JSON, refusal on unknown schema). The ruled
  ecosystem path (prikk RFC 145).
- **(b) Link `prikk-store`.** Refused by prikk's own posture (no stability promise, "the library API last
  if ever"); seal is not even reachable in the library (RFC 112). Only a letter asking prikk to schedule
  RFC 170 could change this, and it is not assumed.
- **Recommendation: (a)**, with the trait so that (b) remains a swap, not a rewrite, if prikk ever rules
  otherwise.
- **If deferred:** M0's storage boundary cannot be designed.

### DR-4 — Who signs a wiki change, and where do the keys live? (the load-bearing decision)

prikk requires an author Ed25519 signature on every patch and a maintainer signature on every seal.
Wiki contributors hold no keys and must not need to; that is the whole point of "participation can
begin with a proposal".

- **(a) The wiki service signs as itself.** One author key per wiki for changes authored through the
  web, with the human's identity recorded in the signed patch message (prikk stores the message as
  signed evidence) and in the service's records; one maintainer key per wiki for seals. Lowest
  friction; prikk `verify` still proves integrity and attribution *to the service*; simple key
  management. The service becomes a key holder and therefore a high-value target, and attribution to a
  person rests on service records, not on a signature.
- **(b) Per-account author keys held by the service**, encrypted at rest, plus a service maintainer key.
  Per-person attribution inside prikk history, with prikk's first-contact continuity per account.
  A store of user keys is a credential honeypot, and prikk has no author-key revocation yet.
- **(c) Client-side keys** (browser signing, bring your own key). Strongest attribution, highest
  friction; unusable for a newcomer; right as an opt-in for maintainers later.
- **The seal, separately:** service-held (as in a), or human maintainers sealing on their own machines
  through a helper, as planeter's model has it. The second makes "the page is updated" wait for a
  maintainer's machine, which a wiki cannot afford as its default.
- **Recommendation: (a) for M2**, with the trust labels saying exactly that ("signed by the wiki
  service on behalf of …"), (b) as a later opt-in, (c) for maintainers who want it. Under DR-2 (a) the
  signer lives in inklkunn's own process and trust domain, so planeter's default build stays keyless.
  The threat model that accompanies RFC 002 is where this is argued in full before anything is built.
- **If deferred:** M2 cannot be designed; M1 can proceed.

### DR-5 — Content model and repository layout: is a wiki repository a Portable Wiki Bundle?

- **Yes:** `pages/**/*.md` in the Portable Wiki Markdown profile with frontmatter (`id`, `title`,
  `aliases`, `tags`, `status`, `license`, `lang`), `attachments/` with sidecar metadata, block
  identifiers `^id` as the anchors claims attach to, `wiki-bundle.yaml` as the manifest. Export is then a
  copy, import is a commit, and prikk's file-level patches fit page-level edits.
- **No:** a bespoke store (structured objects, a database) with Markdown as an export. More freedom,
  no free interchange, and it moves the authoritative state out of prikk.
- **Recommendation: yes.** It is the "file over app" posture wiki-commons argues for and the one
  prikk's model rewards.
- **If deferred:** RFC 001 cannot be written.

### DR-6 — Real-time co-editing: now, or after 1.0?

- **Recommendation: deferred (T9).** The design keeps the hook (a live layer whose durable checkpoints
  are prikk patches, HIST-10), and nothing in M1 to M5 depends on it. The sub transcript's CRDT-over-patch
  hybrid remains the intended shape when demand appears.

### DR-7 — Languages shipped

- **Recommendation: English and Japanese from M1**, the planeter precedent, with Japanese shipped only
  after a fluent reviewer's pass. The owner names the reviewers.

### DR-8 — Default content license

- A public wiki needs a declared default license on every page (wiki-commons LIC-1). **Recommendation:
  CC BY-SA 4.0 as the default for public wikis, declared per wiki and overridable per page**; private
  wikis declare their terms explicitly. The owner's choice.

### Open input (a question, not a decision)

The main transcript links four kickoff documents it produced: a system concept, a requirements analysis,
a project plan and a system architecture specification v0.1. **They are not on disk** under the specs
folder; only the transcript is. If they exist, the architect folds them into Phase 1 (the requirements
specification) and amends this roadmap where they disagree; if not, Phase 1 starts from the transcripts,
which carry the register and the constraints in full.

## 9. Initial risk register

| ID | Risk | Kind | Handling |
|---|---|---|---|
| **R1** | Key custody (DR-4) is the single largest security decision; a wrong default is hard to undo | security | the threat model lands with RFC 002, before M2 |
| **R2** | Two proposals touch the same paragraph: prikk refuses the conflict and defers same-file text transforms, so a proposal whose base moved must be re-authored at the content layer, gently (HIST-9) | design | an M0 spike measures how often prikk's span anchors survive a moved base; RFC 004 designs the re-authoring |
| **R3** | Write throughput: prikk is single-writer per repository with fail-fast locks, and an accept costs tens of milliseconds per patch (planeter PK-68); a busy wiki serializes proposals | performance | measure at M2; one repository per wiki or per namespace is the lever |
| **R4** | Claim identity semantics are unresolved (register P0) | design | RFC 005 part 1 is an investigation with prototypes before part 2 is designed |
| **R5** | Abstraction creep: the knowledge-infrastructure model is elegant and could delay a usable wiki | scope | each milestone is a usable increment; M3 waits for M2 to be real |
| **R6** | The first JavaScript surface in this ecosystem (a richer editor): CSP, authoring-tool accessibility, input-method handling (I18N-7) | inclusion, security | a Markdown form that needs no script first; the editor is a progressive enhancement with its own review |
| **R7** | Two fast-moving upstreams: a prikk or planeter release changes a JSON schema or a contract | correctness | the driver refuses unknown schema versions; re-baseline per release (RFC 010) |
| **R8** | The duty to remove content versus immutable history: prikk cannot remove an object | legal, design | tombstones on the view (HIST-12) from M4; a repository-level removal procedure designed before any public deployment |
| **R9** | A single human authority; rulings wait on the owner's availability | governance | decision requests batched; the read side never waits |
| **R10** | "Inclusive" is unmeasured | product | define the metrics of register item 18 in M2 and instrument from the first proposal |

## 10. Method and roles (project rules §Workflow)

The **architect** (the high-capability model) writes the roadmap, the requirements and design set, the
RFCs and their handoffs, and manages release cycles. The **dev team** (the mid-capability model)
implements against the handoffs and returns review packages with evidence. The **owner** authorizes the
roadmap, the decisions above, releases, tags and letters to the prikk and planeter teams. Both agents are
authorized to commit and push. Entry points are surfaced to the owner as file paths: this roadmap,
[`rfcs/README.md`](rfcs/README.md), and, once they exist, `docs/STATUS.md` and the active handoff.

**The order of work after authorization:** Phase 1, the requirements specification derived from the
transcripts and the rulings (owner approves scope); Phase 2, the design set (charter, external design,
threat model, internal design) and RFCs 001, 002 and 010 with their handoffs; then the dev team is
organized and M0 begins.
