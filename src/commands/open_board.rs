use crate::board::Board;
use crate::config::Config;
use anyhow::Result;
use directories::ProjectDirs;
use std::fs;

pub async fn open_board(project_dirs: &ProjectDirs, board_name: &str) -> Result<()> {
    let config = Config::new(project_dirs)?;

    let board = Board::open(&config, board_name).await?;

    let board_data = board.load().await?;
    let board_data_json = serde_json::to_string_pretty(&board_data)?;
    fs::write("data/board.json", board_data_json)?;

    Ok(())
}
