# blocker_jni.c binds Java_dev_blocker_core_NativeBridge_* by name and throws
# BlockerException via its (String) constructor; keep both from R8.
-keep class dev.blocker.core.NativeBridge {
    native <methods>;
}
-keep class dev.blocker.core.BlockerException {
    <init>(java.lang.String);
}
