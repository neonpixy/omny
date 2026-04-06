//! Interpreter module — Lingo language courtier.
//!
//! Exposes Babel obfuscation (encode/decode), language detection,
//! tokenization, and formula parsing/evaluation as daemon operations.
//! Vocabulary seeds are derived from Vault's master key — programs
//! never handle raw key material.

use std::sync::Arc;

use equipment::{CallDescriptor, ModuleCatalog, PhoneError};
use serde_json::{json, Value};

use prerogative::DaemonModule;
use prerogative::DaemonState;

pub struct InterpreterModule;

fn err(op: &str, msg: impl ToString) -> PhoneError {
    PhoneError::HandlerFailed { call_id: op.into(), message: msg.to_string() }
}

fn ok_json(v: &Value) -> Result<Vec<u8>, PhoneError> {
    serde_json::to_vec(v).map_err(|e| err("serialize", e))
}

fn guard_vault_unlocked(state: &DaemonState) -> Result<(), PhoneError> {
    let vault = state.vault.lock().unwrap();
    if !vault.is_unlocked() {
        return Err(err("interpreter", "Vault is locked — unlock identity first"));
    }
    Ok(())
}

impl DaemonModule for InterpreterModule {
    fn id(&self) -> &str { "interpreter" }
    fn name(&self) -> &str { "Interpreter (Lingo)" }
    fn deps(&self) -> &[&str] { &[] }

    fn register(&self, state: &Arc<DaemonState>) {
        // ── lingo.detect_language ─────────────────────────────
        // Stateless: detects language from text using Unicode script heuristics.
        state.phone.register_raw("interpreter.detect_language", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let text = params.get("text").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.detect_language", "missing 'text'"))?;

            let language = lingo::detection::detect_language(text);
            ok_json(&json!({ "language": language }))
        });

        // ── lingo.tokenize ────────────────────────────────────
        // Stateless: omnilingual tokenization.
        state.phone.register_raw("interpreter.tokenize", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let text = params.get("text").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.tokenize", "missing 'text'"))?;

            let tokens = lingo::tokenize(text);
            ok_json(&json!({ "tokens": tokens }))
        });

        // ── lingo.babel_encode ────────────────────────────────
        // Encode text using Babel obfuscation. Requires Vault for vocabulary seed.
        let s = state.clone();
        state.phone.register_raw("interpreter.babel_encode", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let text = params.get("text").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.babel_encode", "missing 'text'"))?;

            let vocab_seed = {
                let vault = s.vault.lock().unwrap();
                vault.vocabulary_seed()
                    .map_err(|e| err("interpreter.babel_encode", e))?
                    .expose()
                    .to_vec()
            };

            let babel = lingo::Babel::new(&vocab_seed);
            let encoded = babel.encode(text);
            ok_json(&json!({ "encoded": encoded }))
        });

        // ── lingo.babel_decode ────────────────────────────────
        // Decode Babel-encoded text. Requires Vault for vocabulary seed.
        let s = state.clone();
        state.phone.register_raw("interpreter.babel_decode", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let encoded = params.get("encoded").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.babel_decode", "missing 'encoded'"))?;
            let language = params.get("language").and_then(|v| v.as_str());

            let vocab_seed = {
                let vault = s.vault.lock().unwrap();
                vault.vocabulary_seed()
                    .map_err(|e| err("interpreter.babel_decode", e))?
                    .expose()
                    .to_vec()
            };

            let babel = lingo::Babel::new(&vocab_seed);
            let decoded = match language {
                Some(lang) => babel.decode_for_language(encoded, lang),
                None => babel.decode(encoded),
            };
            ok_json(&json!({ "decoded": decoded }))
        });

        // ── lingo.prepare_for_storage ─────────────────────────
        // Encode text for storage (Babel + language tag). Vault for seed.
        let s = state.clone();
        state.phone.register_raw("interpreter.prepare_for_storage", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let text = params.get("text").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.prepare_for_storage", "missing 'text'"))?;
            let source_language = params.get("language").and_then(|v| v.as_str())
                .unwrap_or("en");

            let vocab_seed = {
                let vault = s.vault.lock().unwrap();
                vault.vocabulary_seed()
                    .map_err(|e| err("interpreter.prepare_for_storage", e))?
                    .expose()
                    .to_vec()
            };

            let translator = lingo::UniversalTranslator::new()
                .with_babel(&vocab_seed);
            let stored = translator.prepare_for_storage(text, source_language)
                .map_err(|e| err("interpreter.prepare_for_storage", e))?;

            ok_json(&json!({
                "text": stored.text,
                "source_language": stored.source_language,
                "babel_encoded": stored.babel_encoded,
            }))
        });

        // ── lingo.vocabulary_stats ────────────────────────────
        // Stateless: returns the common token count.
        state.phone.register_raw("interpreter.vocabulary_stats", move |_data| {
            let count = lingo::vocabulary::common_token_count();
            ok_json(&json!({
                "common_token_count": count,
            }))
        });

        // ── lingo.script_for_language ─────────────────────────
        // Stateless: maps a BCP 47 language code to its primary script.
        state.phone.register_raw("interpreter.script_for_language", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let language = params.get("language").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.script_for_language", "missing 'language'"))?;

            let script = lingo::script_for_language(language);
            let separator = lingo::join_separator_for_script(script);
            ok_json(&json!({
                "script": format!("{script:?}"),
                "join_separator": separator,
            }))
        });

        // ── lingo.formula_parse ───────────────────────────────
        // Stateless: parse a spreadsheet formula into an AST.
        state.phone.register_raw("interpreter.formula_parse", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let formula = params.get("formula").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.formula_parse", "missing 'formula'"))?;

            let ast = lingo::FormulaParser::parse(formula)
                .map_err(|e| err("interpreter.formula_parse", e))?;

            ok_json(&json!({ "ast": format!("{ast:?}") }))
        });

        // ── lingo.formula_locale ──────────────────────────────
        // Stateless: translate a canonical formula for display in a locale.
        state.phone.register_raw("interpreter.formula_locale", move |data| {
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let formula = params.get("formula").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.formula_locale", "missing 'formula'"))?;
            let locale_name = params.get("locale").and_then(|v| v.as_str())
                .unwrap_or("en");

            let locale = match locale_name {
                "fr" => lingo::FormulaLocale::french(),
                "de" => lingo::FormulaLocale::german(),
                _ => lingo::FormulaLocale::english(),
            };

            let localized = locale.to_display(formula);
            let canonical = locale.to_canonical(&localized);
            ok_json(&json!({
                "localized": localized,
                "canonical": canonical,
                "decimal_separator": locale.decimal_separator.to_string(),
                "argument_separator": locale.argument_separator.to_string(),
            }))
        });

        // ── lingo.babel_encode_token ──────────────────────────
        // Stateless deterministic single-token encode. For diagnostics.
        let s = state.clone();
        state.phone.register_raw("interpreter.babel_encode_token", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let token = params.get("token").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.babel_encode_token", "missing 'token'"))?;

            let vocab_seed = {
                let vault = s.vault.lock().unwrap();
                vault.vocabulary_seed()
                    .map_err(|e| err("interpreter.babel_encode_token", e))?
                    .expose()
                    .to_vec()
            };

            let babel = lingo::Babel::new(&vocab_seed);
            let symbol = babel.encode_token(token);
            ok_json(&json!({ "symbol": symbol }))
        });

        // ── lingo.babel_decode_symbol ─────────────────────────
        // Stateless deterministic single-symbol decode. For diagnostics.
        let s = state.clone();
        state.phone.register_raw("interpreter.babel_decode_symbol", move |data| {
            guard_vault_unlocked(&s)?;
            let params: Value = serde_json::from_slice(data).unwrap_or(Value::Null);
            let symbol = params.get("symbol").and_then(|v| v.as_str())
                .ok_or_else(|| err("interpreter.babel_decode_symbol", "missing 'symbol'"))?;

            let vocab_seed = {
                let vault = s.vault.lock().unwrap();
                vault.vocabulary_seed()
                    .map_err(|e| err("interpreter.babel_decode_symbol", e))?
                    .expose()
                    .to_vec()
            };

            let babel = lingo::Babel::new(&vocab_seed);
            let token = babel.decode_symbol(symbol);
            ok_json(&json!({ "token": token }))
        });
    }

    fn catalog(&self) -> ModuleCatalog {
        ModuleCatalog::new()
            // Language detection & tokenization
            .with_call(CallDescriptor::new("interpreter.detect_language", "Detect text language (BCP 47)"))
            .with_call(CallDescriptor::new("interpreter.tokenize", "Omnilingual text tokenization"))
            .with_call(CallDescriptor::new("interpreter.script_for_language", "Map BCP 47 code to Unicode script"))
            // Babel obfuscation
            .with_call(CallDescriptor::new("interpreter.babel_encode", "Encode text with Babel obfuscation"))
            .with_call(CallDescriptor::new("interpreter.babel_decode", "Decode Babel-encoded text"))
            .with_call(CallDescriptor::new("interpreter.babel_encode_token", "Deterministic single-token encode"))
            .with_call(CallDescriptor::new("interpreter.babel_decode_symbol", "Deterministic single-symbol decode"))
            // Storage
            .with_call(CallDescriptor::new("interpreter.prepare_for_storage", "Encode text for storage (Babel + language tag)"))
            // Vocabulary
            .with_call(CallDescriptor::new("interpreter.vocabulary_stats", "Common token count and vocabulary info"))
            // Formula engine
            .with_call(CallDescriptor::new("interpreter.formula_parse", "Parse spreadsheet formula to AST"))
            .with_call(CallDescriptor::new("interpreter.formula_locale", "Translate formula for locale display"))
    }
}
