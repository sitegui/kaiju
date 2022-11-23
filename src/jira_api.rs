use crate::config::Config;
use anyhow::Result;
use reqwest::{Client, RequestBuilder};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;

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
    pub id: String,
    pub key: String,
    pub fields: Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DevelopmentInfo {
    pub branches: Vec<Branch>,
    #[serde(rename = "pullRequests")]
    pub merge_requests: Vec<MergeRequest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MergeRequest {
    pub name: String,
    pub status: String,
    pub url: String,
}

impl JiraApi {
    pub fn new(config: &Config) -> Self {
        JiraApi {
            client: Client::builder()
                .timeout(Duration::from_secs(config.api_timeout_seconds))
                .build()
                .unwrap(),
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

        tracing::debug!("Create issue {}", issue);
        let response: Response = self
            .request(
                self.client
                    .post(format!("{}/rest/api/2/issue", self.api_host))
                    .json(issue),
            )
            .await?;

        Ok(response.key)
    }

    pub async fn edit_issue(&self, key: &str, issue: &Value) -> Result<()> {
        tracing::debug!("Edit issue {}: {}", key, issue);

        self.request_no_output(
            self.client
                .put(format!("{}/rest/api/2/issue/{}", self.api_host, key))
                .json(issue),
        )
        .await
    }

    pub async fn transition_issue(&self, key: &str, transition_id: &str) -> Result<()> {
        tracing::debug!("Transition issue {} to {}", key, transition_id);

        self.request_no_output(
            self.client
                .post(format!(
                    "{}/rest/api/2/issue/{}/transitions",
                    self.api_host, key
                ))
                .json(&json!({
                    "transition": {
                        "id": transition_id,
                    }
                })),
        )
        .await
    }

    pub async fn board_configuration(&self, id: &str) -> Result<BoardConfiguration> {
        tracing::debug!("Load board configuration for {}", id);
        self.request(self.client.get(format!(
            "{}/rest/agile/1.0/board/{}/configuration",
            self.api_host, id
        )))
        .await
    }

    pub async fn board_issues(&self, id: &str, fields: &str, jql: &str) -> Result<BoardIssues> {
        tracing::debug!("Load board issues for {}", id);
        self.request(
            self.client
                .get(format!(
                    "{}/rest/agile/1.0/board/{}/issue",
                    self.api_host, id
                ))
                .query(&[("fields", fields), ("jql", jql)]),
        )
        .await
    }

    pub async fn issue(&self, key: &str) -> Result<Issue> {
        tracing::debug!("Load issue {}", key);
        self.request(
            self.client
                .get(format!("{}/rest/api/2/issue/{}", self.api_host, key)),
        )
        .await
    }

    pub async fn development_info(&self, issue_id: &str) -> Result<DevelopmentInfo> {
        tracing::debug!("Load development info for {}", issue_id);

        #[derive(Debug, Deserialize)]
        struct Response {
            detail: Vec<DevelopmentInfo>,
        }

        let response: Response = self
            .request(
                self.client
                    .get(format!(
                        "{}/rest/dev-status/latest/issue/detail",
                        self.api_host
                    ))
                    .query(&[
                        ("issueId", issue_id),
                        ("applicationType", "githube"),
                        ("dataType", "pullrequest"),
                    ]),
            )
            .await?;

        let mut branches = vec![];
        let mut merge_requests = vec![];
        for detail in response.detail {
            branches.extend(detail.branches);
            merge_requests.extend(detail.merge_requests);
        }

        Ok(DevelopmentInfo {
            branches,
            merge_requests,
        })
    }

    async fn request<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = request
            .basic_auth(&self.email, Some(&self.token))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        Ok(response)
    }

    async fn request_no_output(&self, request: RequestBuilder) -> Result<()> {
        request
            .basic_auth(&self.email, Some(&self.token))
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }
}
