use std::path::{Path, PathBuf};

use tauri::http::{Request, Response};

/// Serve compiled assets from throne/dist/.
/// Handles URLs like omny://system/programs/hearth.js, omny://system/lib/@omnidea/ui.js
pub fn handle_omny<R: tauri::Runtime>(_ctx: tauri::UriSchemeContext<'_, R>, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let path = request.uri().path().trim_start_matches('/');

    // Resolve the dist directory (throne/dist/)
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../dist");

    // Try the exact path first, then with .html, then index.html
    let candidates = [
        dist.join(path),
        dist.join(format!("{path}.html")),
        dist.join(path).join("index.html"),
        dist.join("index.html"), // fallback for SPA routes
    ];

    for candidate in &candidates {
        if candidate.is_file() {
            match std::fs::read(candidate) {
                Ok(data) => {
                    let mime = guess_mime(candidate);
                    let cache = if is_hashed_asset(path) {
                        "public, max-age=31536000, immutable"
                    } else {
                        "no-cache"
                    };
                    return Response::builder()
                        .status(200)
                        .header("Content-Type", mime)
                        .header("Cache-Control", cache)
                        .body(data)
                        .unwrap_or_else(|_| error_response(500, "Failed to build response"));
                }
                Err(e) => {
                    log::warn!("Failed to read {}: {e}", candidate.display());
                }
            }
        }
    }

    error_response(404, &format!("Not found: {path}"))
}

/// Resolve Omninet content via daemon.
/// Handles URLs like net://sam.idea/portfolio
pub fn handle_net(
    request: Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
    client: std::sync::Arc<std::sync::Mutex<Option<chancellor_client::DaemonClient>>>,
) {
    std::thread::spawn(move || {
        let host = request
            .uri()
            .host()
            .unwrap_or_default()
            .to_string();
        let path = request.uri().path().to_string();

        let guard = match client.lock() {
            Ok(g) => g,
            Err(_) => {
                responder.respond(error_response(503, "Client lock poisoned"));
                return;
            }
        };

        let client = match guard.as_ref() {
            Some(c) => c,
            None => {
                responder.respond(error_response(503, "Daemon not connected"));
                return;
            }
        };

        let input = serde_json::json!({ "name": host, "path": path });
        match client.call("globe.resolve_name", input) {
            Ok(result) => {
                let body = result.to_string().into_bytes();
                responder.respond(
                    Response::builder()
                        .status(200)
                        .header("Content-Type", "application/json")
                        .body(body)
                        .unwrap_or_else(|_| error_response(200, "{}")),
                );
            }
            Err(e) => {
                let html = format!(
                    "<html><body style='font-family:system-ui;padding:40px;color:#666'>\
                    <h2>Could not resolve {}</h2><p>{}</p></body></html>",
                    html_escape(&host),
                    html_escape(&e.to_string()),
                );
                responder.respond(
                    Response::builder()
                        .status(404)
                        .header("Content-Type", "text/html")
                        .body(html.into_bytes())
                        .unwrap_or_else(|_| error_response(404, "Not found")),
                );
            }
        }
    });
}

fn error_response(status: u16, message: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain")
        .body(message.as_bytes().to_vec())
        .unwrap()
}

fn guess_mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "application/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("wgsl") => "text/plain; charset=utf-8",
        Some("eot") => "application/vnd.ms-fontobject",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn is_hashed_asset(path: &str) -> bool {
    path.starts_with("assets/")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
