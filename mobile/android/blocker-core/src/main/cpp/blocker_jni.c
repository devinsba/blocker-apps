/*
 * blocker_jni.c — JNI shim between dev.blocker.core.NativeBridge (Kotlin/Java)
 * and the C ABI in crates/blocker-ffi/include/blocker.h (Rust).
 *
 * Plain C on purpose: the Rust side already exposes a C ABI, so this file only
 * marshals JNI types. Rules it follows:
 *   - Every char* and BlockerVerdict from blocker.h is freed before returning.
 *   - Strings cross the boundary as real UTF-8 (String.getBytes(UTF_8) /
 *     new String(bytes, UTF_8)), not JNI "modified UTF-8", so emoji and NUL
 *     handling match Rust's expectations.
 *   - Failures throw dev.blocker.core.BlockerException with the message from
 *     blocker_last_error() (falls back to RuntimeException if that class is
 *     not loadable, e.g. in a bare Java harness).
 *   - Engines are passed as a jlong handle; 0 means "closed" and throws
 *     IllegalStateException.
 *
 * The same file builds for Android (NDK, CMakeLists.txt) and as a host
 * library for the plain-Java harness in src/hostTest.
 */
#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "blocker.h"

#define BRIDGE(name) Java_dev_blocker_core_NativeBridge_##name

static jclass g_string_class;          /* java/lang/String */
static jobject g_utf8;                 /* StandardCharsets.UTF_8 */
static jmethodID g_string_ctor;        /* String(byte[], Charset) */
static jmethodID g_string_get_bytes;   /* String.getBytes(Charset) */
static jclass g_blocker_exception;     /* may be NULL */
static jclass g_runtime_exception;
static jclass g_illegal_state;

static jclass global_class(JNIEnv *env, const char *name) {
    jclass local = (*env)->FindClass(env, name);
    if (local == NULL) {
        (*env)->ExceptionClear(env);
        return NULL;
    }
    jclass global = (jclass)(*env)->NewGlobalRef(env, local);
    (*env)->DeleteLocalRef(env, local);
    return global;
}

JNIEXPORT jint JNICALL JNI_OnLoad(JavaVM *vm, void *reserved) {
    (void)reserved;
    JNIEnv *env = NULL;
    if ((*vm)->GetEnv(vm, (void **)&env, JNI_VERSION_1_6) != JNI_OK) {
        return JNI_ERR;
    }
    g_string_class = global_class(env, "java/lang/String");
    g_runtime_exception = global_class(env, "java/lang/RuntimeException");
    g_illegal_state = global_class(env, "java/lang/IllegalStateException");
    /* Cached here because FindClass on later calls may use the system class
     * loader, which cannot see app classes. Optional: absent in bare harnesses. */
    g_blocker_exception = global_class(env, "dev/blocker/core/BlockerException");

    jclass charsets = (*env)->FindClass(env, "java/nio/charset/StandardCharsets");
    if (charsets == NULL || g_string_class == NULL) {
        return JNI_ERR;
    }
    jfieldID utf8_field =
        (*env)->GetStaticFieldID(env, charsets, "UTF_8", "Ljava/nio/charset/Charset;");
    if (utf8_field == NULL) {
        return JNI_ERR;
    }
    jobject utf8 = (*env)->GetStaticObjectField(env, charsets, utf8_field);
    g_utf8 = (*env)->NewGlobalRef(env, utf8);
    (*env)->DeleteLocalRef(env, utf8);
    (*env)->DeleteLocalRef(env, charsets);

    g_string_ctor = (*env)->GetMethodID(env, g_string_class, "<init>",
                                        "([BLjava/nio/charset/Charset;)V");
    g_string_get_bytes = (*env)->GetMethodID(env, g_string_class, "getBytes",
                                             "(Ljava/nio/charset/Charset;)[B");
    if (g_utf8 == NULL || g_string_ctor == NULL || g_string_get_bytes == NULL) {
        return JNI_ERR;
    }
    return JNI_VERSION_1_6;
}

/* ---- errors ------------------------------------------------------------ */

static void throw_message(JNIEnv *env, jclass cls, const char *msg) {
    if ((*env)->ExceptionCheck(env)) {
        return; /* keep the first pending exception (e.g. OutOfMemoryError) */
    }
    if (cls == NULL) {
        cls = g_runtime_exception;
    }
    (*env)->ThrowNew(env, cls, msg);
}

/* Throws BlockerException with blocker_last_error(), or `fallback`. */
static void throw_last_error(JNIEnv *env, const char *fallback) {
    char *err = blocker_last_error();
    throw_message(env, g_blocker_exception, err != NULL ? err : fallback);
    blocker_string_free(err);
}

static const BlockerEngine *engine_of(JNIEnv *env, jlong handle) {
    if (handle == 0) {
        throw_message(env, g_illegal_state, "BlockerEngine is closed");
        return NULL;
    }
    return (const BlockerEngine *)(intptr_t)handle;
}

/* ---- strings ------------------------------------------------------------ */

/* Java String -> malloc'd NUL-terminated UTF-8, or NULL if `s` is null.
 * Sets *failed (with a pending exception) on allocation failure. A Java string
 * containing U+0000 is truncated at that point. */
static char *utf8_from_jstring(JNIEnv *env, jstring s, int *failed) {
    if (s == NULL) {
        return NULL;
    }
    jbyteArray bytes =
        (jbyteArray)(*env)->CallObjectMethod(env, s, g_string_get_bytes, g_utf8);
    if (bytes == NULL || (*env)->ExceptionCheck(env)) {
        *failed = 1;
        return NULL;
    }
    jsize len = (*env)->GetArrayLength(env, bytes);
    char *out = (char *)malloc((size_t)len + 1);
    if (out == NULL) {
        (*env)->DeleteLocalRef(env, bytes);
        throw_message(env, g_runtime_exception, "out of memory");
        *failed = 1;
        return NULL;
    }
    (*env)->GetByteArrayRegion(env, bytes, 0, len, (jbyte *)out);
    out[len] = '\0';
    (*env)->DeleteLocalRef(env, bytes);
    return out;
}

/* UTF-8 -> Java String, or NULL (null in Java) if `s` is NULL or on failure
 * (with a pending exception). Does not free `s`. */
static jstring jstring_from_utf8(JNIEnv *env, const char *s) {
    if (s == NULL) {
        return NULL;
    }
    jsize len = (jsize)strlen(s);
    jbyteArray bytes = (*env)->NewByteArray(env, len);
    if (bytes == NULL) {
        return NULL;
    }
    (*env)->SetByteArrayRegion(env, bytes, 0, len, (const jbyte *)s);
    jstring out = (jstring)(*env)->NewObject(env, g_string_class, g_string_ctor, bytes, g_utf8);
    (*env)->DeleteLocalRef(env, bytes);
    return out;
}

/* Converts and frees a string returned by blocker.h. Throws if `s` is NULL. */
static jstring take_string_or_throw(JNIEnv *env, char *s, const char *fallback) {
    if (s == NULL) {
        throw_last_error(env, fallback);
        return NULL;
    }
    jstring out = jstring_from_utf8(env, s);
    blocker_string_free(s);
    return out;
}

/* For functions where NULL is a normal answer ("invalid", "no host"): a NULL
 * result is an error only if blocker_last_error() is set. */
static jstring take_optional_string(JNIEnv *env, char *s) {
    if (s == NULL) {
        char *err = blocker_last_error();
        if (err != NULL) {
            throw_message(env, g_blocker_exception, err);
            blocker_string_free(err);
        }
        return NULL;
    }
    jstring out = jstring_from_utf8(env, s);
    blocker_string_free(s);
    return out;
}

/* ---- verdicts ----------------------------------------------------------- */

/* Verdict -> String[3] {kind, rule, matched}; kind is "allow" | "block" |
 * "peek". Always frees `v`. See Verdict.fromNative in Kotlin. */
static jobjectArray take_verdict(JNIEnv *env, BlockerVerdict *v) {
    const char *kind;
    switch (v->kind) {
    case BLOCKER_VERDICT_BLOCK: kind = "block"; break;
    case BLOCKER_VERDICT_PEEK: kind = "peek"; break;
    default: kind = "allow"; break;
    }
    jobjectArray out = (*env)->NewObjectArray(env, 3, g_string_class, NULL);
    if (out != NULL) {
        const char *parts[3] = {kind, v->rule, v->matched};
        for (jsize i = 0; i < 3; i++) {
            if (parts[i] == NULL) {
                continue;
            }
            jstring s = jstring_from_utf8(env, parts[i]);
            if (s == NULL) {
                out = NULL; /* exception pending; local ref freed on return */
                break;
            }
            (*env)->SetObjectArrayElement(env, out, i, s);
            (*env)->DeleteLocalRef(env, s);
        }
    }
    blocker_verdict_free(v);
    return out;
}

static uint16_t clamp_minute(jint minute) {
    if (minute < 0) return 0;
    if (minute > 24 * 60 - 1) return 24 * 60 - 1;
    return (uint16_t)minute;
}

/* ---- exports ------------------------------------------------------------ */

JNIEXPORT jstring JNICALL BRIDGE(version)(JNIEnv *env, jclass cls) {
    (void)cls;
    return jstring_from_utf8(env, blocker_version()); /* static, not freed */
}

JNIEXPORT jlong JNICALL BRIDGE(engineFromToml)(JNIEnv *env, jclass cls, jstring toml) {
    (void)cls;
    int failed = 0;
    char *text = utf8_from_jstring(env, toml, &failed);
    if (failed) return 0;
    BlockerEngine *engine = blocker_engine_from_toml(text);
    free(text);
    if (engine == NULL) {
        throw_last_error(env, "failed to compile config");
        return 0;
    }
    return (jlong)(intptr_t)engine;
}

JNIEXPORT jlong JNICALL BRIDGE(engineOpenDir)(JNIEnv *env, jclass cls, jstring dir) {
    (void)cls;
    int failed = 0;
    char *path = utf8_from_jstring(env, dir, &failed);
    if (failed) return 0;
    BlockerEngine *engine = blocker_engine_open_dir(path);
    free(path);
    if (engine == NULL) {
        throw_last_error(env, "failed to open config directory");
        return 0;
    }
    return (jlong)(intptr_t)engine;
}

JNIEXPORT void JNICALL BRIDGE(engineFree)(JNIEnv *env, jclass cls, jlong handle) {
    (void)env;
    (void)cls;
    if (handle != 0) {
        blocker_engine_free((BlockerEngine *)(intptr_t)handle);
    }
}

JNIEXPORT jstring JNICALL BRIDGE(warningsJson)(JNIEnv *env, jclass cls, jlong handle) {
    (void)cls;
    const BlockerEngine *engine = engine_of(env, handle);
    if (engine == NULL) return NULL;
    return take_string_or_throw(env, blocker_engine_warnings_json(engine), "warnings failed");
}

JNIEXPORT jboolean JNICALL BRIDGE(isIdle)(JNIEnv *env, jclass cls, jlong handle, jint day,
                                          jint minute) {
    (void)cls;
    const BlockerEngine *engine = engine_of(env, handle);
    if (engine == NULL) return JNI_TRUE;
    return blocker_engine_is_idle(engine, day, clamp_minute(minute)) ? JNI_TRUE : JNI_FALSE;
}

JNIEXPORT jstring JNICALL BRIDGE(activeRulesJson)(JNIEnv *env, jclass cls, jlong handle,
                                                  jint day, jint minute) {
    (void)cls;
    const BlockerEngine *engine = engine_of(env, handle);
    if (engine == NULL) return NULL;
    return take_string_or_throw(
        env, blocker_engine_active_rules_json(engine, day, clamp_minute(minute)),
        "active rules failed");
}

JNIEXPORT jstring JNICALL BRIDGE(blockedAppsJson)(JNIEnv *env, jclass cls, jlong handle,
                                                  jint day, jint minute) {
    (void)cls;
    const BlockerEngine *engine = engine_of(env, handle);
    if (engine == NULL) return NULL;
    return take_string_or_throw(
        env, blocker_engine_blocked_apps_json(engine, day, clamp_minute(minute)),
        "blocked apps failed");
}

JNIEXPORT jobjectArray JNICALL BRIDGE(checkHost)(JNIEnv *env, jclass cls, jlong handle,
                                                 jstring host, jint day, jint minute) {
    (void)cls;
    const BlockerEngine *engine = engine_of(env, handle);
    if (engine == NULL) return NULL;
    int failed = 0;
    char *h = utf8_from_jstring(env, host, &failed);
    if (failed) return NULL;
    BlockerVerdict v = blocker_engine_check_host(engine, h, day, clamp_minute(minute));
    free(h);
    return take_verdict(env, &v);
}

JNIEXPORT jobjectArray JNICALL BRIDGE(decide)(JNIEnv *env, jclass cls, jlong handle,
                                              jstring hostname, jstring remote_ip,
                                              jint remote_port, jint proto, jstring app_id,
                                              jboolean ech, jboolean peeked,
                                              jstring peeked_host, jint day, jint minute) {
    (void)cls;
    const BlockerEngine *engine = engine_of(env, handle);
    if (engine == NULL) return NULL;

    int failed = 0;
    char *c_host = utf8_from_jstring(env, hostname, &failed);
    char *c_ip = failed ? NULL : utf8_from_jstring(env, remote_ip, &failed);
    char *c_app = failed ? NULL : utf8_from_jstring(env, app_id, &failed);
    char *c_peeked = failed ? NULL : utf8_from_jstring(env, peeked_host, &failed);
    jobjectArray out = NULL;
    if (!failed) {
        BlockerFlow flow;
        memset(&flow, 0, sizeof flow);
        flow.hostname = c_host;
        flow.remote_ip = c_ip;
        flow.remote_port = (uint16_t)(remote_port & 0xffff);
        switch (proto) {
        case BLOCKER_PROTO_TCP: flow.proto = BLOCKER_PROTO_TCP; break;
        case BLOCKER_PROTO_UDP: flow.proto = BLOCKER_PROTO_UDP; break;
        default: flow.proto = BLOCKER_PROTO_OTHER; break;
        }
        flow.app_id = c_app;
        flow.ech = ech == JNI_TRUE;
        BlockerVerdict v = blocker_engine_decide(engine, &flow, peeked == JNI_TRUE, c_peeked,
                                                 day, clamp_minute(minute));
        out = take_verdict(env, &v);
    }
    free(c_host);
    free(c_ip);
    free(c_app);
    free(c_peeked);
    return out;
}

JNIEXPORT jstring JNICALL BRIDGE(normalizeDomain)(JNIEnv *env, jclass cls, jstring input) {
    (void)cls;
    int failed = 0;
    char *in = utf8_from_jstring(env, input, &failed);
    if (failed) return NULL;
    char *out = blocker_normalize_domain(in);
    free(in);
    return take_optional_string(env, out);
}

/* want_more_out: optional int[1] that receives the total bytes wanted (0 if none). */
JNIEXPORT jstring JNICALL BRIDGE(peekHost)(JNIEnv *env, jclass cls, jbyteArray bytes,
                                           jintArray want_more_out) {
    (void)cls;
    size_t want_more = 0;
    char *host = NULL;
    if (bytes != NULL) {
        jsize len = (*env)->GetArrayLength(env, bytes);
        jbyte *buf = (*env)->GetByteArrayElements(env, bytes, NULL);
        if (buf == NULL) return NULL;
        host = blocker_peek_host((const uint8_t *)buf, (size_t)len, &want_more);
        (*env)->ReleaseByteArrayElements(env, bytes, buf, JNI_ABORT);
    }
    if (want_more_out != NULL && (*env)->GetArrayLength(env, want_more_out) > 0) {
        jint w = want_more > INT32_MAX ? INT32_MAX : (jint)want_more;
        (*env)->SetIntArrayRegion(env, want_more_out, 0, 1, &w);
    }
    return take_optional_string(env, host);
}

/* Returns null if the config parses, else the parse error message. */
JNIEXPORT jstring JNICALL BRIDGE(validateConfig)(JNIEnv *env, jclass cls, jstring toml) {
    (void)cls;
    int failed = 0;
    char *text = utf8_from_jstring(env, toml, &failed);
    if (failed) return NULL;
    bool ok = blocker_config_validate(text);
    free(text);
    if (ok) return NULL;
    char *err = blocker_last_error();
    jstring out = jstring_from_utf8(env, err != NULL ? err : "invalid config");
    blocker_string_free(err);
    return out;
}
