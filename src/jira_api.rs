use crate::config::Config;
use anyhow::Result;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug)]
pub struct JiraApi {
    client: Client,
    api_host: String,
    email: String,
    token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardConfiguration {
    pub name: String,
    pub column_config: ColumnsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnsConfig {
    pub columns: Vec<ColumnConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnConfig {
    pub name: String,
    pub statuses: Vec<ColumnStatus>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnStatus {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BoardIssues {
    pub issues: Vec<Issue>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    pub key: String,
    pub fields: Value,
}

impl JiraApi {
    pub fn new(config: &Config) -> Self {
        JiraApi {
            client: Client::new(),
            api_host: config.api_host.clone(),
            email: config.email.clone(),
            token: config.token.clone(),
        }
    }

    pub async fn create_issue(&self, issue: &Value) -> Result<Issue> {
        self.post("rest/api/2/issue", issue).await
    }

    pub async fn board_configuration(&self, id: &str) -> Result<BoardConfiguration> {
        self.get(&format!("rest/agile/1.0/board/{}/configuration", id), &())
            .await
    }

    pub async fn board_issues(&self, id: &str, fields: &str, jql: &str) -> Result<BoardIssues> {
        self.get(
            &format!("rest/agile/1.0/board/{}/issue", id),
            &[("fields", fields), ("jql", jql)],
        )
        .await
    }

    pub async fn issue(&self, key: &str) -> Result<Issue> {
        self.get(&format!("rest/api/2/issue/{}", key), &()).await
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
        let response = self
            .client
            .post(format!("{}/{}", self.api_host, path))
            .basic_auth(&self.email, Some(&self.token))
            .json(body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(response)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &impl Serialize) -> Result<T> {
        let response = self
            .client
            .get(format!("{}/{}", self.api_host, path))
            .query(query)
            .basic_auth(&self.email, Some(&self.token))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(response)
    }
}
