//! HTTP REST API endpoints for resolving canonical network time.
//!
//! ## Layer 8 Architecture: Time Oracle Interface
//! This module exposes a fast, synchronous endpoint for the Desktop UI to fetch 
//! the current Time Oracle epoch (KYN). Because fetching directly from the network 
//! requires an asynchronous mesh query, this endpoint strictly returns the 
//! `kinetic-storage` cached value, which is continuously kept fresh by the 
//! daemon's background Heartbeat worker.

use crate::JsonResponse;
use serde_json::Value;
use crate::state::{get_storage, not_initialized};
use kinetic_core::traits::KynProvider;
use tracing;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct KineticTime {
    pub kyn: u64,
    pub facet: u64,
    pub prism: u64,
}

impl KineticTime {
    pub fn from_kyn(current: kinetic_kyn::types::Kyn, genesis: kinetic_kyn::types::Kyn) -> Self {
        let age = current.0.saturating_sub(genesis.0);
        Self {
            kyn: age,
            facet: age / 3600,
            prism: age / 86400,
        }
    }
}

/// Returns the current verified Kinetic Time from the daemon's internal state.
pub fn handle_get_time(_params: Option<Value>) -> JsonResponse {
    let storage = match get_storage() { Some(s) => s, None => return not_initialized() };
    let kyn_provider =
        kinetic_network::client::beacon::BeaconProvider::new(Some(storage));

    // Always prefer the cache for instantaneous responses,
    // the Heartbeat loop ensures this cache is populated.
    // If the cache somehow fails, fallback to local clock estimation.
    match kyn_provider.load_cached() {
        Ok(drand_data) => {
            let time = KineticTime::from_kyn(
                kinetic_kyn::types::Kyn(drand_data.beacon_idx),
                kinetic_kyn::types::Kyn(kinetic_core::constants::KYN_GENESIS),
            );
            JsonResponse {
                status: "success".to_string(),
                data: Some(serde_json::to_value(time).unwrap()),
                error: None,
            }
        }
        Err(e) => {
            tracing::error!(
                "Failed to read cached Time Oracle kyn for /api/v1/micro/time/current: {}",
                e
            );
            // If offline, we could fallback mathematically here as well,
            // but since it's the daemon, returning an error ensures consumers
            // know the node isn't synced. The CLI implements the offline fallback.
            JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(e.to_string()),
            }
        }
    }
}
