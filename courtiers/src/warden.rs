//! Warden module — Bulwark safety courtier.
//!
//! Exposes safety and protection primitives: trust layer queries, permission
//! checks, kids sphere access, behavioral drift detection, power concentration
//! analysis, reputation scoring, health monitoring, and age tier classification.
//!
//! Bulwark shields without watching — care, not surveillance.
//! All operations are check/query-oriented with no persistent state.

use std::sync::Arc;

use equipment::{CallDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct WardenModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for WardenModule {
    fn id(&self) -> &str { "warden" }
    fn name(&self) -> &str { "Warden (Bulwark)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Trust Layers
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.trust_capabilities ────────────────────────────
        // Get capabilities for a trust layer.
        state.phone.register_raw("warden.trust_capabilities", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let layer_json = params.get("layer")
                .ok_or_else(|| err("warden.trust_capabilities", "missing 'layer'"))?;
            let layer: bulwark::TrustLayer = serde_json::from_value(layer_json.clone())
                .map_err(|e| err("warden.trust_capabilities", format!("invalid layer: {e}")))?;
            let caps = layer.capabilities();
            let caps_json = serde_json::to_value(&caps)
                .map_err(|e| err("warden.trust_capabilities", e))?;
            ok_json(&caps_json)
        });

        // ── bulwark.bond_capabilities ─────────────────────────────
        // Get capabilities for a bond depth.
        state.phone.register_raw("warden.bond_capabilities", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let depth_json = params.get("depth")
                .ok_or_else(|| err("warden.bond_capabilities", "missing 'depth'"))?;
            let depth: bulwark::BondDepth = serde_json::from_value(depth_json.clone())
                .map_err(|e| err("warden.bond_capabilities", format!("invalid depth: {e}")))?;
            let caps = depth.capabilities();
            let caps_json = serde_json::to_value(&caps)
                .map_err(|e| err("warden.bond_capabilities", e))?;
            ok_json(&caps_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Permission Checks
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.check_permission ──────────────────────────────
        // Check if an actor can perform an action on a resource.
        state.phone.register_raw("warden.check_permission", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let checker_json = params.get("checker")
                .ok_or_else(|| err("warden.check_permission", "missing 'checker'"))?;
            let checker: bulwark::PermissionChecker = serde_json::from_value(checker_json.clone())
                .map_err(|e| err("warden.check_permission", format!("invalid checker: {e}")))?;
            let actor_json = params.get("actor")
                .ok_or_else(|| err("warden.check_permission", "missing 'actor'"))?;
            let actor: bulwark::ActorContext = serde_json::from_value(actor_json.clone())
                .map_err(|e| err("warden.check_permission", format!("invalid actor: {e}")))?;
            let action = params.get("action").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.check_permission", "missing 'action'"))?;
            let resource = params.get("resource").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.check_permission", "missing 'resource'"))?;

            let action_obj = bulwark::Action::new(action);
            let resource_obj = bulwark::ResourceScope::new(resource);
            let allowed = checker.can(&actor, &action_obj, &resource_obj);
            ok_json(&json!({ "allowed": allowed }))
        });

        // ═══════════════════════════════════════════════════════════
        // Age Tiers
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.age_tier ──────────────────────────────────────
        // Determine the age tier for a given age.
        state.phone.register_raw("warden.age_tier", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let age = params.get("age").and_then(|v| v.as_u64())
                .ok_or_else(|| err("warden.age_tier", "missing 'age'"))? as u8;
            let config = bulwark::AgeTierConfig::default();
            let tier = bulwark::AgeTier::from_age(age, &config);
            let tier_json = serde_json::to_value(&tier)
                .map_err(|e| err("warden.age_tier", e))?;
            ok_json(&tier_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Kids Sphere
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.kids_sphere_access ─────────────────────────────
        // Check kids sphere access for a pubkey.
        state.phone.register_raw("warden.kids_sphere_access", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = params.get("pubkey").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.kids_sphere_access", "missing 'pubkey'"))?;
            let community_id = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.kids_sphere_access", "missing 'community_id'"))?;
            let registry = bulwark::KidsSphereExclusionRegistry::new();
            let approval: Option<bulwark::KidsSphereApproval> = params.get("approval")
                .and_then(|v| serde_json::from_value(v.clone()).ok());

            let result = bulwark::check_kids_sphere_access(
                pubkey,
                community_id,
                &registry,
                approval.as_ref(),
            );
            let status = match result {
                bulwark::KidsSphereAccessResult::Allowed => "allowed",
                bulwark::KidsSphereAccessResult::DeniedExcluded => "denied_excluded",
                bulwark::KidsSphereAccessResult::DeniedNoApproval => "denied_no_approval",
                bulwark::KidsSphereAccessResult::DeniedExpired => "denied_expired",
                bulwark::KidsSphereAccessResult::Suspended => "suspended",
            };
            ok_json(&json!({
                "result": status,
                "allowed": result.is_allowed(),
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Behavioral Drift Detection
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.compute_drift ─────────────────────────────────
        // Compute behavioral drift from a baseline and recent activities.
        state.phone.register_raw("warden.compute_drift", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let baseline_json = params.get("baseline")
                .ok_or_else(|| err("warden.compute_drift", "missing 'baseline'"))?;
            let baseline: bulwark::BehavioralBaseline = serde_json::from_value(baseline_json.clone())
                .map_err(|e| err("warden.compute_drift", format!("invalid baseline: {e}")))?;
            let activities_json = params.get("activities")
                .ok_or_else(|| err("warden.compute_drift", "missing 'activities'"))?;
            let activities: Vec<bulwark::Activity> = serde_json::from_value(activities_json.clone())
                .map_err(|e| err("warden.compute_drift", format!("invalid activities: {e}")))?;
            let period_weeks = params.get("period_weeks").and_then(|v| v.as_f64())
                .ok_or_else(|| err("warden.compute_drift", "missing 'period_weeks'"))?;
            let proposals = params.get("proposals_available").and_then(|v| v.as_u64()).unwrap_or(0);
            let votes = params.get("votes_cast").and_then(|v| v.as_u64()).unwrap_or(0);
            let role_changes = params.get("role_changes").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

            let drift = bulwark::DriftComputer::compute(
                &baseline, &activities, period_weeks, proposals, votes, role_changes,
            );
            let drift_json = serde_json::to_value(&drift)
                .map_err(|e| err("warden.compute_drift", e))?;
            ok_json(&drift_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Power Concentration Index
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.compute_power_index ───────────────────────────
        // Compute power concentration index for a community.
        state.phone.register_raw("warden.compute_power_index", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let community_id = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.compute_power_index", "missing 'community_id'"))?;
            let member_roles: Vec<Vec<String>> = params.get("member_roles")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .ok_or_else(|| err("warden.compute_power_index", "missing 'member_roles'"))?;
            // proposal_authors: ["alice", "bob", "alice"]
            let proposal_authors_owned: Vec<String> = params.get("proposal_authors")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let proposal_authors_refs: Vec<&str> = proposal_authors_owned.iter().map(|s| s.as_str()).collect();
            // deciding_votes: [["alice", true], ["bob", false]]
            let deciding_votes_raw: Vec<(String, bool)> = params.get("deciding_votes")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let deciding_votes_refs: Vec<(&str, bool)> = deciding_votes_raw.iter()
                .map(|(s, b)| (s.as_str(), *b)).collect();
            // content_authorship: [["alice", 10], ["bob", 5]]
            let content_authorship_raw: Vec<(String, u64)> = params.get("content_authorship")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let content_authorship_refs: Vec<(&str, u64)> = content_authorship_raw.iter()
                .map(|(s, n)| (s.as_str(), *n)).collect();
            let exit_barriers: Vec<f64> = params.get("exit_barriers")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let config = params.get("config")
                .and_then(|v| serde_json::from_value::<bulwark::PowerConcentrationConfig>(v.clone()).ok())
                .unwrap_or_default();

            let index = bulwark::PowerConcentrationComputer::compute(
                community_id,
                &member_roles,
                &proposal_authors_refs,
                &deciding_votes_refs,
                &content_authorship_refs,
                &exit_barriers,
                &config,
            );
            let index_json = serde_json::to_value(&index)
                .map_err(|e| err("warden.compute_power_index", e))?;
            ok_json(&index_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Health Monitoring
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.user_health ───────────────────────────────────
        // Compute user health pulse from factors.
        state.phone.register_raw("warden.user_health", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = params.get("pubkey").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.user_health", "missing 'pubkey'"))?;
            let factors_json = params.get("factors")
                .ok_or_else(|| err("warden.user_health", "missing 'factors'"))?;
            let factors: bulwark::UserHealthFactors = serde_json::from_value(factors_json.clone())
                .map_err(|e| err("warden.user_health", format!("invalid factors: {e}")))?;
            let pulse = bulwark::UserHealthPulse::compute(pubkey, factors);
            let pulse_json = serde_json::to_value(&pulse)
                .map_err(|e| err("warden.user_health", e))?;
            ok_json(&pulse_json)
        });

        // ── bulwark.collective_health ─────────────────────────────
        // Compute collective health pulse from factors.
        state.phone.register_raw("warden.collective_health", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let collective_id_str = params.get("collective_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.collective_health", "missing 'collective_id'"))?;
            let collective_id = uuid::Uuid::parse_str(collective_id_str)
                .map_err(|e| err("warden.collective_health", format!("invalid UUID: {e}")))?;
            let factors_json = params.get("factors")
                .ok_or_else(|| err("warden.collective_health", "missing 'factors'"))?;
            let factors: bulwark::CollectiveHealthFactors = serde_json::from_value(factors_json.clone())
                .map_err(|e| err("warden.collective_health", format!("invalid factors: {e}")))?;
            let contributing_members = params.get("contributing_members").and_then(|v| v.as_u64())
                .ok_or_else(|| err("warden.collective_health", "missing 'contributing_members'"))? as u32;
            let pulse = bulwark::CollectiveHealthPulse::compute(collective_id, factors, contributing_members);
            let pulse_json = serde_json::to_value(&pulse)
                .map_err(|e| err("warden.collective_health", e))?;
            ok_json(&pulse_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Consent
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.validate_consent ──────────────────────────────
        // Validate a Bulwark consent record.
        state.phone.register_raw("warden.validate_consent", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let record_json = params.get("record")
                .ok_or_else(|| err("warden.validate_consent", "missing 'record'"))?;
            let record: bulwark::ConsentRecord = serde_json::from_value(record_json.clone())
                .map_err(|e| err("warden.validate_consent", format!("invalid record: {e}")))?;
            ok_json(&json!({
                "active": record.is_active(),
                "revoked": record.revoked_at.is_some(),
                "expired": record.expires_at.map_or(false, |exp| chrono::Utc::now() >= exp),
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Child Safety Protocol
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.child_safety_protocol ─────────────────────────
        // Get the immutable child safety protocol steps.
        state.phone.register_raw("warden.child_safety_protocol", move |_data| {
            let protocol = bulwark::ChildSafetyProtocol::default();
            let protocol_json = serde_json::to_value(&protocol)
                .map_err(|e| err("warden.child_safety_protocol", e))?;
            ok_json(&protocol_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Network Origin
        // ═══════════════════════════════════════════════════════════

        // ── bulwark.network_origin ─────────────────────────────────
        // Create a network origin and check its bootstrap phase.
        state.phone.register_raw("warden.network_origin", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let origin_pubkey = params.get("origin_pubkey").and_then(|v| v.as_str())
                .ok_or_else(|| err("warden.network_origin", "missing 'origin_pubkey'"))?;
            let threshold = params.get("threshold").and_then(|v| v.as_u64()).unwrap_or(100);

            let origin = bulwark::NetworkOrigin::new(origin_pubkey, threshold);
            let origin_json = serde_json::to_value(&origin)
                .map_err(|e| err("warden.network_origin", e))?;
            ok_json(&origin_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Trust layers
            .with_call(CallDescriptor::new("warden.trust_capabilities", "Get trust layer capabilities"))
            .with_call(CallDescriptor::new("warden.bond_capabilities", "Get bond depth capabilities"))
            // Permissions
            .with_call(CallDescriptor::new("warden.check_permission", "Check actor permission"))
            // Age tiers
            .with_call(CallDescriptor::new("warden.age_tier", "Classify age into tier"))
            // Kids sphere
            .with_call(CallDescriptor::new("warden.kids_sphere_access", "Check kids sphere access"))
            // Behavioral drift
            .with_call(CallDescriptor::new("warden.compute_drift", "Compute behavioral drift"))
            // Power concentration
            .with_call(CallDescriptor::new("warden.compute_power_index", "Compute power index"))
            // Health
            .with_call(CallDescriptor::new("warden.user_health", "Compute user health pulse"))
            .with_call(CallDescriptor::new("warden.collective_health", "Compute collective health"))
            // Consent
            .with_call(CallDescriptor::new("warden.validate_consent", "Validate consent record"))
            // Child safety
            .with_call(CallDescriptor::new("warden.child_safety_protocol", "Get child safety protocol"))
            // Network origin
            .with_call(CallDescriptor::new("warden.network_origin", "Create network origin"))
    }
}
