package dev.blocker.core

/**
 * Raw JNI entry points implemented in `src/main/cpp/blocker_jni.c`.
 *
 * Every method is a static native on class `dev.blocker.core.NativeBridge`, so
 * the shim can also be exercised from plain Java (see `src/hostTest`). Use
 * [BlockerEngine] instead of calling these directly.
 *
 * Conventions shared with the C side:
 * - engine handles are `jlong` pointers; 0 means closed (throws IllegalStateException);
 * - verdicts come back as `String[3] { "allow"|"block"|"peek", rule, matched }`;
 * - failures throw [BlockerException] carrying `blocker_last_error()`.
 */
internal object NativeBridge {
    init {
        System.loadLibrary("blocker_jni")
    }

    @JvmStatic external fun version(): String

    @JvmStatic external fun engineFromToml(toml: String): Long

    @JvmStatic external fun engineOpenDir(dir: String): Long

    @JvmStatic external fun engineFree(handle: Long)

    @JvmStatic external fun warningsJson(handle: Long): String

    @JvmStatic external fun isIdle(handle: Long, day: Int, minute: Int): Boolean

    @JvmStatic external fun activeRulesJson(handle: Long, day: Int, minute: Int): String

    @JvmStatic external fun blockedAppsJson(handle: Long, day: Int, minute: Int): String

    @JvmStatic external fun checkHost(handle: Long, host: String, day: Int, minute: Int): Array<String?>

    @JvmStatic external fun decide(
        handle: Long,
        hostname: String?,
        remoteIp: String?,
        remotePort: Int,
        proto: Int,
        appId: String?,
        ech: Boolean,
        peeked: Boolean,
        peekedHost: String?,
        day: Int,
        minute: Int,
    ): Array<String?>

    /** null if [input] is not a valid domain or URL. */
    @JvmStatic external fun normalizeDomain(input: String): String?

    /** [wantMoreOut] (size >= 1) receives the total byte count wanted, or 0. */
    @JvmStatic external fun peekHost(bytes: ByteArray, wantMoreOut: IntArray?): String?

    /** null if [toml] parses, otherwise the error message. */
    @JvmStatic external fun validateConfig(toml: String): String?
}
