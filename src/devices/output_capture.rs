// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

#[derive(Default)]
pub(super) struct OutputCapture {
    bytes: Vec<u8>,
}

impl OutputCapture {
    const CAPACITY: usize = 65536;

    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    pub fn drain_into(&mut self, output: &mut Vec<u8>) {
        output.extend_from_slice(&self.bytes);
        self.bytes.clear();
    }

    pub fn has_capacity(&self) -> bool {
        self.bytes.len() < Self::CAPACITY
    }

    pub fn push(&mut self, byte: u8) -> bool {
        if !self.has_capacity() {
            return false;
        }
        self.bytes.push(byte);
        true
    }

    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}
