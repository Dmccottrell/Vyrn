// Builds the Learn Vyrn website.
//
//   vyrn learn export build/course
//   node site/build.mjs build/course/course.json runtime/wasm/target/wasm32-unknown-unknown/release/vyrn_wasm.wasm build/site
//   node site/build.mjs ... build/site --single learn-vyrn.html   (also one self-contained page)
//
// Every example and every exercise's solution is run through the same
// WebAssembly Vyrn the site uses. Examples that need something a browser
// doesn't have (a window, a database, a clock) are marked so the site
// doesn't offer to run them; exercises are marked `browser: false` with
// what they need. A solution that runs in the browser but prints the wrong
// thing fails the build: the site would tell learners their right answer
// is wrong.
import { readFileSync, writeFileSync, mkdirSync, copyFileSync, readdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Worker } from "node:worker_threads";
import { loadVyrn } from "../runtime/wasm/vyrn-edge.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const args = process.argv.slice(2);
const singleAt = args.indexOf("--single");
const single = singleAt >= 0 ? args.splice(singleAt, 2)[1] : null;
const [coursePath, wasmPath, out] = args;
if (!coursePath || !wasmPath || !out) {
  console.error("usage: node site/build.mjs course.json vyrn_wasm.wasm out-dir [--single page.html]");
  process.exit(2);
}

const course = JSON.parse(readFileSync(coursePath, "utf8"));
const wasm = readFileSync(wasmPath);
let vyrn = await loadVyrn(wasm);

async function run(source, input) {
  try {
    return vyrn.run(source, input ?? "");
  } catch (e) {
    vyrn = await loadVyrn(wasm); // a trap leaves the instance unusable
    return { ok: false, output: `error: Crashed: ${e.message}\n` };
  }
}

function needs(output) {
  if (/vyrn\.draw|drawing/.test(output)) return "drawing in a window (vyrn.draw)";
  if (/db\.open|databases/.test(output)) return "a database (vyrn.db)";
  if (/no clock/.test(output)) return "the clock";
  if (/PNG/.test(output)) return "PNG images";
  if (/time zones/.test(output)) return "named time zones";
  if (/network|http/i.test(output)) return "the network";
  return "Vyrn on your computer";
}

const clean = (s) => {
  const l = s.split("\n").map((x) => x.replace(/\s+$/, ""));
  while (l.length && l[l.length - 1] === "") l.pop();
  return l.join("\n");
};

// "vyrn check" programs aren't run by the course's tests (they may open a
// window, serve forever or need the network), so here each gets a few
// seconds in a worker; those that finish cleanly get a Run button.
const WORKER = `
const { parentPort } = require("node:worker_threads");
let instance = null;
const enc = new TextEncoder(), dec = new TextDecoder();
parentPort.on("message", async ({ module, source }) => {
  if (!instance) instance = await WebAssembly.instantiate(module, {});
  const x = instance.exports;
  x.vyrn_set_time(Date.now() / 1000);
  const seed = new Uint8Array(32).fill(7), sp = x.vyrn_alloc(32);
  new Uint8Array(x.memory.buffer, sp, 32).set(seed); x.vyrn_seed(sp, 32); x.vyrn_free(sp, 32);
  const bytes = enc.encode(source + "\\0"), ptr = x.vyrn_alloc(bytes.length);
  new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
  const reply = x.vyrn_run_input(ptr, bytes.length);
  const len = new DataView(x.memory.buffer).getUint32(reply, true);
  const out = new Uint8Array(x.memory.buffer, reply + 4, len).slice();
  x.vyrn_free(reply, len + 4);
  parentPort.postMessage({ ok: out[0] === 0, output: dec.decode(out.subarray(1)) });
});`;
const wasmModule = await WebAssembly.compile(wasm);
let worker = null;
function runLimited(source, ms = 4000) {
  if (!worker) worker = new Worker(WORKER, { eval: true });
  const w = worker;
  return new Promise((resolve) => {
    const done = (r) => { clearTimeout(timer); w.removeAllListeners("message"); w.removeAllListeners("error"); resolve(r); };
    const timer = setTimeout(() => { w.terminate(); worker = null; done({ ok: false, output: "timeout" }); }, ms);
    w.once("message", done);
    w.once("error", (e) => { worker = null; done({ ok: false, output: "error: Crashed: " + e.message }); });
    w.postMessage({ module: wasmModule, source });
  });
}

let examples = 0, offlineExamples = 0, exercises = 0, offlineExercises = 0, checked = 0, checkedRun = 0;
const wrong = [];
for (const day of course.days) {
  for (const lesson of day.lessons) {
    const desktop = [];
    const fences = [...lesson.text.matchAll(/^```vyrn\n([\s\S]*?)^```$/gm)];
    for (const [i, f] of fences.entries()) {
      examples++;
      const r = await run(f[1]);
      if (/error: (Unavailable|Crashed)/.test(r.output)) {
        desktop.push(i);
        offlineExamples++;
      }
    }
    if (desktop.length) lesson.desktop = desktop;
    const runnable = [];
    const programs = [...lesson.text.matchAll(/^```vyrn check\n([\s\S]*?)^```$/gm)];
    for (const [i, f] of programs.entries()) {
      checked++;
      const r = await runLimited(f[1]);
      if (r.ok && !/error: (Unavailable|Crashed)/.test(r.output)) {
        runnable.push(i);
        checkedRun++;
      } else if (process.env.VYRN_SITE_DEBUG) {
        console.log(lesson.id, (r.output.match(/error: [^\n]*/) || [r.output.slice(0, 80)])[0].slice(0, 110));
      }
    }
    if (runnable.length) lesson.runnable = runnable;
    const ex = lesson.exercise;
    if (ex && ex.solution) {
      exercises++;
      const r = await run(ex.solution, ex.input);
      if (r.ok && clean(r.output) === clean(ex.expect)) continue;
      if (/error: (Unavailable|Crashed)/.test(r.output)) {
        ex.browser = false;
        ex.needs = needs(r.output);
        offlineExercises++;
      } else {
        wrong.push(`${lesson.id}: ${r.output.split("\n").find((l) => l.startsWith("error")) || "different output"}`);
      }
    }
  }
}

console.log(`examples: ${examples - offlineExamples} run in the browser, ${offlineExamples} need a computer`);
console.log(`exercises: ${exercises - offlineExercises} checked in the browser, ${offlineExercises} need a computer`);
console.log(`other programs: ${checkedRun} of ${checked} also run in the browser`);
if (worker) await worker.terminate();
if (wrong.length) {
  console.error(`these solutions give the wrong output in the browser:\n  ${wrong.join("\n  ")}`);
  process.exit(1);
}

mkdirSync(out, { recursive: true });
const json = JSON.stringify(course);
writeFileSync(join(out, "course.json"), json);
copyFileSync(wasmPath, join(out, "vyrn.wasm"));
for (const f of ["index.html", "style.css", "app.js", "config.js", "package.json", "vercel.json"]) copyFileSync(join(here, f), join(out, f));

// The server functions (Vercel: api/), with Vyrn's syntax for the AI tutor.
mkdirSync(join(out, "api"), { recursive: true });
for (const f of readdirSync(join(here, "api"))) copyFileSync(join(here, "api", f), join(out, "api", f));
const syntax = readFileSync(join(here, "..", "docs", "syntax.md"), "utf8");
writeFileSync(join(out, "api", "_syntax.js"), `// Made by site/build.mjs from docs/syntax.md.\nexport const SYNTAX = ${JSON.stringify(syntax)};\n`);

// The reference documentation, for the Docs pages.
const DOCS = [
  ["tutorial", "Tutorial", "docs/tutorial.md", "A tour of the language in one sitting."],
  ["syntax", "Syntax", "docs/syntax.md", "Every construct at a glance."],
  ["stdlib", "Standard library", "docs/stdlib.md", "The standard library, module by module."],
  ["packages", "Packages", "docs/packages.md", "Projects, packages and vyrn.toml."],
  ["debugging", "Debugging", "docs/debugging.md", "Finding bugs: the debugger and VS Code."],
  ["nn", "Tensors and the GPU", "docs/nn.md", "Tensors, models and the GPU."],
  ["ffi", "C and Python", "docs/ffi.md", "Calling C and Python."],
  ["deploy", "Deploying", "docs/deploy.md", "Building and deploying programs."],
  ["unreal", "Unreal Engine", "unreal/README.md", "Scripting Unreal Engine 5 with Vyrn."],
  ["principles", "Principles", "docs/principles.md", "Why Vyrn is the way it is."],
  ["architecture", "Architecture", "docs/architecture.md", "How the toolchain works inside."],
];
const docs = DOCS.map(([slug, title, path, blurb]) => {
  const text = readFileSync(join(here, "..", path), "utf8");
  return { slug, title, blurb, text };
});
const docsJson = JSON.stringify(docs);
writeFileSync(join(out, "docs.json"), docsJson);

// The package registry's index, which `vyrn search` and `vyrn add` read.
mkdirSync(join(out, "registry"), { recursive: true });
copyFileSync(join(here, "..", "registry", "index.json"), join(out, "registry", "index.json"));
console.log(`wrote ${out}`);

if (single) {
  // One page with everything inside: the course, the styles, the script and
  // Vyrn itself, for hosts that take a single file.
  const page = readFileSync(join(here, "index.html"), "utf8")
    .replace('<link rel="stylesheet" href="style.css">', () => `<style>\n${readFileSync(join(here, "style.css"), "utf8")}</style>`)
    .replace('<script type="application/json" id="course-data"></script>', () => `<script type="application/json" id="course-data">${json.replace(/<\//g, "<\\/")}</script>`)
    .replace('<script type="application/octet-stream" id="vyrn-wasm"></script>', () => `<script type="application/json" id="docs-data">${docsJson.replace(/<\//g, "<\\/")}</script><script type="application/octet-stream" id="vyrn-wasm"></script>`)
    .replace('<script type="application/octet-stream" id="vyrn-wasm"></script>', () => `<script type="application/octet-stream" id="vyrn-wasm">${wasm.toString("base64")}</script>`)
    .replace('<script src="config.js"></script>', () => `<script>\n${readFileSync(join(here, "config.js"), "utf8")}</script>`)
    .replace('<script src="app.js"></script>', () => `<script>\n${readFileSync(join(here, "app.js"), "utf8")}</script>`);
  writeFileSync(single, page);
  console.log(`wrote ${single} (${(page.length / 1e6).toFixed(1)} MB)`);
}
