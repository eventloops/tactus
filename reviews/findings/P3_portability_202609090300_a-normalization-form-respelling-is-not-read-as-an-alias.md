---
id: PR249-MANIFEST-NORMALIZATION-ALIAS
severity: P3
disposition: deferred
category: portability
pr: 249
reviewed_sha: 698777b0929b895480735fbca75eb23e76420524
location: src/workspace_manager.rs:780
provenance: introduced_by_feature
first_bad: PR #249's second repair round (`19a464bb`), which introduced the resolution manifest and `Declaration::names`, a lexical component comparison
guard: `Declaration::names_in_another_case` refuses the case half of the class on every platform, folding case one character at a time (`char::to_lowercase`; the string fold it first used was contextual and read `ΟΣ` beside `οσ` as different names, closed in PR #249's fourth repair round with `a_case_alias_is_read_per_character_so_a_final_sigma_hides_no_contradiction`); the normalization half is pinned as the boundary by `a_declaration_in_another_case_is_refused_and_the_checkout_says_whether_it_named_the_file`, which asserts on each CI platform whether the checkout reads the decomposed spelling as the composed file and that the engine does not, so folding it moves that test
---

## Failure sequence

A conflict repair's index entry is `café.txt`, composed (NFC), as the feedback and the repair
spec spell it to the worker. The worker's manifest respells it decomposed (`cafe` + U+0301),
either alone or beside the composed spelling with the other keyword.

    index: café.txt (composed) unmerged
    manifest: resolved café.txt (composed) / deleted café.txt (decomposed)
    -> `Declaration::names` compares components as written: the decomposed line names nothing
    -> `plan_resolutions` stages `resolved` and ignores the deletion; no contradiction is refused
    -> on a normalization-insensitive volume (macOS APFS and HFS+) the two spellings name one
       file, so the manifest contradicted itself there and the engine did not say so;
       alone, the decomposed declaration is refused as undeclared, though it names the file

PR #249's third-round manifest-contract review, finding 3, executed the planner half and reasoned
the filesystem half. The third repair round fixed the case half of the class —
`names_in_another_case` refuses a contradiction in two cases and a lone respelling in another
case, on every platform — with a string fold that turned out contextual (a final capital sigma
became `ς`, so the Greek pair `ΟΣ`/`οσ`, one file on the Windows guest, escaped it; the fourth
round folds per character), and measured the normalization half on each CI platform through
`a_declaration_in_another_case_is_refused_and_the_checkout_says_whether_it_named_the_file`,
which asserts that the decomposed spelling names the composed file on macOS and nowhere else,
and that the engine reads it as nothing everywhere.

## Why it is deferred

The standard library carries no Unicode normalization tables, so canonical equivalence cannot be
folded the way case is (`str::to_lowercase`); reading it needs either a dependency
(`unicode-normalization`) — a supply-chain decision the owner takes, not a slice — or a filesystem
oracle (`fs::canonicalize` of both spellings under the worktree, equal exactly when the volume
reads them as one file), which is exact for files that exist and silent for a path the worker
deleted. Neither was taken in the third round; the boundary is stated where the grammar is
(`RESOLUTION_MANIFEST`'s doc) and pinned by the test above. The exposure is a worker that writes
a path in a normalization form other than the one it was given, on a macOS checkout, with the
other keyword beside the given spelling; no adapter's text tooling has been measured to do so, and
that is a statement about what was measured, not a promise.

## What the change that takes this up should do

Choose the source of canonical equivalence — the dependency or the filesystem oracle — and fold it
into the alias comparison `plan_resolutions` refuses on, so that a normalization-form respelling
is refused exactly as a case respelling is. Then flip the boundary assertion in the test named
above to the refusal, and drop the "not a case" clause from `RESOLUTION_MANIFEST`'s doc.
