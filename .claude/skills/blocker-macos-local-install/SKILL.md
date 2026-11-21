---
name: blocker-macos-local-install
description: Use when installing Blocker.app locally on macOS.
category: deployment
---

# Blocker macOS local install

Probe the machine before saying what is missing. Docs describe the signed path; the machine decides which path is open.

Checks, in order:

- `security find-identity -v -p codesigning`: a "Developer ID Application" identity is required for the paid path. A dev cert such as "Local Dev" does not count.
- `find ~/Library/Developer/Blocker ~/Downloads -maxdepth 1 -name "*.provisionprofile"`: two Developer ID profiles are needed (app and filter); they belong in `~/Library/Developer/Blocker/`.
- `security find-certificate -c "Developer ID Certification Authority"` must find the G2 intermediate, and `security dump-trust-settings` must show no custom trust on the Developer ID certificate. See `desktop/packaging/macos/README.md`.
- `csrutil status`: SIP must be off only for the ad-hoc test-machine path (`systemextensionsctl developer on`). Never recommend it for the main Mac.
- `ls /Applications | grep -i blocker`: the app must live in `/Applications` for the extension to activate.

Paths, from `desktop/packaging/macos/README.md`:

1. Paid account (real path): `desktop/packaging/macos/build-app.sh --team-id ... --identity ... --app-profile ... --filter-profile ... --notarize-profile ... --out dist`, then install and activate.
2. Test machine only: `build-app.sh --adhoc --out dist` (team `ADHOCTEAM0`, no profiles). Structure and developer-mode activation only.
3. No website blocking: the background service and app closing may still work. Unverified; say so.

Install and activate: `sudo ditto dist/Blocker.app /Applications/Blocker.app`, then `/Applications/Blocker.app/Contents/MacOS/blocker activate`, then approve in System Settings > General > Login Items & Extensions. Logs: `log stream --predicate 'subsystem == "dev.blocker.filter"'`.

Also needed for category lists: a list server (`blocker-server keygen/build/serve`) and its public key. Domain-only rules (`-d`) should not need it; verify before claiming.

Platform status: macOS has website blocking through the filter. Linux and Windows have CLI, list updates and tray only, plus app killing by process name; no packaging. Cite `README.md` and `desktop/README.md` when answering.
