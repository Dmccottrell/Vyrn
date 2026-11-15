//! `use vyrn.crypto`: hashing, password storage and secure random tokens.
//!
//! ```vyrn
//! use vyrn.crypto
//!
//! main:
//!     let stored = crypto.hash_password("correct horse")
//!     print(crypto.check_password("correct horse", stored))
//!     print(crypto.token())
//! end
//! ```
//!
//! Passwords are stored as PBKDF2-HMAC-SHA256 with a random salt and many
//! rounds, so a stolen table of them is slow to attack and two equal
//! passwords look different.

use sha2::{Digest, Sha256};

use super::ops::{err, type_error, Ctx, VmResult};
use super::value::Value;

const ROUNDS: u32 = 100_000;

fn text<'a>(args: &'a [Value], i: usize, what: &str) -> VmResult<&'a str> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.as_str()),
        _ => Err(type_error(format!("{what} must be text"))),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).ok())
        .collect()
}

pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

/// PBKDF2-HMAC-SHA256 with one 32-byte block, as RFC 8018 defines it.
pub fn pbkdf2(password: &[u8], salt: &[u8], rounds: u32) -> [u8; 32] {
    let mut first = salt.to_vec();
    first.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac_sha256(password, &first);
    let mut out = u;
    for _ in 1..rounds {
        u = hmac_sha256(password, &u);
        for (o, x) in out.iter_mut().zip(u) {
            *o ^= x;
        }
    }
    out
}

/// Bytes from the operating system's secure random source.
fn random_bytes(n: usize) -> VmResult<Vec<u8>> {
    let mut buf = vec![0u8; n];
    #[cfg(not(target_arch = "wasm32"))]
    {
        getrandom::getrandom(&mut buf)
            .map_err(|e| err("CryptoError", format!("no secure random numbers: {e}")))?;
        Ok(buf)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = &mut buf;
        Err(err(
            "Unavailable",
            "secure random numbers are not available here",
        ))
    }
}

/// Compares without stopping at the first difference, so timing tells an
/// attacker nothing.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// `crypto.sha256(text)`: the SHA-256 hash, as 64 hex digits.
pub fn sha256(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::str(hex(&Sha256::digest(
        text(args, 0, "the text")?.as_bytes(),
    ))))
}

/// `crypto.hmac(key, text)`: a signature of `text` that only someone with
/// `key` can make, as 64 hex digits.
pub fn hmac(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let key = text(args, 0, "the key")?;
    let message = text(args, 1, "the text")?;
    Ok(Value::str(hex(&hmac_sha256(
        key.as_bytes(),
        message.as_bytes(),
    ))))
}

/// `crypto.hash_password(password)`: text safe to store in place of the
/// password: `pbkdf2-sha256$rounds$salt$hash`.
pub fn hash_password(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let password = text(args, 0, "the password")?;
    let salt = random_bytes(16)?;
    let hash = pbkdf2(password.as_bytes(), &salt, ROUNDS);
    Ok(Value::str(format!(
        "pbkdf2-sha256${ROUNDS}${}${}",
        hex(&salt),
        hex(&hash)
    )))
}

/// `crypto.check_password(password, stored)`: whether the password is the
/// one `hash_password` stored.
pub fn check_password(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let password = text(args, 0, "the password")?;
    let stored = text(args, 1, "the stored hash")?;
    let parts: Vec<&str> = stored.split('$').collect();
    let [kind, rounds, salt, hash] = parts.as_slice() else {
        return Err(err("ValueError", "that is not a stored password hash"));
    };
    let (Some(rounds), Some(salt), Some(hash)) =
        (rounds.parse::<u32>().ok(), unhex(salt), unhex(hash))
    else {
        return Err(err("ValueError", "that is not a stored password hash"));
    };
    if *kind != "pbkdf2-sha256" || rounds == 0 {
        return Err(err("ValueError", "that is not a stored password hash"));
    }
    let again = pbkdf2(password.as_bytes(), &salt, rounds);
    Ok(Value::Bool(same(&again, &hash)))
}

/// `crypto.token(bytes = 32)`: a random token for session ids, reset links
/// and API keys, as hex digits (two per byte).
pub fn token(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let n = match args.first() {
        None | Some(Value::None) | Some(Value::Missing) => 32,
        Some(Value::Int(n)) if (1..=1024).contains(n) => *n as usize,
        Some(_) => {
            return Err(type_error(
                "the token length is a number of bytes from 1 to 1024",
            ))
        }
    };
    Ok(Value::str(hex(&random_bytes(n)?)))
}

/// `crypto.same(a, b)`: whether two texts are equal, compared in constant
/// time (for checking tokens and signatures).
pub fn same_text(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let a = text(args, 0, "the first text")?;
    let b = text(args, 1, "the second text")?;
    Ok(Value::Bool(same(a.as_bytes(), b.as_bytes())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_answers() {
        // RFC 4231 test case 2.
        assert_eq!(
            hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // PBKDF2-HMAC-SHA256, password "password", salt "salt", 1 and 4096 rounds.
        assert_eq!(
            hex(&pbkdf2(b"password", b"salt", 1)),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
        assert_eq!(
            hex(&pbkdf2(b"password", b"salt", 4096)),
            "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
        );
    }
}
