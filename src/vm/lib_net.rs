//! `use vyrn.net`: talking to other programs over the network, for
//! multiplayer games, chat and tools.
//!
//! ```vyrn
//! use vyrn.net
//!
//! main:
//!     let server = net.udp(4000)
//!     let client = net.udp()
//!     client.send("127.0.0.1:4000", {"x": 3, "y": 4})
//!     # ... next frame:
//!     each msg in server.receive_all():
//!         print(msg["from"], msg["data"])
//!     end
//! end
//! ```
//!
//! Two kinds of connection:
//!
//! - **UDP** (`net.udp`): fast, no connection, messages may be lost or
//!   arrive out of order. What games use for positions sent every frame.
//! - **TCP** (`net.listen`, `net.connect`): a reliable, ordered stream.
//!   What games use for chat, lobbies and anything that must arrive.
//!
//! Messages are any Vyrn data (numbers, text, lists, maps, structs), sent
//! as JSON. Nothing ever waits: `receive` gives `none` when there's
//! nothing yet, so it fits in a game loop.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::rc::Rc;

use super::foreign::Foreign;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::value::{Key, Map, Value};

fn net_error(what: &str, e: std::io::Error) -> Value {
    err("NetworkError", format!("{what}: {e}"))
}

fn wrap(f: impl Foreign) -> Value {
    Value::Foreign(Rc::new(Box::new(f)))
}

fn port_arg(args: &[Value], i: usize) -> VmResult<u16> {
    match args.get(i) {
        None | Some(Value::Missing | Value::None) => Ok(0),
        Some(Value::Int(p)) => u16::try_from(*p)
            .map_err(|_| err("ValueError", format!("{p} is not a port (0 to 65535)"))),
        Some(other) => Err(type_error(format!(
            "a port is a number, not {}",
            other.type_name()
        ))),
    }
}

/// `"host:port"`, or a list `["host", port]`.
fn address(v: Option<&Value>) -> VmResult<String> {
    match v {
        Some(Value::Str(s)) => Ok(s.to_string()),
        Some(v @ (Value::List(_) | Value::Tuple(_))) => {
            let parts = ops::collect(v)?;
            match parts.as_slice() {
                [Value::Str(h), Value::Int(p)] => Ok(format!("{h}:{p}")),
                _ => Err(type_error("an address is \"host:port\" or [host, port]")),
            }
        }
        _ => Err(type_error(
            "an address is \"host:port\", like \"127.0.0.1:4000\"",
        )),
    }
}

fn decode(text: &str) -> Value {
    match super::json::parse(text) {
        Ok(j) => ops::from_json(&j),
        Err(_) => Value::str(text),
    }
}

fn message(from: String, data: Value) -> Value {
    let mut m = Map::default();
    for (k, v) in [("from", Value::str(from)), ("data", data)] {
        let key = Value::str(k);
        if let Some(h) = Key::from_value(&key) {
            m.insert(h, key, v);
        }
    }
    Value::Map(Rc::new(RefCell::new(m)))
}

// ==== UDP ===================================================================

pub struct Udp {
    socket: UdpSocket,
}

/// `net.udp(port)`: a UDP socket; without a port, any free one (for
/// clients).
pub fn udp(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let port = port_arg(args, 0)?;
    let socket = UdpSocket::bind(("0.0.0.0", port))
        .map_err(|e| net_error(&format!("cannot use port {port}"), e))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| net_error("cannot set up the socket", e))?;
    Ok(wrap(Udp { socket }))
}

impl Udp {
    fn receive(&self) -> VmResult<Option<Value>> {
        let mut buf = vec![0u8; 65536];
        match self.socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                let text = String::from_utf8_lossy(&buf[..n]);
                Ok(Some(message(from.to_string(), decode(&text))))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            // A message to a closed port elsewhere reports here on some
            // systems; it isn't this socket's failure.
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => Ok(None),
            Err(e) => Err(net_error("cannot receive", e)),
        }
    }
}

impl Foreign for Udp {
    fn type_name(&self) -> &'static str {
        "UdpSocket"
    }

    fn display(&self) -> String {
        format!("UdpSocket(port {})", self.port())
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "port" => Ok(Value::Int(i64::from(self.port()))),
            _ => Err(err(
                "FieldError",
                format!("a UdpSocket has no field `{name}`"),
            )),
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
            // `sock.send("host:port", data)`
            "send" => {
                let to = address(args.first())?;
                let addr = to
                    .to_socket_addrs()
                    .map_err(|e| net_error(&format!("cannot find {to}"), e))?
                    .next()
                    .ok_or_else(|| err("NetworkError", format!("cannot find {to}")))?;
                let text = ops::to_json(&ctx.program, args.get(1).unwrap_or(&Value::None));
                if text.len() > 65000 {
                    return Err(err(
                        "ValueError",
                        "a UDP message must be under 65,000 bytes; use TCP for big data",
                    ));
                }
                self.socket
                    .send_to(text.as_bytes(), addr)
                    .map_err(|e| net_error(&format!("cannot send to {to}"), e))?;
                Ok(Value::None)
            }
            // The next message, `{"from": address, "data": value}`, or none.
            "receive" => Ok(self.receive()?.unwrap_or(Value::None)),
            // Every message waiting, oldest first.
            "receive_all" => {
                let mut all = Vec::new();
                while let Some(m) = self.receive()? {
                    all.push(m);
                }
                Ok(Value::list(all))
            }
            "close" => Ok(Value::None),
            _ => Err(err(
                "FieldError",
                format!("a UdpSocket has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Udp {
    fn port(&self) -> u16 {
        self.socket.local_addr().map(|a| a.port()).unwrap_or(0)
    }
}

// ==== TCP ===================================================================

pub struct Listener {
    listener: TcpListener,
}

/// `net.listen(port)`: waits for TCP connections; `server.accept()` gives
/// each new one (or none).
pub fn listen(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let port = port_arg(args, 0)?;
    let listener = TcpListener::bind(("0.0.0.0", port))
        .map_err(|e| net_error(&format!("cannot listen on port {port}"), e))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| net_error("cannot set up the listener", e))?;
    Ok(wrap(Listener { listener }))
}

impl Foreign for Listener {
    fn type_name(&self) -> &'static str {
        "Listener"
    }

    fn display(&self) -> String {
        format!("Listener(port {})", self.port())
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "port" => Ok(Value::Int(i64::from(self.port()))),
            _ => Err(err(
                "FieldError",
                format!("a Listener has no field `{name}`"),
            )),
        }
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        _: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            "accept" => match self.listener.accept() {
                Ok((stream, from)) => Ok(wrap(Connection::new(stream, from.to_string())?)),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(Value::None),
                Err(e) => Err(net_error("cannot accept a connection", e)),
            },
            "close" => Ok(Value::None),
            _ => Err(err(
                "FieldError",
                format!("a Listener has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Listener {
    fn port(&self) -> u16 {
        self.listener.local_addr().map(|a| a.port()).unwrap_or(0)
    }
}

/// One TCP connection. Each message is one line of JSON.
pub struct Connection {
    stream: RefCell<TcpStream>,
    peer: String,
    pending: RefCell<Vec<u8>>,
    closed: std::cell::Cell<bool>,
}

impl Connection {
    fn new(stream: TcpStream, peer: String) -> VmResult<Connection> {
        stream
            .set_nonblocking(true)
            .map_err(|e| net_error("cannot set up the connection", e))?;
        let _ = stream.set_nodelay(true);
        Ok(Connection {
            stream: RefCell::new(stream),
            peer,
            pending: RefCell::new(Vec::new()),
            closed: std::cell::Cell::new(false),
        })
    }

    /// Reads whatever has arrived, without waiting.
    fn fill(&self) -> VmResult<()> {
        let mut buf = [0u8; 8192];
        loop {
            match self.stream.borrow_mut().read(&mut buf) {
                Ok(0) => {
                    self.closed.set(true);
                    return Ok(());
                }
                Ok(n) => self.pending.borrow_mut().extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => {
                    self.closed.set(true);
                    return Err(net_error(
                        &format!("lost the connection to {}", self.peer),
                        e,
                    ));
                }
            }
        }
    }

    fn next_message(&self) -> VmResult<Option<Value>> {
        self.fill()?;
        let mut pending = self.pending.borrow_mut();
        let Some(end) = pending.iter().position(|&b| b == b'\n') else {
            return Ok(None);
        };
        let line: Vec<u8> = pending.drain(..=end).collect();
        let text = String::from_utf8_lossy(&line[..line.len() - 1]);
        Ok(Some(decode(&text)))
    }
}

/// `net.connect("host:port")`: a TCP connection to a server.
pub fn connect(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let to = address(args.first())?;
    let stream =
        TcpStream::connect(&to).map_err(|e| net_error(&format!("cannot connect to {to}"), e))?;
    Ok(wrap(Connection::new(stream, to)?))
}

impl Foreign for Connection {
    fn type_name(&self) -> &'static str {
        "Connection"
    }

    fn display(&self) -> String {
        format!("Connection({})", self.peer)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "peer" => Ok(Value::str(self.peer.clone())),
            "open" => Ok(Value::Bool(!self.closed.get())),
            _ => Err(err(
                "FieldError",
                format!("a Connection has no field `{name}`"),
            )),
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
            "send" => {
                let mut text = ops::to_json(&ctx.program, args.first().unwrap_or(&Value::None));
                text.push('\n');
                let mut stream = self.stream.borrow_mut();
                // The stream doesn't wait, so write until it has all gone.
                let mut sent = 0;
                let bytes = text.as_bytes();
                while sent < bytes.len() {
                    match stream.write(&bytes[sent..]) {
                        Ok(n) => sent += n,
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(1));
                        }
                        Err(e) => {
                            self.closed.set(true);
                            return Err(net_error(&format!("cannot send to {}", self.peer), e));
                        }
                    }
                }
                Ok(Value::None)
            }
            // The next message, or none if none has arrived yet.
            "receive" => Ok(self.next_message()?.unwrap_or(Value::None)),
            "receive_all" => {
                let mut all = Vec::new();
                while let Some(m) = self.next_message()? {
                    all.push(m);
                }
                Ok(Value::list(all))
            }
            "close" => {
                let _ = self.stream.borrow().shutdown(std::net::Shutdown::Both);
                self.closed.set(true);
                Ok(Value::None)
            }
            _ => Err(err(
                "FieldError",
                format!("a Connection has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
