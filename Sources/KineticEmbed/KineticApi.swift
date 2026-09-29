
/**
 * KineticApi — Manual Swift wrappers over the UniFFI steel bridge.
 *
 * This file lives alongside the auto-generated kinetic_embed.swift.
 * Every function here calls `invokeKineticJson` with the correct
 * JSON-RPC method string and returns the raw JSON response string.
 *
 * Usage in your iOS app:
 *   let api = KineticApi()
 *   let config = api.getConfig()  // returns JSON string
 */
public class KineticApi {

    public init() {}

    // ─────────────────────────────────────────────────────────
    // Bootstrap
    // ─────────────────────────────────────────────────────────

    /** Must be called FIRST before any other API.
     *  @param dataDir Absolute path to the app's private data directory (e.g. getFilesDir().absolutePath)
     *  @param networkMode Optional: "LightNode" or "FullNode" (default)
     *  @param listenPort Optional: TCP listen port, 0 = OS assigned
     */
    public func initKinetic(dataDir: String, networkMode: String? = nil, listenPort: Int? = nil) -> String {
        var params = "{\"data_dir\":\"\\(dataDir)\""
        if let networkMode = networkMode { params += ",\"network_mode\":\"\\(networkMode)\"" }
        if let listenPort = listenPort { params += ",\"listen_port\":\\(listenPort)" }
        params += "}"
        return invokeKineticJson(reqJson: "{\"method\":\"init_kinetic\",\"params\":\\(params)}")
    }

    // ─────────────────────────────────────────────────────────
    // Action
    // ─────────────────────────────────────────────────────────

    public func getActionStatus() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_action_status\"}")
    }

    public func getActionNames() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_action_names\"}")
    }

    /** @param msgJson A JSON-encoded SignedNetworkAction */
    public func publishAction(msgJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"publish_action\",\"params\":\(msgJson)}")
    }

    // ─────────────────────────────────────────────────────────
    // Config
    // ─────────────────────────────────────────────────────────

    public func getConfig() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_config\"}")
    }

    /** @param configJson A JSON-encoded KineticConfig object */
    public func setConfig(configJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"set_config\",\"params\":{\"config\":\(configJson)}}")
    }

    public func ownedNames() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"owned_names\"}")
    }

    public func networkStatus() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"network_status\"}")
    }

    public func networkBootstrap() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"network_bootstrap\"}")
    }

    public func networkNat() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"network_nat\"}")
    }

    public func networkBanned() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"network_banned\"}")
    }

    public func networkPeers() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"network_peers\"}")
    }

    public func getHealth() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_health\"}")
    }

    public func getPeerId() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_peer_id\"}")
    }

    public func dnsFlush() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"dns_flush\"}")
    }

    // ─────────────────────────────────────────────────────────
    // Atlas
    // ─────────────────────────────────────────────────────────

    /** @param nspsJson A JSON array of NSP strings e.g. ["nsp1", "nsp2"] */
    public func atlasSync(nspsJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"atlas_sync\",\"params\":{\"nsps\":\(nspsJson)}}")
    }

    // ─────────────────────────────────────────────────────────
    // Gossip
    // ─────────────────────────────────────────────────────────

    public func gossipSubscribe() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"gossip_subscribe\"}")
    }

    /** @param topic The gossip topic string
     *  @param dataJson A JSON-encoded payload */
    public func gossipPublish(topic: String, dataJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"gossip_publish\",\"params\":{\"topic\":\"\(topic)\",\"data\":\(dataJson)}}")
    }

    public func gossipTopics() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"gossip_topics\"}")
    }

    // ─────────────────────────────────────────────────────────
    // KID (Kinetic Identity Document)
    // ─────────────────────────────────────────────────────────

    public func listKids() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"list_kids\"}")
    }

    public func fetchKid(kid: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"fetch_kid\",\"params\":{\"kid\":\"\(kid)\"}}")
    }

    public func generateKid() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"generate_kid\"}")
    }

    public func rotateKid(kid: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"rotate_kid\",\"params\":{\"kid\":\"\(kid)\"}}")
    }

    public func revokeKid(kid: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"revoke_kid\",\"params\":{\"kid\":\"\(kid)\"}}")
    }

    public func getKidManifest(kid: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_kid_manifest\",\"params\":{\"kid\":\"\(kid)\"}}")
    }

    /** @param manifestJson A JSON-encoded manifest object */
    public func updateKidManifest(kid: String, manifestJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"update_kid_manifest\",\"params\":{\"kid\":\"\(kid)\",\"manifest\":\(manifestJson)}}")
    }

    public func resolveKid(kid: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"resolve_kid\",\"params\":{\"kid\":\"\(kid)\"}}")
    }

    public func publishKid(kid: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"publish_kid\",\"params\":{\"kid\":\"\(kid)\"}}")
    }

    public func publishManifest(kid: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"publish_manifest\",\"params\":{\"kid\":\"\(kid)\"}}")
    }

    // ─────────────────────────────────────────────────────────
    // Time
    // ─────────────────────────────────────────────────────────

    public func getTime() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_time\"}")
    }

    // ─────────────────────────────────────────────────────────
    // VDF / Consensus Math
    // ─────────────────────────────────────────────────────────

    public func getIterations(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_iterations\",\"params\":{\"name\":\"\(name)\"}}")
    }

    public func takeoverIterations(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"takeover_iterations\",\"params\":{\"name\":\"\(name)\"}}")
    }

    public func validateName(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"validate_name\",\"params\":{\"name\":\"\(name)\"}}")
    }

    // ─────────────────────────────────────────────────────────
    // Heartbeat
    // ─────────────────────────────────────────────────────────

    public func getHeartbeat() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_heartbeat\"}")
    }

    public func postHeartbeat() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"post_heartbeat\"}")
    }

    public func postFatHeartbeat() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"post_fat_heartbeat\"}")
    }

    // ─────────────────────────────────────────────────────────
    // Macro (high-level name registration workflow)
    // ─────────────────────────────────────────────────────────

    public func macroTasks() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"macro_tasks\"}")
    }

    public func macroStatus() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"macro_status\"}")
    }

    /** @param name The .kin name to register */
    public func macroRegisterName(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"macro_register_name\",\"params\":{\"name\":\"\(name)\"}}")
    }

    /** @param name The .kin name to renew */
    public func macroRenewName(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"macro_renew_name\",\"params\":{\"name\":\"\(name)\"}}")
    }

    // ─────────────────────────────────────────────────────────
    // System
    // ─────────────────────────────────────────────────────────

    public func shutdownKinetic() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"shutdown_kinetic\"}")
    }

    public func restartKinetic() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"restart_kinetic\"}")
    }

    public func getCaCert() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_ca_cert\"}")
    }

    // ─────────────────────────────────────────────────────────
    // NRS (Name Record System)
    // ─────────────────────────────────────────────────────────

    /** @param recordJson A JSON-encoded name record */
    public func publishRecord(recordJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"publish_record\",\"params\":\(recordJson)}")
    }

    /** @param commitJson A JSON-encoded commit payload */
    public func publishCommit(commitJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"publish_commit\",\"params\":\(commitJson)}")
    }

    public func resolveName(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"resolve_name\",\"params\":{\"name\":\"\(name)\"}}")
    }

    /** @param quorumJson A JSON-encoded quorum proof */
    public func verifyQuorum(quorumJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"verify_quorum\",\"params\":\(quorumJson)}")
    }

    public func getReservedNames() -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_reserved_names\"}")
    }

    public func getZone(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_zone\",\"params\":{\"name\":\"\(name)\"}}")
    }

    /** @param zoneJson A JSON-encoded zone object */
    public func postZone(zoneJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"post_zone\",\"params\":\(zoneJson)}")
    }

    public func publishZone(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"publish_zone\",\"params\":{\"name\":\"\(name)\"}}")
    }

    /** @param zoneJson A JSON-encoded zone object */
    public func postLocalZone(zoneJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"post_local_zone\",\"params\":\(zoneJson)}")
    }

    public func deleteLocalZone(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"delete_local_zone\",\"params\":{\"name\":\"\(name)\"}}")
    }

    public func getLocalZone(name: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"get_local_zone\",\"params\":{\"name\":\"\(name)\"}}")
    }

    /** @param zoneJson A JSON-encoded fat zone object */
    public func publishFatZone(zoneJson: String) -> String {
        return invokeKineticJson(reqJson: "{\"method\":\"publish_fat_zone\",\"params\":\(zoneJson)}")
    }
}