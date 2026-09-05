use serde_json::Value;
use crate::JsonResponse;
use kinetic_core::consensus_math::ConsensusParams;

pub fn handle_get_difficulty(params: Option<Value>) -> JsonResponse {
    let name = match params.and_then(|p| p.get("name").and_then(|n| n.as_str().map(|s| s.to_string()))) {
        Some(n) => n,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) }
    };
    
    let normalized = kinetic_core::types::normalize_name(&name);
    let params_consensus = ConsensusParams::default();
    let iterations = params_consensus.iterations(&normalized);
    let apex = kinetic_core::types::names::extract_apex_name(&normalized).to_string();
    let label = apex.strip_suffix(kinetic_core::constants::NSP_SUFFIX).unwrap_or(&apex).to_string();
    let label_length = label.len();
    
    let ndc_tier = format!("{}_chars", label_length);
    let network_reference_target_minutes = (iterations / 100_000) / 60;
    
    // Assume average mobile IPS for the prediction (e.g., 45,000)
    let host_speed_ips = 45_000;
    let estimated_seconds = iterations / std::cmp::max(host_speed_ips, 1);
    
    let rating_str = format!("Mobile (x{:.1} relative to network baseline)", host_speed_ips as f64 / 100_000.0);

    let res = serde_json::json!({
        "name": normalized,
        "label": label,
        "label_length": label_length,
        "protocol": {
            "iterations": iterations,
            "ndc_tier": ndc_tier,
            "network_reference_target_minutes": network_reference_target_minutes,
            "is_dev_mode": kinetic_core::config::is_dev_mode(),
        },
        "local_prediction": {
            "calibrated": false,
            "host_speed_ips": host_speed_ips,
            "estimated_seconds": estimated_seconds,
            "estimated_formatted": format!("{} seconds", estimated_seconds),
            "hardware_rating": rating_str,
        }
    });

    JsonResponse { status: "success".to_string(), data: Some(res), error: None }
}

pub fn handle_validate_name(params: Option<Value>) -> JsonResponse {
    let name = match params.and_then(|p| p.get("name").and_then(|n| n.as_str().map(|s| s.to_string()))) {
        Some(n) => n,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) }
    };

    let normalized = kinetic_core::types::normalize_name(&name);
    let is_reserved = kinetic_core::types::names::is_reserved_name(&normalized);
    
    match kinetic_core::types::names::is_valid_apex_name(&normalized) {
        Ok(_) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "original": name,
                "normalized": normalized,
                "is_valid": true,
                "is_reserved": is_reserved,
            })),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: Some(serde_json::json!({
                "original": name,
                "normalized": normalized,
                "is_valid": false,
                "is_reserved": is_reserved,
                "error": e.to_string(),
            })),
            error: None,
        }
    }
}
