/**
 * kinetic_jni.cpp — Android JNI bridge for Kinetic.
 *
 * This file exposes the Rust/C engine to Kotlin via JNI.
 * It covers:
 *   1. The main synchronous JSON-RPC bridge (invoke_kinetic_command)
 *   2. The async event callback registration (register_event_callback)
 *
 * Add this file to your Android project's CMakeLists.txt:
 *
 *   add_library(kinetic_jni SHARED kinetic_jni.cpp)
 *   target_include_directories(kinetic_jni PRIVATE path/to/kinetic-embed/include)
 *   target_link_libraries(kinetic_jni kinetic_embed android log)
 *
 * In Kotlin, load the library once in your Application class:
 *   System.loadLibrary("kinetic_jni")
 */

#include <jni.h>
#include <android/log.h>
#include <string>
#include "embed.h"

#define LOG_TAG "KineticJNI"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO,  LOG_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

// ─────────────────────────────────────────────────────────────────────────────
// Global references for async callback
// ─────────────────────────────────────────────────────────────────────────────

static JavaVM*   g_jvm      = nullptr;
static jobject   g_callback = nullptr; // global ref to Kotlin callback object
static jmethodID g_method   = nullptr; // onEvent(String) method ID

/**
 * Called from the Rust engine on a background thread whenever an async
 * event fires (e.g. gossip message received, heartbeat, peer connected).
 * Marshals back to the JVM and calls callback.onEvent(eventJson).
 */
static void native_event_callback(const char* event_json) {
    if (!g_jvm || !g_callback || !g_method) return;

    JNIEnv* env = nullptr;
    bool attached = false;

    int status = g_jvm->GetEnv(reinterpret_cast<void**>(&env), JNI_VERSION_1_6);
    if (status == JNI_EDETACHED) {
        // Called from a Rust background thread — attach it to the JVM.
        if (g_jvm->AttachCurrentThread(&env, nullptr) != JNI_OK) {
            LOGE("Failed to attach background thread to JVM");
            return;
        }
        attached = true;
    }

    jstring j_event = env->NewStringUTF(event_json);
    if (j_event) {
        env->CallVoidMethod(g_callback, g_method, j_event);
        env->DeleteLocalRef(j_event);
    }

    if (env->ExceptionCheck()) {
        LOGE("Exception thrown in Kotlin event callback");
        env->ExceptionDescribe();
        env->ExceptionClear();
    }

    if (attached) {
        g_jvm->DetachCurrentThread();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// JNI_OnLoad — store JavaVM pointer when library is loaded
// ─────────────────────────────────────────────────────────────────────────────

extern "C" JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM* vm, void* /*reserved*/) {
    g_jvm = vm;
    LOGI("Kinetic JNI bridge loaded");
    return JNI_VERSION_1_6;
}

// ─────────────────────────────────────────────────────────────────────────────
// registerEventCallback
// Kotlin: external fun registerEventCallback(callback: KineticEventCallback)
//
// interface KineticEventCallback {
//     fun onEvent(eventJson: String)
// }
// ─────────────────────────────────────────────────────────────────────────────

extern "C" JNIEXPORT void JNICALL
Java_com_kinetic_KineticBridge_registerEventCallback(
        JNIEnv* env, jobject /*thiz*/, jobject callback) {

    // Release any previous global reference.
    if (g_callback) {
        env->DeleteGlobalRef(g_callback);
        g_callback = nullptr;
    }

    if (!callback) {
        register_event_callback(nullptr);
        LOGI("Event callback unregistered");
        return;
    }

    // Store a global reference so the callback object survives GC.
    g_callback = env->NewGlobalRef(callback);

    // Resolve the onEvent(String) method once and cache it.
    jclass cls = env->GetObjectClass(g_callback);
    g_method = env->GetMethodID(cls, "onEvent", "(Ljava/lang/String;)V");
    if (!g_method) {
        LOGE("Could not find onEvent(String) method on callback object");
        env->DeleteGlobalRef(g_callback);
        g_callback = nullptr;
        return;
    }

    register_event_callback(native_event_callback);
    LOGI("Event callback registered");
}

// ─────────────────────────────────────────────────────────────────────────────
// invokeCommand
// Kotlin: external fun invokeCommand(reqJson: String): String
//
// This is the raw C ABI bridge — use KineticApi.kt wrappers instead.
// ─────────────────────────────────────────────────────────────────────────────

extern "C" JNIEXPORT jstring JNICALL
Java_com_kinetic_KineticBridge_invokeCommand(
        JNIEnv* env, jobject /*thiz*/, jstring req_json) {

    if (!req_json) {
        return env->NewStringUTF(R"({"status":"error","error":"null request"})");
    }

    const char* req = env->GetStringUTFChars(req_json, nullptr);
    char* response  = invoke_kinetic_command(req);
    env->ReleaseStringUTFChars(req_json, req);

    jstring result = env->NewStringUTF(response ? response : R"({"status":"error","error":"null response"})");
    free_kinetic_string(response);

    return result;
}
