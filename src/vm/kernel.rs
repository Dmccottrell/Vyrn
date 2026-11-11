//! `kernel fn`: functions on numbers that also run on whole tensors.
//!
//! ```vyrn
//! kernel fn swish(x: f32, beta: f32) -> f32:
//!     return x / (1.0 + exp(-beta * x))
//! end
//!
//! main:
//!     on gpu[0]:
//!         let y = swish(rand(4096, 4096), 1.5)    # one compute shader
//!     end
//! end
//! ```
//!
//! Called with numbers, a kernel is an ordinary function. Called with
//! tensors (all of one shape) and numbers, it is applied element by element
//! (like a NumPy ufunc): the compiler has lowered its body to a small
//! expression IR, and the IR becomes a WGSL compute shader on a GPU, or is
//! evaluated directly on the CPU. Both paths compute in `f32`, so a result
//! does not depend on where it ran.
//!
//! What lowers: `let` bindings, then `return`; `+ - * / **`, unary `-`,
//! comparisons (giving 1.0 or 0.0), `a if c else b`, and the math builtins
//! `exp`, `tanh`, `sqrt`, `abs`, `relu`, `gelu`, `min`, `max`. Anything else
//! leaves the function usable on numbers, and calling it on tensors says
//! what could not be lowered.

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::ast::*;

/// A kernel's body, lowered.
#[derive(Debug, Clone, PartialEq)]
pub struct Kernel {
    pub name: String,
    pub params: Vec<String>,
    /// `let` bindings in order; `KExpr::Local(i)` reads the i-th.
    pub locals: Vec<KExpr>,
    pub result: KExpr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum KExpr {
    Param(usize),
    Local(usize),
    Const(f32),
    Neg(Box<KExpr>),
    Bin(KOp, Box<KExpr>, Box<KExpr>),
    /// Comparisons give 1.0 or 0.0.
    Cmp(KCmp, Box<KExpr>, Box<KExpr>),
    /// `then if cond != 0 else otherwise`
    Select(Box<KExpr>, Box<KExpr>, Box<KExpr>),
    Call(KFn, Vec<KExpr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KCmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KFn {
    Exp,
    Tanh,
    Sqrt,
    Abs,
    Relu,
    Gelu,
    Min,
    Max,
}

impl KFn {
    fn named(name: &str) -> Option<(KFn, usize)> {
        Some(match name {
            "exp" => (KFn::Exp, 1),
            "tanh" => (KFn::Tanh, 1),
            "sqrt" => (KFn::Sqrt, 1),
            "abs" => (KFn::Abs, 1),
            "relu" => (KFn::Relu, 1),
            "gelu" => (KFn::Gelu, 1),
            "min" => (KFn::Min, 2),
            "max" => (KFn::Max, 2),
            _ => return None,
        })
    }
}

/// Lowers a `kernel fn`. `consts` holds the program's numeric constants,
/// which a kernel may read. The error says what could not be lowered.
pub fn lower(f: &FnDecl, consts: &HashMap<String, f32>) -> Result<Kernel, String> {
    let FnBody::Block(body) = &f.body else {
        return Err("it has no body".into());
    };
    let params: Vec<String> = f.params.iter().map(|p| p.name.name.clone()).collect();
    for p in &f.params {
        let numeric = matches!(&p.declared_ty, Some(TypeExpr { kind: TypeExprKind::Named { path, .. }, .. })
            if path.segments.len() == 1
                && matches!(path.segments[0].name.as_str(),
                    "f32" | "f64" | "float" | "f16" | "bf16" | "int" | "i32" | "i64"));
        if !numeric {
            return Err(format!("parameter `{}` is not a number type", p.name.name));
        }
    }
    let mut lowering = Lowering {
        params: &params,
        locals: Vec::new(),
        names: Vec::new(),
        consts,
    };
    let stmts = &body.stmts;
    let Some((last, init)) = stmts.split_last() else {
        return Err("its body is empty".into());
    };
    for s in init {
        match &s.kind {
            StmtKind::Binding(Binding {
                target: BindTarget::Name(n),
                value: Some(v),
                ..
            }) => {
                let e = lowering.expr(v)?;
                lowering.locals.push(e);
                lowering.names.push(n.name.clone());
            }
            _ => return Err("its body is not `let` bindings followed by one `return`".into()),
        }
    }
    let StmtKind::Return(Some(result)) = &last.kind else {
        return Err("it does not end with `return <value>`".into());
    };
    let result = lowering.expr(result)?;
    let locals = lowering.locals;
    Ok(Kernel {
        name: f.name.name.clone(),
        params,
        locals,
        result,
    })
}

struct Lowering<'a> {
    params: &'a [String],
    locals: Vec<KExpr>,
    names: Vec<String>,
    consts: &'a HashMap<String, f32>,
}

impl Lowering<'_> {
    fn expr(&self, e: &Expr) -> Result<KExpr, String> {
        Ok(match &e.kind {
            ExprKind::Int { value, .. } => KExpr::Const(*value as f32),
            ExprKind::Float { value, .. } => KExpr::Const(*value as f32),
            ExprKind::Bool(b) => KExpr::Const(if *b { 1.0 } else { 0.0 }),
            ExprKind::Name(n) => {
                if let Some(i) = self.names.iter().rposition(|x| *x == n.name) {
                    KExpr::Local(i)
                } else if let Some(i) = self.params.iter().position(|x| *x == n.name) {
                    KExpr::Param(i)
                } else if let Some(&c) = self.consts.get(&n.name) {
                    KExpr::Const(c)
                } else {
                    return Err(format!(
                        "it uses `{}`, which is not a parameter, a `let` or a numeric constant",
                        n.name
                    ));
                }
            }
            ExprKind::Unary {
                op: UnaryOp::Neg,
                operand,
            } => KExpr::Neg(Box::new(self.expr(operand)?)),
            ExprKind::Unary {
                op: UnaryOp::Pos,
                operand,
            } => self.expr(operand)?,
            ExprKind::Binary { op, lhs, rhs } => {
                let op = match op {
                    BinaryOp::Add => KOp::Add,
                    BinaryOp::Sub => KOp::Sub,
                    BinaryOp::Mul => KOp::Mul,
                    BinaryOp::Div => KOp::Div,
                    BinaryOp::Pow => KOp::Pow,
                    other => {
                        return Err(format!("it uses `{other:?}`, which kernels do not support"))
                    }
                };
                KExpr::Bin(op, Box::new(self.expr(lhs)?), Box::new(self.expr(rhs)?))
            }
            ExprKind::Compare { operands, ops } if ops.len() == 1 => {
                let op = match ops[0] {
                    CompareOp::Lt => KCmp::Lt,
                    CompareOp::LtEq => KCmp::Le,
                    CompareOp::Gt => KCmp::Gt,
                    CompareOp::GtEq => KCmp::Ge,
                    CompareOp::Eq => KCmp::Eq,
                    CompareOp::NotEq => KCmp::Ne,
                    _ => return Err("it uses `in`, which kernels do not support".into()),
                };
                KExpr::Cmp(
                    op,
                    Box::new(self.expr(&operands[0])?),
                    Box::new(self.expr(&operands[1])?),
                )
            }
            ExprKind::If {
                then_value,
                condition,
                else_value,
            } => KExpr::Select(
                Box::new(self.expr(condition)?),
                Box::new(self.expr(then_value)?),
                Box::new(self.expr(else_value)?),
            ),
            ExprKind::Call { callee, args } => {
                let ExprKind::Name(n) = &callee.kind else {
                    return Err("it calls something other than a math function".into());
                };
                let Some((f, arity)) = KFn::named(&n.name) else {
                    return Err(format!(
                        "it calls `{}`; kernels can call exp, tanh, sqrt, abs, relu, gelu, min and max",
                        n.name
                    ));
                };
                if args.len() != arity || args.iter().any(|a| a.name.is_some()) {
                    return Err(format!("`{}` takes {arity} argument(s) here", n.name));
                }
                KExpr::Call(
                    f,
                    args.iter()
                        .map(|a| self.expr(&a.value))
                        .collect::<Result<_, _>>()?,
                )
            }
            _ => return Err("it uses an expression kernels do not support".into()),
        })
    }
}

// ==== the CPU path ==========================================================

/// One argument of a call on tensors: a tensor's values or a number.
pub enum Input<'a> {
    Values(&'a [f32]),
    Scalar(f32),
}

fn gelu(x: f32) -> f32 {
    const C: f32 = 0.797_884_6;
    0.5 * x * (1.0 + (C * (x + 0.044_715 * x * x * x)).tanh())
}

impl Kernel {
    /// Applies the kernel to `len` elements on the CPU.
    pub fn run_cpu(&self, inputs: &[Input], len: usize) -> Vec<f32> {
        let mut out = Vec::with_capacity(len);
        let mut params = vec![0.0f32; inputs.len()];
        let mut locals = vec![0.0f32; self.locals.len()];
        for i in 0..len {
            for (p, input) in params.iter_mut().zip(inputs) {
                *p = match input {
                    Input::Values(v) => v[i],
                    Input::Scalar(s) => *s,
                };
            }
            for (j, l) in self.locals.iter().enumerate() {
                locals[j] = eval(l, &params, &locals);
            }
            out.push(eval(&self.result, &params, &locals));
        }
        out
    }

    /// The kernel as a WGSL compute shader. `tensor_params[i]` says whether
    /// parameter `i` is a tensor (read from `input{i}`) or a number (read
    /// from `scalars`). Bindings: 0 output, 1 scalars, 2 sizes, then one per
    /// tensor parameter in order.
    pub fn wgsl(&self, tensor_params: &[bool]) -> String {
        let mut s = String::new();
        s.push_str("struct Sizes { len: u32, pad0: u32, pad1: u32, pad2: u32 }\n");
        s.push_str("@group(0) @binding(0) var<storage, read_write> out: array<f32>;\n");
        s.push_str("@group(0) @binding(1) var<storage, read> scalars: array<f32>;\n");
        s.push_str("@group(0) @binding(2) var<uniform> sizes: Sizes;\n");
        let mut binding = 3;
        for (i, &t) in tensor_params.iter().enumerate() {
            if t {
                let _ = writeln!(
                    s,
                    "@group(0) @binding({binding}) var<storage, read> input{i}: array<f32>;"
                );
                binding += 1;
            }
        }
        s.push_str(
            "fn gelu_(x: f32) -> f32 { let c = 0.7978846; return 0.5 * x * (1.0 + tanh(clamp(c * (x + 0.044715 * x * x * x), -15.0, 15.0))); }\n",
        );
        let _ = writeln!(s, "// kernel fn {}", self.name);
        s.push_str("@compute @workgroup_size(64)\nfn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(num_workgroups) groups: vec3<u32>) {\n");
        s.push_str(
            "    let i = gid.x + gid.y * groups.x * 64u;\n    if (i >= sizes.len) { return; }\n",
        );
        // Every scalar is read so the binding stays in the layout.
        s.push_str("    let unused_ = scalars[0];\n");
        let mut scalar = 0;
        for (i, &t) in tensor_params.iter().enumerate() {
            if t {
                let _ = writeln!(s, "    let p{i} = input{i}[i];");
            } else {
                let _ = writeln!(s, "    let p{i} = scalars[{scalar}u];");
                scalar += 1;
            }
        }
        for (j, l) in self.locals.iter().enumerate() {
            let _ = writeln!(s, "    let l{j} = {};", wgsl_expr(l));
        }
        let _ = writeln!(s, "    out[i] = {};\n}}", wgsl_expr(&self.result));
        s
    }
}

fn eval(e: &KExpr, params: &[f32], locals: &[f32]) -> f32 {
    let b = |v: bool| if v { 1.0 } else { 0.0 };
    match e {
        KExpr::Param(i) => params[*i],
        KExpr::Local(i) => locals[*i],
        KExpr::Const(c) => *c,
        KExpr::Neg(x) => -eval(x, params, locals),
        KExpr::Bin(op, x, y) => {
            let (x, y) = (eval(x, params, locals), eval(y, params, locals));
            match op {
                KOp::Add => x + y,
                KOp::Sub => x - y,
                KOp::Mul => x * y,
                KOp::Div => x / y,
                KOp::Pow => x.powf(y),
            }
        }
        KExpr::Cmp(op, x, y) => {
            let (x, y) = (eval(x, params, locals), eval(y, params, locals));
            b(match op {
                KCmp::Lt => x < y,
                KCmp::Le => x <= y,
                KCmp::Gt => x > y,
                KCmp::Ge => x >= y,
                KCmp::Eq => x == y,
                KCmp::Ne => x != y,
            })
        }
        KExpr::Select(c, t, f) => {
            if eval(c, params, locals) != 0.0 {
                eval(t, params, locals)
            } else {
                eval(f, params, locals)
            }
        }
        KExpr::Call(f, args) => {
            let a = |i: usize| eval(&args[i], params, locals);
            match f {
                KFn::Exp => a(0).exp(),
                KFn::Tanh => a(0).tanh(),
                KFn::Sqrt => a(0).sqrt(),
                KFn::Abs => a(0).abs(),
                KFn::Relu => a(0).max(0.0),
                KFn::Gelu => gelu(a(0)),
                KFn::Min => a(0).min(a(1)),
                KFn::Max => a(0).max(a(1)),
            }
        }
    }
}

fn wgsl_expr(e: &KExpr) -> String {
    match e {
        KExpr::Param(i) => format!("p{i}"),
        KExpr::Local(i) => format!("l{i}"),
        KExpr::Const(c) => {
            let text = format!("{c:?}");
            if text.contains(['.', 'e', 'E']) || text.contains("inf") || text.contains("NaN") {
                format!("f32({text})")
            } else {
                format!("{text}.0")
            }
        }
        KExpr::Neg(x) => format!("(-{})", wgsl_expr(x)),
        KExpr::Bin(op, x, y) => {
            let (x, y) = (wgsl_expr(x), wgsl_expr(y));
            match op {
                KOp::Add => format!("({x} + {y})"),
                KOp::Sub => format!("({x} - {y})"),
                KOp::Mul => format!("({x} * {y})"),
                KOp::Div => format!("({x} / {y})"),
                // `pow` is undefined for negative bases in WGSL, where the
                // CPU's `powf` handles whole exponents; both agree on the
                // rest.
                KOp::Pow => format!("pow({x}, {y})"),
            }
        }
        KExpr::Cmp(op, x, y) => {
            let o = match op {
                KCmp::Lt => "<",
                KCmp::Le => "<=",
                KCmp::Gt => ">",
                KCmp::Ge => ">=",
                KCmp::Eq => "==",
                KCmp::Ne => "!=",
            };
            format!("select(0.0, 1.0, {} {o} {})", wgsl_expr(x), wgsl_expr(y))
        }
        KExpr::Select(c, t, f) => format!(
            "select({}, {}, {} != 0.0)",
            wgsl_expr(f),
            wgsl_expr(t),
            wgsl_expr(c)
        ),
        KExpr::Call(f, args) => {
            let a: Vec<String> = args.iter().map(wgsl_expr).collect();
            match f {
                KFn::Exp => format!("exp({})", a[0]),
                KFn::Tanh => format!("tanh(clamp({}, -15.0, 15.0))", a[0]),
                KFn::Sqrt => format!("sqrt({})", a[0]),
                KFn::Abs => format!("abs({})", a[0]),
                KFn::Relu => format!("max({}, 0.0)", a[0]),
                KFn::Gelu => format!("gelu_({})", a[0]),
                KFn::Min => format!("min({}, {})", a[0], a[1]),
                KFn::Max => format!("max({}, {})", a[0], a[1]),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel(src: &str) -> Result<Kernel, String> {
        let module = crate::parser::parse(src).unwrap();
        let ItemKind::Fn(f) = &module.items[0].kind else {
            panic!("not a function")
        };
        lower(f, &HashMap::from([("BETA".to_string(), 2.0)]))
    }

    #[test]
    fn kernels_lower_and_run() {
        let k = kernel("kernel fn swish(x: f32, beta: f32) -> f32:\n    let s = 1.0 / (1.0 + exp(-beta * x))\n    return x * s if x > -20.0 else 0.0\nend\n").unwrap();
        let out = k.run_cpu(&[Input::Values(&[0.0, 1.0, -30.0]), Input::Scalar(1.0)], 3);
        assert_eq!(out[0], 0.0);
        assert!((out[1] - 0.731_058_6).abs() < 1e-6);
        assert_eq!(out[2], 0.0);
        let wgsl = k.wgsl(&[true, false]);
        assert!(wgsl.contains("let p0 = input0[i];"), "{wgsl}");
        assert!(wgsl.contains("let p1 = scalars[0u];"), "{wgsl}");
        assert!(wgsl.contains("let l0 = "), "{wgsl}");
        let k = kernel("kernel fn f(x: f32) -> f32: return x * BETA\n").unwrap();
        assert_eq!(k.run_cpu(&[Input::Scalar(3.0)], 1), vec![6.0]);
    }

    #[test]
    fn unsupported_bodies_say_why() {
        let e =
            kernel("kernel fn f(x: f32) -> f32:\n    print(x)\n    return x\nend\n").unwrap_err();
        assert_eq!(e, "its body is not `let` bindings followed by one `return`");
        let e = kernel("kernel fn f(x: f32) -> f32: return sin(x)\n").unwrap_err();
        assert!(e.contains("it calls `sin`"), "{e}");
        let e = kernel("kernel fn f(xs: [f32]) -> f32: return 1.0\n").unwrap_err();
        assert_eq!(e, "parameter `xs` is not a number type");
    }
}
