//! HTTP REST API handlers for managing Cryptographic Kinetic Identities (KIDs).
//!
//! ## Layer 8 Architecture: The Identity Manager
//! A Kinetic Identity (KID) is a serialized Sovereign keypair that proves ownership 
//! of specific namespaces. This module handles all local operations relating to 
//! identity management: derivation from seed phrases, exporting to disk, and 
//! cryptographically signing `AuthorizedKid` payloads to delegate trust on the DHT.

use crate::JsonResponse;
use crate::state::{get_network, get_storage, RUNTIME};
use serde_json::Value;
use kinetic_core::traits::{KynProvider, StorageEngine};
use kinetic_core::types::Kyn;
use kinetic_core::types::clock::KynNetworkExt;
use serde::Deserialize;
use tracing;

/// Safely fetches the current Kyn using the network client, with verified local database cache fallback.
async fn get_safe_current_kyn() -> Kyn {
    if let Ok(kyn) = get_network().get_current_kyn().await {
        if kyn > 0 {
            return Kyn(kyn);
        }
    }

    let kyn_provider =
        kinetic_network::client::drand::DrandProvider::new(Some(get_storage()));
    match kyn_provider.load_cached() {
        Ok(kyn) if kyn.kyn > 0 => Kyn(kyn.kyn),
        _ => Kyn::now_local(),
    }
}

/// Request payload to generate or resolve a KID
#[derive(Deserialize)]
pub struct GenerateKidRequest {
    /// The base domain name (e.g., example.kin)
    pub base_name: String,
    /// An optional subname (e.g., admin)
    pub sub_name: Option<String>,
    /// Whether to inherit the apex KID when creating a subname (default true)
    #[serde(default = "default_inherit")]
    pub inherit_subname: bool,
    /// Force overwrite existing local keys (default false)
    #[serde(default)]
    pub force: bool,
}

fn default_inherit() -> bool {
    true
}

/// Handles API requests to list all local KID documents stored on the filesystem.
pub fn handle_list_kids(_params: Option<Value>) -> JsonResponse {
    match kinetic_local::kid_manager::list_local_kids() {
        Ok(summaries) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({ "kids": summaries })),
            error: None,
        },
        Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
    }
}

/// Handles API requests to retrieve a specific local KID document by its domain name.
pub fn handle_fetch_kid(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    match kinetic_local::kid_manager::load_local_kid(&name) {
        Ok((doc, path)) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "name": kinetic_core::types::normalize_name(&name),
                "kid_doc": doc,
                "path": path.to_string_lossy(),
            })),
            error: None,
        },
        Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
    }
}

/// Handles API requests to generate a new KID document, keypair, and publish it.
pub fn handle_generate_kid(params: Option<Value>) -> JsonResponse {
    let req: GenerateKidRequest = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid GenerateKidRequest: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let base_fqdn = kinetic_core::types::normalize_name(&req.base_name);
    let final_name = if let Some(sub) = req.sub_name {
        format!("{}.{}", sub, base_fqdn)
    } else {
        base_fqdn
    };

    RUNTIME.get().unwrap().block_on(async {
        let current_kyn = get_safe_current_kyn().await;

        let identity_path = kinetic_local::config::get_base_dir().join("identity.key");
        let res = match kinetic_local::kid_manager::get_or_create_kid_for_name(
            &final_name,
            req.inherit_subname,
            req.force,
            current_kyn,
            &identity_path,
        ) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        };

        // Publish AuthorizedKid wrapper to DHT
        match serde_json::to_vec(&res.auth_kid) {
            Ok(payload_bytes) => {
                if let Err(e) = get_network()
                    .publish_redundant_payload(&res.did, payload_bytes)
                    .await
                {
                    tracing::warn!(did = %res.did, error = %e, "Failed to publish generated KID to DHT");
                }
            }
            Err(e) => {
                tracing::warn!(did = %res.did, error = %e, "Failed to serialize generated KID for DHT");
            }
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "success": true,
                "name": res.name,
                "did": res.did,
                "is_inherited": res.is_inherited,
                "kid_doc": res.kid_doc
            })),
            error: None,
        }
    })
}

/// Handles API requests to rotate the keys of a local KID document and publish the update.
pub fn handle_rotate_kid(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let identity_path = kinetic_local::config::get_base_dir().join("identity.key");
        let rotated = match kinetic_local::kid_manager::rotate_name_kid(&name, &identity_path) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        };

        // Publish rotated document to DHT
        match serde_json::to_vec(&rotated.auth_kid) {
            Ok(payload_bytes) => {
                if let Err(e) = get_network()
                    .publish_redundant_payload(&rotated.did, payload_bytes)
                    .await
                {
                    tracing::warn!(did = %rotated.did, error = %e, "Failed to publish rotated KID to DHT");
                }
            }
            Err(e) => {
                tracing::warn!(did = %rotated.did, error = %e, "Failed to serialize rotated KID for DHT");
            }
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "success": true,
                "name": rotated.name,
                "did": rotated.did,
                "kid_doc": rotated.kid_doc
            })),
            error: None,
        }
    })
}

/// Handles API requests to revoke (deactivate) a local KID document and publish the revocation.
pub fn handle_revoke_kid(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let revoked_doc = match kinetic_local::kid_manager::revoke_local_kid(&name) {
            Ok(d) => d,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        };

        let identity_path = kinetic_local::config::get_base_dir().join("identity.key");
        let auth_kid = match kinetic_local::kid_manager::authorize_kid_document(&name, &revoked_doc, &identity_path) {
            Ok(a) => a,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        };

        // Publish revoked document to DHT
        match serde_json::to_vec(&auth_kid) {
            Ok(payload_bytes) => {
                if let Err(e) = get_network()
                    .publish_redundant_payload(revoked_doc.kid.as_str(), payload_bytes)
                    .await
                {
                    tracing::warn!(did = %revoked_doc.kid, error = %e, "Failed to publish revoked KID to DHT");
                }
            }
            Err(e) => {
                tracing::warn!(did = %revoked_doc.kid, error = %e, "Failed to serialize revoked KID for DHT");
            }
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "success": true,
                "name": kinetic_core::types::normalize_name(&name),
                "did": revoked_doc.kid.as_str(),
                "deactivated": true,
                "kid_doc": revoked_doc
            })),
            error: None,
        }
    })
}

/// Retrieves the locally stored Manifest for a given identity name if present.
pub fn handle_get_kid_manifest(params: Option<Value>) -> JsonResponse {
    let name = match params.as_ref().and_then(|p| p.get("name")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    match kinetic_local::kid_manager::load_local_manifest(&name) {
        Ok(manifest) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "name": kinetic_core::types::normalize_name(&name),
                "manifest": manifest,
            })),
            error: None,
        },
        Err(e) => JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
    }
}

/// Request payload for updating an identity's capability manifest.
#[derive(serde::Deserialize)]
pub struct UpdateManifestRequest {
    /// List of service entries to publish in the manifest.
    pub services: Vec<kinetic_kid::manifest::Service>,
}

/// Creates, signs, persists, and publishes a new version of the capability manifest for an identity.
pub fn handle_update_kid_manifest(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(p) => p,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    let name = match p.get("name").and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'name'".to_string()) },
    };

    let req: UpdateManifestRequest = match p.get("payload") {
        Some(payload) => match serde_json::from_value(payload.clone()) {
            Ok(r) => r,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid UpdateManifestRequest: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'payload'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let current_kyn = get_safe_current_kyn().await;

        let identity_path = kinetic_local::config::get_base_dir().join("identity.key");
        let (manifest, auth_manifest) = match kinetic_local::kid_manager::save_and_sign_local_manifest(
            &name,
            req.services,
            current_kyn,
            &identity_path,
        ) {
            Ok(m) => m,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        };

        // Publish to DHT under hex(sha256(did#manifest))
        let manifest_key = hex::encode(kinetic_primitives::sha256_hash(
            format!("{}#manifest", manifest.kid).as_bytes(),
        ));

        match serde_json::to_vec(&auth_manifest) {
            Ok(payload_bytes) => {
                if let Err(e) = get_network()
                    .publish_redundant_payload(&manifest_key, payload_bytes)
                    .await
                {
                    tracing::warn!(manifest_key = %manifest_key, error = %e, "Failed to publish manifest to DHT");
                }
            }
            Err(e) => {
                tracing::warn!(manifest_key = %manifest_key, error = %e, "Failed to serialize manifest for DHT");
            }
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "success": true,
                "name": kinetic_core::types::normalize_name(&name),
                "manifest": manifest,
            })),
            error: None,
        }
    })
}

/// Handles API requests to resolve a Kinetic Identifier (KID) and its associated manifest.
///
/// # Errors
///
/// Returns an error if the KID cannot be found in the DHT, or if the retrieved payload is invalid.
pub fn handle_resolve_kid(params: Option<Value>) -> JsonResponse {
    let did = match params.as_ref().and_then(|p| p.get("did")).and_then(|n| n.as_str()) {
        Some(n) => n.to_string(),
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing 'did'".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        tracing::info!("Resolving KID via API: {}", did);

        // Resolve KID - Notice how this entire massive match block is now just a single `?`
        let kid_payload = match get_network().resolve_redundant_payload(&did).await {
            Ok(p) => p,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) },
        };

        let kid_doc: kinetic_kid::Document =
            match serde_json::from_slice::<kinetic_core::types::AuthorizedKid>(&kid_payload) {
                Ok(auth) => auth.kid_doc,
                Err(_) => {
                    // Fallback for older raw documents
                    match serde_json::from_slice(&kid_payload) {
                        Ok(d) => d,
                        Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid KID data payload: {}", e)) },
                    }
                }
            };

        // Try to resolve Manifest
        let manifest_key = hex::encode(kinetic_primitives::sha256_hash(
            format!("{}#manifest", did).as_bytes(),
        ));

        let mut res = serde_json::json!({
            "kid_document": kid_doc,
        });

        if let Ok(man_payload) = get_network().resolve_redundant_payload(&manifest_key).await {
            let manifest_opt =
                match serde_json::from_slice::<kinetic_core::types::AuthorizedManifest>(&man_payload) {
                    Ok(auth) => Some(auth.manifest),
                    Err(_) => serde_json::from_slice::<kinetic_kid::Manifest>(&man_payload).ok(),
                };

            if let Some(manifest) = manifest_opt {
                if let Ok(val) = serde_json::to_value(manifest) {
                    res["manifest_document"] = val;
                }
            }
        }

        JsonResponse {
            status: "success".to_string(),
            data: Some(res),
            error: None,
        }
    })
}

/// Handles requests to publish an AuthorizedKID to the DHT/Gossip network.
pub fn handle_publish_kid(params: Option<Value>) -> JsonResponse {
    let auth_kid: kinetic_core::types::AuthorizedKid = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(k) => k,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid AuthorizedKid: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        tracing::info!(
            "Received API publish request for KID: {}",
            auth_kid.kid_doc.kid.as_str()
        );

        // 1. Verify the underlying KID document mathematically
        if let Err(e) = auth_kid.kid_doc.verify() {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid KID signature: {}", e)) };
        }

        // 1b. Verify the wrapper signature against the registered name's Reveal locally.
        // If it fails here, we reject early and don't spam the DHT.
        let reveal_key = format!(
            "{}{}",
            kinetic_core::constants::DB_PREFIX_REVEAL,
            auth_kid.name
        );
        let is_authorized = match get_storage().get(reveal_key.as_bytes()) {
            Ok(Some(bytes)) => {
                if let Ok(record) = serde_json::from_slice::<kinetic_core::types::NameRecord>(&bytes) {
                    kinetic_primitives::verify_mldsa(
                        record.pubkey(),
                        &auth_kid.signable_bytes(kinetic_core::constants::NETWORK_SALT),
                        &auth_kid.owner_signature,
                    )
                    .is_ok()
                } else {
                    false
                }
            }
            _ => {
                let err = kinetic_core::error::PublishError::MissingLocalRevealForKid(auth_kid.name.clone());
                tracing::warn!(error_code = err.code(), "{}", err);
                true // If we don't have it cached, we let the network decide.
            }
        };

        if !is_authorized {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("Invalid authorization signature. The AuthorizedKid must be signed by the name's owner.".to_string()) };
        }

        // 2. Serialize and Publish to DHT
        let payload_bytes = match serde_json::to_vec(&auth_kid) {
            Ok(b) => b,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization failed: {}", e)) },
        };
        let fqdn = auth_kid.kid_doc.kid.as_str().to_string(); // Use DID as the DHT key

        if let Err(e) = get_network().publish_redundant_payload(&fqdn, payload_bytes).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        tracing::info!("Successfully published KID {} to the DHT", fqdn);
        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "status": "success",
                "message": "AuthorizedKID accepted and routed to DHT"
            })),
            error: None,
        }
    })
}

/// Handles API requests to publish an `AuthorizedManifest` to the DHT.
///
/// # Errors
///
/// Returns an error if the local owner signature check fails, the corresponding KID Document
/// cannot be resolved or verified, or if publishing to the DHT fails.
pub fn handle_publish_manifest(params: Option<Value>) -> JsonResponse {
    let auth_manifest: kinetic_core::types::AuthorizedManifest = match params {
        Some(p) => match serde_json::from_value(p) {
            Ok(m) => m,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid AuthorizedManifest: {}", e)) },
        },
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) },
    };

    RUNTIME.get().unwrap().block_on(async {
        let did_str = auth_manifest.manifest.kid.as_str();
        tracing::info!(
            "Received API publish request for Manifest of KID: {}",
            did_str
        );

        // 1b. Verify the wrapper signature against the registered name's Reveal locally.
        let reveal_key = format!(
            "{}{}",
            kinetic_core::constants::DB_PREFIX_REVEAL,
            auth_manifest.name
        );
        let is_authorized = match get_storage().get(reveal_key.as_bytes()) {
            Ok(Some(bytes)) => {
                if let Ok(record) = serde_json::from_slice::<kinetic_core::types::NameRecord>(&bytes) {
                    kinetic_primitives::verify_mldsa(
                        record.pubkey(),
                        &auth_manifest.signable_bytes(kinetic_core::constants::NETWORK_SALT),
                        &auth_manifest.owner_signature,
                    )
                    .is_ok()
                } else {
                    false
                }
            }
            _ => {
                let err = kinetic_core::error::PublishError::MissingLocalRevealForManifest(
                    auth_manifest.name.clone(),
                );
                tracing::warn!(error_code = err.code(), "{}", err);
                true
            }
        };

        if !is_authorized {
            return JsonResponse { status: "error".to_string(), data: None, error: Some("Invalid authorization signature. The AuthorizedManifest must be signed by the name's owner.".to_string()) };
        }

        // 1. Resolve the KID Document from DHT to verify against
        // (Note: The DHT payload for a KID will now be an AuthorizedKid wrapper!)
        let kid_payload = match get_network().resolve_redundant_payload(did_str).await {
            Ok(p) => p,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to resolve KID payload: {}", e)) },
        };

        let kid_doc: kinetic_kid::Document =
            match serde_json::from_slice::<kinetic_core::types::AuthorizedKid>(&kid_payload) {
                Ok(auth_kid) => auth_kid.kid_doc,
                Err(_) => {
                    // Fallback for older raw Documents if any exist
                    match serde_json::from_slice::<kinetic_kid::Document>(&kid_payload) {
                        Ok(d) => d,
                        Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid KID payload on DHT: {}", e)) },
                    }
                }
            };

        // 2. Verify the manifest against the registered KID using network time
        let current_network_time = get_safe_current_kyn().await.to_network_utime().0;
        if let Err(e) = auth_manifest
            .manifest
            .verify_at_time(&kid_doc, current_network_time)
        {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Invalid Manifest signature: {}", e)) };
        }

        // 3. Serialize and Publish to DHT under the derived manifest key
        let manifest_key = hex::encode(kinetic_primitives::sha256_hash(
            format!("{}#manifest", did_str).as_bytes(),
        ));

        let payload_bytes = match serde_json::to_vec(&auth_manifest) {
            Ok(b) => b,
            Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Serialization failed: {}", e)) },
        };

        if let Err(e) = get_network().publish_redundant_payload(&manifest_key, payload_bytes).await {
            return JsonResponse { status: "error".to_string(), data: None, error: Some(e.to_string()) };
        }

        tracing::info!("Successfully published Manifest for {} to the DHT", did_str);
        JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({
                "status": "success",
                "message": "Manifest accepted and routed to DHT"
            })),
            error: None,
        }
    })
}
