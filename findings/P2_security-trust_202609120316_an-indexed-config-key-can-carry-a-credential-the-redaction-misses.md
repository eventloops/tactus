---
id: PR271-R4-CONFIG-KEY-CARRIES-A-CREDENTIAL
severity: P2
disposition: deferred
category: security-trust
pr: 271
reviewed_sha: 4c245225a36f7e815da113445849515a611f67e9
location: src/workspace_manager/fixture.rs:634
provenance: introduced_by_feature
first_bad: 4c245225a36f7e815da113445849515a611f67e9
guard: the next change to the fixture's diagnostic redaction
---

## Failure sequence

Executed by the round-4 fix-check lens at `4c245225a36f7e815da113445849515a611f67e9`.

Round 4 redacts `GIT_CONFIG_VALUE_*` and `GIT_CONFIG_PARAMETERS` from the fixture's assertion
diagnostics, on the reasoning that only those families can carry a secret. **An indexed *key* can
carry one too**: Git accepts `url.https://TOKEN@github.com/.insteadOf` as a configuration key, and
`ls-remote --get-url` accepted exactly that and produced the credential-bearing URL at exit `0`.

Reproduced against the exact-head test binary with a sentinel token:

```
GIT_CONFIG_COUNT=1 \
GIT_CONFIG_KEY_0='url.https://REVIEW_SENTINEL_TOKEN@github.com/.insteadOf' \
GIT_CONFIG_VALUE_0='https://github.com/' \
UPSTROKE_PR271_REPLACEMENT_CONTROL_PROBE=pinned \
"$TEST_BIN" --exact workspace_manager::tests::replacement_control_probe_helper --ignored --nocapture
```

Exit `101`, with the diagnostic printing:

```
"GIT_CONFIG_KEY_0=url.https://REVIEW_SENTINEL_TOKEN@github.com/.insteadOf"
"GIT_CONFIG_VALUE_0=<redacted, 19 bytes>"
```

The value is redacted; the key beside it is not.

## What the change that takes this up should do

Redact indexed key values as well as indexed values, and add a synthetic-token regression that fails
if a diagnostic ever prints one. Correct the claim that only two variable families can carry secrets.

**Not merge-blocking, recorded here for why:** it is a P2 that does not block this pull request's
goal — the same lens confirmed the finding is fixed and guarded, with every repair withdrawn
producing `101` under clean and all five hostile environments. Neither limb of the owner's
2026-09-11 rule is met: it needs an operator to have placed a credential inside a configuration
*key*, which is not this box's configuration (here the credential is in the value, and that is
redacted), and it is unreachable by anyone without push access. It is test-diagnostic code and
reaches no production path.
