//! A GPU backend on wgpu (Vulkan, Metal or DirectX 12), built with the
//! `gpu` cargo feature.
//!
//! Matrix products, element-wise arithmetic, maps (`exp`, `gelu`, ...) and
//! `softmax` run as WGSL compute shaders. Weights are read in their stored
//! form: `f16`, `bf16`, `q8` and `q4` matrices are uploaded packed and
//! converted inside the shader, so a 4-bit model takes 4-bit memory on the
//! GPU. Large weight matrices stay resident on the GPU between calls and
//! are released when the program drops the tensor.
//!
//! Operations the shaders do not cover (reductions, `**`, broadcasting
//! other than with a scalar, very large single buffers), and work too small
//! to be worth a dispatch, run on the CPU reference backend; the result is
//! the same either way.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::dtype::{DType, Storage};
use super::tensor::{
    ElemOp, MapOp, MatmulPlan, Reduce, ReferenceBackend, Tensor, TensorBackend, TensorError,
};

/// How the right-hand side of a matrix product is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Format {
    F32,
    F16,
    BF16,
    Q8,
    Q4,
}

const FORMATS: [Format; 5] = [
    Format::F32,
    Format::F16,
    Format::BF16,
    Format::Q8,
    Format::Q4,
];

impl Format {
    fn of(storage: &Storage) -> Format {
        match storage {
            Storage::F32(_) => Format::F32,
            Storage::F16(_) => Format::F16,
            Storage::BF16(_) => Format::BF16,
            Storage::Quantized(q) if q.dtype == DType::Q4 => Format::Q4,
            Storage::Quantized(_) => Format::Q8,
            Storage::External(e) => match e.dtype() {
                DType::F16 => Format::F16,
                DType::BF16 => Format::BF16,
                _ => Format::F32,
            },
        }
    }

    /// WGSL that reads element `i` of `b` as an f32.
    fn loader(self) -> &'static str {
        match self {
            Format::F32 => "fn load_b(i: u32) -> f32 { return b[i]; }",
            Format::F16 => {
                "fn load_b(i: u32) -> f32 { return unpack2x16float(b[i >> 1u])[i & 1u]; }"
            }
            Format::BF16 => {
                "fn load_b(i: u32) -> f32 {
                    let h = (b[i >> 1u] >> (16u * (i & 1u))) & 0xffffu;
                    return bitcast<f32>(h << 16u);
                }"
            }
            Format::Q8 => {
                "fn load_b(i: u32) -> f32 {
                    let byte = (b[i >> 2u] >> (8u * (i & 3u))) & 0xffu;
                    let q = bitcast<i32>(byte << 24u) >> 24u;
                    return f32(q) * block_scale(i);
                }"
            }
            Format::Q4 => {
                "fn load_b(i: u32) -> f32 {
                    let nibble = (b[i >> 3u] >> (4u * (i & 7u))) & 0xfu;
                    return (f32(nibble) - 8.0) * block_scale(i);
                }"
            }
        }
    }

    fn element(self) -> &'static str {
        match self {
            Format::F32 => "f32",
            _ => "u32",
        }
    }
}

const MATMUL: &str = r#"
struct Params { m: u32, n: u32, k: u32, total: u32 }
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read> b: array<ELEMENT>;
@group(0) @binding(2) var<storage, read> scales: array<u32>;
@group(0) @binding(3) var<storage, read_write> out: array<f32>;
@group(0) @binding(4) var<storage, read> offsets: array<u32>;
@group(0) @binding(5) var<uniform> params: Params;

fn block_scale(i: u32) -> f32 {
    let block = i / 32u;
    return unpack2x16float(scales[block >> 1u])[block & 1u];
}

LOADER

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>,
        @builtin(num_workgroups) groups: vec3<u32>) {
    let idx = gid.x + gid.y * groups.x * 64u;
    if (idx >= params.total) {
        // Keeps `scales` in the layout for formats that do not read it.
        if (idx == 0xffffffffu) { out[0] = f32(scales[0]); }
        return;
    }
    let j = idx % params.n;
    let i = (idx / params.n) % params.m;
    let batch = idx / (params.n * params.m);
    let a_base = offsets[2u * batch] + i * params.k;
    let b_base = offsets[2u * batch + 1u] + j;
    var sum = 0.0;
    for (var p = 0u; p < params.k; p = p + 1u) {
        let av = a[a_base + p];
        if (av != 0.0) {
            sum = sum + av * load_b(b_base + p * params.n);
        }
    }
    out[idx] = sum;
}
"#;

const ELEMENTWISE: &str = r#"
struct Params { len: u32, op: u32, mode: u32, pad: u32 }
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read> b: array<f32>;
@group(0) @binding(2) var<storage, read_write> out: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>,
        @builtin(num_workgroups) groups: vec3<u32>) {
    let idx = gid.x + gid.y * groups.x * 64u;
    if (idx >= params.len) { return; }
    var x = a[0];
    var y = b[0];
    if (params.mode != 2u) { x = a[idx]; }
    if (params.mode != 1u) { y = b[idx]; }
    var r = 0.0;
    switch params.op {
        case 0u: { r = x + y; }
        case 1u: { r = x - y; }
        case 2u: { r = x * y; }
        case 3u: { r = x / y; }
        case 5u: { r = select(0.0, 1.0, x < y); }
        case 6u: { r = select(0.0, 1.0, x <= y); }
        case 7u: { r = select(0.0, 1.0, x > y); }
        default: { r = select(0.0, 1.0, x >= y); }
    }
    out[idx] = r;
}
"#;

const MAP: &str = r#"
struct Params { len: u32, op: u32, pad0: u32, pad1: u32 }
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read_write> out: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>,
        @builtin(num_workgroups) groups: vec3<u32>) {
    let idx = gid.x + gid.y * groups.x * 64u;
    if (idx >= params.len) { return; }
    let x = a[idx];
    var r = 0.0;
    switch params.op {
        case 0u: { r = -x; }
        case 1u: { r = abs(x); }
        case 2u: { r = exp(x); }
        case 3u: { r = sqrt(x); }
        // tanh saturates long before 15; clamping keeps every driver finite.
        case 4u: { r = tanh(clamp(x, -15.0, 15.0)); }
        case 5u: { r = max(x, 0.0); }
        default: {
            let c = 0.7978846;
            r = 0.5 * x * (1.0 + tanh(clamp(c * (x + 0.044715 * x * x * x), -15.0, 15.0)));
        }
    }
    out[idx] = r;
}
"#;

const SOFTMAX: &str = r#"
struct Params { outer: u32, len: u32, inner: u32, pad: u32 }
@group(0) @binding(0) var<storage, read> a: array<f32>;
@group(0) @binding(1) var<storage, read_write> out: array<f32>;
@group(0) @binding(2) var<uniform> params: Params;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>,
        @builtin(num_workgroups) groups: vec3<u32>) {
    let t = gid.x + gid.y * groups.x * 64u;
    if (t >= params.outer * params.inner) { return; }
    let o = t / params.inner;
    let i = t % params.inner;
    let base = o * params.len * params.inner + i;
    var hi = -3.4028235e38;
    for (var j = 0u; j < params.len; j = j + 1u) {
        hi = max(hi, a[base + j * params.inner]);
    }
    var sum = 0.0;
    for (var j = 0u; j < params.len; j = j + 1u) {
        let e = exp(a[base + j * params.inner] - hi);
        out[base + j * params.inner] = e;
        sum = sum + e;
    }
    for (var j = 0u; j < params.len; j = j + 1u) {
        out[base + j * params.inner] = out[base + j * params.inner] / sum;
    }
}
"#;

thread_local! {
    /// Whether a GPU backend is caching weights, so dropped tensors should
    /// be reported.
    static CACHING: Cell<bool> = const { Cell::new(false) };
    /// Storage of tensors dropped or changed since the backend last looked.
    static RELEASED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// The key a resident copy of `storage` is cached under.
fn storage_key(storage: &Storage) -> usize {
    match storage {
        Storage::F32(v) => v.as_ptr() as usize,
        Storage::F16(v) | Storage::BF16(v) => v.as_ptr() as usize,
        Storage::Quantized(q) => q.values.as_ptr() as usize,
        Storage::External(e) => e.as_bytes().as_ptr() as usize,
    }
}

/// Called when a tensor's storage is dropped or written, so a resident
/// copy of it is not used again.
pub fn release(storage: &Storage) {
    if CACHING.with(Cell::get) {
        RELEASED.with(|r| r.borrow_mut().push(storage_key(storage)));
    }
}

/// A weight matrix kept on the GPU.
struct Resident {
    /// Length and format, checked on every use.
    shape: (usize, Format),
    values: wgpu::Buffer,
    scales: wgpu::Buffer,
}

/// Matrices at least this large stay on the GPU between calls.
const RESIDENT_BYTES: usize = 256 << 10;

pub struct WgpuBackend {
    device: wgpu::Device,
    queue: wgpu::Queue,
    name: String,
    matmul: HashMap<Format, wgpu::ComputePipeline>,
    elementwise: wgpu::ComputePipeline,
    map: wgpu::ComputePipeline,
    softmax: wgpu::ComputePipeline,
    resident: RefCell<HashMap<usize, Resident>>,
    /// Compiled `kernel fn` shaders, by WGSL source.
    kernels: RefCell<HashMap<String, wgpu::ComputePipeline>>,
    max_binding: u64,
    /// Operations on fewer elements than this run on the CPU, where they
    /// are cheaper than a dispatch and a copy back.
    pub min_elements: usize,
    dispatches: Cell<u64>,
}

impl WgpuBackend {
    /// Opens the first GPU wgpu finds (a high-performance one if there is
    /// a choice). Fails when there is none.
    pub fn new() -> Result<WgpuBackend, String> {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .map_err(|e| format!("no GPU adapter: {e}"))?;
        let info = adapter.get_info();
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("vyrn"),
            required_limits: limits.clone(),
            ..Default::default()
        }))
        .map_err(|e| format!("cannot open {}: {e}", info.name))?;
        let pipeline = |source: &str| {
            let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: None,
                source: wgpu::ShaderSource::Wgsl(source.into()),
            });
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: None,
                layout: None,
                module: &module,
                entry_point: Some("main"),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        let matmul = FORMATS
            .iter()
            .map(|&f| {
                let source = MATMUL
                    .replace("ELEMENT", f.element())
                    .replace("LOADER", f.loader());
                (f, pipeline(&source))
            })
            .collect();
        let backend = WgpuBackend {
            elementwise: pipeline(ELEMENTWISE),
            map: pipeline(MAP),
            softmax: pipeline(SOFTMAX),
            matmul,
            device,
            queue,
            name: format!("wgpu: {} ({:?})", info.name, info.backend),
            resident: RefCell::new(HashMap::new()),
            kernels: RefCell::new(HashMap::new()),
            max_binding: limits
                .max_storage_buffer_binding_size
                .min(limits.max_buffer_size),
            min_elements: 4096,
            dispatches: Cell::new(0),
        };
        CACHING.with(|c| c.set(true));
        Ok(backend)
    }

    /// Compute dispatches run so far.
    pub fn dispatches(&self) -> u64 {
        self.dispatches.get()
    }

    /// Bytes of weights kept on the GPU.
    pub fn resident_bytes(&self) -> u64 {
        self.drop_released();
        self.resident
            .borrow()
            .values()
            .map(|r| r.values.size() + r.scales.size())
            .sum()
    }

    fn drop_released(&self) {
        let released = RELEASED.with(|r| std::mem::take(&mut *r.borrow_mut()));
        if !released.is_empty() {
            let mut resident = self.resident.borrow_mut();
            for key in released {
                resident.remove(&key);
            }
        }
    }

    fn buffer(&self, bytes: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
        // Bindings need a non-empty size that is a multiple of 4.
        let size = bytes.len().max(4).next_multiple_of(4) as u64;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if bytes.len().is_multiple_of(4) {
            self.queue.write_buffer(&buffer, 0, bytes);
        } else {
            let mut padded = bytes.to_vec();
            padded.resize(size as usize, 0);
            self.queue.write_buffer(&buffer, 0, &padded);
        }
        buffer
    }

    fn storage(&self, bytes: &[u8]) -> wgpu::Buffer {
        self.buffer(bytes, wgpu::BufferUsages::STORAGE)
    }

    fn uniform(&self, words: [u32; 4]) -> wgpu::Buffer {
        self.buffer(&u32_bytes(&words), wgpu::BufferUsages::UNIFORM)
    }

    fn output(&self, len: usize) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (len.max(1) * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })
    }

    /// Whether buffers of these sizes fit in one binding.
    fn fits(&self, bytes: usize) -> bool {
        (bytes as u64) <= self.max_binding && bytes < u32::MAX as usize
    }

    /// Runs `pipeline` over `threads` invocations and reads back `out`.
    fn run(
        &self,
        pipeline: &wgpu::ComputePipeline,
        buffers: &[&wgpu::Buffer],
        threads: usize,
        out: &wgpu::Buffer,
        len: usize,
    ) -> Vec<f32> {
        let entries: Vec<wgpu::BindGroupEntry> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| wgpu::BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        let groups = threads.div_ceil(64).max(1) as u32;
        let x = groups.min(65_535);
        let y = groups.div_ceil(x);
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (len.max(1) * 4) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(x, y, 1);
        }
        encoder.copy_buffer_to_buffer(out, 0, &staging, 0, (len.max(1) * 4) as u64);
        self.queue.submit([encoder.finish()]);
        self.dispatches.set(self.dispatches.get() + 1);
        let slice = staging.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let data = slice.get_mapped_range().expect("mapped after the wait");
        let values = data
            .chunks_exact(4)
            .take(len)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        drop(data);
        staging.unmap();
        values
    }

    /// The packed values and block scales of a matrix, resident or not.
    fn weights(&self, b: &Tensor, format: Format) -> (wgpu::Buffer, wgpu::Buffer) {
        let (values, scales) = packed(&b.storage);
        if values.len() < RESIDENT_BYTES {
            return (self.storage(&values), self.storage(&scales));
        }
        self.drop_released();
        let key = storage_key(&b.storage);
        let shape = (b.storage.len(), format);
        let mut resident = self.resident.borrow_mut();
        let entry = resident.entry(key);
        let entry = match entry {
            std::collections::hash_map::Entry::Occupied(e) if e.get().shape == shape => {
                e.into_mut()
            }
            other => {
                let fresh = Resident {
                    shape,
                    values: self.storage(&values),
                    scales: self.storage(&scales),
                };
                match other {
                    std::collections::hash_map::Entry::Occupied(mut e) => {
                        e.insert(fresh);
                        e.into_mut()
                    }
                    std::collections::hash_map::Entry::Vacant(e) => e.insert(fresh),
                }
            }
        };
        (entry.values.clone(), entry.scales.clone())
    }
}

fn u32_bytes(words: &[u32]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn u16_bytes(values: &[u16]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Storage as the shaders read it: values, then block scales (empty for
/// unquantized formats).
fn packed(storage: &Storage) -> (Vec<u8>, Vec<u8>) {
    match storage {
        Storage::F32(v) => (f32_bytes(v), Vec::new()),
        Storage::F16(v) | Storage::BF16(v) => (u16_bytes(v), Vec::new()),
        Storage::Quantized(q) => (q.values.clone(), u16_bytes(&q.scales)),
        // Already laid out the way the shaders read it.
        Storage::External(e) => (e.as_bytes().to_vec(), Vec::new()),
    }
}

impl TensorBackend for WgpuBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn matmul(&self, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError> {
        let plan = MatmulPlan::new(a, b)?;
        let (m, k, n) = (plan.m, plan.k, plan.n);
        let total = plan.batches() * m * n;
        let a_values = a.values();
        let b_bytes = b.storage.nbytes();
        if total * k < self.min_elements
            || !self.fits(a_values.len() * 4)
            || !self.fits(b_bytes)
            || !self.fits(total * 4)
        {
            return ReferenceBackend.matmul(a, b);
        }
        let format = Format::of(&b.storage);
        let (b_buffer, scales) = self.weights(b, format);
        let offsets: Vec<u32> = plan
            .a_offsets
            .iter()
            .zip(&plan.b_offsets)
            .flat_map(|(&ao, &bo)| [(ao * m * k) as u32, (bo * k * n) as u32])
            .collect();
        let a_buffer = self.storage(&f32_bytes(&a_values));
        let offsets = self.storage(&u32_bytes(&offsets));
        let params = self.uniform([m as u32, n as u32, k as u32, total as u32]);
        let out = self.output(total);
        let data = self.run(
            &self.matmul[&format],
            &[&a_buffer, &b_buffer, &scales, &out, &offsets, &params],
            total,
            &out,
            total,
        );
        Tensor::new(plan.out_shape, data, a.device)
    }

    fn elementwise(&self, op: ElemOp, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError> {
        let code = match op {
            ElemOp::Add => 0,
            ElemOp::Sub => 1,
            ElemOp::Mul => 2,
            ElemOp::Div => 3,
            ElemOp::Lt => 5,
            ElemOp::LtEq => 6,
            ElemOp::Gt => 7,
            ElemOp::GtEq => 8,
            // `powf` handles negative bases with whole exponents; WGSL's
            // `pow` does not.
            ElemOp::Pow => return ReferenceBackend.elementwise(op, a, b),
        };
        let (alen, blen) = (a.storage.len(), b.storage.len());
        let (mode, shape) = if a.shape == b.shape {
            (0, a.shape.clone())
        } else if blen == 1 && b.ndim() <= a.ndim() {
            (1, a.shape.clone())
        } else if alen == 1 && a.ndim() <= b.ndim() {
            (2, b.shape.clone())
        } else {
            return ReferenceBackend.elementwise(op, a, b);
        };
        let len = alen.max(blen);
        if len < self.min_elements || !self.fits(len * 4) {
            return ReferenceBackend.elementwise(op, a, b);
        }
        let a_buffer = self.storage(&f32_bytes(&a.values()));
        let b_buffer = self.storage(&f32_bytes(&b.values()));
        let params = self.uniform([len as u32, code, mode, 0]);
        let out = self.output(len);
        let data = self.run(
            &self.elementwise,
            &[&a_buffer, &b_buffer, &out, &params],
            len,
            &out,
            len,
        );
        Tensor::new(shape, data, a.device)
    }

    fn map(&self, op: MapOp, a: &Tensor) -> Tensor {
        let len = a.storage.len();
        if len < self.min_elements || !self.fits(len * 4) {
            return ReferenceBackend.map(op, a);
        }
        let code = match op {
            MapOp::Neg => 0,
            MapOp::Abs => 1,
            MapOp::Exp => 2,
            MapOp::Sqrt => 3,
            MapOp::Tanh => 4,
            MapOp::Relu => 5,
            MapOp::Gelu => 6,
        };
        let input = self.storage(&f32_bytes(&a.values()));
        let params = self.uniform([len as u32, code, 0, 0]);
        let out = self.output(len);
        let data = self.run(&self.map, &[&input, &out, &params], len, &out, len);
        Tensor {
            shape: a.shape.clone(),
            storage: Storage::F32(data),
            device: a.device,
        }
    }

    fn reduce(&self, op: Reduce, a: &Tensor, axis: Option<i64>) -> Result<Tensor, TensorError> {
        ReferenceBackend.reduce(op, a, axis)
    }

    fn softmax(&self, a: &Tensor, axis: i64) -> Result<Tensor, TensorError> {
        let len = a.storage.len();
        if a.ndim() == 0 || len < self.min_elements || !self.fits(len * 4) {
            return ReferenceBackend.softmax(a, axis);
        }
        let n = a.ndim() as i64;
        let ax = if axis < 0 { axis + n } else { axis };
        if !(0..n).contains(&ax) {
            return ReferenceBackend.softmax(a, axis);
        }
        let ax = ax as usize;
        let outer: usize = a.shape[..ax].iter().product();
        let inner: usize = a.shape[ax + 1..].iter().product();
        let input = self.storage(&f32_bytes(&a.values()));
        let params = self.uniform([outer as u32, a.shape[ax] as u32, inner as u32, 0]);
        let out = self.output(len);
        let data = self.run(
            &self.softmax,
            &[&input, &out, &params],
            outer * inner,
            &out,
            len,
        );
        Tensor::new(a.shape.clone(), data, a.device)
    }

    fn transfer(&self, a: &Tensor, to: super::tensor::Device) -> Result<Tensor, TensorError> {
        ReferenceBackend.transfer(a, to)
    }

    /// A `kernel fn` on tensors: its WGSL is generated for this pattern of
    /// tensor and number arguments, compiled once, and dispatched with one
    /// thread per element.
    fn kernel(
        &self,
        kernel: &super::kernel::Kernel,
        args: &[super::tensor::KernelArg],
        len: usize,
    ) -> Option<Vec<f32>> {
        use super::tensor::KernelArg;
        if len < self.min_elements || !self.fits(len * 4) {
            return None;
        }
        let mask: Vec<bool> = args
            .iter()
            .map(|a| matches!(a, KernelArg::Tensor(_)))
            .collect();
        let source = kernel.wgsl(&mask);
        let mut cache = self.kernels.borrow_mut();
        let pipeline = cache.entry(source.clone()).or_insert_with(|| {
            let module = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&kernel.name),
                    source: wgpu::ShaderSource::Wgsl(source.as_str().into()),
                });
            self.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(&kernel.name),
                    layout: None,
                    module: &module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                })
        });
        let mut scalars: Vec<f32> = args
            .iter()
            .filter_map(|a| match a {
                KernelArg::Scalar(s) => Some(*s),
                KernelArg::Tensor(_) => None,
            })
            .collect();
        if scalars.is_empty() {
            scalars.push(0.0);
        }
        let out = self.output(len);
        let scalars = self.storage(&f32_bytes(&scalars));
        let sizes = self.uniform([len as u32, 0, 0, 0]);
        let mut inputs = Vec::new();
        for a in args {
            if let KernelArg::Tensor(t) = a {
                if !self.fits(t.storage.len() * 4) {
                    return None;
                }
                inputs.push(self.storage(&f32_bytes(&t.values())));
            }
        }
        let mut buffers: Vec<&wgpu::Buffer> = vec![&out, &scalars, &sizes];
        buffers.extend(inputs.iter());
        let pipeline = pipeline.clone();
        drop(cache);
        Some(self.run(&pipeline, &buffers, len, &out, len))
    }
}
