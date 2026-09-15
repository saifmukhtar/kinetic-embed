use crate::JsonResponse;
use serde_json::Value;
use crate::state::RUNTIME;
use tracing;

/// Initiates a graceful shutdown of the Kinetic daemon.
pub fn handle_shutdown(_params: Option<Value>) -> JsonResponse {
    tracing::info!("Shutdown requested via API. Notifying graceful shutdown signal...");
    kinetic_local::shutdown::API_SHUTDOWN.notify_waiters();

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "status": "success",
            "message": "Graceful shutdown initiated"
        })),
        error: None,
    }
}

/// Restarts the Kinetic daemon using the native service manager.
pub fn handle_restart(_params: Option<Value>) -> JsonResponse {
    tracing::info!("Restart requested via API. Notifying graceful shutdown signal...");

    // Set the flag so main.rs exits with code 1 after graceful shutdown
    kinetic_local::shutdown::RESTART_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
    kinetic_local::shutdown::API_RESTART.notify_waiters();

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "status": "success",
            "message": "Restart initiated"
        })),
        error: None,
    }
}

/// Exports the local Proxy Root CA certificate for browser installation.
pub fn handle_get_ca_cert(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        let base_config_dir = kinetic_local::config::get_base_dir();

        let nsp = kinetic_core::constants::NSP_SUFFIX;
        let salt_prefix = &kinetic_core::constants::NETWORK_SALT_HEX[0..4];
        let ca_prefix = format!("{}-{}", nsp, salt_prefix);
        let ca_path = base_config_dir.join(format!("{}.cert.pem", ca_prefix));

        match tokio::fs::read_to_string(&ca_path).await {
            Ok(cert) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({ "cert": cert })),
                error: None,
            },
            Err(e) => {
                tracing::error!("Failed to read CA cert from {:?}: {}", ca_path, e);
                JsonResponse {
                    status: "error".to_string(),
                    data: None,
                    error: Some("CA cert not found".to_string()),
                }
            }
        }
    })
}
