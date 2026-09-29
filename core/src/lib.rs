//! The core JSON-RPC bridge for Kinetic.
//! This crate parses JSON strings, routes them to `kinetic-core` or `kinetic-network`,
//! and returns JSON strings. This is the single unified pipe for all mobile/wasm clients.

pub mod commands;
pub mod state;
pub mod callback;

uniffi::setup_scaffolding!();

/// The single UniFFI "Steel Bridge".
/// Pass any JSON-RPC string in, get a JSON string back.
/// UniFFI auto-generates the Kotlin: `Kinetic.invokeKineticJson(String)`
#[uniffi::export]
pub fn invoke_kinetic_json(req_json: String) -> String {
    execute_command(&req_json)
}

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
        // Bootstrapper — must be called first by the mobile app.
        "init_kinetic" => commands::bootstrap::handle_init_kinetic(req.params),

        "get_action_status" => commands::action::handle_get_action_status(req.params),
        "get_action_names" => commands::action::handle_get_action_names(req.params),
        "publish_action" => commands::action::handle_publish_action(req.params),

        "get_config" => commands::config::handle_get_config(req.params),
        "set_config" => commands::config::handle_set_config(req.params),
        "owned_names" => commands::config::handle_owned_names(req.params),
        "network_status" => commands::config::handle_network_status(req.params),
        "network_bootstrap" => commands::config::handle_network_bootstrap(req.params),
        "network_nat" => commands::config::handle_network_nat(req.params),
        "network_banned" => commands::config::handle_network_banned(req.params),
        "network_peers" => commands::config::handle_network_peers(req.params),
        "get_health" => commands::config::handle_get_health(req.params),
        "get_peer_id" => commands::config::handle_get_peer_id(req.params),
        "dns_flush" => commands::config::handle_dns_flush(req.params),

        "atlas_sync" => commands::atlas::handle_atlas_sync(req.params),

        "gossip_subscribe" => commands::gossip::handle_gossip_subscribe(req.params),
        "gossip_publish" => commands::gossip::handle_gossip_publish(req.params),
        "gossip_topics" => commands::gossip::handle_gossip_topics(req.params),

        "list_kids" => commands::kid::handle_list_kids(req.params),
        "fetch_kid" => commands::kid::handle_fetch_kid(req.params),
        "generate_kid" => commands::kid::handle_generate_kid(req.params),
        "rotate_kid" => commands::kid::handle_rotate_kid(req.params),
        "revoke_kid" => commands::kid::handle_revoke_kid(req.params),
        "get_kid_manifest" => commands::kid::handle_get_kid_manifest(req.params),
        "update_kid_manifest" => commands::kid::handle_update_kid_manifest(req.params),
        "resolve_kid" => commands::kid::handle_resolve_kid(req.params),
        "publish_kid" => commands::kid::handle_publish_kid(req.params),
        "publish_manifest" => commands::kid::handle_publish_manifest(req.params),
        
        "get_time" => commands::time::handle_get_time(req.params),
        
        "get_iterations" => commands::vdf_api::handle_get_iterations(req.params),
        "takeover_iterations" => commands::vdf_api::handle_takeover_iterations(req.params),
        "validate_name" => commands::vdf_api::handle_validate_name(req.params),
        
        "get_heartbeat" => commands::heartbeat::handle_get_heartbeat_status(req.params),
        "post_heartbeat" => commands::heartbeat::handle_post_heartbeat(req.params),
        "post_fat_heartbeat" => commands::heartbeat::handle_post_fat_heartbeat(req.params),
        
        "macro_tasks" => commands::macro_api::handle_macro_tasks(req.params),
        "macro_status" => commands::macro_api::handle_macro_status(req.params),
        "macro_register_name" => commands::macro_api::handle_macro_register_name(req.params),
        "macro_renew_name" => commands::macro_api::handle_macro_renew_name(req.params),
        
        "shutdown_kinetic" => commands::system::handle_shutdown(req.params),
        "restart_kinetic" => commands::system::handle_restart(req.params),
        "get_ca_cert" => commands::system::handle_get_ca_cert(req.params),
        
        "publish_record" => commands::nrs::handle_publish_record(req.params),
        "publish_commit" => commands::nrs::handle_publish_commit(req.params),
        "resolve_name" => commands::nrs::handle_resolve_name(req.params),
        "verify_quorum" => commands::nrs::handle_verify_quorum(req.params),
        "get_reserved_names" => commands::nrs::handle_get_reserved_names(req.params),
        "get_zone" => commands::nrs::handle_get_zone(req.params),
        "post_zone" => commands::nrs::handle_post_zone(req.params),
        "publish_zone" => commands::nrs::handle_publish_zone(req.params),
        "post_local_zone" => commands::nrs::handle_post_local_zone(req.params),
        "delete_local_zone" => commands::nrs::handle_delete_local_zone(req.params),
        "get_local_zone" => commands::nrs::handle_get_local_zone(req.params),
        "publish_fat_zone" => commands::nrs::handle_publish_fat_zone(req.params),
        
        _ => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Unknown method or bridge under construction: {}", req.method)),
        },
    };

    serde_json::to_string(&response).unwrap_or_else(|_| {
        r#"{"status":"error","error":"Failed to serialize response"}"#.to_string()
    })
}

/// Invokes a Kinetic command from the native mobile side (Kotlin/Swift).
/// The caller MUST free the returned string using `free_kinetic_string`.
#[unsafe(no_mangle)]
pub extern "C" fn invoke_kinetic_command(req_ptr: *const std::ffi::c_char) -> *mut std::ffi::c_char {
    if req_ptr.is_null() {
        let err = r#"{"status":"error","error":"Request string is null"}"#;
        return std::ffi::CString::new(err).unwrap().into_raw();
    }

    let req_str = unsafe {
        match std::ffi::CStr::from_ptr(req_ptr).to_str() {
            Ok(s) => s,
            Err(_) => {
                let err = r#"{"status":"error","error":"Request string is not valid UTF-8"}"#;
                return std::ffi::CString::new(err).unwrap().into_raw();
            }
        }
    };

    let response_str = execute_command(req_str);
    
    // We unwrap here safely because our JSON strings shouldn't contain null bytes
    std::ffi::CString::new(response_str).unwrap().into_raw()
}

/// Frees a string previously allocated by `invoke_kinetic_command`.
#[unsafe(no_mangle)]
pub extern "C" fn free_kinetic_string(ptr: *mut std::ffi::c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = std::ffi::CString::from_raw(ptr);
        }
    }
}
