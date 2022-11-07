use anyhow::Result;
use directories::ProjectDirs;

use crate::ask_user_edit::ask_user_edit;
use crate::config::Config;
use crate::issue_code;
use crate::issue_code::{parse_issue_markdown, prepare_api_body};
use crate::jira_api::JiraApi;

pub async fn create_issue(project_dirs: &ProjectDirs) -> Result<()> {
    let config: Config = Config::new(project_dirs)?;

    let template = issue_code::new_issue(&config)?;
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
                issue_markdown = format!(
                    "-- Failed to parse issue: {:#}\n\
                    -- Please edit it to fix the problem\n\
                    -- If you want to abandon the process, provide an empty file\n\
                    {}",
                    error, issue_markdown
                );

                tracing::warn!("Failed to parse issue: {}. Please retry", error);
            }
            Ok(api_body) => break api_body,
        }
    };

    tracing::info!("Will request Jira API");
    let api = JiraApi::new(&config);
    let key = api.create_issue(&api_body).await?;

    tracing::info!("Created issue: {}/browse/{}", config.api_host, key);

    Ok(())
}
