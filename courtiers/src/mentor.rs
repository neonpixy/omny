//! Mentor module — Oracle guidance & onboarding courtier.
//!
//! Exposes sovereignty tiers, disclosure tracking, hint evaluation,
//! activation flow orchestration, recovery flow management, and
//! workflow automation as daemon operations. Oracle has zero internal
//! Omninet dependencies — all types are pure data.
//!
//! Programs use staff name "mentor" which maps to daemon namespace "oracle".

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct MentorModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for MentorModule {
    fn id(&self) -> &str { "mentor" }
    fn name(&self) -> &str { "Mentor (Oracle)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Sovereignty Tiers & Progressive Disclosure
        // ═══════════════════════════════════════════════════════════

        // ── oracle.tier_defaults ──────────────────────────────────
        // Get the default configuration for a sovereignty tier.
        state.phone.register_raw("mentor.tier_defaults", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let tier_json = params.get("tier")
                .ok_or_else(|| err("mentor.tier_defaults", "missing 'tier'"))?;

            let tier: oracle::SovereigntyTier = serde_json::from_value(tier_json.clone())
                .map_err(|e| err("mentor.tier_defaults", e))?;

            let defaults = oracle::TierDefaults::for_tier(tier);
            let defaults_json = serde_json::to_value(&defaults)
                .map_err(|e| err("mentor.tier_defaults", e))?;
            ok_json(&defaults_json)
        });

        // ── oracle.tier_all_defaults ──────────────────────────────
        // Get defaults for all four tiers.
        state.phone.register_raw("mentor.tier_all_defaults", move |_data| {
            let all = oracle::TierDefaults::all();
            let all_json = serde_json::to_value(&all)
                .map_err(|e| err("mentor.tier_all_defaults", e))?;
            ok_json(&all_json)
        });

        // ── oracle.disclosure_create ──────────────────────────────
        // Create a new disclosure tracker at a given tier.
        state.phone.register_raw("mentor.disclosure_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let default_tier = Value::String("Citizen".into());
            let tier_json = params.get("tier").unwrap_or(&default_tier);

            let _tier: oracle::SovereigntyTier = serde_json::from_value(tier_json.clone())
                .map_err(|e| err("mentor.disclosure_create", e))?;

            let tracker = oracle::DisclosureTracker::new();
            let tracker_json = serde_json::to_value(&tracker)
                .map_err(|e| err("mentor.disclosure_create", e))?;
            ok_json(&tracker_json)
        });

        // ── oracle.disclosure_signal ──────────────────────────────
        // Record a disclosure signal on a tracker.
        let s = state.clone();
        state.phone.register_raw("mentor.disclosure_signal", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let tracker_json = params.get("tracker")
                .ok_or_else(|| err("mentor.disclosure_signal", "missing 'tracker'"))?;
            let signal_json = params.get("signal")
                .ok_or_else(|| err("mentor.disclosure_signal", "missing 'signal'"))?;

            let mut tracker: oracle::DisclosureTracker = serde_json::from_value(tracker_json.clone())
                .map_err(|e| err("mentor.disclosure_signal", e))?;
            let signal: oracle::DisclosureSignal = serde_json::from_value(signal_json.clone())
                .map_err(|e| err("mentor.disclosure_signal", e))?;

            let old_tier = tracker.tier();
            tracker.record(&signal);
            let new_tier = tracker.tier();

            if old_tier != new_tier {
                s.email.send_raw("mentor.tier_changed", &serde_json::to_vec(&json!({
                    "from": format!("{old_tier:?}"),
                    "to": format!("{new_tier:?}")
                })).unwrap_or_default());
            }

            let result = serde_json::to_value(&tracker)
                .map_err(|e| err("mentor.disclosure_signal", e))?;
            ok_json(&result)
        });

        // ── oracle.disclosure_tier ────────────────────────────────
        // Get the current tier from a tracker.
        state.phone.register_raw("mentor.disclosure_tier", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let tracker_json = params.get("tracker")
                .ok_or_else(|| err("mentor.disclosure_tier", "missing 'tracker'"))?;

            let tracker: oracle::DisclosureTracker = serde_json::from_value(tracker_json.clone())
                .map_err(|e| err("mentor.disclosure_tier", e))?;

            let tier = tracker.tier();
            let tier_json = serde_json::to_value(&tier)
                .map_err(|e| err("mentor.disclosure_tier", e))?;
            ok_json(&json!({
                "tier": tier_json,
                "feature_visibility": format!("{:?}", oracle::FeatureVisibility::for_tier(tier))
            }))
        });

        // ── oracle.disclosure_override ────────────────────────────
        // Manually override the tier on a tracker.
        let s = state.clone();
        state.phone.register_raw("mentor.disclosure_override", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let tracker_json = params.get("tracker")
                .ok_or_else(|| err("mentor.disclosure_override", "missing 'tracker'"))?;
            let tier_json = params.get("tier")
                .ok_or_else(|| err("mentor.disclosure_override", "missing 'tier'"))?;

            let mut tracker: oracle::DisclosureTracker = serde_json::from_value(tracker_json.clone())
                .map_err(|e| err("mentor.disclosure_override", e))?;
            let tier: oracle::SovereigntyTier = serde_json::from_value(tier_json.clone())
                .map_err(|e| err("mentor.disclosure_override", e))?;

            tracker.set_tier(tier);

            s.email.send_raw("mentor.tier_changed", &serde_json::to_vec(&json!({
                "to": format!("{tier:?}"),
                "manual": true
            })).unwrap_or_default());

            let result = serde_json::to_value(&tracker)
                .map_err(|e| err("mentor.disclosure_override", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Hints
        // ═══════════════════════════════════════════════════════════

        // ── oracle.hint_context_create ─────────────────────────────
        // Create a hint context from key-value pairs.
        state.phone.register_raw("mentor.hint_context_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let values = params.get("values").and_then(|v| v.as_object());

            let mut ctx = oracle::HintContext::new();
            if let Some(obj) = values {
                for (k, v) in obj {
                    if let Some(val) = v.as_str() {
                        ctx.set(k, val);
                    }
                }
            }

            let ctx_json = serde_json::to_value(&ctx)
                .map_err(|e| err("mentor.hint_context_create", e))?;
            ok_json(&ctx_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Activation Flow
        // ═══════════════════════════════════════════════════════════

        // ── oracle.activation_progress ────────────────────────────
        // Get the progress of an activation flow (serialized).
        state.phone.register_raw("mentor.activation_progress", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let flow_json = params.get("flow")
                .ok_or_else(|| err("mentor.activation_progress", "missing 'flow'"))?;

            // ActivationFlow isn't Serialize/Deserialize (it holds trait objects),
            // so we expose progress as a JSON summary instead.
            let total = flow_json.get("total_steps").and_then(|v| v.as_u64()).unwrap_or(0);
            let completed = flow_json.get("completed_steps").and_then(|v| v.as_u64()).unwrap_or(0);
            let is_complete = completed >= total && total > 0;

            ok_json(&json!({
                "total_steps": total,
                "completed_steps": completed,
                "is_complete": is_complete,
                "progress": if total > 0 { completed as f64 / total as f64 } else { 0.0 }
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Recovery
        // ═══════════════════════════════════════════════════════════

        // ── oracle.recovery_status_types ──────────────────────────
        // Get the list of possible recovery statuses.
        state.phone.register_raw("mentor.recovery_status_types", move |_data| {
            ok_json(&json!({
                "statuses": ["Idle", "AwaitingInput", "Verifying", "Syncing", "Complete", "Failed"]
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Workflows
        // ═══════════════════════════════════════════════════════════

        // ── oracle.workflow_create ─────────────────────────────────
        // Create a workflow from trigger/conditions/actions.
        state.phone.register_raw("mentor.workflow_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let workflow_json = params.get("workflow")
                .ok_or_else(|| err("mentor.workflow_create", "missing 'workflow'"))?;

            let workflow: oracle::Workflow = serde_json::from_value(workflow_json.clone())
                .map_err(|e| err("mentor.workflow_create", e))?;

            let result = serde_json::to_value(&workflow)
                .map_err(|e| err("mentor.workflow_create", e))?;
            ok_json(&result)
        });

        // ── oracle.workflow_trigger_create ─────────────────────────
        // Create a trigger for a specific event kind.
        state.phone.register_raw("mentor.workflow_trigger_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let kind = params.get("kind").and_then(|v| v.as_u64()).map(|k| k as u32);
            let author = params.get("author").and_then(|v| v.as_str());

            let trigger = if let Some(k) = kind {
                oracle::Trigger::on_kind(k)
            } else if let Some(a) = author {
                oracle::Trigger::on_author(a)
            } else {
                return Err(err("mentor.workflow_trigger_create", "must specify 'kind' or 'author'"));
            };

            let trigger_json = serde_json::to_value(&trigger)
                .map_err(|e| err("mentor.workflow_trigger_create", e))?;
            ok_json(&trigger_json)
        });

        // ── oracle.workflow_condition_evaluate ─────────────────────
        // Evaluate a condition against a context.
        state.phone.register_raw("mentor.workflow_condition_evaluate", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let condition_json = params.get("condition")
                .ok_or_else(|| err("mentor.workflow_condition_evaluate", "missing 'condition'"))?;
            let context_json = params.get("context")
                .ok_or_else(|| err("mentor.workflow_condition_evaluate", "missing 'context'"))?;

            let condition: oracle::Condition = serde_json::from_value(condition_json.clone())
                .map_err(|e| err("mentor.workflow_condition_evaluate", e))?;
            let context: std::collections::HashMap<String, String> = serde_json::from_value(context_json.clone())
                .map_err(|e| err("mentor.workflow_condition_evaluate", e))?;

            let result = condition.evaluate(&context);
            ok_json(&json!({ "result": result }))
        });

        // ═══════════════════════════════════════════════════════════
        // Sovereignty Tier Utilities
        // ═══════════════════════════════════════════════════════════

        // ── oracle.tiers_list ─────────────────────────────────────
        // List all sovereignty tiers.
        state.phone.register_raw("mentor.tiers_list", move |_data| {
            let tiers: Vec<Value> = oracle::SovereigntyTier::all().iter()
                .map(|t| {
                    let visibility = oracle::FeatureVisibility::for_tier(*t);
                    json!({
                        "tier": format!("{t:?}"),
                        "feature_visibility": format!("{visibility:?}")
                    })
                })
                .collect();
            ok_json(&Value::Array(tiers))
        });

        // ── oracle.feature_visibility ─────────────────────────────
        // Get the feature visibility for a tier.
        state.phone.register_raw("mentor.feature_visibility", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let tier_json = params.get("tier")
                .ok_or_else(|| err("mentor.feature_visibility", "missing 'tier'"))?;

            let tier: oracle::SovereigntyTier = serde_json::from_value(tier_json.clone())
                .map_err(|e| err("mentor.feature_visibility", e))?;

            let visibility = oracle::FeatureVisibility::for_tier(tier);
            let visibility_json = serde_json::to_value(&visibility)
                .map_err(|e| err("mentor.feature_visibility", e))?;
            ok_json(&visibility_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Sovereignty tiers
            .with_call(CallDescriptor::new("mentor.tier_defaults", "Defaults for a tier"))
            .with_call(CallDescriptor::new("mentor.tier_all_defaults", "All tier defaults"))
            .with_call(CallDescriptor::new("mentor.tiers_list", "List all tiers"))
            .with_call(CallDescriptor::new("mentor.feature_visibility", "Feature visibility for tier"))
            // Disclosure tracking
            .with_call(CallDescriptor::new("mentor.disclosure_create", "Create disclosure tracker"))
            .with_call(CallDescriptor::new("mentor.disclosure_signal", "Record disclosure signal"))
            .with_call(CallDescriptor::new("mentor.disclosure_tier", "Get current tier"))
            .with_call(CallDescriptor::new("mentor.disclosure_override", "Override tier manually"))
            // Hints
            .with_call(CallDescriptor::new("mentor.hint_context_create", "Create hint context"))
            // Activation
            .with_call(CallDescriptor::new("mentor.activation_progress", "Get activation progress"))
            // Recovery
            .with_call(CallDescriptor::new("mentor.recovery_status_types", "List recovery statuses"))
            // Workflows
            .with_call(CallDescriptor::new("mentor.workflow_create", "Create workflow"))
            .with_call(CallDescriptor::new("mentor.workflow_trigger_create", "Create trigger"))
            .with_call(CallDescriptor::new("mentor.workflow_condition_evaluate", "Evaluate condition"))
            // Events
            .with_emitted_event(EventDescriptor::new("mentor.tier_changed", "Sovereignty tier changed"))
    }
}
