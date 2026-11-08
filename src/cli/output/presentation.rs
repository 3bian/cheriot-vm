// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::io::{self, Write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputPresentation {
    Compatibility,
    #[cfg(all(feature = "gdb", target_os = "linux"))]
    DebuggerEscaped,
    Escaped,
    Labelled,
    Raw,
}

impl OutputPresentation {
    #[cfg(all(feature = "gdb", target_os = "linux"))]
    pub(crate) fn debugger(raw: bool) -> Self {
        if raw {
            Self::Raw
        } else {
            Self::DebuggerEscaped
        }
    }

    pub(crate) fn diagnostics_to_stderr(self) -> bool {
        match self {
            Self::Raw => true,
            #[cfg(all(feature = "gdb", target_os = "linux"))]
            Self::DebuggerEscaped => true,
            _ => false,
        }
    }

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
            #[cfg(all(feature = "gdb", target_os = "linux"))]
            Self::DebuggerEscaped => {
                for byte in bytes {
                    let (escaped, length) = cheriot_vm::output::debugger_escape(*byte);
                    writer.write_all(&escaped[..length])?;
                }
                Ok(())
            }
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
