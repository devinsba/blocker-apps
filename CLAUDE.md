# CLAUDE.md

Website and app blocker. One Rust core, used directly by the Rust desktop apps
(macOS, Linux, Windows) and through a C ABI by the Swift (iOS/iPadOS) and
Kotlin (Android) apps. See README.md for the platform status matrix.

## Layout

```
Cargo.toml / Cargo.lock      Rust workspace root (target/ lives here)
crates/
  blocker-core/              shared logic: config, schedules, domains, FST DomainSet,
                             SNI/HTTP host peek, guard, signed manifest, Engine, Store
  blocker-ffi/               C ABI over blocker-core
    include/blocker.h        THE C interface (hand-written; keep in sync with src/lib.rs)
    include/module.modulemap Clang module `BlockerFFI` for Swift
    examples/smoke.c         plain-C link test (scripts/test-ffi-c.sh)
  blocker-server/            list server: keygen / build / serve
desktop/
  blocker-cli/               package `blocker`: CLI + root daemon
  blocker-ui/                Rune tray/menu-bar app
  blocker-filter/            macOS NEFilterDataProvider system extension (stub on other OSes)
  packaging/macos/           Blocker.app assembly, signing, notarization
  README.md                  desktop architecture, VPN coexistence, usage
mobile/
  ios/                       Swift: BlockerKit package wraps BlockerFFI; SwiftUI app; NE extensions
  android/                   Kotlin: :blocker-core (C JNI shim -> blocker.h) and :app (Compose, VpnService)
scripts/
  test-ffi-c.sh              build libblocker_ffi.a and link examples/smoke.c
  build-ios-xcframework.sh   mobile/ios/Frameworks/BlockerFFI.xcframework
  build-android.sh           per-ABI libblocker_ffi.a for the NDK build
```

## Rules of the road

- **Blocking decisions belong in `crates/blocker-core`.** Platform code only
  gathers flow info and enforces verdicts. If a mobile app needs new logic,
  add it to the core, expose it in `blocker-ffi`, then wrap it in Swift/Kotlin.
- **Changing the C ABI** means updating, together:
  `crates/blocker-ffi/src/lib.rs`, `crates/blocker-ffi/include/blocker.h`,
  the Swift wrapper in `mobile/ios/BlockerKit`, and the JNI shim + Kotlin API
  in `mobile/android/blocker-core`. The `header_declares_every_export` test
  fails if an export is missing from the header.
- FFI conventions: never unwind across the boundary (wrap in `guarded`);
  returned `char *` are freed with `blocker_string_free`, verdicts with
  `blocker_verdict_free`; failures return NULL/false and set
  `blocker_last_error()`; day is 0 = Monday … 6 = Sunday, `-1` = now.
- Desktop crates must compile on macOS, Linux and Windows. Apple-only deps go
  under `[target.'cfg(target_os = "macos")'.dependencies]` and Apple-only code
  behind `#[cfg(target_os = "macos")]`.
- The guard (`crates/blocker-core/src/guard.rs`) is never overridden: VPN
  control planes, CGNAT/tailnet ranges and Blocker itself are never blocked.
- No Flutter/Dart anymore; desktop is Rust only, iOS is Swift, Android is Kotlin.

## Commands

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --target x86_64-unknown-linux-gnu
cargo check --workspace --target x86_64-pc-windows-msvc
scripts/test-ffi-c.sh
# Android JNI shim + Java harness against the host static lib (no SDK needed):
mobile/android/blocker-core/src/hostTest/run-host-test.sh
# Swift wrapper against the host static lib (no Xcode needed):
cargo build -p blocker-ffi --release && (cd mobile/ios/BlockerKit && BLOCKER_FFI_LOCAL=1 swift test)
```

Local machine notes: Rust comes from rustup (`~/.cargo/bin`), not Homebrew.
Building the iOS app needs full Xcode (`xcode-select -s /Applications/Xcode.app`)
and XcodeGen; the Android app needs the Android SDK, NDK (`ANDROID_NDK_HOME`)
(the Gradle wrapper is checked in).

## Mobile notes

- iOS: `mobile/ios/BlockerKit/Package.swift` uses `Frameworks/BlockerFFI.xcframework`
  when present, else (or with `BLOCKER_FFI_LOCAL=1`) its own module map plus
  `target/release/libblocker_ffi.a`. The Xcode project is generated from
  `project.yml` with XcodeGen and not committed. Tests use swift-testing.
  The app stores rules as `rules.json` in the App Group and renders `config.toml`
  from it. Website blocking uses the NEFilter content filter only when
  `ContentFilterSupport` (BlockerKit) says the device supports it (installed
  filter config, MDM managed-config `supervised` flag, or a successful save on
  this OS version); otherwise the DNS tunnel. The filter is never required.
  Bundle ids / App Group (`dev.blocker.ios`, `group.dev.blocker.shared`) are placeholders.
- Android: natives are `@JvmStatic external` on `dev.blocker.core.NativeBridge`
  (C shim: `blocker-core/src/main/cpp/blocker_jni.c`); verdicts cross JNI as a
  3-string array. `scripts/build-android.sh` drops per-ABI `libblocker_ffi.a`
  into `blocker-core/src/main/jniLibs-static/<abi>/` (gitignored).
- Mobile time zones are resolved on the platform side: apps pass `WeekTime`
  (day, minute) explicitly rather than letting Rust read the clock.
- The iOS app has not been built with a real SDK yet. The Android app builds
  (debug and release) with Gradle 9.9.2 and AGP 9.5.2, which compiles Kotlin
  itself (no `org.jetbrains.kotlin.android` plugin). Don't run Gradle on
  GraalVM: its `jlink` fails AGP's `JdkImageTransform`; use Temurin or Corretto.

## Research

Research lives on unmerged `research/<topic>` branches, never on `main` or
feature branches: one findings file per branch at `research/<topic>.md`. Read
one with `git fetch origin && git show origin/research/<topic>:research/<topic>.md`,
and list them with `git branch -r --list 'origin/research/*'`. New research
goes on a new branch (push it, open no PR). Consult research before
implementing blocking features, making platform architecture decisions, or
answering feasibility questions:

- `research/cross-platform-summary` – comparison and recommendations
- `research/ios-blocking-approaches` – Screen Time, NEFilter, packet tunnel
- `research/android-blocking-approaches` – VpnService, accessibility, usage stats
- `research/macos-non-vpn-blocking` – why the macOS filter coexists with VPNs
- `research/fleet-mdm-report` – Fleet / MDM deployment
- `research/nefilter-udp-and-exclusions` – UDP data peeks, no Apple app exclusions since macOS 11.2
- `research/pkg-daemon-sparkle` – `.pkg` install, out-of-bundle daemon, Sparkle updates
- `research/chromium-profile-policies` – SafeSearch and ECH policies via a user-installed profile
- `research/firefox-ech-safesearch-policy` – Firefox ECH lock, no SafeSearch policy
- `research/screen-time-safesearch` – Screen Time and Safari SafeSearch
- `research/blocklist-licences` – adult, gambling and bypass lists and their licences
