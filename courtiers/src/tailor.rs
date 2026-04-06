//! Tailor module — Regalia design language courtier.
//!
//! Exposes Aura design tokens, Reign theming, ThemeCollection management,
//! ComponentStyleRegistry, Surge animation curves, and layout resolution
//! as daemon operations. All types are serializable — programs receive
//! full JSON representations of themes and tokens.

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct TailorModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for TailorModule {
    fn id(&self) -> &str { "tailor" }
    fn name(&self) -> &str { "Tailor (Regalia)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── regalia.default_aura ──────────────────────────────
        // Stateless: returns the default Aura design token set.
        state.phone.register_raw("tailor.default_aura", move |_data| {
            let aura = regalia::Aura::default();
            let aura_json = serde_json::to_value(&aura)
                .map_err(|e| err("tailor.default_aura", e))?;
            ok_json(&aura_json)
        });

        // ── regalia.default_reign ─────────────────────────────
        // Stateless: returns the default Reign (theme).
        state.phone.register_raw("tailor.default_reign", move |_data| {
            let reign = regalia::Reign::default();
            let reign_json = serde_json::to_value(&reign)
                .map_err(|e| err("tailor.default_reign", e))?;
            ok_json(&reign_json)
        });

        // ── regalia.create_reign ──────────────────────────────
        // Stateless: create a Reign from name + aspect. Returns full theme JSON.
        state.phone.register_raw("tailor.create_reign", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("tailor.create_reign", "missing 'name'"))?;
            let aspect_name = params.get("aspect").and_then(|v| v.as_str())
                .unwrap_or("light");

            let aspect = match aspect_name {
                "dark" => regalia::Aspect::dark(),
                "light" => regalia::Aspect::light(),
                custom => regalia::Aspect::custom(custom),
            };

            // Accept optional Aura override, otherwise default.
            let aura = if let Some(aura_val) = params.get("aura") {
                serde_json::from_value::<regalia::Aura>(aura_val.clone())
                    .map_err(|e| err("tailor.create_reign", format!("invalid aura: {e}")))?
            } else {
                regalia::Aura::default()
            };

            let reign = regalia::Reign::new(name, aura, aspect);
            let reign_json = serde_json::to_value(&reign)
                .map_err(|e| err("tailor.create_reign", e))?;
            ok_json(&reign_json)
        });

        // ── regalia.resolve_crest ─────────────────────────────
        // Stateless: given a Reign JSON, resolve the Crest for its aspect.
        state.phone.register_raw("tailor.resolve_crest", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let reign: regalia::Reign = serde_json::from_value(
                params.get("reign").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.resolve_crest", format!("invalid reign: {e}")))?;

            let crest = reign.crest();
            let crest_json = serde_json::to_value(crest)
                .map_err(|e| err("tailor.resolve_crest", e))?;
            ok_json(&crest_json)
        });

        // ── regalia.resolve_tokens ────────────────────────────
        // Stateless: given a Reign, return all resolved token values for its aspect.
        state.phone.register_raw("tailor.resolve_tokens", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let reign: regalia::Reign = serde_json::from_value(
                params.get("reign").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.resolve_tokens", format!("invalid reign: {e}")))?;

            let crest_json = serde_json::to_value(reign.crest())
                .map_err(|e| err("tailor.resolve_tokens", e))?;
            let span_json = serde_json::to_value(reign.span())
                .map_err(|e| err("tailor.resolve_tokens", e))?;
            let inscription_json = serde_json::to_value(reign.inscription())
                .map_err(|e| err("tailor.resolve_tokens", e))?;
            let arch_json = serde_json::to_value(reign.arch())
                .map_err(|e| err("tailor.resolve_tokens", e))?;
            let umbra_json = serde_json::to_value(reign.umbra())
                .map_err(|e| err("tailor.resolve_tokens", e))?;
            let impulse_json = serde_json::to_value(reign.impulse())
                .map_err(|e| err("tailor.resolve_tokens", e))?;
            let motion_json = serde_json::to_value(reign.motion_preference())
                .map_err(|e| err("tailor.resolve_tokens", e))?;

            ok_json(&json!({
                "name": reign.name,
                "aspect": reign.aspect.name(),
                "crest": crest_json,
                "span": span_json,
                "inscription": inscription_json,
                "arch": arch_json,
                "umbra": umbra_json,
                "impulse": impulse_json,
                "gradients": reign.gradients(),
                "image_styles": reign.image_styles(),
                "motion_preference": motion_json,
                "minimum_touch_target": reign.minimum_touch_target(),
                "minimum_font_size": reign.minimum_font_size(),
            }))
        });

        // ── regalia.parse_reign ───────────────────────────────
        // Stateless: parse a .excalibur JSON file into a validated Reign.
        state.phone.register_raw("tailor.parse_reign", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let excalibur_json = params.get("json").and_then(|v| v.as_str())
                .ok_or_else(|| err("tailor.parse_reign", "missing 'json'"))?;

            let reign: regalia::Reign = serde_json::from_str(excalibur_json)
                .map_err(|e| err("tailor.parse_reign", format!("invalid theme: {e}")))?;

            let reign_json = serde_json::to_value(&reign)
                .map_err(|e| err("tailor.parse_reign", e))?;
            ok_json(&reign_json)
        });

        // ── regalia.component_style_presets ───────────────────
        // Stateless: returns the built-in component style presets.
        state.phone.register_raw("tailor.component_style_presets", move |_data| {
            let presets = vec![
                regalia::ComponentStyle::primary_button(),
                regalia::ComponentStyle::card(),
                regalia::ComponentStyle::input_field(),
                regalia::ComponentStyle::text_body(),
            ];
            let presets_json = serde_json::to_value(&presets)
                .map_err(|e| err("tailor.component_style_presets", e))?;
            ok_json(&presets_json)
        });

        // ── regalia.resolve_component_style ───────────────────
        // Stateless: look up a component style by name from a provided registry.
        state.phone.register_raw("tailor.resolve_component_style", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let registry: regalia::ComponentStyleRegistry = serde_json::from_value(
                params.get("registry").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.resolve_component_style", format!("invalid registry: {e}")))?;
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("tailor.resolve_component_style", "missing 'name'"))?;

            match registry.get(name) {
                Some(style) => {
                    let style_json = serde_json::to_value(style)
                        .map_err(|e| err("tailor.resolve_component_style", e))?;
                    ok_json(&style_json)
                }
                None => ok_json(&json!({ "error": "style not found", "name": name })),
            }
        });

        // ── regalia.surge_value ───────────────────────────────
        // Stateless: evaluate a Surge animation curve at time t.
        state.phone.register_raw("tailor.surge_value", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let curve_type = params.get("curve").and_then(|v| v.as_str())
                .ok_or_else(|| err("tailor.surge_value", "missing 'curve'"))?;
            let t = params.get("t").and_then(|v| v.as_f64())
                .ok_or_else(|| err("tailor.surge_value", "missing 't'"))?;
            let velocity = params.get("velocity").and_then(|v| v.as_f64())
                .unwrap_or(0.0);

            let shift = match curve_type {
                "spring" => regalia::Shift::new(regalia::SpringSurge::default()),
                "ease" => regalia::Shift::new(regalia::EaseSurge::default()),
                "linear" => regalia::Shift::new(regalia::LinearSurge::default()),
                "decay" => regalia::Shift::new(regalia::DecaySurge::default()),
                "snap" => regalia::Shift::snap(),
                other => return Err(err("tailor.surge_value", format!("unknown curve: '{other}'. Use: spring, ease, linear, decay, snap"))),
            };

            let value = shift.value(t);
            let is_complete = shift.is_complete(t, velocity);
            let duration = shift.duration();
            ok_json(&json!({
                "value": value,
                "is_complete": is_complete,
                "duration": duration,
            }))
        });

        // ── regalia.theme_collection_new ──────────────────────
        // Stateless: create a ThemeCollection from an initial Reign JSON.
        state.phone.register_raw("tailor.theme_collection_new", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let reign: regalia::Reign = serde_json::from_value(
                params.get("reign").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.theme_collection_new", format!("invalid reign: {e}")))?;

            let collection = regalia::ThemeCollection::new(reign);
            let collection_json = serde_json::to_value(&collection)
                .map_err(|e| err("tailor.theme_collection_new", e))?;
            ok_json(&collection_json)
        });

        // ── regalia.theme_collection_add ──────────────────────
        // Stateless: add a theme to an existing ThemeCollection.
        let s = state.clone();
        state.phone.register_raw("tailor.theme_collection_add", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let mut collection: regalia::ThemeCollection = serde_json::from_value(
                params.get("collection").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.theme_collection_add", format!("invalid collection: {e}")))?;
            let reign: regalia::Reign = serde_json::from_value(
                params.get("reign").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.theme_collection_add", format!("invalid reign: {e}")))?;

            collection.add(reign)
                .map_err(|e| err("tailor.theme_collection_add", e))?;

            let collection_json = serde_json::to_value(&collection)
                .map_err(|e| err("tailor.theme_collection_add", e))?;

            s_email_send(&s, "tailor.theme_added", &json!({
                "count": collection.count(),
            }));
            ok_json(&collection_json)
        });

        // ── regalia.theme_collection_switch ───────────────────
        // Stateless: switch active theme in a ThemeCollection.
        let s = state.clone();
        state.phone.register_raw("tailor.theme_collection_switch", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let mut collection: regalia::ThemeCollection = serde_json::from_value(
                params.get("collection").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.theme_collection_switch", format!("invalid collection: {e}")))?;
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("tailor.theme_collection_switch", "missing 'name'"))?;

            collection.switch(name)
                .map_err(|e| err("tailor.theme_collection_switch", e))?;

            let collection_json = serde_json::to_value(&collection)
                .map_err(|e| err("tailor.theme_collection_switch", e))?;

            s_email_send(&s, "tailor.theme_switched", &json!({
                "active": collection.active_name(),
            }));
            ok_json(&collection_json)
        });

        // ── regalia.theme_collection_remove ───────────────────
        // Stateless: remove a theme from a ThemeCollection.
        state.phone.register_raw("tailor.theme_collection_remove", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let mut collection: regalia::ThemeCollection = serde_json::from_value(
                params.get("collection").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.theme_collection_remove", format!("invalid collection: {e}")))?;
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("tailor.theme_collection_remove", "missing 'name'"))?;

            let removed = collection.remove(name)
                .map_err(|e| err("tailor.theme_collection_remove", e))?;

            let collection_json = serde_json::to_value(&collection)
                .map_err(|e| err("tailor.theme_collection_remove", e))?;
            let removed_json = serde_json::to_value(&removed)
                .map_err(|e| err("tailor.theme_collection_remove", e))?;

            ok_json(&json!({
                "collection": collection_json,
                "removed": removed_json,
            }))
        });

        // ── regalia.theme_collection_list ─────────────────────
        // Stateless: list theme names and active theme in a collection.
        state.phone.register_raw("tailor.theme_collection_list", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let collection: regalia::ThemeCollection = serde_json::from_value(
                params.get("collection").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.theme_collection_list", format!("invalid collection: {e}")))?;

            let names: Vec<&str> = collection.list();
            ok_json(&json!({
                "themes": names,
                "active": collection.active_name(),
                "count": collection.count(),
            }))
        });

        // ── regalia.formation_kinds ───────────────────────────
        // Stateless: lists available FormationKind variants for layout.
        state.phone.register_raw("tailor.formation_kinds", move |_data| {
            ok_json(&json!({
                "kinds": [
                    { "kind": "Rank", "description": "Horizontal flow (HStack / flex-row)" },
                    { "kind": "Column", "description": "Vertical flow (VStack / flex-column)" },
                    { "kind": "Tier", "description": "Depth stacking (ZStack / position stacked)" },
                    { "kind": "Procession", "description": "Flow-wrap (LazyVGrid / flex-wrap)" },
                    { "kind": "OpenCourt", "description": "Free positioning (Canvas / absolute)" },
                ],
            }))
        });

        // ── regalia.ember_lighten ─────────────────────────────
        // Stateless: lighten a color by a given amount.
        state.phone.register_raw("tailor.ember_lighten", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let color: regalia::Ember = serde_json::from_value(
                params.get("color").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.ember_lighten", format!("invalid color: {e}")))?;
            let amount = params.get("amount").and_then(|v| v.as_f64())
                .ok_or_else(|| err("tailor.ember_lighten", "missing 'amount'"))?;

            let result = color.lighten(amount);
            let result_json = serde_json::to_value(&result)
                .map_err(|e| err("tailor.ember_lighten", e))?;
            ok_json(&result_json)
        });

        // ── regalia.ember_darken ──────────────────────────────
        // Stateless: darken a color by a given amount.
        state.phone.register_raw("tailor.ember_darken", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let color: regalia::Ember = serde_json::from_value(
                params.get("color").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.ember_darken", format!("invalid color: {e}")))?;
            let amount = params.get("amount").and_then(|v| v.as_f64())
                .ok_or_else(|| err("tailor.ember_darken", "missing 'amount'"))?;

            let result = color.darken(amount);
            let result_json = serde_json::to_value(&result)
                .map_err(|e| err("tailor.ember_darken", e))?;
            ok_json(&result_json)
        });

        // ── regalia.gradient_color_at ─────────────────────────
        // Stateless: interpolate a color at position t along a gradient.
        state.phone.register_raw("tailor.gradient_color_at", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let gradient: regalia::Gradient = serde_json::from_value(
                params.get("gradient").cloned().unwrap_or(Value::Null)
            ).map_err(|e| err("tailor.gradient_color_at", format!("invalid gradient: {e}")))?;
            let t = params.get("t").and_then(|v| v.as_f64())
                .ok_or_else(|| err("tailor.gradient_color_at", "missing 't'"))?;

            let color = gradient.color_at(t);
            let color_json = serde_json::to_value(&color)
                .map_err(|e| err("tailor.gradient_color_at", e))?;
            ok_json(&color_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Token defaults
            .with_call(CallDescriptor::new("tailor.default_aura", "Default Aura design tokens"))
            .with_call(CallDescriptor::new("tailor.default_reign", "Default Reign theme"))
            .with_call(CallDescriptor::new("tailor.create_reign", "Create a Reign from name + aspect"))
            // Token resolution
            .with_call(CallDescriptor::new("tailor.resolve_crest", "Resolve Crest for Reign's aspect"))
            .with_call(CallDescriptor::new("tailor.resolve_tokens", "Resolve all tokens for a Reign"))
            // Theme files
            .with_call(CallDescriptor::new("tailor.parse_reign", "Parse .excalibur JSON into Reign"))
            // Component styles
            .with_call(CallDescriptor::new("tailor.component_style_presets", "Built-in component style presets"))
            .with_call(CallDescriptor::new("tailor.resolve_component_style", "Look up style in registry"))
            // Animation
            .with_call(CallDescriptor::new("tailor.surge_value", "Evaluate Surge animation curve at time t"))
            // Theme collections
            .with_call(CallDescriptor::new("tailor.theme_collection_new", "Create a ThemeCollection"))
            .with_call(CallDescriptor::new("tailor.theme_collection_add", "Add theme to collection"))
            .with_call(CallDescriptor::new("tailor.theme_collection_switch", "Switch active theme"))
            .with_call(CallDescriptor::new("tailor.theme_collection_remove", "Remove theme from collection"))
            .with_call(CallDescriptor::new("tailor.theme_collection_list", "List themes in collection"))
            // Layout
            .with_call(CallDescriptor::new("tailor.formation_kinds", "Available layout formation kinds"))
            // Color
            .with_call(CallDescriptor::new("tailor.ember_lighten", "Lighten an Ember color"))
            .with_call(CallDescriptor::new("tailor.ember_darken", "Darken an Ember color"))
            .with_call(CallDescriptor::new("tailor.gradient_color_at", "Interpolate gradient color at position"))
            // Events
            .with_emitted_event(EventDescriptor::new("tailor.theme_added", "Theme was added to collection"))
            .with_emitted_event(EventDescriptor::new("tailor.theme_switched", "Active theme was switched"))
    }
}

/// Helper to send an event on the email bus. Swallows serialization failure
/// (fire-and-forget — event emission should never fail the handler).
fn s_email_send(state: &DaemonState, event: &str, payload: &Value) {
    if let Ok(bytes) = serde_json::to_vec(payload) {
        state.email.send_raw(event, &bytes);
    }
}
