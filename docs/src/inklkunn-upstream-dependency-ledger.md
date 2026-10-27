# inklkunn — Upstream Dependency Ledger

| | |
|---|---|
| Document | A living record of every behaviour of prikk, planeter and wiki-commons that inklkunn's design or code relies on, and how sure we are of each. |
| Version | v1.0 (2026-10-27 — seeded during M0 from the design set's UD-1 … UD-10 and from what T1–T6 measured on prikk 0.54.1 / release asset 0.54.0, planeter 0.4.0 and wiki-commons commit `19f021c6`) |
| Why | A wiki engine whose storage is a pre-1.0 VCS cannot afford an unverified assumption: planeter was burned once by one (its "seal locally, the forge's ref advances" fallback, measured false). Every dependency is named here with its evidence, so a wrong one is caught before it is built on, and so a new reader sees what is real, what is ruled, what is not shipped, and what is still assumed. |
| How to use | Before implementing an RFC, check its rows here (the RFC names them). When an upstream ships, rules or answers, move the row's status and cite the evidence; the version line names the release or the letter. An **ASSUMED** row implemented without promotion to **CONFIRMED** is a risk taken knowingly, and the review request says so. |
| Basis | RFC 010 D-7; requirements §15 (UD-1 … UD-10); planeter's prikk dependency ledger (the shape, and its measured facts carried in as `PL-` rows where inklkunn reuses them) |

## Status legend

- **CONFIRMED** — measured here, on a named binary or commit (the evidence column says which).
- **RULED** — the upstream owner's or architect's word, or an accepted upstream RFC, cited.
- **GATED** — accepted or proposed upstream, **not shipped**; inklkunn designs to it and gates the code on the binary.
- **ASSUMED** — relied on, not yet verified: the live risk list.
- **PARTIAL** — partly confirmed; a named gap remains.

## A. prikk (PK-…)

### A.1 The read surface the driver pins (RFC 001 D-3)

| ID | inklkunn relies on | Status | Evidence / note |
|---|---|---|---|
| PK-1 | `--format json` on `branch list`, `tree`, `cat`, `log`, `show`, `diff`, `extract`, `verify`, each with a `schema_version`: `branch-list-v1`, `tree-listing-v1`, `path-content-v1`, `log-report-v1`, `show-report-v1`, `diff-report-v1`, `extract-report-v1`, `verify-report-v1` | **CONFIRMED** (0.54.1, 2026-10-27) | `inklkunn-prikk::driver::schema` pins the names; `tests/driver.rs` reads every report from the committed fixture. A report naming another schema is refused before its shape is read (`UnsupportedSchema`). **UD-1.** |
| PK-2 | A refusal under `--format json` is **exit 1**, `error: …` on stderr **and**, since 0.54, a `refusal-v1` report on stdout (`{"schema_version","command","refusal","reason","message"}`) | **CONFIRMED** (0.54.1) | measured on `extract` to an unmounted directory (`refusal: io-failure`) and on `cat` of a missing path. The driver prefers the stdout report's message and keeps its `refusal` code. Usage errors are **exit 2**. |
| PK-3 | `log [path]`'s positional argument is the **repository path**, not a file filter; history per file comes from `diff --path` between points or from `show` | **CONFIRMED** (0.54.1) | `prikk log pages/Home.md` → "i/o error: Not a directory". `History` is per view; RD-2's per-page history is built in inklkunn (RFC 003). |
| PK-4 | `cat --max-bytes N` refuses (exit 1, "is N bytes … above the --max-bytes bound … nothing was written") rather than truncating; the bound limits what is written, **not memory** | **CONFIRMED** (0.54.1) | `read()` maps it to `Refused`; the memory bound is the admission gate's (planeter PK-24 says the same). |
| PK-5 | `show <patch-id>` reports paths as `{"unresolved_node_id"}` where `show <block-id>` reports `{"path"}` | **CONFIRMED** (0.54.1) | `storage::PathRef` carries both. |
| PK-6 | The read verbs run with the repository bound **read-only** (`--ro-bind`): `tree`, `cat`, `log`, `show`, `diff`, `verify`, `extract` all succeed; no lock file is created | **CONFIRMED** (0.54.1, under bubblewrap) | the driver binds the repository read-only (STO-5); `extract` needs a writable **sibling** of its target ("the temporary sibling directory `<target>.prikk-extract-tmp`"), which is why the scratch directory is a sibling, never inside the repository. |
| PK-7 | `extract --plan-only` reports `written: false`, `file_count`, `total_bytes`, `largest_file_bytes` without writing; `--max-bytes`/`--max-files` refuse the whole point before the first byte | **CONFIRMED** (0.54.1) | `plan()` and `materialize()`; the bound test leaves no directory behind. |
| PK-8 | `verify --format json` reports `verdict.ok` and `stages[].status`; a failed verdict is a report, not an exit code the driver should read | **PARTIAL** | the success shape is confirmed; a **failing** verdict's exit code is **not yet observed** (a byte-flipped copy still verified). The driver reads the verdict from stdout whatever the exit code (planeter's IS-39 lesson). To confirm: break a fixture deliberately at the next re-baseline. |
| PK-9 | A process under `RLIMIT_DATA` dies in one of four shapes: Rust abort (134/SIGABRT, "memory allocation of … failed"), loader exit 127 ("Cannot allocate memory", "cannot allocate TLS data structures"), a bare **SIGSEGV** before any message (GitHub's ubuntu-24.04 runner), or `verify` exit 1 "did not complete every stage" | **CONFIRMED** (0.54.0 release asset and 0.54.1, 2026-10-27) | the driver reads any of them, and under a ceiling any death by signal, as `CeilingExceeded`, never as a verdict. 64 KiB fails in the loader; 256 KiB runs `verify` on a 60 KB repository. |
| PK-10 | The 0.54.0 release asset `prikk-x86_64-unknown-linux-gnu.tar.gz` is a **dynamically linked glibc** PIE (`ld-linux-x86-64.so.2`), so the sandbox must bind `/usr` and the usr-merge symlinks | **CONFIRMED** (asset downloaded 2026-10-27) | the `Dockerfile` and the sandbox vector assume glibc; a musl asset would change nothing but the bind list could shrink. |
| PK-11 | A repository path (worktree-relative, in bytes) may not exceed **1024 bytes** ("invalid name: repository path exceeds 1024 bytes") | **CONFIRMED** (0.54.1, spike A generator) | bounds the hierarchy depth: with one-character components about 500 levels under `pages/`. `title_to_path` must refuse or the proposal must be refused before `commit` (RFC 004 handoff: a check against this limit, surfaced as a lint before it is a refusal). The requirements' "depth 1,000" measurement target is unattainable; spike A measured depth 500. |

### A.2 Keys, writes and trust (RFC 002)

| ID | inklkunn relies on | Status | Evidence / note |
|---|---|---|---|
| PK-12 | Seeds are read from **files** only: `PRIKK_AUTHOR_SEED_FILE` / `PRIKK_MAINTAINER_SEED_FILE`, else `<key dir>/<role>.seed`; key ids from `PRIKK_*_KEY_ID`, else `<seed path>.key-id`; a seed never travels in the environment (unread since 0.41) | **CONFIRMED** (0.54.1; prikk guide "Security and Signing Setup") | `inklkunn-signer::KeyDir::env()` passes paths and ids; the sandbox sets them with `--setenv` and binds the key directory read-only. |
| PK-13 | `key generate --out <path>` writes the seed (mode 0600) **and** `<path>.key-id` (`ed25519-<16 hex>`); `key public --seed-file` derives the public key; `trust maintainer add --key-id ID --public-key HEX` adopts it; `init` + these replace `setup` | **CONFIRMED** (0.54.1; `create_wiki` test, confined) | the bootstrap signer's sequence. **UD-5**: there is no author-key revocation; rotation is `trust maintainer remove` + a new id (RFC 002 D-8). |
| PK-14 | `commit -m` records the message **signed** with the patch, read back in `log`'s `patch_messages[].message` | **CONFIRMED** (0.54.1) | the CT-03 trailers live there; the fixture's block 2 and `create_wiki`'s `Kind: system` message read back. |
| PK-15 | One worktree and one active WAL per repository; `commit` for another ref is refused while the queue holds patches; `branch switch` refuses a dirty worktree or unsealed work for another ref | **RULED** (prikk CLI help; planeter PK-… ) → **CONFIRMED in part** (0.54.1: `branch switch` with sealed work succeeds in spike B) | **UD-3**: writes are serialized per wiki; seal follows each commit. Spike B's generator re-materializes the worktree after a switch from `extract`. |
| PK-16 | `merge --allow-no-audit --baseline-block B --into REF --from REF` seals a **proven-confluent** merge adopting `--from`'s patches verbatim; any other outcome (`Conflict`, `Deferred`, `NotConfluent`, `Unsupported`, `OrderedDependency`, `EvidenceFailure`, `InvalidCandidate`) refuses and writes nothing; renames that meet, symlinks, create-then-delete sequences refuse | **RULED** (prikk merge guide, DC-74/DC-75) → **CONFIRMED** (0.54.1, spike B 2026-10-27, 120 trials, `rfcs/handoffs/001-foundations/spikes/`) | Two edits to **different pages** (41/41) or a **rename beside an edit of the renamed page** (12/12) merge confluently. Two edits to **one page** are refused in every trial, whether to different paragraphs (32), to the frontmatter and the body (14) or to the same paragraph (21): prikk's confluence is per file for text (UD-4). The refusal's **kind** (`Conflict` or `Deferred`) differed between two runs of the same edit pairs on freshly built repositories; a question for prikk by letter. |
| PK-17 | No object or ref deletion exists; `branch close` keeps pointer, history and objects | **RULED** (CLI help "not delete — pointer, history, and objects stay; reclaims nothing") | **UD-6**: removal is a procedure above prikk (RFC 006). |
| PK-18 | Library embedding is an unscheduled investigation (prikk RFC 170); companions build over the CLI JSON (RFC 145 Shape D); the library API carries no stability promise | **RULED** (prikk RFCs 145, 170, 152 §5) | **UD-2**: the `Storage` trait is the seam; nothing links the crate. |
| PK-19 | `--version` prints `prikk X.Y.Z` | **CONFIRMED** (0.54.0, 0.54.1) | the floor check parses it; an older or unparsable answer is `UnsupportedVersion`. |

### A.3 Cost (RFC 001 D-4; spike A)

| ID | inklkunn relies on | Status | Evidence / note |
|---|---|---|---|
| PK-20 | Peak memory per read verb follows the store size *S* with a per-verb multiple and a floor; the ceiling counts `RLIMIT_DATA`, which can exceed RSS | **CONFIRMED** (0.54.1, spike A 2026-10-27: `rfcs/handoffs/001-foundations/spikes/`) | Smallest working `RLIMIT_DATA` on wiki-shaped text repositories of 3.6 / 18.6 / 76 / 212 MiB (1,000 and 10,000 pages, a 500-deep chain): `branch`, `log` flat 7 MiB; `tree`, `cat` 1.0 × S; `show` (small patch) 10 MiB; **`diff` 4.6 × S** (RSS 3.8 ×: the one verb whose address space exceeds its RSS by much); `extract` 0.9 × S; `verify` 2.0 × S. Latency at 212 MiB: `tree` 0.42 s, `diff` 1.2 s, `verify` 1.5 s. `inklkunn-prikk::cost` keeps the larger of these and planeter's single-large-file worst cases (`tree`/`cat` 2.1 ×, `show` 3.6 ×, `verify` 4.1 ×) until attachments give inklkunn that shape to measure (M1). |

## B. planeter (PL-…)

| ID | inklkunn relies on | Status | Evidence / note |
|---|---|---|---|
| PL-1 | planeter 0.4.0 has **no module mechanism**; its RFC 005 scopes wikis out; **OQ-4** (wikis in v1?) is unruled | **CONFIRMED** (planeter 0.4.0 tree and STATUS, 2026-10-24) | **UD-7, UD-8**: inklkunn is standalone until the joint RFC 008; roadmap theme T8. |
| PL-2 | The sandbox vector (`bwrap --unshare-all --clearenv --die-with-parent --new-session`, `/usr` read-only with usr-merge symlinks, `--proc`, `--dev`, the binary bound, `prlimit --data` in front) and its two findings: bwrap as the direct child (IS-37), spawn and wait on one thread because `--die-with-parent` is per thread (I3) | **CONFIRMED** (planeter `planeter-prikk/src/sandbox.rs` 0.4.0; re-derived in `inklkunn-prikk::sandbox`, measured under bubblewrap 2026-10-27) | inklkunn differs in binding the repository **read-only** for reads and adding a key-directory mount for the signer. |
| PL-3 | The admission shape: cost = max(floor, S × multiple); light pool for floor-cost reads, heavy pool for the rest; ceiling = 1.5 × cost; `Busy` and `TooLarge` as answers | **RULED** (planeter RFC 011) → **CONFIRMED** as code (`inklkunn-prikk::admission`, `cost`) | the multiples are inklkunn's own after spike A (PK-20). |
| PL-4 | The dependency policy (measure before commit; license allow-list; the advisory ladder) and its **rejected** list: an SSH-key crate (unfixable advisory), in-process TLS/SSH stacks, a heavier JWT backend, an in-process HTTPS client | **RULED** (planeter RFC 010; its `docs/dependency-policy.md` rows 2026-09-23/24) | carried into `docs/dependency-policy.md` as evidence, to be re-measured before any inklkunn decision relies on it (RFC 010 D-6). |
| PL-5 | `rusqlite` (bundled) as the store: planeter measured +16 crates at 0.32 | **CONFIRMED** here as +10 at 0.40 (2026-10-27) | `docs/dependency-policy.md`. |
| PL-6 | RFC 012's message catalog, owner-approved glossary and WCAG 2.2 AA structural tests; RFC 004's intent journal and recovery witnesses; the Assurance vocabulary | **RULED** (planeter RFCs 012, 004; 0.4.0) | reused as patterns in RFC 003 (catalog), RFC 004 (recovery) and RFC 002 D-6 (Assurance); versions re-checked at each planeter release (RFC 010 D-2). |

## C. wiki-commons (WC-…)

| ID | inklkunn relies on | Status | Evidence / note |
|---|---|---|---|
| WC-1 | The conformance corpus at commit `19f021c68e581b9e6423b3f2b59c99c01e82bf1a` (2026-10-27): 24 markup cases with `expected.scan.json`, 10 resolution cases with `expected.json` | **CONFIRMED** | copied into `crates/inklkunn-markup/tests/corpus/` with its digest; `floor-check` fails on drift; all 34 pass. |
| WC-2 | The scanner rules are the reference tool's (`tools/pwb/markup.py`, `resolve.py`): masks (code, comments, frontmatter), the link grammar, GitHub anchors, block ids, tags, callouts, directives, envelopes, props; the MKUP-5 resolution order with same-directory preference and index-order ambiguity | **CONFIRMED** (same commit) | restated in `inklkunn-markup::scan`'s module docs; the corpus is the test. **UD-9**: identifiers (`MKUP-`, `META-`, …) may change before 1.0; inklkunn's conformance claims are versioned `@0.4`. |
| WC-3 | The bundle manifest schema requires `format` and `version`; `fidelity`, `markup`, `contents` are the documented optional blocks | **CONFIRMED** (`schemas/bundle-manifest.schema.json`, same commit) | `inklkunn-core::bundle::manifest` writes them; validation against the schema is RFC 007's. |
| WC-4 | The `expected.<dialect>.md` conversions (gollum, dendron, logseq, obsidian, static) are the `pwb` converter's, outside inklkunn's scope | **RULED** (RFC 001 D-7; CORPUS-SOURCE.md) | inklkunn reads and writes the portable profile only. |

## D. Requirements map

| UD | Rows |
|---|---|
| UD-1 (prikk ≥ 0.54 read verbs) | PK-1 … PK-10, PK-19 |
| UD-2 (library) | PK-18 |
| UD-3 (one worktree, one queue) | PK-15 |
| UD-4 (text transforms deferred; conflicts refused) | PK-16 |
| UD-5 (no author-key revocation) | PK-13 |
| UD-6 (no deletion) | PK-17 |
| UD-7, UD-8 (planeter module; OQ-4) | PL-1 |
| UD-9 (wiki-commons identifiers) | WC-2 |
| UD-10 (gaps shown as pending a prikk increment and asked by letter) | the correspondence protocol in `docs/UPSTREAM.md`; PK-8 and PK-11 are the first two asks |
