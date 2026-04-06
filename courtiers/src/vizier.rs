//! Vizier module — collaboration courtier.
//!
//! The Vizier is the Right Hand: multiplayer sessions with Yjs binary sync.
//! The daemon is a dumb pipe — it receives base64-encoded Yjs binary data
//! from the browser, wraps it in a signed relay event, and forwards it to
//! peers. Incoming data is unwrapped and dispatched back to the browser
//! via Equipment Email.
//!
//! Two data channels:
//! - `collab.sync` / `collab.sync_message` — Yjs document sync bytes
//! - `collab.awareness` / `collab.awareness_update` — Yjs awareness state
//!
//! Presence (join/leave/peers) is unchanged.
//!
//! Programs use staff name "vizier" which maps to daemon namespace "collab".

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use globe::collaboration::{CollaborationMessage, KIND_COLLABORATION};
use globe::client::pool::PoolEvent;
use globe::event::OmniEvent;
use globe::filter::OmniFilter;
use globe::UnsignedEvent;
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

// ── Types ────────────────────────────────────────────────────────────

/// An active collaboration session for one `.idea` document.
struct CollabSession {
    /// The `.idea` document this session tracks.
    #[allow(dead_code)]
    idea_id: Uuid,
    /// Our Crown public key hex (author field in events).
    my_crown_id: String,
    /// Known peers in this session: crown_id -> PeerInfo.
    peers: HashMap<String, PeerInfo>,
    /// Omnibus subscription ID (for cleanup on leave).
    #[allow(dead_code)]
    subscription_id: String,
    /// When this session was joined.
    #[allow(dead_code)]
    joined_at: Instant,
}

/// Information about a remote collaborator.
#[derive(Debug, Clone)]
struct PeerInfo {
    /// Crown public key hex.
    crown_id: String,
    /// Human-readable name (from presence).
    display_name: String,
    /// Assigned color (from presence).
    color: String,
    /// Most recent cursor position (opaque JSON).
    cursor: Option<Value>,
    /// When we last heard from this peer.
    last_seen: Instant,
}

/// Shared session store — module-level state protected by a Mutex.
type SessionStore = Arc<Mutex<HashMap<Uuid, CollabSession>>>;

// ── Module ───────────────────────────────────────────────────────────

pub struct VizierModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

/// Parse a UUID from JSON params, returning a PhoneError on failure.
fn parse_idea_id(params: &Value, op: &str) -> Result<Uuid, PhoneError> {
    let id_str = params.get("idea_id").and_then(|v| v.as_str())
        .ok_or_else(|| err(op, "missing 'idea_id'"))?;
    Uuid::parse_str(id_str)
        .map_err(|e| err(op, format!("invalid UUID: {e}")))
}

impl DaemonModule for VizierModule {
    fn id(&self) -> &str { "vizier" }
    fn name(&self) -> &str { "Vizier (Collaboration)" }
    fn deps(&self) -> &[&str] { &["omnibus", "artificer"] }

    fn register(&self, state: &Arc<DaemonState>) {
        let sessions: SessionStore = Arc::new(Mutex::new(HashMap::new()));

        // ── collab.join ──────────────────────────────────────────
        {
            let s = state.clone();
            let sessions = sessions.clone();
            state.phone.register_raw("vizier.join", move |data| {
                let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
                let idea_id = parse_idea_id(&params, "vizier.join")?;

                // Get our Crown identity.
                let omnibus = s.omnibus.omnibus();
                let my_crown_id = omnibus.pubkey_hex()
                    .ok_or_else(|| err("vizier.join", "No identity — create one first"))?;

                // Check if already joined.
                {
                    let store = sessions.lock().unwrap_or_else(|e| e.into_inner());
                    if store.contains_key(&idea_id) {
                        return ok_json(&json!({
                            "ok": true,
                            "already_joined": true,
                            "crown_id": my_crown_id,
                            "idea_id": idea_id.to_string(),
                        }));
                    }
                }

                // Subscribe to relay events for this document (presence/cursor only).
                let mut tag_filters = HashMap::new();
                tag_filters.insert('d', vec![idea_id.to_string()]);
                let filter = OmniFilter {
                    kinds: Some(vec![KIND_COLLABORATION]),
                    tag_filters,
                    ..Default::default()
                };

                let (sub_id, receiver) = omnibus.subscribe(vec![filter]);

                log::info!("collab: joined session for idea {idea_id} (sub={sub_id})");

                // Publish a Crown-signed presence "join" event.
                let join_msg = CollaborationMessage::Presence {
                    info: json!({
                        "crown_id": &my_crown_id,
                        "action": "join",
                    }),
                };
                let content = serde_json::to_string(&join_msg)
                    .unwrap_or_default();
                let unsigned = UnsignedEvent::new(KIND_COLLABORATION, content)
                    .with_d_tag(&idea_id.to_string());
                if let Err(e) = omnibus.sign_and_publish(&unsigned) {
                    log::warn!("collab: failed to publish join presence: {e}");
                }

                // Store the session.
                let now = Instant::now();
                let session = CollabSession {
                    idea_id,
                    my_crown_id: my_crown_id.clone(),
                    peers: HashMap::new(),
                    subscription_id: sub_id,
                    joined_at: now,
                };
                {
                    let mut store = sessions.lock().unwrap_or_else(|e| e.into_inner());
                    store.insert(idea_id, session);
                }

                // Spawn background receiver for incoming relay events.
                spawn_event_receiver(
                    idea_id,
                    my_crown_id.clone(),
                    receiver,
                    s.clone(),
                    sessions.clone(),
                );

                ok_json(&json!({
                    "ok": true,
                    "crown_id": my_crown_id,
                    "idea_id": idea_id.to_string(),
                }))
            });
        }

        // ── collab.leave ─────────────────────────────────────────
        {
            let s = state.clone();
            let sessions = sessions.clone();
            state.phone.register_raw("vizier.leave", move |data| {
                let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
                let idea_id = parse_idea_id(&params, "vizier.leave")?;

                let session = {
                    let mut store = sessions.lock().unwrap_or_else(|e| e.into_inner());
                    store.remove(&idea_id)
                };

                let Some(session) = session else {
                    return Err(err("vizier.leave", "not in a session for this idea"));
                };

                // Publish a Crown-signed presence "leave" event.
                let leave_msg = CollaborationMessage::Presence {
                    info: json!({
                        "crown_id": &session.my_crown_id,
                        "action": "leave",
                    }),
                };
                let content = serde_json::to_string(&leave_msg)
                    .unwrap_or_default();
                let unsigned = UnsignedEvent::new(KIND_COLLABORATION, content)
                    .with_d_tag(&idea_id.to_string());
                let omnibus = s.omnibus.omnibus();
                if let Err(e) = omnibus.sign_and_publish(&unsigned) {
                    log::warn!("collab: failed to publish leave presence: {e}");
                }

                log::info!("collab: left session for idea {idea_id}");

                ok_json(&json!({ "ok": true }))
            });
        }

        // ── collab.peers ─────────────────────────────────────────
        {
            let sessions = sessions.clone();
            state.phone.register_raw("vizier.peers", move |data| {
                let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
                let idea_id = parse_idea_id(&params, "vizier.peers")?;

                let store = sessions.lock().unwrap_or_else(|e| e.into_inner());
                let session = store.get(&idea_id)
                    .ok_or_else(|| err("vizier.peers", "not in a session for this idea"))?;

                let peers: Vec<Value> = session.peers.values().map(|p| {
                    json!({
                        "crown_id": p.crown_id,
                        "display_name": p.display_name,
                        "color": p.color,
                        "cursor": p.cursor,
                    })
                }).collect();

                ok_json(&json!({ "peers": peers }))
            });
        }

        // ── collab.sync ──────────────────────────────────────────
        {
            let s = state.clone();
            let sessions = sessions.clone();
            state.phone.register_raw("vizier.sync", move |data| {
                let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
                let idea_id = parse_idea_id(&params, "vizier.sync")?;
                let data_b64 = params.get("data").and_then(|v| v.as_str())
                    .ok_or_else(|| err("vizier.sync", "missing 'data'"))?;

                // Must be in a session to broadcast.
                {
                    let store = sessions.lock().unwrap_or_else(|e| e.into_inner());
                    store.get(&idea_id)
                        .ok_or_else(|| err("vizier.sync", "not in a session for this idea"))?;
                }

                log::info!("vizier.sync: publishing yjs sync for idea {idea_id} ({} bytes b64)", data_b64.len());

                // Publish Yjs sync bytes as Operations message via relay.
                let ops_msg = CollaborationMessage::Operations {
                    ops: vec![json!({
                        "kind": "yjs_sync",
                        "data": data_b64,
                    })],
                };
                let msg_content = serde_json::to_string(&ops_msg)
                    .unwrap_or_default();
                let unsigned = UnsignedEvent::new(KIND_COLLABORATION, msg_content)
                    .with_d_tag(&idea_id.to_string());
                let omnibus = s.omnibus.omnibus();
                match omnibus.sign_and_publish(&unsigned) {
                    Ok(_) => log::info!("vizier.sync: published to relay OK"),
                    Err(e) => log::warn!("vizier.sync: failed to publish: {e}"),
                }

                ok_json(&json!({ "ok": true }))
            });
        }

        // ── collab.awareness ─────────────────────────────────────
        {
            let s = state.clone();
            let sessions = sessions.clone();
            state.phone.register_raw("vizier.awareness", move |data| {
                let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
                let idea_id = parse_idea_id(&params, "vizier.awareness")?;
                let data_b64 = params.get("data").and_then(|v| v.as_str())
                    .ok_or_else(|| err("vizier.awareness", "missing 'data'"))?;

                // Must be in a session to broadcast.
                let my_crown_id = {
                    let store = sessions.lock().unwrap_or_else(|e| e.into_inner());
                    let session = store.get(&idea_id)
                        .ok_or_else(|| err("vizier.awareness", "not in a session for this idea"))?;
                    session.my_crown_id.clone()
                };

                log::info!("vizier.awareness: publishing awareness for idea {idea_id}");

                // Publish awareness bytes as Cursor message via relay.
                let awareness_msg = CollaborationMessage::Cursor {
                    crown_id: my_crown_id,
                    cursor: json!({
                        "kind": "awareness",
                        "data": data_b64,
                    }),
                };
                let msg_content = serde_json::to_string(&awareness_msg)
                    .unwrap_or_default();
                let unsigned = UnsignedEvent::new(KIND_COLLABORATION, msg_content)
                    .with_d_tag(&idea_id.to_string());
                let omnibus = s.omnibus.omnibus();
                match omnibus.sign_and_publish(&unsigned) {
                    Ok(_) => log::info!("vizier.awareness: published to relay OK"),
                    Err(e) => log::warn!("vizier.awareness: failed to publish: {e}"),
                }

                ok_json(&json!({ "ok": true }))
            });
        }
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            .with_call(CallDescriptor::new("vizier.join", "Join a collaboration session for a document"))
            .with_call(CallDescriptor::new("vizier.leave", "Leave a collaboration session"))
            .with_call(CallDescriptor::new("vizier.peers", "List peers in a collaboration session"))
            .with_call(CallDescriptor::new("vizier.sync", "Send Yjs sync message to peers"))
            .with_call(CallDescriptor::new("vizier.awareness", "Send awareness update to peers"))
            .with_emitted_event(EventDescriptor::new("vizier.sync_message", "Yjs sync data from remote peer"))
            .with_emitted_event(EventDescriptor::new("vizier.awareness_update", "Awareness state from remote peer"))
            .with_emitted_event(EventDescriptor::new("vizier.presence_update", "Peer joined or left"))
    }
}

// ── Background event receiver ────────────────────────────────────────

/// Spawn a task that receives relay events for a document session and
/// dispatches them via Equipment Email.
fn spawn_event_receiver(
    idea_id: Uuid,
    my_crown_id: String,
    mut receiver: tokio::sync::broadcast::Receiver<PoolEvent>,
    state: Arc<DaemonState>,
    sessions: SessionStore,
) {
    let rt = state.omnibus.omnibus().runtime().clone();

    log::info!("collab: spawning event receiver for idea {idea_id}, my_crown_id={my_crown_id}");

    rt.spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(pool_event) => {
                    let event = &pool_event.event;
                    log::info!("collab: receiver got event — author={}, kind={}, d_tag={:?}",
                        event.author, event.kind, event.d_tag());

                    if event.author == my_crown_id {
                        log::info!("collab: dropping own message (author matches my_crown_id)");
                        continue;
                    }

                    let expected_d = idea_id.to_string();
                    if event.d_tag() != Some(&expected_d) {
                        log::info!("collab: dropping — d_tag {:?} != expected {expected_d}", event.d_tag());
                        continue;
                    }

                    if event.kind == KIND_COLLABORATION {
                        log::info!("collab: dispatching collaboration message");
                        dispatch_collab_message(event, idea_id, &state, &sessions);
                    } else {
                        log::info!("collab: ignoring non-collab kind {}", event.kind);
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    log::info!("collab: event receiver closed for {idea_id}");
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("collab: event receiver lagged by {n} events for {idea_id}");
                }
            }
        }
    });
}

/// Handle a parsed CollaborationMessage from a remote peer.
fn dispatch_collab_message(
    event: &OmniEvent,
    idea_id: Uuid,
    state: &DaemonState,
    sessions: &SessionStore,
) {
    let msg: CollaborationMessage = match serde_json::from_str(&event.content) {
        Ok(m) => m,
        Err(e) => {
            log::warn!("collab: failed to parse CollaborationMessage: {e}");
            return;
        }
    };

    log::info!("collab: dispatch_collab_message — variant: {}", match &msg {
        CollaborationMessage::Cursor { .. } => "Cursor",
        CollaborationMessage::Presence { .. } => "Presence",
        CollaborationMessage::Operations { .. } => "Operations",
        CollaborationMessage::Control { .. } => "Control",
    });

    match msg {
        CollaborationMessage::Cursor { crown_id: _, cursor } => {
            if cursor.get("kind").and_then(|v| v.as_str()) == Some("awareness") {
                let data = cursor.get("data").and_then(|v| v.as_str()).unwrap_or_default();
                let email_data = json!({
                    "idea_id": idea_id.to_string(),
                    "data": data,
                    "author": event.author,
                });
                let bytes = serde_json::to_vec(&email_data).unwrap_or_default();
                log::info!("collab: → email send_raw collab.awareness_update");
                state.email.send_raw("vizier.awareness_update", &bytes);
            }
        }

        CollaborationMessage::Presence { info } => {
            let crown_id = info.get("crown_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&event.author);
            let action = info.get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("update");

            {
                let mut store = sessions.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(session) = store.get_mut(&idea_id) {
                    match action {
                        "leave" => {
                            session.peers.remove(crown_id);
                        }
                        _ => {
                            let peer = session.peers.entry(crown_id.to_string())
                                .or_insert_with(|| PeerInfo {
                                    crown_id: crown_id.to_string(),
                                    display_name: String::new(),
                                    color: String::new(),
                                    cursor: None,
                                    last_seen: Instant::now(),
                                });
                            if let Some(name) = info.get("display_name").and_then(|v| v.as_str()) {
                                peer.display_name = name.to_string();
                            }
                            if let Some(color) = info.get("color").and_then(|v| v.as_str()) {
                                peer.color = color.to_string();
                            }
                            peer.last_seen = Instant::now();
                        }
                    }
                }
            }

            let email_data = json!({
                "idea_id": idea_id.to_string(),
                "crown_id": crown_id,
                "action": action,
                "info": info,
            });
            let bytes = serde_json::to_vec(&email_data).unwrap_or_default();
            state.email.send_raw("vizier.presence_update", &bytes);
        }

        CollaborationMessage::Operations { ops } => {
            log::info!("collab: Operations with {} ops", ops.len());
            for op in &ops {
                if op.get("kind").and_then(|v| v.as_str()) == Some("yjs_sync") {
                    let data = op.get("data").and_then(|v| v.as_str()).unwrap_or_default();
                    let email_data = json!({
                        "idea_id": idea_id.to_string(),
                        "data": data,
                        "author": event.author,
                    });
                    let bytes = serde_json::to_vec(&email_data).unwrap_or_default();
                    log::info!("collab: → email send_raw collab.sync_message ({} bytes b64)", data.len());
                    state.email.send_raw("vizier.sync_message", &bytes);
                }
            }
        }

        CollaborationMessage::Control { action } => {
            log::info!("collab: control action for {idea_id}: {action:?}");
            let email_data = json!({
                "idea_id": idea_id.to_string(),
                "control": format!("{action:?}"),
            });
            let bytes = serde_json::to_vec(&email_data).unwrap_or_default();
            state.email.send_raw("vizier.presence_update", &bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_idea_id_valid() {
        let id = Uuid::new_v4();
        let params = json!({ "idea_id": id.to_string() });
        let parsed = parse_idea_id(&params, "test").unwrap();
        assert_eq!(parsed, id);
    }

    #[test]
    fn test_parse_idea_id_missing() {
        let params = json!({});
        assert!(parse_idea_id(&params, "test").is_err());
    }

    #[test]
    fn test_parse_idea_id_invalid() {
        let params = json!({ "idea_id": "not-a-uuid" });
        assert!(parse_idea_id(&params, "test").is_err());
    }

    #[test]
    fn test_peer_info_defaults() {
        let peer = PeerInfo {
            crown_id: "peer1".into(),
            display_name: "Alice".into(),
            color: "#ff0000".into(),
            cursor: None,
            last_seen: Instant::now(),
        };
        assert_eq!(peer.crown_id, "peer1");
        assert!(peer.cursor.is_none());
    }
}
