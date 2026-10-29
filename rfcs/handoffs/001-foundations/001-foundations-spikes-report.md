# M0 spikes report — cost table (A) and content merge (B)

| | |
|---|---|
| For | RFC 001 D-10; handoff 001 T6; the cost table of `inklkunn-prikk` (RFC 001 D-4); RFC 004 (proposals) and the content-level merge (internal design MRG-4, roadmap R2, IS-2) |
| Measured | 2026-10-27, prikk 0.54.1 (`cargo install`, dynamically linked), Linux 7.3, 32 cores, 59 GiB, tmpfs scratch; release build of the generator |
| Reproduce | `cargo build --release -p inklkunn-corpus`, then the commands below; the data are `spikes/spike-a-prikk-0.54.1.json` and `spikes/spike-b-prikk-0.54.1.json` beside this file |
| Scope | Measurement only. No product code depends on these files; the cost table in `crates/inklkunn-prikk/src/cost.rs` cites them. |

## Spike A — the per-verb cost table

### Corpus

Four wiki-shaped repositories from `inklkunn-corpus wiki` (deterministic from a seed): Portable Wiki Markdown
pages of prose with headings, free links to other pages and a block identifier per paragraph, sealed in 4 or 8
blocks plus one editing block that revises one page in twenty. Each has a hierarchy chain **500** levels deep
(`d/d/d/…`): **prikk refuses repository paths over 1024 bytes**, so the requirements' "depth 1,000" target is
unattainable (ledger PK-11, issue IS-3).

```sh
B=target/release/inklkunn-corpus; K=/scratch/keys    # a throwaway key pair, never a real one
$B wiki --out /scratch/w-1m-1k    --keys $K --pages 1000  --total-bytes 1048576   --depth 500 --blocks 4 --seed 1
$B wiki --out /scratch/w-16m-1k   --keys $K --pages 1000  --total-bytes 16777216  --depth 500 --blocks 4 --seed 2
$B wiki --out /scratch/w-64m-10k  --keys $K --pages 10000 --total-bytes 67108864  --depth 500 --blocks 8 --seed 3
$B wiki --out /scratch/w-200m-10k --keys $K --pages 10000 --total-bytes 209715200 --depth 500 --blocks 8 --seed 4
python3 tools/spikes/spike-a.py --prikk ~/.cargo/bin/prikk --repo /scratch/w-1m-1k --repo /scratch/w-16m-1k \
  --repo /scratch/w-64m-10k --repo /scratch/w-200m-10k --scratch /scratch/x --runs 3 --bisect --out spike-a.json
```

| repository | pages | page bytes | store *S* | blocks |
|---|---|---|---|---|
| w-1m-1k | 1,000 | 2.1 MiB | 3.6 MiB | 5 |
| w-16m-1k | 1,000 | 17.1 MiB | 18.6 MiB | 5 |
| w-64m-10k | 10,000 | 68.6 MiB | 76.3 MiB | 9 |
| w-200m-10k | 10,000 | 204.6 MiB | 212.3 MiB | 9 |

(The 1 MiB target became 2.1 MiB of pages: a page is at least 200 bytes of frontmatter and a paragraph.)

### Method

Each verb ran three times in a fresh child; the table keeps the **maximum resident set** (`getrusage`
`ru_maxrss`) and the **median wall time**. Because the driver's ceiling is `RLIMIT_DATA`, not RSS, the
**smallest `RLIMIT_DATA` at which the verb still succeeds** was found by bisection to 256 KiB ("data need").
`cat` is the largest page; `show` is the tip block (the editing block); `diff` is root → tip over the whole
tree; `extract` writes into a fresh directory each time.

### Results (data need / RSS, as multiples of *S*; median seconds)

| verb | w-1m-1k (3.6 MiB) | w-16m-1k (18.6 MiB) | w-64m-10k (76 MiB) | w-200m-10k (212 MiB) | reading |
|---|---|---|---|---|---|
| `branch` | 7 MiB / 12 MiB; 0.001 s | 7 / 12; 0.006 s | 7 / 12; 0.005 s | 7 / 12; 0.006 s | **flat**, 7 MiB |
| `log` | 7 / 12; 0.002 s | 7 / 12; 0.003 s | 7 / 12; 0.011 s | 7 / 12; 0.016 s | **flat**, 7 MiB |
| `tree` | 7 / 12; 0.017 s | 1.01 × / 1.30 ×; 0.072 s | 0.99 × / 1.05 ×; 0.17 s | 1.00 × / 1.02 ×; 0.42 s | **1.0 × S** |
| `cat` (metadata) | 7 / 12; 0.016 s | 1.01 × / 1.30 ×; 0.078 s | 1.00 × / 1.05 ×; 0.17 s | 1.00 × / 1.02 ×; 0.42 s | **1.0 × S** (a whole-tree replay, like `tree`) |
| `cat` (bytes) | 7 / 12; 0.016 s | 1.00 × / 1.31 ×; 0.062 s | 1.00 × / 1.06 ×; 0.18 s | 1.00 × / 1.02 ×; 0.40 s | **1.0 × S** |
| `show` (small patch) | 7 / 12; 0.016 s | 7 / 12; 0.055 s | 10 / 14; 0.16 s | 10 / 14; 0.36 s | **flat** for a small patch; follows the patch's content |
| `diff` (root → tip) | 2.9 × / 4.2 ×; 0.024 s | 3.66 × / 3.78 ×; 0.096 s | 3.69 × / 3.68 ×; 0.40 s | **4.59 ×** / 3.78 ×; 1.16 s | **4.6 × S**: the one verb whose address space exceeds its RSS by much |
| `extract` | 7 / 12; 0.036 s | 0.78 × / 1.10 ×; 0.081 s | 0.89 × / 0.95 ×; 0.28 s | 0.88 × / 0.90 ×; 0.58 s | **0.9 × S** |
| `verify` | 7 / 12; 0.125 s | 1.93 × / 2.20 ×; 0.21 s | 1.99 × / 2.03 ×; 0.87 s | 1.98 × / 1.99 ×; 1.48 s | **2.0 × S** |

### What the cost table takes from this (RFC 001 D-4)

- The floor of 16 MiB covers every flat verb (7 MiB of data; prikk's own working set) with margin.
- **`diff` is the expensive read**: 4.6 × S of address space on 212 MiB. The table sets **5.0 ×** (was a guess
  of 4.2 ×). A whole-history diff of a 200 MiB wiki is a ~1 GiB admission; RFC 003's compare view should diff
  one page (`--path`) wherever it can, which this spike did not measure (a follow-up for the M1 re-baseline).
- `tree`, `cat`, `extract` and `verify` on many-small-page repositories cost **less** than planeter's
  single-large-file multiples (2.1 ×, 2.1 ×, —, 4.1 ×). The table **keeps the larger** because an attachment is
  exactly the single-large-file shape and M0 had none to measure; the shape joins the re-baseline corpus with
  attachments (M1; `docs/UPSTREAM.md` step 5).
- Latency is comfortable: a 212 MiB wiki answers `tree`/`cat` in 0.4 s, `verify` in 1.5 s, `diff` in 1.2 s.
  The server-kept deadlines (30 s for the light verbs, 120 s for `diff`/`extract`, 300 s for `verify`) have an
  order of magnitude of room on this machine; a slow disk would eat some of it.
- `PROVISIONAL` in `cost.rs` is now `false`; `BOUND_TO` stays the floor `0.54.0` (the measurement is on
  0.54.1, a patch release with the same read surface).

## Spike B — content merge (MRG-4, R2, IS-2)

### Corpus and method

`inklkunn-corpus histories` builds a 40-page base wiki (6 sections × 3 paragraphs each, ~4 KiB a page) and
runs **120 trials** of two concurrent edits from one baseline, drawn by a seeded generator from five scenarios
weighted toward what a wiki sees: edits to different pages (35 %), to different paragraphs of one page (30 %),
to the same paragraph (15 %), a rename beside an edit of the renamed page (10 %), a frontmatter edit beside a
body edit (10 %). For each pair:

1. **prikk's verdict.** Edit B is sealed on a branch from the baseline, edit A on `heads/main`, then
   `prikk merge --allow-no-audit --baseline-block <baseline> --into heads/main --from <branch>`: confluent
   (the merge seals) or refused (the kind from prikk's text). This is the direct answer to "would prikk's own
   span anchors still apply at the new tip".
2. **A block-aware three-way merge.** Pages are split into blocks (frontmatter, then blank-line-separated
   paragraphs and headings, compared trimmed); the pair is *clean* when the two sides' changed block sets are
   disjoint on every page. This is the merge inklkunn would do above prikk (MRG-4).
3. **Base movement** is a model, not a measurement of a live wiki: proposals arrive at a rate, reviews take an
   exponentially distributed time, and a base has moved when another proposal was accepted in between.

```sh
$B histories --out /scratch/histories --keys $K --trials 120 --seed 7 --report spike-b.json
```

### Results

| scenario | trials | prikk confluent | prikk refused | block-aware merge clean |
|---|---|---|---|---|
| different pages | 41 | **41** | 0 | 41 |
| rename + edit of the renamed page | 12 | **12** | 0 | 12 |
| one page, different paragraphs | 32 | 0 | 32 (`Deferred` 29, `Conflict` 3) | **32** |
| one page, frontmatter + body | 14 | 0 | 14 (`Deferred`) | **14** |
| one page, same paragraph | 21 | 0 | 21 (`Conflict` 13, `Deferred` 8) | 0 |
| **all** | **120** | **53 (44 %)** | 67 (56 %) | **99 (83 %)** |

- **prikk's confluence is per file for text** (UD-4 confirmed): every pair touching one page was refused,
  whatever the paragraphs. A rename beside an edit *does* merge: prikk follows the file's identity through
  `prikk mv` (12/12).
- **A block-aware merge turns 46 of the 67 refusals into clean merges**: 83 % of all pairs are clean, and the
  remaining 17 % (the same paragraph edited twice) are true conflicts that need a person. **R2 is a real need,
  and MRG-4 is the right place for it.**
- The block model and prikk agree perfectly where both can decide (53 confluent = 53 clean; 21 same-paragraph
  = 21 unclean); the 46 in between are exactly inklkunn's margin over prikk.
- The **kind** of refusal (`Conflict` or `Deferred`) is deterministic for a given edit pair: on 2026-10-27 two
  further runs of this harness with seed 7 agreed with the committed data on all 120 kinds. The variation the
  first draft of this report noted came from a harness change between its two runs (the block model's
  leading-newline fix altered the generated edits), not from prikk; no question to prikk is needed (ledger
  PK-16). Nothing in inklkunn depends on the kind.

**Base movement** (fraction of proposals whose base moved before acceptance, 20,000 simulated proposals each):

| proposals / day | review 1 h | review 1 day | review 1 week |
|---|---|---|---|
| 1 | 0.2 % | 26 % | 77 % |
| 5 | 3 % | 69 % | 95 % |
| 20 | 21 % | 91 % | 98 % |
| 100 | 66 % | 98 % | 99.6 % |

Any wiki whose reviews take a day sees most bases move. The acceptance path cannot assume the base is the tip
(RFC 004: rebase-on-accept with the block-aware merge, and a visible "the page changed under this proposal"
when the same block moved).

### What RFC 004 takes from this

1. Acceptance re-bases the proposal's pages on the view's tip with the block-aware three-way merge; a disjoint
   change set is applied without a person; an overlapping block is shown side by side for the acceptor.
2. The merge is inklkunn's and is recorded in the patch message (`Base:` the proposal's base, the tip it was
   applied to); prikk's `merge` verb is not on the path, because the shapes it refuses are the common ones.
3. Renames are declared with `prikk mv` so that prikk's identity tracking (12/12) keeps working under
   inklkunn's merge.

## Re-baseline at prikk 0.55.0 (2026-10-29, the architect; RFC 010's first)

**Same corpus, same commands** (regenerated by `inklkunn-corpus` at `fec2a50` with prikk 0.55.0 on `PATH`), **plus the
single-large-attachment shape** IS-12 asks for, built by hand: `inklkunn-corpus wiki --pages 50 --total-bytes 204800
--depth 2 --blocks 1 --seed 5`, then a 64 MiB random file at `pages/attachment.bin`, committed and sealed (*S* = 64.3
MiB). Data: `spikes/spike-a-prikk-0.55.0.json` (prikk run bare under `prlimit`, as above; **not** through the driver's
bubblewrap vector, which adds address space: the 2026-10-28 data point of PK-20 was through the vector).

- **Every multiple on the four wiki shapes is unchanged** to within 0.05 (`diff` 4.60 × at 212 MiB, `verify` 1.98 ×,
  `tree`/`cat` 1.00 ×, `extract` 0.88 ×; the flat verbs 6.6 – 6.8 MiB). prikk's +48 bytes per object for `verify`'s
  lookup does not show at these object counts. Latency at 212 MiB: `verify` 1.31 s (1.48), `diff` 0.95 s (1.16),
  `tree` 0.41 s (0.42); the small shapes were noisier (milliseconds, a shared host).
- **The attachment shape (64 MiB, bare):** `tree`, `cat`, `show`, `diff`, `extract` **2.0 × S**; `verify` **4.99 × S**.
  Against the table: `tree`/`cat` 2.1 × (ceiling 3.15 ×), `show` 3.6 ×, `diff` 5.0 ×, `extract` 3.1 × hold; **`verify`'s
  ceiling (6.15 ×) holds but its cost (4.1 ×) understates admission by ~20 %**, the same direction as PK-20's 24 MiB
  point (5.0 ×). IS-12 stands: handoff 003's T6 measures 64 and 200 MiB **through the driver's vector** and raises
  `verify` (and `cat` if the vector's figure again exceeds 2.1 ×) before `-/raw/` serves attachments.
- **IS-7's shape was not seen on 0.55.0** in these shapes: under a ceiling below its need, `verify` aborts
  (`memory allocation of N bytes failed`, exit 134) and prints no report, on `w-16m-1k` (31 – 35 MiB) and `a-64m` (100 –
  320 MiB); at the need it passes. The follow-on handoff stands regardless: a report under a ceiling is never a
  verdict.
- **Decision:** the table is unchanged and **`BOUND_TO` and the floor stay `0.54.0`** (0.55.0 brings no verb inklkunn
  needs and no correctness or security fix a wiki depends on; RFC 010's rule). The test suite passes whole on 0.55.0
  (676 passed, 0 failed, no self-skips).

## Rows for the upstream ledger

PK-11 (the 1024-byte path limit), PK-16 (merge confluence, measured), PK-20 (the cost multiples, measured):
all entered in `docs/src/inklkunn-upstream-dependency-ledger.md` v1.0.
