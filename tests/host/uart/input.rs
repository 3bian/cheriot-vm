// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use nix::pty::openpty;
use nix::sys::termios::{cfmakeraw, tcgetattr, tcsetattr, SetArg, SpecialCharacterIndices};
use nix::unistd::write;

use super::*;

#[test]
fn idle_tty_reads_do_not_end_input_for_zero_and_nonzero_vmin() {
    for minimum in [1, 0] {
        let pty = openpty(None, None).unwrap();
        let mut settings = tcgetattr(&pty.slave).unwrap();
        cfmakeraw(&mut settings);
        settings.control_chars[SpecialCharacterIndices::VMIN as usize] = minimum;
        settings.control_chars[SpecialCharacterIndices::VTIME as usize] = 0;
        tcsetattr(&pty.slave, SetArg::TCSANOW, &settings).unwrap();
        let mut bus = Bus::new();
        bus.add_device("uart".into(), 0x1000, 256, Box::new(Uart::new()))
            .unwrap();
        let path = format!("/proc/self/fd/{}", pty.slave.as_raw_fd());
        let mut input = UartInput::open(&bus, "uart", std::path::Path::new(&path)).unwrap();
        input.poll(&mut bus).unwrap();
        assert!(!input.eof, "VMIN={minimum}");
        write(&pty.master, b"Z").unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while bus.read(0x1004, 4).unwrap() & 1 == 0 {
            assert!(
                Instant::now() < deadline,
                "late byte lost at VMIN={minimum}"
            );
            input.poll(&mut bus).unwrap();
            std::thread::yield_now();
        }
        assert_eq!(bus.read(0x1000, 4).unwrap(), u32::from(b'Z'));
        assert_eq!(tcgetattr(&pty.slave).unwrap(), settings);
        drop(pty.master);
        input.poll(&mut bus).unwrap();
        assert!(input.eof, "hangup must terminate polling");
    }
}

#[test]
fn regular_file_eof_remains_permanent() {
    let path = std::env::temp_dir().join(format!("uart-eof-{}", std::process::id()));
    std::fs::write(&path, []).unwrap();
    let mut bus = Bus::new();
    bus.add_device("uart".into(), 0x1000, 256, Box::new(Uart::new()))
        .unwrap();
    let mut input = UartInput::open(&bus, "uart", &path).unwrap();
    input.poll(&mut bus).unwrap();
    assert!(input.eof);
    std::fs::write(&path, b"X").unwrap();
    input.poll(&mut bus).unwrap();
    assert_eq!(bus.read(0x1004, 4).unwrap() & 1, 0);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn closed_pipe_eof_remains_permanent() {
    let (read, write) = nix::unistd::pipe().unwrap();
    let mut bus = Bus::new();
    bus.add_device("uart".into(), 0x1000, 256, Box::new(Uart::new()))
        .unwrap();
    let path = format!("/proc/self/fd/{}", read.as_raw_fd());
    let mut input = UartInput::open(&bus, "uart", std::path::Path::new(&path)).unwrap();
    drop(write);
    input.poll(&mut bus).unwrap();
    assert!(input.eof);
    input.poll(&mut bus).unwrap();
    assert_eq!(bus.read(0x1004, 4).unwrap() & 1, 0);
}
