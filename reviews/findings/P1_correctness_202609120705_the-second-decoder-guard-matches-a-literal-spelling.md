---
id: PR273-R1-DECODER-GUARD-MATCHES-A-SPELLING
severity: P1
disposition: deferred
category: correctness
pr: 273
reviewed_sha: ba7954a0566cd2ce02c73af1b081b23c6bd36949
location: .github/scripts/test-pr-ready-audit.sh:1602
provenance: introduced_by_feature
first_bad: ba7954a0566cd2ce02c73af1b081b23c6bd36949
guard: the next change to the audit's decoder guard
---

## Failure sequence

Executed by the round-1 fix-check lens at `ba7954a0566cd2ce02c73af1b081b23c6bd36949`.

The new guard that is meant to stop a *second, unguarded* JSON decoder being added matches the
**literal spelling** `json.loads(`. Python permits whitespace before the parenthesis, so a second
decoder written `json.loads (text)` is invisible to it.

In an isolated copy of this head the reviewer added:

```python
def extra_review_reader(text):
    return json.loads (text)
```

Called with `"verdict":"PASS"`, a P1-bearing `findings` array, then a second `"findings":[]`:

```
SECOND_DECODER_EXIT=0
{"verdict": "PASS", "findings": []}
```

and the **unchanged full gate** still reported:

```
test-pr-ready-audit: ok
GATE_SECOND_DECODER_EXIT=0
```

Both scans count only the existing guarded call, so the duplicate-key defect this pull request closes
could be reintroduced through a second decoder and the gate would stay green.

## What the change that takes this up should do

**Enumerate the calls structurally rather than by spelling** — parse the module (`ast`) and inspect
each `json.loads` call's `object_pairs_hook`/`object_hook` argument, rather than grepping text.
**Keep the reviewer's mutation as a positive control**: a second decoder written with a space must
turn the gate red.

**Not merge-blocking, and which limb decided it (owner's rule, 2026-09-11):** *neither* limb holds.
Introducing a second decoder is a change to the repository's own source, so it needs **push access** —
limb (b) fails. And it is not reachable in normal use: it is a latent coverage gap that appears only
when someone later adds a decoder in that spelling, not a defect in the shipped behaviour — limb (a)
fails. **The finding this pull request closes is fixed and executed**: duplicate probes exit `1` with
zero output in both renderings, the repaired audit blocks enqueue where the old parser reached READY
and called merge, and restoring only the pre-fix parser produces all 16 reported failures at exit
`1`.

It is filed as a P1 rather than downgraded because the reviewer classified it P1 and because an
evadable regression guard means this defect can return silently — but it does not hold the merge.
