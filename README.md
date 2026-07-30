# blocker-apps

Multi-platform app skeleton:

- `mobile/flutter_app`: Flutter app skeleton for iOS, Android, and iPadOS.
- `desktop/flutter_app`: Flutter app skeleton for macOS and Windows.
- `shared/blocker_shared`: Shared Dart module for common code used by both apps.

## Quick Start

### macOS App

The easiest way to run the macOS app is using the utility script:

```bash
# Run the app (validates dependencies and initializes project)
./run-macos.sh

# Clean build before running
./run-macos.sh --clean

# Build release version
./run-macos.sh --build
```

The script will automatically:
- ✅ Check Flutter installation
- ✅ Validate macOS toolchain
- ✅ Initialize macOS project if needed
- ✅ Install all dependencies
- ✅ Link the shared module
- ✅ Run the app

### Manual Setup

If you prefer to run manually:

#### Desktop Flutter app

Path: `desktop/flutter_app`

Next step (when Flutter SDK is available):

> ⚠️ `flutter create --overwrite` may overwrite existing files in this directory.

```bash
cd desktop/flutter_app
flutter create --platforms=macos,windows --overwrite .
flutter pub get
flutter run -d macos
```

#### Mobile Flutter app

Path: `mobile/flutter_app`

Next step (when Flutter SDK is available):

> ⚠️ `flutter create --overwrite` may overwrite existing files in this directory.

```bash
cd mobile/flutter_app
flutter create --platforms=android,ios --overwrite .
flutter pub get
flutter run
```

## Features

The blocker app allows you to:

- 🚫 Block specific applications by bundle ID
- 🌐 Block websites and domains (supports wildcards like `*.youtube.com`)
- ⏰ Schedule blocking:
  - Always blocked
  - Time-based (e.g., 9 AM - 5 PM)
  - Day and time-based (e.g., weekdays 9 AM - 5 PM)
- 💾 Persistent storage of blocking rules
- 🎨 Clean, native-looking UI

## Architecture

```
┌─────────────────────────────────────┐
│   Flutter UI (macOS/Windows/Mobile) │
│   - Home screen                     │
│   - Add rule screen                 │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│   Shared Module (blocker_shared)    │
│   - Block rules (models)            │
│   - Blocking logic (service)        │
│   - Storage (persistence)           │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│   Platform-Specific Implementation  │
│   - macOS: Network Extensions       │
│   - iOS: Screen Time + VPN          │
│   - Android: VPN Service            │
└─────────────────────────────────────┘
```

**Key Design Principle**: All blocking logic lives in the shared module. Platform-specific code only handles enforcement.

## Native macOS Blocking

For full macOS blocking functionality (apps + domains), you'll need to set up Network Extensions. See [`desktop/MACOS_SETUP.md`](desktop/MACOS_SETUP.md) for detailed instructions.

### What's Required

1. **Apple Developer Account** (for Network Extension entitlements)
2. **Entitlement Request** (~1-2 weeks approval time)
3. **Xcode Configuration** (capabilities, targets, code signing)
4. **Native Swift Code** (platform channels, Network Extension)

### Current Status

- ✅ Shared blocking logic implemented
- ✅ macOS UI implemented
- ✅ Platform channel interface defined
- ⏳ Native Swift implementation (documented, needs setup)
- ⏳ Network Extension targets (documented, needs creation)

The app will run and allow you to manage blocking rules through the UI, but actual enforcement requires completing the native setup.

## Project Structure

```
blocker-apps/
├── desktop/
│   ├── flutter_app/          # macOS/Windows app
│   │   ├── lib/
│   │   │   ├── screens/       # UI screens
│   │   │   ├── services/      # Platform integration
│   │   │   └── main.dart      # App entry point
│   │   └── macos/             # Native macOS code (after init)
│   └── MACOS_SETUP.md         # macOS setup guide
├── mobile/
│   └── flutter_app/           # iOS/Android app
├── shared/
│   └── blocker_shared/        # Shared logic
│       ├── lib/
│       │   ├── src/
│       │   │   ├── models/    # Data models
│       │   │   └── services/  # Business logic
│       │   └── blocker_shared.dart
│       └── pubspec.yaml
├── research/                  # Platform research docs
└── run-macos.sh              # Utility script
```

## Development Notes

### Adding New Blocking Rules

All rule logic is in the shared module:
- **Models**: `shared/blocker_shared/lib/src/models/`
- **Service**: `shared/blocker_shared/lib/src/services/blocking_service.dart`

To add new rule types:
1. Update the model (e.g., add new `ScheduleType`)
2. Update `BlockingService` logic
3. Update UI to support the new type
4. Platform-specific code automatically picks up changes via the platform channel

### Testing

```bash
# Run Flutter tests
cd desktop/flutter_app
flutter test

# Run shared module tests
cd shared/blocker_shared
flutter test
```

## Resources

- [Flutter Documentation](https://flutter.dev/docs)
- [macOS Network Extensions](https://developer.apple.com/documentation/networkextension)
- [Research Documentation](research/cross-platform-summary.md)
