/*
 * vyrn.h: Vyrn inside other programs.
 *
 * Load Vyrn scripts into a game engine (the Unreal Engine plugin in
 * `unreal/` uses this) or any C or C++ program, call their functions, and
 * let the scripts call back into the program.
 *
 *     VyrnEngine *vyrn = vyrn_new();
 *     vyrn_set_print(vyrn, my_print, NULL);
 *     vyrn_register(vyrn, "log", my_log, NULL);
 *     if (vyrn_load_file(vyrn, "Scripts/door.vyrn") != 0)
 *         puts(vyrn_last_error(vyrn));
 *     char *result = vyrn_call(vyrn, "open", "[90]");   // JSON in, JSON out
 *     ...
 *     vyrn_string_free(result);
 *     vyrn_free(vyrn);
 *
 * Values cross the boundary as JSON text (UTF-8): arguments are a JSON
 * array, results a JSON value. Vyrn numbers, text, lists, maps and structs
 * all have a JSON form.
 *
 * An engine is not thread-safe: use each one from one thread (in Unreal,
 * the game thread). Several engines can run side by side.
 */
#ifndef VYRN_H
#define VYRN_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VyrnEngine VyrnEngine;

/* Receives one line a script printed (without its line break). */
typedef void (*VyrnPrintFn)(void *user, const char *line);

/*
 * A function scripts call with `host.call("name", args...)`. `args_json` is
 * a JSON array of the arguments. Return the result as JSON (or NULL or ""
 * for no result); the text must stay valid until the function returns.
 * To fail, call vyrn_host_error() and return NULL.
 */
typedef const char *(*VyrnHostFn)(void *user, const char *args_json);

/* A new, empty engine. Free it with vyrn_free(). */
VyrnEngine *vyrn_new(void);
void vyrn_free(VyrnEngine *engine);

/* The Vyrn version, such as "0.4.0". */
const char *vyrn_version(void);

/*
 * Loads a script file, with the other .vyrn files of its project that it
 * `use`s. Loading again replaces the script (hot reload), keeping the
 * print function and host functions. Returns 0, or -1 with the reason in
 * vyrn_last_error().
 */
int vyrn_load_file(VyrnEngine *engine, const char *path);

/* Loads a script from text; `name` is used in error messages. */
int vyrn_load_source(VyrnEngine *engine, const char *name, const char *source);

/* Whether the loaded script has a top-level function `name`. */
int vyrn_has_function(VyrnEngine *engine, const char *name);

/*
 * Calls the script's function `name` with the arguments in `args_json` (a
 * JSON array, such as "[1, \"two\"]", or NULL for none). Gives the result
 * as JSON, to free with vyrn_string_free(), or NULL on failure, with the
 * reason in vyrn_last_error().
 */
char *vyrn_call(VyrnEngine *engine, const char *name, const char *args_json);

/* Runs the script's `main:` block, if it has one. 0 or -1. */
int vyrn_run_main(VyrnEngine *engine);

/* Why the last call failed (valid until the next call on this engine). */
const char *vyrn_last_error(VyrnEngine *engine);

/* Frees a string Vyrn gave out. */
void vyrn_string_free(char *text);

/* Where `print` goes. Without one, it goes to standard output. */
void vyrn_set_print(VyrnEngine *engine, VyrnPrintFn print, void *user);

/* Makes `host.call(name, ...)` call `fn` (replacing any earlier one). */
void vyrn_register(VyrnEngine *engine, const char *name, VyrnHostFn fn, void *user);

/* From inside a VyrnHostFn: the reason it failed. */
void vyrn_host_error(VyrnEngine *engine, const char *message);

#ifdef __cplusplus
}
#endif

#endif /* VYRN_H */
