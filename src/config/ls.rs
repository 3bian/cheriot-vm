// Copyright 3Bian Limited and Contributors
// SPDX-License-Identifier: MIT

use crate::config::transaction;
use crate::{
    app_error::{AppError, Result},
    cli::Format,
};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Value};

const MAX_FILE_BYTES: u64 = 4 << 20;

pub(super) enum Node {
    Table(Vec<(String, Node)>),
    Array(Vec<Node>),
    Scalar { kind: &'static str, text: String },
}

impl Node {
    fn from_item(item: &Item) -> Option<Self> {
        match item {
            Item::None => None,
            Item::Value(value) => Some(Self::from_value(value)),
            Item::Table(table) => Some(Self::Table(
                table
                    .iter()
                    .filter_map(|(k, v)| Some((k.to_owned(), Self::from_item(v)?)))
                    .collect(),
            )),
            Item::ArrayOfTables(tables) => Some(Self::Array(
                tables
                    .iter()
                    .map(|table| {
                        Self::Table(
                            table
                                .iter()
                                .filter_map(|(k, v)| Some((k.to_owned(), Self::from_item(v)?)))
                                .collect(),
                        )
                    })
                    .collect(),
            )),
        }
    }

    fn from_value(value: &Value) -> Self {
        match value {
            Value::InlineTable(table) => Self::Table(
                table
                    .iter()
                    .map(|(k, v)| (k.to_owned(), Self::from_value(v)))
                    .collect(),
            ),
            Value::Array(array) => Self::Array(array.iter().map(Self::from_value).collect()),
            Value::String(s) => Self::Scalar {
                kind: "string",
                text: s.value().clone(),
            },
            Value::Integer(i) => Self::Scalar {
                kind: "integer",
                text: i.value().to_string(),
            },
            Value::Float(f) => Self::Scalar {
                kind: "float",
                text: f.value().to_string(),
            },
            Value::Boolean(b) => Self::Scalar {
                kind: "boolean",
                text: b.value().to_string(),
            },
            Value::Datetime(d) => Self::Scalar {
                kind: "datetime",
                text: d.value().to_string(),
            },
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Table(_) => "table",
            Self::Array(_) => "array",
            Self::Scalar { kind, .. } => kind,
        }
    }

    fn item_name(&self, index: usize) -> String {
        if let Self::Table(fields) = self
            && let Some((
                _,
                Self::Scalar {
                    kind: "string",
                    text,
                },
            )) = fields.iter().find(|(k, _)| k == "name")
        {
            return text.clone();
        }
        index.to_string()
    }

    fn children(&self) -> Vec<(String, &Node)> {
        match self {
            Self::Table(fields) => fields.iter().map(|(k, v)| (k.clone(), v)).collect(),
            Self::Array(items) => items
                .iter()
                .enumerate()
                .map(|(i, v)| (v.item_name(i), v))
                .collect(),
            Self::Scalar { .. } => Vec::new(),
        }
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.children().into_iter().map(|(k, _)| k).collect();
        names.sort();
        names
    }

    pub(super) fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Table(fields) => serde_json::Value::Object(
                fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
            Self::Array(items) => {
                serde_json::Value::Array(items.iter().map(Self::to_json).collect())
            }
            Self::Scalar { kind, text } => match *kind {
                "integer" => text
                    .parse::<i64>()
                    .map_or_else(|_| text.clone().into(), Into::into),
                "float" => text
                    .parse::<f64>()
                    .map_or_else(|_| text.clone().into(), Into::into),
                "boolean" => (text == "true").into(),
                _ => text.clone().into(),
            },
        }
    }

    fn child(&self, segment: &str) -> Option<&Node> {
        match self {
            Self::Table(fields) => fields.iter().find(|(k, _)| k == segment).map(|(_, v)| v),
            Self::Array(items) => items
                .iter()
                .enumerate()
                .find(|(i, v)| v.item_name(*i) == segment)
                .or_else(|| {
                    segment
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| items.get(i).map(|v| (i, v)))
                })
                .map(|(_, v)| v),
            Self::Scalar { .. } => None,
        }
    }
}

#[derive(Serialize)]
struct Entry {
    key: String,
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<usize>,
}

fn entry(key: String, node: &Node) -> Entry {
    let (value, count) = match node {
        Node::Scalar { text, .. } => (Some(text.clone()), None),
        Node::Table(fields) => (None, Some(fields.len())),
        Node::Array(items) => (None, Some(items.len())),
    };
    Entry {
        key,
        kind: node.kind(),
        value,
        count,
    }
}

fn config_file(global: Option<&Path>, project: Option<&str>) -> Result<PathBuf> {
    let file = global.map_or_else(|| PathBuf::from("cheriot.toml"), Path::to_path_buf);
    match project {
        Some(name) => crate::config::configuration::select_project(&file, Some(name))
            .map_err(|e| AppError::Message(e.to_string())),
        None => Ok(file),
    }
}

pub(super) fn read_document(global: Option<&Path>, project: Option<&str>) -> Result<Node> {
    let path = config_file(global, project)?;
    if !path.exists() {
        return Err(AppError::Message(
            crate::config::configuration::missing_project(&path).to_string(),
        ));
    }
    let metadata = fs::metadata(&path)
        .map_err(|e| AppError::Message(format!("cannot read {}: {e}", path.display())))?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return Err(AppError::Message(format!(
            "not a readable configuration file: {}",
            path.display()
        )));
    }
    let source = fs::read_to_string(&path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|e| AppError::Message(format!("{}: {e}", path.display())))?;
    Node::from_item(document.as_item())
        .ok_or_else(|| AppError::Message(format!("{} is empty", path.display())))
}

pub(super) fn find<'a>(root: &'a Node, key: &str) -> Option<&'a Node> {
    let mut node = root;
    for segment in transaction::path_segments(key) {
        node = node.child(segment)?;
    }
    Some(node)
}

fn walk<'a>(root: &'a Node, key: &str) -> Result<&'a Node> {
    let mut node = root;
    let mut walked = String::new();
    for segment in transaction::path_segments(key) {
        node = node.child(segment).ok_or_else(|| {
            AppError::Message(format!(
                "no key {segment:?} under /{walked}{}",
                crate::output::choices("Keys", &node.names())
            ))
        })?;
        if !walked.is_empty() {
            walked.push('/');
        }
        walked.push_str(segment);
    }
    Ok(node)
}

pub(super) fn handle(
    global: Option<&Path>,
    project: Option<String>,
    key: Option<String>,
    format: Format,
) -> Result<u8> {
    let root = read_document(global, project.as_deref())?;
    let key = key.unwrap_or_default();
    let node = walk(&root, &key)?;
    let mut entries: Vec<Entry> = if matches!(node, Node::Scalar { .. }) {
        let name = transaction::path_segments(&key)
            .last()
            .copied()
            .unwrap_or("");
        vec![entry(name.to_owned(), node)]
    } else {
        node.children()
            .into_iter()
            .map(|(k, v)| entry(k, v))
            .collect()
    };
    entries.sort_by(|a, b| a.key.cmp(&b.key));
    match crate::output::structured(&entries, &format, "entries")? {
        Some(text) => println!("{text}"),
        None => {
            for e in &entries {
                let suffix = if e.value.is_none() { "/" } else { "" };
                println!("{}{suffix}", e.key);
            }
        }
    }
    Ok(0)
}
