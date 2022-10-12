use std::collections::BTreeMap;
use std::fmt::Write;

use crate::ask_user_edit::ask_user_edit;
use anyhow::{ensure, Context, Result};
use directories::ProjectDirs;
use itertools::Itertools;
use serde_json::{Map, Value};

use crate::config::{Config, IssueFieldConfig, IssueFieldValuesConfig};

pub fn create_issue(project_dirs: &ProjectDirs) -> Result<()> {
    let config: Config = Config::new(project_dirs)?;

    let template = template(&config)?;
    let mut issue_markdown = template.clone();
    let api_body = loop {
        issue_markdown = ask_user_edit(project_dirs, &issue_markdown, "md")?;

        if issue_markdown.trim() == template.trim() || issue_markdown.trim().is_empty() {
            tracing::warn!("Exiting because the user does not want to create the issue");
            return Ok(());
        }

        let maybe_api_body = parse_issue_markdown(&issue_markdown)
            .and_then(|issue| prepare_api_body(&config, issue));
        match maybe_api_body {
            Err(error) => {
                tracing::warn!("Failed to parse issue: {}", error);
                tracing::warn!("Please edit it to fix the problem");
                tracing::warn!("If you want to abandon the process, provide an empty file");
            }
            Ok(api_body) => break api_body,
        }
    };

    println!("{}", serde_json::to_string_pretty(&api_body)?);

    Ok(())
}

fn template(config: &Config) -> Result<String> {
    let mut contents = String::new();

    writeln!(contents, "# Summary")?;
    writeln!(contents)?;
    writeln!(contents, "Description")?;
    writeln!(contents)?;
    writeln!(contents, "```kaiju")?;

    write_default_kaiju_code(&mut contents, config)?;

    writeln!(contents, "```")?;

    Ok(contents)
}

fn write_default_kaiju_code(contents: &mut String, config: &Config) -> Result<()> {
    for field in &config.issue_fields {
        match &field.values {
            IssueFieldValuesConfig::Simple { values } => {
                write_kaiju_values(
                    contents,
                    &field.name,
                    values,
                    field.default_value.as_deref(),
                )?;
            }
            IssueFieldValuesConfig::FromBag { values_from } => {
                match config.value_bag.get(values_from) {
                    None => {
                        tracing::warn!(
                            "Missing value bag {} for field {}",
                            values_from,
                            field.name
                        );
                        writeln!(contents, "# {}:", field.name)?;
                    }
                    Some(bag) => {
                        write_kaiju_values(
                            contents,
                            &field.name,
                            bag.keys(),
                            field.default_value.as_deref(),
                        )?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn write_kaiju_values<'a>(
    contents: &mut String,
    field_name: &str,
    values: impl IntoIterator<Item = &'a String>,
    default_value: Option<&str>,
) -> Result<()> {
    if let Some(default_value) = default_value {
        writeln!(contents, "{}: {}", field_name, default_value)?;
    }

    let other_values: Vec<_> = values
        .into_iter()
        .filter(|value| Some(value.as_str()) != default_value)
        .collect();
    if !other_values.is_empty() {
        writeln!(
            contents,
            "# {}: {}",
            field_name,
            other_values.into_iter().format(", ")
        )?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct CreateIssue {
    summary: String,
    description: String,
    commands: BTreeMap<String, Vec<String>>,
}

fn parse_issue_markdown(source: &str) -> Result<CreateIssue> {
    let mut lines = source.lines();

    let summary = lines
        .next()
        .context("The first line must indicate the summary")?
        .strip_prefix('#')
        .context("The summary line must start with a '#'")?
        .trim()
        .to_owned();

    let mut description_lines = vec![];
    let mut commands: BTreeMap<_, Vec<_>> = BTreeMap::new();
    let mut is_kaiju_code = false;
    for line in lines {
        if is_kaiju_code {
            if line == "```" {
                is_kaiju_code = false;
            } else if !line.starts_with('#') {
                let (command, value) = line.split_once(':').with_context(|| {
                    format!(
                        "Kaiju command must have a colon (:) separating name and value in {}",
                        line
                    )
                })?;
                commands
                    .entry(command.trim().to_owned())
                    .or_default()
                    .push(value.trim().to_owned());
            }
        } else if line == "```kaiju" {
            is_kaiju_code = true;
        } else {
            description_lines.push(line);
        }
    }

    let description = description_lines
        .into_iter()
        .format("\n")
        .to_string()
        .trim()
        .to_owned();

    Ok(CreateIssue {
        summary,
        description,
        commands,
    })
}

fn prepare_api_body(config: &Config, issue: CreateIssue) -> Result<Value> {
    let mut body = Map::new();

    set_in_body(&mut body, "fields.summary", issue.summary)?;
    set_in_body(&mut body, "fields.description", issue.description)?;

    for (name, values) in issue.commands {
        let config_field = config
            .issue_fields
            .iter()
            .find(|issue_field| issue_field.name == name);
        for value in values {
            match config_field {
                None => {
                    set_in_body(&mut body, &name, value)?;
                }
                Some(config_field) => {
                    // TODO: translate values when used from bags
                    let value = match config_field.values {
                        IssueFieldValuesConfig::Simple { .. } => {}
                        IssueFieldValuesConfig::FromBag { values_from } => {}
                    };
                    set_in_body(&mut body, &config_field.api_field, value)?;
                }
            }
        }
    }

    Ok(Value::Object(body))
}

fn set_in_body(body: &mut Map<String, Value>, field: &str, value: String) -> Result<()> {
    let mut scope = body;
    let parts = field.split('.').collect_vec();
    let (&last_part, prefix_parts) = parts
        .split_last()
        .context("The api field must contain at least one path")?;

    // Navigate the hierarchy, building intermediate objects and arrays
    for &part in prefix_parts {
        match part.strip_suffix("[]") {
            None => {
                scope = scope
                    .entry(part)
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .with_context(|| format!("Expected object when setting {}", field))?;
            }
            Some(part) => {
                let array = scope
                    .entry(part)
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .with_context(|| format!("Expected array when setting {}", field))?;
                array.push(Value::Object(Map::new()));
                scope = array
                    .last_mut()
                    .expect("the last element was just pushed into the array")
                    .as_object_mut()
                    .expect("the last element is an object");
            }
        }
    }

    // Set leaf value
    match last_part.strip_suffix("[]") {
        None => {
            ensure!(!scope.contains_key(last_part));
            scope.insert(last_part.to_owned(), Value::String(value));
        }
        Some(part) => {
            let array = scope
                .entry(part)
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .with_context(|| format!("Expected array when setting {}", field))?;
            array.push(Value::String(value));
        }
    }

    Ok(())
}
