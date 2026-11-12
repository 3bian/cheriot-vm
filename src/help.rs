// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use clap::Command;

fn description(out: &mut String, text: &str) {
    for paragraph in text.lines() {
        description_line(out, paragraph);
    }
}
fn description_line(out: &mut String, text: &str) {
    let mut line = String::from("    ");
    for word in text.split_whitespace() {
        if line.len() + word.len() + 1 > 80 && line.len() > 4 {
            out.push_str(line.trim_end());
            out.push('\n');
            line = String::from("    ");
        }
        if line.len() > 4 {
            line.push(' ');
        }
        line.push_str(word);
    }
    if line.len() > 4 {
        out.push_str(&line);
        out.push('\n');
    }
}
fn sentence(text: &str) -> String {
    let text = text.trim_end();
    if text.ends_with(['.', '?', '!']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

const WITHOUT_A_PROJECT: &[&str] = &["init", "inspect"];

fn render(command: &Command, path: &str) -> String {
    let mut out = format!("Usage: {path}");
    if command.has_subcommands() {
        out.push_str(if command.is_subcommand_required_set() {
            " COMMAND"
        } else {
            " [COMMAND]"
        });
    }
    for arg in command.get_positionals().filter(|a| !a.is_hide_set()) {
        let value = arg
            .get_value_names()
            .and_then(|v| v.first())
            .map(|v| v.to_string())
            .unwrap_or_else(|| arg.get_id().to_string().to_uppercase());
        let value = if arg.get_action().takes_values()
            && arg.get_num_args().is_some_and(|n| n.max_values() > 1)
        {
            format!("{value}...")
        } else {
            value
        };
        if arg.is_required_set() {
            out.push_str(&format!(" {value}"));
        } else {
            out.push_str(&format!(" [{value}]"));
        }
    }
    out.push_str(" [OPTIONS]\n");
    if let Some(about) = command.get_about() {
        out.push('\n');
        out.push_str(about.to_string().trim_end_matches('.'));
        out.push_str(".\n");
    }
    let positionals: Vec<_> = command
        .get_positionals()
        .filter(|a| !a.is_hide_set() && a.get_help().is_some())
        .collect();
    if !positionals.is_empty() {
        out.push_str("\nArguments:\n");
        for arg in positionals {
            let value = arg
                .get_value_names()
                .and_then(|v| v.first())
                .map(|v| v.to_string())
                .unwrap_or_else(|| arg.get_id().to_string().to_uppercase());
            out.push_str(&format!("  {value}\n"));
            if let Some(help) = arg.get_help() {
                description(&mut out, &sentence(&help.to_string()));
            }
            out.push('\n');
        }
        out.pop();
    }
    let mut subs: Vec<_> = command
        .get_subcommands()
        .filter(|s| !s.is_hide_set())
        .collect();
    subs.sort_by_key(|s| s.get_name());
    if !subs.is_empty() {
        out.push_str("\nCommands:\n");
        for sub in subs {
            out.push_str(&format!("  {}\n", sub.get_name()));
            if let Some(about) = sub.get_about() {
                description(&mut out, &sentence(&about.to_string()));
            }
            out.push('\n');
        }
        out.pop();
    }
    let mut args: Vec<_> = command
        .get_arguments()
        .filter(|a| a.get_long().is_some() && !a.is_hide_set() && a.get_long() != Some("help"))
        .filter(|a| {
            a.get_long() != Some("config") || !WITHOUT_A_PROJECT.contains(&command.get_name())
        })
        .collect();
    args.sort_by_key(|a| a.get_long().unwrap_or_default());
    out.push_str("\nOptions:\n");
    for arg in args {
        out.push_str("  ");
        if let Some(short) = arg.get_short() {
            out.push_str(&format!("-{short}, "));
        }
        out.push_str("--");
        out.push_str(arg.get_long().unwrap_or_default());
        if arg.get_action().takes_values() {
            let values = arg
                .get_value_parser()
                .possible_values()
                .map(|v| {
                    v.filter(|v| !v.is_hide_set())
                        .map(|v| v.get_name().to_string())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if values.is_empty() {
                let name = arg
                    .get_value_names()
                    .and_then(|v| v.first())
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| arg.get_id().to_string().replace('-', "_").to_uppercase());
                out.push(' ');
                out.push_str(&name);
            } else {
                out.push(' ');
                out.push_str(&values.join("|"));
            }
        }
        out.push('\n');
        if let Some(help) = arg.get_help() {
            description(&mut out, &sentence(&help.to_string()));
        }
        out.push('\n');
    }
    out.push_str("  -h, --help\n    Display this help and exit.\n");
    out
}
pub fn prepare(mut command: Command, path: &str) -> Command {
    command.build();
    if command
        .get_arguments()
        .any(|arg| arg.get_long() == Some("version"))
    {
        command = command.mut_arg("version", |arg| {
            arg.short(None).help("Print the version and exit")
        });
    }
    let rendered = render(&command, path);
    command = command.override_help(rendered);
    let parent = path.to_string();
    command.mut_subcommands(move |child| {
        let child_path = format!("{} {}", parent, child.get_name());
        prepare(child, &child_path)
    })
}

pub(crate) fn print(name: &str) -> crate::app_error::Result<u8> {
    use clap::CommandFactory;
    let mut root = prepare(crate::cli::Cli::command(), "cheriot");
    let command = root
        .find_subcommand_mut(name)
        .ok_or_else(|| crate::app_error::AppError::Message(format!("unknown command {name}")))?;
    command.print_long_help()?;
    Ok(0)
}

pub(crate) fn no_arguments_after(name: &str) -> bool {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    arguments.len() == 1 && arguments[0] == name
}
