// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io;
use std::net::{SocketAddr, TcpListener};

pub fn listen(address: SocketAddr) -> io::Result<TcpListener> {
    if !address.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "debugger requires a loopback address",
        ));
    }
    let listener = TcpListener::bind(address)?;
    listener.set_nonblocking(true)?;
    Ok(listener)
}
