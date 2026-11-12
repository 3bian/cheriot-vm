# Targets

A target says where firmware is executed, debugged, tested and flashed. Each
file here is one target; `cheriot exec --target NAME` finds `NAME.toml` in the
project's `targets/` folder, the SDK's `targets/` folder, the folders in
`CHERIOT_TARGETS_DIR`, the global `targets_dir` setting, or the `targets/`
folder shipped with the `cheriot` program (this one). A `[targets.NAME]` table
in `cheriot.toml` works the same way and takes precedence.

```toml
description = "My board"

[package]                 # how to make the file the board accepts
format = "uf2"            # built in; or:
# command = ["llvm-objcopy", "-O", "binary", "${firmware}", "${package}"]
# file = "${output_dir}/${firmware_name}.bin"

[exec]                    # one of:
emulator = true           #   the built-in emulator, with an optional profile
profile = "my-profile.toml"
# command = ["my-simulator", "${firmware}"]

[debug]                   # same choices as [exec]
command = ["my-debugger", "${firmware}"]

[flash]                   # one of:
volume = "BOARD"          #   copy to the mounted drive with this label
# copy_to = "${project_dir}/images"
# command = ["my-flasher", "--port", "${destination}", "${package}"]
```

Placeholders: `${firmware}`, `${firmware_name}`, `${package}`,
`${project_dir}`, `${build_dir}`, `${output_dir}`, `${sdk}`, `${destination}`
(the `--to` option of `flash`) and `${env:NAME}`.
