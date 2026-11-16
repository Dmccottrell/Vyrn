//! Tensors across the Python boundary.
//!
//! Python's buffer protocol is the common ground: NumPy arrays, PyTorch
//! CPU tensors (through `.numpy()`), `memoryview`, `array.array` and
//! `bytes` all expose their memory through it, with a shape and an element
//! format. Vyrn reads that description and takes the memory without going
//! through Python objects element by element.

use std::any::Any;
use std::rc::Rc;

use pyo3::buffer::{PyBuffer, PyUntypedBuffer};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use super::dtype::{DType, External, Storage};
use super::ops::{err, VmResult};
use super::tensor::{Device, DeviceTensor, Tensor};
use super::value::Value;

/// A Python array as a native tensor, without copying: `x.to_tensor()`.
///
/// Anything with a buffer of `float32` or `float16` values works (NumPy
/// arrays, `memoryview`, `array.array`), and so does anything with
/// `.numpy()` (PyTorch CPU tensors). The tensor reads the array's memory
/// in place: Vyrn holds the Python buffer, which keeps the array alive and
/// its memory where it is, until the last tensor using it is gone. If the
/// program changes the tensor, it gets its own copy first. Arrays that are
/// not contiguous, are big-endian, or hold other types are refused with a
/// fix, rather than silently copied (no hidden costs).
pub fn tensor_from_python(py: Python<'_>, object: &Bound<'_, PyAny>) -> VmResult<Value> {
    let fail = |message: String| err("PythonError", message);
    let mut object = object.clone();
    if PyUntypedBuffer::get(&object).is_err() {
        if let Ok(array) = object.call_method0("numpy") {
            object = array;
        }
    }
    let buffer = PyUntypedBuffer::get(&object).map_err(|e| {
        fail(format!(
            "this Python object does not share its memory ({}); pass a NumPy array",
            e.value(py)
        ))
    })?;
    let format = buffer.format().to_string_lossy().to_string();
    let dtype = match format.trim_start_matches(['@', '=', '<']) {
        "f" if cfg!(target_endian = "little") || !format.starts_with('<') => DType::F32,
        "e" if cfg!(target_endian = "little") || !format.starts_with('<') => DType::F16,
        _ => {
            return Err(fail(format!(
                "the Python array holds `{format}` values; convert it with `.astype(\"float32\")` first"
            )))
        }
    };
    if !buffer.is_c_contiguous() {
        return Err(fail(
            "the Python array is not contiguous in memory; pass `np.ascontiguousarray(x)`".into(),
        ));
    }
    let shape = buffer.shape().to_vec();
    let (ptr, bytes) = (buffer.buf_ptr() as *const u8, buffer.len_bytes());
    let owner: Rc<dyn Any> = Rc::new(buffer);
    // SAFETY: a held Python buffer keeps the exporter's memory valid and
    // its size fixed until it is released, which happens when `owner`,
    // stored in the tensor, is dropped.
    let storage = match unsafe { External::new(ptr, bytes, dtype, owner) } {
        Some(e) => Storage::External(Rc::new(e)),
        // Misaligned memory, which NumPy never produces: read it out.
        None => {
            let values: Vec<f32> = match dtype {
                DType::F16 => object
                    .call_method1("astype", ("float32",))
                    .and_then(|a| PyBuffer::<f32>::get(&a).and_then(|b| b.to_vec(py))),
                _ => PyBuffer::<f32>::get(&object).and_then(|b| b.to_vec(py)),
            }
            .map_err(|e| fail(format!("cannot read the Python array: {}", e.value(py))))?;
            Storage::F32(values)
        }
    };
    let expected: usize = shape.iter().product();
    if expected != storage.len() {
        return Err(fail(format!(
            "the Python array's shape {shape:?} does not match its data"
        )));
    }
    let tensor = Tensor {
        shape,
        storage,
        device: Device::Cpu,
    };
    Ok(Value::Tensor(Rc::new(DeviceTensor::new(tensor))))
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
