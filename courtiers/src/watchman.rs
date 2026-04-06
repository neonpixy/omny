//! Watchman module — Undercroft system health courtier.
//!
//! Exposes deidentified health metrics, network aggregation, community
//! health, economic vitals, quest health, snapshots, and history as
//! daemon operations. All data is aggregate — NO pubkeys, NO individual
//! activity, NO relay URLs. Covenant requirement.
//!
//! Programs use staff name "watchman" which maps to daemon namespace "undercroft".

use std::sync::Arc;

use equipment::{CallDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct WatchmanModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for WatchmanModule {
    fn id(&self) -> &str { "watchman" }
    fn name(&self) -> &str { "Watchman (Undercroft)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Network Health (deidentified relay aggregation)
        // ═══════════════════════════════════════════════════════════

        // ── undercroft.network_health ─────────────────────────────
        // Aggregate network health from the Omnibus relay state.
        let s = state.clone();
        state.phone.register_raw("watchman.network_health", move |_data| {
            let snapshots = s.omnibus.omnibus().relay_health();
            let snapshot_json = serde_json::to_value(&snapshots)
                .map_err(|e| err("watchman.network_health", e))?;
            ok_json(&json!({
                "relay_count": snapshots.len(),
                "relays": snapshot_json
            }))
        });

        // ── undercroft.network_empty ──────────────────────────────
        // Get an empty network health snapshot (for bootstrapping).
        state.phone.register_raw("watchman.network_empty", move |_data| {
            let empty = undercroft::NetworkHealth::empty();
            let empty_json = serde_json::to_value(&empty)
                .map_err(|e| err("watchman.network_empty", e))?;
            ok_json(&empty_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Economic Health
        // ═══════════════════════════════════════════════════════════

        // ── undercroft.economic_health ─────────────────────────────
        // Get economic health from a treasury status.
        state.phone.register_raw("watchman.economic_health", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let treasury_json = params.get("treasury")
                .ok_or_else(|| err("watchman.economic_health", "missing 'treasury'"))?;

            let treasury: fortune::TreasuryStatus = serde_json::from_value(treasury_json.clone())
                .map_err(|e| err("watchman.economic_health", e))?;

            let economic = undercroft::EconomicHealth::from_treasury_status(&treasury);
            let economic_json = serde_json::to_value(&economic)
                .map_err(|e| err("watchman.economic_health", e))?;
            ok_json(&economic_json)
        });

        // ── undercroft.economic_empty ──────────────────────────────
        // Get empty economic health (for bootstrapping).
        state.phone.register_raw("watchman.economic_empty", move |_data| {
            let empty = undercroft::EconomicHealth::empty();
            let empty_json = serde_json::to_value(&empty)
                .map_err(|e| err("watchman.economic_empty", e))?;
            ok_json(&empty_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Community Health
        // ═══════════════════════════════════════════════════════════

        // ── undercroft.community_health ────────────────────────────
        // Get community health from a community + proposals.
        state.phone.register_raw("watchman.community_health", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let community_json = params.get("community")
                .ok_or_else(|| err("watchman.community_health", "missing 'community'"))?;
            let empty_proposals = Value::Array(vec![]);
            let proposals_json = params.get("proposals").unwrap_or(&empty_proposals);

            let community: kingdom::Community = serde_json::from_value(community_json.clone())
                .map_err(|e| err("watchman.community_health", e))?;
            let proposals: Vec<kingdom::Proposal> = serde_json::from_value(proposals_json.clone())
                .map_err(|e| err("watchman.community_health", e))?;

            let health = undercroft::CommunityHealth::from_community(&community, &proposals, None);
            let health_json = serde_json::to_value(&health)
                .map_err(|e| err("watchman.community_health", e))?;
            ok_json(&health_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Quest Health
        // ═══════════════════════════════════════════════════════════

        // ── undercroft.quest_health ───────────────────────────────
        // Get quest health from an observatory report.
        state.phone.register_raw("watchman.quest_health", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let report_json = params.get("report")
                .ok_or_else(|| err("watchman.quest_health", "missing 'report'"))?;

            let report: quest::ObservatoryReport = serde_json::from_value(report_json.clone())
                .map_err(|e| err("watchman.quest_health", e))?;

            let health = undercroft::QuestHealth::from_report(&report);
            let health_score = health.health_score();
            let health_json = serde_json::to_value(&health)
                .map_err(|e| err("watchman.quest_health", e))?;

            ok_json(&json!({
                "health": health_json,
                "score": health_score
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Snapshots & History
        // ═══════════════════════════════════════════════════════════

        // ── undercroft.snapshot_create ─────────────────────────────
        // Create a health snapshot from component health data.
        state.phone.register_raw("watchman.snapshot_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let network_json = params.get("network")
                .ok_or_else(|| err("watchman.snapshot_create", "missing 'network'"))?;
            let empty_communities = Value::Array(vec![]);
            let communities_json = params.get("communities").unwrap_or(&empty_communities);
            let economic_json = params.get("economic")
                .ok_or_else(|| err("watchman.snapshot_create", "missing 'economic'"))?;
            let quest_json = params.get("quest");

            let network: undercroft::NetworkHealth = serde_json::from_value(network_json.clone())
                .map_err(|e| err("watchman.snapshot_create", e))?;
            let communities: Vec<undercroft::CommunityHealth> = serde_json::from_value(communities_json.clone())
                .map_err(|e| err("watchman.snapshot_create", e))?;
            let economic: undercroft::EconomicHealth = serde_json::from_value(economic_json.clone())
                .map_err(|e| err("watchman.snapshot_create", e))?;
            let quest: Option<undercroft::QuestHealth> = quest_json
                .map(|q| serde_json::from_value(q.clone()))
                .transpose()
                .map_err(|e| err("watchman.snapshot_create", e))?;

            let snapshot = undercroft::HealthSnapshot {
                network,
                communities,
                economic,
                quest,
                relay_privacy: None,
                timestamp: chrono::Utc::now(),
            };

            let snapshot_json = serde_json::to_value(&snapshot)
                .map_err(|e| err("watchman.snapshot_create", e))?;
            ok_json(&snapshot_json)
        });

        // ── undercroft.history_create ─────────────────────────────
        // Create a new health history ring buffer.
        state.phone.register_raw("watchman.history_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let max_retention = params.get("max_retention").and_then(|v| v.as_u64()).unwrap_or(168) as usize;

            let history = undercroft::HealthHistory::new(max_retention);
            let history_json = serde_json::to_value(&history)
                .map_err(|e| err("watchman.history_create", e))?;
            ok_json(&history_json)
        });

        // ── undercroft.history_push ───────────────────────────────
        // Push a snapshot into a history ring buffer.
        state.phone.register_raw("watchman.history_push", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let history_json = params.get("history")
                .ok_or_else(|| err("watchman.history_push", "missing 'history'"))?;
            let snapshot_json = params.get("snapshot")
                .ok_or_else(|| err("watchman.history_push", "missing 'snapshot'"))?;

            let mut history: undercroft::HealthHistory = serde_json::from_value(history_json.clone())
                .map_err(|e| err("watchman.history_push", e))?;
            let snapshot: undercroft::HealthSnapshot = serde_json::from_value(snapshot_json.clone())
                .map_err(|e| err("watchman.history_push", e))?;

            history.push(snapshot);

            let result = serde_json::to_value(&history)
                .map_err(|e| err("watchman.history_push", e))?;
            ok_json(&result)
        });

        // ── undercroft.history_latest ─────────────────────────────
        // Get the latest snapshot from a history buffer.
        state.phone.register_raw("watchman.history_latest", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let history_json = params.get("history")
                .ok_or_else(|| err("watchman.history_latest", "missing 'history'"))?;

            let history: undercroft::HealthHistory = serde_json::from_value(history_json.clone())
                .map_err(|e| err("watchman.history_latest", e))?;

            match history.latest() {
                Some(snapshot) => {
                    let snapshot_json = serde_json::to_value(snapshot)
                        .map_err(|e| err("watchman.history_latest", e))?;
                    ok_json(&snapshot_json)
                }
                None => ok_json(&json!({ "empty": true })),
            }
        });

        // ═══════════════════════════════════════════════════════════
        // Top-Level Metrics (for HQ dashboard)
        // ═══════════════════════════════════════════════════════════

        // ── undercroft.metrics ─────────────────────────────────────
        // Compute top-level metrics from a snapshot + node/store stats.
        let s = state.clone();
        state.phone.register_raw("watchman.metrics", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let snapshot_json = params.get("snapshot")
                .ok_or_else(|| err("watchman.metrics", "missing 'snapshot'"))?;
            let node_count = params.get("node_count").and_then(|v| v.as_u64()).unwrap_or(0);

            let snapshot: undercroft::HealthSnapshot = serde_json::from_value(snapshot_json.clone())
                .map_err(|e| err("watchman.metrics", e))?;

            let store_stats = s.omnibus.omnibus().store_stats();
            let metrics = undercroft::HealthMetrics::from_snapshot(&snapshot, node_count, Some(&store_stats));

            let metrics_json = serde_json::to_value(&metrics)
                .map_err(|e| err("watchman.metrics", e))?;
            ok_json(&metrics_json)
        });

        // ── undercroft.metrics_live ───────────────────────────────
        // Build a live snapshot from current Omnibus state and return metrics.
        let s = state.clone();
        state.phone.register_raw("watchman.metrics_live", move |_data| {
            let _snapshots = s.omnibus.omnibus().relay_health();
            let network = undercroft::NetworkHealth::empty();
            let store_stats = s.omnibus.omnibus().store_stats();

            let snapshot = undercroft::HealthSnapshot {
                network,
                communities: vec![],
                economic: undercroft::EconomicHealth::empty(),
                quest: None,
                relay_privacy: None,
                timestamp: chrono::Utc::now(),
            };

            let gospel_peers = s.omnibus.omnibus().peers().len() as u64;
            let metrics = undercroft::HealthMetrics::from_snapshot(&snapshot, gospel_peers, Some(&store_stats));

            let metrics_json = serde_json::to_value(&metrics)
                .map_err(|e| err("watchman.metrics_live", e))?;
            ok_json(&metrics_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Network health
            .with_call(CallDescriptor::new("watchman.network_health", "Aggregate relay health"))
            .with_call(CallDescriptor::new("watchman.network_empty", "Empty network health"))
            // Economic health
            .with_call(CallDescriptor::new("watchman.economic_health", "From treasury status"))
            .with_call(CallDescriptor::new("watchman.economic_empty", "Empty economic health"))
            // Community health
            .with_call(CallDescriptor::new("watchman.community_health", "From community + proposals"))
            // Quest health
            .with_call(CallDescriptor::new("watchman.quest_health", "From observatory report"))
            // Snapshots
            .with_call(CallDescriptor::new("watchman.snapshot_create", "Create health snapshot"))
            // History
            .with_call(CallDescriptor::new("watchman.history_create", "Create history buffer"))
            .with_call(CallDescriptor::new("watchman.history_push", "Push snapshot to history"))
            .with_call(CallDescriptor::new("watchman.history_latest", "Get latest snapshot"))
            // Metrics
            .with_call(CallDescriptor::new("watchman.metrics", "Compute from snapshot"))
            .with_call(CallDescriptor::new("watchman.metrics_live", "Live metrics from Omnibus"))
    }
}
