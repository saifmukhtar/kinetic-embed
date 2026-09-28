use crate::JsonResponse;
use crate::state::get_atlas_nsps;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Payload sent by the kinetic-atlas bridge containing the list of registered foreign NSPs.
#[derive(Deserialize, Serialize, Debug)]
pub struct AtlasSyncPayload {
    /// List of NSPs supported by the atlas bridge.
    pub nsps: Vec<String>,
}

/// Webhook endpoint for the kinetic-atlas bridge to push updated NSP routing tables.
/// This updates the in-memory HashSet used by the DNS resolver.
pub fn handle_atlas_sync(params: Option<Value>) -> JsonResponse {
    let payload: AtlasSyncPayload = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(m) => m,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid params: {}", e)) }
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) }
    };

    let mut clean_nsps = std::collections::HashSet::new();

    // Normalize and add each NSP
    for nsp in payload.nsps {
        let mut t = nsp.trim().to_lowercase();
        if !t.starts_with('.') {
            t.insert(0, '.');
        }
        // Must be longer than just "."
        if t.len() > 1 {
            clean_nsps.insert(t);
        }
    }

    match get_atlas_nsps().write() {
        Ok(mut lock) => {
            let count = clean_nsps.len();
            tracing::info!(
                "Atlas Bridge synced {} NSPs successfully: {:?}",
                count,
                clean_nsps
            );

            *lock = clean_nsps;

            JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({
                    "synced_count": count
                })),
                error: None,
            }
        }
        Err(_) => {
            let sys_err = kinetic_core::error::SystemError::MutexPoisoned("atlas_nsps".into());
            tracing::error!(error = ?sys_err, "Failed to acquire write lock on atlas_nsps");
            JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("Internal Server Error: {}", sys_err.user_message())),
            }
        }
    }
}
