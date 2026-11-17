package dev.blocker.app

import dev.blocker.app.vpn.DnsPacket
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

class DnsPacketTest {
    /** A query for [name] (type A, RD set), id 0x1234. */
    private fun dnsQuery(name: String, qtype: Int = 1): ByteArray {
        val out = java.io.ByteArrayOutputStream()
        out.write(byteArrayOf(0x12, 0x34, 0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0))
        for (label in name.split('.')) {
            out.write(label.length)
            out.write(label.toByteArray())
        }
        out.write(0)
        out.write(byteArrayOf(0, qtype.toByte(), 0, 1))
        return out.toByteArray()
    }

    /** IPv4/UDP packet 10.111.222.1:40000 -> 10.111.222.53:53 carrying [dns]. */
    private fun ipv4Udp(dns: ByteArray, dstPort: Int = 53): ByteArray {
        val total = 28 + dns.size
        val p = ByteArray(total)
        p[0] = 0x45
        p[2] = (total shr 8).toByte(); p[3] = total.toByte()
        p[8] = 64; p[9] = 17
        byteArrayOf(10, 111, 222.toByte(), 1).copyInto(p, 12)
        byteArrayOf(10, 111, 222.toByte(), 53).copyInto(p, 16)
        p[20] = (40000 shr 8).toByte(); p[21] = 40000.toByte()
        p[22] = (dstPort shr 8).toByte(); p[23] = dstPort.toByte()
        p[24] = ((8 + dns.size) shr 8).toByte(); p[25] = (8 + dns.size).toByte()
        dns.copyInto(p, 28)
        return p
    }

    @Test
    fun parsesQuestion() {
        assertEquals("www.example.com" to 1, DnsPacket.parseQuestion(dnsQuery("WWW.Example.com")))
        assertEquals("example.com" to 28, DnsPacket.parseQuestion(dnsQuery("example.com", qtype = 28)))
    }

    @Test
    fun parsesIpv4UdpQuery() {
        val dns = dnsQuery("reddit.com")
        val packet = ipv4Udp(dns)
        // Trailing bytes past the IP total length are ignored.
        assertNotNull(DnsPacket.parseQuery(packet + byteArrayOf(9, 9)))
        val q = DnsPacket.parseQuery(packet)!!
        assertEquals("reddit.com", q.name)
        assertEquals(40000, q.sourcePort)
        assertEquals(53, q.destinationPort)
        assertArrayEquals(dns, q.dns)
    }

    @Test
    fun ignoresNonDnsAndMalformedPackets() {
        assertNull(DnsPacket.parseQuery(ipv4Udp(dnsQuery("a.com"), dstPort = 443)))
        assertNull(DnsPacket.parseQuery(ByteArray(10)))
        val v6 = ipv4Udp(dnsQuery("a.com")).also { it[0] = 0x60 }
        assertNull(DnsPacket.parseQuery(v6))
        val truncated = ipv4Udp(dnsQuery("a.com"))
        assertNull(DnsPacket.parseQuery(truncated, truncated.size - 3))
        val response = dnsQuery("a.com").also { it[2] = 0x81.toByte() }
        assertNull(DnsPacket.parseQuestion(response))
    }

    @Test
    fun nxdomainKeepsIdAndQuestion() {
        val query = dnsQuery("blocked.example") + byteArrayOf(0, 0, 41, 16, 0, 0, 0, 0, 0, 0, 0) // + EDNS OPT
        query[11] = 1 // ARCOUNT = 1
        val answer = DnsPacket.nxdomain(query)
        assertEquals(0x12, answer[0].toInt())
        assertEquals(0x34, answer[1].toInt())
        assertEquals(0x81, answer[2].toInt() and 0xff) // QR + RD
        assertEquals(0x83, answer[3].toInt() and 0xff) // RA + NXDOMAIN
        assertEquals(dnsQuery("blocked.example").size, answer.size) // OPT dropped
        assertEquals(0, answer[11].toInt())
    }

    @Test
    fun responsePacketSwapsAddressesWithValidChecksum() {
        val q = DnsPacket.parseQuery(ipv4Udp(dnsQuery("a.com")))!!
        val payload = DnsPacket.nxdomain(q.dns)
        val p = DnsPacket.ipv4Response(q, payload)
        assertEquals(28 + payload.size, p.size)
        assertArrayEquals(byteArrayOf(10, 111, 222.toByte(), 53), p.copyOfRange(12, 16))
        assertArrayEquals(byteArrayOf(10, 111, 222.toByte(), 1), p.copyOfRange(16, 20))
        assertEquals(53, ((p[20].toInt() and 0xff) shl 8) or (p[21].toInt() and 0xff))
        assertEquals(40000, ((p[22].toInt() and 0xff) shl 8) or (p[23].toInt() and 0xff))
        // A header with a correct checksum sums to 0 (one's complement).
        assertEquals(0, DnsPacket.ipChecksum(p, 0, 20))
    }
}
