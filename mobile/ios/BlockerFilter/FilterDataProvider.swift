import BlockerKit
import Foundation
import NetworkExtension
import os

/// Content filter for supervised devices: decides every new socket flow with
/// blocker-core, peeking at the first outbound bytes (TLS SNI / HTTP Host)
/// when the OS doesn't know the hostname.
///
/// The data provider runs in a strict sandbox (no network, no disk writes),
/// so config arrives through the filter configuration's vendorConfiguration,
/// with the App Group's config.toml as a read-only fallback.
final class FilterDataProvider: NEFilterDataProvider {
    private let log = Logger(subsystem: "dev.blocker.ios.filter", category: "filter")
    private let lock = NSLock()
    private var engine: BlockerEngine?

    /// Max bytes requested for SNI peeking (a ClientHello rarely exceeds this).
    private static let maxPeek = 16 * 1024

    override func startFilter(completionHandler: @escaping (Error?) -> Void) {
        reloadEngine()
        completionHandler(nil)
    }

    override func stopFilter(with reason: NEProviderStopReason, completionHandler: @escaping () -> Void) {
        completionHandler()
    }

    // MARK: - Flows

    override func handleNewFlow(_ flow: NEFilterFlow) -> NEFilterNewFlowVerdict {
        guard let engine = currentEngine(), let socket = flow as? NEFilterSocketFlow else {
            return .allow()
        }
        let info = Self.flowInfo(socket)
        switch engine.decide(flow: info) {
        case .allow:
            return .allow()
        case .block(let rule, let matched):
            log.info("drop \(info.hostname ?? info.remoteIP ?? "?", privacy: .public) rule=\(rule, privacy: .public) matched=\(matched, privacy: .public)")
            return .drop()
        case .peek:
            // Look at outbound bytes only; the verdict comes from handleOutboundData.
            return .filterDataVerdict(
                withFilterInbound: false, peekInboundBytes: 0,
                filterOutbound: true, peekOutboundBytes: 1024)
        }
    }

    override func handleOutboundData(
        from flow: NEFilterFlow, readBytesStartOffset offset: Int, readBytes: Data
    ) -> NEFilterDataVerdict {
        guard let engine = currentEngine(), let socket = flow as? NEFilterSocketFlow else {
            return .allow()
        }
        let peeked: PeekedHost
        switch BlockerEngine.peekHost(readBytes) {
        case .needMore(let total) where offset == 0 && total <= Self.maxPeek:
            // Ask for the rest of the ClientHello before deciding.
            return NEFilterDataVerdict(passBytes: 0, peekBytes: total)
        case .needMore:
            peeked = .none
        case .host(let host):
            peeked = .found(host)
        case .noHost:
            peeked = .none
        }
        switch engine.decide(flow: Self.flowInfo(socket), peekedHost: peeked) {
        case .block(let rule, let matched):
            log.info("drop peeked flow rule=\(rule, privacy: .public) matched=\(matched, privacy: .public)")
            return .drop()
        case .allow, .peek:
            return .allow()
        }
    }

    override func handleInboundData(
        from flow: NEFilterFlow, readBytesStartOffset offset: Int, readBytes: Data
    ) -> NEFilterDataVerdict {
        .allow()
    }

    // MARK: - Engine

    private func currentEngine() -> BlockerEngine? {
        lock.lock()
        defer { lock.unlock() }
        return engine
    }

    /// TODO: reload when the app changes the config. The app re-saves the
    /// NEFilterManager configuration, which updates `filterConfiguration`;
    /// observe it (KVO) or add an NEFilterControlProvider that calls
    /// notifyRulesChanged().
    private func reloadEngine() {
        let next: BlockerEngine?
        do {
            if let toml = filterConfiguration.vendorConfiguration?[SharedConfig.vendorConfigKey] as? String,
               !toml.isEmpty {
                next = try BlockerEngine(configTOML: toml)
            } else {
                next = try SharedConfig.loadEngine()
            }
            for warning in next?.warnings ?? [] {
                log.warning("config: \(warning, privacy: .public)")
            }
        } catch {
            log.error("could not load config: \(error.localizedDescription, privacy: .public)")
            next = nil
        }
        lock.lock()
        engine = next
        lock.unlock()
    }

    private static func flowInfo(_ flow: NEFilterSocketFlow) -> Flow {
        var host = flow.remoteHostname
        var ip: String?
        var port: UInt16 = 0
        if let endpoint = flow.remoteEndpoint as? NWHostEndpoint {
            ip = endpoint.hostname
            port = UInt16(endpoint.port) ?? 0
        }
        if host == ip { host = nil }
        let proto: FlowProtocol
        switch flow.socketProtocol {
        case Int32(IPPROTO_TCP): proto = .tcp
        case Int32(IPPROTO_UDP): proto = .udp
        default: proto = .other
        }
        return Flow(
            hostname: host,
            remoteIP: ip,
            remotePort: port,
            proto: proto,
            appID: flow.sourceAppIdentifier,
            ech: false // ECH isn't visible here; blocker-core's sni module could detect it from the peeked bytes.
        )
    }
}
