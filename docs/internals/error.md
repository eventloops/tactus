# `src/error.rs`

Extended notes for [`src/error.rs`](../../src/error.rs).

These notes preserve the module comments after the annotation repairs. Item headings quote source lines for navigation.

## `#[derive(Debug)]`

Structural problems found in a parsed plan, collected so a single run
surfaces every issue at once instead of failing on the first.

## `#[derive(Debug)]`

An operation's refusal together with warnings gathered before it failed.
The original typed error remains available for callers that classify it.

## `pub error: Box<UpstrokeError>,`

The unchanged refusal, including its original error category.

## `pub warnings: Vec<String>,`

Diagnostics in the order they were gathered before the refusal.

## `write!(f, "{}", self.error)?;`

These errors belong to the formatter; no operation context can be
added when its destination refuses a write.

## `std::error::Error::source(self.error.as_ref())`

Display already includes the original error, so forwarding its
source avoids repeating it when the CLI renders the error chain.

## `pub enum ProcessFate {`

What a Runner established about an invocation's process when it returned an
error, made where the evidence is — the host funnel at its spawn, kill and
reap points, the container runner from its cancel and release results — and
carried by [`UpstrokeError::Runner`] and `runner::RunnerError`.

`NeverStarted`: no process of the invocation was ever started; the launch
was refused, or failed and every resource it reached was released. `Gone`:
a process started and the Runner has since established it is gone — exited
and reaped, or stopped and removed — without a verdict to report. `Unresolved`:
the Runner cannot say; a process of the invocation may still be running.

The integration verification routes on it (`engine::topology::run`): the
first two are observed infrastructure failures with a terminal of their own,
the third ends the command resumably with nothing appended, because a
terminal authorizes cleanup and readmission and neither may run beside a
process whose liveness is unknown (`invariants[INV-15]`; the reviews of
`916852c9`, regression 1). `describe` is the phrase the error's `Display`
carries so a park question or a refusal says which.

## `#[derive(Debug, Error)]`

Library failures classified by the operation or refusal a caller can handle.
A failure with earlier diagnostics retains its category inside
[`Self::WithWarnings`].

## `#[error("failed to {operation} {}: {source}", .path.display())]`

A filesystem operation on a path the engine owns failed. Named for
the operation, because a removal, a write or a rename that fails did
not fail to read (§7's operation-context rule); `Io` stays the
variant for reads.

## ``#[error("the Runner could not complete `{invocation}` ({}): {source}", .fate.describe())]``

A Runner's error, with the [`ProcessFate`] it established. The crate-error
form of `runner::RunnerError`, so a `?` through the attempt path keeps the
fate in the message; the `source` is the refusal, spawn error or runtime
failure the Runner met, boxed as `WithCleanup` boxes its primary.

## ``#[error("cannot resume run `{run_id}`: {message}")]``

A resume precondition failed (§15). Always carries what to do about it:
refusing to continue is only useful if the operator can tell which of
the four things moved — the run, the plan, the config, or the branch.

## `#[error("{message}")]`

A request we could not act on — an id that matches nothing or too many
things, a question already answered, an option that does not exist.
Carries its own whole sentence, because prefixing these with a
command's name (`cannot resume …` on a `status` lookup) misdescribes
what the operator was actually doing.

## `#[error(transparent)]`

Non-fatal diagnostics do not replace or hide the operation's refusal.

## `pub(crate) fn with_warnings(self, mut warnings: Vec<String>) -> Self {`

Carry earlier warnings through a refusal without changing a clean
error's variant. An existing diagnostic bundle is flattened in order.

## Cleanup failures

`WithCleanup` owns the original typed failure and each later cleanup failure.
Successful cleanup returns the original variant unchanged. Further failures
append in observation order. Display renders the primary once, followed by
labelled cleanup failures. Its source forwards the primary's underlying
cause, matching `WithWarnings`, so rendering does not repeat the primary.
The boxed primary breaks the recursive type without shared ownership.

This adds a public error variant. Downstream exhaustive matches must handle
`WithCleanup`; callers can inspect its primary and additional typed errors.
