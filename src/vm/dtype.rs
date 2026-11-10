//! Tensor element types, and the conversions between them.
//!
//! Compute always happens in `f32`. Lower-precision types exist to save
//! memory: a 14B-parameter model needs about 56 GB as `f32`, 28 GB as
//! `f16`, 15 GB as `q8` and 8 GB as `q4`.
//!
//! The quantized types use the block format of llama.cpp's Q8_0 and Q4_0:
//! values are grouped in blocks of 32, and each block stores one `f16` scale.
//! That costs 8.5 bits per value for `q8` and 4.5 bits for `q4`.

use std::borrow::Cow;
use std::fmt;

/// Values per quantization block.
pub const BLOCK: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DType {
    F32,
    F16,
    BF16,
    /// 8-bit integers with a scale per block of 32.
    Q8,
    /// 4-bit integers with a scale per block of 32.
    Q4,
}

impl DType {
    pub const ALL: [DType; 5] = [DType::F32, DType::F16, DType::BF16, DType::Q8, DType::Q4];

    pub fn name(self) -> &'static str {
        match self {
            DType::F32 => "f32",
            DType::F16 => "f16",
            DType::BF16 => "bf16",
            DType::Q8 => "q8",
            DType::Q4 => "q4",
        }
    }

    pub fn from_name(name: &str) -> Option<DType> {
        DType::ALL.into_iter().find(|d| d.name() == name)
    }
}

impl fmt::Display for DType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ---- half precision --------------------------------------------------------

/// Converts to IEEE 754 half precision, rounding to nearest even.
pub fn f32_to_f16(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32;
    let mantissa = bits & 0x7f_ffff;
    if exponent == 0xff {
        // Infinity, or NaN with a quiet bit set.
        return sign | 0x7c00 | if mantissa != 0 { 0x200 } else { 0 };
    }
    let e = exponent - 127 + 15;
    if e >= 31 {
        return sign | 0x7c00;
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        // Subnormal half: shift the full 24-bit significand into place.
        let significand = mantissa | 0x80_0000;
        let shift = (14 - e) as u32;
        let half = significand >> shift;
        let rest = significand & ((1 << shift) - 1);
        let halfway = 1 << (shift - 1);
        let rounded = if rest > halfway || (rest == halfway && half & 1 == 1) {
            half + 1
        } else {
            half
        };
        return sign | rounded as u16;
    }
    let half_mantissa = mantissa >> 13;
    let rest = mantissa & 0x1fff;
    let mut out = ((e as u32) << 10) | half_mantissa;
    if rest > 0x1000 || (rest == 0x1000 && half_mantissa & 1 == 1) {
        // A carry into the exponent is correct, up to infinity.
        out += 1;
    }
    sign | out as u16
}

pub fn f16_to_f32(h: u16) -> f32 {
    let sign = u32::from(h & 0x8000) << 16;
    let exponent = u32::from((h >> 10) & 0x1f);
    let mantissa = u32::from(h & 0x3ff);
    match exponent {
        0 => {
            let magnitude = mantissa as f32 * 2f32.powi(-24);
            if sign != 0 {
                -magnitude
            } else {
                magnitude
            }
        }
        31 => f32::from_bits(sign | 0x7f80_0000 | (mantissa << 13)),
        _ => f32::from_bits(sign | ((exponent + 112) << 23) | (mantissa << 13)),
    }
}

/// Converts to bfloat16, rounding to nearest even.
pub fn f32_to_bf16(x: f32) -> u16 {
    let bits = x.to_bits();
    if x.is_nan() {
        return ((bits >> 16) | 0x40) as u16;
    }
    let lsb = (bits >> 16) & 1;
    (bits.wrapping_add(0x7fff + lsb) >> 16) as u16
}

pub fn bf16_to_f32(b: u16) -> f32 {
    f32::from_bits(u32::from(b) << 16)
}

// ---- quantization ----------------------------------------------------------

/// Block-quantized values: `q8` or `q4`.
#[derive(Clone, Debug, PartialEq)]
pub struct Quantized {
    pub dtype: DType,
    /// One byte per value for `q8` (two's complement); two values per byte
    /// for `q4`, low nibble first, stored with an offset of 8.
    pub values: Vec<u8>,
    /// One `f16` scale per block.
    pub scales: Vec<u16>,
    /// Number of values.
    pub len: usize,
}

impl Quantized {
    pub fn quantize(data: &[f32], dtype: DType) -> Quantized {
        let blocks = data.len().div_ceil(BLOCK);
        let mut scales = Vec::with_capacity(blocks);
        let mut values = Vec::with_capacity(match dtype {
            DType::Q4 => data.len().div_ceil(2),
            _ => data.len(),
        });
        for block in data.chunks(BLOCK) {
            match dtype {
                DType::Q4 => {
                    // Q4_0: the value of largest magnitude maps to -8.
                    let max =
                        block
                            .iter()
                            .copied()
                            .fold(0.0f32, |m, x| if x.abs() > m.abs() { x } else { m });
                    let d = max / -8.0;
                    let inverse = if d != 0.0 { 1.0 / d } else { 0.0 };
                    scales.push(f32_to_f16(d));
                    let mut nibbles = block
                        .iter()
                        .map(|&x| ((x * inverse + 8.5) as i32).clamp(0, 15) as u8);
                    while let Some(low) = nibbles.next() {
                        let high = nibbles.next().unwrap_or(8);
                        values.push(low | (high << 4));
                    }
                }
                _ => {
                    let amax = block.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                    let d = amax / 127.0;
                    let inverse = if d != 0.0 { 1.0 / d } else { 0.0 };
                    scales.push(f32_to_f16(d));
                    values.extend(
                        block
                            .iter()
                            .map(|&x| ((x * inverse).round() as i32).clamp(-127, 127) as i8 as u8),
                    );
                }
            }
        }
        Quantized {
            dtype: if dtype == DType::Q4 {
                DType::Q4
            } else {
                DType::Q8
            },
            values,
            scales,
            len: data.len(),
        }
    }

    pub fn value(&self, i: usize) -> f32 {
        let d = f16_to_f32(self.scales[i / BLOCK]);
        match self.dtype {
            DType::Q4 => {
                let byte = self.values[i / 2];
                // Blocks are 32 values, so a block never starts mid-byte.
                let nibble = if i.is_multiple_of(2) {
                    byte & 0x0f
                } else {
                    byte >> 4
                };
                (i32::from(nibble) - 8) as f32 * d
            }
            _ => f32::from(self.values[i] as i8) * d,
        }
    }

    /// Writes values `start..start + out.len()` into `out`.
    pub fn dequantize_into(&self, start: usize, out: &mut [f32]) {
        let mut i = start;
        let end = start + out.len();
        let mut o = 0;
        while i < end {
            let block = i / BLOCK;
            let d = f16_to_f32(self.scales[block]);
            let block_end = ((block + 1) * BLOCK).min(end);
            match self.dtype {
                DType::Q4 => {
                    for j in i..block_end {
                        let byte = self.values[j / 2];
                        let nibble = if j.is_multiple_of(2) {
                            byte & 0x0f
                        } else {
                            byte >> 4
                        };
                        out[o] = (i32::from(nibble) - 8) as f32 * d;
                        o += 1;
                    }
                }
                _ => {
                    for &q in &self.values[i..block_end] {
                        out[o] = f32::from(q as i8) * d;
                        o += 1;
                    }
                }
            }
            i = block_end;
        }
    }

    pub fn nbytes(&self) -> usize {
        self.values.len() + self.scales.len() * 2
    }
}

/// Memory that belongs to someone else, such as a NumPy array or a
/// memory-mapped file, read in place: the zero-copy path into Vyrn.
///
/// The owner is kept alive for as long as any tensor uses the memory, so
/// it cannot be freed underneath. A tensor never writes to it: the first
/// change copies the values into memory Vyrn owns (copy on write).
pub struct External {
    ptr: *const u8,
    bytes: usize,
    dtype: DType,
    _owner: std::rc::Rc<dyn std::any::Any>,
}

impl External {
    /// Wraps `bytes` bytes at `ptr` holding values of `dtype` (`f32`,
    /// `f16` or `bf16`). Returns `None` if the memory is misaligned for the
    /// type or not a whole number of values.
    ///
    /// # Safety
    ///
    /// The memory must stay valid, and must not be freed or resized, for as
    /// long as `owner` is alive.
    pub unsafe fn new(
        ptr: *const u8,
        bytes: usize,
        dtype: DType,
        owner: std::rc::Rc<dyn std::any::Any>,
    ) -> Option<External> {
        let size = match dtype {
            DType::F32 => 4,
            DType::F16 | DType::BF16 => 2,
            DType::Q8 | DType::Q4 => return None,
        };
        if ptr.is_null() || !(ptr as usize).is_multiple_of(size) || !bytes.is_multiple_of(size) {
            return None;
        }
        Some(External {
            ptr,
            bytes,
            dtype,
            _owner: owner,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: `new`'s contract keeps the memory valid while `_owner`,
        // which this value holds, is alive.
        unsafe { std::slice::from_raw_parts(self.ptr, self.bytes) }
    }

    /// The values, for `f32` memory.
    pub fn f32s(&self) -> &[f32] {
        debug_assert_eq!(self.dtype, DType::F32);
        // SAFETY: as above; `new` checked the alignment and the length.
        unsafe { std::slice::from_raw_parts(self.ptr as *const f32, self.bytes / 4) }
    }

    /// The raw 16-bit values, for `f16` and `bf16` memory.
    pub fn u16s(&self) -> &[u16] {
        debug_assert_ne!(self.dtype, DType::F32);
        // SAFETY: as above.
        unsafe { std::slice::from_raw_parts(self.ptr as *const u16, self.bytes / 2) }
    }

    pub fn len(&self) -> usize {
        self.bytes / if self.dtype == DType::F32 { 4 } else { 2 }
    }

    pub fn is_empty(&self) -> bool {
        self.bytes == 0
    }

    pub fn dtype(&self) -> DType {
        self.dtype
    }
}

impl fmt::Debug for External {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "External({} bytes of {})", self.bytes, self.dtype)
    }
}

impl PartialEq for External {
    fn eq(&self, other: &External) -> bool {
        self.dtype == other.dtype && self.as_bytes() == other.as_bytes()
    }
}

/// How a tensor's values are stored.
#[derive(Clone, Debug, PartialEq)]
pub enum Storage {
    F32(Vec<f32>),
    F16(Vec<u16>),
    BF16(Vec<u16>),
    Quantized(Quantized),
    /// Read in place from memory Vyrn does not own.
    External(std::rc::Rc<External>),
}

impl Storage {
    pub fn from_f32(data: Vec<f32>, dtype: DType) -> Storage {
        match dtype {
            DType::F32 => Storage::F32(data),
            DType::F16 => Storage::F16(data.iter().map(|&x| f32_to_f16(x)).collect()),
            DType::BF16 => Storage::BF16(data.iter().map(|&x| f32_to_bf16(x)).collect()),
            DType::Q8 | DType::Q4 => Storage::Quantized(Quantized::quantize(&data, dtype)),
        }
    }

    pub fn dtype(&self) -> DType {
        match self {
            Storage::F32(_) => DType::F32,
            Storage::F16(_) => DType::F16,
            Storage::BF16(_) => DType::BF16,
            Storage::Quantized(q) => q.dtype,
            Storage::External(e) => e.dtype,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Storage::F32(v) => v.len(),
            Storage::F16(v) | Storage::BF16(v) => v.len(),
            Storage::Quantized(q) => q.len,
            Storage::External(e) => e.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bytes of memory the values occupy.
    pub fn nbytes(&self) -> usize {
        match self {
            Storage::F32(v) => v.len() * 4,
            Storage::F16(v) | Storage::BF16(v) => v.len() * 2,
            Storage::Quantized(q) => q.nbytes(),
            Storage::External(e) => e.bytes,
        }
    }

    /// The values as `f32`: borrowed when they already are, converted
    /// otherwise.
    pub fn values(&self) -> Cow<'_, [f32]> {
        match self {
            Storage::F32(v) => Cow::Borrowed(v),
            Storage::External(e) if e.dtype == DType::F32 => Cow::Borrowed(e.f32s()),
            other => {
                let mut out = vec![0.0; other.len()];
                other.values_into(0, &mut out);
                Cow::Owned(out)
            }
        }
    }

    /// Writes values `start..start + out.len()` into `out` as `f32`.
    pub fn values_into(&self, start: usize, out: &mut [f32]) {
        match self {
            Storage::F32(v) => out.copy_from_slice(&v[start..start + out.len()]),
            Storage::F16(v) => {
                for (o, &h) in out.iter_mut().zip(&v[start..]) {
                    *o = f16_to_f32(h);
                }
            }
            Storage::BF16(v) => {
                for (o, &b) in out.iter_mut().zip(&v[start..]) {
                    *o = bf16_to_f32(b);
                }
            }
            Storage::Quantized(q) => q.dequantize_into(start, out),
            Storage::External(e) => match e.dtype {
                DType::F32 => out.copy_from_slice(&e.f32s()[start..start + out.len()]),
                DType::F16 => {
                    for (o, &h) in out.iter_mut().zip(&e.u16s()[start..]) {
                        *o = f16_to_f32(h);
                    }
                }
                _ => {
                    for (o, &b) in out.iter_mut().zip(&e.u16s()[start..]) {
                        *o = bf16_to_f32(b);
                    }
                }
            },
        }
    }

    pub fn value(&self, i: usize) -> f32 {
        match self {
            Storage::F32(v) => v[i],
            Storage::F16(v) => f16_to_f32(v[i]),
            Storage::BF16(v) => bf16_to_f32(v[i]),
            Storage::Quantized(q) => q.value(i),
            Storage::External(e) => match e.dtype {
                DType::F32 => e.f32s()[i],
                DType::F16 => f16_to_f32(e.u16s()[i]),
                _ => bf16_to_f32(e.u16s()[i]),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_precision_round_trips() {
        for x in [0.0f32, -0.0, 1.0, -2.5, 0.1, 65504.0, 6.1e-5, 1e-7, 1234.5] {
            let back = f16_to_f32(f32_to_f16(x));
            let tolerance = x.abs() * 1e-3 + 6e-8;
            assert!((back - x).abs() <= tolerance, "{x} -> {back}");
        }
        assert_eq!(f32_to_f16(1.0), 0x3c00);
        assert_eq!(f32_to_f16(-2.0), 0xc000);
        assert_eq!(f32_to_f16(70000.0), 0x7c00);
        assert!(f16_to_f32(f32_to_f16(f32::NAN)).is_nan());
        assert_eq!(f16_to_f32(0x0001), 2f32.powi(-24));
    }

    #[test]
    fn bfloat16_round_trips() {
        for x in [0.0f32, 1.0, -3.0, 0.1, 1e30, -1e-30] {
            let back = bf16_to_f32(f32_to_bf16(x));
            assert!((back - x).abs() <= x.abs() * 1e-2, "{x} -> {back}");
        }
        assert_eq!(f32_to_bf16(1.0), 0x3f80);
    }

    #[test]
    fn quantization_error_is_bounded() {
        let data: Vec<f32> = (0..100)
            .map(|i| ((i * 37) % 23) as f32 / 7.0 - 1.5)
            .collect();
        for (dtype, bound) in [(DType::Q8, 0.02), (DType::Q4, 0.25)] {
            let q = Quantized::quantize(&data, dtype);
            let mut out = vec![0.0; data.len()];
            q.dequantize_into(0, &mut out);
            for (i, (&x, &y)) in data.iter().zip(&out).enumerate() {
                assert!((x - y).abs() <= bound, "{dtype}: {x} vs {y}");
                assert_eq!(q.value(i), y);
            }
            // Partial ranges agree with the whole.
            let mut part = vec![0.0; 40];
            q.dequantize_into(17, &mut part);
            assert_eq!(part, out[17..57]);
        }
    }

    #[test]
    fn quantized_sizes() {
        let data = vec![1.0f32; 1024];
        assert_eq!(Storage::from_f32(data.clone(), DType::F32).nbytes(), 4096);
        assert_eq!(Storage::from_f32(data.clone(), DType::F16).nbytes(), 2048);
        // 8.5 and 4.5 bits per value.
        assert_eq!(
            Storage::from_f32(data.clone(), DType::Q8).nbytes(),
            1024 + 64
        );
        assert_eq!(Storage::from_f32(data, DType::Q4).nbytes(), 512 + 64);
    }

    #[test]
    fn zeros_quantize_cleanly() {
        let q = Quantized::quantize(&[0.0; 40], DType::Q4);
        assert!((0..40).all(|i| q.value(i) == 0.0));
    }
}
