mod ask_user_edit;
mod commands;
mod config;

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
    /// Open the Web interface
    Open,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let project_dirs = ProjectDirs::from("", "sitegui", "kaiju")
        .context("Could not determine local configuration directory")?;

    let args = Args::parse();

    match args.command {
        Command::EditConfig => commands::edit_config::edit_config(&project_dirs),
        Command::CreateIssue => commands::create_issue::create_issue(&project_dirs),
        Command::Open => commands::open::open(),
    }
}
