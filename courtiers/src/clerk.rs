//! Clerk module — Hall file I/O courtier.
//!
//! Exposes .idea package reading/writing and the asset pipeline
//! as daemon operations. Programs pass idea IDs; the Clerk resolves
//! paths and encryption keys from Vault automatically.
//!
//! Overrides auto-generated Hall FFI handlers with proper Rust→Rust
//! calls that compose Vault key lookups + Hall I/O in a single operation.

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct ClerkModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn guard_vault_unlocked(state: &DaemonState) -> Result<(), PhoneError> {
    let vault = state.vault.lock().unwrap();
    if !vault.is_unlocked() {
        return Err(err("clerk", "Vault is locked — unlock identity first"));
    }
    Ok(())
}

/// Look up idea path and encryption keys from Vault.
fn resolve_idea(state: &DaemonState, id: &Uuid) -> Result<(std::path::PathBuf, Vec<u8>, Vec<u8>), PhoneError> {
    let mut vault = state.vault.lock().unwrap();
    let entry = vault.get_idea(id)
        .map_err(|e| err("clerk", e))?
        .ok_or_else(|| err("clerk", "idea not found"))?;
    let path = std::path::PathBuf::from(&entry.path);
    let content_key = vault.content_key(id)
        .map_err(|e| err("clerk", e))?;
    let vocab_seed = vault.vocabulary_seed()
        .map_err(|e| err("clerk", e))?;
    Ok((path, content_key.expose().to_vec(), vocab_seed.expose().to_vec()))
}

fn parse_id(params: &Value, op: &str) -> Result<Uuid, PhoneError> {
    let id_str = params.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| err(op, "missing 'id'"))?;
    Uuid::parse_str(id_str).map_err(|e| err(op, format!("invalid UUID: {e}")))
}

impl DaemonModule for ClerkModule {
    fn id(&self) -> &str { "clerk" }
    fn name(&self) -> &str { "Clerk (Hall I/O)" }
    fn deps(&self) -> &[&str] { &["castellan"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── hall.is_idea_package ────────────────────────────────
        state.phone.register_raw("clerk.is_idea_package", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let path = params.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.is_idea_package", "missing 'path'"))?;
            let is_valid = hall::scholar::is_idea_package(std::path::Path::new(path));
            ok_json(&json!({ "valid": is_valid }))
        });

        // ── hall.read_header ───────────────────────────────────
        // Reads just the header — no key needed (sovereignty: headers are public)
        state.phone.register_raw("clerk.read_header", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let path = params.get("path").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.read_header", "missing 'path'"))?;
            let header = hall::scholar::read_header(std::path::Path::new(path))
                .map_err(|e| err("clerk.read_header", e))?;
            let header_json = serde_json::to_value(&header)
                .map_err(|e| err("clerk.read_header", e))?;
            ok_json(&header_json)
        });

        // ── hall.read ──────────────────────────────────────────
        // Full package read: resolves key from Vault by idea ID
        let s = state.clone();
        state.phone.register_raw("clerk.read", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.read")?;
            let (path, content_key, vocab_seed) = resolve_idea(&s, &id)?;

            let read_result = hall::scholar::read(&path, &content_key, Some(&vocab_seed))
                .map_err(|e| err("clerk.read", e))?;

            let package_json = prerogative::api_json::idea_package_json(&read_result.value);

            // Include warnings if any
            if read_result.has_warnings() {
                let warnings: Vec<String> = read_result.warnings.iter()
                    .map(|w| format!("{w:?}"))
                    .collect();
                ok_json(&json!({ "package": package_json, "warnings": warnings }))
            } else {
                ok_json(&package_json)
            }
        });

        // ── hall.write ─────────────────────────────────────────
        // Package write: resolves key from Vault by idea ID
        let s = state.clone();
        state.phone.register_raw("clerk.write", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.write")?;
            let (path, content_key, vocab_seed) = resolve_idea(&s, &id)?;

            // Load existing, apply changes, write back (same load-merge-write as ideas_mod)
            let read_result = hall::scholar::read(&path, &content_key, Some(&vocab_seed))
                .map_err(|e| err("clerk.write", e))?;
            let package = read_result.value;

            hall::scribe::write(&package, &content_key, Some(&vocab_seed))
                .map_err(|e| err("clerk.write", e))?;

            s.email.send_raw("clerk.written", &serde_json::to_vec(&json!({"id": id.to_string()})).unwrap_or_default());
            ok_json(&json!({ "ok": true }))
        });

        // ── hall.asset_import ──────────────────────────────────
        // Import raw bytes as an encrypted asset. Returns SHA-256 hash.
        let s = state.clone();
        state.phone.register_raw("clerk.asset_import", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.asset_import")?;
            let data_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.asset_import", "missing 'data' (base64)"))?;
            let raw_bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD, data_b64
            ).map_err(|e| err("clerk.asset_import", format!("invalid base64: {e}")))?;

            let (path, content_key, vocab_seed) = resolve_idea(&s, &id)?;
            let hash = hall::archivist::import(&raw_bytes, &path, &content_key, &vocab_seed)
                .map_err(|e| err("clerk.asset_import", e))?;

            ok_json(&json!({ "hash": hash }))
        });

        // ── hall.asset_read ────────────────────────────────────
        // Read + decrypt an asset by hash. Returns base64 data.
        let s = state.clone();
        state.phone.register_raw("clerk.asset_read", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.asset_read")?;
            let hash = params.get("hash").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.asset_read", "missing 'hash'"))?;

            let (path, content_key, vocab_seed) = resolve_idea(&s, &id)?;
            let bytes = hall::archivist::read(hash, &path, &content_key, &vocab_seed)
                .map_err(|e| err("clerk.asset_read", e))?;

            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
            ok_json(&json!({ "data": encoded }))
        });

        // ── hall.asset_export ──────────────────────────────────
        // Export an asset to a destination path on disk.
        let s = state.clone();
        state.phone.register_raw("clerk.asset_export", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.asset_export")?;
            let hash = params.get("hash").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.asset_export", "missing 'hash'"))?;
            let dest = params.get("destination").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.asset_export", "missing 'destination'"))?;

            let (path, content_key, vocab_seed) = resolve_idea(&s, &id)?;
            hall::archivist::export(hash, &path, std::path::Path::new(dest), &content_key, &vocab_seed)
                .map_err(|e| err("clerk.asset_export", e))?;

            ok_json(&json!({ "ok": true }))
        });

        // ── hall.asset_list ────────────────────────────────────
        // List all asset hashes in an .idea package.
        let s = state.clone();
        state.phone.register_raw("clerk.asset_list", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.asset_list")?;

            let vault = s.vault.lock().unwrap();
            let entry = vault.get_idea(&id)
                .map_err(|e| err("clerk.asset_list", e))?
                .ok_or_else(|| err("clerk.asset_list", "idea not found"))?;
            let path = std::path::PathBuf::from(&entry.path);

            let hashes = hall::archivist::list(&path)
                .map_err(|e| err("clerk.asset_list", e))?;

            ok_json(&json!(hashes))
        });

        // ── hall.asset_exists ──────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("clerk.asset_exists", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.asset_exists")?;

            let hash = params.get("hash").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.asset_exists", "missing 'hash'"))?;

            let vault = s.vault.lock().unwrap();
            let entry = vault.get_idea(&id)
                .map_err(|e| err("clerk.asset_exists", e))?
                .ok_or_else(|| err("clerk.asset_exists", "idea not found"))?;
            let path = std::path::PathBuf::from(&entry.path);

            let exists = hall::archivist::exists(hash, &path);
            ok_json(&json!({ "exists": exists }))
        });

        // ── hall.asset_delete ──────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("clerk.asset_delete", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "clerk.asset_delete")?;
            let hash = params.get("hash").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.asset_delete", "missing 'hash'"))?;

            let vault = s.vault.lock().unwrap();
            let entry = vault.get_idea(&id)
                .map_err(|e| err("clerk.asset_delete", e))?
                .ok_or_else(|| err("clerk.asset_delete", "idea not found"))?;
            let path = std::path::PathBuf::from(&entry.path);

            hall::archivist::delete(hash, &path)
                .map_err(|e| err("clerk.asset_delete", e))?;

            ok_json(&json!({ "ok": true }))
        });

        // ── hall.extract_image_metadata ────────────────────────
        // Stateless: doesn't need Vault. Takes raw image bytes, returns metadata + blurhash.
        state.phone.register_raw("clerk.extract_image_metadata", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let data_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("clerk.extract_image_metadata", "missing 'data' (base64)"))?;
            let raw_bytes = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD, data_b64
            ).map_err(|e| err("clerk.extract_image_metadata", format!("invalid base64: {e}")))?;

            let metadata = hall::media_utils::extract_image_metadata(&raw_bytes)
                .map_err(|e| err("clerk.extract_image_metadata", e))?;
            let metadata_json = serde_json::to_value(&metadata)
                .map_err(|e| err("clerk.extract_image_metadata", e))?;
            ok_json(&metadata_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Package I/O
            .with_call(CallDescriptor::new("clerk.is_idea_package", "Validate .idea directory"))
            .with_call(CallDescriptor::new("clerk.read_header", "Read .idea header (no key)"))
            .with_call(CallDescriptor::new("clerk.read", "Read full .idea package"))
            .with_call(CallDescriptor::new("clerk.write", "Write .idea package"))
            // Asset pipeline
            .with_call(CallDescriptor::new("clerk.asset_import", "Import bytes as encrypted asset"))
            .with_call(CallDescriptor::new("clerk.asset_read", "Read + decrypt asset by hash"))
            .with_call(CallDescriptor::new("clerk.asset_export", "Export asset to file"))
            .with_call(CallDescriptor::new("clerk.asset_list", "List asset hashes"))
            .with_call(CallDescriptor::new("clerk.asset_exists", "Check if asset exists"))
            .with_call(CallDescriptor::new("clerk.asset_delete", "Delete asset by hash"))
            // Media
            .with_call(CallDescriptor::new("clerk.extract_image_metadata", "Extract image metadata + blurhash"))
            // Events
            .with_emitted_event(EventDescriptor::new("clerk.written", "Package was written to disk"))
    }
}
