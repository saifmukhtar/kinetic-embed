use serde::Serialize;
use serde_json::Value;
use kinetic_core::types::Heartbeat;
use kinetic_core::constants;
use kinetic_core::traits::StorageEngine;
use crate::JsonResponse;
use crate::state::{get_storage, get_network, get_keypair, RUNTIME};

#[derive(Serialize)]
pub struct HeartbeatStatusResponse {
    pub name: String,
    pub status: String,
    pub latest_kyn: u64,
    pub kyns_idle: u64,
}

#[derive(Serialize)]
pub struct HeartbeatsResponse {
    pub current_kyn: u64,
    pub names: Vec<HeartbeatStatusResponse>,
}

pub fn handle_get_heartbeats() -> JsonResponse {
    let storage = get_storage();
    let network = get_network();
    let rt = RUNTIME.get().expect("Runtime not initialized");

    let owned_key = constants::DB_PREFIX_OWNED_NAMES;
    let owned_names: Vec<String> = match storage.get(owned_key) {
        Ok(Some(bytes)) => serde_json::from_slice(&bytes).unwrap_or_default(),
        _ => Vec::new(),
    };

    let current_kyn = rt.block_on(network.get_current_kyn()).unwrap_or(0);

    let mut handles = Vec::new();
    for name in owned_names {
        let network_clone = network.clone();
        let name_clone = name.clone();
        handles.push(rt.spawn(async move {
            let res = network_clone.resolve_heartbeat(&name_clone).await;
            (name_clone, res)
        }));
    }

    let mut statuses = Vec::new();
    for handle in handles {
        if let Ok((name, network_res)) = rt.block_on(handle) {
            match network_res {
                Ok(bytes) => {
                    if let Ok(hb) = serde_json::from_slice::<Heartbeat>(&bytes) {
                        let age = current_kyn.saturating_sub(hb.latest_kyn);
                        let status = if age <= 200 {
                            "Active"
                        } else if age <= 28800 {
                            "Stale"
                        } else {
                            "Idle"
                        };
                        statuses.push(HeartbeatStatusResponse {
                            name,
                            status: status.to_string(),
                            latest_kyn: hb.latest_kyn,
                            kyns_idle: age,
                        });
                    } else {
                        statuses.push(HeartbeatStatusResponse {
                            name,
                            status: "Unknown (Parse Error)".to_string(),
                            latest_kyn: 0,
                            kyns_idle: 0,
                        });
                    }
                }
                Err(_) => {
                    statuses.push(HeartbeatStatusResponse {
                        name,
                        status: "Idle (Not Found on DHT)".to_string(),
                        latest_kyn: 0,
                        kyns_idle: 0,
                    });
                }
            }
        }
    }

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(HeartbeatsResponse {
            current_kyn,
            names: statuses,
        }).unwrap()),
        error: None,
    }
}

pub fn handle_post_heartbeat(params: Option<Value>) -> JsonResponse {
    let name = match params.and_then(|p| p.get("name").and_then(|n| n.as_str().map(|s| s.to_string()))) {
        Some(n) => n,
        None => return JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some("Missing 'name' in params".to_string()),
        }
    };

    let network = get_network();
    let keypair = get_keypair();
    let rt = RUNTIME.get().expect("Runtime not initialized");

    let current_kyn = rt.block_on(network.get_current_kyn()).unwrap_or(0);
    
    let mut heartbeat = Heartbeat {
        name: name.clone(),
        latest_kyn: current_kyn,
        signature: vec![],
        authorization: None,
    };

    let signable_bytes = heartbeat.signable_bytes(constants::NETWORK_SALT);
    
    let sig_bytes = rt.block_on(tokio::task::spawn_blocking(move || keypair.sign(&signable_bytes)))
        .unwrap();
    heartbeat.signature = sig_bytes;

    let payload = serde_json::to_vec(&heartbeat).unwrap();
    
    match rt.block_on(network.publish_heartbeat(&name, payload)) {
        Ok(_) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "message": format!("Manually broadcasted heartbeat for {}", name),
                "kyn": current_kyn
            })),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Publish failed: {}", e)),
        }
    }
}
