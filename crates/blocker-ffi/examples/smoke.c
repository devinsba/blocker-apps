/* Links blocker.h against libblocker_ffi.a from plain C. Run scripts/test-ffi-c.sh. */
#include <stdio.h>
#include <string.h>
#include "blocker.h"

int main(void) {
    const char *toml = "[[rule]]\nname = \"focus\"\ndomains = [\"example.com\"]\n";
    BlockerEngine *engine = blocker_engine_from_toml(toml);
    if (!engine) {
        char *err = blocker_last_error();
        fprintf(stderr, "error: %s\n", err ? err : "?");
        blocker_string_free(err);
        return 1;
    }
    BlockerVerdict v = blocker_engine_check_host(engine, "www.example.com", -1, 0);
    int ok = v.kind == BLOCKER_VERDICT_BLOCK && v.rule && strcmp(v.rule, "focus") == 0;
    printf("blocker %s: www.example.com -> %s (rule %s)\n", blocker_version(),
           v.kind == BLOCKER_VERDICT_BLOCK ? "block" : "allow", v.rule ? v.rule : "-");
    blocker_verdict_free(&v);
    blocker_engine_free(engine);
    return ok ? 0 : 1;
}
