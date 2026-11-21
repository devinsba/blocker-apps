# Packaging Blocker.app

`build-app.sh` assembles `Blocker.app` containing the menu-bar UI (main executable,
`Contents/MacOS/blocker-ui`), the CLI (`Contents/MacOS/blocker`)
and the content-filter system extension
(`Contents/Library/SystemExtensions/com.briandevins.blocker.filter.systemextension`).
The app must live in `/Applications` for the system extension to be activated.

## Requirements

- Apple Developer Program membership (paid). Network Extension and System
  Extension entitlements are restricted and require provisioning profiles.
  Only the team's **Account Holder** can create Developer ID certificates.
- Xcode command line tools (`xcode-select --install`). Full Xcode is optional
  and only makes creating the certificate easier.
- Rust from rustup. `build-app.sh` runs `cargo`, so rustup's bin directory must
  be on `PATH` (`~/.cargo/bin`, or `/opt/homebrew/opt/rustup/bin` for
  Homebrew's rustup).

## One-time machine setup

Do these in order: the profiles need the App IDs and the certificate. Portal
steps are at [developer.apple.com/account](https://developer.apple.com/account)
under **Certificates, IDs & Profiles**.

### 1. Developer ID Application certificate

With Xcode: **Settings → Accounts →** your team **→ Manage Certificates… → +
→ Developer ID Application**.

Without Xcode: Keychain Access **→ Certificate Assistant → Request a
Certificate From a Certificate Authority…** (saved to disk), then in the portal
**Certificates → + → Developer ID Application** (G2 Sub-CA), upload the
request, download the `.cer` and double-click it.

Then install Apple's intermediate, which the certificate chains through.
Without it, signing fails with `errSecInternalComponent` / "unable to build
chain to self-signed root":

```sh
curl -fsSLO https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer
security add-certificates -k ~/Library/Keychains/login.keychain-db DeveloperIDG2CA.cer
```

Leave the certificate's trust on **Use System Defaults**. Setting it to "Always
Trust" makes `codesign` treat it as a root: signatures lose the Team ID and
verification fails with "nested code is modified or invalid". Check with:

```sh
security find-identity -v -p codesigning   # "Developer ID Application: Name (TEAMID)"
security dump-trust-settings               # should print "No Trust Settings were found"
```

The 10 characters in brackets are the Team ID. Export the certificate and its
private key as a `.p12` backup; a team can only have a few Developer ID
certificates.

### 2. App Group

**Identifiers → + → App Groups**: `group.com.briandevins.blocker`. The filter's
`NEMachServiceName` (`group.com.briandevins.blocker.filter`) must start with it.

### 3. App IDs

**Identifiers → + → App IDs → App**, explicit bundle ID:

| Bundle ID | Capabilities |
|---|---|
| `com.briandevins.blocker` | App Groups, Network Extensions, System Extension |
| `com.briandevins.blocker.filter` | App Groups, Network Extensions |

For each, **Configure** next to App Groups and tick
`group.com.briandevins.blocker`, or the profiles won't carry the group.

### 4. Developer ID provisioning profiles

**Profiles → + → Distribution → Developer ID**, one per App ID, each with the
certificate from step 1. Regenerate both whenever a capability changes.

Keep them outside the repo (it is public) in a fixed place, for example:

```sh
mkdir -p ~/Library/Developer/Blocker
mv ~/Downloads/Blocker.provisionprofile ~/Downloads/Blocker_Filter.provisionprofile \
  ~/Library/Developer/Blocker/
```

They are not secret (no private key), but they expire with the certificate and
are tied to this team. `.gitignore` excludes `*.provisionprofile` in case one
lands in the tree.

### 5. Notarization credentials

Create an app-specific password at [account.apple.com](https://account.apple.com)
(Sign-In and Security → App-Specific Passwords), then store it in the keychain:

```sh
xcrun notarytool store-credentials blocker-notary --apple-id you@example.com --team-id TEAMID
```

## Build, sign, notarize

```sh
desktop/packaging/macos/build-app.sh \
  --team-id TEAMID \
  --identity "Developer ID Application: Your Name (TEAMID)" \
  --app-profile ~/Library/Developer/Blocker/Blocker.provisionprofile \
  --filter-profile ~/Library/Developer/Blocker/Blocker_Filter.provisionprofile \
  --notarize-profile blocker-notary \
  --out dist
```

A good build ends with "satisfies its Designated Requirement", notary status
`Accepted` and "The staple and validate action worked!". Confirm Gatekeeper:

```sh
spctl -a -vv -t exec dist/Blocker.app   # source=Notarized Developer ID
```

Signing is done inside-out (extension, CLI as `dev.blocker.cli`, then app). Info.plist and entitlement
templates under `packaging/` contain `__TEAM_ID__` / `__VERSION__` placeholders
that the script substitutes into copies; templates are never modified.

Install and activate:

```sh
sudo rm -rf /Applications/Blocker.app
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
