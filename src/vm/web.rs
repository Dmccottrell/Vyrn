//! `vyrn.web`: a small HTTP/1.1 server that runs a program's `route`s.
//!
//! The server is part of the task scheduler. `web.serve` registers a
//! listener and suspends the calling task; while every task waits, the
//! scheduler accepts connections, and each request runs its route handler
//! as a task of its own. A handler that awaits (a timer, a channel, another
//! task) lets other requests proceed, as in an async Python or JavaScript
//! server. Everything runs on one thread.
//!
//! What a handler returns becomes the response: a string is sent as text,
//! `(status, body)` sets the status, `none` is `204 No Content`, and any
//! other value is sent as JSON. A failure becomes a JSON error with a
//! status chosen from its kind (`NotFound` is 404, `BadRequest` 400, ...).

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use std::time::Duration;

use super::task::TaskHandle;

/// A listening server started by `web.serve`.
pub struct Server {
    pub listener: TcpListener,
    /// Finishes when the server stops, waking the task that called `serve`.
    pub handle: Rc<TaskHandle>,
    /// Requests still to accept, when `serve` was given `requests = n`.
    pub remaining: Option<u64>,
    /// Requests whose handler is still running.
    pub pending: Vec<(Rc<TaskHandle>, TcpStream)>,
}

impl Server {
    /// Whether the server has accepted every request it was asked to and
    /// answered all of them.
    pub fn finished(&self) -> bool {
        self.remaining == Some(0) && self.pending.is_empty()
    }

    pub fn accepting(&self) -> bool {
        self.remaining != Some(0)
    }
}

/// A parsed request.
#[derive(Debug, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

/// Largest request accepted, headers and body together.
const MAX_REQUEST: usize = 16 << 20;

/// Reads one request from `stream`. Gives up on a client that sends
/// nothing for five seconds.
pub fn read_request(stream: &mut TcpStream) -> Result<Request, String> {
    stream
        .set_nonblocking(false)
        .and_then(|_| stream.set_read_timeout(Some(Duration::from_secs(5))))
        .map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    let mut chunk = [0u8; 8192];
    let header_end = loop {
        if let Some(i) = find(&data, b"\r\n\r\n") {
            break i;
        }
        if data.len() > MAX_REQUEST {
            return Err("request headers are too large".into());
        }
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the connection closed before the request was complete".into());
        }
        data.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&data[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let start = lines.next().unwrap_or_default();
    let mut parts = start.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Err(format!("malformed request line {start:?}"));
    };
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let length: usize = match headers.get("content-length") {
        Some(v) => v.parse().map_err(|_| format!("bad Content-Length {v:?}"))?,
        None => 0,
    };
    if length > MAX_REQUEST {
        return Err("request body is too large".into());
    }
    let mut body = data[header_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("the connection closed before the body was complete".into());
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, parse_query(q)),
        None => (target, HashMap::new()),
    };
    Ok(Request {
        method: method.to_ascii_uppercase(),
        path: percent_decode(path),
        query,
        headers,
        body,
    })
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (
                percent_decode(&k.replace('+', " ")),
                percent_decode(&v.replace('+', " ")),
            )
        })
        .collect()
}

pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// A response ready to send.
#[derive(Debug, Clone, PartialEq)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl Response {
    pub fn json(status: u16, body: String) -> Response {
        Response {
            status,
            content_type: "application/json",
            body,
        }
    }

    /// Text, sent as a web page when it is one (it starts with `<`, as a
    /// rendered template does), as Flask sends strings.
    pub fn text(status: u16, body: String) -> Response {
        let page = body.trim_start().starts_with('<');
        Response {
            status,
            content_type: if page {
                "text/html; charset=utf-8"
            } else {
                "text/plain; charset=utf-8"
            },
            body,
        }
    }

    /// A JSON error body: `{"error": kind, "message": message}`.
    pub fn error(status: u16, kind: &str, message: &str) -> Response {
        Response::json(
            status,
            format!(
                "{{\"error\": {}, \"message\": {}}}",
                json_string(kind),
                json_string(message)
            ),
        )
    }

    pub fn write_to(&self, stream: &mut TcpStream) {
        let head = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.status,
            reason(self.status),
            self.content_type,
            self.body.len()
        );
        // The client may already have gone; there is no one to tell.
        let _ = stream
            .write_all(head.as_bytes())
            .and_then(|_| stream.write_all(self.body.as_bytes()))
            .and_then(|_| stream.flush());
    }
}

/// The status for a failure of the given kind.
pub fn status_for(kind: &str) -> u16 {
    // `app.storage.NotFound` is a `NotFound` declared in a module.
    match short_kind(kind) {
        "BadRequest" | "ValueError" | "ParseError" => 400,
        "Unauthorized" => 401,
        "Forbidden" | "PermissionDenied" => 403,
        "NotFound" => 404,
        "Conflict" => 409,
        "TooLarge" => 413,
        "Unavailable" => 503,
        _ => 500,
    }
}

/// A failure's type without its module: `app.storage.NotFound` is
/// `NotFound`.
pub fn short_kind(kind: &str) -> &str {
    kind.rsplit('.').next().unwrap_or(kind)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        301 => "Moved Permanently",
        302 => "Found",
        304 => "Not Modified",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "",
    }
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Accepts the connections waiting on `listener`, without blocking.
pub fn accept_ready(listener: &TcpListener, limit: Option<u64>) -> Vec<TcpStream> {
    let mut streams = Vec::new();
    while limit.is_none_or(|n| (streams.len() as u64) < n) {
        match listener.accept() {
            Ok((stream, _)) => streams.push(stream),
            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            // A connection that failed while being accepted affects only
            // that client.
            Err(_) => break,
        }
    }
    streams
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_strings_are_decoded() {
        let q = parse_query("name=Ada+Lovelace&x=%2F1&flag");
        assert_eq!(q["name"], "Ada Lovelace");
        assert_eq!(q["x"], "/1");
        assert_eq!(q["flag"], "");
        assert_eq!(percent_decode("%zz%41"), "%zzA");
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn failures_map_to_statuses() {
        assert_eq!(status_for("NotFound"), 404);
        assert_eq!(status_for("TypeError"), 500);
        assert_eq!(status_for("app.storage.NotFound"), 404);
        let r = Response::error(404, "NotFound", "no user \"7\"");
        assert_eq!(
            r.body,
            r#"{"error": "NotFound", "message": "no user \"7\""}"#
        );
    }
}
