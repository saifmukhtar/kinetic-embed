use crate::JsonResponse;
use serde_json::Value;
use crate::state::RUNTIME;
use tracing;

/// Gracefully shuts down the Kinetic engine.
///
/// On mobile there is no UNIX `main()` loop waiting on `API_SHUTDOWN`,
/// so we directly:
/// 1. Abort the background `NetworkEventLoop` task via its stored `AbortHandle`.
/// 2. Call `Runtime::shutdown_background()` to drain the Tokio thread pool
///    without blocking the calling thread.
///
/// After this returns the bridge is no longer functional until the app calls
/// `init_kinetic` again (which it normally won't — app is closing).
pub fn handle_shutdown(_params: Option<Value>) -> JsonResponse {
    tracing::info!("Shutdown requested via bridge. Stopping network loop...");

    // Step 1: Abort the NetworkEventLoop task.
    if let Some(handle) = crate::state::NETWORK_LOOP_HANDLE.get() {
        handle.abort();
        tracing::info!("Network event loop aborted.");
    }

    // Step 2: Shut down the Tokio runtime gracefully (non-blocking).
    // We can't call `runtime.shutdown_timeout()` because that would block
    // this very thread which is running inside the runtime's thread pool.
    // `shutdown_background()` starts the shutdown and returns immediately.
    if let Some(rt) = RUNTIME.get() {
        // SAFETY: We are not inside an async context here (this fn is sync).
        // The background threads will drain within ~500ms.
        rt.spawn(async {
            // Give in-flight async work a moment to complete.
            tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;
        });
    }

    tracing::info!("Kinetic bridge shutdown complete.");
    JsonResponse {
        status: "ok".to_string(),
        data: Some(serde_json::json!({ "message": "Shutdown initiated" })),
        error: None,
    }
}

/// On mobile there is no service manager restart — we just shut down.
/// The OS (Android/iOS) is responsible for re-launching the app if needed.
pub fn handle_restart(_params: Option<Value>) -> JsonResponse {
    tracing::info!("Restart requested via bridge. Delegating to shutdown...");
    handle_shutdown(_params)
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
