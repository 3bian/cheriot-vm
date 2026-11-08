// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

pub(super) static NEXT: AtomicU64 = AtomicU64::new(0);

pub(super) struct Session {
    pub(super) child: Child,
    firmware: PathBuf,
    ready: String,
    pub(super) stream: TcpStream,
}

impl Session {
    pub(super) fn new(bytes: &[u8], steps: &str) -> Self {
        Self::new_with_profile(bytes, steps, "cheriot")
    }

    pub(super) fn new_with_arguments(
        bytes: &[u8],
        steps: &str,
        profile: &str,
        arguments: &[&str],
        raw_output: bool,
    ) -> Self {
        Self::new_with_image(bytes, steps, profile, arguments, raw_output, false)
    }

    pub(super) fn new_with_image(
        bytes: &[u8],
        steps: &str,
        profile: &str,
        arguments: &[&str],
        raw_output: bool,
        elf: bool,
    ) -> Self {
        let firmware = std::env::temp_dir().join(format!(
            "cheriot-gdb-{}-{}.bin",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&firmware, bytes).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_cheriot-vm"));
        command
            .args(["--profile", profile, if elf { "--elf" } else { "--raw" }])
            .arg(&firmware);
        if !elf {
            command.args(["--address", "0x80000000"]);
        }
        let mut child = command
            .args([
                "--gdb",
                &format!(
                    "ip={},port={},output={}",
                    "127.0.0.1",
                    0,
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
        let (address, ready) = match Self::listener_address(&mut child) {
            Ok(result) => result,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&firmware);
                panic!("debugger listener did not start: {error}");
            }
        };
        let stream =
            TcpStream::connect_timeout(&address, Duration::from_secs(5)).unwrap_or_else(|error| {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&firmware);
                panic!("cannot connect to debugger at {address}: {error}");
            });
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream.set_nodelay(true).unwrap();
        Self {
            child,
            firmware,
            ready,
            stream,
        }
    }

    pub(super) fn new_with_profile(bytes: &[u8], steps: &str, profile: &str) -> Self {
        Self::new_with_arguments(
            bytes,
            steps,
            &format!("profiles/{profile}.toml"),
            &[],
            false,
        )
    }

    fn listener_address(child: &mut Child) -> Result<(SocketAddr, String), String> {
        use nix::fcntl::{fcntl, FcntlArg, OFlag};
        use std::io::ErrorKind;

        let stderr = child
            .stderr
            .as_mut()
            .ok_or("missing debugger diagnostics")?;
        let flags = OFlag::from_bits_truncate(
            fcntl(&*stderr, FcntlArg::F_GETFL).map_err(|error| error.to_string())?,
        );
        fcntl(&*stderr, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK))
            .map_err(|error| error.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut line = Vec::new();
        let result = loop {
            let mut byte = [0];
            match stderr.read(&mut byte) {
                Ok(0) => {
                    break Err(format!(
                        "diagnostics closed: {}",
                        String::from_utf8_lossy(&line)
                    ))
                }
                Ok(_) => {
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        break Ok(());
                    }
                    if line.len() >= 512 {
                        break Err("debugger ready line exceeded 512 bytes".into());
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) =>
                {
                    if Instant::now() >= deadline {
                        break Err("listener startup timeout".into());
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => break Err(error.to_string()),
            }
        };
        fcntl(&*stderr, FcntlArg::F_SETFL(flags)).map_err(|error| error.to_string())?;
        result?;
        let ready = String::from_utf8(line).map_err(|error| error.to_string())?;
        let address = ready
            .strip_prefix("CHERIOT_GDB_READY ")
            .and_then(|rest| rest.split_whitespace().next())
            .ok_or_else(|| format!("unexpected debugger startup: {ready}"))?
            .parse::<SocketAddr>()
            .map_err(|error| error.to_string())?;
        Ok((address, ready))
    }

    pub(super) fn stderr(&mut self) -> String {
        let mut text = self.ready.clone();
        self.child
            .stderr
            .as_mut()
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        text
    }

    pub(super) fn packet(&mut self, body: &str) {
        let checksum = body.bytes().fold(0_u8, u8::wrapping_add);
        self.stream
            .write_all(format!("${body}#{checksum:02x}").as_bytes())
            .unwrap();
    }

    pub(super) fn query(&mut self, body: &str) -> String {
        self.packet(body);
        self.response()
    }

    pub(super) fn response(&mut self) -> String {
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

    pub(super) fn wait(&mut self) -> ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < deadline, "debugger failed to terminate");
            thread::sleep(Duration::from_millis(5));
        }
    }

    pub(super) fn wait_failure(&mut self) {
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
