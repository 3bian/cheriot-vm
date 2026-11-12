// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

const CONFIG_HELP: &str = "Project configuration file. Defaults to the CHERIOT_CONFIG environment variable, else cheriot.toml";
const COLOR_HELP: &str = "Colour text output: auto (only on a terminal, and not when NO_COLOR is set), always or never. Defaults to the global color setting, else auto";
const JOBS_HELP: &str = "Number of parallel build jobs. Defaults to the CHERIOT_JOBS environment variable, else the global jobs setting, else 1";
const BOARD_HELP: &str = "Board to build for: a [boards.NAME] table in cheriot.toml, NAME.toml in the SDK's boards directory, or a board file. Defaults to the CHERIOT_BOARD environment variable, else the board property in cheriot.toml";
const BUILD_DIR_HELP: &str = "Directory for intermediate build files. Defaults to the CHERIOT_BUILD_DIR environment variable, else the build_dir property in cheriot.toml, else .cheriot in the project directory";
const MODE_HELP: &str = "Build mode. Defaults to the CHERIOT_MODE environment variable, else the mode property in cheriot.toml, else release";
const OUTPUT_DIR_HELP: &str = "Directory for the published firmware. Defaults to the CHERIOT_OUTPUT_DIR environment variable, else the output_dir property in cheriot.toml, else target/BOARD/MODE in the project directory";
const PROJECT_HELP: &str =
    "Project to select, when the configuration file is a workspace of several";
const FLASH_TARGET_HELP: &str = "Board to write to: a [targets.NAME] table in cheriot.toml, a target file, or NAME.toml in a targets directory. Defaults to the project's target setting";
const TARGET_HELP: &str = "Where to run: a [targets.NAME] table in cheriot.toml, a target file, or NAME.toml in a targets directory. Defaults to the project's target setting, else the built-in emulator";
const TOOLCHAIN_DIR_HELP: &str = "Directory containing the CHERIoT LLVM tools. Defaults to the CHERIOT_TOOLCHAIN_DIR environment variable, else the toolchain_dir property in cheriot.toml, else the global toolchain_dir setting, else the first directory on PATH that holds them";

#[derive(Debug, Parser)]
#[command(
    name = "cheriot",
    version,
    disable_help_subcommand = true,
    propagate_version = true,
    about = "CHERIoT firmware development tools"
)]
pub struct Cli {
    #[arg(long, global = true, value_name = "FILE", help = CONFIG_HELP)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true, value_enum, value_name = "WHEN", help = COLOR_HELP)]
    pub color: Option<Color>,
    #[command(subcommand)]
    pub command: Command,
}
#[derive(Clone, Debug, ValueEnum)]
pub enum Color {
    Auto,
    Always,
    Never,
}
#[derive(Clone, Debug, ValueEnum)]
pub enum Mode {
    Debug,
    Release,
}
#[derive(Clone, Debug, ValueEnum)]
pub enum Format {
    Text,
    Json,
    Toml,
}
#[derive(Clone, Debug, ValueEnum)]
pub enum SbomFormat {
    Text,
    Json,
    Toml,
    Cyclonedx,
    Spdx,
}
#[derive(Clone, Debug, ValueEnum)]
pub enum PlanFormat {
    Text,
    Json,
    Toml,
    Shell,
    CompilationDatabase,
}
#[derive(Clone, Debug, ValueEnum)]
pub enum SizeFormat {
    Text,
    Json,
    Toml,
    Bytes,
}
#[derive(Debug, Subcommand)]
pub enum Command {
    #[command(about = "Build the selected project's firmware")]
    Build(BuildArgs),
    #[command(about = "Remove the files a build of the selected project wrote")]
    Clean(CleanArgs),
    #[command(about = "Inspect or modify a project's configuration, or your own global settings")]
    Config(ConfigArgs),
    #[command(
        about = "Debug firmware on a target; the built-in emulator and a GDB server by default"
    )]
    Debug(DebugArgs),
    #[command(about = "Check the configuration and the CHERIoT toolchain, and say what is wrong")]
    Diagnose(DiagnoseArgs),
    #[command(about = "Execute firmware on a target; the built-in emulator by default")]
    Exec(ExecArgs),
    #[command(about = "Build and package firmware, then write it to a board")]
    Flash(FlashArgs),
    #[command(about = "Create a new project from a template folder")]
    Init(InitArgs),
    #[command(about = "Report what an ELF file contains, without running it")]
    Inspect(InspectArgs),
    #[command(about = "Build firmware and package it as the target or the board requires")]
    Package(PackageArgs),
    #[command(about = "Show the commands a build would run, without running them")]
    Plan(PlanArgs),
    #[command(about = "Build firmware and report its section sizes")]
    Size(SizeArgs),
    #[command(about = "Build, execute and check firmware against the project's test settings")]
    Test(TestArgs),
}
#[derive(Debug, Clone, Args, Default)]
pub struct Selection {
    #[arg(long, value_name = "NAME|FILE", help = BOARD_HELP)]
    pub board: Option<String>,
    #[arg(long, value_name = "DIR", help = BUILD_DIR_HELP)]
    pub build_dir: Option<PathBuf>,
    #[arg(
        long,
        value_name = "NAME",
        help = "Firmware to select when the project defines several"
    )]
    pub firmware: Option<String>,
    #[arg(skip)]
    pub jobs: Option<u32>,
    #[arg(long, value_enum, help = MODE_HELP)]
    pub mode: Option<Mode>,
    #[arg(long, value_name = "DIR", help = OUTPUT_DIR_HELP)]
    pub output_dir: Option<PathBuf>,
    #[arg(
        long,
        value_name = "FILE",
        help = "Configuration overlay to apply; repeat to add more"
    )]
    pub overlay: Vec<PathBuf>,
    #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
    pub project: Option<String>,
    #[arg(long, value_name = "DIR", help = TOOLCHAIN_DIR_HELP)]
    pub toolchain_dir: Option<PathBuf>,
}
impl Selection {
    pub fn with_jobs(mut self, jobs: Option<u32>) -> Self {
        self.jobs = jobs;
        self
    }
}
#[derive(Debug, Args)]
pub struct BuildArgs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long, help = "Report the work without running any tool")]
    pub dry_run: bool,
    #[arg(long, hide = true, value_name = "FILE")]
    pub execute_plan: Option<PathBuf>,
    #[arg(
        long,
        value_enum,
        default_value = "text",
        help = "Output format; json and toml report each step instead of showing progress"
    )]
    pub format: Format,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=256), help = JOBS_HELP)]
    pub jobs: Option<u32>,
    #[arg(long, conflicts_with = "verbose", help = "Print errors only")]
    pub quiet: bool,
    #[arg(long, help = "Print each command as it runs")]
    pub verbose: bool,
}
#[derive(Debug, Args)]
pub struct CleanArgs {
    #[command(flatten)]
    pub selection: Selection,
}
#[derive(Debug, Args)]
pub struct InspectArgs {
    #[arg(value_name = "ELF", help = "ELF file to inspect")]
    pub file: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "text", help = "Output format")]
    pub format: Format,
    #[arg(long, value_name = "FILE", help = "Write the report to a file")]
    pub output: Option<PathBuf>,
    #[arg(long, help = "List sections")]
    pub sections: bool,
    #[arg(long, help = "List symbols")]
    pub symbols: bool,
}
#[derive(Debug, Args)]
pub struct PackageArgs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long, value_enum, default_value = "text", help = "Output format")]
    pub format: Format,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=256), help = JOBS_HELP)]
    pub jobs: Option<u32>,
    #[arg(
        long,
        value_name = "NAME|FILE",
        help = "Target whose [package] table says how to package: a [targets.NAME] table in cheriot.toml, a target file, or NAME.toml in a targets directory. Defaults to the project's target setting, else the UF2 layout of the board"
    )]
    pub target: Option<String>,
}
#[derive(Debug, Args)]
pub struct PlanArgs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(
        long,
        value_enum,
        default_value = "text",
        help = "Output format; shell is a script that runs the plan, and compilation-database is compile_commands.json for editors"
    )]
    pub format: PlanFormat,
    #[arg(long, value_name = "FILE", help = "Write the plan to a file")]
    pub output: Option<PathBuf>,
    #[arg(long, help = "Run the compiler feature probes the plan depends on")]
    pub probe_toolchain: bool,
}
#[derive(Debug, Args)]
pub struct SizeArgs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long, value_name = "NAME", help = "Report one compartment only")]
    pub compartment: Option<String>,
    #[arg(
        long,
        value_enum,
        default_value = "text",
        help = "Output format; bytes prints the total alone"
    )]
    pub format: SizeFormat,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=256), help = JOBS_HELP)]
    pub jobs: Option<u32>,
    #[arg(long, value_name = "FILE", help = "Write the report to a file")]
    pub output: Option<PathBuf>,
}
#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[arg(
        long,
        global = true,
        help = "Act on your own settings, which apply to every project, instead of a project's. Works with check, get, inspect, ls, reset and set"
    )]
    pub global: bool,
    #[command(subcommand)]
    pub command: Option<ConfigCommand>,
}
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Validate a project without writing or running the compiler")]
    Check {
        #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
        project: Option<String>,
        #[arg(
            long,
            value_name = "NAME|FILE",
            help = "Also validate a machine profile"
        )]
        profile: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "text", help = "Output format")]
        format: Format,
    },
    #[command(about = "Print one value: as written in the file, else the resolved default")]
    Get {
        #[arg(value_name = "PATH", help = "Path such as /firmwares/hello/threads")]
        key: String,
        #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
        project: Option<String>,
        #[arg(long, value_enum, default_value = "text", help = "Output format")]
        format: Format,
    },
    #[command(
        about = "Show the resolved configuration, where each value came from, and its components"
    )]
    Inspect {
        #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
        project: Option<String>,
        #[arg(long, value_enum, default_value = "text", help = "Output format")]
        format: Format,
    },
    #[command(about = "List the keys at PATH; tables and arrays end with `/`")]
    Ls {
        #[arg(
            value_name = "PATH",
            help = "Path such as /firmwares/hello; defaults to the top level. Array items are addressed by name or index"
        )]
        key: Option<String>,
        #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
        project: Option<String>,
        #[arg(long, value_enum, default_value = "text", help = "Output format")]
        format: Format,
    },
    #[command(about = "Remove values from the configuration file so that defaults apply")]
    Reset {
        #[arg(required = true, value_name = "PATH", help = "Paths to remove")]
        keys: Vec<String>,
        #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
        project: Option<String>,
    },
    #[command(about = "Report the components the project is made of, from its metadata tables")]
    Sbom {
        #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
        project: Option<String>,
        #[arg(
            long,
            value_enum,
            default_value = "text",
            help = "Output format; cyclonedx is CycloneDX 1.5 JSON and spdx is SPDX 2.3 JSON, both with file hashes"
        )]
        format: SbomFormat,
        #[arg(long, value_name = "FILE", help = "Write the report to a file")]
        output: Option<PathBuf>,
    },
    #[command(about = "Set values in the configuration file, validating before writing")]
    Set {
        #[arg(
            required = true,
            value_name = "PATH=VALUE",
            help = "Assignments such as mode=debug"
        )]
        assignments: Vec<String>,
        #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
        project: Option<String>,
    },
}
#[derive(Debug, Args)]
pub struct ExecArgs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(
        value_name = "ELF",
        conflicts_with = "raw",
        help = "Firmware ELF; defaults to the selected project's built output"
    )]
    pub elf: Option<PathBuf>,
    #[arg(
        long,
        value_name = "FILE",
        requires = "address",
        conflicts_with = "elf",
        help = "Raw firmware image; needs --address"
    )]
    pub raw: Option<PathBuf>,
    #[arg(long, requires = "raw", help = "Load address of the raw image")]
    pub address: Option<String>,
    #[arg(
        long,
        conflicts_with = "raw",
        help = "Build the selected project first; up-to-date outputs are not rebuilt"
    )]
    pub build: bool,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=256), requires = "build", help = JOBS_HELP)]
    pub jobs: Option<u32>,
    #[arg(long, value_name = "NAME|FILE", help = TARGET_HELP)]
    pub target: Option<String>,
    #[arg(
        long,
        value_name = "NAME|FILE",
        help = "Machine profile: a [profiles.NAME] table in cheriot.toml, or a profile file. Optional when the project defines exactly one profile"
    )]
    pub profile: Option<PathBuf>,
    #[arg(
        long,
        value_name = "ADDRESS",
        help = "Stop at an address; repeat to add more breakpoints"
    )]
    pub breakpoint: Vec<String>,
    #[arg(
        long,
        value_name = "FILE",
        help = "Write a machine checkpoint when the run ends"
    )]
    pub checkpoint: Option<PathBuf>,
    #[arg(
        long,
        value_name = "disabled|elf-symbol|ADDRESS",
        help = "How the guest reports completion"
    )]
    pub completion: Option<String>,
    #[arg(long, value_parser = ["raw", "labelled", "escaped"], help = "How guest console output is shown: raw bytes, prefixed with the UART name, or with control characters escaped")]
    pub console_format: Option<String>,
    #[arg(
        long,
        value_name = "path=CHIP,lines=N+N[,KEY=VALUE]...",
        help = "Bind a GPIO device to host GPIO lines, through the Linux GPIO character device. Example: path=/dev/gpiochip0,lines=17+27\nOptional keys: device=NAME (profile device), active-low=true|false, bias=as-is|disabled|pull-down|pull-up, drive=push-pull|open-drain|open-source"
    )]
    pub gpio: Vec<String>,
    #[arg(
        long,
        value_name = "path=BUS[,KEY=VALUE]...",
        help = "Bind an I2C device to a host bus, through Linux i2c-dev. Example: path=/dev/i2c-1,addresses=0x48+0x49\nOptional keys: device=NAME (profile device), addresses=A+A (host addresses to allow)"
    )]
    pub i2c: Vec<String>,
    #[arg(
        long,
        value_name = "address=ADDRESS[,length=N]",
        help = "Dump memory when the run ends. Example: address=0x80000000,length=64\nlength is 1 to 256 bytes and defaults to 16"
    )]
    pub memory: Option<String>,
    #[arg(
        long,
        value_name = "NAME+NAME...",
        help = "Print registers when the run ends. Example: pc+x1+x2\nNames: x0..x31, pc, and on CHERIoT c0..c15, pcc, mtcc, mtdc, mscratchc, mepcc"
    )]
    pub registers: Option<String>,
    #[arg(
        long,
        value_name = "path=BUS[,KEY=VALUE]...",
        help = "Bind an SPI device to a host bus, through Linux spidev. Example: path=/dev/spidev0.0,speed=1000000\nOptional keys: device=NAME (profile device), select=0..3, speed=HZ (default 500000)"
    )]
    pub spi: Vec<String>,
    #[arg(
        long,
        help = "Report instructions and code coverage per compartment, the hottest functions, and stack high-water marks"
    )]
    pub stats: bool,
    #[arg(
        long,
        value_name = "FILE",
        help = "Write the --stats report to a file as JSON instead of printing it"
    )]
    pub stats_output: Option<PathBuf>,
    #[arg(long, value_name = "N", help = "Stop after N steps; 0 means no limit")]
    pub steps: Option<u64>,
    #[arg(
        long,
        value_name = "interface=NAME[,device=NAME]",
        help = "Bind an Ethernet device to a host TAP interface, which Linux provides. Example: interface=tap0\nOptional key: device=NAME (profile device)"
    )]
    pub tap: Vec<String>,
    #[arg(long, help = "Print every executed instruction")]
    pub trace: bool,
    #[arg(long, value_parser = ["stop", "vector"], help = "Stop on a trap, or vector to the guest handler")]
    pub trap_policy: Option<String>,
    #[arg(
        long,
        value_name = "in=PATH|out=PATH[,KEY=VALUE]...",
        help = "Bind a UART to a host file, pipe or TTY. Example: out=/dev/stdout or in=/dev/ttyUSB0,out=/dev/ttyUSB0,baud=115200\nOptional keys: device=NAME (profile device), baud=N, data-bits=N, stop-bits=N, parity=none|even|odd, flow=none|rtscts|xonxoff"
    )]
    pub uart: Vec<String>,
}
#[derive(Debug, Args)]
pub struct DebugArgs {
    #[command(flatten)]
    pub exec: ExecArgs,
    #[arg(
        long,
        conflicts_with_all = ["gdb"],
        help = "Serve the Debug Adapter Protocol on standard input and output, for editors such as VS Code"
    )]
    pub dap: bool,
    #[arg(
        long,
        value_name = "FILE",
        help = "GDB client to launch against the server"
    )]
    pub gdb: Option<PathBuf>,
    #[arg(
        long,
        value_name = "ADDRESS",
        default_value = "127.0.0.1",
        help = "Loopback address to listen on"
    )]
    pub host: String,
    #[arg(long, default_value_t = 3333, help = "TCP port to listen on")]
    pub port: u16,
}
#[derive(Debug, Args)]
pub struct DiagnoseArgs {
    #[arg(long, value_enum, default_value = "text", help = "Output format")]
    pub format: Format,
    #[arg(long, value_name = "NAME", help = PROJECT_HELP)]
    pub project: Option<String>,
    #[arg(long, value_name = "DIR", help = TOOLCHAIN_DIR_HELP)]
    pub toolchain_dir: Option<PathBuf>,
}
#[derive(Debug, Args)]
pub struct FlashArgs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long, value_name = "NAME|FILE", help = FLASH_TARGET_HELP)]
    pub target: Option<String>,
    #[arg(
        long,
        value_name = "DIR",
        help = "Directory to write the package to, instead of the one the target's [flash] table names; target commands see it as ${destination}"
    )]
    pub to: Option<String>,
    #[arg(
        long,
        help = "Package and report what would be written without writing it"
    )]
    pub dry_run: bool,
    #[arg(long, value_enum, default_value = "text", help = "Output format")]
    pub format: Format,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=256), help = JOBS_HELP)]
    pub jobs: Option<u32>,
}
#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(
        value_name = "DIR",
        help = "Directory to create the project in; defaults to the current directory"
    )]
    pub directory: Option<PathBuf>,
    #[arg(
        long,
        value_name = "NAME",
        help = "Project name; defaults to the directory name"
    )]
    pub name: Option<String>,
    #[arg(
        long,
        value_name = "DIR|URL",
        help = "Template folder or Git repository to copy. Defaults to the CHERIOT_TEMPLATE environment variable, else the global template setting, else the template folder installed with cheriot"
    )]
    pub template: Option<String>,
    #[arg(
        long,
        value_name = "DIR",
        default_value = "cheriot-rtos/sdk",
        help = "Path to the CHERIoT RTOS SDK, written into the project"
    )]
    pub sdk: String,
    #[arg(
        long,
        value_name = "NAME",
        default_value = "sail",
        help = "Board to start with"
    )]
    pub board: String,
    #[arg(long, help = "Overwrite files that already exist")]
    pub force: bool,
}
#[derive(Debug, Args)]
pub struct TestArgs {
    #[command(flatten)]
    pub selection: Selection,
    #[arg(long, value_name = "NAME|FILE", help = TARGET_HELP)]
    pub target: Option<String>,
    #[arg(
        long,
        value_name = "NAME",
        help = "Run only the named test case; repeat to add more"
    )]
    pub case: Vec<String>,
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..=256), help = JOBS_HELP)]
    pub jobs: Option<u32>,
    #[arg(
        long,
        help = "Use the firmware already built instead of building first"
    )]
    pub no_build: bool,
    #[arg(long, help = "Do not echo the firmware's output while it runs")]
    pub quiet: bool,
    #[arg(long, value_enum, default_value = "text", help = "Output format")]
    pub format: Format,
    #[arg(
        long,
        value_name = "FILE",
        help = "Also write the results as a JUnit XML report"
    )]
    pub junit: Option<PathBuf>,
}
