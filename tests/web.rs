//! `std.web`: a program serves real HTTP requests over TCP, and `Clock`
//! paces a game loop.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::{Duration, Instant};

use vyrn::vm::{compile_source, run_source, Captured, RunError, Vm};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Sends a request, retrying until the server is up, and returns the
/// status and body.
fn request(port: u16, method: &str, path: &str, body: &str) -> (u16, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut stream = loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("server never came up: {e}"),
        }
    };
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    (status, body)
}

const SERVER: &str = r#"use std.web

struct NotFound:
    id: int
end

route GET "/hello/{name}" (name: str):
    return "hello {name}"
end

route GET "/users/{id}" (id: int, db):
    if id not in db: fail NotFound(id)
    return db[id]
end

route GET "/slow" (ms: int):
    await sleep(ms / 1000)
    return {"slept": ms}
end

route POST "/echo" (body):
    return 201, body
end

main:
    web.provide("db", {1: {"name": "Ada"}})
    web.serve(port = PORT, requests = COUNT)
    print("stopped")
end
"#;

fn serve(count: usize) -> (u16, Vm, Captured) {
    let port = free_port();
    let src = SERVER
        .replace("PORT", &port.to_string())
        .replace("COUNT", &count.to_string());
    let program = compile_source(&src).unwrap_or_else(|e| panic!("{e:?}"));
    let out = Captured::default();
    let machine = Vm::with_output(program, Box::new(out.clone()));
    (port, machine, out)
}

#[test]
fn routes_answer_http_requests() {
    let (port, mut machine, out) = serve(6);
    let client = thread::spawn(move || {
        vec![
            request(port, "GET", "/hello/Ada%20L", ""),
            request(port, "GET", "/users/1", ""),
            request(port, "GET", "/users/7", ""),
            request(port, "POST", "/echo", r#"{"x": [1, 2]}"#),
            request(port, "GET", "/nowhere", ""),
            request(port, "GET", "/users/abc", ""),
        ]
    });
    machine.run_main(&[]).unwrap();
    let responses = client.join().unwrap();
    assert_eq!(responses[0], (200, "hello Ada L".to_string()));
    assert_eq!(responses[1], (200, r#"{"name":"Ada"}"#.to_string()));
    assert_eq!(responses[2].0, 404);
    assert!(
        responses[2].1.contains(r#""error": "NotFound""#),
        "{:?}",
        responses[2]
    );
    assert_eq!(responses[3], (201, r#"{"x":[1,2]}"#.to_string()));
    assert_eq!(responses[4].0, 404);
    assert_eq!(responses[5].0, 400);
    assert_eq!(
        out.text(),
        format!("serving on http://127.0.0.1:{port}\nstopped\n")
    );
}

#[test]
fn waiting_handlers_do_not_block_other_requests() {
    let (port, mut machine, _) = serve(3);
    let clients: Vec<_> = (0..3)
        .map(|_| thread::spawn(move || request(port, "GET", "/slow?ms=150", "")))
        .collect();
    let start = Instant::now();
    machine.run_main(&[]).unwrap();
    for client in clients {
        assert_eq!(
            client.join().unwrap(),
            (200, r#"{"slept":150}"#.to_string())
        );
    }
    // Three 150 ms handlers overlapped instead of taking 450 ms.
    let took = start.elapsed();
    assert!(took < Duration::from_millis(400), "took {took:?}");
}

#[test]
fn unknown_modules_and_members_are_compile_errors() {
    let errors = compile_source("use std.nope\nmain: print(1)\n").unwrap_err();
    assert_eq!(
        errors[0].message,
        "the standard library has no module `nope`"
    );
    let errors = compile_source("use std.web\nmain: web.launch()\n").unwrap_err();
    assert_eq!(errors[0].message, "`std.web` has no `launch`");
    // Without the `use`, `web` is just an unknown name.
    assert!(compile_source("main: web.serve()\n").is_err());
}

#[test]
fn names_can_be_imported_from_a_module() {
    let errors = compile_source(
        "use std.web (serve, provide as give)\nmain:\n    give(\"x\", 1)\n    serve\nend\n",
    );
    assert!(errors.is_ok(), "{errors:?}");
}

#[test]
fn clocks_pace_game_loops() {
    let start = Instant::now();
    let out = match run_source(
        "main:\n\
         \x20   var clock = Clock(hz = 100, frames = 5)\n\
         \x20   var t = 0.0\n\
         \x20   while clock.running:\n\
         \x20       t += clock.tick()\n\
         \x20   end\n\
         \x20   print(clock.frame, round(t, 3), clock.dt)\n\
         end\n",
    ) {
        Ok(out) => out,
        Err(RunError::Compile(e)) => panic!("{e:?}"),
        Err(RunError::Runtime(e)) => panic!("{e}"),
    };
    assert_eq!(out, "5 0.05 0.01\n");
    assert!(start.elapsed() >= Duration::from_millis(39));
}

#[test]
fn clocks_can_be_stopped() {
    let out = run_source(
        "main:\n\
         \x20   clock = Clock(hz = 1000)\n\
         \x20   while clock.running:\n\
         \x20       clock.tick()\n\
         \x20       if clock.frame == 3: clock.stop()\n\
         \x20   end\n\
         \x20   print(clock.frame, clock.running)\n\
         end\n",
    );
    assert!(matches!(out.as_deref(), Ok("3 false\n")), "{out:?}");
}
