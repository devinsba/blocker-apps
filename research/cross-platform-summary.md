# Cross-Platform Blocking Implementation Summary

## Quick Reference Guide for iOS and Android App/Domain Blocking

---

## Platform Comparison

| Feature | iOS (Enterprise) | iOS (Consumer) | Android |
|---------|-----------------|----------------|---------|
| **Domain Blocking** | NEFilterDataProvider (supervised) | NEPacketTunnelProvider (VPN) | VpnService with DNS filtering |
| **App Blocking** | NEFilterDataProvider (supervised) | Screen Time APIs | VpnService with per-app filtering |
| **Root Required** | No | No | No |
| **Special Entitlement Required** | Yes (from Apple) | Yes (from Apple) | No (but Google Play policy restrictions) |
| **Supervised Device Required** | Yes (NEFilter/NEDNSProxy) | No (VPN/Screen Time) | No |
| **Primary API** | NEFilterDataProvider | NEPacketTunnelProvider + FamilyControls | VpnService |
| **Traffic Inspection** | Yes (read-only) | Yes (full control) | Yes (full control) |
| **Traffic Modification** | No | Yes (in tunnel) | Yes |
| **Apple/System Apps Bypass** | Yes (Maps, APNs, etc.) | Yes (Maps, APNs, etc.) | No (all apps captured) |
| **One VPN at a Time Limit** | No | Yes | Yes |
| **Distribution Challenges** | MDM deployment only | VPN App Store approval risk | Google Play policy compliance |
| **Consumer Device Support** | No | Yes (with limitations) | Yes |
| **Battery Impact** | Low | Medium-High (VPN) | Medium-High (VPN) |

---

## iOS Implementation Paths

### Path 1: Consumer App (Recommended)
**Use**: Screen Time APIs + Personal VPN (NEPacketTunnelProvider)

**Architecture**:
- **App Blocking**: FamilyControls, ManagedSettings, DeviceActivity
- **Domain Blocking**: NEPacketTunnelProvider with DNS filtering

**Pros**:
- Works on all consumer devices (no supervision required)
- Direct app blocking with native UI
- System-wide domain blocking via VPN
- Can be distributed via App Store

**Cons**:
- VPN conflicts with other VPN apps (only one active)
- Apple apps bypass VPN (Maps, APNs, iCloud, etc.)
- App Store approval risk for VPN approach
- Higher battery impact from VPN tunnel
- "Finicky" Screen Time APIs with minimal documentation

**Best For**: Consumer focus apps, parental controls, productivity apps

**Critical Limitations**:
- Cannot block Apple app traffic reliably
- Must position as VPN-first to reduce rejection risk
- Users choose your app OR their existing VPN
- Expect 5-15% battery impact with VPN

### Path 2: Consumer App (Conservative)
**Use**: Screen Time APIs Only

**Architecture**:
- **App Blocking**: FamilyControls, ManagedSettings, DeviceActivity
- **Domain Blocking**: Limited to Safari websites only

**Pros**:
- Lower App Store rejection risk
- No VPN conflicts
- Better battery life
- Simpler implementation

**Cons**:
- Cannot block domains system-wide (Safari only)
- More limited functionality
- Still requires Screen Time entitlement

**Best For**: Apps focused primarily on app blocking with minimal domain blocking

### Path 3: Enterprise/Supervised Devices
**Use**: Network Extension (NEFilterDataProvider or NEDNSProxyProvider)

**Architecture**:
- **Content Filtering**: NEFilterDataProvider
- **DNS Filtering**: NEDNSProxyProvider

**Pros**:
- Apple's recommended approach
- Comprehensive filtering capabilities
- Lower battery impact than VPN
- Designed for this use case

**Cons**:
- Requires supervised devices (99%+ of consumers don't have)
- MDM/profile deployment only
- Not viable for App Store distribution
- Apple apps still bypass (but less than VPN)

**Best For**: Enterprise/MDM solutions, school deployments, managed environments

### Path 4: iOS 26+ URL Filtering
**Use**: New URL Filter API (Network Extension)
- **Pros**: Privacy-preserving, granular HTTP/HTTPS filtering
- **Cons**: New API (less documentation), likely still requires supervised devices
- **Best For**: Modern enterprise iOS deployments with iOS 26+ requirement

---

## Android Implementation Paths

### Path 1: VPN-Based Blocking (Recommended)
**Use**: VpnService
- **API**: android.net.VpnService
- **Pros**: No root needed, comprehensive control, works on all devices
- **Cons**: Only one VPN at a time, battery consumption, Google Play policies
- **Best For**: All-in-one blocking solution (apps + domains)

### Path 2: DNS-Based Domain Blocking
**Use**: VpnService + Local DNS Server
- **API**: VpnService with DNS filtering
- **Pros**: Efficient, simple, good battery life
- **Cons**: Can be bypassed with hardcoded IPs, doesn't block apps
- **Best For**: Domain/website blocking, ad blocking (careful with Play policy)

### Path 3: Combined VPN + Packet Inspection
**Use**: VpnService with full packet parsing
- **Pros**: Can block both apps and domains, comprehensive
- **Cons**: More complex, higher battery usage
- **Best For**: Advanced blocking with app and domain rules

---

## Recommended Architecture for Cross-Platform Blocker App

### iOS Consumer Implementation (Recommended)
```
App Blocking:
  └─ Screen Time APIs
      ├─ FamilyControls: Request authorization & app selection
      ├─ ManagedSettings: Configure shields/blocks
      ├─ DeviceActivity: Schedule blocking windows
      └─ DeviceActivityMonitor: Handle scheduled events

Domain Blocking (Optional - Higher Risk):
  └─ Personal VPN (NEPacketTunnelProvider)
      ├─ Create packet tunnel extension
      ├─ Configure DNS settings for filtering
      ├─ Implement DNS-based domain blocking
      ├─ Optional: Full packet inspection
      └─ Position as "Focus VPN" to reduce App Store risk

Shared Layer:
  ├─ Block list management (SQLite/Core Data)
  ├─ Schedule configuration
  ├─ Usage statistics
  └─ Settings and preferences

⚠️ Critical Considerations:
  • Only one VPN active at a time (conflicts with other VPNs)
  • Apple apps (Maps, APNs, etc.) bypass VPN
  • VPN approach has App Store approval risk
  • Battery optimization is critical
  • Make VPN toggleable/optional
```

### iOS Enterprise Implementation
```
Content Filtering (Supervised Devices):
  └─ Network Extension
      ├─ NEFilterDataProvider for filtering logic
      ├─ NEFilterManager for configuration
      └─ Deploy via MDM configuration profile

Distribution:
  └─ MDM deployment (not App Store)
```

### Android Implementation
```
Unified Blocking:
  └─ VpnService
      ├─ Virtual TUN interface
      ├─ Local DNS server for domain blocking
      ├─ Packet parser for app identification
      ├─ Filter engine for rules
      └─ Always-On VPN support

Components:
  ├─ VpnService (core)
  ├─ DNS filtering (domain blocks)
  ├─ Packet inspection (app blocks)
  ├─ UsageStatsManager (analytics)
  └─ Rule storage (SQLite)
```

### Shared Architecture (Flutter/React Native)
```
Platform-Agnostic Layer:
  ├─ Block List Management (apps + domains)
  ├─ Schedule Configuration
  ├─ Usage Statistics
  ├─ User Settings
  └─ UI Components

Platform-Specific Layer:
  ├─ iOS Native Module
  │   ├─ Screen Time APIs integration
  │   └─ Network Extension (optional)
  └─ Android Native Module
      └─ VpnService integration
```

---

## Critical Cross-Platform Limitations

### iOS-Specific Limitations

#### 1. Apple Apps Bypass All Network Extensions
**What**: Apple services (Maps, APNs, Find My, iCloud) bypass VPN and content filters
**Impact**: Cannot reliably block Apple app traffic
**Workaround**: Use Screen Time APIs to block Apple apps directly (better than network methods)
**Affects**: NEFilterDataProvider, NEPacketTunnelProvider, NEDNSProxyProvider

#### 2. One VPN at a Time
**What**: Only one VPN connection can be active on iOS
**Impact**: Your VPN conflicts with corporate VPNs, personal VPNs (NordVPN, etc.)
**Workaround**: Make VPN optional/toggleable, provide VPN-free mode
**Affects**: NEPacketTunnelProvider approach only

#### 3. Supervised Device Requirement (Enterprise APIs)
**What**: NEFilterDataProvider and NEDNSProxyProvider require supervised devices on iOS
**Impact**: 99%+ of consumer devices cannot use these APIs
**Workaround**: Use NEPacketTunnelProvider + Screen Time for consumer apps
**Affects**: NEFilterDataProvider, NEDNSProxyProvider (iOS only, macOS doesn't require supervision)

#### 4. App Store Rejection Risk
**What**: Inconsistent enforcement of VPN policy for content blocking
**Impact**: Apps may be rejected or removed after approval
**Workaround**: Position as VPN-first app, avoid "ad blocker" terminology, make blocking secondary feature
**Affects**: NEPacketTunnelProvider approach with blocking features

#### 5. Battery Drain
**What**: VPN tunneling all traffic increases battery consumption
**Impact**: Users may uninstall if impact >5-10%
**Workaround**: DNS-only mode, aggressive optimization, efficient packet handling
**Affects**: NEPacketTunnelProvider approach

#### 6. Limited Debugging
**What**: Network Extensions run in separate process with limited debugging
**Impact**: Difficult to diagnose issues, no disk writes, limited console access
**Workaround**: Use os_log and Console.app, extensive device testing
**Affects**: All Network Extension approaches

#### 7. Screen Time API Documentation
**What**: "Finicky" APIs with minimal official documentation
**Impact**: Trial and error development, unexpected behaviors, easy to misconfigure
**Workaround**: Community resources, extensive testing, proper App Groups configuration
**Affects**: FamilyControls, ManagedSettings, DeviceActivity

### Android-Specific Limitations

#### 1. Google Play VPN Policy
**What**: VPN must be primary feature, cannot be used solely for ad/content blocking
**Impact**: Apps may be rejected if not positioned correctly
**Workaround**: Frame as productivity/focus/VPN app, not pure ad blocker
**Affects**: VpnService-based apps

#### 2. One VPN at a Time
**What**: Only one VPN connection can be active on Android
**Impact**: Conflicts with other VPN apps, corporate VPNs
**Workaround**: Clearly communicate to users, detect existing VPN connections
**Affects**: VpnService approach

#### 3. Battery Consumption
**What**: Routing all traffic through app increases battery usage
**Impact**: User complaints and uninstalls
**Workaround**: Optimize packet handling, DNS-only mode, efficient algorithms
**Affects**: VpnService with full packet inspection

#### 4. DNS Bypass
**What**: Apps can use hardcoded IPs or DNS-over-HTTPS to bypass DNS filtering
**Impact**: Some apps/domains cannot be blocked via DNS-only filtering
**Workaround**: Full packet inspection, but increases complexity and battery drain
**Affects**: DNS-based filtering approach

#### 5. App Identification Complexity
**What**: Must parse packets and use UIDs to identify source apps
**Impact**: More complex than iOS Screen Time approach
**Workaround**: Study NetGuard implementation, use efficient packet parsing
**Affects**: Per-app blocking via VpnService

### Shared Limitations (Both Platforms)

#### 1. VPN Conflicts
**Both**: Only one VPN at a time
**Impact**: Users must choose your app or their existing VPN
**Severity**: High for users with corporate VPNs or existing VPN subscriptions

#### 2. Battery Impact
**Both**: VPN-based approaches drain battery
**Impact**: User retention affected if >5-10% battery drain
**Severity**: High - requires significant optimization effort

#### 3. Store Policy Compliance
**iOS**: Inconsistent VPN blocking policy enforcement
**Android**: Google Play VPN policy restrictions
**Impact**: Rejection risk on both platforms
**Severity**: Medium-High - requires careful positioning

#### 4. Cannot Guarantee 100% Blocking
**iOS**: Apple apps bypass, `includeAllNetworks` doesn't work fully
**Android**: DNS bypass with hardcoded IPs, DoH
**Impact**: Cannot provide absolute blocking guarantees
**Severity**: Medium - must set user expectations appropriately

#### 5. Complexity and Development Time
**Both**: Complex APIs, limited documentation, extensive testing required
**Impact**: Longer development cycles, higher cost
**Severity**: Medium - plan for 3-6 months of development per platform

## Key Differences to Consider

### Permission Model
- **iOS**: Request entitlement from Apple → User grants Family Controls permission
- **Android**: User grants VPN permission (per-app basis)

### Deployment
- **iOS**: App Store review + possible supervised device requirement
- **Android**: Google Play policy compliance (VPN primary purpose)

### Blocking Granularity
- **iOS**: App-level (Screen Time) or network-level (NEFilter)
- **Android**: Network-level with app identification via UID

### Scheduling
- **iOS**: Built-in with DeviceActivity framework
- **Android**: Must implement custom scheduling with AlarmManager/WorkManager

### User Experience
- **iOS**: System-provided shields with customization
- **Android**: Full control over blocking UX (can show custom screens)

---

## Development Roadmap

### Phase 1: Prototyping
1. **iOS**: Build Screen Time API prototype
   - Request entitlement from Apple
   - Implement basic app blocking with FamilyControls
   - Test shield configuration and scheduling
2. **Android**: Build VpnService prototype
   - Implement basic traffic interception
   - Add DNS filtering for domain blocking
   - Test per-app blocking with packet inspection

### Phase 2: Feature Parity
1. **Both Platforms**:
   - App blocking functionality
   - Domain blocking functionality
   - Schedule/time-based blocking
   - Block list management
   - Usage statistics

### Phase 3: Platform-Specific Features
1. **iOS**:
   - Custom shield configurations
   - Family sharing features
   - Evaluate Network Extension for domain blocking (if viable)
2. **Android**:
   - Always-On VPN support
   - Battery optimization
   - Advanced packet filtering rules

### Phase 4: Polish & Optimization
1. **Both Platforms**:
   - Performance optimization
   - Battery efficiency
   - UX refinement
   - Store submission preparation

---

## Critical Path Items

### Before Starting Development

#### iOS
- [ ] Request Screen Time API entitlement from Apple
- [ ] Request Network Extension entitlement (if needed)
- [ ] Set up App Groups in developer account
- [ ] Review Apple's guidelines for Screen Time apps

#### Android
- [ ] Study NetGuard source code thoroughly
- [ ] Review Google Play VPN policy
- [ ] Plan app positioning (productivity vs ad-blocking)
- [ ] Design VpnService architecture

### During Development

#### iOS
- [ ] Enable Family Controls capability in Xcode
- [ ] Configure App Groups for persistence
- [ ] Implement DeviceActivityMonitor extension
- [ ] Test on physical devices (simulators have limitations)

#### Android
- [ ] Implement efficient packet parsing
- [ ] Build local DNS server
- [ ] Handle VPN lifecycle properly
- [ ] Test battery impact extensively

### Before Launch

#### iOS
- [ ] Test supervised device deployment (if using Network Extension)
- [ ] Verify entitlement approval
- [ ] Test on multiple iOS versions
- [ ] Document supervision requirements clearly (if applicable)

#### Android
- [ ] Ensure Play Store policy compliance
- [ ] Position VPN as primary feature
- [ ] Test Always-On VPN compatibility
- [ ] Optimize battery usage

---

## Learning Resources

### iOS
- **Official**:
  - WWDC 2025: "Filter and tunnel network traffic with NetworkExtension"
  - Apple Developer Documentation (NEFilterDataProvider, FamilyControls)
- **Tutorials**:
  - Medium: "A Developer's Guide to Apple's Screen Time APIs" (Julius Brussee)
  - Medium: "Building a Powerful iOS App Blocker with Screen Time APIs" (JC)
  - X2 Mobile: "Implementing a network filter on an iOS device"
- **Sample Code**:
  - apple-ios-samples: SimpleTunnel
  - kingstinct/react-native-device-activity

### Android
- **Official**:
  - Android Developers: VPN Connectivity Guide
  - VpnService API Reference
- **Tutorials**:
  - "Android VPN Development: Guide for Mobile VPN Apps" (November 2025)
  - Medium: "Complete Guide to Implementing a VPN Service in Android" (Kotlin)
- **Sample Code**:
  - NetGuard (github.com/M66B/NetGuard) ⭐ Best reference
  - ToyVPN (official Android sample)
  - android-vpnservice-example

---

## Common Pitfalls to Avoid

### iOS
1. **Forgetting App Groups**: Restrictions won't persist without it
2. **Not requesting entitlement early**: Can block entire project
3. **Expecting good documentation**: APIs are poorly documented
4. **Ignoring supervised device requirement**: Can't distribute Network Extension to consumers (iOS)

### Android
1. **Violating Google Play policy**: Position as VPN/productivity tool
2. **Poor battery optimization**: Users will uninstall quickly
3. **Not handling VPN conflicts**: Only one VPN can be active
4. **Ignoring packet parsing complexity**: Use efficient algorithms or native code

---

## Success Metrics

### Technical
- Block accuracy: 99%+ for configured apps/domains
- Battery impact: <5% additional drain
- VPN/filter stability: <1% crash rate
- Latency impact: <50ms added latency

### User Experience
- Permission grant rate: >70%
- Setup completion: <2 minutes
- Daily active usage: >40%
- Retention (30-day): >50%

---

## Conclusion

Both platforms require platform-specific native implementations, but a shared Flutter/React Native UI layer can provide consistency. iOS has better built-in support for app blocking through Screen Time APIs, while Android requires more custom implementation via VpnService but offers greater flexibility. Plan for significant development time on both platforms due to complexity and platform restrictions.

The key to success is:
1. **Early entitlement requests** (iOS)
2. **Careful policy compliance** (Android)
3. **Thorough testing** of battery and performance
4. **Clear user communication** about permissions and limitations
