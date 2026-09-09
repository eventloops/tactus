---
id: RECOVER-CHERRY-PICK-SAMPLER-COLD-PROBE
severity: P2
disposition: deferred
category: correctness
pr: 7
reviewed_sha:
location: src/engine/topology/recover/tests.rs:9340
provenance: pre_existing
first_bad: PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE
guard: the round that gives this sampler the warm-probe treatment its siblings already have
---

## Failure sequence

`sampled_cherry_pick_child_kills_every_residue_classified_and_recovered`
(`src/engine/topology/recover/tests.rs:9318`) measures **one** `git cherry-pick` duration in a probe
worktree:

```rust
started.elapsed().max(Duration::from_micros(200))    // :9340
```

and then aims all eight kills as fractions of that single number:

```rust
std::thread::sleep(budget.mul_f64(f64::from(run + 1) / f64::from(SAMPLING_N + 1)));   // :9355
```

The schedule is therefore only as good as that one measurement, and there is no seed. When it is
unrepresentative of the runs it schedules, every kill lands after its child has already finished, the harness samples the residue completed commands left,
and the test's own vacuity refusal fires:

> no sample died by the kill: ... the evidence of 8 samples was of completed picks, not of kills:
> `[(After, ExitStatus(unix_wait_status(0))) × 8]`

Observed on `test (macos-latest)` in run
[34304029954](https://github.com/sourcemaps/upstroke/actions/runs/34304029954) — on a pull request
whose entire diff is one Markdown file under `reviews/`, so the change cannot be the cause.

**Why the probe is unrepresentative is not established, and this finding does not claim it.** The
probe is the first invocation in a fresh worktree, so cold-cache inflation is the obvious candidate
and is what `PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE` diagnosed for its sibling. But every sampled
run also builds a fresh fixture and staging worktree, and a parent descheduled until after each child
completes produces the identical signature of eight clean exits. Distinguishing them needs timing
evidence this failure does not carry. What the source does establish is susceptibility: one
measurement, no seed, no recalibration, no retry.

**This is the third sampler in the tree with the same shape, and the only one that never got the
repair.** `PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE` diagnosed exactly this and was fixed in PR7 by
discarding a warm-up probe and taking the median of the next three. PR9's
`sampled_repair_materialization_child_kills_every_residue_classified_and_recovered` was given a warm
pick for the same reason. `git log -S"median" -- src/engine/topology/recover/tests.rs` returns **no
commits**, so the calibration was never added to this file and later lost: it was never applied here
at all. A grep of the current source alone cannot tell those apart, and the first draft of this
finding wrongly rested on one.

## Why this is P2 rather than P3

The severity is not about the code under test, which is fine — the assertion is right and refuses to
pass vacuously when nothing died. It is about what an intermittently red required leg does, and
`PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE` already stated the argument:

> an intermittently red required leg is not a gate — it trains re-running reds, which is how a real
> regression hides

It blocked a documentation-only pull request tonight, and the only way through was to reopen the pull
request to re-trigger CI. That is the training in question, happening.

## What the change that takes this up should do

Apply the PR7 repair here: discard a warm-up probe, take the median of the next three, keep the
fractional schedule, and recalibrate from the durations the runs actually took with one bounded retry
before failing hard.

**`KillableGitChild::exited` is the right measurement only if it is polled promptly, and this sampler
does not poll it at all.** It returns `self.spawned.elapsed()` at the moment `try_wait()` first
observes completion, so calling it once after sleeping the scheduled interval reports the sleep rather
than the child, feeding the over-long schedule straight back into the calibration — which is the error
`PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE` records its own first fix inheriting. This sampler calls it
once at `:9356` and uses only the boolean, so it never obtains an honest duration.

The working precedent is in this tree: `src/engine/topology/attempt/tests.rs:1698-1703` polls in a
one-millisecond loop and keeps the first `Some`, so `spawned.elapsed()` at that point is within a
millisecond of the child's own time. Copy that shape.

Do not weaken the vacuity refusal. It is the assertion doing its job, and removing it would convert a
visible flake into a test that passes while sampling nothing — which is the defect PR9's finding 3 was
raised for, one sampler over.

**Check the other samplers in the same pass.** Three are now known to share this family, and the
repair has been applied one at a time each time it was found.
