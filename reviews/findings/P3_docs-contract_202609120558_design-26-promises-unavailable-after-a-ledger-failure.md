---
id: PR272-R2-DESIGN-26-OVERPROMISES-UNAVAILABLE
severity: P3
disposition: deferred
category: docs-contract
pr: 272
reviewed_sha: abde139600352168995d0b8eec120e87a8e8a883
location: design/26_design_merge_queue_protocol.md:618
provenance: introduced_by_feature
first_bad: abde139600352168995d0b8eec120e87a8e8a883
guard: the next change to the merge-queue protocol design section
---

## Failure sequence

Raised by **both** round-2 review lenses at `abde139600352168995d0b8eec120e87a8e8a883`, one by inspection and one by execution.

`design/26_design_merge_queue_protocol.md:618` — and the paragraph this pull request added to
`docs/internals/engine/topology/integrate.md:132` — say that a later **ledger** failure settles the
verification **unavailable**. The runtime does not do that. At `src/engine/topology/run.rs:175` only
`UpstrokeError::Git` is converted to `Verified::Unavailable`; the catch-all propagates every other
`JudgeError::Other`, so a ledger refusal ends the command instead of settling a terminal.

Executed: inject an already-registered review identity, let an integration review return costing
$2.50, and registration fails so `TopologyRun::step` propagates `Refused`. A probe asserting the
*documented* behaviour failed, exit **`101`**:

```
unavailable_terminals=0, transaction_open=true, live=3.7, replay=1.2
```

Recovery then writes `merge_verification_interrupted`, which carries no spend.

**The runtime behaviour predates this pull request; the false promise does not.** The pull request
body narrows the claim correctly — it is the design section and the new internals paragraph that
still contradict the code.

## What the change that takes this up should do

Narrow both added paragraphs to the failure actually converted and tested: a snapshot failure
returned as `UpstrokeError::Git`. Say that other `Other` errors propagate and end the command, and
that recovery's `merge_verification_interrupted` carries no spend.

**Not merge-blocking, recorded here for why:** both lenses classified it P3, and `ORCH-P1.md` is
explicit that P3s do not hold a merge and that there is no clean-up round. No P1 and no P2 was
returned in this round; all seven targeted witnesses pass at this head and each of the five repair
withdrawals fails at `101`. It is a documentation defect reaching no runtime path — but it is in the
sole living design authority, so it should be taken up rather than left.
