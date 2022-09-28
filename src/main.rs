mod commands;
mod config;

use anyhow::Result;
use clap::{Parser, Subcommand};

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

    let args = Args::parse();

    match args.command {
        Command::EditConfig => commands::edit_config::edit_config(),
        Command::CreateIssue => commands::create_issue::create_issue(),
        Command::Open => commands::open::open(),
    }
}
