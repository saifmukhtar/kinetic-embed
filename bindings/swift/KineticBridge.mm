#import "KineticBridge.h"

// ─────────────────────────────────────────────────────────────────────────────
// RUST C-ABI DECLARATIONS
// ─────────────────────────────────────────────────────────────────────────────

extern "C" {
    // Registers the raw C function pointer in Rust.
    void register_event_callback(void (*cb)(const char*));
    
    // Sends a JSON string to the Rust core and returns a raw C-string.
    char* invoke_kinetic_command(const char* req_ptr);
    
    // Frees the C-string allocated by Rust.
    void free_kinetic_string(char* ptr);
}

// ─────────────────────────────────────────────────────────────────────────────
// GLOBAL STATE & C-TO-OBJECTIVE-C ROUTING
// ─────────────────────────────────────────────────────────────────────────────

// Holds the global block so the C-function can trigger it.
static KineticEventCallback g_eventCallback = nil;

// This is the raw C-function that Rust actually calls.
static void rust_event_callback_handler(const char* json_c_str) {
    if (!json_c_str) return;
    
    // Convert the raw C string back to an Apple NSString.
    NSString *jsonPayload = [NSString stringWithUTF8String:json_c_str];
    
    if (g_eventCallback && jsonPayload) {
        // Fire the block. 
        // Note: This executes on whatever background thread Rust triggered it from.
        // Swift UI code must dispatch this to the Main thread manually if needed!
        g_eventCallback(jsonPayload);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// OBJECTIVE-C CLASS IMPLEMENTATION
// ─────────────────────────────────────────────────────────────────────────────

@implementation KineticBridge

+ (void)registerEventCallback:(KineticEventCallback)callback {
    // Copy the block to the heap so it survives the scope.
    g_eventCallback = [callback copy];
    
    // Pass our static C routing function to Rust.
    register_event_callback(rust_event_callback_handler);
}

+ (NSString *)invokeCommand:(NSString *)jsonRequest {
    if (!jsonRequest) {
        return @"{\"status\":\"error\",\"error\":\"null request string\"}";
    }
    
    // Convert NSString to C string
    const char *c_req = [jsonRequest UTF8String];
    
    // Call into Rust
    char *c_res = invoke_kinetic_command(c_req);
    
    if (!c_res) {
        return @"{\"status\":\"error\",\"error\":\"null response from rust\"}";
    }
    
    // Convert the Rust C string back to NSString
    NSString *result = [NSString stringWithUTF8String:c_res];
    
    // **CRITICAL**: We must tell Rust to free the memory it allocated!
    free_kinetic_string(c_res);
    
    if (!result) {
        return @"{\"status\":\"error\",\"error\":\"string encoding error\"}";
    }
    
    return result;
}

@end
