// Vyrn on WebAssembly hosts: browsers, Vercel Edge Functions, Cloudflare
// Workers, Deno Deploy, Bun and Node. Standard JavaScript only: the module
// needs no imports, and requests and responses are the web `Request` and
// `Response` types every one of these platforms provides.
//
//   const vyrn = await loadVyrn(wasmBytesOrModule);
//   vyrn.run(source)                    -> { ok, output }        (the playground)
//   const handle = vyrn.serve(bundle)   -> (request) => Response (an edge API)

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export async function loadVyrn(wasm) {
  const module = wasm instanceof WebAssembly.Module ? wasm : await WebAssembly.compile(wasm);
  const instance = await WebAssembly.instantiate(module, {});
  const x = instance.exports;

  // Copies bytes into Vyrn's memory, calls `fn`, and returns the reply.
  function call(fn, bytes) {
    const ptr = x.vyrn_alloc(bytes.length);
    new Uint8Array(x.memory.buffer, ptr, bytes.length).set(bytes);
    const reply = x[fn](ptr, bytes.length);
    x.vyrn_free(ptr, bytes.length);
    const len = new DataView(x.memory.buffer).getUint32(reply, true);
    const out = new Uint8Array(x.memory.buffer, reply + 4, len).slice();
    x.vyrn_free(reply, len + 4);
    return out;
  }

  function status(bytes) {
    return { ok: bytes[0] === 0, output: decoder.decode(bytes.subarray(1)) };
  }

  return {
    /** Compiles and runs a program's `main:`; returns what it printed. */
    run(source) {
      return status(call("vyrn_run", encoder.encode(source)));
    },

    /**
     * Loads a program bundle from `vyrn build --target=wasm32-edge` and
     * returns a fetch handler serving its routes.
     */
    serve(bundle) {
      const loaded = status(call("vyrn_load", bundle));
      if (!loaded.ok) throw new Error(loaded.output);
      if (loaded.output) console.log(loaded.output.trimEnd());
      return async function handle(request) {
        const url = new URL(request.url);
        const body = request.body ? new Uint8Array(await request.arrayBuffer()) : new Uint8Array();
        const head = encoder.encode(
          [request.method, url.pathname, url.search.replace(/^\?/, ""), request.headers.get("content-type") ?? ""].join("\0") + "\0",
        );
        const message = new Uint8Array(head.length + body.length);
        message.set(head);
        message.set(body, head.length);
        const reply = call("vyrn_handle", message);
        const code = reply[0] | (reply[1] << 8);
        const end = reply.indexOf(0, 2);
        const type = decoder.decode(reply.subarray(2, end));
        const payload = reply.subarray(end + 1);
        return new Response(code === 204 ? null : payload, {
          status: code,
          headers: { "content-type": type },
        });
      };
    },
  };
}
