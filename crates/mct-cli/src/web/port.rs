//! Stable localhost port: the port a project's dashboard used last is remembered in
//! `<root>/.mct-index/web-port` and reused while it is free.
//!
//! The listener the operating system hands back is the only reservation: a port is never
//! probed and then reopened, so there is no window for another process to take it.

use std::fs;
use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};

use anyhow::Context;

/// First choice for a project that has no remembered port yet.
pub(super) const PREFERRED_PORT: u16 = 8787;

fn port_file(root: &Path) -> PathBuf {
    root.join(".mct-index").join("web-port")
}

/// The remembered port; a missing, unreadable or malformed file means "none".
fn saved(root: &Path) -> Option<u16> {
    let port: u16 = fs::read_to_string(port_file(root))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    (port != 0).then_some(port)
}

/// Replaces the file in one rename so a concurrent start never reads half a number.
fn save(root: &Path, port: u16) -> std::io::Result<()> {
    let file = port_file(root);
    let temporary = file.with_extension("tmp");
    fs::write(&temporary, format!("{port}\n"))?;
    fs::rename(&temporary, file)
}

fn bind_loopback(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port))
}

/// Opens the dashboard listener on 127.0.0.1.
///
/// - `Some(port)` is an explicit choice: bound as given (`0` = any free port) and never
///   substituted or remembered; a busy port is an error.
/// - `None` reuses the remembered port (else `preferred`) while it is free, otherwise takes
///   a port the system picks, and remembers the port actually bound.
pub(super) fn bind(
    root: &Path,
    requested: Option<u16>,
    preferred: u16,
) -> anyhow::Result<TcpListener> {
    if let Some(port) = requested {
        return bind_loopback(port).with_context(|| match port {
            0 => "cannot bind a free localhost port".to_string(),
            _ => format!(
                "port {port} on 127.0.0.1 is unavailable (already in use?); \
                 stop the other process or pick a different --port"
            ),
        });
    }
    let remembered = saved(root);
    let listener = match bind_loopback(remembered.unwrap_or(preferred)) {
        Ok(listener) => listener,
        Err(error) if error.kind() == ErrorKind::AddrInUse => {
            bind_loopback(0).context("cannot bind a free localhost port")?
        }
        Err(error) => return Err(error).context("cannot bind the localhost web port"),
    };
    let port = listener.local_addr()?.port();
    if remembered != Some(port) {
        if let Err(error) = save(root, port) {
            eprintln!("warning: cannot remember web port {port}: {error}");
        }
    }
    Ok(listener)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn project() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".mct-index")).unwrap();
        root
    }

    fn free_port() -> u16 {
        let listener = bind_loopback(0).unwrap();
        listener.local_addr().unwrap().port()
    }

    fn port_of(listener: &TcpListener) -> u16 {
        listener.local_addr().unwrap().port()
    }

    #[test]
    fn first_start_takes_the_preferred_port_and_remembers_it() {
        let root = project();
        let preferred = free_port();
        let listener = bind(root.path(), None, preferred).unwrap();
        assert_eq!(port_of(&listener), preferred);
        assert!(listener.local_addr().unwrap().ip().is_loopback());
        assert_eq!(saved(root.path()), Some(preferred));
    }

    #[test]
    fn a_restart_reuses_the_remembered_port() {
        let root = project();
        let first = port_of(&bind(root.path(), None, free_port()).unwrap());
        // The listener above is gone; a different preferred port must not win.
        let again = bind(root.path(), None, free_port()).unwrap();
        assert_eq!(port_of(&again), first);
    }

    #[test]
    fn a_busy_remembered_port_falls_back_to_another_and_is_remembered() {
        let root = project();
        let held = bind(root.path(), None, free_port()).unwrap();
        let busy = port_of(&held);
        let other = bind(root.path(), None, free_port()).unwrap();
        let moved = port_of(&other);
        assert_ne!(moved, busy);
        assert_eq!(saved(root.path()), Some(moved));
        drop((held, other));
        assert_eq!(
            port_of(&bind(root.path(), None, free_port()).unwrap()),
            moved
        );
    }

    #[test]
    fn projects_keep_independent_ports() {
        let (a, b) = (project(), project());
        let first = bind(a.path(), None, free_port()).unwrap();
        let second = bind(b.path(), None, free_port()).unwrap();
        assert_ne!(port_of(&first), port_of(&second));
        assert_eq!(saved(a.path()), Some(port_of(&first)));
        assert_eq!(saved(b.path()), Some(port_of(&second)));
    }

    #[test]
    fn a_busy_explicit_port_is_an_error_and_changes_nothing() {
        let root = project();
        let remembered = port_of(&bind(root.path(), None, free_port()).unwrap());
        let held = bind_loopback(0).unwrap();
        let busy = port_of(&held);
        let error = bind(root.path(), Some(busy), PREFERRED_PORT).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(&busy.to_string()), "{message}");
        assert!(message.contains("unavailable"), "{message}");
        assert_eq!(saved(root.path()), Some(remembered));
    }

    #[test]
    fn an_explicit_port_is_not_remembered() {
        let root = project();
        let port = free_port();
        let listener = bind(root.path(), Some(port), PREFERRED_PORT).unwrap();
        assert_eq!(port_of(&listener), port);
        assert_eq!(saved(root.path()), None);
    }

    #[test]
    fn a_malformed_port_file_is_ignored_and_replaced() {
        let root = project();
        for junk in ["", "abc", "0", "70000", "-1"] {
            fs::write(port_file(root.path()), junk).unwrap();
            assert_eq!(saved(root.path()), None, "{junk:?}");
        }
        let preferred = free_port();
        let listener = bind(root.path(), None, preferred).unwrap();
        assert_eq!(port_of(&listener), preferred);
        assert_eq!(saved(root.path()), Some(preferred));
    }
}
