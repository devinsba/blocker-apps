import BlockerFFI
import Foundation

/// A compiled set of blocking rules (Rust `blocker_core::Engine`).
///
/// The underlying engine is immutable and thread-safe, so one instance can be
/// shared across threads and network-extension callbacks. Every string and
/// verdict the C library returns is copied into Swift values and freed before
/// a method returns; the engine itself is freed in `deinit`.
public final class BlockerEngine: @unchecked Sendable {
    private let handle: OpaquePointer

    /// Warnings produced while compiling (e.g. a category list that is not downloaded yet).
    public let warnings: [String]

    /// Compiles `config.toml` text. Category lists are not loaded (use ``init(directory:)``).
    public convenience init(configTOML: String) throws {
        let handle = configTOML.withCString { blocker_engine_from_toml($0) }
        guard let handle else { throw Self.lastError(default: "could not compile config") }
        self.init(handle: handle)
    }

    /// Compiles a config directory: `config.toml` plus `lists/<category>.fst`.
    public convenience init(directory: URL) throws {
        let handle = directory.withUnsafeFileSystemRepresentation { path -> OpaquePointer? in
            guard let path else { return nil }
            return blocker_engine_open_dir(path)
        }
        guard let handle else {
            throw Self.lastError(default: "could not open config directory \(directory.path)")
        }
        self.init(handle: handle)
    }

    private init(handle: OpaquePointer) {
        self.handle = handle
        let json = Self.takeString(blocker_engine_warnings_json(handle))
        self.warnings = json.flatMap { try? Self.decode([String].self, from: $0) } ?? []
    }

    deinit {
        blocker_engine_free(handle)
    }

    // MARK: - Queries

    /// Would `host` (a hostname or URL) be blocked at `time`?
    public func checkHost(_ host: String, at time: WeekTime = .now) -> Verdict {
        var raw = host.withCString { blocker_engine_check_host(handle, $0, Self.day(time), Self.minute(time)) }
        return Self.takeVerdict(&raw)
    }

    /// Decides a network flow.
    ///
    /// Call first with `peekedHost: .notPeeked`. If that returns ``Verdict/peek``,
    /// read the flow's first outbound bytes, run ``peekHost(_:)`` and call again
    /// with `.found(host)` or `.none`.
    public func decide(flow: Flow, peekedHost: PeekedHost = .notPeeked, at time: WeekTime = .now) -> Verdict {
        let peeked: Bool
        let peekedName: String?
        switch peekedHost {
        case .notPeeked: (peeked, peekedName) = (false, nil)
        case .found(let h): (peeked, peekedName) = (true, h)
        case .none: (peeked, peekedName) = (true, nil)
        }
        let proto: BlockerProto
        switch flow.proto {
        case .tcp: proto = BLOCKER_PROTO_TCP
        case .udp: proto = BLOCKER_PROTO_UDP
        case .other: proto = BLOCKER_PROTO_OTHER
        }

        var raw = withOptionalCString(flow.hostname) { hostname in
            withOptionalCString(flow.remoteIP) { remoteIP in
                withOptionalCString(flow.appID) { appID in
                    withOptionalCString(peekedName) { peekedName in
                        var cFlow = BlockerFlow(
                            hostname: hostname,
                            remote_ip: remoteIP,
                            remote_port: flow.remotePort,
                            proto: proto,
                            app_id: appID,
                            ech: flow.ech
                        )
                        return blocker_engine_decide(
                            handle, &cFlow, peeked, peekedName, Self.day(time), Self.minute(time))
                    }
                }
            }
        }
        return Self.takeVerdict(&raw)
    }

    /// True when no rule is active at `time` (nothing to filter).
    public func isIdle(at time: WeekTime = .now) -> Bool {
        blocker_engine_is_idle(handle, Self.day(time), Self.minute(time))
    }

    /// Names of the rules active at `time`.
    public func activeRules(at time: WeekTime = .now) -> [String] {
        let json = Self.takeString(blocker_engine_active_rules_json(handle, Self.day(time), Self.minute(time)))
        return json.flatMap { try? Self.decode([String].self, from: $0) } ?? []
    }

    /// Apps blocked by the rules active at `time`.
    public func blockedApps(at time: WeekTime = .now) -> BlockedApps {
        let json = Self.takeString(blocker_engine_blocked_apps_json(handle, Self.day(time), Self.minute(time)))
        return json.flatMap { try? Self.decode(BlockedApps.self, from: $0) } ?? BlockedApps()
    }

    // MARK: - Stateless helpers

    /// blocker-ffi library version.
    public static var version: String {
        String(cString: blocker_version())
    }

    /// Normalizes a user-entered domain or URL ("https://WWW.Example.com/x" ->
    /// "www.example.com"). `nil` if it is not a valid domain.
    public static func normalizeDomain(_ input: String) -> String? {
        takeString(input.withCString { blocker_normalize_domain($0) })
    }

    /// Hostname from the first outbound bytes of a TCP stream (TLS SNI or HTTP Host).
    public static func peekHost(_ data: Data) -> PeekResult {
        var wantMore = 0
        let host: String? = data.withUnsafeBytes { buf in
            guard let base = buf.bindMemory(to: UInt8.self).baseAddress, buf.count > 0 else { return nil }
            return takeString(blocker_peek_host(base, buf.count, &wantMore))
        }
        if let host { return .host(host) }
        if wantMore > 0 { return .needMore(totalBytes: wantMore) }
        return .noHost
    }

    /// Checks that `toml` parses as a blocker config. Throws the parse error otherwise.
    public static func validateConfig(_ toml: String) throws {
        let ok = toml.withCString { blocker_config_validate($0) }
        if !ok { throw lastError(default: "invalid config") }
    }

    // MARK: - FFI plumbing

    private static func day(_ t: WeekTime) -> Int32 { Int32(t.weekday) }
    private static func minute(_ t: WeekTime) -> UInt16 { UInt16(t.minute) }

    /// Copies and frees a library-owned string.
    private static func takeString(_ p: UnsafeMutablePointer<CChar>?) -> String? {
        guard let p else { return nil }
        defer { blocker_string_free(p) }
        return String(cString: p)
    }

    /// Converts and frees a library-owned verdict.
    private static func takeVerdict(_ raw: inout BlockerVerdict) -> Verdict {
        defer { blocker_verdict_free(&raw) }
        switch raw.kind {
        case BLOCKER_VERDICT_BLOCK:
            let rule = raw.rule.map { String(cString: $0) } ?? ""
            let matched = raw.matched.map { String(cString: $0) } ?? ""
            return .block(rule: rule, matched: matched)
        case BLOCKER_VERDICT_PEEK:
            return .peek
        default:
            return .allow
        }
    }

    /// The thread-local error from the last failed call (must be read on the same thread).
    private static func lastError(default fallback: String) -> BlockerError {
        BlockerError(takeString(blocker_last_error()) ?? fallback)
    }

    private static func decode<T: Decodable>(_ type: T.Type, from json: String) throws -> T {
        try JSONDecoder().decode(type, from: Data(json.utf8))
    }
}

/// Calls `body` with a C string for `s`, or NULL when `s` is nil.
private func withOptionalCString<R>(_ s: String?, _ body: (UnsafePointer<CChar>?) throws -> R) rethrows -> R {
    guard let s else { return try body(nil) }
    return try s.withCString { try body($0) }
}
