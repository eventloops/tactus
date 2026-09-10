# Working the finding ledger

`README.md` in this directory says what a finding **is** and how it is filed. This says how a
finding gets **fixed**: what may be batched with what, who reviews it, what blocks a merge, and
what runs in parallel with what.

> **This is the findings sweep, not the standards sweep.** `standards/SWEEP.md` governs the
> file-by-file §6/§7 cleanup of the existing tree and states its own activation rule. The two are
> unrelated and neither defers to the other. Where a document, a branch or a script on the build
> box needs to name this one, it is spelled **findings sweep** in full.

It exists because the ledger stopped being a list and became a backlog. Measured on `master` at
`44edb2a1`, 2026-09-10: **307 open findings** — 14 P1, 106 P2, 186 P3, and one `P4` that is outside
the category vocabulary the gates enforce. Fixing those one pull request at a time is not a plan,
and sending an agent at each of them at once produces a merge-conflict storm and a review bill with
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

**One fix branch per finding, always — including inside a batch.** The branch is created by
pushing a claim commit *before any work starts*, and that push is what claims the finding — but
only if the commit is the claimer's own, which is the whole of the mutex and is set out below.

```
fix-P<n>/<category>_<desc>              one per finding; not expected to open a pull request
bulk-fix-P<n>/<slug>                    the batch; this is what becomes the pull request
```

Both prefixes are already in the branch **grammar** that `.github/scripts/validate-pr-branch.sh`
enforces (PR #251), so nothing needs adding to that vocabulary. That is a narrower claim than it
looks: `scripts/pr-ready-audit.sh` decides a pull request's *lane* from the branch prefix alone, and
neither of these prefixes is a prefix it knows — both fall through to `feature`. §8 says what
follows from that and §9 records it as owed work.

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
- **Branch creation is the mutex, and the server arbitrates it — but only if each claimer pushes
  a commit of its own.** Pushing the shared base SHA claims nothing. The second worker's push is a
  no-op, the server answers `Everything up-to-date`, and it **exits 0**, so both workers believe
  they won; `--force-with-lease` with an empty expectation does not rescue it, because the no-op
  short-circuits before the lease is checked. Each claimer therefore pushes its own empty commit:

  ```bash
  BASE=$(git rev-parse origin/master)
  CLAIM=$(git commit-tree "$BASE^{tree}" -p "$BASE" -m "claim: <session> <utc>")
  git push origin "$CLAIM:refs/heads/fix-P3/correctness_the-filename-description"
  ```

  Measured on the build box in disposable repositories, exit codes captured directly rather than
  through a pipe:

  ```
  A, first claimer                     exit 0    * [new branch]
  B, same finding, its own commit      exit 1    ! [rejected] (non-fast-forward)
  B retrying with --force-with-lease   exit 1    ! [rejected] (stale info)
  B re-basing onto a later master      exit 1    ! [rejected] (non-fast-forward)
  A re-pushing its own claim           exit 0    Everything up-to-date
  ```

  **Distinct commits are the whole of the mechanism.** `commit-tree` is a pure function of tree,
  parent, message and identity, so two claimers whose messages match to the second produce the
  *same* SHA and the bug is back — measured, the second push then answers `Everything up-to-date`
  and exits 0. The message must therefore name the claiming session and the UTC time, and that
  message is also the record of who holds the claim: `git log -1 <ref>` answers it, and nothing
  else on the box does. A holder re-pushing its own claim is always safe; the loser takes the next
  finding.
- **Attribution and revert stay per-finding** even though review and merge are per-batch — but the
  unit of revert is a commit, not a merge. Assembly cherry-picks, and a cherry-pick creates no merge
  commit, so there is no per-finding merge to `git revert -m 1`; reverting the *batch* merge removes
  every member's fix, measured. What cherry-picking does preserve is one commit per commit of each
  fix branch, landed unchanged, so a single finding is rolled back by reverting its own commits and
  the others survive — provided those commits are selected by the finding's `id`, read from the
  trailer block by git's own parser, over the batch's own history and no wider. Every one of those
  three qualifications replaces something a review measured going wrong. §8 gives the command and
  what each part of it is for.

**When the matrix says one finding per pull request, the fix branch *is* the pull request branch.**
No batch branch is created. P1 lanes therefore behave exactly as they do today.

### Assembling a batch

The orchestrator cuts `bulk-fix-P<n>/…` from `master`'s head, then **cherry-picks** each member's
fix branch into it, one member at a time so that a member's commits land contiguously. The range
starts **after** the claim commit — `git cherry-pick "$CLAIM..$TIP"`. Picking the claim commit
itself stops the sequence: it is empty against the batch branch, and `git cherry-pick` halts with
`The previous cherry-pick is now empty` and exits 1, measured.

**A pick that does not apply cleanly is resolved, not blamed.** Members may share a module — that is
what a batch is for (§4) — and two members with correct write sets and no shared function can still
conflict, because a conflict is a fact about *text*, not about scheduling. Measured: one member
changed `alpha()` and added `use std::time::Duration;`; the other changed `beta()` and added
`use std::path::Path;` at the same insertion point. First pick exit **0**, second pick exit **1**,
conflicting on the import block alone.

> A conflict at batch assembly is resolved in the pick that hit it, by the implementer, and the
> member keeps its own commit and its own `Finding:` trailer. Measured on that import conflict:
> resolve the hunk, `git cherry-pick --continue` exit **0**, the member's trailer intact, and the
> per-finding rollback in §8 still selects exactly that one commit and leaves the other member's fix
> and import in place.

An earlier revision of this section aborted the batch and filed the discrepancy against an
"offending finding". On the case above there is no offending finding: both write sets named exactly
the file they wrote and the two fixes touched different functions. Aborting and rebatching under the
same rule reproduces the same conflict. **Abort only when the conflict tells you something** — a
member that wrote somewhere its finding never reserved (§4) is worth filing; a member that turns out
to share a function with another (§5, rule 1) is worth splitting out. A shared import block is
neither, and the fact that the conflict happened proves nothing about which of those it was, so the
diff has to be read before anything is filed.

**A clean assembly is not evidence that the scheduling data was right, and must not be read as
one.** An earlier revision of this section called assembly the cheapest place to discover bad
scheduling data. It does not discover it at all, for two reasons, both measured:

- **A wrong write set applies cleanly.** Two members whose fixes land in one file at different
  hunks were cherry-picked with one member's declared paths naming a module it never touched: both
  picks exit **0**, and the assembled tree carries both hunks. Nothing in a cherry-pick reads a
  finding's declared paths, so a member that wrote outside its reservation is invisible here.
- **Assembly cannot see the other pull request at all.** Each batch is cut from `master`'s head and
  picked in isolation, so the collision §4 exists to prevent — two *concurrent* pull requests
  writing one path — is outside both assemblies' field of view by construction. In §4's
  reproduction each batch assembled at exit **0** and the conflict appeared only at the second
  merge.

What assembly does catch is textual conflict between members of one batch, and that is worth having
because it is early and cheap. It is not a safety net for §4, and §4 has none: nothing anywhere in
this process compares what a fix wrote against what its finding reserved.

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

## 4. The write set — the binding constraint

The matrix says *how many* and *which lenses*. It cannot say *what may run together*. That is
decided by file overlap. It is checked mechanically **before any agent is spawned**, and the check
is not over `location:`.

> **Two pull requests may be in flight together only if their planned write sets are disjoint.**

**`location:` records where the defect is, not where the fix will write.** Those are different sets,
and the second is the one exclusion needs. Scheduling from the first has a hole that is already in
the ledger:

- `PR146-ASTRA-001` is a defect *inside another finding's file* — its `location:` is
  `reviews/findings/P3_correctness_202609042224_sampling-n-is-the-registrys-own-number.md:41`, and
  its fix repairs prose there.
- `SWEEP-BIJECTION-005` **is** that other finding, with
  `location: src/topology/effects/bijection.rs:450`. §8 requires the pull request that resolves it
  to delete that file.

The two `location:` values fall in different lanes — one a path under `reviews/findings/`, the other
`topology/effects` — so a disjointness check computed over `location:` permits both pull requests at
once. It should not. Measured in a disposable repository carrying those two findings' real text,
exit codes captured directly:

```
assemble the correctness batch                        exit 0
assemble the documentation batch                      exit 0
merge the correctness batch — the file is deleted     exit 0
merge the documentation batch                         exit 1   CONFLICT (modify/delete)
```

**Every fix writes at least one path that no `location:` declares: its own finding file, which
resolution deletes (§8).** A fix's write set is therefore always strictly larger than its
`location:`, and a schedule read off `location:` is always reasoning about the wrong set. This pair
is simply the case where the difference collides.

**What the orchestrator reserves is the planned write set**, declared per finding before any agent
is spawned, and made of three parts:

- every path the fix is expected to edit or create, each mapped to its module lane by the rule
  below;
- **its own finding file, always**, because §8 deletes it on resolution;
- **any other finding file it will write** — which is what `PR146-ASTRA-001` is. A `location:` under
  `reviews/findings/` means the fix writes another finding's file, and that file is deleted by
  whichever pull request resolves *its* finding.

Two pull requests may be in flight together only when those sets are disjoint. A historical defect
location does not establish that exclusion and never did.

**The declared set must be complete, which means it includes the paths a fix *forces* and not only
the ones its finding names.** The standing example is `src/export.rs`, whose tests `include_str!`
`README.md`, `MAINTAINING.md`, `design/15_design_event_log_resume_run_layout.md` and
`design/25_design_export_decisions_schema.md` and assert that particular sentences appear in them: a
`docs-contract` fix that rewords one of those sentences has `src/export.rs` in its write set whether
or not the finding mentions it, and is therefore not concurrent with anything else in the `export`
lane. A declared set assembled by reading the `location:` line and nothing else will miss couplings
of that shape, and missing one is how a pair of individually green pull requests produces a red
`master`. That coupling is the lucky case: it breaks an assertion, so CI names it. The unlucky case
is the one below, where nothing does.

**The declared set is a bound, and nothing enforces it.** An implementer that discovers it needs a
path outside its reservation stops and returns the finding to the orchestrator to be rescheduled.
That is a duty, not a mechanism, and this section is exact about how little sits behind it because
three earlier revisions of it each claimed a detector that does not exist.

> **Nothing compares what a fix wrote against what its finding reserved.** Not git, not
> `upstroke-ci`, not `upstroke-pr-policy`, not the merge queue, not `scripts/pr-ready-audit.sh`. A
> pull request that writes outside its reservation lands, and nothing reports that it did.

Measured with a reservation deliberately broken. A reserved `shared.txt`, B reserved
`unrelated.txt`, each also reserving its own finding file, so the declared sets were disjoint and
the rule above permitted both at once. B then also wrote a distant hunk of `shared.txt`. Exit codes
captured directly:

```
assemble batch A                          exit 0
assemble batch B                          exit 0
land A                                    exit 0
rebuild B on A by merge                   exit 0    combined assertions pass
rebuild B on A by cherry-pick             exit 0    combined assertions pass
rebuild B on A by rebase                  exit 0    combined assertions pass
```

The out-of-reservation line is on `master` and no step objected.

**What git catches is textual overlap, which is a different thing and a much smaller one.** The same
violation, varied only in where B wrote:

```
a distant hunk of a file A edited         merge exit 0    undetected
the same line A edited                    merge exit 1
a file A deleted (modify/delete)          merge exit 1
```

Those last two are why an earlier revision believed the queue enforced the reservation: its one
reproduction was the modify/delete case, and it generalised. Git compares text; a reservation is not
text, and the two agree only when a violation happens to land in the same region. CI is no better
placed — it catches a violation only when the violation also breaks an assertion, which the
`src/export.rs` coupling above would and a distant hunk in an unrelated function would not.

**So this is what §4 buys, stated at the size it actually is.** Computing exclusion from planned
writes rather than from `location:` stops the scheduler from *allocating* two concurrent pull
requests onto one path, and a correct allocation cannot produce the modify/delete collision that
motivated the rule. That is scheduling hygiene, and hygiene is not a safety property. **A write set
that is wrong or incomplete is a silent failure with no detector**, and the only two defences are
getting it right before the agent is spawned and reading the diff afterwards — a review duty, not a
gate. §9 records a checker as owed and says what it would and would not buy.

**That rule binds across concurrent pull requests, and only there.** Inside one batch the bar is
adjacency, not the module: two findings may share a module, and may share a file, provided their
fixes do not touch the same function or the same shared declaration (§5, rule 1). A batch is
implemented one member at a time on its own fix branch, so same-module members are never written
concurrently and cannot race. Reading disjointness into the batch as well is what would turn a
69-finding module into 69 pull requests.

**The unit is the module, not the file.** `{src/X.rs, src/X/**}` is one lane, because a fix in
`src/workspace_manager.rs` will nearly always edit `src/workspace_manager/tests.rs` — treating them
as independent lanes makes every batch collide on the test file. Two details decide every count
below, and the first is what an earlier draft of this table got wrong:

- `src/X/**` is **recursive**, and the **outermost** such pair on a path wins.
  `src/workspace_manager/worktree.rs` is in the `workspace_manager` lane, not a lane of its own,
  and neither are `residue.rs`, `parsers.rs`, `object.rs`, `fixture.rs`, `hooks.rs` or
  `containment.rs` beside it.
- A directory with no `X.rs` beside it is a namespace, not a lane. `src/agent/`, `src/engine/`,
  `src/topology/` and `src/runner/` carry a `mod.rs` rather than a sibling parent file, so the
  lanes under them are their children — `agent/proc`, `engine/topology`, `topology/fold` — and
  there is no `agent` lane.

Measured on `master` at `44edb2a1`, 2026-09-10 — 307 finding files naming 111 distinct paths, and
the contention is not evenly spread:

| Module | Findings | P1s |
|---|---|---|
| `workspace_manager` | **69** | 3 |
| `topology/fold` | 25 | 0 |
| `engine/topology` | 21 | 2 |
| `agent/proc` | 19 | **4** |
| `rundir` | 15 | 1 |
| *(the `src/` tail — 20 modules, 1–11 findings each)* | 62 | 1 |
| *(`location:` outside `src/` — 40 paths, 29 of them under `docs/`, `design/`, `.github/`, `reviews/`)* | 57 | 0 |
| *(unschedulable — no `location:` at all, §6)* | 39 | 3 |
| **Total** | **307** | **14** |

**How that was counted**, so that it can be redone rather than believed: the `location:` line of
each of the 307 files at `44edb2a1`, file part only — everything before the first `:` — mapped to a
module by the rule above and counted strictly as written. The other eleven of the 40 non-`src/`
paths are `Cargo.toml`, `DESIGN.md`, `pr8-plan.md`, `scripts/pr-ready-audit.sh` and the seven
unresolved names below.

**Nine findings name a path that does not resolve from the repository root**, tested against
`git ls-tree -r --name-only 44edb2a1`. They are counted outside `src/`, where they are written,
rather than where a reader would guess they were meant. They do not all want the same repair, which
is why §6 makes resolving them a Phase 0 gate and not a bulk rename:

| Unresolved `location:` | What it actually needs |
|---|---|
| `rundir.rs`, `engine/topology.rs`, `topology/events.rs` | a `src/` prefix — the prefixed path exists |
| `coordinator.rs` | *not* a `src/` prefix: the only file of that name is `src/engine/coordinator.rs` |
| `attempt.rs` | ambiguous — `src/engine/attempt.rs` and `src/engine/topology/attempt.rs` both match, and it is a guess nobody should make on a scheduler's behalf |
| `proposals/README.md` | a tree retired on 2026-09-03 (`87dcc6ce`); the finding must be re-pointed or closed |
| `reviews/2026-08-28-macos-proc-signal-single-failure.md`, `reviews/2026-08-28-windows-topology-kill-single-failure.md` | historical review records moved to the private lab repository on 2026-09-04 (`9885e475`); they are not in this repository to point at |
| `report.json` | a run artifact's filename, not a repository path — it has never existed in any commit here |

An earlier revision of this paragraph said five, listed only the first five names, and called four
of them an obvious `src/` prefix away. Three are. `coordinator.rs` needs `src/engine/`, and
`attempt.rs` matches two files, not three. Repairing these nine lines is Phase 0 work (§6) and moves
at most nine findings between rows — fewer in practice, because four of them name nothing in this
repository and so may not move into an `src/` row at all.

An earlier draft of this table gave `workspace_manager` as 58. That was the parent, `tests.rs` and
`worktree.rs` and nothing else, and it dropped the eleven findings filed against `residue.rs` (3),
`parsers.rs` (2), `object.rs` (2), `fixture.rs` (2), `hooks.rs` (1) and `containment.rs` (1).

**`workspace_manager` is 22% of the backlog in a single serial lane, and it is the critical path.**
No amount of added parallelism shortens it, because no second `workspace_manager` pull request may
be in flight beside the first. Its 69 are 3 P1, 34 P2 and 32 P3; folded into the matrix's batch
sizes lane by lane — 24 `correctness` P2 at five to a batch is five pull requests, 15
`docs-contract` P3 at twenty is one, `security-trust` never batches — they come to **16 sequential
pull requests**: 3 in Phase 1, 10 of P2, 3 of P3. Treat 16 as a floor, not an estimate. Adjacency
splits batches further, a repair round costs a lane wall-clock without reducing the count, and a
write set that turns out to be wrong returns its finding to the queue. The tail is where nearly
all the real parallelism lives.

Two consequences the orchestrator must act on:

- **Keep the longest lane hot.** `workspace_manager` starts first and never idles. Scheduling it
  as filler makes it the tail that decides when the sweep ends.
- **The P1s cluster in the two longest lanes.** The `agent/proc` module holds 4 of the 14 P1s and
  `workspace_manager` holds 3; three more have no `location:` and cannot be placed in any lane
  until triage gives them one. With one P1 in flight globally, the two longest chains are forced
  to take turns — which is the single biggest scheduling cost in the whole backlog, and it is why
  P1s run as an opening phase (§6) rather than interleaved.

---

## 5. Four rules that override the matrix

**1. Adjacency beats severity, and adjacency — not the module — is the bar inside a batch.** Two
findings in the same module may share a batch, and normally will. Two findings whose fixes touch the
same **function** may not, even when the matrix permits the count.

**Adjacency is not only functions. It is any place two fixes must both write to one line or one
insertion point**, and the common case is a shared declaration rather than shared logic: the `use`
block at the top of a module, a `mod` list, a `#[derive]` or attribute list, an enum's variants, a
`match` whose arms are added at one place, an allowlist or table appended to at its end. Two fixes
in different functions of one file collide there routinely. Measured: one member changed `alpha()`
and added `use std::time::Duration;`, the other changed `beta()` and added `use std::path::Path;`
after the same line — first pick exit **0**, second exit **1**, conflicting on the import block and
nothing else. A function-only reading of this rule calls that pair adjacency-disjoint and is wrong.

**Adjacency changes who does the work, not whether the batch survives.** A shared function is worth
splitting the batch for, because two fixes rewriting one body need to be read together. A shared
declaration is not: it is a one-line resolution in the pick, taken by the implementer, and §2 says
what happens then. Neither is a scheduling error and neither is filed against a finding.

The two constraints have different scopes and different checks: write-set disjointness (§4) governs
what runs *concurrently* and is computed by the orchestrator over each finding's declared writes —
not its `location:` — before any agent is spawned, and nothing verifies it afterwards; adjacency
governs what shares a *diff*, needs the code read to see, and so is raised by the implementer.

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

**Phase 0 — triage. Nothing else starts until this clears.** Recomputed at `44edb2a1`, 2026-09-10,
**52 findings** cannot be scheduled as they stand — the first five bullets below, which are
disjoint sets and sum exactly. A sixth bullet is not unschedulable but is waste:

- **39 findings have an empty `location:`.** They are unschedulable — with nothing to derive a write
  set from, they cannot be allocated a lane or checked for disjointness (§4). Three of them are P1s.
- **One finding is `disposition: fixed`** and should have been deleted; `README.md` is explicit
  that this directory holds outstanding work and nothing else.
- **Two findings carry the README's placeholder text** verbatim in `disposition`.
- **One finding is `P4`**, outside the vocabulary `.github/scripts/test-pr-policy.sh` enforces.
  It is either a P3 or it is not a finding.
- **Nine `location:` lines name a path that does not exist from the repository root.** §4 lists all
  nine against the repair each one needs, because they are not one rename: three want a `src/`
  prefix, `coordinator.rs` wants `src/engine/`, `attempt.rs` is ambiguous between two files, three
  name trees retired from this repository, and `report.json` names a run artifact that has never
  been a path here. A path a script cannot resolve is a write set it cannot be given (§4).
- **Two P1s share `src/agent/proc.rs:2320`** and are probably one finding filed twice. Fixing a
  duplicate twice wastes a P1 slot, which is the scarcest thing in the sweep.

**How the 52 was counted**, so it can be redone rather than believed: read `location:`, `severity:`
and `disposition:` from all 307 files at `44edb2a1`; take the file part of each nonempty `location:`
and test it against `git ls-tree -r --name-only 44edb2a1`. That gives 39 empty, 9 unresolved, 1
`fixed`, 2 placeholder and 1 `P4`. The five sets were then checked pairwise for overlap; there is
none, so 39 + 9 + 1 + 2 + 1 sums to **52** rather than merely totalling it. An earlier revision of
this section said 48, having counted five unresolved paths instead of nine.

**Phase 0 is not clear until every nonempty `location:` resolves.** That check is run over the whole
directory and must come back empty — not sampled, and not satisfied by the field looking populated.
A finding whose `location:` names a path that is not in the tree cannot be given a write set and so
cannot be scheduled at all (§4). Because the write set is what every exclusion decision is computed
from, an unresolved path left in the ledger does not merely mislay one finding; it removes the basis
for the disjointness the round is relying on.

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

Reviews run on the **head of the pull request**, after the gates are green on it — the batch
branch where there is one, and the fix branch itself where the matrix says one finding per pull
request and no batch branch exists (§2). Every P1 is in that second case, so a rule that reviewed
only batch branches would make Phase 1 unreviewable. What is never reviewed is a fix branch that
is not a pull request head: its diff is not what lands.

**Two lenses always, a third by lane.**

- **Fix-check** — for each finding the pull request closes, by `id`: is it actually closed? It is
  given the finding files, not a summary of them. On a singleton that is one finding.
- **Regression** — does the combined diff break anything that worked? This lens exists because the
  combined diff is a thing no single implementer saw.
- **Third lens** — per the matrix: executed-on-platform, replay, or adversarial.

Mechanics that are not optional, each of which has cost time:

- **Every lens writes to its own output path.** Three lenses that default to one filename overwrite
  each other and the run is only recoverable from per-lens logs.
- **Never push while a review is in flight.** `review-pr.sh` resolves the head through the API,
  which lags a push, and will happily return a valid-looking verdict on the previous tree. It has a
  `git ls-remote` cross-check and refuses on disagreement; do not defeat it by pushing anyway.
- **Read the clause table, not the verdict line.** A `CHANGES_REQUIRED` whose findings are all
  documentation residue its lane is not required to fix is a merge candidate under §8; whether the
  lane is required to fix them is `MAINTAINING.md`'s answer, below, not the verdict line's. Gate 4
  passed on exactly such a verdict:
  all six pass-rule clauses and all thirteen adversarial tests Established, blocking on two prose
  descriptions. Reading the verdict line alone said the gate had failed. It had not.

**Triaging a review.** *What a lane must fix before it may merge is `MAINTAINING.md`'s rule, not
this one, and nothing here loosens it.* `MAINTAINING.md` sets a mandatory fix set per lane —
the P3 findings lane is ready only on a `PASS`, the P1/P2 findings lane fixes P0–P2 and files P3,
feature and sweep work fixes P0–P1 and files P2 and P3 — and which of those a sweep branch selects
is the open question in §9. On top of whatever that lane requires:

- **P1** → repair round on the same head, by a fresh `claude-opus-5` `max` session, then re-review.
- **Anything the lane is not required to fix** → filed as a new finding with a `deferred` ledger
  row, one file per finding, and the batch proceeds.
- **Fixed whatever its label, in every lane:** a finding carrying a failing test, a reproduction or
  a mutation witness, **and a deviation from a mandatory standard in code this change touches.**
  That second exception is `MAINTAINING.md`'s and an earlier draft of this section dropped it.

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
- every finding its lane must fix under `MAINTAINING.md` is fixed rather than filed (§7). The
  `ready-to-merge` label is not the authority here and `MAINTAINING.md` says so — a label is the
  audit's output, never its input — and the audit resolves a `bulk-fix-*` branch to the `feature`
  lane today, a weaker set than findings work carries. Until §9's lane-mapping question is
  answered, a batch that would defer a finding a findings lane must fix does not enter the queue
  on the strength of that label;
- every lens for its lane has reported, and no P1 is outstanding;
- the body carries the six sections and the canonical nine-column ledger header;
- the ledger row for each member finding is present, and each member's file is deleted in the same
  pull request — which is why that deletion is part of the write set §4 reserves for every fix.

**The queue *is* locked while a gate run is in flight.** A gate report is bound to a frozen range;
if `master` moves under it the range is stale, and the packet forbids amending a failed gate's
report — so a moved range costs a whole re-run. This is not hypothetical: it is why Gate 4 ran three
times.

**A branch behind `master` is not hand-updated to merge.** The queue rebuilds it. Update only when
the change genuinely needs something `master` gained.

Merging is the owner's act unless delegated in writing for that pull request, and the delegation is
disclosed in the body. Never push to `master` directly. Delete the batch branch and every member
fix branch after the merge — a fix branch left behind still reads as a live claim on its module.

**Which is why every fix commit carries its finding in a trailer.** The branches are deleted; the
trailer is what survives them, and together with the range the rollback selects over it is the whole
of the per-finding revert guarantee in §2:

```
Finding: <id>                       the finding's `id`, from its frontmatter — not its filename
```

**The value is the `id` and nothing else.** Two earlier revisions of this section used
`<category>_<desc>` from the filename, which drops severity and timestamp, so two different findings
can carry one value — and the ledger's naming permits it. Measured on
`P2_correctness_202609110100_beta.md` and `P3_correctness_202609110200_beta.md`, two findings with
disjoint locations that both reduce to `correctness_beta`: with the batches merged one after the
other, rolling back the second removed both fixes and restored both finding files, `git revert
--no-commit` exit **0**. The `id` is the stable identifier `README.md` already designates for
exactly this, and `scripts/pr-ready-audit.sh` already resolves findings by it. Across all 307 files
at `44edb2a1` the 307 ids are distinct — measured; no gate checks that, so it is a property of the
ledger today rather than a guarantee.

**So a fix branch and its commits are named by different things, deliberately.** The branch carries
`<category>_<desc>` from the filename, because that is what the branch gate resolves back to a
finding file and what makes `git ls-remote --heads origin 'fix-*'` readable as a board (§2). The
commit carries the `id`, because that is what has to be unambiguous years later when the branch is
gone. Naming both after the filename is what produced the failure above.

Rolling one member out of a landed batch:

```bash
MERGE=<the batch's merge commit>
ID=PRX-BETA-DOCSTRING

# the batch's OWN history, and git's own trailer parser
git log --format='%H %(trailers:key=Finding,valueonly,separator=%x2C)' "$MERGE^1..$MERGE^2" \
  | awk -v id="$ID" '$2 == id { print $1 }'                    # newest first
git revert --no-commit <newest> … <oldest>                      # exit 0
git commit -m "revert: $ID"
```

**Two details in that command are the whole of it, and each replaces something that was measured to
fail.**

- **`$MERGE^1..$MERGE^2`, not `<batch-base>..$MERGE`.** The base-to-merge range walks the first
  parent through every batch that landed earlier, so it contains their commits too. On the pair
  above, the documented `B..<second merge>` range selected both batches' fixes; `^1..^2` is the
  second-parent side alone and selected one. Exit codes captured directly:

  ``` base..merge      + a message-wide grep     2 of 2 selected   revert exit 0   BOTH fixes gone,
  both finding files restored merge^1..merge^2 + the trailer parser       1 selected        revert
  exit 0   the other member's fix and its deletion intact ```

- **`%(trailers:…)`, not `--grep`.** `--grep` searches the whole message, so a commit that *quotes*
  another finding's trailer — in a fenced block explaining what it is not — matches it. Measured: an
  anchored `--grep='^Finding: PRX-BETA-CRASH$'` selected a commit whose only real trailer is
  `PRX-BETA-DOCSTRING`. `%(trailers:key=Finding,valueonly)` reads the trailer block and reports
  `PRX-BETA-DOCSTRING` alone, and `git interpret-trailers --parse` agrees.
  `scripts/pr-ready-audit.sh` had to make the same distinction for frontmatter ids and its comment
  says so: a matching line in prose or a code block is not the field.

  The comparison is then string *equality* in `awk`, not a pattern, so no anchoring question arises
  and a prefix relation between two ids cannot mis-select.

With the batch's own history and a parsed trailer matched exactly, the other member's fix survives.
Two cases are worth stating because they are the ones that do not: **reverting the batch merge**
(`git revert -m 1 <merge>`) removes *every* member, so it is a batch-level act and never a
per-finding one; and a **repair round** (§7) commits after assembly, where nothing binds a commit to
a member unless the trailer does. Keep a repair commit to one finding and give it that finding's
trailer. A repair that genuinely spans members forfeits the per-finding revert for the members it
spans, and the batch is the unit again for those — say so in the pull request body rather than
discovering it during a rollback.

---

## 9. What this does not decide

- **Whether `src/workspace_manager.rs` should be split** before the sweep reaches it. Splitting it
  converts the longest serial lane into several parallel ones, which is the single highest-leverage
  change available to the schedule — and it invalidates the `location:` line of all 35 findings that
  point into it. That trade is the owner's.
- **Whether `camwork` opens to Opus implementers** (§1).
- **Whether anything but discipline holds a write-set reservation** (§4). The reservation is
  declared before an agent is spawned, and the agent is trusted not to write outside it. Nothing
  checks that: no gate compares a fix branch's diff against the paths its finding reserved, and
  nothing derives the reservation from the finding file either — `location:` is the only
  machine-readable field there, and §4 is the reason it is not sufficient. The obvious next thing to
  build is a checker that reads the declared set, diffs the fix branch against it, and fails the
  branch on an undeclared path; where the declared set is *recorded* is part of that design and is
  deliberately not settled here. Note what such a checker would and would not buy: it would catch a
  fix that wrote outside its reservation, but not a reservation that was incomplete when it was
  written, which is the harder half and stays a reading duty. **Until it exists there is no
  enforcement at all**, and no partial one either: the queue is not a fallback, because it never
  compares a diff with a reservation (§4, measured). A violation lands silently unless it happens to
  collide textually or break an assertion.
- **Which lane a sweep pull request is in.** `scripts/pr-ready-audit.sh` decides a lane from the
  branch prefix alone and knows three: `codex/findings-p3-*`, `codex/findings-*`, and everything
  else. Both prefixes this process uses fall through to *everything else*. Running the audit's own
  `lane_for` and `must_fix_for` on the names the scheduler generates:

  ```
  fix-P1/correctness_foo      lane=feature       must_fix=[P0 P1]
  bulk-fix-P2/some-slug       lane=feature       must_fix=[P0 P1]
  bulk-fix-P3/some-slug       lane=feature       must_fix=[P0 P1]
  codex/findings-x            lane=findings-p1p2 must_fix=[P0 P1 P2]
  codex/findings-p3-x         lane=findings-p3   must_fix=[P0 P1 P2 P3]
  ```

  So the audit would call a sweep batch `feature` and require P0/P1 only, which is weaker than the
  findings lanes `MAINTAINING.md` sets for findings work. Closing that gap is a change to
  `MAINTAINING.md` and to the audit together — the prefixes PR #251 reserves are reserved pending
  exactly this — and it is deliberately **not** made here. Until it lands, §8's entry rule stands:
  the lane rule in `MAINTAINING.md` governs, and the label does not.
- **The branch-name gate has never been proven to reject on a live pull request.**
  `validate-pr-branch.sh` is on `upstroke-pr-policy` in PR #251, still draft. Its *grammar*
  already covers both prefixes this process uses — the names the scheduler generates were run
  against it and pass, and an `id`-based name was run against it and is rejected — but a gate that
  has only ever been exercised by its own fixtures is not yet a gate, and grammar is not the lane
  question above. Landing #251 and watching it
  refuse one real branch is owed before the sweep leans on it.
