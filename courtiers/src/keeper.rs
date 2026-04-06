//! Keeper module — Sentinal encryption courtier.
//!
//! Exposes encryption, key derivation, key slots, recovery phrases,
//! and password strength as daemon operations. Keys stay in Rust —
//! programs get results, not raw key material.
//!
//! Most operations accept an idea_id and derive the needed key from
//! Vault's master key automatically. Programs never handle raw keys.

use std::sync::Arc;

use equipment::{CallDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct KeeperModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn guard_vault_unlocked(state: &DaemonState) -> Result<(), PhoneError> {
    let vault = state.vault.lock().unwrap();
    if !vault.is_unlocked() {
        return Err(err("keeper", "Vault is locked — unlock identity first"));
    }
    Ok(())
}

fn parse_id(params: &Value, op: &str) -> Result<Uuid, PhoneError> {
    let id_str = params.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| err(op, "missing 'id'"))?;
    Uuid::parse_str(id_str).map_err(|e| err(op, format!("invalid UUID: {e}")))
}

fn b64_decode(s: &str, op: &str) -> Result<Vec<u8>, PhoneError> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .map_err(|e| err(op, format!("invalid base64: {e}")))
}

fn b64_encode(data: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)
}

impl DaemonModule for KeeperModule {
    fn id(&self) -> &str { "keeper" }
    fn name(&self) -> &str { "Keeper (Sentinal)" }
    fn deps(&self) -> &[&str] { &["castellan"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── sentinal.encrypt ───────────────────────────────────
        // Encrypt data using a per-idea content key (derived from Vault master key).
        let s = state.clone();
        state.phone.register_raw("keeper.encrypt", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "keeper.encrypt")?;
            let plaintext_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.encrypt", "missing 'data' (base64)"))?;
            let plaintext = b64_decode(plaintext_b64, "keeper.encrypt")?;

            let mut vault = s.vault.lock().unwrap();
            let content_key = vault.content_key(&id)
                .map_err(|e| err("keeper.encrypt", e))?;

            let encrypted = sentinal::encryption::encrypt_combined(&plaintext, content_key.expose())
                .map_err(|e| err("keeper.encrypt", e))?;

            ok_json(&json!({ "encrypted": b64_encode(&encrypted) }))
        });

        // ── sentinal.decrypt ───────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("keeper.decrypt", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "keeper.decrypt")?;
            let encrypted_b64 = params.get("encrypted").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.decrypt", "missing 'encrypted' (base64)"))?;
            let encrypted = b64_decode(encrypted_b64, "keeper.decrypt")?;

            let mut vault = s.vault.lock().unwrap();
            let content_key = vault.content_key(&id)
                .map_err(|e| err("keeper.decrypt", e))?;

            let plaintext = sentinal::encryption::decrypt_combined(&encrypted, content_key.expose())
                .map_err(|e| err("keeper.decrypt", e))?;

            ok_json(&json!({ "data": b64_encode(&plaintext) }))
        });

        // ── sentinal.encrypt_with_aad ──────────────────────────
        let s = state.clone();
        state.phone.register_raw("keeper.encrypt_with_aad", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "keeper.encrypt_with_aad")?;
            let plaintext_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.encrypt_with_aad", "missing 'data'"))?;
            let aad_b64 = params.get("aad").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.encrypt_with_aad", "missing 'aad'"))?;
            let plaintext = b64_decode(plaintext_b64, "keeper.encrypt_with_aad")?;
            let aad = b64_decode(aad_b64, "keeper.encrypt_with_aad")?;

            let mut vault = s.vault.lock().unwrap();
            let content_key = vault.content_key(&id)
                .map_err(|e| err("keeper.encrypt_with_aad", e))?;

            let encrypted = sentinal::encryption::encrypt_with_aad(&plaintext, &aad, content_key.expose())
                .map_err(|e| err("keeper.encrypt_with_aad", e))?;

            ok_json(&json!({ "encrypted": b64_encode(&encrypted) }))
        });

        // ── sentinal.decrypt_with_aad ──────────────────────────
        let s = state.clone();
        state.phone.register_raw("keeper.decrypt_with_aad", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "keeper.decrypt_with_aad")?;
            let encrypted_b64 = params.get("encrypted").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.decrypt_with_aad", "missing 'encrypted'"))?;
            let aad_b64 = params.get("aad").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.decrypt_with_aad", "missing 'aad'"))?;
            let encrypted = b64_decode(encrypted_b64, "keeper.decrypt_with_aad")?;
            let aad = b64_decode(aad_b64, "keeper.decrypt_with_aad")?;

            let mut vault = s.vault.lock().unwrap();
            let content_key = vault.content_key(&id)
                .map_err(|e| err("keeper.decrypt_with_aad", e))?;

            let plaintext = sentinal::encryption::decrypt_with_aad(&encrypted, &aad, content_key.expose())
                .map_err(|e| err("keeper.decrypt_with_aad", e))?;

            ok_json(&json!({ "data": b64_encode(&plaintext) }))
        });

        // ── sentinal.key_slot_create_password ──────────────────
        // Create a password-protected key slot for sharing
        let s = state.clone();
        state.phone.register_raw("keeper.key_slot_create_password", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "keeper.key_slot_create_password")?;
            let password = params.get("password").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.key_slot_create_password", "missing 'password'"))?;

            let mut vault = s.vault.lock().unwrap();
            let content_key = vault.content_key(&id)
                .map_err(|e| err("keeper.key_slot_create_password", e))?;

            let slot = sentinal::PasswordKeySlot::create(content_key.expose(), password)
                .map_err(|e| err("keeper.key_slot_create_password", e))?;

            let slot_json = serde_json::to_value(&slot)
                .map_err(|e| err("keeper.key_slot_create_password", e))?;
            ok_json(&slot_json)
        });

        // ── sentinal.key_slot_create_public ────────────────────
        // Create a public-key key slot for sharing with a specific recipient
        let s = state.clone();
        state.phone.register_raw("keeper.key_slot_create_public", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "keeper.key_slot_create_public")?;
            let recipient_pubkey_b64 = params.get("recipient_pubkey").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.key_slot_create_public", "missing 'recipient_pubkey'"))?;
            let recipient_crown_id = params.get("recipient_crown_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.key_slot_create_public", "missing 'recipient_crown_id'"))?;

            let recipient_pubkey = b64_decode(recipient_pubkey_b64, "keeper.key_slot_create_public")?;
            let pubkey_arr: [u8; 32] = recipient_pubkey.try_into()
                .map_err(|_| err("keeper.key_slot_create_public", "recipient_pubkey must be 32 bytes"))?;

            let mut vault = s.vault.lock().unwrap();
            let content_key = vault.content_key(&id)
                .map_err(|e| err("keeper.key_slot_create_public", e))?;
            let slot = sentinal::PublicKeySlot::create(
                content_key.expose(),
                &pubkey_arr,
                recipient_crown_id,
            ).map_err(|e| err("keeper.key_slot_create_public", e))?;

            let slot_json = serde_json::to_value(&slot)
                .map_err(|e| err("keeper.key_slot_create_public", e))?;
            ok_json(&slot_json)
        });

        // ── sentinal.key_slot_unwrap_password ──────────────────
        // Unwrap a password-protected key slot
        state.phone.register_raw("keeper.key_slot_unwrap_password", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let slot_json = params.get("slot")
                .ok_or_else(|| err("keeper.key_slot_unwrap_password", "missing 'slot'"))?;
            let password = params.get("password").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.key_slot_unwrap_password", "missing 'password'"))?;

            let slot: sentinal::KeySlot = serde_json::from_value(slot_json.clone())
                .map_err(|e| err("keeper.key_slot_unwrap_password", e))?;

            let content_key = slot.unwrap(sentinal::KeySlotCredential::Password(password))
                .map_err(|e| err("keeper.key_slot_unwrap_password", e))?;

            ok_json(&json!({ "content_key": b64_encode(content_key.expose()) }))
        });

        // ── sentinal.key_slot_unwrap_private ───────────────────
        // Unwrap a public-key slot using the user's Crown private key.
        // Exports keyring from Omnibus → deserializes → gets primary private key.
        let s = state.clone();
        state.phone.register_raw("keeper.key_slot_unwrap_private", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let slot_json = params.get("slot")
                .ok_or_else(|| err("keeper.key_slot_unwrap_private", "missing 'slot'"))?;

            let slot: sentinal::KeySlot = serde_json::from_value(slot_json.clone())
                .map_err(|e| err("keeper.key_slot_unwrap_private", e))?;

            // Get private key: export keyring bytes → deserialize → primary keypair
            let omnibus = s.omnibus.omnibus();
            let keyring_bytes = omnibus.export_keyring()
                .map_err(|e| err("keeper.key_slot_unwrap_private", format!("export keyring: {e}")))?;
            let mut keyring = crown::Keyring::new();
            keyring.load(&keyring_bytes)
                .map_err(|e| err("keeper.key_slot_unwrap_private", format!("load keyring: {e}")))?;
            let keypair = keyring.primary_keypair()
                .ok_or_else(|| err("keeper.key_slot_unwrap_private", "no primary keypair"))?;
            let private_key = keypair.private_key_data()
                .ok_or_else(|| err("keeper.key_slot_unwrap_private", "no private key in keypair"))?;

            let content_key = slot.unwrap(sentinal::KeySlotCredential::PrivateKey(private_key))
                .map_err(|e| err("keeper.key_slot_unwrap_private", e))?;

            ok_json(&json!({ "content_key": b64_encode(content_key.expose()) }))
        });

        // ── sentinal.recovery_generate ─────────────────────────
        // Generate a 24-word BIP-39 recovery phrase
        state.phone.register_raw("keeper.recovery_generate", move |_data| {
            let words = sentinal::recovery::generate_phrase()
                .map_err(|e| err("keeper.recovery_generate", e))?;
            ok_json(&json!({ "words": words }))
        });

        // ── sentinal.recovery_validate ─────────────────────────
        state.phone.register_raw("keeper.recovery_validate", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let words: Vec<String> = params.get("words")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .ok_or_else(|| err("keeper.recovery_validate", "missing 'words' array"))?;

            let valid = sentinal::recovery::validate_phrase(&words);
            ok_json(&json!({ "valid": valid }))
        });

        // ── sentinal.recovery_to_seed ──────────────────────────
        state.phone.register_raw("keeper.recovery_to_seed", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let words: Vec<String> = params.get("words")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .ok_or_else(|| err("keeper.recovery_to_seed", "missing 'words' array"))?;
            let passphrase = params.get("passphrase").and_then(|v| v.as_str()).unwrap_or("");

            let seed = sentinal::recovery::phrase_to_seed(&words, passphrase)
                .map_err(|e| err("keeper.recovery_to_seed", e))?;

            ok_json(&json!({ "seed": b64_encode(seed.expose()) }))
        });

        // ── sentinal.password_strength ─────────────────────────
        // Stateless: estimates password strength. No key material involved.
        state.phone.register_raw("keeper.password_strength", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let password = params.get("password").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.password_strength", "missing 'password'"))?;

            let strength = sentinal::password_strength::estimate_strength(password);
            let strength_json = serde_json::to_value(&strength)
                .map_err(|e| err("keeper.password_strength", e))?;
            ok_json(&strength_json)
        });

        // ── sentinal.generate_salt ─────────────────────────────
        state.phone.register_raw("keeper.generate_salt", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let length = params.get("length").and_then(|v| v.as_u64()).unwrap_or(32) as usize;

            let salt = sentinal::key_derivation::generate_salt(length)
                .map_err(|e| err("keeper.generate_salt", e))?;

            ok_json(&json!({ "salt": b64_encode(&salt) }))
        });

        // ── sentinal.pad ───────────────────────────────────────
        state.phone.register_raw("keeper.pad", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let data_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.pad", "missing 'data'"))?;
            let block_size = params.get("block_size").and_then(|v| v.as_u64())
                .ok_or_else(|| err("keeper.pad", "missing 'block_size'"))? as usize;
            let raw = b64_decode(data_b64, "keeper.pad")?;

            let padded = sentinal::padding::pad_to_multiple(&raw, block_size);
            ok_json(&json!({ "data": b64_encode(&padded) }))
        });

        // ── sentinal.unpad ─────────────────────────────────────
        state.phone.register_raw("keeper.unpad", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let data_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.unpad", "missing 'data'"))?;
            let raw = b64_decode(data_b64, "keeper.unpad")?;

            let unpadded = sentinal::padding::unpad_from_multiple(&raw)
                .map_err(|e| err("keeper.unpad", e))?;
            ok_json(&json!({ "data": b64_encode(&unpadded) }))
        });

        // ── sentinal.onion_wrap ────────────────────────────────
        state.phone.register_raw("keeper.onion_wrap", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let plaintext_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.onion_wrap", "missing 'data'"))?;
            let relay_pubkey_b64 = params.get("relay_pubkey").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.onion_wrap", "missing 'relay_pubkey'"))?;
            let plaintext = b64_decode(plaintext_b64, "keeper.onion_wrap")?;
            let relay_pubkey = b64_decode(relay_pubkey_b64, "keeper.onion_wrap")?;

            let blob = sentinal::onion::wrap_layer(&plaintext, &relay_pubkey)
                .map_err(|e| err("keeper.onion_wrap", e))?;
            ok_json(&json!({ "blob": b64_encode(&blob) }))
        });

        // ── sentinal.onion_unwrap ──────────────────────────────
        state.phone.register_raw("keeper.onion_unwrap", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let blob_b64 = params.get("blob").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.onion_unwrap", "missing 'blob'"))?;
            let relay_privkey_b64 = params.get("relay_privkey").and_then(|v| v.as_str())
                .ok_or_else(|| err("keeper.onion_unwrap", "missing 'relay_privkey'"))?;
            let blob = b64_decode(blob_b64, "keeper.onion_unwrap")?;
            let relay_privkey = b64_decode(relay_privkey_b64, "keeper.onion_unwrap")?;

            let plaintext = sentinal::onion::unwrap_layer(&blob, &relay_privkey)
                .map_err(|e| err("keeper.onion_unwrap", e))?;
            ok_json(&json!({ "data": b64_encode(&plaintext) }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Encryption
            .with_call(CallDescriptor::new("keeper.encrypt", "Encrypt with per-idea key"))
            .with_call(CallDescriptor::new("keeper.decrypt", "Decrypt with per-idea key"))
            .with_call(CallDescriptor::new("keeper.encrypt_with_aad", "Encrypt with AAD"))
            .with_call(CallDescriptor::new("keeper.decrypt_with_aad", "Decrypt with AAD"))
            // Key slots
            .with_call(CallDescriptor::new("keeper.key_slot_create_password", "Create password key slot"))
            .with_call(CallDescriptor::new("keeper.key_slot_create_public", "Create public-key slot"))
            .with_call(CallDescriptor::new("keeper.key_slot_unwrap_password", "Unwrap password slot"))
            .with_call(CallDescriptor::new("keeper.key_slot_unwrap_private", "Unwrap with private key"))
            // Recovery
            .with_call(CallDescriptor::new("keeper.recovery_generate", "Generate recovery phrase"))
            .with_call(CallDescriptor::new("keeper.recovery_validate", "Validate recovery phrase"))
            .with_call(CallDescriptor::new("keeper.recovery_to_seed", "Convert phrase to seed"))
            // Password
            .with_call(CallDescriptor::new("keeper.password_strength", "Estimate password strength"))
            // Utility
            .with_call(CallDescriptor::new("keeper.generate_salt", "Generate random salt"))
            .with_call(CallDescriptor::new("keeper.pad", "PKCS#7 pad"))
            .with_call(CallDescriptor::new("keeper.unpad", "PKCS#7 unpad"))
            // Onion
            .with_call(CallDescriptor::new("keeper.onion_wrap", "Onion-wrap for relay"))
            .with_call(CallDescriptor::new("keeper.onion_unwrap", "Onion-unwrap from relay"))
    }
}
