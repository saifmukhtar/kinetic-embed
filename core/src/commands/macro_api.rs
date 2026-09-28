//! HTTP REST API endpoints and background task workers for VDF generation workflows.
//!
//! ## Layer 8 Architecture: Asynchronous UI Task Management
//! Because generating a Verifiable Delay Function (VDF) for a Standard Domain Registration 
//! takes significant wall-clock time (potentially hours depending on the difficulty), the 
//! UI cannot simply block on an HTTP request.
//!
//! This module implements the **Macro API Pattern**:
//! 1. The user's Desktop UI sends a POST request to initiate a heavy cryptographic workflow.
//! 2. The endpoint immediately spins up a detached `tokio::spawn` worker to execute the math.
//! 3. The endpoint returns a unique `task_id` to the UI instantly (HTTP 202 Accepted).
//! 4. The detached worker computes the VDF in the background, updating a thread-safe `Arc<Mutex>` 
//!    status map at each cryptographic milestone.
//! 5. The UI periodically polls `/api/v1/macro/tasks/{task_id}` to display a real-time progress bar.

use crate::JsonResponse;
use serde_json::Value;
use crate::state::{get_vdf_tasks, get_vdf_semaphore, get_storage, get_network, not_initialized, RUNTIME, VdfTaskStatus};
use kinetic_core::traits::StorageEngine;
use tracing;

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Payload for renewing a registered Kinetic name via VDF.
#[derive(Deserialize)]
pub struct NameRenewRequest {
    /// The name to renew.
    pub name: String,
    /// Optional overridden iteration count.
    pub iterations: Option<u64>,
}

/// Payload for registering a new Kinetic name via VDF.
#[derive(Deserialize)]
pub struct VdfRegisterRequest {
    /// The name to register.
    pub name: String,
    /// Optional overridden iteration count.
    pub iterations: Option<u64>,
}

/// Handles API requests to initiate a backgkyn VDF registration task.
/// Ensures that only one VDF task is actively running.
///
/// # Errors
///
/// Returns an error if a VDF task is already running.
pub fn handle_macro_register_name(params: Option<Value>) -> JsonResponse {
    let req: VdfRegisterRequest = match params.as_ref() {
        Some(p) => match serde_json::from_value(p.clone()) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) }
    };

    let fqdn = kinetic_core::types::normalize_name(&req.name);
    if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
        return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
    }

    const MAX_USER_ITERATIONS: u64 = 10_000_000;
    if req.iterations.unwrap_or(0) > MAX_USER_ITERATIONS {
        return JsonResponse { status: "error".to_string(), data: None, error: Some("Max iterations exceeded".to_string()) };
    }
    let task_id = uuid::Uuid::new_v4().to_string();

    let tasks_arc = get_vdf_tasks();
    // Store initial task state, ensuring only 1 is active
    {
        let mut tasks = tasks_arc.lock().unwrap_or_else(|e| e.into_inner());

        let active_tasks = tasks
            .values()
            .filter(|t| t.progress < 100 && t.error.is_none())
            .count();
        if active_tasks >= 1 {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Registration already in progress for {}", fqdn)) };
        }

        tasks.retain(|_, t| t.progress < 100 && t.error.is_none());

        if tasks.len() >= 50 {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("Too many tasks".to_string()) };
        }

        tasks.insert(
            task_id.clone(),
            VdfTaskStatus {
                status: "Initializing".to_string(),
                iterations: req.iterations.unwrap_or(4_194_304), // Default lower for testing in UI
                progress: 0,
                error: None,
            },
        );
    }

    // Spawn blocking backgkyn task
    let tasks_clone = get_vdf_tasks();
    let network_clone = match get_network() { Some(n) => n, None => return not_initialized() };
    let storage_clone = match get_storage() { Some(s) => s, None => return not_initialized() };
    let task_id_clone = task_id.clone();
    let iterations = req.iterations.unwrap_or(4_194_304);

    RUNTIME.get().unwrap().spawn(async move {
        // Step 1: KYN Time Oracle
        update_task_status(&tasks_clone, &task_id_clone, "Fetching KYN Time Oracle", 10);
        let kyn_provider: std::sync::Arc<dyn kinetic_core::traits::KynProvider> = std::sync::Arc::new(
            kinetic_network::client::beacon::BeaconProvider::new(Some(storage_clone.clone())),
        );
        let drand_data = match kyn_provider.load_cached() {
            Ok(data) => data,
            Err(e) => {
                update_task_error(&tasks_clone, &task_id_clone, format!("KYN Time Oracle error: {}", e));
                return;
            }
        };

        // Step 2: Commitment — generate privately now; broadcast AFTER the VDF proof exists.
        //
        // Option B (C-1 fix): The old flow broadcast the commitment first, then computed the VDF.
        // For any name whose VDF takes longer than the commitment prune window the reveal always
        // arrived to a dead commitment. By deferring the broadcast until the proof is in hand the
        // commitment is always at most ~32 seconds old when the reveal lands — fixing C-1 for
        // every name length, including 2-4 char names whose VDFs take days to months.
        //
        // Anti-front-running is fully preserved: the commitment hash is
        // SHA-256(name‖salt‖randomness‖pubkey) — opaque to any observer during the 32-second
        // window before the reveal appears.
        update_task_status(&tasks_clone, &task_id_clone, "Generating Commitment", 20);
        let identity_path = kinetic_local::config::base_dir().join("identity.key");
        let keypair = match kinetic_local::identity::load_keypair(&identity_path) {
            Ok(k) => k,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Keypair error: {}", e),
                );
                return;
            }
        };
        let pubkey = keypair.to_pubkey();
        let mut salt = [0u8; 32];
        salt[0..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        salt[16..32].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        let sig_bytes = match hex::decode(&drand_data.signature) {
            Ok(b) => b,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Failed to decode Time Oracle signature: {}", e),
                );
                return;
            }
        };

        let challenge = kinetic_core::types::Commitment::derive(
            kinetic_core::constants::NETWORK_SALT,
            &fqdn,
            &salt,
            &sig_bytes,
            &pubkey,
        );

        // Step 3: VDF Evaluation (Blocking)
        update_task_status(
            &tasks_clone,
            &task_id_clone,
            "Computing VDF... (this may take a while)",
            30,
        );
        let required_iters =
            kinetic_core::physics::NetworkPhysics::default().iterations(&fqdn);
        let actual_iterations = std::cmp::max(iterations, required_iters);

        let vdf_engine = kinetic_vdf::RsaVdfEngine::new();
        let challenge_clone = challenge.clone();

        let permit_res = get_vdf_semaphore().acquire_owned().await;
        if permit_res.is_err() {
            update_task_error(&tasks_clone, &task_id_clone, "VDF Semaphore closed".into());
            return;
        }
        let permit = permit_res.unwrap();

        // Spawn blocking to not starve tokio executor
        let proof = match tokio::task::spawn_blocking(move || {
            use kinetic_core::traits::VdfEngine;
            vdf_engine.evaluate(&challenge_clone, actual_iterations)
        })
        .await
        {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("VDF engine error: {}", e),
                );
                return;
            }
            Err(e) => {
                update_task_error(&tasks_clone, &task_id_clone, format!("Task panic: {}", e));
                return;
            }
        };

        drop(permit);

        // Broadcast commitment now that the proof exists (Option B / C-1 fix).
        // Commitment age will be ~32 s when the reveal lands — well within any prune window.
        update_task_status(&tasks_clone, &task_id_clone, "Broadcasting Commitment", 85);
        let commit_bytes = match serde_json::to_vec(&challenge) {
            Ok(b) => b,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Commitment serialization error: {}", e),
                );
                return;
            }
        };
        if let Err(e) = network_clone
            .publish_redundant_payload(&fqdn, commit_bytes)
            .await
        {
            update_task_error(
                &tasks_clone,
                &task_id_clone,
                format!("DHT Commit Error: {}", e),
            );
            return;
        }

        // Commit age rule was removed from VDF protocol. No need to sleep.

        update_task_status(&tasks_clone, &task_id_clone, "Publishing Registration", 90);

        // Generate or fetch KID for the user to attach to the new zone
        update_task_status(&tasks_clone, &task_id_clone, "Injecting Identity (KID)", 92);
        let current_kyn = {
            let kyn_provider =
                kinetic_network::client::beacon::BeaconProvider::new(Some(storage_clone.clone()));
            use kinetic_core::traits::KynProvider;
            match kyn_provider.load_cached() {
                Ok(kyn) => kyn.beacon_idx,
                Err(_) => 0,
            }
        };
        let current_kyn = kinetic_kyn::types::Kyn(current_kyn);
        let identity_path = kinetic_local::config::base_dir().join("identity.key");

        let kid_id = match kinetic_local::kid_manager::get_or_create_kid_for_name(
            &fqdn,
            true,
            false,
            current_kyn,
            &identity_path,
        ) {
            Ok(res) => res.did,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Failed to generate identity: {}", e),
                );
                return;
            }
        };

        // Construct Reveal and Zone with KID injected
        let mut records = HashMap::new();
        records.insert(
            "@".to_string(),
            vec![kinetic_types::nrs::NrsEntry::KID(kid_id)],
        );
        let zone = kinetic_core::types::NrsZone { records };
        let payload = match serde_json::to_vec(&zone) {
            Ok(b) => b,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Serialization failed in VDF task: {}", e),
                );
                tracing::error!(
                    error = ?kinetic_core::error::NrsError::ParseError(e),
                    "Serialization failed in VDF task"
                );
                return;
            }
        };

        let mut reveal = kinetic_types::vdf::Reveal {
            protocol_version: 1,
            name: fqdn.clone(),
            embedded_nrs: payload,
            salt,
            kyn: kinetic_kyn::types::TargetKyn(kinetic_kyn::types::Kyn(drand_data.beacon_idx)),
            beacon_signature: drand_data.signature.clone(),
            iterations: actual_iterations,
            vdf_proof: kinetic_core::types::VdfProof {
                proof_bytes: proof.proof_bytes,
            },
            pubkey: kinetic_primitives::keypairs::IdentityPubKey(pubkey.0.clone()),
            identity_signature: kinetic_primitives::keypairs::IdentitySignature(vec![]),
            authorization: None,
            previous_proof: None,
        };

        let signable = reveal.signable_bytes(kinetic_core::constants::NETWORK_SALT);
        reveal.identity_signature = keypair.sign(&signable);

        // Publish to Network
        let reveal_bytes = match serde_json::to_vec(&reveal) {
            Ok(b) => b,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Serialization failed in VDF task: {}", e),
                );
                tracing::error!(
                    error = ?kinetic_core::error::NrsError::ParseError(e),
                    "Serialization failed in VDF task"
                );
                return;
            }
        };
        if let Err(e) = network_clone
            .publish_redundant_payload(&fqdn, reveal_bytes)
            .await
        {
            update_task_error(
                &tasks_clone,
                &task_id_clone,
                format!("DHT Publish Error: {}", e),
            );
            return;
        }

        // Save to internal storage so Dashboard can see it
        let fqdn_clone = fqdn.clone();
        let _lock = crate::state::get_owned_names_lock()
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut owned = Vec::new();
        if let Ok(Some(bytes)) = storage_clone.get(kinetic_core::constants::DB_PREFIX_OWNED_NAMES)
            && let Ok(names) = serde_json::from_slice::<Vec<String>>(&bytes)
        {
            owned = names;
        }
        if !owned.contains(&fqdn_clone) {
            owned.push(fqdn_clone.clone());
            if owned.len() > 10_000 {
                let skip_count = owned.len() - 10_000;
                owned = owned.into_iter().skip(skip_count).collect();
            }
            if let Ok(b) = serde_json::to_vec(&owned) {
                let _ = storage_clone.put(kinetic_core::constants::DB_PREFIX_OWNED_NAMES, &b);
            }
        }
        drop(_lock);

        // Save default zone file
        let zones_dir = kinetic_local::config::zones_dir().join("config");
        let _ = std::fs::create_dir_all(&zones_dir);
        let path = zones_dir.join(format!("{}.json", fqdn));
        if let Ok(s) = serde_json::to_string_pretty(&zone)
            && let Err(e) = std::fs::write(&path, s)
        {
            tracing::warn!(
                error = ?kinetic_core::error::StorageError::WriteFailed(e.to_string()),
                "Failed to write zone file"
            );
        }

        update_task_status(&tasks_clone, &task_id_clone, "Complete", 100);
    });

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "task_id": task_id,
            "message": "VDF generation started"
        })),
        error: None,
    }
}

/// Handles API requests to renew a Kinetic name via a new VDF proof, leveraging an existing reveal.
///
/// # Errors
///
/// Returns an error if there are issues finding the previous reveal or scheduling the VDF task.
pub fn handle_macro_renew_name(params: Option<Value>) -> JsonResponse {
    let req: NameRenewRequest = match params.as_ref() {
        Some(p) => match serde_json::from_value(p.clone()) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) }
    };

    let fqdn = kinetic_core::types::normalize_name(&req.name);
    if let Err(e) = kinetic_core::types::is_valid_apex_name(&fqdn) {
        return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
    }

    const MAX_USER_ITERATIONS: u64 = 10_000_000;
    if req.iterations.unwrap_or(0) > MAX_USER_ITERATIONS {
        return JsonResponse { status: "error".to_string(), data: None, error: Some("Max iterations exceeded".to_string()) };
    }
    let tasks_arc = get_vdf_tasks();
    let mut tasks = tasks_arc.lock().unwrap_or_else(|e| e.into_inner());

    tasks.retain(|_, t| t.progress < 100 && t.error.is_none());
    if tasks.len() >= 50 {
        return JsonResponse { status: "error".to_string(), data: None, error: Some("Too many tasks".to_string()) };
    }

    let task_id = uuid::Uuid::new_v4().to_string();
    let initial_task = VdfTaskStatus {
        status: "Starting Renewal...".into(),
        progress: 0,
        error: None,
        iterations: req.iterations.unwrap_or(4_194_304),
    };
    tasks.insert(task_id.clone(), initial_task.clone());
    drop(tasks);

    let tasks_clone = get_vdf_tasks();
    let network_clone = match get_network() { Some(n) => n, None => return not_initialized() };
    let storage_clone = match get_storage() { Some(s) => s, None => return not_initialized() };
    let task_id_clone = task_id.clone();
    let iterations = req.iterations.unwrap_or(4_194_304);

    RUNTIME.get().unwrap().spawn(async move {
        // Step 1: Read previous Reveal from database storage
        update_task_status(&tasks_clone, &task_id_clone, "Loading previous reveal", 5);
        let local_reveal_key = format!("{}{}", kinetic_core::constants::DB_PREFIX_REVEAL, fqdn);
        let old_reveal_bytes = match storage_clone.get(local_reveal_key.as_bytes()) {
            Ok(Some(b)) => b,
            _ => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Name {} not found locally", fqdn),
                );
                return;
            }
        };
        let old_record: kinetic_types::name_record::NameEnvelope = match serde_json::from_slice(
            &old_reveal_bytes,
        ) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!(
                    error = ?kinetic_core::error::StorageError::DeserializationFailed(e.to_string()),
                    "{}",
                    kinetic_core::error::StorageError::DeserializationFailed(e.to_string()).user_message()
                );
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Local Reveal corrupted: {}", e),
                );
                return;
            }
        };
        let old_reveal = match old_record {
            kinetic_types::name_record::NameEnvelope::Standard(r) => r,
        };

        // Step 2: KYN Time Oracle
        update_task_status(&tasks_clone, &task_id_clone, "Fetching KYN Time Oracle", 10);
        let kyn_provider: std::sync::Arc<dyn kinetic_core::traits::KynProvider> = std::sync::Arc::new(
            kinetic_network::client::beacon::BeaconProvider::new(Some(storage_clone.clone())),
        );
        let drand_data = match kyn_provider.load_cached() {
            Ok(d) => d,
            Err(e) => {
                update_task_error(&tasks_clone, &task_id_clone, format!("KYN Time Oracle error: {}", e));
                return;
            }
        };

        // Step 3: Commitment — generate privately; broadcast AFTER VDF (Option B / C-1 fix).
        update_task_status(&tasks_clone, &task_id_clone, "Generating Commitment", 20);
        let identity_path = kinetic_local::config::base_dir().join("identity.key");
        let keypair = match kinetic_local::identity::load_keypair(&identity_path) {
            Ok(k) => k,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Keypair error: {}", e),
                );
                return;
            }
        };
        let pubkey_bytes = keypair.to_pubkey();
        let mut salt = [0u8; 32];
        salt[0..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        salt[16..32].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
        let sig_bytes = match hex::decode(&drand_data.signature) {
            Ok(b) => b,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Failed to decode Time Oracle signature: {}", e),
                );
                return;
            }
        };

        let challenge = kinetic_core::types::Commitment::derive(
            kinetic_core::constants::NETWORK_SALT,
            &fqdn,
            &salt,
            &sig_bytes,
            &pubkey_bytes,
        );

        // Step 4: VDF Evaluation (Blocking)
        update_task_status(
            &tasks_clone,
            &task_id_clone,
            "Computing Renewal VDF... (this may take a while)",
            30,
        );

        let required_iters =
            kinetic_core::physics::NetworkPhysics::default().iterations(&fqdn);
        // Renewals get an 80% discount
        let discounted_iters = (required_iters as f64 * 0.2) as u64;
        let actual_iterations = std::cmp::max(iterations, discounted_iters);

        let vdf_engine = kinetic_vdf::RsaVdfEngine::new();
        let challenge_clone = challenge.clone();

        let permit_res = get_vdf_semaphore().acquire_owned().await;
        if permit_res.is_err() {
            update_task_error(&tasks_clone, &task_id_clone, "VDF Semaphore closed".into());
            return;
        }
        let permit = permit_res.unwrap();

        let proof = match tokio::task::spawn_blocking(move || {
            use kinetic_core::traits::VdfEngine;
            vdf_engine.evaluate(&challenge_clone, actual_iterations)
        })
        .await
        {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("VDF engine error: {}", e),
                );
                return;
            }
            Err(e) => {
                update_task_error(&tasks_clone, &task_id_clone, format!("Task panic: {}", e));
                return;
            }
        };

        drop(permit);

        // Broadcast commitment now that the renewal proof exists (Option B / C-1 fix).
        update_task_status(&tasks_clone, &task_id_clone, "Broadcasting Commitment", 85);
        let commit_bytes = match serde_json::to_vec(&challenge) {
            Ok(b) => b,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Commitment serialization error: {}", e),
                );
                return;
            }
        };
        if let Err(e) = network_clone
            .publish_redundant_payload(&fqdn, commit_bytes)
            .await
        {
            update_task_error(
                &tasks_clone,
                &task_id_clone,
                format!("DHT Commit Error: {}", e),
            );
            return;
        }

        // Commit age rule was removed from VDF protocol. No need to sleep.

        update_task_status(&tasks_clone, &task_id_clone, "Publishing Renewal", 90);

        let previous_proof = kinetic_core::types::PreviousProof {
            salt: old_reveal.salt,
            kyn: old_reveal.kyn,
            beacon_signature: old_reveal.beacon_signature.clone(),
            iterations: old_reveal.iterations,
            vdf_proof: old_reveal.vdf_proof.clone(),
            identity_signature: old_reveal.identity_signature.clone(),
        };

        let mut new_reveal = kinetic_types::vdf::Reveal {
            protocol_version: 1,
            name: fqdn.clone(),
            embedded_nrs: old_reveal.embedded_nrs.clone(), // Keep existing zone payload
            salt,
            kyn: kinetic_kyn::types::TargetKyn(kinetic_kyn::types::Kyn(drand_data.beacon_idx)),
            beacon_signature: drand_data.signature.clone(),
            iterations: actual_iterations,
            vdf_proof: kinetic_core::types::VdfProof {
                proof_bytes: proof.proof_bytes,
            },
            pubkey: kinetic_primitives::keypairs::IdentityPubKey(pubkey_bytes.0.clone()),
            identity_signature: kinetic_primitives::keypairs::IdentitySignature(vec![]),
            authorization: None,
            previous_proof: Some(previous_proof),
        };

        let signable = new_reveal.signable_bytes(kinetic_core::constants::NETWORK_SALT);
        new_reveal.identity_signature = keypair.sign(&signable);

        let reveal_bytes = match serde_json::to_vec(&new_reveal) {
            Ok(b) => b,
            Err(e) => {
                update_task_error(
                    &tasks_clone,
                    &task_id_clone,
                    format!("Serialization failed in VDF task: {}", e),
                );
                return;
            }
        };
        if let Err(e) = network_clone
            .publish_redundant_payload(&fqdn, reveal_bytes.clone())
            .await
        {
            update_task_error(
                &tasks_clone,
                &task_id_clone,
                format!("DHT Publish Error: {}", e),
            );
            return;
        }

        let local_reveal_key = format!("{}{}", kinetic_core::constants::DB_PREFIX_REVEAL, fqdn);
        let _ = storage_clone.put(local_reveal_key.as_bytes(), &reveal_bytes);

        update_task_status(&tasks_clone, &task_id_clone, "Complete", 100);
    });

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "task_id": task_id,
            "message": "Renewal VDF generation started"
        })),
        error: None,
    }
}

pub(crate) fn update_task_status(
    tasks: &Arc<Mutex<HashMap<String, VdfTaskStatus>>>,
    id: &str,
    status: &str,
    progress: u64,
) {
    if let Ok(mut map) = tasks.lock()
        && let Some(task) = map.get_mut(id)
    {
        task.status = status.to_string();
        task.progress = progress;
    }
    crate::callback::emit_event("macro_status_update", serde_json::json!({"task_id": id, "status": status, "progress": progress}));
}

pub(crate) fn update_task_error(
    tasks: &Arc<Mutex<HashMap<String, VdfTaskStatus>>>,
    id: &str,
    err: String,
) {
    if let Ok(mut map) = tasks.lock()
        && let Some(task) = map.get_mut(id)
    {
        task.error = Some(err.clone());
        task.status = "Failed".to_string();
    }
    crate::callback::emit_event("macro_error", serde_json::json!({"task_id": id, "error": err}));
}

/// Retrieves all running or recently completed VDF tasks.
pub fn handle_macro_tasks(_params: Option<Value>) -> JsonResponse {
    let tasks_arc = get_vdf_tasks();
    let tasks = {
        let map = tasks_arc.lock().unwrap_or_else(|e| e.into_inner());
        map.clone()
    };
    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::to_value(tasks).unwrap_or_default()),
        error: None,
    }
}

/// Retrieves the current progress and status of a VDF task by ID.
pub fn handle_macro_status(params: Option<Value>) -> JsonResponse {
    let task_id = match params.as_ref().and_then(|p| p.get("task_id")).and_then(|t| t.as_str()) {
        Some(t) => t.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing task_id".to_string()) },
    };
    
    let tasks_arc = get_vdf_tasks();
    let task = {
        let tasks = tasks_arc.lock().unwrap_or_else(|e| e.into_inner());
        tasks.get(&task_id).cloned()
    };

    match task {
        Some(t) => JsonResponse { status: "success".to_string(), data: Some(serde_json::to_value(t).unwrap_or_default()), error: None },
        None => JsonResponse { status: "error".to_string(), data: None, error: Some("Task not found".to_string()) },
    }
}
