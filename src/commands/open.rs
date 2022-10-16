use crate::board::Board;
use crate::config::Config;
use anyhow::Result;
use directories::ProjectDirs;

pub fn open(project_dirs: &ProjectDirs, board_name: &str) -> Result<()> {
    let config = Config::new(project_dirs)?;

    let board = Board::open(&config, board_name)?;

    Ok(())
}
