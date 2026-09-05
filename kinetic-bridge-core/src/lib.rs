//! The core JSON-RPC bridge for Kinetic.
//! This crate parses JSON strings, routes them to `kinetic-core` or `kinetic-network`,
//! and returns JSON strings. This is the single unified pipe for all mobile/wasm clients.

pub mod commands;
pub mod state;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The single incoming command structure expected from iOS/Android/WASM.
#[derive(Debug, Deserialize)]
pub struct JsonRequest {
    /// The method or command to execute (e.g. "get_action_status")
    pub method: String,
    /// The arguments for the command.
    pub params: Option<Value>,
}

/// The unified response structure sent back to the client.
#[derive(Debug, Serialize)]
pub struct JsonResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The ONLY function you need to call from JNI, Swift, or WASM!
/// Simply pass a JSON string, and it returns a JSON string.
pub fn execute_command(json_input: &str) -> String {
    let req: JsonRequest = match serde_json::from_str(json_input) {
        Ok(r) => r,
        Err(e) => {
            return serde_json::to_string(&JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("Failed to parse JSON request: {}", e)),
            }).unwrap_or_default();
        }
    };

    let response = match req.method.as_str() {
        "get_action_status" => commands::action::handle_get_action_status(),
        "get_prime_names" => commands::action::handle_get_prime_names(),
        "get_infra_names" => commands::action::handle_get_infra_names(),
        
        "get_config" => commands::config::handle_get_config(),
        "network_status" => commands::config::handle_network_status(),
        "network_bootstrap" => commands::config::handle_network_bootstrap(),
        
        "get_heartbeats" => commands::heartbeat::handle_get_heartbeats(),
        "post_heartbeat" => commands::heartbeat::handle_post_heartbeat(req.params),
        
        "shutdown" => commands::system::handle_shutdown(),
        "restart" => commands::system::handle_restart(),
        
        "get_zone" => commands::zone::handle_get_zone(req.params.clone()),
        
        "list_kids" => commands::kid::handle_list_kids(),
        "fetch_kid" => commands::kid::handle_fetch_kid(req.params.clone()),
        "generate_kid" => commands::kid::handle_generate_kid(req.params.clone()),
        
        "publish_action" => commands::publish::handle_publish_action(req.params.clone()),
        
        "get_difficulty" => commands::consensus::handle_get_difficulty(req.params.clone()),
        "validate_name" => commands::consensus::handle_validate_name(req.params.clone()),
        _ => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Unknown method: {}", req.method)),
        },
    };

    serde_json::to_string(&response).unwrap_or_else(|_| {
        r#"{"status":"error","error":"Failed to serialize response"}"#.to_string()
    })
}
