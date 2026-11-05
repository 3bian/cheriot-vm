-- Copyright Microsoft and CHERIoT Contributors.
-- SPDX-License-Identifier: MIT

set_project("CHERIoT Hello World")
local rtos = os.getenv("CHERIOT_RTOS")
if not rtos then
    raise("Set CHERIOT_RTOS to a CHERIoT RTOS checkout including the VM board overlay")
end
sdkdir = path.join(rtos, "sdk")
includes(sdkdir)
set_toolchains("cheriot-clang")
option("board")
    set_default("cheriot-vm")
option_end()

compartment("hello")
    add_deps("freestanding", "debug")
    add_cxflags("-fdebug-prefix-map=" .. os.projectdir() .. "=.", {force = true})
    add_files("hello.cc")

firmware("hello_world")
    set_filename("hello_world.elf")
    add_deps("hello")
    on_load(function(target)
        target:values_set("threads", {{
            compartment = "hello",
            priority = 1,
            entry_point = "say_hello",
            stack_size = 0x200,
            trusted_stack_frames = 1
        }}, {expand = false})
    end)
