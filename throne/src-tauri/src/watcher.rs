use std::path::PathBuf;

use notify::{EventKind, RecursiveMode, Watcher};
use tauri::Emitter;

/// Watch `dist/` for compiled asset changes and emit `castle:reload` events.
/// Only used in dev builds — production serves from bundled dist/.
pub fn start_watcher(handle: tauri::AppHandle, dist_dir: PathBuf) {
    std::thread::spawn(move || {
        let app = handle.clone();
        let base = dist_dir.clone();

        let mut watcher = match notify::recommended_watcher(move |res: Result<notify::Event, _>| {
            let event = match res {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("Watcher error: {e}");
                    return;
                }
            };

            match event.kind {
                EventKind::Create(_) | EventKind::Modify(_) => {}
                _ => return,
            }

            for path in &event.paths {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if !matches!(ext, "js" | "css" | "html") {
                    continue;
                }
                let relative = path.strip_prefix(&base).unwrap_or(path);
                let payload = relative.to_string_lossy().to_string();
                log::info!("File changed: {payload}");
                if let Err(e) = app.emit("castle:reload", payload) {
                    log::warn!("Failed to emit reload event: {e}");
                }
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create file watcher: {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&dist_dir, RecursiveMode::Recursive) {
            log::error!("Failed to watch {}: {e}", dist_dir.display());
            return;
        }

        log::info!("Watching {} for changes", dist_dir.display());

        // Keep the thread alive — watcher is dropped when this thread exits
        loop {
            std::thread::park();
        }
    });
}
