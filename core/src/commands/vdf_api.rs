//! API endpoints for consensus math, Name Difficulty Curve (NDC), and name validation.

use crate::JsonResponse;
use serde_json::Value;
use serde::{Deserialize, Serialize};

/// Protocol-level consensus requirements for a name.
#[derive(Serialize)]
pub struct ProtocolRequirements {
    /// The base required iterations to register this name.
    pub iterations: u64,
    /// The mathematical tier classification (e.g., 5_chars).
    pub ndc_tier: String,
    /// The target network baseline time for this tier in minutes.
    pub network_reference_target_minutes: u64,
    /// True if the daemon is currently running in development mode.
    pub is_dev_mode: bool,
}

/// Host-specific time prediction based on startup micro-benchmarking.
#[derive(Serialize)]
pub struct LocalPrediction {
    /// True if the host IPS was successfully calibrated.
    pub calibrated: bool,
    /// Measured Iterations Per Second (derated for sustained thermal load).
    pub host_speed_ips: u64,
    /// The exact estimated wall-clock time in seconds.
    pub estimated_seconds: u64,
    /// A human-readable formatted time estimate.
    pub estimated_formatted: String,
    /// A subjective rating of the hardware speed.
    pub hardware_rating: String,
}

/// Response returned by the pre-flight VDF iterations calculator.
#[derive(Serialize)]
pub struct IterationsResponse {
    /// The normalized name used for the calculation.
    pub name: String,
    /// The extracted apex label of the name.
    pub label: String,
    /// The character length of the label tier.
    pub label_length: usize,
    /// Protocol-level rules.
    pub protocol: ProtocolRequirements,
    /// Host-specific time prediction.
    pub local_prediction: LocalPrediction,
}

/// Query parameters for fetching idle takeover difficulty.
#[derive(Deserialize)]
pub struct TakeoverQuery {
    /// The number of Kyns the name has been idle (since the last heartbeat).
    pub kyns_idle: Option<u64>,
}

/// Response returned by the takeover iterations calculator.
#[derive(Serialize)]
pub struct TakeoverIterationsResponse {
    /// The normalized name used for the calculation.
    pub name: String,
    /// The base required iterations to register this name if it was perfectly new.
    pub base_iterations: u64,
    /// The number of Kyns the name has been idle.
    pub kyns_idle: u64,
    /// The current heavily decayed iterations required to take over the name.
    pub current_iterations: u64,
    /// The exact decay multiplier applied to the base iterations.
    pub decay_multiplier: f64,
}

/// Request to validate a potential name.
#[derive(Deserialize)]
pub struct ValidateRequest {
    /// The raw string name to validate.
    pub name: String,
}

/// Response returned after validating a name.
#[derive(Serialize)]
pub struct ValidateResponse {
    /// The original string provided.
    pub original: String,
    /// The fully canonical normalized string.
    pub normalized: String,
    /// True if the name passes all syntax, length, and LDH validation rules.
    pub is_valid: bool,
    /// True if the name is reserved and cannot be registered via normal PoW.
    pub is_reserved: bool,
    /// A human-readable error if validation failed.
    pub error: Option<String>,
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{} seconds", secs)
    } else {
        let mins = secs / 60;
        let rem = secs % 60;
        if rem == 0 {
            format!("{} minutes", mins)
        } else {
            format!("{} minutes {} seconds", mins, rem)
        }
    }
}

/// Retrieves the base iterations (required VDF iterations) to register a specific name.
pub fn handle_get_iterations(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing or invalid 'name'".to_string()) },
    };

    let normalized = kinetic_core::types::names::normalize_name(&name);
    if let Err(e) = kinetic_core::types::names::is_valid_apex_name(&normalized) {
        return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid name: {}", e)) };
    }

    let physics_params = kinetic_core::physics::NetworkPhysics::default();
    let iterations = physics_params.iterations(&normalized);
    let apex = kinetic_core::types::names::extract_apex_name(&normalized).to_string();
    let label = apex
        .strip_suffix(kinetic_core::constants::NSP_SUFFIX)
        .unwrap_or(&apex)
        .to_string();
    let label_length = label.len();

    let ndc_tier = format!("{}_chars", label_length);
    let network_reference_target_minutes = match label_length {
        0 | 1 => kinetic_core::constants::PHYSICS_NDC_LEN_0_TO_1,
        2 => kinetic_core::constants::PHYSICS_NDC_LEN_2,
        3 => kinetic_core::constants::PHYSICS_NDC_LEN_3,
        4 => kinetic_core::constants::PHYSICS_NDC_LEN_4,
        5 => kinetic_core::constants::PHYSICS_NDC_LEN_5,
        6 => kinetic_core::constants::PHYSICS_NDC_LEN_6,
        7 => kinetic_core::constants::PHYSICS_NDC_LEN_7,
        8..=10 => kinetic_core::constants::PHYSICS_NDC_LEN_8_TO_10,
        11..=17 => kinetic_core::constants::PHYSICS_NDC_LEN_11_TO_17,
        18..=20 => kinetic_core::constants::PHYSICS_NDC_LEN_18_TO_20,
        _ => kinetic_core::constants::TARGET_MINUTES as u64,
    };

    // Mobile fallback prediction baseline (approximate modern smartphone IPS)
    let host_speed_ips = 100_000;
    let estimated_seconds = iterations / std::cmp::max(host_speed_ips, 1);

    let reference_ips = (kinetic_core::constants::BASE_ITERATIONS as f64)
        / (kinetic_core::constants::TARGET_MINUTES * 60.0);

    let rating = if host_speed_ips as f64 > reference_ips * 1.4 {
        "Fast"
    } else if host_speed_ips as f64 > reference_ips * 0.7 {
        "Average"
    } else {
        "Slow"
    };

    let rating_str = format!(
        "{} (x{:.1} relative to network baseline)",
        rating,
        if reference_ips > 0.0 {
            host_speed_ips as f64 / reference_ips
        } else {
            1.0
        }
    );

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(IterationsResponse {
            name: normalized,
            label,
            label_length,
            protocol: ProtocolRequirements {
                iterations,
                ndc_tier,
                network_reference_target_minutes,
                is_dev_mode: kinetic_core::config::is_dev_mode(),
            },
            local_prediction: LocalPrediction {
                calibrated: false,
                host_speed_ips,
                estimated_seconds,
                estimated_formatted: format_duration(estimated_seconds),
                hardware_rating: rating_str,
            },
        }).unwrap()),
        error: None,
    }
}

/// Calculates the decayed takeover iterations for an idle name.
/// Requires the client to pass `?kyns_idle=X` in the query string.
pub fn handle_takeover_iterations(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };
    let name = match p.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };
    let kyns_idle = match p.get("kyns_idle").and_then(|n| n.as_u64()) {
        Some(idle) => idle,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'kyns_idle'".to_string()) },
    };

    let normalized = kinetic_core::types::names::normalize_name(&name);
    let physics_params = kinetic_core::physics::NetworkPhysics::default();
    let base_iterations = physics_params.iterations(&normalized);

    let current_iterations = physics_params.takeover_iterations(base_iterations, kyns_idle);
    let decay_multiplier = if base_iterations > 0 {
        current_iterations as f64 / base_iterations as f64
    } else {
        1.0
    };

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(TakeoverIterationsResponse {
            name: normalized,
            base_iterations,
            kyns_idle,
            current_iterations,
            decay_multiplier,
        }).unwrap()),
        error: None,
    }
}

/// Validates a potential name string according to Kinetic's core naming rules.
pub fn handle_validate_name(params: Option<Value>) -> JsonResponse {
    let req: ValidateRequest = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid params: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let normalized = kinetic_core::types::names::normalize_name(&req.name);
    let is_reserved = kinetic_core::types::names::is_reserved_name(&normalized);
        

    let res = match kinetic_core::types::names::is_valid_apex_name(&normalized) {
        Ok(_) => ValidateResponse {
            original: req.name,
            normalized,
            is_valid: true,
            is_reserved,
            error: None,
        },
        Err(e) => ValidateResponse {
            original: req.name,
            normalized,
            is_valid: false,
            is_reserved,
            error: Some(e.to_string()),
        },
    };

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(res).unwrap()),
        error: None,
    }
}
