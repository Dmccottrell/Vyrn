# `std.nn` Kernels: Vyrn Code on the GPU

Status: element-wise kernels **run today** on the CPU and, with
`--features gpu`, as generated WGSL compute shaders (`src/vm/kernel.rs`,
`src/vm/gpu.rs`). Tile kernels (section 4), which a custom attention
mechanism needs, are design. Each section says which applies.

## 1. The model a developer learns

A `kernel` function is written on numbers. Called with numbers, it is an ordinary
function. Called with tensors, it runs once per element, on whichever
device the tensors live on. That is all there is to learn (rule 2, the
five-minute test):

```vyrn
const BETA = 1.5

## Swish with a sharpness: x * sigmoid(beta * x).
kernel create swish(x: f32, beta: f32) -> f32:
    let gate = 1.0 / (1.0 + exp(-beta * x))
    return x * gate
end

kernel create mix(a: f32, b: f32, t: f32) -> f32:
    return a + (b - a) * t
end

main:
    print(swish(1.0, BETA))                     # a number: 0.8175744761936437

    on gpu[0]:
        let h = rand(4096, 4096) * 8.0 - 4.0
        let y = swish(h, BETA)                  # one compute shader, 16.7M threads
        let z = mix(h, y, 0.25)                 # a second one
        print(z.shape)
    end
end
```

Arguments may mix tensors (all of one shape, on one device) and numbers
(the same for every element). The checker knows the rule: an `f32`
parameter accepts a tensor, and the call's result is then a tensor.

A kernel body is `let` bindings followed by one `return`, using
`+ - * / **`, unary `-`, comparisons, `a if c else b`, numeric constants,
and `exp`, `tanh`, `sqrt`, `abs`, `relu`, `gelu`, `min`, `max`. A body that
does more still works on numbers, and calling it on a tensor says exactly
why it cannot run there:

```text
error: kernel `noisy` works on numbers, but cannot run on tensors because
       its body is not `let` bindings followed by one `return`
```

## 2. How a kernel becomes WGSL

```text
kernel create swish(...)      syntax tree (FnDecl, `kernel` modifier)
        │  src/vm/kernel.rs: lower()
        ▼
Kernel IR                     params, let-bound locals, one result expression
  l0 = 1 / (1 + exp(-p1 * p0))
  result = p0 * l0
        │                        │
        │ wgsl(arg pattern)      │ run_cpu()
        ▼                        ▼
WGSL compute shader           direct evaluation over the elements
        │  src/vm/gpu.rs: compiled once per (kernel, tensor/number pattern), cached
        ▼
dispatch: one invocation per element, 64 per workgroup
```

1. **Lowering.** At compile time, every `kernel` function body is lowered to a
   small expression IR: parameters, `let` locals in order, one result.
   Names resolve to parameters, earlier locals, or numeric constants, which
   are folded in. The IR is stored with the function (`Proto::kernel`), or
   the reason it could not be lowered.
2. **Dispatch.** A call with any tensor argument takes the VM's rarely-used
   call path, shared with `async` function, so ordinary calls pay nothing for it.
   It checks shapes and devices and asks the tensors' backend to run the
   kernel. The reference backend declines, and the IR is evaluated on the
   CPU.
3. **WGSL generation.** The GPU backend generates a shader for the pattern
   of arguments (which are tensors, which are numbers). Tensors become
   read-only storage buffers, numbers one `scalars` buffer, and each IR
   node one WGSL expression. `vyrn bytecode` prints it; this is `swish`:

   ```wgsl
   @group(0) @binding(0) var<storage, read_write> out: array<f32>;
   @group(0) @binding(1) var<storage, read> scalars: array<f32>;
   @group(0) @binding(2) var<uniform> sizes: Sizes;
   @group(0) @binding(3) var<storage, read> input0: array<f32>;
   @compute @workgroup_size(64)
   fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {
       let i = gid.x + gid.y * groups.x * 64u;
       if (i >= sizes.len) { return; }
       let p0 = input0[i];
       let p1 = scalars[0u];
       let l0 = (f32(1.0) / (f32(1.0) + exp(((-p1) * p0))));
       out[i] = (p0 * l0);
   }
   ```

4. **Compilation and caching.** The shader is compiled once per source text
   and cached, so a kernel in a training or decode loop is compiled on its
   first call only. Work under 4,096 elements stays on the CPU, where it is
   cheaper than a dispatch.

**Same results everywhere.** Both paths compute in `f32` from the same IR.
`tanh` is clamped to ±15 (where it is already ±1 in `f32`) because some
drivers return NaN past that, and `gelu` uses the same tanh approximation
on both. `tests/gpu.rs` runs a kernel using every construct on the GPU and
the CPU and requires agreement to 1e-4 relative.

## 3. Memory layout and cache locality

What the GPU sees is decided by how tensors are stored, and Vyrn keeps that
simple enough to guarantee:

1. **Every tensor is dense and row-major.** There are no strided views:
   slicing, transposing (`.T`) and placement produce new contiguous tensors.
   Element `[i, j]` of an `[m, n]` tensor is at `i * n + j`, always. A
   kernel thread `i` therefore reads element `i` of each input, and the 64
   threads of a workgroup read 64 consecutive values: one coalesced memory
   transaction per input.
2. **Python memory must be contiguous too.** `x.to_tensor()` refuses a
   non-contiguous NumPy view with a fix (`np.ascontiguousarray(x)`) rather
   than taking strides that would break rule 1 (`docs/ffi.md`).
3. **Quantized weights are blocks along rows.** `q8` and `q4` (llama.cpp's
   Q8_0 and Q4_0) store 32 consecutive values per block, with one `f16`
   scale per block. A block never crosses a row when the row length is a
   multiple of 32, which holds for every transformer dimension (4096, 5120,
   13824, ...). `q4` packs two values per byte, low nibble first, so the 8
   values a 32-bit word holds are consecutive in a row.
4. **Weights go to the GPU packed, and stay.** A `q4` matrix is uploaded as
   its packed bytes plus a separate scales buffer, and decoded in the
   shader. 4-bit weights take 4-bit memory in VRAM and on the bus. Matrices
   of 256 KB or more stay resident until the tensor is dropped.
5. **Reads are coalesced in the matrix product.** For `x @ w` with `w` of
   shape `[k, n]`, thread `j` computes output column `j` and reads `w[p, j]`
   for each `p`. Neighbouring threads read neighbouring values, which for
   `q4` are neighbouring nibbles of the same 32-bit word, under one shared
   block scale. During decoding (`m = 1`, the case that dominates LLM
   inference), each weight byte is read from memory once per token.
6. **Buffers are 4-byte aligned and padded**, as WGSL storage arrays
   require. Odd-length `q4` data is padded with zero nibbles that no
   thread reads.

## 4. Design: tile kernels for attention

Element-wise kernels cover activations, gates, residual mixing, scaling and
masks. A custom attention mechanism also needs reductions and index
arithmetic. The design extends `kernel` function rather than adding a second
mechanism (rule 1):

```vyrn
## Scores for grouped-query attention: q is [T, D], k is [S, D].
kernel create scores(q: tensor[f16; T, D], k: tensor[q8; S, D], scale: f32) -> tensor[f32; T, S]:
    each t, s in grid(T, S):              # one GPU invocation per output element
        var acc = 0.0
        each d in 0 until D:              # a loop inside the invocation
            acc += q[t, d] * k[s, d]
        end
        out[t, s] = acc * scale if s <= t else -1e30   # causal mask, fused
    end
end

main:
    on gpu[0]:
        let p = softmax(scores(q, k, 0.088), axis = -1) @ v
    end
end
```

How it lowers:

| Vyrn | WGSL |
|---|---|
| `each t, s in grid(T, S)` (outermost, once) | the dispatch grid: `global_invocation_id.xy`, with a bounds check |
| `each d in 0 until D` | a WGSL `for` loop |
| `q[t, d]` on `f16` / `f32` | `q[t * D + d]` (row-major, section 3), widened to `f32` |
| `k[s, d]` on `q8` / `q4` | the block decoder the matrix product shader already uses: `load_b(s * D + d)` |
| `out[t, s] = ...` | the single write of the invocation |
| dimension names `T, S, D` | uniforms filled from the argument shapes, checked against each other before dispatch |

Then three optimizations, each invisible in the source:

1. **Tiling.** When the inner loop reads one input along the reduction
   axis (here `d`), the compiler stages 16 × 16 tiles of both inputs in
   workgroup shared memory, the standard matmul tiling.
2. **Epilogue fusion.** An element-wise `kernel` function applied to a tile
   kernel's result (`swish(x @ w)`) is inlined into that kernel's final
   write, so the intermediate tensor never reaches memory.
3. **Static checks.** The `kernel` modifier already implies `strict`: the
   checker requires typed parameters, so an index out of a declared shape,
   or a `T` used for two sizes, is an error before anything runs.

The order of work: `grid` and inner loops on the CPU evaluator (so every
kernel has an exact reference), then WGSL generation, then tiling, then
fusion.

## 5. What `std.nn` builds on this

The layers in `docs/stdlib.md` (`nn.Linear`, `nn.RMSNorm`, `nn.Attention`,
`nn.MLP`) become ordinary Vyrn code: structs holding weight tensors, and
`forward` methods built from `@` and `kernel` functions. Developers read and copy
them like any other code, and a custom layer is the same kind of code as a
built-in one.
