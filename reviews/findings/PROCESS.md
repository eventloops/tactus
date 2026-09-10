# Working the finding ledger

`README.md` in this directory says what a finding **is** and how it is filed. This says how a
finding gets **fixed**: what may be batched with what, who reviews it, what blocks a merge, and
what runs in parallel with what.

> **This is the findings sweep, not the standards sweep.** `standards/SWEEP.md` governs the
> file-by-file §6/§7 cleanup of the existing tree and states its own activation rule. The two are
> unrelated and neither defers to the other. Where a document, a branch or a script on the build
> box needs to name this one, it is spelled **findings sweep** in full.

It exists because the ledger stopped being a list and became a backlog. Measured on `master` at
2026-09-10: **307 open findings** — 14 P1, 106 P2, 186 P3, and one `P4` that is outside the
category vocabulary the gates enforce. Fixing those one pull request at a time is not a plan, and
sending an agent at each of them at once produces a merge-conflict storm and a review bill with
nothing to show for it. The rules below are what sits between those two failures.

---

## 1. The roster

Four roles, fixed models. A session that is not one of these four is not part of the sweep.

| Role | Model | Effort | CLI | Account |
|---|---|---|---|---|
| **Orchestrator** | `gpt-6-astra` | `xhigh` | `codex` | OpenAI |
| **Implementer** | `claude-opus-5` | `xhigh` — `max` for any P1 | `claude` | `cameron` |
| **Reviewer** | `gpt-6-astra` | `max` | `codex` | OpenAI |
| **Repair** | `claude-opus-5` | `max` | `claude` | `cameron` |

One orchestrator. Implementers, reviewers and repair sessions are spawned per batch and are
**fresh each time** — a session that has already argued for its own patch is not the session to
judge whether the patch worked.

**Implementer and reviewer are never the same family.** Every batch is written by Claude and
judged by Codex. This is not a preference; it is the only structural defence the process has
against a model's own blind spots, and it has already paid: two of the three P1s that PR #258
needed were *introduced by a previous repair* and found by the other family.

`max` for P1s and for repairs, `xhigh` for ordinary implementation. The distinction is worth its
cost only where a wrong answer is expensive, and those are the two places it is.

> **Standing note on accounts.** Every Claude seat in this roster resolves to `cameron`. The
> `camwork` account is reserved for Fable sessions by an owner ruling made when Fable held the
> implementer seat; Fable is no longer in the roster, so that reservation currently protects
> nothing while both implementer and repair contend for one account's capacity. Whether to open
> `camwork` to Opus implementers is the owner's call, not the orchestrator's.

---

## 2. The unit of work

```
finding  ──►  fix branch  ──►  batch branch  ──►  pull request  ──►  merge queue
 (one)         (one each)        (one per PR)         (one)
```

**One fix branch per finding, always — including inside a batch.** The branch is created and
pushed *before any work starts*, and that push is what claims the finding.

```
fix-P<n>/<category>_<desc>              one per finding; not expected to open a pull request
bulk-fix-P<n>/<slug>                    the batch; this is what becomes the pull request
```

Both prefixes are already in the branch vocabulary that `.github/scripts/validate-pr-branch.sh`
enforces, and nothing here needs adding to it.

**`<desc>` is the description part of the finding's FILENAME, not its `id`.** The validator
resolves a `fix-P*/` branch back to exactly one finding file, with the timestamp free:

```
reviews/findings/P1_correctness_202609040301_pid-identity-under-a-host-wildcard-waiter.md
fix-P1/correctness_pid-identity-under-a-host-wildcard-waiter
```

A branch named after the `id` resolves to no finding and the gate rejects it. `<slug>` on the
batch branch is free-form lower-case words joined by single hyphens, because a batch holds many
findings and can name none of them — which is exactly why the two prefixes are separate rather
than one shape the validator would have to guess at.

Three things follow from doing it this way, and all three are the reason for it:

- **The branch list is the work-in-progress board.** `git ls-remote --heads origin 'fix-*'` says
  exactly which findings are being worked right now — before any file has been edited, before any
  commit exists, before anything is visible on `master`. Nothing else on the box answers that
  question.
- **Branch creation is the mutex, and the server arbitrates it.** `git push origin
  <base-sha>:refs/heads/fix-P3/correctness_PR136-FOO` fails if the ref already exists. Two agents
  reaching for the same finding is settled atomically by GitHub, with no lock file to leak and no
  claim registry to fall out of step. The loser takes the next finding.
- **Attribution and revert stay per-finding** even though review and merge are per-batch. A batch
  that has to be partly rolled back is `git revert` of one fix branch's merge, not a re-do.

**When the matrix says one finding per pull request, the fix branch *is* the pull request branch.**
No batch branch is created. P1 lanes therefore behave exactly as they do today.

### Assembling a batch

The orchestrator cuts `bulk-fix-P<n>/…` from `master`'s head, then **cherry-picks** each member's
fix branch into it. **Every pick must apply cleanly.** Batch members are module-disjoint by
construction (§4), so a conflict here is not a scheduling accident — it means a finding's
`location` did not describe where its fix actually landed.

> A conflict at batch assembly aborts the batch, returns its members to the queue, and files the
> `location` discrepancy against the offending finding. It is never resolved by hand.

That is a deliberate design choice: assembly is the cheapest possible place to discover bad
scheduling data, because it happens before a single review token is spent.

---

## 3. The matrix

Findings per pull request, batches of that lane in flight at once, and the review lenses that lane
gets. **Every lane gets a regression lens**; the third column is what that lane needs on top.

| Category | Sev | Findings / PR | In parallel | Lenses |
|---|---|---|---|---|
| `docs-contract` | P3 | 20 | 3 | fix-check + regression |
| `docs-contract` | P2 | 10 | 2 | fix-check + regression |
| `correctness` | P3 | 20 | 2 | fix-check + regression |
| `correctness` | P2 | 5 | 2 | fix-check + regression |
| `correctness` | P1 | 1 | 1 | fix-check + regression |
| `performance` | P3 | 10 | 1 | fix-check + regression |
| `compatibility` | P2 / P3 | 5 | 1 | fix-check + regression |
| `portability` | P3 | 10 | 1 | + **executed on the platform** |
| `portability` | P2 | 5 | 1 | + **executed on the platform** |
| `liveness` | P2 | 3 | 1 | fix-check + regression |
| `liveness` | P1 | 1 | 1 | fix-check + regression |
| `crash-consistency` | P3 | 5 | 1 | + **replay** |
| `crash-consistency` | P2 | 3 | 1 | + **replay** |
| `crash-consistency` | P1 | 1 | 1 | + **replay** |
| `security-trust` | P2 | **1** | 2 | + **adversarial** |
| `security-trust` | P1 | **1** | 1 | + **adversarial** |

**`security-trust` is never batched, at any severity.** Two security findings in one diff means
one review pass covering both, and a pass that is looking at two things is looking properly at
neither. They run alone, and two may run alongside each other in separate pull requests.

**`docs-contract` batches hardest** because a docs fix has no runtime behaviour to regress — but see
the authority exception in §5, which is not a formality: the finding that blocked Gate 4 on
2026-09-09 was filed `docs-contract`.

### Global caps

These bind across every lane at once and override the per-lane column above.

| Cap | Value | Why |
|---|---|---|
| Pull requests in flight | **6** | `/tmp/w1-eight.lock` serialises every gate run, and `winguest` is a single KVM guest on the same box. Unserialised load measured 90+ on 32 cores and reddened required legs on its own. |
| P1s in flight | **1** | A P1 repair rewrites the ground other fixes are standing on. |
| Three-lens lanes at once | **2** | Review spend, and reviewer-family capacity. |

---

## 4. The module lane — the binding constraint

The matrix says *how many* and *which lenses*. It cannot say *what may run together*. That is
decided by file overlap, and it is checked mechanically from `location:` **before any agent is
spawned**.

> **Two pull requests may be in flight together only if their module sets are disjoint.**

**The unit is the module, not the file.** `{src/X.rs, src/X/**}` is one lane, because a fix in
`src/workspace_manager.rs` will nearly always edit `src/workspace_manager/tests.rs` — treating them
as independent lanes makes every batch collide on the test file.

Measured on `master`, 2026-09-10 — 110 distinct files, and the contention is not evenly spread:

| Module | Findings | P1s |
|---|---|---|
| `workspace_manager` (`.rs` 35, `tests.rs` 18, `worktree.rs` 5) | **58** | 3 |
| `agent/proc` | 17 | **4** |
| `rundir` (`.rs` 9, `classify.rs` 4) | 13 | 1 |
| `topology/fold` (`.rs` 7, `check_end.rs` 4) | 11 | 0 |
| `engine/topology` (`run.rs` 6, `recover.rs` 4, …) | 15 | 2 |
| *(the tail — ~35 modules, 1–4 findings each)* | ~154 | 1 |

**`workspace_manager` is 19% of the backlog in a single serial lane, and it is the critical path.**
No amount of added parallelism shortens it; it is roughly nine sequential pull requests on its own.
The tail is where nearly all the real parallelism lives.

Two consequences the orchestrator must act on:

- **Keep the longest lane hot.** `workspace_manager` starts first and never idles. Scheduling it
  as filler makes it the tail that decides when the sweep ends.
- **The P1s cluster in the two longest lanes.** `agent/proc.rs` holds 4 of the 14 P1s and
  `workspace_manager.rs` holds 3. With one P1 in flight globally, the two longest chains are forced
  to take turns — which is the single biggest scheduling cost in the whole backlog, and it is why
  P1s run as an opening phase (§6) rather than interleaved.

---

## 5. Four rules that override the matrix

**1. Adjacency beats severity.** No two findings share a batch if their fixes touch the same
function, even when the matrix permits the count and the modules are disjoint. Module-disjointness
is checked by script; adjacency needs the code read, so the implementer raises it and the batch is
split.

**2. The authority exception.** A `docs-contract` finding cited as authority by a gate report or a
design section **leaves the docs lane** and takes the severity of the thing that depends on it.
Documentation that a gate's evidence rests on is not documentation for batching purposes.
Gate 4's blocker on 2026-09-09 was filed `docs-contract` and was a hard blocker.

**3. Every fix carries a mutation witness, and the witness must kill.** A guard that survives its
own mutation is not a guard. If the witness kills nothing on its first run, that is the signal that
the change is unguarded — not an excuse to weaken the mutation. Recorded because it has already
happened: witness M7 killed nothing, and that was the finding.

**4. Pick lenses by what the fix *touches*, not by the category it was filed under.** Both P1s in
PR #258 were filed `correctness` and were really portability — filesystem case-sensitivity and
symlink resolution — and both were caught by a lens chosen for the code rather than the label.

---

## 6. Priority

**Phase 0 — triage. Nothing else starts until this clears.** Measured 2026-09-10, ~43 findings
cannot be scheduled as they stand:

- **39 findings have an empty `location:`.** They are unschedulable — with no module, they cannot
  be allocated a lane or checked for disjointness. Three of them are P1s.
- **One finding is `disposition: fixed`** and should have been deleted; `README.md` is explicit
  that this directory holds outstanding work and nothing else.
- **Two findings carry the README's placeholder text** verbatim in `disposition`.
- **One finding is `P4`**, outside the vocabulary `.github/scripts/test-pr-policy.sh` enforces.
  It is either a P3 or it is not a finding.
- **Two P1s share `src/agent/proc.rs:2320`** and are probably one finding filed twice. Fixing a
  duplicate twice wastes a P1 slot, which is the scarcest thing in the sweep.

**Phase 1 — the 14 P1s, one at a time.** They are the irreducible serial core, and they run before
the bulk rather than interleaved with it. A P2 batch that lands in `workspace_manager.rs` before
that module's three P1s are repaired is a batch built on ground that is about to move.

**Phase 2 — the bulk, longest lane first.** Slot priority within it:

1. Any P1 that triage promotes out of Phase 0
2. `crash-consistency`, `security-trust`, `liveness`
3. `correctness`
4. `docs-contract`, `performance` — as filler, which is what they are good at

Longest-lane-first is not a preference. The longest chain sets the finish time, so the only
scheduling decision that changes when the sweep ends is whether the longest lane is ever idle.

---

## 7. Review

Reviews run on the **batch branch head**, after the gates are green on it, never on a fix branch.

**Two lenses always, a third by lane.**

- **Fix-check** — for each finding in the batch, by `id`: is it actually closed? It is given the
  finding files, not a summary of them.
- **Regression** — does the combined diff break anything that worked? This lens exists because the
  combined diff is a thing no single implementer saw.
- **Third lens** — per the matrix: executed-on-platform, replay, or adversarial.

Mechanics that are not optional, each of which has cost time:

- **Every lens writes to its own output path.** Three lenses that default to one filename overwrite
  each other and the run is only recoverable from per-lens logs.
- **Never push while a review is in flight.** `review-pr.sh` resolves the head through the API,
  which lags a push, and will happily return a valid-looking verdict on the previous tree. It has a
  `git ls-remote` cross-check and refuses on disagreement; do not defeat it by pushing anyway.
- **Read the clause table, not the verdict line.** A `CHANGES_REQUIRED` whose findings are all P2/P3
  documentation residue is a merge candidate under §8. Gate 4 passed on exactly such a verdict:
  all six pass-rule clauses and all thirteen adversarial tests Established, blocking on two prose
  descriptions. Reading the verdict line alone said the gate had failed. It had not.

**Triaging a review:** P1 → repair round on the same batch branch, by a fresh `claude-opus-5` `max`
session, then re-review. P2/P3 → filed as new findings and the batch proceeds. A finding carrying a
failing test, a reproduction or a mutation witness is fixed whatever its label.

**Repair rounds loop until the lane passes.** They are not capped at one pass. The standing
one-pass stopping rule is suspended for sweep lanes by owner ruling; it still governs ordinary
feature work.

---

## 8. Merging

**The merge queue is not locked for fixes.** It already serialises landing, and better than a lock
would: it builds each entry on `master`'s head plus the entries ahead of it, runs both required
contexts on *that* commit, and lands exactly the commit they passed. Holding it closed until
approvals accumulate idles the one component whose whole purpose is to absorb parallel landings.

Approval gates **entry** to the queue, not the queue itself. A batch enters when:

- both required contexts (`upstroke-ci`, `upstroke-pr-policy`) are green on the exact head;
- every lens for its lane has reported, and no P1 is outstanding;
- the body carries the six sections and the canonical nine-column ledger header;
- the ledger row for each member finding is present, and each member's file is deleted in the same
  pull request.

**The queue *is* locked while a gate run is in flight.** A gate report is bound to a frozen range;
if `master` moves under it the range is stale, and the packet forbids amending a failed gate's
report — so a moved range costs a whole re-run. This is not hypothetical: it is why Gate 4 ran three
times.

**A branch behind `master` is not hand-updated to merge.** The queue rebuilds it. Update only when
the change genuinely needs something `master` gained.

Merging is the owner's act unless delegated in writing for that pull request, and the delegation is
disclosed in the body. Never push to `master` directly. Delete the batch branch and every member
fix branch after the merge — a fix branch left behind still reads as a live claim on its module.

---

## 9. What this does not decide

- **Whether `src/workspace_manager.rs` should be split** before the sweep reaches it. Splitting it
  converts the longest serial lane into several parallel ones, which is the single highest-leverage
  change available to the schedule — and it invalidates the `location:` line of all 35 findings that
  point into it. That trade is the owner's.
- **Whether `camwork` opens to Opus implementers** (§1).
- **The branch-name gate has never been proven to reject on a live pull request.**
  `validate-pr-branch.sh` is on `upstroke-pr-policy` in PR #251, still draft. Its vocabulary
  already covers both prefixes this process uses — the names the scheduler generates were run
  against it and pass, and an `id`-based name was run against it and is rejected — but a gate that
  has only ever been exercised by its own fixtures is not yet a gate. Landing #251 and watching it
  refuse one real branch is owed before the sweep leans on it.
