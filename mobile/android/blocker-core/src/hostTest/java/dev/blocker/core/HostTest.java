package dev.blocker.core;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Arrays;

/**
 * Exercises blocker_jni.c on the host JVM, without Android or Kotlin.
 * Run via run-host-test.sh. Exits non-zero on the first failed check.
 */
public final class HostTest {
    private static int checks = 0;

    private static void check(boolean ok, String what) {
        checks++;
        if (!ok) {
            throw new AssertionError("FAILED: " + what);
        }
        System.out.println("ok   " + what);
    }

    private static void expectThrows(Class<? extends Throwable> type, String what, Runnable r) {
        try {
            r.run();
        } catch (Throwable t) {
            check(type.isInstance(t), what + " -> " + t.getClass().getSimpleName() + ": " + t.getMessage());
            return;
        }
        check(false, what + " (no exception)");
    }

    public static void main(String[] args) throws Exception {
        String config = String.join("\n",
            "[[rule]]",
            "name = \"focus\"",
            "domains = [\"example.com\", \"reddit.com\"]",
            "bundle_ids = [\"com.example.game\"]",
            "",
            "[[rule]]",
            "name = \"évening 🌙\"",
            "domains = [\"news.example.org\"]",
            "schedule = { type = \"windows\", windows = [{ start = \"18:00\", end = \"22:00\" }] }",
            "");

        String version = NativeBridge.version();
        check(version != null && !version.isEmpty(), "version() = " + version);

        long h = NativeBridge.engineFromToml(config);
        check(h != 0, "engineFromToml returns a handle");

        String[] v = NativeBridge.checkHost(h, "www.example.com", 0, 600);
        check(Arrays.equals(v, new String[] {"block", "focus", "example.com"}),
              "checkHost(www.example.com) = " + Arrays.toString(v));
        v = NativeBridge.checkHost(h, "example.net", 0, 600);
        check(Arrays.equals(v, new String[] {"allow", null, null}), "checkHost(example.net) = " + Arrays.toString(v));

        // Scheduled rule with a non-ASCII (supplementary) name round-trips as UTF-8.
        v = NativeBridge.checkHost(h, "news.example.org", 2, 19 * 60);
        check(v[0].equals("block") && v[1].equals("évening 🌙"), "scheduled rule inside window = " + Arrays.toString(v));
        v = NativeBridge.checkHost(h, "news.example.org", 2, 9 * 60);
        check(v[0].equals("allow"), "scheduled rule outside window = " + Arrays.toString(v));

        String active = NativeBridge.activeRulesJson(h, 2, 19 * 60);
        check(active.equals("[\"focus\",\"évening 🌙\"]"), "activeRulesJson = " + active);
        String apps = NativeBridge.blockedAppsJson(h, 2, 9 * 60);
        check(apps.contains("com.example.game"), "blockedAppsJson = " + apps);
        check(!NativeBridge.isIdle(h, 0, 0), "isIdle = false with an always-on rule");
        check(NativeBridge.warningsJson(h).startsWith("["), "warningsJson = " + NativeBridge.warningsJson(h));

        // TCP 443, no hostname -> PEEK; then decide again with the peeked host.
        v = NativeBridge.decide(h, null, "93.184.216.34", 443, 0, "com.android.chrome", false, false, null, 0, 600);
        check(v[0].equals("peek"), "decide(no hostname) = " + Arrays.toString(v));
        v = NativeBridge.decide(h, null, "93.184.216.34", 443, 0, "com.android.chrome", false, true, "reddit.com", 0, 600);
        check(v[0].equals("block") && v[1].equals("focus"), "decide(peeked reddit.com) = " + Arrays.toString(v));
        v = NativeBridge.decide(h, "old.reddit.com", null, 443, 0, null, false, false, null, 0, 600);
        check(v[0].equals("block"), "decide(hostname) = " + Arrays.toString(v));

        check("www.example.com".equals(NativeBridge.normalizeDomain("https://WWW.Example.com/x")),
              "normalizeDomain(url) = " + NativeBridge.normalizeDomain("https://WWW.Example.com/x"));
        check(NativeBridge.normalizeDomain("not a domain!") == null, "normalizeDomain(invalid) = null");

        byte[] http = "GET / HTTP/1.1\r\nHost: Reddit.com\r\n\r\n".getBytes(StandardCharsets.US_ASCII);
        int[] want = new int[1];
        String host = NativeBridge.peekHost(http, want);
        check(host != null && host.equalsIgnoreCase("reddit.com") && want[0] == 0, "peekHost(HTTP) = " + host);
        want[0] = -1;
        host = NativeBridge.peekHost(new byte[] {0x16, 0x03, 0x01}, want);
        check(host == null && want[0] > 0, "peekHost(partial TLS) wants " + want[0] + " bytes");
        check(NativeBridge.peekHost(new byte[0], null) == null, "peekHost(empty) = null");

        check(NativeBridge.validateConfig(config) == null, "validateConfig(valid) = null");
        String err = NativeBridge.validateConfig("[[rule]\nname=");
        check(err != null && !err.isEmpty(), "validateConfig(invalid) = " + err.replace('\n', ' '));

        expectThrows(BlockerException.class, "engineFromToml(invalid)", () -> NativeBridge.engineFromToml("[[rule"));

        Path dir = Files.createTempDirectory("blocker-jni");
        Files.writeString(dir.resolve("config.toml"), config);
        long h2 = NativeBridge.engineOpenDir(dir.toString());
        v = NativeBridge.checkHost(h2, "example.com", 0, 600);
        check(v[0].equals("block"), "engineOpenDir + checkHost = " + Arrays.toString(v));
        NativeBridge.engineFree(h2);

        NativeBridge.engineFree(h);
        NativeBridge.engineFree(0);
        expectThrows(IllegalStateException.class, "checkHost(handle 0)", () -> NativeBridge.checkHost(0, "x.com", 0, 0));

        // Stress: many calls that allocate and free native strings/verdicts.
        long h3 = NativeBridge.engineFromToml(config);
        for (int i = 0; i < 200_000; i++) {
            NativeBridge.checkHost(h3, "www.example.com", 0, 600);
            NativeBridge.activeRulesJson(h3, 0, 600);
        }
        NativeBridge.engineFree(h3);
        check(true, "200k checkHost/activeRulesJson calls completed");

        System.out.println(checks + " checks passed");
    }
}
