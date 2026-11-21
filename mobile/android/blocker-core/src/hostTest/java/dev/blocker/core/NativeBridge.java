package dev.blocker.core;

/**
 * Host-JVM twin of the Kotlin {@code internal object NativeBridge}: identical
 * static native signatures, so the same blocker_jni.c symbols bind to it.
 * Loads the library from the {@code blocker.jni.path} system property.
 */
final class NativeBridge {
    static {
        System.load(System.getProperty("blocker.jni.path"));
    }

    private NativeBridge() {}

    static native String version();
    static native long engineFromToml(String toml);
    static native long engineOpenDir(String dir);
    static native void engineFree(long handle);
    static native String warningsJson(long handle);
    static native boolean isIdle(long handle, int day, int minute);
    static native String activeRulesJson(long handle, int day, int minute);
    static native String blockedAppsJson(long handle, int day, int minute);
    static native String[] checkHost(long handle, String host, int day, int minute);
    static native String[] decide(long handle, String hostname, String remoteIp, int remotePort,
                                  int proto, String appId, boolean ech, boolean peeked,
                                  String peekedHost, int day, int minute);
    static native String normalizeDomain(String input);
    static native String peekHost(byte[] bytes, int[] wantMoreOut);
    static native String validateConfig(String toml);
}
