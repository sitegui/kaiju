use crate::config::{Config, IssueFieldValuesConfig};
use anyhow::{ensure, Context, Result};
use itertools::Itertools;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt::Write;

const COMMENT_PREFIX: &str = "<!--";
const SEPARATOR: &str = ", ";
const COMMENT_SUFFIX: &str = "-->";

pub fn new_issue(config: &Config) -> Result<String> {
    let mut contents = String::new();

    writeln!(contents, "# Summary")?;
    writeln!(contents)?;
    writeln!(contents, "Description")?;
    writeln!(contents)?;
    writeln!(contents, "# Kaiju")?;
    writeln!(contents)?;

    write_default_kaiju_code(&mut contents, config)?;

    Ok(contents)
}

fn write_default_kaiju_code(contents: &mut String, config: &Config) -> Result<()> {
    for field in &config.issue_fields {
        match &field.values {
            IssueFieldValuesConfig::Simple { values } => {
                write_kaiju_values(
                    contents,
                    &field.name,
                    values.iter(),
                    field.default_value.as_deref(),
                )?;
            }
            IssueFieldValuesConfig::FromBag { values_from } => {
                match config.value_bag.get(values_from) {
                    None => {
                        tracing::warn!(
                            "Missing value bag {:?} for field {:?}",
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
    values: impl Iterator<Item = &'a String>,
    default_value: Option<&str>,
) -> Result<()> {
    const MAX_LINE: usize = 80;

    if let Some(default_value) = default_value {
        writeln!(contents, "{}: {}", field_name, default_value)?;
    }

    let other_values = values.filter(|value| Some(value.as_str()) != default_value);

    let mut pending_line = String::new();
    for value in other_values {
        if pending_line.is_empty() {
            write!(pending_line, "{}{}: {}", COMMENT_PREFIX, field_name, value)?;
        } else if pending_line.len() + SEPARATOR.len() + value.len() + COMMENT_SUFFIX.len()
            <= MAX_LINE
        {
            write!(pending_line, "{}{}", SEPARATOR, value)?;
        } else {
            writeln!(contents, "{}{}", pending_line, COMMENT_SUFFIX)?;
            pending_line = String::new();
        }
    }

    if !pending_line.is_empty() {
        writeln!(contents, "{}{}", pending_line, COMMENT_SUFFIX)?;
    }

    Ok(())
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CreateIssue {
    summary: String,
    description: String,
    commands: BTreeMap<String, Vec<String>>,
}

pub fn parse_issue_markdown(source: &str) -> Result<CreateIssue> {
    let mut lines = source.lines().skip_while(|line| line.starts_with("-- "));

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
    let mut has_kaiju_code = false;
    for line in lines {
        let trimmed_line = line.trim();
        if is_kaiju_code {
            if trimmed_line.starts_with("# ") {
                is_kaiju_code = false;
                description_lines.push(line);
            } else if !trimmed_line.starts_with(COMMENT_PREFIX)
                || !trimmed_line.ends_with(COMMENT_SUFFIX)
            {
                let (command, value) = line.split_once(':').with_context(|| {
                    format!(
                        "Kaiju command must have a colon (:) separating name and value in {:?}",
                        line
                    )
                })?;
                commands
                    .entry(command.trim().to_owned())
                    .or_default()
                    .extend(value.split(',').map(|value| value.trim().to_string()));
            }
        } else if trimmed_line == "# Kaiju" {
            is_kaiju_code = true;
            has_kaiju_code = true;
        } else {
            description_lines.push(line);
        }
    }

    ensure!(
        has_kaiju_code,
        "No Kaiju section starting with '# Kaiju' was found"
    );

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

pub fn prepare_api_body(config: &Config, issue: CreateIssue) -> Result<Value> {
    let mut body = Map::new();

    set_in_body(&mut body, "fields.summary", issue.summary)?;
    set_in_body(&mut body, "fields.description", issue.description)?;

    for (name, values) in issue.commands {
        apply_command(config, &mut body, &name, values)
            .with_context(|| format!("Failed to apply command {:?}", name))?;
    }

    Ok(Value::Object(body))
}

fn apply_command(
    config: &Config,
    body: &mut Map<String, Value>,
    name: &str,
    values: Vec<String>,
) -> Result<()> {
    enum CommandTranslator<'a> {
        UnknownField,
        SimpleValue {
            api_field: &'a str,
        },
        ValueBag {
            api_field: &'a str,
            bag_name: &'a str,
            value_bag: &'a BTreeMap<String, String>,
        },
    }

    let issue_field = config
        .issue_fields
        .iter()
        .find(|issue_field| issue_field.name == name);

    let command_translator = match issue_field {
        None => CommandTranslator::UnknownField,
        Some(issue_field) => match &issue_field.values {
            IssueFieldValuesConfig::Simple { .. } => CommandTranslator::SimpleValue {
                api_field: &issue_field.api_field,
            },
            IssueFieldValuesConfig::FromBag { values_from } => {
                let value_bag = config
                    .value_bag
                    .get(values_from)
                    .with_context(|| format!("Value bag {:?} not found", values_from))?;

                CommandTranslator::ValueBag {
                    api_field: &issue_field.api_field,
                    bag_name: values_from,
                    value_bag,
                }
            }
        },
    };

    for value in values {
        match command_translator {
            CommandTranslator::UnknownField => {
                set_in_body(body, name, value)?;
            }
            CommandTranslator::SimpleValue { api_field } => {
                set_in_body(body, api_field, value)?;
            }
            CommandTranslator::ValueBag {
                api_field,
                bag_name,
                value_bag,
            } => {
                let translated_value = value_bag.get(&value).cloned().unwrap_or_else(|| {
                    tracing::info!("Value {:?} not found in value bag {:?}", value, bag_name);
                    value
                });
                set_in_body(body, api_field, translated_value)?;
            }
        }
    }

    Ok(())
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
                    .with_context(|| format!("Expected object when setting {:?}", field))?;
            }
            Some(part) => {
                let array = scope
                    .entry(part)
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .with_context(|| format!("Expected array when setting {:?}", field))?;
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
            ensure!(
                !scope.contains_key(last_part),
                "Cannot set field multiple times: {:?}",
                field
            );
            scope.insert(last_part.to_owned(), Value::String(value));
        }
        Some(part) => {
            let array = scope
                .entry(part)
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .with_context(|| format!("Expected array when setting {:?}", field))?;
            array.push(Value::String(value));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_issue_markdown() {
        let issue = parse_issue_markdown(
            "#    Some summary  
some  
description 
# Kaiju 
command_1  : value_10
command_2:    value_20 ,   value_21
command_1: value_11
<!--command_1: value_12-->
# More
even more description",
        )
        .unwrap();

        assert_eq!(
            issue,
            CreateIssue {
                summary: "Some summary".to_string(),
                description: "some  \ndescription \n# More\neven more description".to_string(),
                commands: BTreeMap::from_iter([
                    (
                        "command_1".to_string(),
                        vec!["value_10".to_string(), "value_11".to_string()]
                    ),
                    (
                        "command_2".to_string(),
                        vec!["value_20".to_string(), "value_21".to_string()]
                    )
                ])
            }
        );
    }
}
