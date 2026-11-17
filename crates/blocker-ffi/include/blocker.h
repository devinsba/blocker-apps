/*
 * blocker.h — C interface to blocker-core.
 *
 * Shared by every client that is not written in Rust:
 *   - iOS / iPadOS (Swift): imported as the `BlockerFFI` Clang module
 *     (see module.modulemap) from BlockerFFI.xcframework.
 *   - Android (Kotlin): wrapped by a small C JNI shim (mobile/android).
 *
 * Ownership rules
 *   - Every `char *` returned by this library is heap-allocated UTF-8 and must
 *     be released with blocker_string_free().
 *   - Every BlockerVerdict must be released with blocker_verdict_free().
 *   - A BlockerEngine is immutable and safe to share across threads; release
 *     it with blocker_engine_free().
 *   - Input strings are borrowed for the duration of the call only.
 *
 * Errors
 *   Functions that can fail return NULL / false and record a message that
 *   blocker_last_error() returns (thread-local, owned by the caller).
 *
 * Keep this file in sync with crates/blocker-ffi/src/lib.rs; the
 * `header_declares_every_export` test fails if an export is missing here.
 */
#ifndef BLOCKER_H
#define BLOCKER_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct BlockerEngine BlockerEngine;

typedef enum {
    BLOCKER_VERDICT_ALLOW = 0,
    BLOCKER_VERDICT_BLOCK = 1,
    /* Hostname unknown: inspect the flow's first bytes and decide again. */
    BLOCKER_VERDICT_PEEK = 2,
} BlockerVerdictKind;

typedef enum {
    BLOCKER_PROTO_TCP = 0,
    BLOCKER_PROTO_UDP = 1,
    BLOCKER_PROTO_OTHER = 2,
} BlockerProto;

typedef struct {
    BlockerVerdictKind kind;
    /* Set only when kind == BLOCKER_VERDICT_BLOCK, otherwise NULL. */
    char *rule;
    char *matched;
} BlockerVerdict;

/* What the OS knows about a new network flow. Any pointer may be NULL. */
typedef struct {
    const char *hostname;
    /* Textual IPv4/IPv6 address. */
    const char *remote_ip;
    uint16_t remote_port;
    BlockerProto proto;
    /* Bundle id / package name of the app that opened the flow. */
    const char *app_id;
    bool ech;
} BlockerFlow;

/* Library version, static string (do not free). */
const char *blocker_version(void);

/* Last error on this thread, or NULL. Caller frees. */
char *blocker_last_error(void);

void blocker_string_free(char *s);
void blocker_verdict_free(BlockerVerdict *verdict);

/* Compile an engine from config.toml text (no downloaded category lists). */
BlockerEngine *blocker_engine_from_toml(const char *config_toml);
/* Compile an engine from a config directory (config.toml + lists/<category>.fst). */
BlockerEngine *blocker_engine_open_dir(const char *dir);
void blocker_engine_free(BlockerEngine *engine);

/* Warnings produced while compiling, as a JSON array of strings. */
char *blocker_engine_warnings_json(const BlockerEngine *engine);

/* day: 0 = Monday ... 6 = Sunday; minute: minutes since local midnight.
 * Pass day = -1 to use the current local time. */
bool blocker_engine_is_idle(const BlockerEngine *engine, int32_t day, uint16_t minute);
/* Names of rules active at the given time, as a JSON array of strings. */
char *blocker_engine_active_rules_json(const BlockerEngine *engine, int32_t day, uint16_t minute);
/* {"bundle_ids": [...], "process_names": [...]} for rules active at the given time. */
char *blocker_engine_blocked_apps_json(const BlockerEngine *engine, int32_t day, uint16_t minute);

/* Would `host` be blocked at the given time? */
BlockerVerdict blocker_engine_check_host(const BlockerEngine *engine, const char *host,
                                         int32_t day, uint16_t minute);

/* Decide a flow. On the second call after a PEEK verdict set `peeked` to true
 * and `peeked_host` to the host found in the first bytes (or NULL if none). */
BlockerVerdict blocker_engine_decide(const BlockerEngine *engine, const BlockerFlow *flow,
                                     bool peeked, const char *peeked_host,
                                     int32_t day, uint16_t minute);

/* Normalize a user-entered domain or URL ("https://WWW.Example.com/x" ->
 * "www.example.com"). NULL if invalid. */
char *blocker_normalize_domain(const char *input);

/* Hostname from the first outbound bytes of a TCP stream (TLS SNI or HTTP
 * Host). Returns NULL if none was found. If more bytes would help,
 * *want_more is set to the total number of bytes wanted (else 0). */
char *blocker_peek_host(const uint8_t *bytes, size_t len, size_t *want_more);

/* Validate config.toml text. Returns true if it parses; else false and sets
 * the last error. */
bool blocker_config_validate(const char *config_toml);

#ifdef __cplusplus
}
#endif

#endif /* BLOCKER_H */
