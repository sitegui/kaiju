use crate::config::Config;
use anyhow::Result;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;

#[derive(Debug)]
pub struct JiraApi {
    client: Client,
    api_host: String,
    email: String,
    token: String,
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

    pub async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T> {
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

    pub async fn get<T: DeserializeOwned>(&self, path: &str, query: &impl Serialize) -> Result<T> {
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
