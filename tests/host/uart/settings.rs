// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use nix::pty::openpty;
use nix::sys::termios::{tcgetattr, ControlFlags};

use super::{UartParity, UartSettings};

#[test]
fn standard_parity_clears_inherited_mark_space_parity() {
    let pty = openpty(None, None).unwrap();
    for parity in [UartParity::Even, UartParity::Odd] {
        let mut termios = tcgetattr(&pty.slave).unwrap();
        let cmspar = ControlFlags::from_bits_retain(nix::libc::CMSPAR);
        termios.control_flags.insert(cmspar);
        UartSettings {
            parity,
            ..UartSettings::default()
        }
        .apply(&mut termios)
        .unwrap();
        assert!(!termios.control_flags.intersects(cmspar));
        assert!(termios.control_flags.contains(ControlFlags::PARENB));
        assert_eq!(
            termios.control_flags.contains(ControlFlags::PARODD),
            matches!(parity, UartParity::Odd)
        );
    }
}
