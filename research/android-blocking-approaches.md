# Android Domain and App Blocking Research

## Overview
Android provides multiple approaches for implementing domain and app blocking functionality, primarily through VPN-based solutions and various system APIs.

---

## Approach 1: VpnService API (Primary Method)

### Description
VpnService is the primary method for implementing no-root firewall and blocking apps on Android. It creates a virtual network interface to intercept and filter network traffic.

### Key API
- **android.net.VpnService**: Base class for building custom VPN solutions

### How It Works
1. Creates a virtual network interface (TUN interface)
2. Configures addresses and routing rules
3. Returns a file descriptor to the application
4. App reads outgoing IP packets from the local interface's file descriptor
5. App can inspect, filter, or modify packets
6. Packets are forwarded (or blocked) based on app logic

### Capabilities
- Intercept all network traffic without root access
- Block specific apps from accessing the internet
- Block specific domains/IP addresses
- Implement custom DNS filtering
- Create per-app firewall rules
- Monitor network traffic

### Required Permissions
```xml
<uses-permission android:name="android.permission.INTERNET" />
<uses-permission android:name="android.permission.WAKE_LOCK" />
<uses-permission android:name="android.permission.RECEIVE_BOOT_COMPLETED" />
```

### Manifest Configuration
```xml
<service
    android:name=".YourVpnService"
    android:permission="android.permission.BIND_VPN_SERVICE">
    <intent-filter>
        <action android:name="android.net.VpnService" />
    </intent-filter>
</service>
```

### Implementation Steps
```
1. Extend VpnService class
2. Request VPN permission from user (VpnService.prepare())
3. Create VPN interface using Builder
4. Configure IP address, DNS servers, and routes
5. Start background thread to read packets from file descriptor
6. Parse IP packets to determine source app and destination
7. Apply filtering rules (allow/block)
8. Forward allowed packets or drop blocked ones
```

### Platform Requirements
- No root access required
- User must grant VPN permission
- Android doesn't allow chaining of VPN services (only one active VPN at a time)

### Example Projects
- **NetGuard**: Popular open-source Android firewall (github.com/M66B/NetGuard)
- **ToyVPN**: Android's official sample project for VpnService basics
- **android-vpnservice-example**: Example implementation using blocking I/O (github.com/mightofcode/android-vpnservice-example)

### Limitations
- Only one VPN service can be active at a time
- Cannot be used alongside another VPN app
- Must handle all packet routing (significant complexity)
- Battery consumption considerations

---

## Approach 2: DNS Filtering

### Description
DNS-based blocking prevents domain resolution for blocked sites/services.

### How It Works
1. Run local DNS server through VpnService
2. Configure VPN to route DNS queries to local server
3. Block or redirect DNS queries for blocked domains
4. Return NXDOMAIN or custom IP for blocked domains

### Capabilities
- Block access to specific domains
- Block VPN services (by blocking their DNS entries)
- Lightweight compared to full packet inspection
- Can block ads and trackers

### Advantages
- Lower overhead than deep packet inspection
- Easier to implement than full VPN packet handling
- Effective for most domain-blocking use cases

### Limitations
- Can be bypassed with hardcoded IP addresses
- Doesn't work for apps that use their own DNS or DNS-over-HTTPS
- Cannot block based on app (only domain)

---

## Approach 3: Always-On VPN

### Description
Android feature that ensures all connections use VPN or are blocked.

### Configuration
- System-level setting
- Blocks all non-VPN network traffic
- Ensures filtering cannot be bypassed by disabling VPN

### Implementation
- User enables in Android Settings > Network & Internet > VPN > Always-on VPN
- App must properly handle being set as always-on VPN
- Provides additional security for blocking apps

### Use Case
- Ensures blocking cannot be bypassed
- Enterprise/parental control scenarios
- Requires user configuration in system settings

---

## Google Play Store Policy (Critical)

### Important Policy Changes
**Effective Date**: November 1, 2022

**Policy Statement**:
> Only VPN apps that use VPNService whose **primary function** is to provide a virtual private network shall be granted permission.

**Impact**:
- VPN apps should not manipulate ads that can impact other services' monetization
- Apps primarily focused on ad-blocking may face rejection
- October 2023: Google forbid Tasker from using VpnService class for blocking apps from internet access
- Google is restricting use of VpnService for non-VPN purposes

### Implications for Blocker Apps
- App must present VPN functionality as primary purpose
- Focus on "productivity" and "focus" features rather than pure ad-blocking
- Emphasize parental controls, screen time management, or productivity
- Be cautious about ad-blocking features in marketing/description

---

## Approach 4: Accessibility Service (App Usage Detection)

### Description
While not directly for blocking, Accessibility Service can detect app launches.

### Capabilities
- Detect when specific apps are launched
- Monitor app usage
- Trigger notifications or warnings

### Limitations
- Cannot actually block apps (only detect)
- Requires significant permissions (raises privacy concerns)
- Can be disabled by user
- Not recommended as primary blocking mechanism

### Use Case
- Companion to VpnService for app detection
- Usage monitoring and analytics
- Trigger reminders rather than hard blocks

---

## Approach 5: Device Admin / Work Profiles (Enterprise)

### Description
Enterprise-focused APIs for device management.

### APIs
- **DevicePolicyManager**: For device administration
- **Work Profiles**: Separate profile with managed apps

### Capabilities
- Disable apps
- Set app restrictions
- Enforce policies

### Requirements
- Typically requires enterprise enrollment or work profile setup
- Not suitable for consumer apps
- Requires device owner or profile owner privileges

### Use Case
- Enterprise/MDM solutions
- Not practical for consumer blocker apps

---

## Recommended Implementation Strategy

### For Domain Blocking
**Primary Approach**: VpnService with DNS filtering

**Implementation**:
1. Create VpnService implementation
2. Set up local DNS server
3. Route all DNS queries through local server
4. Block domains based on user-configured block list
5. Optionally implement full packet inspection for IP-based blocking

**Advantages**:
- No root required
- Effective for most use cases
- Good battery efficiency with DNS-only filtering
- Works on all modern Android versions

### For App Blocking
**Primary Approach**: VpnService with per-app filtering

**Implementation**:
1. Use VpnService to intercept all traffic
2. Parse IP packets to determine source app (using UID)
3. Block all traffic from specific apps
4. Optionally combine with UsageStatsManager for monitoring

**Alternative**:
- Use Accessibility Service for detection + VpnService for blocking
- Provides better app usage insights

### Combined Approach
**Recommended Architecture**:
```
1. VpnService as core blocking mechanism
2. Local DNS server for domain-based blocking
3. Packet filtering for app-based blocking
4. UsageStatsManager for usage analytics
5. Always-On VPN support for bypass prevention
```

---

## Implementation Considerations

### Battery Optimization
- Use efficient packet parsing
- Implement DNS caching
- Avoid unnecessary deep packet inspection
- Consider DNS-only mode for better battery life

### User Experience
- Clear VPN permission explanation
- Easy-to-understand block list management
- Usage statistics and insights
- Whitelist/exceptions support

### Google Play Compliance
- Position as productivity/focus tool
- Emphasize screen time management
- Include parental control features
- Avoid emphasizing ad-blocking in app description
- Ensure VPN is presented as primary functionality

### Performance
- Efficient packet handling (consider native code for packet parsing)
- Background service optimization
- Memory management for long-running service
- Handle VPN reconnection gracefully

---

## Code Examples and Resources

### Official Documentation
- Android Developers: "VPN | Connectivity" guide
- VpnService API Reference
- Microsoft Learn: VpnService Class documentation

### Tutorial Resources
- **Android VPN Development Guide** (November 2025): Comprehensive guide with ToyVPN reference
- **Complete Guide to Implementing a VPN Service in Android** (Medium, Kotlin): Code examples with implementation details
- **NetGuard FAQ**: Practical insights from production app

### Open Source Examples
- **NetGuard** (github.com/M66B/NetGuard): Full-featured Android firewall
  - Shows real-world implementation
  - Handles edge cases
  - Production-quality code
- **android-vpnservice-example**: Simple example with blocking I/O
- **ToyVPN**: Official Android sample

---

## Technical Architecture Example

### High-Level Flow
```
User App
    ↓
VpnService
    ↓
Virtual TUN Interface (file descriptor)
    ↓
Packet Reader Thread
    ↓
Packet Parser (get app UID, destination IP/domain)
    ↓
Filter Engine (apply rules)
    ↓
Verdict: Allow → Forward packet
         Block → Drop packet
```

### Components Needed
1. **VpnService Implementation**: Core service
2. **Packet Parser**: Parse IP/TCP/UDP headers
3. **DNS Server**: Handle DNS queries locally
4. **Filter Engine**: Apply blocking rules
5. **Rule Storage**: SQLite or similar for block lists
6. **UI Layer**: Configure apps/domains to block
7. **Statistics Tracker**: Usage monitoring

---

## Next Steps

1. **Study NetGuard source code** - Learn from production implementation
2. **Build VpnService prototype** - Start with basic traffic interception
3. **Implement DNS filtering** - Add domain blocking capability
4. **Add per-app filtering** - Implement app blocking
5. **Test battery impact** - Optimize for efficiency
6. **Plan Google Play compliance** - Ensure policy adherence
7. **Design user experience** - Focus on productivity/focus messaging

---

## Key Takeaways

- **VpnService is the standard approach** for Android blocking apps
- **No root required**, but requires user VPN permission grant
- **Google Play policies** must be carefully considered in design
- **NetGuard is the best reference** implementation to study
- **DNS filtering** is most efficient for domain blocking
- **Packet inspection** needed for app-level blocking
- **Battery optimization** is critical for user retention
- **Frame as productivity/focus tool** for Play Store compliance
