package dev.blocker.core

/** Transport protocol of a [Flow]; [code] matches `BlockerProto` in blocker.h. */
enum class Proto(internal val code: Int) {
    TCP(0),
    UDP(1),
    OTHER(2),
}

/**
 * What the OS knows about a new network flow (`BlockerFlow` in blocker.h).
 * On Android, [appId] is the package name of the app that owns the socket
 * (from `ConnectivityManager.getConnectionOwnerUid` + `PackageManager`).
 */
data class Flow(
    val hostname: String? = null,
    val remoteIp: String? = null,
    val remotePort: Int = 0,
    val proto: Proto = Proto.TCP,
    val appId: String? = null,
    val ech: Boolean = false,
) {
    init {
        require(remotePort in 0..0xffff) { "remotePort out of range: $remotePort" }
    }
}

/** Result of [BlockerEngine.peekHost]. */
data class PeekResult(
    /** Hostname from the TLS SNI or HTTP Host header, if found. */
    val host: String?,
    /** If more bytes would help: total bytes wanted. Otherwise 0. */
    val wantMore: Int,
)

/** Apps blocked by currently active rules. On Android `bundleIds` are package names. */
data class BlockedApps(
    val bundleIds: List<String>,
    val processNames: List<String>,
)
