// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::app_error::Result;
use crate::cli::ExecArgs;
use crate::exec::machine::cheriot::CheriotSystem;
use crate::exec::runtime::AdapterMachine;
use object::{Object, ObjectSection, ObjectSymbol, SymbolKind};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

const SLICE: u64 = 20_000;
const STEP_BUDGET: u64 = 50_000_000;
const REGISTER_NAMES: [&str; 16] = [
    "czero", "cra", "csp", "cgp", "ctp", "ct0", "ct1", "ct2", "cs0", "cs1", "ca0", "ca1", "ca2",
    "ca3", "ca4", "ca5",
];

struct Row {
    address: u32,
    file: usize,
    line: u64,
}

#[derive(Default)]
struct Symbols {
    files: Vec<PathBuf>,
    rows: Vec<Row>,
    functions: Vec<(u32, u32, String)>,
}

impl Symbols {
    fn load(data: &[u8]) -> Self {
        let mut symbols = Self::default();
        let Ok(file) = object::File::parse(data) else {
            return symbols;
        };
        symbols.functions = file
            .symbols()
            .filter(|symbol| symbol.kind() == SymbolKind::Text && symbol.size() > 0)
            .filter_map(|symbol| {
                let start = u32::try_from(symbol.address()).ok()?;
                let end = start.checked_add(u32::try_from(symbol.size()).ok()?)?;
                Some((start, end, symbol.name().ok()?.to_owned()))
            })
            .collect();
        symbols.functions.sort();
        let _ = symbols.load_lines(&file);
        symbols.rows.sort_by_key(|row| row.address);
        symbols
    }

    fn load_lines(&mut self, file: &object::File) -> std::result::Result<(), gimli::Error> {
        let endian = if file.is_little_endian() {
            gimli::RunTimeEndian::Little
        } else {
            gimli::RunTimeEndian::Big
        };
        let load =
            |id: gimli::SectionId| -> std::result::Result<std::borrow::Cow<[u8]>, gimli::Error> {
                Ok(file
                    .section_by_name(id.name())
                    .and_then(|section| section.uncompressed_data().ok())
                    .unwrap_or_default())
            };
        let sections = gimli::DwarfSections::load(load)?;
        let dwarf = sections.borrow(|section| gimli::EndianSlice::new(section, endian));
        let mut indices: BTreeMap<PathBuf, usize> = BTreeMap::new();
        let mut units = dwarf.units();
        while let Some(header) = units.next()? {
            let unit = dwarf.unit(header)?;
            let Some(program) = unit.line_program.clone() else {
                continue;
            };
            let directory = unit
                .comp_dir
                .map(|dir| PathBuf::from(dir.to_string_lossy().into_owned()))
                .unwrap_or_default();
            let mut rows = program.rows();
            while let Some((header, row)) = rows.next_row()? {
                if row.end_sequence() || !row.is_stmt() {
                    continue;
                }
                let (Some(line), Some(entry)) = (row.line(), row.file(header)) else {
                    continue;
                };
                let mut path = directory.clone();
                if let Some(dir) = entry.directory(header)
                    && let Ok(dir) = dwarf.attr_string(&unit, dir)
                {
                    path.push(dir.to_string_lossy().as_ref());
                }
                if let Ok(name) = dwarf.attr_string(&unit, entry.path_name()) {
                    path.push(name.to_string_lossy().as_ref());
                }
                let Ok(address) = u32::try_from(row.address()) else {
                    continue;
                };
                if address == 0 {
                    continue;
                }
                let next = self.files.len();
                let index = *indices.entry(path.clone()).or_insert(next);
                if index == next {
                    self.files.push(path);
                }
                self.rows.push(Row {
                    address,
                    file: index,
                    line: line.get(),
                });
            }
        }
        Ok(())
    }

    fn line(&self, address: u32) -> Option<(&Path, u64)> {
        let index = self.rows.partition_point(|row| row.address <= address);
        let row = self.rows.get(index.checked_sub(1)?)?;
        (address - row.address < 256).then(|| (self.files[row.file].as_path(), row.line))
    }

    fn function(&self, address: u32) -> Option<&(u32, u32, String)> {
        let index = self
            .functions
            .partition_point(|(start, _, _)| *start <= address);
        self.functions
            .get(index.checked_sub(1)?)
            .filter(|(_, end, _)| address < *end)
    }

    fn addresses(&self, source: &Path, line: u64) -> Vec<u32> {
        let matches = |exact: bool| -> Vec<u32> {
            let mut found: Vec<u32> = self
                .rows
                .iter()
                .filter(|row| row.line == line)
                .filter(|row| {
                    let file = &self.files[row.file];
                    if exact {
                        file == source
                    } else {
                        file.file_name() == source.file_name()
                    }
                })
                .map(|row| row.address)
                .collect();
            found.sort_unstable();
            found.dedup();
            found
        };
        let exact = matches(true);
        if exact.is_empty() {
            matches(false)
        } else {
            exact
        }
    }
}

fn read_message(input: &mut impl BufRead) -> Option<Value> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input.read_line(&mut line).ok()? == 0 {
            return None;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length.filter(|length| *length <= 16 << 20)?];
    input.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

struct Session {
    sequence: u64,
    arguments: Option<ExecArgs>,
    config: Option<PathBuf>,
    machine: Option<AdapterMachine>,
    symbols: Symbols,
    breakpoints: BTreeSet<u32>,
    source_breakpoints: BTreeMap<PathBuf, Vec<u32>>,
    instruction_breakpoints: Vec<u32>,
    running: Option<Run>,
    stopped_at: Option<u32>,
    scratch: Vec<u8>,
    finished: bool,
    stop_on_traps: bool,
}

enum Run {
    Continue,
    Line {
        start: Option<(PathBuf, u64)>,
        function: Option<(u32, u32)>,
        over: bool,
        budget: u64,
    },
    Out {
        target: u32,
        budget: u64,
    },
}

impl Session {
    fn send(&mut self, mut message: Value) {
        self.sequence += 1;
        message["seq"] = self.sequence.into();
        let body = message.to_string();
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "Content-Length: {}\r\n\r\n{body}", body.len());
        let _ = out.flush();
    }

    fn event(&mut self, name: &str, body: Value) {
        self.send(json!({"type": "event", "event": name, "body": body}));
    }

    fn respond(&mut self, request: &Value, body: std::result::Result<Value, String>) {
        let mut response = json!({
            "type": "response",
            "request_seq": request["seq"],
            "command": request["command"],
            "success": body.is_ok(),
        });
        match body {
            Ok(body) => response["body"] = body,
            Err(message) => response["message"] = message.into(),
        }
        self.send(response);
    }

    fn system(&mut self) -> std::result::Result<&mut CheriotSystem, String> {
        self.machine
            .as_mut()
            .map(|machine| &mut machine.system)
            .ok_or_else(|| "no firmware has been launched".to_owned())
    }

    fn pc(&mut self) -> u32 {
        self.system()
            .map_or(0, |system| system.computer.processor.program_counter)
    }

    fn register(&mut self, index: u8) -> Option<crate::exec::machine::cheriot::Capability> {
        self.system()
            .ok()?
            .computer
            .processor
            .read_register(index)
            .ok()
    }

    fn drain(&mut self) {
        let Some(machine) = self.machine.as_mut() else {
            return;
        };
        let mut text = String::new();
        let _ = crate::exec::machine::output::drain(
            &mut machine.system.computer.bus,
            &mut machine.system.host_interface,
            &mut self.scratch,
            |_, _, bytes| {
                text.push_str(&String::from_utf8_lossy(bytes));
                Ok(())
            },
        );
        if !text.is_empty() {
            self.event("output", json!({"category": "stdout", "output": text}));
        }
    }

    fn stop(&mut self, reason: &str, description: Option<String>) {
        self.running = None;
        self.stopped_at = Some(self.pc());
        self.drain();
        let mut body = json!({"reason": reason, "threadId": 1, "allThreadsStopped": true});
        if let Some(description) = description {
            body["description"] = description.clone().into();
            body["text"] = description.into();
        }
        self.event("stopped", body);
    }

    fn finish(&mut self, code: u64) {
        self.running = None;
        self.finished = true;
        self.drain();
        self.event("exited", json!({"exitCode": code}));
        self.event("terminated", json!({}));
    }

    fn rebuild_breakpoints(&mut self) {
        self.breakpoints = self
            .source_breakpoints
            .values()
            .flatten()
            .chain(&self.instruction_breakpoints)
            .copied()
            .collect();
    }

    fn advance(&mut self) {
        for _ in 0..SLICE {
            let pc = self.pc();
            let resuming = self.stopped_at.take() == Some(pc);
            if !resuming && self.breakpoints.contains(&pc) {
                return self.stop("breakpoint", None);
            }
            let outcome = match self.system() {
                Ok(system) => system.step().map(|exit| (exit, system.last_outcome)),
                Err(message) => return self.stop("exception", Some(message)),
            };
            let (exit, last) = match outcome {
                Ok(outcome) => outcome,
                Err(error) => return self.stop("exception", Some(error.to_string())),
            };
            if let Some(code) = exit {
                return self.finish(code);
            }
            if let Some(crate::exec::machine::cheriot::StepOutcome::Trapped {
                address, cause, ..
            }) = last
                && cause & 0x8000_0000 == 0
                && (cause == 3 || self.stop_on_traps)
            {
                let kind = if cause == 3 { "ebreak" } else { "trap" };
                return self.stop(
                    "exception",
                    Some(format!("{kind} at {address:#010x} (cause {cause})")),
                );
            }
            let pc = self.pc();
            let done = match self.running.as_mut() {
                None => true,
                Some(Run::Continue) => false,
                Some(Run::Out { target, budget }) => {
                    *budget = budget.saturating_sub(1);
                    pc == *target || *budget == 0
                }
                Some(Run::Line {
                    start,
                    function,
                    over,
                    budget,
                }) => {
                    *budget = budget.saturating_sub(1);
                    let inside = function.is_none_or(|(low, high)| pc >= low && pc < high);
                    let here = self
                        .symbols
                        .line(pc)
                        .map(|(file, line)| (file.to_path_buf(), line));
                    *budget == 0 || (here.is_some() && here != *start && (inside || !*over))
                }
            };
            if done {
                return self.stop("step", None);
            }
        }
        self.drain();
    }

    fn launch(&mut self, arguments: &Value) -> std::result::Result<(), String> {
        let mut exec = self
            .arguments
            .take()
            .ok_or_else(|| "firmware is already launched".to_owned())?;
        if let Some(program) = arguments["program"].as_str() {
            exec.elf = Some(PathBuf::from(program));
            exec.raw = None;
        }
        if let Some(profile) = arguments["profile"].as_str() {
            exec.profile = Some(PathBuf::from(profile));
        }
        if let Some(config) = arguments["config"].as_str() {
            self.config = Some(PathBuf::from(config));
        }
        if arguments["build"].as_bool() == Some(true) {
            exec.build = true;
        }
        let request = crate::exec::machine_request(exec, self.config.as_deref())
            .map_err(|error| error.to_string())?;
        let machine =
            crate::exec::runtime::adapter_machine(request).map_err(|error| error.to_string())?;
        self.symbols = Symbols::load(&machine.firmware);
        self.machine = Some(machine);
        Ok(())
    }

    fn frames(&mut self) -> Vec<Value> {
        let mut addresses = vec![self.pc()];
        if let Some(link) = self.register(1)
            && link.tag()
            && self.symbols.function(link.address()).is_some()
        {
            addresses.push(link.address());
        }
        addresses
            .iter()
            .enumerate()
            .map(|(index, address)| {
                let name = self
                    .symbols
                    .function(*address)
                    .map_or_else(|| format!("{address:#010x}"), |(_, _, name)| name.clone());
                let mut frame = json!({
                    "id": index + 1,
                    "name": name,
                    "line": 0,
                    "column": 0,
                    "instructionPointerReference": format!("{address:#010x}"),
                });
                if let Some((file, line)) = self.symbols.line(*address) {
                    frame["line"] = line.into();
                    frame["source"] = json!({
                        "name": file.file_name().map(|name| name.to_string_lossy()),
                        "path": file.display().to_string(),
                    });
                }
                frame
            })
            .collect()
    }

    fn registers(&mut self) -> Vec<Value> {
        let mut variables = vec![json!({
            "name": "pc",
            "value": format!("{:#010x}", self.pc()),
            "variablesReference": 0,
        })];
        for (index, name) in REGISTER_NAMES.iter().enumerate() {
            let Some(capability) = self.register(index as u8) else {
                continue;
            };
            let (base, top) = capability.bounds();
            variables.push(json!({
                "name": name,
                "value": format!(
                    "{:#010x} (tag={} base={base:#010x} top={top:#x} perms={:#05x}{})",
                    capability.address(),
                    u8::from(capability.tag()),
                    capability.permissions(),
                    if capability.sealed() { " sealed" } else { "" },
                ),
                "memoryReference": format!("{:#010x}", capability.address()),
                "variablesReference": 0,
            }));
        }
        variables
    }

    fn read_memory(&mut self, arguments: &Value) -> std::result::Result<Value, String> {
        let reference = arguments["memoryReference"].as_str().unwrap_or("0");
        let base = u32::from_str_radix(reference.trim_start_matches("0x"), 16)
            .map_err(|_| format!("invalid memory reference {reference}"))?;
        let address = base.wrapping_add(arguments["offset"].as_i64().unwrap_or(0) as u32);
        let count = arguments["count"].as_u64().unwrap_or(0).min(1024) as usize;
        let mut data = vec![0; count];
        let system = self.system()?;
        crate::debug::inspection::inspect_memory(&system.computer.bus, address, &mut data)
            .map_err(|error| error.to_string())?;
        Ok(json!({
            "address": format!("{address:#010x}"),
            "data": base64(&data),
        }))
    }

    fn evaluate(&mut self, expression: &str) -> std::result::Result<Value, String> {
        let name = expression.trim().trim_start_matches('$');
        let result = if name == "pc" {
            format!("{:#010x}", self.pc())
        } else if let Some(index) = REGISTER_NAMES.iter().position(|register| *register == name) {
            let capability = self
                .register(index as u8)
                .ok_or_else(|| "register is unavailable".to_owned())?;
            format!("{:#010x}", capability.address())
        } else {
            return Err(format!(
                "cannot evaluate {expression:?}; use pc or a register name such as csp"
            ));
        };
        Ok(json!({"result": result, "variablesReference": 0}))
    }

    fn handle(&mut self, request: &Value) -> bool {
        let arguments = &request["arguments"];
        let command = request["command"].as_str().unwrap_or_default().to_owned();
        let body: std::result::Result<Value, String> = match command.as_str() {
            "initialize" => Ok(json!({
                "supportsConfigurationDoneRequest": true,
                "supportsInstructionBreakpoints": true,
                "supportsSteppingGranularity": true,
                "supportsReadMemoryRequest": true,
                "supportsEvaluateForHovers": true,
                "supportsTerminateRequest": true,
                "exceptionBreakpointFilters": [{
                    "filter": "trap",
                    "label": "Synchronous traps",
                    "description": "Stop when the guest takes a synchronous trap, before its handler runs",
                    "default": false,
                }],
            })),
            "launch" => self.launch(arguments).map(|()| json!({})),
            "setBreakpoints" => {
                let source = PathBuf::from(arguments["source"]["path"].as_str().unwrap_or(""));
                let mut addresses = Vec::new();
                let breakpoints: Vec<Value> = arguments["breakpoints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|breakpoint| {
                        let line = breakpoint["line"].as_u64().unwrap_or(0);
                        let found = self.symbols.addresses(&source, line);
                        addresses.extend(&found);
                        match found.first() {
                            Some(address) => json!({
                                "verified": true,
                                "line": line,
                                "instructionReference": format!("{address:#010x}"),
                            }),
                            None => json!({
                                "verified": false,
                                "line": line,
                                "message": "no code was generated for this line",
                            }),
                        }
                    })
                    .collect();
                self.source_breakpoints.insert(source, addresses);
                self.rebuild_breakpoints();
                Ok(json!({"breakpoints": breakpoints}))
            }
            "setInstructionBreakpoints" => {
                let mut addresses = Vec::new();
                let breakpoints: Vec<Value> = arguments["breakpoints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|breakpoint| {
                        let reference = breakpoint["instructionReference"].as_str().unwrap_or("");
                        let offset = breakpoint["offset"].as_i64().unwrap_or(0) as u32;
                        match u32::from_str_radix(reference.trim_start_matches("0x"), 16) {
                            Ok(address) => {
                                addresses.push(address.wrapping_add(offset));
                                json!({"verified": true})
                            }
                            Err(_) => json!({"verified": false, "message": "invalid address"}),
                        }
                    })
                    .collect();
                self.instruction_breakpoints = addresses;
                self.rebuild_breakpoints();
                Ok(json!({"breakpoints": breakpoints}))
            }
            "setExceptionBreakpoints" => {
                self.stop_on_traps = arguments["filters"]
                    .as_array()
                    .is_some_and(|filters| filters.iter().any(|filter| filter == "trap"));
                Ok(json!({}))
            }
            "threads" => Ok(json!({"threads": [{"id": 1, "name": "CHERIoT core"}]})),
            "stackTrace" => {
                let frames = self.frames();
                Ok(json!({"totalFrames": frames.len(), "stackFrames": frames}))
            }
            "scopes" => Ok(json!({"scopes": [{
                "name": "Registers",
                "presentationHint": "registers",
                "variablesReference": 1,
                "expensive": false,
            }]})),
            "variables" => Ok(json!({"variables": self.registers()})),
            "readMemory" => self.read_memory(arguments),
            "evaluate" => self.evaluate(arguments["expression"].as_str().unwrap_or("")),
            "pause" => Ok(json!({})),
            "continue" => self.system().map(|_| json!({"allThreadsContinued": true})),
            "next" | "stepIn" | "stepOut" => self.system().map(|_| json!({})),
            "configurationDone" | "disconnect" | "terminate" => Ok(json!({})),
            other => Err(format!("unsupported request: {other}")),
        };
        let succeeded = body.is_ok();
        self.respond(request, body);
        if !succeeded {
            return true;
        }
        match command.as_str() {
            "initialize" => {}
            "launch" => self.event("initialized", json!({})),
            "configurationDone" => {
                if self.machine.is_some() {
                    self.stop("entry", None);
                }
            }
            "pause" => {
                if self.running.is_some() {
                    self.stop("pause", None);
                }
            }
            "continue" => self.running = Some(Run::Continue),
            "next" | "stepIn" => {
                let pc = self.pc();
                let instruction = arguments["granularity"] == "instruction";
                let start = self
                    .symbols
                    .line(pc)
                    .map(|(file, line)| (file.to_path_buf(), line));
                self.running = Some(Run::Line {
                    budget: if instruction || start.is_none() {
                        1
                    } else {
                        STEP_BUDGET
                    },
                    start,
                    function: self
                        .symbols
                        .function(pc)
                        .map(|(low, high, _)| (*low, *high)),
                    over: command == "next",
                });
            }
            "stepOut" => {
                let target = self.register(1).map_or(0, |link| link.address());
                self.running = Some(Run::Out {
                    target,
                    budget: STEP_BUDGET,
                });
            }
            "disconnect" | "terminate" => return false,
            _ => {}
        }
        true
    }
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let bits = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for index in 0..4 {
            if index <= chunk.len() {
                text.push(ALPHABET[(bits >> (18 - 6 * index) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

pub(super) fn serve(arguments: ExecArgs, config: Option<&Path>) -> Result<u8> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        while let Some(message) = read_message(&mut input) {
            if sender.send(message).is_err() {
                break;
            }
        }
    });
    let mut session = Session {
        sequence: 0,
        arguments: Some(arguments),
        config: config.map(Path::to_path_buf),
        machine: None,
        symbols: Symbols::default(),
        breakpoints: BTreeSet::new(),
        source_breakpoints: BTreeMap::new(),
        instruction_breakpoints: Vec::new(),
        running: None,
        stopped_at: None,
        scratch: Vec::new(),
        finished: false,
        stop_on_traps: false,
    };
    loop {
        let request = if session.running.is_some() && !session.finished {
            match receiver.try_recv() {
                Ok(request) => Some(request),
                Err(mpsc::TryRecvError::Empty) => {
                    session.advance();
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => None,
            }
        } else {
            receiver.recv().ok()
        };
        let Some(request) = request else {
            return Ok(0);
        };
        if request["type"] != "request" {
            continue;
        }
        if !session.handle(&request) {
            return Ok(0);
        }
    }
}
