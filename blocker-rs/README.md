# Blocker (Rust)

A macOS website and app blocker that **coexists with Tailscale, Twingate and
other VPNs**, scales to **millions of blocked domains**, and gets its
blocklists from a **central server**.

```
                 ┌─────────────────────────── central server ───────────────────────────┐
                 │ blocker-server build: upstream lists ─▶ parse ─▶ FST per category    │
                 │                       ─▶ v1/manifest.json (Ed25519-signed)           │
                 │ blocker-server serve  (or any static host / CDN)                     │
                 └───────────────────────────────┬──────────────────────────────────────┘
                                                 │ HTTPS, ETag, content-addressed files
┌────────────────────────────── Mac ─────────────▼──────────────────────────────────────┐
│ blocker daemon (root LaunchDaemon)                                                    │
│   • verifies manifest signature + rollback, verifies SHA-256, installs lists atomically│
│   • closes apps blocked by active rules                                               │
│                                                                                       │
│ /Library/Application Support/Blocker/  config.toml, lists/<category>.fst             │
│                                                                                       │
│ Blocker content filter (NEFilterDataProvider system extension, Rust via objc2)        │
│   app ──▶ socket flow ──▶ [filter: allow / drop / peek SNI] ──▶ VPN tunnel / Wi-Fi    │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

## Why it coexists with VPNs

Tailscale and Twingate are packet-tunnel VPNs: they own a `utun` interface,
routes, and (with MagicDNS / Twingate DNS) the system resolver. Blockers that
also use a VPN tunnel or take over DNS fight them for those slots.

Blocker takes neither slot:

- **No tunnel, routes, or DNS changes.** Enforcement is a NetworkExtension
  *content filter*, a different extension type that macOS runs alongside packet
  tunnels. It sees each flow before it enters any tunnel and only answers
  allow / drop.
- **DNS-independent decisions.** The filter uses the hostname the app connected
  to, or reads it from the TLS ClientHello (SNI) or HTTP `Host` header. That
  works whether DNS went through MagicDNS, Twingate, DoH in the browser, or
  anything else.
- **A guard that is never overridden** (`blocker-core/src/guard.rs`):
  - Tailscale, Twingate, Apple networking daemons and Blocker itself are never
    filtered.
  - Tailscale and Twingate control-plane domains (`tailscale.com`, `ts.net`,
    `twingate.com`, …) can't be blocked, even if a downloaded list contains them.
  - CGNAT `100.64.0.0/10` (tailnet and Twingate resource addresses), the
    Tailscale ULA, and private and link-local ranges always pass.
  - Rules that would cover a protected domain are rejected with a warning.
- **QUIC.** UDP/443 to an unknown host is dropped only while a domain rule is
  active, so browsers fall back to TCP where SNI is visible. Protected apps and
  addresses are exempt, so Tailscale's WireGuard/DERP traffic is never touched.

## Why it scales

- Lists are compiled server-side into an **FST** keyed by reversed labels
  (`com\0example\0ads`). Millions of domains take a few MB on disk. Clients
  **memory-map** the file, so loading is instant and memory is shared.
- A lookup tests each suffix of the hostname (`com`, `example.com`,
  `ads.example.com`). The cost is proportional to the hostname's label count,
  not the list size.
- One entry blocks a domain and all its subdomains, and covered children are
  pruned at build time.

## Crates

| Crate | What |
|---|---|
| `blocker-core` | Config, schedules, domain normalization, list parsing, FST `DomainSet`, SNI/HTTP host parser, guard, signed manifest, verdict `Engine`, on-disk `Store`. Pure Rust, fully unit-tested. |
| `blocker-filter` | The `NEFilterDataProvider` system extension (`BlockerFilterDataProvider`), written in Rust with `objc2`. Hot-reloads the engine when the config or lists change. |
| `blocker` | CLI and root daemon, shipped as `Blocker.app/Contents/MacOS/blocker`: rules, server, updates, app enforcement, extension activation, launchd install. |
| `blocker-server` | Central list server: `keygen`, `build` (fetch → parse → FST → signed manifest, with degraded-list protection), `serve`. |
| `blocker-ui` | Menu-bar app built with [Rune](https://github.com/anomalyco/rune): a tray icon showing whether blocking is active, and a window with rules, blocklists and daemon/filter health. Bundle's main executable. |

## Menu-bar app

The tray icon is a template SF Symbol that follows the menu bar's appearance:

| Icon | Meaning |
|---|---|
| `shield.fill` | Blocking: at least one rule is active right now (menu lists them) |
| `shield` | Not blocking: no rule is active |
| `exclamationmark.shield.fill` | Needs attention: config error, or rules active while the filter is off or awaiting approval |

The window (Open Blocker… in the tray menu) is a Rune view styled with
rune-kit. Its switches enable or disable rules, and Update now fetches lists;
both run the CLI through the macOS administrator prompt. A locked rule's
switch stays disabled while the rule is active. The app reads the config
directory directly and asks the signed CLI for daemon and filter state
(`blocker status --json`), so it needs no privileges of its own.

On Windows and Linux the tray is Rune's own `rune::Tray`, with a drawn icon
(green dot, grey ring, amber "!"). Rune's tray can't change its icon, so a
state change replaces it. Rune's tray doesn't support macOS yet, so there the
tray is an `NSStatusItem` driven from Rune's main thread through
`AppHandle::update`. Headless runs
(`blocker-ui --snap ui.png --scene blocking|idle|attention`) skip the tray.
`BLOCKER_UI_TRACE=1` logs every icon and menu change.

## Using it

Launching `Blocker.app` starts the menu-bar UI: the tray icon shows whether
blocking is active, and its window shows rules, lists and daemon/filter status.
Changes prompt for an admin password. The CLI lives at
`Blocker.app/Contents/MacOS/blocker`:

```bash
# Server side (once): generate the signing key, build, serve.
blocker-server keygen --out secret.key      # prints the public key
blocker-server build --sources sources.toml --out public --key secret.key
blocker-server serve --dir public --addr 0.0.0.0:8080   # or upload `public/` to a CDN
```

```toml
# sources.toml
[[category]]
name = "ads"
description = "Advertising and tracking"
sources = ["https://big.oisd.nl/domainswild2"]

[[category]]
name = "social"
description = "Social media"
sources = ["file:lists/social.txt"]
```

```bash
# Mac side: build/sign/notarize per packaging/README.md, copy to /Applications, then:
B=/Applications/Blocker.app/Contents/MacOS/blocker
$B activate                                   # approve in System Settings once
sudo $B install                               # root daemon: list updates + app closing
sudo $B server https://lists.example.com --public-key <key>
sudo $B rule add work -c social -d news.ycombinator.com -w "mon-fri 09:00-17:00" --locked
sudo $B rule add games -b com.valvesoftware.steam -p Discord -w "mon-fri 09:00-17:00"
sudo $B update
$B status
$B check www.reddit.com --app com.google.Chrome
$B apps                                       # find bundle ids for app rules
log stream --predicate 'subsystem == "dev.blocker.filter"'   # see what gets blocked
```

`--locked` rules can't be edited, removed, disabled, allow-listed around, or
have the filter deactivated while they're active. (Root can still edit the
files; this is a commitment device, not tamper-proofing.)

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
packaging/build-app.sh --adhoc --out dist    # bundle layout check without a Developer ID
```

Running the extension for real requires an Apple Developer Program team with
the Network Extension (content filter) and System Extension capabilities, or a
test Mac with SIP disabled and `systemextensionsctl developer on`. See
[packaging/README.md](packaging/README.md).

### Known limits

- macOS doesn't route some Apple system processes (push, iCloud, Maps, …)
  through content filters.
- **Encrypted Client Hello.** ECH hides the real hostname behind the
  provider's public name (`cloudflare-ech.com` for Cloudflare-fronted sites).
  The filter detects ECH, and `block_ech = true` under `[settings]` drops ECH
  connections while a domain rule is active. That's strict, but it also stops
  every ECH site from loading during that time. The better fix is to turn ECH
  off by browser policy (Chrome `EncryptedClientHelloEnabled = false`, Firefox
  `network.dns.echconfig.enabled = false`), so names stay visible.
- Apps that open raw-IP connections with a non-TLS protocol are only blocked by
  app rules, not domain rules.
