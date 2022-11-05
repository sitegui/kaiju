use crate::config::{Config, IssueFieldValuesConfig};
use anyhow::Result;
use itertools::Itertools;
use std::fmt::Write;

pub fn new_issue(config: &Config) -> Result<String> {
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
