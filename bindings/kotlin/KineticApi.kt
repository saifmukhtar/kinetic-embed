package uniffi.kinetic_embed

/**
 * KineticApi — Manual Kotlin wrappers over the UniFFI steel bridge.
 *
 * This file lives alongside the auto-generated kinetic_embed.kt.
 * Every function here calls `invokeKineticJson` with the correct
 * JSON-RPC method string and returns the raw JSON response string.
 *
 * Usage in your Android app:
 *   val api = KineticApi()
 *   val config = api.getConfig()  // returns JSON string
 */
class KineticApi {

    // ─────────────────────────────────────────────────────────
    // Bootstrap
    // ─────────────────────────────────────────────────────────

    /** Must be called FIRST before any other API.
     *  @param dataDir Absolute path to the app's private data directory (e.g. getFilesDir().absolutePath)
     *  @param networkMode Optional: "LightNode" or "FullNode" (default)
     *  @param listenPort Optional: TCP listen port, 0 = OS assigned
     */
    fun initKinetic(dataDir: String, networkMode: String? = null, listenPort: Int? = null): String {
        val params = buildString {
            append("""{"data_dir":"$dataDir"""")
            if (networkMode != null) append(""","network_mode":"$networkMode"""")
            if (listenPort != null) append(""","listen_port":$listenPort""")
            append("}")
        }
        return invokeKineticJson("""{"method":"init_kinetic","params":$params}""")
    }

    // ─────────────────────────────────────────────────────────
    // Action
    // ─────────────────────────────────────────────────────────

    fun getActionStatus(): String =
        invokeKineticJson("""{"method":"get_action_status"}""")

    fun getActionNames(): String =
        invokeKineticJson("""{"method":"get_action_names"}""")

    /** @param msgJson A JSON-encoded SignedNetworkAction */
    fun publishAction(msgJson: String): String =
        invokeKineticJson("""{"method":"publish_action","params":$msgJson}""")

    // ─────────────────────────────────────────────────────────
    // Config
    // ─────────────────────────────────────────────────────────

    fun getConfig(): String =
        invokeKineticJson("""{"method":"get_config"}""")

    /** @param configJson A JSON-encoded KineticConfig object */
    fun setConfig(configJson: String): String =
        invokeKineticJson("""{"method":"set_config","params":{"config":$configJson}}""")

    fun ownedNames(): String =
        invokeKineticJson("""{"method":"owned_names"}""")

    fun networkStatus(): String =
        invokeKineticJson("""{"method":"network_status"}""")

    fun networkBootstrap(): String =
        invokeKineticJson("""{"method":"network_bootstrap"}""")

    fun networkNat(): String =
        invokeKineticJson("""{"method":"network_nat"}""")

    fun networkBanned(): String =
        invokeKineticJson("""{"method":"network_banned"}""")

    fun networkPeers(): String =
        invokeKineticJson("""{"method":"network_peers"}""")

    fun getHealth(): String =
        invokeKineticJson("""{"method":"get_health"}""")

    fun getPeerId(): String =
        invokeKineticJson("""{"method":"get_peer_id"}""")

    fun dnsFlush(): String =
        invokeKineticJson("""{"method":"dns_flush"}""")

    // ─────────────────────────────────────────────────────────
    // Atlas
    // ─────────────────────────────────────────────────────────

    /** @param nspsJson A JSON array of NSP strings e.g. ["nsp1", "nsp2"] */
    fun atlasSync(nspsJson: String): String =
        invokeKineticJson("""{"method":"atlas_sync","params":{"nsps":$nspsJson}}""")

    // ─────────────────────────────────────────────────────────
    // Gossip
    // ─────────────────────────────────────────────────────────

    fun gossipSubscribe(): String =
        invokeKineticJson("""{"method":"gossip_subscribe"}""")

    /** @param topic The gossip topic string
     *  @param dataJson A JSON-encoded payload */
    fun gossipPublish(topic: String, dataJson: String): String =
        invokeKineticJson("""{"method":"gossip_publish","params":{"topic":"$topic","data":$dataJson}}""")

    fun gossipTopics(): String =
        invokeKineticJson("""{"method":"gossip_topics"}""")

    // ─────────────────────────────────────────────────────────
    // KID (Kinetic Identity Document)
    // ─────────────────────────────────────────────────────────

    fun listKids(): String =
        invokeKineticJson("""{"method":"list_kids"}""")

    fun fetchKid(kid: String): String =
        invokeKineticJson("""{"method":"fetch_kid","params":{"kid":"$kid"}}""")

    fun generateKid(): String =
        invokeKineticJson("""{"method":"generate_kid"}""")

    fun rotateKid(kid: String): String =
        invokeKineticJson("""{"method":"rotate_kid","params":{"kid":"$kid"}}""")

    fun revokeKid(kid: String): String =
        invokeKineticJson("""{"method":"revoke_kid","params":{"kid":"$kid"}}""")

    fun getKidManifest(kid: String): String =
        invokeKineticJson("""{"method":"get_kid_manifest","params":{"kid":"$kid"}}""")

    /** @param manifestJson A JSON-encoded manifest object */
    fun updateKidManifest(kid: String, manifestJson: String): String =
        invokeKineticJson("""{"method":"update_kid_manifest","params":{"kid":"$kid","manifest":$manifestJson}}""")

    fun resolveKid(kid: String): String =
        invokeKineticJson("""{"method":"resolve_kid","params":{"kid":"$kid"}}""")

    fun publishKid(kid: String): String =
        invokeKineticJson("""{"method":"publish_kid","params":{"kid":"$kid"}}""")

    fun publishManifest(kid: String): String =
        invokeKineticJson("""{"method":"publish_manifest","params":{"kid":"$kid"}}""")

    // ─────────────────────────────────────────────────────────
    // Time
    // ─────────────────────────────────────────────────────────

    fun getTime(): String =
        invokeKineticJson("""{"method":"get_time"}""")

    // ─────────────────────────────────────────────────────────
    // VDF / Consensus Math
    // ─────────────────────────────────────────────────────────

    fun getIterations(name: String): String =
        invokeKineticJson("""{"method":"get_iterations","params":{"name":"$name"}}""")

    fun takeoverIterations(name: String): String =
        invokeKineticJson("""{"method":"takeover_iterations","params":{"name":"$name"}}""")

    fun validateName(name: String): String =
        invokeKineticJson("""{"method":"validate_name","params":{"name":"$name"}}""")

    // ─────────────────────────────────────────────────────────
    // Heartbeat
    // ─────────────────────────────────────────────────────────

    fun getHeartbeat(): String =
        invokeKineticJson("""{"method":"get_heartbeat"}""")

    fun postHeartbeat(): String =
        invokeKineticJson("""{"method":"post_heartbeat"}""")

    fun postFatHeartbeat(): String =
        invokeKineticJson("""{"method":"post_fat_heartbeat"}""")

    // ─────────────────────────────────────────────────────────
    // Macro (high-level name registration workflow)
    // ─────────────────────────────────────────────────────────

    fun macroTasks(): String =
        invokeKineticJson("""{"method":"macro_tasks"}""")

    fun macroStatus(): String =
        invokeKineticJson("""{"method":"macro_status"}""")

    /** @param name The .kin name to register */
    fun macroRegisterName(name: String): String =
        invokeKineticJson("""{"method":"macro_register_name","params":{"name":"$name"}}""")

    /** @param name The .kin name to renew */
    fun macroRenewName(name: String): String =
        invokeKineticJson("""{"method":"macro_renew_name","params":{"name":"$name"}}""")

    // ─────────────────────────────────────────────────────────
    // System
    // ─────────────────────────────────────────────────────────

    fun shutdownKinetic(): String =
        invokeKineticJson("""{"method":"shutdown_kinetic"}""")

    fun restartKinetic(): String =
        invokeKineticJson("""{"method":"restart_kinetic"}""")

    fun getCaCert(): String =
        invokeKineticJson("""{"method":"get_ca_cert"}""")

    // ─────────────────────────────────────────────────────────
    // NRS (Name Record System)
    // ─────────────────────────────────────────────────────────

    /** @param recordJson A JSON-encoded name record */
    fun publishRecord(recordJson: String): String =
        invokeKineticJson("""{"method":"publish_record","params":$recordJson}""")

    /** @param commitJson A JSON-encoded commit payload */
    fun publishCommit(commitJson: String): String =
        invokeKineticJson("""{"method":"publish_commit","params":$commitJson}""")

    fun resolveName(name: String): String =
        invokeKineticJson("""{"method":"resolve_name","params":{"name":"$name"}}""")

    /** @param quorumJson A JSON-encoded quorum proof */
    fun verifyQuorum(quorumJson: String): String =
        invokeKineticJson("""{"method":"verify_quorum","params":$quorumJson}""")

    fun getReservedNames(): String =
        invokeKineticJson("""{"method":"get_reserved_names"}""")

    fun getZone(name: String): String =
        invokeKineticJson("""{"method":"get_zone","params":{"name":"$name"}}""")

    /** @param zoneJson A JSON-encoded zone object */
    fun postZone(zoneJson: String): String =
        invokeKineticJson("""{"method":"post_zone","params":$zoneJson}""")

    fun publishZone(name: String): String =
        invokeKineticJson("""{"method":"publish_zone","params":{"name":"$name"}}""")

    /** @param zoneJson A JSON-encoded zone object */
    fun postLocalZone(zoneJson: String): String =
        invokeKineticJson("""{"method":"post_local_zone","params":$zoneJson}""")

    fun deleteLocalZone(name: String): String =
        invokeKineticJson("""{"method":"delete_local_zone","params":{"name":"$name"}}""")

    fun getLocalZone(name: String): String =
        invokeKineticJson("""{"method":"get_local_zone","params":{"name":"$name"}}""")

    /** @param zoneJson A JSON-encoded fat zone object */
    fun publishFatZone(zoneJson: String): String =
        invokeKineticJson("""{"method":"publish_fat_zone","params":$zoneJson}""")
}
