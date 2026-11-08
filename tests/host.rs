// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[path = "firmware/words.rs"]
mod firmware_words;

#[path = "host/connections.rs"]
mod connections;
#[path = "host/ethernet.rs"]
mod ethernet;
#[path = "host/gpio.rs"]
mod gpio;
#[path = "host/gpio/linux.rs"]
mod gpio_linux;
#[path = "host/uart/stdin.rs"]
mod uart_stdin;
#[path = "host/uart/streams.rs"]
mod uart_streams;
