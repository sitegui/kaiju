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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentInfo {
    pub branches: Vec<Branch>,
    pub pull_requests: Vec<PullRequest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    pub name: String,
    pub status: String,
    pub url: String,
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

    pub async fn create_issue(&self, issue: &Value) -> Result<String> {
        #[derive(Debug, Deserialize)]
        struct Response {
            key: String,
        }

        let response: Response = self.post("rest/api/2/issue", issue).await?;

        Ok(response.key)
    }

    pub async fn board_configuration(&self, id: &str) -> Result<BoardConfiguration> {
        tracing::info!("Load board configuration for {}", id);
        self.get(&format!("rest/agile/1.0/board/{}/configuration", id), &())
            .await
    }

    pub async fn board_issues(&self, id: &str, fields: &str, jql: &str) -> Result<BoardIssues> {
        tracing::info!("Load board issues for {}", id);
        self.get(
            &format!("rest/agile/1.0/board/{}/issue", id),
            &[("fields", fields), ("jql", jql)],
        )
        .await
    }

    pub async fn issue(&self, key: &str) -> Result<Issue> {
        tracing::info!("Load issue {}", key);
        self.get(&format!("rest/api/2/issue/{}", key), &()).await
    }

    pub async fn development_info(&self, issue_id: &str) -> Result<DevelopmentInfo> {
        tracing::info!("Load development info for {}", issue_id);

        #[derive(Debug, Deserialize)]
        struct Response {
            detail: Vec<DevelopmentInfo>,
        }

        let response: Response = self
            .get(
                "rest/dev-status/latest/issue/detail",
                &[
                    ("issueId", issue_id),
                    ("applicationType", "githube"),
                    ("dataType", "pullrequest"),
                ],
            )
            .await?;

        let mut branches = vec![];
        let mut pull_requests = vec![];
        for detail in response.detail {
            branches.extend(detail.branches);
            pull_requests.extend(detail.pull_requests);
        }

        Ok(DevelopmentInfo {
            branches,
            pull_requests,
        })
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
