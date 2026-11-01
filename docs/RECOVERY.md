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
the wiki's name, that nobody accepted. prikk 0.55 has no verb that discards a sound queued patch (a patch that is
not damaged), so recovery cannot remove it either. It stops and tells you. **A queued patch is not in the
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
3. **Remove the patch when prikk can.** prikk will have a verb that discards a sound queued patch on purpose,
   named exactly, with a preview, and with a record so the discard can be undone, in **prikk 0.57.0** (ledger
   PK-43). When inklkunn moves to that version, recovery offers the discard as a step **you confirm**, never one
   it takes by itself, and this page gains the command. Until then there is no way to remove the patch, and the
   wiki is meant to wait.
4. **If the wiki must take writes before that**, make a new wiki from the old one's pages: export it as above,
   create a wiki (`inklkunn wiki create`), and `inklkunn wiki import-folder` the exported `pages/` and
   `attachments/` into it. This does not touch the old wiki, which stays flagged, readable and exportable as the
   record of its history; the new wiki's history begins with the import, so it has a new slug and new keys, and
   you decide what happens to the old address.

**What not to do.** Each of these is an action on the repository that the service did not sanction, and each
either rewrites the history or puts an unaccepted change into it:

- `prikk seal` (it publishes the queued patch, signed by whatever key is at hand);
- `prikk rollback-draft`, `prikk rollback-preview` and the rest of the rollback family (they add to what is
  queued; an undo in this wiki is a proposal, RFC 004 D-8);
- `prikk doctor --discard-damaged-commits` or any other repair, which prikk refuses over a patch that is sound,
  and which would be the wrong tool if it did not;
- deleting files under `.prikk/` or in the worktree, and `prikk unlock --force` on a lock a running process
  holds.

The other STOP words (`queued-patch-is-not-the-act`, `published-block-is-not-the-act`, `several-queued`,
`several-acts`, `tip-moved`, `interrupted-state`, `doctor-trouble`, `unknown-lock`, `foreign-process`,
`history-unreadable`) are printed with their meaning by `inklkunn wiki recover <wiki> --plan`. Each one is a state
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
cleanup. Any other code beside it is `doctor-trouble`; a block that is not the act's, or no unfinished act, is the
stop `published-block-is-not-the-act`, with nothing changed. For that stop, look as under `foreign-queued-patch`
(`prikk log` shows the newest block; `prikk verify` says whether the repository is sound) and do not run `prikk
seal` yourself.

## An empty lock file: recovery cannot read the witness

**What you see.** `inklkunn wiki recover <wiki>` (or `--plan`) does not make a plan. It ends with `a witness could
not be read, so nothing is concluded: prikk unlock: … a recorded pid is not a number`, and the wiki stays
flagged. **What it is.** A process that was killed the instant prikk created a lock file, before prikk wrote a word
into it. `prikk unlock` lists it with an empty `kind:` and `recorded pid: (unparseable)`. Recovery does not know
what the lock belongs to, so it clears nothing; nothing was changed.

**Before.** From the repository directory (as under `foreign-queued-patch`):

```sh
prikk unlock                       # the lock, with `recorded pid: (unparseable)`; copy the path it prints
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
