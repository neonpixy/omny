//! Chronicler module — Yoke history & provenance courtier.
//!
//! Exposes version history, relationship graphs, activity timelines,
//! ceremony records, provenance scoring, and AI transparency as daemon
//! operations. All Yoke types are pure data — no I/O, no Vault needed.
//!
//! Programs use staff name "chronicler" which maps to daemon namespace "yoke".

use std::sync::Arc;
use chrono::{DateTime, Utc};

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct ChroniclerModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn parse_uuid(params: &Value, key: &str, op: &str) -> Result<Uuid, PhoneError> {
    let id_str = params.get(key).and_then(|v| v.as_str())
        .ok_or_else(|| err(op, format!("missing '{key}'")))?;
    Uuid::parse_str(id_str).map_err(|e| err(op, format!("invalid UUID: {e}")))
}

impl DaemonModule for ChroniclerModule {
    fn id(&self) -> &str { "chronicler" }
    fn name(&self) -> &str { "Chronicler (Yoke)" }
    fn deps(&self) -> &[&str] { &["castellan"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Version History
        // ═══════════════════════════════════════════════════════════

        // ── yoke.version_chain_create ─────────────────────────────
        // Create a new empty version chain for an idea.
        state.phone.register_raw("chronicler.version_chain_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let idea_id = parse_uuid(&params, "idea_id", "chronicler.version_chain_create")?;
            let chain = yoke::VersionChain::new(idea_id);
            let chain_json = serde_json::to_value(&chain)
                .map_err(|e| err("chronicler.version_chain_create", e))?;
            ok_json(&chain_json)
        });

        // ── yoke.version_tag ──────────────────────────────────────
        // Create a version tag for an idea.
        state.phone.register_raw("chronicler.version_tag", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let idea_id = parse_uuid(&params, "idea_id", "chronicler.version_tag")?;
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.version_tag", "missing 'name'"))?;
            let author = params.get("author").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.version_tag", "missing 'author'"))?;
            let message = params.get("message").and_then(|v| v.as_str());
            let branch = params.get("branch").and_then(|v| v.as_str());

            let clock = x::VectorClock::new();
            let mut tag = yoke::VersionTag::new(idea_id, name, clock, author);
            if let Some(msg) = message {
                tag = tag.with_message(msg);
            }
            if let Some(b) = branch {
                tag = tag.on_branch(b);
            }
            let tag_json = serde_json::to_value(&tag)
                .map_err(|e| err("chronicler.version_tag", e))?;
            ok_json(&tag_json)
        });

        // ── yoke.version_chain_tag ────────────────────────────────
        // Add a version tag to an existing chain (passed as JSON).
        state.phone.register_raw("chronicler.version_chain_tag", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let chain_json = params.get("chain")
                .ok_or_else(|| err("chronicler.version_chain_tag", "missing 'chain'"))?;
            let tag_json = params.get("tag")
                .ok_or_else(|| err("chronicler.version_chain_tag", "missing 'tag'"))?;

            let mut chain: yoke::VersionChain = serde_json::from_value(chain_json.clone())
                .map_err(|e| err("chronicler.version_chain_tag", e))?;
            let tag: yoke::VersionTag = serde_json::from_value(tag_json.clone())
                .map_err(|e| err("chronicler.version_chain_tag", e))?;

            chain.tag_version(tag)
                .map_err(|e| err("chronicler.version_chain_tag", e))?;

            let result = serde_json::to_value(&chain)
                .map_err(|e| err("chronicler.version_chain_tag", e))?;
            ok_json(&result)
        });

        // ── yoke.version_chain_branch ─────────────────────────────
        // Create a branch on a version chain.
        state.phone.register_raw("chronicler.version_chain_branch", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let chain_json = params.get("chain")
                .ok_or_else(|| err("chronicler.version_chain_branch", "missing 'chain'"))?;
            let branch_name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.version_chain_branch", "missing 'name'"))?;
            let from_version = parse_uuid(&params, "from_version", "chronicler.version_chain_branch")?;
            let author = params.get("author").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.version_chain_branch", "missing 'author'"))?;

            let mut chain: yoke::VersionChain = serde_json::from_value(chain_json.clone())
                .map_err(|e| err("chronicler.version_chain_branch", e))?;

            chain.create_branch(branch_name, from_version, author)
                .map_err(|e| err("chronicler.version_chain_branch", e))?;

            let result = serde_json::to_value(&chain)
                .map_err(|e| err("chronicler.version_chain_branch", e))?;
            ok_json(&result)
        });

        // ── yoke.version_chain_merge ──────────────────────────────
        // Merge a branch on a version chain.
        state.phone.register_raw("chronicler.version_chain_merge", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let chain_json = params.get("chain")
                .ok_or_else(|| err("chronicler.version_chain_merge", "missing 'chain'"))?;
            let source_branch = params.get("source_branch").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.version_chain_merge", "missing 'source_branch'"))?;
            let target_branch = params.get("target_branch").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.version_chain_merge", "missing 'target_branch'"))?;
            let merge_version = parse_uuid(&params, "merge_version", "chronicler.version_chain_merge")?;
            let author = params.get("author").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.version_chain_merge", "missing 'author'"))?;

            let mut chain: yoke::VersionChain = serde_json::from_value(chain_json.clone())
                .map_err(|e| err("chronicler.version_chain_merge", e))?;

            chain.merge_branch(source_branch, target_branch, merge_version, author)
                .map_err(|e| err("chronicler.version_chain_merge", e))?;

            let result = serde_json::to_value(&chain)
                .map_err(|e| err("chronicler.version_chain_merge", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Relationship Graph
        // ═══════════════════════════════════════════════════════════

        // ── yoke.link_create ──────────────────────────────────────
        // Create a typed relationship link.
        state.phone.register_raw("chronicler.link_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let source = params.get("source").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.link_create", "missing 'source'"))?;
            let target = params.get("target").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.link_create", "missing 'target'"))?;
            let relationship_json = params.get("relationship")
                .ok_or_else(|| err("chronicler.link_create", "missing 'relationship'"))?;
            let author = params.get("author").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.link_create", "missing 'author'"))?;

            let relationship: yoke::RelationType = serde_json::from_value(relationship_json.clone())
                .map_err(|e| err("chronicler.link_create", e))?;

            let link = yoke::YokeLink::new(source, target, relationship, author);
            let link_json = serde_json::to_value(&link)
                .map_err(|e| err("chronicler.link_create", e))?;
            ok_json(&link_json)
        });

        // ── yoke.graph_query ──────────────────────────────────────
        // Query a relationship graph. Takes a serialized graph snapshot
        // and returns links matching the query parameters.
        state.phone.register_raw("chronicler.graph_query", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let snapshot_json = params.get("graph")
                .ok_or_else(|| err("chronicler.graph_query", "missing 'graph'"))?;
            let entity_id = params.get("entity_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.graph_query", "missing 'entity_id'"))?;
            let direction = params.get("direction").and_then(|v| v.as_str()).unwrap_or("forward");

            let snapshot: yoke::GraphSnapshot = serde_json::from_value(snapshot_json.clone())
                .map_err(|e| err("chronicler.graph_query", e))?;
            let graph = yoke::RelationshipGraph::from_snapshot(snapshot);

            let links = match direction {
                "forward" => graph.links_from(entity_id),
                "backward" | "reverse" => graph.links_to(entity_id),
                _ => return Err(err("chronicler.graph_query", "direction must be 'forward' or 'backward'")),
            };

            let links_json: Vec<Value> = links.iter()
                .map(|l| serde_json::to_value(l).unwrap_or(Value::Null))
                .collect();
            ok_json(&Value::Array(links_json))
        });

        // ── yoke.graph_ancestors ──────────────────────────────────
        // Find provenance ancestors in a graph.
        state.phone.register_raw("chronicler.graph_ancestors", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let snapshot_json = params.get("graph")
                .ok_or_else(|| err("chronicler.graph_ancestors", "missing 'graph'"))?;
            let entity_id = params.get("entity_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.graph_ancestors", "missing 'entity_id'"))?;

            let snapshot: yoke::GraphSnapshot = serde_json::from_value(snapshot_json.clone())
                .map_err(|e| err("chronicler.graph_ancestors", e))?;
            let graph = yoke::RelationshipGraph::from_snapshot(snapshot);

            let ancestors = graph.ancestors(entity_id);
            let result: Vec<Value> = ancestors.iter()
                .map(|n| json!({ "entity_id": n.entity_id, "depth": n.depth, "path": n.path }))
                .collect();
            ok_json(&Value::Array(result))
        });

        // ── yoke.graph_descendants ────────────────────────────────
        // Find provenance descendants in a graph.
        state.phone.register_raw("chronicler.graph_descendants", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let snapshot_json = params.get("graph")
                .ok_or_else(|| err("chronicler.graph_descendants", "missing 'graph'"))?;
            let entity_id = params.get("entity_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.graph_descendants", "missing 'entity_id'"))?;

            let snapshot: yoke::GraphSnapshot = serde_json::from_value(snapshot_json.clone())
                .map_err(|e| err("chronicler.graph_descendants", e))?;
            let graph = yoke::RelationshipGraph::from_snapshot(snapshot);

            let descendants = graph.descendants(entity_id);
            let result: Vec<Value> = descendants.iter()
                .map(|n| json!({ "entity_id": n.entity_id, "depth": n.depth, "path": n.path }))
                .collect();
            ok_json(&Value::Array(result))
        });

        // ── yoke.graph_path ──────────────────────────────────────
        // Find shortest path between two entities.
        state.phone.register_raw("chronicler.graph_path", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let snapshot_json = params.get("graph")
                .ok_or_else(|| err("chronicler.graph_path", "missing 'graph'"))?;
            let from = params.get("from").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.graph_path", "missing 'from'"))?;
            let to = params.get("to").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.graph_path", "missing 'to'"))?;

            let snapshot: yoke::GraphSnapshot = serde_json::from_value(snapshot_json.clone())
                .map_err(|e| err("chronicler.graph_path", e))?;
            let graph = yoke::RelationshipGraph::from_snapshot(snapshot);

            match graph.path_between(from, to) {
                Some(path) => ok_json(&json!({ "path": path })),
                None => ok_json(&json!({ "path": null })),
            }
        });

        // ── yoke.graph_stats ──────────────────────────────────────
        // Get graph statistics (link count, entity count).
        state.phone.register_raw("chronicler.graph_stats", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let snapshot_json = params.get("graph")
                .ok_or_else(|| err("chronicler.graph_stats", "missing 'graph'"))?;

            let snapshot: yoke::GraphSnapshot = serde_json::from_value(snapshot_json.clone())
                .map_err(|e| err("chronicler.graph_stats", e))?;
            let graph = yoke::RelationshipGraph::from_snapshot(snapshot);

            ok_json(&json!({
                "link_count": graph.link_count(),
                "entity_count": graph.entity_count()
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Activity Timeline
        // ═══════════════════════════════════════════════════════════

        // ── yoke.timeline_create ──────────────────────────────────
        // Create a new timeline for an entity.
        state.phone.register_raw("chronicler.timeline_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let owner_id = params.get("owner_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.timeline_create", "missing 'owner_id'"))?;

            let timeline = yoke::Timeline::new(owner_id);
            let timeline_json = serde_json::to_value(&timeline)
                .map_err(|e| err("chronicler.timeline_create", e))?;
            ok_json(&timeline_json)
        });

        // ── yoke.timeline_record ──────────────────────────────────
        // Record an activity on an existing timeline.
        state.phone.register_raw("chronicler.timeline_record", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let timeline_json = params.get("timeline")
                .ok_or_else(|| err("chronicler.timeline_record", "missing 'timeline'"))?;
            let activity_json = params.get("activity")
                .ok_or_else(|| err("chronicler.timeline_record", "missing 'activity'"))?;

            let mut timeline: yoke::Timeline = serde_json::from_value(timeline_json.clone())
                .map_err(|e| err("chronicler.timeline_record", e))?;
            let activity: yoke::ActivityRecord = serde_json::from_value(activity_json.clone())
                .map_err(|e| err("chronicler.timeline_record", e))?;

            timeline.record(activity);

            let result = serde_json::to_value(&timeline)
                .map_err(|e| err("chronicler.timeline_record", e))?;
            ok_json(&result)
        });

        // ── yoke.timeline_milestone ───────────────────────────────
        // Mark a milestone on a timeline.
        state.phone.register_raw("chronicler.timeline_milestone", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let timeline_json = params.get("timeline")
                .ok_or_else(|| err("chronicler.timeline_milestone", "missing 'timeline'"))?;
            let milestone_json = params.get("milestone")
                .ok_or_else(|| err("chronicler.timeline_milestone", "missing 'milestone'"))?;

            let mut timeline: yoke::Timeline = serde_json::from_value(timeline_json.clone())
                .map_err(|e| err("chronicler.timeline_milestone", e))?;
            let milestone: yoke::Milestone = serde_json::from_value(milestone_json.clone())
                .map_err(|e| err("chronicler.timeline_milestone", e))?;

            timeline.mark_milestone(milestone);

            let result = serde_json::to_value(&timeline)
                .map_err(|e| err("chronicler.timeline_milestone", e))?;
            ok_json(&result)
        });

        // ── yoke.timeline_query ───────────────────────────────────
        // Query a timeline by actor, target, community, or action.
        state.phone.register_raw("chronicler.timeline_query", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let timeline_json = params.get("timeline")
                .ok_or_else(|| err("chronicler.timeline_query", "missing 'timeline'"))?;

            let timeline: yoke::Timeline = serde_json::from_value(timeline_json.clone())
                .map_err(|e| err("chronicler.timeline_query", e))?;

            let actor = params.get("actor").and_then(|v| v.as_str());
            let target = params.get("target").and_then(|v| v.as_str());
            let community = params.get("community").and_then(|v| v.as_str());

            let activities: Vec<&yoke::ActivityRecord> = if let Some(a) = actor {
                timeline.by_actor(a)
            } else if let Some(t) = target {
                timeline.for_target(t)
            } else if let Some(c) = community {
                timeline.in_community(c)
            } else {
                // Return all
                timeline.activities.iter().collect()
            };

            let activities_json: Vec<Value> = activities.iter()
                .map(|a| serde_json::to_value(a).unwrap_or(Value::Null))
                .collect();

            ok_json(&json!({
                "activities": activities_json,
                "count": activities_json.len(),
                "milestones": timeline.milestone_count()
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Ceremonies
        // ═══════════════════════════════════════════════════════════

        // ── yoke.ceremony_create ──────────────────────────────────
        // Create and validate a ceremony record.
        let s = state.clone();
        state.phone.register_raw("chronicler.ceremony_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let ceremony_json = params.get("ceremony")
                .ok_or_else(|| err("chronicler.ceremony_create", "missing 'ceremony'"))?;

            let ceremony: yoke::CeremonyRecord = serde_json::from_value(ceremony_json.clone())
                .map_err(|e| err("chronicler.ceremony_create", e))?;

            ceremony.validate()
                .map_err(|e| err("chronicler.ceremony_create", e))?;

            let result = serde_json::to_value(&ceremony)
                .map_err(|e| err("chronicler.ceremony_create", e))?;

            s.email.send_raw("chronicler.ceremony_created", &serde_json::to_vec(&json!({
                "id": ceremony.id.to_string(),
                "type": format!("{:?}", ceremony.ceremony_type)
            })).unwrap_or_default());

            ok_json(&result)
        });

        // ── yoke.ceremony_validate ────────────────────────────────
        // Validate a ceremony record without persisting.
        state.phone.register_raw("chronicler.ceremony_validate", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let ceremony_json = params.get("ceremony")
                .ok_or_else(|| err("chronicler.ceremony_validate", "missing 'ceremony'"))?;

            let ceremony: yoke::CeremonyRecord = serde_json::from_value(ceremony_json.clone())
                .map_err(|e| err("chronicler.ceremony_validate", e))?;

            match ceremony.validate() {
                Ok(()) => ok_json(&json!({ "valid": true })),
                Err(e) => ok_json(&json!({ "valid": false, "error": e.to_string() })),
            }
        });

        // ═══════════════════════════════════════════════════════════
        // Provenance Scoring
        // ═══════════════════════════════════════════════════════════

        // ── yoke.provenance_compute ───────────────────────────────
        // Compute a provenance score for an event.
        state.phone.register_raw("chronicler.provenance_compute", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let event_id = params.get("event_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.provenance_compute", "missing 'event_id'"))?;
            let graph_json = params.get("graph")
                .ok_or_else(|| err("chronicler.provenance_compute", "missing 'graph'"))?;
            let events_json = params.get("events")
                .ok_or_else(|| err("chronicler.provenance_compute", "missing 'events'"))?;
            let empty_corroborations = Value::Array(vec![]);
            let corroborations_json = params.get("corroborations").unwrap_or(&empty_corroborations);
            let challenge_count = params.get("challenge_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let source_reputation = params.get("source_reputation").and_then(|v| v.as_f64()).unwrap_or(0.0);

            let snapshot: yoke::GraphSnapshot = serde_json::from_value(graph_json.clone())
                .map_err(|e| err("chronicler.provenance_compute", e))?;
            let graph = yoke::RelationshipGraph::from_snapshot(snapshot);

            let events = parse_event_data_array(events_json, "chronicler.provenance_compute")?;
            let corroborations: Vec<yoke::Corroboration> = serde_json::from_value(corroborations_json.clone())
                .map_err(|e| err("chronicler.provenance_compute", e))?;

            let score = yoke::ProvenanceComputer::compute(
                event_id, &graph, &events, &corroborations, challenge_count, source_reputation,
            );

            let score_json = serde_json::to_value(&score)
                .map_err(|e| err("chronicler.provenance_compute", e))?;
            ok_json(&score_json)
        });

        // ── yoke.provenance_chain ─────────────────────────────────
        // Build a full provenance chain for an event.
        state.phone.register_raw("chronicler.provenance_chain", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let event_id = params.get("event_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("chronicler.provenance_chain", "missing 'event_id'"))?;
            let graph_json = params.get("graph")
                .ok_or_else(|| err("chronicler.provenance_chain", "missing 'graph'"))?;
            let events_json = params.get("events")
                .ok_or_else(|| err("chronicler.provenance_chain", "missing 'events'"))?;
            let empty_corroborations = Value::Array(vec![]);
            let corroborations_json = params.get("corroborations").unwrap_or(&empty_corroborations);

            let snapshot: yoke::GraphSnapshot = serde_json::from_value(graph_json.clone())
                .map_err(|e| err("chronicler.provenance_chain", e))?;
            let graph = yoke::RelationshipGraph::from_snapshot(snapshot);

            let events = parse_event_data_array(events_json, "chronicler.provenance_chain")?;
            let corroborations: Vec<yoke::Corroboration> = serde_json::from_value(corroborations_json.clone())
                .map_err(|e| err("chronicler.provenance_chain", e))?;

            let chain = yoke::ProvenanceComputer::build_chain(event_id, &graph, &events, corroborations);

            let chain_json = serde_json::to_value(&chain)
                .map_err(|e| err("chronicler.provenance_chain", e))?;
            ok_json(&chain_json)
        });

        // ═══════════════════════════════════════════════════════════
        // AI Transparency
        // ═══════════════════════════════════════════════════════════

        // ── yoke.authorship_create ────────────────────────────────
        // Create a new idea authorship tracker.
        state.phone.register_raw("chronicler.authorship_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let idea_id = parse_uuid(&params, "idea_id", "chronicler.authorship_create")?;
            let authorship = yoke::IdeaAuthorship::new(idea_id);
            let result = serde_json::to_value(&authorship)
                .map_err(|e| err("chronicler.authorship_create", e))?;
            ok_json(&result)
        });

        // ── yoke.authorship_record ────────────────────────────────
        // Record a creation or modification on an authorship tracker.
        state.phone.register_raw("chronicler.authorship_record", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let authorship_json = params.get("authorship")
                .ok_or_else(|| err("chronicler.authorship_record", "missing 'authorship'"))?;
            let digit_id = parse_uuid(&params, "digit_id", "chronicler.authorship_record")?;
            let source_json = params.get("source")
                .ok_or_else(|| err("chronicler.authorship_record", "missing 'source'"))?;
            let action = params.get("action").and_then(|v| v.as_str()).unwrap_or("creation");

            let mut authorship: yoke::IdeaAuthorship = serde_json::from_value(authorship_json.clone())
                .map_err(|e| err("chronicler.authorship_record", e))?;
            let source: yoke::AuthorshipSource = serde_json::from_value(source_json.clone())
                .map_err(|e| err("chronicler.authorship_record", e))?;

            match action {
                "creation" | "create" => authorship.record_creation(digit_id, source),
                "modification" | "modify" => authorship.record_modification(digit_id, source),
                _ => return Err(err("chronicler.authorship_record", "action must be 'creation' or 'modification'")),
            }

            let result = serde_json::to_value(&authorship)
                .map_err(|e| err("chronicler.authorship_record", e))?;
            ok_json(&result)
        });

        // ── yoke.authorship_summary ───────────────────────────────
        // Get a summary of an authorship tracker.
        state.phone.register_raw("chronicler.authorship_summary", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let authorship_json = params.get("authorship")
                .ok_or_else(|| err("chronicler.authorship_summary", "missing 'authorship'"))?;

            let authorship: yoke::IdeaAuthorship = serde_json::from_value(authorship_json.clone())
                .map_err(|e| err("chronicler.authorship_summary", e))?;

            ok_json(&json!({
                "idea_id": authorship.idea_id.to_string(),
                "human_actions": authorship.human_actions,
                "advisor_actions": authorship.advisor_actions,
                "advisor_percentage": authorship.advisor_percentage,
                "human_percentage": authorship.human_percentage(),
                "total_actions": authorship.total_actions(),
                "digits_with_ai": authorship.digits_with_ai(),
                "digits_purely_human": authorship.digits_purely_human()
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Event Kind Utilities
        // ═══════════════════════════════════════════════════════════

        // ── yoke.is_yoke_kind ─────────────────────────────────────
        // Check if an event kind is in Yoke's range (25000-25999).
        state.phone.register_raw("chronicler.is_yoke_kind", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let kind = params.get("kind").and_then(|v| v.as_u64())
                .ok_or_else(|| err("chronicler.is_yoke_kind", "missing 'kind'"))? as u32;
            ok_json(&json!({ "is_yoke": yoke::kind::is_yoke_kind(kind) }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Version history
            .with_call(CallDescriptor::new("chronicler.version_chain_create", "Create version chain for idea"))
            .with_call(CallDescriptor::new("chronicler.version_tag", "Create a version tag"))
            .with_call(CallDescriptor::new("chronicler.version_chain_tag", "Add tag to version chain"))
            .with_call(CallDescriptor::new("chronicler.version_chain_branch", "Create branch on chain"))
            .with_call(CallDescriptor::new("chronicler.version_chain_merge", "Merge branch on chain"))
            // Relationship graph
            .with_call(CallDescriptor::new("chronicler.link_create", "Create typed relationship link"))
            .with_call(CallDescriptor::new("chronicler.graph_query", "Query graph links"))
            .with_call(CallDescriptor::new("chronicler.graph_ancestors", "Find provenance ancestors"))
            .with_call(CallDescriptor::new("chronicler.graph_descendants", "Find provenance descendants"))
            .with_call(CallDescriptor::new("chronicler.graph_path", "Find shortest path"))
            .with_call(CallDescriptor::new("chronicler.graph_stats", "Graph link/entity counts"))
            // Timeline
            .with_call(CallDescriptor::new("chronicler.timeline_create", "Create activity timeline"))
            .with_call(CallDescriptor::new("chronicler.timeline_record", "Record activity"))
            .with_call(CallDescriptor::new("chronicler.timeline_milestone", "Mark milestone"))
            .with_call(CallDescriptor::new("chronicler.timeline_query", "Query timeline"))
            // Ceremonies
            .with_call(CallDescriptor::new("chronicler.ceremony_create", "Create + validate ceremony"))
            .with_call(CallDescriptor::new("chronicler.ceremony_validate", "Validate ceremony structure"))
            // Provenance
            .with_call(CallDescriptor::new("chronicler.provenance_compute", "Compute provenance score"))
            .with_call(CallDescriptor::new("chronicler.provenance_chain", "Build provenance chain"))
            // AI transparency
            .with_call(CallDescriptor::new("chronicler.authorship_create", "Create authorship tracker"))
            .with_call(CallDescriptor::new("chronicler.authorship_record", "Record creation/modification"))
            .with_call(CallDescriptor::new("chronicler.authorship_summary", "Get authorship summary"))
            // Utility
            .with_call(CallDescriptor::new("chronicler.is_yoke_kind", "Check if kind is Yoke range"))
            // Events
            .with_emitted_event(EventDescriptor::new("chronicler.ceremony_created", "Ceremony was created"))
    }
}

/// Parse an array of EventData from JSON.
/// EventData doesn't impl Deserialize, so we construct manually.
fn parse_event_data_array(val: &Value, op: &str) -> Result<Vec<yoke::EventData>, PhoneError> {
    let arr = val.as_array().ok_or_else(|| err(op, "events must be an array"))?;
    arr.iter().map(|item| {
        let id = item.get("id").and_then(|v| v.as_str())
            .ok_or_else(|| err(op, "event missing 'id'"))?;
        let author = item.get("author").and_then(|v| v.as_str())
            .ok_or_else(|| err(op, "event missing 'author'"))?;
        let community_id = item.get("community_id").and_then(|v| v.as_str()).map(String::from);
        let created_str = item.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
        let created_at: DateTime<Utc> = created_str.parse().unwrap_or_else(|_| Utc::now());
        Ok(yoke::EventData {
            id: id.to_string(),
            author: author.to_string(),
            community_id,
            created_at,
        })
    }).collect()
}
