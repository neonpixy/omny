//! Scout module — Zeitgeist discovery & culture courtier.
//!
//! Exposes Tower directory management, query routing, result merging,
//! local caching, trend tracking, and global cross-community trends
//! as daemon operations. Non-serializable Zeitgeist types (TowerDirectory,
//! TrendTracker, GlobalTrendTracker) are held in session maps, accessed
//! by UUID handle.
//!
//! Programs use staff name "scout" which maps to daemon namespace "zeitgeist".

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct ScoutModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn parse_handle(params: &Value, op: &str) -> Result<Uuid, PhoneError> {
    let id_str = params.get("handle").and_then(|v| v.as_str())
        .ok_or_else(|| err(op, "missing 'handle'"))?;
    Uuid::parse_str(id_str).map_err(|e| err(op, format!("invalid handle: {e}")))
}

/// Session maps for non-serializable Zeitgeist types.
struct ScoutSessions {
    directories: Mutex<HashMap<Uuid, zeitgeist::TowerDirectory>>,
    trends: Mutex<HashMap<Uuid, zeitgeist::TrendTracker>>,
    global_trends: Mutex<HashMap<Uuid, zeitgeist::GlobalTrendTracker>>,
}

impl ScoutSessions {
    fn new() -> Self {
        Self {
            directories: Mutex::new(HashMap::new()),
            trends: Mutex::new(HashMap::new()),
            global_trends: Mutex::new(HashMap::new()),
        }
    }
}

impl DaemonModule for ScoutModule {
    fn id(&self) -> &str { "scout" }
    fn name(&self) -> &str { "Scout (Zeitgeist)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        let sessions = Arc::new(ScoutSessions::new());

        // ═══════════════════════════════════════════════════════════
        // Tower Directory (session-based)
        // ═══════════════════════════════════════════════════════════

        // ── zeitgeist.directory_create ─────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.directory_create", move |_data| {
            let id = Uuid::new_v4();
            let directory = zeitgeist::TowerDirectory::new();
            sess.directories.lock().unwrap().insert(id, directory);
            ok_json(&json!({ "handle": id.to_string() }))
        });

        // ── zeitgeist.directory_update ─────────────────────────────
        let sess = sessions.clone();
        let s = state.clone();
        state.phone.register_raw("scout.directory_update", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.directory_update")?;
            let events_json = params.get("events")
                .ok_or_else(|| err("scout.directory_update", "missing 'events'"))?;

            let events: Vec<globe::event::OmniEvent> = serde_json::from_value(events_json.clone())
                .map_err(|e| err("scout.directory_update", e))?;

            let mut dirs = sess.directories.lock().unwrap();
            let directory = dirs.get_mut(&handle)
                .ok_or_else(|| err("scout.directory_update", "unknown handle"))?;

            directory.update(&events);

            let searchable = directory.searchable_towers();
            let harbors = directory.harbors();

            s.email.send_raw("scout.directory_updated", &serde_json::to_vec(&json!({
                "handle": handle.to_string()
            })).unwrap_or_default());

            ok_json(&json!({
                "handle": handle.to_string(),
                "total_towers": searchable.len() + harbors.len(),
                "searchable_count": searchable.len(),
                "harbor_count": harbors.len()
            }))
        });

        // ── zeitgeist.directory_info ──────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.directory_info", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.directory_info")?;

            let dirs = sess.directories.lock().unwrap();
            let directory = dirs.get(&handle)
                .ok_or_else(|| err("scout.directory_info", "unknown handle"))?;

            let searchable = directory.searchable_towers();
            let harbors = directory.harbors();

            ok_json(&json!({
                "handle": handle.to_string(),
                "total_towers": searchable.len() + harbors.len(),
                "searchable_count": searchable.len(),
                "harbor_count": harbors.len()
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Query Routing
        // ═══════════════════════════════════════════════════════════

        // ── zeitgeist.route ───────────────────────────────────────
        let sess = sessions.clone();
        let s = state.clone();
        state.phone.register_raw("scout.route", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let query = params.get("query").and_then(|v| v.as_str())
                .ok_or_else(|| err("scout.route", "missing 'query'"))?;
            let handle = parse_handle(&params, "scout.route")?;

            let dirs = sess.directories.lock().unwrap();
            let directory = dirs.get(&handle)
                .ok_or_else(|| err("scout.route", "unknown directory handle"))?;

            let router = zeitgeist::QueryRouter::new();
            let routed = router.route(query, directory)
                .map_err(|e| err("scout.route", e))?;

            let result_count = routed.len();
            let routed_json: Vec<Value> = routed.iter()
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
                .collect();

            s.email.send_raw("scout.search_completed", &serde_json::to_vec(&json!({
                "query": query,
                "result_count": result_count
            })).unwrap_or_default());

            ok_json(&Value::Array(routed_json))
        });

        // ═══════════════════════════════════════════════════════════
        // Result Merging
        // ═══════════════════════════════════════════════════════════

        // ── zeitgeist.merge_results ───────────────────────────────
        state.phone.register_raw("scout.merge_results", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let batches_json = params.get("batches")
                .ok_or_else(|| err("scout.merge_results", "missing 'batches'"))?;

            let batches: Vec<zeitgeist::merger::TowerResultBatch> = serde_json::from_value(batches_json.clone())
                .map_err(|e| err("scout.merge_results", e))?;

            let merger = zeitgeist::ResultMerger::new();
            let merged = merger.merge(batches);

            let merged_json = serde_json::to_value(&merged)
                .map_err(|e| err("scout.merge_results", e))?;
            ok_json(&merged_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Local Cache
        // ═══════════════════════════════════════════════════════════

        // ── zeitgeist.cache_create ────────────────────────────────
        state.phone.register_raw("scout.cache_create", move |_data| {
            let cache = zeitgeist::LocalCache::new();
            let cache_json = serde_json::to_value(&cache)
                .map_err(|e| err("scout.cache_create", e))?;
            ok_json(&cache_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Trend Tracking — session-based
        // ═══════════════════════════════════════════════════════════

        // ── zeitgeist.trend_create ────────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.trend_create", move |_data| {
            let id = Uuid::new_v4();
            sess.trends.lock().unwrap().insert(id, zeitgeist::TrendTracker::new());
            ok_json(&json!({ "handle": id.to_string() }))
        });

        // ── zeitgeist.trend_record_query ──────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.trend_record_query", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.trend_record_query")?;
            let query = params.get("query").and_then(|v| v.as_str())
                .ok_or_else(|| err("scout.trend_record_query", "missing 'query'"))?;

            let now = chrono::Utc::now().timestamp();
            let mut trackers = sess.trends.lock().unwrap();
            let tracker = trackers.get_mut(&handle)
                .ok_or_else(|| err("scout.trend_record_query", "unknown handle"))?;

            tracker.record_query(query, now);
            ok_json(&json!({ "ok": true }))
        });

        // ── zeitgeist.trend_top ───────────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.trend_top", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.trend_top")?;
            let n = params.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

            let trackers = sess.trends.lock().unwrap();
            let tracker = trackers.get(&handle)
                .ok_or_else(|| err("scout.trend_top", "unknown handle"))?;

            let top = tracker.top(n);
            let top_json: Vec<Value> = top.iter()
                .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
                .collect();
            ok_json(&Value::Array(top_json))
        });

        // ── zeitgeist.trend_decay ─────────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.trend_decay", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.trend_decay")?;

            let mut trackers = sess.trends.lock().unwrap();
            let tracker = trackers.get_mut(&handle)
                .ok_or_else(|| err("scout.trend_decay", "unknown handle"))?;

            tracker.decay();
            ok_json(&json!({ "ok": true }))
        });

        // ═══════════════════════════════════════════════════════════
        // Global Trends — session-based
        // ═══════════════════════════════════════════════════════════

        // ── zeitgeist.global_trend_create ──────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.global_trend_create", move |_data| {
            let id = Uuid::new_v4();
            sess.global_trends.lock().unwrap().insert(id, zeitgeist::GlobalTrendTracker::new());
            ok_json(&json!({ "handle": id.to_string() }))
        });

        // ── zeitgeist.global_trend_record ─────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.global_trend_record", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.global_trend_record")?;
            let topic = params.get("topic").and_then(|v| v.as_str())
                .ok_or_else(|| err("scout.global_trend_record", "missing 'topic'"))?;
            let community_id = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("scout.global_trend_record", "missing 'community_id'"))?;
            let score = params.get("score").and_then(|v| v.as_f64()).unwrap_or(1.0);
            let default_sentiment = Value::String("Unknown".into());
            let sentiment_json = params.get("sentiment").unwrap_or(&default_sentiment);

            let sentiment: zeitgeist::TrendSentiment = serde_json::from_value(sentiment_json.clone())
                .map_err(|e| err("scout.global_trend_record", e))?;

            let mut trackers = sess.global_trends.lock().unwrap();
            let tracker = trackers.get_mut(&handle)
                .ok_or_else(|| err("scout.global_trend_record", "unknown handle"))?;

            tracker.record_community_trend(topic, community_id, score, sentiment);
            ok_json(&json!({ "ok": true }))
        });

        // ── zeitgeist.global_trend_top ────────────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.global_trend_top", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.global_trend_top")?;
            let n = params.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

            let trackers = sess.global_trends.lock().unwrap();
            let tracker = trackers.get(&handle)
                .ok_or_else(|| err("scout.global_trend_top", "unknown handle"))?;

            let top = tracker.top_global(n);
            let top_json: Vec<Value> = top.iter()
                .map(|t| serde_json::to_value(t).unwrap_or(Value::Null))
                .collect();
            ok_json(&Value::Array(top_json))
        });

        // ── zeitgeist.global_trend_perspective ────────────────────
        let sess = sessions.clone();
        state.phone.register_raw("scout.global_trend_perspective", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let handle = parse_handle(&params, "scout.global_trend_perspective")?;
            let topic = params.get("topic").and_then(|v| v.as_str())
                .ok_or_else(|| err("scout.global_trend_perspective", "missing 'topic'"))?;

            let trackers = sess.global_trends.lock().unwrap();
            let tracker = trackers.get(&handle)
                .ok_or_else(|| err("scout.global_trend_perspective", "unknown handle"))?;

            let perspective = tracker.perspective(topic);
            let perspective_json = serde_json::to_value(&perspective)
                .map_err(|e| err("scout.global_trend_perspective", e))?;
            ok_json(&perspective_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Scope Utilities
        // ═══════════════════════════════════════════════════════════

        // ── zeitgeist.scope_create ────────────────────────────────
        state.phone.register_raw("scout.scope_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let scope_type = params.get("type").and_then(|v| v.as_str()).unwrap_or("global");

            let scope = match scope_type {
                "local" => {
                    let community_id = params.get("community_id").and_then(|v| v.as_str())
                        .ok_or_else(|| err("scout.scope_create", "local scope requires 'community_id'"))?;
                    zeitgeist::ZeitgeistScope::Local(community_id.to_string())
                }
                "communities" => {
                    let ids: Vec<String> = params.get("community_ids")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .ok_or_else(|| err("scout.scope_create", "communities scope requires 'community_ids'"))?;
                    zeitgeist::ZeitgeistScope::Communities(ids)
                }
                _ => zeitgeist::ZeitgeistScope::Global,
            };

            let scope_json = serde_json::to_value(&scope)
                .map_err(|e| err("scout.scope_create", e))?;
            ok_json(&scope_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Tower directory
            .with_call(CallDescriptor::new("scout.directory_create", "Create Tower directory"))
            .with_call(CallDescriptor::new("scout.directory_update", "Update with gospel events"))
            .with_call(CallDescriptor::new("scout.directory_info", "Directory summary"))
            // Query routing
            .with_call(CallDescriptor::new("scout.route", "Route query to best Towers"))
            // Result merging
            .with_call(CallDescriptor::new("scout.merge_results", "Merge Tower results"))
            // Local cache
            .with_call(CallDescriptor::new("scout.cache_create", "Create local cache"))
            // Local trends
            .with_call(CallDescriptor::new("scout.trend_create", "Create trend tracker"))
            .with_call(CallDescriptor::new("scout.trend_record_query", "Record query for trends"))
            .with_call(CallDescriptor::new("scout.trend_top", "Top trending topics"))
            .with_call(CallDescriptor::new("scout.trend_decay", "Apply time decay"))
            // Global trends
            .with_call(CallDescriptor::new("scout.global_trend_create", "Create global tracker"))
            .with_call(CallDescriptor::new("scout.global_trend_record", "Record community trend"))
            .with_call(CallDescriptor::new("scout.global_trend_top", "Top global trends"))
            .with_call(CallDescriptor::new("scout.global_trend_perspective", "Community perspectives"))
            // Scope
            .with_call(CallDescriptor::new("scout.scope_create", "Create discovery scope"))
            // Events
            .with_emitted_event(EventDescriptor::new("scout.search_completed", "Search completed"))
            .with_emitted_event(EventDescriptor::new("scout.directory_updated", "Directory refreshed"))
    }
}
