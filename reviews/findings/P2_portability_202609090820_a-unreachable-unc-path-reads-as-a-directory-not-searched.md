---
id: HOST-PATH-CENSUS-UNC-TRANSPORT-ERROR
severity: P2
disposition: deferred
category: portability
pr: 7
reviewed_sha:
location: src/runner/host/tests.rs:5550
provenance: pre_existing
first_bad:
guard: the round that lets the census distinguish "this entry was skipped by the rule" from "this entry could not be reached"
---

## Failure sequence

`every_path_entry_this_runner_searches_names_a_location_on_its_own`
(`src/runner/host/tests.rs:5540-5556`) walks a table of `PATH` entries and, for each one it expects the
resolver to search, asserts on the text of the resolver's error:

```rust
assert!(
    message.contains("1 directory searched,"),
    "`{entry}` names a location and was not searched: {message}"
);
```

One of those entries is the UNC path `\\server\share\bin`. The assertion holds only while a stat under
that path fails in a way the resolver counts as *having searched a directory*. On the Windows guest it
can instead fail in transport:

```
`\\server\share\bin` names a location and was not searched: failed to stat
\\server\share\bin\upstroke-no-such-program.com:
The specified network name is no longer available. (os error 64)
```

`ERROR_NETNAME_DELETED` is not `ERROR_FILE_NOT_FOUND`. The resolver reports honestly that it could not
search the directory, and the test reads that as the resolver declining to search a location it should
have. **The resolver's behaviour is arguably right and the test's expectation is what is wrong**: it
assumes a nonexistent UNC server is always reachable enough for a lookup to complete, which depends on
the guest's network state and on which error the redirector returns at that moment.

Observed on `test (winguest)` in run
[34328137648](https://github.com/sourcemaps/upstroke/actions/runs/34328137648), on a pull request whose
entire diff is one Markdown file under `reviews/findings/`, so the change cannot be the cause. The same
leg passed on the immediately preceding head of the same branch, which is what makes it intermittent
rather than a standing red.

## Why this is P2 rather than P3

Nothing in the resolver behaves wrongly. The severity is about what an intermittently red required leg
costs, and `PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE` already put the argument on the record:

> an intermittently red required leg is not a gate — it trains re-running reds, which is how a real
> regression hides

It blocked a documentation-only pull request, and the token cannot rerun a workflow, so the only way
through was to push again. That is the training happening. This is the second such finding on the same
pull request in one night, against two unrelated tests.

## What the change that takes this up should do

Give the census a third answer. Today each entry is either "searched" or "skipped", and the assertion
reads any other outcome as "skipped". A path that could not be reached is neither, and the distinction
is available: the resolver already has the underlying `io::Error`, and `ERROR_NETNAME_DELETED`,
`ERROR_BAD_NETPATH` and `ERROR_BAD_NET_NAME` are transport failures rather than absence.

Either let the test accept a transport failure as evidence that the entry *was* attempted — which is
what "names a location" is really asserting — or drop the live UNC entry from the table and cover the
UNC-shaped rule with a path that needs no network. Do not simply relax the assertion to accept any
message: that would let a genuinely skipped entry pass, and the census exists to catch exactly that.

Whatever is chosen, keep the positive control. The test's value is that it refuses to pass vacuously.
