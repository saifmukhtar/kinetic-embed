#ifndef KINETIC_EMBED_H
#define KINETIC_EMBED_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * The function signature for the native mobile callback (Kotlin/Swift).
 * It receives a pointer to a null-terminated JSON string.
 * The memory for this string is managed by Rust and is only valid
 * for the duration of the callback execution.
 */
typedef void (*MobileCallback)(const char* event_json);

/**
 * Registers the global event callback for async bridge events.
 * This should be called exactly once by the mobile app when it boots,
 * before calling init_kinetic.
 *
 * @param cb The native function pointer to handle events.
 */
void register_event_callback(MobileCallback cb);

/**
 * Invokes a Kinetic bridge command.
 * 
 * @param req_json A null-terminated JSON string containing {"method": "...", "params": {...}}
 * @return A newly allocated, null-terminated JSON string with the response.
 *         The caller MUST pass this pointer to free_kinetic_string() when done to avoid memory leaks.
 */
char* invoke_kinetic_command(const char* req_json);

/**
 * Frees the memory of a string previously returned by invoke_kinetic_command().
 *
 * @param ptr The pointer returned by invoke_kinetic_command.
 */
void free_kinetic_string(char* ptr);

#ifdef __cplusplus
}
#endif

#endif // KINETIC_EMBED_H
