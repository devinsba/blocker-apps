# macOS Non-VPN Blocking Approaches

## Executive Summary

This document outlines macOS blocking approaches that **do not require VPN connections**, enabling compatibility with Tailscale, WireGuard, and other VPN services. Unlike iOS, macOS does **not require supervised devices** for content filtering on consumer devices, making it significantly more viable for consumer-facing applications.

**Key Finding:** macOS supports multiple types of Network Extensions that can coexist with VPN connections, with NEFilterDataProvider being the primary non-VPN blocking solution.

---

## Critical Differences: macOS vs iOS

| Feature | macOS | iOS |
|---------|-------|-----|
| **Supervised Device Requirement** | ❌ No (for content filters) | ✅ Yes (for NEFilterDataProvider) |
| **Distribution Method** | Developer ID + Notarization | App Store or MDM |
| **VPN Compatibility** | ✅ Can coexist with VPNs | ⚠️ One VPN at a time |
| **Consumer Viability** | ✅ High | ⚠️ Limited (VPN conflicts) |
| **System Extension Architecture** | Yes (since macOS 10.15) | Network Extension (app extensions) |
| **Multiple Network Extensions** | ✅ Possible (different types) | ❌ Limited |

---

## Non-VPN Blocking Solutions for macOS

### Solution 1: NEFilterDataProvider (Recommended)

**Type:** Content Filter System Extension
**Requires VPN:** ❌ No
**Requires Supervision:** ❌ No
**Tailscale Compatible:** ✅ Yes (theoretically)

#### Overview
NEFilterDataProvider is Apple's recommended approach for content filtering on macOS. It provides TCP and UDP flows as well as other IP protocol traffic (like ICMP), allowing filter providers to provide verdicts to either allow or drop flows.

#### Key Capabilities
- **Layer 4 (Transport) Filtering:** Inspect and filter TCP/UDP connections
- **Flow-based Decisions:** Allow or drop network flows
- **Application Awareness:** Identify which application is making the connection
- **Domain/Hostname Filtering:** Access to hostname information for filtering decisions
- **Rule-based Filtering:** Create NEFilterRule objects for static filtering
- **No Traffic Modification:** Can only allow/drop (cannot modify packets)

#### macOS-Specific Behaviors

1. **No Split Architecture:** Unlike iOS, macOS doesn't support separate filter data and control providers. There is just one filter provider.

2. **More Permissive Sandbox:** Runs in a sandbox similar to macOS's traditional App Sandbox, which allows:
   - Writing to disk
   - IPC communication with code that shares an app group
   - More debugging capabilities than iOS

3. **No Supervision Required:** Can be distributed to consumer devices via Developer ID signing and notarization as a DMG.

4. **Better Compatibility:** Designed to coexist with VPN connections since it operates at a different network layer.

#### Limitations

1. **Apple Apps Bypass:** Approximately 50 Apple processes are excluded from NEFilterDataProvider visibility due to an undocumented exclusion list:
   - Maps
   - APNs (Apple Push Notification service)
   - Find My
   - iCloud services
   - And others

2. **No Traffic Modification:** Unlike VPN approaches, you cannot modify traffic, only allow or drop it.

3. **iOS-Specific Features Don't Work on macOS:**
   - `filterBrowsers` property will not work (compiles but has no effect)
   - Different extension lifecycle than iOS

#### Implementation Requirements

**Entitlements:**
```
com.apple.developer.networking.networkextension
  - content-filter-provider (Xcode)
  - content-filter-provider-systemextension (Developer ID)
```

Note: There's some confusion around entitlement naming. Little Snitch uses `content-filter-provider-systemextension`, suggesting this is the correct production value.

**Distribution:**
- Developer ID signing required
- Notarization required
- System Extension approval by user required
- Can be distributed as DMG (not via Mac App Store for system extensions)

**User Approval Required:**
1. Allow system extension in System Preferences → Security & Privacy
2. Approve Network Extension/Content Filter activation

#### Pros
✅ No VPN connection required
✅ No supervision required for macOS
✅ Can coexist with Tailscale and other VPNs
✅ Lower battery impact than VPN approaches
✅ Application-level and domain-level blocking
✅ Official Apple-recommended approach
✅ Comprehensive filtering capabilities

#### Cons
❌ Apple apps bypass the filter (undocumented exclusion list)
❌ Cannot modify traffic (allow/drop only)
❌ Requires system extension approval from user
❌ Complex development and debugging
❌ Cannot distribute via Mac App Store (Developer ID only)
❌ Ongoing issues with macOS 15 beta entitlements

#### Best For
- Consumer macOS blocking applications
- Apps that need to work alongside VPNs
- Domain and app-level blocking
- Productivity/focus/parental control apps

---

### Solution 2: NEDNSProxyProvider

**Type:** DNS Proxy System Extension
**Requires VPN:** ❌ No
**Requires Supervision:** ❓ Unclear for macOS (iOS requires supervision)
**Tailscale Compatible:** ✅ Likely (different extension type)

#### Overview
NEDNSProxyProvider intercepts DNS queries system-wide and allows you to handle DNS resolution, enabling DNS-based blocking without a VPN connection.

#### Key Capabilities
- **DNS Query Interception:** All DNS queries routed through your provider
- **Custom DNS Resolution:** Provide your own DNS responses
- **DNS-based Blocking:** Block domains by returning NXDOMAIN or other responses
- **DoH/DoT Prevention:** Can prevent encrypted DNS to maintain visibility

#### How It Works
Once activated, `mDNSResponder` routes DNS queries through the proxy provider instead of sending them directly to the upstream resolver.

#### Limitations

1. **DNS Cache Issues:** NEDNSProxyProvider does not flush existing DNS cache at startup, so cached DNS requests are used by apps until cache entries expire.

2. **Encrypted DNS Challenges:** DNS over HTTPS (DoH) and DNS over TLS (DoT) create visibility gaps. Apple's recommended workaround is to respond to `_dns.resolver.arpa` queries with NXDOMAIN to prevent mDNSResponder from discovering encrypted DNS support.

3. **Only One Active DNS Proxy:** When a third-party DNS Proxy is loaded, another DNS Proxy is immediately sent a stop message, suggesting potential conflicts with multiple DNS proxy extensions.

4. **Limited to DNS-level Blocking:** Apps can bypass DNS filtering by:
   - Using hardcoded IP addresses
   - Implementing their own DNS resolution
   - Using encrypted DNS (DoH/DoT)

5. **Supervision Requirement Unclear:** Documentation unclear whether macOS requires supervision (iOS does require it).

#### Implementation Requirements

**Platform Support:** macOS 10.15+ (previously incorrectly documented as iOS-only)

**Entitlements:** Similar to NEFilterDataProvider, requires Network Extension entitlements

**User Approval:** Requires system extension approval and network extension activation

#### Pros
✅ No VPN connection required
✅ Lightweight and efficient
✅ System-wide DNS control
✅ Can prevent encrypted DNS
✅ Can coexist with VPNs
✅ Lower battery/performance impact

#### Cons
❌ DNS-only filtering (easily bypassed)
❌ DNS cache persistence issues
❌ Cannot block apps, only domains
❌ Apps with hardcoded IPs bypass filtering
❌ Encrypted DNS (DoH/DoT) creates visibility gaps
❌ Only one DNS proxy can be active
❌ Unclear supervision requirements for macOS

#### Best For
- DNS-based domain blocking
- Lightweight filtering solutions
- Complementary to other filtering approaches
- Ad blocking (though limited by DNS bypass methods)

---

### Solution 3: macOS Packet Filter (pf)

**Type:** Kernel-level Firewall
**Requires VPN:** ❌ No
**Requires Supervision:** ❌ No
**Tailscale Compatible:** ✅ Yes

#### Overview
macOS includes Packet Filter (pf), a stateful packet filter inherited from OpenBSD that operates on IPs, ports, and protocols.

#### Key Capabilities
- **IP/Port-based Blocking:** Block specific IP addresses, ranges, or ports
- **Protocol Filtering:** Filter based on TCP, UDP, ICMP, etc.
- **Stateful Inspection:** Track connection states
- **Rate Limiting:** Limit connection rates
- **Table-based Rules:** Dynamic IP address lists

#### Limitations

1. **Not Application-Aware:** pf operates on IPs/ports/protocols regardless of which process owns the socket. You cannot block specific applications easily.

2. **No GUI:** Command-line only, requires manual configuration.

3. **Configuration Persistence:** The main `/etc/pf.conf` is overwritten during system updates. Must use custom anchor files for persistent rules.

4. **DNS Resolution Required:** For domain-based blocking, you must resolve domains to IPs first (can use tools like `packet-filter-host-blocker`).

5. **Limited to Network Layer:** Cannot inspect application-layer data or hostname information.

#### Implementation Approach

**Manual Configuration:**
```bash
# Create custom anchor file (persists across updates)
sudo vim /etc/pf.anchors/custom.rules

# Include in pf.conf
load anchor "custom" from "/etc/pf.anchors/custom.rules"
```

**Domain Blocking Helper:**
Use `packet-filter-host-blocker` (Python package) to resolve domains and generate pf tables and rules.

#### Pros
✅ No VPN required
✅ No system extension required
✅ Low-level and efficient
✅ Built into macOS
✅ Can coexist with all network extensions
✅ Powerful for IP/port blocking

#### Cons
❌ Not application-aware
❌ No GUI
❌ Requires manual configuration
❌ Configuration overwritten on updates (without custom anchors)
❌ Cannot block by hostname directly
❌ Requires DNS resolution for domain blocking
❌ No easy way to block specific apps

#### Best For
- IP/port-based blocking
- Server/development machines
- Complementary to other solutions
- Advanced users comfortable with command-line tools

---

### Solution 4: Application Firewall + NEFilterDataProvider

**Type:** Hybrid Approach
**Requires VPN:** ❌ No
**Requires Supervision:** ❌ No
**Tailscale Compatible:** ✅ Yes

#### Overview
Combine macOS's built-in Application Firewall with NEFilterDataProvider for comprehensive blocking.

#### How It Works
- **Application Firewall:** Blocks incoming connections on a per-application basis
- **NEFilterDataProvider:** Blocks outgoing connections and domains

#### Capabilities
- **Inbound Blocking:** Application Firewall handles app-based inbound filtering
- **Outbound Blocking:** NEFilterDataProvider handles outbound filtering
- **Comprehensive Coverage:** Both incoming and outgoing traffic control

#### Limitations
- Application Firewall only controls **incoming** traffic
- Cannot block outgoing traffic from specific apps via Application Firewall alone
- Still subject to NEFilterDataProvider limitations (Apple app bypass, etc.)

#### Pros
✅ Comprehensive blocking (inbound + outbound)
✅ No VPN required
✅ Leverages built-in macOS features

#### Cons
❌ Application Firewall limited to inbound only
❌ Still requires NEFilterDataProvider implementation
❌ Inherits all NEFilterDataProvider limitations

#### Best For
- Comprehensive firewall solutions
- Apps needing both inbound and outbound filtering
- Enterprise/managed environments

---

## Comparison of Non-VPN Solutions

| Solution | App Blocking | Domain Blocking | Complexity | VPN Compatible | User Approval Required |
|----------|--------------|-----------------|------------|----------------|------------------------|
| **NEFilterDataProvider** | ✅ Yes | ✅ Yes | High | ✅ Yes | ✅ Yes (sysext) |
| **NEDNSProxyProvider** | ❌ No | ✅ Yes (DNS only) | Medium | ✅ Yes | ✅ Yes (sysext) |
| **pf (Packet Filter)** | ⚠️ Limited | ⚠️ IP-based only | Medium | ✅ Yes | ❌ No (root required) |
| **Application Firewall** | ✅ Inbound only | ❌ No | Low | ✅ Yes | ⚠️ Sometimes |

---

## Recommended Architecture for Tailscale-Compatible Blocker

### Primary Approach: NEFilterDataProvider

**Architecture:**
```
macOS App (Swift/SwiftUI)
├── Main Application
│   ├── UI for block list management
│   ├── Schedule configuration
│   ├── Usage statistics
│   └── System extension lifecycle management
│
└── System Extension (NEFilterDataProvider)
    ├── Content Filter Provider
    │   ├── Flow inspection and filtering
    │   ├── Application identification
    │   ├── Hostname/domain filtering
    │   └── Rule engine (allow/drop verdicts)
    │
    ├── Shared App Group
    │   ├── Block list storage (SQLite/Core Data)
    │   ├── Configuration settings
    │   └── IPC with main app
    │
    └── Logging and Debugging
        └── os_log for debugging (view in Console.app)
```

**Distribution:**
- Developer ID signed and notarized
- Distributed as DMG
- User must approve system extension
- User must approve network extension

**Coexistence with Tailscale:**
- NEFilterDataProvider operates at Layer 4 (Transport)
- Tailscale VPN operates at Layer 3 (Network)
- Traffic flows through both: App → NEFilter → Tailscale → Network
- No routing conflicts since they serve different purposes

### Complementary Approach: Add NEDNSProxyProvider

For more comprehensive coverage, consider adding NEDNSProxyProvider for DNS-level blocking:

**Benefits:**
- Catch domains before DNS resolution
- Prevent encrypted DNS if desired
- Lightweight domain-only blocking option

**Considerations:**
- Only one DNS proxy can be active at a time
- May conflict with Tailscale's DNS features
- Less critical if NEFilterDataProvider handles domain blocking

---

## Open Source References

### LuLu by Objective-See
**GitHub:** https://github.com/objective-see/LuLu
**License:** GPL-3.0

LuLu is an open-source macOS firewall that blocks unauthorized outgoing connections. It uses:
- macOS Network Extension framework
- System Extension architecture
- Application-level filtering
- Real-time connection prompts

**Key Learnings from LuLu:**
- System extension approval workflow
- Application identification techniques
- User prompt design for connection decisions
- Rule storage and management
- Performance optimization strategies

**Code to Study:**
- Network extension implementation
- System extension lifecycle management
- IPC between main app and extension
- Application identification logic
- Rule engine implementation

---

## Implementation Checklist

### Before Development

- [ ] Request Network Extension entitlement from Apple (available to all paid developers)
- [ ] Set up Developer ID signing certificate
- [ ] Understand notarization requirements
- [ ] Study LuLu source code for implementation patterns
- [ ] Test Tailscale + NEFilterDataProvider coexistence on test devices

### During Development

- [ ] Create macOS app with Swift/SwiftUI
- [ ] Create System Extension target with NEFilterDataProvider
- [ ] Configure App Groups for shared data
- [ ] Implement filter provider logic
- [ ] Create rule engine (app and domain filtering)
- [ ] Implement IPC between app and extension
- [ ] Add usage statistics and logging
- [ ] Extensive testing with Tailscale active

### Distribution Preparation

- [ ] Developer ID signing
- [ ] Notarization
- [ ] Create DMG installer
- [ ] Write documentation for system extension approval
- [ ] Test on clean macOS installations
- [ ] Test across multiple macOS versions (10.15+)

### Post-Launch

- [ ] Monitor for macOS updates breaking compatibility
- [ ] Monitor Apple's entitlement requirement changes
- [ ] Handle user support for system extension approval issues
- [ ] Monitor compatibility with Tailscale updates

---

## Known Issues and Workarounds

### Issue 1: Apple Apps Bypass NEFilterDataProvider

**Problem:** ~50 Apple processes bypass content filtering
**Affected Apps:** Maps, APNs, Find My, iCloud, etc.
**Workaround:** None - this is an undocumented exclusion list
**Impact:** Cannot reliably block Apple app traffic

### Issue 2: Entitlement Confusion (macOS 15 Beta)

**Problem:** `content-filter-provider` vs `content-filter-provider-systemextension`
**Status:** Ongoing as of 2025
**Workaround:** Use `content-filter-provider-systemextension` for Developer ID profiles
**Reference:** Little Snitch uses the `-systemextension` variant

### Issue 3: System Extension Approval UX

**Problem:** Users must manually approve system extension in System Preferences
**Impact:** Conversion friction, support overhead
**Workaround:**
- Clear onboarding instructions
- Screenshots/video guides
- Programmatic detection of approval state
- Helpful error messages

### Issue 4: DNS Cache Persistence (NEDNSProxyProvider)

**Problem:** Existing DNS cache not flushed when DNS proxy starts
**Impact:** Previously cached DNS entries bypass filtering temporarily
**Workaround:**
- Document expected behavior
- Flush DNS cache manually: `sudo dscacheutil -flushcache`
- Wait for cache expiration

### Issue 5: Multiple Network Extensions

**Problem:** Behavior with multiple network extensions is not well-documented
**Status:** Testing suggests different types can coexist
**Workaround:**
- Extensive testing with common VPNs/tools
- Detect conflicts and warn users
- Provide diagnostics for troubleshooting

---

## Testing Strategy

### Compatibility Testing

**Test with these VPNs/Network Tools:**
- Tailscale (primary requirement)
- WireGuard
- OpenVPN
- Cisco AnyConnect
- Corporate VPNs
- Little Snitch (another NEFilterDataProvider app)
- NextDNS/1.1.1.1 (DNS proxy apps)

**Test Scenarios:**
1. Install blocking app first, then VPN
2. Install VPN first, then blocking app
3. Toggle both on/off in various orders
4. Verify traffic flows through both correctly
5. Test with and without encrypted DNS

### Blocking Accuracy Testing

**Applications to Test:**
- Safari
- Chrome
- Firefox
- Terminal (curl, wget)
- Mail.app
- Slack
- VS Code
- Apple apps (Maps, iMessage, etc.) - expect bypass

**Domains to Test:**
- Standard HTTP/HTTPS sites
- Sites with CDNs
- Sites with multiple domains
- Subdomains
- Wildcard domain rules

### Performance Testing

**Metrics:**
- Latency impact on connections
- CPU usage (idle and active)
- Memory footprint
- Battery impact (MacBook testing)
- Stability (crash rate, leak detection)

---

## Alternatives: Consumer vs Enterprise

### Consumer Distribution (Recommended)

**Approach:** NEFilterDataProvider via Developer ID
**Distribution:** DMG with installation guide
**Pros:**
- No supervision required
- Works on all consumer Macs
- Can coexist with VPNs
- Full control over installation

**Cons:**
- System extension approval friction
- Cannot use Mac App Store
- Higher support burden (approval issues)

### Enterprise/MDM Distribution

**Approach:** NEFilterDataProvider via MDM profile
**Distribution:** Configuration profile deployment
**Pros:**
- Automated deployment
- Centralized management
- No user approval required (with proper MDM)
- Better for fleet management

**Cons:**
- Requires MDM infrastructure
- Not suitable for consumer market
- More complex setup

---

## Future Considerations

### iOS 18/macOS 15+ URL Filtering API

Apple announced new URL Filtering APIs at WWDC 2025:
- Privacy-preserving URL filtering
- Granular HTTP/HTTPS filtering
- Better than flow-based filtering

**Status:** New API, less documentation
**Requirement:** Likely requires iOS 18+/macOS 15+
**Consideration:** Monitor for macOS consumer availability

### System Extension Deprecation

Apple has been moving toward System Extensions and away from kernel extensions since macOS 10.15.

**Current Status:** System Extensions are the modern approach
**Future Proof:** NEFilterDataProvider as System Extension is the way forward

---

## Conclusion

**For a macOS blocker compatible with Tailscale, the recommended approach is:**

1. **Primary:** NEFilterDataProvider (Content Filter System Extension)
   - No VPN required
   - No supervision required for macOS
   - Can coexist with Tailscale and other VPNs
   - Application and domain-level blocking
   - Apple's recommended approach

2. **Optional Add-on:** NEDNSProxyProvider for DNS-level filtering
   - Lightweight domain blocking
   - Complements NEFilterDataProvider
   - May conflict with Tailscale DNS features (test carefully)

3. **Distribution:** Developer ID + Notarization via DMG
   - Cannot use Mac App Store for System Extensions
   - Requires clear user guidance for approval

**Key Advantages Over iOS:**
- No supervision required (huge win for consumer apps)
- Can coexist with VPNs (Layer 4 vs Layer 3)
- More permissive sandbox (disk access, IPC)
- Better for consumer distribution

**Key Limitations:**
- Apple apps bypass filtering (~50 processes)
- System extension approval required from user
- Cannot distribute via Mac App Store
- Complex development and debugging

**Bottom Line:** macOS is significantly more viable for consumer blocking apps than iOS, especially when VPN compatibility is required. NEFilterDataProvider is the right tool for the job.
