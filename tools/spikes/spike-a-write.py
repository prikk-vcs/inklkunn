#!/usr/bin/env python3
"""Spike A for the WRITE verbs (corrections handoff 002 C1, follow-up F1; ledger PK-38): the smallest `RLIMIT_DATA`
at which each verb the signer runs still succeeds, over a fresh copy of the repository per probe, so that the
signer's cost table (`crates/inklkunn-signer/src/cost.rs`) can be re-measured at every re-baseline
(`docs/UPSTREAM.md` step 5).

usage: spike-a-write.py --prikk PATH --keys DIR --repo DIR [--repo DIR ...] --scratch DIR [--add-mib N ...] [--out report.json]

Each `--repo` is a prikk repository with a worktree (the wiki-shaped corpora of `inklkunn-corpus`, or the
single-attachment shapes of `make-attachment-shape.sh`). The verbs and what they run on:

  worktree-status, verify, branch switch   the repository as it is (switch: to a second view made for the probe)
  commit                                   after a new file of N MiB was written into the worktree, for each
                                           `--add-mib N` (the "7 x the bytes of a file it adds" rule), and, with
                                           no addition, after a one-line edit of an existing page (the floor)
  seal                                     after that commit (the "2 x the bytes it seals" rule)

`--keys` is the throwaway HOME the repositories were made with (`inklkunn-corpus wiki --keys DIR`,
`make-attachment-shape.sh SIZE OUT KEYS`): its key pair is the one the repositories trust. It is never a real key
directory. A verb that signs gets `HOME` pointed there. Measurement only: no product code. The result rows go
into `cost.rs` and the ledger; the output of the run recorded beside spike A's data is in
`rfcs/handoffs/001-foundations/spikes/`.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys

MIB = 1 << 20


def run(argv, cwd, env=None, data_limit=None):
    if data_limit is not None:
        argv = ["prlimit", f"--data={data_limit}:{data_limit}", "--", *argv]
    return subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True)


def store_size(repo):
    total = 0
    for root, _dirs, files in os.walk(os.path.join(repo, ".prikk")):
        for f in files:
            total += os.lstat(os.path.join(root, f)).st_size
    return total


def fresh_copy(src, scratch):
    dst = os.path.join(scratch, "probe")
    shutil.rmtree(dst, ignore_errors=True)
    shutil.copytree(src, dst, symlinks=True)
    return dst


def first_page(repo):
    for root, _dirs, files in os.walk(os.path.join(repo, "pages")):
        for f in sorted(files):
            if f.endswith(".md"):
                return os.path.join(root, f)
    return None


def prepare(prikk, name, repo, env, add_mib):
    """Bring a fresh copy to the state the verb runs in; return the argv of the verb under test."""
    if name == "worktree-status":
        return [prikk, "worktree-status", "--format", "json"]
    if name == "verify":
        return [prikk, "verify", "--format", "json"]
    if name == "branch switch":
        if run([prikk, "branch", "create", "heads/probe", "--from", "heads/main"], repo, env).returncode != 0:
            return None
        return [prikk, "branch", "switch", "heads/probe"]
    # commit / seal: write the change the acceptance would
    if add_mib:
        with open(os.path.join(repo, "pages", "added.bin"), "wb") as f:
            f.write(os.urandom(add_mib * MIB))
    else:
        page = first_page(repo)
        if page is None:
            return None
        with open(page, "a") as f:
            f.write("\nA line added for the probe.\n")
    if name == "commit":
        return [prikk, "commit", "-m", "probe"]
    if run([prikk, "commit", "-m", "probe"], repo, env).returncode != 0:
        return None
    return [prikk, "seal", "--allow-no-audit"]


def works(prikk, name, src, scratch, env, add_mib, limit):
    repo = fresh_copy(src, scratch)
    argv = prepare(prikk, name, repo, env, add_mib)
    if argv is None:
        return None
    return run(argv, repo, env, data_limit=limit).returncode == 0


def bisect(prikk, name, src, scratch, env, add_mib, hi):
    ok = works(prikk, name, src, scratch, env, add_mib, hi)
    if ok is None:
        return None
    while not ok:
        hi *= 2
        if hi > 64 * 1024 * MIB:
            return float("inf")
        ok = works(prikk, name, src, scratch, env, add_mib, hi)
    lo = 1 * MIB
    while hi - lo > 256 * 1024:
        mid = (lo + hi) // 2
        if works(prikk, name, src, scratch, env, add_mib, mid):
            hi = mid
        else:
            lo = mid
    return hi


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--prikk", required=True)
    ap.add_argument("--repo", action="append", required=True)
    ap.add_argument("--keys", required=True)
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--add-mib", type=int, action="append", default=[])
    ap.add_argument("--out")
    args = ap.parse_args()
    prikk = os.path.abspath(args.prikk) if os.sep in args.prikk else args.prikk
    scratch = os.path.abspath(args.scratch)
    os.makedirs(scratch, exist_ok=True)
    env = dict(os.environ, HOME=os.path.abspath(args.keys))
    results = []
    for repo in args.repo:
        repo = os.path.abspath(repo)
        s = store_size(repo)
        print(f"# {repo}  S = {s / MIB:.1f} MiB")
        plan = [("worktree-status", 0), ("verify", 0), ("branch switch", 0), ("commit", 0), ("seal", 0)]
        plan += [(v, n) for n in args.add_mib for v in ("commit", "seal")]
        for name, add in plan:
            need = bisect(prikk, name, repo, scratch, env, add, 64 * MIB)
            row = {
                "repo": repo,
                "store_bytes": s,
                "verb": name,
                "adds_mib": add,
                "need_bytes": None if need is None else (need if need != float("inf") else -1),
            }
            results.append(row)
            if need is None:
                print(f"  {name:<16} adds {add:>4} MiB  (could not be prepared)")
            else:
                print(
                    f"  {name:<16} adds {add:>4} MiB  need {need / MIB:>8.1f} MiB  "
                    f"{need / max(s, 1):>5.2f} x S" + (f"  {need / (add * MIB):>5.2f} x added" if add else "")
                )
    shutil.rmtree(os.path.join(scratch, "probe"), ignore_errors=True)
    if args.out:
        with open(args.out, "w") as f:
            json.dump({"prikk": subprocess.run([prikk, "--version"], capture_output=True, text=True).stdout.strip(), "rows": results}, f, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
