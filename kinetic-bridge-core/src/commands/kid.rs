use serde_json::Value;
use crate::JsonResponse;
use crate::state::{get_storage, get_network, RUNTIME};
use kinetic_local::config::get_base_dir;
use kinetic_core::traits::KynProvider;
use kinetic_core::types::clock::KynNetworkExt;

pub fn handle_list_kids() -> JsonResponse {
    match kinetic_local::kid_manager::list_local_kids() {
        Ok(summaries) => JsonResponse {
            status: "success".to_string(),
            data: Some(serde_json::json!({ "kids": summaries })),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Failed to list KIDs: {}", e)),
        }
    }
}

pub fn handle_fetch_kid(params: Option<Value>) -> JsonResponse {
    let name = match params.and_then(|p| p.get("name").and_then(|n| n.as_str().map(|s| s.to_string()))) {
        Some(n) => n,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing name".to_string()) }
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
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Failed to fetch KID: {}", e)),
        }
    }
}

pub fn handle_generate_kid(params: Option<Value>) -> JsonResponse {
    let p = match params {
        Some(v) => v,
        None => return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing params".to_string()) }
    };
    
    let base_name = p.get("base_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if base_name.is_empty() {
        return JsonResponse { status: "error".to_string(), data: None, error: Some("Missing base_name".to_string()) };
    }
    
    let sub_name = p.get("sub_name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let inherit_subname = p.get("inherit_subname").and_then(|v| v.as_bool()).unwrap_or(true);
    let force = p.get("force").and_then(|v| v.as_bool()).unwrap_or(false);

    let base_fqdn = kinetic_core::types::normalize_name(&base_name);
    let final_name = if let Some(sub) = sub_name {
        format!("{}.{}", sub, base_fqdn)
    } else {
        base_fqdn
    };

    let storage = get_storage();
    let network = get_network();
    let rt = RUNTIME.get().expect("Runtime not initialized");

    let kyn_provider = kinetic_network::client::drand::DrandProvider::new(Some(storage.clone()));
    
    let current_kyn = match rt.block_on(kyn_provider.fetch_latest()) {
        Ok(kyn) => kinetic_core::types::Kyn(kyn.kyn),
        Err(_) => kinetic_core::types::Kyn::now_local(),
    };

    let identity_path = get_base_dir().join("identity.key");
    let res = match kinetic_local::kid_manager::get_or_create_kid_for_name(
        &final_name,
        inherit_subname,
        force,
        current_kyn,
        &identity_path,
    ) {
        Ok(r) => r,
        Err(e) => return JsonResponse { status: "error".to_string(), data: None, error: Some(format!("Failed to generate KID: {}", e)) }
    };

    if let Ok(payload_bytes) = serde_json::to_vec(&res.auth_kid) {
        let _ = rt.block_on(network.publish_redundant_payload(&res.did, payload_bytes));
    }

    JsonResponse {
        status: "success".to_string(),
        data: Some(serde_json::json!({
            "name": res.name,
            "did": res.did,
            "is_inherited": res.is_inherited,
            "kid_doc": res.kid_doc
        })),
        error: None,
    }
}
