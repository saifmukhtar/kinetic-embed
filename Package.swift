// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "KineticEmbed",
    platforms: [
        .iOS(.v13),
        .macOS(.v12)
    ],
    products: [
        .library(
            name: "KineticEmbed",
            targets: ["KineticEmbed"]
        ),
    ],
    targets: [
        // The Swift wrappers and UniFFI generated code
        .target(
            name: "KineticEmbed",
            dependencies: ["KineticBridge"]
        ),
        // The Objective-C++ bridge and C-ABI headers
        .target(
            name: "KineticBridge",
            dependencies: ["KineticCore"],
            cxxSettings: [
                .headerSearchPath("include")
            ]
        ),
        // The pre-compiled Rust XCFramework binary
        .binaryTarget(
            name: "KineticCore",
            url: "https://github.com/saifmukhtar/kinetic-embed/releases/download/v0.1.9/KineticCore.xcframework.zip",
            checksum: "bcbafcab657d680e765aeac7233928402a9668840c43711edce880fa44f6f49c" // 64-character hex string
        )
    ]
)
