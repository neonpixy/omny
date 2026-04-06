//! Sage module — Advisor AI cognition courtier.
//!
//! The AI backbone of Omnidea. Wraps the entire Advisor crate as daemon
//! operations: cognitive loop, thoughts, sessions, memory, expression
//! pressure, skills, consent gates, governance delegation, and pluggable
//! inference providers (Claude, Ollama, LM Studio).
//!
//! Architecture:
//! - Three-mutex state: cognitive (loop), store (data), engine (providers)
//! - Pulse loop: background thread drives cognitive_loop.tick() periodically
//! - Async generation: tokio tasks make HTTP calls to inference providers
//! - Channels: mpsc for generation results + skill results → pulse thread
//!
//! The pulse thread is the sole mutator of CognitiveLoop state.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};
use uuid::Uuid;

use prerogative::DaemonModule;
use prerogative::DaemonState;

// ═══════════════════════════════════════════════════════════════════
// State types — three independent mutexes
// ═══════════════════════════════════════════════════════════════════

/// Cognitive loop state — only the pulse thread mutates this.
struct CognitiveSplit {
    cognitive_loop: advisor::CognitiveLoop,
    config: advisor::AdvisorConfig,
}

/// Engine state — providers, consent, skills, governance.
struct EngineSplit {
    router: advisor::ProviderRouter,
    consent: advisor::ConsentProfile,
    governance: Option<advisor::GovernanceMode>,
    bridge_registry: advisor::BridgeRegistry,
    skill_registry: advisor::SkillRegistry,
    pending_actions: Vec<advisor::PendingAction>,
    token_usage: TokenUsage,
}

/// Token budget tracking (Covenant: Sovereignty — user controls spend).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TokenUsage {
    tokens_used_today: usize,
    daily_limit: usize,
    period_start: chrono::DateTime<chrono::Utc>,
}

impl Default for TokenUsage {
    fn default() -> Self {
        Self {
            tokens_used_today: 0,
            daily_limit: 100_000,
            period_start: chrono::Utc::now(),
        }
    }
}

impl TokenUsage {
    fn check_and_reset(&mut self) {
        let now = chrono::Utc::now();
        if now.signed_duration_since(self.period_start).num_hours() >= 24 {
            self.tokens_used_today = 0;
            self.period_start = now;
        }
    }

    fn record(&mut self, tokens: usize) {
        self.check_and_reset();
        self.tokens_used_today += tokens;
    }

    fn budget_remaining(&mut self) -> usize {
        self.check_and_reset();
        self.daily_limit.saturating_sub(self.tokens_used_today)
    }
}

/// Shared state handles for all Sage subsystems.
#[derive(Clone)]
struct SageHandles {
    cognitive: Arc<Mutex<CognitiveSplit>>,
    store: Arc<Mutex<advisor::CognitiveStore>>,
    engine: Arc<Mutex<EngineSplit>>,
    pulse_active: Arc<AtomicBool>,
    /// Send generation requests to the async worker.
    gen_work_tx: mpsc::Sender<advisor::GenerationContext>,
    /// Receive generation results in the pulse thread.
    gen_result_rx: Arc<Mutex<mpsc::Receiver<advisor::GenerationResult>>>,
    /// Send generation results from async worker.
    gen_result_tx: mpsc::Sender<advisor::GenerationResult>,
}

// ═══════════════════════════════════════════════════════════════════
// Module
// ═══════════════════════════════════════════════════════════════════

pub struct SageModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for SageModule {
    fn id(&self) -> &str { "sage" }
    fn name(&self) -> &str { "Sage (Advisor)" }
    fn deps(&self) -> &[&str] { &["chamberlain", "castellan"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── Initialize state ──────────────────────────────────
        let home_session_id = Uuid::new_v4();
        let config = advisor::AdvisorConfig::default();
        let cognitive_loop = advisor::CognitiveLoop::new(config.clone(), home_session_id);

        let cognitive = Arc::new(Mutex::new(CognitiveSplit {
            cognitive_loop,
            config,
        }));

        let store = Arc::new(Mutex::new(advisor::CognitiveStore::new(100)));

        let mut skill_registry = advisor::SkillRegistry::new();
        advisor::skill::programs::register_all_skills(&mut skill_registry);

        let engine = Arc::new(Mutex::new(EngineSplit {
            router: advisor::ProviderRouter::new(advisor::ProviderRegistry::new()),
            consent: advisor::ConsentProfile::default(),
            governance: None,
            bridge_registry: advisor::BridgeRegistry::with_defaults(),
            skill_registry,
            pending_actions: Vec::new(),
            token_usage: TokenUsage::default(),
        }));

        let pulse_active = Arc::new(AtomicBool::new(false));

        // Channels for generation pipeline
        let (gen_work_tx, gen_work_rx) = mpsc::channel::<advisor::GenerationContext>();
        let (gen_result_tx, gen_result_rx) = mpsc::channel::<advisor::GenerationResult>();

        let handles = SageHandles {
            cognitive: cognitive.clone(),
            store: store.clone(),
            engine: engine.clone(),
            pulse_active: pulse_active.clone(),
            gen_work_tx,
            gen_result_rx: Arc::new(Mutex::new(gen_result_rx)),
            gen_result_tx: gen_result_tx.clone(),
        };

        // Spawn the async generation worker
        spawn_generation_worker(
            gen_work_rx,
            gen_result_tx,
            engine.clone(),
            state.clone(),
        );

        // ═══════════════════════════════════════════════════════
        // Lifecycle
        // ═══════════════════════════════════════════════════════

        // ── advisor.status ────────────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.status", move |_data| {
            let cog = h.cognitive.lock().unwrap();
            let eng = h.engine.lock().unwrap();

            let pressure = cog.cognitive_loop.pressure.value();
            let mode = format!("{:?}", cog.cognitive_loop.mode);
            let provider_count = eng.router.registry.len();
            let pulse = h.pulse_active.load(Ordering::Relaxed);
            let budget = eng.token_usage.tokens_used_today;
            let budget_limit = eng.token_usage.daily_limit;

            ok_json(&json!({
                "mode": mode,
                "pressure": pressure,
                "pulse_active": pulse,
                "provider_count": provider_count,
                "tokens_used_today": budget,
                "daily_token_limit": budget_limit,
                "home_session_id": cog.cognitive_loop.inner_voice.home_session_id.to_string(),
            }))
        });

        // ── advisor.start_pulse ───────────────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.start_pulse", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let interval_ms = params.get("interval_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(2000);

            if h.pulse_active.load(Ordering::Relaxed) {
                return ok_json(&json!({ "status": "already_running" }));
            }

            h.pulse_active.store(true, Ordering::SeqCst);
            spawn_pulse_loop(
                h.clone(),
                s.clone(),
                Duration::from_millis(interval_ms),
            );

            ok_json(&json!({ "status": "started", "interval_ms": interval_ms }))
        });

        // ── advisor.stop_pulse ────────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.stop_pulse", move |_data| {
            h.pulse_active.store(false, Ordering::SeqCst);
            ok_json(&json!({ "status": "stopped" }))
        });

        // ── advisor.mode ──────────────────────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.mode", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let mut cog = h.cognitive.lock().unwrap();

            if let Some(mode_str) = params.get("set").and_then(|v| v.as_str()) {
                let mode = match mode_str {
                    "autonomous" => advisor::CognitiveMode::Autonomous,
                    _ => advisor::CognitiveMode::Assistant,
                };
                let actions = cog.cognitive_loop.apply_command(
                    advisor::StateCommand::SetMode(mode),
                );
                drop(cog);
                process_actions(&actions, &h, &s);
            } else {
                drop(cog);
            }

            let cog = h.cognitive.lock().unwrap();
            ok_json(&json!({ "mode": format!("{:?}", cog.cognitive_loop.mode) }))
        });

        // ═══════════════════════════════════════════════════════
        // Conversation
        // ═══════════════════════════════════════════════════════

        // ── advisor.begin_session ─────────────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.begin_session", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let summary = params.get("summary").and_then(|v| v.as_str()).unwrap_or("Conversation");

            let session = advisor::Session::user(summary.to_string());
            let session_id = session.id;

            let mut st = h.store.lock().unwrap();
            st.save_session(session);
            drop(st);

            let mut cog = h.cognitive.lock().unwrap();
            cog.cognitive_loop.begin_conversation();
            drop(cog);

            s.email.send_raw("sage.session.started",
                &serde_json::to_vec(&json!({ "session_id": session_id.to_string() })).unwrap_or_default());

            ok_json(&json!({ "session_id": session_id.to_string() }))
        });

        // ── advisor.end_session ───────────────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.end_session", move |_data| {
            let mut cog = h.cognitive.lock().unwrap();
            cog.cognitive_loop.end_conversation();
            drop(cog);

            s.email.send_raw("sage.session.ended", &[]);
            ok_json(&json!({ "status": "ended" }))
        });

        // ── advisor.send_message ──────────────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.send_message", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let content = params.get("content").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.send_message", "missing 'content'"))?;
            let session_id = params.get("session_id").and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            // Create user thought
            let thought = advisor::Thought::new(
                session_id.unwrap_or(Uuid::nil()),
                content.to_string(),
                advisor::ThoughtSource::User,
            );
            let thought_id = thought.id;

            let mut st = h.store.lock().unwrap();
            st.save_thought(thought);
            drop(st);

            // Apply pressure event (user spoke → increase urgency)
            let mut cog = h.cognitive.lock().unwrap();
            let actions = cog.cognitive_loop.apply_command(
                advisor::StateCommand::AdjustPressure(
                    advisor::PressureEvent::NovelContent,
                ),
            );
            drop(cog);

            process_actions(&actions, &h, &s);
            ok_json(&json!({ "thought_id": thought_id.to_string() }))
        });

        // ── advisor.generate ──────────────────────────────────
        // Explicit generation request (bypasses pressure threshold).
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.generate", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let prompt = params.get("prompt").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.generate", "missing 'prompt'"))?;
            let max_tokens = params.get("max_tokens").and_then(|v| v.as_u64())
                .map(|t| t as usize);

            let mut context = advisor::GenerationContext::new()
                .with_message(advisor::ConversationMessage::user(prompt));

            if let Some(max) = max_tokens {
                context = context.with_max_tokens(max);
            }

            h.gen_work_tx.send(context)
                .map_err(|e| err("sage.generate", format!("channel closed: {e}")))?;

            s.email.send_raw("sage.generation.started",
                &serde_json::to_vec(&json!({ "explicit": true })).unwrap_or_default());

            ok_json(&json!({ "status": "generating" }))
        });

        // ── advisor.receive_generation ────────────────────────
        // For external providers that call back via TypeScript.
        let h = handles.clone();
        state.phone.register_raw("sage.receive_generation", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let content = params.get("content").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.receive_generation", "missing 'content'"))?;
            let provider_id = params.get("provider_id").and_then(|v| v.as_str())
                .unwrap_or("external");
            let tokens = params.get("tokens_used").and_then(|v| v.as_u64())
                .map(|t| t as usize);

            let result = advisor::GenerationResult {
                content: content.to_string(),
                tokens_used: tokens,
                finish_reason: advisor::FinishReason::Complete,
                provider_id: provider_id.to_string(),
            };

            h.gen_result_tx.send(result)
                .map_err(|e| err("sage.receive_generation", format!("channel closed: {e}")))?;

            ok_json(&json!({ "status": "received" }))
        });

        // ── advisor.sessions ──────────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.sessions", move |_data| {
            let st = h.store.lock().unwrap();
            let sessions: Vec<Value> = st.active_sessions().iter()
                .filter_map(|s| serde_json::to_value(s).ok())
                .collect();
            ok_json(&Value::Array(sessions))
        });

        // ═══════════════════════════════════════════════════════
        // Cognitive State
        // ═══════════════════════════════════════════════════════

        // ── advisor.get_thought ───────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.get_thought", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = parse_uuid(&params, "id", "sage.get_thought")?;
            let st = h.store.lock().unwrap();
            match st.get_thought(id) {
                Some(t) => {
                    let v = serde_json::to_value(t).map_err(|e| err("sage.get_thought", e))?;
                    ok_json(&v)
                }
                None => ok_json(&Value::Null),
            }
        });

        // ── advisor.get_thoughts ──────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.get_thoughts", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let session_id = parse_uuid(&params, "session_id", "sage.get_thoughts")?;
            let st = h.store.lock().unwrap();
            let thoughts: Vec<Value> = st.thoughts_for_session(session_id).iter()
                .filter_map(|t| serde_json::to_value(t).ok())
                .collect();
            ok_json(&Value::Array(thoughts))
        });

        // ── advisor.save_memory ───────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.save_memory", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let content = params.get("content").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.save_memory", "missing 'content'"))?;
            let tags: Vec<String> = params.get("tags")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut memory = advisor::Memory::new(content.to_string());
            memory.tags = tags;
            let memory_id = memory.id;
            let mut st = h.store.lock().unwrap();
            st.save_memory(memory);

            ok_json(&json!({ "memory_id": memory_id.to_string() }))
        });

        // ── advisor.search_memories ───────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.search_memories", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let query = params.get("query").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.search_memories", "missing 'query'"))?;
            let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;

            let mut st = h.store.lock().unwrap();
            let results = st.search_memories(query, limit);
            let results_json: Vec<Value> = results.iter()
                .filter_map(|r| serde_json::to_value(r).ok())
                .collect();
            ok_json(&Value::Array(results_json))
        });

        // ── advisor.add_synapse ───────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.add_synapse", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let synapse: advisor::Synapse = serde_json::from_value(params)
                .map_err(|e| err("sage.add_synapse", format!("invalid synapse: {e}")))?;
            let synapse_id = synapse.id;
            let mut st = h.store.lock().unwrap();
            st.save_synapse(synapse);
            ok_json(&json!({ "synapse_id": synapse_id.to_string() }))
        });

        // ── advisor.query_synapses ────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.query_synapses", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let entity_id = parse_uuid(&params, "entity_id", "sage.query_synapses")?;
            let st = h.store.lock().unwrap();
            let entity_type_str = params.get("entity_type").and_then(|v| v.as_str()).unwrap_or("thought");
            let entity_type = match entity_type_str {
                "session" => advisor::EntityType::Session,
                "idea" => advisor::EntityType::Idea,
                "memory" => advisor::EntityType::Memory,
                _ => advisor::EntityType::Thought,
            };
            let results: Vec<&advisor::Synapse> = st.state.synapses.values()
                .filter(|s| s.involves(entity_type.clone(), entity_id))
                .collect();
            let results_json: Vec<Value> = results.iter()
                .filter_map(|s| serde_json::to_value(s).ok())
                .collect();
            ok_json(&Value::Array(results_json))
        });

        // ── advisor.clipboard_add ─────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.clipboard_add", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let content = params.get("content").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.clipboard_add", "missing 'content'"))?;
            let priority = params.get("priority").and_then(|v| v.as_f64()).unwrap_or(0.5);
            let mut st = h.store.lock().unwrap();
            let entry = advisor::ClipboardEntry::new(content.to_string(), priority);
            st.state.clipboard.add(entry);
            ok_json(&json!({ "status": "added" }))
        });

        // ── advisor.clipboard_read ────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.clipboard_read", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            let st = h.store.lock().unwrap();
            let entries: Vec<Value> = st.state.clipboard.recent(limit).iter()
                .take(limit)
                .filter_map(|e| serde_json::to_value(e).ok())
                .collect();
            ok_json(&Value::Array(entries))
        });

        // ═══════════════════════════════════════════════════════
        // Provider Management
        // ═══════════════════════════════════════════════════════

        // ── advisor.register_provider ─────────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.register_provider", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let provider_type = params.get("type").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.register_provider", "missing 'type'"))?;

            let mut eng = h.engine.lock().unwrap();
            match provider_type {
                "claude" => {
                    let model = params.get("model").and_then(|v| v.as_str())
                        .unwrap_or("claude-sonnet-4-6");
                    let has_key = params.get("api_key_configured")
                        .and_then(|v| v.as_bool()).unwrap_or(false);
                    let provider = advisor::ClaudeProvider::new(model, has_key);
                    eng.router.registry.register(Box::new(provider));
                }
                "local" | "ollama" | "lm_studio" => {
                    let model = params.get("model").and_then(|v| v.as_str())
                        .unwrap_or("local-model");
                    let mut provider = advisor::LocalProvider::new(model);
                    if params.get("model_loaded").and_then(|v| v.as_bool()).unwrap_or(false) {
                        provider.set_model_loaded(true);
                    }
                    eng.router.registry.register(Box::new(provider));
                }
                other => {
                    return Err(err("sage.register_provider",
                        format!("unknown provider type: {other}")));
                }
            }
            drop(eng);

            s.email.send_raw("sage.provider.changed",
                &serde_json::to_vec(&json!({ "action": "registered", "type": provider_type }))
                    .unwrap_or_default());

            ok_json(&json!({ "status": "registered", "type": provider_type }))
        });

        // ── advisor.unregister_provider ───────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.unregister_provider", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let id = params.get("id").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.unregister_provider", "missing 'id'"))?;

            let mut eng = h.engine.lock().unwrap();
            eng.router.registry.unregister(id)
                .map_err(|e| err("sage.unregister_provider", e))?;
            drop(eng);

            s.email.send_raw("sage.provider.changed",
                &serde_json::to_vec(&json!({ "action": "unregistered", "id": id }))
                    .unwrap_or_default());

            ok_json(&json!({ "status": "unregistered" }))
        });

        // ── advisor.list_providers ────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.list_providers", move |_data| {
            let eng = h.engine.lock().unwrap();
            let providers: Vec<Value> = eng.router.registry.provider_info().iter()
                .filter_map(|p| serde_json::to_value(p).ok())
                .collect();
            ok_json(&Value::Array(providers))
        });

        // ── advisor.set_preferences ───────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.set_preferences", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let mut eng = h.engine.lock().unwrap();

            if let Some(strategy) = params.get("strategy").and_then(|v| v.as_str()) {
                eng.router.preferences.strategy = match strategy {
                    "cost" => advisor::SelectionStrategy::CostOptimized,
                    "quality" => advisor::SelectionStrategy::QualityOptimized,
                    "speed" => advisor::SelectionStrategy::SpeedOptimized,
                    _ => advisor::SelectionStrategy::PriorityOrder,
                };
            }

            if let Some(tier) = params.get("security_tier").and_then(|v| v.as_str()) {
                eng.router.security_tier = match tier {
                    "hardened" => advisor::SecurityTier::Hardened,
                    "ultimate" => advisor::SecurityTier::Ultimate,
                    _ => advisor::SecurityTier::Balanced,
                };
            }

            if let Some(preferred) = params.get("preferred_provider").and_then(|v| v.as_str()) {
                eng.router.preferences.preferred_provider = Some(preferred.to_string());
            }

            ok_json(&json!({ "status": "updated" }))
        });

        // ── advisor.select_provider ───────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.select_provider", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let require_tools = params.get("require_tools")
                .and_then(|v| v.as_bool()).unwrap_or(false);

            let mut caps = advisor::ProviderCapabilities::empty();
            if require_tools {
                caps |= advisor::ProviderCapabilities::TOOL_CALLING;
            }

            let eng = h.engine.lock().unwrap();
            match eng.router.select(caps) {
                Ok(provider) => {
                    let info = advisor::ProviderInfo::from_provider(provider);
                    let v = serde_json::to_value(&info)
                        .map_err(|e| err("sage.select_provider", e))?;
                    ok_json(&v)
                }
                Err(e) => ok_json(&json!({ "error": e.to_string() })),
            }
        });

        // ═══════════════════════════════════════════════════════
        // Consent
        // ═══════════════════════════════════════════════════════

        // ── advisor.consent_profile ───────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.consent_profile", move |_data| {
            let eng = h.engine.lock().unwrap();
            let v = serde_json::to_value(&eng.consent)
                .map_err(|e| err("sage.consent_profile", e))?;
            ok_json(&v)
        });

        // ── advisor.set_consent ───────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.set_consent", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let level = params.get("level").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.set_consent", "missing 'level'"))?;

            let consent_level = match level {
                "silent" => advisor::ConsentLevel::Silent,
                "urgent_only" => advisor::ConsentLevel::UrgentOnly,
                "autonomous" => advisor::ConsentLevel::Autonomous,
                _ => advisor::ConsentLevel::Normal,
            };

            let mut cog = h.cognitive.lock().unwrap();
            cog.cognitive_loop.consent = advisor::ExpressionConsent {
                granted: true,
                level: consent_level,
            };

            ok_json(&json!({ "level": level }))
        });

        // ── advisor.set_auto_approve ──────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.set_auto_approve", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let action = params.get("action").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.set_auto_approve", "missing 'action'"))?;
            let auto = params.get("auto_approve").and_then(|v| v.as_bool())
                .ok_or_else(|| err("sage.set_auto_approve", "missing 'auto_approve'"))?;

            let escalation = parse_escalation(action)
                .ok_or_else(|| err("sage.set_auto_approve", "invalid action level"))?;

            let mut eng = h.engine.lock().unwrap();
            eng.consent.set_auto_approve(escalation, auto);

            ok_json(&json!({ "action": action, "auto_approve": auto }))
        });

        // ── advisor.record_approval ───────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.record_approval", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let action = params.get("action").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.record_approval", "missing 'action'"))?;
            let approved = params.get("approved").and_then(|v| v.as_bool())
                .ok_or_else(|| err("sage.record_approval", "missing 'approved'"))?;
            let description = params.get("description").and_then(|v| v.as_str())
                .unwrap_or("user decision");

            let escalation = parse_escalation(action)
                .ok_or_else(|| err("sage.record_approval", "invalid action level"))?;

            let approval = if approved {
                advisor::ConsentApproval::approve(description)
            } else {
                advisor::ConsentApproval::reject(description)
            };

            let mut eng = h.engine.lock().unwrap();
            eng.consent.record_approval(escalation, approval);

            ok_json(&json!({ "recorded": true }))
        });

        // ── advisor.pending_actions ───────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.pending_actions", move |_data| {
            let eng = h.engine.lock().unwrap();
            let actions: Vec<Value> = eng.pending_actions.iter()
                .filter(|a| !a.is_expired())
                .filter_map(|a| serde_json::to_value(a).ok())
                .collect();
            ok_json(&Value::Array(actions))
        });

        // ═══════════════════════════════════════════════════════
        // Skills
        // ═══════════════════════════════════════════════════════

        // ── advisor.list_skills ───────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.list_skills", move |_data| {
            let eng = h.engine.lock().unwrap();
            let skills: Vec<Value> = eng.skill_registry.all().iter()
                .filter_map(|s| serde_json::to_value(s).ok())
                .collect();
            ok_json(&Value::Array(skills))
        });

        // ── advisor.invoke_skill ──────────────────────────────
        let h = handles.clone();
        let s = state.clone();
        state.phone.register_raw("sage.invoke_skill", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let skill_id = params.get("skill_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("sage.invoke_skill", "missing 'skill_id'"))?;
            let arguments: std::collections::HashMap<String, Value> = params.get("arguments")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut call = advisor::SkillCall::new(
                Uuid::new_v4().to_string(),
                skill_id,
            );
            for (k, v) in arguments {
                call = call.with_argument(k, v);
            }

            let eng = h.engine.lock().unwrap();
            match eng.bridge_registry.translate(&call) {
                Ok(advisor::BridgeOutput::DirectResult(result)) => {
                    let v = serde_json::to_value(&result)
                        .map_err(|e| err("sage.invoke_skill", e))?;
                    ok_json(&v)
                }
                Ok(advisor::BridgeOutput::Action(action)) => {
                    // Route through Equipment Phone to the relevant courtier
                    let action_json = serde_json::to_value(&action)
                        .map_err(|e| err("sage.invoke_skill", e))?;
                    drop(eng);

                    s.email.send_raw("sage.skill.executed",
                        &serde_json::to_vec(&json!({
                            "skill_id": skill_id,
                            "action_type": "magic_action",
                        })).unwrap_or_default());

                    ok_json(&json!({ "action": action_json, "routed": true }))
                }
                Err(e) => Err(err("sage.invoke_skill", e)),
            }
        });

        // ═══════════════════════════════════════════════════════
        // Governance
        // ═══════════════════════════════════════════════════════

        // ── advisor.evaluate_proposal ─────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.evaluate_proposal", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let eng = h.engine.lock().unwrap();

            match &eng.governance {
                Some(gov) => {
                    let proposal_id = params.get("proposal_id").and_then(|v| v.as_str())
                        .and_then(|s| Uuid::parse_str(s).ok())
                        .unwrap_or_else(Uuid::new_v4);
                    let summary = params.get("summary").and_then(|v| v.as_str())
                        .unwrap_or("Proposal");
                    let topics: Vec<String> = params.get("topics")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();
                    let charter_sections: Vec<String> = params.get("charter_sections")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();

                    let impact = params.get("impact_assessment").and_then(|v| v.as_str())
                        .unwrap_or("Unknown impact");
                    let analysis = gov.analyze_proposal(
                        proposal_id, summary, &topics, &charter_sections, impact,
                    );
                    let v = serde_json::to_value(&analysis)
                        .map_err(|e| err("sage.evaluate_proposal", e))?;
                    ok_json(&v)
                }
                None => Err(err("sage.evaluate_proposal",
                    "governance mode not active — call advisor.governance_status to activate")),
            }
        });

        // ── advisor.governance_status ─────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.governance_status", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let mut eng = h.engine.lock().unwrap();

            // Optionally initialize governance mode
            if let Some(owner_pubkey) = params.get("activate").and_then(|v| v.as_str()) {
                if eng.governance.is_none() {
                    eng.governance = Some(advisor::GovernanceMode::new(owner_pubkey));
                }
            }

            match &eng.governance {
                Some(gov) => {
                    let v = serde_json::to_value(gov)
                        .map_err(|e| err("sage.governance_status", e))?;
                    ok_json(&v)
                }
                None => ok_json(&json!({ "active": false })),
            }
        });

        // ── advisor.get_config ────────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.get_config", move |_data| {
            let cog = h.cognitive.lock().unwrap();
            let v = serde_json::to_value(&cog.config)
                .map_err(|e| err("sage.get_config", e))?;
            ok_json(&v)
        });

        // ── advisor.set_config ────────────────────────────────
        let h = handles.clone();
        state.phone.register_raw("sage.set_config", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);

            let preset = params.get("preset").and_then(|v| v.as_str());
            let new_config = match preset {
                Some("contemplative") => advisor::AdvisorConfig::contemplative(),
                Some("responsive") => advisor::AdvisorConfig::responsive(),
                _ => advisor::AdvisorConfig::default(),
            };

            let mut cog = h.cognitive.lock().unwrap();
            cog.config = new_config;
            // Note: config changes take effect on next pulse cycle

            ok_json(&json!({ "status": "updated", "preset": preset }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Lifecycle
            .with_call(CallDescriptor::new("sage.status", "Get Sage status"))
            .with_call(CallDescriptor::new("sage.start_pulse", "Start cognitive pulse loop"))
            .with_call(CallDescriptor::new("sage.stop_pulse", "Stop cognitive pulse loop"))
            .with_call(CallDescriptor::new("sage.mode", "Get/set cognitive mode"))
            // Conversation
            .with_call(CallDescriptor::new("sage.begin_session", "Begin conversation session"))
            .with_call(CallDescriptor::new("sage.end_session", "End conversation session"))
            .with_call(CallDescriptor::new("sage.send_message", "Send user message"))
            .with_call(CallDescriptor::new("sage.generate", "Explicit generation request"))
            .with_call(CallDescriptor::new("sage.receive_generation", "Feed external generation result"))
            .with_call(CallDescriptor::new("sage.sessions", "List active sessions"))
            // Cognitive State
            .with_call(CallDescriptor::new("sage.get_thought", "Get thought by ID"))
            .with_call(CallDescriptor::new("sage.get_thoughts", "Get thoughts for session"))
            .with_call(CallDescriptor::new("sage.save_memory", "Save a memory"))
            .with_call(CallDescriptor::new("sage.search_memories", "Search memories"))
            .with_call(CallDescriptor::new("sage.add_synapse", "Add cognitive synapse"))
            .with_call(CallDescriptor::new("sage.query_synapses", "Query synapse graph"))
            .with_call(CallDescriptor::new("sage.clipboard_add", "Add to working memory"))
            .with_call(CallDescriptor::new("sage.clipboard_read", "Read working memory"))
            // Providers
            .with_call(CallDescriptor::new("sage.register_provider", "Register inference provider"))
            .with_call(CallDescriptor::new("sage.unregister_provider", "Remove inference provider"))
            .with_call(CallDescriptor::new("sage.list_providers", "List registered providers"))
            .with_call(CallDescriptor::new("sage.set_preferences", "Set provider preferences"))
            .with_call(CallDescriptor::new("sage.select_provider", "Test provider selection"))
            // Consent
            .with_call(CallDescriptor::new("sage.consent_profile", "Get consent profile"))
            .with_call(CallDescriptor::new("sage.set_consent", "Set expression consent level"))
            .with_call(CallDescriptor::new("sage.set_auto_approve", "Toggle action auto-approval"))
            .with_call(CallDescriptor::new("sage.record_approval", "Approve/reject pending action"))
            .with_call(CallDescriptor::new("sage.pending_actions", "List pending approval actions"))
            // Skills
            .with_call(CallDescriptor::new("sage.list_skills", "List registered skills"))
            .with_call(CallDescriptor::new("sage.invoke_skill", "Invoke a skill directly"))
            // Governance
            .with_call(CallDescriptor::new("sage.evaluate_proposal", "AI proposal analysis"))
            .with_call(CallDescriptor::new("sage.governance_status", "Governance delegation state"))
            // Config
            .with_call(CallDescriptor::new("sage.get_config", "Get advisor config"))
            .with_call(CallDescriptor::new("sage.set_config", "Set advisor config preset"))
            // Events
            .with_emitted_event(EventDescriptor::new("sage.pulse", "Cognitive pulse cycle"))
            .with_emitted_event(EventDescriptor::new("sage.expressed", "Thought expressed"))
            .with_emitted_event(EventDescriptor::new("sage.generation.started", "Generation request sent"))
            .with_emitted_event(EventDescriptor::new("sage.generation.completed", "Generation completed"))
            .with_emitted_event(EventDescriptor::new("sage.awakened", "Autonomous mode activated"))
            .with_emitted_event(EventDescriptor::new("sage.asleep", "Assistant mode activated"))
            .with_emitted_event(EventDescriptor::new("sage.session.started", "Conversation started"))
            .with_emitted_event(EventDescriptor::new("sage.session.ended", "Conversation ended"))
            .with_emitted_event(EventDescriptor::new("sage.provider.changed", "Provider registered/removed"))
            .with_emitted_event(EventDescriptor::new("sage.skill.executed", "Skill was invoked"))
    }
}

// ═══════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════

fn parse_uuid(params: &Value, key: &str, op: &str) -> Result<Uuid, PhoneError> {
    let s = params.get(key).and_then(|v| v.as_str())
        .ok_or_else(|| err(op, format!("missing '{key}'")))?;
    Uuid::parse_str(s).map_err(|e| err(op, format!("invalid UUID: {e}")))
}

fn parse_escalation(s: &str) -> Option<advisor::ConsentEscalation> {
    match s {
        "suggest" => Some(advisor::ConsentEscalation::Suggest),
        "create" => Some(advisor::ConsentEscalation::Create),
        "modify" => Some(advisor::ConsentEscalation::Modify),
        "publish" => Some(advisor::ConsentEscalation::Publish),
        "transact" => Some(advisor::ConsentEscalation::Transact),
        "govern" => Some(advisor::ConsentEscalation::Govern),
        "communicate" => Some(advisor::ConsentEscalation::Communicate),
        _ => None,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Pulse Loop
// ═══════════════════════════════════════════════════════════════════

/// Spawn the background pulse thread that drives the cognitive loop.
fn spawn_pulse_loop(
    handles: SageHandles,
    state: Arc<DaemonState>,
    interval: Duration,
) {
    std::thread::Builder::new()
        .name("sage-pulse".into())
        .spawn(move || {
            log::info!("sage: pulse loop started (interval {:?})", interval);
            loop {
                std::thread::sleep(interval);

                if state.shutdown.load(Ordering::Relaxed)
                    || !handles.pulse_active.load(Ordering::Relaxed)
                {
                    log::info!("sage: pulse loop stopping");
                    break;
                }

                // Drain generation results
                let gen_rx = handles.gen_result_rx.lock().unwrap();
                while let Ok(result) = gen_rx.try_recv() {
                    let mut cog = handles.cognitive.lock().unwrap();
                    let actions = cog.cognitive_loop.receive_generation(result);
                    drop(cog);
                    process_actions(&actions, &handles, &state);
                }
                drop(gen_rx);

                // Pulse the cognitive loop
                let mut cog = handles.cognitive.lock().unwrap();
                let actions = cog.cognitive_loop.tick(interval);
                drop(cog);

                process_actions(&actions, &handles, &state);
            }
        })
        .expect("sage: failed to spawn pulse thread");
}

/// Process CognitiveActions from the loop.
fn process_actions(
    actions: &[advisor::CognitiveAction],
    handles: &SageHandles,
    state: &Arc<DaemonState>,
) {
    for action in actions {
        match action {
            advisor::CognitiveAction::RequestGeneration(context) => {
                // Send to async generation worker
                if let Err(e) = handles.gen_work_tx.send(context.clone()) {
                    log::warn!("sage: generation channel closed: {e}");
                }
                state.email.send_raw("sage.generation.started",
                    &serde_json::to_vec(&json!({ "explicit": false })).unwrap_or_default());
            }
            advisor::CognitiveAction::Express(thought) => {
                let mut st = handles.store.lock().unwrap();
                st.save_thought(thought.clone());
                drop(st);

                let thought_json = serde_json::to_vec(
                    &serde_json::to_value(thought).unwrap_or(Value::Null)
                ).unwrap_or_default();
                state.email.send_raw("sage.expressed", &thought_json);
            }
            advisor::CognitiveAction::Store(thought) => {
                let mut st = handles.store.lock().unwrap();
                st.save_thought(thought.clone());
            }
            advisor::CognitiveAction::ModifyState(cmd) => {
                let mut cog = handles.cognitive.lock().unwrap();
                let sub_actions = cog.cognitive_loop.apply_command(cmd.clone());
                drop(cog);
                // Recurse for sub-actions (bounded depth — commands rarely chain)
                process_actions(&sub_actions, handles, state);
            }
            advisor::CognitiveAction::Emit(event) => {
                match event {
                    advisor::CognitiveEvent::Awakened => {
                        state.email.send_raw("sage.awakened", &[]);
                    }
                    advisor::CognitiveEvent::Asleep => {
                        state.email.send_raw("sage.asleep", &[]);
                    }
                    advisor::CognitiveEvent::TickCompleted { pressure, mode } => {
                        let data = serde_json::to_vec(
                            &json!({ "pressure": pressure, "mode": mode })
                        ).unwrap_or_default();
                        state.email.send_raw("sage.pulse", &data);
                    }
                    advisor::CognitiveEvent::PressureThresholdReached { pressure, threshold } => {
                        log::info!("sage: pressure threshold reached ({pressure:.2} >= {threshold:.2})");
                    }
                    advisor::CognitiveEvent::InnerVoiceThought { summary } => {
                        log::debug!("sage: inner voice — {summary}");
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// Async Generation Worker
// ═══════════════════════════════════════════════════════════════════

/// Spawn a long-lived async task that processes generation requests.
///
/// Receives GenerationContext from the pulse thread, selects a provider,
/// makes HTTP calls (with tool-use loop), and sends results back.
fn spawn_generation_worker(
    work_rx: mpsc::Receiver<advisor::GenerationContext>,
    result_tx: mpsc::Sender<advisor::GenerationResult>,
    engine: Arc<Mutex<EngineSplit>>,
    state: Arc<DaemonState>,
) {
    std::thread::Builder::new()
        .name("sage-generation".into())
        .spawn(move || {
            // Get the tokio runtime from Omnibus
            let rt = state.omnibus.omnibus().runtime().clone();

            while let Ok(context) = work_rx.recv() {
                let result_tx = result_tx.clone();
                let engine = engine.clone();
                let state_clone = state.clone();

                rt.spawn(async move {
                    let result = execute_generation(context, &engine, &state_clone).await;

                    // Record token usage
                    if let Some(tokens) = result.tokens_used {
                        let mut eng = engine.lock().unwrap();
                        eng.token_usage.record(tokens);
                    }

                    state_clone.email.send_raw("sage.generation.completed",
                        &serde_json::to_vec(&json!({
                            "provider": &result.provider_id,
                            "tokens": result.tokens_used,
                            "finish_reason": format!("{:?}", result.finish_reason),
                        })).unwrap_or_default());

                    if let Err(e) = result_tx.send(result) {
                        log::warn!("sage: result channel closed: {e}");
                    }
                });
            }
            log::info!("sage: generation worker shutting down");
        })
        .expect("sage: failed to spawn generation worker");
}

/// Execute a generation request against the selected provider.
async fn execute_generation(
    context: advisor::GenerationContext,
    engine: &Arc<Mutex<EngineSplit>>,
    _state: &Arc<DaemonState>,
) -> advisor::GenerationResult {
    // Select provider and extract info (drop lock before HTTP call)
    let (provider_id, model, _is_cloud, skills) = {
        let eng = engine.lock().unwrap();

        // Check token budget
        let mut usage = eng.token_usage.clone();
        if usage.budget_remaining() == 0 {
            // Try to select a local provider instead
            match eng.router.select(advisor::ProviderCapabilities::OFFLINE_CAPABLE) {
                Ok(p) => {
                    let id = p.id().to_string();
                    let model = id.clone();
                    (id, model, false, Vec::new())
                }
                Err(_) => {
                    return advisor::GenerationResult {
                        content: "Token budget exhausted and no local provider available.".into(),
                        tokens_used: None,
                        finish_reason: advisor::FinishReason::Error,
                        provider_id: "budget".into(),
                    };
                }
            }
        } else {
            match eng.router.select(advisor::ProviderCapabilities::empty()) {
                Ok(provider) => {
                    let id = provider.id().to_string();
                    let _cloud = provider.is_cloud();
                    let model = id.clone();
                    let skills: Vec<advisor::SkillDefinition> = eng.skill_registry.all()
                        .into_iter().cloned().collect();
                    (id, model, _cloud, skills)
                }
                Err(e) => {
                    return advisor::GenerationResult {
                        content: format!("No provider available: {e}"),
                        tokens_used: None,
                        finish_reason: advisor::FinishReason::Error,
                        provider_id: "none".into(),
                    };
                }
            }
        }
    };

    // Build and send HTTP request
    if provider_id.as_str() == "anthropic.claude" {
        execute_claude_generation(&context, &model, &skills).await
    } else {
        // OpenAI-compatible endpoint (Ollama, LM Studio)
        execute_openai_compatible_generation(&context, &provider_id).await
    }
}

/// Execute a generation against the Claude API.
async fn execute_claude_generation(
    context: &advisor::GenerationContext,
    model: &str,
    skills: &[advisor::SkillDefinition],
) -> advisor::GenerationResult {
    let request = context.to_claude_request(model, skills);

    let client = reqwest::Client::new();
    // TODO: Read API key from Vault. For now, check ANTHROPIC_API_KEY env var.
    let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();

    if api_key.is_empty() {
        return advisor::GenerationResult {
            content: "Claude API key not configured. Set ANTHROPIC_API_KEY or use advisor.set_api_key.".into(),
            tokens_used: None,
            finish_reason: advisor::FinishReason::Error,
            provider_id: "anthropic.claude".into(),
        };
    }

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&request)
        .send()
        .await;

    match response {
        Ok(resp) => {
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return advisor::GenerationResult {
                    content: format!("Claude API error ({status}): {body}"),
                    tokens_used: None,
                    finish_reason: advisor::FinishReason::Error,
                    provider_id: "anthropic.claude".into(),
                };
            }

            match resp.json::<advisor::ClaudeResponse>().await {
                Ok(claude_resp) => claude_resp.to_generation_result("anthropic.claude"),
                Err(e) => advisor::GenerationResult {
                    content: format!("Failed to parse Claude response: {e}"),
                    tokens_used: None,
                    finish_reason: advisor::FinishReason::Error,
                    provider_id: "anthropic.claude".into(),
                },
            }
        }
        Err(e) => advisor::GenerationResult {
            content: format!("HTTP request failed: {e}"),
            tokens_used: None,
            finish_reason: advisor::FinishReason::Error,
            provider_id: "anthropic.claude".into(),
        },
    }
}

/// Execute a generation against an OpenAI-compatible endpoint (Ollama, LM Studio).
async fn execute_openai_compatible_generation(
    context: &advisor::GenerationContext,
    provider_id: &str,
) -> advisor::GenerationResult {
    // Determine endpoint URL based on provider
    let base_url = match provider_id {
        "local" => std::env::var("OLLAMA_URL")
            .unwrap_or_else(|_| "http://localhost:11434".into()),
        _ => std::env::var("LM_STUDIO_URL")
            .unwrap_or_else(|_| "http://localhost:1234".into()),
    };

    let url = format!("{base_url}/v1/chat/completions");

    // Build OpenAI-compatible request
    let messages: Vec<Value> = context.conversation_history.iter()
        .map(|m| json!({
            "role": match m.role {
                advisor::MessageRole::System => "system",
                advisor::MessageRole::User => "user",
                advisor::MessageRole::Assistant => "assistant",
            },
            "content": m.content,
        }))
        .collect();

    let mut body = json!({
        "messages": messages,
        "temperature": context.temperature,
    });

    if let Some(max) = context.max_tokens {
        body["max_tokens"] = json!(max);
    }

    if let Some(system) = &context.system_prompt {
        // Prepend system message if not in history
        if let Some(msgs) = body["messages"].as_array_mut() {
            msgs.insert(0, json!({ "role": "system", "content": system }));
        }
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await;

    match response {
        Ok(resp) => {
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return advisor::GenerationResult {
                    content: format!("Local API error ({status}): {body}"),
                    tokens_used: None,
                    finish_reason: advisor::FinishReason::Error,
                    provider_id: provider_id.into(),
                };
            }

            // Parse OpenAI-compatible response
            match resp.json::<Value>().await {
                Ok(json) => {
                    let content = json["choices"][0]["message"]["content"]
                        .as_str()
                        .unwrap_or("")
                        .to_string();
                    let tokens = json["usage"]["total_tokens"]
                        .as_u64()
                        .map(|t| t as usize);
                    let finish = match json["choices"][0]["finish_reason"].as_str() {
                        Some("stop") => advisor::FinishReason::Complete,
                        Some("length") => advisor::FinishReason::MaxTokens,
                        Some("tool_calls") => advisor::FinishReason::ToolCall,
                        _ => advisor::FinishReason::Complete,
                    };

                    advisor::GenerationResult {
                        content,
                        tokens_used: tokens,
                        finish_reason: finish,
                        provider_id: provider_id.into(),
                    }
                }
                Err(e) => advisor::GenerationResult {
                    content: format!("Failed to parse response: {e}"),
                    tokens_used: None,
                    finish_reason: advisor::FinishReason::Error,
                    provider_id: provider_id.into(),
                },
            }
        }
        Err(e) => advisor::GenerationResult {
            content: format!("HTTP request failed: {e}"),
            tokens_used: None,
            finish_reason: advisor::FinishReason::Error,
            provider_id: provider_id.into(),
        },
    }
}
