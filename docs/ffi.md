# Foreign Code and Decentralized Modules

Vyrn reaches three kinds of code outside itself:

| Component | Vyrn syntax | Status | Source |
|---|---|---|---|
| Embedded CPython | `use python.datasets` | runs (`--features python`) | `src/vm/python.rs`, `src/vm/python_tensor.rs` |
| Zero-copy tensors from Python | `array.to_tensor()` | runs | `src/vm/dtype.rs` (`External`), `src/vm/python_tensor.rs` |
| C and C++ libraries | `@link("m")` + `extern "c":` | runs (`--features ffi`) | `src/vm/ffi.rs`, `src/check.rs` |
| Modules from any git host | `use "github.com/ann/neural@v1"` | runs (default) | `src/project/` |

The two bridges plug into the VM through one trait, `Foreign`
(`src/vm/foreign.rs`): attribute access, calls, indexing, `len`, iteration
and operators. A build without a bridge has the same VM, and a program that
uses one gets an error saying which feature to build with.

Every snippet below is taken, shortened, from the code; the tests that
exercise it are `tests/bridges.rs` and `tests/project.rs`.

---

## 1. The CPython embed

### 1.1 Shape

CPython runs inside the `vyrn` process (through PyO3, linked against the
system's `libpython`), started on the first `use python.*`. A call into
Python is a function call, not a message to another process. Vyrn is
single-threaded, so each call attaches to the interpreter for its duration
(`Python::attach`), and nothing else holds the GIL.

Rule 1 applies: module paths use dots everywhere, so it is
`use python.datasets`, never `use python::datasets`.

### 1.2 From `use` to the C API

```vyrn
use python.datasets

main:
    let ds = datasets.load_dataset("imdb", split = "train")
    print(len(ds))
end
```

**Parse.** `use python.datasets` is an ordinary `UseDecl` with path
`[python, datasets]`.

**Compile.** The compiler turns a Python import into a global that is
filled before anything else runs (`src/vm/compiler.rs`):

```rust
/// `use python.numpy as np`, `use python.os.path (join)`.
fn python_use(&mut self, decl: &UseDecl, path: &[&str]) {
    let module = Value::str(path.join("."));
    match &decl.kind {
        UseKind::Module { alias } => {
            let local = alias.as_ref().map_or(path[path.len() - 1], |a| a.name.as_str());
            self.foreign_global(local, decl.path.span, "python.import", vec![module]);
        }
        UseKind::Names(names) => {
            for n in names {
                let local = n.alias.as_ref().unwrap_or(&n.name).name.clone();
                self.foreign_global(&local, n.name.span, "python.import",
                    vec![module.clone(), Value::str(n.name.name.clone())]);
            }
        }
    }
}
```

**Bytecode.** `vyrn bytecode` shows the result. The module initializer
calls the `python.import` builtin and stores the module in a global; `main`
calls a method on it with a keyword argument:

```text
fn #1 <module>() registers=2
    k0 = Native(47)              ; python.import
    k1 = Str("datasets")
     0  LoadConst { dst: 0, k: 0 }
     1  LoadConst { dst: 1, k: 1 }
     2  Call { func: 0, argc: 1 }
     3  SetGlobal { src: 0, idx: 0 }

fn #0 main() registers=6
     0  GetGlobal { dst: 2, idx: 0 }                    ; datasets
     1  LoadConst { dst: 3, k: 0 }                      ; "imdb"
     2  LoadConst { dst: 4, k: 1 }                      ; "train"
     3  LoadConst { dst: 5, k: 2 }                      ; ("split",)
     4  CallMethodKw { base: 1, argc: 2, name: 0 }      ; .load_dataset
```

**Run.** `python.import` is `PyImport_ImportModule`; `CallMethodKw` on a
foreign receiver becomes `PyObject_Call` on the attribute, with a tuple of
positional arguments and a dict of keyword arguments:

```rust
pub fn import(module: &str, name: Option<&str>) -> VmResult<Value> {
    Python::attach(|py| {
        let m = py.import(module).map_err(|e| python_error(py, e))?;   // PyImport_ImportModule
        let object = match name {
            Some(n) => m.getattr(n).map_err(|e| python_error(py, e))?, // PyObject_GetAttr
            None => m.into_any(),
        };
        Ok(from_python(&object))
    })
}

impl Foreign for PyValue {
    fn call_method(&self, _: &mut Ctx, name: &str, args: &[Value], kwargs: &[(String, Value)]) -> VmResult<Value> {
        Python::attach(|py| {
            let (args, kwargs) = args_kwargs(py, args, kwargs)?;        // PyTuple + PyDict
            self.object.bind(py)
                .call_method(name, args, kwargs.as_ref())                // PyObject_Call
                .map(|v| from_python(&v))
                .map_err(|e| python_error(py, e))
        })
    }
    // get_attr, call, index, len, items, binary (+ - * / // % **), equals, truthy
}
```

### 1.3 The type boundary

| Vyrn | Python | Back to Vyrn |
|---|---|---|
| `none`, `bool`, `int`, `float`, `str` | same | converted |
| `[T]`, `(A, B)`, `{K: V}` | `list`, `tuple`, `dict` | stay Python objects |
| `tensor` | `memoryview` of float32, accepted by NumPy and PyTorch | see section 2 |
| Python object | the object itself | a Python object |

Python containers stay Python objects on the way back: `len`, `x[i]`,
`for`, attributes and operators work on them directly, and nothing is
converted until the program asks (rule 4). A Python exception becomes a
Vyrn failure of kind `PythonError` whose message starts with the Python
class: `catch PythonError as e`. A missing module says how to install it:

```text
error: Python has no module `datasets`; install it with `python3 -m pip install datasets`
```

Packages come from the Python that `vyrn` was linked against. A project-local
environment (`[python] venv = ".venv"` in `vyrn.toml`, put on `sys.path`
at start-up) is the next step.

---

## 2. The zero-copy tensor bridge

### 2.1 Requirement

A tensor that came from Python must be usable in Vyrn's native matrix code
without copying its data, while keeping Vyrn's guarantees: memory is never
freed underneath a tensor, and changing a tensor never changes someone
else's.

### 2.2 Borrowed storage

A tensor's storage can be memory owned by someone else
(`src/vm/dtype.rs`):

```rust
/// Memory that belongs to someone else, such as a NumPy array or a
/// memory-mapped file, read in place.
pub struct External {
    ptr: *const u8,
    bytes: usize,
    dtype: DType,                              // f32, f16 or bf16
    _owner: Rc<dyn Any>,                       // keeps the memory alive
}

impl External {
    /// # Safety
    /// The memory must stay valid, and must not be freed or resized, for as
    /// long as `owner` is alive.
    pub unsafe fn new(ptr: *const u8, bytes: usize, dtype: DType,
                      owner: Rc<dyn Any>) -> Option<External> { /* checks alignment and length */ }

    pub fn f32s(&self) -> &[f32] {
        // SAFETY: `new`'s contract keeps the memory valid while `_owner` lives.
        unsafe { std::slice::from_raw_parts(self.ptr as *const f32, self.bytes / 4) }
    }
}

pub enum Storage {
    F32(Vec<f32>), F16(Vec<u16>), BF16(Vec<u16>), Quantized(Quantized),
    External(Rc<External>),
}
```

Every reader (matrix products, element-wise math, the GPU upload, the
weights writer) reads `External` memory directly; `values()` returns it as a
borrowed slice. Writing is copy on write:

```rust
pub fn data_mut(&mut self) -> Result<&mut Vec<f32>, TensorError> {
    // Borrowed memory is copied into Vyrn's own on the first change.
    if let Storage::External(e) = &self.storage {
        if e.dtype() == DType::F32 {
            self.storage = Storage::F32(e.f32s().to_vec());
        }
    }
    ...
}
```

### 2.3 Taking a Python buffer

Python's buffer protocol is the common ground: NumPy arrays, `memoryview`,
`array.array` and PyTorch CPU tensors (through `.numpy()`) all describe
their memory with a pointer, a shape and an element format. Vyrn holds the
buffer, which pins the exporter's memory, and stores it as the owner
(`src/vm/python_tensor.rs`):

```rust
pub fn tensor_from_python(py: Python<'_>, object: &Bound<'_, PyAny>) -> VmResult<Value> {
    let buffer = PyUntypedBuffer::get(&object)?;                  // PyObject_GetBuffer
    let dtype = match buffer.format() { "f" => DType::F32, "e" => DType::F16, other => refuse(other)? };
    if !buffer.is_c_contiguous() { refuse_noncontiguous()? }
    let shape = buffer.shape().to_vec();
    let (ptr, bytes) = (buffer.buf_ptr() as *const u8, buffer.len_bytes());
    let owner: Rc<dyn Any> = Rc::new(buffer);                    // PyBuffer_Release on drop
    // SAFETY: a held Python buffer keeps the exporter's memory valid and its
    // size fixed until it is released, which happens when `owner` is dropped.
    let storage = Storage::External(Rc::new(unsafe { External::new(ptr, bytes, dtype, owner) }?));
    Ok(Value::Tensor(Rc::new(DeviceTensor::new(Tensor { shape, storage, device: Device::Cpu }))))
}
```

What the tests check: a change NumPy makes in place is visible through the
tensor; a write through Vyrn copies first and leaves NumPy's array alone; a
tensor outlives the NumPy array it came from; a 1000 × 1000 array goes
straight into `@`.

### 2.4 Python types to Vyrn tensors

| Python | Vyrn | Copy? |
|---|---|---|
| contiguous `float32` array | `tensor[f32]` | none |
| contiguous `float16` array | `tensor[f16]` | none |
| PyTorch CPU tensor | through `.numpy()`, as above | none |
| `float64`, integer arrays | refused: "convert it with `.astype("float32")`" | the program decides |
| non-contiguous view | refused: "pass `np.ascontiguousarray(x)`" | the program decides |
| big-endian array | refused | the program decides |
| `bfloat16` (PyTorch) | planned: `.view(torch.int16)` exported as raw `bf16` | none |

Refusing, rather than silently converting, is rule 4: a copy of a large
array is a cost the program should see.

### 2.5 Hugging Face to native matrix code

```vyrn
use python.datasets
use python.numpy as np

main:
    # Hugging Face downloads and caches the dataset; any dataset with an
    # `embedding` column of float32 vectors works the same way.
    let ds = datasets.load_dataset("your-org/imdb-embeddings", split = "train")
    let rows = ds.with_format("numpy")["embedding"]          # (n, 768) float32
    let x = np.ascontiguousarray(rows, dtype = "float32").to_tensor()

    # From here on everything is native Vyrn, on the GPU if there is one.
    on gpu[0]:
        let w1 = rand(768, 256) * 0.02
        let w2 = rand(256, 2) * 0.02
        let logits = gelu((x on gpu[0]) @ w1) @ w2
        print(logits.shape, softmax(logits, axis = -1)[0])
    end
end
```

`x` shares the dataset's memory: no byte is copied between Python and the
matrix product. Moving it with `on gpu[0]` is the one copy, and it is
written in the source.

**Safetensors files** take the same path through Python
(`safetensors.numpy.load_file(path)` gives NumPy arrays), and Vyrn's own
`load_weights` reads them natively. The next step is for `load_weights` to
memory-map the file and hand out `External` tensors over the mapping, which
reaches the same zero-copy result without Python (the `std.fs.map` design
in `docs/stdlib.md`).

---

## 3. The C and C++ bridge

### 3.1 Declaring C functions

```vyrn
@link("accel")                       # libaccel.so / accel.dll; omit for libc
extern "c":
    fn accel_sum(x: tensor[f32], n: i32) -> f32
    fn accel_new() -> ptr u8
    fn accel_add(h: ptr u8, x: tensor[f32], n: i32)
    fn accel_free(h: ptr u8)
end

main:
    let x = arange(1024)
    unsafe:
        print(accel_sum(x, 1024))
        let h = accel_new()
        accel_add(h, x, 1024)
        accel_free(h)
    end
end
```

**Compile.** Each declaration becomes a signature string checked at compile
time. A type that cannot cross is an error with a fix, not a crash:

```rust
/// A C function's signature in the form the FFI builtin reads: `f64(f64,ptr)`.
fn c_signature(f: &FnDecl) -> Result<String, (String, Span)> { ... }
// accel_add  ->  "void(ptr,ptr,i32)"
```

**Run.** At start-up the library is opened (`libloading`), the symbol found,
and a libffi call description (`Cif`) prepared. A call converts each
argument to its C type, range-checking integers, and calls through libffi
(`src/vm/ffi.rs`):

```rust
let slots: Vec<Slot> = self.params.iter().zip(args).enumerate()
    .map(|(i, (&ty, v))| self.convert(i, ty, v, &mut strings))   // i32 range checks, CString, tensor pointer
    .collect::<VmResult<_>>()?;
let ffi_args: Vec<Arg> = slots.iter().map(Slot::arg).collect();
// SAFETY: `cif` describes the declared signature, every argument was
// converted to exactly that C type, and the pointers in `slots` (strings,
// tensor data) stay alive until the call returns.
unsafe { self.cif.call::<f32>(self.code, &ffi_args) }
```

| Vyrn | C |
|---|---|
| `int` | `i8` ... `u64` (out-of-range values are an `OverflowError`) |
| `float` | `f32`, `f64` |
| `bool` | `_Bool` |
| `str` | `const char*`, NUL-terminated, valid for the call |
| `tensor[f32]` | `float*` to its data, valid for the call |
| C pointer, `none` | `void*`, `NULL` |

### 3.2 The borrow checker and raw pointers

Vyrn's borrow checking is function-local and has no lifetime annotations
(`docs/architecture.md`), so raw pointers are kept out of the safe language
instead of being tracked through it:

1. **Pointers into Vyrn memory never become values.** A tensor or string
   passed to C is converted to a pointer inside the call and the pointer is
   gone when the call returns. Nothing in Vyrn can hold it, store it or
   return it, so there is nothing to track.
2. **Pointers from C are opaque handles** (`ptr u8`). Vyrn can compare them,
   pass them back to C and check them against `none`, but cannot read
   through them. The memory belongs to the C library, which also frees it.
3. **Calls that pass or return pointers go in `unsafe:`.** The checker
   enforces it (`src/check.rs`):

   ```text
   error: `accel_add` passes raw pointers to C, so it can only be called in an `unsafe:` block
     = help: put the call in `unsafe:` ... `end`: you promise the C code reads and
             writes only what it was given, and keeps no pointer to Vyrn memory after it returns
   ```

   Functions that take and return only numbers, such as `cos`, are safe to
   call anywhere: no memory crosses.
4. **`mut` marks writes.** A C parameter declared `mut x: tensor[f32]` needs
   `mut x` at the call, so the existing rules apply: the argument must be a
   `var`, and it cannot also appear elsewhere in the same call.

One gap remains: C writing through a tensor pointer is visible to every
variable sharing that tensor, because the bridge does not yet give a `mut`
tensor argument its own copy before the call. Until it does, pass `mut`
tensors that are not shared.

### 3.3 Headers and C++

Hand-written `extern` blocks are the one mechanism (rule 1). Header import
is a tool that writes them: `vyrn bind accel.h > accel.vyrn` (design) parses
the header with libclang and emits one `extern "c":` block, mapping
`float*` to `tensor[f32]` or `ptr f32` by an annotation table. C++ is
reached through its C ABI: an `extern "C"` header of free functions over
opaque handles, the same pattern CUDA, cuBLAS and most accelerator SDKs
already ship. Calling C++ classes directly is not planned.

---

## 4. The decentralized module resolver

### 4.1 Syntax and versions

```vyrn
use "github.com/Dmccottrell/A.I-SML@v3" as yuvra
use "codeberg.org/ann/models/llama@v2.1"          # a directory inside a repository
use "gitlab.com/lab/tokenizers@main"              # a branch
```

| Version | Means |
|---|---|
| `@v3` | the newest tag `v3.x.y` |
| `@v3.1` | the newest tag `v3.1.y` |
| `@v3.1.2` | exactly that tag |
| `@main` | the newest commit on branch `main` |
| `@3f9c2e1...` (40 hex digits) | exactly that commit |

Without `as`, a module is bound to its last path segment, made into an
identifier (`A.I-SML` becomes `A_I_SML`). A module's entry point is
`lib.vyrn`; its own `use a.b` lines are relative to its directory.

There is no registry: the `use` line is the dependency (one place to write
it), and `vyrn.lock` records exactly what it resolved to (one place that
records it). `vyrn add <url>` resolves ahead of time and `vyrn fetch` fills
the cache for offline work.

### 4.2 The fetch and cache loop

Tags and branches are listed with git's own HTTPS protocol (the "smart
HTTP" ref advertisement), which every git host speaks, so any host works
without a special API. An archive of the chosen commit is downloaded,
unpacked in memory, hashed by content, and stored in a content-addressed
cache (`src/project/resolve.rs`):

```rust
pub fn resolve(&mut self, text: &str) -> Result<PathBuf, String> {
    let url = ModuleUrl::parse(text).map_err(|e| e.0)?;            // host, repo, subdir, version
    let root = match self.lock.modules.get(text).cloned() {
        Some(locked) => {
            let dir = self.cache.join(&locked.hash);                // ~/.vyrn/modules/sha256-…
            if !dir.is_dir() {
                if self.offline { return Err(not_cached(text)); }
                let tree = self.download(&url, &locked.commit)?;
                let hash = tree_hash(&tree);
                if hash != locked.hash {
                    return Err(format!("`{text}` changed since it was locked: commit {} now hashes \
                        to {hash}, but vyrn.lock says {}", locked.commit, locked.hash));
                }
                self.store(&tree, &hash)?;
            }
            dir
        }
        None => {
            if self.offline { return Err(not_locked(text)); }
            let refs = self.fetcher.list_refs(&url)?;               // GET /info/refs?service=git-upload-pack
            let (version, commit) = select(&url.version, &refs)     // newest matching tag
                .ok_or_else(|| no_match(&url, &refs))?;
            let tree = self.download(&url, &commit)?;               // archive of that commit, unpacked
            let hash = tree_hash(&tree);
            let dir = self.store(&tree, &hash)?;                    // unpack beside, then rename
            self.lock.modules.insert(text.to_string(), Locked { version, commit, hash });
            self.lock_changed = true;
            dir
        }
    };
    Ok(match &url.subdir { Some(sub) => root.join(sub), None => root })
}
```

The hash covers the files, not the archive, so it is the same from any
mirror and does not depend on how the host compresses archives:

```rust
pub fn tree_hash(tree: &Tree) -> String {
    let mut h = Sha256::new();
    for (path, data) in tree {                                     // sorted by path
        h.update((path.len() as u64).to_le_bytes()); h.update(path.as_bytes());
        h.update((data.len() as u64).to_le_bytes()); h.update(data);
    }
    format!("sha256-{}", hex(h.finalize()))
}
```

```text
# vyrn.lock: written by vyrn. Commit it; do not edit it.

[[module]]
url = "github.com/Dmccottrell/A.I-SML@v3"
version = "v3.1.0"
commit = "dddddddddddddddddddddddddddddddddddddddd"
hash = "sha256-…"
```

**Security properties.**

- HTTPS only; `HTTPS_PROXY` and `SSL_CERT_FILE` are honoured, for corporate
  networks.
- Nothing runs on install: a module is source code, never a build or
  install script.
- Archives are unpacked in memory. Links and devices are skipped, and any
  path that would land outside the module (`..`, absolute paths) aborts the
  whole module.
- A locked module whose bytes change is refused, even under the same commit
  and even from the original host. Accepting new code takes an explicit
  `vyrn add`.
- The cache is written beside its final place and renamed, so a crash never
  leaves half a module.

### 4.3 Linking at compile time

Fetched modules join the program at the syntax-tree level, before checking
and bytecode generation (`src/project/loader.rs`):

1. **Load.** Starting from the main file, each `use` is located: the
   standard library and `python.*` are left to the compiler; local
   `use a.b` is `a/b.vyrn` under the project; a URL is resolved as above and
   its `lib.vyrn` loaded. Each file is parsed at its own base offset
   (`file index << 24`), so every span records which file it came from and
   errors name the right file.
2. **Namespace.** Each file gets a prefix (`yuvra`, `geo.shapes`). Its
   top-level declarations are renamed into it, so two modules can both
   declare `load`.
3. **Rewrite references.** A scope-aware pass rewrites each name to the
   declaration it means: the file's own names, names imported with
   `use m (a, b as c)`, and `module.member` collapsed to one name (in
   expressions, types, patterns and decorators). Local variables are
   tracked, so a variable is never mistaken for an import of the same name.
   A missing member is an error at this point: `module `yuvra` has no
   `train``.
4. **Merge.** The files' items become one module, dependencies first, and
   go through the checker and compiler as if they were one file.

### 4.4 Offline mode

After the first fetch, a project builds with no network:

| State | Behaviour |
|---|---|
| Locked and cached | No network access at all; the cache answers. |
| Locked, not cached (new machine) | Downloads that exact commit and verifies the hash. With `VYRN_OFFLINE=1`: "run `vyrn fetch` while online". |
| Not locked | Resolves and locks. With `VYRN_OFFLINE=1`: "run `vyrn add <url>` while online". |

`vyrn fetch` downloads every locked module, for machines that will be
offline later, and `vyrn build` bundles every fetched module into the
executable, so a built program needs neither the network nor the cache.

### 4.5 Tested

`tests/project.rs` runs the resolver against an in-memory git host:
resolving `@v1` to the newest `v1.x.y`, writing `vyrn.lock`, answering from
the cache offline after a newer version is published, refusing a rewritten
commit, listing the existing tags when none match, `as yuvra` with
`A.I-SML@v3`, namespaces, and errors that name their file.
`tests/net.rs` (`cargo test --test net -- --ignored`) runs the HTTPS fetcher
against a real public repository. In the sandbox this was developed in, the
ref listing from github.com worked and the archive host
(`codeload.github.com`) was blocked by the sandbox's network policy (`curl`
gets the same 403), so the archive download has been verified only through
the in-memory host.
