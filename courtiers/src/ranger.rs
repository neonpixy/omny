//! Ranger module — World courtier.
//!
//! Wraps World-level capabilities: Omnibus management, Tower management,
//! MagicalIndex search, and the physical world bridge (places, presence,
//! handoffs, etc.). The Ranger patrols the boundaries between digital
//! and physical Omnidea.
//!
//! `omnibus` and `tower` are already direct dependencies. The `state.omnibus`
//! field (OmnibusRef) provides access to both via `.omnibus()` and `.tower()`.
//!
//! Programs address the Ranger directly — all ops use the `ranger.*` namespace.

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct RangerModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for RangerModule {
    fn id(&self) -> &str { "ranger" }
    fn name(&self) -> &str { "Ranger (World)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Omnibus — Node Runtime
        // ═══════════════════════════════════════════════════════════

        // ── world.omnibus_status ──────────────────────────────────
        // Get the full Omnibus status snapshot.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_status", move |_data| {
            let status = s.omnibus.omnibus().status();
            let status_json = serde_json::to_value(&status)
                .map_err(|e| err("ranger.omnibus_status", e))?;
            ok_json(&status_json)
        });

        // ── world.omnibus_pubkey ──────────────────────────────────
        // Get the node's public key (crown_id).
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_pubkey", move |_data| {
            match s.omnibus.omnibus().pubkey_hex() {
                Some(pk) => ok_json(&json!({ "pubkey": pk })),
                None => ok_json(&json!({ "pubkey": null, "reason": "no identity loaded" })),
            }
        });

        // ── world.omnibus_profile ─────────────────────────────────
        // Get the current identity profile as JSON.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_profile", move |_data| {
            match s.omnibus.omnibus().profile_json() {
                Some(profile) => {
                    let v: Value = serde_json::from_str(&profile)
                        .unwrap_or(Value::String(profile));
                    ok_json(&v)
                }
                None => ok_json(&json!({ "profile": null })),
            }
        });

        // ── world.omnibus_peers ───────────────────────────────────
        // Get discovered mDNS peers.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_peers", move |_data| {
            let peers = s.omnibus.omnibus().peers();
            let peers_json: Vec<Value> = peers.iter()
                .map(|p| json!({"peer": format!("{:?}", p)}))
                .collect();
            ok_json(&json!({
                "peers": peers_json,
                "count": peers_json.len()
            }))
        });

        // ── world.omnibus_connect ─────────────────────────────────
        // Connect to a specific relay URL.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_connect", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let url = params.get("url").and_then(|v| v.as_str())
                .ok_or_else(|| err("ranger.omnibus_connect", "missing 'url'"))?;

            s.omnibus.omnibus().connect_relay(url)
                .map_err(|e| err("ranger.omnibus_connect", e))?;
            ok_json(&json!({ "ok": true }))
        });

        // ── world.omnibus_set_home ────────────────────────────────
        // Set the home node URL.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_set_home", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let url = params.get("url").and_then(|v| v.as_str())
                .ok_or_else(|| err("ranger.omnibus_set_home", "missing 'url'"))?;

            s.omnibus.omnibus().set_home_node(url)
                .map_err(|e| err("ranger.omnibus_set_home", e))?;

            s.email.send_raw("ranger.home_node_set", &serde_json::to_vec(&json!({
                "url": url
            })).unwrap_or_default());

            ok_json(&json!({ "ok": true }))
        });

        // ── world.omnibus_post ────────────────────────────────────
        // Post content (sign + publish text note).
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_post", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let content = params.get("content").and_then(|v| v.as_str())
                .ok_or_else(|| err("ranger.omnibus_post", "missing 'content'"))?;

            let event = s.omnibus.omnibus().post(content)
                .map_err(|e| err("ranger.omnibus_post", e))?;
            let event_json = prerogative::api_json::omni_event_json(&event);
            ok_json(&event_json)
        });

        // ── world.omnibus_publish ─────────────────────────────────
        // Publish a pre-built OmniEvent.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_publish", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let event_json = params.get("event").and_then(|v| v.as_str()).unwrap_or("{}");
            let event: globe::event::OmniEvent = serde_json::from_str(event_json)
                .map_err(|e| err("ranger.omnibus_publish", format!("invalid event: {e}")))?;

            s.omnibus.omnibus().publish(event)
                .map_err(|e| err("ranger.omnibus_publish", e))?;
            ok_json(&json!({ "ok": true }))
        });

        // ── world.omnibus_relay_health ────────────────────────────
        // Get health snapshots for all relays.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_relay_health", move |_data| {
            let health = s.omnibus.omnibus().relay_health();
            let v: Vec<Value> = health.iter()
                .map(prerogative::api_json::relay_health_json)
                .collect();
            ok_json(&Value::Array(v))
        });

        // ── world.omnibus_store_stats ─────────────────────────────
        // Get event store statistics.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_store_stats", move |_data| {
            let stats = s.omnibus.omnibus().store_stats();
            let v = prerogative::api_json::store_stats_json(&stats);
            ok_json(&v)
        });

        // ── world.omnibus_recent_logs ─────────────────────────────
        // Get recent log entries from the Omnibus log capture.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_recent_logs", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let count = params.get("count").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

            let logs = s.omnibus.omnibus().recent_logs(count);
            let logs_json: Vec<Value> = logs.iter()
                .map(prerogative::api_json::log_entry_json)
                .collect();
            ok_json(&Value::Array(logs_json))
        });

        // ── world.omnibus_gospel_dump ─────────────────────────────
        // Dump the gospel registry contents.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_gospel_dump", move |_data| {
            match s.omnibus.omnibus().gospel_registry() {
                Some(registry) => {
                    let events = registry.all_events();
                    let events_json: Vec<Value> = events.iter()
                        .map(prerogative::api_json::omni_event_json)
                        .collect();
                    ok_json(&json!({
                        "events": events_json,
                        "count": events_json.len()
                    }))
                }
                None => ok_json(&json!({ "events": [], "count": 0 })),
            }
        });

        // ── world.omnibus_gospel_save ─────────────────────────────
        // Save the gospel registry to the encrypted database.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_gospel_save", move |_data| {
            s.omnibus.omnibus().save_gospel();
            ok_json(&json!({ "ok": true }))
        });

        // ── world.omnibus_connect_discovered ──────────────────────
        // Connect to all mDNS-discovered peers.
        let s = state.clone();
        state.phone.register_raw("ranger.omnibus_connect_discovered", move |_data| {
            s.omnibus.omnibus().connect_discovered_peers();
            ok_json(&json!({ "ok": true }))
        });

        // ═══════════════════════════════════════════════════════════
        // Tower — Always-On Nodes
        // ═══════════════════════════════════════════════════════════

        // ── world.tower_status ────────────────────────────────────
        // Get Tower status if running in Tower mode.
        let s = state.clone();
        state.phone.register_raw("ranger.tower_status", move |_data| {
            match s.omnibus.tower() {
                Some(t) => {
                    let ts = t.status();
                    let mode_val = serde_json::to_value(ts.mode).unwrap_or(Value::Null);
                    ok_json(&json!({
                        "enabled": true,
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
                    }))
                }
                None => ok_json(&json!({ "enabled": false })),
            }
        });

        // ── world.tower_is_active ─────────────────────────────────
        // Quick check if Tower mode is active.
        let s = state.clone();
        state.phone.register_raw("ranger.tower_is_active", move |_data| {
            ok_json(&json!({ "active": s.omnibus.tower().is_some() }))
        });

        // ═══════════════════════════════════════════════════════════
        // Config
        // ═══════════════════════════════════════════════════════════

        // ── world.config_get ──────────────────────────────────────
        // Get the current daemon config.
        let s = state.clone();
        state.phone.register_raw("ranger.config_get", move |_data| {
            let config = s.config.lock().unwrap();
            let config_json = serde_json::to_value(&*config)
                .map_err(|e| err("ranger.config_get", e))?;
            ok_json(&config_json)
        });

        // ── world.config_set ──────────────────────────────────────
        // Update the daemon config.
        let s = state.clone();
        state.phone.register_raw("ranger.config_set", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let new_config_json = params.get("config")
                .ok_or_else(|| err("ranger.config_set", "missing 'config'"))?;

            let new_config: omnibus::DaemonConfig = serde_json::from_value(new_config_json.clone())
                .map_err(|e| err("ranger.config_set", e))?;

            let mut config = s.config.lock().unwrap();
            *config = new_config;

            s.email.send_raw("ranger.config_changed", &[]);
            ok_json(&json!({ "ok": true }))
        });

        // ═══════════════════════════════════════════════════════════
        // Identity Management (via Omnibus)
        // ═══════════════════════════════════════════════════════════

        // ── world.identity_update_name ────────────────────────────
        // Update the display name and re-publish profile.
        let s = state.clone();
        state.phone.register_raw("ranger.identity_update_name", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("ranger.identity_update_name", "missing 'name'"))?;

            s.omnibus.omnibus().update_display_name(name)
                .map_err(|e| err("ranger.identity_update_name", e))?;

            s.email.send_raw("ranger.identity_updated", &serde_json::to_vec(&json!({
                "name": name
            })).unwrap_or_default());

            ok_json(&json!({ "ok": true }))
        });

        // ── world.identity_export_keyring ─────────────────────────
        // Export the keyring for syncing to another device.
        let s = state.clone();
        state.phone.register_raw("ranger.identity_export_keyring", move |_data| {
            let keyring_bytes = s.omnibus.omnibus().export_keyring()
                .map_err(|e| err("ranger.identity_export_keyring", e))?;
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &keyring_bytes);
            ok_json(&json!({ "keyring": encoded }))
        });

        // ── world.identity_import_keyring ─────────────────────────
        // Import a keyring from another device.
        let s = state.clone();
        state.phone.register_raw("ranger.identity_import_keyring", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let keyring_b64 = params.get("keyring").and_then(|v| v.as_str())
                .ok_or_else(|| err("ranger.identity_import_keyring", "missing 'keyring'"))?;
            let keyring_bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD, keyring_b64
            ).map_err(|e| err("ranger.identity_import_keyring", format!("invalid base64: {e}")))?;

            s.omnibus.omnibus().import_keyring(&keyring_bytes)
                .map_err(|e| err("ranger.identity_import_keyring", e))?;

            s.email.send_raw("ranger.identity_imported", &[]);
            ok_json(&json!({ "ok": true }))
        });

        // ═══════════════════════════════════════════════════════════
        // Data Directory
        // ═══════════════════════════════════════════════════════════

        // ── world.data_dir ────────────────────────────────────────
        // Get the data directory path.
        let s = state.clone();
        state.phone.register_raw("ranger.data_dir", move |_data| {
            ok_json(&json!({ "path": s.data_dir.to_string_lossy() }))
        });

    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Omnibus — node runtime
            .with_call(CallDescriptor::new("ranger.omnibus_status", "Full node status"))
            .with_call(CallDescriptor::new("ranger.omnibus_pubkey", "Node public key"))
            .with_call(CallDescriptor::new("ranger.omnibus_profile", "Identity profile"))
            .with_call(CallDescriptor::new("ranger.omnibus_peers", "Discovered peers"))
            .with_call(CallDescriptor::new("ranger.omnibus_connect", "Connect to relay"))
            .with_call(CallDescriptor::new("ranger.omnibus_set_home", "Set home node"))
            .with_call(CallDescriptor::new("ranger.omnibus_post", "Post content"))
            .with_call(CallDescriptor::new("ranger.omnibus_publish", "Publish event"))
            .with_call(CallDescriptor::new("ranger.omnibus_relay_health", "Relay health snapshots"))
            .with_call(CallDescriptor::new("ranger.omnibus_store_stats", "Event store stats"))
            .with_call(CallDescriptor::new("ranger.omnibus_recent_logs", "Recent log entries"))
            .with_call(CallDescriptor::new("ranger.omnibus_gospel_dump", "Dump gospel"))
            .with_call(CallDescriptor::new("ranger.omnibus_gospel_save", "Save gospel"))
            .with_call(CallDescriptor::new("ranger.omnibus_connect_discovered", "Connect discovered peers"))
            // Tower
            .with_call(CallDescriptor::new("ranger.tower_status", "Tower status"))
            .with_call(CallDescriptor::new("ranger.tower_is_active", "Check if Tower active"))
            // Config
            .with_call(CallDescriptor::new("ranger.config_get", "Get daemon config"))
            .with_call(CallDescriptor::new("ranger.config_set", "Update daemon config"))
            // Identity
            .with_call(CallDescriptor::new("ranger.identity_update_name", "Update display name"))
            .with_call(CallDescriptor::new("ranger.identity_export_keyring", "Export keyring"))
            .with_call(CallDescriptor::new("ranger.identity_import_keyring", "Import keyring"))
            // Data
            .with_call(CallDescriptor::new("ranger.data_dir", "Data directory path"))
            // Events
            .with_emitted_event(EventDescriptor::new("ranger.home_node_set", "Home node changed"))
            .with_emitted_event(EventDescriptor::new("ranger.config_changed", "Config updated"))
            .with_emitted_event(EventDescriptor::new("ranger.identity_updated", "Identity changed"))
            .with_emitted_event(EventDescriptor::new("ranger.identity_imported", "Keyring imported"))
    }
}
