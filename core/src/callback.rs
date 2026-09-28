use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::OnceLock;
use serde_json::Value;

/// The function signature for the native mobile callback (Kotlin/Swift).
pub type MobileCallback = extern "C" fn(*const c_char);

/// The global registry holding the native callback pointer.
static GLOBAL_EVENT_CALLBACK: OnceLock<MobileCallback> = OnceLock::new();

/// Registers the mobile event callback.
/// This should be called exactly once by the native side when the app boots.
#[unsafe(no_mangle)]
pub extern "C" fn register_event_callback(cb: MobileCallback) {
    if GLOBAL_EVENT_CALLBACK.set(cb).is_err() {
        tracing::warn!("Event callback was already registered. Ignoring.");
    } else {
        tracing::info!("Native event callback registered successfully.");
    }
}

/// Helper function for Rust background tasks to emit events to the mobile UI.
pub fn emit_event(topic: &str, data: Value) {
    if let Some(callback) = GLOBAL_EVENT_CALLBACK.get() {
        let event = serde_json::json!({
            "topic": topic,
            "data": data,
        });
        
        let json_str = match serde_json::to_string(&event) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Failed to serialize event payload for topic '{}': {}", topic, e);
                return;
            }
        };

        match CString::new(json_str) {
            Ok(c_str) => {
                // Call the native mobile closure synchronously.
                // The mobile side MUST copy the string before returning.
                callback(c_str.as_ptr());
            }
            Err(e) => {
                tracing::error!("Event payload contained null bytes, cannot send to C-ABI: {}", e);
            }
        }
    }
}
