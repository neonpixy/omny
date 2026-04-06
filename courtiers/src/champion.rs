//! Champion module — Quest gamification & progression courtier.
//!
//! Exposes XP awards, achievements, missions, challenges, cooperative
//! activities, rewards, progression status, and the quest observatory
//! as daemon operations. Quest is pure data — no I/O, no Vault needed.
//!
//! Programs use staff name "champion" which maps to daemon namespace "quest".

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct ChampionModule;

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

impl DaemonModule for ChampionModule {
    fn id(&self) -> &str { "champion" }
    fn name(&self) -> &str { "Champion (Quest)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Configuration
        // ═══════════════════════════════════════════════════════════

        // ── quest.config_presets ───────────────────────────────────
        // Get available configuration presets.
        state.phone.register_raw("champion.config_presets", move |_data| {
            let casual = serde_json::to_value(&quest::QuestConfig::casual())
                .map_err(|e| err("champion.config_presets", e))?;
            let standard = serde_json::to_value(&quest::QuestConfig::default())
                .map_err(|e| err("champion.config_presets", e))?;
            let ambitious = serde_json::to_value(&quest::QuestConfig::ambitious())
                .map_err(|e| err("champion.config_presets", e))?;
            ok_json(&json!({
                "casual": casual,
                "standard": standard,
                "ambitious": ambitious
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Engine — serialized round-trip via JSON
        // ═══════════════════════════════════════════════════════════

        // ── quest.engine_create ────────────────────────────────────
        // Create a new quest engine with optional config preset.
        state.phone.register_raw("champion.engine_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let preset = params.get("preset").and_then(|v| v.as_str()).unwrap_or("standard");

            let config = match preset {
                "casual" => quest::QuestConfig::casual(),
                "ambitious" => quest::QuestConfig::ambitious(),
                _ => quest::QuestConfig::default(),
            };

            let engine = quest::QuestEngine::new(config);
            let summary = engine.summary();
            let summary_json = serde_json::to_value(&summary)
                .map_err(|e| err("champion.engine_create", e))?;
            ok_json(&json!({ "created": true, "summary": summary_json }))
        });

        // ═══════════════════════════════════════════════════════════
        // Progression & XP
        // ═══════════════════════════════════════════════════════════

        // ── quest.status ──────────────────────────────────────────
        // Get a QuestStatus snapshot for an actor from a serialized engine.
        state.phone.register_raw("champion.status", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let actor = params.get("actor").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.status", "missing 'actor'"))?;

            // Minimal status for an unknown actor
            let status = quest::QuestStatus {
                actor: actor.to_string(),
                level: params.get("level").and_then(|v| v.as_u64()).unwrap_or(1) as u32,
                total_xp: params.get("total_xp").and_then(|v| v.as_u64()).unwrap_or(0),
                xp_to_next_level: params.get("xp_to_next_level").and_then(|v| v.as_u64()).unwrap_or(100),
                active_missions: params.get("active_missions").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                completed_missions: params.get("completed_missions").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                achievements_earned: params.get("achievements_earned").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                total_cool_earned: params.get("total_cool_earned").and_then(|v| v.as_u64()).unwrap_or(0),
                badges: vec![],
                streak_days: params.get("streak_days").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                suggested_difficulty: quest::Difficulty::Normal,
            };

            let status_json = serde_json::to_value(&status)
                .map_err(|e| err("champion.status", e))?;
            ok_json(&status_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Missions
        // ═══════════════════════════════════════════════════════════

        // ── quest.mission_create ──────────────────────────────────
        // Create a mission definition.
        state.phone.register_raw("champion.mission_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.mission_create", "missing 'name'"))?;
            let description = params.get("description").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.mission_create", "missing 'description'"))?;
            let xp_reward = params.get("xp_reward").and_then(|v| v.as_u64()).unwrap_or(0);

            let mut mission = quest::Mission::new(name, description);
            if xp_reward > 0 {
                mission = mission.with_xp_reward(xp_reward);
            }

            // Add objectives if provided
            if let Some(objectives) = params.get("objectives").and_then(|v| v.as_array()) {
                for obj in objectives {
                    let obj_id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("objective");
                    let obj_desc = obj.get("description").and_then(|v| v.as_str()).unwrap_or("Objective");
                    let target = obj.get("target").and_then(|v| v.as_u64()).unwrap_or(1);
                    let metric = obj.get("metric").and_then(|v| v.as_str()).unwrap_or("metric");
                    mission = mission.with_objective(quest::Objective::new(obj_id, obj_desc, target, metric));
                }
            }

            let mission_json = serde_json::to_value(&mission)
                .map_err(|e| err("champion.mission_create", e))?;
            ok_json(&mission_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Achievements
        // ═══════════════════════════════════════════════════════════

        // ── quest.achievement_create ──────────────────────────────
        // Create an achievement definition.
        state.phone.register_raw("champion.achievement_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = params.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.achievement_create", "missing 'id'"))?;
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.achievement_create", "missing 'name'"))?;
            let description = params.get("description").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.achievement_create", "missing 'description'"))?;
            let criteria_id = params.get("criteria_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.achievement_create", "missing 'criteria_id'"))?;

            let default_category = Value::String("Creation".into());
            let category_json = params.get("category")
                .unwrap_or(&default_category);
            let default_tier = Value::String("Bronze".into());
            let tier_json = params.get("tier")
                .unwrap_or(&default_tier);

            let category: quest::AchievementCategory = serde_json::from_value(category_json.clone())
                .map_err(|e| err("champion.achievement_create", e))?;
            let tier: quest::AchievementTier = serde_json::from_value(tier_json.clone())
                .map_err(|e| err("champion.achievement_create", e))?;

            let achievement = quest::Achievement::new(id, name, description, category, criteria_id, tier);
            let achievement_json = serde_json::to_value(&achievement)
                .map_err(|e| err("champion.achievement_create", e))?;
            ok_json(&achievement_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Challenges
        // ═══════════════════════════════════════════════════════════

        // ── quest.challenge_create ────────────────────────────────
        // Create a challenge definition.
        state.phone.register_raw("champion.challenge_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.challenge_create", "missing 'name'"))?;
            let description = params.get("description").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.challenge_create", "missing 'description'"))?;

            let challenge = quest::Challenge::new(name, description);
            let challenge_json = serde_json::to_value(&challenge)
                .map_err(|e| err("champion.challenge_create", e))?;
            ok_json(&challenge_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Rewards
        // ═══════════════════════════════════════════════════════════

        // ── quest.reward_create ───────────────────────────────────
        // Create a reward definition.
        state.phone.register_raw("champion.reward_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let reward_type_json = params.get("reward_type")
                .ok_or_else(|| err("champion.reward_create", "missing 'reward_type'"))?;
            let source_id = params.get("source_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.reward_create", "missing 'source_id'"))?;
            let source_type_json = params.get("source_type")
                .ok_or_else(|| err("champion.reward_create", "missing 'source_type'"))?;
            let recipient = params.get("recipient").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.reward_create", "missing 'recipient'"))?;

            let reward_type: quest::RewardType = serde_json::from_value(reward_type_json.clone())
                .map_err(|e| err("champion.reward_create", e))?;
            let source_type: quest::RewardSource = serde_json::from_value(source_type_json.clone())
                .map_err(|e| err("champion.reward_create", e))?;

            let reward = quest::Reward::new(reward_type, source_id, source_type, recipient);
            let reward_json = serde_json::to_value(&reward)
                .map_err(|e| err("champion.reward_create", e))?;
            ok_json(&reward_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Badges
        // ═══════════════════════════════════════════════════════════

        // ── quest.badge_create ────────────────────────────────────
        // Create a badge definition.
        state.phone.register_raw("champion.badge_create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = params.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.badge_create", "missing 'id'"))?;
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.badge_create", "missing 'name'"))?;
            let description = params.get("description").and_then(|v| v.as_str())
                .ok_or_else(|| err("champion.badge_create", "missing 'description'"))?;
            let icon = params.get("icon").and_then(|v| v.as_str()).unwrap_or("badge");

            let default_tier = Value::String("Bronze".into());
            let tier_json = params.get("tier").unwrap_or(&default_tier);
            let tier: quest::BadgeTier = serde_json::from_value(tier_json.clone())
                .map_err(|e| err("champion.badge_create", e))?;

            let badge = quest::Badge::new(id, name, description, icon, tier);
            let badge_json = serde_json::to_value(&badge)
                .map_err(|e| err("champion.badge_create", e))?;
            ok_json(&badge_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Observatory (deidentified metrics for Undercroft)
        // ═══════════════════════════════════════════════════════════

        // ── quest.observatory_report ──────────────────────────────
        // Generate an observatory report from a quest engine.
        state.phone.register_raw("champion.observatory_report", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let summary_json = params.get("summary")
                .ok_or_else(|| err("champion.observatory_report", "missing 'summary'"))?;

            let summary: quest::QuestSummary = serde_json::from_value(summary_json.clone())
                .map_err(|e| err("champion.observatory_report", e))?;

            // Return the summary as the observatory view (deidentified)
            ok_json(&json!({
                "total_participants": summary.total_participants,
                "total_achievements_earned": summary.total_achievements_earned,
                "total_missions_completed": summary.total_missions_completed,
                "active_challenges": summary.active_challenges,
                "active_raids": summary.active_raids,
                "total_cool_distributed": summary.total_cool_distributed
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Streak
        // ═══════════════════════════════════════════════════════════

        // ── quest.streak_info ─────────────────────────────────────
        // Get streak information for an actor (from a status snapshot).
        state.phone.register_raw("champion.streak_info", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let status_json = params.get("status")
                .ok_or_else(|| err("champion.streak_info", "missing 'status'"))?;

            let status: quest::QuestStatus = serde_json::from_value(status_json.clone())
                .map_err(|e| err("champion.streak_info", e))?;

            ok_json(&json!({
                "streak_days": status.streak_days,
                "suggested_difficulty": format!("{:?}", status.suggested_difficulty)
            }))
        });

        // ═══════════════════════════════════════════════════════════
        // Difficulty & Calibration
        // ═══════════════════════════════════════════════════════════

        // ── quest.difficulty_levels ────────────────────────────────
        // List available difficulty levels.
        state.phone.register_raw("champion.difficulty_levels", move |_data| {
            ok_json(&json!({
                "levels": ["Gentle", "Normal", "Ambitious", "Heroic"]
            }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Config
            .with_call(CallDescriptor::new("champion.config_presets", "Get config presets"))
            .with_call(CallDescriptor::new("champion.engine_create", "Create quest engine"))
            // Status & Progression
            .with_call(CallDescriptor::new("champion.status", "Get actor quest status"))
            .with_call(CallDescriptor::new("champion.streak_info", "Get streak info"))
            .with_call(CallDescriptor::new("champion.difficulty_levels", "List difficulty levels"))
            // Missions
            .with_call(CallDescriptor::new("champion.mission_create", "Create mission"))
            // Achievements
            .with_call(CallDescriptor::new("champion.achievement_create", "Create achievement"))
            // Challenges
            .with_call(CallDescriptor::new("champion.challenge_create", "Create challenge"))
            // Rewards & Badges
            .with_call(CallDescriptor::new("champion.reward_create", "Create reward"))
            .with_call(CallDescriptor::new("champion.badge_create", "Create badge"))
            // Observatory
            .with_call(CallDescriptor::new("champion.observatory_report", "Deidentified metrics"))
            // Events
            .with_emitted_event(EventDescriptor::new("champion.achievement_unlocked", "Achievement earned"))
            .with_emitted_event(EventDescriptor::new("champion.level_up", "Actor leveled up"))
            .with_emitted_event(EventDescriptor::new("champion.mission_completed", "Mission completed"))
    }
}
