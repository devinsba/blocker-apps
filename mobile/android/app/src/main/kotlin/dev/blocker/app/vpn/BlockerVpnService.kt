package dev.blocker.app.vpn

import android.content.Context
import android.content.Intent
import android.net.ConnectivityManager
import android.net.VpnService
import android.os.ParcelFileDescriptor
import android.util.Log
import dev.blocker.app.R
import dev.blocker.app.data.ConfigRepository
import dev.blocker.core.BlockerEngine
import dev.blocker.core.Verdict
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.Inet4Address
import java.net.InetAddress
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * DNS-only VPN: the tunnel advertises a fake DNS server ([VPN_DNS]) and routes
 * only that one address into the TUN interface, so the app sees every DNS
 * query (and nothing else). Each queried name goes through
 * [BlockerEngine.checkHost]; blocked names get NXDOMAIN, others are forwarded
 * to the underlying network's resolver through a [protect]ed socket.
 *
 * This is a working skeleton, not a finished filter. Known gaps (TODOs below):
 * - IPv4/UDP only: no IPv6 DNS server, no DNS over TCP.
 * - Private DNS (DoT) in "strict" mode, DoH in browsers and hard-coded IPs
 *   bypass DNS filtering entirely. Full coverage needs routing all traffic
 *   (0.0.0.0/0) through the TUN and using engine.decide() + peekHost() on
 *   TCP flows, i.e. a userspace TCP/IP stack as NetGuard does.
 * - App blocking (engine.blockedApps().bundleIds = package names) is not
 *   enforced here; see [applyAppRules].
 * - No foreground notification / boot receiver yet.
 */
class BlockerVpnService : VpnService() {
    private var tun: ParcelFileDescriptor? = null
    private var reader: Thread? = null
    private var forwarders: ExecutorService? = null
    private var upstreamDns: List<InetAddress> = emptyList()
    private val tunWriteLock = Any()

    @Volatile
    private var engine: BlockerEngine? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                stopVpn()
                stopSelf()
                return START_NOT_STICKY
            }
            ACTION_RELOAD -> {
                if (tun != null) reloadEngine()
                return START_STICKY
            }
            // ACTION_START, or null when restarted by the system / always-on VPN.
            else -> startVpn()
        }
        return START_STICKY
    }

    override fun onRevoke() {
        // The user started another VPN or revoked consent in Settings.
        stopVpn()
        stopSelf()
    }

    override fun onDestroy() {
        stopVpn()
        super.onDestroy()
    }

    private fun startVpn() {
        if (tun != null) return
        if (!reloadEngine()) {
            stopSelf()
            return
        }
        // Must be read before establish(): afterwards the VPN is the default network.
        upstreamDns = findUpstreamDns()

        val builder = Builder()
            .setSession(getString(R.string.vpn_session_name))
            .addAddress(VPN_ADDRESS, 32)
            .addDnsServer(VPN_DNS)
            .addRoute(VPN_DNS, 32) // only DNS to our fake server enters the tunnel
            .setMtu(MTU)
            // TODO(shutdown): a blocking read() on the TUN fd is not reliably woken by
            //   close(); switch to non-blocking + poll (android.system.Os.poll) with a
            //   wake-up pipe so stopVpn() can join the reader thread.
            .setBlocking(true)
        // Our own forwarded queries must not loop back into the tunnel.
        builder.addDisallowedApplication(packageName)
        applyAppRules(builder)
        // TODO(ipv6): addAddress("fd00:b10c::1", 128), addDnsServer/addRoute an IPv6
        //   DNS address, and handle IPv6 in DnsPacket; otherwise IPv6-only networks
        //   resolve through the system's IPv6 DNS servers unfiltered.
        // TODO(metered): setUnderlyingNetworks(null) tracks the default network; consider
        //   setMetered(false) on API 29+ so apps don't treat the VPN as metered.

        val fd = builder.establish()
        if (fd == null) {
            // VpnService.prepare() was not approved (or was revoked).
            Log.w(TAG, "VPN not prepared; ask the user via VpnService.prepare()")
            stopSelf()
            return
        }
        tun = fd
        forwarders = Executors.newFixedThreadPool(FORWARD_THREADS)
        reader = Thread({ readLoop(fd) }, "blocker-vpn-reader").also { it.start() }
        _running.value = true
        Log.i(TAG, "DNS filter started; upstream=$upstreamDns")
    }

    private fun stopVpn() {
        reader?.interrupt()
        reader = null
        forwarders?.shutdownNow()
        forwarders = null
        try {
            tun?.close() // unblocks the reader's read()
        } catch (e: IOException) {
            Log.w(TAG, "closing tun", e)
        }
        tun = null
        engine?.close()
        engine = null
        _running.value = false
    }

    /** Swaps in an engine built from the current config. Returns false on failure. */
    private fun reloadEngine(): Boolean = try {
        val fresh = ConfigRepository(filesDir).openEngine()
        val old = engine
        engine = fresh
        old?.close() // in-flight lookups on `old` fail fast and retry on `fresh`
        fresh.warnings.forEach { Log.w(TAG, "config: $it") }
        true
    } catch (t: Throwable) {
        Log.e(TAG, "could not load rules", t)
        false
    }

    /**
     * Placeholder for per-app blocking. Android allows either an allow-list or
     * a deny-list of apps per tunnel, not both, and one VPN at a time, so app
     * blocking cannot simply be layered on this DNS-only tunnel.
     *
     * TODO(apps): options, see research/android-blocking-approaches.md:
     *   1. Full-tunnel mode: route 0.0.0.0/0 + ::/0 and drop packets whose owner
     *      (ConnectivityManager.getConnectionOwnerUid, API 29+) maps to a package in
     *      engine.blockedApps().bundleIds; needs a userspace TCP/IP stack to forward
     *      everything else.
     *   2. UsageStatsManager / AccessibilityService to detect the blocked app in the
     *      foreground and cover it with a blocking screen.
     */
    @Suppress("UNUSED_PARAMETER")
    private fun applyAppRules(builder: Builder) {
        val apps = engine?.blockedApps()?.bundleIds.orEmpty()
        if (apps.isNotEmpty()) Log.i(TAG, "app rules not enforced yet: $apps")
    }

    private fun readLoop(fd: ParcelFileDescriptor) {
        val input = FileInputStream(fd.fileDescriptor)
        val output = FileOutputStream(fd.fileDescriptor)
        val buffer = ByteArray(MTU)
        try {
            while (!Thread.currentThread().isInterrupted) {
                val n = input.read(buffer)
                if (n <= 0) continue
                val packet = buffer.copyOf(n)
                // TODO(parsing): only IPv4/UDP/53 is understood; everything else that
                //   reaches the tunnel (it shouldn't, given the single /32 route) is dropped.
                val query = DnsPacket.parseQuery(packet) ?: continue
                handleQuery(query, output)
            }
        } catch (e: IOException) {
            if (tun != null) Log.w(TAG, "tun read failed", e)
        }
    }

    private fun handleQuery(query: DnsQuery, output: FileOutputStream) {
        val verdict = verdictFor(query.name)
        if (verdict is Verdict.Block) {
            Log.d(TAG, "blocked ${query.name} (rule ${verdict.rule}, matched ${verdict.matched})")
            write(output, DnsPacket.ipv4Response(query, DnsPacket.nxdomain(query.dns)))
            return
        }
        // Forward off the reader thread so one slow upstream doesn't stall all lookups.
        forwarders?.execute {
            val answer = forward(query.dns) ?: return@execute // client will retry
            write(output, DnsPacket.ipv4Response(query, answer))
        }
    }

    /** checkHost with a retry if the engine was swapped mid-call. Fails open. */
    private fun verdictFor(host: String): Verdict {
        repeat(2) {
            val e = engine ?: return Verdict.Allow
            try {
                return e.checkHost(host)
            } catch (closed: IllegalStateException) {
                // closed by reloadEngine(); retry with the new one
            } catch (t: Throwable) {
                Log.e(TAG, "checkHost($host) failed", t)
                return Verdict.Allow
            }
        }
        return Verdict.Allow
    }

    /** Sends [dns] to the first upstream resolver that answers. */
    private fun forward(dns: ByteArray): ByteArray? {
        for (server in upstreamDns) {
            try {
                DatagramSocket().use { socket ->
                    protect(socket) // bypass the tunnel
                    socket.soTimeout = UPSTREAM_TIMEOUT_MS
                    socket.send(DatagramPacket(dns, dns.size, server, DnsPacket.DNS_PORT))
                    val buf = ByteArray(MAX_DNS_RESPONSE)
                    val reply = DatagramPacket(buf, buf.size)
                    socket.receive(reply)
                    return buf.copyOf(reply.length)
                }
            } catch (e: IOException) {
                Log.d(TAG, "upstream $server failed: ${e.message}")
            }
        }
        // TODO(upstream): if the answer is truncated (TC bit), retry over TCP.
        return null
    }

    private fun write(output: FileOutputStream, packet: ByteArray) {
        synchronized(tunWriteLock) {
            try {
                output.write(packet)
            } catch (e: IOException) {
                Log.w(TAG, "tun write failed", e)
            }
        }
    }

    /** IPv4 resolvers of the current default network (before our VPN is up). */
    private fun findUpstreamDns(): List<InetAddress> {
        val cm = getSystemService(ConnectivityManager::class.java)
        val servers = cm?.activeNetwork?.let { cm.getLinkProperties(it) }?.dnsServers.orEmpty()
            .filterIsInstance<Inet4Address>()
        // TODO(upstream): track network changes (registerDefaultNetworkCallback) and
        //   refresh this list; let the user pick a resolver instead of the fallback.
        return servers.ifEmpty { listOf(InetAddress.getByName(FALLBACK_DNS)) }
    }

    companion object {
        private const val TAG = "BlockerVpn"
        const val ACTION_START = "dev.blocker.app.vpn.START"
        const val ACTION_STOP = "dev.blocker.app.vpn.STOP"
        const val ACTION_RELOAD = "dev.blocker.app.vpn.RELOAD"

        /** Tunnel address and the fake DNS server apps are told to use. */
        private const val VPN_ADDRESS = "10.111.222.1"
        private const val VPN_DNS = "10.111.222.53"
        private const val FALLBACK_DNS = "1.1.1.1"
        private const val MTU = 1500
        private const val MAX_DNS_RESPONSE = 4096
        private const val UPSTREAM_TIMEOUT_MS = 5000
        private const val FORWARD_THREADS = 4

        private val _running = MutableStateFlow(false)
        val running: StateFlow<Boolean> = _running.asStateFlow()

        fun start(context: Context) {
            context.startService(Intent(context, BlockerVpnService::class.java).setAction(ACTION_START))
        }

        fun stop(context: Context) {
            context.startService(Intent(context, BlockerVpnService::class.java).setAction(ACTION_STOP))
        }

        /** Recompiles rules after config.toml changed. No-op if not running. */
        fun reload(context: Context) {
            if (_running.value) {
                context.startService(Intent(context, BlockerVpnService::class.java).setAction(ACTION_RELOAD))
            }
        }
    }
}
