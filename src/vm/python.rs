//! `use python.numpy as np`: Python libraries through an embedded CPython.
//!
//! Built with `--features python`. The interpreter is started the first
//! time a program imports a Python module, inside the `vyrn` process, so
//! calls cost a function call, not a message to another process.
//!
//! Values cross the boundary like this:
//!
//! | Vyrn | Python | Back to Vyrn |
//! |---|---|---|
//! | `none`, `bool`, `int`, `float`, `str` | the same | converted |
//! | `[T]`, `(A, B)`, `{K: V}` | `list`, `tuple`, `dict` (converted item by item) | stays a Python object |
//! | `tensor` | a `memoryview` over a copy of the data, which NumPy and PyTorch accept | (see `tensor_from_python`) |
//! | a Python object | the object itself | stays a Python object |
//!
//! Python containers stay Python objects on the way back: `len`, indexing,
//! `for` and attribute access work on them directly, and nothing is copied
//! until the program asks for it (rule 4: no hidden costs). A Python
//! exception becomes a Vyrn failure of kind `PythonError`.

use std::any::Any;
use std::rc::Rc;

use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

use super::foreign::Foreign;
use super::ops::{err, Arith, Ctx, VmResult};
use super::value::Value;

/// A Python object held by Vyrn.
pub struct PyValue {
    pub object: Py<PyAny>,
}

fn python_error(py: Python<'_>, e: PyErr) -> Value {
    let kind = e
        .get_type(py)
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "Exception".into());
    err("PythonError", format!("{kind}: {}", e.value(py)))
}

/// `use python.module` or `use python.module (name)`.
pub fn import(module: &str, name: Option<&str>) -> VmResult<Value> {
    Python::attach(|py| {
        let m = py.import(module).map_err(|e| {
            let base = python_error(py, e);
            match &base {
                Value::Error(inner) if inner.message.contains("ModuleNotFoundError") => err(
                    "PythonError",
                    format!(
                        "Python has no module `{module}`; install it with `python3 -m pip install {}`",
                        module.split('.').next().unwrap_or(module)
                    ),
                ),
                _ => base,
            }
        })?;
        let object = match name {
            Some(n) => m.getattr(n).map_err(|e| python_error(py, e))?,
            None => m.into_any(),
        };
        Ok(from_python(&object))
    })
}

/// A Vyrn value as a Python object.
pub fn to_python<'py>(py: Python<'py>, value: &Value) -> VmResult<Bound<'py, PyAny>> {
    let fail = |e: PyErr| python_error(py, e);
    Ok(match value {
        Value::None => py.None().into_bound(py),
        Value::Bool(b) => PyBool::new(py, *b).to_owned().into_any(),
        Value::Int(i) => PyInt::new(py, *i).into_any(),
        Value::Float(f) => PyFloat::new(py, *f).into_any(),
        Value::Str(s) => PyString::new(py, s).into_any(),
        Value::List(items) => {
            let items: Vec<Bound<PyAny>> = items
                .borrow()
                .iter()
                .map(|v| to_python(py, v))
                .collect::<VmResult<_>>()?;
            PyList::new(py, items).map_err(fail)?.into_any()
        }
        Value::Tuple(items) => {
            let items: Vec<Bound<PyAny>> = items
                .iter()
                .map(|v| to_python(py, v))
                .collect::<VmResult<_>>()?;
            PyTuple::new(py, items).map_err(fail)?.into_any()
        }
        Value::Map(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map.borrow().entries() {
                dict.set_item(to_python(py, k)?, to_python(py, v)?)
                    .map_err(fail)?;
            }
            dict.into_any()
        }
        Value::Tensor(t) => super::python_tensor::tensor_to_python(py, t)?,
        Value::Foreign(f) => match f.as_any().downcast_ref::<PyValue>() {
            Some(p) => p.object.bind(py).clone(),
            None => {
                return Err(err(
                    "TypeError",
                    format!("a {} cannot be passed to Python", f.type_name()),
                ))
            }
        },
        other => {
            return Err(err(
                "TypeError",
                format!("{} cannot be passed to Python", other.type_name()),
            ))
        }
    })
}

/// A Python object as a Vyrn value: scalars and strings are converted,
/// everything else stays a Python object.
pub fn from_python(object: &Bound<'_, PyAny>) -> Value {
    if object.is_none() {
        return Value::None;
    }
    if let Ok(b) = object.cast::<PyBool>() {
        return Value::Bool(b.is_true());
    }
    if object.is_instance_of::<PyInt>() {
        if let Ok(i) = object.extract::<i64>() {
            return Value::Int(i);
        }
    }
    if object.is_instance_of::<PyFloat>() {
        if let Ok(f) = object.extract::<f64>() {
            return Value::Float(f);
        }
    }
    if let Ok(s) = object.cast::<PyString>() {
        return Value::str(s.to_string());
    }
    wrap(object.clone().unbind())
}

pub fn wrap(object: Py<PyAny>) -> Value {
    Value::Foreign(Rc::new(Box::new(PyValue { object })))
}

fn args_kwargs<'py>(
    py: Python<'py>,
    args: &[Value],
    kwargs: &[(String, Value)],
) -> VmResult<(Bound<'py, PyTuple>, Option<Bound<'py, PyDict>>)> {
    let items: Vec<Bound<PyAny>> = args
        .iter()
        .map(|v| to_python(py, v))
        .collect::<VmResult<_>>()?;
    let tuple = PyTuple::new(py, items).map_err(|e| python_error(py, e))?;
    let dict = if kwargs.is_empty() {
        None
    } else {
        let d = PyDict::new(py);
        for (k, v) in kwargs {
            d.set_item(k, to_python(py, v)?)
                .map_err(|e| python_error(py, e))?;
        }
        Some(d)
    };
    Ok((tuple, dict))
}

impl Foreign for PyValue {
    fn type_name(&self) -> &'static str {
        "python object"
    }

    fn display(&self) -> String {
        Python::attach(|py| {
            let o = self.object.bind(py);
            o.str()
                .map(|s| s.to_string())
                .unwrap_or_else(|_| "<python object>".into())
        })
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        Python::attach(|py| {
            let o = self.object.bind(py);
            o.getattr(name)
                .map(|v| from_python(&v))
                .map_err(|e| python_error(py, e))
        })
    }

    fn call(&self, _: &mut Ctx, args: &[Value], kwargs: &[(String, Value)]) -> VmResult<Value> {
        Python::attach(|py| {
            let (args, kwargs) = args_kwargs(py, args, kwargs)?;
            self.object
                .bind(py)
                .call(args, kwargs.as_ref())
                .map(|v| from_python(&v))
                .map_err(|e| python_error(py, e))
        })
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Value],
        kwargs: &[(String, Value)],
    ) -> VmResult<Value> {
        // `.to_tensor()` turns a Python array into a native Vyrn tensor.
        if name == "to_tensor" && args.is_empty() && kwargs.is_empty() {
            return Python::attach(|py| {
                super::python_tensor::tensor_from_python(py, self.object.bind(py))
            });
        }
        let _ = ctx;
        Python::attach(|py| {
            let (args, kwargs) = args_kwargs(py, args, kwargs)?;
            self.object
                .bind(py)
                .call_method(name, args, kwargs.as_ref())
                .map(|v| from_python(&v))
                .map_err(|e| python_error(py, e))
        })
    }

    fn index(&self, key: &Value) -> VmResult<Value> {
        Python::attach(|py| {
            let k = to_python(py, key)?;
            self.object
                .bind(py)
                .get_item(k)
                .map(|v| from_python(&v))
                .map_err(|e| python_error(py, e))
        })
    }

    fn len(&self) -> VmResult<usize> {
        Python::attach(|py| self.object.bind(py).len().map_err(|e| python_error(py, e)))
    }

    fn items(&self) -> VmResult<Vec<Value>> {
        Python::attach(|py| {
            let iter = self
                .object
                .bind(py)
                .try_iter()
                .map_err(|e| python_error(py, e))?;
            iter.map(|item| {
                item.map(|v| from_python(&v))
                    .map_err(|e| python_error(py, e))
            })
            .collect()
        })
    }

    fn binary(&self, op: Arith, other: &Value, flipped: bool) -> Option<VmResult<Value>> {
        Some(Python::attach(|py| {
            let me = self.object.bind(py).clone();
            let them = to_python(py, other)?;
            let (a, b) = if flipped { (them, me) } else { (me, them) };
            let result = match op {
                Arith::Add => a.add(b),
                Arith::Sub => a.sub(b),
                Arith::Mul => a.mul(b),
                Arith::Div => a.div(b),
                Arith::FloorDiv => a.floor_div(b),
                Arith::Rem => a.rem(b),
                Arith::Pow => a.pow(b, py.None()),
                _ => {
                    return Err(err(
                        "TypeError",
                        "this operator is not defined for Python objects",
                    ))
                }
            };
            result
                .map(|v| from_python(&v))
                .map_err(|e| python_error(py, e))
        }))
    }

    fn equals(&self, other: &Value) -> bool {
        Python::attach(|py| {
            to_python(py, other)
                .ok()
                .and_then(|o| self.object.bind(py).eq(o).ok())
                .unwrap_or(false)
        })
    }

    fn truthy(&self) -> bool {
        Python::attach(|py| self.object.bind(py).is_truthy().unwrap_or(true))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
