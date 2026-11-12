// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use object::{Object, ObjectSection, ObjectSymbol, SectionKind, SymbolKind};
use serde::Serialize;

struct Region {
    name: String,
    start: u32,
    end: u32,
}

pub(crate) struct Insights {
    base: u32,
    counts: Vec<u64>,
    code: Vec<Region>,
    functions: Vec<Region>,
    stacks: Vec<Region>,
    lowest: Vec<u32>,
    not_stack_users: Vec<(u32, u32)>,
    outside: u64,
}

#[derive(Serialize)]
pub(crate) struct CodeReport {
    pub name: String,
    pub instructions: u64,
    pub percent: f64,
    pub covered_bytes: u64,
    pub size: u64,
    pub coverage_percent: f64,
}

#[derive(Serialize)]
pub(crate) struct FunctionReport {
    pub name: String,
    pub instructions: u64,
    pub percent: f64,
}

#[derive(Serialize)]
pub(crate) struct StackReport {
    pub name: String,
    pub size: u64,
    pub peak_bytes: u64,
    pub peak_percent: f64,
}

#[derive(Serialize)]
pub(crate) struct Report {
    pub instructions: u64,
    pub outside_code: u64,
    pub compartments: Vec<CodeReport>,
    pub functions: Vec<FunctionReport>,
    pub stacks: Vec<StackReport>,
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        (part as f64 * 1000.0 / whole as f64).round() / 10.0
    }
}

fn display_name(section: &str) -> String {
    section
        .trim_start_matches('.')
        .trim_end_matches("_code")
        .to_owned()
}

impl Insights {
    pub(crate) fn from_elf(data: &[u8]) -> Option<Self> {
        let file = object::File::parse(data).ok()?;
        let region = |name: &str, start: u64, size: u64| {
            let start = u32::try_from(start).ok()?;
            let end = start.checked_add(u32::try_from(size).ok()?)?;
            (end > start).then(|| Region {
                name: name.to_owned(),
                start,
                end,
            })
        };
        let mut code = Vec::new();
        let mut stacks = Vec::new();
        for section in file.sections() {
            let name = section.name().unwrap_or("");
            if section.kind() == SectionKind::Text {
                code.extend(region(
                    &display_name(name),
                    section.address(),
                    section.size(),
                ));
            } else if (name.ends_with("_stack") || name.contains("_stack_"))
                && !name.contains("trusted")
            {
                stacks.extend(region(
                    name.trim_start_matches('.'),
                    section.address(),
                    section.size(),
                ));
            }
        }
        code.sort_by_key(|region| region.start);
        let base = code.first()?.start;
        let end = code.iter().map(|region| region.end).max()?;
        let mut functions: Vec<Region> = file
            .symbols()
            .filter(|symbol| symbol.kind() == SymbolKind::Text && symbol.size() > 0)
            .filter_map(|symbol| region(symbol.name().ok()?, symbol.address(), symbol.size()))
            .collect();
        functions.sort_by_key(|region| region.start);
        functions.dedup_by_key(|region| region.start);
        let lowest = stacks.iter().map(|stack| stack.end).collect();
        let not_stack_users = code
            .iter()
            .filter(|region| {
                region.name.starts_with("loader") || region.name == "compartment_switcher"
            })
            .map(|region| (region.start, region.end))
            .collect();
        Some(Self {
            base,
            counts: vec![0; ((end - base) as usize).div_ceil(2)],
            code,
            functions,
            stacks,
            lowest,
            not_stack_users,
            outside: 0,
        })
    }

    #[inline]
    pub(crate) fn record(&mut self, pc: u32, sp: u32) {
        match self
            .counts
            .get_mut((pc.wrapping_sub(self.base) >> 1) as usize)
        {
            Some(count) if pc >= self.base => *count += 1,
            _ => self.outside += 1,
        }
        if self
            .not_stack_users
            .iter()
            .any(|(start, end)| pc >= *start && pc < *end)
        {
            return;
        }
        for (stack, lowest) in self.stacks.iter().zip(&mut self.lowest) {
            if sp >= stack.start && sp < *lowest {
                *lowest = sp;
            }
        }
    }

    fn slots(&self, region: &Region) -> &[u64] {
        let start = ((region.start.saturating_sub(self.base)) >> 1) as usize;
        let end = (((region.end.saturating_sub(self.base)) + 1) >> 1) as usize;
        &self.counts[start.min(self.counts.len())..end.min(self.counts.len())]
    }

    pub(crate) fn report(&self) -> Report {
        let inside: u64 = self.counts.iter().sum();
        let total = inside + self.outside;
        let mut compartments: Vec<CodeReport> = self
            .code
            .iter()
            .map(|region| {
                let slots = self.slots(region);
                let instructions: u64 = slots.iter().sum();
                let mut covered = 0_u64;
                let mut carry = false;
                for (index, count) in slots.iter().enumerate() {
                    if *count > 0 {
                        covered += 2;
                        carry = slots.get(index + 1).is_some_and(|next| *next == 0);
                    } else if carry {
                        covered += 2;
                        carry = false;
                    }
                }
                let size = u64::from(region.end - region.start);
                let covered = covered.min(size);
                CodeReport {
                    name: region.name.clone(),
                    instructions,
                    percent: percent(instructions, total),
                    covered_bytes: covered,
                    size,
                    coverage_percent: percent(covered, size),
                }
            })
            .collect();
        compartments.sort_by_key(|c| std::cmp::Reverse(c.instructions));
        let mut functions: Vec<FunctionReport> = self
            .functions
            .iter()
            .map(|region| {
                let instructions: u64 = self.slots(region).iter().sum();
                FunctionReport {
                    name: region.name.clone(),
                    instructions,
                    percent: percent(instructions, total),
                }
            })
            .filter(|function| function.instructions > 0)
            .collect();
        functions.sort_by_key(|f| std::cmp::Reverse(f.instructions));
        let stacks = self
            .stacks
            .iter()
            .zip(&self.lowest)
            .map(|(stack, lowest)| {
                let size = u64::from(stack.end - stack.start);
                let peak = u64::from(stack.end - lowest);
                StackReport {
                    name: stack.name.clone(),
                    size,
                    peak_bytes: peak,
                    peak_percent: percent(peak, size),
                }
            })
            .collect();
        Report {
            instructions: total,
            outside_code: self.outside,
            compartments,
            functions,
            stacks,
        }
    }
}

pub(crate) fn text(report: &Report) -> String {
    use crate::output;
    let mut text = String::new();
    output::heading(&mut text, "Execution");
    output::fields(
        &mut text,
        &[
            ("Instructions", report.instructions.to_string()),
            ("Outside firmware code", report.outside_code.to_string()),
        ],
    );
    let rows: Vec<[String; 5]> = report
        .compartments
        .iter()
        .map(|c| {
            [
                c.name.clone(),
                c.instructions.to_string(),
                format!("{:.1}%", c.percent),
                format!("{:.1}%", c.coverage_percent),
                format!("{}/{}", c.covered_bytes, c.size),
            ]
        })
        .collect();
    output::heading(&mut text, "Compartments and libraries");
    output::table(
        &mut text,
        &["NAME", "INSTRUCTIONS", "SHARE", "COVERAGE", "BYTES"],
        &rows,
    );
    if !report.functions.is_empty() {
        let room = output::WIDTH - "  INSTRUCTIONS  SHARE   ".len();
        let hottest = || report.functions.iter().take(15);
        let rows: Vec<[String; 3]> = hottest()
            .map(|f| {
                [
                    f.instructions.to_string(),
                    format!("{:.1}%", f.percent),
                    output::clip(&f.name, room),
                ]
            })
            .collect();
        output::heading(&mut text, "Hottest functions");
        output::table(&mut text, &["INSTRUCTIONS", "SHARE", "FUNCTION"], &rows);
        if hottest().any(|f| f.name.chars().count() > room) {
            text.push('\n');
            output::lines(
                &mut text,
                ["Long names are cut short here; --stats-output FILE writes them in full."],
            );
        }
    }
    if !report.stacks.is_empty() {
        let rows: Vec<[String; 4]> = report
            .stacks
            .iter()
            .map(|s| {
                [
                    s.name.clone(),
                    s.size.to_string(),
                    s.peak_bytes.to_string(),
                    format!("{:.1}%", s.peak_percent),
                ]
            })
            .collect();
        output::heading(&mut text, "Stack high-water marks");
        output::table(&mut text, &["STACK", "SIZE", "PEAK", "USED"], &rows);
    }
    text.trim_end().to_owned()
}
