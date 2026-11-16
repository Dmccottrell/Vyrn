//! Mapped files (`fs.map`) and `vyrn.nn`: loading models, sampling and
//! generating text.
//!
//! `fs.map(path)` maps a file into memory: nothing is read until it is
//! touched, so a file larger than memory can be searched or sliced. On
//! WebAssembly, where there is no mapping, the file is read once instead.
//!
//! `vyrn.nn` is the model side of tensors:
//!
//! | API | Does |
//! |---|---|
//! | `nn.load(path, dtype)` | the tensors of a safetensors file, converted as they are read |
//! | `nn.save(path, tensors)` | writes them |
//! | `nn.tokenizer(path)` | a Hugging Face `tokenizer.json` |
//! | `nn.sample(logits, temperature, top_k, top_p)` | picks the next token |
//! | `nn.generate(step, tokens, ...)` | a generator of new tokens: `step(tokens)` gives the logits |

use std::rc::Rc;

use super::builtins::{integer, number};
use super::foreign::Foreign;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::tensor::Tensor;
use super::value::Value;

// ==== fs.map ===================================================================

enum Bytes {
    #[cfg(not(target_arch = "wasm32"))]
    Mapped(memmap2::Mmap),
    Read(Vec<u8>),
    Closed,
}

/// `fs.map(path)`: a file's bytes, read from disk only as they are used.
pub struct Mapped {
    bytes: std::cell::RefCell<Bytes>,
    path: String,
}

impl Mapped {
    fn with<T>(&self, f: impl FnOnce(&[u8]) -> T) -> VmResult<T> {
        match &*self.bytes.borrow() {
            #[cfg(not(target_arch = "wasm32"))]
            Bytes::Mapped(m) => Ok(f(m)),
            Bytes::Read(v) => Ok(f(v)),
            Bytes::Closed => Err(err("ValueError", format!("{} was closed", self.path))),
        }
    }
}

pub fn fs_map(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => return Err(type_error("`fs.map` takes a file's path")),
    };
    let file = std::fs::File::open(&path).map_err(|e| {
        err(
            if e.kind() == std::io::ErrorKind::NotFound {
                "NotFound"
            } else {
                "IoError"
            },
            format!("cannot open {path}: {e}"),
        )
    })?;
    #[cfg(not(target_arch = "wasm32"))]
    let bytes = {
        // SAFETY: the mapping is read-only; another program shrinking the
        // file meanwhile is outside what any reader can guard against.
        match unsafe { memmap2::Mmap::map(&file) } {
            Ok(m) => Bytes::Mapped(m),
            Err(_) => Bytes::Read(std::fs::read(&path).map_err(|e| err("IoError", e.to_string()))?),
        }
    };
    #[cfg(target_arch = "wasm32")]
    let bytes = {
        let _ = file;
        Bytes::Read(std::fs::read(&path).map_err(|e| err("IoError", e.to_string()))?)
    };
    Ok(Value::Foreign(Rc::new(Box::new(Mapped {
        bytes: std::cell::RefCell::new(bytes),
        path,
    }))))
}

fn range(args: &[Value], len: usize) -> VmResult<(usize, usize)> {
    let at = |i: usize, default: usize| -> VmResult<usize> {
        match args.get(i) {
            None | Some(Value::None) | Some(Value::Missing) => Ok(default),
            Some(v) => {
                let n = integer(v, "a position")?;
                let n = if n < 0 { len as i64 + n } else { n };
                Ok((n.max(0) as usize).min(len))
            }
        }
    };
    let (start, end) = (at(0, 0)?, at(1, len)?);
    Ok((start, end.max(start)))
}

impl Foreign for Mapped {
    fn type_name(&self) -> &'static str {
        "Mapped"
    }

    fn display(&self) -> String {
        format!("<mapped {}>", self.path)
    }

    fn len(&self) -> VmResult<usize> {
        self.with(<[u8]>::len)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "path" => Ok(Value::str(self.path.clone())),
            "size" => Ok(Value::Int(self.with(<[u8]>::len)? as i64)),
            _ => Err(err("FieldError", format!("a mapped file has no `{name}`"))),
        }
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        args: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            // `data.bytes(start, end)`: those bytes, as numbers.
            "bytes" => self.with(|b| {
                let (s, e) = range(args, b.len())?;
                Ok(Value::list(
                    b[s..e].iter().map(|x| Value::Int(i64::from(*x))).collect(),
                ))
            })?,
            // `data.text(start, end)`: those bytes as text.
            "text" => self.with(|b| {
                let (s, e) = range(args, b.len())?;
                Ok(Value::str(String::from_utf8_lossy(&b[s..e]).into_owned()))
            })?,
            // `data.lines()`: every line, as text.
            "lines" => self.with(|b| {
                let text = String::from_utf8_lossy(b);
                Value::list(text.lines().map(Value::str).collect())
            }),
            // `data.count_lines()`, without making the lines.
            "count_lines" => self.with(|b| {
                let n = b.iter().filter(|c| **c == b'\n').count();
                Value::Int((n + usize::from(b.last().is_some_and(|c| *c != b'\n'))) as i64)
            }),
            // `data.find("needle", start = 0)`: where it first appears, or -1.
            "find" => {
                let needle = match args.first() {
                    Some(Value::Str(s)) => s.as_bytes().to_vec(),
                    _ => return Err(type_error("`find` takes the text to look for")),
                };
                self.with(|b| {
                    let (start, _) = range(&args[1..], b.len())?;
                    if needle.is_empty() {
                        return Ok(Value::Int(start as i64));
                    }
                    Ok(Value::Int(
                        b[start..]
                            .windows(needle.len())
                            .position(|w| w == needle.as_slice())
                            .map_or(-1, |i| (start + i) as i64),
                    ))
                })?
            }
            "close" => {
                *self.bytes.borrow_mut() = Bytes::Closed;
                Ok(Value::None)
            }
            _ => Err(err(
                "FieldError",
                format!("a mapped file has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `fs.tensor_view(data, dtype, shape, offset = 0)`: a tensor made from
/// bytes of a mapped file (`"f32"`, `"f16"` or `"bf16"`, little-endian).
pub fn tensor_view(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let Some(Value::Foreign(f)) = args.first() else {
        return Err(type_error("`fs.tensor_view` takes a file from `fs.map`"));
    };
    let mapped = f
        .as_any()
        .downcast_ref::<Mapped>()
        .ok_or_else(|| type_error("`fs.tensor_view` takes a file from `fs.map`"))?;
    let dtype = match args.get(1) {
        Some(Value::Str(s)) => s.to_string(),
        _ => {
            return Err(type_error(
                "give the element type: \"f32\", \"f16\" or \"bf16\"",
            ))
        }
    };
    let shape: Vec<usize> = ops::collect(args.get(2).unwrap_or(&Value::None))?
        .iter()
        .map(|d| integer(d, "a dimension").map(|d| d.max(0) as usize))
        .collect::<VmResult<_>>()?;
    let offset = match args.get(3) {
        Some(v) if !matches!(v, Value::None | Value::Missing) => {
            integer(v, "the offset")?.max(0) as usize
        }
        _ => 0,
    };
    let count: usize = shape.iter().product();
    let width = match dtype.as_str() {
        "f32" => 4,
        "f16" | "bf16" => 2,
        other => {
            return Err(err(
                "ValueError",
                format!("`{other}` is not f32, f16 or bf16"),
            ))
        }
    };
    let data = mapped.with(|b| -> VmResult<Vec<f32>> {
        let end = offset
            .checked_add(count * width)
            .filter(|e| *e <= b.len())
            .ok_or_else(|| {
                err(
                    "IndexError",
                    format!(
                        "{} values from byte {offset} run past the end of the file",
                        count
                    ),
                )
            })?;
        let raw = &b[offset..end];
        Ok(match dtype.as_str() {
            "f32" => raw
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
            "f16" => raw
                .chunks_exact(2)
                .map(|c| super::dtype::f16_to_f32(u16::from_le_bytes([c[0], c[1]])))
                .collect(),
            _ => raw
                .chunks_exact(2)
                .map(|c| super::dtype::bf16_to_f32(u16::from_le_bytes([c[0], c[1]])))
                .collect(),
        })
    })??;
    let t = Tensor::new(shape, data, ctx.current_device()).map_err(ops::tensor_error)?;
    ops::alloc(t)
}

// ==== vyrn.nn ==================================================================

/// The logits as numbers: a tensor (its last row, for a batch of
/// positions) or a list.
fn logits_of(v: &Value) -> VmResult<Vec<f64>> {
    match v {
        Value::Tensor(t) => {
            let values = t.values();
            let row = t.shape.last().copied().unwrap_or(values.len()).max(1);
            let start = values.len().saturating_sub(row);
            Ok(values[start..].iter().map(|x| f64::from(*x)).collect())
        }
        other => ops::collect(other)?
            .iter()
            .map(|x| number(x, "a logit"))
            .collect(),
    }
}

/// `nn.sample(logits, temperature = 1, top_k = 0, top_p = 1)`: the next
/// token. Temperature 0 always picks the likeliest; `top_k` keeps that
/// many likeliest, `top_p` the likeliest whose chances add up to it.
/// Uses `vyrn.random`, so `random.seed(n)` repeats a run.
pub fn sample(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let logits = logits_of(args.first().unwrap_or(&Value::None))?;
    if logits.is_empty() {
        return Err(err("ValueError", "`nn.sample` needs at least one logit"));
    }
    let opt = |i: usize, default: f64, what: &str| -> VmResult<f64> {
        match args.get(i) {
            None | Some(Value::None) | Some(Value::Missing) => Ok(default),
            Some(v) => number(v, what),
        }
    };
    let temperature = opt(1, 1.0, "the temperature")?;
    let top_k = opt(2, 0.0, "top_k")? as usize;
    let top_p = opt(3, 1.0, "top_p")?;
    let best = logits
        .iter()
        .enumerate()
        .fold(
            (0, f64::NEG_INFINITY),
            |b, (i, x)| if *x > b.1 { (i, *x) } else { b },
        )
        .0;
    if temperature <= 0.0 {
        return Ok(Value::Int(best as i64));
    }
    // Softmax at the temperature, likeliest first.
    let max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mut probs: Vec<(usize, f64)> = logits
        .iter()
        .enumerate()
        .map(|(i, x)| (i, ((x - max) / temperature).exp()))
        .collect();
    probs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    if top_k > 0 {
        probs.truncate(top_k);
    }
    let total: f64 = probs.iter().map(|p| p.1).sum();
    if top_p < 1.0 {
        let mut kept = 0.0;
        let mut n = 0;
        for p in &probs {
            n += 1;
            kept += p.1 / total;
            if kept >= top_p {
                break;
            }
        }
        probs.truncate(n.max(1));
    }
    let total: f64 = probs.iter().map(|p| p.1).sum();
    let mut pick = super::stdlib::next_f64() * total;
    for (i, p) in &probs {
        pick -= p;
        if pick <= 0.0 {
            return Ok(Value::Int(*i as i64));
        }
    }
    Ok(Value::Int(probs.last().map_or(best, |p| p.0) as i64))
}

/// `nn.generate(...)` is written in Vyrn (the Prelude's `__nn_generate`,
/// which the VM runs instead), since it calls the model's own `step`.
pub fn generate(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Err(err("Unavailable", "`nn.generate` runs from the Prelude"))
}
