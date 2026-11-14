# A wiki's keys: status, rotation and the compromise runbook

inklkunn signs changes on behalf of its wikis (SECURITY.md; threat model RR-1). Each wiki has an **author key**
and a **maintainer key**, held by the service sealed under the operator's secret (RFC 002 D-3). This page is
for the operator: the commands that show and change them, what each one costs, and what to do when a key may
have leaked. Nothing here prints a secret, and nothing here can.

## The commands

Run on the host that runs the service, as the user that owns its data directory, with the same settings as
the service (`INKLKUNN_DATA_DIR`, `INKLKUNN_SECRET_FILE`, and the optional `INKLKUNN_KEY_DIR`,
`INKLKUNN_SEED_DIR`, `INKLKUNN_PRIKK`; `src/config.rs` documents them):

```sh
inklkunn wiki keys status <wiki>                       # key ids, generation, dates; each superseded key: retired or not, trusted by the repository or not
inklkunn wiki keys create <wiki>                       # keys for an initialised repository that has none
inklkunn wiki keys rotate <wiki> --confirm <slug>      # new keys; the old maintainer key stays trusted
inklkunn wiki keys rotate <wiki> --confirm <slug> --retire-old --accept-unverifiable-history
inklkunn wiki keys retire <wiki> --key <maintainer key id> --confirm <slug> --accept-unverifiable-history   # retire a superseded key that is still trusted, by its id
inklkunn wiki keys finish-retirement <wiki>            # finish a retirement that stopped half way, from whatever step remains
inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>   # give such a retirement up: the old key is trusted again, nothing is retired
```

`<wiki>` is the slug or the id. **A key directory with no key store in it is never made into one by `status` or `rotate`:** they refuse, naming the path, so a mistyped `INKLKUNN_KEY_DIR` cannot start a second, empty store; only `create` (and creating a wiki) makes the store, and two processes doing so at once end with one. **Re-authentication:** the key store does not open without the operator's
secret file, so these commands are the operator proving possession of it; the web administration surface will
ask for the password again instead. `rotate` also needs the slug typed, so the wrong wiki is not rotated by a
slip. `create`, `rotate`, `retire`, `finish-retirement` and `abandon-retirement` are **audited**: an event is written *before* the act (and the act is
refused if it cannot be written) and another after it (`system:operator-cli`, `keys-create` / `keys-rotate` / `keys-retire` /
`keys-finish-retirement` / `keys-abandon-retirement`, with what became of the old key); a trail that shows "begun" and no ending means the process stopped in the middle.
The ending of a `rotate --retire-old`, a `retire` and a `finish-retirement` whose record was made begins with the retired key's id in one
fixed form (`retirement recorded: key <id>`): it is how `abandon-retirement` knows a retirement finished (below). Read
the trail with `inklkunn audit export`. If a retirement fails after the keys rotated, the command exits 1 and says what
became of the old key, and **each outcome names its own command**:

| What the command says | The old key is | Nothing new is sealed? | The command names |
|---|---|---|---|
| the retirement was **not begun** (the key record could not take its state, or the trust list could not be read) | still trusted | no, nothing is paused | `retire` (below) |
| removing the key **failed** (the retirement stays begun) | still trusted | yes, until the retirement ends | `finish-retirement`, which removes the key itself, and `abandon-retirement` |
| the key was **removed but its record could not be made** | already out of the trust store | yes, until the retirement ends | `finish-retirement` and `abandon-retirement` |

`status` reads the repository's trust list and says for each superseded key whether the key record marks it retired and
whether the repository still trusts it (the two differ while a retirement is unfinished). Nothing needs `prikk` to be run
by hand.

The seed directory (`INKLKUNN_SEED_DIR`) must be on a tmpfs, owned by the user the service runs as, and not
writable by anyone else (mode 0700 is right); the signer refuses a directory that others can write to, since
they could swap a call's directory for their own. The default `/dev/shm/inklkunn` is made that way.

## What a rotation does

prikk binds a key id to one public key for good and has no key rotation of its own (ledger PK-35), so a
rotation is a **new pair of key ids**: `inklkunn-<wiki id>-author-<generation>-<8 hex>` and `…-maintainer-…`.
New keys are generated on the tmpfs, the new maintainer key is adopted in the wiki's repository, and only then
is the sealed record replaced; a failure before that leaves the wiki exactly as it was. From then on changes
are signed with the new keys. A change being accepted at that moment, in this process or another, holds the wiki's write lock: the rotation is refused at once (`wiki … is being written by process N`) and changes nothing; run it again when that is over.

## Keep the old key, or retire it

This is the choice, and prikk 0.54.1 makes it a hard one (ledger **PK-34**):

| | The old maintainer key is… | History it sealed | A copy of its seed can… |
|---|---|---|---|
| **default** | still trusted in the repository | keeps verifying | still seal changes that verify |
| `--retire-old` | removed from the repository's trust | **stops verifying**: `prikk verify` fails with publication-trust issues and the wiki shows *Verification failed* | seal nothing |

**A rotation of either kind is refused, and changes nothing, while `doctor` names a publication a kill left unfinished** (a seal's cleanup, or a view's creation: the record completing it appends is signed by whichever maintainer key is current, so recovery finishes it first); the error names `inklkunn wiki recover <wiki>`, and the rotation is run again after.

Use the default for a routine rotation (a schedule, a change of operator): the service no longer holds the
old seed, and nothing else is known to. Use `--retire-old` when the old seed may be in someone else's hands,
and accept that the wiki will report *Verification failed* until prikk can keep a retired key's history
verifiable. **The wiki goes on taking changes** (handoff 004, P2 and P2′): the rotation first runs `prikk verify` and
refuses to retire (the old key stays trusted; the error says the wiki needs recovery first) unless the repository
verifies; it removes the key from trust; it runs `verify` again and records in the key record the blocks, ref states and
ref updates that lost their trusted signature. An acceptance whose `verify` fails by publication trust and by nothing
else, naming only what the record holds, is confirmed, and recovery's `verify` step passes with a note; the
verification page says the only failures are those the retirement of that key recorded. Anything else still stops the
wiki: another failed condition, and a block, ref state or ref update the record does not hold, whoever signed it and
wherever it sits in the history (a branch published or a rollback made after the retirement with the old seed, a
block of another key that was taken out of trust later). The second flag exists so that cost cannot be paid by accident. The owner ruled this design
(DR-10, 2026-10-29) after prikk **confirmed** it on the same day: a leaked copy of a still-trusted key can
seal anything that verifies, and a rotation that keeps history verifiable while removing the key **is not
scheduled** in prikk.

### What a default rotation still tells you: seals made after the rotation

A copy of the repository cannot prove that a block was sealed by the old key *after* the new one was adopted
(adoption is local and recorded nowhere a copy can check). inklkunn can, because it knows the moment. At the
commit point of every rotation the key record keeps, for the replaced maintainer key, **the newest block of
every view**, and `inklkunn wiki keys status <wiki>` (and every rotation, which also writes a `keys-late-seal`
event to the audit trail for each finding) lists the superseded keys and **reports every block one of them
sealed that is not in the history up to those points**: *sealed by a superseded key after the rotation*. It
changes nothing by itself, and `status` exits non-zero when it finds one, and whenever a retirement is unfinished (writes are paused). prikk lists who sealed a block only for keys
it still trusts, so **the check cannot see a key that is out of the trust store**: for such a key `status` says so
instead of saying that no block was sealed after the rotation, and for the key of an unfinished retirement it prints the
untraceable findings (below). A record that calls a key retired while the repository trusts it again means the repository is
older than the key record (restored on its own): `status` says to restore the key directory from the same backup and
retire the key again.

**What it covers:** a leaked seed used against the wiki's own repository while the service's records are intact
(a leaked backup, a leaked seed file). **What it does not:** a host that has been taken over (the records are
the attacker's then; that is what `--retire-old` is for), a view's tip recorded before it existed (an
empty tip: every block of that view is then "after"), and, **with prikk 0.55.0 or 0.56.0**, a repository that no
longer verifies (after `--retire-old` those versions print no list of who sealed what, ledger PK-39, so the check says it
could not run). **With prikk 0.57.0 or later the check reads who sealed each block from `verify --format json`
(`block_seals`, the key ids as the seals record them, trusted or not), so it also runs after a retirement and can
name a block the retired key sealed after its replacement.** The list is used only where it is whole: when
every stage is evaluated, `failed_objects` is empty (prikk's own rule: a damaged Block is missing from the list and named
in `failed_objects`; a failure found after the objects stage does not shorten the list), and verification holds or fails
on publication trust alone (the retired key). When `verify` fails on anything else (a block or a ref that cannot be
read, a stage that failed or did not run) the list may leave blocks out without saying so, so the check says it could not
run (it never answers "no late seals" from a list that may be short) until the repository is repaired. inklkunn's rule is
stricter than prikk's until prikk's reference states its own. `status` says which: it prints `this check cannot see <key>` only when the
check could not.

## If a wiki's keys may have leaked (the compromise runbook)

1. **Look first** (`inklkunn wiki keys status <wiki>`): a block sealed by a superseded key after its rotation
   is evidence that an old seed is in use. It is evidence of a leaked *copy* only if the service's own records
   are intact; if the host was taken over, trust nothing this report says.
2. **Decide whether the seeds could have left the host.** The seeds are decrypted only for one call, on a
   tmpfs, and stored sealed otherwise; an attacker needs the key records **and** the operator's secret, or a
   running service to ride. If only the operator's secret leaked and the key records did not, rotate (default)
   and change the secret. If the records and the secret both leaked, or the host was taken over, the seeds are
   gone: continue.
3. **Stop signing** (stop the service, or the wiki's writes) and note the time the compromise could have started.
4. **Rotate with `--retire-old --accept-unverifiable-history`.** The old key can seal nothing more. If a key was kept at an
   earlier rotation (or a retirement was given up, see below) and may have leaked, **retire it by its id**:
   `inklkunn wiki keys retire <wiki> --key <id> --confirm <slug> --accept-unverifiable-history` (`status` lists the
   superseded keys, whether each is retired and whether the repository still trusts it, and names this command beside each key
   that is still trusted; `retire` has the cost of `rotate --retire-old`, the loss of the *Verified* label, and the same flag). The current key cannot be retired: rotate first.
   **A retirement records only what it can trace, or nothing** (handoff 004, Z1). After the key is removed, every
   finding of `prikk verify` must be traceable to the key's own history as this service made it: a block the key sealed
   up to the newest blocks the key record kept when that key was replaced (the same set the check in `status` calls
   *known*); a ref state that is the `ref_state_id` of one of that history's log entries; ref updates as they are. A
   block sealed with the key **after** that (a late seal), a ref state no log entry shows (a `branch close`, a
   `branch create` or any other act with the stolen key that seals no block), a tag, a block that lost its signature
   and was not the key's, or a recorded tip the repository no longer shows: if **anything** is untraceable, the
   retirement **records nothing and stays unfinished**. The key stays out of the trust store (a leaked key is never kept
   trusted for it), the key record gains nothing, writes stay paused as for any unfinished retirement, and `retire`,
   `rotate --retire-old`, `finish-retirement` and the `UNFINISHED` line of `status` each name every such finding and say
   one sentence: *the repository holds acts with this key that cannot be traced to what it sealed for this service;
   nothing excuses them, and this service seals nothing more until the history is restored from before them (this
   section) or the retirement is given up with `abandon-retirement`, which trusts the key again*. Recovery stops at
   `retirement-unfinished` with the same sentence; `inklkunn wiki recover` does not cure this and no text sends you there.
   **What it means.** An act made with a key after it was replaced needs write access to the wiki's repository
   *outside this service*. Treat it as a **host compromise**: restore the repository, the key directory and the store
   **together** from a backup taken before the act (every write after the backup is lost), run `status`, and retire
   again; or give the retirement up with `abandon-retirement` and accept the leaked key as trusted (the wiki then
   verifies, and the act stays in its history). Nothing in the product excuses an act it cannot trace, and nothing
   rewrites history.
5. **Read what happened.** Export the audit trail (`inklkunn audit export`: JSON Lines, oldest first) and read
   the wiki's history with `prikk log` in its repository. Because the history the old key sealed no longer
   verifies, `prikk verify` cannot be the check any more. The reconciliation (RFC 002 D-7: every sealed change
   against an acceptance record, and the reverse) is the check that fits. The acceptance records it reads are RFC 004's, and
   they exist: every acceptance is recorded. The reconciliation runs as a step of `inklkunn wiki recover` and has no command
   or schedule of its own; for the window of a compromise, compare the history change by change with the acceptances the
   wiki shows.
6. **State what was signed during the window**, to the wiki's maintainers.
7. **Change the operator's secret** (it re-seals nothing; plan a re-key of the key records — not yet a command).

**After a retirement the service goes on taking changes** (handoff 004, P2′). Each acceptance seals, and the check that
closes it (`prikk verify`) still fails for the old key's history; the signer confirms the change when the failure is
publication trust and nothing else and names only the blocks, ref states and ref updates the retirement recorded. A
failure with anything else in it answers `SealedUnconfirmed` with the new block's ids instead of success (a test
pins this); whatever records the acceptance (RFC 004) must treat that answer as "sealed, record the block", never as a
reason to retry. A retirement made by an earlier version of inklkunn excuses nothing: every change is then answered that way.

**A retirement that stopped half way** (handoff 004, Q1, W1 – W7) does not strand the wiki, and **pauses it**.
`rotate --retire-old` (which is the rotation followed by `retire` of the key it replaced: one retirement, one code path) writes a `retiring` state into the authenticated key record *before* it removes the old key from
the repository's trust: the blocks the old key had sealed (from the `prikk verify` just before), how many records prikk
checked, the newest block of every view, the old key's **public** key and the ids of the maintainer keys the repository
trusts. If anything after that stops (the command is killed, the removal fails, `verify` cannot run after the removal,
the two looks at it disagree, prikk's count of what it checked differs, the record is too large, the key directory
cannot be written), the state stays, `rotate` exits 1 and names the way out, and **the signer seals nothing new until
the person who runs the wiki has ended the retirement** (W1): an acceptance on the web or through the API is refused
before anything is written, with the sentence *a change to this wiki's keys is not finished* (the API answers the
problem `retirement-unfinished`; the web page says the person who runs the wiki must finish it and that the proposal is
kept), recovery will not seal a queued patch, and no view is created. The proposal stays held, and can be accepted again
afterwards. A block written meanwhile could not be told from what the removal made fail, and would close the only way
out. Recovery stops with `retirement-unfinished`; `status` prints `UNFINISHED` with both commands. Ending it takes one of
two commands, each under the wiki's write lock and each audited:

```sh
inklkunn wiki keys finish-retirement <wiki>
inklkunn wiki keys abandon-retirement <wiki> --confirm <slug>
```

`finish-retirement` does **whatever step remains**. If the old key is *still trusted* (the command was killed before
the removal, or the removal failed) it takes a fresh `verify` (which must pass, as `rotate` requires), writes the state
again from it, **removes the key from the trust store itself**, and goes on. If the key is *already removed* it requires
that every view's newest block is the one the state kept and that the maintainer keys the repository trusts are exactly
the ones it trusted then, less the retired key (a key added or removed outside inklkunn in between would have its blocks
recorded under this retirement). Then it takes the `verify` after the removal and records its findings exactly as the
retirement itself would have, and clears the state. It **records nothing and keeps the state** when `verify` cannot run or
is not a clean publication-trust failure, when the count of what prikk checked differs, when the repository changed
outside inklkunn, and when **anything is untraceable** (compromise runbook, step 4: a late block, a ref state or a tag
nothing in the key's history explains, a block that lost its trusted signature and was not the old key's: a record that
left it out would look complete and would not be). A refusal that no later run can cure says so and names `abandon-retirement`.

`abandon-retirement` gives the retirement up. With the old key still trusted it clears the state; the rotation stands,
as a rotation that kept the old key. With the old key already removed it adopts the old key in the repository's trust
again, under its own id and with the public key the state kept, and then clears the state. It prints one sentence (*the
old maintainer key … is trusted again; nothing is retired; to retire it later run `inklkunn wiki keys retire <wiki> --key
<id> --confirm <slug> --accept-unverifiable-history`*). The wiki verifies again, nothing is excused and nothing needs to be. `--confirm` is the slug, as for `rotate`.

**What abandoning costs.** The old key — perhaps the leaked one — is trusted again and **nothing is retired**. A later
`rotate --retire-old` does not help: it retires the *current* key, which after an abandon is the new one. To take the old
key out of trust afterwards, run `inklkunn wiki keys retire <wiki> --key <id> --confirm <slug> --accept-unverifiable-history`
(the flag accepts the cost, as for `rotate --retire-old`: the wiki loses the *Verified* label; without it nothing is
changed): the same look before, the
same state, removal, look after and record as `rotate --retire-old`, made on that key's own entry of the key record. It
refuses the current key (rotate first), an id the key record does not know, a key already retired, a key the repository
does not trust, and any key while another retirement is unfinished.

**Abandon never trusts again a key whose retirement finished.** Before it adopts a removed key again it reads the audit
trail; if the trail records that key's retirement as finished (the record was made, by `rotate --retire-old`, `retire` or
`finish-retirement`), it refuses and changes nothing: *the key record looks older than the audit trail; restore the key
directory from a later backup*. That is the case of a key directory restored from a backup taken while the retirement was
unfinished. A trail that cannot be read is no answer: the key is not adopted either, and the command can be run again.
Events written before this rule carry no key id in the fixed form and count as *not known to be finished*. **The check
has limits:** a retirement whose ending event was never written (the process was killed after the record, or the append to the
audit trail failed), or lost when the audit store and the key directory are restored together, is invisible to it; abandon
then trusts the key again, honestly, and names `retire`.

Each of `rotate --retire-old`, `retire`, `finish-retirement` and `abandon-retirement` **ends by recording a fresh verdict** through
the check the server runs (the verification page's), so that the page never keeps saying *the only failures are those the
retirement of key … recorded* after a later retirement has made that untrue; if the check cannot run, the stored
explanation is cleared instead and the command says so.

An *author* key cannot be revoked (prikk has no author trust list): retiring it means only that the service
stops using it. That is why the maintainer key is the one that matters.

## Making a wiki, filling it, taking a copy out

```sh
inklkunn wiki create <slug> --name "<name>"            # the keys are made with the wiki; the deployment too, if it is the first
inklkunn wiki import-folder <wiki> <dir> --dry-run     # what would be written, what is refused and why; writes nothing
inklkunn wiki import-folder <wiki> <dir>               # one accepted change, Kind: import, under the system principal
inklkunn wiki export <wiki> --bundle <dir> --archive <file>
```

`create` is the act that is asked to bring a key store into being: it makes one when the key directory is new or
empty (and refuses a directory of other files, saying so). A creation that is killed leaves a mark, and the next
`create` or `serve` removes what it made, key record included, and says so on standard error. `import-folder`
refuses the whole folder when any entry is refused and names every reason; it never follows a link. An export's
destination must not exist, and both outputs are readable by you alone (they hold private pages too). The archive
is prikk's own (`prikk archive verify <file>` checks it without inklkunn). Every one of these is audited like the key
commands. A seed never appears in any of their output, in a bundle or in an archive (a test looks for the real seeds
in every file of a deployment).

**A bundle's knowledge, and the claim threads.** When a bundle's manifest declares the knowledge extension
(`extensions_used: inklkunn-knowledge`, version 1), `import-folder` also reads its `knowledge/claims/` and
`knowledge/sources/` files, and judges them as an acceptance judges a change that is not a curation proposal: a claim that
arrives with a community status, a source with a review day or an availability, or a file that names a source, a page or a
wording the wiki will not have, is refused by file and rule, and the whole import with it (`--dry-run` says so first). No
import sets what only a curation proposal sets. Removing a claim or a source file, and replacing a file that cannot be read,
are not offered in this version. The claim threads and the records of how views were made, which an export carries under `extensions/inklkunn-knowledge/`, belong to the exporting service and are not imported.

**Hold note: an import that adopts history.** An import that receives a history (`bundle import --adopt` or
`archive import --adopt` of prikk, the bundle import of RFC 007) on prikk **0.57.0 or earlier** must be followed by one
`prikk verify` of the wiki's repository. Without it every later commit reads about twice the whole history (42 MB per commit
at 20 MiB of content, measured by prikk; with it, 0.1 MB). Fixed in prikk 0.58.0. `import-folder` makes one change and
adopts no history, so it needs no `verify`.

## When the key store is lost

Restore it from a backup of the key directory (keep that backup **apart from** the operator's secret). **Back up the key
directory after every key command** (`create`, `rotate`, `retire`, `finish-retirement`, `abandon-retirement`). A backup is a
moment in time: a record restored from before a retirement finished still says that retirement is unfinished, and
`abandon-retirement` refuses to trust that key again because the audit trail knows better (above). Restore from a backup
taken after. `finish-retirement` from an older record, when the old key was already out of the trust store at the backup,
works **only if nothing was written to the wiki and no key changed since**; otherwise it refuses (the views moved, the
trusted keys differ), and so does `abandon-retirement` when the trail says the retirement finished, and the wiki stays
paused: then restore a later backup. **After restoring the key directory and the store together, run
`inklkunn wiki keys status <wiki>` and retire again whatever should be retired** (a retirement lost with the restore leaves
the key trusted, and the audit trail that would have said so was restored with it). A key directory written by a development
build in format `/4` (never released) is refused with a sentence that says so. There
is no command that makes new keys for a wiki that already had some: prikk refuses to adopt the original key id
with a different key (PK-35), and `create` says so rather than hiding it. If no backup exists, the wiki's
repository can still be read and exported; signing new changes needs a recovery design that RFC 002 does not
yet have (raised in the review request).
