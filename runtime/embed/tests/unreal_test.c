/*
 * Runs the Unreal plugin's example scripts (unreal/VyrnScripting/Scripts)
 * against a fake Unreal: the same host functions the plugin registers,
 * answering from a tiny pretend world. Checks that unreal.vyrn and the
 * examples use the host functions correctly. Run by tests/run.sh.
 */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "vyrn.h"

static int failures = 0;
#define CHECK(cond, ...) do { if (!(cond)) { failures++; printf("FAIL line %d: ", __LINE__); printf(__VA_ARGS__); printf("\n"); } } while (0)

/* The pretend world: our actor, and a player that walks past it. */
static double me[3] = {0, 0, 100}, rot[3] = {0, 0, 0};
static double player[3] = {1000, 0, 100};
static double now = 0;
static int spawned = 0, events = 0;
static char answer[512], last_event[128];

static double arg(const char *args, int n) {
    const char *p = args + 1;
    for (int i = 0; i < n; i++) { p = strchr(p, ','); if (!p) return 0; p++; }
    return atof(p);
}

static const char *host(void *user, const char *args) {
    const char *name = (const char *)user;
    if (!strcmp(name, "get_location")) snprintf(answer, sizeof answer, "{\"x\":%g,\"y\":%g,\"z\":%g}", me[0], me[1], me[2]);
    else if (!strcmp(name, "set_location")) { for (int i = 0; i < 3; i++) me[i] = arg(args, i); return "true"; }
    else if (!strcmp(name, "move")) { for (int i = 0; i < 3; i++) me[i] += arg(args, i); return "true"; }
    else if (!strcmp(name, "get_rotation")) snprintf(answer, sizeof answer, "{\"pitch\":%g,\"yaw\":%g,\"roll\":%g}", rot[0], rot[1], rot[2]);
    else if (!strcmp(name, "set_rotation")) { for (int i = 0; i < 3; i++) rot[i] = arg(args, i); return ""; }
    else if (!strcmp(name, "rotate")) { for (int i = 0; i < 3; i++) rot[i] += arg(args, i); return ""; }
    else if (!strcmp(name, "forward")) {
        double y = rot[1] * M_PI / 180;
        snprintf(answer, sizeof answer, "{\"x\":%g,\"y\":%g,\"z\":0}", cos(y), sin(y));
    }
    else if (!strcmp(name, "right")) return "{\"x\":0,\"y\":1,\"z\":0}";
    else if (!strcmp(name, "time")) snprintf(answer, sizeof answer, "%g", now);
    else if (!strcmp(name, "owner")) return "\"BP_Thing_1\"";
    else if (!strcmp(name, "player")) return "\"PlayerPawn_0\"";
    else if (!strcmp(name, "location_of")) snprintf(answer, sizeof answer, "{\"x\":%g,\"y\":%g,\"z\":%g}", player[0], player[1], player[2]);
    else if (!strcmp(name, "find")) return "[\"Target_1\",\"Target_2\"]";
    else if (!strcmp(name, "destroy")) return "true";
    else if (!strcmp(name, "trace")) return "{\"actor\":\"PlayerPawn_0\",\"point\":{\"x\":1,\"y\":2,\"z\":3},\"normal\":{\"x\":-1,\"y\":0,\"z\":0},\"distance\":250}";
    else if (!strcmp(name, "spawn")) { spawned++; return "\"BP_Projectile_C_1\""; }
    else if (!strcmp(name, "key_down")) return strstr(args, "\"W\"") ? "true" : "false";
    else if (!strcmp(name, "event")) { events++; snprintf(last_event, sizeof last_event, "%s", args); return ""; }
    else if (!strcmp(name, "get_var")) {
        if (strstr(args, "projectile")) return "\"/Game/BP_Projectile.BP_Projectile_C\"";
        if (strstr(args, "speed")) return "180";
        return "null";
    }
    else if (!strcmp(name, "set_var") || !strcmp(name, "log") || !strcmp(name, "warn")
             || !strcmp(name, "error") || !strcmp(name, "screen")) return "";
    else return NULL;
    return answer;
}

static const char *NAMES[] = {"log", "warn", "error", "screen", "event", "owner", "get_location",
    "set_location", "move", "get_rotation", "set_rotation", "rotate", "forward", "right", "time",
    "spawn", "find", "player", "location_of", "destroy", "trace", "key_down", "get_var", "set_var"};

static void quiet(void *user, const char *line) { (void)user; printf("  | %s\n", line); }

static VyrnEngine *start(const char *dir, const char *file) {
    VyrnEngine *e = vyrn_new();
    vyrn_set_print(e, quiet, NULL);
    for (size_t i = 0; i < sizeof NAMES / sizeof *NAMES; i++)
        vyrn_register(e, NAMES[i], host, (void *)NAMES[i]);
    char path[1024];
    snprintf(path, sizeof path, "%s/%s", dir, file);
    if (vyrn_load_file(e, path) != 0) { printf("FAIL loading %s: %s\n", file, vyrn_last_error(e)); failures++; }
    return e;
}

static int call(VyrnEngine *e, const char *name, const char *args) {
    if (!vyrn_has_function(e, name)) return 0;
    char *r = vyrn_call(e, name, args);
    if (!r) { printf("FAIL %s: %s\n", name, vyrn_last_error(e)); failures++; return 0; }
    vyrn_string_free(r);
    return 1;
}

int main(int argc, char **argv) {
    const char *dir = argc > 1 ? argv[1] : "../../../../unreal/VyrnScripting/Scripts";

    /* spinner: turns at the "speed" variable (180 degrees a second). */
    VyrnEngine *e = start(dir, "spinner.vyrn");
    call(e, "begin_play", NULL);
    for (int i = 0; i < 10; i++) { now += 0.1; call(e, "tick", "[0.1]"); }
    CHECK(fabs(rot[1] - 180) < 0.001, "spinner yaw %g", rot[1]);
    CHECK(fabs(me[2] - 100) < 20.001, "spinner bobs near its start: z %g", me[2]);
    vyrn_free(e);

    /* door: opens when the player is near, and says so. */
    me[0] = me[1] = 0; me[2] = 100;
    e = start(dir, "door.vyrn");
    call(e, "begin_play", NULL);
    call(e, "tick", "[0.1]");
    CHECK(me[2] == 100 && events == 0, "door stays shut while the player is far: z %g", me[2]);
    player[0] = 100;
    for (int i = 0; i < 20; i++) call(e, "tick", "[0.1]");
    CHECK(fabs(me[2] - 350) < 0.001, "door opened to %g", me[2]);
    CHECK(events == 1 && strstr(last_event, "door_opened"), "door event %d %s", events, last_event);
    player[0] = 5000;
    for (int i = 0; i < 20; i++) call(e, "tick", "[0.1]");
    CHECK(fabs(me[2] - 100) < 0.001 && strstr(last_event, "door_closed"), "door closed: z %g, %s", me[2], last_event);
    vyrn_free(e);

    /* turret: turns towards the player and fires. */
    me[0] = me[1] = 0; rot[1] = 0; player[0] = 0; player[1] = 1000; events = 0;
    e = start(dir, "turret.vyrn");
    call(e, "begin_play", NULL);
    for (int i = 0; i < 60; i++) call(e, "tick", "[0.05]");
    CHECK(fabs(rot[1] - 90) < 2.001, "turret faces the player: yaw %g", rot[1]);
    CHECK(spawned >= 1, "turret fired %d times", spawned);
    CHECK(events == spawned && strstr(last_event, "turret_fired") && strstr(last_event, "PlayerPawn_0"), "turret events %s", last_event);
    vyrn_free(e);

    /* The rest of unreal.vyrn, through a test script. */
    e = vyrn_new();
    for (size_t i = 0; i < sizeof NAMES / sizeof *NAMES; i++)
        vyrn_register(e, NAMES[i], host, (void *)NAMES[i]);
    vyrn_set_print(e, quiet, NULL);
    char path[1024];
    snprintf(path, sizeof path, "%s/unreal_check.vyrn", dir);
    FILE *f = fopen(path, "w");
    fputs("use unreal\n"
          "create check() -> list:\n"
          "    let h = unreal.trace(unreal.Vec3(), unreal.Vec3(100.0, 0.0, 0.0))\n"
          "    if h == none: return []\n"
          "    return [unreal.me(), unreal.find(\"enemy\"), unreal.key_down(\"W\"), unreal.key_down(\"S\"),\n"
          "            h.actor, h.distance, str(h.normal), unreal.time() >= 0, unreal.destroy(\"x\"),\n"
          "            unreal.get_var(\"missing\", 7), unreal.forward().length(), unreal.distance_to(\"p\") > 0,\n"
          "            str(unreal.Vec3(3.0, 4.0, 0.0).normalized()), unreal.spawn_actor(\"/Script/Engine.PointLight\", unreal.Vec3()),\n"
          "            unreal.move(unreal.Vec3(1.0, 0.0, 0.0)), str(unreal.rotation())]\n"
          "end\n"
          "create chatter():\n"
          "    unreal.log(\"hi\")\n"
          "    unreal.warn(1)\n"
          "    unreal.error([2])\n"
          "    unreal.screen(\"on screen\", 3.0, \"red\")\n"
          "    unreal.set_var(\"score\", 10)\n"
          "    unreal.event(\"scored\", {\"points\": 10})\n"
          "    unreal.set_rotation(unreal.Rotator(0.0, 45.0, 0.0))\n"
          "    unreal.rotate(yaw = 5.0)\n"
          "end\n", f);
    fclose(f);
    if (vyrn_load_file(e, path) != 0) { printf("FAIL: %s\n", vyrn_last_error(e)); failures++; }
    char *r = vyrn_call(e, "check", NULL);
    CHECK(r != NULL, "check: %s", vyrn_last_error(e));
    if (r) {
        printf("check -> %s\n", r);
        CHECK(strstr(r, "\"BP_Thing_1\",[\"Target_1\",\"Target_2\"],true,false,\"PlayerPawn_0\",250"), "check values");
        CHECK(strstr(r, "\"(0.6") && strstr(r, ", 0.8, 0.0)\""), "normalized");
    }
    vyrn_string_free(r);
    CHECK(call(e, "chatter", NULL) && rot[1] == 50 && strstr(last_event, "scored"), "chatter: yaw %g", rot[1]);
    remove(path);
    vyrn_free(e);

    if (failures) { printf("%d failed\n", failures); return 1; }
    printf("all passed\n");
    return 0;
}
