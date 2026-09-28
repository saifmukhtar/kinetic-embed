//! HTTP REST API handlers for querying the Action transparency layer.

use crate::JsonResponse;
use crate::state::{get_network, get_storage, not_initialized, RUNTIME};
use kinetic_local::action::GLOBAL_ACTION_STATE;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// A period of time when the network was halted.
#[derive(Serialize)]
pub struct PausePeriod {
    /// The exact Kyn when the network was halted.
    pub start_kyn: u64,
    /// The exact Kyn when the network was resumed.
    pub end_kyn: u64,
}

/// High-level metrics summarizing the action state.
#[derive(Serialize)]
pub struct ActionMetrics {
    /// Total number of action/action commands executed since genesis.
    pub total_executed_actions: usize,
}

/// A frontend-friendly representation of the Action/Action State.
#[derive(Serialize)]
pub struct ActionStatusResponse {
    /// Genesis Kyn when action tracking started.
    pub genesis_kyn: u64,
    /// The current exact network Kyn.
    pub current_kyn: u64,
    /// The mathematically verified uptime age of the network in kyns.
    pub active_kyn_age: u64,
    /// Active ML-DSA-65 root public key controlling the network (hex encoded).
    pub active_sovereign_key_hex: Option<String>,
    /// Master boolean flag if the network is currently paused.
    pub is_halted: bool,
    /// The exact Kyn when the network was halted (if currently halted).
    pub halt_start_kyn: Option<u64>,
    /// Total number of drand kyns the network has been paused for since genesis.
    pub total_paused_kyns: u64,
    /// The last time the network was paused (if ever).
    pub last_pause: Option<PausePeriod>,
    /// Summary metrics for the dashboard.
    pub metrics: ActionMetrics,
}

/// Handles requests to retrieve the human-readable active action state.
pub fn handle_get_action_status(_params: Option<Value>) -> JsonResponse {
    let action_state = match GLOBAL_ACTION_STATE.lock() {
        Ok(s) => s,
        Err(e) => {
            return JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("MutexPoisoned: {}", e)),
            };
        }
    };

    let active_key_hex = action_state.active_sovereign_key.as_ref().map(|k| hex::encode(k.as_bytes()));

    // Fetch verified Kyn from the node's constantly updating local cache
    let current_kyn = {
        let kyn_provider =
            kinetic_network::client::beacon::BeaconProvider::new(Some(match get_storage() { Some(s) => s, None => return not_initialized() }));
        use kinetic_core::traits::KynProvider;
        match kyn_provider.load_cached() {
            Ok(kyn) => kyn.beacon_idx,
            Err(_) => 0, // Fallback to OS clock if DB is completely empty (genesis)
        }
    };

    let active_kyn_age = current_kyn
        .saturating_sub(action_state.genesis_kyn.0.0)
        .saturating_sub(action_state.total_paused_kyns);

    let last_pause = action_state
        .pause_history
        .last()
        .map(|(start, end)| PausePeriod {
            start_kyn: start.0.0,
            end_kyn: end.0.0,
        });

    let metrics = ActionMetrics {
        total_executed_actions: action_state.executed_hashes.len(),
    };

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(ActionStatusResponse {
            genesis_kyn: action_state.genesis_kyn.0.0,
            current_kyn,
            active_kyn_age,
            active_sovereign_key_hex: active_key_hex,
            is_halted: action_state.is_halted,
            halt_start_kyn: action_state.halt_start_kyn.map(|k| k.0.0),
            total_paused_kyns: action_state.total_paused_kyns,
            last_pause,
            metrics,
        }).unwrap()),
        error: None,
    }
}

/// Aggregated response containing both prime and infrastructure name mappings.
#[derive(Serialize)]
pub struct ActionNamesResponse {
    /// Mapped 1-character prime names (e.g., 'a', '7').
    pub primes: HashMap<String, String>,
    /// Mapped protocol infrastructure names (e.g., 'seed', 'node', 'api').
    pub infras: HashMap<String, String>,
}

/// Handles requests to retrieve all mapped Action names (primes and infras) in a single call.
pub fn handle_get_action_names(_params: Option<Value>) -> JsonResponse {
    let action_state = match GLOBAL_ACTION_STATE.lock() {
        Ok(s) => s,
        Err(e) => {
            return JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("MutexPoisoned: {}", e)),
            };
        }
    };

    let primes = HashMap::new();
    let infras = HashMap::new();

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(ActionNamesResponse { primes, infras }).unwrap()),
        error: None,
    }
}

use kinetic_core::traits::KynProvider;

#[derive(Serialize)]
pub struct PublishResponse {
    pub status: String,
    pub message: String,
}

/// Handles API requests to publish a `SignedNetworkAction` to the DHT/Gossip network.
pub fn handle_publish_action(params: Option<Value>) -> JsonResponse {
    let msg: kinetic_types::action::SignedNetworkAction = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(m) => m,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid params: {}", e)) }
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) }
    };

    tracing::info!("Received API publish request for Action action");

    RUNTIME.get().unwrap().block_on(async {
        let _current_kyn = {
            let kyn_provider =
                kinetic_network::client::beacon::BeaconProvider::new(Some(match get_storage() { Some(s) => s, None => return not_initialized() }));
            match kyn_provider.load_cached() {
                Ok(kyn) => kyn.beacon_idx,
                Err(_) => match kyn_provider.fetch_latest().await {
                    Ok(kyn) => kyn.beacon_idx,
                    Err(_) => 0,
                },
            }
        };

        let is_valid = {
            let mut action_state = kinetic_local::action::GLOBAL_ACTION_STATE.lock().unwrap();
            let res = kinetic_core::action::process_action_message(
                &mut action_state,
                &msg,
                kinetic_kyn::types::CurrentKyn(kinetic_kyn::types::Kyn(0)),
            );
            match res {
                Ok(_) => {
                    let path = std::env::var(kinetic_core::constants::ENV_ACTION)
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|_| {
                            let config = kinetic_local::config::load_config();
                            kinetic_local::config::base_dir()
                                .join(config.peer.storage_dir)
                                .join("action.db")
                        });
                    if let Err(e) = kinetic_local::action::save_action_to_disk(&action_state, &path) {
                        tracing::error!("Failed to save modified action state to disk: {}", e);
                    }
                    true
                }
                Err(e) => {
                    tracing::warn!("Rejecting action message via API: {}", e);
                    return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid action message: {}", e)) };
                }
            }
        };

        if !is_valid {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("Action message validation failed".to_string()) };
        }

        let payload_bytes = match serde_json::to_vec(&msg) {
            Ok(b) => b,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization failed: {}", e)) }
        };

        let mut envelope = vec![kinetic_types::network::NetworkOpcode::Action as u8];
        envelope.extend(payload_bytes);

        match match get_network() { Some(n) => n, None => return not_initialized() }.broadcast_gossip(kinetic_core::constants::GOSSIP_TOPIC_GLOBAL, envelope).await {
            Ok(_) => {
                tracing::info!("Successfully published Action Message to the Gossip network");
                JsonResponse {
                    status: "success".to_string(),
                    data: Some(serde_json::to_value(PublishResponse {
                        status: "success".to_string(),
                        message: "Action action accepted and routed to P2P network".to_string(),
                    }).unwrap()),
                    error: None,
                }
            }
            Err(e) => {
                tracing::error!("Failed to publish Action Message to P2P network: {}", e);
                JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to broadcast: {}", e)) }
            }
        }
    })
}
