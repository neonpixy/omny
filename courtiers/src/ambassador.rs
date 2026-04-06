//! Ambassador module — Nexus federation & interop courtier.
//!
//! Exposes export (15 formats), import (7 formats), protocol bridges,
//! federation scope management, and export profiles as daemon operations.
//! Export operations accept Digits as JSON; the Ambassador serializes
//! output as base64. Import operations accept base64 file data and
//! return Digits as JSON.

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct AmbassadorModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn guard_vault_unlocked(state: &DaemonState) -> Result<(), PhoneError> {
    let vault = state.vault.lock().unwrap();
    if !vault.is_unlocked() {
        return Err(err("ambassador", "Vault is locked — unlock identity first"));
    }
    Ok(())
}

fn parse_uuid(params: &Value, key: &str, op: &str) -> Result<Option<Uuid>, PhoneError> {
    match params.get(key).and_then(|v| v.as_str()) {
        Some(s) => {
            let id = Uuid::parse_str(s).map_err(|e| err(op, format!("invalid UUID '{key}': {e}")))?;
            Ok(Some(id))
        }
        None => Ok(None),
    }
}

fn b64_encode(data: &[u8]) -> String {
    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, data)
}

fn b64_decode(s: &str, op: &str) -> Result<Vec<u8>, PhoneError> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
        .map_err(|e| err(op, format!("invalid base64: {e}")))
}

impl DaemonModule for AmbassadorModule {
    fn id(&self) -> &str { "ambassador" }
    fn name(&self) -> &str { "Ambassador (Nexus)" }
    fn deps(&self) -> &[&str] { &["castellan"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── nexus.supported_export_formats ─────────────────────
        // Stateless: list all export formats supported by the default registry.
        state.phone.register_raw("ambassador.supported_export_formats", move |_data| {
            let registry = nexus::ExporterRegistry::with_defaults();
            let formats = registry.supported_formats();
            let format_list: Vec<Value> = formats.iter().map(|f| {
                json!({
                    "format": format!("{f:?}"),
                    "extension": f.extension(),
                    "mime_type": f.mime_type(),
                })
            }).collect();
            ok_json(&json!({ "formats": format_list }))
        });

        // ── nexus.supported_import_formats ─────────────────────
        // Stateless: list all importers and their MIME types.
        state.phone.register_raw("ambassador.supported_import_formats", move |_data| {
            let registry = nexus::ImporterRegistry::with_defaults();
            let ids = registry.list();
            let importer_list: Vec<Value> = ids.iter().map(|id| {
                json!({ "id": id })
            }).collect();
            ok_json(&json!({
                "importers": importer_list,
                "count": ids.len(),
            }))
        });

        // ── nexus.supported_bridges ───────────────────────────
        // Stateless: list registered protocol bridges.
        state.phone.register_raw("ambassador.supported_bridges", move |_data| {
            let registry = nexus::BridgeRegistry::with_defaults();
            let ids = registry.list();
            ok_json(&json!({
                "bridges": ids,
                "count": ids.len(),
            }))
        });

        // ── nexus.export ──────────────────────────────────────
        // Export Digits to a legacy format. Accepts digits as JSON array,
        // returns output as base64 with metadata.
        let s = state.clone();
        state.phone.register_raw("ambassador.export", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);

            let digits: Vec<ideas::Digit> = serde_json::from_value(
                params.get("digits").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("ambassador.export", format!("invalid digits: {e}")))?;

            let root_id = parse_uuid(&params, "root_id", "ambassador.export")?;

            let format_str = params.get("format").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.export", "missing 'format'"))?;
            let format: nexus::ExportFormat = serde_json::from_value(json!(format_str))
                .map_err(|e| err("ambassador.export", format!("invalid format '{format_str}': {e}")))?;

            let quality_str = params.get("quality").and_then(|v| v.as_str());
            let quality = match quality_str {
                Some(q) => serde_json::from_value::<nexus::ExportQuality>(json!(q))
                    .unwrap_or_default(),
                None => nexus::ExportQuality::default(),
            };

            let config = nexus::ExportConfig::new(format)
                .with_quality(quality);

            let registry = nexus::ExporterRegistry::with_defaults();
            let output = registry.export(&digits, root_id, &config)
                .map_err(|e| err("ambassador.export", e))?;

            s.email.send_raw("ambassador.exported", &serde_json::to_vec(&json!({
                "format": format_str,
                "size": output.size(),
                "filename": &output.filename,
            })).unwrap_or_default());

            ok_json(&json!({
                "data": b64_encode(&output.data),
                "filename": output.filename,
                "mime_type": output.mime_type,
                "size": output.size(),
            }))
        });

        // ── nexus.import ──────────────────────────────────────
        // Import external file data into Digits. Accepts base64 file data
        // and MIME type, returns Digits as JSON.
        let s = state.clone();
        state.phone.register_raw("ambassador.import", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);

            let data_b64 = params.get("data").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.import", "missing 'data' (base64)"))?;
            let file_bytes = b64_decode(data_b64, "ambassador.import")?;

            let mime_type = params.get("mime_type").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.import", "missing 'mime_type'"))?;
            let author = params.get("author").and_then(|v| v.as_str())
                .unwrap_or("unknown");

            let merge_strategy_str = params.get("merge_strategy").and_then(|v| v.as_str());
            let merge_strategy = match merge_strategy_str {
                Some(s) => serde_json::from_value::<nexus::MergeStrategy>(json!(s))
                    .unwrap_or_default(),
                None => nexus::MergeStrategy::default(),
            };

            let config = nexus::ImportConfig::new(author)
                .with_merge_strategy(merge_strategy);

            let registry = nexus::ImporterRegistry::with_defaults();
            let output = registry.import(&file_bytes, mime_type, &config)
                .map_err(|e| err("ambassador.import", e))?;

            let digits_json = serde_json::to_value(&output.digits)
                .map_err(|e| err("ambassador.import", e))?;

            s.email.send_raw("ambassador.imported", &serde_json::to_vec(&json!({
                "mime_type": mime_type,
                "digit_count": output.digit_count(),
            })).unwrap_or_default());

            ok_json(&json!({
                "digits": digits_json,
                "root_digit_id": output.root_digit_id.map(|id| id.to_string()),
                "warnings": output.warnings,
                "digit_count": output.digit_count(),
            }))
        });

        // ── nexus.export_profiles ─────────────────────────────
        // Stateless: list available export profiles.
        state.phone.register_raw("ambassador.export_profiles", move |_data| {
            ok_json(&json!({
                "profiles": [
                    { "name": "print", "description": "High-quality for physical printing (PDF, PNG)" },
                    { "name": "office", "description": "Microsoft Office / LibreOffice (DOCX, XLSX, PPTX, ODF)" },
                    { "name": "web", "description": "Web-ready (HTML, SVG, PNG, JSON)" },
                    { "name": "source", "description": "Developer formats (JSON, CSV, Markdown, TXT)" },
                    { "name": "media", "description": "Media export (PNG, JPG, SVG)" },
                    { "name": "data", "description": "Structured data (CSV, JSON, XLSX)" },
                    { "name": "everything", "description": "Every applicable format" },
                ],
            }))
        });

        // ── nexus.profile_formats ─────────────────────────────
        // Stateless: given a profile name and digits, return applicable formats.
        state.phone.register_raw("ambassador.profile_formats", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let profile_str = params.get("profile").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.profile_formats", "missing 'profile'"))?;

            let profile: nexus::ExportProfile = serde_json::from_value(json!(profile_str))
                .map_err(|e| err("ambassador.profile_formats", format!("invalid profile '{profile_str}': {e}")))?;

            let digits: Vec<ideas::Digit> = serde_json::from_value(
                params.get("digits").cloned().unwrap_or(json!([]))
            ).unwrap_or_default();

            let formats = nexus::profile_formats(profile, &digits);
            let format_list: Vec<Value> = formats.iter().map(|f| {
                json!({
                    "format": format!("{f:?}"),
                    "extension": f.extension(),
                    "mime_type": f.mime_type(),
                })
            }).collect();
            ok_json(&json!({ "formats": format_list }))
        });

        // ── nexus.format_info ─────────────────────────────────
        // Stateless: get extension and MIME type for a format name.
        state.phone.register_raw("ambassador.format_info", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let format_str = params.get("format").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.format_info", "missing 'format'"))?;

            let format: nexus::ExportFormat = serde_json::from_value(json!(format_str))
                .map_err(|e| err("ambassador.format_info", format!("invalid format '{format_str}': {e}")))?;

            ok_json(&json!({
                "format": format_str,
                "extension": format.extension(),
                "mime_type": format.mime_type(),
            }))
        });

        // ── nexus.federation_scope_check ──────────────────────
        // Stateless: check if a community is visible under a given scope.
        state.phone.register_raw("ambassador.federation_scope_check", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let community_id = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.federation_scope_check", "missing 'community_id'"))?;
            let communities: Vec<String> = params.get("visible_communities")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let scope = if communities.is_empty() {
                nexus::FederationScope::new()
            } else {
                nexus::FederationScope::from_communities(communities)
            };

            ok_json(&json!({
                "community_id": community_id,
                "is_visible": scope.is_visible(community_id),
                "is_unrestricted": scope.is_unrestricted(),
                "scope_size": scope.len(),
            }))
        });

        // ── nexus.export_scoped ───────────────────────────────
        // Export with federation scope enforcement.
        let s = state.clone();
        state.phone.register_raw("ambassador.export_scoped", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);

            let digits: Vec<ideas::Digit> = serde_json::from_value(
                params.get("digits").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("ambassador.export_scoped", format!("invalid digits: {e}")))?;

            let root_id = parse_uuid(&params, "root_id", "ambassador.export_scoped")?;

            let format_str = params.get("format").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.export_scoped", "missing 'format'"))?;
            let format: nexus::ExportFormat = serde_json::from_value(json!(format_str))
                .map_err(|e| err("ambassador.export_scoped", format!("invalid format: {e}")))?;

            let community_id = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("ambassador.export_scoped", "missing 'community_id'"))?;
            let communities: Vec<String> = params.get("visible_communities")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let scope = if communities.is_empty() {
                nexus::FederationScope::new()
            } else {
                nexus::FederationScope::from_communities(communities)
            };

            let config = nexus::ExportConfig::new(format);
            let registry = nexus::ExporterRegistry::with_defaults();
            let output = registry.export_scoped(&digits, root_id, &config, &scope, community_id)
                .map_err(|e| err("ambassador.export_scoped", e))?;

            s.email.send_raw("ambassador.exported", &serde_json::to_vec(&json!({
                "format": format_str,
                "size": output.size(),
                "filename": &output.filename,
                "community_id": community_id,
            })).unwrap_or_default());

            ok_json(&json!({
                "data": b64_encode(&output.data),
                "filename": output.filename,
                "mime_type": output.mime_type,
                "size": output.size(),
            }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Discovery
            .with_call(CallDescriptor::new("ambassador.supported_export_formats", "List supported export formats"))
            .with_call(CallDescriptor::new("ambassador.supported_import_formats", "List supported importers"))
            .with_call(CallDescriptor::new("ambassador.supported_bridges", "List registered protocol bridges"))
            .with_call(CallDescriptor::new("ambassador.format_info", "Get format extension and MIME type"))
            // Export
            .with_call(CallDescriptor::new("ambassador.export", "Export Digits to legacy format"))
            .with_call(CallDescriptor::new("ambassador.export_scoped", "Export with federation scope enforcement"))
            // Import
            .with_call(CallDescriptor::new("ambassador.import", "Import external file into Digits"))
            // Profiles
            .with_call(CallDescriptor::new("ambassador.export_profiles", "List export profiles"))
            .with_call(CallDescriptor::new("ambassador.profile_formats", "Formats for a profile given digit types"))
            // Federation
            .with_call(CallDescriptor::new("ambassador.federation_scope_check", "Check community visibility"))
            // Events
            .with_emitted_event(EventDescriptor::new("ambassador.exported", "Content was exported"))
            .with_emitted_event(EventDescriptor::new("ambassador.imported", "Content was imported"))
    }
}
