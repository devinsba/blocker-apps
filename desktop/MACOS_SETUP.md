# macOS App Blocking Setup

This document outlines the steps to set up native app and domain blocking on macOS.

## Overview

The macOS app uses **Network Extensions** for blocking functionality. Unlike iOS, macOS does NOT require supervised devices for content filtering, making it suitable for consumer distribution.

## Architecture

```
Flutter App (UI)
    ↓ Platform Channel
Native macOS Code
    ↓
Network Extension
    ├── Content Filter (NEFilterDataProvider)
    │   └── Blocks domains at network level
    └── DNS Proxy (NEDNSProxyProvider)
        └── Blocks DNS queries
```

For app blocking, we use:
- **NSRunningApplication** API to detect running apps
- **Process termination** to block apps (requires proper entitlements)

## Prerequisites

1. **Apple Developer Account** (required for Network Extension entitlements)
2. **Xcode** (latest version)
3. **Flutter SDK** installed

## Step 1: Initialize macOS Project

```bash
cd desktop/flutter_app
flutter create --platforms=macos .
```

## Step 2: Request Entitlements from Apple

Network Extensions require special entitlements from Apple:

1. Go to https://developer.apple.com/contact/request/network-extension/
2. Request the following entitlements:
   - `com.apple.developer.networking.networkextension`
   - Content Filter Provider capability

Expected approval time: 1-2 weeks

## Step 3: Configure Xcode Project

### Add Capabilities

1. Open `macos/Runner.xcworkspace` in Xcode
2. Select the Runner target
3. Go to "Signing & Capabilities"
4. Add the following capabilities:
   - Network Extensions
   - App Sandbox (with appropriate exceptions)
   - Keychain Sharing (for shared data between app and extension)

### Add Entitlements File

Create/edit `macos/Runner/Runner.entitlements`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.developer.networking.networkextension</key>
    <array>
        <string>content-filter-provider</string>
        <string>dns-proxy</string>
    </array>
    <key>com.apple.security.app-sandbox</key>
    <true/>
    <key>com.apple.security.network.client</key>
    <true/>
    <key>com.apple.security.network.server</key>
    <true/>
    <key>com.apple.application-identifier</key>
    <string>$(AppIdentifierPrefix)$(CFBundleIdentifier)</string>
    <key>keychain-access-groups</key>
    <array>
        <string>$(AppIdentifierPrefix)com.blocker.shared</string>
    </array>
</dict>
</plist>
```

## Step 4: Create Network Extension Targets

### Content Filter Extension

1. In Xcode: File → New → Target
2. Select "macOS" → "Network Extension"
3. Choose "Content Filter Provider"
4. Name it "BlockerContentFilter"

### DNS Proxy Extension (Optional, for domain blocking)

1. In Xcode: File → New → Target
2. Select "macOS" → "Network Extension"
3. Choose "DNS Proxy Provider"
4. Name it "BlockerDNSProxy"

## Step 5: Implement Native Swift Code

### Platform Channel Handler

Create `macos/Runner/BlockingMethodChannel.swift`:

```swift
import Cocoa
import FlutterMacOS
import NetworkExtension

class BlockingMethodChannel {
    private let channel: FlutterMethodChannel
    private var blockedApps: Set<String> = []
    private var blockedDomains: Set<String> = []

    init(messenger: FlutterBinaryMessenger) {
        channel = FlutterMethodChannel(
            name: "com.blocker/blocking",
            binaryMessenger: messenger
        )

        channel.setMethodCallHandler { [weak self] call, result in
            self?.handle(call, result: result)
        }
    }

    private func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        switch call.method {
        case "enableAppBlocking":
            enableAppBlocking(call, result: result)
        case "disableAppBlocking":
            disableAppBlocking(call, result: result)
        case "enableDomainBlocking":
            enableDomainBlocking(call, result: result)
        case "disableDomainBlocking":
            disableDomainBlocking(call, result: result)
        case "updateAllRules":
            updateAllRules(call, result: result)
        case "isPlatformSupported":
            result(true) // macOS is always supported
        case "requestPermissions":
            requestPermissions(result: result)
        default:
            result(FlutterMethodNotImplemented)
        }
    }

    private func enableAppBlocking(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        guard let args = call.arguments as? [String: Any],
              let bundleIds = args["bundleIds"] as? [String] else {
            result(FlutterError(code: "INVALID_ARGS", message: nil, details: nil))
            return
        }

        blockedApps.formUnion(bundleIds)
        startMonitoringApps()
        result(true)
    }

    private func disableAppBlocking(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        guard let args = call.arguments as? [String: Any],
              let bundleIds = args["bundleIds"] as? [String] else {
            result(FlutterError(code: "INVALID_ARGS", message: nil, details: nil))
            return
        }

        blockedApps.subtract(bundleIds)
        result(true)
    }

    private func enableDomainBlocking(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        guard let args = call.arguments as? [String: Any],
              let domains = args["domains"] as? [String] else {
            result(FlutterError(code: "INVALID_ARGS", message: nil, details: nil))
            return
        }

        blockedDomains.formUnion(domains)
        updateContentFilter()
        result(true)
    }

    private func disableDomainBlocking(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        guard let args = call.arguments as? [String: Any],
              let domains = args["domains"] as? [String] else {
            result(FlutterError(code: "INVALID_ARGS", message: nil, details: nil))
            return
        }

        blockedDomains.subtract(domains)
        updateContentFilter()
        result(true)
    }

    private func updateAllRules(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        guard let args = call.arguments as? [String: Any],
              let apps = args["apps"] as? [String],
              let domains = args["domains"] as? [String] else {
            result(FlutterError(code: "INVALID_ARGS", message: nil, details: nil))
            return
        }

        blockedApps = Set(apps)
        blockedDomains = Set(domains)

        startMonitoringApps()
        updateContentFilter()

        result(true)
    }

    private func requestPermissions(result: @escaping FlutterResult) {
        // Request system extension approval
        NEFilterManager.shared().loadFromPreferences { error in
            if let error = error {
                result(FlutterError(code: "PERMISSION_ERROR",
                                  message: error.localizedDescription,
                                  details: nil))
                return
            }

            NEFilterManager.shared().isEnabled = true
            NEFilterManager.shared().saveToPreferences { error in
                if let error = error {
                    result(FlutterError(code: "PERMISSION_ERROR",
                                      message: error.localizedDescription,
                                      details: nil))
                } else {
                    result(true)
                }
            }
        }
    }

    private func startMonitoringApps() {
        // Monitor running applications and terminate blocked ones
        Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { [weak self] _ in
            self?.checkRunningApps()
        }
    }

    private func checkRunningApps() {
        let runningApps = NSWorkspace.shared.runningApplications

        for app in runningApps {
            if let bundleId = app.bundleIdentifier,
               blockedApps.contains(bundleId) {
                app.terminate()
            }
        }
    }

    private func updateContentFilter() {
        // Update the content filter with new domain rules
        // This will be implemented in the Network Extension
        UserDefaults(suiteName: "group.com.blocker.shared")?.set(
            Array(blockedDomains),
            forKey: "blockedDomains"
        )
    }
}
```

### Register Channel in AppDelegate

Edit `macos/Runner/AppDelegate.swift`:

```swift
import Cocoa
import FlutterMacOS

@NSApplicationMain
class AppDelegate: FlutterAppDelegate {
    private var blockingChannel: BlockingMethodChannel?

    override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        return true
    }

    override func applicationDidFinishLaunching(_ notification: Notification) {
        let controller = mainFlutterWindow?.contentViewController as! FlutterViewController
        blockingChannel = BlockingMethodChannel(messenger: controller.engine.binaryMessenger)
    }
}
```

## Step 6: Implement Content Filter Extension

Edit `BlockerContentFilter/FilterDataProvider.swift`:

```swift
import NetworkExtension

class FilterDataProvider: NEFilterDataProvider {
    private var blockedDomains: Set<String> = []

    override func startFilter(completionHandler: @escaping (Error?) -> Void) {
        loadBlockedDomains()
        completionHandler(nil)
    }

    override func stopFilter(with reason: NEProviderStopReason, completionHandler: @escaping () -> Void) {
        completionHandler()
    }

    override func handleNewFlow(_ flow: NEFilterFlow) -> NEFilterNewFlowVerdict {
        guard let socketFlow = flow as? NEFilterSocketFlow,
              let remoteEndpoint = socketFlow.remoteEndpoint as? NWHostEndpoint else {
            return .allow()
        }

        let hostname = remoteEndpoint.hostname

        if shouldBlock(domain: hostname) {
            return .drop()
        }

        return .allow()
    }

    private func loadBlockedDomains() {
        if let domains = UserDefaults(suiteName: "group.com.blocker.shared")?
            .array(forKey: "blockedDomains") as? [String] {
            blockedDomains = Set(domains)
        }
    }

    private func shouldBlock(domain: String) -> Bool {
        for blocked in blockedDomains {
            if blocked.hasPrefix("*.") {
                let baseDomain = String(blocked.dropFirst(2))
                if domain.hasSuffix(baseDomain) || domain == baseDomain {
                    return true
                }
            } else if domain == blocked || domain.hasSuffix(".\(blocked)") {
                return true
            }
        }
        return false
    }
}
```

## Step 7: Update Home Screen to Use Platform Channel

The home screen already saves rules using `BlockStorage`. We need to integrate with the platform channel:

```dart
// Add to _HomeScreenState
final PlatformBlocker _platformBlocker = PlatformBlocker();

// Update _saveRules method
Future<void> _saveRules() async {
  await _storage.saveRules(_blockingService.getAllRules());
  // Update platform blocking
  await _platformBlocker.updateAllRules(_blockingService.getAllRules());
}

// Add permission check in initState
@override
void initState() {
  super.initState();
  _loadRules();
  _checkPermissions();
}

Future<void> _checkPermissions() async {
  final supported = await _platformBlocker.isPlatformSupported();
  if (supported) {
    await _platformBlocker.requestPermissions();
  }
}
```

## Step 8: Build and Test

```bash
cd desktop/flutter_app
flutter run -d macos
```

## Notes

- **App Blocking**: Uses `NSRunningApplication` to monitor and terminate apps
- **Domain Blocking**: Uses Network Extension Content Filter
- **Permissions**: User must approve the Network Extension in System Settings
- **Distribution**: Can be distributed via Mac App Store (with approved entitlements)

## Future Enhancements

1. **Screen Time Integration**: Use ScreenTime API for better app blocking (macOS 13+)
2. **DNS Proxy**: Implement NEDNSProxyProvider for more efficient domain blocking
3. **Scheduling**: Add background task to update blocking rules based on schedules
4. **Notifications**: Notify user when blocked apps are attempted to launch

## Troubleshooting

- **Extension not loading**: Check entitlements and provisioning profile
- **Permission errors**: User must approve extension in System Settings → Privacy & Security
- **Apps not blocking**: Ensure bundle ID is correct using Activity Monitor
