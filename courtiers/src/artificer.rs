//! Artificer module — Magic rendering courtier.
//!
//! Manages document state sessions (DocumentState + DocumentHistory),
//! renders digits to RenderSpecs, and projects code (SwiftUI, React, HTML).
//!
//! Sessions are keyed by idea ID. Creating a session loads digits from
//! the .idea package into a DocumentState. Operations mutate state
//! through CRDT-safe DigitOperations. History tracks undo/redo.
//!
//! Stateless operations (type registry, projection, accessibility)
//! don't require a session.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use magic::CodeProjection;

use prerogative::DaemonModule;
use prerogative::DaemonState;
use prerogative::editor_types::FieldKey;

/// A live document editing session managed by the Artificer.
struct MagicSession {
    document: magic::DocumentState,
    history: magic::DocumentHistory,
    renderer_registry: magic::RendererRegistry,
}

pub struct ArtificerModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn parse_id(params: &Value, op: &str) -> Result<Uuid, PhoneError> {
    let id_str = params.get("id").and_then(|v| v.as_str())
        .ok_or_else(|| err(op, "missing 'id'"))?;
    Uuid::parse_str(id_str).map_err(|e| err(op, format!("invalid UUID: {e}")))
}

type Sessions = Arc<Mutex<HashMap<Uuid, MagicSession>>>;

fn with_session<F, R>(sessions: &Sessions, id: &Uuid, op: &str, f: F) -> Result<R, PhoneError>
where
    F: FnOnce(&mut MagicSession) -> Result<R, PhoneError>,
{
    let mut map = sessions.lock().unwrap();
    let session = map.get_mut(id)
        .ok_or_else(|| err(op, format!("no session for idea {id} — call artificer.session_create first")))?;
    f(session)
}

// ── Editor helpers ──────────────────────────────────────────────────

fn guard_vault_unlocked(state: &DaemonState) -> Result<(), PhoneError> {
    let vault = state.vault.lock().unwrap();
    if !vault.is_unlocked() {
        return Err(err("artificer.editor", "Vault is locked — unlock identity first"));
    }
    Ok(())
}

/// Extract text content from a Digit's content field.
fn digit_text_content(digit: &ideas::Digit) -> String {
    digit.content.as_str().unwrap_or("").to_string()
}

/// Save a session's markdown content back to .idea on disk.
fn save_session(state: &DaemonState, id: &Uuid, id_str: &str) -> Result<(), PhoneError> {
    guard_vault_unlocked(state)?;

    let sessions = state.editor_sessions.lock()
        .unwrap_or_else(|e| e.into_inner());
    let session = sessions.get(id)
        .ok_or_else(|| err("artificer.editor_save", "no open session for this idea"))?;

    if !session.dirty {
        return Ok(());
    }

    log::info!("editor.save: saving session {}", id_str);

    let field_texts: Vec<(Uuid, String, String)> = session.field_texts.iter()
        .map(|((digit_id, field_name), text)| (*digit_id, field_name.clone(), text.clone()))
        .collect();

    let field_count = field_texts.len();
    drop(sessions);

    let mut vault = state.vault.lock().unwrap();
    let entry = vault.get_idea(id)
        .map_err(|e| err("artificer.editor_save", e))?
        .ok_or_else(|| err("artificer.editor_save", "idea not found"))?
        .clone();

    let path = std::path::PathBuf::from(&entry.path);
    let content_key = vault.content_key(id)
        .map_err(|e| err("artificer.editor_save", e))?;
    let vocab_seed = vault.vocabulary_seed()
        .map_err(|e| err("artificer.editor_save", e))?;

    let read_result = hall::scholar::read(
        &path, content_key.expose(), Some(vocab_seed.expose()),
    ).map_err(|e| err("artificer.editor_save", e))?;

    if read_result.has_warnings() {
        for w in &read_result.warnings {
            log::warn!("editor.save: Hall read warning for {}: {}", id_str, w);
        }
    }

    let mut package = read_result.value;
    let disk_digit_count = package.digits.len();

    let mut updated_count = 0usize;
    for (digit_id, _field_name, text) in &field_texts {
        if let Some(digit) = package.digits.get_mut(digit_id) {
            digit.content = x::Value::String(text.clone());
            digit.modified = chrono::Utc::now();
            updated_count += 1;
        } else {
            log::warn!("editor.save: digit {} not found on disk", digit_id);
        }
    }

    log::info!(
        "editor.save: updated {}/{} digits ({} on disk) for {}",
        updated_count, field_count, disk_digit_count, id_str
    );

    package.header.modified = chrono::Utc::now();
    package.header.babel.enabled = true;
    package.header.babel.vocabulary_seed = Some("vault-derived".to_string());

    let bytes = hall::scribe::write(&package, content_key.expose(), Some(vocab_seed.expose()))
        .map_err(|e| err("artificer.editor_save", e))?;

    log::info!("editor.save: wrote {} bytes to {}", bytes, path.display());

    let root_id = package.header.content.root_digit_id;
    let mut updated_entry = entry;
    updated_entry.modified_at = chrono::Utc::now();
    if let Some(root) = package.digits.get(&root_id) {
        if let Some(title_val) = root.properties.get("title") {
            if let Some(title_str) = title_val.as_str() {
                updated_entry.title = Some(title_str.to_string());
            }
        }
    }
    vault.register_idea(updated_entry)
        .map_err(|e| err("artificer.editor_save", e))?;

    let mut sessions = state.editor_sessions.lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(session) = sessions.get_mut(id) {
        session.dirty = false;
    }

    let event = serde_json::to_vec(&json!({"id": id_str})).unwrap_or_default();
    state.email.send_raw("artificer.editor_saved", &event);

    log::info!("editor.save: session {} saved successfully", id_str);

    Ok(())
}

impl DaemonModule for ArtificerModule {
    fn id(&self) -> &str { "artificer" }
    fn name(&self) -> &str { "Artificer (Magic)" }
    fn deps(&self) -> &[&str] { &["castellan", "bard"] }

    fn register(&self, state: &Arc<DaemonState>) {
        let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

        // ── magic.session_create ───────────────────────────────
        // Create a Magic session for an idea — loads digits into DocumentState.
        let s = state.clone();
        let sess = sessions.clone();
        state.phone.register_raw("artificer.session_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.session_create")?;
            let author = params.get("author").and_then(|v| v.as_str()).unwrap_or("unknown");

            let mut document = magic::DocumentState::new(author.to_string());

            // Optionally load digits from the .idea package
            let load_from_disk = params.get("load").and_then(|v| v.as_bool()).unwrap_or(true);
            if load_from_disk {
                let mut vault = s.vault.lock().unwrap();
                if vault.is_unlocked() {
                    if let Ok(Some(entry)) = vault.get_idea(&id) {
                        let path = std::path::PathBuf::from(&entry.path);
                        if let Ok(content_key) = vault.content_key(&id) {
                            let vocab_seed = vault.vocabulary_seed().ok();
                            if let Ok(read_result) = hall::scholar::read(
                                &path,
                                content_key.expose(),
                                vocab_seed.as_ref().map(|s| s.expose()),
                            ) {
                                let digits: Vec<ideas::Digit> = read_result.value.digits
                                    .into_values().collect();
                                let root_id = read_result.value.header.content.root_digit_id;
                                document.load_digits(digits, Some(root_id));
                            }
                        }
                    }
                }
            }

            let history = magic::DocumentHistory::with_max_depth(100);
            let mut renderer_registry = magic::RendererRegistry::new();
            magic::imagination::register_all_renderers(&mut renderer_registry);

            let mut map = sess.lock().unwrap();
            map.insert(id, MagicSession { document, history, renderer_registry });

            s.email.send_raw("artificer.session_created", &serde_json::to_vec(&json!({"id": id.to_string()})).unwrap_or_default());
            ok_json(&json!({ "ok": true, "id": id.to_string() }))
        });

        // ── magic.session_close ────────────────────────────────
        let s = state.clone();
        let sess = sessions.clone();
        state.phone.register_raw("artificer.session_close", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.session_close")?;

            let mut map = sess.lock().unwrap();
            map.remove(&id);

            s.email.send_raw("artificer.session_closed", &serde_json::to_vec(&json!({"id": id.to_string()})).unwrap_or_default());
            ok_json(&json!({ "ok": true }))
        });

        // ── magic.digit_count ──────────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("artificer.digit_count", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.digit_count")?;
            with_session(&sess, &id, "artificer.digit_count", |s| {
                ok_json(&json!({ "count": s.document.digit_count() }))
            })
        });

        // ── magic.digit ────────────────────────────────────────
        // Get a single digit by ID
        let sess = sessions.clone();
        state.phone.register_raw("artificer.digit", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.digit")?;
            let digit_id_str = params.get("digit_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.digit", "missing 'digit_id'"))?;
            let digit_id = Uuid::parse_str(digit_id_str)
                .map_err(|e| err("artificer.digit", format!("invalid digit UUID: {e}")))?;

            with_session(&sess, &id, "artificer.digit", |s| {
                let digit = s.document.digit(digit_id)
                    .ok_or_else(|| err("artificer.digit", "digit not found"))?;
                let digit_json = serde_json::to_value(digit)
                    .map_err(|e| err("artificer.digit", e))?;
                ok_json(&digit_json)
            })
        });

        // ── magic.all_digits ───────────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("artificer.all_digits", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.all_digits")?;
            with_session(&sess, &id, "artificer.all_digits", |s| {
                let digits_json: Vec<Value> = s.document.digits()
                    .filter_map(|d| serde_json::to_value(d).ok())
                    .collect();
                ok_json(&Value::Array(digits_json))
            })
        });

        // ── magic.insert ───────────────────────────────────────
        // Insert a digit via Action (tracked in history).
        let s = state.clone();
        let sess = sessions.clone();
        state.phone.register_raw("artificer.insert", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.insert")?;
            let digit_json = params.get("digit")
                .ok_or_else(|| err("artificer.insert", "missing 'digit'"))?;
            let digit: ideas::Digit = serde_json::from_value(digit_json.clone())
                .map_err(|e| err("artificer.insert", e))?;
            let parent_id = params.get("parent_id").and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            with_session(&sess, &id, "artificer.insert", |session| {
                let action = magic::Action::InsertDigit { digit, parent_id, index: None };
                let (op, _inverse) = action.execute(&mut session.document)
                    .map_err(|e| err("artificer.insert", e))?;
                session.history.record(magic::HistoryEntry {
                    operation: op.clone(),
                    inverse: _inverse,
                });
                let op_json = serde_json::to_value(&op)
                    .map_err(|e| err("artificer.insert", e))?;
                s.email.send_raw("artificer.action_applied", &serde_json::to_vec(&json!({"id": id.to_string()})).unwrap_or_default());
                ok_json(&op_json)
            })
        });

        // ── magic.update ───────────────────────────────────────
        let s = state.clone();
        let sess = sessions.clone();
        state.phone.register_raw("artificer.update", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.update")?;
            let digit_id_str = params.get("digit_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.update", "missing 'digit_id'"))?;
            let digit_id = Uuid::parse_str(digit_id_str)
                .map_err(|e| err("artificer.update", format!("invalid digit UUID: {e}")))?;
            let field = params.get("field").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.update", "missing 'field'"))?;
            let old_value = params.get("old_value")
                .ok_or_else(|| err("artificer.update", "missing 'old_value'"))?;
            let new_value = params.get("new_value")
                .ok_or_else(|| err("artificer.update", "missing 'new_value'"))?;

            let old_x = prerogative::api_json::json_to_x(old_value);
            let new_x = prerogative::api_json::json_to_x(new_value);

            with_session(&sess, &id, "artificer.update", |session| {
                let action = magic::Action::UpdateDigit {
                    digit_id,
                    field: field.to_string(),
                    old_value: old_x,
                    new_value: new_x,
                };
                let (op, inverse) = action.execute(&mut session.document)
                    .map_err(|e| err("artificer.update", e))?;
                session.history.record(magic::HistoryEntry { operation: op.clone(), inverse });
                let op_json = serde_json::to_value(&op)
                    .map_err(|e| err("artificer.update", e))?;
                s.email.send_raw("artificer.action_applied", &serde_json::to_vec(&json!({"id": id.to_string()})).unwrap_or_default());
                ok_json(&op_json)
            })
        });

        // ── magic.delete ───────────────────────────────────────
        let s = state.clone();
        let sess = sessions.clone();
        state.phone.register_raw("artificer.delete", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.delete")?;
            let digit_id_str = params.get("digit_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.delete", "missing 'digit_id'"))?;
            let digit_id = Uuid::parse_str(digit_id_str)
                .map_err(|e| err("artificer.delete", format!("invalid digit UUID: {e}")))?;

            with_session(&sess, &id, "artificer.delete", |session| {
                let action = magic::Action::DeleteDigit { digit_id, snapshot: None, parent_id: None };
                let (op, inverse) = action.execute(&mut session.document)
                    .map_err(|e| err("artificer.delete", e))?;
                session.history.record(magic::HistoryEntry { operation: op.clone(), inverse });
                let op_json = serde_json::to_value(&op)
                    .map_err(|e| err("artificer.delete", e))?;
                s.email.send_raw("artificer.action_applied", &serde_json::to_vec(&json!({"id": id.to_string()})).unwrap_or_default());
                ok_json(&op_json)
            })
        });

        // ── magic.undo ─────────────────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("artificer.undo", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.undo")?;
            with_session(&sess, &id, "artificer.undo", |session| {
                let entry = session.history.pop_undo()
                    .ok_or_else(|| err("artificer.undo", "nothing to undo"))?;
                let (redo_op, redo_inverse) = entry.inverse.execute(&mut session.document)
                    .map_err(|e| err("artificer.undo", e))?;
                session.history.push_redo(magic::HistoryEntry {
                    operation: redo_op.clone(),
                    inverse: redo_inverse,
                });
                let op_json = serde_json::to_value(&redo_op)
                    .map_err(|e| err("artificer.undo", e))?;
                ok_json(&op_json)
            })
        });

        // ── magic.redo ─────────────────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("artificer.redo", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.redo")?;
            with_session(&sess, &id, "artificer.redo", |session| {
                let entry = session.history.pop_redo()
                    .ok_or_else(|| err("artificer.redo", "nothing to redo"))?;
                let (op, inverse) = entry.inverse.execute(&mut session.document)
                    .map_err(|e| err("artificer.redo", e))?;
                session.history.record(magic::HistoryEntry {
                    operation: op.clone(),
                    inverse,
                });
                let op_json = serde_json::to_value(&op)
                    .map_err(|e| err("artificer.redo", e))?;
                ok_json(&op_json)
            })
        });

        // ── magic.can_undo / magic.can_redo ────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("artificer.can_undo", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.can_undo")?;
            with_session(&sess, &id, "artificer.can_undo", |s| {
                ok_json(&json!({ "can_undo": s.history.can_undo() }))
            })
        });

        let sess = sessions.clone();
        state.phone.register_raw("artificer.can_redo", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.can_redo")?;
            with_session(&sess, &id, "artificer.can_redo", |s| {
                ok_json(&json!({ "can_redo": s.history.can_redo() }))
            })
        });

        // ── magic.render ───────────────────────────────────────
        // Render a digit to a RenderSpec using the session's renderer registry.
        let sess = sessions.clone();
        state.phone.register_raw("artificer.render", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_id(&params, "artificer.render")?;
            let digit_id_str = params.get("digit_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.render", "missing 'digit_id'"))?;
            let digit_id = Uuid::parse_str(digit_id_str)
                .map_err(|e| err("artificer.render", format!("invalid UUID: {e}")))?;
            let mode_str = params.get("mode").and_then(|v| v.as_str()).unwrap_or("display");
            let mode = match mode_str {
                "editing" => magic::RenderMode::Editing,
                "thumbnail" => magic::RenderMode::Thumbnail,
                "print" => magic::RenderMode::Print,
                _ => magic::RenderMode::Display,
            };

            with_session(&sess, &id, "artificer.render", |session| {
                let digit = session.document.digit(digit_id)
                    .ok_or_else(|| err("artificer.render", "digit not found"))?
                    .clone();
                let context = magic::RenderContext {
                    available_width: params.get("width").and_then(|v| v.as_f64()).unwrap_or(800.0),
                    available_height: params.get("height").and_then(|v| v.as_f64()).unwrap_or(600.0),
                    color_scheme: magic::ColorScheme::Dark,
                    text_scale: 1.0,
                    reduce_motion: false,
                };
                let spec = session.renderer_registry.render(&digit, mode, &context);
                let spec_json = serde_json::to_value(&spec)
                    .map_err(|e| err("artificer.render", e))?;
                ok_json(&spec_json)
            })
        });

        // ── magic.project_swiftui ──────────────────────────────
        // Stateless: project digits into SwiftUI code.
        state.phone.register_raw("artificer.project_swiftui", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let digits_json = params.get("digits")
                .ok_or_else(|| err("artificer.project_swiftui", "missing 'digits'"))?;
            let digits: Vec<ideas::Digit> = serde_json::from_value(digits_json.clone())
                .map_err(|e| err("artificer.project_swiftui", e))?;
            let root_id = params.get("root_id").and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let reign = regalia::Reign::default();
            let context = magic::ProjectionContext::build(&digits, root_id, reign);
            let projection = magic::SwiftUIProjection;
            let files = projection.project(&context)
                .map_err(|e| err("artificer.project_swiftui", e))?;
            let files_json = serde_json::to_value(&files)
                .map_err(|e| err("artificer.project_swiftui", e))?;
            ok_json(&files_json)
        });

        // ── magic.project_react ────────────────────────────────
        state.phone.register_raw("artificer.project_react", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let digits_json = params.get("digits")
                .ok_or_else(|| err("artificer.project_react", "missing 'digits'"))?;
            let digits: Vec<ideas::Digit> = serde_json::from_value(digits_json.clone())
                .map_err(|e| err("artificer.project_react", e))?;
            let root_id = params.get("root_id").and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let reign = regalia::Reign::default();
            let context = magic::ProjectionContext::build(&digits, root_id, reign);
            let projection = magic::ReactProjection;
            let files = projection.project(&context)
                .map_err(|e| err("artificer.project_react", e))?;
            let files_json = serde_json::to_value(&files)
                .map_err(|e| err("artificer.project_react", e))?;
            ok_json(&files_json)
        });

        // ── magic.project_html ─────────────────────────────────
        state.phone.register_raw("artificer.project_html", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let digits_json = params.get("digits")
                .ok_or_else(|| err("artificer.project_html", "missing 'digits'"))?;
            let digits: Vec<ideas::Digit> = serde_json::from_value(digits_json.clone())
                .map_err(|e| err("artificer.project_html", e))?;
            let root_id = params.get("root_id").and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let reign = regalia::Reign::default();
            let context = magic::ProjectionContext::build(&digits, root_id, reign);
            let projection = magic::HtmlProjection;
            let files = projection.project(&context)
                .map_err(|e| err("artificer.project_html", e))?;
            let files_json = serde_json::to_value(&files)
                .map_err(|e| err("artificer.project_html", e))?;
            ok_json(&files_json)
        });

        // ── magic.type_registry ────────────────────────────────
        // Get the core digit type definitions (stateless).
        state.phone.register_raw("artificer.type_registry", move |_data| {
            let registry = magic::DigitTypeRegistry::with_core_types();
            let registry_json = serde_json::to_value(&registry)
                .map_err(|e| err("artificer.type_registry", e))?;
            ok_json(&registry_json)
        });

        // ════════════════════════════════════════════════════════════
        // Editor — .idea document persistence for browser editors
        // ════════════════════════════════════════════════════════════

        // ── editor.open ─────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("artificer.editor_open", move |data| {
            guard_vault_unlocked(&s)?;

            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id_str = params.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.editor_open", "missing 'id'"))?;
            let id = Uuid::parse_str(id_str)
                .map_err(|e| err("artificer.editor_open", format!("invalid UUID: {e}")))?;

            // If a session already exists, return its current state (idempotent).
            {
                let sessions = s.editor_sessions.lock().unwrap();
                if let Some(session) = sessions.get(&id) {
                    let mut fields_json = serde_json::Map::new();
                    for ((digit_id, field_name), text) in &session.field_texts {
                        fields_json.insert(
                            digit_id.to_string(),
                            json!({
                                "field": field_name,
                                "text": text,
                            }),
                        );
                    }

                    return ok_json(&json!({
                        "id": id_str,
                        "fields": Value::Object(fields_json),
                    }));
                }
            }

            // Load the .idea package from disk.
            let (package, requested_field) = {
                let mut vault = s.vault.lock().unwrap();
                let entry = vault.get_idea(&id)
                    .map_err(|e| err("artificer.editor_open", e))?
                    .ok_or_else(|| err("artificer.editor_open", "idea not found"))?
                    .clone();

                let path = std::path::PathBuf::from(&entry.path);
                let content_key = vault.content_key(&id)
                    .map_err(|e| err("artificer.editor_open", e))?;
                let vocab_seed = vault.vocabulary_seed()
                    .map_err(|e| err("artificer.editor_open", e))?;
                let read_result = hall::scholar::read(
                    &path, content_key.expose(), Some(vocab_seed.expose()),
                ).map_err(|e| err("artificer.editor_open", e))?;

                let field = params.get("field").and_then(|v| v.as_str())
                    .unwrap_or("body").to_string();

                (read_result.value, field)
            };

            // Build session from .idea package.
            let mut field_texts: HashMap<FieldKey, String> = HashMap::new();
            let mut fields_json = serde_json::Map::new();

            for (digit_id, digit) in &package.digits {
                let text_content = digit_text_content(digit);
                let field_name = requested_field.clone();

                fields_json.insert(
                    digit_id.to_string(),
                    json!({
                        "field": &field_name,
                        "text": &text_content,
                        "type": digit.digit_type(),
                    }),
                );

                field_texts.insert((*digit_id, field_name), text_content);
            }

            let session = prerogative::EditorSession {
                field_texts,
                dirty: false,
            };

            let mut sessions = s.editor_sessions.lock().unwrap();
            sessions.insert(id, session);

            ok_json(&json!({
                "id": id_str,
                "fields": Value::Object(fields_json),
            }))
        });

        // ── editor.set_content ──────────────────────────────────
        // Browser sends markdown updates (debounced) for .idea persistence.
        let s = state.clone();
        state.phone.register_raw("artificer.editor_set_content", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id_str = params.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.editor_set_content", "missing 'id'"))?;
            let id = Uuid::parse_str(id_str)
                .map_err(|e| err("artificer.editor_set_content", format!("invalid UUID: {e}")))?;

            let digit_id_str = params.get("digit_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.editor_set_content", "missing 'digit_id'"))?;
            let digit_id = Uuid::parse_str(digit_id_str)
                .map_err(|e| err("artificer.editor_set_content", format!("invalid digit UUID: {e}")))?;

            let field = params.get("field").and_then(|v| v.as_str())
                .unwrap_or("body");
            let content = params.get("content").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.editor_set_content", "missing 'content'"))?;

            let mut sessions = s.editor_sessions.lock().unwrap();
            let session = sessions.get_mut(&id)
                .ok_or_else(|| err("artificer.editor_set_content", "no open session for this idea"))?;

            session.field_texts.insert((digit_id, field.to_string()), content.to_string());
            session.dirty = true;

            let event = serde_json::to_vec(&json!({
                "id": id_str,
                "digit_id": digit_id_str,
                "field": field,
            })).unwrap_or_default();
            s.email.send_raw("artificer.editor_changed", &event);

            ok_json(&json!({ "ok": true }))
        });

        // ── editor.save ─────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("artificer.editor_save", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id_str = params.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.editor_save", "missing 'id'"))?;
            let id = Uuid::parse_str(id_str)
                .map_err(|e| err("artificer.editor_save", format!("invalid UUID: {e}")))?;

            {
                let sessions = s.editor_sessions.lock()
                    .unwrap_or_else(|e| e.into_inner());
                let is_dirty = sessions.get(&id).map(|s| s.dirty).unwrap_or(false);
                if !is_dirty {
                    return ok_json(&json!({ "ok": true, "saved": false }));
                }
            }

            save_session(&s, &id, id_str)?;
            ok_json(&json!({ "ok": true, "saved": true }))
        });

        // ── editor.close ────────────────────────────────────────
        let s = state.clone();
        state.phone.register_raw("artificer.editor_close", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id_str = params.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| err("artificer.editor_close", "missing 'id'"))?;
            let id = Uuid::parse_str(id_str)
                .map_err(|e| err("artificer.editor_close", format!("invalid UUID: {e}")))?;

            {
                let sessions = s.editor_sessions.lock()
                    .unwrap_or_else(|e| e.into_inner());
                let is_dirty = sessions.get(&id).map(|s| s.dirty).unwrap_or(false);
                if is_dirty {
                    drop(sessions);
                    save_session(&s, &id, id_str)?;
                }
            }

            let mut sessions = s.editor_sessions.lock()
                .unwrap_or_else(|e| e.into_inner());
            sessions.remove(&id);

            let event = serde_json::to_vec(&json!({"id": id_str})).unwrap_or_default();
            s.email.send_raw("artificer.editor_closed", &event);

            ok_json(&json!({ "ok": true }))
        });

        // ── Auto-save loop ──────────────────────────────────────
        let s = state.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(std::time::Duration::from_secs(3));

                if s.shutdown.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                let dirty_ids: Vec<(Uuid, String)> = {
                    let sessions = s.editor_sessions.lock()
                        .unwrap_or_else(|e| e.into_inner());
                    sessions.iter()
                        .filter(|(_, session)| session.dirty)
                        .map(|(id, _)| (*id, id.to_string()))
                        .collect()
                };

                if !dirty_ids.is_empty() {
                    log::info!("editor auto-save: {} dirty session(s)", dirty_ids.len());
                }
                for (id, id_str) in &dirty_ids {
                    match save_session(&s, id, id_str) {
                        Ok(()) => {}
                        Err(e) => log::warn!("editor auto-save failed for {}: {:?}", id_str, e),
                    }
                }
            }
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Sessions
            .with_call(CallDescriptor::new("artificer.session_create", "Create Magic session for idea"))
            .with_call(CallDescriptor::new("artificer.session_close", "Close Magic session"))
            // Document queries
            .with_call(CallDescriptor::new("artificer.digit_count", "Count digits in session"))
            .with_call(CallDescriptor::new("artificer.digit", "Get digit by ID"))
            .with_call(CallDescriptor::new("artificer.all_digits", "Get all digits"))
            // Document mutations
            .with_call(CallDescriptor::new("artificer.insert", "Insert digit (tracked)"))
            .with_call(CallDescriptor::new("artificer.update", "Update digit field (tracked)"))
            .with_call(CallDescriptor::new("artificer.delete", "Delete digit (tracked)"))
            // History
            .with_call(CallDescriptor::new("artificer.undo", "Undo last action"))
            .with_call(CallDescriptor::new("artificer.redo", "Redo last undo"))
            .with_call(CallDescriptor::new("artificer.can_undo", "Check if undo available"))
            .with_call(CallDescriptor::new("artificer.can_redo", "Check if redo available"))
            // Rendering
            .with_call(CallDescriptor::new("artificer.render", "Render digit to RenderSpec"))
            // Projection
            .with_call(CallDescriptor::new("artificer.project_swiftui", "Project to SwiftUI"))
            .with_call(CallDescriptor::new("artificer.project_react", "Project to React"))
            .with_call(CallDescriptor::new("artificer.project_html", "Project to HTML"))
            // Type registry
            .with_call(CallDescriptor::new("artificer.type_registry", "Get core type definitions"))
            // Events
            .with_emitted_event(EventDescriptor::new("artificer.session_created", "Session created"))
            .with_emitted_event(EventDescriptor::new("artificer.session_closed", "Session closed"))
            .with_emitted_event(EventDescriptor::new("artificer.action_applied", "Action applied to document"))
            // Editor — .idea persistence
            .with_call(CallDescriptor::new("artificer.editor_open", "Open a .idea for editing"))
            .with_call(CallDescriptor::new("artificer.editor_set_content", "Update field content (markdown)"))
            .with_call(CallDescriptor::new("artificer.editor_save", "Save session to disk"))
            .with_call(CallDescriptor::new("artificer.editor_close", "Save and close editor session"))
            .with_emitted_event(EventDescriptor::new("artificer.editor_changed", "Editor content changed"))
            .with_emitted_event(EventDescriptor::new("artificer.editor_saved", "Editor saved to disk"))
            .with_emitted_event(EventDescriptor::new("artificer.editor_closed", "Editor session closed"))
    }
}
