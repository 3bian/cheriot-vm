// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::exec::machine::EmulatorError;
use crate::exec::machine::ExceptionCause;
use crate::exec::machine::MachineConfiguration;
use crate::exec::machine::execution::TrapPolicy;
use crate::exec::machine::firmware::load_elf;

#[test]
#[ignore = "opt-in: set RISCV_TEST_BINARIES to a directory of compiled riscv-tests"]
fn upstream_unprivileged_tests() {
    let directory = std::env::var("RISCV_TEST_BINARIES").expect("RISCV_TEST_BINARIES is required");
    let mut paths: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "elf"))
        .collect();
    paths.sort();
    assert_eq!(
        paths.len(),
        50,
        "the pinned suite must contain all 50 selected programs"
    );

    for path in &paths {
        let mut computer = MachineConfiguration::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/profiles/rv32imc.toml"
        )))
        .unwrap()
        .build(0x8000_0000)
        .unwrap();
        computer.trap_policy = TrapPolicy::Stop;
        computer.processor.program_counter = load_elf(
            &mut computer.bus,
            &std::fs::read(path).unwrap(),
            computer.processor.profile,
        )
        .unwrap();
        let mut completed = false;

        for _ in 0..1_000_000 {
            match computer.step() {
                Ok(_) => (),
                Err(EmulatorError::Trap {
                    cause: ExceptionCause::Breakpoint,
                    ..
                }) => {
                    assert_eq!(
                        computer.processor.read_register(10).unwrap(),
                        0,
                        "{} failed upstream case {}",
                        path.display(),
                        computer.processor.read_register(3).unwrap()
                    );
                    completed = true;
                    break;
                }
                Err(error) => panic!("{}: {error}", path.display()),
            }
        }

        assert!(completed, "{} timed out", path.display());
        println!(
            "PASS {} ({} retired)",
            path.file_name().unwrap().to_string_lossy(),
            computer.processor.machine_registers.retired
        );
    }

    println!("{} upstream programs passed", paths.len());
}
