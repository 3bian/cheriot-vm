# CHERIoT Debug for VS Code

A minimal extension that runs `cheriot debug --dap` as the debug adapter, so
firmware can be debugged in the emulator without GDB.

Copy or symlink this folder into `~/.vscode/extensions/cheriot-debug`, restart
VS Code, and add a launch configuration:

```json
{
  "type": "cheriot",
  "request": "launch",
  "name": "Debug CHERIoT firmware",
  "build": true,
  "profile": "vm"
}
```

`program`, `profile`, `config` and `build` mean the same as the `cheriot debug`
options. Source breakpoints, stepping, pause, registers, memory and guest
console output are supported. The call stack shows the current function and
its caller only. The adapter needs a CHERIoT profile and an ELF with debug
information.
