---
id: PR265-R6-I686-TEST-COMPILE
severity: P2
disposition: deferred
category: portability
pr: 265
reviewed_sha: c683b1cd17895e2e46954f1ae7068079928beff4
location: src/agent/proc.rs:6401
provenance: introduced_by_feature
first_bad: c683b1cd17895e2e46954f1ae7068079928beff4
guard: the next portability pass over the termination tests
---

## Failure sequence

Executed by the round-6 regression lens through `upstroke-build`:

```
cargo +1.85.0 check --locked --all-targets --all-features --target i686-unknown-linux-gnu
```

Base `749966f` exits `0`; head `c683b1cd17895e2e46954f1ae7068079928beff4` exits `101` with two errors introduced by this
pull request's tests:

- `src/agent/proc.rs:6401` — `c_long::from(SECCOMP_MODE_FILTER)` needs `i32: From<u32>`, which
  does not exist on a 32-bit target (`E0277`).
- `src/agent/proc.rs:6641` — `rlim_cur` is `u32` here, `headroom` is `u64` (`E0308`).

These are Linux-wide test definitions and **compile regardless of `UPSTROKE_HELPER_IDENTITY`**, so
turning the identity path off does not avoid them.

## What the change that takes this up should do

Use checked conversions to the actual libc field types at both sites, rather than `from`
conversions that happen to hold on 64-bit. Then re-run the command above and show `0`.

**Not merge-blocking, recorded here for why:** `i686-unknown-linux-gnu` is not one of this
repository's required contexts and is not in the eight-command baseline, both of which are green at
this head; the break is confined to test definitions and reaches no production path. It is real and
should be fixed — it is filed rather than merged over silently.
