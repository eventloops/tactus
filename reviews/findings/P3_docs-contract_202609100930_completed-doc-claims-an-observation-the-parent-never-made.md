---
id: PR259-COMPLETED-DOC-CLAIMS-AN-UNMADE-OBSERVATION
severity: P3
disposition: deferred
category: docs-contract
pr: 259
reviewed_sha: 31de5e93f97cd19173cd06821ca189c142e61be1
location: src/workspace_manager/fixture.rs:813
provenance: fix_regression
first_bad: PR259-R4-COMPLETED-DOC-OVERSTATED-THE-AIM — the round-5 and round-6 rewrites of that P3 replaced one unobservable claim with another; the wording at the location above is the third form the sentence has taken
guard: none — the sentence is documentation of a test fixture, which no gate reads; the behaviour it describes is guarded by the samplers' own floors (`killed_while_running >= 1` and `killed_while_writing >= 1` in recover, the eight-kill, non-`After` and mid-write floors in dispatch), all of which are unchanged and none of which depends on this sentence being true
---

## Failure sequence

`KillBudget::completed`'s documentation, reached from `run_until`'s at
`src/workspace_manager/fixture.rs:813`, says the second completion path means

    a poll reached the aim with the child still running, and the kill sent

The parent cannot know that. The sequence, reasoned from the code and not reproduced:

1. just before aim `A`, `try_wait()` returns `None`;
2. the parent is descheduled before it reads `now`;
3. the child exits before `A`;
4. the parent resumes after `A`, observes `now >= deadline`, and attempts to kill a child that
   has already exited;
5. `wait()` returns the completion status, and its `reaped` clock is fed to `completed`.

No poll reached `A` while observing the child alive, yet the documentation says one did. The same
overclaim appears in `run_until`'s own documentation and in the pull request summary.

## Why this is deferred rather than fixed

**It changes no behaviour and cannot make either sampler pass more cheaply.** The successful `wait`
status still classifies the child as a completion, `reaped` is still measured after exit and is still
a genuine upper bound on the child's run, and the value still feeds `KillBudget` exactly as it would
have on the path the sentence describes. The floors that decide whether a sampler's evidence is
adequate are untouched.

It is **reasoned-only**: the `gpt-6-astra` `ultra` review of `31de5e93` that raised it recorded no
executed reproduction and no mutation witness for it, and graded it P3 on that basis, stating it bears
on neither a Gate 4 pass-rule clause nor row 10's sampled evidence.

Under the owner's merge rule of 2026-09-09 — only P2s and P3s, none conflicting with the Gate 4 goal,
documented as findings and then merged — this is documented here and merged.

## What closing it would look like

Say what the parent actually observed: that the deadline passed without a completion being seen, and
that the kill was attempted against a child whose state at that instant was unknown. Do not replace it
with a fourth claim about what a poll saw. The three surrounding statements — `run_until`'s
documentation, `completed`'s, and the pull request summary — must agree, since this defect has twice
been reintroduced by correcting one of them alone.
