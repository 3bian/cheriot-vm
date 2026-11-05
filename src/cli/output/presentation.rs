// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{self, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputPresentation {
    Compatibility,
    Escaped,
    Labelled,
    Raw,
}

impl OutputPresentation {
    pub(crate) fn present(
        self,
        writer: &mut impl Write,
        kind: &str,
        name: &str,
        bytes: &[u8],
    ) -> io::Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }
        match self {
            Self::Raw | Self::Compatibility => writer.write_all(bytes),
            Self::Labelled => writeln!(
                writer,
                "{kind} [{name}]: {}",
                String::from_utf8_lossy(bytes)
            ),
            Self::Escaped => {
                write!(writer, "{kind} [{}]: ", name.escape_default())?;
                for byte in bytes {
                    for escaped in std::ascii::escape_default(*byte) {
                        writer.write_all(&[escaped])?;
                    }
                }
                writeln!(writer)
            }
        }
    }
}
