//! Marshal module — Jail accountability courtier.
//!
//! Exposes accountability primitives: trust graph operations, flag raising
//! and pattern detection, graduated response, community admission checks,
//! appeal lifecycle, accused rights, and anti-weaponization detection.
//!
//! Jail holds accountable without punishing — restorative, not retributive.
//! Most operations construct or query data structures. The trust graph is
//! built per-request from provided data (no persistent graph in DaemonState yet).

use std::sync::Arc;

use equipment::{CallDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct MarshalModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for MarshalModule {
    fn id(&self) -> &str { "marshal" }
    fn name(&self) -> &str { "Marshal (Jail)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Trust Graph
        // ═══════════════════════════════════════════════════════════

        // ── jail.create_graph ─────────────────────────────────────
        // Create an empty trust graph (returned as JSON for client-side use).
        state.phone.register_raw("marshal.create_graph", move |_data| {
            let graph = jail::TrustGraph::new();
            let graph_json = serde_json::to_value(&graph)
                .map_err(|e| err("marshal.create_graph", e))?;
            ok_json(&graph_json)
        });

        // ── jail.add_edge ─────────────────────────────────────────
        // Add a verification edge to a trust graph.
        state.phone.register_raw("marshal.add_edge", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let graph_json = params.get("graph")
                .ok_or_else(|| err("marshal.add_edge", "missing 'graph'"))?;
            let mut graph: jail::TrustGraph = serde_json::from_value(graph_json.clone())
                .map_err(|e| err("marshal.add_edge", format!("invalid graph: {e}")))?;
            let edge_json = params.get("edge")
                .ok_or_else(|| err("marshal.add_edge", "missing 'edge'"))?;
            let edge: jail::VerificationEdge = serde_json::from_value(edge_json.clone())
                .map_err(|e| err("marshal.add_edge", format!("invalid edge: {e}")))?;

            graph.add_edge(edge).map_err(|e| err("marshal.add_edge", e))?;
            let graph_json = serde_json::to_value(&graph)
                .map_err(|e| err("marshal.add_edge", e))?;
            ok_json(&graph_json)
        });

        // ── jail.query_intelligence ───────────────────────────────
        // Query network intelligence for a target from a querier's perspective.
        state.phone.register_raw("marshal.query_intelligence", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let graph_json = params.get("graph")
                .ok_or_else(|| err("marshal.query_intelligence", "missing 'graph'"))?;
            let graph: jail::TrustGraph = serde_json::from_value(graph_json.clone())
                .map_err(|e| err("marshal.query_intelligence", format!("invalid graph: {e}")))?;
            let querier = params.get("querier").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.query_intelligence", "missing 'querier'"))?;
            let target = params.get("target").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.query_intelligence", "missing 'target'"))?;
            let all_flags: Vec<jail::AccountabilityFlag> = params.get("flags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let config = jail::JailConfig::default();
            let scope = params.get("scope")
                .and_then(|v| serde_json::from_value::<jail::FederationScope>(v.clone()).ok())
                .unwrap_or_default();
            let intel = jail::query_intelligence_scoped(
                &graph, querier, target, &all_flags, &config, &scope,
            );
            let intel_json = serde_json::to_value(&intel)
                .map_err(|e| err("marshal.query_intelligence", e))?;
            ok_json(&intel_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Flags
        // ═══════════════════════════════════════════════════════════

        // ── jail.raise_flag ───────────────────────────────────────
        // Create an accountability flag.
        state.phone.register_raw("marshal.raise_flag", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let flag_json = params.get("flag")
                .ok_or_else(|| err("marshal.raise_flag", "missing 'flag'"))?;
            let flag: jail::AccountabilityFlag = serde_json::from_value(flag_json.clone())
                .map_err(|e| err("marshal.raise_flag", format!("invalid flag: {e}")))?;
            let result = serde_json::to_value(&flag)
                .map_err(|e| err("marshal.raise_flag", e))?;
            ok_json(&result)
        });

        // ── jail.detect_pattern ───────────────────────────────────
        // Detect cross-community flag patterns for a target.
        state.phone.register_raw("marshal.detect_pattern", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let flags_json = params.get("flags")
                .ok_or_else(|| err("marshal.detect_pattern", "missing 'flags'"))?;
            let flags: Vec<jail::AccountabilityFlag> = serde_json::from_value(flags_json.clone())
                .map_err(|e| err("marshal.detect_pattern", format!("invalid flags: {e}")))?;
            let target = params.get("target").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.detect_pattern", "missing 'target'"))?;

            let config = jail::JailConfig::default();
            let pattern = jail::flag::detect_pattern(&flags, target, &config);
            let pattern_json = serde_json::to_value(&pattern)
                .map_err(|e| err("marshal.detect_pattern", e))?;
            ok_json(&pattern_json)
        });

        // ── jail.detect_weaponization ─────────────────────────────
        // Detect serial filing within a time window.
        state.phone.register_raw("marshal.detect_weaponization", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let flags_json = params.get("flags")
                .ok_or_else(|| err("marshal.detect_weaponization", "missing 'flags'"))?;
            let flags: Vec<jail::AccountabilityFlag> = serde_json::from_value(flags_json.clone())
                .map_err(|e| err("marshal.detect_weaponization", format!("invalid flags: {e}")))?;
            let window_days = params.get("window_days").and_then(|v| v.as_u64()).unwrap_or(30);
            let threshold = params.get("threshold").and_then(|v| v.as_u64()).unwrap_or(5) as usize;

            let serial = jail::flag::weaponization::detect_serial_filing(&flags, window_days, threshold);
            let result = json!({
                "serial_filing": serial.is_some(),
                "indicator": serial.map(|i| serde_json::to_value(&i).unwrap_or(Value::Null)),
            });
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Graduated Response
        // ═══════════════════════════════════════════════════════════

        // ── jail.begin_response ───────────────────────────────────
        // Begin a graduated response (starts at Education level).
        state.phone.register_raw("marshal.begin_response", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let target = params.get("target").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.begin_response", "missing 'target'"))?;
            let reason = params.get("reason").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.begin_response", "missing 'reason'"))?;
            let initiated_by = params.get("initiated_by").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.begin_response", "missing 'initiated_by'"))?;

            let response = jail::GraduatedResponse::begin(target, reason, initiated_by);
            let response_json = serde_json::to_value(&response)
                .map_err(|e| err("marshal.begin_response", e))?;
            ok_json(&response_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Admission
        // ═══════════════════════════════════════════════════════════

        // ── jail.check_admission ──────────────────────────────────
        // Check whether a prospect should be admitted to a community.
        state.phone.register_raw("marshal.check_admission", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let graph_json = params.get("graph")
                .ok_or_else(|| err("marshal.check_admission", "missing 'graph'"))?;
            let graph: jail::TrustGraph = serde_json::from_value(graph_json.clone())
                .map_err(|e| err("marshal.check_admission", format!("invalid graph: {e}")))?;
            let prospect = params.get("prospect").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.check_admission", "missing 'prospect'"))?;
            let community_id = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.check_admission", "missing 'community_id'"))?;
            let members: Vec<String> = params.get("members")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .ok_or_else(|| err("marshal.check_admission", "missing 'members'"))?;
            let flags: Vec<jail::AccountabilityFlag> = params.get("flags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let config = jail::JailConfig::default();

            let recommendation = jail::check_admission(
                &graph, prospect, community_id, &members, &flags, &config,
            );
            let rec_json = serde_json::to_value(&recommendation)
                .map_err(|e| err("marshal.check_admission", e))?;
            ok_json(&rec_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Rights (always on)
        // ═══════════════════════════════════════════════════════════

        // ── jail.accused_rights ───────────────────────────────────
        // Get the always-on accused rights.
        state.phone.register_raw("marshal.accused_rights", move |_data| {
            let rights = jail::AccusedRights::always();
            let rights_json = serde_json::to_value(&rights)
                .map_err(|e| err("marshal.accused_rights", e))?;
            ok_json(&rights_json)
        });

        // ── jail.reporter_protection ──────────────────────────────
        // Get reporter protection for a flag.
        state.phone.register_raw("marshal.reporter_protection", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let flag_id = params.get("flag_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.reporter_protection", "missing 'flag_id'"))?;
            let flag_uuid = uuid::Uuid::parse_str(flag_id)
                .map_err(|e| err("marshal.reporter_protection", format!("invalid UUID: {e}")))?;
            let reporter = params.get("reporter").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.reporter_protection", "missing 'reporter'"))?;
            let protection = jail::ReporterProtection::for_flag(reporter, flag_uuid);
            let protection_json = serde_json::to_value(&protection)
                .map_err(|e| err("marshal.reporter_protection", e))?;
            ok_json(&protection_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Appeal
        // ═══════════════════════════════════════════════════════════

        // ── jail.create_appeal ────────────────────────────────────
        // Create an appeal object.
        state.phone.register_raw("marshal.create_appeal", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let appeal_json = params.get("appeal")
                .ok_or_else(|| err("marshal.create_appeal", "missing 'appeal'"))?;
            let appeal: jail::Appeal = serde_json::from_value(appeal_json.clone())
                .map_err(|e| err("marshal.create_appeal", format!("invalid appeal: {e}")))?;
            let result = serde_json::to_value(&appeal)
                .map_err(|e| err("marshal.create_appeal", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Configuration
        // ═══════════════════════════════════════════════════════════

        // ── jail.default_config ───────────────────────────────────
        // Get the default Jail configuration.
        state.phone.register_raw("marshal.default_config", move |_data| {
            let config = jail::JailConfig::default();
            let config_json = serde_json::to_value(&config)
                .map_err(|e| err("marshal.default_config", e))?;
            ok_json(&config_json)
        });

        // ── jail.validate_config ──────────────────────────────────
        // Validate a Jail configuration against Covenant constraints.
        state.phone.register_raw("marshal.validate_config", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let config_json = params.get("config")
                .ok_or_else(|| err("marshal.validate_config", "missing 'config'"))?;
            let config: jail::JailConfig = serde_json::from_value(config_json.clone())
                .map_err(|e| err("marshal.validate_config", format!("invalid config: {e}")))?;
            let result = config.validate();
            ok_json(&json!({ "valid": result.is_ok(), "reason": result.err().map(|e| e.to_string()) }))
        });

        // ═══════════════════════════════════════════════════════════
        // Federation Scope
        // ═══════════════════════════════════════════════════════════

        // ── jail.query_flags_scoped ───────────────────────────────
        // Query flags visible within a federation scope.
        state.phone.register_raw("marshal.query_flags_scoped", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let flags_json = params.get("flags")
                .ok_or_else(|| err("marshal.query_flags_scoped", "missing 'flags'"))?;
            let flags: Vec<jail::AccountabilityFlag> = serde_json::from_value(flags_json.clone())
                .map_err(|e| err("marshal.query_flags_scoped", format!("invalid flags: {e}")))?;
            let target = params.get("target").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.query_flags_scoped", "missing 'target'"))?;
            let scope_json = params.get("scope")
                .ok_or_else(|| err("marshal.query_flags_scoped", "missing 'scope'"))?;
            let scope: jail::FederationScope = serde_json::from_value(scope_json.clone())
                .map_err(|e| err("marshal.query_flags_scoped", format!("invalid scope: {e}")))?;

            let graph = jail::TrustGraph::new();
            let querier = params.get("querier").and_then(|v| v.as_str())
                .ok_or_else(|| err("marshal.query_flags_scoped", "missing 'querier'"))?;
            let propagation_degrees = params.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
            let scoped_flags = jail::query_flags_scoped(&graph, querier, target, &flags, propagation_degrees, &scope);
            let scoped_json: Vec<Value> = scoped_flags.iter()
                .filter_map(|f| serde_json::to_value(f).ok())
                .collect();
            ok_json(&Value::Array(scoped_json))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Trust graph
            .with_call(CallDescriptor::new("marshal.create_graph", "Create empty trust graph"))
            .with_call(CallDescriptor::new("marshal.add_edge", "Add verification edge"))
            .with_call(CallDescriptor::new("marshal.query_intelligence", "Query network intelligence"))
            // Flags
            .with_call(CallDescriptor::new("marshal.raise_flag", "Raise accountability flag"))
            .with_call(CallDescriptor::new("marshal.detect_pattern", "Detect flag patterns"))
            .with_call(CallDescriptor::new("marshal.detect_weaponization", "Detect flag abuse"))
            // Response
            .with_call(CallDescriptor::new("marshal.begin_response", "Begin graduated response"))
            // Admission
            .with_call(CallDescriptor::new("marshal.check_admission", "Check community admission"))
            // Rights
            .with_call(CallDescriptor::new("marshal.accused_rights", "Get accused rights"))
            .with_call(CallDescriptor::new("marshal.reporter_protection", "Get reporter protection"))
            // Appeal
            .with_call(CallDescriptor::new("marshal.create_appeal", "Create appeal"))
            // Config
            .with_call(CallDescriptor::new("marshal.default_config", "Get default Jail config"))
            .with_call(CallDescriptor::new("marshal.validate_config", "Validate Jail config"))
            // Federation scope
            .with_call(CallDescriptor::new("marshal.query_flags_scoped", "Query flags in scope"))
    }
}
