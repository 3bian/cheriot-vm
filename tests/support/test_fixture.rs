// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use std::{
    fs,
    path::{Path, PathBuf},
};

#[path = "test_directory.rs"]
mod test_directory;

#[path = "rtos_tree.rs"]
pub mod rtos_tree;

pub struct TestFixture {
    pub directory: PathBuf,
    temporary_directory: test_directory::TestDirectory,
}

impl TestFixture {
    pub fn new() -> Self {
        let temporary_directory = test_directory::TestDirectory::new();
        let directory = temporary_directory.directory.clone();
        fs::create_dir_all(directory.join("boards")).unwrap();
        let overlay_sdk =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("integrations/cheriot-rtos/sdk");
        rtos_tree::copy_tree(&overlay_sdk, &directory.join("sdk"));
        fs::write(
            directory.join("cheriot.toml"),
            include_str!("producer-consumer.toml"),
        )
        .unwrap();
        let libraries = directory.join("sdk/lib");
        for entry in fs::read_dir(libraries).unwrap().flatten() {
            let catalogue = entry.path().join("cheriot.toml");
            if !catalogue.is_file() {
                continue;
            }
            let document: toml::Value =
                toml::from_str(&fs::read_to_string(catalogue).unwrap()).unwrap();
            if let Some(components) = document
                .get("sdk_components")
                .and_then(toml::Value::as_table)
            {
                for component in components.values() {
                    for field in ["sources", "force_includes", "includes"] {
                        if let Some(sources) = component
                            .get("build")
                            .and_then(|build| build.get(field))
                            .and_then(toml::Value::as_array)
                        {
                            for source in sources {
                                let relative =
                                    source.as_str().unwrap().strip_prefix("${sdk}/").unwrap();
                                let path = directory.join("sdk").join(relative);
                                if field == "includes" {
                                    fs::create_dir_all(path).unwrap();
                                } else {
                                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                                    fs::write(path, "/* SDK source fixture */\n").unwrap();
                                }
                            }
                        }
                    }
                }
            }
        }
        let sdk_document: toml::Value =
            toml::from_str(&fs::read_to_string(directory.join("sdk/cheriot.toml")).unwrap())
                .unwrap();
        for directory_name in sdk_document["sdk_compatibility"]["required_directories"]
            .as_array()
            .unwrap()
        {
            fs::create_dir_all(directory.join("sdk").join(directory_name.as_str().unwrap()))
                .unwrap();
        }
        for required in sdk_document["sdk_compatibility"]["required_files"]
            .as_array()
            .unwrap()
        {
            let path = directory
                .join("sdk")
                .join(required["path"].as_str().unwrap());
            if !path.exists() {
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(
                    path,
                    required
                        .get("contains")
                        .and_then(toml::Value::as_str)
                        .unwrap_or("/* SDK fixture */\n"),
                )
                .unwrap();
            }
        }
        for relative in [
            "core/allocator/main.cc",
            "core/software_revoker/revoker.cc",
            "software_revoker.ldscript",
        ] {
            let path = directory.join("sdk").join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            if !path.exists() {
                fs::write(path, "/* SDK test fixture */\n").unwrap();
            }
        }
        fs::write(
            directory.join("boards/sail.toml"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/integrations/cheriot-rtos/sdk/boards/sail.toml"
            )),
        )
        .unwrap();
        for path in ["include/platform/sail", "include/platform/generic-riscv"] {
            fs::create_dir_all(directory.join("sdk").join(path)).unwrap();
        }
        for name in ["producer.cc", "consumer.cc"] {
            fs::write(
                directory.join(name),
                "// source existence fixture; not a firmware compilation test\n",
            )
            .unwrap();
        }
        Self {
            directory,
            temporary_directory,
        }
    }

    pub fn project_file(&self) -> PathBuf {
        self.directory.join("cheriot.toml")
    }

    pub fn replace_text(&self, name: &str, from: &str, to: &str) {
        let path = self.directory.join(name);
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains(from), "missing {from}");
        fs::write(path, contents.replace(from, to)).unwrap();
    }

    pub fn write_file(&self, name: &str, contents: &str) {
        self.temporary_directory.write_file(name, contents);
    }

    pub fn write_plan_placeholders(&self) {
        for script in [
            "sdk/library.ldscript",
            "sdk/compartment.ldscript",
            "sdk/privileged-compartment.ldscript",
            "sdk/software_revoker.ldscript",
            "sdk/core/loader/loader.ldscript",
        ] {
            let script = self.directory.join(script);
            fs::create_dir_all(script.parent().unwrap()).unwrap();
            fs::write(script, "/* linker input existence fixture */\n").unwrap();
        }
    }
}
