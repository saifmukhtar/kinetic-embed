use std::collections::HashMap;
use serde::Serialize;
use kinetic_local::governance::GLOBAL_GOVERNANCE_STATE;
use crate::JsonResponse;

/// A frontend-friendly representation of the Governance State.
#[derive(Serialize)]
pub struct GovernanceStatusResponse {
    pub genesis_kyn: u64,
    pub active_sovereign_key_hex: Option<String>,
    pub is_halted: bool,
    pub halt_start_kyn: Option<u64>,
    pub total_paused_kyns: u64,
}

/// Retrieves the human-readable active governance state.
pub fn handle_get_action_status() -> JsonResponse {
    let gov = GLOBAL_GOVERNANCE_STATE.lock().unwrap();
    
    let active_key_hex = gov.active_sovereign_key.as_ref().map(hex::encode);

    let res = GovernanceStatusResponse {
        genesis_kyn: gov.genesis_kyn.0,
        active_sovereign_key_hex: active_key_hex,
        is_halted: gov.is_halted,
        halt_start_kyn: gov.halt_start_kyn.map(|k| k.0),
        total_paused_kyns: gov.total_paused_kyns,
    };

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(res).unwrap()),
        error: None,
    }
}

/// Retrieves the list of mapped prime names.
pub fn handle_get_prime_names() -> JsonResponse {
    let gov = GLOBAL_GOVERNANCE_STATE.lock().unwrap();
    
    let mut primes_hex = HashMap::new();
    for (name, pubkey_bytes) in &gov.mapped_prime_names {
        primes_hex.insert(name.clone(), hex::encode(pubkey_bytes));
    }

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(primes_hex).unwrap()),
        error: None,
    }
}

/// Retrieves the list of mapped infrastructure names.
pub fn handle_get_infra_names() -> JsonResponse {
    let gov = GLOBAL_GOVERNANCE_STATE.lock().unwrap();
    
    let mut infra_hex = HashMap::new();
    for (name, pubkey_bytes) in &gov.mapped_infra_names {
        infra_hex.insert(name.clone(), hex::encode(pubkey_bytes));
    }

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(infra_hex).unwrap()),
        error: None,
    }
}
