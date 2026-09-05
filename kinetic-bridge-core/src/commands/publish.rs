use serde_json::Value;
use crate::JsonResponse;
use crate::state::{get_storage, get_network, RUNTIME};
use kinetic_core::traits::{StorageEngine, KynProvider};
use kinetic_core::types::clock::KynNetworkExt;

pub fn handle_publish_action(params: Option<Value>) -> JsonResponse {
    let msg = match params {
        Some(v) => match serde_json::from_value::<kinetic_core::governance::SignedGovernanceMessage>(v) {
            Ok(m) => m,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid SignedGovernanceMessage payload: {}", e)) }
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) }
    };

    let storage = get_storage();
    let network = get_network();
    let rt = RUNTIME.get().expect("Runtime not initialized");

    let current_kyn = {
        let kyn_provider = kinetic_network::client::drand::DrandProvider::new(Some(storage.clone()));
        match kyn_provider.load_cached_kyn() {
            Ok(kyn) => kyn.kyn,
            Err(_) => match rt.block_on(kyn_provider.fetch_latest()) {
                Ok(kyn) => kyn.kyn,
                Err(_) => kinetic_core::types::Kyn::now_local().0,
            },
        }
    };

    let is_valid = {
        let mut gov = kinetic_local::governance::GLOBAL_GOVERNANCE_STATE.lock().unwrap();
        match kinetic_core::governance::process_governance_message(
            &mut gov,
            &msg,
            kinetic_types::clock::Kyn(current_kyn),
        ) {
            Ok(_) => {
                let path = kinetic_local::config::get_base_dir().join("governance.bin");
                let _ = kinetic_local::governance::save_governance_to_disk(&gov, &path);
                true
            }
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid governance message: {}", e)) }
        }
    };

    if !is_valid {
        return JsonResponse { status: "error".to_string(), data: None, error: Some("Governance message validation failed".to_string()) };
    }

    let payload_bytes = serde_json::to_vec(&msg).unwrap();
    let mut envelope = vec![kinetic_types::network::NetworkOpcode::Governance as u8];
    envelope.extend(payload_bytes);

    match rt.block_on(network.broadcast_gossip(kinetic_core::constants::GOSSIP_TOPIC_GLOBAL, envelope)) {
        Ok(_) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "message": "Governance action accepted and routed to P2P network"
            })),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Failed to broadcast: {}", e)),
        }
    }
}
