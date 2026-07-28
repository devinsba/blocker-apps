# Fleet MDM - Comprehensive Research Report

## Executive Summary

Fleet is an open-source, cross-platform device management (MDM) solution that supports iOS, Android, macOS, Windows, Linux, and ChromeOS. It's the **only verified, production-ready open-source MDM** that covers all platforms needed for the blocker app project. Fleet is built on osquery, nanoMDM, Nudge, and swiftDialog, offering GitOps-enabled MDM under an MIT license (core features).

**Key Verdict for Blocker App Project**: ✅ **Recommended** - Best available option for testing supervision features and enterprise deployment prototyping.

---

## Overview

### What is Fleet?

Fleet is a device management platform that combines:
- **MDM Capabilities**: Traditional mobile device management for iOS, macOS, Android
- **Osquery Management**: SQL-based system monitoring and security
- **GitOps Integration**: Infrastructure-as-code approach to device management
- **Cross-Platform Support**: Single platform for all major operating systems

### Project Status

- **GitHub**: github.com/fleetdm/fleet (verified, 2.7k+ stars)
- **License**: Majority MIT (open-source core), Premium features under separate license
- **Development**: Very active (daily commits as of 2025)
- **Maturity**: Production-ready, used by Netflix, Stripe, Fastly, Uber
- **Scale**: Deployments from dozens to 400,000+ hosts

---

## Platform Support

### Supported Operating Systems

| Platform | Support Level | MDM Features | Status |
|----------|--------------|--------------|--------|
| **macOS** | ✅ Full | Complete Apple MDM | Production |
| **iOS** | ✅ Full | Complete Apple MDM | Added 2025 |
| **iPadOS** | ✅ Full | Complete Apple MDM | Added 2025 |
| **Android** | ✅ Full | Android Enterprise | Moved to production 2025 |
| **Windows** | ✅ Full | OMA-DM/SCEP | Production |
| **Linux** | ✅ Full | Agent-based | Production |
| **ChromeOS** | ✅ Full | Chrome management | Production |

**Perfect Match**: Fleet supports all platforms planned for the blocker app (iOS, Android, macOS, Windows, Linux).

---

## Key Features

### MDM Capabilities

#### Apple MDM (iOS, macOS, iPadOS)
- **Apple Push Notification Service (APNs)** integration
- **Apple Business Manager (ABM)** integration
  - Automatic enrollment (zero-touch deployment)
  - Multiple ABM token support (2025)
- **Volume Purchasing Program (VPP)** for app distribution
- **Declarative Device Management (DDM)** support (modern Apple MDM)
- Configuration profiles deployment
- Remote lock, wipe, and device commands
- FileVault encryption management
- App installation and management

#### Android MDM
- **Android Management API** integration
- **Google Play Store** app management
- Android work profiles
- App configuration management
- SCEP certificate deployment (2025)
- Moved out of experimental status in 2025

#### Cross-Platform Features
- **GitOps-enabled**: Manage fleet via Git repositories
- **API-first**: REST API for all operations
- **CLI tool**: `fleetctl` command-line interface
- **Webhook events**: Real-time notifications
- **Query engine**: SQL-based osquery integration
- **Policy enforcement**: Compliance and security policies
- **Software deployment**: Automated patch management (Premium)

### Security & Compliance

- **CIS Benchmarks**: Out-of-the-box support for macOS and Windows
- **Threat Detection**: SQL-based queries for security monitoring
- **Vulnerability Scanning**: Identify missing patches and CVEs
- **Compliance Monitoring**: Custom policies and automated checks
- **Audit Logging**: Complete audit trail of all actions
- **RBAC**: Role-based access control

### Integration Ecosystem

**Ready-to-use integrations**:
- Snowflake (data warehouse)
- Splunk (SIEM)
- GitHub Actions (CI/CD)
- Vanta (compliance)
- Elastic (logging/search)
- Jira (ticketing)
- Zendesk (support)

---

## Architecture

### Infrastructure Dependencies

Fleet requires three components:

1. **MySQL Database**
   - Stores device inventory, configuration, policies
   - Supports standard MySQL or compatible variants

2. **Redis-compatible Cache**
   - Redis or Valkey (open-source Redis fork)
   - Used for real-time data and session management

3. **TLS Certificate**
   - Required for secure HTTPS communication
   - Can use Let's Encrypt or commercial certs

### Optional Components

4. **S3-compatible Storage**
   - For software installers and large files
   - AWS S3, MinIO, or compatible services

5. **Logging/SIEM Integration**
   - For centralized log management
   - Splunk, Elastic, or similar

### Core Technology Stack

- **Backend**: Go (Golang)
- **Frontend**: TypeScript/React
- **Database**: MySQL
- **Cache**: Redis/Valkey
- **Agent**: osquery (C++)
- **Apple MDM**: Built on nanoMDM

---

## Deployment Options

### 1. Docker Compose (Recommended for Testing)

**Fastest path to self-hosted Fleet**:
```bash
git clone https://github.com/fleetdm/fleet.git
cd fleet && docker compose up
```

**Access**: http://localhost:8080

**Pros**:
- ✅ Quick setup (5-10 minutes)
- ✅ Single command deployment
- ✅ Good for development/testing
- ✅ Includes all dependencies

**Cons**:
- ❌ Not production-ready by default
- ❌ Requires manual TLS configuration
- ❌ Limited scalability

### 2. Kubernetes (Recommended for Production)

**Deployment methods**:
- Helm chart (official)
- Terraform modules

**Requirements**:
- Helm v3 or Terraform v1.10.2+
- Kubernetes cluster (self-hosted, k3s, managed cloud)

**Pros**:
- ✅ Production-ready
- ✅ Auto-scaling support
- ✅ High availability
- ✅ Easy updates/rollbacks

**Cons**:
- ❌ More complex setup
- ❌ Requires K8s knowledge
- ❌ Higher resource requirements

### 3. Cloud Providers

**Supported platforms**:
- AWS (ECS, EKS, EC2)
- Google Cloud Platform (GKE, Compute Engine)
- Microsoft Azure (AKS, VMs)
- DigitalOcean (Kubernetes, Droplets)

**Pros**:
- ✅ Managed infrastructure
- ✅ Easy scaling
- ✅ Built-in backups

**Cons**:
- ❌ Cloud costs
- ❌ Vendor lock-in

### 4. On-Premises / Air-Gapped

Fleet supports:
- Bare metal servers
- Private data centers
- Air-gapped networks (no internet)
- Proxmox virtualization

**Pros**:
- ✅ Complete control
- ✅ Data sovereignty
- ✅ No external dependencies

**Cons**:
- ❌ Self-managed infrastructure
- ❌ Manual updates required

### 5. Fleet Cloud (Managed Hosting)

**Official managed service**: Available from Fleet team

**Pros**:
- ✅ Zero infrastructure management
- ✅ Professional support
- ✅ Automatic updates
- ✅ Enterprise SLAs

**Cons**:
- ❌ Costs $7/host/month (Premium tier)
- ❌ Data hosted by third party

---

## Licensing & Pricing

### Open Source (Free - MIT License)

**What's Included**:
- ✅ Core MDM functionality
- ✅ Apple MDM (iOS, macOS, iPadOS)
- ✅ Android MDM
- ✅ Windows, Linux, ChromeOS management
- ✅ osquery management
- ✅ GitOps/Infrastructure-as-code
- ✅ REST API
- ✅ CLI tool (fleetctl)
- ✅ Web UI
- ✅ Basic integrations
- ✅ Community support

**Source Code**:
- GitHub: github.com/fleetdm/fleet
- Available for self-hosting
- Can be modified and forked

**Commitment**:
- Free features will always be free
- Won't move free features to paid tiers
- Majority of new capabilities benefit all users
- No delayed releases (free & paid ship simultaneously)

### Fleet Premium ($7/host/month)

**Additional Features**:
- ✅ Enterprise support with SLAs
- ✅ Managed cloud hosting option
- ✅ Automated software deployment
- ✅ Advanced patch management
- ✅ Premium integrations
- ✅ Priority feature requests
- ✅ Professional services

**License**:
- Separate license (code in /ee directory)
- Requires license key
- Source code still visible on GitHub

### Open Core Model

**Structure**:
```
fleet/
├─ Core Features (MIT License)         ← Free forever
│  ├─ MDM for all platforms
│  ├─ osquery management
│  ├─ API, CLI, Web UI
│  └─ GitOps workflows
│
└─ Premium Features (/ee directory)    ← Paid tier
   ├─ Advanced software deployment
   ├─ Enhanced support
   └─ Additional integrations
```

**Philosophy**:
- Core is fully functional and production-ready
- Premium adds enterprise conveniences
- No "crippled" open-source version
- Transparent about what's free vs paid

---

## Setup Requirements for Blocker App Testing

### For iOS/iPadOS MDM Testing

#### Prerequisites
1. **Apple Developer Account** ($99/year)
   - Required for APNs certificate
   - Required for app signing

2. **Apple Push Notification Service (APNs) Certificate**
   - Generated via Apple Developer Portal
   - Must be renewed annually
   - Requires CSR from Fleet server

3. **Apple Business Manager (ABM)** - Optional but recommended
   - Required for automatic enrollment (zero-touch)
   - Requires DUNS number for validation
   - Free to set up

4. **Volume Purchasing Program (VPP)** - Optional
   - For App Store app distribution
   - Free program from Apple

5. **Test Devices**
   - iPhone or iPad you can wipe
   - Must be supervised for NEFilterDataProvider testing

#### Setup Steps
```
1. Deploy Fleet server (Docker or cloud)
2. Request APNs certificate from Apple
3. Upload APNs certificate to Fleet
4. (Optional) Connect Apple Business Manager
5. Wipe test iPhone/iPad
6. Supervise via Apple Configurator
7. Enroll device in Fleet
8. Deploy blocking app with supervision-required features
9. Test NEFilterDataProvider or NEDNSProxyProvider
```

### For Android MDM Testing

#### Prerequisites
1. **Google Play Console** - For app distribution
2. **Android Management API** - Automatically enabled by Fleet
3. **Test Device** - Android phone/tablet

#### Setup Steps
```
1. Deploy Fleet server
2. Enable Android MDM in Fleet settings
3. Enroll Android test device
4. Deploy blocking app
5. Test VpnService implementation
6. Test MDM-controlled configurations
```

### Infrastructure Requirements

**Minimal Setup (Development/Testing)**:
```
Server:
- 2 CPU cores
- 4GB RAM
- 50GB disk
- Docker installed

Database:
- MySQL 5.7+ or 8.0+
- Or use Docker Compose (includes MySQL)

Cache:
- Redis 6+ or Valkey
- Or use Docker Compose (includes Redis)

Network:
- Public IP or ngrok for Apple APNs callback
- HTTPS/TLS certificate (Let's Encrypt works)
```

**Production Setup**:
```
Server:
- 4-8+ CPU cores (scales with device count)
- 8-16GB+ RAM
- 100GB+ SSD
- Kubernetes cluster recommended

Database:
- Managed MySQL (AWS RDS, GCP CloudSQL, etc.)
- Automated backups
- Replication for HA

Cache:
- Managed Redis/Valkey
- High availability configuration

Storage:
- S3-compatible storage for installers
- CDN for software distribution

Monitoring:
- Prometheus/Grafana
- Log aggregation (Splunk, Elastic)
```

---

## Limitations & Drawbacks

### 1. Mobile MDM Features Lag Behind Specialists

**Issue**: iOS/Android features may not match dedicated platforms like Jamf or Intune

**Impact**:
- Some advanced Apple MDM features may be missing
- Newer iOS/Android features take time to implement
- Less polished than 20+ year old commercial MDMs

**Severity**: Medium - Core features work, but edge cases may be unsupported

**Workaround**: Check feature list before committing; most common use cases covered

### 2. Learning Curve for GitOps

**Issue**: Configuration-as-code requires different mindset than GUI-only tools

**Impact**:
- Teams accustomed to point-and-click may struggle
- Requires learning YAML/JSON configuration
- Git workflows may be unfamiliar

**Severity**: Medium - Steeper initial learning curve

**Workaround**: Fleet has GUI for most tasks; GitOps is optional but powerful

### 3. Setup Complexity

**Issue**: Not the easiest tool to set up initially

**Impact**:
- Requires understanding of infrastructure (MySQL, Redis, TLS)
- APNs setup has multiple steps
- Not "one-click" deployment

**Severity**: Medium - Initial hurdle, but well-documented

**Workaround**: Use Docker Compose for quick start; extensive guides available

### 4. Documentation Gaps

**Issue**: Documentation could be more comprehensive in some areas

**Impact**:
- Some features lack detailed examples
- Community knowledge required for edge cases
- Mobile MDM docs newer/less mature

**Severity**: Low-Medium - Generally good docs, but not perfect

**Workaround**: Active community in Slack/GitHub; responsive maintainers

### 5. MySQL Compatibility Issues

**Issue**: Compatibility problems with some MySQL variants

**Impact**:
- MariaDB may have issues
- Aurora MySQL requires specific versions
- Future releases may not support all variants

**Severity**: Low - Standard MySQL works fine

**Workaround**: Use official MySQL 8.0 or managed MySQL services

### 6. Not a Complete RMM/PSA

**Issue**: Fleet won't replace Remote Monitoring & Management or PSA tools

**Impact**:
- No built-in ticketing system
- Limited helpdesk features
- Focused on MDM/security, not service delivery

**Severity**: Low - Fleet is MDM-focused, not an all-in-one tool

**Workaround**: Integrate with existing RMM/PSA via API

### 7. Enterprise Support in Free Tier

**Issue**: Community-driven support model in free tier

**Impact**:
- No guaranteed response times
- No 24/7 vendor support
- Rely on community/GitHub issues

**Severity**: Medium for enterprise, Low for testing

**Workaround**: Pay for Premium ($7/host/month) if enterprise support needed

### 8. Premium Features Behind Paywall

**Issue**: Some features require Premium license

**Impact**:
- Automated software deployment is Premium
- Enhanced support requires payment
- Some integrations are Premium

**Severity**: Low - Core MDM is fully functional

**Workaround**: Free tier is production-ready; Premium adds conveniences

---

## Strengths & Advantages

### 1. True Cross-Platform Support

**Benefit**: Single platform manages iOS, Android, macOS, Windows, Linux, ChromeOS

**Why It Matters**:
- Unified management for blocker app across all platforms
- Single API/interface for all device types
- Consistent policy enforcement

**Competitors**: Most MDMs are platform-specific (Jamf = Apple, Intune = Microsoft-focused)

### 2. Open Source Core

**Benefit**: Full source code available, MIT licensed

**Why It Matters**:
- Can self-host without vendor lock-in
- No per-device costs in free tier
- Transparent security (can audit code)
- Community contributions

**Competitors**: Most MDMs are closed-source SaaS

### 3. GitOps & Infrastructure-as-Code

**Benefit**: Manage configurations via Git repositories

**Why It Matters**:
- Version control for all settings
- Automated deployments
- Easy rollback
- Team collaboration via pull requests

**Competitors**: Unique to Fleet in MDM space

### 4. Built on osquery

**Benefit**: SQL-based device querying and monitoring

**Why It Matters**:
- Powerful security monitoring
- Custom compliance checks
- Real-time threat detection
- Granular device insights

**Competitors**: Unique integration; osquery widely respected

### 5. Active Development & Community

**Benefit**: Daily commits, responsive maintainers, growing community

**Why It Matters**:
- Bugs get fixed quickly
- New features added regularly
- iOS/Android support actively improving
- Long-term viability

**Competitors**: Many open-source MDMs are abandoned or slow

### 6. Production-Ready Scale

**Benefit**: Proven at 400,000+ devices

**Why It Matters**:
- Won't outgrow the platform
- Performance tested at scale
- Used by major companies (Netflix, Stripe, Uber)

**Competitors**: Most open-source MDMs don't scale

### 7. Flexible Deployment

**Benefit**: Run anywhere - cloud, on-prem, air-gapped, containers

**Why It Matters**:
- Start with Docker Compose
- Migrate to Kubernetes for production
- No cloud vendor lock-in
- Works in restricted environments

**Competitors**: Many MDMs are SaaS-only

### 8. No Feature-Gating in Open Source

**Benefit**: Core MDM fully functional, not crippled trial version

**Why It Matters**:
- Can use in production without paying
- All platforms supported in free tier
- Premium adds conveniences, not core features

**Competitors**: Many "open-source" MDMs have severely limited free tiers

---

## Use Cases for Blocker App Project

### 1. Testing Supervised iOS Features

**Scenario**: Develop and test NEFilterDataProvider or NEDNSProxyProvider

**How Fleet Helps**:
```
1. Deploy Fleet server locally (Docker Compose)
2. Get APNs certificate from Apple
3. Supervise test iPhone via Apple Configurator
4. Enroll in Fleet MDM
5. Deploy blocker app with supervision-required features
6. Test content filtering APIs
7. Verify blocking functionality
```

**Cost**: $0 (free tier) + $99/year Apple Developer account

**Alternative Cost**: Jamf Pro starts at ~$8/device/month (~$96/year per device)

### 2. Prototyping Enterprise Version

**Scenario**: Build enterprise version of blocker app for schools/businesses

**How Fleet Helps**:
- Test deployment via MDM profiles
- Configure app remotely
- Deploy to multiple supervised devices
- Demonstrate to enterprise customers

**Value**: Can demo enterprise capabilities without expensive commercial MDM

### 3. Cross-Platform Testing

**Scenario**: Ensure blocker app works consistently across iOS, Android, desktop

**How Fleet Helps**:
- Single platform to manage all test devices
- Consistent deployment process
- Unified monitoring and logging
- Test cross-platform feature parity

**Value**: Streamlined testing workflow

### 4. Learning MDM Concepts

**Scenario**: Understand how MDM works before enterprise sales

**How Fleet Helps**:
- Full MDM implementation to study
- Open source code to examine
- Documentation and community knowledge
- Hands-on experience with real MDM

**Value**: Better understanding of enterprise customer needs

### 5. Small-Scale Enterprise Deployment

**Scenario**: Deploy blocker app to small business or school (< 100 devices)

**How Fleet Helps**:
- Free tier supports unlimited devices
- Self-hosted = no per-device costs
- Can manage with minimal resources
- Scales up if needed

**Value**: $0 vs $800+/month for commercial MDM (100 devices)

---

## Alternatives Comparison

| MDM Solution | iOS | Android | Desktop | Open Source | Active | Production-Ready |
|--------------|-----|---------|---------|-------------|--------|------------------|
| **Fleet** | ✅ | ✅ | ✅ | ✅ MIT (core) | ✅ Very | ✅ Yes |
| NanoMDM | ✅ | ❌ | ✅ (macOS) | ✅ Apache | ✅ Yes | ✅ Yes |
| Commandment | ✅ | ❌ | ✅ (macOS) | ✅ MIT | ⚠️ Slow | ⚠️ Limited |
| MicroMDM | ✅ | ❌ | ✅ (macOS) | ✅ MIT | ❌ Maintenance | ⚠️ Frozen |
| Headwind MDM | ❌ | ✅ | ❌ | ✅ | ✅ Yes | ✅ Yes |
| myMDM | ❓ | ❓ | ❓ | ❓ Claimed | ❓ | ❌ No code |
| **Jamf Pro** | ✅ | ❌ | ✅ (macOS) | ❌ | ✅ | ✅ Yes |
| **Microsoft Intune** | ✅ | ✅ | ✅ | ❌ | ✅ | ✅ Yes |

**Fleet is the only open-source, cross-platform, production-ready MDM** covering all platforms.

---

## Getting Started Guide

### Quick Start (15 minutes)

#### 1. Deploy Fleet with Docker Compose

```bash
# Clone repository
git clone https://github.com/fleetdm/fleet.git
cd fleet

# Start Fleet
docker compose up

# Wait for services to start (2-3 minutes)
# Access Fleet at: http://localhost:8080
```

#### 2. Initial Setup

1. Open browser to http://localhost:8080
2. Create admin account
3. Complete onboarding wizard
4. Fleet is ready!

#### 3. Enable Apple MDM (For iOS Testing)

1. In Fleet UI: Settings → Integrations → MDM
2. Click "Get APNs certificate"
3. Download CSR file
4. Go to Apple Push Certificates Portal: identity.apple.com/pushcert
5. Upload CSR, download certificate
6. Upload certificate to Fleet
7. Apple MDM enabled!

#### 4. Enroll Test Device

**Option A: Manual Enrollment (Quick)**
1. In Fleet UI: Devices → Add device
2. Download enrollment profile
3. Email to test device
4. Install profile on device
5. Device appears in Fleet!

**Option B: Supervised Enrollment (For Testing NEFilterDataProvider)**
1. Wipe test iPhone
2. Use Apple Configurator on Mac
3. Supervise device during setup
4. Enroll in Fleet via configuration
5. Device is supervised + enrolled!

### Next Steps

1. **Deploy Test App**
   - Upload blocker app to Fleet
   - Deploy to test device
   - Verify installation

2. **Configure Policies**
   - Create device compliance policies
   - Test app behavior under different configurations
   - Monitor via osquery

3. **Test Blocking Features**
   - Enable supervision-required APIs
   - Test NEFilterDataProvider/NEDNSProxyProvider
   - Verify domain/app blocking

4. **Iterate & Refine**
   - Adjust configurations
   - Test edge cases
   - Document findings

---

## Resources

### Official Documentation
- **Main Docs**: https://fleetdm.com/docs
- **Guides**: https://fleetdm.com/guides
- **API Reference**: https://fleetdm.com/docs/rest-api

### Code & Development
- **GitHub**: https://github.com/fleetdm/fleet
- **Docker Image**: hub.docker.com/r/fleetdm/fleet
- **Contributing**: https://fleetdm.com/docs/contributing

### Community & Support
- **Slack**: Join via fleetdm.com
- **GitHub Issues**: github.com/fleetdm/fleet/issues
- **Community Forum**: GitHub Discussions

### Specific Guides
- **Apple MDM Setup**: https://fleetdm.com/guides/apple-mdm-setup
- **Deploy with Docker**: https://fleetdm.com/guides/deploy-fleet-on-docker-compose
- **Deploy on Kubernetes**: https://fleetdm.com/guides/deploy-fleet-on-kubernetes
- **Install App Store Apps**: https://fleetdm.com/guides/install-app-store-apps

### Learning Resources
- **What is Apple MDM**: https://fleetdm.com/articles/what-is-apple-mdm
- **osquery Guide**: https://fleetdm.com/guides/osquery-a-tool-to-easily-ask-questions-about-operating-systems
- **Why Fleet**: https://fleetdm.com/docs/get-started/why-fleet
- **FAQ**: https://fleetdm.com/docs/get-started/faq

---

## Recommendations for Blocker App Project

### Short-Term (Development & Testing)

✅ **Use Fleet for:**
1. Testing iOS supervision features (NEFilterDataProvider, NEDNSProxyProvider)
2. Prototyping Android MDM deployment
3. Learning MDM concepts hands-on
4. Cross-platform testing environment
5. Demonstrating enterprise capabilities

**Setup**: Docker Compose on local machine or cloud VM

**Cost**: $0 (free tier) + $99/year Apple Developer account

**Timeline**: 1-2 hours initial setup, ongoing testing as needed

### Mid-Term (Enterprise Prototype)

✅ **Use Fleet for:**
1. Building enterprise version demo
2. Testing with small group of early customers
3. Validating supervised deployment workflows
4. Customer demonstrations

**Setup**: Kubernetes on cloud provider (GCP, AWS, Azure)

**Cost**: $10-50/month cloud costs (small deployment)

**Timeline**: 1-2 weeks for production-grade setup

### Long-Term (Production Consideration)

**Consumer App (App Store)**:
- ❌ Don't use MDM - Use Screen Time + VPN approach
- Consumers don't have supervised devices
- Fleet not needed for consumer distribution

**Enterprise App (Business/Schools)**:
- ✅ Consider Fleet for customers who:
  - Want self-hosted MDM
  - Have budget constraints
  - Value open source

- ⚠️ Consider Commercial MDM for customers who:
  - Need 24/7 enterprise support
  - Want fully managed solution
  - Have large budgets (1000+ devices)

**Hybrid Approach**:
```
Consumer Product:
└─ Distribute via App Stores (no MDM)

Enterprise Add-On:
└─ Offer Fleet deployment guide for self-hosted
└─ Offer Jamf/Intune integration for managed environments
└─ Charge for enterprise support/configuration services
```

---

## Final Verdict

### For Blocker App Project: ✅ **Strongly Recommended**

**Why Fleet is the Best Choice**:

1. ✅ **Only viable option** - No other open-source MDM covers all platforms
2. ✅ **Production-ready** - Used at massive scale by major companies
3. ✅ **Free for testing** - $0 cost for development/testing phase
4. ✅ **Well-documented** - Good guides for Apple MDM setup
5. ✅ **Active development** - iOS/Android support actively improving
6. ✅ **Future-proof** - Can grow with the project if needed

**What You Get**:
- Test supervision-required iOS features (NEFilterDataProvider)
- Test Android MDM deployment (VpnService)
- Prototype enterprise version
- Learn MDM deeply
- Demonstrate to enterprise customers
- Optional path to production for enterprise tier

**What You Don't Get**:
- Perfect parity with Jamf/Intune (mobile features lag)
- 24/7 enterprise support (in free tier)
- Zero learning curve (GitOps takes time)

**Trade-Off**: Worth it for the cost savings and flexibility

### Alternatives to Consider

**If Fleet doesn't meet needs**:
1. **Jamf Now** - Best Apple-only MDM, but $4-8/device/month
2. **Microsoft Intune** - Best for Microsoft-centric orgs, part of M365
3. **Google Workspace** - Best for ChromeOS + Android, free tier available
4. **NanoMDM** - If only need Apple platforms, simpler than Fleet

**But none match Fleet's cross-platform + open-source combination.**

---

## Conclusion

Fleet is the **best available open-source MDM solution** for the blocker app project. It's the only option that:
- Supports all required platforms (iOS, Android, macOS, Windows, Linux)
- Is production-ready and actively maintained
- Has verifiable source code (unlike myMDM)
- Can be self-hosted at zero cost
- Scales from testing to enterprise deployment

**Recommendation**: Deploy Fleet with Docker Compose for initial testing, then evaluate Kubernetes deployment if enterprise customers require it. The free tier is fully functional for development, testing, and small-scale enterprise deployments.

**Next Step**: Follow the Quick Start guide above to get Fleet running and begin testing supervised iOS features within 15-30 minutes.
