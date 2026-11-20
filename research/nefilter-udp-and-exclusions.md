# NEFilterDataProvider: UDP data peeks and excluded Apple processes (macOS)

Desk research for issue #8, which covers Phase 0 S1 and S5 of
`docs/plans/macos-public-release.md` (on the `macos-app` branch). It feeds Phase 1.6 (QUIC Initial
peek) and the Safari coverage question. Researched 2026-11-20 against the
macOS 27.0 SDK headers (Xcode 27.1), XNU `bsd/net/content_filter.c` (main),
Apple's documentation JSON, WWDC19 session 714, and Apple DTS forum posts.

## Answers in brief

- **S1, UDP data:** Yes. Every primary source agrees that a content filter
  gets UDP flows with data callbacks, and that a data verdict can drop them.
  No Apple page says "UDP datagram bytes reach `handleOutboundData`" in so
  many words. But the SDK header writes UDP-specific rules into the data
  callbacks and the pause verdict, and the kernel queues and pauses UDP data
  per flow. So the API was built for it. What is documented about how UDP
  data is delivered:
  - A UDP "flow" is one 4-tuple (local address/port, remote address/port),
    not one datagram. A single socket can have many flows.
  - Offsets and peek counts are bytes in one stream that runs across all of
    the flow's datagrams.
  - A datagram is never partly released.
  - The kernel garbage-collects a UDP flow that is held without a verdict
    for 10 s.
- **S5, exclusions:** Today there is no exclusion list. The undocumented
  `ContentFilterExclusionList` (about 50 Apple apps and daemons) existed only
  in macOS 11.0 and 11.1. Apple removed it in 11.2 (beta 2, January 2021).
  Apple never documented it. The only Apple-staff statement is Quinn: changes
  "were made in this space in the macOS 11.2 release". The key is absent from
  this machine's NetworkExtension framework on macOS 27.2.
- **Safari:** Neither Safari nor `com.apple.WebKit.Networking` is named in any
  report of the old list. Apple DTS says Safari's traffic goes through
  NSURLSession, which is one of the connect-by-name APIs that fill in
  `remoteHostname`. A developer on macOS 15.0.1 confirms that Safari flows
  carry a hostname and Chromium browsers' flows do not. On macOS,
  `sourceAppAuditToken` names the app (Safari) even when a system helper
  process opened the connection. `sourceProcessAuditToken` names the helper.
- **Code issue found along the way:** `desktop/blocker-filter/src/provider.rs`
  reads `sourceAppIdentifier`, but the SDK marks it
  `API_UNAVAILABLE(macos)`. On macOS, an app's identity has to come from
  `sourceAppAuditToken`.

## 1. UDP flows in NEFilterDataProvider

### What the sources say

1. **UDP flows reach the data provider by default.** WWDC19 session 714
   ("Network Extensions for the Modern Mac"): "By default, the system will
   pass every single flow of TCP and UDP data to your NEFilterDataProvider
   subclass." The speaker also says the decision can be made "right up front
   when the flow is first opened or you can wait after you've seen some amount
   of the flow's data." Source: <https://developer.apple.com/videos/play/wwdc2019/714/>
2. **The data callbacks are written with UDP in mind.** In
   `NEFilterDataProvider.h` (macOS 27.0 SDK), the doc comment for `readBytes`
   on both `handleInboundDataFromFlow:` and `handleOutboundDataFromFlow:`
   says: "For non-UDP/TCP flows, since data may optionally include the IP
   header, readBytes includes a 4-bytes NEFilterDataAttribute field preceding
   the user data." So for UDP, `readBytes` is plain payload with no attribute
   prefix and no IP or UDP header. A QUIC parser gets the QUIC packet at
   offset 0.
3. **UDP has its own pause semantics.** In the same header, `pauseVerdict`
   says: "TCP flows may be paused indefinitely. UDP flows will be dropped if
   not resumed within 10 seconds of being paused." That rule only makes sense
   if UDP flows go through the data-verdict machinery.
4. **Peeking is described in bytes, not packets.** The parameter docs for
   `filterDataVerdictWithFilterInbound:peekInboundBytes:filterOutbound:peekOutboundBytes:`
   and `dataVerdictWithPassBytes:peekBytes:` speak only of "the number of
   outbound bytes that the filter needs to see in the subsequent call to
   handleOutboundDataFromFlow". Sources: SDK header, and
   <https://developer.apple.com/documentation/networkextension/nefilternewflowverdict/filterdataverdict(withfilterinbound:peekinboundbytes:filteroutbound:peekoutboundbytes:)>
   The `offset` argument is "measured from the beginning of the flow's
   outbound data". Source:
   <https://developer.apple.com/documentation/networkextension/nefilterdataprovider/handleoutbounddata(from:readbytesstartoffset:readbytes:)>
5. **Kernel behaviour (XNU `bsd/net/content_filter.c`, the CFIL socket
   filter that the NE content filter sits on).** Source:
   <https://github.com/apple-oss-distributions/xnu/blob/main/bsd/net/content_filter.c>
   - Flow identity: "For UDP and other datagram protocols, as soon as traffic
     is seen for each new flow identified by its 4-tuple of source
     address/port and destination address/port, a `struct cfil_info` is
     created. Each datagram socket may have multiple flows." So a
     `handleNewFlow` call is per 4-tuple, not per datagram. A connected QUIC
     socket is one flow.
   - Byte accounting: each queued datagram adds `datalen` to
     `cfi_pending_last`. Pass and peek offsets are stream offsets across the
     flow's datagrams.
   - Whole datagrams only: when the pass offset falls inside an mbuf, the
     code says "The first mbuf can partially pass … Stop on partial pass".
     The datagram stays queued until the whole of it is passed.
   - Limits: a held UDP flow is garbage-collected when no verdict arrives for
     `UDP_FLOW_GC_ACTION_TO` (10 s), or when its queue exceeds the per-flow
     mbuf threshold (2 MB, or 1/32 of platform mbuf clusters if that is
     larger). Past the threshold, new data is tail-dropped with `EPIPE`.
6. **Per connection, not per packet.** Apple DTS: NEFilterFlow analyses data
   per connection. For per-packet inspection, use `NEFilterPacketProvider`.
   Source: <https://developer.apple.com/forums/thread/691335>
7. **Dropping UDP after a peek works in the field.** A developer (Apr 2023)
   runs a DNS firewall that returns
   `filterDataVerdict(withFilterInbound: false, peekInboundBytes: 0, filterOutbound: true, peekOutboundBytes: 1560)`
   for UDP/53 and then `.drop()` from `handleOutboundData`. It "works
   normally on most Mac computers". The failure in that thread was
   `CFIL: Falied to create UDP flow` kernel spam that showed up only with two
   content filters installed. Apple's reply blamed a missing local endpoint
   on UDP and conflicts between filters. Source:
   <https://developer.apple.com/forums/thread/728397>
8. **QUIC shows up as UDP.** Quinn (DTS, Oct 2024) lists the common ways
   filters misclassify flows: "A flow might come through as UDP when they're
   expecting TCP. The most obvious example of this is HTTP/3, which uses QUIC."
   Source: <https://developer.apple.com/forums/thread/767291>

### Implications for Phase 1.6 (QUIC Initial peek)

- What `decide` does today (a `Peek` on UDP/443 with no hostname, followed by
  a drop on failure) fits the API. A `dropVerdict` from
  `handleOutboundData` on a UDP flow is supported.
- `FIRST_PEEK = 2048` is larger than one QUIC Initial (at least 1200 bytes,
  usually about 1250 to 1350). No source says whether the framework calls
  `handleOutboundData` with fewer bytes than requested when the next datagram
  is late. For TCP it does: the current TLS peek works with short
  ClientHellos. If it waits on UDP, the client's first Initial is held until
  the client sends another datagram, or for up to 10 s. **The hands-on test
  must settle this.**
- A ClientHello can span more than one Initial (post-quantum key shares in
  Chrome do this). Asking for more bytes with
  `dataVerdictWithPassBytes:0 peekBytes:N` keeps both datagrams queued, and
  the second one reaches the filter at the next stream offset. Payloads are
  concatenated with no datagram boundaries, so the parser must walk QUIC
  packet lengths itself.
- Keep the peek decision quick. A held UDP flow is dropped after 10 s with
  no verdict.

## 2. Processes excluded from content filters

| Period | Behaviour | Source |
|---|---|---|
| macOS 10.15 | No exclusion list reported | — |
| macOS 11.0 to 11.1 | Key `ContentFilterExclusionList` in `/System/Library/Frameworks/NetworkExtension.framework/Versions/Current/Resources/Info.plist`. It listed about 50 to 53 Apple apps and daemons (App Store, Maps, iCloud, FaceTime, Music, Software Update, `nsurlsessiond`, and others) whose traffic content filters and NE firewalls never saw. Undocumented. | FB8808172 cited in <https://developer.apple.com/forums/thread/664115>; Jamf, 1 Feb 2021, <https://www.jamf.com/blog/the-network-is-back-in-big-sur/> |
| macOS 11.2 beta 2 (13 Jan 2021) onward | Key removed. LuLu and similar firewalls see Apple traffic again. Apple told press the list was temporary, caused by bugs from the move away from network kernel extensions. | Jamf (above); Quinn (DTS, Oct 2023): "changes were made in this space in the macOS 11.2 release", <https://developer.apple.com/forums/thread/664115> |
| macOS 27.2 (this machine, build 26B123) | `plutil -p` of the framework Info.plist shows no `ContentFilterExclusionList` and no exclusion key | Local check, 2026-11-20 |

- Apple has never published an exclusion list for content filters, before
  11.2 or after. Quinn says DTS knows of no class of flows that a filter
  misses: "Not that I'm aware of". He treats an unexplained gap between a
  packet trace and the filter as something to report. Source:
  <https://developer.apple.com/forums/thread/767291>
- Two exclusions remain that are documented but have nothing to do with the
  old list:
  - `NEFilterSettings` rules filter only the flows you ask for.
  - The packet-tunnel exclusions (`excludeAPNs`, `excludeCellularServices`)
    apply only to VPN tunnels, not content filters. Quinn cites them as the
    "similar kerfuffle" for packet tunnels.
- Nobody has published the full 11.0 list from a primary source. Press
  reports name apps and daemons, and none of them name Safari or WebKit. It
  does not matter for a product that requires a current macOS.
- `research/macos-non-vpn-blocking.md` (on `macos-app`) still says "Apple apps
  bypass the filter (undocumented exclusion list)" with "Workaround: None".
  That was true only for 11.0 and 11.1 and should be corrected.

## 3. Safari and `com.apple.WebKit.Networking`

- **Not excluded.** No source places Safari or WebKit on any exclusion list,
  and none exists today (section 2).
- **Hostname present.** `remoteHostname` (macOS 11+): "This property is only
  non-nil if the flow was created using Network.framework or NSURLSession"
  (SDK header). Quinn (DTS, May 2021), on a macOS 11 content filter: "Was the
  connection created by Network framework or NSURLSession (the latter includes
  Safari)?" Source: <https://developer.apple.com/forums/thread/679860>.
  A developer on macOS 15.0.1 reports hostnames for Safari and Firefox and
  only IP addresses for Chrome, Arc, Brave, Edge and Opera. Quinn's reply:
  apps that resolve and then connect by IP give the filter no DNS name, and
  "There's nothing that macOS can do about this". Sources:
  <https://developer.apple.com/forums/thread/767285>,
  <https://developer.apple.com/forums/thread/760266>
- **Attribution.** WebKit does its networking in a separate XPC process. The
  header for `sourceProcessAuditToken` (macOS 13+) says: "In cases where the
  connection was created by a system process on behalf of the source
  application, sourceProcessAuditToken will be different from
  sourceAppAuditToken and will contain the audit token of the system
  process." So `sourceAppAuditToken` should name Safari, and
  `sourceProcessAuditToken` should name `com.apple.WebKit.Networking`. Not
  checked here: whether WebKit sets the delegation so this holds for every
  Safari flow.
- **Browser flows are iOS-only.** On macOS, `filterBrowsers` is
  `API_DEPRECATED("filterBrowsers is not supported on macOS", macos(10.11, 10.15))`,
  so Safari traffic arrives as `NEFilterSocketFlow`. That is what
  `provider.rs` already assumes.
- **Safari over HTTP/3.** These would be UDP socket flows. Safari's QUIC goes
  through Network.framework, so its UDP flows should also carry
  `remoteHostname`. No source confirms this. Test it.

## 4. Side finding: `sourceAppIdentifier` is unavailable on macOS

`NEFilterFlow.h`:
`@property (readonly, nullable) NSString *sourceAppIdentifier API_AVAILABLE(ios(11.0)) API_UNAVAILABLE(macos, watchos, tvos);`
Apple's documentation page also lists no macOS availability. Source:
<https://developer.apple.com/documentation/networkextension/nefilterflow/sourceappidentifier>
`provider.rs` `extract()` uses it for `app_id`. On macOS, expect `nil`, which
means app-based rules never match. Use `sourceAppAuditToken`: get its code
signing identifier with `SecCodeCopyGuestWithAttributes` and
`kSecGuestAttributeAudit`. Confirm on hardware.

## What the hands-on ticket ("UDP peek and Safari visibility") must test

1. **UDP peek and drop.** Return `filterDataVerdict(outbound, peek 2048)` on
   a UDP/443 flow. Check:
   - that `handleOutboundData` is called;
   - that `readBytes` starts with the QUIC long header (first byte
     `0xC0`–`0xFF`, version `0x00000001`);
   - its length (one Initial, or several);
   - the `offset`;
   - that `dropVerdict` stops the connection (for example,
     `curl --http3-only` or Chrome to a QUIC host falls back or fails).
2. **Short peeks.** With peek 2048 and a single Initial of about 1250 bytes,
   check whether the callback fires at once or waits for more data. Log the
   latency. Then check `dataVerdictWithPassBytes:0 peekBytes:N` across two
   Initials (Chrome with post-quantum key shares).
3. **Safari visibility.** Load HTTPS and HTTP/3 sites in Safari. Log
   `remoteHostname`, `socketProtocol`, and the code signing identifiers from
   `sourceAppAuditToken` and `sourceProcessAuditToken` for each flow.
   Confirm the hostname is present on both TCP and UDP flows, and that
   blocking a domain blocks Safari.
4. **App identity.** Log `sourceAppIdentifier` on macOS. It is expected to be
   `nil`, and app rules must then switch to the audit token.
5. **No exclusions.** Watch for flows from Apple daemons (`nsurlsessiond`,
   App Store, Software Update) to confirm that no exclusion remains on the
   target macOS versions.
