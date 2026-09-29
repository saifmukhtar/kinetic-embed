package uniffi.kinetic_embed

/**
 * KineticBridge — JNI declarations for the async event callback bridge.
 *
 * This is the JNI side of the Kinetic Android integration.
 * It handles:
 *   1. Async events fired by the Rust engine (gossip, heartbeat, peer events, etc.)
 *   2. Raw command invocation via the C ABI (fallback — prefer KineticApi for normal calls)
 *
 * The native implementation lives in kinetic_jni.cpp.
 *
 * Usage in your Application class:
 *
 *   class MyApp : Application() {
 *       override fun onCreate() {
 *           super.onCreate()
 *           KineticBridge.registerEventCallback(object : KineticEventCallback {
 *               override fun onEvent(eventJson: String) {
 *                   // handle async events from Rust engine
 *               }
 *           })
 *       }
 *   }
 */
object KineticBridge {

    init {
        System.loadLibrary("kinetic_jni")
    }

    /**
     * Registers a Kotlin callback to receive async events from the Rust engine.
     * Must be called BEFORE initKinetic().
     * Pass null to unregister.
     *
     * @param callback The object implementing KineticEventCallback, or null to unregister.
     */
    external fun registerEventCallback(callback: KineticEventCallback?)

    /**
     * Raw C ABI bridge — invokes any Kinetic command by JSON string.
     * Prefer using KineticApi for normal synchronous calls.
     * Use this only for low-level or custom commands not covered by KineticApi.
     *
     * @param reqJson A JSON string: {"method": "...", "params": {...}}
     * @return A JSON response string.
     */
    external fun invokeCommand(reqJson: String): String
}

/**
 * Implement this interface to receive async events from the Rust engine.
 *
 * All events arrive as a JSON string. Parse the "type" field to identify the event:
 *
 * Examples:
 *   {"type": "gossip_message", "topic": "...", "data": "..."}
 *   {"type": "peer_connected",  "peer_id": "..."}
 *   {"type": "heartbeat",       "kyn": 12345}
 */
interface KineticEventCallback {
    fun onEvent(eventJson: String)
}
