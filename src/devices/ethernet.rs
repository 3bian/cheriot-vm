// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::any::Any;
use std::collections::VecDeque;
use std::io::ErrorKind;

use crate::devices::{register_width, Device, DeviceActivity, InterruptLine};
use crate::error::EmulatorError;

mod frame_transport;
mod network_backend;

pub use frame_transport::FrameTransport;
pub use network_backend::NetworkBackend;

const MAX_FRAME: usize = 1518;
const MIN_HOST_FRAME: usize = 14;
const QUEUE_CAPACITY: usize = 8;

pub struct Ethernet {
    backend: NetworkBackend,
    interrupt_mask: u32,
    link: bool,
    outgoing_buffer: Vec<u8>,
    read_cursor: usize,
    received: VecDeque<Vec<u8>>,
    status: u32,
    transmit_length: usize,
    transmitted: VecDeque<Vec<u8>>,
}

impl Ethernet {
    pub fn new(backend: NetworkBackend) -> Self {
        Self {
            link: !matches!(backend, NetworkBackend::Disconnected),
            backend,
            interrupt_mask: 0,
            transmit_length: 0,
            outgoing_buffer: Vec::new(),
            transmitted: VecDeque::new(),
            received: VecDeque::new(),
            read_cursor: 0,
            status: 0,
        }
    }

    pub fn inject_frame(&mut self, frame: &[u8]) -> Result<(), EmulatorError> {
        if !self.link
            || !matches!(self.backend, NetworkBackend::InMemory)
            || frame.is_empty()
            || frame.len() > MAX_FRAME
            || self.received.len() >= QUEUE_CAPACITY
        {
            return Err(EmulatorError::Configuration(
                "Ethernet receive queue full or invalid frame size".into(),
            ));
        }

        self.received.push_back(frame.to_vec());
        self.status |= 2;

        Ok(())
    }

    pub fn pending_frames(&self) -> usize {
        self.received.len()
    }

    pub fn set_link(&mut self, enabled: bool) {
        self.link = enabled && !matches!(self.backend, NetworkBackend::Disconnected);
    }

    pub fn take_transmitted(&mut self) -> Option<Vec<u8>> {
        self.transmitted.pop_front()
    }

    fn poll_host(&mut self) {
        if !self.link {
            return;
        }

        let NetworkBackend::Host(transport) = &mut self.backend else {
            return;
        };
        let mut buffer = [0_u8; MAX_FRAME + 1];

        for _ in 0..QUEUE_CAPACITY {
            match transport.receive(&mut buffer) {
                Ok(length)
                    if (MIN_HOST_FRAME..=MAX_FRAME).contains(&length)
                        && self.received.len() < QUEUE_CAPACITY =>
                {
                    self.received.push_back(buffer[..length].to_vec());
                    self.status |= 2;
                }
                Ok(_) => self.status |= 4,
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.status |= 4;
                    break;
                }
            }
        }
    }

    fn transmit_frame(&mut self) -> bool {
        match &mut self.backend {
            NetworkBackend::Disconnected => false,
            NetworkBackend::Host(transport) => {
                self.outgoing_buffer.len() >= MIN_HOST_FRAME
                    && transport
                        .transmit(&self.outgoing_buffer)
                        .is_ok_and(|written| written == self.outgoing_buffer.len())
            }
            NetworkBackend::InMemory => {
                if self.transmitted.len() >= QUEUE_CAPACITY {
                    return false;
                }

                self.transmitted.push_back(self.outgoing_buffer.clone());
                true
            }
        }
    }
}

impl Device for Ethernet {
    fn activity(&self) -> DeviceActivity {
        DeviceActivity {
            host_errors: false,
            interrupt_lines: &[InterruptLine::MachineExternal],
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn interrupt(&self) -> Option<InterruptLine> {
        (self.status & self.interrupt_mask != 0).then_some(InterruptLine::MachineExternal)
    }

    fn read(&mut self, offset: u32, width: usize) -> Result<u32, EmulatorError> {
        register_width(offset, width, 36)?;
        Ok(match offset {
            0 => u32::from(self.link),
            4 => self.status,
            8 => self.transmit_length as u32,
            12 => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "TX_DATA is write-only".into(),
                })
            }
            16 => 0,
            20 => self.received.front().map_or(0, |frame| frame.len() as u32),
            24 => {
                if let Some(frame) = self.received.front() {
                    let mut value = 0_u32;

                    for index in 0..4 {
                        if let Some(byte) = frame.get(self.read_cursor + index) {
                            value |= (*byte as u32) << (index * 8);
                        }
                    }

                    self.read_cursor = (self.read_cursor + 4).min(frame.len());
                    value
                } else {
                    0
                }
            }
            28 => 0,
            32 => self.interrupt_mask,
            _ => unreachable!(),
        })
    }

    fn write(&mut self, offset: u32, width: usize, value: u32) -> Result<(), EmulatorError> {
        register_width(offset, width, 36)?;

        match offset {
            4 => self.status &= !(value & 5),
            8 => {
                if value == 0 || value as usize > MAX_FRAME {
                    return Err(EmulatorError::Bus {
                        address: offset,
                        explanation: "Ethernet frame length must be 1..1518".into(),
                    });
                }

                self.transmit_length = value as usize;
                self.outgoing_buffer.clear();
            }
            12 => {
                if self.transmit_length == 0 {
                    return Err(EmulatorError::Bus {
                        address: offset,
                        explanation: "configure frame length before writing TX_DATA".into(),
                    });
                }

                for byte in value.to_le_bytes() {
                    if self.outgoing_buffer.len() < self.transmit_length {
                        self.outgoing_buffer.push(byte);
                    }
                }
            }
            16 => {
                if value != 1 {
                    return Err(EmulatorError::Bus {
                        address: offset,
                        explanation: "TX_SEND accepts 1".into(),
                    });
                }

                if self.transmit_length != 0
                    && self.outgoing_buffer.len() == self.transmit_length
                    && self.link
                    && self.transmit_frame()
                {
                    self.status |= 1;
                } else {
                    self.status |= 4;
                }

                self.outgoing_buffer.clear();
                self.transmit_length = 0;
            }
            28 => {
                if value & 1 != 0 {
                    self.received.pop_front();
                    self.read_cursor = 0;

                    if self.received.is_empty() {
                        self.status &= !2;
                    }
                }
            }
            32 => self.interrupt_mask = value & 7,
            _ => {
                return Err(EmulatorError::Bus {
                    address: offset,
                    explanation: "Ethernet register is read-only".into(),
                })
            }
        }

        Ok(())
    }

    fn tick(&mut self, _instructions: u64) {
        self.poll_host();
    }

    fn poll_host_inputs(&mut self) {
        self.poll_host();
    }
}

#[cfg(test)]
#[path = "../../tests/devices/ethernet/state.rs"]
mod tests;
