import Foundation

/// A point in the weekly schedule, in local time.
public struct WeekTime: Hashable, Sendable {
    /// 0 = Monday ... 6 = Sunday.
    public var weekday: Int
    /// Minutes since local midnight, 0..<1440.
    public var minute: Int

    public init(weekday: Int, minute: Int) {
        precondition((0...6).contains(weekday), "weekday must be 0 (Mon) ... 6 (Sun)")
        precondition((0..<1440).contains(minute), "minute must be 0..<1440")
        self.weekday = weekday
        self.minute = minute
    }

    public init(weekday: Int, hour: Int, minute: Int = 0) {
        self.init(weekday: weekday, minute: hour * 60 + minute)
    }

    /// The current local time.
    public static var now: WeekTime { WeekTime(date: Date()) }

    /// `date` in `calendar`'s time zone (the current one by default).
    public init(date: Date, calendar: Calendar = .current) {
        let c = calendar.dateComponents([.weekday, .hour, .minute], from: date)
        // Calendar weekday: 1 = Sunday ... 7 = Saturday.
        let weekday = ((c.weekday ?? 2) + 5) % 7
        self.init(weekday: weekday, minute: (c.hour ?? 0) * 60 + (c.minute ?? 0))
    }
}

/// What to do with a host or flow.
public enum Verdict: Hashable, Sendable {
    case allow
    /// Blocked by the rule named `rule`; `matched` is the domain, bundle id or
    /// reason ("quic", "encrypted client hello") that matched.
    case block(rule: String, matched: String)
    /// The hostname is unknown: inspect the flow's first outbound bytes with
    /// ``BlockerEngine/peekHost(_:)`` and call ``BlockerEngine/decide(flow:peekedHost:at:)`` again.
    case peek

    public var isBlocked: Bool {
        if case .block = self { return true }
        return false
    }
}

public enum FlowProtocol: Hashable, Sendable {
    case tcp, udp, other
}

/// What the OS knows about a new network flow.
public struct Flow: Hashable, Sendable {
    public var hostname: String?
    /// Textual IPv4/IPv6 address.
    public var remoteIP: String?
    public var remotePort: UInt16
    public var proto: FlowProtocol
    /// Bundle id of the app that opened the flow.
    public var appID: String?
    /// The TLS ClientHello used Encrypted Client Hello.
    public var ech: Bool

    public init(
        hostname: String? = nil,
        remoteIP: String? = nil,
        remotePort: UInt16 = 0,
        proto: FlowProtocol = .tcp,
        appID: String? = nil,
        ech: Bool = false
    ) {
        self.hostname = hostname
        self.remoteIP = remoteIP
        self.remotePort = remotePort
        self.proto = proto
        self.appID = appID
        self.ech = ech
    }
}

/// Result of inspecting a flow's first bytes, passed to the second
/// ``BlockerEngine/decide(flow:peekedHost:at:)`` call after a ``Verdict/peek``.
public enum PeekedHost: Hashable, Sendable {
    /// First call: nothing inspected yet.
    case notPeeked
    /// The bytes named this host (TLS SNI or HTTP Host).
    case found(String)
    /// The bytes were inspected and carried no hostname.
    case none
}

/// Result of ``BlockerEngine/peekHost(_:)``.
public enum PeekResult: Hashable, Sendable {
    case host(String)
    /// Not enough bytes yet: retry once `totalBytes` bytes are available.
    case needMore(totalBytes: Int)
    /// No hostname in these bytes (not TLS/HTTP, or no SNI).
    case noHost

    /// As the argument for the follow-up decide call; `nil` while more bytes are wanted.
    public var peekedHost: PeekedHost? {
        switch self {
        case .host(let h): return .found(h)
        case .noHost: return PeekedHost.none
        case .needMore: return nil
        }
    }
}

/// Bundle ids and process names blocked by the active rules.
public struct BlockedApps: Hashable, Sendable, Decodable {
    public var bundleIDs: [String]
    public var processNames: [String]

    public init(bundleIDs: [String] = [], processNames: [String] = []) {
        self.bundleIDs = bundleIDs
        self.processNames = processNames
    }

    enum CodingKeys: String, CodingKey {
        case bundleIDs = "bundle_ids"
        case processNames = "process_names"
    }
}

public struct BlockerError: Error, Hashable, Sendable, CustomStringConvertible {
    public var message: String
    public init(_ message: String) { self.message = message }
    public var description: String { message }
}

extension BlockerError: LocalizedError {
    public var errorDescription: String? { message }
}
