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

The probe is the first invocation in a fresh worktree, so it pays for a cold filesystem cache. Its
number is inflated relative to the runs it schedules. When the inflation is large enough, every kill
lands after its child has already finished, the harness samples the residue completed commands left,
and the test's own vacuity refusal fires:

> no sample died by the kill: ... the evidence of 8 samples was of completed picks, not of kills:
> `[(After, ExitStatus(unix_wait_status(0))) × 8]`

Observed on `test (macos-latest)` in run
[34304029954](https://github.com/sourcemaps/upstroke/actions/runs/34304029954) — on a pull request
whose entire diff is one Markdown file under `reviews/`, so the change cannot be the cause. There is no
seed; the whole variance is one measured duration.

**This is the third sampler in the tree with the same shape, and the only one that never got the
repair.** `PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE` diagnosed exactly this and was fixed in PR7 by
discarding a warm-up probe and taking the median of the next three. PR9's
`sampled_repair_materialization_child_kills_every_residue_classified_and_recovered` was given a warm
pick for the same reason. Grepping this function for `median`, `warm`, `three` or `discard` finds
nothing: the repair was applied per-sampler and this one was missed.

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
before failing hard. `KillableGitChild::exited` exists for that measurement and is already used at
`:9356`.

Do not weaken the vacuity refusal. It is the assertion doing its job, and removing it would convert a
visible flake into a test that passes while sampling nothing — which is the defect PR9's finding 3 was
raised for, one sampler over.

**Check the other samplers in the same pass.** Three are now known to share this family, and the
repair has been applied one at a time each time it was found.
