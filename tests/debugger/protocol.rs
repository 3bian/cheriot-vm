// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#![cfg(all(feature = "gdb", target_os = "linux"))]

use std::fs;
use std::io::{Read, Write};
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use self::session::{Session, NEXT};

#[path = "protocol/session.rs"]
mod session;
#[path = "../firmware/uart_htif.rs"]
mod uart_htif;

fn assert_htif_routing(device: &str, detach: bool) {
    use std::os::fd::AsRawFd;

    use nix::fcntl::OFlag;
    use nix::unistd::pipe2;

    for profile in ["cheriot", "rv32e", "rv32imc"] {
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
                &uart_htif::program(profile == "cheriot", byte, true),
                "100",
                configuration.to_str().unwrap(),
                &["--completion", "0x80000100", "--uart", &uart],
                raw,
            );
            drop(writer);
            if detach {
                assert_eq!(session.query("D"), "OK");
            } else {
                session.packet("c");
                assert_eq!(session.response(), "W00", "{profile}/{device}/{raw}");
            }
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

fn boundary_elf(profile: &str) -> Vec<u8> {
    let mut image = vec![0_u8; 0x108];
    image[..7].copy_from_slice(b"\x7fELF\x01\x01\x01");
    for (offset, value) in [(16, 2_u16), (18, 243), (40, 52), (42, 32), (44, 2)] {
        image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }
    let flags = match profile {
        "cheriot" => 0x70009,
        "rv32ec" => 9,
        _ => 1,
    };
    for (offset, value) in [(20, 1_u32), (24, 0x8000_0ffe), (28, 52), (36, flags)] {
        image[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (index, (address, offset, size)) in
        [(0x8000_0ffe_u32, 0x100_u32, 2_u32), (0x8000_1000, 0x102, 6)]
            .into_iter()
            .enumerate()
    {
        for (field, value) in [1, offset, address, address, size, size, 5, 1]
            .into_iter()
            .enumerate()
        {
            let start = 52 + index * 32 + field * 4;
            image[start..start + 4].copy_from_slice(&value.to_le_bytes());
        }
    }
    image[0x100..].copy_from_slice(&[0x93, 0x80, 0x10, 0x00, 0x73, 0x00, 0x10, 0x00]);
    image
}

fn boundary_profile(profile: &str, split: bool) -> String {
    let section = if profile == "cheriot" {
        "cheriot-memory"
    } else {
        "memory"
    };
    let first_size = if split { 0x1000 } else { 0x2000 };
    let mut text = format!(
        "[machine]\nname='boundary'\n\
         [processor]\ntype='{profile}'\n\
         [[{section}]]\nname='first'\ntype='ram'\nbase=0x80000000\nsize={first_size}\n"
    );
    if split {
        text.push_str(&format!(
            "[[{section}]]\nname='second'\ntype='ram'\nbase=0x80001000\nsize=0x1000\n"
        ));
    }
    if profile == "cheriot" {
        text.push_str(
            "[[cheriot-memory]]\nname='bitmap'\ntype='ram'\nbase=0x83000000\nsize=0x100000\n",
        );
    }
    text
}

fn increment_guest() -> Vec<u8> {
    crate::firmware_words::firmware_from_words(&[0x0010_8093_u32; 5])
}

fn invalid_htif_after_uart(cheriot: bool) -> Vec<u8> {
    let mut firmware = uart_htif::program(cheriot, b'H', false);
    let old = 0x0101_01b7_u32.to_le_bytes();
    let new = 0x0200_01b7_u32.to_le_bytes();
    let offset = firmware.windows(4).position(|bytes| bytes == old).unwrap();
    firmware[offset..offset + 4].copy_from_slice(&new);
    firmware
}

#[test]
fn control_byte_inside_a_packet_is_not_an_interrupt() {
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let mut session = Session::new_with_profile(&increment_guest(), "20", profile);
        session.query("qSupported:swbreak+;hwbreak+");
        assert_ne!(session.query("X80000000,1:\x03"), "OK");
        assert_eq!(session.query("m80000000,4"), "93801000");
        session.packet("s");
        assert!(session.response().starts_with("S05"), "{profile}");
        assert_eq!(session.query("p1"), "01000000");
        session.packet("k");
        session.wait_failure();
    }
}

#[test]
fn debugger_breakpoints_support_instructions_spanning_executable_regions() {
    let mut mismatches = Vec::new();
    for profile in ["cheriot", "rv32ec", "rv32imc"] {
        for split in [false, true] {
            let configuration = std::env::temp_dir().join(format!(
                "vm-debugger-boundary-{}-{}.toml",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::write(&configuration, boundary_profile(profile, split)).unwrap();
            for (kind, expected) in [(0, "swbreak:;"), (1, "hwbreak:;")] {
                let mut session = Session::new_with_image(
                    &boundary_elf(profile),
                    "10",
                    configuration.to_str().unwrap(),
                    &["--completion", "disabled"],
                    false,
                    true,
                );
                session.query("qSupported:swbreak+;hwbreak+");
                assert_eq!(session.query("m80000ffe,8"), "9380100073001000");
                let result = session.query(&format!("Z{kind},80000ffe,4"));
                if result != "OK" {
                    mismatches.push(format!("{profile}/split={split}/Z{kind}: {result}"));
                } else {
                    session.packet("c");
                    assert!(session.response().contains(expected));
                    assert_eq!(session.query("p1"), "00000000");
                    assert_eq!(session.query("p20"), "fe0f0080");
                }
                assert!(session.query("s").starts_with("S05"));
                assert_eq!(session.query("p1"), "01000000");
                assert_eq!(session.query("p20"), "02100080");
                session.packet("k");
                session.wait_failure();
            }
            fs::remove_file(configuration).unwrap();
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("; "));
}

#[test]
fn debugger_flushes_guest_output_before_reporting_execution_failure() {
    use nix::fcntl::OFlag;
    use nix::unistd::pipe2;
    use std::os::fd::AsRawFd;

    for profile in ["cheriot", "rv32e", "rv32imc"] {
        for (raw, dedicated) in [(false, false), (true, false), (false, true)] {
            let firmware = invalid_htif_after_uart(profile == "cheriot");
            let (reader, writer) = pipe2(OFlag::O_CLOEXEC).unwrap();
            let uart = format!("out=/proc/{}/fd/{}", std::process::id(), writer.as_raw_fd());
            let mut arguments = vec!["--completion", "0x80000100"];
            if dedicated {
                arguments.extend(["--uart", &uart]);
            }
            let mut session = Session::new_with_arguments(
                &firmware,
                "100",
                &format!("profiles/{profile}.toml"),
                &arguments,
                raw,
            );
            drop(writer);
            session.packet("c");
            session.wait_failure();
            let mut output = Vec::new();
            session
                .child
                .stdout
                .as_mut()
                .unwrap()
                .read_to_end(&mut output)
                .unwrap();
            let errors = session.stderr();
            let mut redirected = Vec::new();
            fs::File::from(reader).read_to_end(&mut redirected).unwrap();
            assert_eq!(
                output,
                if dedicated { b"".as_slice() } else { b"U" },
                "{profile}/{raw}: {errors}"
            );
            assert_eq!(redirected, if dedicated { b"U".as_slice() } else { b"" });
            assert!(
                errors.contains("unsupported HTIF request"),
                "{profile}/{raw}: {errors}"
            );
        }
    }
}

#[test]
fn debugger_output_mode_survives_detach() {
    let mut mismatches = Vec::new();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        for raw in [false, true] {
            for action in ["continue", "detach", "detach-after-uart"] {
                let mut session = Session::new_with_arguments(
                    &uart_htif::program(profile == "cheriot", 0x1b, true),
                    "100",
                    &format!("profiles/{profile}.toml"),
                    &["--completion", "0x80000100"],
                    raw,
                );
                match action {
                    "continue" => {
                        session.packet("c");
                        assert_eq!(session.response(), "W00");
                    }
                    _ => {
                        if action == "detach-after-uart" {
                            for _ in 0..if profile == "cheriot" { 5 } else { 3 } {
                                assert!(session.query("s").starts_with("S05"));
                            }
                        }
                        assert_eq!(session.query("D"), "OK");
                    }
                }
                assert!(session.wait().success(), "{profile}/{raw}/{action}");
                let mut output = Vec::new();
                session
                    .child
                    .stdout
                    .as_mut()
                    .unwrap()
                    .read_to_end(&mut output)
                    .unwrap();
                let expected = if raw { b"U\x1b".as_slice() } else { b"U\\x1b" };
                if output != expected {
                    mismatches.push(format!("{profile}/{raw}/{action}: {output:?}"));
                }
            }
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("; "));
}

#[test]
fn debugger_preserves_execution_error_when_output_is_blocked() {
    use nix::errno::Errno;
    use nix::fcntl::OFlag;
    use nix::unistd::{pipe2, write};
    use std::os::fd::AsRawFd;

    let (_reader, writer) = pipe2(OFlag::O_CLOEXEC | OFlag::O_NONBLOCK).unwrap();
    loop {
        match write(&writer, &[b'X'; 4096]) {
            Ok(_) => (),
            Err(Errno::EAGAIN) => break,
            Err(error) => panic!("fill output pipe: {error}"),
        }
    }
    let uart = format!("out=/proc/{}/fd/{}", std::process::id(), writer.as_raw_fd());
    let mut session = Session::new_with_arguments(
        &invalid_htif_after_uart(false),
        "100",
        "profiles/rv32imc.toml",
        &["--completion", "0x80000100", "--uart", &uart],
        false,
    );
    drop(writer);
    let started = Instant::now();
    session.packet("c");
    session.wait_failure();
    assert!(started.elapsed() < Duration::from_secs(3));
    let errors = session.stderr();
    assert!(errors.contains("unsupported HTIF request"), "{errors}");
}

#[test]
fn detach_preserves_dedicated_uart_routing_and_htif_presentation() {
    assert_htif_routing("tohost", true);
}

#[test]
fn distinct_uart_name_routes_htif_to_stdout() {
    assert_htif_routing("uart0", false);
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
    let diagnostics = session.stderr();
    assert!(
        diagnostics.contains("step limit reached after 20 steps"),
        "{diagnostics}"
    );
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
fn monitor_errors_have_the_same_wire_output_on_both_architectures() {
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let mut session = Session::new_with_profile(&0x0000_006f_u32.to_le_bytes(), "0", profile);
        session.query("qSupported:swbreak+;hwbreak+");
        session.packet("qRcmd,6e6f73756368");
        let expected: String = b"error: unknown monitor command; use monitor help\n"
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(session.response(), format!("O{expected}"), "{profile}");
        assert_eq!(session.response(), "OK", "{profile}");
        session.packet("k");
        session.wait_failure();
    }
}

fn elapsed_profile(profile: &str) -> String {
    let text = fs::read_to_string(format!("profiles/{profile}.toml"))
        .unwrap()
        .replace("timing = \"per-step\"", "timing = \"elapsed\"")
        .replace("steps_per_tick = 100", "steps_per_tick = 1");
    let configuration = cheriot_vm::MachineConfiguration::parse(&text).unwrap();
    assert_eq!(configuration.timing, cheriot_vm::TimingPolicy::Elapsed);
    assert_eq!(configuration.steps_per_tick, 1);
    text
}

#[test]
fn inferred_clocks_pause_in_the_debugger_and_wfi_remains_interruptible() {
    let program = [0x1050_0073_u32.to_le_bytes(), 0xffdf_f06f_u32.to_le_bytes()].concat();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let configuration = std::env::temp_dir().join(format!(
            "cheriot-host-clock-{}-{}.toml",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let text = elapsed_profile(profile)
            .replace("[[timer64]]\nname = \"timer0\"\nbase = 0x02000000\n", "");
        fs::write(&configuration, text).unwrap();
        let mut session =
            Session::new_with_arguments(&program, "0", configuration.to_str().unwrap(), &[], false);
        session.query("qSupported:swbreak+;hwbreak+");
        let status_time = |session: &mut Session| {
            session.packet("qRcmd,737461747573");
            let response = session.response();
            assert!(response.starts_with('O'), "{response}");
            let bytes: Vec<u8> = response.as_bytes()[1..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            assert_eq!(session.response(), "OK");
            let text = String::from_utf8(bytes).unwrap();
            text.split("virtual_time=")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<u64>()
                .unwrap()
        };
        assert_eq!(status_time(&mut session), 0);
        thread::sleep(Duration::from_millis(20));
        assert_eq!(status_time(&mut session), 0);
        session.stream.write_all(&[3]).unwrap();
        assert_eq!(status_time(&mut session), 0);
        for _ in 0..2 {
            let previous = status_time(&mut session);
            session.packet("c");
            thread::sleep(Duration::from_millis(30));
            let interruption = Instant::now();
            session.stream.write_all(&[3]).unwrap();
            assert!(session.response().starts_with('S'));
            assert!(interruption.elapsed() < Duration::from_secs(2));
            let stopped = status_time(&mut session);
            assert!(stopped > previous, "{profile}: {previous} -> {stopped}");
            session.stream.write_all(&[3]).unwrap();
            assert_eq!(status_time(&mut session), stopped);
            thread::sleep(Duration::from_millis(20));
            assert_eq!(status_time(&mut session), stopped);
        }
        drop(session);
        fs::remove_file(configuration).unwrap();
    }
}

#[test]
fn interrupt_while_halted_at_a_breakpoint_preserves_its_resume_behavior() {
    let firmware = increment_guest();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        for kind in [0, 1] {
            for interrupt in [false, true] {
                let mut session = Session::new_with_profile(&firmware, "20", profile);
                session.query("qSupported:swbreak+;hwbreak+");
                assert_eq!(session.query(&format!("Z{kind},80000004,4")), "OK");
                session.packet("c");
                let expected = if kind == 0 { "swbreak" } else { "hwbreak" };
                assert!(
                    session.response().contains(expected),
                    "{profile}, kind={kind}"
                );
                assert_eq!(session.query("p20"), "04000080");
                assert_eq!(session.query("p1"), "01000000");
                if interrupt {
                    session.stream.write_all(&[3, 3]).unwrap();
                }
                assert_eq!(session.query("p20"), "04000080");
                for (pc, register) in [("08000080", "02000000"), ("0c000080", "03000000")] {
                    session.packet("s");
                    assert!(
                        session.response().starts_with("S05"),
                        "{profile}, kind={kind}, interrupt={interrupt}"
                    );
                    assert_eq!(session.query("p20"), pc);
                    assert_eq!(session.query("p1"), register);
                    if interrupt {
                        session.stream.write_all(&[3]).unwrap();
                        assert_eq!(session.query("p20"), pc);
                    }
                }
                assert_eq!(session.query("Z0,80000010,4"), "OK");
                session.packet("c");
                assert!(session.response().contains("swbreak"));
                assert_eq!(session.query("p20"), "10000080");
                assert_eq!(session.query("p1"), "04000000");
                session.packet("k");
                session.wait_failure();
            }
        }
    }
}

#[test]
fn interrupt_while_halted_at_startup_preserves_later_steps_and_continue() {
    let firmware = increment_guest();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        for interrupt in [false, true] {
            let mut session = Session::new_with_profile(&firmware, "20", profile);
            session.query("qSupported:swbreak+;hwbreak+");
            if interrupt {
                session.stream.write_all(&[3, 3]).unwrap();
            }
            assert_eq!(session.query("p20"), "00000080", "{profile}");
            for (pc, register) in [("04000080", "01000000"), ("08000080", "02000000")] {
                session.packet("s");
                assert!(
                    session.response().starts_with("S05"),
                    "{profile}, interrupt={interrupt}"
                );
                assert_eq!(session.query("p20"), pc, "{profile}");
                assert_eq!(session.query("p1"), register, "{profile}");
                if interrupt {
                    session.stream.write_all(&[3]).unwrap();
                    assert_eq!(session.query("p20"), pc, "{profile}");
                }
            }
            assert_eq!(session.query("Z0,8000000c,4"), "OK");
            session.packet("c");
            assert!(session.response().contains("swbreak"), "{profile}");
            assert_eq!(session.query("p20"), "0c000080");
            assert_eq!(session.query("p1"), "03000000");
            session.packet("k");
            session.wait_failure();
        }
    }
}

#[test]
fn interrupt_while_running_still_allows_step_and_resume() {
    let firmware = [0x0010_8093_u32.to_le_bytes(), 0xffdf_f06f_u32.to_le_bytes()].concat();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let mut session = Session::new_with_profile(&firmware, "0", profile);
        session.query("qSupported:swbreak+;hwbreak+");
        for _ in 0..2 {
            session.packet("c");
            thread::sleep(Duration::from_millis(20));
            session.stream.write_all(&[3]).unwrap();
            assert!(session.response().starts_with("S02"), "{profile}");
            let pc = session.query("p20");
            assert!(pc == "00000080" || pc == "04000080", "{profile}: {pc}");
            session.packet("s");
            assert!(session.response().starts_with("S05"), "{profile}");
            let next = session.query("p20");
            assert_eq!(
                next,
                if pc == "00000080" {
                    "04000080"
                } else {
                    "00000080"
                }
            );
        }
        session.packet("k");
        session.wait_failure();
    }
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
fn negotiated_memory_reads_respect_the_inspection_budget() {
    let firmware: Vec<u8> = (0..4096).map(|index| (index % 251) as u8).collect();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let mut session = Session::new_with_profile(&firmware, "20", profile);
        let features = session.query("qSupported:swbreak+;hwbreak+");
        assert!(features.contains("PacketSize=0800"), "{features}");
        for offset in [0, 1024, 2048, 3072] {
            let actual = session.query(&format!("m{:x},400", 0x8000_0000_u32 + offset));
            let expected: String = firmware[offset as usize..offset as usize + 1024]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(actual, expected, "{profile} offset {offset}");
        }
        assert!(session.query("m80000000,401").starts_with('E'));
        assert!(session.query("m10000000,4").starts_with('E'));
        assert!(session.query("mffffffff,2").starts_with('E'));
    }
}

#[test]
fn one_hertz_wfi_remains_interruptible_through_the_debugger() {
    let program = [0x1050_0073_u32.to_le_bytes(), 0xffdf_f06f_u32.to_le_bytes()].concat();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        let configuration = std::env::temp_dir().join(format!(
            "cheriot-slow-clock-{}-{}.toml",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let text = elapsed_profile(profile)
            .replace("timer_hz = 2000", "timer_hz = 1")
            .replace("[[timer64]]\nname = \"timer0\"\nbase = 0x02000000\n", "");
        fs::write(&configuration, text).unwrap();
        let mut session =
            Session::new_with_arguments(&program, "0", configuration.to_str().unwrap(), &[], false);
        session.query("qSupported:swbreak+;hwbreak+");
        session.packet("c");
        thread::sleep(Duration::from_millis(30));
        let started = Instant::now();
        session.stream.write_all(&[3]).unwrap();
        assert!(session.response().starts_with("S02"));
        assert!(started.elapsed() < Duration::from_millis(500), "{profile}");
        drop(session);
        fs::remove_file(configuration).unwrap();
    }
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
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(&words);
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
fn software_and_hardware_breakpoints_report_their_own_types() {
    let mut mismatches = Vec::new();
    for profile in ["cheriot", "rv32e", "rv32imc"] {
        for (kind, expected) in [(0, "swbreak:;"), (1, "hwbreak:;")] {
            let mut session =
                Session::new_with_profile(&0x0010_8093_u32.to_le_bytes(), "10", profile);
            session.query("qSupported:swbreak+;hwbreak+");
            assert_eq!(session.query(&format!("Z{kind},80000000,4")), "OK");
            session.packet("c");
            let stop = session.response();
            if !stop.contains(expected) {
                mismatches.push(format!("{profile}/Z{kind}: {stop}"));
            }
            assert_eq!(session.query("p1"), "00000000");
            assert_eq!(session.query("p20"), "00000080");
            session.packet("s");
            assert!(session.response().starts_with("S05"));
            assert_eq!(session.query("p1"), "01000000");
            session.packet("k");
            session.wait_failure();
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("; "));
}

#[test]
fn uart_named_tohost_still_routes_htif_to_stdout() {
    assert_htif_routing("tohost", false);
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
fn wire_removal_keeps_the_other_breakpoint_type_on_all_supplied_profiles() {
    let words = [0x0010_8093_u32, 0xffdf_f06f];
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(&words);
    for profile in ["cheriot", "rv32e", "rv32imc"] {
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
            let expected = if removed == 0 {
                "hwbreak:;"
            } else {
                "swbreak:;"
            };
            assert!(
                session.response().contains(expected),
                "{profile}, z{removed}"
            );
            assert_eq!(session.query("p1"), "00000000", "{profile}, z{removed}");
            assert_eq!(session.query("p20"), "00000080");
            session.packet("k");
            session.wait_failure();
        }
    }
}
