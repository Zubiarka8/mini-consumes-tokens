//! Exiting the whole process after a stretch without use (`--exit-idle`).
//!
//! The timeout comes from the flag or `MCT_EXIT_IDLE` when given, else from
//! the `mcp` setting (minutes, 0 = never) the dashboard keeps in
//! `.mct-index/web-settings.json`. The setting is read again on every check,
//! so a change made in the dashboard applies without restarting the server.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use mct_obsidian::PublishGuard;

use crate::maintainer::Activity;
use crate::readiness::{IndexState, Readiness};
use crate::shutdown;

/// Longest settings file read: the real one holds a handful of short values.
const MAX_SETTINGS_BYTES: u64 = 4096;
/// How often a server with no timeout looks for one saved in the dashboard.
const SETTINGS_POLL: Duration = Duration::from_secs(60);
/// Most minutes the dashboard accepts: one day.
const MAX_MINUTES: u64 = 24 * 60;

/// The timeout saved in the dashboard, if one is set and above zero.
pub fn saved_timeout(root: &Path) -> Option<Duration> {
    let file = File::open(root.join(".mct-index").join("web-settings.json")).ok()?;
    let mut text = String::new();
    file.take(MAX_SETTINGS_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    let minutes = serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("mcp")?
        .as_u64()
        .filter(|minutes| (1..=MAX_MINUTES).contains(minutes))?;
    Some(Duration::from_secs(minutes * 60))
}

/// What the loop does next, given the timeout in force and the idle time.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Exit,
    Sleep(Duration),
}

fn next_step(timeout: Option<Duration>, idle: Duration, indexing: bool) -> Step {
    let Some(timeout) = timeout else {
        return Step::Sleep(SETTINGS_POLL);
    };
    if indexing {
        return Step::Sleep(timeout.min(SETTINGS_POLL));
    }
    match timeout.checked_sub(idle) {
        None | Some(Duration::ZERO) => Step::Exit,
        // Short waits keep a lowered setting from being missed for long.
        Some(left) => Step::Sleep(left.min(SETTINGS_POLL).max(Duration::from_secs(1))),
    }
}

/// Exits the process once it has been idle for the timeout in force. Startup
/// indexing counts as use, and a vault publication in progress finishes first.
pub async fn exit_when_idle(
    root: PathBuf,
    flag: Option<Duration>,
    activity: Activity,
    readiness: Readiness,
    publish_guard: PublishGuard,
) {
    loop {
        let timeout = flag.or_else(|| saved_timeout(&root));
        let indexing = matches!(readiness.state(), IndexState::Indexing { .. });
        match next_step(timeout, activity.idle_for(), indexing) {
            Step::Exit => {
                tracing::info!(?timeout, "idle timeout reached; exiting");
                shutdown::exit_when_not_publishing(&publish_guard, || std::process::exit(0));
            }
            Step::Sleep(wait) => tokio::time::sleep(wait).await,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh project folder, removed on drop; no settings file yet.
    struct Project(PathBuf);

    impl Project {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "mct-exit-idle-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(root.join(".mct-index")).unwrap();
            Self(root)
        }

        fn with(settings: &str) -> Self {
            let project = Self::new();
            std::fs::write(project.0.join(".mct-index/web-settings.json"), settings).unwrap();
            project
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_saved_minutes_become_the_timeout() {
        let root = Project::with(r#"{"theme":"dark","mcp":30}"#);
        assert_eq!(saved_timeout(&root.0), Some(Duration::from_secs(1800)));
    }

    #[test]
    fn zero_missing_invalid_or_oversized_values_mean_no_timeout() {
        for settings in [
            r#"{"mcp":0}"#,
            r#"{"theme":"dark"}"#,
            r#"{"mcp":"30"}"#,
            r#"{"mcp":-5}"#,
            r#"{"mcp":1441}"#,
            "{ broken",
        ] {
            assert_eq!(
                saved_timeout(&Project::with(settings).0),
                None,
                "{settings}"
            );
        }
        assert_eq!(saved_timeout(&Project::new().0), None);
    }

    #[test]
    fn the_loop_exits_only_once_idle_for_the_whole_timeout() {
        let timeout = Some(Duration::from_secs(600));
        let secs = Duration::from_secs;
        assert_eq!(next_step(timeout, secs(600), false), Step::Exit);
        assert_eq!(next_step(timeout, secs(900), false), Step::Exit);
        assert_eq!(next_step(timeout, secs(580), false), Step::Sleep(secs(20)));
        assert_eq!(
            next_step(timeout, secs(0), false),
            Step::Sleep(SETTINGS_POLL)
        );
    }

    #[test]
    fn indexing_or_no_timeout_never_exits() {
        let secs = Duration::from_secs;
        assert_eq!(
            next_step(Some(secs(10)), secs(999), true),
            Step::Sleep(secs(10))
        );
        assert_eq!(
            next_step(None, secs(999), false),
            Step::Sleep(SETTINGS_POLL)
        );
    }
}
