use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use chancellor_client::{ClientType, DaemonClient};

/// Path to the daemon pidfile: `~/.omnidea/daemon.pid`.
pub fn pidfile_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".omnidea").join("daemon.pid")
}

/// Path to the daemon binary (relative to the chancellor workspace).
fn daemon_binary() -> PathBuf {
    // In dev: use the debug build in chancellor/target/debug/
    // src-tauri → throne → Omny → chancellor
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../chancellor/target/debug/chancellor");
    if dev_path.exists() {
        return dev_path;
    }
    // Fallback: hope it's on PATH
    PathBuf::from("chancellor")
}

/// Path to the tray binary.
fn tray_binary() -> PathBuf {
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../chancellor/target/debug/herald");
    if dev_path.exists() {
        return dev_path;
    }
    PathBuf::from("herald")
}

/// Try to connect to a running daemon. If not running, boot it first.
pub fn connect() -> Option<DaemonClient> {
    // First attempt: maybe it's already running
    if let Ok(client) = DaemonClient::connect_as(ClientType::Throne) {
        log::info!("Connected to existing daemon");
        return Some(client);
    }

    // Not running — boot it
    log::info!("Daemon not running, starting it...");
    let bin = daemon_binary();
    match Command::new(&bin).arg("--daemon").spawn() {
        Ok(_) => log::info!("Daemon started: {}", bin.display()),
        Err(e) => {
            log::error!("Failed to start daemon at {}: {e}", bin.display());
            return None;
        }
    }

    // Wait for the socket to appear (daemon daemonizes, so spawn returns immediately)
    for i in 0..20 {
        thread::sleep(Duration::from_millis(250));
        if let Ok(client) = DaemonClient::connect_as(ClientType::Throne) {
            log::info!("Connected to daemon after {}ms", (i + 1) * 250);
            return Some(client);
        }
    }

    log::error!("Daemon started but failed to connect within 5 seconds");
    None
}

/// Stop the daemon cleanly via IPC (synchronous — call before exit).
#[allow(dead_code)]
pub fn stop_daemon() {
    match DaemonClient::connect_as(ClientType::Throne) {
        Ok(client) => {
            log::info!("Sending stop to daemon...");
            let _ = client.call("daemon.stop", serde_json::json!({}));
        }
        Err(e) => {
            log::warn!("Could not connect to daemon for stop: {e}");
        }
    }
}

/// Launch the tray app (call on browser quit so daemon gets a status icon).
pub fn launch_tray() {
    let bin = tray_binary();
    match Command::new(&bin).spawn() {
        Ok(_) => log::info!("Tray launched: {}", bin.display()),
        Err(e) => log::warn!("Failed to launch tray at {}: {e}", bin.display()),
    }
}

/// Kill any running tray process (call when Throne takes over).
pub fn kill_tray() {
    // pkill by process name — simple and effective
    let _ = Command::new("pkill").arg("-f").arg("herald").output();
}
