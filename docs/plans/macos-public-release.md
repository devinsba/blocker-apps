# Plan: macOS public release

Ship Blocker for macOS to the general public, aimed first at people blocking adult
content and gambling, with scheduled rules (such as social media during work or
school) alongside. Terms are defined in [GLOSSARY.md](../../GLOSSARY.md); the two
load-bearing decisions are [ADR 0001](../adr/0001-offline-locks.md) (offline key
locks) and [ADR 0002](../adr/0002-daemon-is-rule-gatekeeper.md) (the daemon is
the only gatekeeper).

Phases are ordered by dependency. Phase 0 answers platform questions that can
change later phases; do it first and update this plan with what it finds.

## Decisions this plan implements

- Distribution: notarized Developer ID `.pkg`, Sparkle updates, never downgrades.
  Free, no accounts. Server URL and public key are built in.
- Rules cover the whole Mac (every account). Schedules are `Always` or weekly
  windows (`Schedule` already supports both).
- Every rule has one **lock**: none, **self lock** (cooldown of daemon-witnessed
  time, then re-confirm or lapse; cooldown only lengthens), or **key lock**
  (unlock code from the keyholder's authenticator; keyholder picks the unlock
  window length per unlock; changes persist).
- **Strengthening** applies at once with no admin rights. **Weakening** passes the
  rule's lock; a lock-free rule needs admin. Global settings and uninstall pass
  the strongest lock on the Mac.
- One **keyholder** per Mac, enrolled by QR (TOTP), which they also save as a
  backup. Replacing them needs their code. **Recovery**: 14 days of witnessed
  time, then key locks become self locks; logged.
- Tamper handling: config shadow and restore, daemon outside the app bundle,
  filter watchdog, clock-jump detection (scheduled rules treated as active while
  the clock is suspect), **tamper log** shown to the keyholder at the next unlock.
- Categories from your server: `adult`, `gambling`, `bypass` (VPN, proxy and
  anonymizer services). A rule that blocks `adult` or `gambling` includes `bypass`
  by default. The guard always wins.
- ECH and QUIC are dropped only as a logged last resort: QUIC Initial packets are
  decrypted to read the SNI; ECH flows prefer the macOS-provided hostname; the
  **browser profile** turns ECH off in covered browsers. SOCKS4a/5 is peeked.
- **SafeSearch** is an option of the browser profile: Chromium policies, Safari via
  Screen Time "Limit Adult Websites" with the keyholder setting the passcode,
  browsers that are not covered are closed.
- A **setup wizard** walks through every manual step.

## Phase 0: platform spikes (on a real Mac, each a yes/no with notes)

Record each answer in this file. A "no" changes the named task.

| # | Question | Affects |
|---|---|---|
| S1 | Does `NEFilterDataProvider` deliver outbound UDP datagram bytes for peeking (a `filterDataVerdict` on a UDP `NEFilterSocketFlow`)? | 1.6 QUIC |
| S2 | Does a user-installed `.mobileconfig` (no MDM) with a `com.google.Chrome` payload make Chrome apply `ForceGoogleSafeSearch`, `ForceYouTubeRestrict` and `EncryptedClientHelloEnabled`? Check `chrome://policy`. Same for Edge (`ForceBingSafeSearch`) and Brave. | 4.1 |
| S3 | Do Chrome's `EncryptedClientHelloEnabled` policy and Firefox's `network.dns.echconfig.enabled` pref (lockable through `policies.json` `Preferences`) still exist and work? Does Firefox have any SafeSearch policy? | 4.1, 4.3 |
| S4 | Does Screen Time "Limit Adult Websites" force Google SafeSearch in Safari? | 4.2 |
| S5 | Does Safari's or WebKit's networking process show up in the filter, or is it on Apple's exclusion list? | Whole product; if excluded, Safari becomes a browser that is not covered |
| S6 | Can the daemon re-enable a content filter the user turned off in System Settings without a user prompt? | 2.6 |
| S7 | If `Blocker.app` is deleted, does a LaunchDaemon whose binary lives outside the bundle keep running, and does the system extension get removed? | 2.5 |
| S8 | Can a Developer ID `.pkg` install the app, the out-of-bundle daemon and the LaunchDaemon plist in one pass, with the extension still activated from `/Applications`? | 6.1 |

## Phase 1: core (`crates/blocker-core`)

All pure Rust and unit-tested. No FFI change unless noted.

1.1 **Lock model.** Replace `Rule.locked: bool` ([config.rs](../../crates/blocker-core/src/config.rs#L104)) with
`lock: Lock` where `Lock = None | SelfLock { cooldown_minutes } | KeyLock`.
Accept the old `locked = true` on read, mapping it to `SelfLock` with a 24h cooldown
(no public users yet, so no long migration). Add a `schema_version` to the config.

1.2 **Change classification.** A pure function `classify(old: &Config, new: &Config) -> Change`
that reports, per rule and for global settings, whether a change strengthens,
weakens, or both. Weakening covers: removing or disabling a rule, removing domains,
categories or apps, narrowing a schedule, lowering a lock, shortening a cooldown,
adding to `allow_domains`, `allow_apps` or `allow_cidrs`, changing the server, and
removing `bypass` from a rule that blocks `adult` or `gambling`. Table-driven tests
for every case. This is the heart of the gatekeeper; test it hardest.

1.3 **Pending changes and witnessed time.** Types for `PendingChange { rule, change,
witnessed_minutes, cooldown_minutes, state }` with states `Waiting`, `AwaitingConfirm
{ until }`, `Lapsed`, `Applied`. The daemon advances `witnessed_minutes` itself;
the core only holds the transitions. At most one pending change per rule; cancel at
any time.

1.4 **Keyholder.** TOTP verify (RFC 6238, SHA-1, 6 digits, 30 s, ±1 step) against a
stored secret. Use a small, audited crate, or HMAC from RustCrypto plus about 30
lines. `UnlockWindow { until }`. Replacing the keyholder needs a valid code.
Recovery state: `Recovery { witnessed_minutes }` with a 14-day threshold.

1.5 **ECH host priority.** In `Engine::decide` ([engine.rs](../../crates/blocker-core/src/engine.rs#L206-L209)),
when `flow.ech` is true prefer `flow.hostname` over the peeked outer SNI. The
`block_ech` drop then applies only when there is no hostname. Turn `block_ech` from
a user setting into that fallback: always on while a locked rule blocks hosts.
Log every fallback drop with the reason `ech-fallback`.

1.6 **QUIC Initial peek** (depends on S1). New module `quic.rs`: parse the long
header, derive Initial keys (RFC 9001 §5.2, v1 salt; also handle v2, RFC 9369),
remove header protection, decrypt with AES-128-GCM, reassemble CRYPTO frames
across Initial packets, and hand the ClientHello to the existing parser in
[sni.rs](../../crates/blocker-core/src/sni.rs). Dependencies: RustCrypto `aes`,
`aes-gcm`, `hkdf`, `sha2` (pure Rust, builds for every desktop and mobile target).
Tests: RFC 9001 Appendix A sample packets, plus a captured multi-packet Chrome
ClientHello. In `decide`, a UDP/443 flow with no hostname gets `Peek` instead of
the drop; drop it only if the peek fails (`quic-fallback`, logged).

1.7 **SOCKS peek.** Add SOCKS4a and SOCKS5 (`CONNECT`, domain address type) to
`sni::peek_host`, next to the existing HTTP `CONNECT` support. Tests for each address
type.

1.8 **Tamper log type.** An append-only list of `TamperEvent { at, kind, detail }`
with kinds `ConfigEdited`, `FilterDisabled`, `DaemonMissing`, `AppMissing`,
`ClockJump`, `ProfileRemoved`, `Downgrade`, `RecoveryStarted`. Also
`since_last_unlock()`.

## Phase 2: daemon as gatekeeper (`desktop/blocker-cli`)

2.1 **Daemon-owned state.** Move rules, locks, pending changes, the keyholder
secret, witnessed time and the tamper log into root-only state (mode 0600) under
`/Library/Application Support/Blocker/`. The daemon renders `config.toml` for the
filter from that state. `config.toml` stays the filter's read-only input; it is no
longer anyone's write path.

2.2 **Local socket API.** A Unix socket readable by all users. Requests: `status`,
`propose(new_rules)`, `confirm(pending)`, `cancel(pending)`, `unlock(code,
minutes)`, `enroll_keyholder`, `replace_keyholder(code)`, `start_recovery`,
`uninstall`. The daemon runs `classify` (1.2): it applies a strengthen, queues a
self-lock weaken, needs an open unlock window for a key-lock weaken, and needs the
caller to be an admin (check the peer's uid against the `admin` group) for a
lock-free weaken.

2.3 **Witnessed time.** A monotonic counter advanced each daemon tick and saved
every minute. Pending changes and recovery advance only on it.

2.4 **Clock check.** Compare the system clock with the list server's HTTP `Date`
header on each list check. A jump beyond a threshold logs `ClockJump`, and while
the clock is suspect every scheduled rule counts as active.

2.5 **Install outside the bundle** (depends on S7). Put the daemon binary at
`/Library/Application Support/Blocker/bin/blocker` and point the LaunchDaemon there
(today [launchd.rs](../../desktop/blocker-cli/src/launchd.rs) points into
`/Applications/Blocker.app`). If the app goes missing, log `AppMissing` and post
a notification on each tick interval.

2.6 **Watchdogs.** Each tick:
- If `config.toml` differs from what the daemon rendered, re-render it and log
  `ConfigEdited`.
- If the filter is disabled or missing, log `FilterDisabled`, try to re-enable it
  (S6), and set the attention state.
- If the browser profile is missing while required, log `ProfileRemoved`.

2.7 **Uninstall.** `blocker uninstall` goes through the socket and passes the
strongest lock on the Mac, then removes the daemon, the extension, the profile
and the state.

2.8 **Schema and downgrade.** Refuse to load state with a newer `schema_version`
than the binary knows. Keep the last version that ran in state; a lower running
version logs `Downgrade`.

## Phase 3: CLI and menu-bar app

3.1 **CLI.** The `rule`, `server` and `update` subcommands talk to the socket
instead of writing files. `--locked` becomes `--lock none|self:<hours>|key`. Show
pending changes and the unlock window in `status`.

3.2 **UI.** Replace the `osascript` path in
[privileged.rs](../../desktop/blocker-ui/src/privileged.rs) with socket calls.
Show each rule's lock, its pending change (with the cooldown remaining and a
Confirm button once it is due), and an "Unlock…" flow that takes the code and the
window length. Show the tamper log count to the keyholder after an unlock. The
attention icon also covers tamper events and a suspect clock.

3.3 **Setup wizard** (first run, and re-entrant if a step is missing):
1. Approve the system extension (screen showing exactly where to click).
2. Preselected rule: Adult + Gambling + Bypass, always, self lock with a 24h
   cooldown; the user can edit it before confirming.
3. Optional keyholder: show the QR code, ask the keyholder to scan it and save
   it as a backup, verify one code, and show the honest warning ("for this lock to
   hold, the keyholder should own this Mac's admin password").
4. Install the browser profile (System Settings opens to Profiles); SafeSearch on
   or off.
5. If SafeSearch is on: guide the Screen Time setup for Safari, with the keyholder
   entering the Screen Time passcode.
6. Optional scheduled rules (for example, Social during work hours).

## Phase 4: browser profile and SafeSearch

4.1 **Profile** (depends on S2 and S3). Generate a signed `.mobileconfig` with
payloads for Chrome, Edge and Brave: `EncryptedClientHelloEnabled=false` always,
and `ForceGoogleSafeSearch`, `ForceYouTubeRestrict=2` (Strict) and Edge's
`ForceBingSafeSearch` when SafeSearch is on. Removing the profile, or turning
SafeSearch off, is a weaken.

4.2 **Safari** (depends on S4 and S5). If Screen Time forces SafeSearch, the wizard
step stays as planned; otherwise Safari becomes a browser that is not covered
when SafeSearch is on.

4.3 **Firefox** (depends on S3). Write `policies.json` with the locked ECH pref
(and SafeSearch if a policy exists). If that isn't viable, Firefox is a browser
that is not covered.

4.4 **Browsers that are not covered.** A server-maintained list of browser bundle
ids. While SafeSearch is on, those apps are closed by the existing app enforcement
([apps.rs](../../desktop/blocker-cli/src/apps.rs)). Apps hitting `ech-fallback`
repeatedly are candidates for that list.

## Phase 5: server and lists (`crates/blocker-server`)

5.1 Check the licences of the candidate upstream lists (HaGeZi NSFW, gambling and
DoH/VPN/proxy-bypass lists; OISD NSFW; StevenBlack porn and gambling extensions).
Use only lists whose licences allow redistribution; record the licences in
`sources.toml`.

5.2 Build the `adult`, `gambling` and `bypass` categories. The existing guard
rejects protected domains; add a server-side test that `bypass` never contains
anything the guard protects.

5.3 Host the server and the `apple-app-site-association` path (needed later for
passkeys) on your domain. Bake the URL and public key into the build.

## Phase 6: packaging and updates (`desktop/packaging/macos`)

6.1 **`.pkg`** (depends on S8). Extend `build-app.sh` to produce a signed,
notarized `.pkg` that installs the app, the out-of-bundle daemon and the
LaunchDaemon plist, then opens the app for the wizard.

6.2 **Sparkle** with EdDSA-signed appcasts, configured never to offer a downgrade.
An update replaces both the app and the out-of-bundle daemon binary.

6.3 Update [desktop/packaging/macos/README.md](../../desktop/packaging/macos/README.md)
and [desktop/README.md](../../desktop/README.md): the manual `sudo` steps become
the developer path only.

## Out of scope for this release

Online keyholder approval and tamper alerts, passkeys, DNS-proxy SafeSearch,
per-account rules, accounts and payments, and locks on iOS and Android (the lock
model lives in the core, so mobile can adopt it later through the FFI).
