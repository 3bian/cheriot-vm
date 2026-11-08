// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn buffered_output_preserves_bytes_across_full_buffers_and_final_flush() {
    let mut received = Vec::new();
    let mut output = GuestOutput::new(&mut received);
    let bytes = [b'A', b'\n', 0, 0xff];

    for _ in 0..20_000 {
        output.write(&bytes).unwrap();
    }

    output.flush().unwrap();
    drop(output);
    assert_eq!(received, bytes.repeat(20_000));
}

#[test]
fn dedicated_uart_output_keeps_other_kinds_on_their_presented_destination() {
    for (presentation, expected) in [
        (OutputPresentation::Compatibility, b"H\x1b".as_slice()),
        #[cfg(all(feature = "gdb", target_os = "linux"))]
        (OutputPresentation::DebuggerEscaped, b"H\\x1b"),
        (OutputPresentation::Escaped, b"HTIF [tohost]: H\\x1b\n"),
        (OutputPresentation::Labelled, b"HTIF [tohost]: H\x1b\n"),
        (OutputPresentation::Raw, b"H\x1b"),
    ] {
        let path = std::env::temp_dir().join(format!(
            "vm-output-kind-{}-{presentation:?}",
            std::process::id()
        ));
        let binding = crate::cli::uart::output_binding::UartOutputBinding {
            device: "tohost".into(),
            file: std::fs::File::create(&path).unwrap(),
        };
        let mut output = GuestOutput::new(Vec::new()).with_uart_outputs(vec![binding]);
        output
            .present("UART", "tohost", b"U\x1b", presentation)
            .unwrap();
        output
            .present("HTIF", "tohost", b"H\x1b", presentation)
            .unwrap();
        output.flush().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"U\x1b", "{presentation:?}");
        assert_eq!(output.writer.get_ref(), expected, "{presentation:?}");
        drop(output);
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(all(feature = "gdb", target_os = "linux"))]
#[test]
fn debugger_escaping_preserves_printable_bytes_and_encodes_controls() {
    let mut output = GuestOutput::new(Vec::new());
    output
        .present(
            "UART",
            "uart0",
            b"\0\t\n\r\x1b !\"'\\~\x7f\x80\xff",
            OutputPresentation::DebuggerEscaped,
        )
        .unwrap();
    output.flush().unwrap();
    assert_eq!(
        output.writer.get_ref(),
        b"\\x00\t\n\\x0d\\x1b !\"'\\~\\x7f\\x80\\xff"
    );
}

#[test]
fn flush_reports_host_write_errors() {
    struct FailedOutput;

    impl Write for FailedOutput {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut output = GuestOutput::new(FailedOutput);
    output.write(b"pending").unwrap();
    assert_eq!(
        output.flush().unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}

#[test]
fn idle_poll_flushes_unterminated_prompt_after_the_interval() {
    let mut output = GuestOutput::new(Vec::new());
    output.write(b"prompt> ").unwrap();
    output.last_flush = Instant::now() - Duration::from_millis(20);
    output.write(b"").unwrap();
    assert_eq!(output.writer.get_ref(), b"prompt> ");
}

#[test]
fn short_lines_are_batched_until_flush() {
    let mut output = GuestOutput::new(Vec::new());
    output.write(b"one\n").unwrap();
    output.write(b"two\n").unwrap();
    output.flush().unwrap();
    assert_eq!(output.writer.get_ref(), b"one\ntwo\n");
}

#[test]
fn escaped_presentation_neutralizes_guest_controls_and_device_names() {
    let mut output = GuestOutput::new(Vec::new());
    output
        .present(
            "UART",
            "serial\x1b]0;title\x07",
            b"ok\n\x1b[31m\0\xff",
            OutputPresentation::Escaped,
        )
        .unwrap();
    output.flush().unwrap();
    let bytes = output.writer.get_ref();
    assert!(!bytes.contains(&0x1b));
    assert!(!bytes.contains(&0x07));
    assert!(!bytes.contains(&0));
    assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 1);
    let text = std::str::from_utf8(bytes).unwrap();
    assert!(text.contains("ok\\n\\x1b[31m\\x00\\xff"));
}

#[test]
fn raw_presentation_preserves_arbitrary_bytes_without_labels() {
    let mut output = GuestOutput::new(Vec::new());
    output
        .present(
            "UART",
            "name",
            &[0, 0xff, b'\n', 0x1b],
            OutputPresentation::Raw,
        )
        .unwrap();
    output.flush().unwrap();
    assert_eq!(output.writer.get_ref(), &[0, 0xff, b'\n', 0x1b]);
}
