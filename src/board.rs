use crate::config::{BoardConfig, Config};
use crate::jira_api::JiraApi;
use anyhow::{Context, Result};
use futures::future;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::time::Instant;
use tokio::sync::Mutex;

#[derive(Debug)]
pub struct Board {
    api: JiraApi,
    config: BoardConfig,
    name: String,
    columns: Vec<Column>,
    epic_by_key: Mutex<BTreeMap<String, BoardEpicData>>,
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
    short_name: String,
    color: Option<String>,
}

#[derive(Debug)]
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

        #[derive(Debug, Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            name: String,
            column_config: ResponseColumnsConfig,
        }

        #[derive(Debug, Deserialize)]
        struct ResponseColumnsConfig {
            columns: Vec<ResponseColumnConfig>,
        }

        #[derive(Debug, Deserialize)]
        struct ResponseColumnConfig {
            name: String,
            statuses: Vec<ResponseColumnStatus>,
        }

        #[derive(Debug, Deserialize)]
        struct ResponseColumnStatus {
            id: String,
        }

        tracing::info!("Will request configuration from Jira");
        let jira_data: Response = api
            .get(
                &format!("rest/agile/1.0/board/{}/configuration", board.board_id),
                &(),
            )
            .await?;

        tracing::debug!("Got = {:?}", jira_data);

        let num_skip = if board.show_first_column { 0 } else { 1 };
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
            .collect();

        Ok(Board {
            api,
            config: board,
            name: jira_data.name,
            columns,
            epic_by_key: Mutex::new(BTreeMap::new()),
        })
    }

    pub async fn load(&self) -> Result<BoardData> {
        // Determine which fields are needed
        let mut request_fields = BTreeSet::new();
        request_fields.insert("status");
        request_fields.insert("summary");
        request_fields.insert("parent");
        for card_avatar in &self.config.card_avatars {
            request_fields.insert(card_avatar);
        }
        let fields = request_fields.into_iter().join(",");

        // Load all columns
        let num_columns = self.columns.len();
        let columns = future::try_join_all(
            self.columns
                .iter()
                .enumerate()
                .map(|(i, column)| self.load_column(&fields, column, i == num_columns - 1)),
        )
        .await?;

        Ok(BoardData {
            name: self.name.clone(),
            columns,
        })
    }

    async fn load_column(
        &self,
        fields: &str,
        column: &Column,
        is_last: bool,
    ) -> Result<BoardColumnData> {
        let mut jql = format!("status in ({})", column.status_ids.iter().format(","));
        if let (true, Some(filter_resolved)) = (is_last, &self.config.filter_last_column_resolved) {
            write!(jql, " and resolved >= {:?}", filter_resolved)?;
        }

        #[derive(Debug, Deserialize)]
        struct Response {
            issues: Vec<ResponseIssue>,
        }

        #[derive(Debug, Deserialize)]
        struct ResponseIssue {
            key: String,
            fields: Value,
        }

        let start = Instant::now();
        tracing::info!("Will request Jira for issues in column {}", column.name);
        let response: Response = self
            .api
            .get(
                &format!("rest/agile/1.0/board/{}/issue", self.config.board_id),
                &[("fields", fields), ("jql", &jql)],
            )
            .await?;

        tracing::info!("Finished column {} in {:?}", column.name, start.elapsed());
        tracing::debug!("Got {:?}", response);

        let issues = future::try_join_all(
            response
                .issues
                .into_iter()
                .map(|issue| self.load_issue(issue.key, issue.fields)),
        )
        .await?;

        Ok(BoardColumnData {
            name: column.name.clone(),
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
        for card_avatar in &self.config.card_avatars {
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
            Some(key) => Some(self.load_epic(key).await?),
        };

        Ok(BoardIssueData {
            key,
            summary,
            status,
            avatars: avatars.into_iter().collect(),
            epic,
        })
    }

    async fn load_epic(&self, key: &str) -> Result<BoardEpicData> {
        let mut epic_by_key = self.epic_by_key.lock().await;

        if let Some(cached) = epic_by_key.get(key) {
            return Ok(cached.clone());
        }

        #[derive(Debug, Deserialize)]
        struct Response {
            fields: BTreeMap<String, Value>,
        }

        let response: Response = self
            .api
            .get(&format!("rest/api/2/issue/{}", key), &())
            .await?;
        let short_name = response
            .fields
            .get(&self.config.epic_short_name)
            .unwrap_or(&Value::Null)
            .as_str()
            .context("Could not extract short name for epic issue")?
            .to_owned();

        let color = self
            .config
            .epic_color
            .as_ref()
            .and_then(|field| response.fields.get(field).unwrap_or(&Value::Null).as_str())
            .and_then(Self::translate_color)
            .map(ToOwned::to_owned);

        let epic = BoardEpicData {
            key: key.to_owned(),
            short_name,
            color,
        };

        epic_by_key.insert(key.to_owned(), epic.clone());

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
