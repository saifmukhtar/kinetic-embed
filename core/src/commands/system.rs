use crate::JsonResponse;
use serde_json::Value;
use crate::state::RUNTIME;
use tracing;

/// Gracefully shuts down the Kinetic engine.
///
/// On mobile, to fully terminate blocking VDF math threads without
/// cancellation tokens, we use Option A: detonate the process.
/// This replicates the desktop daemon behavior where `/system/shutdown`
/// exits the process.
pub fn handle_shutdown(_params: Option<Value>) -> JsonResponse {
    tracing::info!("Shutdown requested via bridge. Stopping network loop...");

    // Step 1: Abort the NetworkEventLoop task.
    if let Some(handle) = crate::state::NETWORK_LOOP_HANDLE.get() {
        handle.abort();
        tracing::info!("Network event loop aborted.");
    }

    // Step 2: Detonate the process to kill VDF threads.
    if let Some(rt) = RUNTIME.get() {
        // We sleep for 500ms first so that the FFI can actually return the
        // JSON success response to the Kotlin/Swift caller before dying.
        rt.spawn(async {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            tracing::warn!("Executing strict Option A shutdown: exiting process to kill VDF threads.");
            std::process::exit(0);
        });
    }

    tracing::info!("Kinetic bridge shutdown complete.");
    JsonResponse {
        status: "ok".to_string(),
        data: Some(serde_json::json!({ "message": "Shutdown initiated, process exiting in 500ms" })),
        error: None,
    }
}

/// Restart alias — simply triggers the same process exit.
/// The mobile OS (or the app's internal logic) will handle re-launching the Activity.
pub fn handle_restart(_params: Option<Value>) -> JsonResponse {
    tracing::info!("Restart requested via bridge. Delegating to shutdown Option A...");
    handle_shutdown(_params)
}

/// Exports the local Proxy Root CA certificate for browser installation.
pub fn handle_get_ca_cert(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        let base_config_dir = kinetic_local::config::base_dir();

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
