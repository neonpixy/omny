//! Magistrate module — Kingdom governance courtier.
//!
//! Exposes governance primitives as daemon operations: charter creation,
//! proposal lifecycle, vote tallying, decision processes, federation
//! checks, community structures, challenge filing, and diplomatic channels.
//!
//! Kingdom is pure data structures and logic — no persistent state, no async.
//! All operations construct, validate, or query governance objects.

use std::sync::Arc;

use equipment::{CallDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct MagistrateModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

impl DaemonModule for MagistrateModule {
    fn id(&self) -> &str { "magistrate" }
    fn name(&self) -> &str { "Magistrate (Kingdom)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ═══════════════════════════════════════════════════════════
        // Charter operations
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.create_charter ────────────────────────────────
        // Create a new charter for a community.
        state.phone.register_raw("magistrate.create_charter", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let community_id_str = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.create_charter", "missing 'community_id'"))?;
            let community_id = uuid::Uuid::parse_str(community_id_str)
                .map_err(|e| err("magistrate.create_charter", format!("invalid UUID: {e}")))?;
            let name = params.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.create_charter", "missing 'name'"))?;
            let purpose = params.get("purpose").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.create_charter", "missing 'purpose'"))?;

            let charter = kingdom::Charter::new(community_id, name, purpose);
            let charter_json = serde_json::to_value(&charter)
                .map_err(|e| err("magistrate.create_charter", e))?;
            ok_json(&charter_json)
        });

        // ── kingdom.validate_charter ──────────────────────────────
        // Validate a charter's structure (deserialize and check).
        state.phone.register_raw("magistrate.validate_charter", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let charter_json = params.get("charter")
                .ok_or_else(|| err("magistrate.validate_charter", "missing 'charter'"))?;
            let _charter: kingdom::Charter = serde_json::from_value(charter_json.clone())
                .map_err(|e| err("magistrate.validate_charter", format!("invalid charter: {e}")))?;
            ok_json(&json!({ "valid": true }))
        });

        // ═══════════════════════════════════════════════════════════
        // Proposal operations
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.create_proposal ───────────────────────────────
        // Create a new proposal.
        state.phone.register_raw("magistrate.create_proposal", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let proposal_json = params.get("proposal")
                .ok_or_else(|| err("magistrate.create_proposal", "missing 'proposal'"))?;
            let proposal: kingdom::Proposal = serde_json::from_value(proposal_json.clone())
                .map_err(|e| err("magistrate.create_proposal", format!("invalid proposal: {e}")))?;
            let result = serde_json::to_value(&proposal)
                .map_err(|e| err("magistrate.create_proposal", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Vote & Decision operations
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.cast_vote ─────────────────────────────────────
        // Create a vote on a proposal.
        state.phone.register_raw("magistrate.cast_vote", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let voter = params.get("voter").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.cast_vote", "missing 'voter'"))?;
            let proposal_id_str = params.get("proposal_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.cast_vote", "missing 'proposal_id'"))?;
            let proposal_id = uuid::Uuid::parse_str(proposal_id_str)
                .map_err(|e| err("magistrate.cast_vote", format!("invalid UUID: {e}")))?;
            let position_json = params.get("position")
                .ok_or_else(|| err("magistrate.cast_vote", "missing 'position'"))?;
            let position: kingdom::VotePosition = serde_json::from_value(position_json.clone())
                .map_err(|e| err("magistrate.cast_vote", format!("invalid position: {e}")))?;

            let mut vote = kingdom::Vote::new(voter, proposal_id, position);
            if let Some(reason) = params.get("reason").and_then(|v| v.as_str()) {
                vote = vote.with_reason(reason);
            }
            if let Some(weight) = params.get("weight").and_then(|v| v.as_f64()) {
                vote = vote.with_weight(weight);
            }

            let vote_json = serde_json::to_value(&vote)
                .map_err(|e| err("magistrate.cast_vote", e))?;
            ok_json(&vote_json)
        });

        // ── kingdom.tally_votes ───────────────────────────────────
        // Tally votes using a decision process.
        state.phone.register_raw("magistrate.tally_votes", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let votes_json = params.get("votes")
                .ok_or_else(|| err("magistrate.tally_votes", "missing 'votes'"))?;
            let votes: Vec<kingdom::Vote> = serde_json::from_value(votes_json.clone())
                .map_err(|e| err("magistrate.tally_votes", format!("invalid votes: {e}")))?;
            let eligible = params.get("eligible_voters").and_then(|v| v.as_u64())
                .ok_or_else(|| err("magistrate.tally_votes", "missing 'eligible_voters'"))? as u32;
            let process_id = params.get("process").and_then(|v| v.as_str()).unwrap_or("direct_vote");

            let process: Box<dyn kingdom::DecisionProcess> = match process_id {
                "consensus" => Box::new(kingdom::ConsensusProcess),
                "consent" => Box::new(kingdom::ConsentProcess),
                "super_majority" => {
                    let threshold = params.get("threshold").and_then(|v| v.as_f64()).unwrap_or(0.67);
                    Box::new(kingdom::SuperMajorityProcess { threshold })
                }
                "ranked_choice" => Box::new(kingdom::RankedChoiceProcess),
                _ => Box::new(kingdom::DirectVoteProcess),
            };

            let tally = process.tally(&votes, eligible);
            let tally_json = serde_json::to_value(&tally)
                .map_err(|e| err("magistrate.tally_votes", e))?;
            ok_json(&tally_json)
        });

        // ═══════════════════════════════════════════════════════════
        // Federation operations
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.check_subsidiarity ────────────────────────────
        // Check whether a decision belongs at the proper governance level.
        state.phone.register_raw("magistrate.check_subsidiarity", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let check_json = params.get("check")
                .ok_or_else(|| err("magistrate.check_subsidiarity", "missing 'check'"))?;
            let check: kingdom::SubsidiarityCheck = serde_json::from_value(check_json.clone())
                .map_err(|e| err("magistrate.check_subsidiarity", format!("invalid check: {e}")))?;
            let result = serde_json::to_value(&check)
                .map_err(|e| err("magistrate.check_subsidiarity", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Challenge operations
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.create_challenge ──────────────────────────────
        // Create a governance challenge.
        state.phone.register_raw("magistrate.create_challenge", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let challenge_json = params.get("challenge")
                .ok_or_else(|| err("magistrate.create_challenge", "missing 'challenge'"))?;
            let challenge: kingdom::Challenge = serde_json::from_value(challenge_json.clone())
                .map_err(|e| err("magistrate.create_challenge", format!("invalid challenge: {e}")))?;
            let result = serde_json::to_value(&challenge)
                .map_err(|e| err("magistrate.create_challenge", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Community operations
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.create_community ──────────────────────────────
        // Create a new community object.
        state.phone.register_raw("magistrate.create_community", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let community_json = params.get("community")
                .ok_or_else(|| err("magistrate.create_community", "missing 'community'"))?;
            let community: kingdom::Community = serde_json::from_value(community_json.clone())
                .map_err(|e| err("magistrate.create_community", format!("invalid community: {e}")))?;
            let result = serde_json::to_value(&community)
                .map_err(|e| err("magistrate.create_community", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Exit with Dignity
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.reject_exit_penalty ───────────────────────────
        // Check charter text for illegal exit penalties.
        state.phone.register_raw("magistrate.reject_exit_penalty", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let clause = params.get("clause").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.reject_exit_penalty", "missing 'clause'"))?;
            let result = kingdom::reject_exit_penalty_clause(clause);
            ok_json(&json!({ "rejected": result.is_err(), "reason": result.err().map(|e| e.to_string()) }))
        });

        // ═══════════════════════════════════════════════════════════
        // Affected-Party Consent
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.check_affected_party ──────────────────────────
        // Evaluate affected-party consent constraints on a proposal.
        state.phone.register_raw("magistrate.check_affected_party", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let constraints_json = params.get("constraints")
                .ok_or_else(|| err("magistrate.check_affected_party", "missing 'constraints'"))?;
            let constraints: Vec<kingdom::ProposalConstraint> = serde_json::from_value(constraints_json.clone())
                .map_err(|e| err("magistrate.check_affected_party", format!("invalid constraints: {e}")))?;
            let votes_json = params.get("votes")
                .ok_or_else(|| err("magistrate.check_affected_party", "missing 'votes'"))?;
            let votes: Vec<kingdom::AffectedPartyVote> = serde_json::from_value(votes_json.clone())
                .map_err(|e| err("magistrate.check_affected_party", format!("invalid votes: {e}")))?;

            let result = kingdom::evaluate_affected_party_constraints(&constraints, &votes);
            ok_json(&json!({ "permitted": result.is_ok(), "reason": result.err().map(|e| e.to_string()) }))
        });

        // ═══════════════════════════════════════════════════════════
        // Deliberation minimum
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.check_deliberation ────────────────────────────
        // Check whether minimum deliberation time has passed.
        state.phone.register_raw("magistrate.check_deliberation", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let minimum_secs = params.get("minimum_secs").and_then(|v| v.as_u64())
                .ok_or_else(|| err("magistrate.check_deliberation", "missing 'minimum_secs'"))?;
            let started_str = params.get("started_at").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.check_deliberation", "missing 'started_at'"))?;
            let started_at: chrono::DateTime<chrono::Utc> = started_str.parse()
                .map_err(|e| err("magistrate.check_deliberation", format!("invalid timestamp: {e}")))?;

            let constraints = vec![kingdom::ProposalConstraint::DeliberationMinimum(minimum_secs)];
            let result = kingdom::check_deliberation_minimum(&constraints, started_at);
            ok_json(&json!({ "met": result.is_ok(), "reason": result.err().map(|e| e.to_string()) }))
        });

        // ═══════════════════════════════════════════════════════════
        // Diplomacy
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.create_treaty ─────────────────────────────────
        // Create a new treaty between communities.
        state.phone.register_raw("magistrate.create_treaty", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let treaty_json = params.get("treaty")
                .ok_or_else(|| err("magistrate.create_treaty", "missing 'treaty'"))?;
            let treaty: kingdom::Treaty = serde_json::from_value(treaty_json.clone())
                .map_err(|e| err("magistrate.create_treaty", format!("invalid treaty: {e}")))?;
            let result = serde_json::to_value(&treaty)
                .map_err(|e| err("magistrate.create_treaty", e))?;
            ok_json(&result)
        });

        // ── kingdom.create_channel ────────────────────────────────
        // Create a diplomatic channel.
        let _s = state.clone();
        state.phone.register_raw("magistrate.create_channel", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let channel_json = params.get("channel")
                .ok_or_else(|| err("magistrate.create_channel", "missing 'channel'"))?;
            let channel: kingdom::DiplomaticChannel = serde_json::from_value(channel_json.clone())
                .map_err(|e| err("magistrate.create_channel", format!("invalid channel: {e}")))?;
            let result = serde_json::to_value(&channel)
                .map_err(|e| err("magistrate.create_channel", e))?;
            ok_json(&result)
        });

        // ═══════════════════════════════════════════════════════════
        // Governance Health
        // ═══════════════════════════════════════════════════════════

        // ── kingdom.governance_budget ─────────────────────────────
        // Create a governance budget for a community.
        state.phone.register_raw("magistrate.governance_budget", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let community_id = params.get("community_id").and_then(|v| v.as_str())
                .ok_or_else(|| err("magistrate.governance_budget", "missing 'community_id'"))?;
            let budget = kingdom::GovernanceBudget::new(community_id);
            let budget_json = serde_json::to_value(&budget)
                .map_err(|e| err("magistrate.governance_budget", e))?;
            ok_json(&budget_json)
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Charter
            .with_call(CallDescriptor::new("magistrate.create_charter", "Create community charter"))
            .with_call(CallDescriptor::new("magistrate.validate_charter", "Validate charter structure"))
            // Proposals
            .with_call(CallDescriptor::new("magistrate.create_proposal", "Create governance proposal"))
            // Voting
            .with_call(CallDescriptor::new("magistrate.cast_vote", "Cast vote on proposal"))
            .with_call(CallDescriptor::new("magistrate.tally_votes", "Tally votes with decision process"))
            // Federation
            .with_call(CallDescriptor::new("magistrate.check_subsidiarity", "Check governance level"))
            // Challenges
            .with_call(CallDescriptor::new("magistrate.create_challenge", "File governance challenge"))
            // Community
            .with_call(CallDescriptor::new("magistrate.create_community", "Create community object"))
            // Exit
            .with_call(CallDescriptor::new("magistrate.reject_exit_penalty", "Check for exit penalties"))
            // Affected party
            .with_call(CallDescriptor::new("magistrate.check_affected_party", "Evaluate affected-party constraints"))
            .with_call(CallDescriptor::new("magistrate.check_deliberation", "Check deliberation minimum"))
            // Diplomacy
            .with_call(CallDescriptor::new("magistrate.create_treaty", "Create treaty"))
            .with_call(CallDescriptor::new("magistrate.create_channel", "Create diplomatic channel"))
            // Governance health
            .with_call(CallDescriptor::new("magistrate.governance_budget", "Create governance budget"))
    }
}
