use serde_json::Value;
use crate::JsonResponse;

pub fn handle_shutdown() -> JsonResponse {
    kinetic_local::shutdown::API_SHUTDOWN.notify_waiters();
    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "message": "Graceful shutdown initiated"
        })),
        error: None,
    }
}

pub fn handle_restart() -> JsonResponse {
    kinetic_local::shutdown::RESTART_REQUESTED.store(true, std::sync::atomic::Ordering::SeqCst);
    kinetic_local::shutdown::API_RESTART.notify_waiters();
    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "message": "Restart initiated"
        })),
        error: None,
    }
}
