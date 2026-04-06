//! Envoy module — Globe networking courtier.
//!
//! Consolidates network, discovery, gospel, health, tower, and globe
//! operations under the unified `envoy.*` namespace. All ops are now
//! registered as `envoy.<operation>` for a flat, consistent API surface.

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct EnvoyModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for EnvoyModule {
    fn id(&self) -> &str { "envoy" }
    fn name(&self) -> &str { "Envoy (Globe)" }
    fn deps(&self) -> &[&str] { &["omnibus"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Relay & Publishing (absorbs network_mod)
        // ═══════════════════════════════════════════════════════════

        let s = state.clone();
        state.phone.register_raw("envoy.post", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let content = params.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let omnibus = s.omnibus.omnibus();
            match omnibus.post(content) {
                Ok(event) => {
                    let v = prerogative::api_json::omni_event_json(&event);
                    ok_json(&v)
                }
                Err(e) => Err(err("envoy.post", e)),
            }
        });

        // OmniEvent serde IS the wire protocol contract — deserializing inbound
        // events via serde is intentional here. Do not replace with hand-built JSON.
        let s = state.clone();
        state.phone.register_raw("envoy.publish", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let event_json = params.get("event").and_then(|v| v.as_str()).unwrap_or("{}");
            let event: globe::event::OmniEvent = serde_json::from_str(event_json)
                .map_err(|e| err("envoy.publish", format!("invalid event: {e}")))?;
            let omnibus = s.omnibus.omnibus();
            omnibus.publish(event).map_err(|e| err("envoy.publish", e))?;
            ok_json(&json!({"ok": true}))
        });

        let s = state.clone();
        state.phone.register_raw("envoy.connect_relay", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let url = params.get("url").and_then(|v| v.as_str())
                .ok_or_else(|| err("envoy.connect_relay", "missing 'url'"))?;
            let omnibus = s.omnibus.omnibus();
            omnibus.connect_relay(url).map_err(|e| err("envoy.connect_relay", e))?;
            ok_json(&json!({"ok": true}))
        });

        let s = state.clone();
        state.phone.register_raw("envoy.set_home", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let url = params.get("url").and_then(|v| v.as_str())
                .ok_or_else(|| err("envoy.set_home", "missing 'url'"))?;
            let omnibus = s.omnibus.omnibus();
            omnibus.set_home_node(url).map_err(|e| err("envoy.set_home", e))?;
            ok_json(&json!({"ok": true}))
        });

        // ═══════════════════════════════════════════════════════════
        // Discovery (absorbs discovery_mod)
        // ═══════════════════════════════════════════════════════════

        let s = state.clone();
        state.phone.register_raw("envoy.peers", move |_data| {
            let peers = s.omnibus.omnibus().peers();
            let peers_json: Vec<Value> = peers.iter()
                .map(|p| json!({"name": format!("{:?}", p)}))
                .collect();
            ok_json(&Value::Array(peers_json))
        });

        let s = state.clone();
        state.phone.register_raw("envoy.peer_count", move |_data| {
            ok_json(&json!({"count": s.omnibus.omnibus().peers().len()}))
        });

        let s = state.clone();
        state.phone.register_raw("envoy.connect_discovered", move |_data| {
            s.omnibus.omnibus().connect_discovered_peers();
            ok_json(&json!({"ok": true}))
        });

        // ═══════════════════════════════════════════════════════════
        // Gospel (absorbs gospel_mod)
        // ═══════════════════════════════════════════════════════════

        let s = state.clone();
        state.phone.register_raw("envoy.gospel_dump", move |_data| {
            let omnibus = s.omnibus.omnibus();
            match omnibus.gospel_registry() {
                Some(registry) => {
                    let events = registry.all_events();
                    let events_json: Vec<Value> = events.iter()
                        .map(prerogative::api_json::omni_event_json)
                        .collect();
                    ok_json(&Value::Array(events_json))
                }
                None => ok_json(&Value::Array(vec![])),
            }
        });

        let s = state.clone();
        state.phone.register_raw("envoy.gospel_save", move |_data| {
            let omnibus = s.omnibus.omnibus();
            omnibus.save_gospel();
            ok_json(&json!({"ok": true}))
        });

        // ═══════════════════════════════════════════════════════════
        // Health (absorbs health_mod)
        // ═══════════════════════════════════════════════════════════

        let s = state.clone();
        state.phone.register_raw("envoy.relay_health", move |_data| {
            let health = s.omnibus.omnibus().relay_health();
            let v: Vec<Value> = health.iter()
                .map(prerogative::api_json::relay_health_json)
                .collect();
            ok_json(&Value::Array(v))
        });

        let s = state.clone();
        state.phone.register_raw("envoy.store_stats", move |_data| {
            let stats = s.omnibus.omnibus().store_stats();
            let v = prerogative::api_json::store_stats_json(&stats);
            ok_json(&v)
        });

        let s = state.clone();
        state.phone.register_raw("envoy.logs", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let logs = s.omnibus.omnibus().recent_logs(count);
            let logs_json: Vec<Value> = logs.iter()
                .map(prerogative::api_json::log_entry_json)
                .collect();
            ok_json(&Value::Array(logs_json))
        });

        // ═══════════════════════════════════════════════════════════
        // Tower (absorbs tower_mod)
        // ═══════════════════════════════════════════════════════════

        let s = state.clone();
        state.phone.register_raw("envoy.tower_status", move |_data| {
            match s.omnibus.tower() {
                Some(t) => {
                    let ts = t.status();
                    let mode_val = serde_json::to_value(ts.mode).unwrap_or(Value::Null);
                    let v = json!({
                        "mode": mode_val,
                        "name": ts.name,
                        "relay_url": ts.relay_url,
                        "relay_port": ts.relay_port,
                        "relay_connections": ts.relay_connections,
                        "has_identity": ts.has_identity,
                        "pubkey": ts.pubkey,
                        "gospel_peers": ts.gospel_peers,
                        "gospel_peer_urls": ts.gospel_peer_urls,
                        "uptime_secs": ts.uptime_secs,
                        "event_count": ts.event_count,
                        "indexed_count": ts.indexed_count,
                        "communities": ts.communities,
                        "federated_communities": ts.federated_communities,
                        "connection_policy": ts.connection_policy,
                        "allowlist_size": ts.allowlist_size,
                        "connections_rejected": ts.connections_rejected,
                    });
                    ok_json(&v)
                }
                None => ok_json(&json!({"enabled": false})),
            }
        });

        state.phone.register_raw("envoy.tower_start", |_data| {
            Err(err("envoy.tower_start", "Tower requires config change + daemon restart"))
        });

        state.phone.register_raw("envoy.tower_stop", |_data| {
            Err(err("envoy.tower_stop", "Tower requires config change + daemon restart"))
        });

        // ═══════════════════════════════════════════════════════════
        // Globe operations
        // ═══════════════════════════════════════════════════════════

        // envoy.event_verify — verify an event's signature (stateless)
        state.phone.register_raw("envoy.event_verify", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let event_json = params.get("event").and_then(|v| v.as_str())
                .ok_or_else(|| err("envoy.event_verify", "missing 'event'"))?;
            let event: globe::event::OmniEvent = serde_json::from_str(event_json)
                .map_err(|e| err("envoy.event_verify", format!("invalid event: {e}")))?;
            let valid = globe::EventBuilder::verify(&event).unwrap_or(false);
            ok_json(&json!({ "valid": valid }))
        });

        // envoy.relay_count — how many relays are connected
        let s = state.clone();
        state.phone.register_raw("envoy.relay_count", move |_data| {
            let count = s.omnibus.omnibus().relay_health().len();
            ok_json(&json!({ "count": count }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Relay & publishing
            .with_call(CallDescriptor::new("envoy.post", "Post content"))
            .with_call(CallDescriptor::new("envoy.publish", "Publish event"))
            .with_call(CallDescriptor::new("envoy.connect_relay", "Connect to relay"))
            .with_call(CallDescriptor::new("envoy.set_home", "Set home node"))
            // Discovery
            .with_call(CallDescriptor::new("envoy.peers", "List discovered peers"))
            .with_call(CallDescriptor::new("envoy.peer_count", "Count discovered peers"))
            .with_call(CallDescriptor::new("envoy.connect_discovered", "Connect to all discovered"))
            // Gospel
            .with_call(CallDescriptor::new("envoy.gospel_dump", "Dump gospel registry"))
            .with_call(CallDescriptor::new("envoy.gospel_save", "Persist gospel to disk"))
            // Health
            .with_call(CallDescriptor::new("envoy.relay_health", "Relay health snapshots"))
            .with_call(CallDescriptor::new("envoy.store_stats", "Event store stats"))
            .with_call(CallDescriptor::new("envoy.logs", "Recent log entries"))
            // Tower
            .with_call(CallDescriptor::new("envoy.tower_status", "Tower status"))
            .with_call(CallDescriptor::new("envoy.tower_start", "Start Tower"))
            .with_call(CallDescriptor::new("envoy.tower_stop", "Stop Tower"))
            // Globe
            .with_call(CallDescriptor::new("envoy.event_verify", "Verify event signature"))
            .with_call(CallDescriptor::new("envoy.relay_count", "Count connected relays"))
            // Events
            .with_emitted_event(EventDescriptor::new("envoy.connected", "Relay connected"))
            .with_emitted_event(EventDescriptor::new("envoy.disconnected", "Relay disconnected"))
    }
}
