//! One background maintainer per index (ADR-028), and an opt-in idle sleep
//! (ADR-029).
//!
//! Every process that serves an index (each MCP session, the dashboard) used
//! to run its own watcher and periodic full reconciliation of the same
//! project. Now only the holder of the index's maintainer lease
//! ([`Index::try_acquire_maintainer`]) does. The others serve queries (their
//! caches follow the owner's publications by generation), keep the explicit
//! `reindex` tool, and retry the lease at a bounded interval, so one of them
//! takes over when the owner exits.
//!
//! With [`MaintainerSettings::idle`] set, a process that has answered no
//! index-using request for that long goes to sleep. It stops its watcher
//! without the final reconciliation, releases the lease so an active process
//! can maintain instead, and stops retrying. The next index-using request
//! ([`Activity::wake`]) waits while it takes the lease if it is free,
//! restarts the watcher and reconciles once. Only then is the request
//! answered, so a change made while asleep is never served stale.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, PoisonError};
use std::thread::{JoinHandle, Thread};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, MaintainerLease};
use tokio::sync::Mutex;

use crate::background::{self, WatcherHandle};

/// How long a follower waits between lease attempts, before jitter (up to a
/// third more, so followers started together do not retry together).
pub const DEFAULT_LEASE_RETRY: Duration = Duration::from_secs(30);

/// What the maintainer runs once it holds the lease.
#[derive(Clone)]
pub struct MaintainerSettings {
    pub root: PathBuf,
    pub exclude: ExcludeSet,
    pub debounce: Duration,
    pub reconciliation_interval: Duration,
    pub lease_retry: Duration,
    /// Sleep after this long without an index-using request; `None` (the
    /// default) never sleeps.
    pub idle: Option<Duration>,
}

/// The background maintenance thread of this process. Dropping it stops the
/// thread: an owner releases the lease (and its watcher), a follower stops
/// retrying.
pub struct Maintainer {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

struct Shared {
    stop: AtomicBool,
    owner: AtomicBool,
    activity: Activity,
}

impl Maintainer {
    /// Whether this process currently holds the lease and maintains the index.
    pub fn is_owner(&self) -> bool {
        self.shared.owner.load(Ordering::SeqCst)
    }

    /// The handle requests use to mark activity and wake maintenance.
    pub fn activity(&self) -> Activity {
        self.shared.activity.clone()
    }
}

impl Drop for Maintainer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            self.shared.activity.notify();
            thread.thread().unpark();
            let _ = thread.join();
        }
        // Nothing will refresh any more: never leave a request waiting.
        self.shared.activity.set_phase(Phase::Active, None);
    }
}

/// Request-side view of the maintainer: when this process was last used, and
/// whether it sleeps. Cheap to clone; a request takes one uncontended lock.
#[derive(Clone)]
pub struct Activity {
    inner: Arc<(std::sync::Mutex<ActivityState>, Condvar)>,
}

struct ActivityState {
    last_use: Instant,
    phase: Phase,
    /// The maintenance thread, unparked by a wake.
    thread: Option<Thread>,
    /// Why the last wake could not refresh the index, if it failed.
    refresh_error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Active,
    Asleep,
    /// A request asked to wake; requests wait until the refresh is done.
    Waking,
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

impl Activity {
    /// A fresh, active state; hand a clone to [`spawn`] and to the server.
    pub fn new() -> Self {
        Self {
            inner: Arc::new((
                std::sync::Mutex::new(ActivityState {
                    last_use: Instant::now(),
                    phase: Phase::Active,
                    thread: None,
                    refresh_error: None,
                }),
                Condvar::new(),
            )),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, ActivityState> {
        self.inner.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn notify(&self) {
        self.inner.1.notify_all();
    }

    /// Whether maintenance is asleep right now.
    pub fn is_asleep(&self) -> bool {
        self.state().phase != Phase::Active
    }

    /// Marks an index-using request. If maintenance sleeps, wakes it and
    /// waits until it has reconciled the index once; concurrent requests wait
    /// on the same refresh. `Err` carries why that refresh failed: the index
    /// is then answered as it stands, possibly stale.
    pub fn wake_blocking(&self) -> Result<(), String> {
        let mut state = self.state();
        state.last_use = Instant::now();
        if state.phase == Phase::Active {
            return Ok(());
        }
        if state.phase == Phase::Asleep {
            state.phase = Phase::Waking;
            if let Some(thread) = &state.thread {
                thread.unpark();
            }
        }
        let state = self
            .inner
            .1
            .wait_while(state, |s| s.phase == Phase::Waking)
            .unwrap_or_else(PoisonError::into_inner);
        state.refresh_error.clone().map_or(Ok(()), Err)
    }

    /// [`Activity::wake_blocking`] for async callers: only a sleeping
    /// maintainer costs a blocking-pool thread.
    pub async fn wake(&self) -> Result<(), String> {
        if !self.is_asleep() {
            return self.wake_blocking();
        }
        let activity = self.clone();
        tokio::task::spawn_blocking(move || activity.wake_blocking())
            .await
            .unwrap_or_else(|err| Err(err.to_string()))
    }

    /// Time since the last index-using request, or since start-up if none came.
    pub fn idle_for(&self) -> Duration {
        self.state().last_use.elapsed()
    }

    /// When the idle timeout falls due, counted from the last request.
    fn idle_deadline(&self, idle: Duration) -> Instant {
        self.state().last_use + idle
    }

    fn set_phase(&self, phase: Phase, refresh_error: Option<String>) {
        let mut state = self.state();
        state.phase = phase;
        state.refresh_error = refresh_error;
        drop(state);
        self.notify();
    }
}

/// Starts this process's maintenance thread: it takes the lease when it can
/// and then watches and reconciles `settings.root`, or follows until the
/// lease is free. A lease that cannot be coordinated at all (the lock file
/// cannot be opened) falls back to maintaining anyway, as before ADR-028.
pub fn spawn(
    index: Arc<Mutex<Index>>,
    registry: LanguageRegistry,
    settings: MaintainerSettings,
    activity: Activity,
) -> Maintainer {
    let shared = Arc::new(Shared {
        stop: AtomicBool::new(false),
        owner: AtomicBool::new(false),
        activity,
    });
    let thread = std::thread::Builder::new()
        .name("mct-maintainer".into())
        .spawn({
            let shared = Arc::clone(&shared);
            move || {
                shared.activity.state().thread = Some(std::thread::current());
                Run {
                    index: &index,
                    registry: &registry,
                    settings: &settings,
                    shared: &shared,
                }
                .run();
            }
        })
        .map_err(|err| tracing::warn!(error = %err, "cannot start background maintenance"))
        .ok();
    Maintainer { shared, thread }
}

struct Run<'a> {
    index: &'a Arc<Mutex<Index>>,
    registry: &'a LanguageRegistry,
    settings: &'a MaintainerSettings,
    shared: &'a Shared,
}

/// What ended a stretch of maintenance.
enum Next {
    Stop,
    Sleep,
}

/// What the maintainer holds while it owns the index.
struct Owned {
    watcher: WatcherHandle,
    _lease: Option<MaintainerLease>,
}

impl Run<'_> {
    fn stopping(&self) -> bool {
        self.shared.stop.load(Ordering::SeqCst)
    }

    fn run(&self) {
        // A takeover or a wake must catch up: nobody watched until now.
        let mut catch_up = false;
        loop {
            let next = match self.follow_until_owner(catch_up) {
                Err(next) => next,
                Ok(owned) => self.maintain(owned),
            };
            match next {
                Next::Stop => return,
                Next::Sleep => {
                    if !self.sleep() {
                        return;
                    }
                    catch_up = true;
                }
            }
        }
    }

    /// Retries the lease until it is held (`Ok`), or until a stop or the idle
    /// timeout. A follower that wakes from sleep reconciles once before it
    /// lets the waiting request through, since nobody may have watched.
    fn follow_until_owner(&self, mut catch_up: bool) -> Result<Owned, Next> {
        let mut announced = false;
        loop {
            if self.stopping() {
                return Err(Next::Stop);
            }
            let lease = match self.index.blocking_lock().try_acquire_maintainer() {
                Ok(Some(lease)) => Some(Some(lease)),
                Ok(None) => None,
                Err(err) => {
                    tracing::warn!(error = %err, "cannot coordinate background maintenance; maintaining anyway");
                    Some(None)
                }
            };
            if let Some(lease) = lease {
                return self.become_owner(lease, catch_up || announced);
            }
            if !announced {
                tracing::info!(
                    "another process maintains this index; serving queries and following its updates"
                );
                announced = true;
            }
            if catch_up {
                self.refresh_and_activate();
                catch_up = false;
            }
            let retry = Instant::now() + with_jitter(self.settings.lease_retry);
            if self.park_until(retry) {
                return Err(Next::Sleep);
            }
        }
    }

    /// Starts the watcher. Edits made before the watch was registered
    /// produce no event of their own, so they are reconciled too: after a
    /// takeover or a wake (`catch_up`) at once, before any waiting request is
    /// let through; at the first start (edits since the startup reindex) one
    /// debounce later, as an edit would have been. Unchanged, that costs a
    /// hash-skipped walk and writes nothing.
    fn become_owner(&self, lease: Option<MaintainerLease>, catch_up: bool) -> Result<Owned, Next> {
        let first_reconciliation = if catch_up {
            self.settings.reconciliation_interval
        } else {
            self.settings.debounce
        };
        let watcher = match background::spawn_watcher_with_schedule(
            Arc::clone(self.index),
            self.registry.clone(),
            self.settings.root.clone(),
            self.settings.exclude.clone(),
            self.settings.debounce,
            self.settings.reconciliation_interval,
            first_reconciliation,
        ) {
            Ok(watcher) => watcher,
            Err(err) => {
                tracing::warn!(error = %err, "failed to start auto-reindex watcher; continuing without it");
                // The lease drops: another process may do better.
                self.shared.activity.set_phase(Phase::Active, None);
                return Err(Next::Stop);
            }
        };
        self.shared.owner.store(true, Ordering::SeqCst);
        if catch_up {
            // After the watcher started, so no later edit falls in a gap.
            tracing::info!("took over background maintenance of this index");
            self.refresh_and_activate();
        }
        Ok(Owned {
            watcher,
            _lease: lease,
        })
    }

    /// Runs one hash-skipped full reindex and lets waiting requests through.
    fn refresh_and_activate(&self) {
        let error = match self.index.blocking_lock().reindex(self.registry, false) {
            Ok(_) => None,
            Err(err) => {
                tracing::warn!(error = %err, "catch-up reindex failed; answering from the index as it stands");
                Some(format!("the index could not be refreshed: {err}"))
            }
        };
        self.shared.activity.set_phase(Phase::Active, error);
    }

    /// Owns the index until a stop or the idle timeout, then stops the
    /// watcher on purpose (no final reconciliation) and drops the lease.
    fn maintain(&self, owned: Owned) -> Next {
        let next = loop {
            if self.stopping() {
                break Next::Stop;
            }
            if self.park_until(far_future()) {
                break Next::Sleep;
            }
        };
        owned.watcher.stop();
        self.shared.owner.store(false, Ordering::SeqCst);
        next
    }

    /// Parks until `until`, a stop, or the idle deadline. Returns whether
    /// the idle timeout fell due.
    fn park_until(&self, until: Instant) -> bool {
        loop {
            if self.stopping() {
                return false;
            }
            let now = Instant::now();
            let idle_at = self
                .settings
                .idle
                .map(|idle| self.shared.activity.idle_deadline(idle));
            if idle_at.is_some_and(|at| at <= now) {
                return true;
            }
            let wake_at = idle_at.map_or(until, |at| at.min(until));
            if wake_at <= now {
                return false;
            }
            std::thread::park_timeout(wake_at - now);
        }
    }

    /// Sleeps until a request wakes it (`true`) or a stop (`false`).
    fn sleep(&self) -> bool {
        tracing::info!("no index-using request for the idle timeout; maintenance sleeps");
        {
            let mut state = self.shared.activity.state();
            // A request that arrived meanwhile is let through instead.
            if state.last_use + self.settings.idle.unwrap_or_default() > Instant::now() {
                return true;
            }
            state.phase = Phase::Asleep;
        }
        loop {
            if self.stopping() {
                return false;
            }
            if self.shared.activity.state().phase == Phase::Waking {
                tracing::info!("index-using request; maintenance wakes");
                return true;
            }
            std::thread::park();
        }
    }
}

fn far_future() -> Instant {
    Instant::now() + Duration::from_secs(60 * 60 * 24 * 365)
}

/// `base` plus up to a third more, varying per process and call.
fn with_jitter(base: Duration) -> Duration {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos())
        ^ std::process::id();
    let spread = (base.as_millis() / 3).max(1);
    base + Duration::from_millis((u128::from(seed) % spread) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_stays_within_a_third_of_the_base() {
        for _ in 0..100 {
            let wait = with_jitter(Duration::from_secs(30));
            assert!(wait >= Duration::from_secs(30) && wait <= Duration::from_secs(40));
        }
    }

    #[test]
    fn an_active_process_answers_without_waiting() {
        let activity = Activity::new();
        assert!(!activity.is_asleep());
        assert_eq!(activity.wake_blocking(), Ok(()));
    }

    #[test]
    fn idle_time_restarts_with_each_index_using_request() {
        let activity = Activity::new();
        std::thread::sleep(Duration::from_millis(30));
        assert!(activity.idle_for() >= Duration::from_millis(30));
        assert_eq!(activity.wake_blocking(), Ok(()));
        assert!(activity.idle_for() < Duration::from_millis(30));
    }
}
