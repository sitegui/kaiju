use crate::config::Config;
use anyhow::Result;
use std::io;

pub fn edit_config() -> Result<()> {
    let new_contents = scrawl::with(&Config::read_contents()?)?;

    Config::write_contents(new_contents)?;

    Ok(())
}
