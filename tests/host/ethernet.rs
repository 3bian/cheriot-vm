// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::{self, ErrorKind};
use std::rc::Rc;

use cheriot_vm::cheriot::StepOutcome;
use cheriot_vm::cheriot::{permissions, Capability};
use cheriot_vm::devices::ethernet::{Ethernet, FrameTransport, NetworkBackend};
use cheriot_vm::devices::{Device, InterruptLine};
use cheriot_vm::host::ethernet::linux_tap;
use cheriot_vm::host::HostConnections;
use cheriot_vm::MachineConfiguration;

#[derive(Default)]
struct TransportState {
    incoming: VecDeque<Result<Vec<u8>, ErrorKind>>,
    receive_calls: usize,
    sent: Vec<Vec<u8>>,
    transmit_result: Option<Result<usize, ErrorKind>>,
}

struct TestTransport(Rc<RefCell<TransportState>>);

impl FrameTransport for TestTransport {
    fn receive(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        state.receive_calls += 1;
        let frame = state
            .incoming
            .pop_front()
            .unwrap_or(Err(ErrorKind::WouldBlock))?;
        let length = frame.len().min(buffer.len());
        buffer[..length].copy_from_slice(&frame[..length]);
        Ok(length)
    }

    fn transmit(&mut self, frame: &[u8]) -> io::Result<usize> {
        let mut state = self.0.borrow_mut();
        state.sent.push(frame.to_vec());
        state
            .transmit_result
            .take()
            .unwrap_or(Ok(frame.len()))
            .map_err(Into::into)
    }
}

#[test]
fn cheriot_guest_sends_and_receives_using_capability_authorized_register_access() {
    let profile = include_str!("../../profiles/cheriot.toml");
    let mut instructions: Vec<u32> = vec![
        0x03d000db, 0x20000137, 0x202080db, 0x00e00113, 0x0020a423, 0x11223137, 0x34410113,
        0x0020a623, 0x0020a623, 0x0020a623, 0x0020a623, 0x00100113, 0x0020a823, 0x0040a183,
    ];
    instructions.resize(100, 0x00000013);
    instructions.extend([0x0140a203, 0x0180a283, 0x00100113, 0x0020ae23]);
    let bytes: Vec<_> = crate::firmware_words::firmware_from_words(&instructions);
    let mut system = MachineConfiguration::parse(profile)
        .unwrap()
        .build_cheriot_raw(0x80000000, &bytes)
        .unwrap();
    system.computer.timing = cheriot_vm::TimingPolicy::PerStep;
    system.computer.set_steps_per_tick(1).unwrap();
    let (device, state) = host_device();
    state.borrow_mut().incoming.push_back(Ok(vec![0x5a; 14]));
    *system
        .computer
        .bus
        .device_mut::<Ethernet>("ethernet0")
        .unwrap() = device;

    for _ in instructions {
        assert_eq!(system.step().unwrap(), None);
        assert!(matches!(
            system.last_outcome,
            Some(StepOutcome::Retired { .. })
        ));
    }

    assert_eq!(
        state.borrow().sent,
        [vec![
            0x44, 0x33, 0x22, 0x11, 0x44, 0x33, 0x22, 0x11, 0x44, 0x33, 0x22, 0x11, 0x44, 0x33,
        ]]
    );
    assert_eq!(
        system
            .computer
            .processor
            .read_register(3)
            .unwrap()
            .address(),
        3
    );
    assert_eq!(
        system
            .computer
            .processor
            .read_register(4)
            .unwrap()
            .address(),
        14
    );
    assert_eq!(
        system
            .computer
            .processor
            .read_register(5)
            .unwrap()
            .address(),
        0x5a5a5a5a
    );
    assert_eq!(
        system
            .computer
            .bus
            .device::<Ethernet>("ethernet0")
            .unwrap()
            .pending_frames(),
        0
    );
}

#[test]
fn host_frames_roundtrip_through_device_registers_and_interrupts() {
    let (mut device, state) = host_device();
    device.write(32, 4, 7).unwrap();
    let frame: Vec<_> = (0..1518).map(|index| index as u8).collect();
    send_frame(&mut device, &frame);
    assert_eq!(state.borrow().sent.as_slice(), std::slice::from_ref(&frame));
    assert_eq!(device.read(4, 4).unwrap(), 1);
    assert_eq!(device.interrupt(), Some(InterruptLine::MachineExternal));
    assert!(device.take_transmitted().is_none());
    device.write(4, 4, 1).unwrap();
    assert_eq!(device.interrupt(), None);

    state.borrow_mut().incoming.push_back(Ok(frame.clone()));
    device.tick(1);
    device.write(4, 4, 7).unwrap();
    assert_eq!(device.read(4, 4).unwrap(), 2);
    assert_eq!(device.interrupt(), Some(InterruptLine::MachineExternal));
    assert_eq!(receive_frame(&mut device), frame);
    assert_eq!(device.interrupt(), None);
}

#[test]
fn denied_capability_stores_cannot_transmit_host_frames() {
    let root = Capability::from_memory(0x7e3e_0000_2000_0010, true);
    let authorities = [
        root.clear_tag(),
        Capability::from_memory(root.to_memory().0 | (1 << 54), true),
        root.restrict_permissions(permissions::LOAD),
        root.set_bounds(2, true).0,
    ];

    for authority in authorities {
        let mut system = MachineConfiguration::parse(include_str!("../../profiles/cheriot.toml"))
            .unwrap()
            .build_cheriot_raw(0x80000000, &0x0020a023_u32.to_le_bytes())
            .unwrap();
        let (mut device, state) = host_device();
        device.write(8, 4, 14).unwrap();

        for _ in 0..4 {
            device.write(12, 4, 0x55555555).unwrap();
        }

        *system
            .computer
            .bus
            .device_mut::<Ethernet>("ethernet0")
            .unwrap() = device;
        system
            .computer
            .processor
            .write_register(1, authority)
            .unwrap();
        system
            .computer
            .processor
            .write_integer_register(2, 1)
            .unwrap();
        system.step().unwrap();
        assert!(matches!(
            system.last_outcome,
            Some(StepOutcome::Trapped { cause: 28, .. })
        ));
        assert!(state.borrow().sent.is_empty());
        assert_eq!(system.computer.last_store, None);

        system.computer.bus.write(0x20000010, 4, 1).unwrap();
        assert_eq!(state.borrow().sent, [vec![0x55; 14]]);
    }
}

#[test]
fn host_receive_work_and_queues_are_bounded() {
    let (mut device, state) = host_device();

    for index in 0..9 {
        state.borrow_mut().incoming.push_back(Ok(vec![index; 14]));
    }

    device.tick(u64::MAX);
    assert_eq!(state.borrow().receive_calls, 8);
    assert_eq!(device.pending_frames(), 8);
    device.tick(1);
    assert_eq!(device.pending_frames(), 8);
    assert_eq!(device.read(4, 4).unwrap(), 6);

    for index in 0..8 {
        assert_eq!(receive_frame(&mut device), vec![index; 14]);
    }

    assert_eq!(device.read(4, 4).unwrap(), 4);
    device.write(4, 4, 4).unwrap();

    for length in [0, 13, 1519, 65536] {
        state.borrow_mut().incoming.push_back(Ok(vec![0; length]));
        device.tick(1);
        assert_eq!(device.pending_frames(), 0);
        assert_eq!(device.read(4, 4).unwrap(), 4);
        device.write(4, 4, 4).unwrap();
    }
}

#[test]
fn host_transport_failures_do_not_complete_or_split_packets() {
    let (mut device, state) = host_device();

    for failure in [
        Err(ErrorKind::WouldBlock),
        Err(ErrorKind::Interrupted),
        Err(ErrorKind::BrokenPipe),
        Ok(13),
    ] {
        state.borrow_mut().transmit_result = Some(failure);
        send_frame(&mut device, &[42; 14]);
        assert_eq!(device.read(4, 4).unwrap(), 4);
        device.write(4, 4, 4).unwrap();
    }

    assert_eq!(state.borrow().sent.len(), 4);
    send_frame(&mut device, &[42; 13]);
    assert_eq!(state.borrow().sent.len(), 4);
    assert_eq!(device.read(4, 4).unwrap(), 4);
    device.write(4, 4, 4).unwrap();

    for failure in [ErrorKind::WouldBlock, ErrorKind::Interrupted] {
        state.borrow_mut().incoming.push_back(Err(failure));
        device.tick(1);
        assert_eq!(device.read(4, 4).unwrap(), 0);
    }

    state.borrow_mut().incoming.push_back(Err(ErrorKind::Other));
    device.tick(1);
    assert_eq!(device.read(4, 4).unwrap(), 4);
    device.write(4, 4, 4).unwrap();

    state.borrow_mut().incoming.push_back(Ok(vec![42; 14]));
    let calls = state.borrow().receive_calls;
    device.set_link(false);
    device.tick(1);
    send_frame(&mut device, &[42; 14]);
    assert_eq!(state.borrow().receive_calls, calls);
    assert_eq!(state.borrow().sent.len(), 4);
    assert_eq!(device.read(0, 4).unwrap(), 0);
    assert_eq!(device.read(4, 4).unwrap(), 4);
}

#[test]
fn tap_names_and_missing_interfaces_fail_cleanly() {
    for name in [
        "",
        ".",
        "..",
        "tap/path",
        "tap%u",
        "has space",
        "name\0",
        "0123456789abcdef",
    ] {
        let error = match linux_tap::open(name) {
            Err(error) => error.to_string(),
            Ok(_) => panic!("invalid TAP name accepted: {name:?}"),
        };
        assert!(error.contains("TAP interface names"), "{error}");
    }

    let profile = include_str!("../../profiles/rv32imc.toml");
    let mut computer = MachineConfiguration::parse(profile)
        .unwrap()
        .build(0x8000_0000)
        .unwrap();
    let connections = HostConnections {
        tap_interface: Some(("ethernet0".into(), "vm-absent0".into())),
        ..HostConnections::default()
    };
    assert!(connections.connect(&mut computer.bus).is_err());
    assert_eq!(computer.bus.read(0x20000000, 4).unwrap(), 1);

    let invalid_map = profile.replace("base = 0x30000000", "base = 0x20000000");
    let error = match MachineConfiguration::parse(&invalid_map)
        .unwrap()
        .build(0x8000_0000)
    {
        Err(error) => error.to_string(),
        Ok(_) => panic!("overlapping devices accepted"),
    };
    assert!(error.contains("overlap"), "{error}");
    assert!(!error.contains("cannot find TAP"), "{error}");
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires CHERIOT_TEST_TAP owned by the current user, up with address 192.0.2.1/24"]
fn tap_exchanges_arp_frames_with_linux() {
    let name = std::env::var("CHERIOT_TEST_TAP").expect("CHERIOT_TEST_TAP must name the test TAP");

    for cheriot in [false, true] {
        eprintln!("Checking TAP attachment with CHERIoT profile: {cheriot}");
        let mut bus = if cheriot {
            MachineConfiguration::parse(include_str!("../../profiles/cheriot.toml"))
                .unwrap()
                .build_cheriot_raw(0x80000000, &0x00000013_u32.to_le_bytes())
                .unwrap()
                .computer
                .bus
        } else {
            MachineConfiguration::parse(include_str!("../../profiles/rv32imc.toml"))
                .unwrap()
                .build(0x8000_0000)
                .unwrap()
                .bus
        };
        HostConnections {
            tap_interface: Some(("ethernet0".into(), name.clone())),
            ..HostConnections::default()
        }
        .connect(&mut bus)
        .unwrap();
        verify_arp_exchange(bus.device_mut::<Ethernet>("ethernet0").unwrap());
        drop(bus);

        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_cheriot-vm"));
        command.current_dir(env!("CARGO_MANIFEST_DIR"));

        if cheriot {
            command.args([
                "--profile",
                "profiles/cheriot.toml",
                "--elf",
                "examples/hello_world/hello_world.elf",
                "--breakpoint",
                "0x80000000",
            ]);
        } else {
            command.args([
                "--profile",
                "profiles/rv32imc.toml",
                "--raw",
                "examples/assembly/hello-uart.bin",
                "--address",
                "0x80000000",
                "--breakpoint",
                "0x80000000",
            ]);
        }

        let output = command
            .args(["--tap", &format!("device=ethernet0,interface={name}")])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[cfg(target_os = "linux")]
fn verify_arp_exchange(device: &mut Ethernet) {
    let mut request = vec![0xff; 6];
    request.extend([0x02, 0, 0, 0, 0, 2]);
    request.extend([0x08, 0x06, 0, 1, 0x08, 0, 6, 4, 0, 1]);
    request.extend([0x02, 0, 0, 0, 0, 2, 192, 0, 2, 2]);
    request.extend([0, 0, 0, 0, 0, 0, 192, 0, 2, 1]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut next_request = std::time::Instant::now();

    while std::time::Instant::now() < deadline {
        if std::time::Instant::now() >= next_request {
            send_frame(device, &request);
            assert_eq!(device.read(4, 4).unwrap() & 5, 1);
            next_request = std::time::Instant::now() + std::time::Duration::from_millis(50);
        }

        device.tick(1);

        while device.pending_frames() != 0 {
            let reply = receive_frame(device);

            if reply.len() >= 42 && reply[12..14] == [0x08, 0x06] && reply[20..22] == [0, 2] {
                assert_eq!(&reply[..6], &[0x02, 0, 0, 0, 0, 2]);
                assert_eq!(&reply[28..32], &[192, 0, 2, 1]);
                assert_eq!(&reply[38..42], &[192, 0, 2, 2]);
                return;
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    panic!("Linux did not reply to the guest ARP request through TAP");
}

fn host_device() -> (Ethernet, Rc<RefCell<TransportState>>) {
    let state = Rc::new(RefCell::new(TransportState::default()));
    let transport = TestTransport(state.clone());
    let device = Ethernet::new(NetworkBackend::Host(Box::new(transport)));
    (device, state)
}

fn receive_frame(device: &mut Ethernet) -> Vec<u8> {
    let length = device.read(20, 4).unwrap() as usize;
    let mut frame = Vec::new();

    while frame.len() < length {
        frame.extend(device.read(24, 4).unwrap().to_le_bytes());
    }

    frame.truncate(length);
    device.write(28, 4, 1).unwrap();
    frame
}

fn send_frame(device: &mut Ethernet, frame: &[u8]) {
    device.write(8, 4, frame.len() as u32).unwrap();

    for chunk in frame.chunks(4) {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        device.write(12, 4, u32::from_le_bytes(word)).unwrap();
    }

    device.write(16, 4, 1).unwrap();
}
