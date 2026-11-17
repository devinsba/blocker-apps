# Packaging Blocker.app

`build-app.sh` assembles `Blocker.app` containing the menu-bar UI (main executable,
`Contents/MacOS/blocker-ui`), the CLI (`Contents/MacOS/blocker`)
and the content-filter system extension
(`Contents/Library/SystemExtensions/dev.blocker.app.filter.systemextension`).
The app must live in `/Applications` for the system extension to be activated.

## Requirements

- Apple Developer Program membership (paid). Network Extension and System
  Extension entitlements are restricted and require provisioning profiles.
- Rust toolchain, Xcode command line tools.

## One-time Apple developer setup

1. Register an **App Group** with identifier `<TEAM_ID>.dev.blocker.app` (macOS group
   identifiers are prefixed with the Team ID).
2. Create two App IDs (explicit):
   - `dev.blocker.app` with capabilities **System Extension**, **Network
     Extensions** (Content Filter Provider), and **App Groups**.
   - `dev.blocker.app.filter` with **Network Extensions** (Content Filter
     Provider) and **App Groups**.
3. Create two **Developer ID** provisioning profiles (distribution outside the
   Mac App Store), one per App ID, and download them.
4. Install a "Developer ID Application" certificate in your keychain.
5. For notarization, store credentials once:
   `xcrun notarytool store-credentials blocker-notary --apple-id you@example.com --team-id TEAMID`

## Build, sign, notarize

```sh
desktop/packaging/macos/build-app.sh \
  --team-id ABCDE12345 \
  --identity "Developer ID Application: Your Name (ABCDE12345)" \
  --app-profile ~/Downloads/app.provisionprofile \
  --filter-profile ~/Downloads/filter.provisionprofile \
  --notarize-profile blocker-notary \
  --out dist
```

Signing is done inside-out (extension, CLI as `dev.blocker.cli`, then app). Info.plist and entitlement
templates under `packaging/` contain `__TEAM_ID__` / `__VERSION__` placeholders
that the script substitutes into copies; templates are never modified.

Install and activate:

```sh
sudo ditto dist/Blocker.app /Applications/Blocker.app
/Applications/Blocker.app/Contents/MacOS/blocker activate
```

Launching Blocker.app starts the menu-bar UI (no Dock icon). The CLI at
`Contents/MacOS/blocker` is what the LaunchDaemon and docs use.

Approve the extension in System Settings > General > Login Items & Extensions
(Network Extensions / Content Filters).

## Optional: start the UI at login

`launch-agent.plist` is a template for a LaunchAgent that starts the menu-bar UI
at login. Install it manually (the build never does):

```sh
sudo cp packaging/launch-agent.plist /Library/LaunchAgents/dev.blocker.ui.plist
sudo chown root:wheel /Library/LaunchAgents/dev.blocker.ui.plist
launchctl bootstrap gui/$(id -u) /Library/LaunchAgents/dev.blocker.ui.plist
```

Remove with `launchctl bootout gui/$(id -u)/dev.blocker.ui` and delete the file.

## Local development without a paid profile

On a **test machine** only: disable SIP, then enable system extension developer
mode, and build ad-hoc:

```sh
systemextensionsctl developer on
desktop/packaging/macos/build-app.sh --adhoc --out dist
```

Ad-hoc builds embed no provisioning profiles and use team ID `ADHOCTEAM0`; they
are for structure testing and developer-mode activation only.

## Logs

```sh
log stream --predicate 'subsystem == "dev.blocker.filter"'
tail -f /Library/Logs/Blocker/daemon.log
systemextensionsctl list
```
