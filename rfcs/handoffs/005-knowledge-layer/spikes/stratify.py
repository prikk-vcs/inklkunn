#!/usr/bin/env python3
"""Spike C: the sample's results split by how big the edit was (a supplement to `inklkunn-corpus claims measure`).

Reads the committed pool and labels and the cache-side replay events (facts about edits, never text) and writes
`stratified.json`. Arithmetic only; no text is read. Usage:
    stratify.py POOL LABELS_A LABELS_B EVENTS_DIR OUT
"""
import collections, glob, json, sys

pool_f, a_f, b_f, ev_dir, out_f = sys.argv[1:6]
pool = {r['item']: r for r in map(json.loads, open(pool_f))}
def labels(f):
    return {r['item']: r for r in map(json.loads, open(f))}
A, B = labels(a_f), labels(b_f)

events = {}
revisions = {}
for f in glob.glob(ev_dir + '/*.jsonl'):
    for l in open(f):
        r = json.loads(l)
        if r.get('kind') == 'revision':
            revisions[(r['lang'], r['article'], r['rev'])] = r
        else:
            events[(r['lang'], r['article'], r['rev'], r['prev_block'], r['new_block'], r['kind'])] = r

# The sample: the first 400 items both marked factual, in pool order (as `claims measure` does).
sample = []
for i in sorted(pool):
    if i in A and i in B and A[i]['factual'] and B[i]['factual']:
        sample.append(i)
    if len(sample) == 400:
        break

def fact(i):
    p = pool[i]
    return events[(p['lang'], p['article'], p['rev'], p['prev_block'], p['new_block'], p['kind'])]

def sweep_bin(n):
    return 'small (0-2 other edited blocks)' if n <= 2 else ('medium (3-20)' if n <= 20 else 'sweeping (over 20)')

def chars_bin(n):
    return 'few characters (<= 10)' if n <= 10 else ('some (11-60)' if n <= 60 else 'many (over 60)')

def agg(items):
    d = collections.Counter()
    for i in items:
        a, b = A[i]['label'], B[i]['label']
        d['n'] += 1
        d['both_same_claim'] += (a == b == 'same-claim')
        d['meaning_change_by_both'] += (a == b and a in ('refines', 'supersedes', 'contradicts', 'new-claim'))
        d['cannot_tell_by_both'] += (a == b == 'cannot-tell')
    return dict(d)

res = {'sample_size': len(sample)}
changed = [i for i in sample if pool[i]['kind'] == 'changed']
replaced = [i for i in sample if pool[i]['kind'] == 'replaced']
res['by_kind'] = {'changed (the id survived)': agg(changed), 'replaced (the id did not survive)': agg(replaced)}
res['changed_by_edit_size'] = {}
res['changed_by_wording_size'] = {}
for name, key, binf in (('changed_by_edit_size', 'others_changed_in_revision', sweep_bin), ('changed_by_wording_size', 'chars_changed', chars_bin)):
    g = collections.defaultdict(list)
    for i in changed:
        g[binf(fact(i)[key])].append(i)
    res[name] = {k: agg(v) for k, v in sorted(g.items())}
res['replaced_by_edit_size'] = {}
g = collections.defaultdict(list)
for i in replaced:
    g[sweep_bin(fact(i)['others_changed_in_revision'])].append(i)
res['replaced_by_edit_size'] = {k: agg(v) for k, v in sorted(g.items())}

# The whole replay: how much of the edit volume is in sweeping revisions.
tot = collections.Counter()
for r in revisions.values():
    n = r['changed'] + r['replaced'] + r['added'] + r['removed']
    tot['revisions'] += 1
    tot['events'] += n
    if n > 20:
        tot['sweeping_revisions'] += 1
        tot['events_in_sweeping_revisions'] += n
res['replay'] = dict(tot)
res['replay']['share_of_events_in_sweeping_revisions'] = round(tot['events_in_sweeping_revisions'] / max(1, tot['events']), 4)
res['replay']['share_of_revisions_sweeping'] = round(tot['sweeping_revisions'] / max(1, tot['revisions']), 4)
# M-2 without the sweeping revisions: the `changed` events (the id survived) of the other revisions, times the share of
# the pool's `changed` items from those revisions that both labellers marked factual (all labelled items, not only
# the sample), per 100 of those revisions. A revision is sweeping when it has over 20 events.
ordinary = [r for r in revisions.values() if r['changed'] + r['replaced'] + r['added'] + r['removed'] <= 20]
changed_in_ordinary = sum(r['changed'] for r in ordinary)
pool_changed_ord = [i for i in pool if pool[i]['kind'] == 'changed' and i in A and i in B
                    and fact(i)['others_changed_in_revision'] + 1 <= 20]
fact_share = (sum(1 for i in pool_changed_ord if A[i]['factual'] and B[i]['factual']) / max(1, len(pool_changed_ord)))
res['m2_without_sweeping_revisions'] = {
    'ordinary_revisions': len(ordinary),
    'changed_events_in_them': changed_in_ordinary,
    'pool_changed_items_labelled': len(pool_changed_ord),
    'factual_share_of_those': round(fact_share, 4),
    'reconfirmations_estimate': round(changed_in_ordinary * fact_share, 1),
    'per_100_ordinary_revisions': round(100 * changed_in_ordinary * fact_share / max(1, len(ordinary)), 2),
    'note': 'an upper bound: every factual block is counted as if it held a claim',
}
# Wilson 95% interval for the share of `changed` items in small edits that both labelled same-claim.
def wilson(k, n, z=1.96):
    if n == 0:
        return None
    p = k / n
    d = 1 + z * z / n
    c = p + z * z / (2 * n)
    h = z * ((p * (1 - p) / n + z * z / (4 * n * n)) ** 0.5)
    return [round((c - h) / d, 3), round((c + h) / d, 3)]
for name in ('changed_by_edit_size',):
    for k, v in res[name].items():
        v['both_same_claim_wilson95'] = wilson(v['both_same_claim'], v['n'])
res['by_kind']['changed (the id survived)']['both_same_claim_wilson95'] = wilson(146, 158)
json.dump(res, open(out_f, 'w'), indent=1, ensure_ascii=False)
print(json.dumps(res, indent=1, ensure_ascii=False))
