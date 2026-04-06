//! Tribune module — Polity constitutional guard courtier.
//!
//! Exposes constitutional review, rights/duties/protections registries,
//! breach detection, consent validation, immutable foundation checks,
//! amendment validation, and anti-weaponization checks.
//!
//! Polity is the Covenant's immune system — it guards, not governs.
//! All operations are query/check-oriented with no persistent state.

use std::sync::Arc;

use equipment::{CallDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct TribuneModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for TribuneModule {
    fn id(&self) -> &str { "tribune" }
    fn name(&self) -> &str { "Tribune (Polity)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Constitutional Review
        // ═══════════════════════════════════════════════════════════

        // ── polity.review ─────────────────────────────────────────
        // Perform constitutional review on an action description.
        state.phone.register_raw("tribune.review", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let description = params.get("description").and_then(|v| v.as_str())
                .ok_or_else(|| err("tribune.review", "missing 'description'"))?;
            let actor = params.get("actor").and_then(|v| v.as_str())
                .ok_or_else(|| err("tribune.review", "missing 'actor'"))?;

            // Parse any violation types declared by the action
            let violates: Vec<polity::ProhibitionType> = params.get("violates")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let action = polity::ActionDescription {
                description: description.to_string(),
                actor: actor.to_string(),
                violates,
            };

            // Build registries with Covenant defaults
            let rights_registry = polity::RightsRegistry::with_covenant_rights();
            let protections_registry = polity::ProtectionsRegistry::with_covenant_protections();
            let reviewer = polity::ConstitutionalReviewer::new(&rights_registry, &protections_registry);

            let review = reviewer.review(&action);
            let review_json = serde_json::to_value(&review)
                .map_err(|e| err("tribune.review", e))?;
            ok_json(&review_json)
        });

        // ── polity.is_prohibited ──────────────────────────────────
        // Quick check: does an action violate any absolute prohibition?
        state.phone.register_raw("tribune.is_prohibited", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let description = params.get("description").and_then(|v| v.as_str())
                .ok_or_else(|| err("tribune.is_prohibited", "missing 'description'"))?;
            let actor = params.get("actor").and_then(|v| v.as_str()).unwrap_or("unknown");

            let violates: Vec<polity::ProhibitionType> = params.get("violates")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let action = polity::ActionDescription {
                description: description.to_string(),
                actor: actor.to_string(),
                violates,
            };

            let rights_registry = polity::RightsRegistry::with_covenant_rights();
            let protections_registry = polity::ProtectionsRegistry::with_covenant_protections();
            let reviewer = polity::ConstitutionalReviewer::new(&rights_registry, &protections_registry);

            let prohibited = reviewer.is_absolutely_prohibited(&action);
            ok_json(&json!({ "prohibited": prohibited }))
        });

        // ═══════════════════════════════════════════════════════════
        // Immutable Foundation
        // ═══════════════════════════════════════════════════════════

        // ── polity.immutable_rights ───────────────────────────────
        // Get the immutable right categories.
        state.phone.register_raw("tribune.immutable_rights", move |_data| {
            let rights: Vec<Value> = polity::ImmutableFoundation::IMMUTABLE_RIGHTS
                .iter()
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
                .collect();
            ok_json(&Value::Array(rights))
        });

        // ── polity.absolute_prohibitions ──────────────────────────
        // Get the absolute prohibition types.
        state.phone.register_raw("tribune.absolute_prohibitions", move |_data| {
            let prohibitions: Vec<Value> = polity::ImmutableFoundation::ABSOLUTE_PROHIBITIONS
                .iter()
                .map(|p| serde_json::to_value(p).unwrap_or(Value::Null))
                .collect();
            ok_json(&Value::Array(prohibitions))
        });

        // ── polity.axioms ─────────────────────────────────────────
        // Get the three axioms.
        state.phone.register_raw("tribune.axioms", move |_data| {
            let axioms: Vec<Value> = polity::ImmutableFoundation::AXIOMS
                .iter()
                .map(|a| Value::String(a.to_string()))
                .collect();
            ok_json(&Value::Array(axioms))
        });

        // ── polity.would_violate ──────────────────────────────────
        // Heuristic check whether a proposed change would violate foundations.
        state.phone.register_raw("tribune.would_violate", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let description = params.get("description").and_then(|v| v.as_str())
                .ok_or_else(|| err("tribune.would_violate", "missing 'description'"))?;
            let violates = polity::ImmutableFoundation::would_violate(description);
            ok_json(&json!({ "violates": violates }))
        });

        // ═══════════════════════════════════════════════════════════
        // Consent
        // ═══════════════════════════════════════════════════════════

        // ── polity.create_consent ─────────────────────────────────
        // Create a new consent record.
        state.phone.register_raw("tribune.create_consent", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let grantor = params.get("grantor").and_then(|v| v.as_str())
                .ok_or_else(|| err("tribune.create_consent", "missing 'grantor'"))?;
            let recipient = params.get("recipient").and_then(|v| v.as_str())
                .ok_or_else(|| err("tribune.create_consent", "missing 'recipient'"))?;
            let scope_json = params.get("scope")
                .ok_or_else(|| err("tribune.create_consent", "missing 'scope'"))?;
            let scope: polity::ConsentScope = serde_json::from_value(scope_json.clone())
                .map_err(|e| err("tribune.create_consent", format!("invalid scope: {e}")))?;

            let record = polity::ConsentRecord::new(grantor, recipient, scope);
            let record_json = serde_json::to_value(&record)
                .map_err(|e| err("tribune.create_consent", e))?;
            ok_json(&record_json)
        });

        // ── polity.validate_consent ───────────────────────────────
        // Validate a consent record (check if active, expired, revoked).
        state.phone.register_raw("tribune.validate_consent", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let record_json = params.get("record")
                .ok_or_else(|| err("tribune.validate_consent", "missing 'record'"))?;
            let record: polity::ConsentRecord = serde_json::from_value(record_json.clone())
                .map_err(|e| err("tribune.validate_consent", format!("invalid record: {e}")))?;

            ok_json(&json!({
                "active": record.is_active(),
                "revoked": record.is_revoked(),
                "expired": record.is_expired(),
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Breach Detection
        // ═══════════════════════════════════════════════════════════

        // ── polity.create_breach ──────────────────────────────────
        // Create a breach record from JSON description.
        state.phone.register_raw("tribune.create_breach", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let breach_json = params.get("breach")
                .ok_or_else(|| err("tribune.create_breach", "missing 'breach'"))?;
            let breach: polity::Breach = serde_json::from_value(breach_json.clone())
                .map_err(|e| err("tribune.create_breach", format!("invalid breach: {e}")))?;
            let result = serde_json::to_value(&breach)
                .map_err(|e| err("tribune.create_breach", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Anti-Weaponization
        // ═══════════════════════════════════════════════════════════

        // ── polity.check_invocation ───────────────────────────────
        // Check a rights invocation against anti-weaponization constraints.
        state.phone.register_raw("tribune.check_invocation", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let invocation_json = params.get("invocation")
                .ok_or_else(|| err("tribune.check_invocation", "missing 'invocation'"))?;
            let invocation: polity::RightInvocation = serde_json::from_value(invocation_json.clone())
                .map_err(|e| err("tribune.check_invocation", format!("invalid invocation: {e}")))?;

            let result = polity::InvocationCheck::check(&invocation);
            let result_json = serde_json::to_value(&result)
                .map_err(|e| err("tribune.check_invocation", e))?;
            ok_json(&result_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Covenant Code Validation
        // ═══════════════════════════════════════════════════════════

        // ── polity.covenant_axioms ────────────────────────────────
        // Get the Covenant axiom constants.
        state.phone.register_raw("tribune.covenant_axioms", move |_data| {
            let axioms: Vec<Value> = polity::COVENANT_AXIOMS
                .iter()
                .map(|a| Value::String(a.to_string()))
                .collect();
            ok_json(&Value::Array(axioms))
        });

        // ── polity.default_oath ───────────────────────────────────
        // Get the default enactment oath.
        state.phone.register_raw("tribune.default_oath", move |_data| {
            ok_json(&json!({ "oath": polity::DEFAULT_OATH }))
        });

        // ═══════════════════════════════════════════════════════════
        // Rights Registry
        // ═══════════════════════════════════════════════════════════

        // ── polity.list_rights ────────────────────────────────────
        // List all Covenant rights from the registry.
        state.phone.register_raw("tribune.list_rights", move |_data| {
            let registry = polity::RightsRegistry::with_covenant_rights();
            let rights_json = serde_json::to_value(&registry)
                .map_err(|e| err("tribune.list_rights", e))?;
            ok_json(&rights_json)
        });

        // ── polity.list_duties ────────────────────────────────────
        // List all Covenant duties from the registry.
        state.phone.register_raw("tribune.list_duties", move |_data| {
            let registry = polity::DutiesRegistry::with_covenant_duties();
            let duties_json = serde_json::to_value(&registry)
                .map_err(|e| err("tribune.list_duties", e))?;
            ok_json(&duties_json)
        });

        // ── polity.list_protections ───────────────────────────────
        // List all Covenant protections from the registry.
        state.phone.register_raw("tribune.list_protections", move |_data| {
            let registry = polity::ProtectionsRegistry::with_covenant_protections();
            let protections_json = serde_json::to_value(&registry)
                .map_err(|e| err("tribune.list_protections", e))?;
            ok_json(&protections_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Review
            .with_call(CallDescriptor::new("tribune.review", "Constitutional review of action"))
            .with_call(CallDescriptor::new("tribune.is_prohibited", "Check absolute prohibition"))
            // Immutable foundation
            .with_call(CallDescriptor::new("tribune.immutable_rights", "List immutable rights"))
            .with_call(CallDescriptor::new("tribune.absolute_prohibitions", "List absolute prohibitions"))
            .with_call(CallDescriptor::new("tribune.axioms", "Get three axioms"))
            .with_call(CallDescriptor::new("tribune.would_violate", "Heuristic foundation violation check"))
            // Consent
            .with_call(CallDescriptor::new("tribune.create_consent", "Create consent record"))
            .with_call(CallDescriptor::new("tribune.validate_consent", "Validate consent state"))
            // Breach
            .with_call(CallDescriptor::new("tribune.create_breach", "Create breach record"))
            // Anti-weaponization
            .with_call(CallDescriptor::new("tribune.check_invocation", "Check rights invocation"))
            // Covenant code
            .with_call(CallDescriptor::new("tribune.covenant_axioms", "Get Covenant axiom constants"))
            .with_call(CallDescriptor::new("tribune.default_oath", "Get default enactment oath"))
            // Registries
            .with_call(CallDescriptor::new("tribune.list_rights", "List all Covenant rights"))
            .with_call(CallDescriptor::new("tribune.list_duties", "List all Covenant duties"))
            .with_call(CallDescriptor::new("tribune.list_protections", "List all Covenant protections"))
    }
}
