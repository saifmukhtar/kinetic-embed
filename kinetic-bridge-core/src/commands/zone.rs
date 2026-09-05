use serde_json::Value;
use crate::JsonResponse;
use crate::state::{get_storage, get_network, get_keypair, RUNTIME};
use kinetic_core::traits::StorageEngine;

pub fn handle_get_zone(params: Option<Value>) -> JsonResponse {
    let name = match params.and_then(|p| p.get("name").and_then(|n| n.as_str().map(|s| s.to_string()))) {
        Some(n) => n,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing name".to_string()) }
    };
    let storage = get_storage();
    let fqdn = kinetic_core::types::normalize_name(&name);
    let reveal_key = format!("{}{}", kinetic_core::constants::DB_PREFIX_REVEAL, fqdn);

    match storage.get(reveal_key.as_bytes()) {
        Ok(Some(bytes)) => {
            if let Ok(record) = serde_json::from_slice::<kinetic_core::types::NameRecord>(&bytes) {
                let payload = match record {
                    kinetic_core::types::NameRecord::Standard(ref r) => &r.payload,
                    kinetic_core::types::NameRecord::Prime { ref payload, .. } => payload,
                    kinetic_core::types::NameRecord::Infra { ref payload, .. } => payload,
                };
                if let Ok(zone) = serde_json::from_slice::<kinetic_core::types::NrsZone>(payload) {
                    return JsonResponse { status: "success".to_string(), data: Some(serde_json::to_value(zone).unwrap()), error: None };
                }
            }
        }
        _ => {}
    }
    JsonResponse { status: "error".to_string(), data: None, error: Some("Zone not found locally".to_string()) }
}
