//! Stops the dashboard once nobody uses it, so a forgotten `mct-cli web`
//! does not keep a port, a watcher and the maintainer lease for days. An open
//! tab keeps it alive: the page asks for `api/meta` every few seconds while
//! visible and about once a minute while hidden.
//!
//! The limit can be changed while the server runs (`POST api/settings`, from the
//! settings page). A passive request (`api/meta?passive=1`) does not count as
//! use: the page sends it for live updates, and only a person's input or a
//! normal request keeps the server alive.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often the watchdog looks at the clock at most.
const MAX_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// When the last request ended and whether one is being served.
pub(super) struct Activity {
    started: Instant,
    last_ms: AtomicU64,
    busy: AtomicBool,
    /// Seconds without use before the server stops; 0 means never.
    timeout_secs: AtomicU64,
}

/// Longest limit the settings page may set: one day.
pub(super) const MAX_MINUTES: u64 = 24 * 60;

impl Activity {
    #[cfg(test)]
    pub(super) fn new() -> Arc<Self> {
        Self::with_timeout(None)
    }

    pub(super) fn with_timeout(timeout: Option<Duration>) -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            last_ms: AtomicU64::new(0),
            busy: AtomicBool::new(false),
            timeout_secs: AtomicU64::new(timeout.map_or(0, |t| t.as_secs())),
        })
    }

    /// Marks a request as being served until the guard is dropped.
    pub(super) fn serving(&self) -> Serving<'_> {
        self.busy.store(true, Ordering::SeqCst);
        Serving(Some(self))
    }

    /// A request that is not use: it neither blocks nor restarts the clock.
    pub(super) fn passive(&self) -> Serving<'_> {
        Serving(None)
    }

    fn timeout(&self) -> Option<Duration> {
        match self.timeout_secs.load(Ordering::SeqCst) {
            0 => None,
            secs => Some(Duration::from_secs(secs)),
        }
    }

    #[cfg(test)]
    pub(super) fn minutes(&self) -> u64 {
        self.timeout_secs.load(Ordering::SeqCst) / 60
    }

    /// Sets the limit in minutes (0: never stop).
    pub(super) fn set_minutes(&self, minutes: u64) {
        self.timeout_secs
            .store(minutes.min(MAX_MINUTES) * 60, Ordering::SeqCst);
    }

    fn idle_for(&self) -> Option<Duration> {
        if self.busy.load(Ordering::SeqCst) {
            return None;
        }
        let last = Duration::from_millis(self.last_ms.load(Ordering::SeqCst));
        Some(self.started.elapsed().saturating_sub(last))
    }
}

/// Ends a request on drop, a panic included.
pub(super) struct Serving<'a>(Option<&'a Activity>);

impl Drop for Serving<'_> {
    fn drop(&mut self) {
        let Some(activity) = self.0 else { return };
        let now = u64::try_from(activity.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        activity.last_ms.store(now, Ordering::SeqCst);
        activity.busy.store(false, Ordering::SeqCst);
    }
}

/// How long to wait before looking again, or `None` once `idle` reached
/// `timeout`.
fn next_check(idle: Duration, timeout: Duration) -> Option<Duration> {
    let left = timeout.checked_sub(idle).filter(|left| !left.is_zero())?;
    Some(left.min(MAX_CHECK_INTERVAL))
}

/// Calls `on_idle` from a background thread once no request counted as use
/// for the current limit (re-read each round, so the settings page can change
/// it). The thread is detached: `on_idle` is expected to end the process.
pub(super) fn watch(activity: Arc<Activity>, on_idle: impl FnOnce() + Send + 'static) {
    let spawned = std::thread::Builder::new()
        .name("mct-web-idle".into())
        .spawn(move || loop {
            let wait = match (activity.timeout(), activity.idle_for()) {
                (Some(timeout), Some(idle)) => match next_check(idle, timeout) {
                    Some(wait) => wait,
                    None => return on_idle(),
                },
                (Some(timeout), None) => MAX_CHECK_INTERVAL.min(timeout),
                (None, _) => MAX_CHECK_INTERVAL,
            };
            std::thread::sleep(wait);
        });
    if let Err(error) = spawned {
        eprintln!("warning: the idle timeout is off: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waits_for_what_is_left_capped_and_stops_at_the_timeout() {
        let timeout = Duration::from_secs(600);
        assert_eq!(
            next_check(Duration::ZERO, timeout),
            Some(MAX_CHECK_INTERVAL)
        );
        assert_eq!(
            next_check(Duration::from_secs(590), timeout),
            Some(Duration::from_secs(10))
        );
        assert_eq!(next_check(timeout, timeout), None);
        assert_eq!(next_check(Duration::from_secs(900), timeout), None);
    }

    #[test]
    fn a_request_being_served_is_never_idle_and_ending_it_restarts_the_clock() {
        let activity = Activity::new();
        let serving = activity.serving();
        assert_eq!(activity.idle_for(), None);
        drop(serving);
        assert!(activity
            .idle_for()
            .is_some_and(|idle| idle < Duration::from_secs(5)));
    }

    #[test]
    fn a_passive_request_is_not_use_and_the_limit_changes_while_running() {
        let activity = Activity::with_timeout(Some(Duration::from_secs(600)));
        drop(activity.passive());
        assert!(activity
            .idle_for()
            .is_some_and(|idle| idle < Duration::from_secs(5)));
        assert_eq!(activity.minutes(), 10);
        activity.set_minutes(30);
        assert_eq!(activity.timeout(), Some(Duration::from_secs(1800)));
        activity.set_minutes(0);
        assert_eq!(activity.timeout(), None);
    }
}
