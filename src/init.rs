use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::{
    process::{command, require_success},
    scaffold::{SCAFFOLD, seal_scaffold},
    util::read_json,
};

#[derive(Deserialize)]
struct ScaffoldLock {
    commit: String,
    snapshot: String,
}

pub fn initialize_project(directory: &Path) -> Result<serde_json::Value> {
    let destination = if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        std::env::current_dir()?.join(directory)
    };
    if destination.exists() {
        bail!("destination already exists: {}", destination.display());
    }
    fs::create_dir_all(&destination)
        .with_context(|| format!("create {}", destination.display()))?;
    if let Err(error) = SCAFFOLD.extract(&destination) {
        let _ = fs::remove_dir_all(&destination);
        return Err(error)
            .with_context(|| format!("extract scaffold to {}", destination.display()));
    }
    seal_scaffold(&destination)?;
    let lock: ScaffoldLock = read_json(destination.join(".v4hook-template-lock.json"))?;
    require_success(
        &command(&["git", "init", "-b", "main"]),
        &destination,
        None,
        true,
    )?;
    Ok(serde_json::json!({
        "directory": destination.to_string_lossy(),
        "commit": lock.commit,
        "snapshot": lock.snapshot,
    }))
}
