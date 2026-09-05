use serde_json::Value;
use crate::JsonResponse;
use crate::state::{get_network, RUNTIME};

/// Retrieves the current daemon configuration.
pub fn handle_get_config() -> JsonResponse {
    let config = kinetic_local::config::load_config();
    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(config).unwrap()),
        error: None,
    }
}

/// Retrieves the current network status (peer count, DHT size, uptime).
pub fn handle_network_status() -> JsonResponse {
    let network = get_network();
    let rt = RUNTIME.get().expect("Runtime not initialized");
    
    match rt.block_on(network.get_network_status()) {
        Ok(status) => JsonResponse {
            status: "success".to_string(),
            data: Some(status),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Network error: {}", e)),
        }
    }
}

/// Manually triggers a Kademlia network bootstrap.
pub fn handle_network_bootstrap() -> JsonResponse {
    let network = get_network();
    let rt = RUNTIME.get().expect("Runtime not initialized");
    
    match rt.block_on(network.rebootstrap_network()) {
        Ok(_) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "message": "Network bootstrap initiated."
            })),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Bootstrap failed: {}", e)),
        }
    }
}
