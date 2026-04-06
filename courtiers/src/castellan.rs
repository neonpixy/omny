//! Castellan module — Vault encrypted storage courtier.
//!
//! Exposes vault lifecycle (status, lock, unlock) and full-text search
//! as daemon operations. Programs use courtier name "castellan" which
//! maps to daemon namespace "vault".
//!
//! These are composed workflow handlers that coordinate Vault state,
//! Email events, and API JSON serialization in a single place.

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct CastellanModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for CastellanModule {
    fn id(&self) -> &str { "castellan" }
    fn name(&self) -> &str { "Castellan (Vault)" }
    fn deps(&self) -> &[&str] { &["chamberlain"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── vault.status ──────────────────────────────────────────
        // Vault lock state + idea count.
        let s = state.clone();
        state.phone.register_raw("castellan.status", move |_data| {
            let vault = s.vault.lock().unwrap();
            let count = vault.idea_count().unwrap_or(0);
            ok_json(&json!({
                "unlocked": vault.is_unlocked(),
                "idea_count": count,
            }))
        });

        // ── vault.lock ────────────────────────────────────────────
        // Lock the vault and emit an event.
        let s = state.clone();
        state.phone.register_raw("castellan.lock", move |_data| {
            let mut vault = s.vault.lock().unwrap();
            vault.lock().map_err(|e| err("castellan.lock", e))?;
            s.email.send_raw("castellan.locked", b"{}");
            ok_json(&json!({"ok": true}))
        });

        // ── vault.unlock ──────────────────────────────────────────
        // Unlock the vault with a password and emit an event.
        let s = state.clone();
        state.phone.register_raw("castellan.unlock", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let default_pw = std::env::var("VAULT_PASSWORD")
                .unwrap_or_else(|_| "omnidea-vault-local".into());
            let password = params.get("password")
                .and_then(|v| v.as_str())
                .unwrap_or(&default_pw);
            let mut vault = s.vault.lock().unwrap();
            let data_dir = s.data_dir.clone();
            vault.unlock(password, data_dir).map_err(|e| err("castellan.unlock", e))?;
            s.email.send_raw("castellan.unlocked", b"{}");
            ok_json(&json!({"ok": true}))
        });

        // ── vault.search ──────────────────────────────────────────
        // Full-text search across all ideas in the vault.
        let s = state.clone();
        state.phone.register_raw("castellan.search", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let query = params.get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let limit = params.get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(20) as usize;
            let vault = s.vault.lock().unwrap();
            let hits = vault.search(query, limit)
                .map_err(|e| err("castellan.search", e))?;
            let hits_json: Vec<Value> = hits.iter()
                .map(prerogative::api_json::search_hit_json)
                .collect();
            ok_json(&Value::Array(hits_json))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            .with_call(CallDescriptor::new("castellan.status", "Vault lock state + idea count"))
            .with_call(CallDescriptor::new("castellan.lock", "Lock vault"))
            .with_call(CallDescriptor::new("castellan.unlock", "Unlock vault"))
            .with_call(CallDescriptor::new("castellan.search", "Full-text search"))
            .with_emitted_event(EventDescriptor::new("castellan.locked", "Vault was locked"))
            .with_emitted_event(EventDescriptor::new("castellan.unlocked", "Vault was unlocked"))
    }
}
