mod commands;
mod daemon;
mod protocols;
#[cfg(debug_assertions)]
mod watcher;

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager, RunEvent, WindowEvent};

pub fn run() {
    env_logger::init();

    // Shared daemon client for protocol handlers
    let net_client: Arc<Mutex<Option<chancellor_client::DaemonClient>>> =
        Arc::new(Mutex::new(None));
    let setup_client = Arc::clone(&net_client);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        // omny:// — serve bundled program assets
        .register_uri_scheme_protocol("omny", move |ctx, request| {
            protocols::handle_omny(ctx, &request)
        })
        // net:// — resolve Omninet content via daemon (async)
        .register_asynchronous_uri_scheme_protocol("net", move |_ctx, request, responder| {
            protocols::handle_net(request, responder, Arc::clone(&net_client));
        })
        .setup(move |app| {
            // Kill any running tray (Throne takes over)
            daemon::kill_tray();

            // Connect to daemon (boot it if not running)
            let client = daemon::connect();

            // Store in shared Arc for protocol handlers (separate connection)
            if let Ok(mut guard) = setup_client.lock() {
                *guard = client.as_ref().and_then(|_| {
                    chancellor_client::DaemonClient::connect_as(chancellor_client::ClientType::Throne).ok()
                });
            }

            // Start push event forwarder if connected
            if let Some(ref client) = client {
                start_event_forwarder(app.handle().clone(), client);
            }

            app.manage(commands::AppState::new(client));

            if let Some(window) = app.get_webview_window("main") {
                #[cfg(target_os = "macos")]
                set_window_radius(&window, 15.0);

                // Devtools available via right-click → Inspect Element in debug builds
            }

            // Start file watcher for HMR in dev builds
            #[cfg(debug_assertions)]
            {
                let dist_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");
                watcher::start_watcher(app.handle().clone(), dist_dir);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::omninet_run,
            commands::omninet_platform,
            commands::omninet_status,
        ])
        .on_window_event(|window, event| {
            // Close button hides the window instead of quitting
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
                log::info!("Window hidden (close button)");
            }
        })
        .build(tauri::generate_context!())
        .expect("error building Throne");

    use std::sync::atomic::{AtomicBool, Ordering};
    static TRAY_LAUNCHED: AtomicBool = AtomicBool::new(false);

    app.run(|app, event| {
        match event {
            // Dock icon clicked — show the window
            RunEvent::Reopen { .. } => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            // App actually exiting (Cmd+Q or programmatic) — launch tray
            RunEvent::Exit => {
                if !TRAY_LAUNCHED.swap(true, Ordering::SeqCst) {
                    log::info!("Throne exiting, launching tray...");
                    daemon::launch_tray();
                }
            }
            _ => {}
        }
    });
}

/// Set the native NSWindow corner radius + white background on macOS.
#[cfg(target_os = "macos")]
fn set_window_radius(window: &tauri::WebviewWindow, radius: f64) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let ptr = match window.ns_window() {
        Ok(w) => w,
        Err(e) => {
            log::warn!("Could not get NSWindow: {e}");
            return;
        }
    };

    unsafe {
        let ns_win = ptr as *mut AnyObject;

        // Keep window itself transparent so corners are rounded
        let clear: *mut AnyObject = msg_send![objc2::class!(NSColor), clearColor];
        let () = msg_send![ns_win, setBackgroundColor: clear];

        // Round corners + white background on the layer (clips to rounded corners)
        let content_view: *mut AnyObject = msg_send![ns_win, contentView];
        let () = msg_send![content_view, setWantsLayer: true];
        let layer: *mut AnyObject = msg_send![content_view, layer];
        let () = msg_send![layer, setCornerRadius: radius];
        let () = msg_send![layer, setMasksToBounds: true];

        let ns_white: *mut AnyObject = msg_send![objc2::class!(NSColor), whiteColor];
        let cg_white: *const std::ffi::c_void = msg_send![ns_white, CGColor];
        let () = msg_send![layer, setBackgroundColor: cg_white];
    }
}

/// Forward daemon push events to the Tauri event system.
pub fn start_event_forwarder(handle: tauri::AppHandle, client: &chancellor_client::DaemonClient) {
    // Tell the daemon we want push events — this starts the daemon-side forwarder
    // that writes events to the IPC stream for this client connection.
    match client.call("events.subscribe", serde_json::json!({})) {
        Ok(_) => log::info!("Subscribed to daemon push events"),
        Err(e) => log::warn!("Failed to subscribe to daemon events: {e}"),
    }

    match client.subscribe_events() {
        Ok(rx) => {
            std::thread::spawn(move || {
                while let Ok(event) = rx.recv() {
                    // Tauri event names can't contain dots — replace with slashes
                    let event_name = format!("omninet:{}", event.event.replace('.', "/"));
                    log::info!("[throne] forwarding event '{}' → '{}'", event.event, event_name);
                    if let Err(e) = handle.emit(&event_name, event.data) {
                        log::warn!("Failed to forward event {}: {e}", event.event);
                    }
                }
                log::info!("Event forwarder stopped (daemon disconnected)");
            });
        }
        Err(e) => {
            log::warn!("Could not subscribe to daemon events: {e}");
        }
    }
}
