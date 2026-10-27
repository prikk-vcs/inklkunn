#!/usr/bin/env python3
"""Spike A (RFC 001 D-10 a): per-verb peak memory and latency of prikk's read verbs on generated wikis.

usage: spike-a.py --prikk PATH --repo DIR [--repo DIR ...] --scratch DIR [--runs N] [--bisect] --out report.json

For each repository and each read verb the driver uses, the verb runs `N` times in a fresh child whose
peak resident set is read with getrusage(RUSAGE_CHILDREN) and whose wall time is measured; the table keeps
the maximum RSS and the median time. With --bisect, the smallest RLIMIT_DATA (bytes, to 256 KiB) at which
the verb still succeeds is found by bisection, because the driver's ceiling is RLIMIT_DATA, not RSS, and
the two differ (planeter measured 3.6 x versus 3.1 x on `show`).

Measurement only: no product code; the result rows go into the cost table of inklkunn-prikk and the ledger.
"""
import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile

WRAPPER = (
    "import resource,subprocess,sys,time,json;"
    "t=time.monotonic();"
    "r=subprocess.run(sys.argv[1:],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);"
    "ru=resource.getrusage(resource.RUSAGE_CHILDREN);"
    "print(json.dumps({'rss_kib':ru.ru_maxrss,'secs':time.monotonic()-t,'code':r.returncode}))"
)


def prikk_json(prikk, repo, args):
    out = subprocess.run([prikk, *args, "--format", "json"], cwd=repo, capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


def store_size(repo):
    total = 0
    for root, _dirs, files in os.walk(os.path.join(repo, ".prikk")):
        for f in files:
            total += os.lstat(os.path.join(root, f)).st_size
    return total


def largest_page(repo):
    best = (0, None)
    for root, _dirs, files in os.walk(os.path.join(repo, "pages")):
        for f in files:
            p = os.path.join(root, f)
            s = os.stat(p).st_size
            if s > best[0]:
                best = (s, os.path.relpath(p, repo))
    return best[1]


def measure_once(argv, cwd, data_limit=None):
    cmd = [sys.executable, "-I", "-c", WRAPPER]
    if data_limit is not None:
        cmd = ["prlimit", f"--data={data_limit}", "--", *cmd]
    out = subprocess.run(cmd + argv, cwd=cwd, capture_output=True, text=True)
    try:
        return json.loads(out.stdout.strip().splitlines()[-1])
    except (IndexError, json.JSONDecodeError):
        return {"rss_kib": 0, "secs": 0.0, "code": 255}


def bisect_data(build_argv, cwd, lo, hi):
    """Smallest RLIMIT_DATA in [lo, hi] (to 256 KiB) at which the verb succeeds; None when hi fails too.
    `build_argv` is called for every probe, so verbs that need a fresh target directory get one."""
    step = 256 * 1024
    if measure_once(build_argv(), cwd, hi)["code"] != 0:
        return None
    while hi - lo > step:
        mid = (lo + hi) // 2
        if measure_once(build_argv(), cwd, mid)["code"] == 0:
            hi = mid
        else:
            lo = mid
    return hi


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--prikk", required=True)
    ap.add_argument("--repo", action="append", required=True)
    ap.add_argument("--scratch", required=True)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--bisect", action="store_true")
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    version = subprocess.run([a.prikk, "--version"], capture_output=True, text=True, check=True).stdout.strip()
    report = {"prikk": version, "repos": []}
    for repo in a.repo:
        s = store_size(repo)
        log = prikk_json(a.prikk, repo, ["log"])
        blocks = log["blocks"]
        tip, root = blocks[0]["block_id"], blocks[-1]["block_id"]
        page = largest_page(repo)
        tree = prikk_json(a.prikk, repo, ["tree"])
        verbs = {
            "branch": ["branch", "list", "--format", "json"],
            "tree": ["tree", "--format", "json"],
            "cat": ["cat", "--path", page, "--format", "json"],
            "cat-bytes": ["cat", "--path", page, "--max-bytes", "1073741824"],
            "log": ["log", "--format", "json"],
            "show": ["show", tip, "--format", "json"],
            "diff": ["diff", "--from", root, "--to", tip, "--format", "json"],
            "extract": None,  # built per run: a fresh target directory
            "verify": ["verify", "--format", "json"],
        }
        rows = {}
        for verb, argv in verbs.items():
            extract_dir = {"n": 0}

            def fresh():
                extract_dir["n"] += 1
                d = os.path.join(a.scratch, f"x-{os.getpid()}-{extract_dir['n']}")
                shutil.rmtree(d, ignore_errors=True)
                return d

            def build_argv():
                if verb == "extract":
                    return ["extract", "--ref", "heads/main", fresh(), "--format", "json"]
                return argv

            samples = []
            for _ in range(a.runs):
                argv_now = [a.prikk, *build_argv()]
                samples.append(measure_once(argv_now, repo))
            rss = max(x["rss_kib"] for x in samples) * 1024
            secs = statistics.median(x["secs"] for x in samples)
            codes = sorted({x["code"] for x in samples})
            row = {"rss_bytes": rss, "rss_over_store": round(rss / s, 2) if s else None, "median_secs": round(secs, 4), "exit_codes": codes}
            if a.bisect and codes == [0]:
                lo, hi = 256 * 1024, max(rss * 8, 64 * 1024 * 1024)
                need = bisect_data(lambda: [a.prikk, *build_argv()], repo, lo, hi)
                row["data_limit_need_bytes"] = need
                row["data_over_store"] = round(need / s, 2) if (need and s) else None
            rows[verb] = row
            print(f"{os.path.basename(repo):32} {verb:10} rss={rss/1048576:8.1f} MiB  ({row['rss_over_store']} x S)  t={secs:.3f}s  data_need={row.get('data_limit_need_bytes')}", file=sys.stderr)
        for d in os.listdir(a.scratch):
            if d.startswith(f"x-{os.getpid()}-"):
                shutil.rmtree(os.path.join(a.scratch, d), ignore_errors=True)
        report["repos"].append({
            "repo": repo,
            "store_bytes": s,
            "files": len(tree["entries"]),
            "blocks": len(blocks),
            "largest_page": page,
            "verbs": rows,
        })
    with open(a.out, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2)
    # Markdown table
    print("\n| repository | S (MiB) | files | " + " | ".join(verbs.keys()) + " |")
    print("|---|---|---|" + "---|" * len(verbs))
    for r in report["repos"]:
        cells = []
        for v in verbs:
            row = r["verbs"][v]
            need = row.get("data_limit_need_bytes")
            cells.append(f"{row['rss_bytes']/1048576:.0f} MiB ({row['rss_over_store']}x){' / data ' + format(need/1048576, '.0f') + ' MiB (' + str(row.get('data_over_store')) + 'x)' if need else ''} {row['median_secs']}s")
        print(f"| {os.path.basename(r['repo'])} | {r['store_bytes']/1048576:.1f} | {r['files']} | " + " | ".join(cells) + " |")


if __name__ == "__main__":
    main()
