use crate::config::{BoardLocalConfig, Config};
use crate::jira_api::JiraApi;
use anyhow::{Context, Result};
use futures::future;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct Board {
    api: JiraApi,
    api_host: String,
    local_config: BoardLocalConfig,
    jira_config: Mutex<Option<BoardJiraConfig>>,
    epic_by_key: Mutex<BTreeMap<String, Arc<Mutex<Option<BoardEpicData>>>>>,
}

#[derive(Debug, Clone)]
struct BoardJiraConfig {
    columns: Vec<Column>,
    name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardData {
    name: String,
    columns: Vec<BoardColumnData>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardColumnData {
    name: String,
    issues: Vec<BoardIssueData>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardIssueData {
    key: String,
    jira_link: String,
    summary: String,
    status: String,
    avatars: Vec<BoardAvatarData>,
    epic: Option<BoardEpicData>,
}

#[derive(Debug, Clone, Serialize, Ord, PartialOrd, Eq, PartialEq)]
pub struct BoardAvatarData {
    name: String,
    image: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BoardEpicData {
    key: String,
    jira_link: String,
    short_name: String,
    color: Option<String>,
}

#[derive(Debug, Clone)]
struct Column {
    name: String,
    status_ids: Vec<String>,
}

impl Board {
    pub async fn open(config: &Config, board_name: &str) -> Result<Self> {
        let board = config
            .board
            .get(board_name)
            .with_context(|| {
                format!(
                    "Board '{}' not found in the config. Valid names are: {}",
                    board_name,
                    config.board.keys().format(", ")
                )
            })?
            .clone();

        let api = JiraApi::new(config);

        Ok(Board {
            api,
            api_host: config.api_host.clone(),
            local_config: board,
            jira_config: Mutex::new(None),
            epic_by_key: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn load(&self) -> Result<BoardData> {
        // Determine which fields are needed
        let mut request_fields = BTreeSet::new();
        request_fields.insert("status");
        request_fields.insert("summary");
        request_fields.insert("parent");
        for card_avatar in &self.local_config.card_avatars {
            request_fields.insert(card_avatar);
        }
        let fields = request_fields.into_iter().join(",");

        let jira_config = self.jira_config().await?;

        // Load all columns
        let num_columns = jira_config.columns.len();
        let columns = future::try_join_all(
            jira_config
                .columns
                .into_iter()
                .enumerate()
                .map(|(i, column)| self.load_column(&fields, column, i == num_columns - 1)),
        )
        .await?;

        Ok(BoardData {
            name: jira_config.name,
            columns,
        })
    }

    async fn jira_config(&self) -> Result<BoardJiraConfig> {
        let mut cached = self.jira_config.lock().await;

        if let Some(config) = cached.as_ref() {
            return Ok(config.clone());
        }

        tracing::info!("Will request configuration from Jira");
        let jira_data = self
            .api
            .board_configuration(&self.local_config.board_id)
            .await?;
        tracing::debug!("Got = {:?}", jira_data);

        let num_skip = if self.local_config.show_first_column {
            0
        } else {
            1
        };
        let columns = jira_data
            .column_config
            .columns
            .into_iter()
            .skip(num_skip)
            .map(|column| {
                let statuses = column
                    .statuses
                    .into_iter()
                    .map(|status| status.id)
                    .collect();

                Column {
                    name: column.name,
                    status_ids: statuses,
                }
            })
            .collect_vec();

        let jira_config = BoardJiraConfig {
            columns,
            name: jira_data.name,
        };

        *cached = Some(jira_config.clone());

        Ok(jira_config)
    }

    async fn load_column(
        &self,
        fields: &str,
        column: Column,
        is_last: bool,
    ) -> Result<BoardColumnData> {
        let mut jql = format!("status in ({})", column.status_ids.iter().format(","));
        if let (true, Some(filter_resolved)) =
            (is_last, &self.local_config.filter_last_column_resolved)
        {
            write!(jql, " and resolved >= {:?}", filter_resolved)?;
        }

        let start = Instant::now();
        tracing::info!("Will request Jira for issues in column {}", column.name);
        let response = self
            .api
            .board_issues(&self.local_config.board_id, fields, &jql)
            .await?;

        tracing::debug!("Got {:?}", response);

        let issues = future::try_join_all(
            response
                .issues
                .into_iter()
                .map(|issue| self.load_issue(issue.key, issue.fields)),
        )
        .await?;

        tracing::info!("Finished column {} in {:?}", column.name, start.elapsed());

        Ok(BoardColumnData {
            name: column.name,
            issues,
        })
    }

    async fn load_issue(&self, key: String, fields: Value) -> Result<BoardIssueData> {
        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Avatar {
            avatar_urls: AvatarUrls,
            display_name: String,
        }

        #[derive(Debug, Deserialize)]
        struct AvatarUrls {
            #[serde(rename = "32x32")]
            size_32: String,
        }

        let summary = fields["summary"]
            .as_str()
            .context("Could not extract summary field")?
            .to_owned();

        let status = fields["status"]["name"]
            .as_str()
            .context("Could not extract status field")?
            .to_owned();

        let mut avatars = BTreeSet::new();
        for card_avatar in &self.local_config.card_avatars {
            if let Some(value) = fields.get(card_avatar) {
                if value.is_null() {
                    continue;
                }

                let values = if value.is_array() {
                    value.clone()
                } else {
                    Value::Array(vec![value.clone()])
                };

                let parsed_values: Vec<Avatar> = serde_json::from_value(values)?;
                for parsed_value in parsed_values {
                    avatars.insert(BoardAvatarData {
                        name: parsed_value.display_name,
                        image: parsed_value.avatar_urls.size_32,
                    });
                }
            }
        }

        let epic = match fields["parent"]["key"].as_str() {
            None => None,
            Some(key) => Some(self.load_epic(key.to_string()).await?),
        };

        Ok(BoardIssueData {
            jira_link: format!("{}/browse/{}", self.api_host, key),
            key,
            summary,
            status,
            avatars: avatars.into_iter().collect(),
            epic,
        })
    }

    async fn epic_cache(&self, key: String) -> Arc<Mutex<Option<BoardEpicData>>> {
        let mut epic_by_key = self.epic_by_key.lock().await;

        let epic_cache = epic_by_key
            .entry(key)
            .or_insert_with(|| Arc::new(Mutex::new(None)));

        epic_cache.clone()
    }

    async fn load_epic(&self, key: String) -> Result<BoardEpicData> {
        let epic_cache = self.epic_cache(key.clone()).await;
        let mut cached = epic_cache.lock().await;

        if let Some(epic) = cached.as_ref() {
            return Ok(epic.clone());
        }

        let response = self.api.issue(&key).await?;
        let short_name = response
            .fields
            .get(&self.local_config.epic_short_name)
            .unwrap_or(&Value::Null)
            .as_str()
            .context("Could not extract short name for epic issue")?
            .to_owned();

        let color = self
            .local_config
            .epic_color
            .as_ref()
            .and_then(|field| response.fields.get(field).unwrap_or(&Value::Null).as_str())
            .and_then(Self::translate_color)
            .map(ToOwned::to_owned);

        let epic = BoardEpicData {
            key: key.to_owned(),
            jira_link: format!("{}/browse/{}", self.api_host, key),
            short_name,
            color,
        };

        *cached = Some(epic.clone());

        Ok(epic)
    }

    fn translate_color(color_name: &str) -> Option<&str> {
        match color_name {
            "purple" => Some("#8777D9"),
            "blue" => Some("#2684FF"),
            "green" => Some("#57D9A3"),
            "teal" => Some("#00C7E6"),
            "yellow" => Some("#FFC400"),
            "orange" => Some("#FF7452"),
            "grey" => Some("#6B778C"),
            "dark_purple" => Some("#5243AA"),
            "dark_blue" => Some("#0052CC"),
            "dark_green" => Some("#00875A"),
            "dark_teal" => Some("#00A3BF"),
            "dark_yellow" => Some("#FF991F"),
            "dark_orange" => Some("#DE350B"),
            "dark_grey" => Some("#253858"),
            _ => {
                tracing::warn!("Could not translate color '{}'", color_name);
                None
            }
        }
    }
}
