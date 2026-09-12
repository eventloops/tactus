// Allowlist placement: the **funnel section** of `effects/allowlist.toml`, by
// attachment to `src/workspace_manager.rs` -- the shape
// `src/runner/container/tests.rs` and `src/agent/proc/test_support/readiness.rs`
// established for a funnel's out-of-line child. This file builds the scratch
// repositories the Worktree/Snapshot/Ref/Object suites measure against, so it
// names `fs::write`, `fs::create_dir_all` and `std::process::Command` directly.
//
// `PR6-LANEF-004`: a Rust lint level is scoped by the MODULE TREE and not by
// the file, so without an attribute here the parent's inner allow of all three
// would reach this file silently and no reviewed record would name the file
// doing the work. `clippy::disallowed_macros` is RE-DENIED rather than
// inherited -- measured at zero sites -- so a `println!` here is still a build
// error. `decisions.effect_site_inventory.mechanism` (2).
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]
#![deny(clippy::disallowed_macros)]

use super::*;

// `OsStr` came from the parent's import list until the `m4-workspace` split
// moved its last production user into a child; named here for the same reason.
use std::cell::{Cell, RefCell};
use std::ffi::OsStr;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

// -----------------------------------------------------------------------
// Fixtures
// -----------------------------------------------------------------------

pub(crate) static SCRATCH: AtomicU32 = AtomicU32::new(0);

// -----------------------------------------------------------------------
// Observing the removal retry
// -----------------------------------------------------------------------

/// What to run after each of this thread's removal attempts.
///
/// A named alias because the raw shape trips `clippy::type_complexity`, and
/// because the two `thread_local!` slots below read better for having it.
type AttemptObserver = Box<dyn FnMut(u32)>;

// Attempts `remove_tree_once_handles_close` has made **on this thread**, and
// the observer to run after each.
//
// Thread-local rather than global, and that is the whole reason it is sound:
// the suite runs tests in parallel and several of them remove worktrees, so a
// process-wide counter would be another test's number as often as this one's.
// The primitive runs on the thread that called `remove_worktree`, so a
// thread-local counts exactly the removals the observing test drove.
//
// `PR5-R1-CFG-TEST-SHRINKS-THE-DOMAIN` is why the parent's half of this seam
// is declared at the bottom of `src/workspace_manager.rs` rather than beside
// the primitive: `effects::production_region` truncates a source at its first
// `#[cfg(test)]`, so a `#[cfg(test)]` item above the funnels would take every
// one of them out of the census that proves the Worktree group has them.
//
// `//` rather than `///`: rustdoc does not document a macro invocation, and
// `-D unused-doc-comments` says so.
thread_local! {
    static REMOVAL_ATTEMPTS: Cell<u32> = const { Cell::new(0) };
    static REMOVAL_ATTEMPT_OBSERVER: RefCell<Option<AttemptObserver>> =
        const { RefCell::new(None) };
}

/// A live observation of this thread's removal attempts, ended by dropping it.
///
/// Held by the observing test for exactly as long as the observation is wanted;
/// its `Drop` uninstalls the observer, so a test that unwinds cannot leave a
/// closure behind for whatever runs next on this thread.
pub(crate) struct AttemptObservation {
    /// Not `Send`: the counter and the observer are this thread's.
    _not_send: PhantomData<*const ()>,
}

impl AttemptObservation {
    /// Attempts made since the observation began.
    pub(crate) fn count(&self) -> u32 {
        REMOVAL_ATTEMPTS.with(Cell::get)
    }
}

impl Drop for AttemptObservation {
    fn drop(&mut self) {
        REMOVAL_ATTEMPT_OBSERVER.with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                *slot = None;
            }
        });
    }
}

/// Start counting this thread's removal attempts, running `observer` after each.
///
/// The observer runs **on the removing thread, after the attempt has already
/// returned**, which is the property the closing-handle control is built on: a
/// test that releases a held handle from here knows the attempt it is releasing
/// against has completed, rather than hoping it has. Nothing outside the loop
/// can establish that, which is why this seam exists at all.
pub(crate) fn observe_removal_attempts(observer: AttemptObserver) -> AttemptObservation {
    REMOVAL_ATTEMPTS.with(|count| count.set(0));
    REMOVAL_ATTEMPT_OBSERVER.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            *slot = Some(observer);
        }
    });
    AttemptObservation {
        _not_send: PhantomData,
    }
}

/// Record that attempt `attempt` has completed, and run the observer.
///
/// Called from `super::note_removal_attempt`, the `#[cfg(test)]` half of the
/// primitive's seam. `try_borrow_mut` rather than `borrow_mut` so that an
/// observer which somehow removes a tree of its own is a no-op here instead of
/// a panic inside production code.
pub(crate) fn note_removal_attempt(attempt: u32) {
    REMOVAL_ATTEMPTS.with(|count| count.set(attempt));
    REMOVAL_ATTEMPT_OBSERVER.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            if let Some(observer) = slot.as_mut() {
                observer(attempt);
            }
        }
    });
}

/// A scratch directory unique to this process *and* to this call, because
/// the suite runs tests in parallel and two fixtures sharing a directory
/// would each measure the other's Git repository.
pub(crate) fn scratch(tag: &str) -> PathBuf {
    let ordinal = SCRATCH.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "upstroke-wm-{tag}-{}-{ordinal}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the scratch directory");
    dir
}

pub(crate) fn git_out(dir: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git")
}

pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let output = git_out(dir, args);
    assert!(
        output.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A real repository, a real private root, and a manager over both.
/// The fixture's run id: a canonical ULID, as `derive` requires
/// (`DESIGN.md` §15, "run-id = ULID"), spelt to be recognisable in a path.
pub(crate) const RUN_ID: &str = "01KZSWEEP00000000000000001";

pub(crate) struct Fixture {
    pub(crate) root: PathBuf,
    pub(crate) base: PathBuf,
    pub(crate) private: PathBuf,
    pub(crate) manager: WorkspaceManager,
    /// The first commit.
    pub(crate) seed: String,
    /// The tip of `main`.
    pub(crate) head: String,
    /// A commit on a side branch, based on `seed`, for the cherry-picks.
    pub(crate) side: String,
}

impl Fixture {
    /// A SHA-1 repository, whatever `GIT_DEFAULT_HASH` says in the
    /// environment: the object format is part of what a test asserts about
    /// (an object id's length, the null id's spelling), so the fixture pins
    /// it rather than inheriting it (§12). [`Self::with_object_format`] is
    /// the other format.
    pub(crate) fn new(tag: &str) -> Self {
        Self::with_object_format(tag, "sha1")
    }

    /// A repository of the given object format, `sha1` or `sha256`.
    pub(crate) fn with_object_format(tag: &str, object_format: &str) -> Self {
        let root = scratch(tag);
        let base = root.join("repo");
        let private = root.join("private");
        fs::create_dir_all(&base).expect("repo directory");
        fs::create_dir_all(&private).expect("private root");

        let object_format = format!("--object-format={object_format}");
        git(&base, &["init", "-q", "-b", "main", &object_format]);
        git(&base, &["config", "user.email", "tests@upstroke.local"]);
        git(&base, &["config", "user.name", "upstroke tests"]);
        // Line endings are pinned for the same reason the object format is
        // (§12, and `PR126-REVIEW2-NULL-TESTS-INHERIT-THE-HASH-FORMAT`): an
        // ambient Git setting that silently changes what a test observes.
        // With `core.autocrlf` on, as it is on the Windows guest, a blob
        // written as `A\n` is checked out as `A\r\n`, so a test comparing
        // checked-out content against what it wrote fails on that platform
        // alone while the blob is the one it asked for.
        git(&base, &["config", "core.autocrlf", "false"]);
        git(&base, &["config", "core.eol", "lf"]);
        // Whether `refs/replace/*` is honoured is pinned for the same reason
        // and against the same class of accident: an operator's
        // `core.useReplaceRefs = false`, or one an `init.templateDir` copied
        // into this repository's own config, decides what four of these
        // suites measure. See `pin_replacement_refs_in`, and
        // `without_ambient_replacement_controls` for the half a repository's
        // configuration cannot reach.
        pin_replacement_refs_in(&base);
        // `git worktree add` writes a reflog entry; keep the repository
        // self-contained so nothing depends on a global config.
        git(&base, &["config", "core.logAllRefUpdates", "true"]);
        fs::write(base.join("a.txt"), "one\n").expect("seed file");
        git(&base, &["add", "-A"]);
        git(&base, &["commit", "-q", "-m", "seed"]);
        let seed = git(&base, &["rev-parse", "HEAD"]);

        fs::write(base.join("b.txt"), "two\n").expect("second file");
        git(&base, &["add", "-A"]);
        git(&base, &["commit", "-q", "-m", "second"]);
        let head = git(&base, &["rev-parse", "HEAD"]);

        git(&base, &["checkout", "-q", "-b", "side", &seed]);
        fs::write(base.join("c.txt"), "side\n").expect("side file");
        git(&base, &["add", "-A"]);
        git(&base, &["commit", "-q", "-m", "side"]);
        let side = git(&base, &["rev-parse", "HEAD"]);
        git(&base, &["checkout", "-q", "main"]);

        // The run's public directory, where `RunLock::acquire` would have
        // created the cleanup lease a coordinator's ref writes hold
        // (`rundir::hold_cleanup_lease_for_child`). No run lock is taken over
        // this fixture, so the directory is made here for the lease file to
        // land in, exactly where the engine's own run directory puts it.
        fs::create_dir_all(crate::rundir::public_dir(&base, RUN_ID))
            .expect("the run's public directory");
        let manager =
            WorkspaceManager::derive(&base, &private, RUN_ID, "inc-1").expect("derive the manager");
        Self {
            root,
            base,
            private,
            manager,
            seed,
            head,
            side,
        }
    }

    /// Re-open a fixture a **previous process** built.
    ///
    /// A kill child dies by `std::process::abort()`, so its `Drop` never
    /// runs and its scratch tree survives it. The parent then has to speak
    /// about that tree — which repository, which private root, which
    /// commits — and re-deriving it is the only honest way: a value passed
    /// through an environment variable would be the child's belief about
    /// its own state, and the whole point of a kill test is that the
    /// child's beliefs did not survive.
    ///
    /// The manager is derived with the **same** run id and incarnation as
    /// [`Self::new`], because an intent records both and a reclaim that
    /// derived a different pair would be reclaiming another run's residue.
    pub(crate) fn adopt(root: PathBuf) -> Self {
        let base = root.join("repo");
        let private = root.join("private");
        let head = git(&base, &["rev-parse", "main"]);
        let seed = git(&base, &["rev-parse", "main~1"]);
        let side = git(&base, &["rev-parse", "side"]);
        let manager = WorkspaceManager::derive(&base, &private, RUN_ID, "inc-1")
            .expect("derive the manager over an adopted fixture");
        Self {
            root,
            base,
            private,
            manager,
            seed,
            head,
            side,
        }
    }

    pub(crate) fn created(tag: &str) -> Self {
        let fixture = Self::new(tag);
        fixture
            .manager
            .create_execution_root(&mut NoHooks)
            .expect("create the execution root");
        fixture
    }

    /// [`Self::created`] over a SHA-256 repository, for the tests that assert
    /// something about both object formats.
    pub(crate) fn created_sha256(tag: &str) -> Self {
        let fixture = Self::with_object_format(tag, "sha256");
        fixture
            .manager
            .create_execution_root(&mut NoHooks)
            .expect("create the execution root");
        fixture
    }

    pub(crate) fn task(&self, key: &str, generation: u32) -> Slot {
        Slot::Task {
            key: key.to_owned(),
            generation,
        }
    }

    /// A task worktree at `head`, intent first.
    pub(crate) fn add_task(&self, hooks: &mut dyn EffectHooks, key: &str, generation: u32) -> Slot {
        let slot = self.task(key, generation);
        self.manager
            .write_intent(hooks, &slot)
            .expect("write the intent");
        self.manager
            .add_worktree(hooks, &slot, &self.head)
            .expect("add the worktree");
        slot
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

// -----------------------------------------------------------------------
// The primitives a topology module's test cannot reach for itself
// -----------------------------------------------------------------------

/// Write `bytes` at `path`, creating the parent directories.
///
/// This is a test's *worker*: in production an agent subprocess edits files
/// and the engine never does (DESIGN.md §4). A test has no agent, so it
/// writes what the agent would have written — and it does it here, where
/// the write is inside the reviewed funnel module, rather than in the
/// topology module whose whole point is that it cannot.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("the parent directory of a fixture file");
    }
    fs::write(path, bytes).expect("write a fixture file");
}

/// Create `path` and every missing parent.
pub(crate) fn create_dir(path: &Path) {
    fs::create_dir_all(path).expect("create a fixture directory");
}

/// A fan-out directory the object store already holds, to construct Git's
/// temporary object file in.
///
/// Git creates the fan-out directory when it is missing and writes the
/// temporary file inside it, so any two-hexadecimal-digit name would do; one
/// that already exists puts the file beside real objects, which is the shape
/// the trace in
/// [`temporary_object_files`](crate::workspace_manager::temporary_object_files)
/// records. Shared by the three places a test constructs the element —
/// `construct_element`, `plant_stage_residue` and the dispatch
/// materialization test — so that none of them plants at the object root,
/// where the arm the scan always had would see it and the fan-out arm this
/// element is evidence for would not be exercised
/// (`PR258-GRID-PLANTS-AT-THE-OBJECT-ROOT`).
pub(crate) fn fan_out_directory(objects: &Path) -> PathBuf {
    let mut names: Vec<PathBuf> = fs::read_dir(objects)
        .expect("the object directory")
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path.file_name().is_some_and(|name| {
                    let name = name.to_string_lossy();
                    name.len() == 2 && name.chars().all(|character| character.is_ascii_hexdigit())
                })
        })
        .collect();
    names.sort();
    names
        .into_iter()
        .next()
        .expect("a store with objects in it has a fan-out directory")
}

/// Remove `path` if it is there. Idempotent, like every reclaim.
pub(crate) fn remove_file(path: &Path) {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("removing {}: {error}", path.display()),
    }
}

/// Remove the empty directory at `path`: a worker's file operation, standing
/// for a tool that removes a directory it has emptied.
pub(crate) fn remove_dir(path: &Path) {
    fs::remove_dir(path)
        .unwrap_or_else(|error| panic!("removing the directory {}: {error}", path.display()));
}

/// Run this test binary again, `--exact --ignored`, with `env` set, and
/// return its exit status.
///
/// The kill-test shape `src/rundir.rs` established: `Injection::Kill` is
/// `std::process::abort()`, a real process death, so the child has to be a
/// real process and the claim is what it left on disk. `env` is a list
/// rather than a map so a caller can pass the same key twice and see the
/// last win, exactly as `Command` does.
pub(crate) fn run_kill_child(test: &str, env: &[(&str, &OsStr)]) -> std::process::ExitStatus {
    let mut command = Command::new(std::env::current_exe().expect("this test binary"));
    command
        .args(["--exact", test, "--ignored", "--nocapture"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    command.status().expect("spawn the kill child")
}

/// Run this test binary again, `--exact --ignored --nocapture`, with `env` set
/// and its stdout piped, adopted by a [`readiness::Producer`]: the caller waits
/// for the line the helper prints once it is ready, and the child is
/// terminated, reaped and its reader joined when the producer drops, however
/// the caller's scope ends.
///
/// For the suites under `src/engine/topology/**`, where `clippy.toml` denies
/// `std::process::Command` to tests as well as production and which therefore
/// cannot spawn a helper of their own.
///
/// Unix only, because its one caller is: the helper it spawns holds a shared
/// `flock`, which Windows has no counterpart of. An ungated definition with a
/// `cfg(unix)` caller is dead code on the Windows legs, and CI's `-D warnings`
/// makes that a build error the Linux box cannot see (`#275`, CI run
/// 34693369689).
#[cfg(unix)]
pub(crate) fn spawn_ready_helper(
    test: &str,
    env: &[(&str, &OsStr)],
) -> crate::agent::proc::test_support::readiness::Producer {
    let mut command = Command::new(std::env::current_exe().expect("this test binary"));
    command
        .args(["--exact", test, "--ignored", "--nocapture"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    crate::agent::proc::test_support::readiness::Producer::adopt(
        command.spawn().expect("spawn the ready helper"),
    )
}

// -----------------------------------------------------------------------
// The ambient controls over `refs/replace/*`
// -----------------------------------------------------------------------

/// Removed outright from a Git child that measures replacement behaviour:
/// each of these decides that behaviour by its **presence**, or hands the
/// child a path or a ref namespace the fixture did not write.
///
/// `GIT_NO_REPLACE_OBJECTS` and `GIT_REPLACE_REF_BASE` are `git-replace(1)`'s
/// and `git(1)`'s own; `GIT_CONFIG_PARAMETERS` is the undocumented variable
/// Git itself uses to propagate `-c` into its subprocesses, and it reaches
/// `core.useReplaceRefs` exactly as `-c` does; `GIT_CONFIG` redirects
/// `git config`'s **writes**, so a fixture that pins a key with it set would
/// pin it in the operator's file and not in its own repository.
///
/// `GIT_TEMPLATE_DIR` is the seventeenth mechanism, and the one that says why
/// the enumeration is no longer what correctness rests on: it was found by a
/// reviewer within the hour of the previous sixteen being written down.
/// Measured on git 2.43.0, `git init` copies a template's `config` **into the
/// new repository**, above everything `git init` itself writes there, so a
/// template naming `[core] useReplaceRefs = false` decides the question for a
/// repository the fixture created and never asked -- and it does it at
/// creation, which no later environment neutralisation can undo. The same
/// value arrives through `init.templateDir` in a global or system file
/// (measured), and that half is closed by the two pins below.
pub(crate) const REPLACEMENT_CONTROLS_REMOVED: &[&str] = &[
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG",
    "GIT_TEMPLATE_DIR",
];

/// Removed by prefix: the indexed configuration pairs, every index of them.
///
/// **The claim this carried until round 4 was an artefact of the environment it
/// was measured in, and is corrected rather than left standing.** It said that
/// on git 2.43.0 a lone `GIT_CONFIG_KEY_0`/`GIT_CONFIG_VALUE_0` takes effect
/// with `GIT_CONFIG_COUNT` absent. It does not: measured 2026-09-12 with the
/// count genuinely unset, `git config --get core.useReplaceRefs` exits `1` and
/// prints nothing, and exits `0` printing `false` with the count at `1`. What
/// made the earlier reading come out the other way is that this box's build
/// wrapper exports `GIT_CONFIG_COUNT=1` and an indexed pair of its own into
/// every `cargo test` it runs, so the "uncounted" pair was counted by an
/// inherited count nobody had taken away. `git-config(1)` describes the
/// documented behaviour and git 2.43.0 has it.
///
/// What closes the vector is the `GIT_CONFIG_COUNT=0` pin below, and these
/// removals are a second, independent way: measured on git 2.43.0, emptying
/// this list leaves the grid green, so it is defence in depth and not the
/// load-bearing half. They also close the case the pin cannot -- a pair set on
/// the `Command` itself rather than inherited, which the sweep over this
/// process's environment would not see.
pub(crate) const REPLACEMENT_CONTROL_PREFIXES: &[&str] = &["GIT_CONFIG_KEY_", "GIT_CONFIG_VALUE_"];

/// Pinned to a fixed value rather than removed, because an absent value is
/// not a neutral one: absent `GIT_CONFIG_GLOBAL` means *read `$HOME`'s and
/// `$XDG_CONFIG_HOME`'s*, and absent `GIT_CONFIG_COUNT` means *read whatever
/// indexed pairs are there*.
///
/// The two file variables are pinned at a file this process wrote empty, so
/// no `[core] useReplaceRefs`, and no `include.path`/`includeIf` reaching
/// one, survives at either level -- including at `git init`, where a global
/// `init.templateDir` otherwise copies a `config` **into the new repository**
/// and lands below every later `git config` (measured). With both pinned,
/// `git config --show-scope --list` reports `command` and `local` and nothing
/// else (measured), which is the whole claim.
///
/// `GIT_CONFIG_NOSYSTEM` is redundant while `GIT_CONFIG_SYSTEM` is pinned --
/// git(1) says setting the latter means the build-time system file is not
/// read, and the scope listing above confirms it. It is pinned anyway so that
/// neither variable is alone in carrying the claim. The grid does go red when
/// it is dropped, but that is `assert_replacement_controls_pinned`'s own
/// precondition failing, not a measured vector; do not read it as evidence
/// this one is load-bearing.
pub(crate) const REPLACEMENT_CONTROLS_PINNED: &[&str] = &[
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_NOSYSTEM",
];

/// An empty Git configuration file, written once per test process.
///
/// A file this process wrote rather than a path it expects to be absent: Git
/// reads a missing file as empty today, but "nothing created this path" is a
/// claim about the rest of the run, and "this file is empty" is a fact.
/// `/dev/null` is not portable to the Windows leg of the matrix.
pub(crate) fn neutral_git_config() -> &'static Path {
    static FILE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    FILE.get_or_init(|| {
        let path =
            std::env::temp_dir().join(format!("upstroke-neutral-gitconfig-{}", std::process::id()));
        fs::write(&path, b"").unwrap_or_else(|error| {
            panic!(
                "writing the neutral Git configuration {}: {error}",
                path.display()
            )
        });
        path
    })
    .as_path()
}

/// [`REPLACEMENT_CONTROLS_PINNED`] with the values they are pinned to.
pub(crate) fn pinned_replacement_controls() -> Vec<(&'static str, OsString)> {
    let neutral = neutral_git_config().as_os_str().to_owned();
    vec![
        ("GIT_CONFIG_COUNT", OsString::from("0")),
        ("GIT_CONFIG_GLOBAL", neutral.clone()),
        ("GIT_CONFIG_SYSTEM", neutral),
        ("GIT_CONFIG_NOSYSTEM", OsString::from("1")),
    ]
}

/// Whether `key` is one of the enumerated ambient controls, under either
/// name rule: Windows matches environment keys case-insensitively, so a
/// filter that matched only the documented spelling would pass
/// `git_no_replace_objects` straight through on that leg of the matrix.
pub(crate) fn is_ambient_replacement_control(key: &OsStr) -> bool {
    let name = key.to_string_lossy().to_ascii_uppercase();
    REPLACEMENT_CONTROLS_REMOVED.contains(&name.as_str())
        || REPLACEMENT_CONTROLS_PINNED.contains(&name.as_str())
        || REPLACEMENT_CONTROL_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

/// Take every ambient control over `refs/replace/*` away from `command`'s
/// child, so what the child measures is what the code under test installs.
///
/// Rounds 1 and 2 of PR #271 each shipped a witness that passed without its
/// own fix, through two different controls -- `GIT_NO_REPLACE_OBJECTS` and
/// then `core.useReplaceRefs=false`. This is the enumeration both were
/// missing, applied in one place. It is not closed by construction, so the
/// witnesses that could pass silently also call
/// [`assert_replacement_refs_are_live`], which closes it by measurement.
///
/// `the_neutraliser_defeats_every_ambient_control_it_enumerates` is where
/// this function is exercised: CI exports none of these, so without that grid
/// a name could be dropped here and nothing would go red until a reviewer
/// exported it, which is how rounds 1 and 2 were found. Measured against it,
/// dropping `GIT_NO_REPLACE_OBJECTS`, `GIT_CONFIG_PARAMETERS`,
/// `GIT_REPLACE_REF_BASE`, the `GIT_CONFIG_COUNT` pin, the
/// `GIT_CONFIG_GLOBAL` pin or the `GIT_CONFIG_SYSTEM` pin each turns it red.
/// `GIT_CONFIG` is not one of that grid's rows -- it does not change what a
/// Git child reads -- and has its own witness,
/// `a_redirected_git_config_cannot_capture_a_fixtures_own_pin`, which goes red
/// when it is dropped from the list here.
pub(crate) fn without_ambient_replacement_controls(command: &mut Command) {
    for key in REPLACEMENT_CONTROLS_REMOVED {
        command.env_remove(key);
    }
    for (key, _) in std::env::vars_os() {
        let name = key.to_string_lossy().to_ascii_uppercase();
        if REPLACEMENT_CONTROL_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            command.env_remove(&key);
        }
    }
    for (key, value) in pinned_replacement_controls() {
        command.env(key, value);
    }
}

/// This process's environment as [`without_ambient_replacement_controls`]
/// would leave it, for a boundary that takes a base rather than a `Command`.
pub(crate) fn environment_without_ambient_replacement_controls() -> Vec<(OsString, OsString)> {
    let mut base: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(key, _)| !is_ambient_replacement_control(key))
        .collect();
    for (key, value) in pinned_replacement_controls() {
        base.push((OsString::from(key), value));
    }
    base
}

/// Every enumerated control this process still carries, with its value, for
/// a diagnostic that says what the environment actually was.
///
/// **Two of them carry their value redacted, and that is not caution.** This
/// box's build wrapper derives `GIT_CONFIG_COUNT=1`,
/// `GIT_CONFIG_KEY_0=http.https://github.com/.extraHeader` and a
/// `GIT_CONFIG_VALUE_0` holding a GitHub personal access token, and exports all
/// three into every `cargo test` it runs (measured 2026-09-12). A diagnostic
/// that prints an enumerated control's value therefore prints that token into a
/// failing test's output, and into CI's log, the first time one of these
/// assertions fires. `GIT_CONFIG_PARAMETERS` is redacted for the same reason:
/// it is how Git propagates `-c` to its own subprocesses, so it carries the
/// same pairs. The length is kept, because "set and empty" and "set to
/// something" are different diagnoses.
pub(crate) fn ambient_replacement_controls() -> Vec<String> {
    let mut seen: Vec<String> = std::env::vars_os()
        .filter(|(key, _)| is_ambient_replacement_control(key))
        .map(|(key, value)| {
            let name = key.to_string_lossy().into_owned();
            if carries_a_credential(&key) {
                format!("{name}=<redacted, {} bytes>", value.len())
            } else {
                format!("{name}={}", value.to_string_lossy())
            }
        })
        .collect();
    seen.sort();
    seen
}

/// Whether `key`'s **value** may be a credential, and so must never reach a
/// panic message. See [`ambient_replacement_controls`] for the measurement.
fn carries_a_credential(key: &OsStr) -> bool {
    let name = key.to_string_lossy().to_ascii_uppercase();
    name.starts_with("GIT_CONFIG_VALUE_") || name == "GIT_CONFIG_PARAMETERS"
}

/// Refuse to measure replacement behaviour in a process that still carries an
/// ambient control over it, and then refuse again on the evidence.
///
/// The first half is the enumeration by name: nothing in
/// [`REPLACEMENT_CONTROLS_REMOVED`] survived, `GIT_CONFIG_COUNT` is the `0`
/// that closes the indexed pairs whatever indices are present, and the two
/// configuration files are files this process can see and that are **empty**
/// -- their *paths* are per-process, so what a child checks is what they hold
/// and never which path the parent chose.
///
/// The second half is [`assert_replacement_refs_are_live`], which is the one
/// that does not depend on the list being complete.
pub(crate) fn assert_replacement_controls_pinned(tag: &str) {
    for key in REPLACEMENT_CONTROLS_REMOVED {
        assert!(
            std::env::var_os(key).is_none(),
            "`{tag}`: `{key}` reached this process, and the reads it governs \
             would answer about the environment rather than about the code \
             under test: {:?}",
            ambient_replacement_controls()
        );
    }
    assert_eq!(
        std::env::var_os("GIT_CONFIG_COUNT").as_deref(),
        Some(OsStr::new("0")),
        "`{tag}`: `GIT_CONFIG_COUNT` must be pinned at `0`; absent, a lone \
         `GIT_CONFIG_KEY_0` still takes effect (measured on git 2.43.0): {:?}",
        ambient_replacement_controls()
    );
    assert!(
        std::env::var_os("GIT_CONFIG_NOSYSTEM").is_some(),
        "`{tag}`: `GIT_CONFIG_NOSYSTEM` must be pinned: {:?}",
        ambient_replacement_controls()
    );
    for key in ["GIT_CONFIG_GLOBAL", "GIT_CONFIG_SYSTEM"] {
        let Some(path) = std::env::var_os(key) else {
            panic!(
                "`{tag}`: `{key}` must be pinned at an empty file, or the level \
                 it names -- and every `include.path` reaching one -- decides \
                 what this process measures: {:?}",
                ambient_replacement_controls()
            );
        };
        let path = PathBuf::from(path);
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "`{tag}`: reading the pinned `{key}` at {}: {error}",
                path.display()
            )
        });
        assert!(
            bytes.is_empty(),
            "`{tag}`: the pinned `{key}` at {} is not empty, so it is not neutral",
            path.display()
        );
    }
    assert_replacement_refs_are_live(tag);
}

/// What a probe found when it asked whether `refs/replace/*` is honoured.
///
/// A verdict rather than a panic because two of its answers are *evidence* --
/// the grid's controlled leg wants to be told that a treatment really did
/// disable replacements -- and because the third thing that can happen, a
/// `git` command failing outright, must never be counted as either. See
/// [`replacement_liveness`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ReplacementLiveness {
    /// `git replace` wrote under `refs/replace/` and a child read through it.
    Live,
    /// `git replace` wrote somewhere else, so this process is not speaking
    /// about the namespace the design and the finding name.
    RefsElsewhere { found: String },
    /// The refs are where they belong and a child ignored them.
    NotHonoured { read: String },
}

/// The exit status the grid's probe helper uses for
/// [`ReplacementLiveness::RefsElsewhere`] and
/// [`ReplacementLiveness::NotHonoured`].
///
/// Distinct from `0` (live) and from the `101` a panicking `git` helper gives,
/// so a caller can tell *the treatment disabled replacements* from *the setup
/// never ran*. A grid that cannot tell them apart credits a malformed
/// configuration file as evidence of replacement suppression, which is what the
/// include-path row did with a backslash in `TMPDIR` (PR #271, round 3).
pub(crate) const REPLACEMENT_DISABLED_EXIT: i32 = 3;

/// Measure -- in **this process's** environment, over a throwaway repository
/// -- whether a Git child still honours `refs/replace/*`.
///
/// The enumeration above is a list of names, and a list of names is exactly
/// what cost this pull request three rounds: a witness sensitive to one vector
/// and blind to the next passes silently. This is the closure that does not
/// depend on the list being complete, and every replacement witness in the
/// crate runs it before it measures anything, so a control nobody enumerated
/// costs a loud failure rather than a false green.
///
/// The child is `read_only_git`'s and `HostEnvironment::from_process`'s own
/// child: both inherit this process's environment, which is why measuring this
/// process is measuring theirs.
///
/// **Every `git` here runs in this process's environment, `git init`
/// included.** That is deliberate, and it is what closes `GIT_TEMPLATE_DIR`
/// honestly rather than by exclusion: a template writes its `config` into the
/// repository at creation, so a probe whose own `init` were neutralised would
/// build a clean instrument, report `Live`, and say nothing about the
/// repositories the witness then builds. The probe pins no repository-local
/// `core.useReplaceRefs` for the same reason -- a local pin would outrank the
/// operator's `~/.gitconfig` and hide a control this exists to find.
pub(crate) fn replacement_liveness(tag: &str) -> ReplacementLiveness {
    let root = scratch(&format!("replacement-live-{tag}"));
    let repo = root.join("repo");
    create_dir(&repo);
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "tests@upstroke.local"]);
    git(&repo, &["config", "user.name", "upstroke tests"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "core.eol", "lf"]);

    write_file(&repo.join("probe.txt"), b"recorded\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "recorded"]);
    let recorded = git(&repo, &["rev-parse", "HEAD"]);

    write_file(&repo.join("probe.txt"), b"replacing\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "replacing"]);
    let replacing = git(&repo, &["rev-parse", "HEAD"]);
    assert_ne!(recorded, replacing, "two distinct commits");

    git(&repo, &["replace", &recorded, &replacing]);
    let found = git(
        &repo,
        &["for-each-ref", "--format=%(refname)", "refs/replace/"],
    );
    let verdict = if found == format!("refs/replace/{recorded}") {
        let read = git(&repo, &["show", &format!("{recorded}:probe.txt")]);
        if read == "replacing" {
            ReplacementLiveness::Live
        } else {
            ReplacementLiveness::NotHonoured { read }
        }
    } else {
        ReplacementLiveness::RefsElsewhere { found }
    };
    let _ = fs::remove_dir_all(&root);
    verdict
}

/// [`replacement_liveness`], as the precondition a witness states before it
/// trusts anything it measures.
pub(crate) fn assert_replacement_refs_are_live(tag: &str) {
    let controls = ambient_replacement_controls();
    match replacement_liveness(tag) {
        ReplacementLiveness::Live => {}
        ReplacementLiveness::RefsElsewhere { found } => panic!(
            "`{tag}`: `git replace` wrote `{found}` rather than under \
             `refs/replace/`, so this process is not speaking about the \
             namespace the design and the finding name. `GIT_REPLACE_REF_BASE` \
             moves it, and a probe that writes and reads through the same moved \
             base would answer yes to a question it was not asked. The \
             enumerated controls this process carries are {controls:?}"
        ),
        ReplacementLiveness::NotHonoured { read } => panic!(
            "`{tag}`: this process cannot witness replacement isolation, \
             because a Git child of it does not honour `refs/replace/*` in the \
             first place -- it read `{read}` where the replacing object says \
             `replacing`. The enumerated controls it carries are {controls:?}; \
             if none of them explains this, the enumeration in \
             `REPLACEMENT_CONTROLS_REMOVED` and `REPLACEMENT_CONTROLS_PINNED` \
             is missing a mechanism, and closing it is the fix -- never \
             weakening this check"
        ),
    }
}

/// The repository configuration a fixture that measures replacement
/// behaviour pins for itself, for the reason `Fixture::new` pins
/// `core.autocrlf` and `core.eol` (§12).
///
/// Repository-local configuration outranks the system and global files and
/// everything they include, so this is what survives an operator's
/// `core.useReplaceRefs = false` in `~/.gitconfig`. It does **not** outrank
/// `-c`, `GIT_CONFIG_COUNT` or `GIT_NO_REPLACE_OBJECTS` (measured, all
/// three), which is why it is the second half of the closure and never the
/// whole of it.
///
/// The write itself goes through [`without_ambient_replacement_controls`],
/// because `GIT_CONFIG` sends `git config`'s writes to the file it names: a
/// pin made under it would land in the operator's file, succeed, and leave
/// this repository saying nothing.
pub(crate) fn pin_replacement_refs_in(repo: &Path) {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .args(["config", "core.useReplaceRefs", "true"]);
    without_ambient_replacement_controls(&mut command);
    let out = command.output().expect("run git");
    assert!(
        out.status.success(),
        "pinning core.useReplaceRefs in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Write `include.path = <included>` into the Git configuration file `file`,
/// **using Git's own configuration writer**.
///
/// A path is not a configuration value, and spelling one with `format!` was a
/// measured defect (PR #271, round 3): on git 2.43.0 an unquoted path
/// containing `#` is truncated at the comment character, so the include
/// silently stops including anything, and one containing a backslash is read as
/// an escape -- `git config --list` over the result exits `128` with `bad
/// config line 2`. `git config --file` quotes the first and doubles the second,
/// and both then resolve. It creates the file if it is not there.
///
/// The write goes through [`without_ambient_replacement_controls`] for the
/// reason [`pin_replacement_refs_in`]'s does: `GIT_CONFIG` redirects `git
/// config`'s writes.
pub(crate) fn write_include_path(file: &Path, included: &Path) {
    let mut command = Command::new("git");
    command
        .args(["config", "--file"])
        .arg(file)
        .arg("include.path")
        .arg(included);
    without_ambient_replacement_controls(&mut command);
    let out = command.output().expect("run git");
    assert!(
        out.status.success(),
        "writing `include.path` into {}: {}",
        file.display(),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Where a replacement witness tells its helper to do the work.
///
/// Guarded by a variable because each helper is `#[ignore]`, and a run with
/// `--include-ignored` would otherwise execute its body in a process that still
/// carries whatever the machine exported -- which is the one environment a
/// replacement witness must never be measured in.
pub(crate) const REPLACEMENT_WITNESS: &str = "UPSTROKE_PR271_REPLACEMENT_WITNESS";

/// Run this test binary again at `test`, `--exact --ignored`, with **every**
/// ambient control over `refs/replace/*` taken away from the child, and return
/// its exit status.
///
/// [`run_kill_child`] sets variables; this one takes away the ones that would
/// make a witness pass for the wrong reason. `Command` inherits the parent's
/// environment, so a suite run under an exported `GIT_NO_REPLACE_OBJECTS=1` --
/// which is what this pull request makes upstroke's own gates supply -- would
/// hand the child the protection the child exists to prove the code installs.
/// Round 2 removed that one variable and the reviewer reached the same silent
/// pass through `core.useReplaceRefs=false` instead, so what is removed here is
/// the enumeration [`without_ambient_replacement_controls`] carries, and the
/// child measures what is left before it trusts it.
///
/// **Every replacement witness in the crate goes through here** (PR #271,
/// round 4), in `src/workspace_manager/tests.rs`, `src/gates.rs` and
/// `src/engine/tests.rs` alike. Rounds 1, 2 and 3 each shipped one witness that
/// could pass without its fix, and each repair reached only the witness a
/// reviewer had named; there is now one door and no witness beside it.
pub(crate) fn run_replacement_witness_child(test: &str) -> std::process::ExitStatus {
    let mut command = Command::new(std::env::current_exe().expect("this test binary"));
    command
        .args(["--exact", test, "--ignored", "--nocapture"])
        .env(REPLACEMENT_WITNESS, "1");
    without_ambient_replacement_controls(&mut command);
    command.status().expect("spawn the witness child")
}

/// A `git` child a test can kill at a chosen moment.
///
/// The residue sampler's child, and deliberately **blind to what it is
/// running**: no argv reaches [`Self::kill`], so a per-command count taken
/// over these cannot be defeated inside this type. It is the same shape
/// `mod tests`'s `SampledChild` uses, which stays there because the
/// four-command sampler stays there; this one exists because the
/// two-command sampler of `T-ATTEMPT` lives in a module that cannot name
/// [`Command`].
pub(crate) struct KillableGitChild {
    child: std::process::Child,
    /// The origin of every clock below, read before `Command::spawn` is
    /// called, so the child's run — the exec the parent never sees, the
    /// pick, the exit — begins after it and ends before any reading below
    /// that is taken once its exit was established; [`time_git`]'s probe is
    /// timed from the same place. Until 2026-09-10 it was read once the
    /// spawn had returned (the ultra review of `d1fef26d`, finding 1): a
    /// parent paused 20 ms between the two let a 1.02 ms pick complete
    /// before the origin existed, the poll found it gone at 4.5 µs, the
    /// samplers fed that back, and every later rung was aimed at 22–178 µs,
    /// under the pick's first write.
    origin: std::time::Instant,
    /// The clock once `Command::spawn` had returned: the spawn's own
    /// latency, which every clock below includes, and during which the
    /// child may already be running, or done. The two samplers of this
    /// fixture aim from the origin and subtract nothing; the `T-ATTEMPT`
    /// sampler sets its rungs as delays after the spawn's return and reads
    /// its clocks from there ([`Self::spawned`]).
    spawned: std::time::Duration,
    /// The clock once a kill attempt at this child had returned, or `None`
    /// if none was ever made. Written only by [`Self::kill`], read after
    /// [`std::process::Child::kill`] returned, whatever it returned. It
    /// orders the attempt against the child's clock and nothing more: it
    /// does not say the signal was sent ([`Self::kill_error`] does), and it
    /// does not say the child had exited ([`Self::reaped`] does).
    fired: Option<std::time::Duration>,
    /// What [`std::process::Child::kill`] returned when it did not return
    /// `Ok`: nothing was sent, and the child ran on.
    kill_error: Option<std::io::Error>,
    /// The clock once [`Self::wait`] had returned the child's status, or
    /// `None` until it has.
    reaped: Option<std::time::Duration>,
}

/// The `git` a sampled child runs, so that a kill of the child is a kill of
/// git.
///
/// On Unix that is `git` on the `PATH`. On Windows `git` on the `PATH` is Git
/// for Windows' `cmd\git.exe`, a 46 KB launcher that starts the real
/// `mingw64\bin\git.exe` as its own child and waits for it, so a
/// `Child::kill` there ends the launcher and the pick runs on to completion:
/// the winguest lane at `56ea88c9` recorded 30 kills, ten of which found no
/// write and twenty a finished pick, none an interruption
/// (`PR249-KILL-SAMPLER-WINDOWS-WRAPPER`). So the sampled child is the real
/// binary, found through `git --exec-path` (`<prefix>/mingw64/libexec/git-core`)
/// as `<prefix>/mingw64/bin/git.exe`, whose DLLs sit beside it. Resolved
/// once per process; `git` when that layout is absent.
pub(crate) fn sampled_git() -> &'static Path {
    static RESOLVED: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    RESOLVED.get_or_init(|| {
        let fallback = PathBuf::from("git");
        if !cfg!(windows) {
            return fallback;
        }
        let Ok(output) = Command::new("git")
            .arg("--exec-path")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
        else {
            return fallback;
        };
        if !output.status.success() {
            return fallback;
        }
        let exec_path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        match exec_path
            .parent()
            .and_then(Path::parent)
            .map(|prefix| prefix.join("bin").join("git.exe"))
        {
            Some(real) if real.is_file() => real,
            _ => fallback,
        }
    })
}

impl KillableGitChild {
    /// Spawn `git -C cwd <args>` with its streams discarded; [`sampled_git`]
    /// says which `git`.
    pub(crate) fn spawn(cwd: &Path, args: &[String]) -> Self {
        let origin = std::time::Instant::now();
        let child = Command::new(sampled_git())
            .arg("-C")
            .arg(cwd)
            .args(["-c", "core.fsmonitor=false"])
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the sampled git child");
        let spawned = origin.elapsed();
        Self {
            child,
            origin,
            spawned,
            fired: None,
            kill_error: None,
            reaped: None,
        }
    }

    /// Send the kill, recording the clock once the attempt has returned and
    /// what it returned.
    ///
    /// Neither says the child has exited. `Child::kill` returns `Ok` once
    /// the signal is sent, and also for a child that has already exited
    /// (std documents that; on Windows `TerminateProcess` answers
    /// `ERROR_ACCESS_DENIED` for a process that has ended or is ending, and
    /// std reports that as `Ok`), and it returns `Err` when nothing was
    /// sent at all and the child runs on. So the clock recorded here
    /// orders the attempt and bounds nothing: a child that was still
    /// running at a failed attempt, or that exited between the poll that
    /// found it running and the system call, ends with a completion's
    /// status, and what the samplers feed back for it is [`Self::reaped`],
    /// read once [`Self::wait`] has returned that status.
    /// Until 2026-09-10 the samplers fed such a child back as this clock:
    /// first as the clock read *before* the call (the ultra review of
    /// `2d3fa9d1`, finding 1: a 20 ms pause between the read and the call
    /// fed a completed 1.09 ms pick back as 121 µs, below the pick and
    /// clamped to [`KillBudget::FLOOR`], and seven kills of children that
    /// had not begun were the evidence of a batch), then as the clock read
    /// after it (the ultra review of `8441c5fe`, finding 1: an attempt made
    /// to fail on the first kill fed a child still running at 117 µs back
    /// as 117 µs, and the batch collapsed the same way).
    pub(crate) fn kill(&mut self) {
        let outcome = self.child.kill();
        self.fired = Some(self.origin.elapsed());
        self.kill_error = outcome.err();
    }

    /// Whether the child has exited on its own, and when the parent saw it.
    ///
    /// The duration is the clock at the poll that found the child gone, from
    /// the origin read before its spawn: the parent's observation of the
    /// exit, not the child's own time, which no platform hands a parent —
    /// `wait4` carries a child's CPU times and no wall-clock exit, and
    /// Windows' `GetProcessTimes`, which does, is not bound here. The poll
    /// is the next one when the parent holds a core — at once while
    /// [`Self::run_until`] spins, after a sleep of a millisecond before then
    /// — and the one after the scheduler's wake-up when it does not, and
    /// [`KillBudget`] follows the number as the poll read it.
    ///
    /// `None` while it is still running.
    pub(crate) fn exited(&mut self) -> Option<std::time::Duration> {
        match self.child.try_wait() {
            Ok(Some(_)) => Some(self.origin.elapsed()),
            _ => None,
        }
    }

    /// Reap it, recording the clock once its status is in. The wait status
    /// is the only thing a kill changes.
    pub(crate) fn wait(&mut self) -> std::process::ExitStatus {
        let status = self.child.wait().expect("reap the sampled git child");
        self.reaped = Some(self.origin.elapsed());
        status
    }

    /// Leave the child running until `aim` after its spawn or until it
    /// exits on its own, and say which: `Some` with how long it ran, as the
    /// parent saw it, if it exited first; `None` if it was still running at
    /// the aim — and then **the kill has fired**, sent by the poll that found
    /// it running there, and [`Self::fired`] says when.
    ///
    /// Polled, never slept through. `sleep(aim)` then `kill` reports nothing
    /// about the child and wakes when the scheduler pleases, so on a loaded
    /// host the kill is late by the wake-up and the sampler cannot tell a
    /// child it missed from one it never aimed inside. Here [`Self::exited`]
    /// is asked once a millisecond while the aim is far and continuously
    /// once it is within [`Self::SPIN_WITHIN`], and the poll that reaches
    /// the aim with the child still running sends the kill itself, with no
    /// return to the caller between the two. What remains is the
    /// platform's: between that poll's `try_wait` and the kill's own system
    /// call the parent can be descheduled, and a child that exits in that
    /// window is missed by the kill and ends on its own terms — its status
    /// is a completion, not the kill's signature, and the samplers feed it
    /// back like any completion, as [`Self::reaped`], the clock once
    /// [`Self::wait`] has returned that status.
    pub(crate) fn run_until(&mut self, aim: std::time::Duration) -> Option<std::time::Duration> {
        let deadline = self.origin + aim;
        loop {
            if let Some(ran) = self.exited() {
                return Some(ran);
            }
            let now = std::time::Instant::now();
            if now >= deadline {
                self.kill();
                return None;
            }
            if deadline - now > Self::SPIN_WITHIN {
                std::thread::sleep(std::time::Duration::from_millis(1));
            } else {
                std::thread::yield_now();
            }
        }
    }

    /// How close to its aim [`Self::run_until`] stops sleeping and polls
    /// continuously: wider than a loaded host's wake-up from a
    /// one-millisecond sleep.
    const SPIN_WITHIN: std::time::Duration = std::time::Duration::from_millis(4);

    /// The clock once `Command::spawn` had returned, from the origin read
    /// before it: the spawn's own latency, which [`Self::fired`],
    /// [`Self::exited`] and [`Self::reaped`] all include. The two samplers
    /// of this fixture aim from the origin, feed back from it and subtract
    /// nothing. The `T-ATTEMPT` sampler (`engine::topology::attempt::tests`)
    /// sets each rung as a delay after the spawn's return and asserts that
    /// its kill fired no sooner than the rung, so it subtracts this from
    /// `fired`, and from the clock `exited` read, to bring both to the
    /// reference its rungs and its retry budget have. Read from the origin,
    /// its `fired` counted the spawn's latency toward the rung: the ultra
    /// review of `ec87d6ed` (finding 1) paused the spawn one second after
    /// the origin and removed that sampler's deadline loop, and every kill,
    /// fired the instant the spawn returned, read 1.0002 s against rungs of
    /// 0.66–7.5 ms and passed.
    pub(crate) fn spawned(&self) -> std::time::Duration {
        self.spawned
    }

    /// The clock once a kill attempt at this child had returned, if one was
    /// ever made — whatever it returned. It orders the attempt against the
    /// child's clock and says nothing else: not that the signal was sent
    /// ([`Self::kill_error`]), not that the child had exited
    /// ([`Self::reaped`]).
    pub(crate) fn fired(&self) -> Option<std::time::Duration> {
        self.fired
    }

    /// What the kill attempt returned when it was not `Ok`: nothing was
    /// sent, and the child ran on to an end of its own.
    pub(crate) fn kill_error(&self) -> Option<&std::io::Error> {
        self.kill_error.as_ref()
    }

    /// The clock once [`Self::wait`] had returned the child's status, if it
    /// has, from the origin read before its spawn — whichever way the child
    /// ended: killed, missed by the kill, or never reached by a failed
    /// attempt. For a child that ended with a completion's status this is
    /// what the samplers feed back.
    pub(crate) fn reaped(&self) -> Option<std::time::Duration> {
        self.reaped
    }
}

/// Whether `status` is the death `std::process::abort()` produces.
///
/// **Not `!status.success()`.** A kill child that reaches its own
/// `unreachable!` panics, and a panic also fails to succeed — so a parent
/// that accepted any unsuccessful exit would read "the injection stopped
/// killing" as "the injection killed", and would then go on to inspect a
/// directory the panicking child's `Drop` had already deleted. Measured:
/// exactly that, on a kill armed at a site the child never reached.
///
/// Unix has a value for it — `SIGABRT`, which no Rust panic raises. Windows
/// does not expose one portably (`abort()` reaches `__fastfail`, whose code
/// has moved between CRT versions), so there the oracle is the *negation*
/// of the panic's own exit code, which `std::process::abort` cannot produce
/// and `panic!` always does.
pub(crate) fn died_by_abort(status: &std::process::ExitStatus) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::process::ExitStatusExt::signal(status) == Some(libc::SIGABRT)
    }
    #[cfg(windows)]
    {
        /// What a Rust process exits with when a panic unwinds out of main.
        const PANIC: i32 = 101;
        !status.success() && status.code() != Some(PANIC)
    }
}

/// Whether `status` carries this platform's signature of a
/// [`std::process::Child::kill`].
///
/// A **value** per platform, not `!status.success()`: a command that merely
/// failed also fails to succeed, and reading that as a kill is how a
/// kill-count keeps counting after the kill is gone.
pub(crate) fn died_by_kill(status: &std::process::ExitStatus) -> bool {
    // `Child::kill` sends `SIGKILL`, and no exit a child reaches on its own
    // carries a signal at all.
    #[cfg(unix)]
    {
        std::os::unix::process::ExitStatusExt::signal(status) == Some(libc::SIGKILL)
    }
    // `Child::kill` is `TerminateProcess(handle, 1)`; the sampler's probe
    // asserts the same command exits 0 when nothing kills it, so 1 is not
    // an end these commands reach by themselves.
    #[cfg(windows)]
    {
        status.code() == Some(1)
    }
}

/// Time one uninterrupted run of `git -C cwd <args>`.
///
/// The kill ladder is fractions of this duration, which is the only
/// variance a replay can pin — see `mod tests`'s `measure_budget` for the
/// argument, and for why the measurement runs in a **probe slot of its
/// own** rather than in the worktree the samples will kill in.
pub(crate) fn time_git(cwd: &Path, args: &[String]) -> std::time::Duration {
    let start = std::time::Instant::now();
    let output = git_out(cwd, &args.iter().map(String::as_str).collect::<Vec<_>>());
    let elapsed = start.elapsed();
    assert!(
        output.status.success(),
        "the probe must really run: git {args:?} in {}: {}",
        cwd.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    elapsed
}

/// A kill sampler's budget: how long an uninterrupted run of the sampled
/// command takes on this machine, now.
///
/// A sampler aims its kills at fixed fractions of one duration, and every
/// sampler in this tree that took that duration from a single measured run
/// has been red on a hosted macOS runner with every kill landing after its
/// child had already finished (`PR7-SAMPLER-SCHEDULES-FROM-A-COLD-PROBE`,
/// `PR80-MACOS-WORKSPACE-SAMPLER-COLD-PROBE-RECURRENCE`,
/// `RECOVER-CHERRY-PICK-SAMPLER-COLD-PROBE`,
/// `G4B-O10-REPAIR-MATERIALIZE-SAMPLER-MACOS-KILL-FLOOR`). So the budget is
/// never one number. It starts as the median of a probe's runs after a
/// discarded warm-up, and it follows the sampled children themselves: a
/// child that finished before its kill has measured the command under the
/// sampler's own conditions, at that moment, and the next kill is aimed
/// inside what it took. An inflated probe is corrected by the first child
/// that outruns it; a host that drifts is tracked rung by rung.
///
/// It follows in both directions and is not capped at the probe: a host
/// that slows after the probe needs rungs past it to reach the pick's
/// writes, and a completion the parent saw late — woken after the exit, or
/// a child the kill did not stop, fed back as the clock once its `wait` had
/// returned — is followed as it was read, never as a ceiling. What the
/// ladder follows is the median of the last [`KillBudget::RECENT`] such
/// completions, over however many exist: the first sets the budget alone,
/// of two the longer is taken, and from three on one late observation
/// among three is outvoted by the other two while two are not — so a late
/// first observation holds until two shorter completions follow it, and
/// three late ones hold until two do ([`KillBudget::current`]).
pub(crate) struct KillBudget {
    probe: std::time::Duration,
    /// The clock at which the parent established the exit of each child
    /// that ended with a completion's status, from the origin read before
    /// its spawn, oldest first.
    completed: Vec<std::time::Duration>,
}

impl KillBudget {
    /// No budget below this: a measurement that small is the clock's, not
    /// the command's.
    pub(crate) const FLOOR: std::time::Duration = std::time::Duration::from_micros(200);

    /// How many of the most recent completions the budget follows: three,
    /// so that once three exist one late observation among them is outvoted
    /// by the other two, and so that the first completion replaces the probe
    /// at once. Until three exist nothing is outvoted: one completion sets
    /// the ladder alone, and of two the longer is taken.
    pub(crate) const RECENT: usize = 3;

    /// From a probe's uninterrupted runs, in order. The first is the
    /// warm-up and is discarded — the first run in a fresh worktree pays
    /// for cold caches and, on Windows, an antivirus pass — and the budget
    /// is the median of the rest, because the failure mode of one
    /// measurement is one outlier, which a median discards and a mean
    /// keeps.
    pub(crate) fn probed(runs: &[std::time::Duration]) -> Self {
        assert!(
            runs.len() > 1,
            "a budget needs a warm-up run and at least one after it: {runs:?}"
        );
        let after_warm_up = runs.get(1..).unwrap_or_default();
        Self {
            probe: median(after_warm_up)
                .unwrap_or(Self::FLOOR)
                .max(Self::FLOOR),
            completed: Vec::new(),
        }
    }

    /// What the probe measured, for the record a red run carries.
    pub(crate) fn probe(&self) -> std::time::Duration {
        self.probe
    }

    /// The budget now: the median of the last [`Self::RECENT`] children
    /// that finished before their kill, over however many have — one alone,
    /// the longer of two, the middle of three — or the probe's while none
    /// has. [`median`] takes the upper of an even count, so a late first
    /// completion is not displaced by one shorter one; two are needed.
    pub(crate) fn current(&self) -> std::time::Duration {
        let recent: Vec<std::time::Duration> = self
            .completed
            .iter()
            .rev()
            .take(Self::RECENT)
            .copied()
            .collect();
        median(&recent).unwrap_or(self.probe).max(Self::FLOOR)
    }

    /// Where the kill of rung `rung` (from zero) of `rungs` is aimed: the
    /// ladder's fraction `(rung + 1) / (rungs + 1)` of the current budget,
    /// so the rungs stay spread through the command and never reach its
    /// measured end.
    pub(crate) fn aim(&self, rung: u32, rungs: u32) -> std::time::Duration {
        self.current()
            .mul_f64(f64::from(rung + 1) / f64::from(rungs + 1))
    }

    /// A child ended with a completion's status: `clock` is the clock at
    /// which the parent established its exit, from the origin read before
    /// its spawn — the poll that found it gone, or, when the kill attempt
    /// did not stop it, the return of the `wait` that reaped it. The two
    /// differ in what the parent did, not in where the exit fell against
    /// the aim, which the parent cannot see: on the first, no poll reached
    /// the aim with the child running and no kill was sent — a parent
    /// descheduled across the aim and the exit alike finds the child gone
    /// at its next poll, whichever of the two came first; on the second, a
    /// poll reached the aim with the child still running, and the kill sent
    /// from there missed it, or was not sent at all
    /// ([`KillableGitChild::kill_error`]). The budget follows what the child
    /// took either way, as the parent read it.
    pub(crate) fn completed(&mut self, clock: std::time::Duration) {
        self.completed.push(clock);
    }

    /// How many children have moved the budget.
    pub(crate) fn completions(&self) -> usize {
        self.completed.len()
    }
}

/// The median of `durations`; `None` of none.
pub(crate) fn median(durations: &[std::time::Duration]) -> Option<std::time::Duration> {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied()
}
