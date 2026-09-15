use std::sync::{Arc, OnceLock};
use kinetic_storage::KineticStorage;
use kinetic_network::client::NetworkClient;
use tokio::runtime::Runtime;
use kinetic_primitives::keys::KineticKeypair;
use std::collections::HashSet;
use std::sync::RwLock;

pub static RUNTIME: OnceLock<Runtime> = OnceLock::new();
pub static STORAGE: OnceLock<Arc<KineticStorage>> = OnceLock::new();
pub static NETWORK: OnceLock<NetworkClient> = OnceLock::new();
pub static KEYPAIR: OnceLock<KineticKeypair> = OnceLock::new();
pub static ATLAS_NSPS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

pub fn get_storage() -> Arc<KineticStorage> {
    STORAGE.get().expect("Storage not initialized").clone()
}

pub fn get_network() -> NetworkClient {
    NETWORK.get().expect("Network not initialized").clone()
}

pub fn get_keypair() -> KineticKeypair {
    KEYPAIR.get().expect("Keypair not initialized").clone()
}

pub fn get_atlas_nsps() -> &'static RwLock<HashSet<String>> {
    ATLAS_NSPS.get_or_init(|| RwLock::new(HashSet::new()))
}
