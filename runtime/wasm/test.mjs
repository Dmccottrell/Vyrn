// Tests the WebAssembly runtime in Node (18 or later):
//
//   cargo build --release --target wasm32-unknown-unknown --manifest-path runtime/wasm/Cargo.toml
//   node runtime/wasm/test.mjs
//
// It runs programs, checks errors, and serves requests from a bundle made
// with `vyrn build --target=wasm32-edge` when VYRN_EDGE_DIR points at one.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { strict as assert } from "node:assert";
import { loadVyrn } from "./vyrn-edge.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const wasm = readFileSync(join(here, "target/wasm32-unknown-unknown/release/vyrn_wasm.wasm"));
assert.deepEqual(WebAssembly.Module.imports(new WebAssembly.Module(wasm)), [], "the module needs no imports");
const vyrn = await loadVyrn(wasm);

let r = vyrn.run('fn fib(n: int) -> int:\n    if n < 2: return n\n    return fib(n - 1) + fib(n - 2)\nend\nmain: print(fib(20), tensor([[1.0, 2.0]]) @ tensor([[3.0], [4.0]]))\n');
assert.deepEqual(r, { ok: true, output: "6765 tensor([[11.0]])\n" });

r = vyrn.run("main:\n    let n = 1\n    n = 2\nend\n");
assert.equal(r.ok, false);
assert.match(r.output, /cannot assign to `n`: it is declared with `let`/);

r = vyrn.run('main:\n    try:\n        print(now())\n    catch Unavailable:\n        print("no clock")\n    end\nend\n');
assert.deepEqual(r, { ok: true, output: "no clock\n" });

if (process.env.VYRN_EDGE_DIR) {
  const dir = process.env.VYRN_EDGE_DIR;
  const program = (await import(join(dir, "program.mjs"))).default;
  const handle = (await loadVyrn(readFileSync(join(dir, "vyrn.wasm")))).serve(program);
  const res = await handle(new Request("https://example.com/users/1"));
  assert.equal(res.status, 200);
  console.log("edge:", res.status, await res.text());
}
console.log("ok");
