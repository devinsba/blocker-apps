import BlockerKit
import NetworkExtension
import SwiftUI

/// Installs/enables the two domain-blocking extensions and picks between them.
///
/// - Content filter (BlockerFilter, NEFilterDataProvider): only runs on
///   supervised devices (or with an MDM-installed filter profile). Full flow
///   inspection with SNI peeking, and it leaves the VPN slot free.
/// - DNS tunnel (BlockerTunnel, NEPacketTunnelProvider): works on consumer
///   devices but occupies the single VPN slot.
///
/// The filter is optional. ``ContentFilterSupport`` decides from the
/// available signals whether it can run; when it can, it is the preferred
/// method and replaces the tunnel, otherwise the tunnel is used.
@MainActor
final class NetworkProtection: ObservableObject {
    static let tunnelBundleID = "dev.blocker.ios.tunnel"
    /// `UserDefaults` key for the last filter-save attempt (a `ContentFilterSupport.ProbeRecord`).
    static let probeDefaultsKey = "contentFilterProbe"

    @Published private(set) var filterEnabled = false
    @Published private(set) var tunnelEnabled = false
    @Published private(set) var filterSupport: ContentFilterSupport = .unknown
    @Published private(set) var busy = false
    @Published var lastError: String?

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    var method: WebsiteProtectionMethod { .preferred(for: filterSupport) }

    /// Whether websites are blocked by the preferred method right now.
    var websitesBlocked: Bool {
        switch method {
        case .contentFilter: return filterEnabled
        case .dnsTunnel: return tunnelEnabled
        }
    }

    private var osVersion: String {
        let v = ProcessInfo.processInfo.operatingSystemVersion
        return "\(v.majorVersion).\(v.minorVersion).\(v.patchVersion)"
    }

    private var lastProbe: ContentFilterSupport.ProbeRecord? {
        get {
            defaults.data(forKey: Self.probeDefaultsKey)
                .flatMap { try? JSONDecoder().decode(ContentFilterSupport.ProbeRecord.self, from: $0) }
        }
        set {
            if let newValue, let data = try? JSONEncoder().encode(newValue) {
                defaults.set(data, forKey: Self.probeDefaultsKey)
            } else {
                defaults.removeObject(forKey: Self.probeDefaultsKey)
            }
        }
    }

    private var managedSupervised: Bool? {
        ContentFilterSupport.managedSupervisedFlag(
            from: defaults.dictionary(forKey: ContentFilterSupport.managedConfigDefaultsKey)
        )
    }

    func refresh() async {
        let filter = NEFilterManager.shared()
        var installed = false
        do {
            try await filter.loadFromPreferences()
            installed = filter.providerConfiguration != nil
            filterEnabled = installed && filter.isEnabled
        } catch {
            lastError = "Content filter: \(error.localizedDescription)"
        }
        filterSupport = ContentFilterSupport.evaluate(.init(
            filterConfigurationInstalled: installed,
            managedSupervised: managedSupervised,
            lastProbe: lastProbe,
            osVersion: osVersion
        ))
        do {
            let tunnels = try await NETunnelProviderManager.loadAllFromPreferences()
            tunnelEnabled = tunnels.contains { $0.isEnabled && $0.connection.status == .connected }
        } catch {
            lastError = "DNS tunnel: \(error.localizedDescription)"
        }
    }

    /// Turns website blocking on or off using the preferred method.
    func setWebsiteBlocking(_ on: Bool) async {
        switch method {
        case .contentFilter: await setFilter(enabled: on)
        case .dnsTunnel: await setTunnel(enabled: on)
        }
    }

    /// Tries the content filter on a device whose support is unknown. On a
    /// supervised device this enables it (and retires the tunnel); otherwise
    /// the failure is recorded and the tunnel stays the method.
    func tryContentFilter() async {
        await setFilter(enabled: true)
    }

    func setFilter(enabled: Bool) async {
        busy = true
        defer { busy = false }
        do {
            let manager = NEFilterManager.shared()
            try await manager.loadFromPreferences()
            let config = manager.providerConfiguration ?? NEFilterProviderConfiguration()
            config.filterSockets = true
            // The data provider's sandbox may not read the App Group, so the
            // config text travels with the filter configuration too.
            config.vendorConfiguration = [SharedConfig.vendorConfigKey: SharedConfig.readConfigTOML() ?? ""]
            manager.providerConfiguration = config
            manager.localizedDescription = "Blocker"
            manager.isEnabled = enabled
            try await manager.saveToPreferences()
            filterEnabled = enabled
            lastProbe = .init(succeeded: true, osVersion: osVersion)
            lastError = nil
            // The filter covers what the tunnel did without taking the VPN slot.
            if enabled && tunnelEnabled {
                await applyTunnel(enabled: false)
            }
        } catch {
            let ns = error as NSError
            switch ContentFilterSupport.classifySaveError(domain: ns.domain, code: ns.code) {
            case .unsupported:
                // Not supervised: remember it for this OS version and fall back.
                lastProbe = .init(
                    succeeded: false,
                    osVersion: osVersion,
                    message: "The content filter needs a supervised device. Using DNS filtering instead."
                )
                lastError = nil
            case .transient:
                lastError = "Content filter: \(error.localizedDescription)"
            }
        }
        await refresh()
    }

    func setTunnel(enabled: Bool) async {
        busy = true
        defer { busy = false }
        await applyTunnel(enabled: enabled)
    }

    private func applyTunnel(enabled: Bool) async {
        do {
            let existing = try await NETunnelProviderManager.loadAllFromPreferences()
            let manager = existing.first ?? NETunnelProviderManager()
            let proto = (manager.protocolConfiguration as? NETunnelProviderProtocol) ?? NETunnelProviderProtocol()
            proto.providerBundleIdentifier = Self.tunnelBundleID
            proto.serverAddress = "Blocker (on device)"
            manager.protocolConfiguration = proto
            manager.localizedDescription = "Blocker DNS"
            manager.isEnabled = enabled
            try await manager.saveToPreferences()
            if enabled {
                try await manager.loadFromPreferences()
                try manager.connection.startVPNTunnel()
            } else {
                manager.connection.stopVPNTunnel()
            }
            tunnelEnabled = enabled
        } catch {
            lastError = "DNS tunnel: \(error.localizedDescription)"
        }
    }
}

struct ProtectionView: View {
    @EnvironmentObject private var screenTime: ScreenTimeAuthorization
    @StateObject private var network = NetworkProtection()

    var body: some View {
        Form {
            Section {
                LabeledContent("Status", value: "\(screenTime.status)")
                Button("Allow Screen Time Access") {
                    Task { await screenTime.requestAuthorization() }
                }
                if let error = screenTime.lastError {
                    Text(error).font(.caption).foregroundStyle(.secondary)
                }
            } header: {
                Text("Apps (Screen Time)")
            } footer: {
                Text("Blocks and shields apps you pick. Not implemented yet.")
            }

            Section {
                Toggle("Block websites", isOn: Binding(
                    get: { network.websitesBlocked },
                    set: { on in Task { await network.setWebsiteBlocking(on) } }
                ))
                .disabled(network.busy)
                LabeledContent("Method", value: methodLabel)
                if case .unknown = network.filterSupport {
                    Button("Use Content Filter (supervised devices)") {
                        Task { await network.tryContentFilter() }
                    }
                    .disabled(network.busy)
                }
                if network.method == .contentFilter && network.tunnelEnabled {
                    Button("Turn Off DNS Tunnel (not needed)") {
                        Task { await network.setTunnel(enabled: false) }
                    }
                }
                if let error = network.lastError {
                    Text(error).font(.caption).foregroundStyle(.red)
                }
            } header: {
                Text("Websites")
            } footer: {
                Text(websitesFooter)
            }

            Section("About") {
                LabeledContent("Engine", value: BlockerEngine.version)
            }
        }
        .navigationTitle("Protection")
        .task { await network.refresh() }
        // MDMs can push managed app configuration (the `supervised` flag) at any time.
        .onReceive(NotificationCenter.default.publisher(for: UserDefaults.didChangeNotification)) { _ in
            Task { await network.refresh() }
        }
    }

    private var methodLabel: String {
        switch network.method {
        case .contentFilter: return "Content filter"
        case .dnsTunnel: return "DNS filtering (VPN)"
        }
    }

    private var websitesFooter: String {
        switch network.filterSupport {
        case .available(.configurationInstalled):
            return "This device has a content filter configuration (supervised or MDM-managed), so websites are blocked by the content filter. Your VPN slot stays free."
        case .available(.managedConfiguration):
            return "Your device management reports this device is supervised, so websites are blocked by the content filter. Your VPN slot stays free."
        case .available(.probeSucceeded):
            return "This device supports the content filter, so websites are blocked by it. Your VPN slot stays free."
        case .unknown:
            return "Websites are blocked with on-device DNS filtering, which uses the device's single VPN slot. On a supervised (MDM-managed) device the content filter can be used instead."
        case .unavailable(let reason):
            return "\(reason) DNS filtering uses the device's single VPN slot."
        }
    }
}
