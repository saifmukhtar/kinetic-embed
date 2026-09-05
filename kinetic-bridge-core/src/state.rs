use std::sync::{Arc, OnceLock};
use kinetic_storage::KineticStorage;
use kinetic_network::client::NetworkClient;
use tokio::runtime::Runtime;
use kinetic_primitives::keys::KineticKeypair;

pub static RUNTIME: OnceLock<Runtime> = OnceLock::new();
pub static STORAGE: OnceLock<Arc<KineticStorage>> = OnceLock::new();
pub static NETWORK: OnceLock<NetworkClient> = OnceLock::new();
pub static KEYPAIR: OnceLock<KineticKeypair> = OnceLock::new();

pub fn get_storage() -> Arc<KineticStorage> {
    STORAGE.get().expect("Storage not initialized").clone()
}

pub fn get_network() -> NetworkClient {
    NETWORK.get().expect("Network not initialized").clone()
}

pub fn get_keypair() -> KineticKeypair {
    KEYPAIR.get().expect("Keypair not initialized").clone()
}
