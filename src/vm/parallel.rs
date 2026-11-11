//! `task.parallel(f, items)`: work spread over every CPU core.
//!
//! Each worker thread builds its own copy of the program and its own VM,
//! so nothing is shared between threads and nothing needs locking (the
//! same model as Python's multiprocessing, without separate processes).
//! Arguments and results cross between threads as plain data: numbers,
//! text, lists, tuples, maps, sets, structs and objects (copied). Functions,
//! files and other live values cannot cross.

use std::sync::Arc;

use super::machine::Vm;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::program::Program;
use super::value::{Instance, Key, Map, Value};

/// Builds a fresh copy of the running program, on any thread.
pub type Rebuild = Arc<dyn Fn() -> Result<Program, String> + Send + Sync>;

/// A value as plain data, able to move to another thread.
#[derive(Clone, Debug)]
pub enum Portable {
    None,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    List(Vec<Portable>),
    Tuple(Vec<Portable>),
    Map(Vec<(Portable, Portable)>),
    Set(Vec<Portable>),
    Instance {
        type_name: String,
        fields: Vec<Portable>,
        object: bool,
    },
}

pub fn to_portable(program: &Program, v: &Value) -> Result<Portable, String> {
    let many = |items: &[Value]| -> Result<Vec<Portable>, String> {
        items.iter().map(|i| to_portable(program, i)).collect()
    };
    Ok(match v {
        Value::None | Value::Missing => Portable::None,
        Value::Bool(b) => Portable::Bool(*b),
        Value::Int(i) => Portable::Int(*i),
        Value::Float(f) => Portable::Float(*f),
        Value::Str(s) => Portable::Str(s.to_string()),
        Value::List(xs) => Portable::List(many(&xs.borrow())?),
        Value::Tuple(xs) => Portable::Tuple(many(xs)?),
        Value::Map(m) => Portable::Map(
            m.borrow()
                .entries()
                .iter()
                .map(|(k, v)| Ok((to_portable(program, k)?, to_portable(program, v)?)))
                .collect::<Result<_, String>>()?,
        ),
        Value::Struct(inst) => Portable::Instance {
            type_name: program
                .symbols
                .name(program.types[inst.ty as usize].name)
                .to_string(),
            fields: many(&inst.fields)?,
            object: false,
        },
        Value::Object(o) => {
            let o = o.borrow();
            Portable::Instance {
                type_name: program
                    .symbols
                    .name(program.types[o.ty as usize].name)
                    .to_string(),
                fields: many(&o.fields)?,
                object: true,
            }
        }
        Value::Foreign(f) => match super::stdlib::Set::of(v) {
            Some(set) => Portable::Set(many(&set.values())?),
            None => return Err(format!("a {} cannot be sent to another core", f.type_name())),
        },
        other => {
            return Err(format!(
                "a {} cannot be sent to another core; send plain data (numbers, text, lists, maps, structs)",
                other.type_name()
            ))
        }
    })
}

pub fn from_portable(program: &Program, p: &Portable) -> Value {
    let many = |items: &[Portable]| -> Vec<Value> {
        items.iter().map(|i| from_portable(program, i)).collect()
    };
    match p {
        Portable::None => Value::None,
        Portable::Bool(b) => Value::Bool(*b),
        Portable::Int(i) => Value::Int(*i),
        Portable::Float(f) => Value::Float(*f),
        Portable::Str(s) => Value::str(s.clone()),
        Portable::List(xs) => Value::list(many(xs)),
        Portable::Tuple(xs) => Value::tuple(many(xs)),
        Portable::Map(entries) => {
            let mut m = Map::default();
            for (k, v) in entries {
                let key = from_portable(program, k);
                if let Some(h) = Key::from_value(&key) {
                    m.insert(h, key, from_portable(program, v));
                }
            }
            Value::Map(std::rc::Rc::new(std::cell::RefCell::new(m)))
        }
        Portable::Set(xs) => super::stdlib::Set::from_values(many(xs))
            .map(super::stdlib::Set::value)
            .unwrap_or(Value::None),
        Portable::Instance {
            type_name,
            fields,
            object,
        } => {
            let ty = program
                .types
                .iter()
                .position(|t| program.symbols.name(t.name) == type_name);
            match ty {
                Some(ty) => {
                    let inst = Instance {
                        ty: ty as u16,
                        fields: many(fields),
                    };
                    if *object {
                        Value::Object(std::rc::Rc::new(std::cell::RefCell::new(inst)))
                    } else {
                        Value::Struct(std::rc::Rc::new(inst))
                    }
                }
                None => Value::None,
            }
        }
    }
}

/// `task.parallel(f, items, workers)`: `[f(x) each x in items]`, with the
/// items shared out among `workers` threads (one per core by default).
/// `f` must be a top-level function, so each thread has its own copy.
pub fn parallel(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let name = match args.first() {
        Some(Value::Function(c)) if c.captures.is_empty() && c.bound.is_none() => {
            c.proto.name.clone()
        }
        _ => {
            return Err(type_error(
                "`task.parallel` takes a top-level function (`create work(x):`), which every core runs its own copy of",
            ))
        }
    };
    if ctx.program.global(&name).is_none() {
        return Err(type_error(format!(
            "`task.parallel` needs a top-level function; `{name}` is not one"
        )));
    }
    let items = ops::collect(args.get(1).unwrap_or(&Value::None))?;
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    let workers = match args.get(2) {
        Some(Value::Int(n)) if *n > 0 => *n as usize,
        _ => cores,
    }
    .min(items.len().max(1));
    let Some(rebuild) = ctx.rebuild.clone() else {
        // Embedded without a way to rebuild the program: run here.
        return Err(err(
            "Unavailable",
            "`task.parallel` needs the program's source to start other cores",
        ));
    };
    let portable: Vec<Portable> = items
        .iter()
        .map(|v| to_portable(&ctx.program, v))
        .collect::<Result<_, _>>()
        .map_err(type_error)?;
    // Share the items out in order: worker `w` takes a contiguous chunk.
    let chunk = portable.len().div_ceil(workers);
    let chunks: Vec<Vec<Portable>> = portable.chunks(chunk.max(1)).map(|c| c.to_vec()).collect();
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let rebuild = rebuild.clone();
            let name = name.clone();
            std::thread::spawn(move || -> Result<Vec<Portable>, String> {
                let program = rebuild()?;
                let mut vm = Vm::with_output(program, Box::new(std::io::stdout()));
                let mut out = Vec::with_capacity(chunk.len());
                for item in &chunk {
                    let arg = from_portable(vm.program(), item);
                    let result = vm
                        .call_named(&name, vec![arg])
                        .map_err(|e| format!("{}: {}", e.kind, e.message))?;
                    out.push(to_portable(vm.program(), &result)?);
                }
                Ok(out)
            })
        })
        .collect();
    let mut results = Vec::with_capacity(items.len());
    for h in handles {
        let part = h
            .join()
            .map_err(|_| err("ParallelError", "a worker thread crashed"))?
            .map_err(|e| err("ParallelError", format!("in `{name}` on another core: {e}")))?;
        results.extend(part.iter().map(|p| from_portable(&ctx.program, p)));
    }
    Ok(Value::list(results))
}
