//! API endpoints for manually broadcasting heartbeats and checking real-time DHT heartbeat status.

use crate::JsonResponse;
use crate::state::{get_network, get_storage, KEYPAIR, RUNTIME};
use serde_json::Value;
use kinetic_core::constants;
use kinetic_core::types::{Heartbeat, KynNetworkExt};
use serde::Serialize;
use kinetic_core::traits::StorageEngine;

/// Represents the DHT heartbeat status of a locally owned name.
#[derive(Serialize)]
pub struct HeartbeatStatusResponse {
    /// The apex name.
    pub name: String,
    /// The calculated status (Active, Stale, Idle).
    pub status: String,
    /// The Kyn number of the last accepted heartbeat on the DHT.
    pub latest_kyn: u64,
    /// The number of Kyns this name has been idle.
    pub kyns_idle: u64,
}

/// Response returned when querying the status of all locally owned names.
#[derive(Serialize)]
pub struct HeartbeatsResponse {
    /// The current network Kyn.
    pub current_kyn: u64,
    /// Status of each locally owned name.
    pub names: Vec<HeartbeatStatusResponse>,
}

/// Active heartbeat freshness window (200 Kyns ~ 10 minutes at 3s/kyn).
const ACTIVE_HEARTBEAT_MAX_KYNS: u64 = 200;
/// Expiration window (28,800 Kyns = 1 Prism / 24 hours at 3s/kyn).
const STALE_HEARTBEAT_MAX_KYNS: u64 = 28_800;

/// Safely fetches the current Kyn using the network client, with verified local database cache fallback.
async fn get_safe_current_kyn() -> u64 {
    if let Ok(kyn) = get_network().get_current_kyn().await {
        if kyn > 0 {
            return kyn;
        }
    }

    let kyn_provider =
        kinetic_network::client::drand::DrandProvider::new(Some(get_storage()));
    use kinetic_core::traits::KynProvider;
    match kyn_provider.load_cached() {
        Ok(kyn) if kyn.kyn > 0 => kyn.kyn,
        _ => kinetic_core::types::Kyn::now_local().0,
    }
}

/// Fetches the real-time DHT heartbeat status of all locally owned names.
pub fn handle_get_heartbeats(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        let owned_key = constants::DB_PREFIX_OWNED_NAMES;
        let owned_names: Vec<String> = match get_storage().get(owned_key) {
            Ok(Some(bytes)) => match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("DeserializationFailed: {}", e)) },
            },
            Ok(None) => Vec::new(),
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        };

        let current_kyn = get_safe_current_kyn().await;

        let mut handles = Vec::new();
        for name in owned_names {
            let name_clone = name.clone();
            handles.push(tokio::spawn(async move {
                let res = get_network().resolve_heartbeat(&name_clone).await;
                (name_clone, res)
            }));
        }

        let mut statuses = Vec::new();
        for handle in handles {
            if let Ok((name, network_res)) = handle.await {
                match network_res {
                    Ok(bytes) => {
                        if let Ok(hb) = serde_json::from_slice::<Heartbeat>(&bytes) {
                            let age = current_kyn.saturating_sub(hb.latest_kyn);
                            let status = if age <= ACTIVE_HEARTBEAT_MAX_KYNS {
                                "Active"
                            } else if age <= STALE_HEARTBEAT_MAX_KYNS {
                                "Stale"
                            } else {
                                "Idle"
                            };
                            statuses.push(HeartbeatStatusResponse {
                                name,
                                status: status.to_string(),
                                latest_kyn: hb.latest_kyn,
                                kyns_idle: age,
                            });
                        } else {
                            statuses.push(HeartbeatStatusResponse {
                                name,
                                status: "Unknown (Parse Error)".to_string(),
                                latest_kyn: 0,
                                kyns_idle: 0,
                            });
                        }
                    }
                    Err(_) => {
                        statuses.push(HeartbeatStatusResponse {
                            name,
                            status: "Idle (Not Found on DHT)".to_string(),
                            latest_kyn: 0,
                            kyns_idle: 0,
                        });
                    }
                }
            }
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::to_value(HeartbeatsResponse {
                current_kyn,
                names: statuses,
            }).unwrap()),
            error: None,
        }
    })
}

/// Manually constructs and broadcasts a heartbeat for a specific name to the DHT.
pub fn handle_post_heartbeat(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    let normalized = kinetic_core::types::names::normalize_name(&name);
    if let Err(e) = kinetic_core::types::names::is_valid_apex_name(&normalized) {
        return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid name: {}", e)) };
    }

    RUNTIME.get().unwrap().block_on(async {
        let current_kyn = get_safe_current_kyn().await;

        let mut heartbeat = Heartbeat {
            name: normalized.clone(),
            latest_kyn: current_kyn,
            signature: vec![],
            authorization: None,
        };

        let signable_bytes = heartbeat.signable_bytes(constants::NETWORK_SALT);
        let keypair = KEYPAIR.get().expect("Daemon Keypair not loaded in bridge").clone();

        let sig_bytes = match tokio::task::spawn_blocking(move || keypair.sign(&signable_bytes)).await {
            Ok(s) => s,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Task spawn failed: {}", e)) },
        };
        heartbeat.signature = sig_bytes;

        let payload = match serde_json::to_vec(&heartbeat) {
            Ok(p) => p,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to serialize heartbeat: {}", e)) },
        };

        match get_network().publish_heartbeat(&normalized, payload).await {
            Ok(_) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({
                    "message": format!("Manually broadcasted heartbeat for {}", normalized),
                    "kyn": current_kyn
                })),
                error: None,
            },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

use serde::Deserialize;

/// Request payload for manually broadcasting a Fat Heartbeat.
#[derive(Deserialize)]
pub struct FatHeartbeatRequest {
    /// The private key of the hot key, hex encoded, to sign the heartbeat.
    pub hot_key_hex: String,
    /// The master-key authorized delegation proof.
    pub authorized_manifest: kinetic_core::types::identity::AuthorizedManifest,
}

/// Manually constructs and broadcasts a Fat Heartbeat for a specific name to the DHT,
/// using a delegated hot key and an authorized manifest instead of the daemon master key.
pub fn handle_post_fat_heartbeat(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let name = match p.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    let req: FatHeartbeatRequest = match p.get("payload") {
        Some(payload) => match serde_json::from_value(payload.clone()) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid FatHeartbeatRequest payload: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'payload'".to_string()) },
    };

    let normalized = kinetic_core::types::names::normalize_name(&name);
    if let Err(e) = kinetic_core::types::names::is_valid_apex_name(&normalized) {
        return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid name: {}", e)) };
    }

    // Verify the capability is present in the manifest
    let has_cap = req
        .authorized_manifest
        .manifest
        .services
        .iter()
        .any(|s| s.service_type == "kinetic.capability.heartbeat");
    if !has_cap {
        return JsonResponse { status: "error".to_string(), data: None, error: Some("Provided AuthorizedManifest does not contain kinetic.capability.heartbeat capability.".to_string()) };
    }

    // Load the hot key
    let hot_key_bytes = match hex::decode(&req.hot_key_hex) {
        Ok(b) => b,
        Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid hot_key_hex: {}", e)) },
    };
    let keypair = match kinetic_primitives::keys::KineticKeypair::from_slice(&hot_key_bytes) {
        Ok(k) => k,
        Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid ML-DSA keypair: {}", e)) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let current_kyn = get_safe_current_kyn().await;

        let mut heartbeat = Heartbeat {
            name: normalized.clone(),
            latest_kyn: current_kyn,
            signature: vec![],
            authorization: Some(Box::new(req.authorized_manifest)),
        };

        let signable_bytes = heartbeat.signable_bytes(constants::NETWORK_SALT);

        let sig_bytes = match tokio::task::spawn_blocking(move || keypair.sign(&signable_bytes)).await {
            Ok(s) => s,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Task spawn failed: {}", e)) },
        };
        heartbeat.signature = sig_bytes;

        let payload = match serde_json::to_vec(&heartbeat) {
            Ok(p) => p,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to serialize fat heartbeat: {}", e)) },
        };

        match get_network().publish_heartbeat(&normalized, payload).await {
            Ok(_) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({
                    "status": "success",
                    "message": format!("Manually broadcasted Fat Heartbeat for {}", normalized),
                    "kyn": current_kyn
                })),
                error: None,
            },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}
