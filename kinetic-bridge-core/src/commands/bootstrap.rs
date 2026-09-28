//! Bootstrap command for mobile FFI.
//!
//! When the Android/iOS app launches, it calls `init_kinetic` once.
//! This function:
//! 1. Builds the Tokio multi-thread runtime and stores it in `RUNTIME`.
//! 2. Opens the RocksDB storage engine and stores it in `STORAGE`.
//! 3. Loads or generates the user's `KineticKeypair` and stores it in `KEYPAIR`.
//! 4. Boots the libp2p `NetworkEventLoop`, connects to bootstrap nodes, and stores
//!    the resulting `NetworkClient` in `NETWORK`.
//!
//! This must be called BEFORE any other bridge command.

use crate::JsonResponse;
use crate::state::{RUNTIME, STORAGE, NETWORK, KEYPAIR, GOSSIP_TX};
use serde_json::Value;
use std::sync::Arc;
use kinetic_network::{NetworkConfig, NetworkEventLoop, NetworkMode};
use kinetic_storage::KineticStorage;
use kinetic_core::traits::KynProvider;

/// Mobile-compatible init params. The data_dir is the only required field —
/// it should be the app's private data directory (e.g. Android `getFilesDir()`).
#[derive(serde::Deserialize, Debug)]
pub struct InitParams {
    /// Absolute path to the app's private data directory.
    /// e.g. `/data/user/0/com.kinetic.app/files`
    pub data_dir: String,
    /// Optional: override the network mode. "LightNode" or "FullNode" (default).
    pub network_mode: Option<String>,
    /// Optional: override TCP listen port (default 0 = OS assigned).
    pub listen_port: Option<u16>,
}

/// Initializes the entire Kinetic engine.
/// This is the FIRST call the native app should make after installing the callback.
/// It is idempotent — if called a second time it returns early with a success status.
pub fn handle_init_kinetic(params: Option<Value>) -> JsonResponse {
    // Idempotent guard — if already initialized, return immediately.
    if RUNTIME.get().is_some() {
        return JsonResponse {
            status: "ok".to_string(),
            data: Some(serde_json::json!({"message": "Kinetic already initialized"})),
            error: None,
        };
    }

    let p: InitParams = match params.as_ref() {
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(r) => r,
            Err(e) => return JsonResponse {
                status: "error".to_string(),
                data: None,
                error: Some(format!("Invalid init params: {}", e)),
            },
        },
        None => return JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some("init_kinetic requires params.data_dir".to_string()),
        },
    };

    // Step 1: Install the Rustls crypto provider (required for TLS on mobile).
    // Ignore error if it was already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();

    // Step 2: Build the Tokio multi-thread runtime.
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("kinetic-worker")
        .build()
    {
        Ok(r) => r,
        Err(e) => return JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(format!("Failed to build Tokio runtime: {}", e)),
        },
    };

    // Run all async initialization inside the new runtime.
    let result: Result<String, String> = rt.block_on(async {
        let data_dir = std::path::PathBuf::from(&p.data_dir);
        std::fs::create_dir_all(&data_dir)
            .map_err(|e| format!("Failed to create data_dir: {}", e))?;

        // Step 3: Open RocksDB storage.
        let storage_path = data_dir.join("kinetic.db");
        let storage = Arc::new(
            KineticStorage::new(storage_path)
                .map_err(|e| format!("Failed to open storage: {}", e))?,
        );

        // Step 4: Load or generate the keypair.
        let identity_path = data_dir.join("identity.key");
        let keypair = match kinetic_local::identity::load_keypair(&identity_path) {
            Ok(k) => k,
            Err(_) => {
                // First run — generate and persist a new identity.
                let new_kp = kinetic_primitives::keys::KineticKeypair::generate();
                // Persist raw seed bytes securely.
                if let Err(e) = kinetic_local::secure_fs::write_secret(
                    &identity_path,
                    &new_kp.to_bytes(),
                ) {
                    tracing::warn!("Could not persist new identity: {}", e);
                }
                new_kp
            }
        };

        // Step 5: Fetch initial Kyn from the Time Oracle (optional — falls back to 0).
        let kyn_provider: Arc<dyn KynProvider> = Arc::new(
            kinetic_network::client::beacon::BeaconProvider::new(Some(storage.clone())),
        );
        let initial_kyn = match kyn_provider.fetch_latest().await {
            Ok(k) => {
                tracing::info!("KYN Time Oracle connected — kyn #{}", k.kyn);
                k.kyn
            }
            Err(e) => {
                tracing::warn!("KYN unavailable on startup (offline?): {}. Proceeding with kyn=0.", e);
                0
            }
        };

        // Step 6: Mine the P2P keypair (CPU-bound, ~1 second on mobile).
        let local_key = kinetic_network::pow::mine_p2p_keypair(
            kinetic_types::clock::Kyn(initial_kyn),
            kinetic_core::constants::POW_DIFFICULTY_BITS,
        );

        // Step 7: Set up the Kyn broadcast watch channel.
        let (_kyn_tx, kyn_rx) = tokio::sync::watch::channel(initial_kyn);

        // Step 8: Build the gossip broadcast channel and store the sender.
        let (gossip_tx, _gossip_rx) = tokio::sync::broadcast::channel::<(
            String,
            Vec<u8>,
            kinetic_network::libp2p::gossipsub::MessageId,
            kinetic_network::libp2p::PeerId,
        )>(256);
        // Store gossip sender for gossip_subscribe to use.
        let _ = GOSSIP_TX.set(gossip_tx.clone());

        // Step 9: Build the network config.
        let mode = match p.network_mode.as_deref() {
            Some("LightNode") => NetworkMode::Edge,
            _ => NetworkMode::Router,
        };
        let port = p.listen_port.unwrap_or(0);
        let listen_addrs = vec![
            format!("/ip4/0.0.0.0/tcp/{}", port).parse().unwrap(),
        ];

        let network_config = NetworkConfig {
            mode,
            listen_addrs,
            quic_listen_addrs: vec![
                format!("/ip4/0.0.0.0/udp/{}/quic-v1", port).parse().unwrap(),
            ],
            bootstrap_nodes: kinetic_core::constants::BOOTSTRAP_NODES
                .iter()
                .filter_map(|s| s.parse().ok())
                .collect(),
            seed_domain: vec![],
            enable_mdns: false,   // mDNS is LAN-only; not useful on mobile data
            enable_upnp: false,   // UPnP not available on mobile NATs
            enable_relay_server: false,
            initial_kyn,
            external_address: None,
            max_reveals_per_hour: 100,
            lru_cache_size: std::num::NonZeroUsize::new(1_000).unwrap(),
            disable_pow: false,
            test_mode: false,
            disable_storage_sync: false,
        };

        let vdf_engine: Arc<dyn kinetic_core::traits::VdfEngine> =
            Arc::new(kinetic_vdf::RsaVdfEngine::new());

        // Step 10: Boot the NetworkEventLoop.
        let (network_client, network_loop) = NetworkEventLoop::new(
            network_config,
            local_key,
            storage.clone(),
            kyn_rx,
            None,                     // no incoming mpsc channel needed on mobile
            Some(gossip_tx),          // wire up gossip broadcast
            vdf_engine,
        )
        .map_err(|e| format!("Failed to boot network: {}", e))?;

        // Detach the event loop — it runs forever in the background.
        // We store the AbortHandle so shutdown_kinetic can cancel it cleanly.
        let join_handle = tokio::spawn(network_loop.run());
        let _ = crate::state::NETWORK_LOOP_HANDLE.set(join_handle.abort_handle());

        // Step 11: Commit to global singletons.
        let _ = STORAGE.set(storage);
        let _ = KEYPAIR.set(keypair);
        let _ = NETWORK.set(network_client);

        Ok(format!(
            "Kinetic initialized. Peer connected to Kademlia mesh. kyn={}",
            initial_kyn
        ))
    });

    // Step 12: Commit the runtime to the global singleton LAST (after all async work is done).
    let _ = RUNTIME.set(rt);

    match result {
        Ok(msg) => JsonResponse {
            status: "ok".to_string(),
            data: Some(serde_json::json!({"message": msg})),
            error: None,
        },
        Err(e) => JsonResponse {
            status: "error".to_string(),
            data: None,
            error: Some(e),
        },
    }
}
