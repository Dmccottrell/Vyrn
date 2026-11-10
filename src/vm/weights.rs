//! Reading and writing model weights in the safetensors format.
//!
//! A safetensors file is an 8-byte little-endian header length, a JSON
//! header naming each tensor's element type, shape and byte range, and then
//! the raw values. Tensors are read one at a time straight from the file, so
//! loading needs memory for the model plus one tensor, never for the file
//! twice. A tensor can be converted while loading, for example from `f16`
//! to `q4`, so a model that is too large at full precision never has to fit
//! in memory at full precision.

use std::fs::File;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};

use super::dtype::{DType, Storage};
use super::json::{self, Json};
use super::tensor::{Device, Tensor};

/// Headers larger than this are rejected as corrupt.
const MAX_HEADER: u64 = 100 * 1024 * 1024;

/// Reads every tensor in a safetensors file, in the order the header lists
/// them. `convert` changes each tensor's element type as it is read.
pub fn load(
    path: &str,
    device: Device,
    convert: Option<DType>,
) -> Result<Vec<(String, Tensor)>, String> {
    let mut file = File::open(path).map_err(|e| format!("cannot open {path}: {e}"))?;
    let file_len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut len_bytes = [0u8; 8];
    file.read_exact(&mut len_bytes)
        .map_err(|_| format!("{path} is too short to be a safetensors file"))?;
    let header_len = u64::from_le_bytes(len_bytes);
    if header_len > MAX_HEADER || header_len + 8 > file_len {
        return Err(format!(
            "{path} is not a safetensors file (bad header length)"
        ));
    }
    let mut header = vec![0u8; header_len as usize];
    file.read_exact(&mut header)
        .map_err(|e| format!("cannot read the header of {path}: {e}"))?;
    let header =
        std::str::from_utf8(&header).map_err(|_| format!("the header of {path} is not UTF-8"))?;
    let header =
        json::parse(header).map_err(|e| format!("the header of {path} is not valid: {e}"))?;
    let Json::Object(entries) = header else {
        return Err(format!("the header of {path} is not a JSON object"));
    };
    let data_start = 8 + header_len;

    let mut tensors = Vec::with_capacity(entries.len());
    for (name, info) in &entries {
        if name == "__metadata__" {
            continue;
        }
        let bad = |what: &str| format!("tensor `{name}` in {path}: {what}");
        let dtype = info
            .get("dtype")
            .and_then(Json::as_str)
            .ok_or_else(|| bad("missing dtype"))?;
        let shape: Vec<usize> = info
            .get("shape")
            .and_then(Json::as_array)
            .ok_or_else(|| bad("missing shape"))?
            .iter()
            .map(|d| d.as_u64().map(|d| d as usize))
            .collect::<Option<_>>()
            .ok_or_else(|| bad("invalid shape"))?;
        let offsets = info
            .get("data_offsets")
            .and_then(Json::as_array)
            .and_then(|o| Some((o.first()?.as_u64()?, o.get(1)?.as_u64()?)))
            .ok_or_else(|| bad("missing data_offsets"))?;
        let numel: usize = shape.iter().product();
        let width = match dtype {
            "F64" | "I64" => 8,
            "F32" | "I32" => 4,
            "F16" | "BF16" | "I16" => 2,
            "I8" | "U8" | "BOOL" => 1,
            other => return Err(bad(&format!("element type {other} is not supported"))),
        };
        let (start, end) = offsets;
        if end < start || (end - start) as usize != numel * width || data_start + end > file_len {
            return Err(bad("its byte range does not match its shape"));
        }
        let mut bytes = vec![0u8; (end - start) as usize];
        file.seek(SeekFrom::Start(data_start + start))
            .and_then(|_| file.read_exact(&mut bytes))
            .map_err(|e| bad(&format!("read failed: {e}")))?;

        let halves = |bytes: &[u8]| -> Vec<u16> {
            bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect()
        };
        let storage = match dtype {
            "F32" => Storage::F32(
                bytes
                    .chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
            ),
            "F16" => Storage::F16(halves(&bytes)),
            "BF16" => Storage::BF16(halves(&bytes)),
            "F64" => Storage::F32(
                bytes
                    .chunks_exact(8)
                    .map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0; 8])) as f32)
                    .collect(),
            ),
            "I64" => Storage::F32(
                bytes
                    .chunks_exact(8)
                    .map(|c| i64::from_le_bytes(c.try_into().unwrap_or([0; 8])) as f32)
                    .collect(),
            ),
            "I32" => Storage::F32(
                bytes
                    .chunks_exact(4)
                    .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32)
                    .collect(),
            ),
            "I16" => Storage::F32(
                bytes
                    .chunks_exact(2)
                    .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])))
                    .collect(),
            ),
            "I8" => Storage::F32(bytes.iter().map(|&b| f32::from(b as i8)).collect()),
            _ => Storage::F32(bytes.iter().map(|&b| f32::from(b)).collect()),
        };
        drop(bytes);
        let mut tensor = Tensor {
            shape,
            storage,
            device,
        };
        if let Some(dtype) = convert {
            tensor = tensor.to_dtype(dtype);
        }
        tensors.push((name.clone(), tensor));
    }
    Ok(tensors)
}

/// Writes tensors to a safetensors file. Quantized tensors have no
/// safetensors element type and are rejected.
pub fn save(path: &str, tensors: &[(String, &Tensor)]) -> Result<(), String> {
    let mut header = String::from("{");
    let mut offset = 0usize;
    for (i, (name, tensor)) in tensors.iter().enumerate() {
        let dtype = match tensor.dtype() {
            DType::F32 => "F32",
            DType::F16 => "F16",
            DType::BF16 => "BF16",
            other => {
                return Err(format!(
                    "tensor `{name}` is {other}, which safetensors cannot store; convert it with `.to(f16)` first"
                ))
            }
        };
        if i > 0 {
            header.push(',');
        }
        json::write_string(name, &mut header);
        let shape: Vec<String> = tensor.shape.iter().map(usize::to_string).collect();
        let size = tensor.nbytes();
        header.push_str(&format!(
            ":{{\"dtype\":\"{dtype}\",\"shape\":[{}],\"data_offsets\":[{offset},{}]}}",
            shape.join(","),
            offset + size
        ));
        offset += size;
    }
    header.push('}');
    // Pad so the data starts 8-byte aligned, as the format recommends.
    while (8 + header.len()) % 8 != 0 {
        header.push(' ');
    }

    let file = File::create(path).map_err(|e| format!("cannot create {path}: {e}"))?;
    let mut out = BufWriter::new(file);
    let write = |out: &mut BufWriter<File>, bytes: &[u8]| {
        out.write_all(bytes)
            .map_err(|e| format!("cannot write {path}: {e}"))
    };
    write(&mut out, &(header.len() as u64).to_le_bytes())?;
    write(&mut out, header.as_bytes())?;
    for (_, tensor) in tensors {
        match &tensor.storage {
            Storage::F32(values) => {
                for v in values {
                    write(&mut out, &v.to_le_bytes())?;
                }
            }
            Storage::F16(values) | Storage::BF16(values) => {
                for v in values {
                    write(&mut out, &v.to_le_bytes())?;
                }
            }
            Storage::Quantized(_) => unreachable!("rejected above"),
        }
    }
    out.flush().map_err(|e| format!("cannot write {path}: {e}"))
}
