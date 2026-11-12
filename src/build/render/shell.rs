// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::build::build_graph::{BuildCommand, BuildPlan};
use crate::config::diagnostic::Result;
use std::path::Path;

pub fn quote_shell_argument(argument: &str) -> String {
    let plain = !argument.is_empty()
        && argument
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_@%+=:,./-".contains(character));
    if plain {
        argument.to_owned()
    } else {
        format!("'{}'", argument.replace('\'', "'\"'\"'"))
    }
}

const OPTIONS_WITH_A_VALUE: &[&str] = &[
    "-C", "-D", "-I", "-L", "-MF", "-MQ", "-MT", "-T", "-U", "-Xclang", "-include", "-iquote",
    "-isystem", "-o", "-target", "-x",
];

fn relative_to(argument: &str, base: Option<&Path>) -> String {
    let Some(base) = base.and_then(Path::to_str).filter(|base| base.len() > 1) else {
        return argument.to_owned();
    };
    let prefix = format!("{base}/");
    if let Some(rest) = argument.strip_prefix(&prefix) {
        return rest.to_owned();
    }
    for option in ["-I", "-L"] {
        if let Some(rest) = argument
            .strip_prefix(option)
            .and_then(|path| path.strip_prefix(&prefix))
        {
            return format!("{option}{rest}");
        }
    }
    match argument.split_once('=') {
        Some((option, path)) if option.starts_with('-') && !option.starts_with("-D") => {
            match path.strip_prefix(&prefix) {
                Some(rest) => format!("{option}={rest}"),
                None => argument.to_owned(),
            }
        }
        _ => argument.to_owned(),
    }
}

pub fn format_shell_command(command: &BuildCommand, base: Option<&Path>) -> Vec<String> {
    let mut lines = Vec::new();
    if base != Some(command.cwd.as_path()) {
        lines.push(format!(
            "cd {} &&",
            quote_shell_argument(&command.cwd.to_string_lossy())
        ));
    }
    let base = base.filter(|base| *base == command.cwd.as_path());
    for (name, value) in &command.environment {
        lines.push(quote_shell_argument(&format!("{name}={value}")));
    }
    lines.push(quote_shell_argument(&relative_to(
        &command.executable,
        base,
    )));
    let mut arguments = command.argv.iter().map(|argument| argument.value.as_str());
    while let Some(argument) = arguments.next() {
        let mut line = format!("  {}", quote_shell_argument(&relative_to(argument, base)));
        if OPTIONS_WITH_A_VALUE.contains(&argument)
            && let Some(value) = arguments.next()
        {
            line.push(' ');
            line.push_str(&quote_shell_argument(&relative_to(value, base)));
        }
        lines.push(line);
    }
    for (operator, file) in [(">", &command.stdout), ("2>", &command.stderr)] {
        if let Some(file) = file {
            lines.push(format!(
                "  {operator} {}",
                quote_shell_argument(&relative_to(&file.to_string_lossy(), base))
            ));
        }
    }
    let last = lines.len().saturating_sub(1);
    for line in &mut lines[..last] {
        line.push_str(" \\");
    }
    lines
}

pub fn render_shell_script(plan: &BuildPlan) -> Result<String> {
    plan.validate()?;
    let json = serde_json::to_string_pretty(plan)?;
    Ok(format!(
        "#!/bin/sh\nset -eu\nexec cheriot build --execute-plan - <<'CHERIOT_PLAN_JSON'\n{json}\nCHERIOT_PLAN_JSON\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::diagnostic::Sourced;

    fn command(arguments: &[&str]) -> BuildCommand {
        BuildCommand {
            executable: "/tools/bin/clang++".into(),
            argv: arguments
                .iter()
                .map(|argument| Sourced::derived((*argument).to_owned(), "test"))
                .collect(),
            cwd: "/work/project".into(),
            environment: Default::default(),
            stdout: Some("/work/project/build/log.txt".into()),
            stderr: None,
        }
    }

    #[test]
    fn a_command_is_one_argument_to_a_line_and_can_be_pasted() {
        let command = command(&[
            "-c",
            "-target",
            "riscv32-unknown-cheriotrtos",
            "-DNAME=\"a value\"",
            "-I/work/project/include",
            "--script=/work/project/build/x.ldscript",
            "-o",
            "/work/project/build/a.o",
            "/sdk/core/a.cc",
        ]);
        let full = format_shell_command(&command, None);
        assert_eq!(full[0], "cd /work/project && \\");
        assert_eq!(full[1], "/tools/bin/clang++ \\");
        assert!(full.contains(&"  -o /work/project/build/a.o \\".to_owned()));
        assert_eq!(full.last().unwrap(), "  > /work/project/build/log.txt");
        assert!(
            full[..full.len() - 1]
                .iter()
                .all(|line| line.ends_with(" \\"))
        );

        let short = format_shell_command(&command, Some(Path::new("/work/project")));
        assert_eq!(
            short,
            [
                "/tools/bin/clang++ \\",
                "  -c \\",
                "  -target riscv32-unknown-cheriotrtos \\",
                "  '-DNAME=\"a value\"' \\",
                "  -Iinclude \\",
                "  --script=build/x.ldscript \\",
                "  -o build/a.o \\",
                "  /sdk/core/a.cc \\",
                "  > build/log.txt",
            ]
        );
    }
}
