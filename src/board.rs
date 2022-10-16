use crate::config::{BoardConfig, Config};
use crate::jira_api::JiraApi;
use anyhow::{Context, Result};
use itertools::Itertools;
use serde::Deserialize;

#[derive(Debug)]
pub struct Board {
    api: JiraApi,
    config: BoardConfig,
    name: String,
    columns: Vec<Column>,
}

#[derive(Debug)]
struct Column {
    name: String,
    statuses: Vec<String>,
}

impl Board {
    pub fn open(config: &Config, board_name: &str) -> Result<Self> {
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
        let api = JiraApi::new(&config);

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
        let jira_data: Response = api.get(&format!(
            "rest/agile/1.0/board/{}/configuration",
            board.board_id
        ))?;

        tracing::debug!("Got = {:?}", jira_data);

        let columns = jira_data
            .column_config
            .columns
            .into_iter()
            .map(|column| {
                let statuses = column
                    .statuses
                    .into_iter()
                    .map(|status| status.id)
                    .collect();

                Column {
                    name: column.name,
                    statuses,
                }
            })
            .collect();

        Ok(Board {
            api,
            config: board,
            name: jira_data.name,
            columns,
        })
    }
}
