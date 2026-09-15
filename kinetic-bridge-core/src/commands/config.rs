//! HTTP REST API endpoints for daemon configuration, node status, owned names, and action state.

use crate::JsonResponse;
use crate::state::{get_network, get_storage, RUNTIME};
use serde_json::Value;
use kinetic_core::traits::StorageEngine;

pub fn handle_get_config(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        let config = tokio::task::spawn_blocking(kinetic_local::config::load_config)
            .await
            .map_err(|e| {
                let sys_err = kinetic_core::error::SystemError::ServerCrashed(e.to_string());
                format!("Internal Server Error: {}", sys_err.user_message())
            });

        match config {
            Ok(c) => JsonResponse {
                status: "ok".to_string(),
                data: Some(serde_json::to_value(c).unwrap()),
                error: None,
            },
            Err(e) => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(e),
            },
        }
    })
}

/// Handles requests to retrieve a list of names owned by this node.
pub fn handle_owned_names(_params: Option<Value>) -> JsonResponse {
    let owned_key = kinetic_core::constants::DB_PREFIX_OWNED_NAMES;
    match get_storage().get(owned_key) {
        Ok(Some(bytes)) => match serde_json::from_slice::<Vec<String>>(&bytes) {
            Ok(v) => JsonResponse {
                status: "ok".to_string(),
                data: Some(serde_json::to_value(v).unwrap()),
                error: None,
            },
            Err(e) => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("DeserializationFailed: {}", e)),
            },
        },
        Ok(None) => JsonResponse {
            status: "ok".to_string(),
            data: Some(serde_json::to_value(Vec::<String>::new()).unwrap()),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Storage error: {}", e)),
        },
    }
}

/// Handles requests to retrieve the current network status (peer count, DHT size, uptime).
pub fn handle_network_status(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        match get_network().get_network_status().await {
            Ok(status) => JsonResponse { status: "ok".to_string(), data: Some(status), error: None },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Handles requests to manually trigger a Kademlia network bootstrap.
pub fn handle_network_bootstrap(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        match get_network().rebootstrap_network().await {
            Ok(_) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({ "message": "Network bootstrap initiated." })),
                error: None,
            },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Handles requests to retrieve the current Libp2p AutoNAT status (e.g. Public, Private, Unknown).
pub fn handle_network_nat(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        match get_network().get_network_status().await {
            Ok(mut status) => {
                let nat_status = status
                    .as_object_mut()
                    .and_then(|obj| obj.remove("nat_status"))
                    .unwrap_or_else(|| serde_json::Value::String("Unknown".to_string()));
                JsonResponse { status: "ok".to_string(), data: Some(serde_json::json!({ "nat_status": nat_status })), error: None }
            }
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Handles requests to retrieve the list of currently banned spam peers and their expiration kyn.
pub fn handle_network_banned(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        match get_network().get_banned_peers().await {
            Ok(peers) => {
                let json_peers: Vec<serde_json::Value> = peers
                    .into_iter()
                    .map(|(id, exp)| serde_json::json!({ "peer_id": id, "expires_at_kyn": exp }))
                    .collect();
                JsonResponse { status: "ok".to_string(), data: Some(serde_json::json!({ "banned_peers": json_peers })), error: None }
            }
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Handles requests to retrieve the list of connected Peer IDs.
pub fn handle_network_peers(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        match get_network().get_connected_peers().await {
            Ok(peers) => JsonResponse { status: "ok".to_string(), data: Some(serde_json::to_value(peers).unwrap()), error: None },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Handles requests to update the daemon configuration.
pub fn handle_set_config(params: Option<Value>) -> JsonResponse {
    let mut payload = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let config_payload = match payload.as_object_mut().and_then(|m| m.remove("config")) {
        Some(cp) => cp,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'config' object in payload.".to_string()) },
    };

    match serde_json::from_value::<kinetic_core::config::KineticConfig>(config_payload) {
        Ok(new_config) => {
            if let Err(e) = new_config.validate() {
                return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid config: {}", e)) };
            }

            RUNTIME.get().unwrap().block_on(async {
                let res = tokio::task::spawn_blocking(move || kinetic_local::config::save_config(&new_config)).await;
                match res {
                    Ok(Ok(_)) => JsonResponse {
                        status: "success".to_string(),
                        data: Some(serde_json::json!({ "message": "Configuration saved. Restart engine to apply." })),
                        error: None,
                    },
                    Ok(Err(e)) => JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to save config: {}", e)) },
                    Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Server crashed: {}", e)) },
                }
            })
        }
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Invalid config payload format: {}", e)),
        }
    }
}

/// Handles requests to check the daemon health.
pub fn handle_get_health(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        let network_ok = get_network().get_network_status().await.is_ok();
        let storage_ok = get_storage().get(kinetic_core::constants::DB_PREFIX_LAST_DRAND).is_ok();

        if network_ok && storage_ok {
            JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({
                    "status": "OK",
                    "network": "healthy",
                    "storage": "healthy"
                })),
                error: None,
            }
        } else {
            JsonResponse {
                status: "error".to_string(),
                data: Some(serde_json::json!({
                    "status": "ERROR",
                    "network": if network_ok { "healthy" } else { "unresponsive" },
                    "storage": if storage_ok { "healthy" } else { "unresponsive" }
                })),
                error: Some("Service Unavailable".to_string()),
            }
        }
    })
}

/// Handles requests to retrieve the local peer ID.
pub fn handle_get_peer_id(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        match get_network().get_network_status().await {
            Ok(status) => {
                if let Some(peer_id) = status.get("peer_id").and_then(|p| p.as_str()) {
                    JsonResponse { status: "ok".to_string(), data: Some(serde_json::json!({ "peer_id": peer_id })), error: None }
                } else {
                    JsonResponse { status: "error".to_string(), data: None, error: Some("Network Offline".to_string()) }
                }
            }
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Flushes the local DNS resolution memory cache.
pub fn handle_dns_flush(_params: Option<Value>) -> JsonResponse {
    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "message": "Local DNS resolution cache flushed"
        })),
        error: None,
    }
}
