//! HTTP REST API handlers for the Kinetic Name Registration System (NRS).
//!
//! ## Layer 8 Architecture: The Registration Gateway
//! This file is the primary ingress point for the local Desktop UI to interact with the global 
//! Kademlia DHT. It handles the highly complex multi-stage cryptographic flow of domain 
//! registration (Commit, Reveal, Verify).
//!
//! ### The Publishing Flow
//! ```text
//! [Desktop UI] -> POST /nrs/publish -> [handle_publish_record]
//!                                                |
//!                                   +------------v-------------+
//!                                   | 1. Cryptographic Verify  |
//!                                   | 2. Staleness Math Check  |
//!                                   | 3. Local DB Persistence  |
//!                                   | 4. Libp2p Swarm Injection|
//!                                   +--------------------------+
//!                                                |
//!                                       [Gossipsub / Kademlia]
//! ```

use crate::JsonResponse;
use tracing;
use crate::state::{get_network, get_storage, not_initialized, RUNTIME};
use kinetic_core::traits::KynProvider;
use kinetic_core::traits::StorageEngine;
use kinetic_core::types::vdf::RevealExt;
use kinetic_kyn::types::KynNetworkExt;
use kinetic_verify::signatures::VerifySignature;
use kinetic_kyn::types::Kyn;
use serde_json::Value;

/// Safely fetches the current Kyn using the network client, with verified local database cache fallback.
async fn get_safe_current_kyn(network: &kinetic_network::client::NetworkClient, storage: &std::sync::Arc<kinetic_storage::KineticStorage>) -> Kyn {
    if let Ok(kyn) = network.get_current_kyn().await {
        if kyn > 0 {
            return Kyn(kyn);
        }
    }

    let kyn_provider =
        kinetic_network::client::beacon::BeaconProvider::new(Some(storage.clone()));
    match kyn_provider.load_cached() {
        Ok(kyn) if kyn.kyn > 0 => Kyn(kyn.kyn),
        _ => kinetic_kyn::types::Kyn(0),
    }
}

use std::sync::Mutex;
pub static OWNED_NAMES_LOCK: Mutex<()> = Mutex::new(());

/// Injects a fully verified `Reveal` payload into the global Kademlia DHT.
pub fn handle_publish_record(params: Option<Value>) -> JsonResponse {
    let record: kinetic_types::name_record::NameEnvelope = match params.as_ref().and_then(|p| p.get("record")) {
        Some(r) => match serde_json::from_value(r.clone()) {
            Ok(rec) => rec,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid record: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'record'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        tracing::info!("Received API publish request for name: {}", record.name());

        // Normalize to canonical format
        let fqdn = kinetic_core::types::normalize_name(record.name());
        if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid name: {}", e)) };
        }

        let mut name_record = record;

        // For Standard names, we need to validate and enforce KYN Time Oracle staleness.
        // Premium names bypass VDF staleness checks.
        let mut is_standard = false;
        let mut kyn = 0;
        if let kinetic_types::name_record::NameEnvelope::Standard(ref mut reveal) = name_record {
            reveal.name = fqdn.clone();
            if let Err(e) = reveal.validate() {
                return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid Reveal: {}", e)) };
            }
            is_standard = true;
            kyn = reveal.kyn;
        }

        // Enforce Time Oracle staleness
        let network = match crate::state::get_network() { Some(n) => n, None => return not_initialized() };
        let storage = match crate::state::get_storage() { Some(s) => s, None => return not_initialized() };
        let current_kyn = get_safe_current_kyn(&network, &storage).await.0;

        if is_standard && current_kyn > 0 {
            if kyn > current_kyn {
                return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Reveal rejected: VDF kyn {} is in the future (current kyn: {}).", kyn, current_kyn)) };
            }
            let age = current_kyn - kyn;
            if age > kinetic_core::types::RESQUARING_EPOCH_KYNS {
                return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Reveal rejected: VDF kyn {} is {} kyns old (max allowed: {}). Please re-compute a fresh VDF proof.", kyn, age, kinetic_core::types::RESQUARING_EPOCH_KYNS)) };
            }
        }

        let payload_bytes = match serde_json::to_vec(&name_record) {
            Ok(b) => b,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization failed: {}", e)) },
        };
        let payload_clone = payload_bytes.clone();

        if let Err(e) = match get_network() { Some(n) => n, None => return not_initialized() }.publish_redundant_payload(&fqdn, payload_bytes).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        tracing::info!("Successfully queued payload for {} to the DHT network", fqdn);

        let storage = match get_storage() { Some(s) => s, None => return not_initialized() };
        let fqdn_persist = fqdn.clone();
        let name_record_clone = name_record.clone();

        if let Err(e) = tokio::task::spawn_blocking(move || {
            let owned_key = kinetic_core::constants::DB_PREFIX_OWNED_NAMES;
            let _lock = OWNED_NAMES_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let mut owned = Vec::new();
            if let Ok(Some(bytes)) = storage.get(owned_key) {
                if let Ok(names) = serde_json::from_slice::<Vec<String>>(&bytes) {
                    owned = names;
                }
            }
            if !owned.contains(&fqdn_persist) {
                owned.push(fqdn_persist.clone());
                if owned.len() > 10_000 {
                    let skip_count = owned.len() - 10_000;
                    owned = owned.into_iter().skip(skip_count).collect();
                }
                if let Ok(b) = serde_json::to_vec(&owned) {
                    let _ = storage.put(owned_key, &b);
                }
            }
            drop(_lock);

            let reveal_key = format!("{}{}", kinetic_core::constants::DB_PREFIX_REVEAL, fqdn_persist);
            if let Ok(reveal_bytes) = serde_json::to_vec(&name_record_clone) {
                let _ = storage.put(reveal_key.as_bytes(), &reveal_bytes);
            }
        }).await {
            tracing::warn!("Failed to persist to daemon storage: {}", e);
        } else {
            tracing::info!("Persisted {} to daemon storage for automatic Heartbeats", fqdn);
        }

        // Spawn a background task to verify quorum threshold
        let network = match get_network() { Some(n) => n, None => return not_initialized() };
        let fqdn_clone = fqdn.clone();

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            match network.verify_quorum(&fqdn_clone, payload_clone).await {
                Ok(quorum) if quorum >= 3 => {
                    tracing::info!("Quorum reached for {}: {}/5 nodes confirmed.", fqdn_clone, quorum);
                }
                Ok(quorum) => {
                    let err = kinetic_core::error::PublishError::QuorumFailed(fqdn_clone.to_string(), quorum);
                    tracing::warn!(error_code = err.code(), "{}", err);
                }
                Err(e) => {
                    let err = kinetic_core::error::PublishError::QuorumCheckError(fqdn_clone.to_string(), e.to_string());
                    tracing::warn!(error_code = err.code(), "{}", err);
                }
            }
        });

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "status": "success",
                "message": "Payload accepted and routed to DHT network."
            })),
            error: None,
        }
    })
}

/// Handles API requests to commit a name registration hash to the DHT.
///
/// # Errors
///
/// Returns an error if the name is invalid, the commitment hash is all-zeros,
/// serialization fails, or DHT publishing fails.
pub fn handle_publish_commit(params: Option<Value>) -> JsonResponse {
    let req: kinetic_core::types::CommitRequest = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid CommitRequest: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        tracing::info!("Received API commit request for name: {}", req.name);

        // Normalize to canonical format
        let fqdn = kinetic_core::types::normalize_name(&req.name);
        if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid name: {}", e)) };
        }

        if req.commitment.hash == [0u8; 32] {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("Commitment hash must not be all-zeros. Please provide a valid cryptographic commitment.".to_string()) };
        }

        let payload_bytes = match serde_json::to_vec(&req.commitment) {
            Ok(b) => b,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization failed: {}", e)) },
        };

        if let Err(e) = match get_network() { Some(n) => n, None => return not_initialized() }.publish_redundant_payload(&fqdn, payload_bytes.clone()).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        tracing::info!("Successfully queued Commitment for {} to the DHT network", fqdn);

        // Spawn a background task to verify quorum threshold
        let network = match get_network() { Some(n) => n, None => return not_initialized() };
        let fqdn_clone = fqdn.clone();

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
            match network.verify_quorum(&fqdn_clone, payload_bytes).await {
                Ok(quorum) if quorum >= 3 => tracing::info!(
                    "Quorum reached for commitment of {}: {}/5 nodes confirmed.",
                    fqdn_clone,
                    quorum
                ),
                Ok(quorum) => {
                    let err = kinetic_core::error::PublishError::CommitmentQuorumFailed(
                        fqdn_clone.to_string(),
                        quorum,
                    );
                    tracing::warn!(error_code = err.code(), "{}", err);
                }
                Err(e) => {
                    let err = kinetic_core::error::PublishError::CommitmentQuorumCheckError(
                        fqdn_clone.to_string(),
                        e.to_string(),
                    );
                    tracing::warn!(error_code = err.code(), "{}", err);
                }
            }
        });

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "status": "success",
                "message": "Commitment accepted and routed to DHT network."
            })),
            error: None,
        }
    })
}

/// Handles API requests to resolve a Kinetic name.
/// Searches the DHT and falls back to a local daemon backup if the name cannot be found on the network.
///
/// # Errors
///
/// Returns a standard Kinetic ApiError if the name is not found
/// or if resolution fails due to network offline states or data corruption.
pub fn handle_resolve_name(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);

        if kinetic_core::types::names::is_reserved_name(&fqdn) {
            let apex = kinetic_core::types::names::extract_apex_name(&fqdn);
            let apex_no_tld = apex.trim_end_matches(kinetic_core::constants::NSP_SUFFIX);
            let local_zone_file = kinetic_local::config::zones_dir()
                .join("local")
                .join(format!("{}.json", apex_no_tld));

            if let Ok(content) = tokio::fs::read_to_string(&local_zone_file).await {
                if let Ok(zone) = serde_json::from_str::<kinetic_core::types::NrsZone>(&content) {
                    let payload = serde_json::to_vec(&zone).unwrap_or_default();
                    let dummy_json = serde_json::json!({
                        "owner_kid": "reserved_local",
                        "payload": payload,
                        "signature": [],
                        "timestamp": 0
                    });
                    if let Ok(record) =
                        serde_json::from_value::<kinetic_types::name_record::NameEnvelope>(dummy_json)
                    {
                        return JsonResponse {
                            status: "success".to_string(),
                            data: Some(serde_json::to_value(record).unwrap()),
                            error: None,
                        };
                    }
                }
            }

            return JsonResponse { status: "error".to_string(), data: None, error: Some("Not Found".to_string()) };
        }

        let record = match match get_network() { Some(n) => n, None => return not_initialized() }.resolve_redundant_payload(&fqdn).await {
            Ok(payload) => {
                let record = match serde_json::from_slice::<kinetic_types::name_record::NameEnvelope>(&payload) {
                    Ok(r) => r,
                    Err(_) => return JsonResponse { status: "error".to_string(), data: None, error: Some("Invalid NameEnvelope payload on DHT".to_string()) },
                };

                let dev_mode = kinetic_core::config::is_dev_mode();
                if !dev_mode {
                    if let Err(e) = record.verify_signature(kinetic_core::constants::NETWORK_SALT) {
                        let err = kinetic_core::error::ResolutionError::SignatureVerificationFailed(
                            e.to_string(),
                        );
                        tracing::warn!(error_code = err.code(), "{}", err);
                        return JsonResponse { status: "error".to_string(), data: None, error: Some(err.to_string()) };
                    }
                }
                record
            }
            Err(kinetic_core::error::ResolutionError::NotFound { .. }) => {
                // Fallback to local storage if DHT lookup fails or returns nothing
                // This rescues users who lost their local record cache (.record.json) and the DHT dropped their record
                let reveal_key = format!("{}{}", kinetic_core::constants::DB_PREFIX_REVEAL, fqdn);
                let storage = match get_storage() { Some(s) => s, None => return not_initialized() };

                let record_bytes = match tokio::task::spawn_blocking(move || storage.get(reveal_key.as_bytes())).await {
                    Ok(Ok(Some(bytes))) => bytes,
                    _ => return JsonResponse { status: "error".to_string(), data: None, error: Some("Not Found".to_string()) },
                };

                match serde_json::from_slice::<kinetic_types::name_record::NameEnvelope>(&record_bytes) {
                    Ok(r) => r,
                    Err(_) => return JsonResponse { status: "error".to_string(), data: None, error: Some("Stored registration data is corrupted.".to_string()) },
                }
            }
            Err(e) => {
                tracing::warn!(error_code = e.code(), "Resolution error: {}", e.to_string());
                return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
            }
        };

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::to_value(record).unwrap()),
            error: None,
        }
    })
}

/// Represents the result of a DHT quorum verification check.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct QuorumResponse {
    /// The fully qualified domain name.
    pub name: String,
    /// Number of distinct peers that confirmed storage.
    pub quorum_count: usize,
}

/// Handles requests to verify DHT quorum for a specific name record payload.
pub fn handle_verify_quorum(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let name = match p.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    let record: kinetic_types::name_record::NameEnvelope = match p.get("record") {
        Some(r) => match serde_json::from_value(r.clone()) {
            Ok(rec) => rec,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid NameEnvelope payload: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'record'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);
        if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        let payload = serde_json::to_vec(&record).unwrap();

        match match get_network() { Some(n) => n, None => return not_initialized() }.verify_quorum(&fqdn, payload).await {
            Ok(count) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({
                    "name": fqdn,
                    "quorum_count": count,
                })),
                error: None,
            },
            Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        }
    })
}

/// Represents the status of a reserved name in the local network configuration.
#[derive(serde::Serialize)]
pub struct ReservedNameStatus {
    /// The reserved name (e.g., "example", "localhost").
    pub name: String,
    /// True if a local zone override file exists for this name.
    pub active: bool,
}

/// Handles API requests to get the list of reserved names and their active local status.
pub fn handle_get_reserved_names(_params: Option<Value>) -> JsonResponse {
    let local_dir = kinetic_local::config::zones_dir().join("local");
    let mut statuses = Vec::new();
    for r in kinetic_core::types::RESERVED_NAMES {
        let path = local_dir.join(format!("{}.json", r));
        statuses.push(serde_json::json!({
            "name": r.to_string(),
            "active": path.exists()
        }));
    }

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::Value::Array(statuses)),
        error: None,
    }
}

/// Handles API requests to retrieve a local zone file for a given name.
///
/// # Errors
///
/// Returns an error if the zone file does not exist or has an invalid format.
pub fn handle_get_zone(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);
        if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        let path = kinetic_local::config::zones_dir()
            .join("config")
            .join(format!("{}.json", fqdn));
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => match serde_json::from_str::<kinetic_core::types::NrsZone>(&content) {
                Ok(zone) => JsonResponse {
                    status: "success".to_string(),
                    data: Some(serde_json::to_value(zone).unwrap()),
                    error: None,
                },
                Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Parse error: {}", e)) },
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some("Not Found".to_string()),
            },
            Err(e) => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("Read failed: {}", e)),
            },
        }
    })
}

/// Handles API requests to save changes to a local zone file without broadcasting to the network.
///
pub fn handle_post_zone(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let name = match p.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    let zone: kinetic_core::types::NrsZone = match p.get("zone") {
        Some(r) => match serde_json::from_value(r.clone()) {
            Ok(rec) => rec,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid NrsZone payload: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'zone'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);
        if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        let zones_dir = kinetic_local::config::zones_dir().join("config");
        let path = zones_dir.join(format!("{}.json", fqdn));

        let content = match serde_json::to_string_pretty(&zone) {
            Ok(c) => c,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization failed: {}", e)) },
        };

        if let Err(e) = tokio::fs::create_dir_all(&zones_dir).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to create zones directory: {}", e)) };
        }

        if let Err(e) = tokio::fs::write(&path, content).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Write failed: {}", e)) };
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({ "success": true })),
            error: None,
        }
    })
}

/// Handles API requests to cryptographically sign a local zone file and publish the updated Reveal to the DHT.
///
/// # Errors
///
/// Returns an error if the zone file or the local registration record is missing/corrupted,
/// if the daemon identity key cannot be loaded, or if the DHT publish operation fails.
pub fn handle_publish_zone(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);
        if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        // 1. Read the current zone file asynchronously
        let zone_path = kinetic_local::config::zones_dir()
            .join("config")
            .join(format!("{}.json", fqdn));
        let content = match tokio::fs::read_to_string(&zone_path).await {
            Ok(c) => c,
            Err(_) => return JsonResponse { status: "error".to_string(), data: None, error: Some("Not Found".to_string()) },
        };
        let zone: kinetic_core::types::NrsZone = match serde_json::from_str(&content) {
            Ok(z) => z,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("ParseError: {}", e)) },
        };

        // 2. Load the persisted Reveal (stored at registration time)
        let reveal_key = format!("{}{}", kinetic_core::constants::DB_PREFIX_REVEAL, fqdn);
        let storage = match get_storage() { Some(s) => s, None => return not_initialized() };
        let r_key = reveal_key.clone();
        let reveal_bytes = match tokio::task::spawn_blocking(move || storage.get(r_key.as_bytes())).await {
            Ok(Ok(Some(bytes))) => bytes,
            _ => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Not registered local: {}", fqdn)) },
        };

        let mut record: kinetic_types::name_record::NameEnvelope = match serde_json::from_slice(&reveal_bytes) {
            Ok(r) => r,
            Err(_) => return JsonResponse { status: "error".to_string(), data: None, error: Some("Stored registration data is corrupted.".to_string()) },
        };

        // 3. Load the daemon keypair and re-sign with the updated payload
        let keypair = match crate::state::get_keypair() { Some(k) => k, None => return not_initialized() };
        let pubkey_bytes = keypair.as_bytes();
        if record.pubkey() != pubkey_bytes.as_slice() {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("The daemon key does not match the owner key for this name registration.".to_string()) };
        }

        let payload = match serde_json::to_vec(&zone) {
            Ok(p) => p,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("ZoneSerializationFailed: {}", e)) },
        };

        match &mut record {
            kinetic_types::name_record::NameEnvelope::Standard(r) => {
                r.embedded_nrs = payload;
                let signable = r.signable_bytes(kinetic_core::constants::NETWORK_SALT);
                r.identity_signature = keypair.sign(&signable);
            }
            kinetic_types::name_record::NameEnvelope::Prime {
                name,
                payload: p,
                signature: s,
                ..
            }
            | kinetic_types::name_record::NameEnvelope::Infra {
                name,
                payload: p,
                signature: s,
                ..
            } => {
                *p = payload.clone();
                let mut signable = Vec::new();
                signable.extend_from_slice(&(name.len() as u32).to_be_bytes());
                signable.extend_from_slice(name.as_bytes());
                signable.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                signable.extend_from_slice(&payload);
                signable.extend_from_slice(kinetic_core::constants::NETWORK_SALT);

                *s = keypair.sign(&signable);
            }
        }

        // 4. Update the stored Reveal so future zone publishes reflect the latest payload
        if let Ok(updated_bytes) = serde_json::to_vec(&record) {
            let storage = match get_storage() { Some(s) => s, None => return not_initialized() };
            let reveal_key_for_put = reveal_key.clone();
            tokio::task::spawn_blocking(move || {
                let _ = storage.put(reveal_key_for_put.as_bytes(), &updated_bytes);
            });
        }

        // 5. Serialize and publish to the DHT
        let dht_payload = match serde_json::to_vec(&record) {
            Ok(p) => p,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization error: {}", e)) },
        };

        if let Err(e) = match get_network() { Some(n) => n, None => return not_initialized() }.publish_redundant_payload(&fqdn, dht_payload).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        tracing::info!("Zone published to DHT for {}", fqdn);
        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "status": "success",
                "message": "Zone published to the Kinetic DHT network."
            })),
            error: None,
        }
    })
}

/// Handles API requests to save changes to a reserved local zone file (e.g. example.kin).
pub fn handle_post_local_zone(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let name = match p.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    let zone: kinetic_core::types::NrsZone = match p.get("zone") {
        Some(r) => match serde_json::from_value(r.clone()) {
            Ok(rec) => rec,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid NrsZone payload: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'zone'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);
        if !kinetic_core::types::names::is_reserved_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("This endpoint is strictly for reserved local names (e.g. example.kin).".to_string()) };
        }

        let apex = kinetic_core::types::names::extract_apex_name(&fqdn);
        let apex_no_tld = apex.trim_end_matches(kinetic_core::constants::NSP_SUFFIX);

        let local_dir = kinetic_local::config::zones_dir().join("local");
        if let Err(e) = tokio::fs::create_dir_all(&local_dir).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to create local zones directory: {}", e)) };
        }
        let path = local_dir.join(format!("{}.json", apex_no_tld));

        let content = match serde_json::to_string_pretty(&zone) {
            Ok(c) => c,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization failed: {}", e)) },
        };

        if let Err(e) = tokio::fs::write(&path, content).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Write failed: {}", e)) };
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({ "success": true })),
            error: None,
        }
    })
}

/// Handles API requests to delete a reserved local zone file.
pub fn handle_delete_local_zone(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);
        if !kinetic_core::types::names::is_reserved_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("This endpoint is strictly for reserved local names (e.g. example.kin).".to_string()) };
        }

        let apex = kinetic_core::types::names::extract_apex_name(&fqdn);
        let apex_no_tld = apex.trim_end_matches(kinetic_core::constants::NSP_SUFFIX);

        let path = kinetic_local::config::zones_dir()
            .join("local")
            .join(format!("{}.json", apex_no_tld));

        match tokio::fs::remove_file(&path).await {
            Ok(_) => JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::json!({ "success": true })),
                error: None,
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some("Not Found".to_string()),
            },
            Err(e) => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("File delete failed: {}", e)),
            },
        }
    })
}

/// Handles API requests to retrieve a reserved local zone file.
pub fn handle_get_local_zone(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::normalize_name(&name);

        if !kinetic_core::types::names::is_reserved_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("This endpoint is strictly for reserved local names (e.g. example.kin).".to_string()) };
        }

        let apex = kinetic_core::types::names::extract_apex_name(&fqdn);
        let apex_no_tld = apex.trim_end_matches(kinetic_core::constants::NSP_SUFFIX);

        let path = kinetic_local::config::zones_dir()
            .join("local")
            .join(format!("{}.json", apex_no_tld));

        match tokio::fs::read_to_string(&path).await {
            Ok(content) => match serde_json::from_str::<kinetic_core::types::NrsZone>(&content) {
                Ok(zone) => JsonResponse {
                    status: "success".to_string(),
                    data: Some(serde_json::to_value(zone).unwrap()),
                    error: None,
                },
                Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Parse error: {}", e)) },
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some("Not Found".to_string()),
            },
            Err(e) => JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("Read failed: {}", e)),
            },
        }
    })
}

/// Request payload for manually broadcasting a Fat NRS Zone update.
#[derive(serde::Deserialize)]
pub struct FatZoneRequest {
    /// The private key of the delegated hot key, hex encoded.
    pub hot_key_hex: String,
    /// The master-key authorized delegation proof.
    pub authorized_manifest: kinetic_core::types::identity::AuthorizedManifest,
    /// The new DNS zone data.
    pub zone: kinetic_core::types::NrsZone,
}

/// Publishes a Fat NRS NameEnvelope (Zone Update) using a delegated hot key.
pub fn handle_publish_fat_zone(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let name = match p.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    let req: FatZoneRequest = match serde_json::from_value(p) {
        Ok(r) => r,
        Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid FatZoneRequest: {}", e)) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let fqdn = kinetic_core::types::names::normalize_name(&name);
        if let Err(e) = kinetic_core::types::names::is_valid_apex_name(&fqdn) {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        // 1. Verify the capability is present in the manifest
        let has_cap = req
            .authorized_manifest
            .manifest
            .services
            .iter()
            .any(|s| s.service_type == "kinetic.capability.dns_update");
        if !has_cap {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("Provided AuthorizedManifest does not contain kinetic.capability.dns_update capability.".to_string()) };
        }

        // 2. Load the hot key
        let hot_key_bytes = match hex::decode(&req.hot_key_hex) {
            Ok(b) => b,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid hot_key_hex: {}", e)) },
        };
        let keypair = match kinetic_primitives::keys::KineticKeypair::from_slice(&hot_key_bytes) {
            Ok(k) => k,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid ML-DSA keypair: {}", e)) },
        };

        // 3. Load the persisted Reveal (stored at registration time) to retain the valid VDF proof
        let reveal_key = format!("{}{}", kinetic_core::constants::DB_PREFIX_REVEAL, fqdn);
        let storage = match get_storage() { Some(s) => s, None => return not_initialized() };
        let r_key = reveal_key.clone();
        let reveal_bytes = match tokio::task::spawn_blocking(move || storage.get(r_key.as_bytes())).await {
            Ok(Ok(Some(bytes))) => bytes,
            _ => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Not registered local: {}", fqdn)) },
        };

        let mut record: kinetic_types::name_record::NameEnvelope = match serde_json::from_slice(&reveal_bytes) {
            Ok(r) => r,
            Err(_) => return JsonResponse { status: "error".to_string(), data: None, error: Some("Stored registration data is corrupted.".to_string()) },
        };

        // 4. Update payload and authorization, then sign with hot key
        let payload_bytes = match serde_json::to_vec(&req.zone) {
            Ok(b) => b,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("ZoneSerializationFailed: {}", e)) },
        };

        match &mut record {
            kinetic_types::name_record::NameEnvelope::Standard(reveal) => {
                reveal.embedded_nrs = payload_bytes;
                reveal.authorization = Some(Box::new(req.authorized_manifest));
                let signable = reveal.signable_bytes(kinetic_core::constants::NETWORK_SALT);
                reveal.identity_signature = tokio::task::spawn_blocking(move || keypair.sign(&signable)).await.unwrap();
            }
            kinetic_types::name_record::NameEnvelope::Prime {
                name,
                payload,
                authorization,
                signature,
                ..
            }
            | kinetic_types::name_record::NameEnvelope::Infra {
                name,
                payload,
                authorization,
                signature,
                ..
            } => {
                *payload = payload_bytes;
                *authorization = Some(Box::new(req.authorized_manifest));

                let mut signable = Vec::new();
                signable.extend_from_slice(&(name.len() as u32).to_be_bytes());
                signable.extend_from_slice(name.as_bytes());
                signable.extend_from_slice(&(payload.len() as u32).to_be_bytes());
                signable.extend_from_slice(payload);
                signable.extend_from_slice(kinetic_core::constants::NETWORK_SALT);

                *signature = tokio::task::spawn_blocking(move || keypair.sign(&signable)).await.unwrap();
            }
        }

        // 5. Save the updated reveal locally so the daemon serves the newest zone on fallback
        let final_bytes = serde_json::to_vec(&record).unwrap();
        let network = match get_network() { Some(n) => n, None => return not_initialized() };

        let s2 = match get_storage() { Some(s) => s, None => return not_initialized() };
        let fqdn2 = fqdn.clone();
        let fb = final_bytes.clone();
        tokio::task::spawn_blocking(move || {
            let _ = s2.put(reveal_key.as_bytes(), &fb);
        });

        if let Err(e) = network.publish_redundant_payload(&fqdn2, final_bytes).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "status": "success",
                "message": format!("Fat Zone payload successfully broadcast for {}", fqdn)
            })),
            error: None,
        }
    })
}
