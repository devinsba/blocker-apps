# Blocker for Android

Kotlin / Jetpack Compose app that uses the shared Rust blocking engine
(`crates/blocker-core`) through its C ABI (`crates/blocker-ffi`).

```
 Kotlin (app)             dev.blocker.app      Compose UI, BlockerVpnService
    │
 Kotlin (:blocker-core)   dev.blocker.core     BlockerEngine, Verdict, WeekTime, Flow
    │  external funs on NativeBridge (jlong handle, String[] verdicts)
 C JNI shim               blocker_jni.c        -> libblocker_jni.so (built by CMake/NDK)
    │  #include "blocker.h"
 C ABI (Rust)             crates/blocker-ffi   libblocker_ffi.a per ABI (scripts/build-android.sh)
    │
 Rust core                crates/blocker-core  rules, schedules, domain sets, SNI parsing
```

## Prerequisites

- Android SDK with platform 35 and CMake 3.22.1 (Android Studio > SDK Manager).
- Android NDK r27c (`27.2.12479018`, pinned in `gradle/libs.versions.toml`).
  Set `ANDROID_NDK_HOME` to it for the Rust build. If you use a different NDK,
  change `ndk` in the version catalog to match.
- Rust via rustup (`~/.cargo/bin`); the script adds the four Android targets.
- JDK 17 or newer.
- Gradle 8.14+ once, to create the wrapper (see below).

## Build

```sh
# 1. Rust core for every Android ABI -> blocker-core/src/main/jniLibs-static/<abi>/
export ANDROID_NDK_HOME=$HOME/Library/Android/sdk/ndk/27.2.12479018
scripts/build-android.sh                     # from the repo root
# ABIS="x86_64" scripts/build-android.sh     # just the emulator ABI

# 2. App (the Gradle wrapper is checked in and fetches Gradle 8.14.3)
cd mobile/android

# App, unit tests, instrumented tests
./gradlew :app:assembleDebug
./gradlew test                                 # JVM unit tests
./gradlew :blocker-core:connectedAndroidTest   # needs a device/emulator
```

Android Studio: open `mobile/android`, and create `local.properties` with
`sdk.dir=...` if it doesn't do so itself.

If you built only some ABIs, also restrict the Gradle build to them, e.g.
`./gradlew -Pblocker.abis=x86_64 :app:assembleDebug` (default in
`gradle.properties`: all four). CMake stops with a clear message if a
`libblocker_ffi.a` is missing.

## Layout

| Path | What |
|------|------|
| `blocker-core/src/main/cpp/blocker_jni.c` | Plain-C JNI shim. Converts strings as real UTF-8, frees every `char*` / `BlockerVerdict`, throws `BlockerException` with `blocker_last_error()`. |
| `blocker-core/src/main/cpp/CMakeLists.txt` | Builds `libblocker_jni.so`; finds `blocker.h` relative to the repo root and links `jniLibs-static/<abi>/libblocker_ffi.a`. |
| `blocker-core/src/main/kotlin/dev/blocker/core/` | Public Kotlin API: `BlockerEngine` (`AutoCloseable`, thread-safe), `Verdict`, `WeekTime`, `Flow`, `BlockerException`; `NativeBridge` holds the `external` functions. |
| `blocker-core/src/test`, `src/androidTest` | JVM tests (WeekTime, JSON helper, Verdict decoding), and instrumented tests of the real native engine. |
| `blocker-core/src/hostTest/` | Runs the JNI shim on the host JVM, without Android: `run-host-test.sh`. |
| `app/src/main/kotlin/dev/blocker/app/` | Compose UI (rules, add rule, check host), `BlockerViewModel`, `data/` (config.toml), `vpn/` (VpnService + DNS packet code). |
| `app/src/test` | JVM tests for the config reader and DNS packet code. |

`BlockerEngine` API at a glance:

```kotlin
BlockerEngine.openDir(dir).use { engine ->          // or fromToml(text)
    engine.checkHost("www.reddit.com")               // Verdict.Block("focus", "reddit.com")
    engine.decide(Flow(remoteIp = ip, remotePort = 443), peeked = false)  // may be Verdict.Peek
    engine.activeRules(WeekTime.now()); engine.blockedApps(); engine.warnings
}
BlockerEngine.normalizeDomain("https://WWW.Example.com/x")  // "www.example.com"
BlockerEngine.peekHost(firstBytes)                          // PeekResult(host, wantMore)
BlockerEngine.validateConfig(text)                          // null if valid, else the error
BlockerEngine.version
```

Times are always computed in Kotlin (`WeekTime.now()`, device time zone; day
0 = Monday) and passed to Rust explicitly.

## The app

- **Rules**: reads `filesDir/blocker/config.toml` (seeded with a sample on
  first run; same format as the desktop apps, `bundle_ids` = package names),
  shows each rule and whether it is active now, plus engine warnings.
  `filesDir/blocker/lists/` is where downloaded category sets would go.
- **Add rule**: domain field normalized live by the Rust core; the new
  config is validated and compiled by Rust before it is written (atomically).
- **Check host**: shows the verdict for a host or URL at the current time.
- **DNS filter**: `BlockerVpnService` is a DNS-only VPN. It routes just a fake
  DNS server address (`10.111.222.53/32`) into the TUN, parses each IPv4/UDP
  query, answers NXDOMAIN when `engine.checkHost(name)` blocks, and forwards
  everything else to the network's resolver over a `protect()`ed socket.
- Layout adapts: bottom navigation bar on phones, navigation rail at
  600dp and wider, and the rules list reflows into columns on tablets.

## What is stubbed / not done

- **Kotlin and Gradle have not been compiled with AGP here**: this tree was
  written on a machine without the Android SDK/NDK. Dependency versions in
  `gradle/libs.versions.toml` may need a bump when you first sync.
- **VPN filter gaps** (TODOs in `vpn/`): no IPv6 DNS, no DNS over TCP / TC
  retry, no handling of Private DNS strict mode, DoH or hard-coded IPs
  (needs a full tunnel with `engine.decide()` + `peekHost()` on TCP flows),
  upstream resolvers are not refreshed on network changes (falls back to
  1.1.1.1), the TUN reader uses blocking reads (switch to poll for clean
  shutdown), and there is no foreground notification or boot receiver.
- **App blocking** is not enforced: `engine.blockedApps()` is read but not
  applied. Android allows one VPN at a time and an allow- *or* deny-list per
  tunnel, so this needs either a full-tunnel firewall keyed on
  `getConnectionOwnerUid`, or UsageStatsManager / AccessibilityService plus a
  blocking screen (see the `research/android-blocking-approaches` branch).
- No editing or deleting rules, no `locked`-rule enforcement in the UI, and no
  category list downloads (`[server]` / manifest sync) yet.
- Google Play only allows `VpnService` when a VPN is the app's core function;
  review the policy before publishing.
