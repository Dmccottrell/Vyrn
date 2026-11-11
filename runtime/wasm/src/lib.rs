//! Vyrn as a WebAssembly module.
//!
//! Built for `wasm32-unknown-unknown` with no imports, so any WebAssembly
//! host can load it: browsers (the playground), and edge platforms such as
//! Vercel Edge Functions, Cloudflare Workers and Deno Deploy (`vyrn build
//! --target=wasm32-edge`). `runtime/wasm/vyrn-edge.mjs` is the JavaScript
//! side.
//!
//! The interface is plain memory and integers:
//!
//! | Export | Does |
//! |---|---|
//! | `vyrn_alloc(len) -> ptr` | memory for the host to write input into |
//! | `vyrn_free(ptr, len)` | releases memory from `vyrn_alloc` or a reply |
//! | `vyrn_run(ptr, len) -> reply` | compiles and runs a program's `main:` (the playground) |
//! | `vyrn_load(ptr, len) -> reply` | loads a program bundle, runs `main:` hosted, keeps it for requests |
//! | `vyrn_handle(ptr, len) -> reply` | answers one HTTP request with the loaded program's routes |
//!
//! A reply is a pointer to `[u32 length][bytes]`; the host reads it and
//! frees `length + 4` bytes. `vyrn_run` and `vyrn_load` reply with one
//! status byte (0 ok, 1 error) and UTF-8 text. A request is
//! `method \0 path \0 query \0 content-type \0 body`; its reply is
//! `[u16 status][content-type \0][body]`.

use std::cell::RefCell;
use std::path::PathBuf;

use vyrn::project::bundle::Bundle;
use vyrn::project::loader;
use vyrn::vm::{compile_checked, compile_source, Captured, Vm};

thread_local! {
    /// The program loaded by `vyrn_load`, and where it prints.
    static PROGRAM: RefCell<Option<(Vm, Captured)>> = const { RefCell::new(None) };
}

#[no_mangle]
pub extern "C" fn vyrn_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

/// # Safety
///
/// `ptr` and `len` must come from `vyrn_alloc` (the length asked for) or
/// from a reply (its length plus 4), and be freed once.
#[no_mangle]
pub unsafe extern "C" fn vyrn_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

/// The host's input. SAFETY (for callers): `ptr..ptr+len` was written by
/// the host into memory from `vyrn_alloc`.
unsafe fn input<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

/// Hands bytes to the host as `[u32 length][bytes]`.
fn reply(bytes: Vec<u8>) -> *mut u8 {
    let mut out = Vec::with_capacity(bytes.len() + 4);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&bytes);
    let len = out.len();
    let ptr = vyrn_alloc(len);
    // SAFETY: `ptr` has room for `len` bytes, freshly allocated above.
    unsafe { std::ptr::copy_nonoverlapping(out.as_ptr(), ptr, len) };
    ptr
}

fn status_reply(ok: bool, text: String) -> *mut u8 {
    let mut bytes = vec![u8::from(!ok)];
    bytes.extend_from_slice(text.as_bytes());
    reply(bytes)
}

/// Compiles and runs a program; replies with what it printed, or its
/// errors rendered against the source.
///
/// # Safety
///
/// `ptr..ptr+len` must be UTF-8 source written by the host.
#[no_mangle]
pub unsafe extern "C" fn vyrn_run(ptr: *const u8, len: usize) -> *mut u8 {
    let src = String::from_utf8_lossy(input(ptr, len)).into_owned();
    let program = match compile_source(&src) {
        Ok(p) => p,
        Err(errors) => {
            let text: String = errors
                .iter()
                .map(|e| e.render("main.vyrn", &src) + "\n")
                .collect();
            return status_reply(false, text);
        }
    };
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    let result = vm.run_main(&[]);
    let mut text = out.text();
    match result {
        Ok(_) => status_reply(true, text),
        Err(e) => {
            text.push_str(&format!("error: {e}\n"));
            status_reply(false, text)
        }
    }
}

/// Loads a program bundle (the format `vyrn build` writes), runs its
/// `main:` with `web.serve` handing requests to the host, and keeps it.
///
/// # Safety
///
/// `ptr..ptr+len` must be bundle bytes written by the host.
#[no_mangle]
pub unsafe extern "C" fn vyrn_load(ptr: *const u8, len: usize) -> *mut u8 {
    let Some(mut bundle) = Bundle::decode(input(ptr, len)) else {
        return status_reply(false, "this is not a Vyrn program bundle".into());
    };
    let entry = PathBuf::from(&bundle.entry);
    let root = PathBuf::from(&bundle.root);
    let loaded = match loader::load(&entry, &root, &mut bundle) {
        Ok(l) => l,
        Err((errors, _)) => {
            let text: String = errors
                .iter()
                .map(|e| format!("error: {}\n", e.message))
                .collect();
            return status_reply(false, text);
        }
    };
    let program = match compile_checked(&loaded.module) {
        Ok(p) => p,
        Err(errors) => {
            let text: String = errors
                .iter()
                .map(|e| format!("error: {}\n", e.message))
                .collect();
            return status_reply(false, text);
        }
    };
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.set_hosted(true);
    if let Err(e) = vm.run_main(&[]) {
        return status_reply(false, format!("{}error: {e}\n", out.text()));
    }
    let printed = out.text();
    PROGRAM.with(|p| *p.borrow_mut() = Some((vm, out)));
    status_reply(true, printed)
}

/// Answers one HTTP request with the loaded program's routes.
///
/// # Safety
///
/// `ptr..ptr+len` must be a request in the format above, written by the
/// host.
#[no_mangle]
pub unsafe extern "C" fn vyrn_handle(ptr: *const u8, len: usize) -> *mut u8 {
    let data = input(ptr, len);
    let mut parts = data.splitn(5, |b| *b == 0);
    let mut text = || String::from_utf8_lossy(parts.next().unwrap_or_default()).into_owned();
    let (method, path, query, content_type) = (text(), text(), text(), text());
    let body: Vec<u8> = data
        .splitn(5, |b| *b == 0)
        .nth(4)
        .unwrap_or_default()
        .to_vec();
    let response = PROGRAM.with(|p| match p.borrow_mut().as_mut() {
        Some((vm, _)) => vm.handle_request(
            &method,
            &path,
            &query,
            &body,
            (!content_type.is_empty()).then_some(content_type.as_str()),
        ),
        None => vyrn::vm::web::Response::error(
            503,
            "Unavailable",
            "no program is loaded; call vyrn_load first",
        ),
    });
    let mut bytes = response.status.to_le_bytes().to_vec();
    bytes.extend_from_slice(response.content_type.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(response.body.as_bytes());
    reply(bytes)
}
