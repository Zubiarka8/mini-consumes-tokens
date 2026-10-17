//! Timing contract of the background watcher (issue #196): an automatic
//! index update runs only after a *quiet period* with no file changes, a new
//! change inside that period restarts it, a burst of saves is one update, and
//! ignored files never cause one.
//!
//! `background::spawn_watcher` takes a concrete `Index` (not a `Reindexer`),
//! so updates are observed two ways: through the index contents (polled with
//! deadlines) and through the watcher's own `tracing` event
//! ("auto-reindex after file changes settled"), captured by a process-wide
//! subscriber that records one timestamp per completed update. The tests
//! share that recorder, so they run one at a time behind `SERIAL`.
//!
//! Real OS watcher backends add latency on top of the debounce, so every
//! "must not have happened yet" check leaves a wide margin before the
//! deadline it protects, and every "must have happened" check polls up to
//! `SETTLE_MARGIN`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior (same policy as tests/background_watcher.rs).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::background;
use tokio::sync::Mutex;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{EnvFilter, Layer};

/// The quiet period under test: long enough that the margins below dwarf
/// scheduling noise, short enough to keep the file fast.
const QUIET: Duration = Duration::from_secs(2);
/// Upper bound for "this must eventually happen" polls (CI watcher latency).
const SETTLE_MARGIN: Duration = Duration::from_secs(20);
/// Slack for clock reads taken on either side of an OS event.
const TOLERANCE: Duration = Duration::from_millis(100);

const UPDATE_MSG: &str = "auto-reindex after file changes settled";
const FAILED_MSG: &str = "auto-reindex failed";

// ---------------------------------------------------------------------------
// Update recorder: a global tracing layer (the watcher logs from its own
// `std::thread`, so a thread-local default would not see it).
// ---------------------------------------------------------------------------

static UPDATES: StdMutex<Vec<Instant>> = StdMutex::new(Vec::new());
static FAILURES: StdMutex<usize> = StdMutex::new(0);
static SERIAL: Mutex<()> = Mutex::const_new(());

struct Recorder;

struct MessageOf(String);

impl Visit for MessageOf {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

impl<S: Subscriber> Layer<S> for Recorder {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let mut message = MessageOf(String::new());
        event.record(&mut message);
        if message.0.contains(UPDATE_MSG) {
            UPDATES.lock().unwrap().push(Instant::now());
        } else if message.0.contains(FAILED_MSG) {
            *FAILURES.lock().unwrap() += 1;
        }
    }
}

fn install_recorder() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let subscriber = tracing_subscriber::registry()
            .with(Recorder)
            .with(EnvFilter::new("mct_mcp_server=debug"));
        tracing::subscriber::set_global_default(subscriber).unwrap();
    });
}

/// Takes the file-wide lock, resets the recorder and returns the guard.
async fn begin() -> tokio::sync::MutexGuard<'static, ()> {
    let guard = SERIAL.lock().await;
    install_recorder();
    UPDATES.lock().unwrap().clear();
    *FAILURES.lock().unwrap() = 0;
    guard
}

fn updates() -> Vec<Instant> {
    UPDATES.lock().unwrap().clone()
}

fn failures() -> usize {
    *FAILURES.lock().unwrap()
}

// ---------------------------------------------------------------------------
// Helpers copied from tests/background_watcher.rs.
// ---------------------------------------------------------------------------

/// Canonicalized because on macOS the temp dir is a symlink and FSEvents
/// reports resolved paths (see background_watcher.rs).
fn tempdir() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;
    let id = nanos.wrapping_add(COUNTER.fetch_add(1, Ordering::Relaxed));
    let dir = std::env::temp_dir().join(format!("mct-reindex-delay-{id}"));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

/// Polls `condition` every 50 ms until true or `SETTLE_MARGIN` elapses.
async fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + SETTLE_MARGIN;
    loop {
        if condition() {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

type Watcher = notify_debouncer_full::Debouncer<
    notify::RecommendedWatcher,
    notify_debouncer_full::RecommendedCache,
>;

struct Fixture {
    dir: PathBuf,
    index: Arc<Mutex<Index>>,
    _watcher: Watcher,
}

/// Indexes `dir` as it is, then starts the watcher with the `QUIET` period.
fn start(dir: &Path, exclude: ExcludeSet) -> Fixture {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(dir, exclude.clone()).unwrap();
    index.reindex(&registry, false).unwrap();
    let index = Arc::new(Mutex::new(index));
    let watcher = background::spawn_watcher(
        Arc::clone(&index),
        registry,
        dir.to_path_buf(),
        exclude,
        QUIET,
    )
    .unwrap();
    Fixture {
        dir: dir.to_path_buf(),
        index,
        _watcher: watcher,
    }
}

impl Fixture {
    /// `Some(true/false)` if `name` is indexed; `None` while an update holds
    /// the index lock.
    fn has(&self, name: &str) -> Option<bool> {
        self.index
            .try_lock()
            .ok()
            .map(|guard| !guard.find_symbol(name).unwrap().is_empty())
    }

    async fn indexed(&self, name: &str) -> bool {
        wait_until(|| self.has(name) == Some(true)).await
    }

    fn save(&self, file: &str, body: &str) {
        let path = self.dir.join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
    }
}

/// The instant of the first completed update that is not before `since`.
fn first_update_after(since: Instant) -> Option<Instant> {
    updates().into_iter().find(|at| *at >= since)
}

// ---------------------------------------------------------------------------
// a) nothing happens before the quiet period elapses.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn no_update_runs_before_the_quiet_period_elapses() {
    let _serial = begin().await;
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\n").unwrap();
    let fx = start(&dir, ExcludeSet::default());

    let saved_at = Instant::now();
    fx.save("lib.rs", "pub fn one() {}\npub fn two() {}\n");

    // Halfway through the quiet period: still the old index, no update.
    tokio::time::sleep(QUIET / 2).await;
    assert_eq!(
        fx.has("two"),
        Some(false),
        "index changed before the quiet period elapsed"
    );
    assert!(
        updates().is_empty(),
        "an update ran before the quiet period"
    );

    // Then it does update, and not earlier than the quiet period after the save.
    assert!(fx.indexed("two").await, "the save was never indexed");
    let done = first_update_after(saved_at).expect("update event not recorded");
    assert!(
        done.duration_since(saved_at) + TOLERANCE >= QUIET,
        "update finished {:?} after the save, before the {:?} quiet period",
        done.duration_since(saved_at),
        QUIET
    );
    assert_eq!(failures(), 0);
}

// ---------------------------------------------------------------------------
// b) a second save inside the period restarts it.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_second_save_inside_the_period_resets_it() {
    let _serial = begin().await;
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\n").unwrap();
    let fx = start(&dir, ExcludeSet::default());

    let first_at = Instant::now();
    fx.save("lib.rs", "pub fn one() {}\npub fn first_save() {}\n");
    tokio::time::sleep(QUIET / 2).await;
    let second_at = Instant::now();
    fx.save(
        "lib.rs",
        "pub fn one() {}\npub fn first_save() {}\npub fn second_save() {}\n",
    );

    // Past the ORIGINAL deadline (first_at + QUIET) but well before the reset
    // one (second_at + QUIET): without a reset the index would be fresh now.
    let original_deadline = first_at + QUIET;
    let probe = original_deadline + QUIET / 4;
    tokio::time::sleep_until(tokio::time::Instant::from_std(probe)).await;
    assert_eq!(
        fx.has("second_save"),
        Some(false),
        "the second save did not reset the quiet period"
    );
    assert_eq!(fx.has("first_save"), Some(false));
    assert!(
        updates().is_empty(),
        "an update ran at the original deadline"
    );

    assert!(fx.indexed("second_save").await, "second save never indexed");
    let done = first_update_after(first_at).expect("update event not recorded");
    assert!(
        done.duration_since(second_at) + TOLERANCE >= QUIET,
        "update ran {:?} after the second save, before the reset period {:?}",
        done.duration_since(second_at),
        QUIET
    );
    assert_eq!(failures(), 0);
}

// ---------------------------------------------------------------------------
// c) a burst of saves is a single update.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_burst_of_saves_yields_a_single_update() {
    const SAVES: usize = 8;
    let _serial = begin().await;
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\n").unwrap();
    let fx = start(&dir, ExcludeSet::default());

    // 8 saves, 150 ms apart (~1.2 s total, inside one quiet period), spread
    // over a rewritten file and several new files.
    let mut body = String::from("pub fn one() {}\n");
    for n in 0..SAVES {
        body.push_str(&format!("pub fn burst_{n}() {{}}\n"));
        fx.save("lib.rs", &body);
        fx.save(
            &format!("extra_{n}.rs"),
            &format!("pub fn extra_{n}() {{}}\n"),
        );
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    let last = format!("burst_{}", SAVES - 1);
    let last_extra = format!("extra_{}", SAVES - 1);
    assert!(fx.indexed(&last).await, "burst never indexed");
    assert!(fx.indexed(&last_extra).await, "burst files never indexed");
    // Let any (wrongly) separate follow-up update surface.
    tokio::time::sleep(QUIET * 2).await;

    for n in 0..SAVES {
        assert_eq!(fx.has(&format!("burst_{n}")), Some(true), "burst_{n} lost");
        assert_eq!(fx.has(&format!("extra_{n}")), Some(true), "extra_{n} lost");
    }
    assert_eq!(
        updates().len(),
        1,
        "a burst of {SAVES} saves must produce exactly one update"
    );
    assert_eq!(failures(), 0);
}

// ---------------------------------------------------------------------------
// d) changes arriving while an update is running are applied afterwards.
// ---------------------------------------------------------------------------

/// An update needs the index lock, so holding it from the test stands in for
/// an update in progress (the real one is too fast to race reliably): the
/// watcher must neither run alongside it nor drop what arrives meanwhile.
#[tokio::test]
async fn changes_during_a_running_update_are_applied_afterwards() {
    let _serial = begin().await;
    let dir = tempdir();
    fs::write(dir.join("a.rs"), "pub fn a_old() {}\n").unwrap();
    fs::write(dir.join("b.rs"), "pub fn b_old() {}\n").unwrap();
    let fx = start(&dir, ExcludeSet::default());

    let busy = fx.index.lock().await; // "an update is running"

    // First change settles while the lock is held: the watcher blocks on it.
    fx.save("a.rs", "pub fn a_new() {}\n");
    tokio::time::sleep(QUIET + QUIET / 2).await;
    // A second change arrives while that first update is still waiting/running.
    fx.save("b.rs", "pub fn b_new() {}\n");
    fx.save("c.rs", "pub fn c_new() {}\n");
    tokio::time::sleep(QUIET + QUIET / 2).await;

    assert!(
        updates().is_empty(),
        "an update completed while another held the index"
    );
    assert!(
        busy.find_symbol("a_new").unwrap().is_empty(),
        "index changed while locked"
    );
    drop(busy);

    for name in ["a_new", "b_new", "c_new"] {
        assert!(fx.indexed(name).await, "{name} was lost");
    }
    // Let the queue drain, then check nothing stale survived and no update failed.
    tokio::time::sleep(QUIET * 2).await;
    for gone in ["a_old", "b_old"] {
        assert_eq!(fx.has(gone), Some(false), "{gone} still indexed");
    }
    assert!(!updates().is_empty());
    assert_eq!(failures(), 0);
}

// ---------------------------------------------------------------------------
// e) ignored files never trigger an update.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn files_excluded_by_ignore_rules_never_trigger_an_update() {
    let _serial = begin().await;
    let dir = tempdir();
    fs::write(dir.join("lib.rs"), "pub fn one() {}\n").unwrap();
    fs::write(dir.join(".mctignore"), "@import-gitignore\nsecret.rs\n").unwrap();
    fs::write(dir.join(".gitignore"), "generated/\n").unwrap();
    let exclude = ExcludeSet::for_project(&dir);
    let fx = start(&dir, exclude);

    // One lone event is not enough: write several ignored files, one by one,
    // across more than a full quiet period.
    fx.save("secret.rs", "pub fn secret_one() {}\n");
    fx.save("generated/out.rs", "pub fn generated_one() {}\n");
    tokio::time::sleep(QUIET).await;
    fx.save("secret.rs", "pub fn secret_two() {}\n");
    fx.save("generated/deep/more.rs", "pub fn generated_two() {}\n");
    tokio::time::sleep(QUIET * 2).await;

    assert!(updates().is_empty(), "an ignored file triggered an update");
    for name in ["secret_one", "secret_two", "generated_one", "generated_two"] {
        assert_eq!(fx.has(name), Some(false), "{name} was indexed");
    }

    // Positive control: the watcher is alive, and a normal file does update.
    fx.save("visible.rs", "pub fn visible() {}\n");
    assert!(fx.indexed("visible").await, "control file never indexed");
    assert_eq!(
        updates().len(),
        1,
        "control save must be exactly one update"
    );
    assert_eq!(fx.has("secret_two"), Some(false));
    assert_eq!(failures(), 0);
}
