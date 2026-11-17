import Foundation

/// The question of a DNS query carried in a raw IP/UDP packet, as read from
/// `NEPacketTunnelFlow`. Used by the packet tunnel to decide each lookup with
/// ``BlockerEngine/checkHost(_:at:)``.
///
/// Only what the tunnel needs is parsed: IPv4 or IPv6 (no extension headers),
/// UDP, and the first question of a standard query.
public struct DNSQuery: Hashable, Sendable {
    public var name: String
    public var type: UInt16
    public var id: UInt16
    /// Offset of the DNS message inside the IP packet.
    public var dnsOffset: Int

    /// Parses `packet` (a full IP packet). `nil` if it is not a UDP/53 DNS query.
    public init?(ipPacket packet: Data) {
        let b = [UInt8](packet)
        guard let first = b.first else { return nil }
        let udp: Int
        switch first >> 4 {
        case 4:
            let ihl = Int(first & 0x0f) * 4
            guard ihl >= 20, b.count >= ihl + 8, b[9] == 17 else { return nil }
            udp = ihl
        case 6:
            guard b.count >= 48, b[6] == 17 else { return nil }
            udp = 40
        default:
            return nil
        }
        let dstPort = UInt16(b[udp + 2]) << 8 | UInt16(b[udp + 3])
        guard dstPort == 53 else { return nil }
        self.init(dnsMessage: b, offset: udp + 8)
    }

    /// Parses a DNS message starting at `offset` in `b`.
    public init?(dnsMessage b: [UInt8], offset: Int = 0) {
        let o = offset
        guard b.count >= o + 12 else { return nil }
        let id = UInt16(b[o]) << 8 | UInt16(b[o + 1])
        let isResponse = b[o + 2] & 0x80 != 0
        let qdcount = UInt16(b[o + 4]) << 8 | UInt16(b[o + 5])
        guard !isResponse, qdcount >= 1 else { return nil }

        var i = o + 12
        var labels: [String] = []
        while true {
            guard i < b.count else { return nil }
            let len = Int(b[i])
            i += 1
            if len == 0 { break }
            // Compression pointers never appear in a query's first question.
            guard len < 64, i + len <= b.count else { return nil }
            guard let label = String(bytes: b[i..<i + len], encoding: .ascii) else { return nil }
            labels.append(label)
            i += len
        }
        guard i + 4 <= b.count, !labels.isEmpty else { return nil }
        self.name = labels.joined(separator: ".").lowercased()
        self.type = UInt16(b[i]) << 8 | UInt16(b[i + 1])
        self.id = id
        self.dnsOffset = o
    }
}
