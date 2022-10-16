use crate::board::Board;
use crate::config::Config;
use anyhow::Result;
use directories::ProjectDirs;
use std::fs;

pub fn open(project_dirs: &ProjectDirs, board_name: &str) -> Result<()> {
    let config = Config::new(project_dirs)?;

    let board = Board::open(&config, board_name)?;

    let board_data = board.load()?;
    let board_data_json = serde_json::to_string_pretty(&board_data)?;
    fs::write("data/board.json", board_data_json)?;

    Ok(())
}
