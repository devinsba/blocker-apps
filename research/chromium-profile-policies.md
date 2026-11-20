# Chromium policies from a user-installed profile (macOS, no MDM)

Question (issue #5, plan Phase 0 S2 and the Chromium half of S3): on a Mac with no
MDM, does a manually installed `.mobileconfig` make Chrome, Edge and Brave apply
managed policies? What are the exact names, types and values of
`ForceGoogleSafeSearch`, `ForceYouTubeRestrict`, `ForceBingSafeSearch` and
`EncryptedClientHelloEnabled`, and each browser's preference domain?

Researched 2026-11-20 from Chromium source (main branch), Apple's
`apple/device-management` schema repo, Microsoft Learn and Google's Chrome
Enterprise help. Not yet confirmed hands-on; see "Open points".

## Short answer

Yes, by the documented mechanism and by the source code. A manually installed
profile carrying a `com.apple.ManagedClient.preferences` payload with `Forced`
settings for the browser's domain is allowed on macOS without MDM. Chromium reads
forced (managed) preference values as **mandatory** policy. It only drops a short
list of "sensitive" policies on non-MDM, non-domain-joined Macs, and none of the
four policies we need is on that list.

## 1. macOS: manual install of a managed-preferences payload

- The Managed Preferences payload (`com.apple.ManagedClient.preferences`) is
  supported on macOS since 10.7 with `allowmanualinstall: true`,
  `supervised: false`, `userapprovedmdm: false`, on both device and user channels.
  Its `PayloadContent` is keyed by app preference domain, each holding `Forced`
  and/or `Set-Once` arrays of `mcx_preference_settings`.
  Source: [apple/device-management `com.apple.ManagedClient.preferences.yaml`](https://github.com/apple/device-management/blob/release/mdm/profiles/com.apple.ManagedClient.preferences.yaml)
- Since macOS 11 the `profiles` CLI can no longer install profiles; a
  downloaded/opened profile has to be approved by the user in System Settings
  (now General > Device Management, formerly Privacy & Security > Profiles), and an
  unapproved one is discarded after about eight minutes. There is no API to
  install silently; only MDM can.
  Sources: [WWDC20 "What's new in managing Apple devices"](https://developer.apple.com/videos/play/wwdc2020/10639/),
  [Tech Talk "Deploy macOS Big Sur in your organization"](https://developer.apple.com/videos/play/tech-talks/10870/),
  [Apple Support: configuration profiles on Mac](https://support.apple.com/guide/mac-help/configuration-profiles-standardize-settings-mh35561/mac)
- Google's own Mac instructions build exactly this kind of profile: copy the
  bundled `com.google.Chrome.plist` MCX example, convert it with `mcxToProfile`
  to `com.google.Chrome.mobileconfig`, deploy. Google's docs then say to upload it
  to an MDM; they do not document manual install, but the payload is the same one
  Apple allows manually.
  Source: [Chrome Enterprise help: set Chrome browser policies on managed computers](https://support.google.com/chrome/a/answer/187202?hl=en)

### Removal (user override)

- Profile removal: top-level `PayloadRemovalDisallowed = true` on macOS 10.15+
  "only affects removal of _manually_ installed profiles": removing it then needs
  admin auth, or the removal password if a `com.apple.profileRemovalPassword`
  payload is present (that payload is also `allowmanualinstall: true` on macOS).
  An admin who knows the password can still remove it.
  Sources: [TopLevel.yaml](https://github.com/apple/device-management/blob/release/mdm/profiles/TopLevel.yaml),
  [com.apple.profileRemovalPassword.yaml](https://github.com/apple/device-management/blob/release/mdm/profiles/com.apple.profileRemovalPassword.yaml)
- So a self-locking user (likely an admin) can always remove the profile.
  Blocker can only detect it (plan: `ProfileRemoved` event) and react. A removal
  password held by the keyholder would make key-locked rules meaningful here.

## 2. Chromium: how policy is loaded on Mac

From [`components/policy/core/common/policy_loader_mac.mm`](https://chromium.googlesource.com/chromium/src/+/main/components/policy/core/common/policy_loader_mac.mm):

- For each known policy, Chromium reads `CFPreferencesCopyAppValue(name, domain)`.
  If `CFPreferencesAppValueIsForced` is true the policy is
  `POLICY_LEVEL_MANDATORY`, otherwise `POLICY_LEVEL_RECOMMENDED`. Scope is
  `MACHINE` when the value is in `/Library/Managed Preferences/<domain>.plist`,
  else `USER` (`/Library/Managed Preferences/<login>/<domain>.plist`).
- Values the user writes with `defaults write` into `~/Library/Preferences` are
  not forced, so they load only as **recommended** and cannot override a forced
  value. This is the main reason a user can't simply undo the profile's settings.
- After loading, if the Mac is not MDM-enrolled and not AD/OD-joined
  (`base::IsManagedOrEnterpriseDevice()`, which parses `/usr/bin/profiles status`
  for "MDM enrollment"/"Enrolled via DEP"; [`base/enterprise_util_mac.mm`](https://chromium.googlesource.com/chromium/src/+/main/base/enterprise_util_mac.mm)),
  Chromium calls `FilterSensitivePolicies`. That blocks only policies marked
  `sensitive: true` in their definitions plus non-Web-Store entries of
  `ExtensionInstallForcelist`/`ExtensionSettings`
  ([`policy_loader_common.cc`](https://chromium.googlesource.com/chromium/src/+/main/components/policy/core/common/policy_loader_common.cc),
  [`generate_policy_source.py`](https://chromium.googlesource.com/chromium/src/+/main/components/policy/tools/generate_policy_source.py):
  "policies that are considered only if the user is part of an AD domain on
  Windows, managed on Mac, or enrolled in Chrome Enterprise Core").
  A newer flag, `kUseManagementServiceForSensitivePolicies`, changes how
  "managed" is decided but still only filters that same sensitive list.
- None of `ForceGoogleSafeSearch`, `ForceYouTubeRestrict` or
  `EncryptedClientHelloEnabled` has `sensitive: true` (definitions below), so they
  apply on an unmanaged Mac.

### Preference domain per browser

From [`chrome/browser/policy/chrome_browser_policy_connector.cc`](https://chromium.googlesource.com/chromium/src/+/main/chrome/browser/policy/chrome_browser_policy_connector.cc):
Google Chrome branding hard-codes `com.google.Chrome` "no matter what this app's
bundle ID actually is. All channels of Chrome should obey the same policies."
Chrome for Testing uses `com.google.ChromeForTesting`; any other build (Chromium,
Brave and other forks) uses its own base bundle ID.

| Browser | Domain | Notes |
|---|---|---|
| Google Chrome (Stable, Beta, Dev, Canary) | `com.google.Chrome` | one domain for all channels |
| Chrome for Testing | `com.google.ChromeForTesting` | separate; **not covered** unless added |
| Microsoft Edge (all channels, 78.0.249.2+) | `com.microsoft.Edge` | [Microsoft Learn](https://learn.microsoft.com/en-us/deployedge/configure-microsoft-edge-on-mac) |
| Brave (release) | `com.brave.Browser` | `MAC_BUNDLE_ID` in [brave-core `app/theme/brave/BRANDING`](https://github.com/brave/brave-core/blob/master/app/theme/brave/BRANDING) |
| Brave Beta / Nightly | `com.brave.Browser.beta` / `com.brave.Browser.nightly` | follows from the bundle-ID rule; Brave's support page says the same but blocked automated fetches, so this is unverified |
| Chromium | `org.chromium.Chromium` | follows from the bundle-ID rule; unverified |

brave-core does not override `policy_loader_mac` (its `chromium_src/components/policy/core/common`
only patches `policy_map.cc` and `policy_types.h`), so Brave inherits the loader above.

## 3. Policy definitions

Chrome definitions are from
[`components/policy/resources/templates/policy_definitions/Miscellaneous/`](https://chromium.googlesource.com/chromium/src/+/main/components/policy/resources/templates/policy_definitions/Miscellaneous/)
(`ForceGoogleSafeSearch.yaml`, `ForceYouTubeRestrict.yaml`,
`EncryptedClientHelloEnabled.yaml`). Edge definitions are from Microsoft Learn
([ForceGoogleSafeSearch](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-policies/forcegooglesafesearch),
[ForceYouTubeRestrict](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-policies/forceyoutuberestrict),
[ForceBingSafeSearch](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-policies/forcebingsafesearch),
[EncryptedClientHelloEnabled](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-policies/encryptedclienthelloenabled)).

| Key | Plist type | Value we set | Chrome | Edge (macOS) | Recommended? | Per profile |
|---|---|---|---|---|---|---|
| `ForceGoogleSafeSearch` | `<true/>` boolean | `true` | 41+ | 77+ | no (mandatory only) | yes |
| `ForceYouTubeRestrict` | `<integer>` enum 0 Off / 1 Moderate / 2 Strict | `2` (Strict) | 55+ | 77+ | no | yes |
| `ForceBingSafeSearch` (Edge only) | `<integer>` enum 0 NoRestrictions / 1 Moderate / 2 Strict | `2` (Strict) | n/a | 77+ | no | yes |
| `EncryptedClientHelloEnabled` | `<false/>` boolean, default `true` | `false` | 105+ | 108+ | Chrome: not stated; Edge: no | no (machine-wide) |

All four support dynamic refresh (no browser restart needed). Exact wording:

- `ForceGoogleSafeSearch`: "Setting the policy to Enabled means SafeSearch in
  Google Search is always active, and users can't change this setting."
- `ForceYouTubeRestrict` Strict: "Strict Restricted mode on YouTube is always
  active." Chrome's YAML says it has no effect on the Android YouTube app; on Mac it
  only affects YouTube in the browser.
- `ForceBingSafeSearch` Strict: "the strict setting filters adult text, images,
  and videos"; "Users can't change this setting."
- `EncryptedClientHelloEnabled`: "If it is disabled, Google Chrome will not
  enable ECH." Both vendors call it "a temporary measure to control the initial
  experimental implementation. It will be replaced with final controls as the
  protocol finalizes." Chrome's YAML carries the `system-security` tag but not
  `sensitive: true`. **Risk:** the key could be renamed or removed; watch release
  notes and keep the filter's ECH fallback (plan 1.5).

Brave: it uses the Chromium policy schema, so the same keys apply. No first-party
Brave doc was readable (support.brave.app returned 403 to automated fetches).

## 4. Ways around it that the profile does not close

- Removing the profile (admin, or anyone with the removal password).
- Using a Chromium browser on a domain the profile doesn't cover (Chrome for
  Testing, Chromium, Brave Beta/Nightly, Vivaldi, Opera, Arc and so on). GLOSSARY
  "SafeSearch" already says to block browsers that aren't covered.
- Using Google/Bing/YouTube in a browser with no such policies (Safari, Firefox):
  covered by other plan items (4.2/4.3).
- Chrome shows "Managed by your organization" once platform policy is present.
  This is UX, not a bypass, but the setup wizard should explain it.

## Open points for the hands-on ticket

"Hands-on: browser profile and Screen Time on an unmanaged Mac" should confirm on
current macOS and current browser builds:

1. After the manual install, `chrome://policy`, `edge://policy` and `brave://policy`
   list all four keys as Source "Platform", Level "Mandatory", Scope "Machine"
   (device-scoped profile), Status OK.
2. Google SafeSearch is locked on, YouTube is Strict, Bing is Strict, and ECH is
   off (for example, the `chrome://net-export` log or a known ECH test site shows no ECH).
3. Removal with `PayloadRemovalDisallowed` plus `com.apple.profileRemovalPassword`
   behaves as documented.
4. Brave Beta/Nightly domain names, and whether Brave adds any policy gating.
