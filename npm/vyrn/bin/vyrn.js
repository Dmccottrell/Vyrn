#!/usr/bin/env node
// The `vyrn` command installed by npm: runs the real program, downloading
// it first if the install step was skipped (as with `--ignore-scripts`).
"use strict";
const fs = require("fs");
const { spawnSync } = require("child_process");
const { install, binaryPath } = require("../install.js");

async function main() {
  const binary = binaryPath();
  if (!fs.existsSync(binary)) await install();
  const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) throw result.error;
  process.exit(result.status === null ? 1 : result.status);
}

main().catch((e) => {
  console.error(`vyrn: ${e.message}`);
  process.exit(1);
});
