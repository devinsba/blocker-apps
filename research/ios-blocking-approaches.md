# iOS Domain and App Blocking Research

## Overview
iOS provides multiple approaches for implementing domain and app blocking functionality, each with different capabilities, requirements, and use cases.

---

## Approach 1: Network Extension Framework (Domain/Network Blocking)

### Description
Network Extension framework allows filtering network traffic at the system level using content filters.

### Key APIs
- **NEFilterManager**: Manages filter configuration
- **NEFilterDataProvider**: Subclass to implement filtering logic (provides filter verdicts)
- **NEFilterControlProvider**: Manages filter control operations
- **NEFilterFlow**: Represents network flows to filter

### Capabilities
- Filter TCP and UDP flows
- Filter other IP protocol traffic (e.g., ICMP)
- Inspect traffic flows and provide 'allow' or 'drop' verdicts
- System-wide network filtering
- Cannot modify traffic (read-only inspection)

### iOS 26 (2025) Updates - URL Filter API
- New URL Filter type for filtering HTTP/HTTPS requests based on full URL
- Privacy-preserving content filtering with cryptographic approach
- More granular blocking capabilities
- Paradigm shift toward privacy-preserving filtering

### Platform Requirements
- **iOS**: Requires supervised devices for production deployment
- **macOS**: Different distribution options available
- Requires special entitlements from Apple
- Testing can use NEFilterManager configuration
- Production requires configuration profile deployment

### Implementation Details
```
1. Create Network Extension target in Xcode
2. Subclass NEFilterDataProvider
3. Override flow handling methods
4. Implement filtering logic (allow/block decisions)
5. Configure using NEFilterManager (testing) or profile (production)
```

### Limitations
- Supervised device requirement for iOS production
- Cannot modify traffic
- Runs in very restrictive sandbox:
  - Cannot make network calls
  - Cannot write data to disk
- Significant distribution/deployment limitations (see TN3134)

### Resources
- WWDC 2025 Session: "Filter and tunnel network traffic with NetworkExtension"
- Apple Developer Documentation: NEFilterDataProvider, NEFilterManager
- X2 Mobile Tutorial (May 2025): "Implementing a network filter on an iOS device"
- GitHub: robovm/apple-ios-samples SimpleTunnel example

---

## Approach 2: Personal VPN / Packet Tunnel (Domain Blocking - Consumer Apps)

### Description
NEPacketTunnelProvider allows creating a VPN tunnel on iOS that can intercept and filter network traffic. This is the approach used by many consumer VPN apps in the App Store for domain blocking and content filtering.

### Key API
- **NEPacketTunnelProvider**: Creates a packet tunnel for VPN functionality
- **NETunnelProviderManager**: Manages tunnel configuration
- **NEPacketTunnelNetworkSettings**: Configures DNS, IP addresses, and routing

### How It Works
1. Creates a virtual network interface (TUN interface)
2. Routes traffic through the tunnel
3. Intercepts packets at IP layer
4. Can implement DNS-based filtering or packet inspection
5. Forwards or drops packets based on filtering rules

### Capabilities
- Domain/website blocking via DNS filtering
- Content filtering via packet inspection
- Works on non-supervised consumer devices
- Can be distributed via App Store
- User grants VPN permission (not supervision required)
- Full control over packet handling and routing

### Platform Requirements
- Requires Network Extension entitlement from Apple
- User must grant VPN permission
- Works on all iOS devices (no supervision required)
- Only one VPN can be active at a time on device

### Implementation Approaches

#### DNS-Based Filtering (Recommended)
```
1. Create NEPacketTunnelProvider extension
2. Configure custom DNS settings
3. Route DNS queries through local resolver
4. Block domains by returning NXDOMAIN or sinkhole IP
5. Forward allowed traffic through tunnel
```

#### Full Packet Inspection
```
1. Create NEPacketTunnelProvider extension
2. Read all IP packets from tunnel
3. Parse packet headers (IP/TCP/UDP)
4. Inspect destination addresses/ports
5. Apply filtering rules and forward/drop packets
```

### Apple's Official Position vs Reality

**Official Guidance**: Apple engineers state that NEPacketTunnelProvider should only be used for actual VPN tunneling, not for content filtering. They recommend using NEFilterDataProvider for content filtering instead.

**Reality**: Many consumer VPN apps successfully use NEPacketTunnelProvider for domain blocking and ad-blocking features and are approved on the App Store. However, Apple's position is inconsistent.

### App Store Approval Challenges

#### Known Rejections
- **Guideline 2.5.1**: Some apps have been rejected for "using a VPN profile to block ads or other content in a third-party app"
- Enforcement appears inconsistent - some apps with blocking features are approved while others are rejected

#### Successful Apps
- Apps like IVPN, AdGuard, and others have introduced "AntiTracker" and ad-blocking features
- These apps position VPN as the primary feature with blocking as secondary functionality
- Success often depends on how the app is marketed and described

#### Risk Factors
- Using NEPacketTunnelProvider "for a purpose other than a VPN" is not a DTS-supported use case
- No clear public guidance on what will be approved vs rejected
- Apps could be pulled after initial approval if Apple changes enforcement
- Better to position as a VPN with privacy features rather than pure ad/content blocker

### Technical Limitations

#### 1. Apple Apps Bypass VPN Tunnels
**Critical Issue**: Some Apple services bypass Network Extensions entirely, including:
- Apple Maps
- Apple Push Notification Service (APNs)
- Find My
- iCloud services
- Location Services

**The `includeAllNetworks` Problem**:
- Apple introduced `includeAllNetworks` property (iOS 14+) to force all traffic through VPN
- Documentation originally said "the system sends all network traffic over the tunnel"
- Current documentation now says "**most** network traffic" (changed without notice)
- Tests on iOS 16.1+ show traffic still leaks to Apple servers even with this enabled
- Apple's position: "NetworkExtension framework makes no guarantees about where traffic will flow"

**What This Means**:
- Cannot reliably block all domains/traffic on iOS
- Apple services may transmit data outside VPN tunnel
- No way to fully prevent Apple app bypass
- This affects privacy and parental control use cases

#### 2. Only One VPN at a Time
- iOS allows only one active VPN connection
- Your app conflicts with other VPN apps
- Users must choose between your app and their existing VPN
- Corporate VPNs take precedence over personal VPNs

#### 3. Operating at Wrong Layer
- NEPacketTunnelProvider operates at IP layer
- Does not receive application-level metadata
- Cannot easily determine which app generated traffic
- DNS interception is hacky (Apple recommends NEDNSProxyProvider instead)

#### 4. Battery and Performance Impact
- Tunneling all traffic increases battery drain
- Must handle all packet routing efficiently
- Poor implementation significantly impacts device performance
- Users may uninstall due to battery concerns

### DNS Proxy Alternative (NEDNSProxyProvider)

**What It Is**: Dedicated API for DNS filtering and proxy functionality

**Limitations**:
- **iOS**: Only works on supervised devices (same limitation as NEFilterDataProvider)
- **macOS**: Works on all devices without supervision requirement
- Not viable for consumer iOS apps

**Apple's Recommendation**: Use NEDNSProxyProvider for DNS filtering, but supervision requirement makes this impractical for consumer apps.

### Limitations Summary

| Limitation | Impact | Workaround |
|------------|--------|------------|
| Apple apps bypass VPN | Cannot block Apple traffic | None - Apple's intentional design |
| One VPN at a time | Conflicts with other VPNs | Educate users, provide toggle |
| App Store rejection risk | App may be removed | Position as VPN-first with privacy features |
| Battery drain | User complaints/uninstalls | Optimize packet handling, DNS-only mode |
| No app-level metadata | Hard to identify source app | Use heuristics, focus on domain blocking |
| `includeAllNetworks` doesn't work fully | Traffic leaks | Accept limitation, document behavior |

### When to Use This Approach

**Good Fit**:
- Consumer-facing apps (non-supervised devices)
- Privacy-focused VPN with blocking features
- Focus apps with website blocking
- Parental control apps (with supervision as optional)

**Not a Good Fit**:
- Enterprise content filtering (use NEFilterDataProvider)
- Apps requiring guaranteed blocking of all traffic
- Apps needing to block Apple services
- Pure ad-blockers (high rejection risk)

### Resources
- Substack: "iOS Network Extensions and Personal VPN: A Developer's Guide" by Anton Gubarenko
- NetworkSpy Blog: "iOS Network Extension VPN: Frameworks, Packet Tunnels, and API Debugging"
- IVPN Blog: "Insights about Apple App Store Rules for VPN Apps"
- Mullvad Blog: "Why we still don't use includeAllNetworks"
- Apple Developer Forums: NEPacketTunnelProvider discussions

---

## Approach 3: DNS Proxy Provider (Domain Blocking - Supervised Devices Only)

### Description
NEDNSProxyProvider is Apple's dedicated API for DNS filtering and custom DNS resolution.

### Key API
- **NEDNSProxyProvider**: Intercepts and handles DNS queries
- **NEDNSProxyManager**: Manages DNS proxy configuration

### Capabilities
- Intercept all DNS queries system-wide
- Implement custom DNS resolution logic
- Block domains by controlling DNS responses
- DNS-over-HTTPS/DNS-over-TLS support
- Lower overhead than full VPN tunneling

### Platform Requirements
- **iOS**: Only works on supervised devices
- **macOS**: Works on all devices (no supervision required)
- Requires Network Extension entitlement

### Limitations
- **iOS consumer apps cannot use this** due to supervision requirement
- Only handles DNS (can be bypassed with hardcoded IPs)
- Cannot inspect or modify actual traffic
- Cannot identify source app for DNS queries

### When to Use
- macOS apps (no supervision required)
- Enterprise iOS deployments with supervised devices
- MDM-managed environments
- Not viable for consumer iOS apps

---

## Approach 4: Screen Time APIs (App Blocking)

### Description
Screen Time APIs provide app-level blocking and monitoring capabilities introduced for parental controls and focus/productivity apps.

### Key Frameworks
1. **FamilyControls**: Requests user permission and allows app/website selection
2. **ManagedSettings**: Configures actual blocking restrictions
3. **DeviceActivity**: Schedules and monitors usage

### Additional APIs
- **DeviceActivityMonitor**: Extension for monitoring scheduled activities
- **ShieldConfiguration**: Customizes shield UI
- **ShieldAction**: Handles shield interaction events

### Capabilities
- Block apps completely (hide from home screen, prevent launch)
- Shield apps (overlay on top, allow launch but show warning)
- Schedule blocks (time-based restrictions)
- Monitor app usage
- Website blocking
- Category-based blocking

### Platform Requirements
- Requires special entitlement from Apple (request before development)
- Requires App Groups capability (critical for persistence)
- Must enable Family Controls in Xcode
- Works on non-supervised devices

### Implementation Pattern
```
1. Request FamilyControls authorization
2. Use FamilyActivityPicker to select apps to block
3. Configure ManagedSettings to set up blocks
4. Use DeviceActivity to schedule when blocks are active
5. Implement DeviceActivityMonitor extension for monitoring
6. Customize ShieldConfiguration for UI
```

### Blocking vs Shielding
- **Blocking**: Hides app from home screen, prevents launch completely
- **Shielding**: Shows overlay on app, allows launch with restriction UI

### Known Challenges
- APIs described as "very finnicky"
- Minimal official Apple documentation
- Requires careful configuration of App Groups for persistence
- Half-baked documentation ("solving a puzzle with half the pieces missing")

### Resources
- Medium: "A Developer's Guide to Apple's Screen Time APIs" by Julius Brussee
- Medium: "SwiftUI Tutorial: iOS App Blocker with Screen Time APIs" by JC
- Blog: pedroesli.com - "Using Screen Time API to block apps for a specified time"
- GitHub: kingstinct/react-native-device-activity

---

## Comparison: All iOS Blocking Approaches

| Feature | Content Filter (NEFilter) | Personal VPN (NEPacketTunnel) | DNS Proxy (NEDNSProxy) | Screen Time APIs |
|---------|--------------------------|------------------------------|----------------------|------------------|
| **Primary Use Case** | Enterprise content filtering | Consumer domain blocking | DNS filtering | App blocking |
| **Supervised Device Required (iOS)** | Yes | No | Yes | No |
| **Entitlement Required** | Yes | Yes | Yes | Yes |
| **Can Block Apps** | Indirectly (network) | Indirectly (network) | Indirectly (DNS) | Directly |
| **Can Block Domains** | Yes | Yes | Yes (DNS only) | Limited (websites) |
| **Can Inspect Traffic** | Yes (read-only) | Yes (full control) | No (DNS only) | No |
| **Can Modify Traffic** | No | Yes | No | N/A |
| **Apple Apps Bypass** | Yes (Maps, etc.) | Yes (APNs, Maps, etc.) | Yes | No |
| **Scheduling Support** | Manual | Manual | Manual | Built-in (DeviceActivity) |
| **UI Customization** | N/A | Full control | N/A | Yes (ShieldConfiguration) |
| **Consumer iOS Viable** | No | Yes | No | Yes |
| **Battery Impact** | Low | Medium-High | Low | Minimal |
| **App Store Approval** | N/A (enterprise) | Risky (inconsistent) | N/A (supervision) | Generally approved |
| **Distribution Model** | MDM/Profile | App Store | MDM/Profile | App Store |
| **Conflicts with VPN** | No | Yes (only one VPN) | No | No |

---

## Recommended Approach for Blocker App

### For Consumer iOS Apps (Non-Supervised Devices)

#### Best Combination: Screen Time + Personal VPN
**Recommended Architecture**:
1. **Screen Time APIs** for app blocking
   - Direct app blocking and shielding
   - Rich scheduling with DeviceActivity
   - Better user experience for app restrictions
   - Generally approved by App Store

2. **NEPacketTunnelProvider VPN** for domain blocking
   - DNS-based filtering for website/domain blocking
   - Works on consumer devices
   - Position as "Focus VPN" or "Privacy VPN"
   - Make VPN the primary feature to reduce App Store risk

**Critical Considerations**:
- Users must choose between your VPN and other VPNs (only one active)
- Apple apps will bypass VPN blocking (APNs, Maps, etc.)
- App Store approval is risky - position carefully
- Higher battery impact from VPN tunnel

#### Alternative: Screen Time Only
**If you want to avoid VPN risks**:
- Use only Screen Time APIs
- Block apps directly
- Use website blocking for Safari domains (limited)
- Safer App Store approval path
- No VPN conflicts

**Limitations**:
- Cannot block domains system-wide (only Safari)
- Cannot block network access for specific apps
- Limited to what Screen Time APIs provide

### For Enterprise/Supervised Devices

#### Best Approach: Content Filter or DNS Proxy
- **NEFilterDataProvider**: Most comprehensive filtering
- **NEDNSProxyProvider**: DNS-based filtering with lower overhead
- Deploy via MDM and configuration profiles
- Full control over network traffic
- No App Store distribution concerns

### For macOS Apps

#### Best Approach: Any Network Extension
- **NEFilterDataProvider**: No supervision requirement on macOS
- **NEDNSProxyProvider**: No supervision requirement on macOS
- **NEPacketTunnelProvider**: VPN-based if needed
- All approaches viable for consumer macOS apps

### Platform-Specific Strategies

#### iOS Consumer App Recommendation
```
Primary: Screen Time APIs (app blocking) + NEPacketTunnelProvider (domain blocking)

Pros:
✓ Works on all consumer devices
✓ Direct app blocking with good UX
✓ Domain blocking via VPN DNS filtering
✓ Can be distributed via App Store

Cons:
✗ VPN conflicts with other VPN apps
✗ App Store rejection risk for VPN approach
✗ Apple apps bypass VPN blocking
✗ Higher battery usage
✗ Cannot guarantee 100% blocking coverage

Mitigation:
- Position as "Focus & Productivity VPN"
- Make VPN optional/toggleable
- Educate users about Apple bypass behavior
- Optimize battery usage aggressively
- Provide DNS-only mode option
```

#### iOS Enterprise App Recommendation
```
Primary: NEFilterDataProvider or NEDNSProxyProvider

Pros:
✓ Designed for this use case
✓ Lower overhead than VPN
✓ No VPN conflicts
✓ Official Apple recommendation

Cons:
✗ Requires supervised devices
✗ Not viable for consumer distribution
✗ Apple apps still bypass (but less than VPN)

Distribution:
- MDM deployment
- Configuration profiles
- Enterprise App Store
```

---

## Deep Dive: Critical Limitations You Must Understand

### 1. Apple Apps Bypass - The Biggest Limitation

**What Gets Bypassed**:
- Apple Maps traffic
- Apple Push Notification Service (APNs)
- Find My network traffic
- iCloud sync traffic
- FaceTime/iMessage in some cases
- Location Services
- System update checks
- Potentially other undocumented Apple services

**Technical Details**:
- Affects ALL Network Extension approaches (NEFilter, NEPacketTunnel, NEDNSProxy)
- The `includeAllNetworks` flag was supposed to prevent this (iOS 14+)
- Apple quietly changed behavior in iOS 16+ without announcement
- Documentation changed from "all traffic" to "most traffic"
- Apple's official position: "NetworkExtension makes no guarantees about where traffic will flow"

**Real-World Impact**:
- **Privacy apps**: Cannot guarantee full privacy protection
- **Parental controls**: Children can potentially access restricted content via Apple apps
- **Focus apps**: Cannot prevent all distractions (Maps, Apple News, etc.)
- **Enterprise filtering**: Some Apple traffic escapes monitoring

**No Workaround**: This is intentional by Apple. Consider this an unavoidable limitation.

**What You Can Do**:
- Clearly document this limitation to users
- Focus on blocking third-party apps and domains
- Use Screen Time APIs to block Apple apps (works better than network-based blocking)
- Set expectations appropriately in marketing

### 2. Supervised Device Requirements

**Which APIs Require Supervision**:
- NEFilterDataProvider (iOS only; macOS doesn't require it)
- NEDNSProxyProvider (iOS only; macOS doesn't require it)

**Which APIs Don't Require Supervision**:
- NEPacketTunnelProvider (works on consumer devices)
- Screen Time APIs (works on consumer devices)

**What "Supervised" Means**:
- Device must be enrolled in MDM or Apple Configurator
- Requires wiping device to enable (or setup during initial configuration)
- Shows "This iPhone is supervised and managed" message
- Provides additional management controls
- Typically used in enterprise/school/parental control scenarios

**Why This Kills Consumer Apps**:
- 99%+ of consumer devices are not supervised
- Users won't wipe their device to use your app
- Makes NEFilterDataProvider and NEDNSProxyProvider non-viable for App Store apps
- Forces use of NEPacketTunnelProvider despite Apple's recommendations against it

### 3. App Store Rejection Risks for VPN Blocking Apps

**The Paradox**:
- Apple says NEPacketTunnelProvider is only for VPNs, not content filtering
- Apple says to use NEFilterDataProvider for content filtering
- NEFilterDataProvider requires supervised devices on iOS
- Therefore, consumer apps are forced to "misuse" NEPacketTunnelProvider

**Rejection Scenarios**:
- **Guideline 2.5.1**: "Using VPN profile to block ads or content in third-party apps"
- **Unclear enforcement**: Some apps approved, others rejected for same functionality
- **Post-approval removal**: Apps can be pulled after being live for months/years

**Apps That Have Succeeded**:
- IVPN (AntiTracker feature)
- AdGuard
- NextDNS
- Various "Focus" and "Parental Control" apps

**Success Factors**:
- Position VPN as primary feature
- Describe blocking as "privacy protection" not "ad blocking"
- Don't emphasize blocking in app name/marketing
- Frame as productivity/focus/parental control tool
- Avoid words like "firewall" or "ad blocker"

**Risk Assessment**: Medium-High
- No guarantees
- Inconsistent enforcement
- Could change at any time
- Plan for possibility of rejection

### 4. Performance and Battery Limitations

**VPN Tunnel (NEPacketTunnelProvider) Impact**:
- All traffic routed through your code
- Must handle every packet efficiently
- Poor implementation = severe battery drain
- Users WILL uninstall if battery impact >5-10%

**Optimization Strategies**:
- DNS-only filtering (don't inspect all packets)
- Efficient packet parsing (use native code if needed)
- Avoid unnecessary processing
- Cache DNS results
- Minimize wake locks
- Profile extensively on real devices

**Screen Time APIs Impact**:
- Minimal battery impact
- System handles most work
- Extension runs only during scheduled events

**Content Filter (NEFilterDataProvider) Impact**:
- Lower than VPN tunnel
- Only receives relevant flows
- System optimized

### 5. Debugging and Development Challenges

**Network Extension Debugging**:
- Extension runs in separate process
- Limited debugging capabilities
- Cannot easily use Xcode debugger
- Logging is challenging (no disk writes, limited console)
- Must use os_log and view in Console.app
- Crashes are hard to diagnose

**Screen Time APIs Debugging**:
- Described as "very finnicky"
- Minimal official documentation
- Community relies on trial and error
- Many edge cases and unexpected behaviors
- App Groups configuration critical but easy to misconfigure

**Testing Requirements**:
- Must test on physical devices (simulators limited)
- Must test across iOS versions (behavior changes)
- Must test with other VPNs installed (conflicts)
- Must test battery impact over extended periods
- Must test with various network conditions

### 6. One VPN at a Time Limitation

**The Problem**:
- iOS allows only one active VPN connection
- Your VPN app conflicts with:
  - Corporate VPNs
  - Personal VPNs (NordVPN, ExpressVPN, etc.)
  - Other filtering apps using VPN approach
  - Always-On VPN configurations

**User Impact**:
- Users must choose your app OR their VPN
- Many users have existing VPN subscriptions
- Corporate device users cannot use your app during work hours
- Creates friction and potential refunds

**Mitigation Strategies**:
- Make VPN feature optional/toggleable
- Clearly communicate VPN conflict in onboarding
- Provide "VPN-free mode" using only Screen Time APIs
- Detect existing VPN and show helpful message
- Allow easy enable/disable of VPN feature

### 7. Enterprise vs Consumer Trade-offs

| Aspect | Enterprise (Supervised) | Consumer (Non-Supervised) |
|--------|------------------------|---------------------------|
| **Best API** | NEFilterDataProvider | NEPacketTunnelProvider + Screen Time |
| **Blocking Reliability** | High | Medium (Apple bypass issues) |
| **Deployment** | MDM/Profile | App Store |
| **User Friction** | Low (admin controlled) | High (permissions, VPN conflicts) |
| **Distribution Scale** | Limited (known organizations) | Unlimited (App Store) |
| **Revenue Model** | B2B licensing | B2C subscriptions/ads |
| **Support Burden** | Lower (IT admins) | Higher (end users) |
| **App Store Risk** | None (not on store) | Medium-High |

### 8. What You Cannot Do on iOS (Consumer Devices)

**Impossible or Extremely Difficult**:
- ❌ Block all Apple app traffic reliably
- ❌ Guarantee 100% blocking coverage
- ❌ Work alongside other VPN apps simultaneously
- ❌ Identify source app for all network traffic (VPN approach)
- ❌ Block at app-level using network methods (use Screen Time instead)
- ❌ Modify network traffic contents (only allow/block verdicts)
- ❌ Write comprehensive logs to disk from Network Extension
- ❌ Use NEFilterDataProvider or NEDNSProxyProvider without supervision
- ❌ Get detailed app metadata in VPN tunnel provider

**Possible but Challenging**:
- ⚠️ Get App Store approval for VPN blocking app (inconsistent)
- ⚠️ Achieve <5% battery impact with VPN tunnel (requires optimization)
- ⚠️ Block domains in apps that use hardcoded IPs (DNS bypass)
- ⚠️ Block apps that use DNS-over-HTTPS to custom servers
- ⚠️ Debug Network Extension issues effectively
- ⚠️ Maintain backward compatibility across iOS versions (API changes)

---

## Next Steps

### Before Writing Any Code

1. **Decide on target market**:
   - Consumer (non-supervised)? → Plan for Screen Time + VPN approach
   - Enterprise (supervised)? → Plan for NEFilterDataProvider approach
   - Both? → Plan for two different implementations

2. **Request entitlements from Apple**:
   - Screen Time API entitlement (Family Controls)
   - Network Extension entitlement
   - Allow 1-2 weeks for approval
   - Explain your use case clearly in request

3. **Assess App Store risk tolerance**:
   - High risk tolerance? → Include VPN domain blocking
   - Low risk tolerance? → Screen Time APIs only
   - Create contingency plan for VPN rejection

4. **Plan for limitations**:
   - Document Apple bypass behavior in user education
   - Design UI to handle VPN conflicts gracefully
   - Set realistic expectations about blocking coverage
   - Plan battery optimization strategy

### Development Phase

5. **Prototype Screen Time APIs first**:
   - Broader device support
   - Lower App Store risk
   - Generally approved
   - Core app blocking functionality

6. **Add VPN functionality second** (if desired):
   - DNS-based filtering for domains
   - Position as "Focus VPN" or "Privacy VPN"
   - Make optional/toggleable
   - Optimize battery aggressively

7. **Test extensively**:
   - Multiple iOS versions
   - With/without other VPNs installed
   - Battery impact over days/weeks
   - Various network conditions
   - Both Wi-Fi and cellular

8. **Monitor and adapt**:
   - Track App Store review feedback
   - Monitor battery complaints
   - Be prepared to remove VPN feature if rejected
   - Have Screen Time-only version ready as fallback

### Critical Questions to Answer

- Are you building for consumer or enterprise?
- Can you accept Apple apps bypassing blocking?
- Is VPN conflict acceptable for your use case?
- What's your risk tolerance for App Store rejection?
- Can you achieve acceptable battery performance?
- Do you have resources for extensive testing and optimization?

---

## Additional Notes

- **All approaches require entitlements from Apple** - request early
- **No perfect solution exists** - every approach has significant trade-offs
- **Apple holds all the cards** - they can change rules/behavior anytime
- **Screen Time APIs are safest bet** for consumer apps
- **VPN approach is necessary** for system-wide domain blocking on consumer devices
- **macOS has fewer limitations** - consider starting there
- **iOS 26 improvements** may change landscape (monitor WWDC announcements)
- **Documentation is sparse** - rely on community knowledge and experimentation
- **Expect significant development time** - these are complex, finicky APIs
- **User education is critical** - limitations must be clearly communicated
