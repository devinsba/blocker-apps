// swift-tools-version:5.9
//
// BlockerKit: Swift wrapper over the Rust blocker-core C ABI (crates/blocker-ffi).
//
// The C module `BlockerFFI` comes from one of two places:
//
//  * XCFramework mode (default when ../Frameworks/BlockerFFI.xcframework exists):
//    a binaryTarget built by scripts/build-ios-xcframework.sh with iOS device,
//    iOS simulator and macOS slices. This is what the Xcode app project uses.
//
//  * Local mode (BLOCKER_FFI_LOCAL=1, or when the XCFramework is missing):
//    a systemLibrary target whose module map points at the canonical header in
//    crates/blocker-ffi/include, and an absolute-path link against the host
//    static library from `cargo build -p blocker-ffi --release`
//    (<repo>/target/release/libblocker_ffi.a, or $CARGO_TARGET_DIR/release).
//    This needs only the Command Line Tools, so `swift build` / `swift test`
//    work on a Mac without Xcode. It uses unsafeFlags, which SwiftPM allows for
//    root and local-path packages (fine here; BlockerKit is never published).
//
// BLOCKER_FFI_LOCAL=0 forces XCFramework mode.
import Foundation
import PackageDescription

let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repoRoot = packageDir
    .deletingLastPathComponent() // ios
    .deletingLastPathComponent() // mobile
    .deletingLastPathComponent() // repo
let xcframeworkRelPath = "../Frameworks/BlockerFFI.xcframework"
let xcframeworkExists = FileManager.default.fileExists(
    atPath: packageDir.appendingPathComponent(xcframeworkRelPath).path)

let env = ProcessInfo.processInfo.environment
let localMode: Bool = {
    switch env["BLOCKER_FFI_LOCAL"] {
    case "1", "true", "yes": return true
    case "0", "false", "no": return false
    default: return !xcframeworkExists
    }
}()

let cargoTargetDir: URL = env["CARGO_TARGET_DIR"].map { URL(fileURLWithPath: $0) }
    ?? repoRoot.appendingPathComponent("target")
let hostStaticLib = cargoTargetDir.appendingPathComponent("release/libblocker_ffi.a").path

// Rust's std on Apple platforms needs these (same set scripts/test-ffi-c.sh links).
var linker: [LinkerSetting] = [
    .linkedFramework("CoreFoundation"),
    .linkedFramework("Security"),
]

let ffiTarget: Target
if localMode {
    ffiTarget = .systemLibrary(name: "BlockerFFI", path: "Sources/BlockerFFI")
    linker.append(.unsafeFlags([hostStaticLib]))
} else {
    ffiTarget = .binaryTarget(name: "BlockerFFI", path: xcframeworkRelPath)
}

let package = Package(
    name: "BlockerKit",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [
        .library(name: "BlockerKit", targets: ["BlockerKit"]),
    ],
    targets: [
        ffiTarget,
        .target(
            name: "BlockerKit",
            dependencies: ["BlockerFFI"],
            linkerSettings: linker
        ),
        .testTarget(name: "BlockerKitTests", dependencies: ["BlockerKit"]),
    ]
)
