// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn eeprom_write_and_sequential_read_and_nack() {
    let mut bus = I2c::new();
    bus.attach(0x50, Box::new(Eeprom::new())).unwrap();
    bus.write(0, 4, 0x50).unwrap();

    for byte in [0x10, 0xab] {
        bus.write(4, 4, byte).unwrap();
        bus.write(8, 4, 1).unwrap();
        assert_ne!(bus.read(12, 4).unwrap() & 1, 0);
    }

    bus.write(8, 4, 3).unwrap();
    bus.write(4, 4, 0x10).unwrap();
    bus.write(8, 4, 1).unwrap();
    bus.write(8, 4, 2).unwrap();
    assert_eq!(bus.read(4, 4).unwrap(), 0xab);
    bus.write(0, 4, 0x51).unwrap();
    bus.write(8, 4, 2).unwrap();
    assert_ne!(bus.read(12, 4).unwrap() & 8, 0);
}

#[test]
fn host_i2c_buffers_writes_and_preserves_write_read_repeated_start() {
    use std::cell::RefCell;
    use std::io;
    use std::rc::Rc;

    type Calls = Rc<RefCell<Vec<(u8, Vec<u8>, usize)>>>;
    struct Transport(Calls);
    impl I2cTransport for Transport {
        fn transfer(&mut self, address: u8, write: &[u8], read: &mut [u8]) -> io::Result<()> {
            self.0
                .borrow_mut()
                .push((address, write.to_vec(), read.len()));
            read.copy_from_slice(&[0xab, 0xcd][..read.len()]);
            Ok(())
        }
    }
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut bus = I2c::with_host(Box::new(Transport(calls.clone())));
    bus.write(0, 4, 0x50).unwrap();
    bus.write(16, 4, 1 << 16).unwrap();
    bus.write(4, 4, 0x10).unwrap();
    bus.write(8, 4, 1).unwrap();
    assert!(calls.borrow().is_empty());
    bus.write(8, 4, 4).unwrap();
    assert!(calls.borrow().is_empty());
    bus.write(8, 4, 2).unwrap();
    assert_eq!(bus.read(4, 4).unwrap(), 0xab);
    bus.write(8, 4, 2).unwrap();
    assert_eq!(bus.read(4, 4).unwrap(), 0xcd);
    assert_eq!(*calls.borrow(), [(0x50, vec![0x10], 2)]);
    bus.write(8, 4, 3).unwrap();
    for byte in [0x10, 0x42] {
        bus.write(4, 4, byte).unwrap();
        bus.write(8, 4, 1).unwrap();
    }
    assert_eq!(calls.borrow().len(), 1);
    bus.write(8, 4, 3).unwrap();
    assert_eq!(calls.borrow()[1], (0x50, vec![0x10, 0x42], 0));
    assert_eq!(bus.read(12, 4).unwrap(), 5);
}

#[test]
fn host_i2c_rejects_invalid_accesses_and_latches_transport_failures() {
    use std::cell::Cell;
    use std::io;
    use std::rc::Rc;

    struct Failing(Rc<Cell<usize>>);
    impl I2cTransport for Failing {
        fn transfer(&mut self, _: u8, _: &[u8], _: &mut [u8]) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "injected failure",
            ))
        }
    }
    let calls = Rc::new(Cell::new(0));
    let mut bus = I2c::with_host(Box::new(Failing(calls.clone())));
    for (offset, width, value) in [(8, 1, 1), (8, 4, 5), (24, 4, 0), (24, 4, 4097), (0, 4, 128)] {
        assert!(bus.write(offset, width, value).is_err());
    }
    assert_eq!(calls.get(), 0);
    bus.write(0, 4, 0x50).unwrap();
    bus.write(8, 4, 1).unwrap();
    bus.write(16, 4, 1 << 16).unwrap();
    assert_eq!(calls.get(), 0);
    assert!(bus.write(8, 4, 3).is_err());
    assert_eq!(calls.get(), 1);
    assert!(bus.host_error().unwrap().contains("injected failure"));
    assert!(bus.write(8, 4, 2).is_err());
    assert_eq!(calls.get(), 1);
}

#[test]
fn host_i2c_rejects_read_overrun_and_bounds_pending_writes() {
    use std::io;
    struct Transport;
    impl I2cTransport for Transport {
        fn transfer(&mut self, _: u8, _: &[u8], read: &mut [u8]) -> io::Result<()> {
            read.fill(0x5a);
            Ok(())
        }
    }
    let mut bus = I2c::with_host(Box::new(Transport));
    bus.write(0, 4, 0x50).unwrap();
    bus.write(8, 4, 2).unwrap();
    assert_eq!(bus.read(4, 4).unwrap(), 0x5a);
    assert!(bus.write(8, 4, 2).is_err());
    assert!(bus.host_error().unwrap().contains("burst exhausted"));
    let mut bus = I2c::with_host(Box::new(Transport));
    bus.write(0, 4, 0x50).unwrap();
    for _ in 0..4096 {
        bus.write(8, 4, 1).unwrap();
    }
    assert!(bus.write(8, 4, 1).is_err());
    assert!(bus.host_error().unwrap().contains("4096 bytes"));
}

#[test]
fn host_i2c_address_allowlist_blocks_transfers_before_the_host_is_called() {
    use std::cell::Cell;
    use std::io;
    use std::rc::Rc;

    struct Transport(Rc<Cell<usize>>);
    impl I2cTransport for Transport {
        fn transfer(&mut self, _: u8, _: &[u8], read: &mut [u8]) -> io::Result<()> {
            self.0.set(self.0.get() + 1);
            read.fill(0x5a);
            Ok(())
        }
    }
    let calls = Rc::new(Cell::new(0));
    let mut bus = I2c::with_host(Box::new(Transport(calls.clone())));
    for invalid in [vec![], vec![0x07], vec![0x78], vec![0x50, 0x50]] {
        assert!(bus.set_host_addresses(invalid).is_err());
    }
    bus.set_host_addresses(vec![0x50]).unwrap();
    bus.write(0, 4, 0x50).unwrap();
    bus.write(8, 4, 2).unwrap();
    assert_eq!(calls.get(), 1);
    bus.write(8, 4, 3).unwrap();
    bus.write(0, 4, 0x51).unwrap();
    assert!(bus.write(8, 4, 1).is_err());
    assert_eq!(calls.get(), 1);
    assert!(bus.host_error().unwrap().contains("0x51 is not permitted"));
}

#[test]
fn host_read_length_fits_the_frozen_sdk_window_and_preserves_interrupt_enable() {
    let mut bus = I2c::new();
    assert_eq!(bus.read(16, 4).unwrap(), 0);
    bus.write(16, 4, (4095 << 16) | 1).unwrap();
    assert_eq!(bus.read(16, 4).unwrap(), (4095 << 16) | 1);
    assert!(bus.read(24, 4).is_err());
    assert!(bus.write(24, 4, 1).is_err());
    bus.write(16, 4, 0).unwrap();
    assert_eq!(bus.read(16, 4).unwrap(), 0);
}
