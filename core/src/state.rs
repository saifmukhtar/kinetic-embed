// All imports consolidated at the top — no mid-file imports.
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use kinetic_storage::KineticStorage;
use kinetic_network::client::NetworkClient;
use tokio::runtime::Runtime;
use tokio::sync::Semaphore;
use kinetic_primitives::keypairs::IdentityPrivKey;

// ── Core singletons ─────────────────────────────────────────────────────────

pub static RUNTIME: OnceLock<Runtime> = OnceLock::new();
pub static STORAGE: OnceLock<Arc<KineticStorage>> = OnceLock::new();
pub static NETWORK: OnceLock<NetworkClient> = OnceLock::new();
pub static KEYPAIR: OnceLock<IdentityPrivKey> = OnceLock::new();

/// Abort handle for the `NetworkEventLoop::run()` background task.
/// Stored during `init_kinetic` so that `handle_shutdown` can cancel it cleanly.
pub static NETWORK_LOOP_HANDLE: OnceLock<tokio::task::AbortHandle> = OnceLock::new();

// ── Gossip broadcast channel ─────────────────────────────────────────────────

/// The real gossip sender is set by `init_kinetic` after wiring the NetworkEventLoop.
/// We do NOT use `get_or_init` here — that would create a dangling channel that
/// would cause `OnceLock::set` in bootstrap to silently fail, discarding the real
/// sender that is wired to the P2P swarm.
pub static GOSSIP_TX: OnceLock<
    tokio::sync::broadcast::Sender<(
        String,
        Vec<u8>,
        kinetic_network::MessageId,
        kinetic_network::PeerId,
    )>,
> = OnceLock::new();

/// Returns the gossip sender, or `None` if `init_kinetic` has not been called yet.
/// Callers must handle `None` gracefully — do not panic.
pub fn get_gossip_tx() -> Option<
    tokio::sync::broadcast::Sender<(
        String,
        Vec<u8>,
        kinetic_network::MessageId,
        kinetic_network::PeerId,
    )>,
> {
    GOSSIP_TX.get().cloned()
}

// ── Safe accessor helpers ─────────────────────────────────────────────────────
//
// All accessors return `Option` instead of panicking with `.expect()`.
// Every command that calls these checks for `None` and returns a proper
// error JsonResponse to the mobile app instead of crashing the process.

pub fn get_storage() -> Option<Arc<KineticStorage>> {
    STORAGE.get().cloned()
}

pub fn get_network() -> Option<NetworkClient> {
    NETWORK.get().cloned()
}

pub fn get_keypair() -> Option<IdentityPrivKey> {
    KEYPAIR.get().cloned()
}

// ── Misc singletons ───────────────────────────────────────────────────────────

pub static ATLAS_NSPS: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();

pub fn get_atlas_nsps() -> &'static RwLock<HashSet<String>> {
    ATLAS_NSPS.get_or_init(|| RwLock::new(HashSet::new()))
}

/// Mutex used to serialize writes to the owned-names list in RocksDB.
pub static OWNED_NAMES_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn get_owned_names_lock() -> &'static Mutex<()> {
    OWNED_NAMES_LOCK.get_or_init(|| Mutex::new(()))
}

// ── VDF task tracking ─────────────────────────────────────────────────────────

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
    VDF_TASKS
        .get_or_init(|| Arc::new(Mutex::new(HashMap::new())))
        .clone()
}

pub fn get_vdf_semaphore() -> Arc<Semaphore> {
    // 1 concurrent VDF proof at a time on mobile to avoid thermal throttling.
    VDF_SEMAPHORE
        .get_or_init(|| Arc::new(Semaphore::new(1)))
        .clone()
}

// ── Guard helpers ─────────────────────────────────────────────────────────────

/// Returns a standard "not initialized" JsonResponse.
/// Used by every command that calls get_network()/get_storage() etc.
/// so error handling is consistent across the entire bridge.
pub fn not_initialized() -> crate::JsonResponse {
    crate::JsonResponse {
        status: "error".to_string(),
        data: None,
        error: Some("Kinetic not initialized. Call init_kinetic first.".to_string()),
    }
}
