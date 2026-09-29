#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

// A Swift-friendly closure/block that receives the JSON string.
typedef void (^KineticEventCallback)(NSString * _Nonnull jsonPayload);

/**
 * Objective-C++ Bridge to safely route raw C-ABI callbacks and pointers
 * between the Rust static library and Swift.
 */
@interface KineticBridge : NSObject

/**
 * Registers the global event callback for asynchronous Kinetic events.
 * This should be called exactly once when the iOS app launches.
 */
+ (void)registerEventCallback:(KineticEventCallback)callback;

/**
 * Invokes a synchronous Kinetic JSON-RPC command directly via the raw C ABI.
 * (Provided as an ultra-fast alternative to the UniFFI wrapper).
 */
+ (NSString *)invokeCommand:(NSString *)jsonRequest;

@end

NS_ASSUME_NONNULL_END
