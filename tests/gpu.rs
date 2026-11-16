//! The wgpu backend must agree with the CPU reference. Runs with
//! `cargo test --features gpu`; without a GPU (or a software Vulkan driver
//! such as lavapipe) the tests report that and pass.
#![cfg(feature = "gpu")]

use vyrn::vm::dtype::{DType, Storage};
use vyrn::vm::gpu::WgpuBackend;
use vyrn::vm::tensor::{DeviceTensor, ElemOp, MapOp, ReferenceBackend};
use vyrn::vm::{compile_source, Device, Tensor, TensorBackend, Value, Vm};

fn backend() -> Option<WgpuBackend> {
    match WgpuBackend::new() {
        Ok(mut b) => {
            // Send everything to the GPU, however small.
            b.min_elements = 0;
            Some(b)
        }
        Err(e) => {
            eprintln!("skipping: {e}");
            None
        }
    }
}

/// Deterministic values in roughly [-2, 2].
fn data(n: usize, seed: u64) -> Vec<f32> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..n)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            ((state >> 40) as f32 / (1u64 << 24) as f32) * 4.0 - 2.0
        })
        .collect()
}

fn tensor(shape: &[usize], seed: u64, dtype: DType) -> Tensor {
    let n = shape.iter().product();
    Tensor {
        shape: shape.to_vec(),
        storage: Storage::from_f32(data(n, seed), dtype),
        device: Device::Gpu(0),
    }
}

fn assert_close(gpu: &Tensor, cpu: &Tensor, what: &str) {
    assert_eq!(gpu.shape, cpu.shape, "{what}: shapes");
    let (g, c) = (gpu.to_vec(), cpu.to_vec());
    for (i, (x, y)) in g.iter().zip(&c).enumerate() {
        let tolerance = 1e-4 * (1.0 + y.abs());
        assert!(
            (x - y).abs() <= tolerance || (x.is_nan() && y.is_nan()),
            "{what}: element {i}: gpu {x} vs cpu {y}"
        );
    }
}

#[test]
fn matmul_matches_the_reference_in_every_format() {
    let Some(gpu) = backend() else { return };
    let shapes: &[(&[usize], &[usize])] = &[
        (&[3, 64], &[64, 5]),
        (&[64], &[64, 7]),
        (&[4, 64], &[64]),
        (&[2, 3, 64], &[64, 9]),
        (&[2, 1, 3, 32], &[4, 32, 6]),
        (&[1, 96], &[96, 130]),
    ];
    for dtype in [DType::F32, DType::F16, DType::BF16, DType::Q8, DType::Q4] {
        for (i, (sa, sb)) in shapes.iter().enumerate() {
            let a = tensor(sa, i as u64 + 1, DType::F32);
            let b = tensor(sb, i as u64 + 100, dtype);
            let expected = ReferenceBackend.matmul(&a, &b).unwrap();
            let got = gpu.matmul(&a, &b).unwrap();
            assert_close(&got, &expected, &format!("{sa:?} @ {sb:?} as {dtype:?}"));
        }
    }
    assert!(gpu.dispatches() >= 30);
}

#[test]
fn shape_errors_are_the_same() {
    let Some(gpu) = backend() else { return };
    let a = tensor(&[2, 3], 1, DType::F32);
    let b = tensor(&[4, 2], 2, DType::F32);
    assert_eq!(
        gpu.matmul(&a, &b).unwrap_err(),
        ReferenceBackend.matmul(&a, &b).unwrap_err()
    );
}

#[test]
fn elementwise_maps_and_softmax_match() {
    let Some(gpu) = backend() else { return };
    let a = tensor(&[3, 50], 7, DType::F32);
    let b = tensor(&[3, 50], 8, DType::F32);
    let s = tensor(&[1], 9, DType::F32);
    for op in [
        ElemOp::Add,
        ElemOp::Sub,
        ElemOp::Mul,
        ElemOp::Div,
        ElemOp::Pow,
        ElemOp::Lt,
        ElemOp::LtEq,
        ElemOp::Gt,
        ElemOp::GtEq,
    ] {
        for (x, y) in [(&a, &b), (&a, &s), (&s, &b)] {
            assert_close(
                &gpu.elementwise(op, x, y).unwrap(),
                &ReferenceBackend.elementwise(op, x, y).unwrap(),
                &format!("{op:?}"),
            );
        }
    }
    let row = tensor(&[50], 10, DType::F32);
    assert_close(
        &gpu.elementwise(ElemOp::Add, &a, &row).unwrap(),
        &ReferenceBackend.elementwise(ElemOp::Add, &a, &row).unwrap(),
        "broadcast add",
    );
    let positive = Tensor::new(
        vec![3, 50],
        a.to_vec().iter().map(|x| x.abs()).collect(),
        Device::Gpu(0),
    )
    .unwrap();
    for op in [
        MapOp::Neg,
        MapOp::Abs,
        MapOp::Exp,
        MapOp::Tanh,
        MapOp::Relu,
        MapOp::Gelu,
    ] {
        assert_close(
            &gpu.map(op, &a),
            &ReferenceBackend.map(op, &a),
            &format!("{op:?}"),
        );
    }
    assert_close(
        &gpu.map(MapOp::Sqrt, &positive),
        &ReferenceBackend.map(MapOp::Sqrt, &positive),
        "Sqrt",
    );
    let cube = tensor(&[4, 5, 6], 11, DType::F32);
    for axis in [-1, 0, 1, 2] {
        assert_close(
            &gpu.softmax(&cube, axis).unwrap(),
            &ReferenceBackend.softmax(&cube, axis).unwrap(),
            &format!("softmax axis {axis}"),
        );
    }
}

#[test]
fn large_weights_stay_on_the_gpu_until_dropped() {
    let Some(gpu) = backend() else { return };
    let x = tensor(&[1, 1024], 1, DType::F32);
    let w = DeviceTensor::new(tensor(&[1024, 1024], 2, DType::Q4));
    let first = gpu.matmul(&x, &w).unwrap();
    let resident = gpu.resident_bytes();
    // 4 bits a value plus a 2-byte scale per 32 values.
    assert_eq!(resident, 1024 * 1024 / 2 + 1024 * 1024 / 32 * 2);
    let second = gpu.matmul(&x, &w).unwrap();
    assert_eq!(first.to_vec(), second.to_vec());
    assert_eq!(gpu.resident_bytes(), resident);
    drop(w);
    assert_eq!(gpu.resident_bytes(), 0);
}

#[test]
fn programs_run_their_gpu_blocks_on_it() {
    let Some(gpu) = backend() else { return };
    let program = compile_source(
        "main:\n\
         \x20   on gpu[0]:\n\
         \x20       w = tensor([[1.0, 2.0], [3.0, 4.0]], dtype = \"q8\")\n\
         \x20       x = tensor([[1.0, 1.0]])\n\
         \x20       return softmax(x @ w).sum()\n\
         \x20   end\n\
         end\n",
    )
    .unwrap();
    let mut machine = Vm::new(program);
    machine.set_gpu_backend(Box::new(gpu));
    let total = machine.run_main(&[]).unwrap();
    match total {
        Value::Float(t) => assert!((t - 1.0).abs() < 1e-5, "{t}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn generated_kernel_shaders_match_the_cpu() {
    use vyrn::vm::kernel::{lower, Input};
    use vyrn::vm::tensor::KernelArg;
    let Some(gpu) = backend() else { return };
    let module = vyrn::parse(
        "kernel create f(x: f32, y: f32, k: f32) -> f32:\n    let g = 1.0 / (1.0 + exp(-k * x))\n    let h = max(y, -1.0) ** 2.0\n    return x * g + tanh(h) if x > y else gelu(x) - abs(y) + sqrt(abs(x)) + min(x, y)\nend\n",
    )
    .unwrap();
    let vyrn::ast::ItemKind::Fn(f) = &module.items[0].kind else {
        unreachable!()
    };
    let kernel = lower(f, &Default::default()).unwrap();
    let x = tensor(&[37, 41], 3, DType::F32);
    let y = tensor(&[37, 41], 4, DType::F32);
    let len = 37 * 41;
    let got = gpu
        .kernel(
            &kernel,
            &[
                KernelArg::Tensor(&x),
                KernelArg::Tensor(&y),
                KernelArg::Scalar(1.7),
            ],
            len,
        )
        .expect("ran on the GPU");
    let (xv, yv) = (x.to_vec(), y.to_vec());
    let want = kernel.run_cpu(
        &[Input::Values(&xv), Input::Values(&yv), Input::Scalar(1.7)],
        len,
    );
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert!(
            (g - w).abs() <= 1e-4 * (1.0 + w.abs()),
            "element {i}: gpu {g} vs cpu {w}"
        );
    }
}
