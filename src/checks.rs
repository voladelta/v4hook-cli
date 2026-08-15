use std::path::Path;

use anyhow::Result;

use crate::{
    model::{CheckEvidence, LoadedConfig},
    process::{CommandResult, require_success},
    util::sha256_bytes,
};

fn evidence(name: &str, result: &CommandResult) -> CheckEvidence {
    CheckEvidence {
        name: name.to_owned(),
        command: result.command.clone(),
        duration_ms: result.duration_ms,
        stdout_hash: sha256_bytes(&result.stdout),
        stderr_hash: sha256_bytes(&result.stderr),
    }
}

pub fn run_check_suite(config: &LoadedConfig) -> Result<Vec<CheckEvidence>> {
    let commands = vec![
        (
            "format",
            ["forge", "fmt", "--check"].map(str::to_owned).to_vec(),
        ),
        ("lint", ["forge", "lint"].map(str::to_owned).to_vec()),
        (
            "static-analysis",
            config.value.checks.static_analysis.clone(),
        ),
        ("build", ["forge", "build"].map(str::to_owned).to_vec()),
        ("unit", config.value.checks.unit.clone()),
        ("fuzz", config.value.checks.fuzz.clone()),
        ("invariant", config.value.checks.invariant.clone()),
    ];
    let cwd = Path::new(&config.project_root);
    commands
        .into_iter()
        .map(|(name, command)| {
            let result = require_success(&command, cwd, None, false)?;
            Ok(evidence(name, &result))
        })
        .collect()
}
