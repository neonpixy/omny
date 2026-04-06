//! Treasurer module — Fortune economics courtier.
//!
//! Exposes the entire Fortune crate as daemon operations: treasury management,
//! balance/ledger queries, UBI distribution, demurrage decay, bearer cash,
//! exchange/escrow, flow-back redistribution, cooperative economics, commons
//! trusts, commerce (products, carts, storefronts, orders, checkout), and
//! financial pattern detection.
//!
//! All state lives in DaemonState. Programs call `fortune.*` operations through
//! the Phone; they never import Fortune directly. Keys and identity come from
//! Vault/Omnibus automatically.

use std::sync::Arc;

use equipment::{CallDescriptor, EventDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct TreasurerModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn guard_vault_unlocked(state: &DaemonState) -> Result<(), PhoneError> {
    let vault = state.vault.lock().unwrap();
    if !vault.is_unlocked() {
        return Err(err("treasurer", "Vault is locked — unlock identity first"));
    }
    Ok(())
}

/// Extract a required string field from JSON params.
fn req_str<'a>(params: &'a Value, field: &str, op: &str) -> Result<&'a str, PhoneError> {
    params.get(field).and_then(|v| v.as_str())
        .ok_or_else(|| err(op, format!("missing '{field}'")))
}

/// Extract a required i64 field from JSON params.
fn req_i64(params: &Value, field: &str, op: &str) -> Result<i64, PhoneError> {
    params.get(field).and_then(|v| v.as_i64())
        .ok_or_else(|| err(op, format!("missing '{field}'")))
}

/// Extract an optional i64 field from JSON params.
fn opt_i64(params: &Value, field: &str) -> Option<i64> {
    params.get(field).and_then(|v| v.as_i64())
}

/// Extract an optional u32 field from JSON params.
fn opt_u32(params: &Value, field: &str) -> Option<u32> {
    params.get(field).and_then(|v| v.as_u64()).map(|v| v as u32)
}

/// Extract an optional string field from JSON params.
fn opt_str(params: &Value, field: &str) -> Option<String> {
    params.get(field).and_then(|v| v.as_str()).map(|s| s.to_string())
}

impl DaemonModule for TreasurerModule {
    fn id(&self) -> &str { "treasurer" }
    fn name(&self) -> &str { "Treasurer (Fortune)" }
    fn deps(&self) -> &[&str] { &["castellan"] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  POLICY
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.policy.get ────────────────────────────────────
        // Returns the active FortunePolicy as JSON.
        state.phone.register_raw("treasurer.policy.get", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let preset = params.get("preset").and_then(|v| v.as_str()).unwrap_or("default");
            let policy = match preset {
                "testing" => fortune::FortunePolicy::testing(),
                "conservative" => fortune::FortunePolicy::conservative(),
                _ => fortune::FortunePolicy::default_policy(),
            };
            let policy_json = serde_json::to_value(&policy)
                .map_err(|e| err("treasurer.policy.get", e))?;
            ok_json(&policy_json)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  TREASURY
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.treasury.status ───────────────────────────────
        // Returns a TreasuryStatus snapshot: max supply, circulation, utilization.
        state.phone.register_raw("treasurer.treasury.status", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let policy = parse_policy(&params);
            let treasury = fortune::Treasury::new(policy);
            let status = treasury.status();
            let status_json = serde_json::to_value(&status)
                .map_err(|e| err("treasurer.treasury.status", e))?;
            ok_json(&status_json)
        });

        // ── fortune.treasury.max_supply ───────────────────────────
        // Calculates max supply from given network metrics.
        state.phone.register_raw("treasurer.treasury.max_supply", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let policy = parse_policy(&params);
            let mut treasury = fortune::Treasury::new(policy);
            let metrics = parse_metrics(&params);
            treasury.update_metrics(metrics);
            ok_json(&json!({
                "max_supply": treasury.max_supply(),
                "available": treasury.available()
            }))
        });

        // ── fortune.treasury.mint ─────────────────────────────────
        // Mint Cool into circulation. Returns actual amount minted.
        let s = state.clone();
        state.phone.register_raw("treasurer.treasury.mint", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let amount = req_i64(&params, "amount", "treasurer.treasury.mint")?;
            let recipient = req_str(&params, "recipient", "treasurer.treasury.mint")?;
            let reason = parse_mint_reason(&params);
            let policy = parse_policy(&params);
            let mut treasury = fortune::Treasury::new(policy);
            let metrics = parse_metrics(&params);
            treasury.update_metrics(metrics);

            let minted = treasury.mint(amount, recipient, reason)
                .map_err(|e| err("treasurer.treasury.mint", e))?;

            s.email.send_raw("treasurer.treasury.minted", &serde_json::to_vec(&json!({
                "amount": minted, "recipient": recipient
            })).unwrap_or_default());

            ok_json(&json!({ "minted": minted }))
        });

        // ── fortune.treasury.mint_exact ───────────────────────────
        // Mint exact amount or fail.
        let s = state.clone();
        state.phone.register_raw("treasurer.treasury.mint_exact", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let amount = req_i64(&params, "amount", "treasurer.treasury.mint_exact")?;
            let recipient = req_str(&params, "recipient", "treasurer.treasury.mint_exact")?;
            let reason = parse_mint_reason(&params);
            let policy = parse_policy(&params);
            let mut treasury = fortune::Treasury::new(policy);
            let metrics = parse_metrics(&params);
            treasury.update_metrics(metrics);

            treasury.mint_exact(amount, recipient, reason)
                .map_err(|e| err("treasurer.treasury.mint_exact", e))?;
            ok_json(&json!({ "ok": true, "amount": amount }))
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  BALANCE / LEDGER
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.balance.get ───────────────────────────────────
        // Look up a person's balance (liquid + locked).
        state.phone.register_raw("treasurer.balance.get", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.balance.get")?;
            let ledger = fortune::Ledger::new();
            let balance = ledger.balance(pubkey);
            ok_json(&json!({
                "liquid": balance.liquid,
                "locked": balance.locked,
                "total": balance.total()
            }))
        });

        // ── fortune.ledger.credit ─────────────────────────────────
        // Credit Cool to an account.
        let s = state.clone();
        state.phone.register_raw("treasurer.ledger.credit", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ledger.credit")?;
            let amount = req_i64(&params, "amount", "treasurer.ledger.credit")?;
            let reason = parse_transaction_reason(&params);
            let reference = opt_str(&params, "reference");
            let mut ledger = fortune::Ledger::new();

            ledger.credit(pubkey, amount, reason, reference);
            let balance = ledger.balance(pubkey);

            s.email.send_raw("treasurer.balance.changed", &serde_json::to_vec(&json!({
                "pubkey": pubkey, "liquid": balance.liquid, "locked": balance.locked
            })).unwrap_or_default());

            ok_json(&json!({
                "ok": true,
                "liquid": balance.liquid,
                "locked": balance.locked
            }))
        });

        // ── fortune.ledger.debit ──────────────────────────────────
        // Debit Cool from an account. Fails if insufficient.
        let s = state.clone();
        state.phone.register_raw("treasurer.ledger.debit", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ledger.debit")?;
            let amount = req_i64(&params, "amount", "treasurer.ledger.debit")?;
            let reason = parse_transaction_reason(&params);
            let reference = opt_str(&params, "reference");
            let mut ledger = fortune::Ledger::new();

            ledger.debit(pubkey, amount, reason, reference)
                .map_err(|e| err("treasurer.ledger.debit", e))?;
            let balance = ledger.balance(pubkey);

            s.email.send_raw("treasurer.balance.changed", &serde_json::to_vec(&json!({
                "pubkey": pubkey, "liquid": balance.liquid, "locked": balance.locked
            })).unwrap_or_default());

            ok_json(&json!({
                "ok": true,
                "liquid": balance.liquid,
                "locked": balance.locked
            }))
        });

        // ── fortune.ledger.transfer ───────────────────────────────
        // Atomic transfer between two accounts.
        let s = state.clone();
        state.phone.register_raw("treasurer.ledger.transfer", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let sender = req_str(&params, "sender", "treasurer.ledger.transfer")?;
            let recipient = req_str(&params, "recipient", "treasurer.ledger.transfer")?;
            let amount = req_i64(&params, "amount", "treasurer.ledger.transfer")?;
            let reference = opt_str(&params, "reference");
            let mut ledger = fortune::Ledger::new();

            ledger.transfer(sender, recipient, amount, reference)
                .map_err(|e| err("treasurer.ledger.transfer", e))?;

            s.email.send_raw("treasurer.transfer.completed", &serde_json::to_vec(&json!({
                "sender": sender, "recipient": recipient, "amount": amount
            })).unwrap_or_default());

            ok_json(&json!({ "ok": true }))
        });

        // ── fortune.ledger.transactions ───────────────────────────
        // Get transaction history for a pubkey.
        state.phone.register_raw("treasurer.ledger.transactions", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ledger.transactions")?;
            let ledger = fortune::Ledger::new();
            let txns = ledger.transactions_for(pubkey);
            let txns_json: Vec<Value> = txns.iter().map(|t| {
                serde_json::to_value(t).unwrap_or(Value::Null)
            }).collect();
            ok_json(&json!({ "transactions": txns_json }))
        });

        // ── fortune.ledger.summary ────────────────────────────────
        // Get aggregated transaction stats for a pubkey.
        state.phone.register_raw("treasurer.ledger.summary", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ledger.summary")?;
            let ledger = fortune::Ledger::new();
            let summary = ledger.summary(pubkey);
            let summary_json = serde_json::to_value(&summary)
                .map_err(|e| err("treasurer.ledger.summary", e))?;
            ok_json(&summary_json)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  UBI
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.ubi.check_eligibility ─────────────────────────
        // Check whether a pubkey is eligible for UBI.
        state.phone.register_raw("treasurer.ubi.check_eligibility", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ubi.check_eligibility")?;
            let policy = parse_policy(&params);
            let ubi = fortune::UbiDistributor::new();
            let ledger = fortune::Ledger::new();
            let treasury = fortune::Treasury::new(policy.clone());
            let eligibility = ubi.check_eligibility(pubkey, &ledger, &treasury, &policy);
            let elig_json = serde_json::to_value(&eligibility)
                .map_err(|e| err("treasurer.ubi.check_eligibility", e))?;
            ok_json(&elig_json)
        });

        // ── fortune.ubi.claim ─────────────────────────────────────
        // Claim UBI for a pubkey. Mints from treasury, credits to ledger.
        let s = state.clone();
        state.phone.register_raw("treasurer.ubi.claim", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ubi.claim")?;
            let policy = parse_policy(&params);
            let mut ubi = fortune::UbiDistributor::new();
            let mut ledger = fortune::Ledger::new();
            let mut treasury = fortune::Treasury::new(policy.clone());
            let metrics = parse_metrics(&params);
            treasury.update_metrics(metrics);

            // Verify identity before claiming
            ubi.verify_identity(pubkey);

            let claim = ubi.claim(pubkey, &mut ledger, &mut treasury, &policy)
                .map_err(|e| err("treasurer.ubi.claim", e))?;
            let claim_json = serde_json::to_value(&claim)
                .map_err(|e| err("treasurer.ubi.claim", e))?;

            s.email.send_raw("treasurer.ubi.claimed", &serde_json::to_vec(&json!({
                "pubkey": pubkey, "amount": claim.amount
            })).unwrap_or_default());

            ok_json(&claim_json)
        });

        // ── fortune.ubi.verify_identity ───────────────────────────
        // Register a verified identity (eligible for UBI).
        state.phone.register_raw("treasurer.ubi.verify_identity", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ubi.verify_identity")?;
            let mut ubi = fortune::UbiDistributor::new();
            ubi.verify_identity(pubkey);
            ok_json(&json!({ "ok": true }))
        });

        // ── fortune.ubi.flag_account ──────────────────────────────
        // Flag an account (temporarily ineligible for UBI).
        state.phone.register_raw("treasurer.ubi.flag_account", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ubi.flag_account")?;
            let mut ubi = fortune::UbiDistributor::new();
            ubi.flag_account(pubkey);
            ok_json(&json!({ "ok": true }))
        });

        // ── fortune.ubi.unflag_account ────────────────────────────
        // Unflag an account (restore UBI eligibility).
        state.phone.register_raw("treasurer.ubi.unflag_account", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.ubi.unflag_account")?;
            let mut ubi = fortune::UbiDistributor::new();
            ubi.unflag_account(pubkey);
            ok_json(&json!({ "ok": true }))
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  DEMURRAGE
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.demurrage.calculate ───────────────────────────
        // Calculate decay for a given balance over N days.
        state.phone.register_raw("treasurer.demurrage.calculate", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let balance = req_i64(&params, "balance", "treasurer.demurrage.calculate")?;
            let days = params.get("days").and_then(|v| v.as_u64()).unwrap_or(30) as u32;
            let policy = parse_policy(&params);
            let engine = fortune::DemurrageEngine::new();
            let decay = engine.calculate_decay(balance, days, &policy);
            ok_json(&json!({
                "decay": decay,
                "balance_after": balance - decay,
                "days": days,
                "rate": policy.demurrage_rate
            }))
        });

        // ── fortune.demurrage.preview ─────────────────────────────
        // Preview what demurrage would look like for an account.
        state.phone.register_raw("treasurer.demurrage.preview", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let pubkey = req_str(&params, "pubkey", "treasurer.demurrage.preview")?;
            let policy = parse_policy(&params);
            let engine = fortune::DemurrageEngine::new();
            let ledger = fortune::Ledger::new();
            let preview = engine.preview(pubkey, &ledger, &policy);
            let preview_json = serde_json::to_value(&preview)
                .map_err(|e| err("treasurer.demurrage.preview", e))?;
            ok_json(&preview_json)
        });

        // ── fortune.demurrage.run_cycle ───────────────────────────
        // Run a full demurrage cycle across all accounts.
        let s = state.clone();
        state.phone.register_raw("treasurer.demurrage.run_cycle", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let policy = parse_policy(&params);
            let mut engine = fortune::DemurrageEngine::new();
            let mut ledger = fortune::Ledger::new();
            let mut treasury = fortune::Treasury::new(policy.clone());
            let metrics = parse_metrics(&params);
            treasury.update_metrics(metrics);

            let result = engine.run_cycle(&mut ledger, &mut treasury, &policy);
            let result_json = serde_json::to_value(&result)
                .map_err(|e| err("treasurer.demurrage.run_cycle", e))?;

            s.email.send_raw("treasurer.demurrage.cycle_complete", &serde_json::to_vec(&json!({
                "cycle_number": result.cycle_number,
                "total_decayed": result.total_decayed,
                "accounts_processed": result.accounts_processed
            })).unwrap_or_default());

            ok_json(&result_json)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  CASH (Bearer Notes)
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.cash.issue ────────────────────────────────────
        // Issue a bearer cash note. Locks Cool in the issuer's balance.
        let s = state.clone();
        state.phone.register_raw("treasurer.cash.issue", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let issuer = req_str(&params, "issuer", "treasurer.cash.issue")?;
            let amount = req_i64(&params, "amount", "treasurer.cash.issue")?;
            let memo = opt_str(&params, "memo");
            let expiry_days = opt_u32(&params, "expiry_days");
            let policy = parse_policy(&params);
            let mut mint = fortune::CashMint::new();
            let mut ledger = fortune::Ledger::new();
            let mut treasury = fortune::Treasury::new(policy.clone());
            let metrics = parse_metrics(&params);
            treasury.update_metrics(metrics);
            let mut registry = fortune::CashRegistry::new();

            // Ensure issuer has balance for locking
            let initial_balance = req_i64(&params, "issuer_balance", "treasurer.cash.issue")
                .unwrap_or(0);
            if initial_balance > 0 {
                ledger.credit(issuer, initial_balance, fortune::TransactionReason::Initial, None);
            }

            let note = mint.issue(
                issuer, amount, memo, expiry_days,
                &mut ledger, &mut treasury, &mut registry, &policy,
            ).map_err(|e| err("treasurer.cash.issue", e))?;

            let note_json = serde_json::to_value(&note)
                .map_err(|e| err("treasurer.cash.issue", e))?;

            s.email.send_raw("treasurer.cash.issued", &serde_json::to_vec(&json!({
                "serial": note.serial, "issuer": issuer, "amount": amount
            })).unwrap_or_default());

            ok_json(&note_json)
        });

        // ── fortune.cash.redeem ───────────────────────────────────
        // Redeem a bearer cash note. Transfers Cool to the redeemer.
        let s = state.clone();
        state.phone.register_raw("treasurer.cash.redeem", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let serial = req_str(&params, "serial", "treasurer.cash.redeem")?;
            let redeemer = req_str(&params, "redeemer", "treasurer.cash.redeem")?;
            let mut redemption = fortune::CashRedemption::new();
            let mut ledger = fortune::Ledger::new();
            let mut treasury = fortune::Treasury::new(fortune::FortunePolicy::default_policy());
            let mut registry = fortune::CashRegistry::new();

            let result = redemption.redeem(
                serial, redeemer,
                &mut ledger, &mut treasury, &mut registry,
            ).map_err(|e| err("treasurer.cash.redeem", e))?;

            let result_json = serde_json::to_value(&result)
                .map_err(|e| err("treasurer.cash.redeem", e))?;

            s.email.send_raw("treasurer.cash.redeemed", &serde_json::to_vec(&json!({
                "serial": serial, "redeemer": redeemer, "amount": result.amount
            })).unwrap_or_default());

            ok_json(&result_json)
        });

        // ── fortune.cash.revoke ───────────────────────────────────
        // Revoke an active cash note, returning Cool to issuer.
        let s = state.clone();
        state.phone.register_raw("treasurer.cash.revoke", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let serial = req_str(&params, "serial", "treasurer.cash.revoke")?;
            let reason = req_str(&params, "reason", "treasurer.cash.revoke")?;
            let mut mint = fortune::CashMint::new();
            let mut ledger = fortune::Ledger::new();
            let mut treasury = fortune::Treasury::new(fortune::FortunePolicy::default_policy());
            let mut registry = fortune::CashRegistry::new();

            mint.revoke(serial, reason, &mut ledger, &mut treasury, &mut registry)
                .map_err(|e| err("treasurer.cash.revoke", e))?;

            s.email.send_raw("treasurer.cash.revoked", &serde_json::to_vec(&json!({
                "serial": serial, "reason": reason
            })).unwrap_or_default());

            ok_json(&json!({ "ok": true }))
        });

        // ── fortune.cash.validate_serial ──────────────────────────
        // Validate a XXXX-XXXX-XXXX serial format.
        state.phone.register_raw("treasurer.cash.validate_serial", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let serial = req_str(&params, "serial", "treasurer.cash.validate_serial")?;
            let valid = fortune::cash::note::validate_serial(serial);
            ok_json(&json!({ "valid": valid }))
        });

        // ── fortune.cash.normalize_serial ─────────────────────────
        // Normalize serial input (uppercase, add dashes).
        state.phone.register_raw("treasurer.cash.normalize_serial", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let input = req_str(&params, "input", "treasurer.cash.normalize_serial")?;
            let normalized = fortune::cash::note::normalize_serial(input);
            ok_json(&json!({ "serial": normalized }))
        });

        // ── fortune.cash.generate_serial ──────────────────────────
        // Generate a new random serial.
        state.phone.register_raw("treasurer.cash.generate_serial", move |_data| {
            let serial = fortune::cash::note::generate_serial();
            ok_json(&json!({ "serial": serial }))
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  EXCHANGE / ESCROW
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.exchange.propose ──────────────────────────────
        // Create a new trade proposal between two parties.
        state.phone.register_raw("treasurer.exchange.propose", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let proposer = req_str(&params, "proposer", "treasurer.exchange.propose")?;
            let recipient = req_str(&params, "recipient", "treasurer.exchange.propose")?;
            let offering_cool = req_i64(&params, "offering_cool", "treasurer.exchange.propose")?;
            let requesting_cool = opt_i64(&params, "requesting_cool").unwrap_or(0);
            let message = opt_str(&params, "message");

            let offering_items: Vec<String> = params.get("offering_items")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let requesting_items: Vec<String> = params.get("requesting_items")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut trade = fortune::TradeProposal::new(
                proposer, recipient, offering_cool, requesting_cool,
            ).map_err(|e| err("treasurer.exchange.propose", e))?;

            if !offering_items.is_empty() || !requesting_items.is_empty() {
                trade = trade.with_items(offering_items, requesting_items);
            }
            if let Some(msg) = message {
                trade = trade.with_message(msg);
            }

            let trade_json = serde_json::to_value(&trade)
                .map_err(|e| err("treasurer.exchange.propose", e))?;
            ok_json(&trade_json)
        });

        // ── fortune.exchange.accept ───────────────────────────────
        // Accept a trade proposal (advances to Accepted status).
        state.phone.register_raw("treasurer.exchange.accept", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trade_json = params.get("trade")
                .ok_or_else(|| err("treasurer.exchange.accept", "missing 'trade'"))?;
            let mut trade: fortune::TradeProposal = serde_json::from_value(trade_json.clone())
                .map_err(|e| err("treasurer.exchange.accept", e))?;
            trade.accept()
                .map_err(|e| err("treasurer.exchange.accept", e))?;
            let result = serde_json::to_value(&trade)
                .map_err(|e| err("treasurer.exchange.accept", e))?;
            ok_json(&result)
        });

        // ── fortune.exchange.execute ──────────────────────────────
        // Execute an accepted trade (marks as Executed).
        let s = state.clone();
        state.phone.register_raw("treasurer.exchange.execute", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trade_json = params.get("trade")
                .ok_or_else(|| err("treasurer.exchange.execute", "missing 'trade'"))?;
            let mut trade: fortune::TradeProposal = serde_json::from_value(trade_json.clone())
                .map_err(|e| err("treasurer.exchange.execute", e))?;
            trade.execute()
                .map_err(|e| err("treasurer.exchange.execute", e))?;

            s.email.send_raw("treasurer.exchange.executed", &serde_json::to_vec(&json!({
                "trade_id": trade.id.to_string()
            })).unwrap_or_default());

            let result = serde_json::to_value(&trade)
                .map_err(|e| err("treasurer.exchange.execute", e))?;
            ok_json(&result)
        });

        // ── fortune.exchange.cancel ───────────────────────────────
        // Cancel a trade proposal.
        state.phone.register_raw("treasurer.exchange.cancel", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trade_json = params.get("trade")
                .ok_or_else(|| err("treasurer.exchange.cancel", "missing 'trade'"))?;
            let mut trade: fortune::TradeProposal = serde_json::from_value(trade_json.clone())
                .map_err(|e| err("treasurer.exchange.cancel", e))?;
            trade.cancel();
            let result = serde_json::to_value(&trade)
                .map_err(|e| err("treasurer.exchange.cancel", e))?;
            ok_json(&result)
        });

        // ── fortune.exchange.reject ───────────────────────────────
        // Reject a trade proposal.
        state.phone.register_raw("treasurer.exchange.reject", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trade_json = params.get("trade")
                .ok_or_else(|| err("treasurer.exchange.reject", "missing 'trade'"))?;
            let mut trade: fortune::TradeProposal = serde_json::from_value(trade_json.clone())
                .map_err(|e| err("treasurer.exchange.reject", e))?;
            trade.reject();
            let result = serde_json::to_value(&trade)
                .map_err(|e| err("treasurer.exchange.reject", e))?;
            ok_json(&result)
        });

        // ── fortune.escrow.create ─────────────────────────────────
        // Create an escrow between client and provider.
        state.phone.register_raw("treasurer.escrow.create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let client = req_str(&params, "client", "treasurer.escrow.create")?;
            let provider = req_str(&params, "provider", "treasurer.escrow.create")?;
            let amount = req_i64(&params, "amount", "treasurer.escrow.create")?;

            let conditions: Vec<fortune::ReleaseCondition> = params.get("conditions")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let escrow = fortune::EscrowRecord::new(client, provider, amount)
                .with_conditions(conditions);

            let escrow_json = serde_json::to_value(&escrow)
                .map_err(|e| err("treasurer.escrow.create", e))?;
            ok_json(&escrow_json)
        });

        // ── fortune.escrow.release ────────────────────────────────
        // Release escrowed funds to the provider.
        let s = state.clone();
        state.phone.register_raw("treasurer.escrow.release", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let escrow_json = params.get("escrow")
                .ok_or_else(|| err("treasurer.escrow.release", "missing 'escrow'"))?;
            let mut escrow: fortune::EscrowRecord = serde_json::from_value(escrow_json.clone())
                .map_err(|e| err("treasurer.escrow.release", e))?;
            escrow.release()
                .map_err(|e| err("treasurer.escrow.release", e))?;

            s.email.send_raw("treasurer.escrow.released", &serde_json::to_vec(&json!({
                "escrow_id": escrow.id.to_string(), "amount": escrow.amount
            })).unwrap_or_default());

            let result = serde_json::to_value(&escrow)
                .map_err(|e| err("treasurer.escrow.release", e))?;
            ok_json(&result)
        });

        // ── fortune.escrow.refund ─────────────────────────────────
        // Refund escrowed funds to the client.
        let s = state.clone();
        state.phone.register_raw("treasurer.escrow.refund", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let escrow_json = params.get("escrow")
                .ok_or_else(|| err("treasurer.escrow.refund", "missing 'escrow'"))?;
            let mut escrow: fortune::EscrowRecord = serde_json::from_value(escrow_json.clone())
                .map_err(|e| err("treasurer.escrow.refund", e))?;
            escrow.refund();

            s.email.send_raw("treasurer.escrow.refunded", &serde_json::to_vec(&json!({
                "escrow_id": escrow.id.to_string(), "amount": escrow.amount
            })).unwrap_or_default());

            let result = serde_json::to_value(&escrow)
                .map_err(|e| err("treasurer.escrow.refund", e))?;
            ok_json(&result)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  FLOW-BACK (Progressive Redistribution)
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.flowback.calculate ────────────────────────────
        // Calculate flow-back amount for a given balance.
        state.phone.register_raw("treasurer.flowback.calculate", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let balance = req_i64(&params, "balance", "treasurer.flowback.calculate")?;
            let policy = parse_policy(&params);
            let fb = fortune::FlowBack::new();
            let amount = fb.calculate(balance, &policy.flow_back_tiers);
            ok_json(&json!({
                "flow_back_amount": amount,
                "balance_after": balance - amount
            }))
        });

        // ── fortune.flowback.preview ──────────────────────────────
        // Preview flow-back for a given balance (includes effective rate).
        state.phone.register_raw("treasurer.flowback.preview", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let balance = req_i64(&params, "balance", "treasurer.flowback.preview")?;
            let policy = parse_policy(&params);
            let fb = fortune::FlowBack::new();
            let preview = fb.preview(balance, &policy.flow_back_tiers);
            let preview_json = serde_json::to_value(&preview)
                .map_err(|e| err("treasurer.flowback.preview", e))?;
            ok_json(&preview_json)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  COOPERATIVE
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.cooperative.create ─────────────────────────────
        // Create a new cooperative with a surplus distribution model.
        state.phone.register_raw("treasurer.cooperative.create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = req_str(&params, "name", "treasurer.cooperative.create")?;
            let surplus_model: fortune::SurplusDistribution = params.get("surplus_model")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(fortune::SurplusDistribution::Equal);

            let coop = fortune::Cooperative::new(name, surplus_model);
            let coop_json = serde_json::to_value(&coop)
                .map_err(|e| err("treasurer.cooperative.create", e))?;
            ok_json(&coop_json)
        });

        // ── fortune.cooperative.add_member ────────────────────────
        // Add a member to a cooperative.
        state.phone.register_raw("treasurer.cooperative.add_member", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let coop_json = params.get("cooperative")
                .ok_or_else(|| err("treasurer.cooperative.add_member", "missing 'cooperative'"))?;
            let mut coop: fortune::Cooperative = serde_json::from_value(coop_json.clone())
                .map_err(|e| err("treasurer.cooperative.add_member", e))?;

            let member_json = params.get("member")
                .ok_or_else(|| err("treasurer.cooperative.add_member", "missing 'member'"))?;
            let member: fortune::CooperativeMember = serde_json::from_value(member_json.clone())
                .map_err(|e| err("treasurer.cooperative.add_member", e))?;

            coop.add_member(member)
                .map_err(|e| err("treasurer.cooperative.add_member", e))?;

            let result = serde_json::to_value(&coop)
                .map_err(|e| err("treasurer.cooperative.add_member", e))?;
            ok_json(&result)
        });

        // ── fortune.cooperative.distribute_surplus ─────────────────
        // Calculate surplus distribution based on the cooperative's model.
        state.phone.register_raw("treasurer.cooperative.distribute_surplus", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let coop_json = params.get("cooperative")
                .ok_or_else(|| err("treasurer.cooperative.distribute_surplus", "missing 'cooperative'"))?;
            let coop: fortune::Cooperative = serde_json::from_value(coop_json.clone())
                .map_err(|e| err("treasurer.cooperative.distribute_surplus", e))?;
            let surplus = req_i64(&params, "surplus", "treasurer.cooperative.distribute_surplus")?;

            let distribution = coop.distribute_surplus(surplus);
            let dist_json: Vec<Value> = distribution.iter().map(|(pubkey, share)| {
                json!({ "pubkey": pubkey, "share": share })
            }).collect();

            ok_json(&json!({ "distribution": dist_json }))
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  COMMONS TRUST
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.trust.create ──────────────────────────────────
        // Create a new commons trust.
        state.phone.register_raw("treasurer.trust.create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let name = req_str(&params, "name", "treasurer.trust.create")?;
            let trust_type: fortune::TrustType = params.get("trust_type")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or(fortune::TrustType::Knowledge);

            let trust = fortune::CommonsTrust::new(name, trust_type);
            let trust_json = serde_json::to_value(&trust)
                .map_err(|e| err("treasurer.trust.create", e))?;
            ok_json(&trust_json)
        });

        // ── fortune.trust.add_steward ─────────────────────────────
        // Add a steward to a trust.
        state.phone.register_raw("treasurer.trust.add_steward", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trust_json = params.get("trust")
                .ok_or_else(|| err("treasurer.trust.add_steward", "missing 'trust'"))?;
            let mut trust: fortune::CommonsTrust = serde_json::from_value(trust_json.clone())
                .map_err(|e| err("treasurer.trust.add_steward", e))?;
            let pubkey = req_str(&params, "pubkey", "treasurer.trust.add_steward")?;

            trust.add_steward(pubkey);

            let result = serde_json::to_value(&trust)
                .map_err(|e| err("treasurer.trust.add_steward", e))?;
            ok_json(&result)
        });

        // ── fortune.trust.remove_steward ──────────────────────────
        // Remove a steward from a trust.
        state.phone.register_raw("treasurer.trust.remove_steward", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trust_json = params.get("trust")
                .ok_or_else(|| err("treasurer.trust.remove_steward", "missing 'trust'"))?;
            let mut trust: fortune::CommonsTrust = serde_json::from_value(trust_json.clone())
                .map_err(|e| err("treasurer.trust.remove_steward", e))?;
            let pubkey = req_str(&params, "pubkey", "treasurer.trust.remove_steward")?;

            trust.remove_steward(pubkey);

            let result = serde_json::to_value(&trust)
                .map_err(|e| err("treasurer.trust.remove_steward", e))?;
            ok_json(&result)
        });

        // ── fortune.trust.add_asset ───────────────────────────────
        // Add an asset to a trust.
        state.phone.register_raw("treasurer.trust.add_asset", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trust_json = params.get("trust")
                .ok_or_else(|| err("treasurer.trust.add_asset", "missing 'trust'"))?;
            let mut trust: fortune::CommonsTrust = serde_json::from_value(trust_json.clone())
                .map_err(|e| err("treasurer.trust.add_asset", e))?;

            let name = req_str(&params, "asset_name", "treasurer.trust.add_asset")?;
            let description = req_str(&params, "asset_description", "treasurer.trust.add_asset")?;
            let asset_type = req_str(&params, "asset_type", "treasurer.trust.add_asset")?;
            let added_by = req_str(&params, "added_by", "treasurer.trust.add_asset")?;

            let asset = fortune::TrustAsset::new(name, description, asset_type, added_by);
            trust.add_asset(asset);

            let result = serde_json::to_value(&trust)
                .map_err(|e| err("treasurer.trust.add_asset", e))?;
            ok_json(&result)
        });

        // ── fortune.trust.record_stewardship ──────────────────────
        // Record a stewardship action.
        state.phone.register_raw("treasurer.trust.record_stewardship", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let trust_json = params.get("trust")
                .ok_or_else(|| err("treasurer.trust.record_stewardship", "missing 'trust'"))?;
            let mut trust: fortune::CommonsTrust = serde_json::from_value(trust_json.clone())
                .map_err(|e| err("treasurer.trust.record_stewardship", e))?;

            let steward = req_str(&params, "steward", "treasurer.trust.record_stewardship")?;
            let action = req_str(&params, "action", "treasurer.trust.record_stewardship")?;

            let record = fortune::StewardshipRecord::new(steward, action);
            trust.record_stewardship(record);

            let result = serde_json::to_value(&trust)
                .map_err(|e| err("treasurer.trust.record_stewardship", e))?;
            ok_json(&result)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  COMMERCE — Products
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.commerce.product.create ────────────────────────
        // Create a new product listing linked to an .idea digit.
        state.phone.register_raw("treasurer.commerce.product.create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let idea_id = req_str(&params, "idea_id", "treasurer.commerce.product.create")?;
            let seller_pubkey = req_str(&params, "seller_pubkey", "treasurer.commerce.product.create")?;
            let product = fortune::Product::new(idea_id, seller_pubkey);
            let product_json = serde_json::to_value(&product)
                .map_err(|e| err("treasurer.commerce.product.create", e))?;
            ok_json(&product_json)
        });

        // ── fortune.commerce.product.add_variant ──────────────────
        // Add a variant to a product.
        state.phone.register_raw("treasurer.commerce.product.add_variant", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let product_json = params.get("product")
                .ok_or_else(|| err("treasurer.commerce.product.add_variant", "missing 'product'"))?;
            let mut product: fortune::Product = serde_json::from_value(product_json.clone())
                .map_err(|e| err("treasurer.commerce.product.add_variant", e))?;

            let name = req_str(&params, "variant_name", "treasurer.commerce.product.add_variant")?;
            let price_modifier = opt_i64(&params, "price_modifier_cents").unwrap_or(0);
            let variant = fortune::ProductVariant::new(name, price_modifier);
            product.add_variant(variant);

            let result = serde_json::to_value(&product)
                .map_err(|e| err("treasurer.commerce.product.add_variant", e))?;
            ok_json(&result)
        });

        // ── fortune.commerce.product.reserve ──────────────────────
        // Reserve stock for a pending order.
        state.phone.register_raw("treasurer.commerce.product.reserve", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let product_json = params.get("product")
                .ok_or_else(|| err("treasurer.commerce.product.reserve", "missing 'product'"))?;
            let mut product: fortune::Product = serde_json::from_value(product_json.clone())
                .map_err(|e| err("treasurer.commerce.product.reserve", e))?;
            let quantity = params.get("quantity").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

            product.reserve(quantity)
                .map_err(|e| err("treasurer.commerce.product.reserve", e))?;

            let result = serde_json::to_value(&product)
                .map_err(|e| err("treasurer.commerce.product.reserve", e))?;
            ok_json(&result)
        });

        // ── fortune.commerce.product.release_reservation ──────────
        // Release a reservation (cancelled order).
        state.phone.register_raw("treasurer.commerce.product.release_reservation", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let product_json = params.get("product")
                .ok_or_else(|| err("treasurer.commerce.product.release_reservation", "missing 'product'"))?;
            let mut product: fortune::Product = serde_json::from_value(product_json.clone())
                .map_err(|e| err("treasurer.commerce.product.release_reservation", e))?;
            let quantity = params.get("quantity").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

            product.release_reservation(quantity);

            let result = serde_json::to_value(&product)
                .map_err(|e| err("treasurer.commerce.product.release_reservation", e))?;
            ok_json(&result)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  COMMERCE — Storefront
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.commerce.storefront.create ────────────────────
        // Create a new storefront.
        state.phone.register_raw("treasurer.commerce.storefront.create", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let owner_pubkey = req_str(&params, "owner_pubkey", "treasurer.commerce.storefront.create")?;
            let name = req_str(&params, "name", "treasurer.commerce.storefront.create")?;
            let description = req_str(&params, "description", "treasurer.commerce.storefront.create")?;

            let storefront = fortune::Storefront::new(owner_pubkey, name, description);
            let store_json = serde_json::to_value(&storefront)
                .map_err(|e| err("treasurer.commerce.storefront.create", e))?;
            ok_json(&store_json)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  COMMERCE — Cart
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.commerce.cart.apply ────────────────────────────
        // Apply an action (add, remove, update quantity, clear) to a cart.
        state.phone.register_raw("treasurer.commerce.cart.apply", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let cart_json = params.get("cart");
            let mut cart: fortune::Cart = cart_json
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let action_json = params.get("action")
                .ok_or_else(|| err("treasurer.commerce.cart.apply", "missing 'action'"))?;
            let action: fortune::CartAction = serde_json::from_value(action_json.clone())
                .map_err(|e| err("treasurer.commerce.cart.apply", e))?;

            cart.apply(action);

            let result = serde_json::to_value(&cart)
                .map_err(|e| err("treasurer.commerce.cart.apply", e))?;
            ok_json(&result)
        });

        // ── fortune.commerce.cart.summary ─────────────────────────
        // Get cart summary: total, item count, sellers.
        state.phone.register_raw("treasurer.commerce.cart.summary", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let cart_json = params.get("cart");
            let cart: fortune::Cart = cart_json
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let sellers: Vec<&str> = cart.sellers();
            ok_json(&json!({
                "total_cents": cart.total_cents(),
                "item_count": cart.item_count(),
                "is_empty": cart.is_empty(),
                "sellers": sellers
            }))
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  COMMERCE — Checkout & Orders
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.commerce.checkout.create_proposal ──────────────
        // Create a trade proposal from a cart for a specific seller.
        state.phone.register_raw("treasurer.commerce.checkout.create_proposal", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let cart_json = params.get("cart")
                .ok_or_else(|| err("treasurer.commerce.checkout.create_proposal", "missing 'cart'"))?;
            let cart: fortune::Cart = serde_json::from_value(cart_json.clone())
                .map_err(|e| err("treasurer.commerce.checkout.create_proposal", e))?;
            let seller = req_str(&params, "seller", "treasurer.commerce.checkout.create_proposal")?;
            let buyer = req_str(&params, "buyer", "treasurer.commerce.checkout.create_proposal")?;

            let proposal = fortune::CheckoutEngine::create_proposal(&cart, seller, buyer)
                .map_err(|e| err("treasurer.commerce.checkout.create_proposal", e))?;
            let proposal_json = serde_json::to_value(&proposal)
                .map_err(|e| err("treasurer.commerce.checkout.create_proposal", e))?;
            ok_json(&proposal_json)
        });

        // ── fortune.commerce.checkout.execute ──────────────────────
        // Execute checkout: create an Order from an accepted proposal.
        let s = state.clone();
        state.phone.register_raw("treasurer.commerce.checkout.execute", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let proposal_json = params.get("proposal")
                .ok_or_else(|| err("treasurer.commerce.checkout.execute", "missing 'proposal'"))?;
            let proposal: fortune::TradeProposal = serde_json::from_value(proposal_json.clone())
                .map_err(|e| err("treasurer.commerce.checkout.execute", e))?;
            let cart_json = params.get("cart")
                .ok_or_else(|| err("treasurer.commerce.checkout.execute", "missing 'cart'"))?;
            let cart: fortune::Cart = serde_json::from_value(cart_json.clone())
                .map_err(|e| err("treasurer.commerce.checkout.execute", e))?;

            let order = fortune::CheckoutEngine::execute(&proposal, &cart)
                .map_err(|e| err("treasurer.commerce.checkout.execute", e))?;

            s.email.send_raw("treasurer.commerce.order.placed", &serde_json::to_vec(&json!({
                "order_id": order.id.to_string(),
                "buyer": order.buyer_pubkey,
                "seller": order.seller_pubkey,
                "total_cents": order.total_cents
            })).unwrap_or_default());

            let order_json = serde_json::to_value(&order)
                .map_err(|e| err("treasurer.commerce.checkout.execute", e))?;
            ok_json(&order_json)
        });

        // ── fortune.commerce.order.advance ─────────────────────────
        // Advance an order through its lifecycle.
        let s = state.clone();
        state.phone.register_raw("treasurer.commerce.order.advance", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let order_json = params.get("order")
                .ok_or_else(|| err("treasurer.commerce.order.advance", "missing 'order'"))?;
            let mut order: fortune::Order = serde_json::from_value(order_json.clone())
                .map_err(|e| err("treasurer.commerce.order.advance", e))?;

            order.advance_status()
                .map_err(|e| err("treasurer.commerce.order.advance", e))?;

            s.email.send_raw("treasurer.commerce.order.status_changed", &serde_json::to_vec(&json!({
                "order_id": order.id.to_string(),
                "status": format!("{:?}", order.status)
            })).unwrap_or_default());

            let result = serde_json::to_value(&order)
                .map_err(|e| err("treasurer.commerce.order.advance", e))?;
            ok_json(&result)
        });

        // ── fortune.commerce.order.dispute ─────────────────────────
        // Dispute a delivered order.
        let s = state.clone();
        state.phone.register_raw("treasurer.commerce.order.dispute", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let order_json = params.get("order")
                .ok_or_else(|| err("treasurer.commerce.order.dispute", "missing 'order'"))?;
            let mut order: fortune::Order = serde_json::from_value(order_json.clone())
                .map_err(|e| err("treasurer.commerce.order.dispute", e))?;

            order.dispute()
                .map_err(|e| err("treasurer.commerce.order.dispute", e))?;

            s.email.send_raw("treasurer.commerce.order.disputed", &serde_json::to_vec(&json!({
                "order_id": order.id.to_string()
            })).unwrap_or_default());

            let result = serde_json::to_value(&order)
                .map_err(|e| err("treasurer.commerce.order.dispute", e))?;
            ok_json(&result)
        });

        // ── fortune.commerce.order.receipt ──────────────────────────
        // Create a receipt from a completed order.
        state.phone.register_raw("treasurer.commerce.order.receipt", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let order_json = params.get("order")
                .ok_or_else(|| err("treasurer.commerce.order.receipt", "missing 'order'"))?;
            let order: fortune::Order = serde_json::from_value(order_json.clone())
                .map_err(|e| err("treasurer.commerce.order.receipt", e))?;

            let receipt = fortune::Receipt::from_order(order);
            let receipt_json = serde_json::to_value(&receipt)
                .map_err(|e| err("treasurer.commerce.order.receipt", e))?;
            ok_json(&receipt_json)
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  PATTERN DETECTION (R2F)
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.pattern.classify_tier ──────────────────────────
        // Classify a transaction amount into Private/Receipted/Approved.
        state.phone.register_raw("treasurer.pattern.classify_tier", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let amount = req_i64(&params, "amount", "treasurer.pattern.classify_tier")?;
            let policy: fortune::TransactionTierPolicy = params.get("tier_policy")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let tier = policy.classify(amount);
            let tier_json = serde_json::to_value(&tier)
                .map_err(|e| err("treasurer.pattern.classify_tier", e))?;
            ok_json(&json!({ "tier": tier_json }))
        });

        // ── fortune.pattern.check_cash_denomination ────────────────
        // Check whether a cash note denomination is within the community cap.
        state.phone.register_raw("treasurer.pattern.check_cash_denomination", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let denomination = req_i64(&params, "denomination", "treasurer.pattern.check_cash_denomination")?;
            let policy: fortune::TransactionTierPolicy = params.get("tier_policy")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let within_cap = policy.is_cash_note_within_cap(denomination);
            ok_json(&json!({ "within_cap": within_cap }))
        });

        // ── fortune.pattern.create_receipt ─────────────────────────
        // Create a transaction receipt classified by community policy.
        state.phone.register_raw("treasurer.pattern.create_receipt", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let from_pubkey = req_str(&params, "from_pubkey", "treasurer.pattern.create_receipt")?;
            let to_pubkey = req_str(&params, "to_pubkey", "treasurer.pattern.create_receipt")?;
            let amount = req_i64(&params, "amount", "treasurer.pattern.create_receipt")?;
            let community_id = req_str(&params, "community_id", "treasurer.pattern.create_receipt")?;
            let policy: fortune::TransactionTierPolicy = params.get("tier_policy")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let receipt = fortune::TransactionReceipt::new(
                from_pubkey.to_string(), to_pubkey.to_string(),
                amount, community_id.to_string(), &policy,
            );
            let receipt_json = serde_json::to_value(&receipt)
                .map_err(|e| err("treasurer.pattern.create_receipt", e))?;
            ok_json(&receipt_json)
        });

        // ── fortune.pattern.create_approval ────────────────────────
        // Create a multi-sig approval request for an Approved-tier transaction.
        state.phone.register_raw("treasurer.pattern.create_approval", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let receipt_json = params.get("receipt")
                .ok_or_else(|| err("treasurer.pattern.create_approval", "missing 'receipt'"))?;
            let receipt: fortune::TransactionReceipt = serde_json::from_value(receipt_json.clone())
                .map_err(|e| err("treasurer.pattern.create_approval", e))?;
            let requested_by = req_str(&params, "requested_by", "treasurer.pattern.create_approval")?;
            let required_approvals = params.get("required_approvals")
                .and_then(|v| v.as_u64()).unwrap_or(2) as usize;
            let expires_hours = params.get("expires_hours")
                .and_then(|v| v.as_u64()).unwrap_or(48);
            let expires_at = chrono::Utc::now() + chrono::Duration::hours(expires_hours as i64);

            let approval = fortune::ApprovalRequest::new(
                receipt, requested_by.to_string(), required_approvals, expires_at,
            );
            let approval_json = serde_json::to_value(&approval)
                .map_err(|e| err("treasurer.pattern.create_approval", e))?;
            ok_json(&approval_json)
        });

        // ── fortune.pattern.add_signature ──────────────────────────
        // Add an approval or rejection signature to an approval request.
        state.phone.register_raw("treasurer.pattern.add_signature", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let approval_json = params.get("approval")
                .ok_or_else(|| err("treasurer.pattern.add_signature", "missing 'approval'"))?;
            let mut approval: fortune::ApprovalRequest = serde_json::from_value(approval_json.clone())
                .map_err(|e| err("treasurer.pattern.add_signature", e))?;
            let approver = req_str(&params, "approver_pubkey", "treasurer.pattern.add_signature")?;
            let approved = params.get("approved").and_then(|v| v.as_bool()).unwrap_or(true);

            approval.add_signature(approver.to_string(), approved)
                .map_err(|e| err("treasurer.pattern.add_signature", e))?;

            let result = serde_json::to_value(&approval)
                .map_err(|e| err("treasurer.pattern.add_signature", e))?;
            ok_json(&result)
        });

        // ── fortune.pattern.detect_structuring ────────────────────
        // Detect transactions systematically below the receipted ceiling.
        state.phone.register_raw("treasurer.pattern.detect_structuring", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let receipts: Vec<fortune::TransactionReceipt> = params.get("receipts")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let tier_policy: fortune::TransactionTierPolicy = params.get("tier_policy")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let community_id = req_str(&params, "community_id", "treasurer.pattern.detect_structuring")?;
            let config: fortune::DetectorConfig = params.get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut detector = fortune::FinancialPatternDetector::new(config);
            let alerts = detector.detect_structuring(&receipts, &tier_policy, community_id);
            let alerts_json = serde_json::to_value(&alerts)
                .map_err(|e| err("treasurer.pattern.detect_structuring", e))?;
            ok_json(&json!({ "alerts": alerts_json }))
        });

        // ── fortune.pattern.detect_volume_anomaly ──────────────────
        // Detect sudden spikes in transaction volume.
        state.phone.register_raw("treasurer.pattern.detect_volume_anomaly", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let period_counts: std::collections::HashMap<String, Vec<u64>> = params.get("period_counts")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let community_id = req_str(&params, "community_id", "treasurer.pattern.detect_volume_anomaly")?;
            let config: fortune::DetectorConfig = params.get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut detector = fortune::FinancialPatternDetector::new(config);
            let alerts = detector.detect_volume_anomaly(&period_counts, community_id);
            let alerts_json = serde_json::to_value(&alerts)
                .map_err(|e| err("treasurer.pattern.detect_volume_anomaly", e))?;
            ok_json(&json!({ "alerts": alerts_json }))
        });

        // ── fortune.pattern.detect_circular_flow ──────────────────
        // Detect funds cycling A -> B -> C -> A with net transfer near zero.
        state.phone.register_raw("treasurer.pattern.detect_circular_flow", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let receipts: Vec<fortune::TransactionReceipt> = params.get("receipts")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let community_id = req_str(&params, "community_id", "treasurer.pattern.detect_circular_flow")?;
            let tolerance = opt_i64(&params, "tolerance").unwrap_or(100);
            let config: fortune::DetectorConfig = params.get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut detector = fortune::FinancialPatternDetector::new(config);
            let alerts = detector.detect_circular_flow(&receipts, community_id, tolerance);
            let alerts_json = serde_json::to_value(&alerts)
                .map_err(|e| err("treasurer.pattern.detect_circular_flow", e))?;
            ok_json(&json!({ "alerts": alerts_json }))
        });

        // ── fortune.pattern.detect_rapid_cash_cycling ──────────────
        // Detect cash notes minted and redeemed in quick succession.
        state.phone.register_raw("treasurer.pattern.detect_rapid_cash_cycling", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let events: Vec<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>, String, String)> =
                params.get("events")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
            let community_id = req_str(&params, "community_id", "treasurer.pattern.detect_rapid_cash_cycling")?;
            let config: fortune::DetectorConfig = params.get("config")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let mut detector = fortune::FinancialPatternDetector::new(config);
            let alerts = detector.detect_rapid_cash_cycling(&events, community_id);
            let alerts_json = serde_json::to_value(&alerts)
                .map_err(|e| err("treasurer.pattern.detect_rapid_cash_cycling", e))?;
            ok_json(&json!({ "alerts": alerts_json }))
        });

        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
        //  FEDERATION SCOPE
        // ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

        // ── fortune.federation.create_scope ────────────────────────
        // Create a federation scope (unrestricted or community-scoped).
        state.phone.register_raw("treasurer.federation.create_scope", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let communities: Vec<String> = params.get("communities")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();

            let scope = if communities.is_empty() {
                fortune::EconomicFederationScope::new()
            } else {
                fortune::EconomicFederationScope::from_communities(communities)
            };

            let scope_json = serde_json::to_value(&scope)
                .map_err(|e| err("treasurer.federation.create_scope", e))?;
            ok_json(&scope_json)
        });

        // ── fortune.federation.is_visible ─────────────────────────
        // Check whether a community is visible under a given scope.
        state.phone.register_raw("treasurer.federation.is_visible", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let scope_json = params.get("scope");
            let scope: fortune::EconomicFederationScope = scope_json
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let community_id = req_str(&params, "community_id", "treasurer.federation.is_visible")?;
            ok_json(&json!({
                "visible": scope.is_visible(community_id),
                "unrestricted": scope.is_unrestricted()
            }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // ── Policy ──
            .with_call(CallDescriptor::new("treasurer.policy.get", "Get economic policy (default/testing/conservative)"))
            // ── Treasury ──
            .with_call(CallDescriptor::new("treasurer.treasury.status", "Treasury status snapshot"))
            .with_call(CallDescriptor::new("treasurer.treasury.max_supply", "Calculate max supply from metrics"))
            .with_call(CallDescriptor::new("treasurer.treasury.mint", "Mint Cool into circulation"))
            .with_call(CallDescriptor::new("treasurer.treasury.mint_exact", "Mint exact amount or fail"))
            // ── Balance / Ledger ──
            .with_call(CallDescriptor::new("treasurer.balance.get", "Get balance (liquid + locked)"))
            .with_call(CallDescriptor::new("treasurer.ledger.credit", "Credit Cool to account"))
            .with_call(CallDescriptor::new("treasurer.ledger.debit", "Debit Cool from account"))
            .with_call(CallDescriptor::new("treasurer.ledger.transfer", "Atomic transfer between accounts"))
            .with_call(CallDescriptor::new("treasurer.ledger.transactions", "Transaction history for pubkey"))
            .with_call(CallDescriptor::new("treasurer.ledger.summary", "Aggregated transaction stats"))
            // ── UBI ──
            .with_call(CallDescriptor::new("treasurer.ubi.check_eligibility", "Check UBI eligibility"))
            .with_call(CallDescriptor::new("treasurer.ubi.claim", "Claim UBI allocation"))
            .with_call(CallDescriptor::new("treasurer.ubi.verify_identity", "Register verified identity"))
            .with_call(CallDescriptor::new("treasurer.ubi.flag_account", "Flag account (ineligible)"))
            .with_call(CallDescriptor::new("treasurer.ubi.unflag_account", "Unflag account"))
            // ── Demurrage ──
            .with_call(CallDescriptor::new("treasurer.demurrage.calculate", "Calculate decay for balance"))
            .with_call(CallDescriptor::new("treasurer.demurrage.preview", "Preview demurrage for account"))
            .with_call(CallDescriptor::new("treasurer.demurrage.run_cycle", "Run full demurrage cycle"))
            // ── Cash ──
            .with_call(CallDescriptor::new("treasurer.cash.issue", "Issue bearer cash note"))
            .with_call(CallDescriptor::new("treasurer.cash.redeem", "Redeem bearer cash note"))
            .with_call(CallDescriptor::new("treasurer.cash.revoke", "Revoke active cash note"))
            .with_call(CallDescriptor::new("treasurer.cash.validate_serial", "Validate XXXX-XXXX-XXXX serial"))
            .with_call(CallDescriptor::new("treasurer.cash.normalize_serial", "Normalize serial input"))
            .with_call(CallDescriptor::new("treasurer.cash.generate_serial", "Generate random serial"))
            // ── Exchange / Escrow ──
            .with_call(CallDescriptor::new("treasurer.exchange.propose", "Create trade proposal"))
            .with_call(CallDescriptor::new("treasurer.exchange.accept", "Accept trade proposal"))
            .with_call(CallDescriptor::new("treasurer.exchange.execute", "Execute accepted trade"))
            .with_call(CallDescriptor::new("treasurer.exchange.cancel", "Cancel trade proposal"))
            .with_call(CallDescriptor::new("treasurer.exchange.reject", "Reject trade proposal"))
            .with_call(CallDescriptor::new("treasurer.escrow.create", "Create escrow"))
            .with_call(CallDescriptor::new("treasurer.escrow.release", "Release escrow to provider"))
            .with_call(CallDescriptor::new("treasurer.escrow.refund", "Refund escrow to client"))
            // ── Flow-back ──
            .with_call(CallDescriptor::new("treasurer.flowback.calculate", "Calculate flow-back amount"))
            .with_call(CallDescriptor::new("treasurer.flowback.preview", "Preview flow-back with effective rate"))
            // ── Cooperative ──
            .with_call(CallDescriptor::new("treasurer.cooperative.create", "Create cooperative"))
            .with_call(CallDescriptor::new("treasurer.cooperative.add_member", "Add member to cooperative"))
            .with_call(CallDescriptor::new("treasurer.cooperative.distribute_surplus", "Calculate surplus distribution"))
            // ── Commons Trust ──
            .with_call(CallDescriptor::new("treasurer.trust.create", "Create commons trust"))
            .with_call(CallDescriptor::new("treasurer.trust.add_steward", "Add steward to trust"))
            .with_call(CallDescriptor::new("treasurer.trust.remove_steward", "Remove steward from trust"))
            .with_call(CallDescriptor::new("treasurer.trust.add_asset", "Add asset to trust"))
            .with_call(CallDescriptor::new("treasurer.trust.record_stewardship", "Record stewardship action"))
            // ── Commerce: Products ──
            .with_call(CallDescriptor::new("treasurer.commerce.product.create", "Create product listing"))
            .with_call(CallDescriptor::new("treasurer.commerce.product.add_variant", "Add variant to product"))
            .with_call(CallDescriptor::new("treasurer.commerce.product.reserve", "Reserve product stock"))
            .with_call(CallDescriptor::new("treasurer.commerce.product.release_reservation", "Release stock reservation"))
            // ── Commerce: Storefront ──
            .with_call(CallDescriptor::new("treasurer.commerce.storefront.create", "Create storefront"))
            // ── Commerce: Cart ──
            .with_call(CallDescriptor::new("treasurer.commerce.cart.apply", "Apply action to cart"))
            .with_call(CallDescriptor::new("treasurer.commerce.cart.summary", "Get cart summary"))
            // ── Commerce: Checkout & Orders ──
            .with_call(CallDescriptor::new("treasurer.commerce.checkout.create_proposal", "Create checkout proposal"))
            .with_call(CallDescriptor::new("treasurer.commerce.checkout.execute", "Execute checkout"))
            .with_call(CallDescriptor::new("treasurer.commerce.order.advance", "Advance order status"))
            .with_call(CallDescriptor::new("treasurer.commerce.order.dispute", "Dispute delivered order"))
            .with_call(CallDescriptor::new("treasurer.commerce.order.receipt", "Create order receipt"))
            // ── Pattern Detection ──
            .with_call(CallDescriptor::new("treasurer.pattern.classify_tier", "Classify transaction tier"))
            .with_call(CallDescriptor::new("treasurer.pattern.check_cash_denomination", "Check cash denomination cap"))
            .with_call(CallDescriptor::new("treasurer.pattern.create_receipt", "Create transaction receipt"))
            .with_call(CallDescriptor::new("treasurer.pattern.create_approval", "Create approval request"))
            .with_call(CallDescriptor::new("treasurer.pattern.add_signature", "Add approval signature"))
            .with_call(CallDescriptor::new("treasurer.pattern.detect_structuring", "Detect structuring patterns"))
            .with_call(CallDescriptor::new("treasurer.pattern.detect_volume_anomaly", "Detect volume anomalies"))
            .with_call(CallDescriptor::new("treasurer.pattern.detect_circular_flow", "Detect circular flows"))
            .with_call(CallDescriptor::new("treasurer.pattern.detect_rapid_cash_cycling", "Detect rapid cash cycling"))
            // ── Federation Scope ──
            .with_call(CallDescriptor::new("treasurer.federation.create_scope", "Create federation scope"))
            .with_call(CallDescriptor::new("treasurer.federation.is_visible", "Check community visibility"))
            // ── Events ──
            .with_emitted_event(EventDescriptor::new("treasurer.treasury.minted", "Cool minted into circulation"))
            .with_emitted_event(EventDescriptor::new("treasurer.balance.changed", "Balance changed"))
            .with_emitted_event(EventDescriptor::new("treasurer.transfer.completed", "Transfer completed"))
            .with_emitted_event(EventDescriptor::new("treasurer.ubi.claimed", "UBI claimed"))
            .with_emitted_event(EventDescriptor::new("treasurer.demurrage.cycle_complete", "Demurrage cycle finished"))
            .with_emitted_event(EventDescriptor::new("treasurer.cash.issued", "Cash note issued"))
            .with_emitted_event(EventDescriptor::new("treasurer.cash.redeemed", "Cash note redeemed"))
            .with_emitted_event(EventDescriptor::new("treasurer.cash.revoked", "Cash note revoked"))
            .with_emitted_event(EventDescriptor::new("treasurer.exchange.executed", "Trade executed"))
            .with_emitted_event(EventDescriptor::new("treasurer.escrow.released", "Escrow released"))
            .with_emitted_event(EventDescriptor::new("treasurer.escrow.refunded", "Escrow refunded"))
            .with_emitted_event(EventDescriptor::new("treasurer.commerce.order.placed", "Order placed"))
            .with_emitted_event(EventDescriptor::new("treasurer.commerce.order.status_changed", "Order status changed"))
            .with_emitted_event(EventDescriptor::new("treasurer.commerce.order.disputed", "Order disputed"))
    }
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  Parsing helpers
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

/// Parse FortunePolicy from params, falling back to default.
fn parse_policy(params: &Value) -> fortune::FortunePolicy {
    params.get("policy")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Parse NetworkMetrics from params, falling back to zeros.
fn parse_metrics(params: &Value) -> fortune::treasury::NetworkMetrics {
    let active_users = params.get("active_users")
        .and_then(|v| v.as_u64()).unwrap_or(0);
    let total_ideas = params.get("total_ideas")
        .and_then(|v| v.as_u64()).unwrap_or(0);
    let total_collectives = params.get("total_collectives")
        .and_then(|v| v.as_u64()).unwrap_or(0);
    fortune::treasury::NetworkMetrics {
        active_users,
        total_ideas,
        total_collectives,
    }
}

/// Parse MintReason from params, defaulting to Initial.
fn parse_mint_reason(params: &Value) -> fortune::treasury::MintReason {
    match params.get("reason").and_then(|v| v.as_str()) {
        Some("ubi") => fortune::treasury::MintReason::Ubi,
        Some("reward") => fortune::treasury::MintReason::Reward,
        Some("correction") => fortune::treasury::MintReason::Correction,
        Some("testing") => fortune::treasury::MintReason::Testing,
        _ => fortune::treasury::MintReason::Initial,
    }
}

/// Parse TransactionReason from params, defaulting to Transfer.
fn parse_transaction_reason(params: &Value) -> fortune::TransactionReason {
    match params.get("reason").and_then(|v| v.as_str()) {
        Some("ubi") => fortune::TransactionReason::Ubi,
        Some("transfer") => fortune::TransactionReason::Transfer,
        Some("purchase") => fortune::TransactionReason::Purchase,
        Some("reward") => fortune::TransactionReason::Reward,
        Some("initial") => fortune::TransactionReason::Initial,
        Some("demurrage") => fortune::TransactionReason::Demurrage,
        Some("fee") => fortune::TransactionReason::Fee,
        Some("correction") => fortune::TransactionReason::Correction,
        Some("cash_redeemed") => fortune::TransactionReason::CashRedeemed,
        Some("cash_issuance") => fortune::TransactionReason::CashIssuance,
        Some("cash_expired") => fortune::TransactionReason::CashExpired,
        _ => fortune::TransactionReason::Transfer,
    }
}
