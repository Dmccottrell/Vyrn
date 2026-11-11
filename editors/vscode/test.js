// Tests the parts of the extension that do not need VS Code:
//   node editors/vscode/test.js    (with `vyrn` built: cargo build)
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { execFileSync } = require("child_process");
const { parseCheckOutput } = require("./diagnostics");

const vyrn = path.join(__dirname, "../../target/debug/vyrn");
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "vyrn-vscode-"));
const file = path.join(dir, "main.vyrn");
fs.writeFileSync(file, "main:\n    let n = 1\n    n = 2\n    print(1 + \"a\")\nend\n");
let stderr = "";
try {
  execFileSync(vyrn, ["check", file], { stdio: "pipe" });
} catch (e) {
  stderr = e.stderr.toString();
}
const problems = parseCheckOutput(stderr);
assert.equal(problems.length, 2, stderr);
assert.equal(problems[0].message, "cannot assign to `n`: it is declared with `let`");
assert.deepEqual([problems[0].line, problems[0].column, problems[0].width], [3, 5, 1]);
assert.equal(problems[0].hint, "declare it with `var n` to change it");
assert.ok(problems[0].file.endsWith("main.vyrn"));
assert.equal(problems[1].message, "cannot apply `+` to int and str");
assert.deepEqual([problems[1].line, problems[1].column, problems[1].width], [4, 11, 7]);

// The grammar is valid JSON and every pattern compiles as a regular expression.
const grammar = JSON.parse(fs.readFileSync(path.join(__dirname, "syntaxes/vyrn.tmLanguage.json"), "utf8"));
let patterns = 0;
(function walk(node) {
  if (Array.isArray(node)) return node.forEach(walk);
  if (node && typeof node === "object") {
    for (const key of ["match", "begin", "end"]) {
      if (typeof node[key] === "string") {
        new RegExp(node[key]);
        patterns++;
      }
    }
    Object.values(node).forEach(walk);
  }
})(grammar);
const config = JSON.parse(fs.readFileSync(path.join(__dirname, "language-configuration.json"), "utf8"));
const increase = new RegExp(config.indentationRules.increaseIndentPattern);
const decrease = new RegExp(config.indentationRules.decreaseIndentPattern);
assert.ok(increase.test("create f(x):") && increase.test("    if x > 1:  # big") && !increase.test("    x = 1"));
assert.ok(decrease.test("end") && decrease.test("    else:") && decrease.test("    is Shape.Circle(r):") && !decrease.test("    ending = 1"));
fs.rmSync(dir, { recursive: true, force: true });
console.log(`ok: diagnostics parsed, ${patterns} grammar patterns compile`);
