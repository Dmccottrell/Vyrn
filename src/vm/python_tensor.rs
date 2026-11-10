//! Tensors across the Python boundary.
//!
//! Python's buffer protocol is the common ground: NumPy arrays, PyTorch
//! CPU tensors (through `.numpy()`), `memoryview`, `array.array` and
//! `bytes` all expose their memory through it, with a shape and an element
//! format. Vyrn reads that description and takes the memory without going
//! through Python objects element by element.

use pyo3::buffer::PyBuffer;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::ops::{err, VmResult};
use super::tensor::{Device, DeviceTensor, Tensor};
use super::value::Value;

/// A Python array as a native tensor: `x.to_tensor()`. Accepts anything
/// that offers a buffer of float32 values, and objects with `.numpy()`
/// (PyTorch). The memory must be contiguous; call `.copy()` or
/// `np.ascontiguousarray(x)` first otherwise.
pub fn tensor_from_python(py: Python<'_>, object: &Bound<'_, PyAny>) -> VmResult<Value> {
    let fail = |message: String| err("PythonError", message);
    let mut object = object.clone();
    if PyBuffer::<f32>::get(&object).is_err() {
        if let Ok(array) = object.call_method0("numpy") {
            object = array;
        }
    }
    let buffer = PyBuffer::<f32>::get(&object).map_err(|e| {
        fail(format!(
            "this Python object has no float32 buffer ({}); convert it with `.astype(\"float32\")` first",
            e.value(py)
        ))
    })?;
    if !buffer.is_c_contiguous() {
        return Err(fail(
            "the Python array is not contiguous; pass `np.ascontiguousarray(x)`".into(),
        ));
    }
    let shape = buffer.shape().to_vec();
    let data = buffer
        .to_vec(py)
        .map_err(|e| fail(format!("cannot read the Python array: {}", e.value(py))))?;
    let tensor = Tensor::new(shape, data, Device::Cpu).map_err(super::ops::tensor_error)?;
    Ok(Value::Tensor(std::rc::Rc::new(DeviceTensor::new(tensor))))
}

/// A tensor as a Python `memoryview` of float32 with the tensor's shape,
/// which `np.asarray` and `torch.frombuffer` accept without another copy.
pub fn tensor_to_python<'py>(py: Python<'py>, t: &Tensor) -> VmResult<Bound<'py, PyAny>> {
    let values = t.values();
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    let bytes = PyBytes::new(py, &bytes);
    let shape: Vec<usize> = if t.shape.is_empty() {
        vec![1]
    } else {
        t.shape.clone()
    };
    let view = py
        .eval(c"memoryview", None, None)
        .and_then(|mv| mv.call1((bytes,)))
        .and_then(|v| v.call_method1("cast", ("f", shape)))
        .map_err(|e| {
            err(
                "PythonError",
                format!("cannot pass the tensor to Python: {}", e.value(py)),
            )
        })?;
    Ok(view)
}
