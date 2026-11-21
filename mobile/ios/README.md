# Blocker for iOS / iPadOS

SwiftUI app plus two Network Extensions, all sharing the Rust blocking engine
(`crates/blocker-core`) through its C ABI (`crates/blocker-ffi`).

```
mobile/ios/
├── BlockerKit/          Swift package: safe wrapper over the C ABI (+ tests)
├── Blocker/             SwiftUI app (rules, add rule, check a host, protection)
├── BlockerFilter/       NEFilterDataProvider extension (supervised devices)
├── BlockerTunnel/       NEPacketTunnelProvider extension (DNS filtering, skeleton)
├── Shared/              App Group config code compiled into all three targets
├── Frameworks/          BlockerFFI.xcframework (built, git-ignored)
└── project.yml          XcodeGen spec (the .xcodeproj is generated, git-ignored)
```

## Prerequisites

- Rust via rustup (`~/.cargo/bin` on `PATH`). The build script adds the
  `aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios` and macOS targets.
- Full **Xcode 15+** selected with `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`
  for anything iOS. The Command Line Tools alone are enough only for BlockerKit's host tests.
- [XcodeGen](https://github.com/yonaskolb/XcodeGen): `brew install xcodegen`.
- An Apple developer team with the Network Extension capability; the Family
  Controls entitlement for Screen Time (requested from Apple).

## Build

```sh
# 1. Rust engine -> mobile/ios/Frameworks/BlockerFFI.xcframework
#    (iOS device, iOS simulator arm64+x86_64, and macOS slices)
scripts/build-ios-xcframework.sh

# 2. Xcode project
cd mobile/ios
xcodegen generate
open Blocker.xcodeproj      # set your team, then run the "Blocker" scheme
```

Re-run step 1 after changing Rust code, and `xcodegen generate` after editing
`project.yml` or adding files.

### BlockerKit tests without Xcode

```sh
cargo build -p blocker-ffi --release      # or: scripts/build-ios-xcframework.sh --macos
cd mobile/ios/BlockerKit
BLOCKER_FFI_LOCAL=1 swift test
```

Tests use swift-testing (the Command Line Tools ship `Testing.framework` but not XCTest).

### How BlockerKit finds the C library

`BlockerKit/Package.swift` picks one of two sources for the `BlockerFFI` Clang module:

| Mode | When | `BlockerFFI` target |
|---|---|---|
| XCFramework | `../Frameworks/BlockerFFI.xcframework` exists and `BLOCKER_FFI_LOCAL` is unset or `0` | `binaryTarget` on the XCFramework (headers in `Headers/BlockerFFI/`) |
| Local | `BLOCKER_FFI_LOCAL=1`, or no XCFramework yet | `systemLibrary` (`Sources/BlockerFFI/module.modulemap` → the canonical `crates/blocker-ffi/include/blocker.h`), and BlockerKit links `<repo>/target/release/libblocker_ffi.a` (or `$CARGO_TARGET_DIR/release`) by absolute path |

Why this design: Xcode builds for iOS need the XCFramework, but it can only be
produced with full Xcode. Local mode keeps `swift build` / `swift test` working
on a Mac with only the Command Line Tools. It links the `.a` by path (rather
than `-L … -lblocker_ffi`) because cargo also leaves a `.dylib` in
`target/release` that the linker would otherwise prefer. Local mode relies on
`unsafeFlags`, which SwiftPM allows for root and local-path packages; Xcode
does not pass your shell environment to package resolution, so inside Xcode the
mode follows whether the XCFramework exists.

`CoreFoundation` and `Security` are linked in both modes (needed by Rust std on
Apple platforms, same as `scripts/test-ffi-c.sh`).

## Architecture

Enforcement follows the `research/ios-blocking-approaches` and
`research/cross-platform-summary` branches:

| What | How | Status |
|---|---|---|
| Apps | Screen Time: FamilyControls authorization + `FamilyActivityPicker`, ManagedSettings shields, DeviceActivity schedules | **Stub** (`Blocker/ScreenTimeAuthorization.swift`) |
| Domains, consumer devices | `BlockerTunnel` (NEPacketTunnelProvider): routes only DNS into the tunnel and decides each query with `BlockerEngine.checkHost` | **Skeleton**: queries parsed and decided; answering/forwarding is TODO |
| Domains, supervised devices | `BlockerFilter` (NEFilterDataProvider): `handleNewFlow` → `engine.decide`; on `.peek`, peeks outbound bytes and decides on the TLS SNI / HTTP Host via `BlockerEngine.peekHost` | Implemented, untested on device |

**Choosing the website-blocking method.** The content filter is optional and
used only where iOS allows it (supervised devices, or an MDM-installed filter
profile). iOS has no public "is supervised" API, so
`ContentFilterSupport` (in BlockerKit, unit-tested) combines signals:

1. a filter configuration is already installed (`NEFilterManager` has a
   `providerConfiguration`: pushed by an MDM profile, or saved earlier by the app);
2. managed app configuration from the MDM (`com.apple.configuration.managed`)
   has `supervised = true` (or `false`). Set it per device in your MDM's
   app configuration (Fleet, Jamf, Intune, ...);
3. the outcome of the last attempt to save a filter configuration on this iOS
   version. `NEFilterErrorDomain` invalid / disabled / permission-denied means
   "not supervised"; that is retried after an OS update or when the MDM flag appears.

Result: `.available` → the "Block websites" switch drives the content filter,
and the DNS tunnel is turned off because the filter covers it without the VPN slot;
`.unknown` → the DNS tunnel is used and Protection offers "Use Content
Filter (supervised devices)", which tries it once; `.unavailable` → the DNS tunnel only.
The logic lives in `NetworkProtection` (`Blocker/ProtectionView.swift`) and
re-evaluates when managed configuration changes.

**Config.** Everything reads the App Group container
(`group.dev.blocker.shared`, `Blocker/` subdirectory):

- `config.toml` – the blocker-core config every engine compiles (same format as desktop).
- `rules.json` – the app's editable rule model. blocker-ffi can validate and
  compile TOML but has no TOML → JSON export, so the app keeps its own model
  and renders `config.toml` from it (`RuleModel.renderConfigTOML` in
  `Shared/SharedConfig.swift`). Every save is compiled with blocker-core before
  it is written. Sample rules are seeded on first launch.
- `lists/<category>.fst` – category lists (not downloaded on iOS yet; rules
  using categories produce engine warnings, shown under the rules list).

The content filter's sandbox may not read the group container, so the app also
passes `config.toml` in the filter's `vendorConfiguration`.

**BlockerKit API** (`BlockerKit/Sources/BlockerKit`): `BlockerEngine`
(`init(configTOML:)`, `init(directory:)`, `checkHost(_:at:)`,
`decide(flow:peekedHost:at:)`, `activeRules(at:)`, `blockedApps(at:)`,
`isIdle(at:)`, `warnings`, static `normalizeDomain`, `peekHost`,
`validateConfig`, `version`), `Verdict`, `Flow`, `PeekedHost`, `PeekResult`,
`WeekTime` (Mon = 0, minutes since midnight, `.now`), and `DNSQuery` (DNS
question from a raw IP/UDP packet, for the tunnel). Every C string and verdict
is copied and freed inside the call; the engine is freed in `deinit` and is
safe to share across threads.

**UI.** `NavigationSplitView` (sidebar → list → detail on iPad, a stack on
iPhone): Rules (with "Active" badges from `activeRules`), Add Rule (domains
normalized live with `normalizeDomain`, optional weekly window, lock),
Check a Host (verdict now or at a chosen day/time), Protection (Screen Time
stub, and one "Block websites" switch that uses the content filter or the DNS tunnel
as described above, via `NEFilterManager` / `NETunnelProviderManager`).

## Stubbed / not done

- **Screen Time** app blocking: authorization, picker, shields and the
  DeviceActivityMonitor extension are TODOs; the Family Controls entitlement is
  commented out in `Blocker/Blocker.entitlements`. blocker-core's `bundle_ids`
  can't be applied on iOS directly (Screen Time hands out opaque tokens).
- **DNS tunnel** doesn't answer queries yet (synthesize NXDOMAIN for blocked
  names, forward the rest upstream). Don't enable it on a real device until
  it does: it captures all DNS.
- **Config reload** in running extensions: the filter loads at start; the
  tunnel reloads on any app message. The app doesn't send updates yet.
- iOS content filters may also need an `NEFilterControlProvider` extension;
  not included.
- No category list download, app icon, or localization.
- Each target statically links its own copy of the Rust library.

## What has been verified

On a Mac with only the Command Line Tools (no iOS SDK):

- `BLOCKER_FFI_LOCAL=1 swift build` and `swift test` in `BlockerKit` (17 tests).
- XCFramework mode with a hand-assembled macOS-only XCFramework.
- App and extension sources: `swiftc -parse`, plus type-checked against the
  **macOS** SDK by compiling them in a throwaway package that depends on
  BlockerKit. The tunnel compiles cleanly; the filter's only error is
  `sourceAppIdentifier`, which is iOS-only; the app compiles once a few iOS-only
  SwiftUI modifiers (`keyboardType`, `textInputAutocapitalization`,
  `navigationBarTitleDisplayMode`, `EditButton`) are stubbed. Nothing has been
  built against the iOS SDK, run in the simulator or on a device.
- `scripts/build-ios-xcframework.sh` (iOS mode) has not been run end to end; it
  stops with a clear error without full Xcode.
