//! `kalinga lsp` speaks the Language Server Protocol on stdin / stdout.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};

use serde_json::{Value, json};

struct Server {
    child: Child,
    out: BufReader<std::process::ChildStdout>,
}

impl Server {
    fn start() -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_kalinga"))
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start kalinga lsp");
        let out = BufReader::new(child.stdout.take().expect("stdout"));
        Server { child, out }
    }

    fn send(&mut self, msg: Value) {
        let body = msg.to_string();
        let stdin = self.child.stdin.as_mut().expect("stdin");
        write!(stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).expect("write");
        stdin.flush().expect("flush");
    }

    fn receive(&mut self) -> Value {
        let mut len = 0;
        loop {
            let mut line = String::new();
            self.out.read_line(&mut line).expect("header");
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(n) = line.strip_prefix("Content-Length: ") {
                len = n.parse().expect("length");
            }
        }
        let mut body = vec![0; len];
        self.out.read_exact(&mut body).expect("body");
        serde_json::from_slice(&body).expect("json")
    }

    /// The next message for which `pick` is true (others, like log messages, are skipped).
    fn receive_where(&mut self, pick: impl Fn(&Value) -> bool) -> Value {
        loop {
            let msg = self.receive();
            if pick(&msg) {
                return msg;
            }
        }
    }
}

#[test]
fn diagnostics_hover_and_shutdown_over_stdio() {
    let dir = std::env::temp_dir().join("kalinga-lsp-protocol");
    std::fs::create_dir_all(&dir).expect("dir");
    let uri =
        format!("file:///{}", dir.join("main.kl").display().to_string().replace('\\', "/").trim_start_matches('/'));
    let mut s = Server::start();
    s.send(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"capabilities": {}}}));
    let init = s.receive_where(|m| m["id"] == 1);
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true);
    s.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));

    let text = "fn twice(n: int) -> int {\n    return n * 2\n}\n\nfn main() {\n    let s: str = twice(2)\n}\n";
    s.send(json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
        "textDocument": {"uri": uri, "languageId": "kalinga", "version": 1, "text": text}}}));
    let diags = s.receive_where(|m| m["method"] == "textDocument/publishDiagnostics");
    let list = diags["params"]["diagnostics"].as_array().expect("list").clone();
    assert_eq!(list.len(), 1, "{:?}", list);
    assert_eq!(list[0]["range"]["start"]["line"], 5);
    assert!(list[0]["message"].as_str().is_some_and(|m| m.contains("expected `str`")), "{:?}", list[0]);

    s.send(json!({"jsonrpc": "2.0", "id": 2, "method": "textDocument/hover", "params": {
        "textDocument": {"uri": uri}, "position": {"line": 5, "character": 19}}}));
    let hover = s.receive_where(|m| m["id"] == 2);
    let value = hover["result"]["contents"]["value"].as_str().expect("hover text").to_string();
    assert!(value.contains("fn twice(n: int) -> int"), "{}", value);

    s.send(json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"}));
    s.receive_where(|m| m["id"] == 3);
    s.send(json!({"jsonrpc": "2.0", "method": "exit"}));
    let status = s.child.wait().expect("exit");
    assert!(status.success());
}
