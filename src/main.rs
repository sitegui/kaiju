mod ask_user_edit;
mod board;
mod commands;
mod config;
mod issue_code;
mod jira_api;
mod local_jira_cache;

use crate::commands::{create_issue, edit_config, open_board};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use directories::ProjectDirs;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Edit the configurations for Kaiju
    EditConfig,
    /// Create a new issue
    CreateIssue,
    /// Open the Web interface in a browser
    OpenBoard {
        /// The name of the board, as defined in the config file
        board_name: String,
        /// Does not force the open in a browser
        #[clap(long)]
        no_browser: bool,
        /// Serve the web resources directly from the local folder. Useful when developing Kaiju
        /// itself
        #[clap(long)]
        dev_mode: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let project_dirs = ProjectDirs::from("", "sitegui", "kaiju")
        .context("Could not determine local configuration directory")?;

    let args = Args::parse();

    match args.command {
        Command::EditConfig => edit_config::edit_config(&project_dirs),
        Command::CreateIssue => create_issue::create_issue(&project_dirs).await,
        Command::OpenBoard {
            board_name,
            no_browser,
            dev_mode,
        } => open_board::open_board(&project_dirs, &board_name, no_browser, dev_mode).await,
    }
}
