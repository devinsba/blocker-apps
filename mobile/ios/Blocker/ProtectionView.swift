import BlockerKit
import NetworkExtension
import SwiftUI

/// Installs/enables the two domain-blocking extensions and the Screen Time hook.
///
/// - Content filter (BlockerFilter, NEFilterDataProvider): only runs on
///   supervised devices (or with an MDM-installed profile). Full flow
///   inspection with SNI peeking.
/// - DNS tunnel (BlockerTunnel, NEPacketTunnelProvider): works on consumer
///   devices but occupies the single VPN slot.
@MainActor
final class NetworkProtection: ObservableObject {
    static let tunnelBundleID = "dev.blocker.ios.tunnel"

    @Published private(set) var filterEnabled = false
    @Published private(set) var tunnelEnabled = false
    @Published var lastError: String?

    func refresh() async {
        do {
            let filter = NEFilterManager.shared()
            try await filter.loadFromPreferences()
            filterEnabled = filter.isEnabled
            let tunnels = try await NETunnelProviderManager.loadAllFromPreferences()
            tunnelEnabled = tunnels.contains { $0.isEnabled && $0.connection.status == .connected }
        } catch {
            lastError = error.localizedDescription
        }
    }

    func setFilter(enabled: Bool) async {
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
        } catch {
            // Expected on unsupervised devices: NEFilterErrorDomain "configuration invalid / disabled".
            lastError = "Content filter: \(error.localizedDescription)"
        }
    }

    func setTunnel(enabled: Bool) async {
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
                Toggle("DNS filtering (VPN)", isOn: Binding(
                    get: { network.tunnelEnabled },
                    set: { on in Task { await network.setTunnel(enabled: on) } }
                ))
                Toggle("Content filter (supervised)", isOn: Binding(
                    get: { network.filterEnabled },
                    set: { on in Task { await network.setFilter(enabled: on) } }
                ))
                if let error = network.lastError {
                    Text(error).font(.caption).foregroundStyle(.red)
                }
            } header: {
                Text("Websites")
            } footer: {
                Text("The DNS tunnel uses the device's single VPN slot. The content filter only works on supervised (MDM-managed) devices.")
            }

            Section("About") {
                LabeledContent("Engine", value: BlockerEngine.version)
            }
        }
        .navigationTitle("Protection")
        .task { await network.refresh() }
    }
}
