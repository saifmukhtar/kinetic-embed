//! Metrics and telemetry endpoints for the Kinetic bridge.
//!
//! Provides real-time network statistics readable by the mobile app:
//! - `get_metrics`        — peer count, bytes sent/received, kyn, uptime, NAT status
//! - `get_network_status` — raw network status JSON from the NetworkClient

use crate::JsonResponse;
use crate::state::{get_network, not_initialized, RUNTIME};
use serde_json::Value;

/// Returns a flat metrics snapshot: peers, bandwidth, kyn, nat_status, uptime.
/// Suitable for a "dashboard" or "connection status" screen on mobile.
pub fn handle_get_metrics(_params: Option<Value>) -> JsonResponse {
    let rt = match RUNTIME.get() {
        Some(r) => r,
        None => return not_initialized(),
    };

    let network = match get_network() { Some(n) => n, None => return not_initialized() };
    let status = rt.block_on(async move { network.get_network_status().await });

    match status {
        Ok(v) => JsonResponse {
            status: "ok".to_string(),
            data: Some(v),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Failed to get metrics: {}", e)),
        },
    }
}

/// Returns the list of currently connected peers (their multiaddresses and peer IDs).
pub fn handle_get_connected_peers(_params: Option<Value>) -> JsonResponse {
    let rt = match RUNTIME.get() {
        Some(r) => r,
        None => return not_initialized(),
    };

    let network = match get_network() { Some(n) => n, None => return not_initialized() };
    let peers = rt.block_on(async move { network.get_connected_peers().await });

    match peers {
        Ok(list) => JsonResponse {
            status: "ok".to_string(),
            data: Some(serde_json::json!({ "peers": list, "count": list.len() })),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Failed to get peers: {}", e)),
        },
    }
}
