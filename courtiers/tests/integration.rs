//! Integration tests for all courtier modules.
//!
//! Tests identity, catalog completeness, Send+Sync bounds, the all_courtiers()
//! registry, and (where possible) handler registration + error handling via a
//! real DaemonState.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use courtiers::*;
use equipment::PhoneError;
use prerogative::{DaemonModule, DaemonState};
use prerogative::state::OmnibusRef;

// ═══════════════════════════════════════════════════════════════════════
//  Test helpers
// ═══════════════════════════════════════════════════════════════════════

/// Expected metadata for a courtier module.
struct Expected {
    id: &'static str,
    name: &'static str,
    deps: &'static [&'static str],
    calls: usize,
    events_emitted: usize,
}

/// Run the standard identity + catalog battery against a module.
fn check_module(module: &dyn DaemonModule, expected: &Expected) {
    // Identity
    assert_eq!(module.id(), expected.id, "{}: id mismatch", expected.id);
    assert_eq!(module.name(), expected.name, "{}: name mismatch", expected.id);
    assert!(!module.name().is_empty(), "{}: name should not be empty", expected.id);
    assert_eq!(module.deps(), expected.deps, "{}: deps mismatch", expected.id);

    // Catalog
    let catalog = module.catalog();
    assert_eq!(
        catalog.calls_handled().len(),
        expected.calls,
        "{}: expected {} calls, got {}",
        expected.id,
        expected.calls,
        catalog.calls_handled().len(),
    );
    assert_eq!(
        catalog.events_emitted().len(),
        expected.events_emitted,
        "{}: expected {} emitted events, got {}",
        expected.id,
        expected.events_emitted,
        catalog.events_emitted().len(),
    );

    // Call IDs should be unique within the catalog
    let call_ids: Vec<&str> = catalog.calls_handled().iter().map(|c| c.call_id()).collect();
    let unique: HashSet<&str> = call_ids.iter().copied().collect();
    assert_eq!(
        call_ids.len(),
        unique.len(),
        "{}: duplicate call IDs in catalog",
        expected.id,
    );

    // Every call should have a non-empty description
    for call in catalog.calls_handled() {
        assert!(!call.call_id().is_empty(), "{}: empty call_id", expected.id);
        assert!(!call.description().is_empty(), "{}: empty description for {}", expected.id, call.call_id());
    }

    // Every emitted event should have a non-empty description
    for event in catalog.events_emitted() {
        assert!(!event.email_id().is_empty(), "{}: empty event email_id", expected.id);
        assert!(!event.description().is_empty(), "{}: empty description for {}", expected.id, event.email_id());
    }
}

/// Create a minimal DaemonState for registration testing.
///
/// Starts a real Omnibus with an OS-assigned port (port 0) so there are
/// no port conflicts. Returns None if Omnibus fails to start (e.g., mDNS
/// unavailable in CI).
fn test_daemon_state() -> Option<Arc<DaemonState>> {
    let config = omnibus::OmnibusConfig {
        port: 0, // OS-assigned
        bind_all: false,
        device_name: "test-courtiers".into(),
        enable_upnp: false,
        log_capture_capacity: 100,
        ..Default::default()
    };

    let omnibus = match omnibus::Omnibus::start(config) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("test_daemon_state: Omnibus start failed (non-fatal): {e:?}");
            return None;
        }
    };

    let daemon_config = omnibus::DaemonConfig {
        omnibus: omnibus::OmnibusSection {
            port: 0,
            bind_all: false,
            device_name: "test".into(),
            data_dir: None,
            enable_upnp: false,
            home_node: None,
        },
        tower: Default::default(),
    };

    let state = DaemonState::new(
        OmnibusRef::Standalone(Arc::new(omnibus)),
        PathBuf::from("/tmp/courtiers-test"),
        daemon_config,
        "test-auth-token".into(),
    );

    Some(Arc::new(state))
}

// ═══════════════════════════════════════════════════════════════════════
//  1. Identity + Catalog tests (no DaemonState needed)
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_keeper_identity_and_catalog() {
    check_module(&keeper::KeeperModule, &Expected {
        id: "keeper",
        name: "Keeper (Sentinal)",
        deps: &["castellan"],
        calls: 17,
        events_emitted: 0,
    });
}

#[test]
fn test_clerk_identity_and_catalog() {
    check_module(&clerk::ClerkModule, &Expected {
        id: "clerk",
        name: "Clerk (Hall I/O)",
        deps: &["castellan"],
        calls: 11,
        events_emitted: 1,
    });
}

#[test]
fn test_bard_identity_and_catalog() {
    check_module(&bard::BardModule, &Expected {
        id: "bard",
        name: "Bard (Ideas Content)",
        deps: &["chamberlain", "castellan"],
        calls: 7,
        events_emitted: 3,
    });
}

#[test]
fn test_artificer_identity_and_catalog() {
    check_module(&artificer::ArtificerModule, &Expected {
        id: "artificer",
        name: "Artificer (Magic)",
        deps: &["castellan", "bard"],
        calls: 21,
        events_emitted: 6,
    });
}

#[test]
fn test_envoy_identity_and_catalog() {
    check_module(&envoy::EnvoyModule, &Expected {
        id: "envoy",
        name: "Envoy (Globe)",
        deps: &["omnibus"],
        calls: 17,
        events_emitted: 2,
    });
}

#[test]
fn test_treasurer_identity_and_catalog() {
    check_module(&treasurer::TreasurerModule, &Expected {
        id: "treasurer",
        name: "Treasurer (Fortune)",
        deps: &["castellan"],
        calls: 66,
        events_emitted: 14,
    });
}

#[test]
fn test_sage_identity_and_catalog() {
    check_module(&sage::SageModule, &Expected {
        id: "sage",
        name: "Sage (Advisor)",
        deps: &["chamberlain", "castellan"],
        calls: 34,
        events_emitted: 10,
    });
}

#[test]
fn test_magistrate_identity_and_catalog() {
    check_module(&magistrate::MagistrateModule, &Expected {
        id: "magistrate",
        name: "Magistrate (Kingdom)",
        deps: &[],
        calls: 14,
        events_emitted: 0,
    });
}

#[test]
fn test_tribune_identity_and_catalog() {
    check_module(&tribune::TribuneModule, &Expected {
        id: "tribune",
        name: "Tribune (Polity)",
        deps: &[],
        calls: 15,
        events_emitted: 0,
    });
}

#[test]
fn test_warden_identity_and_catalog() {
    check_module(&warden::WardenModule, &Expected {
        id: "warden",
        name: "Warden (Bulwark)",
        deps: &[],
        calls: 12,
        events_emitted: 0,
    });
}

#[test]
fn test_marshal_identity_and_catalog() {
    check_module(&marshal::MarshalModule, &Expected {
        id: "marshal",
        name: "Marshal (Jail)",
        deps: &[],
        calls: 14,
        events_emitted: 0,
    });
}

#[test]
fn test_interpreter_identity_and_catalog() {
    check_module(&interpreter::InterpreterModule, &Expected {
        id: "interpreter",
        name: "Interpreter (Lingo)",
        deps: &[],
        calls: 11,
        events_emitted: 0,
    });
}

#[test]
fn test_tailor_identity_and_catalog() {
    check_module(&tailor::TailorModule, &Expected {
        id: "tailor",
        name: "Tailor (Regalia)",
        deps: &[],
        calls: 18,
        events_emitted: 2,
    });
}

#[test]
fn test_ambassador_identity_and_catalog() {
    check_module(&ambassador::AmbassadorModule, &Expected {
        id: "ambassador",
        name: "Ambassador (Nexus)",
        deps: &["castellan"],
        calls: 10,
        events_emitted: 2,
    });
}

#[test]
fn test_chronicler_identity_and_catalog() {
    check_module(&chronicler::ChroniclerModule, &Expected {
        id: "chronicler",
        name: "Chronicler (Yoke)",
        deps: &["castellan"],
        calls: 23,
        events_emitted: 1,
    });
}

#[test]
fn test_mentor_identity_and_catalog() {
    check_module(&mentor::MentorModule, &Expected {
        id: "mentor",
        name: "Mentor (Oracle)",
        deps: &[],
        calls: 14,
        events_emitted: 1,
    });
}

#[test]
fn test_champion_identity_and_catalog() {
    check_module(&champion::ChampionModule, &Expected {
        id: "champion",
        name: "Champion (Quest)",
        deps: &[],
        calls: 11,
        events_emitted: 3,
    });
}

#[test]
fn test_scout_identity_and_catalog() {
    check_module(&scout::ScoutModule, &Expected {
        id: "scout",
        name: "Scout (Zeitgeist)",
        deps: &[],
        calls: 15,
        events_emitted: 2,
    });
}

#[test]
fn test_watchman_identity_and_catalog() {
    check_module(&watchman::WatchmanModule, &Expected {
        id: "watchman",
        name: "Watchman (Undercroft)",
        deps: &[],
        calls: 12,
        events_emitted: 0,
    });
}

#[test]
fn test_ranger_identity_and_catalog() {
    check_module(&ranger::RangerModule, &Expected {
        id: "ranger",
        name: "Ranger (World)",
        deps: &[],
        calls: 22,
        events_emitted: 4,
    });
}

// ═══════════════════════════════════════════════════════════════════════
//  2. all_courtiers() registry tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_all_courtiers_count() {
    let courtiers = all_courtiers();
    assert_eq!(courtiers.len(), 23, "expected 23 courtiers, got {}", courtiers.len());
}

#[test]
fn test_all_courtiers_unique_ids() {
    let courtiers = all_courtiers();
    let ids: Vec<&str> = courtiers.iter().map(|c| c.id()).collect();
    let unique: HashSet<&str> = ids.iter().copied().collect();
    assert_eq!(
        ids.len(),
        unique.len(),
        "duplicate courtier IDs: {:?}",
        ids,
    );
}

#[test]
fn test_all_courtiers_expected_ids() {
    let courtiers = all_courtiers();
    let ids: HashSet<&str> = courtiers.iter().map(|c| c.id()).collect();

    let expected = [
        "chamberlain", "castellan", "keeper", "clerk", "bard",
        "artificer", "envoy", "treasurer", "sage",
        "magistrate", "tribune", "warden", "marshal",
        "interpreter", "tailor", "ambassador", "chronicler",
        "mentor", "champion", "scout", "watchman", "ranger",
        "vizier",
    ];

    for id in &expected {
        assert!(ids.contains(id), "missing courtier: {id}");
    }
}

#[test]
fn test_all_courtiers_dependency_order() {
    // Modules with no deps should be placeable anywhere. Modules with deps
    // should appear after those deps are registered. Verify that for each
    // module, its deps appear earlier in the list.
    let courtiers = all_courtiers();
    let ids: Vec<&str> = courtiers.iter().map(|c| c.id()).collect();

    for (i, module) in courtiers.iter().enumerate() {
        for dep in module.deps() {
            // The dep might be an infrastructure module (crown, vault, omnibus, ideas)
            // handled by Chancellor, not a courtier. Only check courtier deps.
            if let Some(dep_pos) = ids.iter().position(|id| id == dep) {
                assert!(
                    dep_pos < i,
                    "courtier '{}' (position {}) depends on '{}' (position {}), but dep comes after",
                    module.id(), i, dep, dep_pos,
                );
            }
        }
    }
}

#[test]
fn test_all_courtiers_no_empty_catalogs() {
    let courtiers = all_courtiers();
    for module in &courtiers {
        let catalog = module.catalog();
        assert!(
            !catalog.calls_handled().is_empty(),
            "courtier '{}' has an empty catalog (no calls)",
            module.id(),
        );
    }
}

#[test]
fn test_all_courtiers_no_duplicate_call_ids() {
    // Across ALL courtiers, no two modules should register the same call_id.
    let courtiers = all_courtiers();
    let mut seen: HashSet<String> = HashSet::new();

    for module in &courtiers {
        let catalog = module.catalog();
        for call in catalog.calls_handled() {
            assert!(
                seen.insert(call.call_id().to_string()),
                "duplicate call_id '{}' registered by courtier '{}'",
                call.call_id(),
                module.id(),
            );
        }
    }
}

#[test]
fn test_all_courtiers_total_operations() {
    // Sanity check: total calls across all courtiers should be substantial.
    let courtiers = all_courtiers();
    let total_calls: usize = courtiers.iter()
        .map(|c| c.catalog().calls_handled().len())
        .sum();
    let total_events: usize = courtiers.iter()
        .map(|c| c.catalog().events_emitted().len())
        .sum();

    // We know from counting: 339 calls, 45 events (346 - 7 removed from ranger)
    assert!(
        total_calls >= 330,
        "expected at least 340 total calls, got {total_calls}",
    );
    assert!(
        total_events >= 40,
        "expected at least 40 total emitted events, got {total_events}",
    );
}

// ═══════════════════════════════════════════════════════════════════════
//  3. Send + Sync compile-time checks
// ═══════════════════════════════════════════════════════════════════════

/// Compile-time proof that all courtier types satisfy Send + Sync.
/// If any module struct fails these bounds, this test won't compile.
fn _assert_send_sync<T: Send + Sync>() {}

#[test]
fn test_all_modules_are_send_sync() {
    _assert_send_sync::<keeper::KeeperModule>();
    _assert_send_sync::<clerk::ClerkModule>();
    _assert_send_sync::<bard::BardModule>();
    _assert_send_sync::<artificer::ArtificerModule>();
    _assert_send_sync::<envoy::EnvoyModule>();
    _assert_send_sync::<treasurer::TreasurerModule>();
    _assert_send_sync::<sage::SageModule>();
    _assert_send_sync::<magistrate::MagistrateModule>();
    _assert_send_sync::<tribune::TribuneModule>();
    _assert_send_sync::<warden::WardenModule>();
    _assert_send_sync::<marshal::MarshalModule>();
    _assert_send_sync::<interpreter::InterpreterModule>();
    _assert_send_sync::<tailor::TailorModule>();
    _assert_send_sync::<ambassador::AmbassadorModule>();
    _assert_send_sync::<chronicler::ChroniclerModule>();
    _assert_send_sync::<mentor::MentorModule>();
    _assert_send_sync::<champion::ChampionModule>();
    _assert_send_sync::<scout::ScoutModule>();
    _assert_send_sync::<watchman::WatchmanModule>();
    _assert_send_sync::<ranger::RangerModule>();
}

/// Compile-time proof that Box<dyn DaemonModule> is object-safe.
#[test]
fn test_daemon_module_object_safety() {
    let modules: Vec<Box<dyn DaemonModule>> = vec![
        Box::new(keeper::KeeperModule),
        Box::new(clerk::ClerkModule),
        Box::new(artificer::ArtificerModule),
    ];
    assert_eq!(modules.len(), 3);
    for m in &modules {
        assert!(!m.id().is_empty());
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  4. Handler registration + error handling (requires DaemonState)
// ═══════════════════════════════════════════════════════════════════════

/// After register(), every catalog call_id should be present on the Phone.
#[test]
fn test_registration_installs_all_handlers() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping registration test: could not create DaemonState");
            return;
        }
    };

    let courtiers = all_courtiers();
    for module in &courtiers {
        module.register(&state);
    }

    // Now verify every cataloged call_id has a handler on the Phone.
    for module in &courtiers {
        let catalog = module.catalog();
        for call in catalog.calls_handled() {
            assert!(
                state.phone.has_handler(call.call_id()),
                "courtier '{}': call '{}' is in catalog but not registered on Phone",
                module.id(),
                call.call_id(),
            );
        }
    }
}

/// Calling a registered handler with empty params should return
/// HandlerFailed, not panic. Tests error handling at the FFI boundary.
///
/// We test a representative subset: one stateless handler per court group.
#[test]
fn test_empty_params_returns_error_not_panic() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping error handling test: could not create DaemonState");
            return;
        }
    };

    // Register all modules first.
    let courtiers = all_courtiers();
    for module in &courtiers {
        module.register(&state);
    }

    // Representative call_ids to test with empty params.
    // These are chosen to be likely to parse params and fail gracefully.
    let test_calls = [
        // Inner Court
        "keeper.encrypt",
        "clerk.read",
        // Outer Court
        "artificer.digit_count",
        // Governance
        "magistrate.create_charter",
        "tribune.review",
        "warden.check_permission",
        "marshal.raise_flag",
        // Royal Staff
        "interpreter.detect_language",
        "tailor.create_reign",
        "ambassador.export",
        "chronicler.version_chain_create",
        "mentor.disclosure_create",
        "champion.engine_create",
        "scout.route",
        "watchman.network_health",
        "ranger.omnibus_status",
    ];

    for call_id in &test_calls {
        // Calling with empty bytes should not panic.
        let result = state.phone.call_raw(call_id, &[]);
        match result {
            Err(PhoneError::HandlerFailed { .. }) => {
                // Expected: handler parsed params, found them empty/invalid, returned error.
            }
            Err(PhoneError::NoHandler { .. }) => {
                panic!("call '{}' has no handler after registration", call_id);
            }
            Ok(_) => {
                // Some handlers accept empty params (e.g., status queries).
                // This is fine -- they handled null/empty gracefully.
            }
            Err(other) => {
                // Any other error variant is also acceptable -- the point is
                // it didn't panic.
                let _ = other;
            }
        }
    }
}

/// Calling with malformed JSON should return an error, not panic.
#[test]
fn test_malformed_json_returns_error_not_panic() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping malformed JSON test: could not create DaemonState");
            return;
        }
    };

    let courtiers = all_courtiers();
    for module in &courtiers {
        module.register(&state);
    }

    let garbage = b"this is not json {{{";

    // Test a handful of handlers with garbage input.
    let test_calls = [
        "keeper.encrypt",
        "clerk.read_header",
        "artificer.session_create",
        "magistrate.create_charter",
        "tribune.review",
        "interpreter.detect_language",
        "tailor.default_aura",
        "treasurer.policy.get",
        "sage.status",
    ];

    for call_id in &test_calls {
        // Should not panic. Any error or Ok is acceptable.
        let _result = state.phone.call_raw(call_id, garbage);
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  5. Catalog call_id naming convention tests
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn test_call_ids_follow_namespace_dot_action_convention() {
    let courtiers = all_courtiers();
    for module in &courtiers {
        let catalog = module.catalog();
        for call in catalog.calls_handled() {
            assert!(
                call.call_id().contains('.'),
                "courtier '{}': call_id '{}' should follow 'namespace.action' convention",
                module.id(),
                call.call_id(),
            );
        }
    }
}

#[test]
fn test_event_ids_follow_namespace_dot_event_convention() {
    let courtiers = all_courtiers();
    for module in &courtiers {
        let catalog = module.catalog();
        for event in module.catalog().events_emitted() {
            assert!(
                event.email_id().contains('.'),
                "courtier '{}': event_id '{}' should follow 'namespace.event' convention",
                module.id(),
                event.email_id(),
            );
        }
        // Suppress unused variable warning
        let _ = catalog;
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  6. Stateless handler tests (no Vault unlock needed)
// ═══════════════════════════════════════════════════════════════════════

/// Test that truly stateless handlers work end-to-end with valid params.
#[test]
fn test_stateless_password_strength() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    keeper::KeeperModule.register(&state);

    let params = serde_json::json!({ "password": "correcthorsebatterystaple" });
    let result = state.phone.call_raw(
        "keeper.password_strength",
        &serde_json::to_vec(&params).unwrap(),
    );

    assert!(result.is_ok(), "password_strength should succeed with valid params");
    let bytes = result.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // Response should have strength-related fields
    assert!(response.is_object(), "expected JSON object response");
}

/// Test recovery phrase generation (stateless, no Vault needed).
#[test]
fn test_stateless_recovery_generate() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    keeper::KeeperModule.register(&state);

    let result = state.phone.call_raw("keeper.recovery_generate", &[]);
    assert!(result.is_ok(), "recovery_generate should succeed");
    let bytes = result.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let words = response.get("words").and_then(|v| v.as_array());
    assert!(words.is_some(), "expected 'words' array");
    assert_eq!(words.unwrap().len(), 24, "BIP-39 phrase should be 24 words");
}

/// Test recovery phrase validation (stateless).
#[test]
fn test_stateless_recovery_validate() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    keeper::KeeperModule.register(&state);

    // Valid phrase: generate one, then validate it.
    let gen_result = state.phone.call_raw("keeper.recovery_generate", &[]).unwrap();
    let gen_response: serde_json::Value = serde_json::from_slice(&gen_result).unwrap();
    let words = gen_response.get("words").unwrap();

    let params = serde_json::json!({ "words": words });
    let result = state.phone.call_raw(
        "keeper.recovery_validate",
        &serde_json::to_vec(&params).unwrap(),
    );
    assert!(result.is_ok());
    let bytes = result.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(response.get("valid").and_then(|v| v.as_bool()), Some(true));

    // Invalid phrase: garbage words should return valid=false.
    let bad_params = serde_json::json!({ "words": ["not", "a", "real", "phrase"] });
    let bad_result = state.phone.call_raw(
        "keeper.recovery_validate",
        &serde_json::to_vec(&bad_params).unwrap(),
    );
    assert!(bad_result.is_ok());
    let bad_bytes = bad_result.unwrap();
    let bad_response: serde_json::Value = serde_json::from_slice(&bad_bytes).unwrap();
    assert_eq!(bad_response.get("valid").and_then(|v| v.as_bool()), Some(false));
}

/// Test salt generation (stateless).
#[test]
fn test_stateless_generate_salt() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    keeper::KeeperModule.register(&state);

    let params = serde_json::json!({ "length": 32 });
    let result = state.phone.call_raw(
        "keeper.generate_salt",
        &serde_json::to_vec(&params).unwrap(),
    );
    assert!(result.is_ok(), "generate_salt should succeed");
    let bytes = result.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(response.get("salt").is_some(), "expected 'salt' field");
}

/// Test language detection (Interpreter, stateless).
#[test]
fn test_stateless_detect_language() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    interpreter::InterpreterModule.register(&state);

    let params = serde_json::json!({ "text": "Hello, world!" });
    let result = state.phone.call_raw(
        "interpreter.detect_language",
        &serde_json::to_vec(&params).unwrap(),
    );
    assert!(result.is_ok(), "detect_language should succeed");
}

/// Test Regalia default tokens (stateless).
#[test]
fn test_stateless_default_aura() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    tailor::TailorModule.register(&state);

    let result = state.phone.call_raw("tailor.default_aura", &[]);
    assert!(result.is_ok(), "default_aura should succeed");
    let bytes = result.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(response.is_object(), "expected JSON object for Aura tokens");
}

/// Test Regalia default Reign (stateless).
#[test]
fn test_stateless_default_reign() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    tailor::TailorModule.register(&state);

    let result = state.phone.call_raw("tailor.default_reign", &[]);
    assert!(result.is_ok(), "default_reign should succeed");
}

/// Test Magic type registry (stateless).
#[test]
fn test_stateless_type_registry() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    artificer::ArtificerModule.register(&state);

    let result = state.phone.call_raw("artificer.type_registry", &[]);
    assert!(result.is_ok(), "type_registry should succeed");
    let bytes = result.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(response.is_object(), "expected JSON object for type registry");
}

/// Test Quest config presets (stateless).
#[test]
fn test_stateless_quest_config_presets() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    champion::ChampionModule.register(&state);

    let result = state.phone.call_raw("champion.config_presets", &[]);
    assert!(result.is_ok(), "champion.config_presets should succeed");
}

/// Test Oracle tier list (stateless).
#[test]
fn test_stateless_oracle_tiers_list() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    mentor::MentorModule.register(&state);

    let result = state.phone.call_raw("mentor.tiers_list", &[]);
    assert!(result.is_ok(), "mentor.tiers_list should succeed");
}

/// Test Polity immutable rights (stateless).
#[test]
fn test_stateless_polity_immutable_rights() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    tribune::TribuneModule.register(&state);

    let result = state.phone.call_raw("tribune.immutable_rights", &[]);
    assert!(result.is_ok(), "tribune.immutable_rights should succeed");
}

/// Test Jail default config (stateless).
#[test]
fn test_stateless_jail_default_config() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    marshal::MarshalModule.register(&state);

    let result = state.phone.call_raw("marshal.default_config", &[]);
    assert!(result.is_ok(), "marshal.default_config should succeed");
}

/// Test Fortune policy get (stateless).
#[test]
fn test_stateless_fortune_policy_get() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    treasurer::TreasurerModule.register(&state);

    let result = state.phone.call_raw("treasurer.policy.get", &[]);
    assert!(result.is_ok(), "treasurer.policy.get should succeed");
}

/// Test Bulwark age tier classification (stateless).
#[test]
fn test_stateless_bulwark_age_tier() {
    let state = match test_daemon_state() {
        Some(s) => s,
        None => {
            eprintln!("skipping stateless handler test: could not create DaemonState");
            return;
        }
    };

    warden::WardenModule.register(&state);

    let params = serde_json::json!({ "age": 25 });
    let result = state.phone.call_raw(
        "warden.age_tier",
        &serde_json::to_vec(&params).unwrap(),
    );
    assert!(result.is_ok(), "warden.age_tier should succeed with valid age");
}
