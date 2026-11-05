# CHERIoT Virtual Machine

CHERIoT and RV32 RISC-V firmware with simulated memory and devices.

## Command-Line Options

Run the following command for the current list:

```sh
./target/release/cheriot-vm --help
```

```text
Usage: cheriot-vm --profile FILE --elf FILE [OPTION]...
   or: cheriot-vm --profile FILE --raw FILE [OPTION]...

Run CHERIoT and RV32 RISC-V firmware with simulated memory and devices.

Options:
  --address ADDRESS
    Load a raw binary at this address (default: 0).

  --breakpoint ADDRESS
    Stop before executing at this address.

  --checkpoint FILE
    Write an architecture-aware comparison snapshot.

  --completion disabled|elf-symbol|ADDRESS
    Select the optional HTIF observer.

  --elf FILE
    Load an ELF32 program.

  --gdb [KEY=VALUE[,KEY=VALUE]...]
    Start halted for GDB (requires the gdb feature).
    Optional settings:
      ip=IP (loopback only; default: 127.0.0.1)
      port=PORT (default: 333)
      output=escaped|raw (default: escaped)

  --gpio [device=NAME,]path=PATH,lines=N[+N]...
    Connect GPIO to Linux chip line offsets.
    Optional settings:
      active-low=true|false (default: false)
      bias=as-is|disabled|pull-up|pull-down (default: as-is)
      drive=push-pull|open-drain|open-source (default: push-pull)

  --i2c [device=NAME,]path=PATH
    Connect I2C to a Linux adapter (host sets clock).
    Optional settings:
      addresses=N[+N]... (unique 0x08..0x77; default: all)

  --memory address=ADDRESS[,length=BYTES]
    Inspect 1-256 RAM/ROM bytes on stop (default: 16).

  --output raw|labelled|escaped
    Select guest-output presentation.

  --profile FILE
    Select a TOML machine profile.

  --raw FILE
    Load a raw binary.

  --registers [names=REGISTER[+REGISTER]...]
    Show registers on stop; all if names are omitted.

  --spi [device=NAME,]path=PATH
    Connect SPI to Linux spidev.
    Optional settings:
      select=N (0..3; default: 0)
      speed=HZ (1..50000000; default: 500000)

  --steps NUMBER
    Limit attempted steps; 0 is unlimited.

  --tap [device=NAME,]interface=NAME
    Connect Ethernet to an existing Linux TAP interface.

  --trace
    Print instructions and traps.

  --trap-policy stop|vector
    Stop on a trap or enter the guest handler.

  --uart [device=NAME,]in=PATH[,out=PATH]
  --uart [device=NAME,]out=PATH
    Connect UART input, output or both.
    At least one of in or out is required.
    Optional host TTY settings:
      baud=RATE (default: keep current rate)
      data-bits=5|6|7|8 (default: 8)
      flow=none|rtscts|xonxoff (default: none)
      parity=none|even|odd (default: none)
      stop-bits=1|2 (default: 1)

  -h, --help
    Display this help and exit.
```

Square brackets mark optional parts; ... means the preceding part may repeat. A + separates list items, and lists may contain just one item. For GDB, KEY is ip, port or output; supply any subset as one comma-separated argument, or omit the argument to use the defaults.

Use --profile with either --elf or --raw. Addresses and counts accept decimal or 0x hexadecimal values. Repeat connection options to connect different devices. Other value-taking options can only be supplied once; repeated --trace switches are harmless. Checkpoints are comparison snapshots, not restore images.

## Build and Run

Install Rust and Cargo with [rustup](https://github.com/rust-lang/rustup). The rust-toolchain.toml file selects the required toolchain. From this folder, build and run the bundled guest:

```sh
cargo build --locked --release
./target/release/cheriot-vm \
  --elf examples/hello_world/hello_world.elf \
  --profile profiles/cheriot1.toml \
  --steps 1000000
```

The program prints Hello world compartment: Hello world and tells the VM when it has finished. The included ELF matches profiles/cheriot1.toml and has debug symbols.

To build the VM with guest debugger support, run this in the VM folder:

```sh
cargo build --locked --release --features gdb
```

Cargo builds the VM itself. Build your guest program separately with the compiler and build system for its target.

### Build the CHERIoT Examples

The ELF is included, so you only need these steps if you want to rebuild it. You will need Git, xmake, CMake, Ninja, a C/C++ build compiler and a CHERIoT RTOS checkout.

To build the CHERIoT LLVM toolchain from source, run these commands from this folder. The build needs substantial disk space and time:

```sh
git clone --branch cheriot --single-branch https://github.com/CHERIoT-Platform/llvm-project.git ../cheriot-llvm
git -C ../cheriot-llvm checkout ee468e7b3ef1e0f8f314c3a4a6cae7b5930d6c13
cmake -S ../cheriot-llvm/llvm -B ../cheriot-llvm-build -G Ninja \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLVM_ENABLE_PROJECTS="clang;clang-tools-extra;lld" \
  -DLLVM_TARGETS_TO_BUILD=RISCV \
  -DLLVM_ENABLE_UNWIND_TABLES=OFF \
  -DLLVM_DISTRIBUTION_COMPONENTS="clang;clangd;lld;llvm-objdump;llvm-objcopy" \
  -DCMAKE_INSTALL_PREFIX="$PWD/../cheriot-tools"
cmake --build ../cheriot-llvm-build --target install-distribution
export CHERIOT_TOOLCHAIN="$(cd ../cheriot-tools && pwd)"
```

If you already have the CHERIoT LLVM tools, set CHERIOT_TOOLCHAIN to their installation directory instead:

```sh
export CHERIOT_TOOLCHAIN=/absolute/path/to/cheriot-tools
```

Next, prepare the CHERIoT RTOS checkout and the VM's board support.

For example, from this folder, create a checkout next to it:

```sh
git clone --recursive https://github.com/CHERIoT-Platform/cheriot-rtos.git ../cheriot-rtos
export CHERIOT_RTOS="$(cd ../cheriot-rtos && pwd)"
cp -a integrations/cheriot-rtos/. "$CHERIOT_RTOS/"
```

The copy installs the VM's board and drivers into that RTOS checkout. If you already have a checkout, set CHERIOT_RTOS to its absolute path instead.

CHERIOT_TOOLCHAIN is the LLVM installation directory passed to --sdk; it is separate from CHERIOT_RTOS. From this folder:

```sh
cd examples/hello_world
xmake f --clean --board=cheriot-vm --sdk="$CHERIOT_TOOLCHAIN" -m debug -y
xmake
cd ../..
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml \
  --elf examples/hello_world/build/cheriot/cheriot/debug/hello_world.elf \
  --steps 1000000
```

The build produces hello_world.elf under examples/hello_world/build/cheriot/cheriot/debug/. When debugging it, select that file so the symbols match the program running in the VM.

### ELF and Raw Firmware

An ELF file contains the firmware along with information about where to load it and where execution starts. It can also contain symbols and source information for a debugger. The bundled Hello World program is an ELF file.

A raw binary contains only bytes. It does not describe its load address or entry point, so you must provide an address that matches both the firmware and the profile. The VM loads the bytes there and starts execution at that address. Raw binaries do not carry ELF symbols or capability tags.

For example, to load your own raw firmware at 0x80000000:

```sh
./target/release/cheriot-vm \
  --address 0x80000000 \
  --profile profiles/cheriot1.toml \
  --raw firmware.bin \
  --steps 100000 \
  --trap-policy stop
```

Replace firmware.bin with your file. This example stops on an architectural trap and allows up to 100,000 attempted steps; adjust those settings for your program. For source-level debugging, use the matching ELF file when one is available.

Firmware must match the profile's processor, memory and devices. The supplied cheriot-vm board JSON describes the UART and other devices in the cheriot1 profile. The CHERIoT profile uses the 16550 UART register layout.

Execution is unlimited by default; reaching a step limit reports incomplete execution. HTIF lets a guest report its exit status. For a guest with a tohost symbol:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml \
  --elf firmware.elf \
  --completion elf-symbol
```

For a generic ELF without that completion interface:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml \
  --elf firmware.elf \
  --completion disabled
```

## Debugging Guest Programs

The debugger lets you pause and inspect a program running inside the VM. Build your C or C++ guest with debug symbols using your project's build system, and keep its matching ELF and source files. On Linux, install a multiarch GDB and use a VM built with debugger support, as described in Build and Run.

Start the VM with your profile and guest ELF. For the bundled example:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml \
  --elf examples/hello_world/hello_world.elf \
  --gdb
```

With no settings, --gdb listens on 127.0.0.1:333. To choose another port, use:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml \
  --elf examples/hello_world/hello_world.elf \
  --gdb port=3333
```

You can also specify both settings:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml \
  --elf examples/hello_world/hello_world.elf \
  --gdb ip=127.0.0.1,port=3333
```

Use the selected port in GDB or VS Code. If your host restricts ports below 1024, use the port-3333 example above rather than running the VM as root. The guest starts paused. Once the VM reports that it is listening, open another terminal and start GDB with the same ELF:

```sh
gdb-multiarch examples/hello_world/hello_world.elf
```

Attach within 30 seconds, then set breakpoints or step through instructions:

```text
(gdb) target remote 127.0.0.1:333
(gdb) break say_hello
(gdb) continue
(gdb) stepi
(gdb) info registers
(gdb) x/4wx 0x80000000
```

Replace the example paths with your own ELF and profile. Attachment leaves the guest paused; the VM has already loaded the program, so do not run GDB's load command. Source breakpoints need the source files recorded in the ELF. For an address breakpoint:

```text
(gdb) break *0x80000004
```

Continue, interrupt, instruction stepping, breakpoints, register inspection and side-effect-free RAM inspection are supported. For CHERIoT, inspect the program-counter capability and memory tags with:

```text
(gdb) monitor capability pcc
(gdb) monitor tags 0x80000000 4
```

GDB access to registers and memory is read-only. Native capability-pointer display and full cross-compartment unwinding are not supported.

GDB's detach command resumes the guest. Its kill command terminates the VM. A completed guest ends the debugging session; a debugger stop on its own does not mean that the guest completed successfully.

## Debugging Guest Programs with Visual Studio Code

Follow the guest build and debugger prerequisites above, then install the Microsoft C/C++ extension (ms-vscode.cpptools).

1. Copy the three files from integrations/vscode into a .vscode folder at the root of your guest project, then open that project folder in VS Code. Merge with existing configurations if you already have them.
2. Choose **Terminal**, then **Run Task**, then **CHERIoT VM: Start guest debugger**. Enter the absolute paths to the VM executable, matching profile and guest ELF. The default endpoint is 127.0.0.1:333.
3. When the VM reports that it is listening, open **Run and Debug**, select **Attach to CHERIoT guest** or **Attach to RV32 guest**, and press **F5**. Enter the same ELF and endpoint, plus your GDB path, within the 30-second connection window.
4. Use VS Code's breakpoints, **Continue**, **Pause** and instruction-stepping controls.

GDB commands in the Debug Console need the -exec prefix:

```text
-exec monitor capability pcc
```

The task starts the VM; it does not build your guest. If the ELF's source paths differ from your project, adjust sourceFileMap in launch.json. The two attach configurations retain different source mappings for CHERIoT and RV32 guests. Stop the VM task through **Terminal**, then **Terminate Task**, when you finish.

Copied templates passed headless adapter tests with GDB 16.3 and Microsoft C/C++ 1.29.3, including source breakpoints. The current templates have not been tested in the graphical VS Code UI.

## Profiles

Choose a profile that matches your firmware:

- **CHERIoT:** profiles/cheriot1.toml uses tagged memory and a 16550 UART register layout with polled input.
- **RV32E:** profiles/rv32e.toml provides 16 integer registers and the simple UART layout with input interrupts.
- **RV32IMC:** profiles/rv32imc.toml provides 32 integer registers, multiplication and compressed instructions, and the simple UART layout with input interrupts.

All three place RAM at 0x80000000 and share the other peripheral addresses. CHERIoT uses tagged RAM; the RV32 profiles use untagged RAM. The processor, memory map and device register layouts must match your firmware. A custom profile can select any of the nine supported processors.

## Host Connections

Devices are simulated unless you connect them to the host. Each connection uses comma-separated settings in one flag. The device name is optional when the profile has one matching device. If several match, add device=NAME using the name from the profile. Settings can appear in any order. Repeat a flag to connect different devices.

UART output goes to stdout by default; input stays disconnected until requested. GPIO pins, SPI and I²C peripherals remain simulated, and Ethernet uses in-memory queues, until you connect them to host resources.

Commas separate settings, so paths containing commas cannot be used here. GPIO line offsets use + as their separator. The examples below use firmware.elf as a placeholder for your program. Host adapters require Linux and permission to use the selected resources.

### UART Input and Output

UART connects a guest serial device to a host stream. The supplied profiles name their UART uart0. These options work with both CHERIoT and RV32; your firmware must use the UART register layout selected by its profile and read incoming bytes itself. The bundled Hello World program only prints a message and exits.

When using --uart, supply at least one path: input, output, or both in one connection. Settings can appear in any order. With no output path, guest UART output still goes to stdout. With no input path, that UART receives no host input.

#### Terminal and Pipe Input

To read from your terminal, or from a pipe connected to stdin:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --uart in=/dev/stdin
```

Press Enter to send a line when using a terminal in its normal line-input mode. The VM leaves stdin's terminal settings unchanged. Firmware that echoes input may produce a second copy alongside your terminal's own echo.

You can also pipe bytes into the guest:

```sh
printf 'Hello\n' | ./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --uart in=/dev/stdin
```

Input streams through a bounded 4,096-byte receive queue; the total stream can be larger. The VM pauses host reads while that queue is full. For files and pipes, EOF ends host input without stopping the guest; it is not delivered as a special UART byte. A guest waiting for more input may therefore keep running after EOF. An idle zero-byte terminal read does not end input; the VM keeps polling until data arrive or the terminal hangs up.

#### Output and File Capture

To select stdin and stdout explicitly:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --uart in=/dev/stdin,out=/dev/stdout \
  --output raw
```

The output option controls stdout presentation: raw preserves guest bytes, labelled adds device labels, and escaped makes terminal-control bytes visible. Without it, CHERIoT retains its compatibility presentation and RV32 uses labels. In raw mode, VM diagnostics go to stderr.

Paths can also name regular files. For example, read input.bin and capture this UART's output in uart.log:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --uart in=input.bin,out=uart.log
```

To capture output without enabling host input:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --uart out=uart.log
```

An output file is created or overwritten. Use different input and output files. A dedicated output path receives only the selected UART’s bytes, without labels or escaping. VM diagnostics and HTIF output keep their normal destinations, including when a UART is named tohost. Use /dev/null as the output path to discard that UART's output.

Output is buffered and flushed during execution at roughly 16 ms intervals and when execution stops. This is not a real-time guarantee. A stalled consumer or host device can delay or fail output; a guest step limit does not impose a host wall-clock deadline.

#### Host Serial Ports

To connect both directions to a host serial port using 115,200 baud and 8N1:

```sh
./target/release/cheriot-vm \
  --profile profiles/rv32imc.toml --elf firmware.elf \
  --uart in=/dev/ttyUSB0,out=/dev/ttyUSB0,baud=115200
```

Optional host serial settings are:

- **baud:** a supported standard Linux rate, such as 9600, 57600 or 115200. If omitted, the port's existing rate is retained.
- **data-bits:** 5, 6, 7 or 8; defaults to 8 when host settings are requested.
- **parity:** none, even or odd; defaults to none.
- **stop-bits:** 1 or 2; defaults to 1. Five data bits with two stop bits is rejected because the host convention cannot represent two full stop bits for that combination.
- **flow:** none, rtscts for hardware flow control, or xonxoff for software flow control; defaults to none.

For example, to request even parity and hardware flow control:

```sh
./target/release/cheriot-vm \
  --profile profiles/rv32imc.toml --elf firmware.elf \
  --uart in=/dev/ttyUSB0,out=/dev/ttyUSB0,baud=115200,data-bits=8,parity=even,stop-bits=1,flow=rtscts
```

Match these settings to the connected equipment. Hardware flow control requires suitable wiring; software flow control reserves XON/XOFF bytes for the host terminal driver.

Requesting any serial setting puts the selected host TTY into raw mode. Without serial settings, the VM leaves its configuration unchanged. These settings require an explicit TTY path; stdin/stdout and ordinary files keep their existing settings. They configure the host connection, not guest UART timing. Unsupported rates or settings that the host does not accept produce an error.

The VM checks the applied settings, drains pending serial output and restores the original settings on orderly exit. Forced termination may leave the port changed. Use an account with permission to access the port, and avoid using a port that another application owns. Physical serial framing and electrical operation have not been qualified on hardware.

#### Multiple UARTs and Debugging

For a custom profile with several UARTs, select each by its profile name:

```sh
./target/release/cheriot-vm \
  --profile my-board.toml --elf firmware.elf \
  --uart device=uart0,in=/dev/stdin \
  --uart device=uart1,out=/dev/ttyUSB0
```

Omit the name only when the profile has exactly one UART. Repeat the flag for different UARTs; combine input, output and serial settings for the same UART into one flag. A host input path can only be bound once. Non-stdout output paths must also be unique; several UARTs can share stdout.

During debugging, dedicated output paths must be a terminal, pipe or /dev/null; regular output files are rejected. To capture debugger-session stdout, pipe it through tee. Debugger output presentation is selected separately through the GDB option:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --gdb port=3333,output=raw | tee guest.log
```

### Linux Devices and Networking

Connect only the resources your program needs. These settings can be used individually or together:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --gpio path=/dev/gpiochip0,lines=17+27 \
  --i2c path=/dev/i2c-1 \
  --spi path=/dev/spidev0.0 \
  --tap interface=tap0
```

GPIO maps guest bits 0 and 1 to chip offsets 17 and 27. Optional active-low reverses their logical level; bias selects as-is, disabled, pull-up or pull-down; drive selects push-pull, open-drain or open-source. Defaults are active-high, existing bias and push-pull output drive. Lines start as inputs. It requires Linux GPIO character-device v2 support (kernel 5.10 or later); polling can miss short pulses. TAP connects to an existing interface named tap0. Firmware needs drivers for the VM's emulated controllers.

SPI connects guest chip select 0 by default, uses eight-bit transfers and starts at 500,000 Hz. The guest selects mode 0–3. For another guest chip select or speed:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --spi device=spi0,path=/dev/spidev0.1,select=1,speed=1000000
```

Repeat SPI connections for distinct guest chip selects. Deselect before changing mode. Linux chip-select retention between transfers depends on the host controller and needs hardware validation.

I²C can restrict host access with an optional addresses list:

```sh
./target/release/cheriot-vm \
  --profile profiles/cheriot1.toml --elf firmware.elf \
  --i2c path=/dev/i2c-1,addresses=0x50+0x68
```

Without a list, unreserved seven-bit addresses are allowed. Linux controls the adapter clock; the VM does not set I²C bus speed. I²C forwards write transactions at STOP, or combines queued writes with a following read using a repeated START. For a multi-byte host read, firmware sets bits 16–27 of the existing CONTROL register to the read length minus one before the first read command. The default is one byte; the maximum is 4,096. This stays inside the bundled SDK's 24-byte register window. Preserve bit 0 if completion interrupts are enabled; the SDK's interrupt-enable helper resets the length to one, so configure it first. Other read/write direction changes require STOP first. Physical access permits unreserved seven-bit addresses on the selected host bus; kernel-owned addresses are not forced. The host adapter does not expose arbitrary Linux ioctls to the guest.

Physical GPIO, SPI and I²C transfers have not been qualified on hardware. Their protocol and error paths have mock coverage. Kernel device operations can block; use a supervised process when a wall-clock limit is required. Live GPIO, TAP, SPI and I²C connections are unavailable during GDB sessions.

## Licence

The CHERIoT profile is experimental; complete compliance and security isolation have not been established. Licensed under the [MIT licence](LICENSE).
