// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{self, Cursor};

use super::*;

#[test]
fn exact_limits_and_empty_input_are_accepted() {
    for length in [0, 1, 4096] {
        let bytes = vec![42; length];
        assert_eq!(read_bounded(&bytes[..], 4096, "test").unwrap(), bytes);
    }
    assert!(read_bounded(io::empty(), 0, "test").unwrap().is_empty());
}

#[test]
fn excess_input_stops_at_one_byte_beyond_the_limit() {
    let mut reader = Cursor::new(vec![42; 8192]);
    assert!(read_bounded(&mut reader, 4096, "test").is_err());
    assert_eq!(reader.position(), 4097);
    assert!(read_bounded(io::repeat(42), 4096, "unending input").is_err());
}

#[test]
fn read_errors_are_preserved() {
    struct FailingReader;
    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("input failed"))
        }
    }
    let error = read_bounded(FailingReader, 100, "test").unwrap_err();
    assert!(error.to_string().contains("input failed"));
}
