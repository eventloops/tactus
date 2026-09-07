---
id: WORKSPACE-GIT-BUILDER-DOES-NOT-PIN-ATTRIBUTES-OR-FILTERS
severity: P2
disposition: deferred
category: correctness
pr: 145
reviewed_sha: 84c21e01676c9e38c79321984441da86d220ce7e
location: src/workspace_manager.rs:2900
provenance: pre_existing
first_bad:
guard: the next change to `WorkspaceManager::command` -- a behaviour change with a design surface, since disabling `core.attributesFile`, filter configuration and global or system config for every funnel Git child changes what the funnel does for a configured user and so changes `DESIGN.md` in the same pull request; escalates to the owner if a later pass labels it P1 or P2 rather than accepting the deferral
---

## The claim that was withdrawn

PR #145 built its sampled Git child through `WorkspaceManager::command` and claimed that under
production's pins -- an empty `core.hooksPath`, `core.fsmonitor=false`, `protocol.file.allow=never`
and `GIT_NO_REPLACE_OBJECTS` -- `git add`, `write-tree` and `cherry-pick` spawn no children. **The
claim is false and the fourth frontier pass had the probe**: with all four pins active, a
global-style attributes file assigning a clean filter still printed `FILTER_RAN`. The builder does
not disable `core.attributesFile`, filter configuration, or global and system Git configuration, so
a user's configuration reaches every Git child the funnel spawns.

Building the sampled child through the production builder is kept: it is strictly better than the
transcription it replaced, and it is what production runs. The *claim* is withdrawn.

## Failure sequence

The user's global attributes assign `filter=slow` -> a funnel `git add` starts that filter as a
child -> the engine's kill of the Git leader (or an external one) does not reach the filter -> the
filter goes on reading and writing objects into a repository the engine believes quiescent.
Reachable on every platform, and on Windows the sampler kills only the leader, so the filter
survives by construction there.

## What the change that takes this up should do

Decide what a funnel Git child is allowed to see of the user's configuration, and say it in
`DESIGN.md` -- in the section file under `design/` that carries the workspace funnel -- in the same
pull request as the code, because `DESIGN.md` is the only living authority for product design and
there is no separate record to keep in step. Then pin it in `WorkspaceManager::command`:
`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL` pointed at a file the engine owns and knows to be
empty, `core.attributesFile` pointed at the same, and filter, clean and smudge configuration
cleared.

**Not `/dev/null`, which is what this paragraph used to prescribe** (PR #145 pass 5, finding 4).
Windows is a first-class target and has no such path; its null device is `NUL`, Git's own handling
of a null device as a config file is not the same on the two platforms, and a literal POSIX device
name written into a builder every platform runs is exactly the portability defect `standards/`
forbids. An empty file the engine creates under the run directory is one behaviour on both
platforms, and its path is built with `std::path` like every other path this engine holds.

That is a product behaviour change -- it alters what every funnel primitive
does for a user with global attributes -- and it is the reason this is a finding rather than a
repair inside a test-only pull request. The probe above is the regression test it should carry, and
it has to run on Windows as well, since that is where the filter survives by construction.
