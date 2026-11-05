// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(all(feature = "gdb", target_os = "linux"))]

use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[path = "../firmware/uart_htif.rs"]
mod uart_htif;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Session {
    child: Child,
    firmware: PathBuf,
    stream: TcpStream,
}

impl Session {
    fn new(bytes: &[u8], steps: &str) -> Self {
        Self::new_with_profile(bytes, steps, "cheriot1")
    }

    fn new_with_arguments(
        bytes: &[u8],
        steps: &str,
        profile: &str,
        arguments: &[&str],
        raw_output: bool,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let firmware = std::env::temp_dir().join(format!(
            "cheriot-gdb-{}-{}.bin",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&firmware, bytes).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"))
            .args(["--profile", profile, "--raw"])
            .arg(&firmware)
            .args([
                "--address",
                "0x80000000",
                "--gdb",
                &format!(
                    "ip={},port={},output={}",
                    address.ip(),
                    address.port(),
                    if raw_output { "raw" } else { "escaped" }
                ),
                "--steps",
                steps,
            ])
            .args(arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let stream = loop {
            if let Ok(stream) = TcpStream::connect(address) {
                break stream;
            }
            assert!(Instant::now() < deadline, "listener startup timeout");
            thread::sleep(Duration::from_millis(5));
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        Self {
            child,
            firmware,
            stream,
        }
    }

    fn new_with_profile(bytes: &[u8], steps: &str, profile: &str) -> Self {
        Self::new_with_arguments(
            bytes,
            steps,
            &format!("profiles/{profile}.toml"),
            &[],
            false,
        )
    }

    fn packet(&mut self, body: &str) {
        let checksum = body.bytes().fold(0_u8, u8::wrapping_add);
        self.stream
            .write_all(format!("${body}#{checksum:02x}").as_bytes())
            .unwrap();
    }

    fn response(&mut self) -> String {
        let mut byte = [0];
        loop {
            self.stream.read_exact(&mut byte).unwrap();
            if byte[0] == b'$' {
                break;
            }
        }
        let mut body = Vec::new();
        loop {
            self.stream.read_exact(&mut byte).unwrap();
            if byte[0] == b'#' {
                break;
            }
            body.push(byte[0]);
            assert!(body.len() < 8192);
        }
        let mut checksum = [0; 2];
        self.stream.read_exact(&mut checksum).unwrap();
        assert_eq!(
            u8::from_str_radix(std::str::from_utf8(&checksum).unwrap(), 16).unwrap(),
            body.iter().fold(0_u8, |sum, byte| sum.wrapping_add(*byte))
        );
        self.stream.write_all(b"+").unwrap();
        String::from_utf8(body).unwrap()
    }

    fn query(&mut self, body: &str) -> String {
        self.packet(body);
        self.response()
    }

    fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "debugger failed to terminate");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_failure(&mut self) {
        assert!(!self.wait().success());
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.firmware);
    }
}

fn assert_htif_routing(device: &str) {
    use std::os::fd::AsRawFd;

    use nix::fcntl::OFlag;
    use nix::unistd::pipe2;

    for profile in ["cheriot1", "rv32e", "rv32imc"] {
        for (byte, raw, expected) in [
            (b'H', false, b"H".as_slice()),
            (0x1b, false, b"\\x1b"),
            (0x1b, true, b"\x1b"),
        ] {
            let configuration = std::env::temp_dir().join(format!(
                "vm-debugger-output-{}-{}.toml",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::write(
                &configuration,
                fs::read_to_string(format!("profiles/{profile}.toml"))
                    .unwrap()
                    .replace("name = \"uart0\"", &format!("name = \"{device}\"")),
            )
            .unwrap();
            let (reader, writer) = pipe2(OFlag::O_CLOEXEC).unwrap();
            let uart = format!(
                "device={device},out=/proc/{}/fd/{}",
                std::process::id(),
                writer.as_raw_fd()
            );
            let mut session = Session::new_with_arguments(
                &uart_htif::program(profile == "cheriot1", byte, true),
                "100",
                configuration.to_str().unwrap(),
                &["--completion", "0x80000100", "--uart", &uart],
                raw,
            );
            drop(writer);
            session.packet("c");
            assert_eq!(session.response(), "W00", "{profile}/{device}/{raw}");
            assert!(session.wait().success());
            let mut uart = Vec::new();
            fs::File::from(reader).read_to_end(&mut uart).unwrap();
            let mut output = Vec::new();
            session
                .child
                .stdout
                .as_mut()
                .unwrap()
                .read_to_end(&mut output)
                .unwrap();
            assert_eq!(uart, b"U", "{profile}/{device}/{raw}: stdout={output:?}");
            assert_eq!(output, expected, "{profile}/{device}/{raw}");
            fs::remove_file(configuration).unwrap();
        }
    }
}

#[test]
fn distinct_uart_name_routes_htif_to_stdout() {
    assert_htif_routing("uart0");
}

#[test]
fn explicit_detach_resumes_the_same_cumulative_budget() {
    let mut session = Session::new(&[0x85, 0, 0x6f, 0xf0, 0xff, 0xff], "20");
    for _ in 0..3 {
        session.packet("s");
        assert!(session.response().starts_with("S05"));
    }
    assert_eq!(session.query("Z0,80000000,2"), "OK");
    assert_eq!(session.query("D"), "OK");
    session.wait_failure();
    let mut diagnostics = String::new();
    session
        .child
        .stderr
        .as_mut()
        .unwrap()
        .read_to_string(&mut diagnostics)
        .unwrap();
    assert!(
        diagnostics.contains("step limit reached after 20 steps"),
        "{diagnostics}"
    );
}

#[test]
fn wire_protocol_reads_rejects_writes_and_handles_virtual_breakpoints() {
    let mut session = Session::new(&[0x85, 0, 0x6f, 0xf0, 0xff, 0xff], "20");
    assert!(session
        .query("qSupported:swbreak+;hwbreak+")
        .contains("qXfer:features:read+"));
    assert_eq!(session.query("p20"), "00000080");
    assert_eq!(session.query("p10"), "xxxxxxxx");
    assert_eq!(session.query("p10000"), "");
    assert_eq!(session.query("m80000000,6"), "85006ff0ffff");
    assert!(session.query("m10000000,4").starts_with('E'));
    assert!(session.query("mffffffff,2").starts_with('E'));
    assert!(session.query("P1=01000000").starts_with('E'));
    assert!(session.query("M80000000,1:00").starts_with('E'));
    assert_eq!(session.query("Z0,80000000,2"), "OK");
    session.packet("c");
    assert!(session.response().contains("swbreak"));
    assert_eq!(session.query("p20"), "00000080");
    session.packet("s");
    assert!(session.response().starts_with("S05"));
    assert_eq!(session.query("p20"), "02000080");
    session.packet("c");
    assert!(session.response().contains("swbreak"));
    assert_eq!(session.query("m80000000,6"), "85006ff0ffff");
    assert_eq!(session.query("z0,80000000,2"), "OK");
    session.packet("c");
    assert!(session.response().starts_with('S'));
    session.packet("k");
    session.wait_failure();
}

#[test]
fn fragmented_requests_and_ctrl_c_remain_responsive() {
    let mut session = Session::new(&0x0000_006f_u32.to_le_bytes(), "0");
    for byte in b"$p20#d2" {
        session.stream.write_all(&[*byte]).unwrap();
    }
    assert_eq!(session.response(), "00000080");
    session.packet("c");
    thread::sleep(Duration::from_millis(20));
    let started = Instant::now();
    session.stream.write_all(&[3]).unwrap();
    assert!(session.response().starts_with("S02"));
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(session.query("p20"), "00000080");
    session.packet("k");
    session.wait_failure();
}

#[test]
fn output_backpressure_stops_guest_without_blocking_debug_control() {
    let words = [
        0x03d0_00db_u32,
        0x1000_0137,
        0x2020_80db,
        0x0410_0193,
        0x0030_a023,
        0xffdf_f06f,
    ];
    let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    let mut session = Session::new(&bytes, "0");
    session.packet("c");
    assert!(session.response().starts_with("S19"));
    let started = Instant::now();
    assert_eq!(session.query("p0"), "00000000");
    session.packet("k");
    session.wait_failure();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn malformed_oversized_and_pc_changing_requests_end_safely() {
    for packet in [
        b"$c80000000#eb".to_vec(),
        b"$p20#00".to_vec(),
        [vec![b'$'], vec![b'a'; 4097]].concat(),
    ] {
        let mut session = Session::new(&0x0000_006f_u32.to_le_bytes(), "0");
        session.stream.write_all(&packet).unwrap();
        session.wait_failure();
    }
    let mut session = Session::new(&0x0000_006f_u32.to_le_bytes(), "0");
    session.stream.shutdown(std::net::Shutdown::Both).unwrap();
    session.wait_failure();
}

#[test]
fn uart_named_tohost_still_routes_htif_to_stdout() {
    assert_htif_routing("tohost");
}

#[test]
fn wire_removal_keeps_the_other_breakpoint_type_on_all_supplied_profiles() {
    let words = [0x0010_8093_u32, 0xffdf_f06f];
    let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    for profile in ["cheriot1", "rv32e", "rv32imc"] {
        for removed in [0, 1] {
            let mut session = Session::new_with_profile(&bytes, "10", profile);
            session.query("qSupported:swbreak+;hwbreak+");
            for _ in 0..2 {
                assert_eq!(session.query("Z0,80000000,4"), "OK");
                assert_eq!(session.query("Z1,80000000,4"), "OK");
            }
            for _ in 0..2 {
                assert_eq!(session.query(&format!("z{removed},80000000,4")), "OK");
            }
            session.packet("c");
            assert!(session.response().contains("05"));
            assert_eq!(session.query("p1"), "00000000", "{profile}, z{removed}");
            assert_eq!(session.query("p20"), "00000080");
            session.packet("k");
            session.wait_failure();
        }
    }
}
