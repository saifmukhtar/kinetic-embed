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
pub static OWNED_NAMES_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn get_owned_names_lock() -> &'static Mutex<()> {
    OWNED_NAMES_LOCK.get_or_init(|| Mutex::new(()))
}

pub static GOSSIP_TX: OnceLock<tokio::sync::broadcast::Sender<(String, Vec<u8>, kinetic_network::libp2p::PeerId, kinetic_network::libp2p::gossipsub::MessageId)>> = OnceLock::new();

pub fn get_gossip_tx() -> tokio::sync::broadcast::Sender<(String, Vec<u8>, kinetic_network::libp2p::PeerId, kinetic_network::libp2p::gossipsub::MessageId)> {
    GOSSIP_TX.get_or_init(|| {
        let (tx, _) = tokio::sync::broadcast::channel(1000);
        tx
    }).clone()
}

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

use std::collections::HashMap;
use std::sync::Mutex;
use tokio::sync::Semaphore;
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct VdfTaskStatus {
    pub status: String,
    pub iterations: u64,
    pub progress: u64,
    pub error: Option<String>,
}

pub static VDF_TASKS: OnceLock<Arc<Mutex<HashMap<String, VdfTaskStatus>>>> = OnceLock::new();
pub static VDF_SEMAPHORE: OnceLock<Arc<Semaphore>> = OnceLock::new();

pub fn get_vdf_tasks() -> Arc<Mutex<HashMap<String, VdfTaskStatus>>> {
    VDF_TASKS.get_or_init(|| Arc::new(Mutex::new(HashMap::new()))).clone()
}

pub fn get_vdf_semaphore() -> Arc<Semaphore> {
    VDF_SEMAPHORE.get_or_init(|| Arc::new(Semaphore::new(1))).clone() // 1 concurrent VDF by default
}
