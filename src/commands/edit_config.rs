use anyhow::Result;
use directories::ProjectDirs;

use crate::ask_user_edit::ask_user_edit;
use crate::config::Config;

pub fn edit_config(project_dirs: &ProjectDirs) -> Result<()> {
    let current_contents = Config::read_contents(project_dirs)?;

    let new_contents = ask_user_edit(project_dirs, &current_contents, "toml")?;

    Config::write_contents(project_dirs, new_contents)?;

    Ok(())
}
