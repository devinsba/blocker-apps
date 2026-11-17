# blocker-apps

Website and app blocker for desktop and mobile. It coexists with Tailscale,
Twingate and other VPNs, scales to millions of blocked domains, and gets signed
blocklists from a central server.

The blocking logic is written once, in Rust (`crates/blocker-core`), and used
everywhere: directly by the Rust desktop apps, and through a C ABI
(`crates/blocker-ffi`, header `blocker.h`) by the Swift and Kotlin mobile apps.

```
                         crates/blocker-core  (Rust: rules, schedules, FST lists, SNI, guard, engine)
                           │                       │
              Rust crate dep│                       │ crates/blocker-ffi  →  blocker.h  (C ABI, staticlib)
                           ▼                       ▼
   desktop/ (Rust: macOS, Linux, Windows)     mobile/ios (Swift: `import BlockerFFI` → BlockerKit)
     blocker-cli, blocker-ui,                 mobile/android (Kotlin → C JNI shim → blocker.h)
     blocker-filter (macOS NE extension)
                                       crates/blocker-server  (builds and serves signed lists)
```

## Layout

| Path | What |
|---|---|
| `crates/blocker-core` | Platform-independent blocking decisions. Pure Rust, unit-tested. |
| `crates/blocker-ffi` | C ABI over the core: `include/blocker.h`, `include/module.modulemap`. Builds `libblocker_ffi.a`. |
| `crates/blocker-server` | Central list server: `keygen`, `build` (fetch → FST → signed manifest), `serve`. |
| `desktop/blocker-cli` | `blocker` CLI and root daemon. |
| `desktop/blocker-ui` | Tray / menu-bar app built with Rune. |
| `desktop/blocker-filter` | macOS `NEFilterDataProvider` system extension (macOS-only; a stub elsewhere). |
| `desktop/packaging/macos` | Builds, signs and notarizes `Blocker.app`. |
| `mobile/ios` | iOS / iPadOS app in Swift (SwiftUI), `BlockerKit` Swift package, NE extensions. |
| `mobile/android` | Android app in Kotlin (Compose), `:blocker-core` JNI module, `VpnService`. |
| `scripts/` | Cross-compilation and test helpers. |
| `research/` | Platform research on blocking approaches. Read before platform work. |

## Platform status

| Platform | Language | Domain blocking | App blocking |
|---|---|---|---|
| macOS | Rust | Content filter system extension (works alongside VPNs) | Closes blocked apps |
| Linux | Rust | Not yet (CLI, list updates and tray UI run) | Kills blocked processes by name |
| Windows | Rust | Not yet (CLI, list updates and tray UI run) | Kills blocked processes by name |
| iOS / iPadOS | Swift | `NEFilterDataProvider` (supervised) / packet tunnel DNS: skeletons | Screen Time: stubbed |
| Android | Kotlin | `VpnService` DNS filtering: skeleton | Not yet |

## Building

Rust workspace (all desktop apps, the core, the FFI library and the server):

```bash
cargo build --workspace
```

```bash
cargo test --workspace
```

C ABI smoke test (links `blocker.h` + `libblocker_ffi.a` from plain C):

```bash
scripts/test-ffi-c.sh
```

macOS app bundle: see [desktop/packaging/macos/README.md](desktop/packaging/macos/README.md).
Desktop architecture and usage: [desktop/README.md](desktop/README.md).
iOS / iPadOS: [mobile/ios/README.md](mobile/ios/README.md).
Android: [mobile/android/README.md](mobile/android/README.md).
