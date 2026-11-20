# Firefox enterprise policy: ECH lock, SafeSearch, and macOS delivery

Answers ticket #6 (plan `docs/plans/macos-public-release.md`, Phase 0 S3 and
Phase 4.3). Researched 2026-11-20 against Mozilla's docs and source (`main` =
163.0a1, `release` = 161.0, `beta` = 162.0, `esr153`, `esr140`).

## Answer

- **ECH: yes.** A dedicated policy, `DisableEncryptedClientHello: true`,
  sets *and locks* both `network.dns.echconfig.enabled` and
  `network.dns.http3_echconfig.enabled` to false (Firefox 127+, ESR 128+).
  The generic `Preferences` policy can also lock them, because `network.` is an
  allowed prefix. Use the dedicated policy because it is one boolean.
- **SafeSearch: no policy exists.** Firefox has nothing like Chrome's
  `ForceGoogleSafeSearch` or `ForceYouTubeRestrict`. The `SearchEngines` policy
  can only make address-bar searches go to a SafeSearch URL. It does nothing
  when the user opens google.com, bing.com or youtube.com directly.
- **Delivery: use `/Library/Preferences/org.mozilla.firefox`, not the app
  bundle.** Mozilla now documents this as the way to set policies on a Mac
  without device management. A root daemon can write it, and Firefox updates
  don't touch it. Writing `policies.json` into `Firefox.app` breaks the code
  signature. Mozilla says macOS can then refuse to open Firefox as "damaged",
  and Mozilla is preparing to refuse privileged updates for modified bundles.
- **Can the user override it?** On Firefox 160+ (with the source-restriction
  fix, which is on `release` 161 and was uplifted to `esr153`), no: policies
  from the user's own preferences rank below admin sources, and
  `DisableMacOSUserPolicies` drops them entirely. On ESR 140 and older,
  **yes**: a standard user can override the policy from
  `~/Library/Preferences/org.mozilla.firefox` with `defaults write`.

## 1. Locking ECH off

### Dedicated policy

`DisableEncryptedClientHello` "Disable[s] the TLS Feature for Encrypted Client
Hello. Note that TLS Client Hellos will still contain an ECH extension, but this
extension will not be used by Firefox during the TLS handshake." Compatibility:
Firefox 127, ESR 128, Firefox Enterprise 149. Preferences affected:
`network.dns.echconfig.enabled`, `network.dns.http3_echconfig.enabled`.
([admin docs](https://firefox-admin-docs.mozilla.org/reference/policies/disableencryptedclienthello/))

The implementation locks both prefs
([`Policies.sys.mjs`](https://github.com/mozilla-firefox/firefox/blob/main/browser/components/enterprisepolicies/Policies.sys.mjs)):

```js
DisableEncryptedClientHello: {
  onBeforeAddons(manager, param) {
    if (param) {
      lazy.PoliciesUtils.setAndLockPref("network.dns.echconfig.enabled", false);
      lazy.PoliciesUtils.setAndLockPref("network.dns.http3_echconfig.enabled", false);
    }
  },
},
```

The ECH extension that remains is GREASE. `security.tls.ech.grease_probability`
defaults to 100
([`StaticPrefList.yaml`](https://github.com/mozilla-firefox/firefox/blob/main/modules/libpref/init/StaticPrefList.yaml)).
With real ECH off, the real host goes out in the clear SNI, which the SNI
check reads. That is an inference from how GREASE ECH works and still needs a
hands-on check (packet capture or the filter's `ech-fallback` counter).

### Preferences policy

`Preferences` sets prefs with `Status` `"default"`, `"locked"`, `"user"` or
`"clear"`. `"locked"` means "Read-Only" (Firefox 81 / ESR 78.3+). Only listed
prefixes are accepted, and `network.` is on the list
([policy-templates docs](https://github.com/mozilla/policy-templates/blob/master/docs/index.md#preferences),
which say they are superseded by the admin docs;
[`allowedPrefixes` in source](https://github.com/mozilla-firefox/firefox/blob/main/browser/components/enterprisepolicies/Policies.sys.mjs)).
`security.tls.ech.*` is not allowed. The only `security.` prefs allowed are a
fixed list, and no ECH pref is on it.

```json
{ "policies": { "Preferences": {
  "network.dns.echconfig.enabled":       { "Value": false, "Status": "locked" },
  "network.dns.http3_echconfig.enabled": { "Value": false, "Status": "locked" }
} } }
```

Both prefs are boolean and default to `true`
([`StaticPrefList.yaml`](https://github.com/mozilla-firefox/firefox/blob/main/modules/libpref/init/StaticPrefList.yaml)).

Related: Firefox resolves HTTPS records through the platform DNS by default
(`network.dns.native_https_query: true`), so Firefox can get ECH configs even
without DoH. Locking DoH off (`DNSOverHTTPS: { "Enabled": false, "Locked": true }`)
doesn't replace the ECH lock. It only matters if the product relies on seeing
DNS.

## 2. SafeSearch

- The full policy list at
  [firefox-admin-docs.mozilla.org/reference/policies](https://firefox-admin-docs.mozilla.org/reference/policies/)
  has no SafeSearch, safe-search or YouTube restricted-mode policy. The source
  has no `safesearch` or `safe=active` either.
- `SearchEngines` (`Add`, `Default`, `Remove`, `PreventInstalls`) works in all
  Firefox builds since 139; before that it was ESR-only.
  ([policy-templates docs](https://github.com/mozilla/policy-templates/blob/master/docs/index.md#searchengines))
  Workaround: add an engine whose `URLTemplate` is
  `https://www.google.com/search?q={searchTerms}&safe=active`, make it the
  `Default`, `Remove` the built-in engines, and set `PreventInstalls`. This only
  covers searches typed in the address bar and search bar.
- Other options, outside Firefox: block the SafeSearch-less search endpoints at
  the filter, or treat Firefox as a browser that is not covered while
  SafeSearch is on (Phase 4.4). `WebsiteFilter` blocks URL patterns, but it
  can't rewrite query parameters.

## 3. Delivering policies on macOS without MDM

### Where Firefox reads policies (current source)

`EnterprisePoliciesParent._buildProvider()` combines the JSON provider
(`policies.json` in `XREAppDist`, which is `Firefox.app/Contents/Resources/distribution`)
with the macOS provider. The macOS provider is added last and takes precedence
for any top-level policy both sources set
([`EnterprisePoliciesParent.sys.mjs`](https://github.com/mozilla-firefox/firefox/blob/main/toolkit/components/enterprisepolicies/EnterprisePoliciesParent.sys.mjs)).
ESR 140 does the same through
`CombinedProvider(platformProvider, jsonProvider)`.

The macOS reader
([`nsMacPreferencesReader.mm`](https://github.com/mozilla-firefox/firefox/blob/main/xpcom/base/nsMacPreferencesReader.mm),
[`macOSPoliciesParser.sys.mjs`](https://github.com/mozilla-firefox/firefox/blob/main/toolkit/components/enterprisepolicies/macOSPoliciesParser.sys.mjs))
uses three sources:

| Source | Where | Who can write it |
|---|---|---|
| `managed` | `/Library/Managed Preferences/<bundle id>.plist`, used only if it is owned by root and not group- or world-writable | device-scope configuration profile |
| `system` | `CFPreferences` with `kCFPreferencesAnyUser`, i.e. `/Library/Preferences/org.mozilla.firefox.plist` | root |
| `user` | the current user's domain (`~/Library/Preferences/...`, any host and current host), plus user-scope profiles | the user |

Admin policies are `{ ...system, ...managed }`. User policies only fill in
policies no admin source sets, and `DisableMacOSUserPolicies` (set by an admin
source) drops them. Policies are on if `EnterprisePoliciesEnabled` is true in
any source. These changes landed as Bug 2073015 ("Restrict macOS policy sources
to privileged domains", "Rank user policies below all policy providers"). They
are on `main`, `release` (161) and `esr153` (uplifted 2026-11-19). They are
**not** on `esr140`.

The admin docs agree: "Policies from user sources rank below administrator
policies, so users cannot override an administrator-set policy."
`DisableMacOSUserPolicies` is supported in Firefox 160+ and not in ESR
([admin docs](https://firefox-admin-docs.mozilla.org/reference/policies/disablemacosuserpolicies/)).

**ESR 140 and older:** the reader returned
`[[NSUserDefaults standardUserDefaults] dictionaryRepresentation]`
([esr140 reader](https://github.com/mozilla-firefox/firefox/blob/esr140/xpcom/base/nsMacPreferencesReader.mm)).
`NSUserDefaults` searches the user's own domain before `/Library/Preferences`,
so `defaults write org.mozilla.firefox DisableEncryptedClientHello -bool false`
by a standard user overrode the root-written value. That would also override
`policies.json`, because the platform provider wins. Only a forced (managed
profile) value beat it.

### Option A: `/Library/Preferences/org.mozilla.firefox` (recommended)

Mozilla's current guide: "On a Mac without device management, set the same
keys system-wide with sudo defaults write."
([Configuring policies](https://firefox-admin-docs.mozilla.org/guides/policies-configuration/),
[mac README](https://github.com/mozilla/policy-templates/blob/master/mac/README.md))

```sh
sudo defaults write /Library/Preferences/org.mozilla.firefox EnterprisePoliciesEnabled -bool TRUE
sudo defaults write /Library/Preferences/org.mozilla.firefox DisableEncryptedClientHello -bool TRUE
sudo defaults write /Library/Preferences/org.mozilla.firefox DisableMacOSUserPolicies -bool TRUE
```

Nested policies can be flattened with `__` (e.g. `Homepage__URL`). The file
lives outside the bundle, so it survives Firefox updates and reinstalls, and it
leaves the code signature intact. Write it with `defaults` or `CFPreferences`
rather than editing the plist directly, because `cfprefsd` caches it. The mac
README says to run `sudo defaults read` if Firefox doesn't see a change.
Policies apply at Firefox startup, so a running Firefox has to restart.

### Option B: configuration profile (`org.mozilla.firefox` payload)

This is Mozilla's preferred route: a device-scope profile with
`EnterprisePoliciesEnabled` and the policies, nested in `mcx_preference_settings`
([mac README](https://github.com/mozilla/policy-templates/blob/master/mac/README.md),
[Configuring policies](https://firefox-admin-docs.mozilla.org/guides/policies-configuration/)).
The profile could go into the same signed `.mobileconfig` as the Chrome
payloads (Phase 4.1). Whether a manually installed profile, without MDM, ends
up as a root-owned `/Library/Managed Preferences/org.mozilla.firefox.plist`
(the only form the reader accepts as `managed`) is the same open question as
S2. If it doesn't, Option A covers it.

### Option C: `policies.json` inside `Firefox.app` (avoid)

- Mozilla's guide: "changing the contents of the app bundle breaks its code
  signature. macOS can then refuse to open Firefox with the message 'Firefox'
  is damaged and can't be opened. You should limit this to development use
  cases where signing doesn't matter."
  ([Configuring policies](https://firefox-admin-docs.mozilla.org/guides/policies-configuration/))
- [Bug 2089906](https://bugzilla.mozilla.org/show_bug.cgi?id=2089906)
  (ASSIGNED, by spohl): "Adding files to the app bundle breaks its code
  signature"; "we want to keep installed bundles intact so that the privileged
  updater can verify them". It recommends the plist "for Macs without device
  management".
- [Bug 2089900](https://bugzilla.mozilla.org/show_bug.cgi?id=2089900)
  (ASSIGNED, no milestone): proposes refusing elevated updates for bundles
  that fail `codesign --verify --deep --strict`, which names enterprise
  deployments that add files as an expected failure. It is gated on telemetry
  (bug 2089901) and has not landed.
- On macOS 13+, modifying another app's bundle also needs the user to grant
  "App Management" ("Allow apps to update or delete other apps") in Privacy &
  Security
  ([Apple Mac Help](https://support.apple.com/guide/mac-help/change-privacy-security-settings-mac-mchl211c911f/mac)).
  Whether that applies to a root LaunchDaemon was not checked.
- A user who drags in a fresh `Firefox.app` replaces the file. Whether
  Mozilla's in-place updates keep `distribution/` was not checked against the
  updater; it doesn't matter if Option A is used.

## 4. Remaining gaps for the plan

- **Older Firefox:** on ESR 140 (and Firefox before 160/161), a standard user
  can override every policy through their own preferences domain. Detect the
  installed version (`CFBundleShortVersionString`) and treat old builds as a
  browser that is not covered, or accept the gap.
- **Other builds:** the reader keys on the app's own bundle id
  (`[NSBundle mainBundle] bundleIdentifier`), so Nightly, Developer Edition
  and forks (LibreWolf, Waterfox, Zen, …) don't read `org.mozilla.firefox`.
  They belong on the Phase 4.4 list of browsers that are not covered.
- **Not hands-on verified:** that `about:policies` shows the policy as active
  from `/Library/Preferences` on 161; that a user `defaults write` is ignored;
  and that, with the lock in place, the filter sees the real SNI from Firefox.
  These need a person on a real Mac.
- **Admin users** can `sudo defaults delete` the key. That is the same class of
  weaken as removing the profile, and the daemon can re-write it and log the
  change.
