// Copyright 3bian Limited and Contributors
// SPDX-License-Identifier: MIT

use cheriot_vm::cheriot::Capability;

fn compare_vectors(vectors: &str) {
    let mut cases = 0;
    let mut checks = 0;
    let mut current = None;
    let mut seen = Vec::new();

    for line in vectors.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();

        match fields.first().copied() {
            Some("CASE") => {
                assert!(current.is_none() || seen.len() == 7, "incomplete case");
                assert_eq!(fields.len(), 5);
                let index: usize = fields[1].parse().unwrap();
                assert_eq!(index, cases);
                let bits = fields[2].parse().unwrap();
                let tag = match fields[3] {
                    "0" => false,
                    "1" => true,
                    other => panic!("invalid tag {other}"),
                };
                let operand: u32 = fields[4].parse().unwrap();
                current = Some((Capability::from_memory(bits, tag), operand));
                cases += 1;
                seen.clear();
            }
            Some("CAP") => {
                assert_eq!(fields.len(), 8);
                let (input, operand) = current.expect("CASE must precede CAP");
                let label = fields[1];
                assert!(!seen.contains(&label), "duplicate {label}");
                seen.push(label);
                let index = cases - 1;
                let mut authority_bits = 0x4e3e_0000_0000_0000 | (index % 18) as u64;

                if index % 2 != 0 {
                    authority_bits &= !(1 << 62);
                }

                let authority = Capability::from_memory(authority_bits, true);
                let actual = match label {
                    "decode" => input,
                    "address" => input.set_address(operand),
                    "bounds" => input.set_bounds(operand, false).0,
                    "exact" => input.set_bounds(operand, true).0,
                    "permissions" => input.restrict_permissions(operand as u16),
                    "seal" => input.seal(authority),
                    "unseal" => input.seal(authority).unseal(authority),
                    _ => panic!("unexpected operation {label}"),
                };
                let expected: Vec<u64> = fields[2..].iter().map(|n| n.parse().unwrap()).collect();
                let (bits, tag) = actual.to_memory();
                let (base, top) = actual.bounds();
                assert_eq!(
                    [
                        bits,
                        u64::from(tag),
                        u64::from(base),
                        top,
                        u64::from(actual.permissions()),
                        u64::from(actual.object_type())
                    ],
                    expected.as_slice(),
                    "case {index} {label}, input={input:?}, operand={operand:#x}"
                );
                checks += 1;
            }
            _ => panic!("unexpected reference output: {line}"),
        }
    }

    assert_eq!(cases, 3072);
    assert_eq!(seen.len(), 7);
    assert_eq!(checks, 21504);
    println!("Compared {cases} inputs / {checks} reference capability results, six fields each");
}

#[test]
fn pinned_capability_vectors() {
    compare_vectors(include_str!("../vectors/capability-vectors.txt"));
}

#[test]
#[ignore = "requires regenerated vectors; run ../verification/tools/compare-with-sail.sh"]
fn regenerated_capability_vectors() {
    compare_vectors(&crate::reference_input::read_reference_text(
        "SAIL_CAPABILITY_VECTORS",
    ));
}
