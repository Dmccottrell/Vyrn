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
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
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

let examples = 0, offlineExamples = 0, exercises = 0, offlineExercises = 0;
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
if (wrong.length) {
  console.error(`these solutions give the wrong output in the browser:\n  ${wrong.join("\n  ")}`);
  process.exit(1);
}

mkdirSync(out, { recursive: true });
const json = JSON.stringify(course);
writeFileSync(join(out, "course.json"), json);
copyFileSync(wasmPath, join(out, "vyrn.wasm"));
for (const f of ["index.html", "style.css", "app.js"]) copyFileSync(join(here, f), join(out, f));
console.log(`wrote ${out}`);

if (single) {
  // One page with everything inside: the course, the styles, the script and
  // Vyrn itself, for hosts that take a single file.
  const page = readFileSync(join(here, "index.html"), "utf8")
    .replace('<link rel="stylesheet" href="style.css">', () => `<style>\n${readFileSync(join(here, "style.css"), "utf8")}</style>`)
    .replace('<script type="application/json" id="course-data"></script>', () => `<script type="application/json" id="course-data">${json.replace(/<\//g, "<\\/")}</script>`)
    .replace('<script type="application/octet-stream" id="vyrn-wasm"></script>', () => `<script type="application/octet-stream" id="vyrn-wasm">${wasm.toString("base64")}</script>`)
    .replace('<script src="app.js"></script>', () => `<script>\n${readFileSync(join(here, "app.js"), "utf8")}</script>`);
  writeFileSync(single, page);
  console.log(`wrote ${single} (${(page.length / 1e6).toFixed(1)} MB)`);
}
