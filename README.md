# blocker-apps

Multi-platform app skeleton:

- `mobile/flutter_app`: Flutter app skeleton for iOS, Android, and iPadOS.
- `desktop/flutter_app`: Flutter app skeleton for macOS and Windows.
- `shared/blocker_shared`: Shared Dart module for common code used by both apps.

## Mobile Flutter app

Path: `mobile/flutter_app`

Next step (when Flutter SDK is available):

> ⚠️ `flutter create --overwrite` may overwrite existing files in this directory.

```bash
cd mobile/flutter_app
flutter create --platforms=android,ios --overwrite .
flutter pub get
flutter run
```

## Desktop Flutter app

Path: `desktop/flutter_app`

Next step (when Flutter SDK is available):

> ⚠️ `flutter create --overwrite` may overwrite existing files in this directory.

```bash
cd desktop/flutter_app
flutter create --platforms=macos,windows --overwrite .
flutter pub get
flutter run -d macos
```