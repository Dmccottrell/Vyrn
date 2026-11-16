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
static double health = 100;
static int calls = 0, ai_moves = 0, ai_stops = 0, montages = 0, authority = 1;
static VyrnEngine *current;
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
    else if (!strcmp(name, "get_property")) {
        if (strstr(args, "\"Health\"")) { snprintf(answer, sizeof answer, "%g", health); return answer; }
        vyrn_host_error(current, "no such property");
        return NULL;
    }
    else if (!strcmp(name, "set_property")) return "";
    else if (!strcmp(name, "call_function")) {
        if (strstr(args, "\"TakeHit\"")) { health -= arg(args, 2); calls++; return "null"; }
        snprintf(answer, sizeof answer, "%s", args);
        return answer;
    }
    else if (!strcmp(name, "ai_move_to")) { ai_moves++; return "true"; }
    else if (!strcmp(name, "ai_stop")) { ai_stops++; return ""; }
    else if (!strcmp(name, "ai_moving")) return "false";
    else if (!strcmp(name, "play_montage")) { montages++; return "0.8"; }
    else if (!strcmp(name, "stop_montage") || !strcmp(name, "set_replicates") || !strcmp(name, "set_rotation_of")) return "";
    else if (!strcmp(name, "has_authority")) return authority ? "true" : "false";
    else if (!strcmp(name, "net_mode")) return "\"listen_server\"";
    else if (!strcmp(name, "action")) return "[0.5,1]";
    else if (!strcmp(name, "add_input_context")) return "true";
    else if (!strcmp(name, "class_of")) return "\"BP_Enemy_C\"";
    else if (!strcmp(name, "tags_of")) return "[\"enemy\"]";
    else if (!strcmp(name, "find_class")) return "[\"BP_Enemy_C_1\",\"BP_Enemy_C_2\"]";
    else if (!strcmp(name, "rotation_of")) return "{\"pitch\":0,\"yaw\":45,\"roll\":0}";
    else if (!strcmp(name, "set_location_of")) return "true";
    else if (!strcmp(name, "set_var") || !strcmp(name, "log") || !strcmp(name, "warn")
             || !strcmp(name, "error") || !strcmp(name, "screen")) return "";
    else return NULL;
    return answer;
}

static const char *NAMES[] = {"log", "warn", "error", "screen", "event", "owner", "get_location",
    "set_location", "move", "get_rotation", "set_rotation", "rotate", "forward", "right", "time",
    "spawn", "find", "player", "location_of", "destroy", "trace", "key_down", "get_var", "set_var",
    "get_property", "set_property", "call_function", "ai_move_to", "ai_stop", "ai_moving",
    "play_montage", "stop_montage", "has_authority", "net_mode", "set_replicates", "action",
    "add_input_context", "class_of", "tags_of", "find_class", "rotation_of", "set_rotation_of",
    "set_location_of"};

static void quiet(void *user, const char *line) { (void)user; printf("  | %s\n", line); }

static VyrnEngine *start(const char *dir, const char *file) {
    VyrnEngine *e = vyrn_new();
    current = e;
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

    /* enemy: chases while far, then hits on a cooldown, on the server only. */
    me[0] = 0; me[1] = 0; me[2] = 100; player[0] = 1000; player[1] = 0; events = 0;
    e = start(dir, "enemy.vyrn");
    for (int i = 0; i < 5; i++) { now += 0.1; call(e, "tick", "[0.1]"); }
    CHECK(ai_moves == 5 && calls == 0, "enemy chases: %d moves, %d hits", ai_moves, calls);
    player[0] = 100;
    for (int i = 0; i < 30; i++) { now += 0.1; call(e, "tick", "[0.1]"); }
    /* Three seconds near: hits at 0, 1.2 and 2.4 seconds. */
    CHECK(calls == 3 && health == 70, "enemy hits: %d hits, health %g", calls, health);
    CHECK(events == 3 && strstr(last_event, "enemy_attacked"), "enemy events: %d %s", events, last_event);
    authority = 0;
    int before = calls;
    for (int i = 0; i < 30; i++) { now += 0.1; call(e, "tick", "[0.1]"); }
    CHECK(calls == before, "clients don't decide: %d hits", calls - before);
    authority = 1;
    vyrn_free(e);

    /* The rest of unreal.vyrn, through a test script. */
    e = vyrn_new();
    current = e;
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
          "create deep() -> list:\n"
          "    return [unreal.get(\"BP_Hero\", \"Health\"), unreal.call(\"self\", \"Greet\", \"hi\", 2), unreal.action(\"/Game/IA_Move.IA_Move\"),\n"
          "            unreal.net_mode(), unreal.has_authority(), unreal.class_of(\"x\"), unreal.tags_of(\"x\"), len(unreal.find_class(\"/Game/BP_Enemy.BP_Enemy_C\")),\n"
          "            str(unreal.rotation_of(\"x\")), unreal.add_input_context(\"/Game/IMC.IMC\"), unreal.ai_moving(\"x\")]\n"
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
    r = vyrn_call(e, "deep", NULL);
    CHECK(r != NULL, "deep: %s", vyrn_last_error(e));
    if (r) {
        printf("deep -> %s\n", r);
        CHECK(strstr(r, "[\"self\",\"Greet\",\"hi\",2]") != NULL, "call passes its arguments: %s", r);
        CHECK(strstr(r, "[0.5,1],\"listen_server\",true,\"BP_Enemy_C\",[\"enemy\"],2,\"(pitch 0.0, yaw 45.0, roll 0.0)\",true,false]") != NULL, "deep values: %s", r);
    }
    vyrn_string_free(r);
    remove(path);
    vyrn_free(e);

    if (failures) { printf("%d failed\n", failures); return 1; }
    printf("all passed\n");
    return 0;
}
