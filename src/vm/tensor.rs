//! Tensors, devices, and the backends that execute tensor instructions.
//!
//! The VM never computes tensor math itself. `MatMul`, element-wise
//! arithmetic on tensors, reductions and transfers are all delegated to the
//! [`TensorBackend`] of the device the operands live on. [`ReferenceBackend`]
//! runs everything on the CPU and is the default for every device, so code
//! that targets `gpu[0]` runs anywhere. A CUDA, Metal or WebGPU backend
//! implements the same trait and is installed with
//! [`crate::vm::Vm::set_gpu_backend`].

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::ops::{Deref, DerefMut};

use super::dtype::{DType, Storage};

/// Where a tensor's memory lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Device {
    Cpu,
    Gpu(u16),
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Device::Cpu => write!(f, "cpu"),
            Device::Gpu(i) => write!(f, "gpu:{i}"),
        }
    }
}

/// A dense, row-major tensor.
///
/// Values may be stored as `f32`, `f16`, `bf16`, or block-quantized `q8` and
/// `q4` (see [`super::dtype`]). Every operation computes in `f32`; results
/// are `f32` tensors. Lower precision is for weights, to save memory.
#[derive(Clone, Debug, PartialEq)]
pub struct Tensor {
    pub shape: Vec<usize>,
    pub storage: Storage,
    pub device: Device,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TensorError {
    Shape(String),
    Device(String),
}

impl Tensor {
    pub fn new(shape: Vec<usize>, data: Vec<f32>, device: Device) -> Result<Tensor, TensorError> {
        let expected: usize = shape.iter().product();
        if expected != data.len() {
            return Err(TensorError::Shape(format!(
                "shape {shape:?} needs {expected} values, got {}",
                data.len()
            )));
        }
        Ok(Tensor {
            shape,
            storage: Storage::F32(data),
            device,
        })
    }

    pub fn filled(shape: Vec<usize>, value: f32, device: Device) -> Tensor {
        let n = shape.iter().product();
        Tensor {
            shape,
            storage: Storage::F32(vec![value; n]),
            device,
        }
    }

    pub fn scalar(value: f32, device: Device) -> Tensor {
        Tensor {
            shape: Vec::new(),
            storage: Storage::F32(vec![value]),
            device,
        }
    }

    pub fn dtype(&self) -> DType {
        self.storage.dtype()
    }

    /// Number of elements.
    pub fn numel(&self) -> usize {
        self.storage.len()
    }

    /// Bytes of memory the values occupy.
    pub fn nbytes(&self) -> usize {
        self.storage.nbytes()
    }

    /// The values as `f32`, converted if they are stored another way.
    pub fn values(&self) -> Cow<'_, [f32]> {
        self.storage.values()
    }

    pub fn to_vec(&self) -> Vec<f32> {
        self.values().into_owned()
    }

    pub fn value(&self, i: usize) -> f32 {
        self.storage.value(i)
    }

    /// The values for changing in place. Only `f32` tensors can be changed
    /// element by element.
    pub fn data_mut(&mut self) -> Result<&mut Vec<f32>, TensorError> {
        // Borrowed memory is copied into Vyrn's own on the first change.
        if let Storage::External(e) = &self.storage {
            if e.dtype() == DType::F32 {
                self.storage = Storage::F32(e.f32s().to_vec());
            }
        }
        match &mut self.storage {
            Storage::F32(v) => Ok(v),
            other => Err(TensorError::Shape(format!(
                "a {} tensor cannot be changed element by element; convert it with `.to(f32)` first",
                other.dtype()
            ))),
        }
    }

    /// The same values stored as `dtype`.
    pub fn to_dtype(&self, dtype: DType) -> Tensor {
        if dtype == self.dtype() {
            return self.clone();
        }
        Tensor {
            shape: self.shape.clone(),
            storage: Storage::from_f32(self.to_vec(), dtype),
            device: self.device,
        }
    }

    pub fn ndim(&self) -> usize {
        self.shape.len()
    }

    pub fn strides(&self) -> Vec<usize> {
        strides(&self.shape)
    }

    /// Swaps the last two dimensions. A 0-D or 1-D tensor is unchanged.
    pub fn transpose(&self) -> Tensor {
        let n = self.ndim();
        if n < 2 {
            return self.clone();
        }
        let (rows, cols) = (self.shape[n - 2], self.shape[n - 1]);
        let matrix = rows * cols;
        let values = self.values();
        let mut data = vec![0.0; values.len()];
        for (batch, chunk) in values.chunks(matrix.max(1)).enumerate() {
            let out = &mut data[batch * matrix..(batch + 1) * matrix];
            for r in 0..rows {
                for c in 0..cols {
                    out[c * rows + r] = chunk[r * cols + c];
                }
            }
        }
        let mut shape = self.shape.clone();
        shape.swap(n - 2, n - 1);
        // A transposed matrix of weights keeps its precision.
        Tensor {
            shape,
            storage: Storage::F32(data),
            device: self.device,
        }
        .to_dtype(self.dtype())
    }

    /// The same values, viewed with another shape. The storage, quantized
    /// or not, is kept as is.
    pub fn reshape(&self, shape: Vec<usize>) -> Result<Tensor, TensorError> {
        let expected: usize = shape.iter().product();
        if expected != self.numel() {
            return Err(TensorError::Shape(format!(
                "cannot reshape {} values into {shape:?}",
                self.numel()
            )));
        }
        Ok(Tensor {
            shape,
            storage: self.storage.clone(),
            device: self.device,
        })
    }

    /// Selects elements, Python/NumPy style. The result keeps one dimension
    /// for each slice and new axis, and drops one for each integer index.
    pub fn select(&self, subs: &[Sub]) -> Result<Tensor, TensorError> {
        let ndim = self.ndim();
        let consuming = subs
            .iter()
            .filter(|s| matches!(s, Sub::Index(_) | Sub::Slice { .. }))
            .count();
        if consuming > ndim {
            return Err(TensorError::Shape(format!(
                "{consuming} indices given for a tensor with {ndim} dimensions"
            )));
        }
        let mut expanded = Vec::with_capacity(ndim + 2);
        let mut seen_ellipsis = false;
        for sub in subs {
            if let Sub::Ellipsis = sub {
                if seen_ellipsis {
                    return Err(TensorError::Shape("only one `...` is allowed".into()));
                }
                seen_ellipsis = true;
                for _ in 0..ndim - consuming {
                    expanded.push(Sub::full());
                }
            } else {
                expanded.push(*sub);
            }
        }
        while expanded
            .iter()
            .filter(|s| matches!(s, Sub::Index(_) | Sub::Slice { .. }))
            .count()
            < ndim
        {
            expanded.push(Sub::full());
        }

        let mut out_shape = Vec::new();
        let mut picks: Vec<Vec<usize>> = Vec::with_capacity(ndim);
        let mut dim = 0;
        for sub in &expanded {
            match *sub {
                Sub::NewAxis => out_shape.push(1),
                Sub::Index(i) => {
                    let size = self.shape[dim];
                    let index = normalize_index(i, size).ok_or_else(|| {
                        TensorError::Shape(format!(
                            "index {i} is out of range for dimension {dim} of size {size}"
                        ))
                    })?;
                    picks.push(vec![index]);
                    dim += 1;
                }
                Sub::Slice { start, stop, step } => {
                    let indices = slice_indices(start, stop, step, self.shape[dim])
                        .map_err(TensorError::Shape)?;
                    out_shape.push(indices.len());
                    picks.push(indices);
                    dim += 1;
                }
                Sub::Ellipsis => {}
            }
        }

        let strides = self.strides();
        let total: usize = picks.iter().map(Vec::len).product();
        let mut data = Vec::with_capacity(total);
        if total > 0 {
            // An odometer over the selected indices of every dimension.
            let mut counter = vec![0usize; picks.len()];
            'next: loop {
                let offset: usize = counter
                    .iter()
                    .enumerate()
                    .map(|(d, &c)| picks[d][c] * strides[d])
                    .sum();
                data.push(self.storage.value(offset));
                let mut d = picks.len();
                while d > 0 {
                    d -= 1;
                    counter[d] += 1;
                    if counter[d] < picks[d].len() {
                        continue 'next;
                    }
                    counter[d] = 0;
                }
                break;
            }
        }
        Tensor::new(out_shape, data, self.device)
    }

    /// Writes one element addressed by an integer index per dimension.
    pub fn set(&mut self, index: &[i64], value: f32) -> Result<(), TensorError> {
        if index.len() != self.ndim() {
            return Err(TensorError::Shape(format!(
                "assigning into a tensor needs {} indices, got {}",
                self.ndim(),
                index.len()
            )));
        }
        let strides = self.strides();
        let mut offset = 0;
        for (d, &i) in index.iter().enumerate() {
            let size = self.shape[d];
            let i = normalize_index(i, size).ok_or_else(|| {
                TensorError::Shape(format!("index {i} is out of range for size {size}"))
            })?;
            offset += i * strides[d];
        }
        self.data_mut()?[offset] = value;
        Ok(())
    }
}

/// Widens a tensor element to a scalar without the noise a plain cast adds:
/// `0.1f32 as f64` is 0.10000000149011612, but the element was written and
/// is printed as 0.1, so the scalar should be 0.1 too.
pub fn widen(x: f32) -> f64 {
    if x.fract() == 0.0 || !x.is_finite() {
        return f64::from(x);
    }
    x.to_string().parse().unwrap_or(f64::from(x))
}

// ---- device memory -----------------------------------------------------------

thread_local! {
    /// Bytes held by live tensors, per device.
    static USAGE: RefCell<HashMap<Device, usize>> = RefCell::new(HashMap::new());
    /// Capacity per device, where one is set.
    static LIMITS: RefCell<HashMap<Device, usize>> = RefCell::new(HashMap::new());
}

/// Bytes currently held by tensors on `device`.
pub fn memory_used(device: Device) -> usize {
    USAGE.with(|u| u.borrow().get(&device).copied().unwrap_or(0))
}

/// The capacity set for `device`, if any.
pub fn memory_limit(device: Device) -> Option<usize> {
    LIMITS.with(|l| l.borrow().get(&device).copied())
}

/// Sets the capacity of `device`; `None` removes the limit.
pub fn set_memory_limit(device: Device, limit: Option<usize>) {
    LIMITS.with(|l| match limit {
        Some(bytes) => {
            l.borrow_mut().insert(device, bytes);
        }
        None => {
            l.borrow_mut().remove(&device);
        }
    });
}

/// Fails if `bytes` more would not fit on `device`.
pub fn reserve(device: Device, bytes: usize) -> Result<(), String> {
    let Some(limit) = memory_limit(device) else {
        return Ok(());
    };
    let used = memory_used(device);
    if used + bytes > limit {
        return Err(format!(
            "{device} needs {} for this tensor but only {} of {} is free",
            format_bytes(bytes),
            format_bytes(limit.saturating_sub(used)),
            format_bytes(limit)
        ));
    }
    Ok(())
}

fn track(device: Device, bytes: usize, add: bool) {
    USAGE.with(|u| {
        let mut usage = u.borrow_mut();
        let entry = usage.entry(device).or_insert(0);
        *entry = if add {
            *entry + bytes
        } else {
            entry.saturating_sub(bytes)
        };
    });
}

/// Bytes in a readable unit: `512 B`, `3.5 MB`, `12.0 GB`.
pub fn format_bytes(bytes: usize) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// A tensor held by a program. Its bytes count against its device for as
/// long as it lives, which is what `memory_used` reports and device limits
/// are checked against.
pub struct DeviceTensor(Tensor);

impl DeviceTensor {
    pub fn new(tensor: Tensor) -> DeviceTensor {
        track(tensor.device, tensor.nbytes(), true);
        DeviceTensor(tensor)
    }
}

impl Clone for DeviceTensor {
    fn clone(&self) -> DeviceTensor {
        DeviceTensor::new(self.0.clone())
    }
}

impl Drop for DeviceTensor {
    fn drop(&mut self) {
        #[cfg(feature = "gpu")]
        super::gpu::release(&self.0.storage);
        track(self.0.device, self.0.nbytes(), false);
    }
}

impl Deref for DeviceTensor {
    type Target = Tensor;
    fn deref(&self) -> &Tensor {
        &self.0
    }
}

/// Changes in place keep the size and the device, so the count stays right.
impl DerefMut for DeviceTensor {
    fn deref_mut(&mut self) -> &mut Tensor {
        // A copy of the old contents may be resident on a GPU.
        #[cfg(feature = "gpu")]
        super::gpu::release(&self.0.storage);
        &mut self.0
    }
}

impl fmt::Debug for DeviceTensor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One subscript in a tensor index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sub {
    Index(i64),
    Slice {
        start: Option<i64>,
        stop: Option<i64>,
        step: Option<i64>,
    },
    Ellipsis,
    NewAxis,
}

impl Sub {
    pub fn full() -> Sub {
        Sub::Slice {
            start: None,
            stop: None,
            step: None,
        }
    }
}

pub fn strides(shape: &[usize]) -> Vec<usize> {
    let mut strides = vec![1; shape.len()];
    for d in (0..shape.len().saturating_sub(1)).rev() {
        strides[d] = strides[d + 1] * shape[d + 1];
    }
    strides
}

/// Resolves a possibly negative index against a length.
pub fn normalize_index(index: i64, len: usize) -> Option<usize> {
    let len = len as i64;
    let i = if index < 0 { index + len } else { index };
    (0..len).contains(&i).then_some(i as usize)
}

/// The indices a Python slice selects from a sequence of length `len`.
pub fn slice_indices(
    start: Option<i64>,
    stop: Option<i64>,
    step: Option<i64>,
    len: usize,
) -> Result<Vec<usize>, String> {
    let step = step.unwrap_or(1);
    if step == 0 {
        return Err("a slice step cannot be zero".into());
    }
    let len = len as i64;
    let clamp = |v: i64, low: i64, high: i64| v.max(low).min(high);
    let resolve = |v: i64| if v < 0 { v + len } else { v };
    let mut out = Vec::new();
    if step > 0 {
        let start = start.map(|s| clamp(resolve(s), 0, len)).unwrap_or(0);
        let stop = stop.map(|s| clamp(resolve(s), 0, len)).unwrap_or(len);
        let mut i = start;
        while i < stop {
            out.push(i as usize);
            i += step;
        }
    } else {
        let start = start
            .map(|s| clamp(resolve(s), -1, len - 1))
            .unwrap_or(len - 1);
        let stop = stop.map(|s| clamp(resolve(s), -1, len - 1)).unwrap_or(-1);
        let mut i = start;
        while i > stop {
            out.push(i as usize);
            i += step;
        }
    }
    Ok(out)
}

/// NumPy broadcasting: dimensions are aligned from the right, and a
/// dimension of size 1 stretches to match the other.
pub fn broadcast_shapes(a: &[usize], b: &[usize]) -> Result<Vec<usize>, TensorError> {
    let n = a.len().max(b.len());
    let mut out = vec![0; n];
    for i in 0..n {
        let da = if i < n - a.len() {
            1
        } else {
            a[i - (n - a.len())]
        };
        let db = if i < n - b.len() {
            1
        } else {
            b[i - (n - b.len())]
        };
        out[i] = if da == db || db == 1 {
            da
        } else if da == 1 {
            db
        } else {
            return Err(TensorError::Shape(format!(
                "shapes {a:?} and {b:?} cannot be broadcast together"
            )));
        };
    }
    Ok(out)
}

/// For each position of `out_shape`, the offset into a tensor of `shape`
/// broadcast to it.
pub fn broadcast_offsets(shape: &[usize], out_shape: &[usize]) -> Vec<usize> {
    let total: usize = out_shape.iter().product();
    let pad = out_shape.len() - shape.len();
    let source_strides = strides(shape);
    let mut offsets = Vec::with_capacity(total);
    let mut counter = vec![0usize; out_shape.len()];
    for _ in 0..total {
        let mut offset = 0;
        for (d, &c) in counter.iter().enumerate().skip(pad) {
            let sd = d - pad;
            if shape[sd] != 1 {
                offset += c * source_strides[sd];
            }
        }
        offsets.push(offset);
        for d in (0..counter.len()).rev() {
            counter[d] += 1;
            if counter[d] < out_shape[d] {
                break;
            }
            counter[d] = 0;
        }
    }
    offsets
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElemOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapOp {
    Neg,
    Abs,
    Exp,
    Sqrt,
    Tanh,
    Relu,
    Gelu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reduce {
    Sum,
    Mean,
    Max,
    Min,
    ArgMax,
}

/// The shapes of a matrix product `a @ b`: `[.., m, k] @ [.., k, n]`, with
/// the leading (batch) dimensions broadcast.
pub struct MatmulPlan {
    pub m: usize,
    pub k: usize,
    pub n: usize,
    /// For each batch of the result, the batch of `a` and of `b` it uses.
    pub a_offsets: Vec<usize>,
    pub b_offsets: Vec<usize>,
    pub out_shape: Vec<usize>,
}

impl MatmulPlan {
    pub fn new(a: &Tensor, b: &Tensor) -> Result<MatmulPlan, TensorError> {
        if a.ndim() == 0 || b.ndim() == 0 {
            return Err(TensorError::Shape(
                "`@` needs tensors with at least one dimension; use `*` for scalars".into(),
            ));
        }
        // A vector on the left is a row, a vector on the right a column.
        let a_shape: Vec<usize> = if a.ndim() == 1 {
            vec![1, a.shape[0]]
        } else {
            a.shape.clone()
        };
        let b_shape: Vec<usize> = if b.ndim() == 1 {
            vec![b.shape[0], 1]
        } else {
            b.shape.clone()
        };
        let (m, k) = (a_shape[a_shape.len() - 2], a_shape[a_shape.len() - 1]);
        let (k2, n) = (b_shape[b_shape.len() - 2], b_shape[b_shape.len() - 1]);
        if k != k2 {
            return Err(TensorError::Shape(format!(
                "cannot multiply {:?} @ {:?}: inner dimensions {k} and {k2} differ",
                a.shape, b.shape
            )));
        }
        let a_batch = &a_shape[..a_shape.len() - 2];
        let b_batch = &b_shape[..b_shape.len() - 2];
        let batch = broadcast_shapes(a_batch, b_batch)?;
        let a_offsets = broadcast_offsets(a_batch, &batch);
        let b_offsets = broadcast_offsets(b_batch, &batch);
        let mut out_shape = batch;
        if a.ndim() > 1 {
            out_shape.push(m);
        }
        if b.ndim() > 1 {
            out_shape.push(n);
        }
        Ok(MatmulPlan {
            m,
            k,
            n,
            a_offsets,
            b_offsets,
            out_shape,
        })
    }

    pub fn batches(&self) -> usize {
        self.a_offsets.len()
    }
}

/// Executes tensor instructions for one kind of device.
pub trait TensorBackend {
    fn name(&self) -> &str;
    /// Matrix product, batched over leading dimensions, which broadcast.
    fn matmul(&self, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError>;
    /// Element-wise arithmetic and comparison with broadcasting.
    fn elementwise(&self, op: ElemOp, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError>;
    fn map(&self, op: MapOp, a: &Tensor) -> Tensor;
    /// Reduces along `axis`, or over every element when `axis` is `None`.
    fn reduce(&self, op: Reduce, a: &Tensor, axis: Option<i64>) -> Result<Tensor, TensorError>;
    fn softmax(&self, a: &Tensor, axis: i64) -> Result<Tensor, TensorError>;
    /// Copies a tensor to another device.
    fn transfer(&self, a: &Tensor, to: Device) -> Result<Tensor, TensorError>;

    /// Applies a `kernel fn` element by element over `len` elements.
    /// `None` leaves it to the CPU evaluator.
    fn kernel(
        &self,
        kernel: &super::kernel::Kernel,
        args: &[KernelArg],
        len: usize,
    ) -> Option<Vec<f32>> {
        let _ = (kernel, args, len);
        None
    }
}

/// An argument of a kernel applied to tensors.
pub enum KernelArg<'a> {
    Tensor(&'a Tensor),
    Scalar(f32),
}

/// Runs every tensor operation on the CPU, for any device. Results are
/// tagged with the operands' device, so placement rules are enforced exactly
/// as they would be with real accelerators.
pub struct ReferenceBackend;

fn normalize_axis(axis: i64, ndim: usize) -> Result<usize, TensorError> {
    let n = ndim as i64;
    let a = if axis < 0 { axis + n } else { axis };
    if (0..n).contains(&a) {
        Ok(a as usize)
    } else {
        Err(TensorError::Shape(format!(
            "axis {axis} is out of range for a tensor with {ndim} dimensions"
        )))
    }
}

/// Splits a shape around `axis` into (outer, length, inner) element counts.
fn around_axis(shape: &[usize], axis: usize) -> (usize, usize, usize) {
    let outer = shape[..axis].iter().product();
    let inner = shape[axis + 1..].iter().product();
    (outer, shape[axis], inner)
}

fn gelu(x: f32) -> f32 {
    const C: f32 = 0.797_884_6; // sqrt(2 / pi)
    0.5 * x * (1.0 + (C * (x + 0.044_715 * x * x * x)).tanh())
}

impl TensorBackend for ReferenceBackend {
    fn name(&self) -> &str {
        "reference (cpu)"
    }

    fn matmul(&self, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError> {
        let plan = MatmulPlan::new(a, b)?;
        let (m, k, n) = (plan.m, plan.k, plan.n);
        let batches = plan.batches();
        let (a_offsets, b_offsets) = (&plan.a_offsets, &plan.b_offsets);
        let mut out = vec![0.0f32; batches * m * n];
        let a_values = a.values();
        // Weights stored at lower precision are converted one row at a
        // time, so a quantized matrix is never expanded in memory.
        let b_values = match &b.storage {
            Storage::F32(v) => Some(v.as_slice()),
            _ => None,
        };
        let mut row = vec![0.0f32; if b_values.is_some() { 0 } else { n }];
        for bi in 0..batches {
            let a_mat = &a_values[a_offsets[bi] * m * k..][..m * k];
            let b_start = b_offsets[bi] * k * n;
            let o_mat = &mut out[bi * m * n..][..m * n];
            // p-outer order: each row of `b` is read once per batch, and
            // the inner loop walks rows of `b` and `out` contiguously, so
            // it vectorizes and stays in cache.
            for p in 0..k {
                let b_row: &[f32] = match b_values {
                    Some(v) => &v[b_start + p * n..][..n],
                    None => {
                        b.storage.values_into(b_start + p * n, &mut row);
                        &row
                    }
                };
                for i in 0..m {
                    let av = a_mat[i * k + p];
                    if av == 0.0 {
                        continue;
                    }
                    let o_row = &mut o_mat[i * n..(i + 1) * n];
                    for (o, &bv) in o_row.iter_mut().zip(b_row) {
                        *o += av * bv;
                    }
                }
            }
        }

        Tensor::new(plan.out_shape, out, a.device)
    }

    fn elementwise(&self, op: ElemOp, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError> {
        let f = |x: f32, y: f32| -> f32 {
            match op {
                ElemOp::Add => x + y,
                ElemOp::Sub => x - y,
                ElemOp::Mul => x * y,
                ElemOp::Div => x / y,
                ElemOp::Pow => x.powf(y),
                ElemOp::Lt => f32::from(u8::from(x < y)),
                ElemOp::LtEq => f32::from(u8::from(x <= y)),
                ElemOp::Gt => f32::from(u8::from(x > y)),
                ElemOp::GtEq => f32::from(u8::from(x >= y)),
            }
        };
        let device = a.device;
        let (av, bv) = (a.values(), b.values());
        if a.shape == b.shape {
            let data = av.iter().zip(bv.iter()).map(|(&x, &y)| f(x, y)).collect();
            return Tensor::new(a.shape.clone(), data, device);
        }
        if bv.len() == 1 && b.ndim() <= a.ndim() {
            let y = bv[0];
            let data = av.iter().map(|&x| f(x, y)).collect();
            return Tensor::new(a.shape.clone(), data, device);
        }
        if av.len() == 1 && a.ndim() <= b.ndim() {
            let x = av[0];
            let data = bv.iter().map(|&y| f(x, y)).collect();
            return Tensor::new(b.shape.clone(), data, device);
        }
        let shape = broadcast_shapes(&a.shape, &b.shape)?;
        let ao = broadcast_offsets(&a.shape, &shape);
        let bo = broadcast_offsets(&b.shape, &shape);
        let data = ao.iter().zip(&bo).map(|(&i, &j)| f(av[i], bv[j])).collect();
        Tensor::new(shape, data, device)
    }

    fn map(&self, op: MapOp, a: &Tensor) -> Tensor {
        let f: fn(f32) -> f32 = match op {
            MapOp::Neg => |x| -x,
            MapOp::Abs => f32::abs,
            MapOp::Exp => f32::exp,
            MapOp::Sqrt => f32::sqrt,
            MapOp::Tanh => f32::tanh,
            MapOp::Relu => |x| x.max(0.0),
            MapOp::Gelu => gelu,
        };
        Tensor {
            shape: a.shape.clone(),
            storage: Storage::F32(a.values().iter().map(|&x| f(x)).collect()),
            device: a.device,
        }
    }

    fn reduce(&self, op: Reduce, a: &Tensor, axis: Option<i64>) -> Result<Tensor, TensorError> {
        let combine = |values: &mut dyn Iterator<Item = f32>| -> f32 {
            match op {
                Reduce::Sum => values.sum(),
                Reduce::Mean => {
                    let (sum, count) = values.fold((0.0, 0usize), |(s, c), v| (s + v, c + 1));
                    if count == 0 {
                        f32::NAN
                    } else {
                        sum / count as f32
                    }
                }
                Reduce::Max => values.fold(f32::NEG_INFINITY, f32::max),
                Reduce::Min => values.fold(f32::INFINITY, f32::min),
                Reduce::ArgMax => {
                    let mut best = (0usize, f32::NEG_INFINITY);
                    for (i, v) in values.enumerate() {
                        if v > best.1 {
                            best = (i, v);
                        }
                    }
                    best.0 as f32
                }
            }
        };
        match axis {
            None => {
                let values = a.values();
                if values.is_empty() && matches!(op, Reduce::Max | Reduce::Min | Reduce::ArgMax) {
                    return Err(TensorError::Shape("reduction of an empty tensor".into()));
                }
                let value = combine(&mut values.iter().copied());
                Ok(Tensor::scalar(value, a.device))
            }
            Some(axis) => {
                let axis = normalize_axis(axis, a.ndim())?;
                let (outer, len, inner) = around_axis(&a.shape, axis);
                let all = a.values();
                let mut data = Vec::with_capacity(outer * inner);
                for o in 0..outer {
                    for i in 0..inner {
                        let mut values = (0..len).map(|j| all[(o * len + j) * inner + i]);
                        data.push(combine(&mut values));
                    }
                }
                let mut shape = a.shape.clone();
                shape.remove(axis);
                Tensor::new(shape, data, a.device)
            }
        }
    }

    fn softmax(&self, a: &Tensor, axis: i64) -> Result<Tensor, TensorError> {
        if a.ndim() == 0 {
            return Ok(Tensor::scalar(1.0, a.device));
        }
        let axis = normalize_axis(axis, a.ndim())?;
        let (outer, len, inner) = around_axis(&a.shape, axis);
        let mut data = a.to_vec();
        for o in 0..outer {
            for i in 0..inner {
                let at = |j: usize| (o * len + j) * inner + i;
                let max = (0..len)
                    .map(|j| data[at(j)])
                    .fold(f32::NEG_INFINITY, f32::max);
                let mut sum = 0.0;
                for j in 0..len {
                    let e = (data[at(j)] - max).exp();
                    data[at(j)] = e;
                    sum += e;
                }
                for j in 0..len {
                    data[at(j)] /= sum;
                }
            }
        }
        Tensor::new(a.shape.clone(), data, a.device)
    }

    fn transfer(&self, a: &Tensor, to: Device) -> Result<Tensor, TensorError> {
        let mut moved = a.clone();
        moved.device = to;
        Ok(moved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(shape: &[usize], data: &[f32]) -> Tensor {
        Tensor::new(shape.to_vec(), data.to_vec(), Device::Cpu).unwrap()
    }

    #[test]
    fn matmul_2d() {
        let a = t(&[2, 3], &[1., 2., 3., 4., 5., 6.]);
        let b = t(&[3, 2], &[7., 8., 9., 10., 11., 12.]);
        let c = ReferenceBackend.matmul(&a, &b).unwrap();
        assert_eq!(c.shape, vec![2, 2]);
        assert_eq!(c.to_vec(), vec![58., 64., 139., 154.]);
    }

    #[test]
    fn matmul_broadcasts_a_shared_weight_over_a_batch() {
        let x = t(&[2, 1, 2], &[1., 2., 3., 4.]);
        let w = t(&[2, 2], &[1., 0., 0., 1.]);
        let y = ReferenceBackend.matmul(&x, &w).unwrap();
        assert_eq!(y.shape, vec![2, 1, 2]);
        assert_eq!(y.to_vec(), x.to_vec());
    }

    #[test]
    fn matmul_with_vectors() {
        let m = t(&[2, 2], &[1., 2., 3., 4.]);
        let v = t(&[2], &[1., 1.]);
        assert_eq!(
            ReferenceBackend.matmul(&m, &v).unwrap().to_vec(),
            vec![3., 7.]
        );
        assert_eq!(
            ReferenceBackend.matmul(&v, &m).unwrap().to_vec(),
            vec![4., 6.]
        );
        assert_eq!(
            ReferenceBackend.matmul(&v, &v).unwrap().shape,
            Vec::<usize>::new()
        );
    }

    #[test]
    fn matmul_rejects_mismatched_shapes() {
        let a = t(&[2, 3], &[0.; 6]);
        assert!(matches!(
            ReferenceBackend.matmul(&a, &a),
            Err(TensorError::Shape(_))
        ));
    }

    #[test]
    fn broadcasting() {
        let a = t(&[2, 3], &[1., 2., 3., 4., 5., 6.]);
        let row = t(&[3], &[10., 20., 30.]);
        let col = t(&[2, 1], &[100., 200.]);
        let r = ReferenceBackend.elementwise(ElemOp::Add, &a, &row).unwrap();
        assert_eq!(r.to_vec(), vec![11., 22., 33., 14., 25., 36.]);
        let c = ReferenceBackend.elementwise(ElemOp::Add, &a, &col).unwrap();
        assert_eq!(c.to_vec(), vec![101., 102., 103., 204., 205., 206.]);
        assert!(broadcast_shapes(&[2, 3], &[2]).is_err());
    }

    #[test]
    fn selection() {
        let a = t(&[2, 3], &[1., 2., 3., 4., 5., 6.]);
        let last_col = a.select(&[Sub::full(), Sub::Index(-1)]).unwrap();
        assert_eq!(
            (last_col.shape.clone(), last_col.to_vec()),
            (vec![2], vec![3., 6.])
        );
        let row = a.select(&[Sub::Index(1)]).unwrap();
        assert_eq!(row.to_vec(), vec![4., 5., 6.]);
        let reversed = a
            .select(&[
                Sub::Ellipsis,
                Sub::Slice {
                    start: None,
                    stop: None,
                    step: Some(-1),
                },
            ])
            .unwrap();
        assert_eq!(reversed.to_vec(), vec![3., 2., 1., 6., 5., 4.]);
        let expanded = a.select(&[Sub::NewAxis]).unwrap();
        assert_eq!(expanded.shape, vec![1, 2, 3]);
        let scalar = a.select(&[Sub::Index(0), Sub::Index(2)]).unwrap();
        assert_eq!((scalar.shape.len(), scalar.value(0)), (0, 3.));
    }

    #[test]
    fn reductions_and_softmax() {
        let a = t(&[2, 2], &[1., 5., 3., 2.]);
        assert_eq!(
            ReferenceBackend
                .reduce(Reduce::Sum, &a, None)
                .unwrap()
                .to_vec(),
            vec![11.]
        );
        assert_eq!(
            ReferenceBackend
                .reduce(Reduce::Sum, &a, Some(0))
                .unwrap()
                .to_vec(),
            vec![4., 7.]
        );
        assert_eq!(
            ReferenceBackend
                .reduce(Reduce::ArgMax, &a, Some(-1))
                .unwrap()
                .to_vec(),
            vec![1., 0.]
        );
        let s = ReferenceBackend.softmax(&a, -1).unwrap();
        assert!((s.value(0) + s.value(1) - 1.0).abs() < 1e-6);
        assert!(s.value(1) > s.value(0));
    }

    #[test]
    fn transpose() {
        let a = t(&[2, 3], &[1., 2., 3., 4., 5., 6.]);
        let at = a.transpose();
        assert_eq!(at.shape, vec![3, 2]);
        assert_eq!(at.to_vec(), vec![1., 4., 2., 5., 3., 6.]);
    }

    #[test]
    fn matmul_with_quantized_weights() {
        let x: Vec<f32> = (0..2 * 64).map(|i| (i % 7) as f32 / 7.0 - 0.4).collect();
        let w: Vec<f32> = (0..64 * 48)
            .map(|i| ((i * 13) % 17) as f32 / 17.0 - 0.5)
            .collect();
        let x = t(&[2, 64], &x);
        let w = t(&[64, 48], &w);
        let exact = ReferenceBackend.matmul(&x, &w).unwrap().to_vec();
        for (dtype, tolerance) in [
            (DType::F16, 0.01),
            (DType::BF16, 0.1),
            (DType::Q8, 0.1),
            (DType::Q4, 1.0),
        ] {
            let packed = w.to_dtype(dtype);
            assert!(packed.nbytes() < w.nbytes());
            let approx = ReferenceBackend.matmul(&x, &packed).unwrap();
            assert_eq!(approx.dtype(), DType::F32);
            for (e, a) in exact.iter().zip(approx.values().iter()) {
                assert!((e - a).abs() < tolerance, "{dtype}: {e} vs {a}");
            }
        }
    }

    #[test]
    fn python_slices() {
        assert_eq!(
            slice_indices(None, None, None, 4).unwrap(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(slice_indices(Some(-2), None, None, 4).unwrap(), vec![2, 3]);
        assert_eq!(
            slice_indices(None, None, Some(-1), 3).unwrap(),
            vec![2, 1, 0]
        );
        assert_eq!(
            slice_indices(Some(1), Some(100), Some(2), 6).unwrap(),
            vec![1, 3, 5]
        );
        assert!(slice_indices(None, None, Some(0), 3).is_err());
    }
}
