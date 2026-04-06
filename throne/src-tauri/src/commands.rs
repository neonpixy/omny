use std::sync::Mutex;

use chancellor_client::DaemonClient;
use serde_json::Value;
use tauri::State;

pub struct AppState {
    pub client: Mutex<Option<DaemonClient>>,
}

impl AppState {
    pub fn new(client: Option<DaemonClient>) -> Self {
        Self {
            client: Mutex::new(client),
        }
    }
}

/// Execute a pipeline via the daemon.
/// This is the main bridge: JS calls window.omninet.run(pipeline) → invoke('omninet_run').
#[tauri::command]
pub fn omninet_run(pipeline: String, state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.client.lock().map_err(|e| e.to_string())?;
    let client = guard.as_ref().ok_or("Daemon not connected")?;

    let pipeline_val: Value =
        serde_json::from_str(&pipeline).map_err(|e| format!("Invalid pipeline JSON: {e}"))?;

    log::info!("omninet_run: pipeline={}", &pipeline[..pipeline.len().min(120)]);
    let steps = pipeline_val
        .get("steps")
        .and_then(|s| s.as_array())
        .ok_or("Pipeline must have a 'steps' array")?;

    let mut last_result = Value::Null;

    for (i, step) in steps.iter().enumerate() {
        let op = step
            .get("op")
            .and_then(|o| o.as_str())
            .ok_or_else(|| format!("Step {i} missing 'op'"))?;

        let input = step.get("input").cloned().unwrap_or(Value::Object(Default::default()));

        match client.call(op, input) {
            Ok(result) => last_result = result,
            Err(e) => {
                let err = serde_json::json!({
                    "ok": false,
                    "error": e.to_string(),
                    "failed_step": i,
                });
                return Ok(err.to_string());
            }
        }
    }

    let success = serde_json::json!({ "ok": true, "result": last_result });
    Ok(success.to_string())
}

/// Platform operations (chrome height, capture, etc.)
#[tauri::command]
pub fn omninet_platform(
    op: String,
    input: String,
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    log::info!("omninet_platform: op={op}");

    // daemon.restart is handled by Throne, not the daemon.
    if op == "daemon.restart" {
        return restart_daemon(&state, app);
    }

    let guard = state.client.lock().map_err(|e| e.to_string())?;
    let client = guard.as_ref().ok_or("Daemon not connected")?;

    let input_val: Value =
        serde_json::from_str(&input).unwrap_or(Value::Object(Default::default()));

    log::info!("omninet_platform: calling daemon...");
    match client.call(&op, input_val) {
        Ok(result) => {
            let success = serde_json::json!({ "ok": true, "result": result });
            Ok(success.to_string())
        }
        Err(e) => {
            let err = serde_json::json!({ "ok": false, "error": e.to_string() });
            Ok(err.to_string())
        }
    }
}

/// Stop the daemon, wait for full exit, reboot, reconnect, re-forward events.
fn restart_daemon(state: &State<'_, AppState>, app: tauri::AppHandle) -> Result<String, String> {
    // 1. Tell the running daemon to stop and drop our client connection.
    {
        let mut guard = state.client.lock().map_err(|e| e.to_string())?;
        if let Some(client) = guard.as_ref() {
            let _ = client.call("daemon.stop", serde_json::json!({}));
        }
        // Drop the client so the old connection is fully closed.
        *guard = None;
    }

    // 2. Wait for the daemon to fully exit (port release, socket cleanup).
    //    Poll the pidfile — when the process is gone, we're clear.
    let pidfile = crate::daemon::pidfile_path();
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        if !pidfile.exists() {
            break;
        }
    }
    // Extra grace for OS to release the port.
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 3. Boot and reconnect.
    let new_client = crate::daemon::connect();
    if let Some(ref client) = new_client {
        crate::start_event_forwarder(app, client);
    }

    // 4. Swap the client in AppState.
    {
        let mut guard = state.client.lock().map_err(|e| e.to_string())?;
        *guard = new_client;
    }

    let success = serde_json::json!({ "ok": true, "result": {"restarted": true} });
    Ok(success.to_string())
}

/// Get daemon status.
#[tauri::command]
pub fn omninet_status(state: State<'_, AppState>) -> Result<String, String> {
    let guard = state.client.lock().map_err(|e| e.to_string())?;

    match guard.as_ref() {
        Some(client) => match client.call("daemon.status", serde_json::json!({})) {
            Ok(result) => Ok(serde_json::json!({ "connected": true, "status": result }).to_string()),
            Err(e) => Ok(serde_json::json!({ "connected": false, "error": e.to_string() }).to_string()),
        },
        None => Ok(serde_json::json!({ "connected": false, "error": "No client" }).to_string()),
    }
}
