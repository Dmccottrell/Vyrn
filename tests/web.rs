//! `vyrn.web`: a program serves real HTTP requests over TCP, and `Clock`
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

const SERVER: &str = r#"use vyrn.web

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
fn json_bodies_become_the_declared_struct() {
    let port = free_port();
    let src = format!(
        "use vyrn.http\n\
         struct Item:\n    id: int\n    name: str\nend\n\
         route POST \"/items\" (body: Item) -> (int, Item):\n    return 201, body\nend\n\
         route POST \"/names\" (body: Item): return body.name\n\
         main: http.serve(port = {port}, requests = 3)\n"
    );
    let program = compile_source(&src).unwrap_or_else(|e| panic!("{e:?}"));
    let mut machine = Vm::with_output(program, Box::new(Captured::default()));
    let client = thread::spawn(move || {
        vec![
            request(port, "POST", "/items", r#"{"name": "desk", "id": 2}"#),
            request(port, "POST", "/names", r#"{"id": 3, "name": "lamp"}"#),
            request(port, "POST", "/items", r#"{"id": 3}"#),
        ]
    });
    machine.run_main(&[]).unwrap();
    let responses = client.join().unwrap();
    assert_eq!(responses[0], (201, r#"{"id":2,"name":"desk"}"#.to_string()));
    assert_eq!(responses[1], (200, "lamp".to_string()));
    assert_eq!(responses[2].0, 400);
    assert!(
        responses[2].1.contains("missing field `name`"),
        "{:?}",
        responses[2]
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
    let errors = compile_source("use vyrn.nope\nmain: print(1)\n").unwrap_err();
    assert_eq!(errors[0].message, "Vyrn's library has no module `nope`");
    // The old `std` root says what to write instead.
    let errors = compile_source("use std.math\nmain: print(1)\n").unwrap_err();
    assert_eq!(
        errors[0].message,
        "Vyrn's library is `vyrn`, not `std`: write `use vyrn.math`"
    );
    let errors = compile_source("use vyrn.web\nmain: web.launch()\n").unwrap_err();
    assert_eq!(errors[0].message, "`vyrn.web` has no `launch`");
    // Without the `use`, `web` is just an unknown name.
    assert!(compile_source("main: web.serve()\n").is_err());
}

#[test]
fn names_can_be_imported_from_a_module() {
    let errors = compile_source(
        "use vyrn.web (serve, provide as give)\nmain:\n    give(\"x\", 1)\n    serve\nend\n",
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
         \x20   loop clock.running:\n\
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
         \x20   loop clock.running:\n\
         \x20       clock.tick()\n\
         \x20       if clock.frame == 3: clock.stop()\n\
         \x20   end\n\
         \x20   print(clock.frame, clock.running)\n\
         end\n",
    );
    assert!(matches!(out.as_deref(), Ok("3 false\n")), "{out:?}");
}

#[test]
fn hosted_programs_answer_requests_without_a_socket() {
    let program = compile_source(
        "use vyrn.http\n\
         struct Item:\n    id: int\n    name: str\nend\n\
         struct NotFound:\n    id: int\nend\n\
         route GET \"/items/{id}\" (id: int, db):\n    if id not in db: fail NotFound(id)\n    return db[id]\nend\n\
         route POST \"/items\" (body: Item, db):\n    db[body.id] = body\n    return 201, body\nend\n\
         route GET \"/search\" (q: str, limit: int): return \"{q}:{limit}\"\n\
         main:\n    http.provide(\"db\", {1: Item(1, \"lamp\")})\n    http.serve(port = 1)\n    print(\"main returned\")\nend\n",
    )
    .unwrap();
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.set_hosted(true);
    vm.run_main(&[]).unwrap();
    assert_eq!(out.text(), "main returned\n");
    let r = vm.handle_request("GET", "/items/1", "", b"", None);
    assert_eq!(
        (r.status, r.body.as_str()),
        (200, r#"{"id":1,"name":"lamp"}"#)
    );
    let r = vm.handle_request(
        "POST",
        "/items",
        "",
        br#"{"id": 2, "name": "desk"}"#,
        Some("application/json"),
    );
    assert_eq!(r.status, 201);
    let r = vm.handle_request("GET", "/items/2", "", b"", None);
    assert_eq!(r.body, r#"{"id":2,"name":"desk"}"#);
    let r = vm.handle_request("GET", "/items/9", "", b"", None);
    assert_eq!(r.status, 404);
    let r = vm.handle_request("GET", "/search", "q=vyrn+lang&limit=3", b"", None);
    assert_eq!((r.status, r.body.as_str()), (200, "vyrn lang:3"));
    let r = vm.handle_request("GET", "/nowhere", "", b"", None);
    assert_eq!(r.status, 404);
}

/// Starts a program serving `count` connections, and the VM's thread.
fn serve_program(source: &str, count: usize) -> (u16, std::thread::JoinHandle<()>) {
    let port = free_port();
    let src = source
        .replace("PORT", &port.to_string())
        .replace("COUNT", &count.to_string());
    let handle = std::thread::spawn(move || {
        let program = compile_source(&src).unwrap_or_else(|e| panic!("{e:?}"));
        let mut machine = Vm::with_output(program, Box::new(Captured::default()));
        machine.run_main(&[]).expect("runs");
    });
    std::thread::sleep(std::time::Duration::from_millis(150));
    (port, handle)
}

/// A request with headers, giving the whole raw response.
fn raw(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connects");
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    stream.write_all(request.as_bytes()).unwrap();
    let mut out = String::new();
    let _ = stream.read_to_string(&mut out);
    out
}

#[test]
fn requests_responses_and_middleware() {
    let (port, server) = serve_program(
        "use vyrn.http\n\
         create timing(request, next):\n\
         \x20   if request.header(\"X-Key\") != \"secret\": return http.Response(401, {\"error\": \"who are you?\"})\n\
         \x20   let answer = next(request)\n\
         \x20   return answer\n\
         end\n\
         route GET \"/echo\" (request):\n\
         \x20   return {\"method\": request.method, \"path\": request.path, \"agent\": request.headers[\"user-agent\"], \"q\": request.query}\n\
         end\n\
         route POST \"/raw\" (request):\n\
         \x20   return http.Response(201, request.json, {\"X-Made-By\": \"vyrn\"})\n\
         end\n\
         route GET \"/page\" ():\n\
         \x20   return http.Response(200, \"<h1>hi</h1>\", {\"Content-Type\": \"text/html\", \"Cache-Control\": \"no-store\"})\n\
         end\n\
         main:\n\
         \x20   http.use(timing)\n\
         \x20   http.serve(port = PORT, requests = COUNT)\n\
         end\n",
        4,
    );
    let echo = raw(
        port,
        "GET /echo?a=1 HTTP/1.1\r\nHost: x\r\nUser-Agent: test\r\nX-Key: secret\r\n\r\n",
    );
    assert!(echo.starts_with("HTTP/1.1 200"), "{echo}");
    assert!(
        echo.ends_with(
            "{\"method\":\"GET\",\"path\":\"/echo\",\"agent\":\"test\",\"q\":{\"a\":\"1\"}}"
        ),
        "{echo}"
    );
    let body = "{\"n\": 5}";
    let made = raw(
        port,
        &format!("POST /raw HTTP/1.1\r\nX-Key: secret\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()),
    );
    assert!(
        made.starts_with("HTTP/1.1 201")
            && made.contains("X-Made-By: vyrn")
            && made.ends_with("{\"n\":5}"),
        "{made}"
    );
    let page = raw(port, "GET /page HTTP/1.1\r\nX-Key: secret\r\n\r\n");
    assert!(
        page.contains("Content-Type: text/html\r\n")
            && page.contains("Cache-Control: no-store")
            && page.ends_with("<h1>hi</h1>"),
        "{page}"
    );
    let denied = raw(port, "GET /echo HTTP/1.1\r\n\r\n");
    assert!(
        denied.starts_with("HTTP/1.1 401") && denied.contains("who are you?"),
        "{denied}"
    );
    server.join().unwrap();
}

#[test]
fn html_forms_fill_route_parameters() {
    let (port, server) = serve_program(
        "use vyrn.http\n\
         route POST \"/signup\" (name: str, age: int, request):\n\
         \x20   return {\"name\": name, \"age\": age + 1, \"form\": request.form}\n\
         end\n\
         main:\n\
         \x20   http.serve(port = PORT, requests = COUNT)\n\
         end\n",
        1,
    );
    let body = "name=Ada+Lovelace&age=36&note=a%26b";
    let out = raw(
        port,
        &format!("POST /signup HTTP/1.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}", body.len()),
    );
    assert!(
        out.starts_with("HTTP/1.1 200")
            && out.ends_with("{\"name\":\"Ada Lovelace\",\"age\":37,\"form\":{\"age\":\"36\",\"name\":\"Ada Lovelace\",\"note\":\"a&b\"}}"),
        "{out}"
    );
    server.join().unwrap();
}

#[test]
fn programs_can_test_their_own_routes() {
    let program = compile_source(
        "use vyrn.http\n\
         struct Note:\n    title: str\nend\n\
         route GET \"/\" (): return \"<h1>Hello</h1>\"\n\
         route GET \"/hello/{name}\" (name: str, times: int = 1): return \"hi {name}\" * times\n\
         route POST \"/signup\" (name: str, age: int): return 201, {\"name\": name, \"next\": age + 1}\n\
         route POST \"/notes\" (body: Note): return {\"saved\": body.title}\n\
         route GET \"/secret\" (): return \"cake\"\n\
         create guard(request, next):\n\
         \x20   if request.path == \"/secret\" and request.header(\"X-Key\") != \"k\": return http.Response(401, \"no\")\n\
         \x20   return next(request)\n\
         end\n\
         main:\n\
         \x20   http.use(guard)\n\
         \x20   let r = http.test(\"GET\", \"/\")\n\
         \x20   print(r, r.status, r.ok, r.text, r.headers[\"content-type\"], r[\"status\"])\n\
         \x20   print(http.test(\"GET\", \"/hello/ada?times=2\").text)\n\
         \x20   let s = http.test(\"POST\", \"/signup\", form = {\"name\": \"Ada L\", \"age\": 36})\n\
         \x20   print(s.status, s.json)\n\
         \x20   print(http.test(\"POST\", \"/notes\", {\"title\": \"milk\"}).json)\n\
         \x20   print(http.test(\"GET\", \"/secret\").status, http.test(\"GET\", \"/secret\", headers = {\"X-Key\": \"k\"}).text)\n\
         \x20   print(http.test(\"GET\", \"/nope\").status, http.test(\"POST\", \"/signup\", form = {\"name\": \"x\"}).status)\n\
         end\n",
    )
    .unwrap();
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.run_main(&[]).unwrap();
    assert_eq!(
        out.text(),
        "<response 200> 200 true <h1>Hello</h1> text/html; charset=utf-8 200\n\
         hi adahi ada\n\
         201 {\"name\": \"Ada L\", \"next\": 37}\n\
         {\"saved\": \"milk\"}\n\
         401 cake\n\
         404 400\n"
    );
}

#[test]
fn route_parameters_left_out_take_their_defaults() {
    let program = compile_source(
        "use vyrn.http\n\
         route GET \"/search\" (q: str, page: int = 1, tag: str? = none): return \"{q} {page} {tag}\"\n\
         main:\n\
         \x20   print(http.test(\"GET\", \"/search?q=tea\").text, http.test(\"GET\", \"/search?q=tea&page=3&tag=x\").text)\n\
         \x20   print(http.test(\"GET\", \"/search\").status, int(\"ff\", 16), int(\"-0b101\", 2))\n\
         end\n",
    )
    .unwrap();
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.run_main(&[]).unwrap();
    assert_eq!(out.text(), "tea 1 none tea 3 x\n400 255 -5\n");
}

#[test]
fn server_sent_events_stream_to_the_page() {
    let (port, server) = serve_program(
        "use vyrn.http\n\
         route GET \"/ticks\" (stream):\n\
         \x20   each i in 1 to 3:\n\
         \x20       stream.send({\"tick\": i})\n\
         \x20       await sleep(0.01)\n\
         \x20   end\n\
         \x20   stream.send(\"bye\", event = \"done\")\n\
         end\n\
         main: http.serve(port = PORT, requests = COUNT)\n",
        1,
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET /ticks HTTP/1.1\r\nAccept: text/event-stream\r\n\r\n")
        .unwrap();
    let mut out = String::new();
    let _ = stream.read_to_string(&mut out);
    server.join().unwrap();
    assert!(
        out.starts_with("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream"),
        "{out}"
    );
    assert!(
        out.ends_with("data: {\"tick\":1}\n\ndata: {\"tick\":2}\n\ndata: {\"tick\":3}\n\nevent: done\ndata: bye\n\n"),
        "{out}"
    );
}

/// A WebSocket frame from a client: always masked.
fn client_frame(text: &str) -> Vec<u8> {
    let mask = [7u8, 1, 9, 3];
    let mut out = vec![0x81, 0x80 | text.len() as u8];
    out.extend_from_slice(&mask);
    out.extend(text.bytes().enumerate().map(|(i, b)| b ^ mask[i % 4]));
    out
}

#[test]
fn websockets_talk_both_ways() {
    let (port, server) = serve_program(
        "use vyrn.http\n\
         route GET \"/chat\" (socket):\n\
         \x20   socket.send(\"welcome\")\n\
         \x20   loop:\n\
         \x20       let msg = await socket.receive()\n\
         \x20       if msg == none: stop\n\
         \x20       if msg is map:\n\
         \x20           socket.send({\"got\": msg[\"n\"] * 2})\n\
         \x20       else:\n\
         \x20           socket.send(\"echo: {msg}\")\n\
         \x20       end\n\
         \x20   end\n\
         end\n\
         main: http.serve(port = PORT, requests = COUNT)\n",
        1,
    );
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    s.write_all(
        b"GET /chat HTTP/1.1\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
    )
    .unwrap();
    let mut buf = vec![0u8; 4096];
    let mut got = Vec::new();
    let read_until = |s: &mut TcpStream, got: &mut Vec<u8>, buf: &mut Vec<u8>, needle: &[u8]| {
        while !got.windows(needle.len()).any(|w| w == needle) {
            let n = s
                .read(buf)
                .unwrap_or_else(|e| panic!("{e}; so far: {:?}", String::from_utf8_lossy(got)));
            assert!(n > 0, "closed early: {:?}", String::from_utf8_lossy(got));
            got.extend_from_slice(&buf[..n]);
        }
    };
    read_until(&mut s, &mut got, &mut buf, b"welcome");
    let text = String::from_utf8_lossy(&got).into_owned();
    assert!(
        text.starts_with("HTTP/1.1 101")
            && text.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo="),
        "{text}"
    );
    s.write_all(&client_frame("hello")).unwrap();
    read_until(&mut s, &mut got, &mut buf, b"echo: hello");
    s.write_all(&client_frame("{\"n\": 21}")).unwrap();
    read_until(&mut s, &mut got, &mut buf, b"{\"got\":42}");
    // Close: the handler's receive gives none and the route ends.
    s.write_all(&[0x88, 0x80, 0, 0, 0, 0]).unwrap();
    server.join().unwrap();
}
