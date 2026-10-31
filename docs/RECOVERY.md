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
prikk status                              # that one patch is queued, and its id
prikk show <patch id>                     # what the patch changes, file by file
prikk worktree-status                     # whether the worktree matches it
prikk log                                 # the sealed history, which the patch is not part of
```

**What you may do.**

1. **Decide whether the change is wanted.** `prikk show` is the answer. If the content is wanted, copy it out
   now (the text of the pages it changes); it is put in through the front door, as an ordinary proposal, once
   the wiki takes writes again. It is never sealed from the side.
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

The other STOP words (`queued-patch-is-not-the-act`, `several-queued`, `several-acts`, `tip-moved`,
`interrupted-state`, `doctor-trouble`, `unknown-lock`, `foreign-process`, `history-unreadable`) are printed with
their meaning by `inklkunn wiki recover <wiki> --plan`. Each one is a state where the witnesses disagree or
recovery has no answer; the principle is the same: read, export, decide, and do not guess.
