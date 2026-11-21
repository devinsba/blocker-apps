import Foundation
import Testing
@testable import BlockerKit

private let config = """
[[rule]]
name = "focus"
domains = ["example.com", "reddit.com"]
bundle_ids = ["com.example.game"]
process_names = ["Game"]

[[rule]]
name = "work-hours"
domains = ["youtube.com"]
schedule = { type = "windows", windows = [{ days = ["mon","tue","wed","thu","fri"], start = "09:00", end = "17:00" }] }
"""

private let mondayNoon = WeekTime(weekday: 0, hour: 12)
private let saturdayNoon = WeekTime(weekday: 5, hour: 12)

@Suite struct EngineTests {
    let engine: BlockerEngine

    init() throws {
        engine = try BlockerEngine(configTOML: config)
    }

    @Test func blocksListedDomainAndSubdomains() {
        #expect(engine.checkHost("www.example.com", at: mondayNoon) == .block(rule: "focus", matched: "example.com"))
        #expect(engine.checkHost("https://Reddit.com/r/swift", at: mondayNoon).isBlocked)
        #expect(engine.checkHost("apple.com", at: mondayNoon) == .allow)
    }

    @Test func respectsSchedule() {
        #expect(engine.checkHost("youtube.com", at: mondayNoon) == .block(rule: "work-hours", matched: "youtube.com"))
        #expect(engine.checkHost("youtube.com", at: saturdayNoon) == .allow)
        #expect(engine.activeRules(at: mondayNoon).sorted() == ["focus", "work-hours"])
        #expect(engine.activeRules(at: saturdayNoon) == ["focus"])
        #expect(!engine.isIdle(at: saturdayNoon))
    }

    @Test func blockedApps() {
        let apps = engine.blockedApps(at: mondayNoon)
        #expect(apps.bundleIDs == ["com.example.game"])
        #expect(apps.processNames == ["Game"])
    }

    @Test func decideFlows() {
        // Known hostname.
        #expect(engine.decide(flow: Flow(hostname: "example.com", remotePort: 443), at: mondayNoon).isBlocked)
        // Bundle id rule.
        #expect(engine.decide(flow: Flow(remotePort: 443, appID: "com.example.game"), at: mondayNoon)
            == .block(rule: "focus", matched: "com.example.game"))
        // Unknown TCP host: peek, then decide with what was found.
        let bare = Flow(remoteIP: "93.184.216.34", remotePort: 443, proto: .tcp)
        #expect(engine.decide(flow: bare, at: mondayNoon) == .peek)
        #expect(engine.decide(flow: bare, peekedHost: .found("www.reddit.com"), at: mondayNoon).isBlocked)
        #expect(engine.decide(flow: bare, peekedHost: .found("apple.com"), at: mondayNoon) == .allow)
        #expect(engine.decide(flow: bare, peekedHost: .none, at: mondayNoon) == .allow)
        // QUIC to an unknown host is dropped while a domain rule is active (block_quic defaults on).
        #expect(engine.decide(flow: Flow(remotePort: 443, proto: .udp), at: mondayNoon)
            == .block(rule: "focus", matched: "quic"))
    }

    @Test func nowDefaultsWork() {
        // `focus` has no schedule, so it is active whatever the current time is.
        #expect(engine.checkHost("example.com").isBlocked)
        #expect(engine.activeRules().contains("focus"))
    }

    @Test func noWarningsForPlainDomains() {
        #expect(engine.warnings.isEmpty)
    }
}

@Suite struct StaticHelperTests {
    @Test func version() {
        #expect(!BlockerEngine.version.isEmpty)
    }

    @Test func normalizeDomain() {
        #expect(BlockerEngine.normalizeDomain("https://WWW.Example.com/x?y") == "www.example.com")
        #expect(BlockerEngine.normalizeDomain("*.reddit.com") == "reddit.com")
        #expect(BlockerEngine.normalizeDomain("") == nil)
        #expect(BlockerEngine.normalizeDomain("[::1]") == nil)
    }

    @Test func validateConfig() throws {
        try BlockerEngine.validateConfig(config)
        #expect(throws: BlockerError.self) { try BlockerEngine.validateConfig("[[rule]]\nname = 3\n") }
        do {
            try BlockerEngine.validateConfig("not toml ===")
            Issue.record("expected an error")
        } catch let error as BlockerError {
            #expect(error.message.contains("parsing config"))
        }
    }

    @Test func invalidConfigThrows() {
        #expect(throws: BlockerError.self) { try BlockerEngine(configTOML: "[[rule]]\nname = [1]\n") }
    }

    @Test func openDirectory() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("blockerkit-\(UUID())")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try config.write(to: dir.appendingPathComponent("config.toml"), atomically: true, encoding: .utf8)
        let engine = try BlockerEngine(directory: dir)
        #expect(engine.checkHost("example.com", at: mondayNoon).isBlocked)
    }

    @Test func missingCategoryListIsAWarning() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("blockerkit-\(UUID())")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        try "[[rule]]\nname = \"s\"\ncategories = [\"social\"]\n"
            .write(to: dir.appendingPathComponent("config.toml"), atomically: true, encoding: .utf8)
        let engine = try BlockerEngine(directory: dir)
        #expect(!engine.warnings.isEmpty)
    }

    @Test func peekHTTPHost() {
        let req = Data("GET / HTTP/1.1\r\nHost: www.Example.com\r\nAccept: */*\r\n\r\n".utf8)
        #expect(BlockerEngine.peekHost(req) == .host("www.example.com"))
        #expect(BlockerEngine.peekHost(Data()) == .noHost)
    }

    @Test func peekTLSSNI() {
        let hello = clientHello(sni: "www.reddit.com")
        #expect(BlockerEngine.peekHost(hello) == .host("www.reddit.com"))
        #expect(BlockerEngine.peekHost(hello).peekedHost == .found("www.reddit.com"))
        // A truncated ClientHello asks for the whole record.
        if case .needMore(let total) = BlockerEngine.peekHost(hello.prefix(20)) {
            #expect(total == hello.count)
        } else {
            Issue.record("expected needMore for a truncated ClientHello")
        }
    }

    @Test func weekTimeFromDate() {
        var cal = Calendar(identifier: .gregorian)
        cal.timeZone = TimeZone(identifier: "UTC")!
        // 2026-11-16 is a Monday.
        let date = cal.date(from: DateComponents(year: 2026, month: 11, day: 16, hour: 9, minute: 30))!
        #expect(WeekTime(date: date, calendar: cal) == WeekTime(weekday: 0, minute: 570))
        let sunday = cal.date(from: DateComponents(year: 2026, month: 11, day: 22, hour: 23, minute: 59))!
        #expect(WeekTime(date: sunday, calendar: cal) == WeekTime(weekday: 6, minute: 1439))
    }
}

@Suite struct DNSQueryTests {
    @Test func parsesIPv4Query() throws {
        let packet = ipv4UDPDNSQuery(name: "WWW.Example.com", id: 0xBEEF)
        let q = try #require(DNSQuery(ipPacket: packet))
        #expect(q.name == "www.example.com")
        #expect(q.type == 1)
        #expect(q.id == 0xBEEF)
        #expect(q.dnsOffset == 28)
    }

    @Test func rejectsNonDNS() {
        var packet = [UInt8](ipv4UDPDNSQuery(name: "a.com", id: 1))
        packet[23] = 80 // destination port 80
        #expect(DNSQuery(ipPacket: Data(packet)) == nil)
        #expect(DNSQuery(ipPacket: Data([0x45])) == nil)
    }
}

// MARK: - Fixtures

/// Minimal TLS 1.2-framed ClientHello with only a server_name extension.
private func clientHello(sni: String) -> Data {
    let name = Array(sni.utf8)
    func u16(_ v: Int) -> [UInt8] { [UInt8(v >> 8 & 0xff), UInt8(v & 0xff)] }
    func u24(_ v: Int) -> [UInt8] { [UInt8(v >> 16 & 0xff)] + u16(v) }

    let serverNameList = [0x00] + u16(name.count) + name
    let sniExt = u16(0x0000) + u16(serverNameList.count + 2) + u16(serverNameList.count) + serverNameList
    var body: [UInt8] = [0x03, 0x03] + [UInt8](repeating: 0xAB, count: 32)
    body += [0x00] // session id
    body += u16(2) + [0x13, 0x01] // cipher suites
    body += [0x01, 0x00] // compression methods
    body += u16(sniExt.count) + sniExt
    let handshake = [0x01] + u24(body.count) + body
    let record = [0x16, 0x03, 0x01] + u16(handshake.count) + handshake
    return Data(record)
}

private func ipv4UDPDNSQuery(name: String, id: UInt16) -> Data {
    var dns: [UInt8] = [UInt8(id >> 8), UInt8(id & 0xff), 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]
    for label in name.split(separator: ".") {
        dns.append(UInt8(label.utf8.count))
        dns += Array(label.utf8)
    }
    dns += [0x00, 0x00, 0x01, 0x00, 0x01] // root, type A, class IN
    let udpLen = 8 + dns.count
    let udp: [UInt8] = [0xC0, 0x00, 0x00, 53, UInt8(udpLen >> 8), UInt8(udpLen & 0xff), 0, 0]
    let total = 20 + udpLen
    let ip: [UInt8] = [0x45, 0, UInt8(total >> 8), UInt8(total & 0xff), 0, 0, 0, 0, 64, 17, 0, 0,
                       10, 0, 0, 2, 198, 18, 0, 1]
    return Data(ip + udp + dns)
}
