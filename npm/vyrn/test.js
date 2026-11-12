// Checks the npm package with a vyrn built locally (no download):
//   VYRN_BINARY_PATH=../../target/release/vyrn node test.js
"use strict";
const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const built = process.env.VYRN_BINARY_PATH || path.join(__dirname, "../../target/release/vyrn");
assert.ok(fs.existsSync(built), `build vyrn first: ${built}`);
const { target } = require("./install.js");
assert.match(target(), /^(x86_64|aarch64)-/);

const env = { ...process.env, VYRN_BINARY_PATH: built };
const installed = spawnSync(process.execPath, [path.join(__dirname, "install.js")], { env, encoding: "utf8" });
assert.equal(installed.status, 0, installed.stderr);

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "vyrn-npm-"));
const file = path.join(dir, "hello.vyrn");
fs.writeFileSync(file, 'main:\n    print("hello from npm")\nend\n');
const run = spawnSync(process.execPath, [path.join(__dirname, "bin/vyrn.js"), "run", file], { encoding: "utf8" });
assert.equal(run.status, 0, run.stderr);
assert.equal(run.stdout, "hello from npm\n");
const failing = spawnSync(process.execPath, [path.join(__dirname, "bin/vyrn.js"), "check", path.join(dir, "nope.vyrn")], { encoding: "utf8" });
assert.notEqual(failing.status, 0, "exit codes pass through");
console.log("ok: the npm package installs and runs vyrn");
