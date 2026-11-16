//! `vyrn debug`: the terminal debugger and the Debug Adapter Protocol
//! server, driven by scripted input.

use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Capture {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

const PROGRAM: &str = "struct Player:\n    name: str\n    score: int\nend\n\
\n\
create bonus(points: int) -> int:\n    let doubled = points * 2\n    return doubled + 1\nend\n\
\n\
main:\n    var players = [Player(\"Ann\", 3), Player(\"Bo\", 5)]\n    var total = 0\n\
\x20   each p in players:\n        total += bonus(p.score)\n    end\n    print(\"total\", total)\nend\n";

fn program_file(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("vyrn-debug-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("score.vyrn");
    std::fs::write(&path, PROGRAM).unwrap();
    path
}

#[test]
fn the_terminal_debugger_steps_breaks_and_shows_variables() {
    let path = program_file("terminal");
    let out = Capture::default();
    let commands = "n\nn\nvars\nbreak 7\nc\nwhere\np points\nout\nn\np total\np players[1].name\np nothing\ndelete 7\nc\n";
    let code = vyrn::debugger::run_terminal(
        &path,
        &[],
        Box::new(std::io::Cursor::new(commands.as_bytes().to_vec())),
        Box::new(out.clone()),
    );
    let text = out.text();
    assert_eq!(code, 0, "{text}");
    for expected in [
        "-> score.vyrn:12 in main",
        "-> score.vyrn:14 in main",
        "players = [Player(name=\"Ann\", score=3), Player(name=\"Bo\", score=5)]\ntotal = 0",
        "breakpoint at score.vyrn:7",
        "-> breakpoint, score.vyrn:7 in bonus",
        "-> bonus at score.vyrn:7\n   main at score.vyrn:15",
        "points = 3",
        "total = 7",
        "players[1].name = \"Bo\"",
        "there is no `nothing` here",
        "total 18",
        "The program finished.",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in:\n{text}");
    }
}

#[test]
fn quitting_stops_the_program() {
    let path = program_file("quit");
    let out = Capture::default();
    vyrn::debugger::run_terminal(
        &path,
        &[],
        Box::new(std::io::Cursor::new(b"q\n".to_vec())),
        Box::new(out.clone()),
    );
    let text = out.text();
    assert!(
        text.contains("Stopped.") && !text.contains("total 18"),
        "{text}"
    );
}

/// Input that arrives in batches, as an editor sends it.
struct Feed {
    receive: std::sync::mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
    at: usize,
}

impl std::io::Read for Feed {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.at == self.buf.len() {
            match self.receive.recv() {
                Ok(b) => {
                    self.buf = b;
                    self.at = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.at);
        out[..n].copy_from_slice(&self.buf[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

fn frame(json: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{json}", json.len())
}

/// The messages the server sent, as JSON texts.
fn messages(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(at) = rest.find("Content-Length: ") {
        let after = &rest[at + 16..];
        let end = after.find("\r\n").unwrap();
        let len: usize = after[..end].parse().unwrap();
        let body_start = end + 4;
        out.push(after[body_start..body_start + len].to_string());
        rest = &after[body_start + len..];
    }
    out
}

#[test]
fn the_debug_adapter_answers_an_editor() {
    let path = program_file("dap");
    let p = path.display().to_string().replace('\\', "\\\\");
    let requests = [
        r#"{"seq":1,"type":"request","command":"initialize","arguments":{"adapterID":"vyrn"}}"#.to_string(),
        format!(r#"{{"seq":2,"type":"request","command":"launch","arguments":{{"program":"{p}"}}}}"#),
        format!(
            r#"{{"seq":3,"type":"request","command":"setBreakpoints","arguments":{{"source":{{"path":"{p}"}},"breakpoints":[{{"line":7}}]}}}}"#
        ),
        r#"{"seq":4,"type":"request","command":"configurationDone"}"#.to_string(),
        // At the breakpoint:
        r#"{"seq":5,"type":"request","command":"stackTrace","arguments":{"threadId":1}}"#.to_string(),
        r#"{"seq":6,"type":"request","command":"scopes","arguments":{"frameId":1}}"#.to_string(),
        r#"{"seq":7,"type":"request","command":"variables","arguments":{"variablesReference":11}}"#.to_string(),
        r#"{"seq":8,"type":"request","command":"evaluate","arguments":{"expression":"points","frameId":0}}"#.to_string(),
        r#"{"seq":9,"type":"request","command":"variables","arguments":{"variablesReference":100000}}"#.to_string(),
        // Off the breakpoint, then run to the end.
        format!(
            r#"{{"seq":10,"type":"request","command":"setBreakpoints","arguments":{{"source":{{"path":"{p}"}},"breakpoints":[]}}}}"#
        ),
        r#"{"seq":11,"type":"request","command":"continue","arguments":{"threadId":1}}"#.to_string(),
    ];
    // Like an editor: the first four, then the rest once it has stopped.
    let (send, receive) = std::sync::mpsc::channel::<Vec<u8>>();
    let out = Capture::default();
    let server = {
        let out = out.clone();
        std::thread::spawn(move || {
            let input = std::io::BufReader::new(Feed {
                receive,
                buf: Vec::new(),
                at: 0,
            });
            vyrn::debugger::run_dap(Box::new(input), Box::new(out));
        })
    };
    let first: String = requests[..4].iter().map(|r| frame(r)).collect();
    send.send(first.into_bytes()).unwrap();
    let start = std::time::Instant::now();
    while !out.text().contains("\"event\":\"stopped\"") {
        assert!(
            start.elapsed().as_secs() < 20,
            "never stopped:\n{}",
            out.text()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let rest: String = requests[4..].iter().map(|r| frame(r)).collect();
    send.send(rest.into_bytes()).unwrap();
    drop(send);
    server.join().unwrap();
    let sent = messages(&out.text());
    let find = |needle: &str| {
        sent.iter()
            .find(|m| m.contains(needle))
            .unwrap_or_else(|| panic!("no message with {needle}:\n{}", sent.join("\n")))
            .clone()
    };
    assert!(find("\"command\":\"initialize\"").contains("supportsConfigurationDoneRequest"));
    find("\"event\":\"initialized\"");
    assert!(find("\"request_seq\":3").contains("{\"verified\":true,\"line\":7}"));
    assert!(find("\"event\":\"stopped\"").contains("\"reason\":\"breakpoint\""));
    let stack = find("\"command\":\"stackTrace\"");
    assert!(
        stack.contains("\"name\":\"bonus\",\"line\":7")
            && stack.contains("\"name\":\"main\",\"line\":15"),
        "{stack}"
    );
    // main's variables (frame 1), with the loop variable.
    let vars = find("\"request_seq\":7");
    for v in [
        "\"name\":\"players\"",
        "\"name\":\"total\",\"value\":\"0\"",
        "\"name\":\"p\",\"value\":\"Player(name=\\\"Ann\\\", score=3)\"",
    ] {
        assert!(vars.contains(v), "missing {v} in {vars}");
    }
    assert!(find("\"request_seq\":8").contains("\"result\":\"3\""));
    // `players` (the first expandable value) opens into its items.
    assert!(find("\"request_seq\":9").contains("\"name\":\"0\""));
    let output = find("\"category\":\"stdout\"");
    assert!(output.contains("total 18"), "{output}");
    assert!(find("\"event\":\"exited\"").contains("\"exitCode\":0"));
    find("\"event\":\"terminated\"");
}
