# Screen Time "Limit Adult Websites" and SafeSearch on macOS

Question (issue #7, plan Phase 0 S4): on macOS, does Screen Time > Content &
Privacy > web content "Limit Adult Websites" force SafeSearch for Google, Bing and
YouTube in Safari (and other browsers)? Can an app set it up, or only the user?
What does the Screen Time passcode protect, and how hard is it to get around on
an admin account?

Researched 2026-11-20 from Apple and Google documentation. Nothing here was
tried on a Mac; the open items go to the hands-on ticket (#13).

## Answer

- **No primary source says it forces SafeSearch.** Apple describes "Limit Adult
  Websites" only as a site filter, and Google's SafeSearch help doesn't mention
  Apple or Screen Time. The only claims that it forces SafeSearch come from
  third-party blogs, and those blogs contradict each other. Treat it as unknown
  until someone tests it on a Mac.
- **An app can't set it through Screen Time APIs on macOS.** FamilyControls
  authorization is iOS/iPadOS only. **But a user-installed configuration profile
  can turn on the same built-in filter.** The
  `com.apple.familycontrols.contentfilter` payload is macOS-only, needs no
  supervision and can be installed by hand.
- **The passcode is weaker than the plan assumes.** Apple says the passcode can be
  changed or removed by verifying "with your password or Touch ID". On the user's
  own admin account, that likely means the user gets around the keyholder's
  passcode without knowing it.

## 1. What "Limit Adult Websites" does

Apple's current Mac Help (macOS 27) lists the web content levels under Safari:
Unrestricted Access, **Limit Adult Websites** ("Automatically limits access to
many adult websites", with Customize to add allowed or restricted sites), and
Allowed Websites Only.
([Apple: Change App Store, Media, Web & Games settings](https://support.apple.com/guide/mac-help/change-app-store-media-web-games-settings-mchlbcf0dfe2/mac);
same wording in the [macOS 13 page](https://support.apple.com/guide/mac-help/change-content-restrictions-settings-screen-mchlbcf0dfe2/13.0/mac/13.0).)
Neither page mentions SafeSearch, search engines or YouTube. The only search
settings in Content & Privacy are Siri's ("Web Search Content in Siri").

The Apple Platform Deployment guide calls this the built-in filter. It applies
to "Safari and third-party apps", is meant for "simple or limited
content-filtering needs", and Apple points organizations with "complex or legally
mandated" needs to a proxy or a third-party filter instead. On macOS it is
configured with the ParentalControlsContentFilter payload. Managing it also
stops Safari from clearing its history and website data. The guide doesn't
mention SafeSearch.
([Apple: Filter content for Apple devices](https://support.apple.com/guide/deployment/filter-content-dep1129ff8d2/web))

Apple's MDM schema for that payload (`com.apple.familycontrols.contentfilter`)
has only these keys: `restrictWeb`, `useContentFilter`, the allow list
(`allowListEnabled`, `siteAllowList`, `filterAllowList`) and `filterDenyList`.
None of them is a SafeSearch or search-engine key, and the schema repo
doesn't contain the word "safesearch".
([apple/device-management: com.apple.familycontrols.contentfilter.yaml](https://github.com/apple/device-management/blob/release/mdm/profiles/com.apple.familycontrols.contentfilter.yaml))

Google lists who can lock SafeSearch: parents through Family Link, schools,
"Administrators of your device or network", some public Wi-Fi networks, and local
law. Apple and Screen Time are not mentioned.
([Google: Filter explicit results using SafeSearch](https://support.google.com/websearch/answer/510))

Third-party claims disagree with each other. A parental-control vendor says the
setting forces Google/Bing SafeSearch
([NexSpy](https://nexspy.com/blog/safe-search-on-iphone)), while other guides say
Screen Time doesn't control Google SafeSearch
([Obex](https://www.obex.so/blog/safe-search-on-safari)). None of them points to
an Apple or Google source.

**For the plan:** SafeSearch in Safari can't be assumed. What the documentation
does support is that Safari gets Apple's adult-site filter, which overlaps the
Adult category Blocker already blocks.

## 2. Can a third-party app configure it on macOS?

- **FamilyControls / ManagedSettings: no.** `AuthorizationCenter`,
  `requestAuthorization(for:)` and `FamilyControlsMember.individual` are listed
  for iOS/iPadOS 16+ only, with no macOS or Mac Catalyst entry.
  `ManagedSettingsStore`, `WebContentSettings` and `FilterPolicy`
  (`.auto(_:except:)` "blocks adult content") list iOS/iPadOS 15 and Mac
  Catalyst 15, but not native macOS. Without authorization, a Catalyst app can't
  use them either. `SafariSettings` covers only cookies and AutoFill, not
  search.
  ([FamilyControls](https://developer.apple.com/documentation/familycontrols),
  [AuthorizationCenter](https://developer.apple.com/documentation/familycontrols/authorizationcenter),
  [FamilyControlsMember.individual](https://developer.apple.com/documentation/familycontrols/familycontrolsmember/individual),
  [WebContentSettings](https://developer.apple.com/documentation/managedsettings/webcontentsettings),
  [FilterPolicy](https://developer.apple.com/documentation/managedsettings/webcontentsettings/filterpolicy),
  [SafariSettings](https://developer.apple.com/documentation/managedsettings/safarisettings))
- **A configuration profile: yes, per Apple's schema.**
  `com.apple.familycontrols.contentfilter` is macOS 10.7+, `supervised: false`,
  `allowmanualinstall: true`, and works on both the device and user channels.
  So Blocker's browser profile could carry
  `restrictWeb=true, useContentFilter=true` and turn on Apple's adult filter
  without the user touching Screen Time. The user still approves the profile in
  System Settings.
  ([schema](https://github.com/apple/device-management/blob/release/mdm/profiles/com.apple.familycontrols.contentfilter.yaml))
  A manually installed profile can also carry `com.apple.profileRemovalPassword`
  on macOS (`supervised: false`, `allowmanualinstall: true`).
  ([schema](https://github.com/apple/device-management/blob/release/mdm/profiles/com.apple.profileRemovalPassword.yaml))
  Apple doesn't document whether that password stops an admin from removing the
  profile with `sudo profiles`.

## 3. The Screen Time passcode

- **What it protects:** "Require a passcode to prevent unauthorized changes to
  Screen Time settings, and to grant more time when limits expire."
  ([Apple: Change Manage Screen Time settings](https://support.apple.com/guide/mac-help/change-manage-screen-time-settings-on-mac-mm710ea791a4/mac))
- **The fallback:** both changing and turning off the passcode accept "the
  current four-digit passcode (or verify your identity with your password or
  Touch ID)".
  ([Apple: Set, change, or remove a Screen Time passcode on Mac](https://support.apple.com/guide/mac-help/set-change-remove-a-screen-time-passcode-mac-mchl336af525/mac))
  Apple doesn't say whose password. If it is the local account's login password,
  the keyholder's passcode doesn't hold against the user on their own account.
- **Admin accounts:** Apple prompts to convert an admin account to standard only
  when a parent sets the passcode for a family member.
  ([Apple: Set up Screen Time for yourself](https://support.apple.com/guide/mac-help/set-up-screen-time-for-yourself-mchle4e2edfe/26/mac/26))
  Apple shows no such prompt for a user setting up Screen Time for themselves.
- **Per-user scope (inference, not tested):** Screen Time settings belong to a
  user account. An admin can create a second account that has no Screen Time.
  Blocker's rules cover every account on the Mac.

## What the hands-on ticket (#13) must confirm

1. With Limit Adult Websites on, does Google search in Safari come back with
   SafeSearch on and locked? Check for `safe=active`, a lock on
   google.com/safesearch, or a redirect to forcesafesearch. Do the same for Bing
   strict and YouTube Restricted Mode.
2. Does the filter apply to Chrome, Firefox and Edge on macOS, or only to
   Safari/WebKit?
3. Does a manually installed `com.apple.familycontrols.contentfilter` profile
   turn on the same filter, and does it show in Screen Time?
4. On an admin account, does "your password or Touch ID" let the user remove a
   passcode the keyholder set? Can a profile protected by a removal password be
   removed with `sudo profiles`?
