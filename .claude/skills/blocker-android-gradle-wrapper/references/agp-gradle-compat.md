# AGP / Gradle / Java compatibility seen in blocker-apps

- AGP 8.13.0 + Gradle 9.7.1: fails at `com.android.application` plugin application. Error: `Plugin 'com.android.internal.application' relies on 'org.gradle.api.problems.internal.InternalProblems', a Gradle internal API that was removed in Gradle 9.6.0.`
- AGP 8.13.0 + Gradle 9.5.1: plugin loads; configuration stops without an Android SDK.
- Gradle 8.14.3 on Java 25 (GraalVM CE 25.0.4): does not start.
- Gradle releases listed at services.gradle.org/versions/all in range 9.5-9.7: 9.7.1, 9.7.0, 9.6.1, 9.6.0, 9.5.1, 9.5.0.
- AGP 9 has Kotlin built in, so moving to it meant removing the `kotlin-android` plugin (done in PR #18).
- AGP 9.5.2 + Kotlin (compose plugin) 2.4.21 + Gradle 9.9.2 (current): debug and release build locally on Corretto 25, and CI's `:app:assembleDebug test` passes on Temurin 17.
