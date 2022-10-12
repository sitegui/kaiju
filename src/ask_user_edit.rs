use std::process::Command;
use std::thread::sleep;
use std::time::Duration;
use std::{env, fs};

use anyhow::{ensure, Context, Result};
use directories::ProjectDirs;
use time::OffsetDateTime;

pub fn ask_user_edit(
    project_dirs: &ProjectDirs,
    contents: &str,
    extension: &str,
) -> Result<String> {
    let editor_cmd = env::var("VISUAL").or_else(|_| env::var("EDITOR")).context(
        "Couldn't determine which editor to use. \
        Please set the environment variable VISUAL with the command prefix you want to use",
    )?;

    // Split "a-cmd an-arg an-arg2" into the base program ("a-cmd") and the base args ("an-arg",
    // "an-arg2")
    let words: Vec<String> = shell_words::split(&editor_cmd)
        .with_context(|| format!("Couldn't parse editor command: {}", editor_cmd))?;
    let (program, base_args) = words
        .split_first()
        .context("Editor command cannot be empty")?;

    let timestamp = OffsetDateTime::now_utc().unix_timestamp();

    let temp_path = project_dirs
        .cache_dir()
        .join(format!("edit-{timestamp}.{extension}"));

    if let Some(parent) = temp_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&temp_path, contents)?;

    let status = Command::new(program)
        .args(base_args)
        .arg(&temp_path)
        .status()
        .with_context(|| format!("Failed to executed editor {editor_cmd}"))?;

    ensure!(
        status.success(),
        "The external editor didn't finish with success: {}",
        status
    );

    // Ugly hack: in my tests, it seems that IntelliJ takes a while to save the modified file
    // **after** returning from the command line :/
    // I don't know another safer way to detect whether the file was actually save, so I will just
    // sleep here and hope for the best.
    sleep(Duration::from_secs(1));

    let new_contents = fs::read_to_string(&temp_path)?;

    Ok(new_contents)
}
