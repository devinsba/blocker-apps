import BlockerKit
import Foundation
import NetworkExtension
import os

/// DNS-only "VPN" for consumer devices (the research's recommended consumer
/// path for domain blocking). Only DNS is routed into the tunnel: the system
/// resolver is pointed at a virtual server address inside it, every query is
/// decided with `BlockerEngine.checkHost`, and all other traffic stays off the
/// tunnel.
///
/// SKELETON: queries are parsed and decided, but answering them is TODO
/// (see `handle(_:protocolFamily:)`). Until then, enabling the tunnel breaks
/// DNS, so ship it only once forwarding works.
final class PacketTunnelProvider: NEPacketTunnelProvider {
    private let log = Logger(subsystem: "dev.blocker.ios.tunnel", category: "tunnel")
    private let lock = NSLock()
    private var engine: BlockerEngine?

    /// Address assigned to the tunnel interface and the virtual DNS server
    /// (198.18.0.0/15 is reserved for benchmarking, so it won't collide).
    private static let tunnelAddress = "198.18.0.2"
    private static let dnsAddress = "198.18.0.53"

    override func startTunnel(options: [String: NSObject]?, completionHandler: @escaping (Error?) -> Void) {
        reloadEngine()

        let settings = NEPacketTunnelNetworkSettings(tunnelRemoteAddress: "127.0.0.1")
        let ipv4 = NEIPv4Settings(addresses: [Self.tunnelAddress], subnetMasks: ["255.255.255.255"])
        // Route only the virtual DNS server into the tunnel.
        ipv4.includedRoutes = [NEIPv4Route(destinationAddress: Self.dnsAddress, subnetMask: "255.255.255.255")]
        settings.ipv4Settings = ipv4
        let dns = NEDNSSettings(servers: [Self.dnsAddress])
        dns.matchDomains = [""] // all lookups
        settings.dnsSettings = dns
        settings.mtu = 1500

        setTunnelNetworkSettings(settings) { [weak self] error in
            if let error {
                completionHandler(error)
                return
            }
            self?.readPackets()
            completionHandler(nil)
        }
    }

    override func stopTunnel(with reason: NEProviderStopReason, completionHandler: @escaping () -> Void) {
        completionHandler()
    }

    /// The app sends any message to ask for a config reload after editing rules.
    override func handleAppMessage(_ messageData: Data, completionHandler: ((Data?) -> Void)?) {
        reloadEngine()
        completionHandler?(nil)
    }

    // MARK: - Packets

    private func readPackets() {
        packetFlow.readPackets { [weak self] packets, protocols in
            guard let self else { return }
            for (packet, family) in zip(packets, protocols) {
                self.handle(packet, protocolFamily: family)
            }
            self.readPackets()
        }
    }

    private func handle(_ packet: Data, protocolFamily: NSNumber) {
        guard let query = DNSQuery(ipPacket: packet) else { return }
        let engine = currentEngine()
        let verdict = engine?.checkHost(query.name) ?? .allow
        switch verdict {
        case .block(let rule, let matched):
            log.info("block \(query.name, privacy: .public) rule=\(rule, privacy: .public) matched=\(matched, privacy: .public)")
            // TODO(dns): answer locally. Build a response from `packet`: swap the
            // IP/UDP addresses and ports, set QR=1 and RCODE=3 (NXDOMAIN) or
            // answer A 0.0.0.0 / AAAA ::, recompute the IPv4 header checksum
            // (UDP checksum may be 0 on IPv4), then
            // packetFlow.writePackets([response], withProtocols: [protocolFamily]).
        case .allow, .peek:
            // TODO(dns): forward the DNS message (packet[query.dnsOffset...]) to the
            // upstream resolver with an NWConnection (UDP, keyed by query id), and
            // write the reply back wrapped in IP/UDP headers addressed to the
            // original source. Upstream: the resolver from the network before the
            // tunnel came up, or a fixed DoH/DoT resolver for reliability.
            break
        }
    }

    // MARK: - Engine

    private func currentEngine() -> BlockerEngine? {
        lock.lock()
        defer { lock.unlock() }
        return engine
    }

    private func reloadEngine() {
        let next: BlockerEngine?
        do {
            next = try SharedConfig.loadEngine()
        } catch {
            log.error("could not load config: \(error.localizedDescription, privacy: .public)")
            next = nil
        }
        lock.lock()
        engine = next
        lock.unlock()
    }
}
