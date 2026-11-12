//! The parts of `vyrn.http` beyond "a route returns a value":
//!
//! - `request`: a route parameter with that name receives the whole
//!   request (method, path, headers, query, body, json, client).
//! - `http.Response(status, body, headers)`: full control of the answer.
//! - `stream`: a route parameter with that name turns the route into
//!   server-sent events; `stream.send(data, event)` pushes one to the page.
//! - `socket`: a route parameter with that name accepts a WebSocket;
//!   `socket.send(x)` and `await socket.receive()` talk both ways.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpStream;
use std::rc::Rc;

use super::foreign::Foreign;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::program::Program;
use super::task::{Pending, TaskHandle};
use super::value::{Key, Map, Value};
use super::web;

pub(super) fn text_map(entries: &HashMap<String, String>) -> Value {
    let mut keys: Vec<&String> = entries.keys().collect();
    keys.sort();
    let mut m = Map::default();
    for k in keys {
        let key = Value::str(k.clone());
        if let Some(h) = Key::from_value(&key) {
            m.insert(h, key, Value::str(entries[k].clone()));
        }
    }
    Value::Map(Rc::new(RefCell::new(m)))
}

// ==== the request ============================================================

/// What a route sees of a request when it takes a `request` parameter.
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub query: HashMap<String, String>,
    pub body: Vec<u8>,
    pub client: String,
}

impl HttpRequest {
    pub fn value(self) -> Value {
        Value::Foreign(Rc::new(Box::new(self)))
    }
}

impl Foreign for HttpRequest {
    fn type_name(&self) -> &'static str {
        "Request"
    }

    fn display(&self) -> String {
        format!("<request {} {}>", self.method, self.path)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "method" => Ok(Value::str(self.method.clone())),
            "path" => Ok(Value::str(self.path.clone())),
            // Header names in lower case: `request.headers["user-agent"]`.
            "headers" => Ok(text_map(&self.headers)),
            "query" => Ok(text_map(&self.query)),
            // The fields of a submitted HTML form (empty for other bodies).
            "form" => Ok(text_map(&match self.headers.get("content-type") {
                Some(t) if t.contains("x-www-form-urlencoded") => {
                    super::web::parse_query(&String::from_utf8_lossy(&self.body))
                }
                _ => Default::default(),
            })),
            "body" => Ok(Value::str(String::from_utf8_lossy(&self.body).into_owned())),
            // The body read as JSON, or none if it is not JSON.
            "json" => Ok(super::json::parse(&String::from_utf8_lossy(&self.body))
                .map(|j| ops::from_json(&j))
                .unwrap_or(Value::None)),
            "client" => Ok(Value::str(self.client.clone())),
            _ => Err(err("FieldError", format!("a request has no `{name}`"))),
        }
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        args: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            // `request.header("Authorization")`: any case; none if absent.
            "header" => match args.first() {
                Some(Value::Str(h)) => Ok(self
                    .headers
                    .get(&h.to_ascii_lowercase())
                    .map_or(Value::None, |v| Value::str(v.clone()))),
                _ => Err(type_error("`header` takes the header's name")),
            },
            _ => Err(err(
                "FieldError",
                format!("a request has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==== the response ===========================================================

/// `http.Response(status = 200, body = "", headers = {})`.
pub struct HttpResponse {
    pub status: u16,
    pub body: Value,
    pub headers: Vec<(String, String)>,
}

pub fn response(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let status = match args.first() {
        None | Some(Value::None) | Some(Value::Missing) => 200,
        Some(Value::Int(s)) if (100..600).contains(s) => *s as u16,
        Some(other) => {
            return Err(type_error(format!(
                "a status is a number from 100 to 599, not {}",
                ops::display(&ctx.program, other)
            )))
        }
    };
    let body = match args.get(1) {
        None | Some(Value::Missing) => Value::str(""),
        Some(v) => v.clone(),
    };
    let mut headers = Vec::new();
    if let Some(Value::Map(m)) = args.get(2) {
        for (k, v) in m.borrow().entries() {
            let (Value::Str(k), v) = (k, v) else {
                return Err(type_error("header names are text"));
            };
            let v = match v {
                Value::Str(s) => s.to_string(),
                other => ops::display(&ctx.program, other),
            };
            headers.push((k.to_string(), v));
        }
    }
    Ok(Value::Foreign(Rc::new(Box::new(HttpResponse {
        status,
        body,
        headers,
    }))))
}

impl Foreign for HttpResponse {
    fn type_name(&self) -> &'static str {
        "Response"
    }

    fn display(&self) -> String {
        format!("<response {}>", self.status)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "status" => Ok(Value::Int(i64::from(self.status))),
            "body" => Ok(self.body.clone()),
            "headers" => {
                let map: HashMap<String, String> = self.headers.iter().cloned().collect();
                Ok(text_map(&map))
            }
            _ => Err(err("FieldError", format!("a response has no `{name}`"))),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==== server-sent events ===================================================

/// Writes the headers that start an event stream.
pub fn start_events(stream: &mut TcpStream) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n",
    )?;
    stream.flush()
}

/// The `stream` of an event route: `stream.send(data, event = none)`.
pub struct EventStream {
    pub tcp: RefCell<TcpStream>,
    pub closed: Cell<bool>,
}

impl Foreign for EventStream {
    fn type_name(&self) -> &'static str {
        "EventStream"
    }

    fn display(&self) -> String {
        "<event stream>".into()
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "closed" => Ok(Value::Bool(self.closed.get())),
            _ => Err(err(
                "FieldError",
                format!("an event stream has no `{name}`"),
            )),
        }
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            "send" => {
                if self.closed.get() {
                    return Err(err("ConnectionClosed", "the page stopped listening"));
                }
                let data = match args.first() {
                    Some(Value::Str(s)) => s.to_string(),
                    Some(v) => ops::to_json(&ctx.program, v),
                    None => String::new(),
                };
                let event = named
                    .iter()
                    .find(|(k, _)| k == "event")
                    .map(|(_, v)| v)
                    .or(args.get(1));
                let mut message = String::new();
                if let Some(Value::Str(e)) = event {
                    message.push_str(&format!("event: {e}\n"));
                }
                for line in data.split('\n') {
                    message.push_str(&format!("data: {line}\n"));
                }
                message.push('\n');
                let mut tcp = self.tcp.borrow_mut();
                if tcp
                    .write_all(message.as_bytes())
                    .and_then(|_| tcp.flush())
                    .is_err()
                {
                    self.closed.set(true);
                    return Err(err("ConnectionClosed", "the page stopped listening"));
                }
                Ok(Value::None)
            }
            "close" => {
                self.closed.set(true);
                let _ = self.tcp.borrow().shutdown(std::net::Shutdown::Both);
                Ok(Value::None)
            }
            _ => Err(err(
                "FieldError",
                format!("an event stream has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==== WebSockets =============================================================

/// SHA-1, for the WebSocket handshake only (RFC 6455 requires it).
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut msg = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[4 * i],
                chunk[4 * i + 1],
                chunk[4 * i + 2],
                chunk[4 * i + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 20];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            out.push(if k <= c.len() {
                T[(n >> (18 - 6 * k) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// The `Sec-WebSocket-Accept` answer to a client's key.
pub fn accept_key(key: &str) -> String {
    base64(&sha1(
        format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", key.trim()).as_bytes(),
    ))
}

/// Answers a WebSocket handshake.
pub fn upgrade(stream: &mut TcpStream, key: &str) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    let reply = format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(key)
    );
    stream.write_all(reply.as_bytes())?;
    stream.flush()?;
    stream.set_nonblocking(true)
}

/// A frame from the server: never masked.
fn frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0x80 | opcode];
    match payload.len() {
        n if n < 126 => out.push(n as u8),
        n if n < 65536 => {
            out.push(126);
            out.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            out.push(127);
            out.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    out.extend_from_slice(payload);
    out
}

/// One frame parsed from the front of `buf`: (fin, opcode, payload, bytes used).
fn parse_frame(buf: &[u8]) -> Option<(bool, u8, Vec<u8>, usize)> {
    if buf.len() < 2 {
        return None;
    }
    let fin = buf[0] & 0x80 != 0;
    let opcode = buf[0] & 0x0f;
    let masked = buf[1] & 0x80 != 0;
    let mut len = u64::from(buf[1] & 0x7f);
    let mut at = 2;
    if len == 126 {
        len = u64::from(u16::from_be_bytes([*buf.get(2)?, *buf.get(3)?]));
        at = 4;
    } else if len == 127 {
        let b: [u8; 8] = buf.get(2..10)?.try_into().ok()?;
        len = u64::from_be_bytes(b);
        at = 10;
    }
    let mask: Option<[u8; 4]> = if masked {
        let m = buf.get(at..at + 4)?.try_into().ok()?;
        at += 4;
        Some(m)
    } else {
        None
    };
    let end = at.checked_add(usize::try_from(len).ok()?)?;
    let mut payload = buf.get(at..end)?.to_vec();
    if let Some(m) = mask {
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= m[i % 4];
        }
    }
    Some((fin, opcode, payload, end))
}

struct SocketState {
    tcp: TcpStream,
    buf: Vec<u8>,
    /// A message arriving in pieces.
    partial: Vec<u8>,
    closed: bool,
}

impl SocketState {
    /// The next whole message: Some(Some(text)), Some(None) when the
    /// connection closed, None when nothing complete has arrived yet.
    fn next_message(&mut self) -> Option<Option<String>> {
        loop {
            if let Some((fin, opcode, payload, used)) = parse_frame(&self.buf) {
                self.buf.drain(..used);
                match opcode {
                    0x8 => {
                        let _ = self.tcp.write_all(&frame(0x8, &[]));
                        self.closed = true;
                        return Some(None);
                    }
                    0x9 => {
                        let _ = self.tcp.write_all(&frame(0xA, &payload));
                        continue;
                    }
                    0xA => continue,
                    _ => {
                        self.partial.extend_from_slice(&payload);
                        if fin {
                            let text = String::from_utf8_lossy(&std::mem::take(&mut self.partial))
                                .into_owned();
                            return Some(Some(text));
                        }
                        continue;
                    }
                }
            }
            if self.closed {
                return Some(None);
            }
            let mut chunk = [0u8; 8192];
            match self.tcp.read(&mut chunk) {
                Ok(0) => {
                    self.closed = true;
                    return Some(None);
                }
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => return None,
                Err(_) => {
                    self.closed = true;
                    return Some(None);
                }
            }
        }
    }
}

/// The `socket` of a WebSocket route.
pub struct WebSocket {
    state: Rc<RefCell<SocketState>>,
}

impl WebSocket {
    pub fn new(tcp: TcpStream) -> WebSocket {
        WebSocket {
            state: Rc::new(RefCell::new(SocketState {
                tcp,
                buf: Vec::new(),
                partial: Vec::new(),
                closed: false,
            })),
        }
    }
}

/// A `receive` waiting for its message.
struct Receiving {
    handle: Rc<TaskHandle>,
    state: Rc<RefCell<SocketState>>,
}

impl Pending for Receiving {
    fn handle(&self) -> &Rc<TaskHandle> {
        &self.handle
    }

    fn poll(&mut self, _: &Program) -> Option<Result<Value, Value>> {
        let message = self.state.borrow_mut().next_message()?;
        Some(Ok(message.map_or(Value::None, |text| {
            // JSON comes back as Vyrn values; other text as text.
            match super::json::parse(&text) {
                Ok(j) if text.trim_start().starts_with(['{', '[']) => ops::from_json(&j),
                _ => Value::str(text),
            }
        })))
    }
}

impl Foreign for WebSocket {
    fn type_name(&self) -> &'static str {
        "WebSocket"
    }

    fn display(&self) -> String {
        "<websocket>".into()
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "closed" => Ok(Value::Bool(self.state.borrow().closed)),
            _ => Err(err("FieldError", format!("a WebSocket has no `{name}`"))),
        }
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            // `socket.send(x)`: text as it is, anything else as JSON.
            "send" => {
                let text = match args.first() {
                    Some(Value::Str(s)) => s.to_string(),
                    Some(v) => ops::to_json(&ctx.program, v),
                    None => String::new(),
                };
                let mut s = self.state.borrow_mut();
                if s.closed {
                    return Err(err("ConnectionClosed", "the other side has gone"));
                }
                let bytes = frame(0x1, text.as_bytes());
                // The socket is non-blocking: keep writing until it is all out.
                let mut sent = 0;
                while sent < bytes.len() {
                    match s.tcp.write(&bytes[sent..]) {
                        Ok(n) => sent += n,
                        Err(e) if e.kind() == ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(1))
                        }
                        Err(_) => {
                            s.closed = true;
                            return Err(err("ConnectionClosed", "the other side has gone"));
                        }
                    }
                }
                Ok(Value::None)
            }
            // `msg = await socket.receive()`: the next message, or none
            // once the other side has closed.
            "receive" => {
                let handle = Rc::new(TaskHandle::new(ctx.new_task_id(), "receive"));
                ctx.jobs.push(Box::new(Receiving {
                    handle: handle.clone(),
                    state: self.state.clone(),
                }));
                Ok(Value::Task(handle))
            }
            "close" => {
                let mut s = self.state.borrow_mut();
                if !s.closed {
                    let _ = s.tcp.write_all(&frame(0x8, &[]));
                    s.closed = true;
                }
                Ok(Value::None)
            }
            _ => Err(err(
                "FieldError",
                format!("a WebSocket has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The response for a route's `http.Response` value.
pub fn response_of(v: &Value, program: &Program) -> Option<web::Response> {
    let Value::Foreign(f) = v else { return None };
    let r = f.as_any().downcast_ref::<HttpResponse>()?;
    let mut out = match &r.body {
        Value::Str(s) => web::Response::text(r.status, s.to_string()),
        Value::None => web::Response::text(r.status, String::new()),
        other => web::Response::json(r.status, ops::to_json(program, other)),
    };
    for (k, v) in &r.headers {
        if k.eq_ignore_ascii_case("content-type") {
            out.content_type = v.clone();
        } else {
            out.headers.push((k.clone(), v.clone()));
        }
    }
    Some(out)
}

/// What `http.get`, `http.post`, ... and `http.test` give: `r.status`,
/// `r.ok`, `r.text`, `r.json` (the body parsed, or none) and `r.headers`
/// (names in lower case). `r["status"]` works too.
pub struct HttpReply {
    pub status: u16,
    pub text: String,
    pub json: Value,
    pub headers: Value,
}

impl HttpReply {
    pub fn value(status: u16, text: String, headers: Vec<(String, String)>) -> Value {
        let json = super::json::parse(&text)
            .ok()
            .map(|j| ops::from_json(&j))
            .unwrap_or(Value::None);
        let headers: HashMap<String, String> = headers
            .into_iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect();
        Value::Foreign(Rc::new(Box::new(HttpReply {
            status,
            text,
            json,
            headers: text_map(&headers),
        })))
    }
}

impl Foreign for HttpReply {
    fn type_name(&self) -> &'static str {
        "Response"
    }

    fn display(&self) -> String {
        format!("<response {}>", self.status)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "status" => Ok(Value::Int(i64::from(self.status))),
            "ok" => Ok(Value::Bool((200..300).contains(&self.status))),
            "text" => Ok(Value::str(self.text.clone())),
            "json" => Ok(self.json.clone()),
            "headers" => Ok(self.headers.clone()),
            _ => Err(err(
                "FieldError",
                format!(
                    "a Response has `status`, `ok`, `text`, `json` and `headers`, not `{name}`"
                ),
            )),
        }
    }

    fn index(&self, key: &Value) -> VmResult<Value> {
        match key {
            Value::Str(name) => self.get_attr(name),
            _ => Err(type_error("a Response is indexed by a field name")),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_handshake_answer_matches_the_standard() {
        // The example in RFC 6455, section 1.3.
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
        assert_eq!(base64(&sha1(b"abc")), "qZk+NkcGgWq6PiVxeFDCbJzQ2J0=");
    }

    #[test]
    fn frames_round_trip() {
        let f = frame(0x1, b"hello");
        let (fin, op, payload, used) = parse_frame(&f).unwrap();
        assert!(fin && op == 1 && payload == b"hello" && used == f.len());
        // A masked frame, as browsers send.
        let mask = [1u8, 2, 3, 4];
        let mut masked = vec![0x81, 0x80 | 3];
        masked.extend_from_slice(&mask);
        masked.extend(b"abc".iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        assert_eq!(parse_frame(&masked).unwrap().2, b"abc");
        let long = frame(0x2, &[7u8; 300]);
        assert_eq!(parse_frame(&long).unwrap().2.len(), 300);
        assert!(parse_frame(&long[..10]).is_none());
    }
}
