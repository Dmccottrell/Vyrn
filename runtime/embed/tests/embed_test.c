/* Exercises the C API in include/vyrn.h. Run by tests/run.sh. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "vyrn.h"

static int failures = 0;
#define CHECK(cond, ...) do { if (!(cond)) { failures++; printf("FAIL line %d: ", __LINE__); printf(__VA_ARGS__); printf("\n"); } } while (0)

static char printed[4096];
static void on_print(void *user, const char *line) {
    (void)user;
    strcat(printed, line);
    strcat(printed, "|");
}

static VyrnEngine *engine;
static double actor_x = 0;
static char answer[256];

/* host.call("move_actor", dx) moves an imaginary actor and gives its new x. */
static const char *move_actor(void *user, const char *args) {
    (void)user;
    double dx = atof(args + 1); /* args is "[dx]" */
    actor_x += dx;
    snprintf(answer, sizeof answer, "{\"x\": %g}", actor_x);
    return answer;
}

static const char *always_fails(void *user, const char *args) {
    (void)user; (void)args;
    vyrn_host_error(engine, "no such actor");
    return NULL;
}

/* Calls back into the engine from inside a host function: must be refused. */
static char reentry_error[512];
static const char *reenter(void *user, const char *args) {
    (void)user; (void)args;
    char *r = vyrn_call(engine, "add", "[1, 2]");
    snprintf(reentry_error, sizeof reentry_error, "%s", r ? "allowed!" : vyrn_last_error(engine));
    vyrn_string_free(r);
    return "1";
}

static const char *script =
    "use vyrn.host\n"
    "object Counter:\n"
    "    n: int = 0\n"
    "end\n"
    "const STATE = Counter()\n"
    "create add(a: int, b: int) -> int: return a + b\n"
    "create bump() -> int:\n"
    "    STATE.n += 1\n"
    "    return STATE.n\n"
    "end\n"
    "create greet(name: str):\n"
    "    print(\"hello, {name}\")\n"
    "    print(\"bye\")\n"
    "end\n"
    "create walk(dx: float) -> float:\n"
    "    let r = host.call(\"move_actor\", dx)\n"
    "    return r[\"x\"]\n"
    "end\n"
    "create broken(): host.call(\"bad\")\n"
    "create info() -> map: return {\"name\": \"door\", \"open\": true, \"angles\": [0, 90]}\n"
    "create boom(): return 1 / 0\n"
    "create again() -> int: return host.call(\"reenter\")\n";

int main(void) {
    engine = vyrn_new();
    CHECK(strlen(vyrn_version()) > 0, "version");
    vyrn_set_print(engine, on_print, NULL);
    vyrn_register(engine, "move_actor", move_actor, NULL);
    vyrn_register(engine, "bad", always_fails, NULL);
    vyrn_register(engine, "reenter", reenter, NULL);

    char *r = vyrn_call(engine, "add", "[1, 2]");
    CHECK(r == NULL, "call before load fails");

    CHECK(vyrn_load_source(engine, "test.vyrn", script) == 0, "load: %s", vyrn_last_error(engine));
    CHECK(vyrn_has_function(engine, "add"), "has add");
    CHECK(!vyrn_has_function(engine, "missing"), "has missing");

    r = vyrn_call(engine, "add", "[1, 2]");
    CHECK(r && strcmp(r, "3") == 0, "add gave %s / %s", r ? r : "NULL", vyrn_last_error(engine));
    vyrn_string_free(r);

    r = vyrn_call(engine, "bump", NULL); vyrn_string_free(r);
    r = vyrn_call(engine, "bump", "[]");
    CHECK(r && strcmp(r, "2") == 0, "state kept between calls: %s", r ? r : vyrn_last_error(engine));
    vyrn_string_free(r);

    r = vyrn_call(engine, "greet", "[\"Ada\"]");
    CHECK(r != NULL, "greet: %s", vyrn_last_error(engine));
    vyrn_string_free(r);
    CHECK(strcmp(printed, "hello, Ada|bye|") == 0, "printed '%s'", printed);

    r = vyrn_call(engine, "walk", "[2.5]"); vyrn_string_free(r);
    r = vyrn_call(engine, "walk", "[1.5]");
    CHECK(r && atof(r) == 4.0 && actor_x == 4.0, "walk gave %s", r ? r : vyrn_last_error(engine));
    vyrn_string_free(r);

    r = vyrn_call(engine, "info", NULL);
    CHECK(r && strstr(r, "\"door\"") && strstr(r, "[0,90]"), "info gave %s", r ? r : vyrn_last_error(engine));
    printf("info -> %s\n", r);
    vyrn_string_free(r);

    r = vyrn_call(engine, "broken", NULL);
    CHECK(r == NULL && strstr(vyrn_last_error(engine), "no such actor"), "host error: %s", vyrn_last_error(engine));
    printf("host error -> %s\n", vyrn_last_error(engine));

    r = vyrn_call(engine, "again", NULL);
    CHECK(r && strcmp(r, "1") == 0, "again: %s", r ? r : vyrn_last_error(engine));
    CHECK(strstr(reentry_error, "already running") != NULL, "re-entry: %s", reentry_error);
    vyrn_string_free(r);
    r = vyrn_call(engine, "add", "[2, 2]");
    CHECK(r && strcmp(r, "4") == 0, "usable after re-entry attempt");
    vyrn_string_free(r);

    r = vyrn_call(engine, "boom", NULL);
    CHECK(r == NULL && strlen(vyrn_last_error(engine)) > 0, "runtime error");
    printf("runtime error -> %s\n", vyrn_last_error(engine));

    r = vyrn_call(engine, "nope", NULL);
    CHECK(r == NULL, "missing function");
    printf("missing -> %s\n", vyrn_last_error(engine));

    r = vyrn_call(engine, "add", "[1,");
    CHECK(r == NULL, "bad json");
    printf("bad json -> %s\n", vyrn_last_error(engine));

    CHECK(vyrn_load_source(engine, "bad.vyrn", "create f(:\n") != 0, "syntax error");
    printf("syntax error -> %s\n", vyrn_last_error(engine));

    /* Hot reload: a new version of the script; host functions survive. */
    FILE *f = fopen("reload_test.vyrn", "w");
    fputs("use vyrn.host\n"
          "create walk(dx: float) -> float: return host.call(\"move_actor\", dx * 10)[\"x\"]\n"
          "main:\n    print(\"main ran\")\nend\n", f);
    fclose(f);
    CHECK(vyrn_load_file(engine, "reload_test.vyrn") == 0, "load file: %s", vyrn_last_error(engine));
    CHECK(!vyrn_has_function(engine, "add"), "old functions gone");
    r = vyrn_call(engine, "walk", "[1]");
    CHECK(r && atof(r) == 14.0, "reloaded walk gave %s", r ? r : vyrn_last_error(engine));
    vyrn_string_free(r);
    printed[0] = 0;
    CHECK(vyrn_run_main(engine) == 0, "main: %s", vyrn_last_error(engine));
    CHECK(strcmp(printed, "main ran|") == 0, "main printed '%s'", printed);
    remove("reload_test.vyrn");

    CHECK(vyrn_load_file(engine, "does_not_exist.vyrn") != 0, "missing file");
    CHECK(vyrn_has_function(engine, "walk"), "failed reload keeps the old script");
    printf("missing file -> %s\n", vyrn_last_error(engine));

    vyrn_free(engine);
    vyrn_free(NULL);
    if (failures) { printf("%d failed\n", failures); return 1; }
    printf("all passed\n");
    return 0;
}
