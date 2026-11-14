# A wiki that needs recovery

When an act that writes a wiki's repository stops half-way (the process was killed, the machine lost power,
prikk failed between its two steps), the wiki is **flagged *needs recovery***. This page is for the operator:
what that means, what keeps working, the commands that look and the one that acts, and what to do when recovery
**stops** and leaves the decision to you. Nothing here rewrites a wiki's history, and nothing here needs you to
touch prikk's files by hand.

## What a flagged wiki does

- **Reads, search, history and exports keep working.** The wiki's sealed history is exactly as it was.
- **Writes are refused**: nothing is accepted into the history while the flag is set. Proposals already made are
  kept as they are.
- The flag is cleared only by recovery finishing, never by time and never by a restart alone. Each start of the
  service runs recovery for every flagged wiki, and says in its log which ones it could not finish.

## The commands

Run on the host that runs the service, as the user that owns its data directory, with the same settings as the
service (see `docs/KEYS.md`):

```sh
inklkunn wiki recover <wiki> --plan   # shows what recovery would do, and changes nothing
inklkunn wiki recover <wiki>          # does it (audited before and after)
```

`<wiki>` is the slug or the id. The plan names every witness recovery read (the lease record, the unfinished
acts, `prikk status`, `prikk doctor`, the worktree) and either lists the steps it will take, each one something
the acceptance itself would have done, or says **STOP** with a fixed word and what that word means. A recovery
that stops leaves the wiki flagged and the repository exactly as it found it; it records the plan and that it
stopped, so the trail shows what was seen.

## STOP `foreign-queued-patch`: a queued patch that no act of the service explains

**What it is.** prikk holds one patch that was committed and not yet sealed (it is *queued*), and no recorded act
of the service describes it. Recovery will not seal it: sealing would put a change into the history, signed in
the wiki's name, that nobody accepted. prikk 0.55 and 0.56 have no verb that discards a sound queued patch (a
patch that is not damaged), so recovery cannot remove it either; prikk 0.57.0 has `prikk uncommit`, which recovery
uses **only when you type the patch's id** (step 3 below). It stops and tells you. **A queued patch is not in the
history and does no harm while it waits**, which is why stopping is the safe thing to do.

**Where it comes from.** Usually one of these; recovery cannot tell which:

- someone ran `prikk commit` by hand in the wiki's repository directory, outside the service;
- the service's database was restored from a copy older than the repository, so the record of the act that made
  the patch is gone (the audit trail, `inklkunn audit export`, shows whether acts end where the repository does);
- a defect. If neither of the above fits, keep the plan text and the output of the commands below and report it.

**Look, without changing anything.** The wiki's repository is
`$INKLKUNN_DATA_DIR/wikis/<first two characters of the wiki id>/<wiki id>`. Every command here only reads:

```sh
inklkunn wiki recover <wiki> --plan       # the STOP, and the witnesses it rests on
cd $INKLKUNN_DATA_DIR/wikis/<xx>/<wiki id>
prikk status --format json                # one patch is queued: queue.count is 1, queue.patches names its patch_id
prikk show <patch id>                     # what the patch changes: the text each operation replaces and puts
prikk worktree-status                     # the worktree is the patch's result: clean against the baseline
prikk log                                 # the sealed history, which the patch is not part of
```

(`prikk status` without `--format json` counts the queue and does not print the patch id.) What the first command
prints is, line for line: what was read (`no act is unfinished`, `prikk status: 1 patch(es) queued`, `prikk
doctor: 0 error(s), 0 warning(s)`, `the worktree is the baseline`, `0 lock(s) held`), then `STOP
(foreign-queued-patch)` with the meaning above, `recovery changes nothing and the wiki stays flagged for a person
to decide`, and no step. `inklkunn wiki recover <wiki>` (without `--plan`) stops the same way, and records the plan
and that it stopped.

**What you may do.**

1. **Decide whether the change is wanted.** `prikk show` is the answer: it prints, for each operation, the text
   that is replaced and the text that takes its place. The worktree holds every page the patch changes whole, as
   the patch leaves it (that is what `prikk worktree-status` saying *clean against baseline* means while a patch
   is queued), so a page you want is copied out of the worktree now. It is put in through the front door, as an
   ordinary proposal, once the wiki takes writes again. It is never sealed from the side.
2. **Keep the evidence and wait.** This is the right choice when nothing is urgent. The wiki serves every read;
   only acceptances wait. `inklkunn wiki export <wiki> --bundle <dir> --archive <file>` writes the whole wiki
   (the sealed history, never the queued patch) to somewhere safe meanwhile.
3. **Take the patch back (prikk 0.57.0 and later).** `prikk uncommit` takes the newest queued commit out of the
   queue and keeps it in prikk's own recovery log, so that `prikk doctor --recovery-restore <run>` puts it back. With
   0.57.0 a run of `inklkunn wiki recover <wiki>` (without `--plan`) adds a note that says what prikk would do (prikk's
   own preview, the commit's message and the paths it changes) and the command that is your confirmation, with the wiki's
   id in it: `inklkunn wiki recover <wiki id> --discard-queued-patch <patch id>`. `--plan` does not show the preview,
   because prikk takes its own lock to make it and a plan shown writes nothing: it says the preview is shown in a run
   (that run takes nothing back without your word). A stale lock left beside the patch is cleared by that run first, and
   the run then stops with the preview. **You confirm by typing the patch's id** (64
   characters, as `prikk status --format json` prints it and the note repeats it); recovery never takes a patch
   back on its own, and not for a patch that is the change of a recorded act (that one is sealed, or stopped on),
   not when more than one patch is queued, and not for a queue that targets another branch than the worktree's.
   The step is audited with the id you typed, recorded in the plan the act keeps, and bound to that one patch in the
   signer. It leaves the queue empty, puts the worktree back to what the tip says (the patch's files in it go: copy out
   any page you want first, as in step 1), checks the repository, clears the flag, and prints the recovery run that
   undoes it. Before 0.57.0 there is no such verb: the plan says so, `--discard-queued-patch` is not used (the plan and the
   audit trail say that, and why: it is never recorded as a take-back that did not happen), and the wiki is meant to wait.
4. **If the wiki must take writes and the patch cannot be taken back** (prikk before 0.57.0, or you would rather keep it), make a new wiki from the old one's pages: export it as above,
   create a wiki (`inklkunn wiki create`), and `inklkunn wiki import-folder` the exported `pages/` and
   `attachments/` into it. This does not touch the old wiki, which stays flagged, readable and exportable as the
   record of its history; the new wiki's history begins with the import, so it has a new slug and new keys, and
   you decide what happens to the old address.

**What not to do.** Each of these is an action on the repository that the service did not sanction, and each
either rewrites the history or puts an unaccepted change into it:

- `prikk seal` (it publishes the queued patch, signed by whatever key is at hand);
- `prikk rollback-draft`, `prikk rollback-preview` and the rest of the rollback family (they add to what is
  queued; an undo in this wiki is a proposal, RFC 004 D-8);
- `prikk uncommit` run by hand, which does what step 3 does without its checks (that the queue holds exactly this patch, on the
  branch the worktree is on) and without the audit, and leaves the worktree as the patch left it;
- `prikk doctor --discard-damaged-commits` or any other repair, which prikk refuses over a patch that is sound,
  and which would be the wrong tool if it did not;
- deleting files under `.prikk/` or in the worktree, and `prikk unlock --force` on a lock a running process
  holds.

The other STOP words (`queued-patch-is-not-the-act`, `published-block-is-not-the-act`, `several-queued`,
`several-acts`, `tip-moved`, `interrupted-state`, `doctor-trouble`, `unknown-lock`, `foreign-process`,
`history-unreadable`, and those of the sections below: `retirement-unfinished`,
`cleanup-beside-trust-findings` and `view-creation-unclear`) are printed with their meaning by `inklkunn wiki recover <wiki> --plan`. Each one is a state
where the witnesses disagree or recovery has no answer; the principle is the same: read, export, decide, and do not
guess. Two states have no word, because recovery cannot even read them; they are the next two sections.

## A `seal` that was killed after it published its block: finished by recovery

Not a stop, and nothing for you to do: this is what recovery does by itself, so that you know what its plan means.
A `seal` publishes the block and then empties the queue. Killed between the two, it leaves the block in the history
**and** the patch in the queue. `prikk status --format json` then refuses (`integrity error`), and `prikk doctor`
reports exactly one error, `PRIKK-VERIFY-REF-ACTIVE-CLEANUP-PENDING`, whose recommendation is to run `prikk seal
--allow-no-audit` for the ref. The plan shows it as the step *run seal --allow-no-audit once, as prikk's doctor
recommends* (`complete-publication`), and recovery takes it **only when** `doctor` names that code and nothing
else, and the block that was published is the unfinished act's own: the one patch it seals has the act's message
and files, to the digest, and sits on the block the act was made on. The step makes no block (the ref is already
published); recovery checks that the tip is the same block afterwards, and that `doctor` no longer reports the
cleanup, **and that the queue is exactly that one patch**: `status --format json` refuses in this state, but the plain
text of `prikk status` reads (it prints `queued patches: N targeting REF` and `active WAL records: N`), and the step is
refused unless it shows one patch on the view in a log of one record with no torn byte (it does not show *which* patch;
that is a question for prikk). Any other code beside it is `doctor-trouble`; a block that is not the act's, or no unfinished act, is the
stop `published-block-is-not-the-act`, with nothing changed. For that stop, look as under `foreign-queued-patch`
(`prikk log` shows the newest block; `prikk verify` says whether the repository is sound) and do not run `prikk
seal` yourself.

## A key retirement that was begun and not finished: `retirement-unfinished`

**What you see.** The plan says `STOP (retirement-unfinished)` and names the key and the commands, when a patch waits to
be sealed, when an interrupted view creation waits to be completed (both sign a record) or when `prikk doctor` reports `PRIKK-TRUST-PUBLICATION-UNTRUSTED` errors for blocks, ref states and ref
updates a retired key sealed. Acceptances on the web or through the API are answered *a change to this wiki's keys is
not finished* (the proposal is kept). The key record holds the state a retirement began from but not its findings
(`rotate --retire-old` stopped half way, or was killed, and said so with a non-zero exit when it could). **What it is.**
The retirement was begun and not ended: the old key may still be trusted (the removal never happened), or be out of the
repository's trust with nothing yet saying which failures are its. **Nothing new is sealed while it lasts** (the signer
refuses every acceptance, every `seal` of a queued patch and every new view, before it changes anything), because a
block written now could not be told from what the removal made fail. **What to do.** Run one of two commands (they need
the operator's secret, as every key command does; **recovery never runs them**):

```sh
inklkunn wiki keys finish-retirement <wiki>                    # does whatever step remains, the removal of the key included
inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>  # gives it up: the old key is trusted again, nothing is retired
```

Abandoning leaves the old key trusted and retires nothing; `inklkunn wiki keys retire <wiki> --key <id> --confirm <slug>
--accept-unverifiable-history` retires it later, by its id (`docs/KEYS.md`).

After either command, run `inklkunn wiki recover <wiki>`. You never run `prikk trust maintainer` yourself. If `finish-retirement` refuses with
a reason no later run can cure (the repository changed outside inklkunn, the trusted keys are not the ones the retirement
began with, a block that lost its signature was not the old key's, prikk counted something different), it says so and
names `abandon-retirement`; do not edit the key record. See `docs/KEYS.md`.

**Findings that cannot be traced: recovery does not cure them.** When the retirement's `verify` after the removal names an
act the service did not make with the old key (a block sealed with it after the rotation that replaced it, a ref state
from a `branch close` or `branch create`, a tag), `retire` / `rotate --retire-old` / `finish-retirement` record nothing and
the retirement stays unfinished: the plan stops at `retirement-unfinished` (never at `doctor-trouble`), and the stop and
`status` carry one sentence: *the repository holds acts with this key that cannot be traced to what it sealed for this
service; nothing excuses them, and this service seals nothing more until the history is restored from before them
(`docs/KEYS.md`, compromise) or the retirement is given up with `abandon-retirement`, which trusts the key again*.
`inklkunn wiki recover` cannot cure this, and no text tells you to run it for it; `finish-retirement` refuses with the
same findings. The way out is in `docs/KEYS.md` (the compromise runbook, step 4): restore the repository, the key
directory and the store together from a backup taken before the act, or give the retirement up.

## A killed `seal` on a wiki with a retired key: finished by recovery, or `cleanup-beside-trust-findings`

**What you see.** The pending cleanup of the section above (a block published, its patch still queued) on a wiki whose
retirement is recorded. **What it is.** While the cleanup is pending `prikk verify` fails by two conditions
(`blocking-ref-publication` and `publication-trust`). Recovery finishes the cleanup (`complete-publication`, as above)
**only when** `verify` fails by exactly those two, **every** publication-trust finding is in a retirement's recorded set,
`doctor` shows nothing but the cleanup once those findings are taken out, and the published block is the unfinished act's;
the rule that excuses a failure (publication trust alone, all of it recorded) and the rule an acceptance confirms by do not
change. **After the step** the ordinary rule must hold: `verify` fails by publication trust alone, and all of it recorded;
otherwise recovery stops and the flag stays. **When it stops here** (`STOP (cleanup-beside-trust-findings)`), a
publication-trust finding is not in any retirement's record, or something else fails besides: nothing is changed, and it
is for the architect to decide. Do not run `prikk seal` yourself.

## An empty lock file: recovery cannot read the witness

**What you see.** `inklkunn wiki recover <wiki>` (or `--plan`) does not make a plan. It ends with `a witness could
not be read, so nothing is concluded: prikk unlock: … a recorded pid is not a number`, and the wiki stays
flagged. **What it is.** A process that was killed the instant prikk created a lock file, before prikk wrote a word
into it. `prikk unlock` lists it with `recorded pid: (unparseable)` (0.55.0, 0.56.0) or `recorded pid: (not read: the lock body is incomplete)` (0.57.0 and later). Recovery does not know
what the lock belongs to, so it clears nothing; nothing was changed.

**Before.** From the repository directory (as under `foreign-queued-patch`):

```sh
prikk unlock                       # the lock, with `recorded pid: (unparseable)` or `(not read: …)`; copy the path it prints
ls -l <that path>                  # size 0: the file was never written to
ps ax | grep -E 'prikk|bwrap'      # no prikk or bwrap of anybody is working in this repository
```

If the file is not empty, or any process is working in the repository, this is not the state; stop here and read
`foreign-process` instead. **The step.**

```sh
prikk unlock --lock <that path>    # asks you to confirm; clears that one file and nothing else
```

(`--yes` skips the question; `--force` is not needed and is not for a lock a running process holds.) **After.**
`prikk unlock` prints `no locks currently held`, `prikk status` and `prikk verify` read as before, and `inklkunn
wiki recover <wiki> --plan` now makes a plan; run `inklkunn wiki recover <wiki>`. Nothing in the history was
touched at any point.

## A lone `ACTIVE-REF-METADATA-DEBRIS` warning: recovery stops at `doctor-trouble`

**What you see.** The plan says `STOP (doctor-trouble)`. **What it is.** A `seal` killed just after it published
its block, when the queue was already emptied and the ref's metadata was not: `prikk doctor` reports one warning,
`PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS` (*active WAL is empty but stale ref metadata remains*), with the
recommendation *no repair is required; the next guarded active-WAL append will replace stale metadata*. The history
is sound and complete; what is unfinished is the service's records of the act, and recovery will not finish them
beside a warning it has no answer for. prikk 0.55 has no verb for this metadata.

**Before.** From the repository directory, check that it is only that:

```sh
prikk doctor                       # one warning, PRIKK-DOCTOR-ACTIVE-REF-METADATA-DEBRIS, and nothing else but `info`
prikk status --format json         # queue.count is 0
prikk verify                       # passes (exit status 0)
prikk log --limit 3                # the newest block is the change the interrupted acceptance meant
```

**What you may do.** Nothing changes the repository: there is no prikk verb that removes the metadata, and the
next guarded append that would replace it is a commit, which only the service makes, after recovery. So the wiki
waits, readable and exportable, as under `foreign-queued-patch` (keep the evidence; or make a new wiki from the
export if it must take writes). Do **not** delete files under `.prikk/active/`, run `prikk commit` or `prikk seal`
by hand to make the metadata go away, or use any `prikk doctor` repair: each acts on the repository without the
service's record. **After.** Nothing changed: the plan stops at `doctor-trouble` again until prikk answers (this is
one of the two states asked of prikk in the next letter, `docs/UPSTREAM.md`).

## A view whose creation was killed: finished by recovery, or `view-creation-unclear`

**What you see.** After the service died while a reviewer was creating a view (an alternative view of the wiki), the
wiki is flagged and `inklkunn wiki recover <wiki> --plan` says `view creation <id> for heads/<name> never ended`,
beside what prikk shows of the view and whether `doctor` names its publication. Nothing else is wrong with the wiki.
**What it is.** `prikk branch create` publishes the new ref in two writes: the ref's pointer, then the ref log.
Killed between them it leaves the view **listed** (`prikk branch list` shows it), `prikk verify` failing, and `prikk
doctor` reporting exactly one error, `PRIKK-VERIFY-REF-POINTER-LEADS-LOG` (*authoritative pointer leads committed ref
log by one transition*), whose recommendation is ``run `prikk ref complete heads/<name>` ``. While it stands, prikk
refuses every other publication, a seal included, so no acceptance can run: that is why the wiki is flagged. (A kill
before the pointer leaves nothing in the repository; a kill after both writes leaves a whole view. Recovery tells the
three apart from what prikk shows, never from the listing alone, because the listing shows a half-published view
as it shows a whole one.)

**What recovery does by itself.** Not a stop, and nothing for you to do. Recovery reads the act's record (the name, the
view it was forked from, and the block that view was at), `prikk branch list --all` (closed views included) and
`prikk doctor`, and takes exactly one of these:

- the view is listed at the block it was forked from and `doctor` names nothing unfinished: the act ends *created*
  (`end-view-created`);
- the same, with `doctor` naming exactly that one ref's publication and nothing else: the step `complete-view-creation`,
  which is prikk's own recommendation (`prikk ref complete heads/<name>`, not `seal`), run by the signer once, and then
  the act ends *created*. The signer does it only when the recovery act records that very step for that very creation,
  `doctor` names exactly that lead, and prikk's own `ref complete heads/<name> --plan-only` says it will append the
  first record of the ref (no log tip, sequence 1, nothing to remove first) onto the block the creation's record names as
  the view's start (the block its source pointed at when the act was written, not wherever the source points later);
  afterwards `doctor` must no longer name it. It makes no block, and a key retirement that is not finished pauses
  it as it pauses a seal;
- the view is not listed and `doctor` names nothing unfinished: nothing happened, and the act ends *failed*
  (`interrupted`; `end-view-failed`), which frees the name.

**`STOP (view-creation-unclear)`.** Anything else is a stop, with the repository exactly as recovery found it: the
view is listed at **another block** than the one it was forked from, or its block cannot be read, or the view is not
listed while `doctor` names its publication, or a patch is queued, a seal's cleanup is pending or a commit is torn
beside it. (A second error in `doctor`, or a lead for another ref, is `doctor-trouble`; two creations that never
ended are `several-acts`.) The act stays open and the wiki stays flagged.

**Look, without changing anything.** From the repository directory (as under `foreign-queued-patch`):

```sh
inklkunn wiki recover <wiki> --plan   # the act, what prikk shows, and the STOP with its meaning
prikk branch list --all               # the view, with its closed mark; the name is never made again if it is closed
prikk doctor                          # the error PRIKK-VERIFY-REF-POINTER-LEADS-LOG, if the publication is unfinished
prikk ref complete heads/<name> --plan-only   # what prikk would append: the ref, the target block, the sequence; writes nothing
prikk log --ref heads/<name> --limit 1        # the view's newest block: it must be the one the plan names
```

**What you may do.** If the output shows the lead for exactly that view at the block it was forked from, the stop is not
this one: run `inklkunn wiki recover <wiki>` and recovery takes the step. For the stop itself, keep the output above
and wait, as under `foreign-queued-patch`: the wiki serves every read, the history is untouched, and `inklkunn wiki
export` writes it to somewhere safe meanwhile. Which block the view should have is a decision about the wiki's
records, for the architect, who needs that output. **Do not** run `prikk ref complete`, `prikk branch close` or any
`prikk doctor` repair yourself on a wiki the service runs: recovery runs the first only bound to its recorded act,
and none is a step to take beside a service that may be writing. **After.** Nothing changed: the plan stops at
`view-creation-unclear` again until the state is one of the three above.
