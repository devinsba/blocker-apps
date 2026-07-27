# blocker-apps

Multi-platform app skeleton:

- `mobile/flutter_app`: Flutter app skeleton for iOS, Android, and iPadOS.
- `desktop/flutter_app`: Flutter app skeleton for macOS and Windows.
- `shared/blocker_shared`: Shared Dart module for common code used by both apps.

## Mobile Flutter app

Path: `mobile/flutter_app`

Next step (when Flutter SDK is available):

```bash
cd mobile/flutter_app
flutter create --platforms=android,ios .
flutter pub get
flutter run
```

## Desktop Flutter app

Path: `desktop/flutter_app`

Next step (when Flutter SDK is available):

```bash
cd desktop/flutter_app
flutter create --platforms=macos,windows .
flutter pub get
flutter run -d macos
```