# macOS: one .pkg for app + out-of-bundle daemon, BTM, SMAppService, Sparkle

Question (issue #10, plan Phase 0 S8, tasks 2.5, 6.1, 6.2): can one notarized
Developer ID `.pkg` install `Blocker.app` in `/Applications`, the daemon at
`/Library/Application Support/Blocker/bin/blocker` and
`/Library/LaunchDaemons/dev.blocker.daemon.plist` (bootstrapped in
`postinstall`), with the system extension still activated by the app? What does
Background Task Management (BTM, macOS 13+) show, and can the user switch the
daemon off? `SMAppService.daemon` or a legacy plist? Can Sparkle update both
the app and the daemon, and never offer a downgrade?

Researched 2026-11-20 from Apple developer docs (via their JSON endpoints),
the `launchd.plist(5)` and `pkgbuild(1)` man pages on macOS 27.2, Sparkle's docs
and Sparkle 2.x source. Desk research only: nothing here was run on a Mac.

## Answer

1. **Yes, one `.pkg` can do all of it.** Nothing in Apple's docs stops a
   Developer ID Installer-signed, notarized product archive from laying down an
   app in `/Applications`, a signed binary elsewhere and a plist in
   `/Library/LaunchDaemons`. Apple's own SMAppService sample `.pkg` installs a
   helper app into `/Library/Application Support/<developer>` [A4]. The system
   extension is not installed by the package: the app must request activation
   from `Contents/Library/SystemExtensions`, and activation checks that the app
   sits in an Applications directory [A5]. So `postinstall` bootstraps the
   daemon and then opens the app in the console user's session, and the app
   (or `blocker activate`) submits the activation request. The user still
   approves the extension in System Settings.
2. **BTM always shows the daemon and gives it a switch; without MDM that can't
   be avoided.** Apple: "Every app that supports a `LoginItem`, `LaunchAgent`,
   or `LaunchDaemon` has a corresponding switch in the Login Items panel"
   [A3]. That applies to legacy plists and SMAppService alike. The only way to
   pre-approve or lock items is the `com.apple.servicemanagement` (Managed
   Login Items) payload, which needs **user-approved MDM** [A6, A7], which this
   product won't have. Design for the switch being turned off.
3. **Prefer the legacy plist for the daemon.** `SMAppService.daemon` needs the
   plist in the registering app's `Contents/Library/LaunchDaemons` and a
   bundle-relative `BundleProgram` [A2, M1], so the daemon lives inside *some*
   bundle and dies with it. It also needs the user to approve it in System
   Settings before it can run (`requiresApproval`) [A3, A8]. A legacy plist
   written by a root `postinstall` needs no extra approval step, only the
   "Background Items Added" notification [A7]. Both get the same Login Items
   switch, so SMAppService buys no tamper resistance.
4. **Sparkle can update both, but only through package updates, and those
   always prompt for an admin password.** For a `.pkg` enclosure Sparkle runs
   `/usr/sbin/installer -pkg <pkg> -target /` as root [S4]. Package updates
   always need user authorization, so they can't install silently, and they get
   no delta updates and no key-rotation fallback [S1]. Sparkle's own advice is
   that apps that install daemons or system extensions don't need package
   installers [S1]. The other option is a plain app-bundle update, after which
   the root daemon copies its new binary out of the updated bundle itself (see
   Recommendation).
5. **Never offering a downgrade: yes, with signed feeds.** Sparkle offers an
   update only when `isItemNewer:` holds, meaning host version < item version
   (`SUStandardVersionComparator`) [S5]. Sparkle itself says package installs
   are open to downgrade attacks and recommends opting into **signed feeds**
   (`SURequireSignedFeed`, which needs `SUVerifyUpdateBeforeExtraction` and
   EdDSA) [S1, S2, S3]. The feed-signing failure expiry fallback is "not
   supported for installer package updates" [S2]. The `.pkg` itself does no
   downgrade check for a plain file such as the daemon binary
   (`BundleIsVersionChecked` covers bundles only [M2]), so the daemon's
   `schema_version` and last-run-version check (plan 2.8) is still needed.

## Details

### Package layout and signing

- Sign the app, the extension and the daemon binary with **Developer ID
  Application** and the hardened runtime. Sign the product archive with
  **Developer ID Installer** (`productbuild --sign` or `productsign`) [A9].
  Notarize the `.pkg` with `notarytool`; the service tickets nested items too.
  Staple the `.pkg` [A10].
- Bundle component plist: set `BundleIsRelocatable` to false. It defaults to
  false only for packages built with `pkgbuild` on macOS 27 and later [M2];
  otherwise the installer may update a moved copy instead of `/Applications`.
  `BundleIsVersionChecked` stops a package from overwriting a newer app bundle
  on disk [M2].
- The legacy plist should carry `AssociatedBundleIdentifiers =
  dev.blocker.app` so Login Items shows "Blocker" instead of the certificate's
  organization name. The Team ID of the `ProgramArguments` executable must
  match the app's, and Launch Services must know the app [A3, M1].
  `launchd.rs::render_plist` doesn't emit this key today.
- `postinstall` order: copy and sign-check the daemon, write the plist
  root:wheel 0644, `launchctl bootout` then `bootstrap system`, then
  `launchctl asuser <console uid> open /Applications/Blocker.app` for the
  wizard and extension activation.

### What BTM does to a legacy daemon

- Installing it posts one notification that background items were added
  (no repeat for 24 h), and the item appears under System Settings > General >
  Login Items & Extensions [A7].
- The user can switch it off. When switched off, launchd refuses to spawn it
  ("Service cannot be launched because of BTM policy") [F1]. The app can read
  that state with `SMAppService.statusForLegacyPlist(at:)` [A3].
- Since macOS 26, quitting an app with background tasks may prompt the user
  to keep or stop them; MDM rules suppress that prompt [A7].
- BTM data is cleaned up lazily (overnight maintenance). `sfltool dumpbtm` and
  `sfltool resetbtm` are the diagnostics [A4, A7, F1].
- Not found in primary docs: whether switching off a *daemon* (not an agent)
  asks for an admin password. The hands-on ticket must check this.

Tamper implications: a switched-off daemon is not running, so it can't log its
own disablement. The filter (a system extension, not in BTM) keeps enforcing
its last `config.toml`. The menu-bar app should check
`statusForLegacyPlist` and the daemon's socket on each launch, log
`DaemonMissing`, and show the attention state. Without MDM this is detection,
not prevention; the same is true of SMAppService.

### SMAppService.daemon trade-offs

| | Legacy plist in `/Library/LaunchDaemons` | `SMAppService.daemon` |
|---|---|---|
| Binary location | anywhere (`/Library/Application Support/Blocker/bin`) | inside the registering bundle (`BundleProgram`, bundle-relative) [M1] |
| Survives deleting Blocker.app | yes, if the binary is outside the app (S7 must confirm) | only if the host bundle is a separate helper bundle (Apple's sample puts one in `/Library/Application Support`) [A4] |
| Extra approval | none (notification only) | user approves in Login Items; `requiresApproval` until then, and again if revoked [A8] |
| Login Items switch | yes [A3] | yes [A3] |
| Plist integrity | root-owned file, writable by an admin | covered by the bundle's code signature [A4] |
| Registration | `launchctl bootstrap` from `postinstall` | `register()` must be called by the bundle's own process [A2, A4] |

Conclusion: the legacy plist fits the plan (daemon outside the bundle, no extra
approval step). Revisit SMAppService only if Apple deprecates legacy plists.

### System extension and app deletion

- "The system automatically uninstalls any system extensions when the user
  deletes the corresponding app" [A5]. So deleting `Blocker.app` removes the
  filter even though the out-of-bundle daemon survives. The daemon's
  `AppMissing` handling (plan 2.5) should treat this as filter loss.
- Activation checks: extension in `Contents/Library/SystemExtensions`, app in
  an Applications directory, valid signature and entitlements [A5]. Updating
  the extension means activating the new version again; the delegate's
  `actionForReplacingExtension` decides [A5]. After any app update (Sparkle or
  `.pkg`) the app must submit the activation request again.

### Sparkle options for 6.2

- **A. Package updates** (enclosure is a flat `.pkg`, the "bare" form Sparkle
  recommends [S1]): one artifact updates app, daemon and plist through
  `postinstall`. Costs: an admin password on every update, no silent installs,
  no deltas, no key rotation fallback [S1]. Must turn on signed feeds against
  downgrades [S1].
- **B. App-bundle updates plus daemon self-update**: Sparkle swaps
  `Blocker.app` the normal way. The root daemon notices the new
  `Contents/MacOS/blocker`, checks its code signature (Team ID, identifier
  `dev.blocker.cli`) and that its version is higher, copies it to
  `/Library/Application Support/Blocker/bin`, and restarts itself. This
  matches Sparkle's "daemons ... do not need package installers" advice [S1].
  Cost: the copy-and-verify code in the daemon, which must reject lower versions.
- In both cases set `SURequireSignedFeed`, `SUVerifyUpdateBeforeExtraction`
  and EdDSA signing [S2, S3]. `sparkle:criticalUpdate` stops the user skipping
  an update [S3].
- The UI is Rust, not AppKit, so Sparkle needs embedding through an
  Objective-C bridge (`SPUStandardUpdaterController`). Not researched here.

Recommendation: B. It avoids an admin prompt on every update, and the
daemon-side version check is needed anyway (plan 2.8). Keep the `.pkg` for
first install and for a manual reinstall.

## What the hands-on ticket must confirm

On a clean macOS 13+ Mac with a signed, notarized `.pkg`:

1. The install puts all three pieces in place; `postinstall` bootstrap works;
   `launchctl print system/dev.blocker.daemon` shows it running.
2. The app opened by `postinstall` activates the extension from
   `/Applications`, and the approval flow works.
3. What Login Items shows for the daemon (name with
   `AssociatedBundleIdentifiers`), and whether switching it off needs an admin
   password. Does it stay off across a reboot? What does
   `statusForLegacyPlist` return?
4. S7: delete `Blocker.app`. The daemon keeps running? The extension is
   removed? Does BTM drop the daemon's entry or its approval?
5. Install an older `.pkg` over a newer one: what happens to the app
   (`BundleIsVersionChecked`) and to the daemon binary.

## Sources

- [A2] Apple, `SMAppService.daemon(plistName:)`: "The property list name must correspond to a property list in the calling app's `Contents/Library/LaunchDaemons` directory". https://developer.apple.com/documentation/servicemanagement/smappservice/daemon(plistname:)
- [A3] Apple, Updating helper executables from earlier versions of macOS (Login Items switch, `AssociatedBundleIdentifiers`, Team ID match, `statusForLegacyPlist`, "Launch daemons require authentication by the user"). https://developer.apple.com/documentation/servicemanagement/updating-helper-executables-from-earlier-versions-of-macos
- [A4] Apple sample, Updating your app package installer to use the new Service Management API (helper in `/Library/Application Support`, plists covered by code signature, `sfltool resetbtm`). https://developer.apple.com/documentation/servicemanagement/updating-your-app-package-installer-to-use-the-new-service-management-api
- [A5] Apple, Installing system extensions and drivers (activation checks, auto-uninstall on app deletion, updates). https://developer.apple.com/documentation/systemextensions/installing-system-extensions-and-drivers
- [A6] Apple, ServiceManagementManagedLoginItems payload (requires user-approved MDM). https://developer.apple.com/documentation/devicemanagement/servicemanagementmanagedloginitems
- [A7] Apple Platform Deployment, Manage login items and background tasks on Mac (notifications, 24 h, macOS 26 prompt, `sfltool`). https://support.apple.com/guide/deployment/manage-login-items-background-tasks-mac-depdca572563/web
- [A8] Apple, `SMAppService.Status.requiresApproval`. https://developer.apple.com/documentation/servicemanagement/smappservice/status-swift.enum/requiresapproval
- [A9] Apple, Resolving common notarization issues (Developer ID Installer for packages, `pkgutil --check-signature`). https://developer.apple.com/documentation/security/resolving-common-notarization-issues
- [A10] Apple, Customizing the notarization workflow (flat installer packages, nested tickets, stapling). https://developer.apple.com/documentation/security/customizing-the-notarization-workflow
- [M1] `launchd.plist(5)`, macOS 27.2: `BundleProgram` "only supported for plists that are installed using SMAppService"; `AssociatedBundleIdentifiers`.
- [M2] `pkgbuild(1)`, macOS 27.2: `BundleIsRelocatable` (defaults to false on macOS 27+), `BundleIsVersionChecked`, `BundleOverwriteAction`.
- [S1] Sparkle, Package Updates. https://sparkle-project.org/documentation/package-updates/
- [S2] Sparkle, Customization (`SURequireSignedFeed`, `SUVerifyUpdateBeforeExtraction`, signed feed failure expiry not supported for packages). https://sparkle-project.org/documentation/customization/
- [S3] Sparkle, Publishing an update (signed appcasts, critical updates, `minimumAutoupdateVersion`). https://sparkle-project.org/documentation/publishing/
- [S4] Sparkle source, `Autoupdate/SUGuidedPackageInstaller.m` (`/usr/sbin/installer -pkg … -target /`, run as root). https://github.com/sparkle-project/Sparkle/blob/2.x/Autoupdate/SUGuidedPackageInstaller.m
- [S5] Sparkle source, `Sparkle/SUAppcastDriver.m` (`isItemNewer:` gate). https://github.com/sparkle-project/Sparkle/blob/2.x/Sparkle/SUAppcastDriver.m
- [F1] Apple Developer Forums thread 717609 (Apple engineers: BTM policy denial log line, `AssociatedBundleIdentifiers`, lazy cleanup). https://developer.apple.com/forums/thread/717609
