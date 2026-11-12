# {{name}}

A CHERIoT firmware project.

```sh
cheriot config check      # validate cheriot.toml
cheriot build             # build the firmware
cheriot test              # build, run and check the output
cheriot exec --build      # run it in the emulator
```

The CHERIoT RTOS SDK is expected at `{{sdk}}`, with the `cheriot` integration
files copied over it. Change `sdk` in `cheriot.toml` if it is elsewhere.
