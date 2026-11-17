package dev.blocker.app.vpn

import java.net.InetAddress

/** A DNS query read from the TUN interface (IPv4 + UDP only for now). */
class DnsQuery(
    val sourceIp: ByteArray,
    val destinationIp: ByteArray,
    val sourcePort: Int,
    val destinationPort: Int,
    /** The UDP payload: the raw DNS message. */
    val dns: ByteArray,
    /** First question's name, lowercase, without the trailing dot. */
    val name: String,
    val qtype: Int,
) {
    val destination: InetAddress get() = InetAddress.getByAddress(destinationIp)
}

/**
 * Pure-Kotlin packet helpers for the DNS-only VPN (no Android APIs, so they
 * are covered by JVM unit tests).
 *
 * TODO(ipv6): parse IPv6 (version 6, next header 17) queries and build IPv6
 *   responses; today the tunnel only advertises an IPv4 DNS server.
 * TODO(tcp): DNS over TCP (truncated answers, large records) is not handled.
 */
object DnsPacket {
    const val DNS_PORT = 53
    private const val IPV4_HEADER = 20
    private const val UDP_HEADER = 8
    private const val DNS_HEADER = 12
    private const val PROTO_UDP = 17
    private const val RCODE_NXDOMAIN = 3

    /** Parses an IPv4/UDP packet to port 53 carrying a standard query, or null. */
    fun parseQuery(packet: ByteArray, length: Int = packet.size): DnsQuery? {
        if (length < IPV4_HEADER + UDP_HEADER + DNS_HEADER) return null
        val version = u8(packet, 0) shr 4
        if (version != 4) return null // TODO(ipv6)
        val ihl = (u8(packet, 0) and 0x0f) * 4
        if (ihl < IPV4_HEADER || u8(packet, 9) != PROTO_UDP) return null
        val totalLength = u16(packet, 2)
        if (totalLength > length || totalLength < ihl + UDP_HEADER) return null
        // Fragmented datagrams (MF set or offset != 0) are not reassembled.
        if (u16(packet, 6) and 0x3fff != 0) return null

        val udp = ihl
        val srcPort = u16(packet, udp)
        val dstPort = u16(packet, udp + 2)
        val udpLength = u16(packet, udp + 4)
        if (dstPort != DNS_PORT || udpLength < UDP_HEADER || udp + udpLength > totalLength) return null

        val dns = packet.copyOfRange(udp + UDP_HEADER, udp + udpLength)
        val (name, qtype) = parseQuestion(dns) ?: return null
        return DnsQuery(
            sourceIp = packet.copyOfRange(12, 16),
            destinationIp = packet.copyOfRange(16, 20),
            sourcePort = srcPort,
            destinationPort = dstPort,
            dns = dns,
            name = name,
            qtype = qtype,
        )
    }

    /** First question of a DNS query message: (name, qtype), or null. */
    fun parseQuestion(dns: ByteArray): Pair<String, Int>? {
        if (dns.size < DNS_HEADER) return null
        val flags = u16(dns, 2)
        if (flags and 0x8000 != 0) return null // a response, not a query
        if ((flags shr 11) and 0xf != 0) return null // opcode != QUERY
        if (u16(dns, 4) < 1) return null // QDCOUNT
        val end = questionEnd(dns) ?: return null
        val labels = mutableListOf<String>()
        var i = DNS_HEADER
        while (true) {
            val len = u8(dns, i++)
            if (len == 0) break
            labels += String(dns, i, len, Charsets.ISO_8859_1).lowercase()
            i += len
        }
        return labels.joinToString(".") to u16(dns, end - 4)
    }

    /** Index just past the first question (name + qtype + qclass), or null. */
    private fun questionEnd(dns: ByteArray): Int? {
        var i = DNS_HEADER
        var nameLength = 0
        while (true) {
            if (i >= dns.size) return null
            val len = u8(dns, i++)
            if (len == 0) break
            if (len and 0xc0 != 0) return null // compression pointers don't occur in questions
            nameLength += len + 1
            if (nameLength > 255) return null
            i += len
        }
        return if (i + 4 <= dns.size) i + 4 else null
    }

    /**
     * An NXDOMAIN answer to [query]: same id and question, QR set, RD copied,
     * RA set, no records (any EDNS OPT record is dropped).
     */
    fun nxdomain(query: ByteArray): ByteArray {
        val end = questionEnd(query) ?: throw IllegalArgumentException("not a DNS query")
        val out = query.copyOf(end)
        out[2] = (0x80 or (u8(query, 2) and 0x79)).toByte() // QR | opcode | RD (clear AA, TC)
        out[3] = (0x80 or RCODE_NXDOMAIN).toByte() // RA | NXDOMAIN
        out[4] = 0; out[5] = 1 // QDCOUNT = 1
        for (k in 6 until 12) out[k] = 0 // ANCOUNT, NSCOUNT, ARCOUNT
        return out
    }

    /**
     * Wraps a DNS [response] in an IPv4/UDP packet addressed back to whoever
     * sent [query] (source and destination swapped). UDP checksum is 0
     * (optional for IPv4).
     */
    fun ipv4Response(query: DnsQuery, response: ByteArray): ByteArray {
        val total = IPV4_HEADER + UDP_HEADER + response.size
        require(total <= 0xffff) { "response too large" }
        val p = ByteArray(total)
        p[0] = 0x45 // version 4, IHL 5
        put16(p, 2, total)
        put16(p, 6, 0x4000) // don't fragment
        p[8] = 64 // TTL
        p[9] = PROTO_UDP.toByte()
        query.destinationIp.copyInto(p, 12)
        query.sourceIp.copyInto(p, 16)
        put16(p, 10, ipChecksum(p, 0, IPV4_HEADER))

        put16(p, IPV4_HEADER, query.destinationPort)
        put16(p, IPV4_HEADER + 2, query.sourcePort)
        put16(p, IPV4_HEADER + 4, UDP_HEADER + response.size)
        response.copyInto(p, IPV4_HEADER + UDP_HEADER)
        return p
    }

    internal fun ipChecksum(b: ByteArray, offset: Int, length: Int): Int {
        var sum = 0L
        var i = offset
        while (i + 1 < offset + length) {
            sum += u16(b, i)
            i += 2
        }
        if (i < offset + length) sum += u8(b, i) shl 8
        while (sum shr 16 != 0L) sum = (sum and 0xffff) + (sum shr 16)
        return sum.inv().toInt() and 0xffff
    }

    private fun u8(b: ByteArray, i: Int) = b[i].toInt() and 0xff
    private fun u16(b: ByteArray, i: Int) = (u8(b, i) shl 8) or u8(b, i + 1)
    private fun put16(b: ByteArray, i: Int, v: Int) {
        b[i] = (v shr 8).toByte()
        b[i + 1] = v.toByte()
    }
}
