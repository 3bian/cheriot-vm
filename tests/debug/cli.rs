// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[cfg(all(unix, feature = "gdb"))]
mod gdb_server {
    use super::*;
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};
    use std::time::Instant;

    fn packet(stream: &mut TcpStream, request: &str) -> String {
        let checksum = request.bytes().fold(0u8, |sum, b| sum.wrapping_add(b));
        write!(stream, "${request}#{checksum:02x}").unwrap();
        let mut data = Vec::new();
        let mut b = [0u8; 1];
        loop {
            stream.read_exact(&mut b).unwrap();
            data.push(b[0]);
            if let Some(i) = data.iter().position(|c| *c == b'#')
                && data.len() >= i + 3
            {
                break;
            }
        }
        stream.write_all(b"+").unwrap();
        let start = data.iter().position(|c| *c == b'$').unwrap() + 1;
        let end = data.iter().position(|c| *c == b'#').unwrap();
        String::from_utf8(data[start..end].to_vec()).unwrap()
    }
    #[test]
    fn gdb_rsp_reads_registers_and_memory_sets_breakpoints_and_steps() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut child = Server(
            cheriot()
                .args([
                    "debug",
                    "tests/fixtures/hello_world.elf",
                    "--profile",
                    "profiles/cheriot.toml",
                    "--port",
                    &port.to_string(),
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            if let Ok(s) = TcpStream::connect(("127.0.0.1", port)) {
                break s;
            };
            assert!(
                Instant::now() < deadline,
                "debug server did not start; status {:?}",
                child.0.try_wait()
            );
            thread::sleep(Duration::from_millis(20));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        assert!(packet(&mut stream, "?").starts_with('T'));
        assert!(!packet(&mut stream, "g").is_empty());
        assert_eq!(packet(&mut stream, "m80000000,4"), "5b07d003");
        assert_eq!(packet(&mut stream, "Z0,80000000,4"), "OK");
        assert_eq!(packet(&mut stream, "z0,80000000,4"), "OK");
        assert!(packet(&mut stream, "s").starts_with('S'));
    }

    #[test]
    fn debug_launches_the_gdb_client_with_separate_arguments_after_listening() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake gdb");
        let record = dir.path().join("arguments.json");
        std::fs::write(
            &script,
            r##"#!/usr/bin/env python3
import json, os, socket, sys
with open(os.environ['CHERIOT_TEST_GDB_ARGS'], 'w') as f:
    json.dump(sys.argv[1:], f)
endpoint=sys.argv[-1].split(' ')[-1]
host,port=endpoint.rsplit(':',1)
s=socket.create_connection((host,int(port)),timeout=5)
s.settimeout(5)
s.sendall(b'$D#44')
data=b''
while b'#' not in data or len(data)<data.index(b'#')+3:
    data += s.recv(4096)
s.sendall(b'+')
s.close()
"##,
        )
        .unwrap();
        let mut mode = std::fs::metadata(&script).unwrap().permissions();
        mode.set_mode(0o755);
        std::fs::set_permissions(&script, mode).unwrap();
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = socket.local_addr().unwrap().port();
        drop(socket);
        let out = cheriot()
            .args([
                "debug",
                "tests/fixtures/hello_world.elf",
                "--profile",
                "profiles/cheriot.toml",
                "--port",
                &port.to_string(),
                "--gdb",
            ])
            .arg(&script)
            .env("CHERIOT_TEST_GDB_ARGS", &record)
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", stderr(&out));
        let args: Vec<String> = serde_json::from_slice(&std::fs::read(record).unwrap()).unwrap();
        assert_eq!(args[0], "tests/fixtures/hello_world.elf");
        assert_eq!(args[1], "-ex");
        assert_eq!(args[2], format!("target remote 127.0.0.1:{port}"));
        assert!(stdout(&out).contains("Hello world"));
    }
}

struct Dap {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: std::io::BufReader<std::process::ChildStdout>,
    sequence: u64,
}

impl Dap {
    fn start() -> Self {
        let mut child = cheriot()
            .args(["debug", "--dap"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: std::io::BufReader::new(child.stdout.take().unwrap()),
            child,
            sequence: 0,
        }
    }

    fn send(&mut self, command: &str, arguments: serde_json::Value) {
        use std::io::Write;
        self.sequence += 1;
        let body = serde_json::json!({
            "seq": self.sequence, "type": "request", "command": command, "arguments": arguments,
        })
        .to_string();
        write!(self.input, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.input.flush().unwrap();
    }

    fn receive(&mut self) -> serde_json::Value {
        use std::io::{BufRead, Read};
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert!(
                self.output.read_line(&mut line).unwrap() > 0,
                "adapter closed"
            );
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        self.output.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn response(&mut self, command: &str, console: &mut String) -> serde_json::Value {
        self.until(console, |m| {
            m["type"] == "response" && m["command"] == command
        })
    }

    fn event(&mut self, name: &str, console: &mut String) -> serde_json::Value {
        self.until(console, |m| m["type"] == "event" && m["event"] == name)
    }

    fn until(
        &mut self,
        console: &mut String,
        wanted: impl Fn(&serde_json::Value) -> bool,
    ) -> serde_json::Value {
        loop {
            let message = self.receive();
            if message["event"] == "output" {
                console.push_str(message["body"]["output"].as_str().unwrap());
            } else if wanted(&message) {
                return message;
            } else if message["event"] == "terminated" {
                panic!("the guest finished while the test was waiting for something else");
            }
        }
    }
}

#[test]
fn debug_adapter_launches_breaks_steps_and_runs_to_completion() {
    let mut dap = Dap::start();
    let mut console = String::new();
    dap.send("initialize", serde_json::json!({"adapterID": "cheriot"}));
    let capabilities = dap.response("initialize", &mut console);
    assert_eq!(capabilities["body"]["supportsInstructionBreakpoints"], true);

    dap.send(
        "launch",
        serde_json::json!({
            "program": "tests/fixtures/hello_world.elf",
            "profile": "profiles/cheriot.toml",
        }),
    );
    let launched = dap.response("launch", &mut console);
    assert_eq!(launched["success"], true, "{launched}");
    dap.event("initialized", &mut console);
    dap.send("configurationDone", serde_json::json!({}));
    dap.response("configurationDone", &mut console);
    assert_eq!(
        dap.event("stopped", &mut console)["body"]["reason"],
        "entry"
    );

    dap.send("stackTrace", serde_json::json!({"threadId": 1}));
    let frames = dap.response("stackTrace", &mut console);
    let entry = frames["body"]["stackFrames"][0]["instructionPointerReference"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(entry, "0x80000000");
    dap.send("variables", serde_json::json!({"variablesReference": 1}));
    let variables = dap.response("variables", &mut console);
    assert_eq!(variables["body"]["variables"][0]["name"], "pc");
    assert_eq!(variables["body"]["variables"][0]["value"], entry.as_str());
    dap.send(
        "readMemory",
        serde_json::json!({"memoryReference": entry, "count": 4}),
    );
    assert_eq!(dap.response("readMemory", &mut console)["success"], true);

    dap.send(
        "next",
        serde_json::json!({"threadId": 1, "granularity": "instruction"}),
    );
    dap.response("next", &mut console);
    assert_eq!(dap.event("stopped", &mut console)["body"]["reason"], "step");
    dap.send("evaluate", serde_json::json!({"expression": "pc"}));
    let pc = dap.response("evaluate", &mut console);
    assert_ne!(pc["body"]["result"], entry.as_str());

    let symbols = run(&[
        "inspect",
        "tests/fixtures/hello_world.elf",
        "--symbols",
        "--format",
        "json",
    ]);
    let symbols: serde_json::Value = serde_json::from_slice(&symbols.stdout).unwrap();
    let say_hello = symbols["symbols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|symbol| symbol["name"] == "_Z9say_hellov")
        .map(|symbol| format!("{:#010x}", symbol["address"].as_u64().unwrap()))
        .unwrap();
    dap.send(
        "setInstructionBreakpoints",
        serde_json::json!({"breakpoints": [{"instructionReference": say_hello}]}),
    );
    let set = dap.response("setInstructionBreakpoints", &mut console);
    assert_eq!(set["body"]["breakpoints"][0]["verified"], true);
    dap.send("continue", serde_json::json!({"threadId": 1}));
    dap.response("continue", &mut console);
    assert_eq!(
        dap.event("stopped", &mut console)["body"]["reason"],
        "breakpoint"
    );
    dap.send("stackTrace", serde_json::json!({"threadId": 1}));
    let frames = dap.response("stackTrace", &mut console);
    assert_eq!(frames["body"]["stackFrames"][0]["name"], "_Z9say_hellov");
    dap.send(
        "setInstructionBreakpoints",
        serde_json::json!({"breakpoints": []}),
    );
    dap.response("setInstructionBreakpoints", &mut console);
    dap.send("continue", serde_json::json!({"threadId": 1}));
    dap.response("continue", &mut console);
    assert_eq!(dap.event("exited", &mut console)["body"]["exitCode"], 0);
    dap.event("terminated", &mut console);
    assert!(console.contains("Hello world"), "{console}");

    dap.send("disconnect", serde_json::json!({}));
    dap.response("disconnect", &mut console);
    assert!(dap.child.wait().unwrap().success());
}
