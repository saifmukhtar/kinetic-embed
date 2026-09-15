use crate::JsonResponse;
use crate::state::{get_network, RUNTIME};
use serde_json::Value;

/// Subscribes to a Gossipsub topic and streams live events via Server-Sent Events (SSE).
/// Subscribes to a Gossipsub topic and streams live events via the global C-Callback.
pub fn handle_gossip_subscribe(params: Option<Value>) -> JsonResponse {
    let topic = match params.as_ref().and_then(|p| p.get("topic")).and_then(|t| t.as_str()) {
        Some(t) => t.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'topic'".to_string()) },
    };

    let task_topic = topic.clone();
    RUNTIME.get().unwrap().spawn(async move {
        let mut rx = crate::state::get_gossip_tx().subscribe();
        tracing::info!("Background task subscribed to gossip topic: {}", task_topic);
        
        while let Ok((msg_topic, payload, _, _)) = rx.recv().await {
            if msg_topic == task_topic {
                // We attempt to decode it as a string first, fallback to hex.
                let data = match String::from_utf8(payload) {
                    Ok(s) => serde_json::Value::String(s),
                    Err(e) => serde_json::Value::String(hex::encode(e.into_bytes())),
                };
                crate::callback::emit_event(&task_topic, data);
            }
        }
    });

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "message": format!("Successfully subscribed to gossip topic: {}", topic)
        })),
        error: None,
    }
}

/// Broadcasts a JSON payload to a Gossipsub topic across the P2P mesh network.
pub fn handle_gossip_publish(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };
    
    let topic = match p.get("topic").and_then(|t| t.as_str()) {
        Some(t) => t.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'topic'".to_string()) },
    };

    let payload = match p.get("payload") {
        Some(p) => p.clone(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'payload'".to_string()) },
    };

    let payload_bytes = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to serialize payload: {}", e)) },
    };

    RUNTIME.get().unwrap().block_on(async {
        match get_network().broadcast_gossip(&topic, payload_bytes).await {
            Ok(_) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({
                    "message": format!("Payload successfully broadcasted to topic: {}", topic)
                })),
                error: None,
            },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Retrieves a list of active Gossipsub topics the node is currently listening to.
pub fn handle_get_gossip_topics(_params: Option<Value>) -> JsonResponse {
    RUNTIME.get().unwrap().block_on(async {
        match get_network().get_gossip_topics().await {
            Ok(topics) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({ "topics": topics })),
                error: None,
            },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}
