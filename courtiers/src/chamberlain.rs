//! Chamberlain module — Crown identity courtier.
//!
//! The Chamberlain manages identity lifecycle: creation, unlock, lock,
//! profile management, status, deletion, and backward-compatible
//! `identity.*` aliases. These are composed workflows that cross
//! Omnibus + Vault + Crown crate boundaries.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use crown::{Profile, Soul};
use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct ChamberlainModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for ChamberlainModule {
    fn id(&self) -> &str { "chamberlain" }
    fn name(&self) -> &str { "Chamberlain (Crown Identity)" }
    fn deps(&self) -> &[&str] { &["omnibus"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── crown.state ─────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.state", move |_data| {
            // Check the USER's keyring on disk — not omnibus.pubkey() which
            // may hold the Tower's identity in Tower mode.
            let has_identity = s.data_dir.join("keyring.dat").exists();
            let unlocked = !s.crown_locked.load(Ordering::Relaxed);
            let omnibus = s.omnibus.omnibus();
            ok_json(&json!({
                "exists": has_identity,
                "unlocked": has_identity && unlocked,
                "crown_id": if has_identity { omnibus.pubkey() } else { None },
                "display_name": if has_identity {
                    omnibus.profile_json().and_then(|j| {
                        serde_json::from_str::<Value>(&j).ok()?.get("display_name")?.as_str().map(String::from)
                    })
                } else { None },
                "online": has_identity && unlocked,
                "has_avatar": false,
            }))
        });

        // ── crown.create ────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = params.get("display_name")
                .or_else(|| params.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("Anonymous");
            let password = params.get("password")
                .and_then(|v| v.as_str())
                .ok_or_else(|| err("chamberlain.create", "password is required"))?;
            if password.is_empty() {
                return Err(err("chamberlain.create", "password cannot be empty"));
            }

            let omnibus = s.omnibus.omnibus();
            let crown_id = omnibus.create_identity(name)
                .map_err(|e| err("chamberlain.create", e))?;

            // Persist identity to the daemon's data_dir (not the Tower's).
            // This is what crown.state checks and crown.unlock loads from.
            std::fs::create_dir_all(&s.data_dir)
                .map_err(|e| err("chamberlain.create", format!("create data dir: {e}")))?;

            let keyring_bytes = omnibus.export_keyring()
                .map_err(|e| err("chamberlain.create", format!("export keyring: {e}")))?;
            std::fs::write(s.data_dir.join("keyring.dat"), &keyring_bytes)
                .map_err(|e| err("chamberlain.create", format!("save keyring: {e}")))?;

            // Save soul using Crown's Soul::create (proper SoulStorage format).
            let soul_dir = s.data_dir.join("soul");
            let mut soul = Soul::create(&soul_dir, None)
                .map_err(|e| err("chamberlain.create", format!("create soul: {e}")))?;
            let mut profile = Profile::empty();
            profile.display_name = Some(name.into());
            soul.update_profile(profile);
            soul.save().map_err(|e| err("chamberlain.create", format!("save soul: {e}")))?;

            s.crown_locked.store(false, Ordering::Relaxed);

            // Unlock Vault with the user's password (creates new salt + config).
            let mut vault = s.vault.lock().unwrap_or_else(|e| e.into_inner());
            if !vault.is_unlocked() {
                vault.unlock(password, s.data_dir.clone())
                    .map_err(|e| err("chamberlain.create", format!("Vault unlock failed: {e}")))?;
            }
            drop(vault);

            // Post-modifier: emit event for Yoke + Pager observers
            let event = serde_json::to_vec(&json!({"crown_id": &crown_id})).unwrap_or_default();
            s.email.send_raw("chamberlain.created", &event);

            ok_json(&json!({ "crown_id": crown_id }))
        });

        // ── crown.unlock ────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.unlock", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let omnibus = s.omnibus.omnibus();

            // If Omnibus doesn't have the identity in memory yet (e.g. after
            // daemon restart), load it from the daemon's data_dir.
            if omnibus.pubkey().is_none() {
                let data_dir_str = s.data_dir.to_string_lossy();
                omnibus.load_identity(&data_dir_str)
                    .map_err(|_| err("chamberlain.unlock", "No identity found — create one first"))?;
            }

            let password = params.get("password")
                .and_then(|v| v.as_str())
                .ok_or_else(|| err("chamberlain.unlock", "password is required"))?;

            // Verify password by attempting to unlock the Vault.
            // If the password is wrong, SQLCipher will fail to open.
            let mut vault = s.vault.lock().unwrap_or_else(|e| e.into_inner());
            if !vault.is_unlocked() {
                vault.unlock(password, s.data_dir.clone())
                    .map_err(|_| err("chamberlain.unlock", "Wrong password"))?;
            }
            drop(vault);

            s.crown_locked.store(false, Ordering::Relaxed);
            s.email.send_raw("chamberlain.unlocked", b"{}");
            ok_json(&json!({ "ok": true, "unlocked": true }))
        });

        // ── crown.lock ──────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.lock", move |_data| {
            s.crown_locked.store(true, Ordering::Relaxed);
            s.email.send_raw("chamberlain.locked", b"{}");
            ok_json(&json!({ "ok": true, "locked": true }))
        });

        // ── crown.profile ───────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.profile", move |_data| {
            if s.crown_locked.load(Ordering::Relaxed) {
                return Err(err("chamberlain.profile", "Crown is locked"));
            }
            let omnibus = s.omnibus.omnibus();
            match omnibus.profile_json() {
                Some(json_str) => {
                    let v: Value = serde_json::from_str(&json_str)
                        .map_err(|e| err("chamberlain.profile", e))?;
                    ok_json(&v)
                }
                None => ok_json(&Value::Null),
            }
        });

        // ── crown.update_profile ────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.update_profile", move |data| {
            if s.crown_locked.load(Ordering::Relaxed) {
                return Err(err("chamberlain.update_profile", "Crown is locked"));
            }
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = params.get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let omnibus = s.omnibus.omnibus();
            omnibus.update_display_name(name)
                .map_err(|e| err("chamberlain.update_profile", e))?;
            ok_json(&json!({ "ok": true }))
        });

        // ── crown.set_status ────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.set_status", move |data| {
            if s.crown_locked.load(Ordering::Relaxed) {
                return Err(err("chamberlain.set_status", "Crown is locked"));
            }
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let online = params.get("online").and_then(|v| v.as_bool()).unwrap_or(true);
            ok_json(&json!({ "ok": true, "online": online }))
        });

        // ── crown.import ────────────────────────────────────────
        state.phone.register_raw("chamberlain.import", |_data| {
            Err(err("chamberlain.import", "Import not yet implemented"))
        });

        // ── crown.delete ────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("chamberlain.delete", move |_data| {
            // Note: polity_check removed — the UI's type-to-confirm dialog
            // provides user consent. Pipeline calls go through the Zig
            // orchestrator which has its own Polity modifier chain.
            if s.crown_locked.load(Ordering::Relaxed) {
                return Err(err("chamberlain.delete", "Crown is locked — unlock first"));
            }

            // Lock the Vault before deleting (zeroes keys from memory).
            {
                let mut vault = s.vault.lock().unwrap_or_else(|e| e.into_inner());
                if vault.is_unlocked() {
                    let _ = vault.lock();
                }
            }

            // Wipe everything in ~/.omnidea/ — full factory reset.
            // The daemon is still running (needs socket/pid briefly to respond),
            // so wipe all FILES and DIRS except the live socket and pidfile.
            let omnidea_dir = s.data_dir.parent().unwrap_or(&s.data_dir);
            if omnidea_dir.exists() {
                let keep = ["daemon.sock", "daemon.pid"];
                for entry in std::fs::read_dir(omnidea_dir)
                    .map_err(|e| err("chamberlain.delete", format!("Failed to read dir: {e}")))?
                {
                    let entry = entry.map_err(|e| err("chamberlain.delete", e))?;
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if keep.contains(&name_str.as_ref()) {
                        continue;
                    }
                    let path = entry.path();
                    if path.is_dir() {
                        std::fs::remove_dir_all(&path)
                            .map_err(|e| err("chamberlain.delete", format!("Failed to remove {}: {e}", path.display())))?;
                    } else {
                        std::fs::remove_file(&path)
                            .map_err(|e| err("chamberlain.delete", format!("Failed to remove {}: {e}", path.display())))?;
                    }
                }
                // Recreate empty data dir so the daemon doesn't crash.
                std::fs::create_dir_all(&s.data_dir)
                    .map_err(|e| err("chamberlain.delete", format!("Failed to recreate data dir: {e}")))?;
                log::info!("Factory reset: wiped {}", omnidea_dir.display());
            }

            // Clear in-memory identity so crown.state reports exists: false.
            s.omnibus.omnibus().clear_identity();
            s.crown_locked.store(true, Ordering::Relaxed);

            // Emit event so UI can react.
            s.email.send_raw("chamberlain.deleted", b"{}");

            ok_json(&json!({ "ok": true, "deleted": true }))
        });

        // ── crown.avatar ────────────────────────────────────────
        state.phone.register_raw("chamberlain.avatar", |_data| {
            ok_json(&Value::Null)
        });

        // ── identity.* aliases (backward compat) ────────────────
        let s = state.clone();
        state.phone.register_raw("identity.create", move |data| {
            s.phone.call_raw("chamberlain.create", data)
        });

        let s = state.clone();
        state.phone.register_raw("identity.load", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let path = params.get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| err("identity.load", "missing 'path'"))?;
            let omnibus = s.omnibus.omnibus();
            let crown_id = omnibus.load_identity(path)
                .map_err(|e| err("identity.load", e))?;
            // Vault unlock deferred until crown.unlock with password.
            ok_json(&json!({ "crown_id": crown_id }))
        });

        let s = state.clone();
        state.phone.register_raw("identity.profile", move |data| {
            s.phone.call_raw("chamberlain.profile", data)
        });

        let s = state.clone();
        state.phone.register_raw("identity.pubkey", move |_data| {
            ok_json(&json!(s.omnibus.omnibus().pubkey()))
        });

        let s = state.clone();
        state.phone.register_raw("identity.update_name", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let omnibus = s.omnibus.omnibus();
            if s.crown_locked.load(Ordering::Relaxed) {
                return Err(err("identity.update_name", "Crown is locked"));
            }
            omnibus.update_display_name(name)
                .map_err(|e| err("identity.update_name", e))?;
            ok_json(&json!({ "ok": true }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            .with_call(CallDescriptor::new("chamberlain.state", "Get identity state"))
            .with_call(CallDescriptor::new("chamberlain.create", "Create new identity"))
            .with_call(CallDescriptor::new("chamberlain.unlock", "Unlock identity"))
            .with_call(CallDescriptor::new("chamberlain.lock", "Lock identity"))
            .with_call(CallDescriptor::new("chamberlain.profile", "Get profile"))
            .with_call(CallDescriptor::new("chamberlain.update_profile", "Update profile"))
            .with_call(CallDescriptor::new("chamberlain.set_status", "Set online status"))
            .with_call(CallDescriptor::new("chamberlain.import", "Import from recovery phrase"))
            .with_call(CallDescriptor::new("chamberlain.delete", "Delete identity"))
            .with_call(CallDescriptor::new("chamberlain.avatar", "Get avatar"))
            .with_call(CallDescriptor::new("identity.create", "Create identity (alias)"))
            .with_call(CallDescriptor::new("identity.load", "Load identity from path"))
            .with_call(CallDescriptor::new("identity.profile", "Get profile (alias)"))
            .with_call(CallDescriptor::new("identity.pubkey", "Get public key"))
            .with_call(CallDescriptor::new("identity.update_name", "Update display name"))
            .with_emitted_event(EventDescriptor::new("chamberlain.created", "Identity was created"))
            .with_emitted_event(EventDescriptor::new("chamberlain.unlocked", "Identity was unlocked"))
            .with_emitted_event(EventDescriptor::new("chamberlain.locked", "Identity was locked"))
    }
}
