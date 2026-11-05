// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn loopback_and_missing_select() {
    let mut spi = Spi::new();
    spi.attach(2, Box::new(LoopbackPeripheral)).unwrap();
    spi.write(8, 4, 2).unwrap();
    spi.write(0, 4, 0x95).unwrap();
    assert_eq!(spi.read(0, 4).unwrap(), 0x95);
    spi.write(8, 4, 0).unwrap();
    spi.write(0, 4, 42).unwrap();
    assert_eq!(spi.read(4, 4).unwrap(), 3);
}

#[test]
fn host_spi_orders_selection_and_transfers_and_latches_failures() {
    use std::cell::RefCell;
    use std::io;
    use std::rc::Rc;

    struct Peripheral(Rc<RefCell<Vec<String>>>);
    impl SpiPeripheral for Peripheral {
        fn select(&mut self, selected: bool) -> io::Result<()> {
            self.0.borrow_mut().push(format!("select {selected}"));
            Ok(())
        }
        fn transfer(&mut self, value: u8, mode: u8) -> io::Result<u8> {
            self.0.borrow_mut().push(format!("transfer {value} {mode}"));
            if value == 0xff {
                return Err(io::Error::other("injected SPI failure"));
            }
            Ok(0x5a)
        }
    }
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut spi = Spi::new();
    spi.replace_peripheral(0, Box::new(Peripheral(calls.clone())))
        .unwrap();
    assert!(spi.write(0, 1, 7).is_err());
    assert!(spi.write(8, 4, 4).is_err());
    assert!(calls.borrow().is_empty());
    spi.write(12, 4, 3).unwrap();
    spi.write(8, 4, 0).unwrap();
    spi.write(8, 4, 0).unwrap();
    spi.write(0, 4, 0x42).unwrap();
    assert_eq!(spi.read(0, 4).unwrap(), 0x5a);
    spi.write(8, 4, u32::MAX).unwrap();
    assert_eq!(
        *calls.borrow(),
        ["select true", "transfer 66 3", "select false"]
    );
    spi.write(8, 4, 0).unwrap();
    assert!(spi.write(0, 4, 0xff).is_err());
    let count = calls.borrow().len();
    assert!(spi.host_error().unwrap().contains("injected SPI failure"));
    assert!(spi.write(0, 4, 0x42).is_err());
    assert_eq!(calls.borrow().len(), count);
}
