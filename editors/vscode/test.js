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

// The debugger contribution matches what `vyrn debug --dap` serves.
const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, "package.json"), "utf8"));
const debuggerEntry = manifest.contributes.debuggers.find((d) => d.type === "vyrn");
assert.ok(debuggerEntry && debuggerEntry.configurationAttributes.launch.required.includes("program"));
assert.ok(manifest.contributes.breakpoints.some((b) => b.language === "vyrn"));
const dap = require("child_process").spawnSync(vyrn, ["debug", "--dap"], {
  input: (() => {
    const m = JSON.stringify({ seq: 1, type: "request", command: "initialize", arguments: {} });
    const d = JSON.stringify({ seq: 2, type: "request", command: "disconnect" });
    return `Content-Length: ${m.length}\r\n\r\n${m}Content-Length: ${d.length}\r\n\r\n${d}`;
  })(),
});
assert.ok(dap.stdout.toString().includes('"supportsConfigurationDoneRequest":true'), dap.stdout.toString());

console.log(`ok: diagnostics parsed, ${patterns} grammar patterns compile`);

// The language server answers `initialize` and reports errors on open.
const { spawn } = require("child_process");
const lsp = spawn(vyrn, ["lsp"]);
let buffer = Buffer.alloc(0);
const replies = [];
lsp.stdout.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const head = buffer.indexOf("\r\n\r\n");
    if (head < 0) break;
    const length = Number(/Content-Length: (\d+)/.exec(buffer.slice(0, head).toString())[1]);
    if (buffer.length < head + 4 + length) break;
    replies.push(JSON.parse(buffer.slice(head + 4, head + 4 + length).toString()));
    buffer = buffer.slice(head + 4 + length);
  }
});
const send = (message) => {
  const body = JSON.stringify({ jsonrpc: "2.0", ...message });
  lsp.stdin.write(`Content-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`);
};
send({ id: 1, method: "initialize", params: {} });
send({
  method: "textDocument/didOpen",
  params: { textDocument: { uri: "file://" + file, text: "main:\n    print(nope)\nend\n" } },
});
send({ id: 2, method: "textDocument/hover", params: { textDocument: { uri: "file://" + file }, position: { line: 1, character: 5 } } });
setTimeout(() => {
  lsp.kill();
  const init = replies.find((r) => r.id === 1);
  assert.ok(init.result.capabilities.completionProvider, "the server offers completion");
  const diagnostics = replies.find((r) => r.method === "textDocument/publishDiagnostics");
  assert.ok(diagnostics.params.diagnostics[0].message.startsWith("`nope` is not defined"));
  const hover = replies.find((r) => r.id === 2);
  assert.ok(hover.result.contents.value.includes("print"), "hover documents print");
  fs.rmSync(dir, { recursive: true, force: true });
  console.log("ok: language server answers initialize, diagnostics and hover");
}, 1500);
