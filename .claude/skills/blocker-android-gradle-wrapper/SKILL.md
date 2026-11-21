---
name: blocker-android-gradle-wrapper
description: Use when bumping Gradle or AGP, or gradlew fails, including jlink, JdkImageTransform, GraalVM or JAVA_HOME build errors.
category: build
---

# Android Gradle wrapper (mobile/android)

Pin the wrapper to the newest Gradle that the pinned Android Gradle Plugin supports. Do not jump to the newest Gradle release.

- The project is on AGP 9.x (`agp` in `mobile/android/gradle/libs.versions.toml`), with built-in Kotlin: no `kotlin-android` plugin. The `kotlin-compose` plugin version (`kotlin`) sets the KGP on the classpath. AGP 9.5.2 loads on Gradle 9.9.2. AGP 8.x fails on Gradle 9.6+ (`InternalProblems` was removed), so never go back to AGP 8 with a newer Gradle.
- Gradle 8.14.x does not start on this Mac's Java 25, so the floor here is 9.5.x.
- Don't run Gradle on GraalVM. AGP runs `jlink` from Gradle's JDK, and GraalVM's `jlink` fails with `Module jdk.internal.vm.ci not found` (the error shows as `JdkImageTransform` / `androidJdkImage`). Use a standard JDK; the user's default is Corretto 25. Don't pin a JDK in the repo (no `gradle-daemon-jvm.properties`): the user chooses it.
- JDK failures first: if a build dies in `JdkImageTransform` / `androidJdkImage` or with a configuration-cache error on `JdkImageInput`, the JDK is the cause. Switch JDK; don't disable the configuration cache. A long-lived shell can keep an old `JAVA_HOME` after the default changes, so build with `export JAVA_HOME=$HOME/.local/opt/sdkman/candidates/java/current` (SDKMAN's `current`) or open a new terminal.
- Bump: set `distributionUrl` in `gradle/wrapper/gradle-wrapper.properties` to the target version, then run `./gradlew wrapper --gradle-version <v> --no-daemon`. Editing the URL first is what lets a wrapper that can't start still bootstrap the new version.
- If `./gradlew wrapper` fails at project configuration ("SDK location not found"), regenerate in an empty scratch project: `touch settings.gradle.kts`, run the wrapper task there, copy `gradlew`, `gradlew.bat` and `gradle/wrapper/gradle-wrapper.jar` back, and take the new properties file. Use the session scratchpad, never `/tmp`.
- The generator may add default keys (`retries`, `retryBackOffMs`); keep them.
- Verify with `./gradlew --version`. Full configuration needs `ANDROID_HOME` or `sdk.dir` in `local.properties`. Without an SDK the build stops at `:blocker-core`, so report only "AGP loads" and say the rest is unchecked.
- Commit the wrapper change with a message that states why the version is capped.

Background for the cap and the AGP 9 question: see references/agp-gradle-compat.md.
